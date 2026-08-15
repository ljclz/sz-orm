//! 真实传输层 — tonic gRPC + reqwest HTTP/JSON
//!
//! 提供 [`TonicGrpcCallHandler`] 和 [`ReqwestHttpCallHandler`]，均实现
//! 既有 [`RemoteCallHandler`] trait，分别通过真实 tonic gRPC 和 reqwest HTTP/JSON
//! 调用远端跨语言事务参与者的 prepare/commit/rollback 端点。
//!
//! # feature gate
//!
//! 仅在 `cross-lang-dtx` feature 启用时编译。

use super::protocol::RemoteCallHandler;
use super::{CrossLangTxError, ParticipantAuth, ParticipantResponse, COORDINATOR_PROTOCOL_VERSION};
use parking_lot::RwLock;
use std::sync::Arc;
use std::time::Duration;

// ── tonic proto 消息（由 build.rs 生成） ──────────────────────────────
mod proto {
    tonic::include_proto!("szorm.dtx");
}

use proto::cross_lang_tx_service_client::CrossLangTxServiceClient;
use proto::{CrossLangTxRequest, CrossLangTxResponse};

// ============================================================================
// RealTransportConfig — 真实传输配置
// ============================================================================

/// 真实传输配置
///
/// 控制 gRPC/HTTP 传输的默认超时、最大重试次数和 TLS 配置。
#[derive(Debug, Clone)]
pub struct RealTransportConfig {
    /// 默认超时（毫秒）
    pub default_timeout_ms: u64,
    /// 最大重试次数
    pub max_retries: u32,
    /// TLS 配置（None 表示不使用 TLS）
    pub tls_config: Option<TlsConfig>,
}

/// TLS 配置
#[derive(Debug, Clone)]
pub struct TlsConfig {
    /// 客户端证书（mTLS，可选）
    pub cert: Option<Vec<u8>>,
    /// 客户端私钥（mTLS，可选）
    pub key: Option<Vec<u8>>,
    /// CA 证书
    pub ca: Vec<u8>,
}

impl RealTransportConfig {
    /// 创建默认配置（timeout 5000ms, max_retries 3, 无 TLS）
    pub fn new() -> Self {
        Self {
            default_timeout_ms: 5000,
            max_retries: 3,
            tls_config: None,
        }
    }

    /// 设置默认超时（链式）
    #[must_use]
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.default_timeout_ms = timeout_ms;
        self
    }

    /// 设置最大重试次数（链式）
    #[must_use]
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// 设置 TLS 配置（链式）
    #[must_use]
    pub fn with_tls_config(mut self, tls_config: TlsConfig) -> Self {
        self.tls_config = Some(tls_config);
        self
    }
}

impl Default for RealTransportConfig {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// TonicGrpcCallHandler — 真实 tonic gRPC 传输
// ============================================================================

/// 真实 tonic gRPC 调用处理器
///
/// 通过 tonic + prost 调用远端跨语言参与者的 `CrossLangTxService.Call` RPC，
/// 实现 [`RemoteCallHandler`] trait。支持 mTLS 双向认证与 Token 认证。
///
/// # 协议
///
/// 远端参与者需实现 `szorm.dtx.CrossLangTxService` gRPC 服务（见
/// `proto/cross_lang_tx.proto`），统一入口 `Call(CrossLangTxRequest)`，
/// 按 `method` 字段分发 prepare/commit/rollback/query_status。
///
/// # 超时
///
/// 通过 `tokio::time::timeout` 包装 gRPC 调用，超时返回
/// [`CrossLangTxError::Timeout`]。
///
/// # 鉴权
///
/// - [`ParticipantAuth::Mtls`]：配置 tonic TLS（双向认证）
/// - [`ParticipantAuth::Token`]：注入 gRPC metadata `authorization: Bearer {token}`
pub struct TonicGrpcCallHandler {
    /// gRPC 端点地址（如 `http://127.0.0.1:50051` 或 `https://...`）
    endpoint: String,
    /// 鉴权方式
    auth: ParticipantAuth,
    /// 超时（毫秒）
    timeout_ms: u64,
    /// 协议版本
    protocol_version: u32,
    /// 懒连接的 gRPC 客户端（缓存在 OnceCell 中）
    client: RwLock<Option<CrossLangTxServiceClient<tonic::transport::Channel>>>,
}

impl TonicGrpcCallHandler {
    /// 创建新的 tonic gRPC 调用处理器
    ///
    /// `endpoint` 应为不含 scheme 的地址（如 `127.0.0.1:50051`），
    /// 内部会根据 auth 自动拼接 `http://` 或 `https://` 前缀。
    pub fn new(endpoint: String, auth: ParticipantAuth) -> Self {
        Self {
            endpoint,
            auth,
            timeout_ms: 5000,
            protocol_version: COORDINATOR_PROTOCOL_VERSION,
            client: RwLock::new(None),
        }
    }

    /// 设置超时（链式，默认 5000ms）
    #[must_use]
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    /// 设置协议版本（链式）
    #[must_use]
    pub fn with_protocol_version(mut self, version: u32) -> Self {
        self.protocol_version = version;
        self
    }

    /// 返回端点地址
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// 返回协议版本
    pub fn protocol_version(&self) -> u32 {
        self.protocol_version
    }

    /// 构建 tonic endpoint URL（根据 auth 决定 http/https）
    fn endpoint_url(&self) -> String {
        let scheme = if matches!(self.auth, ParticipantAuth::Mtls { .. }) {
            "https"
        } else {
            "http"
        };
        format!("{scheme}://{}", self.endpoint)
    }

    /// 构建 gRPC metadata（Token 认证注入 authorization header）
    fn build_metadata(&self) -> Result<tonic::metadata::MetadataMap, CrossLangTxError> {
        let mut metadata = tonic::metadata::MetadataMap::new();
        if let ParticipantAuth::Token(ref token) = self.auth {
            let header_value = format!("Bearer {token}")
                .parse::<tonic::metadata::MetadataValue<tonic::metadata::Ascii>>()
                .map_err(|_e| CrossLangTxError::AuthFailed)?;
            metadata.insert("authorization", header_value);
        }
        Ok(metadata)
    }

    /// 懒连接：获取或创建 gRPC 客户端
    ///
    /// 在当前 tokio 运行时中 block_on 连接。连接失败返回
    /// [`CrossLangTxError::Transport`]。
    fn get_or_connect(
        &self,
    ) -> Result<CrossLangTxServiceClient<tonic::transport::Channel>, CrossLangTxError> {
        if let Some(client) = self.client.read().clone() {
            return Ok(client);
        }
        let url = self.endpoint_url();
        let endpoint = tonic::transport::Endpoint::from_shared(url.clone())
            .map_err(|e| CrossLangTxError::Transport(format!("invalid endpoint {url}: {e}")))?
            .timeout(Duration::from_millis(self.timeout_ms));

        // mTLS 配置（如有）
        let endpoint = match &self.auth {
            ParticipantAuth::Mtls { cert, key, ca } => {
                let mut tls = tonic::transport::ClientTlsConfig::new()
                    .ca_certificate(tonic::transport::Certificate::from_pem(ca.clone()));
                if !cert.is_empty() && !key.is_empty() {
                    let identity = tonic::transport::Identity::from_pem(cert.clone(), key.clone());
                    tls = tls.identity(identity);
                }
                endpoint
                    .tls_config(tls)
                    .map_err(|e| CrossLangTxError::Transport(format!("TLS config failed: {e}")))?
            }
            _ => endpoint,
        };

        // block_on 连接（在当前 tokio 运行时或创建临时运行时）
        let channel = block_on(async move { endpoint.connect().await })
            .ok_or_else(|| CrossLangTxError::Transport("connect timeout".to_string()))?
            .map_err(|e| CrossLangTxError::Transport(format!("connect failed: {e}")))?;
        let client = CrossLangTxServiceClient::new(channel);
        *self.client.write() = Some(client.clone());
        Ok(client)
    }

    /// 将 ParticipantResponse 从 proto 响应转换
    fn from_proto_response(resp: CrossLangTxResponse) -> ParticipantResponse {
        ParticipantResponse {
            success: resp.success,
            payload: resp.payload,
            error: if resp.error.is_empty() {
                None
            } else {
                Some(resp.error)
            },
            latency_ms: resp.latency_ms,
        }
    }
}

impl RemoteCallHandler for TonicGrpcCallHandler {
    fn call(
        &self,
        method: &str,
        tx_id: &str,
        payload: &[u8],
    ) -> Result<ParticipantResponse, CrossLangTxError> {
        // 协议版本检查（与远端协商；此处检查本地版本一致性）
        super::protocol::check_protocol_version(
            COORDINATOR_PROTOCOL_VERSION,
            self.protocol_version,
        )?;

        let mut client = self.get_or_connect()?;
        let metadata = self.build_metadata()?;

        let mut request = tonic::Request::new(CrossLangTxRequest {
            method: method.to_string(),
            tx_id: tx_id.to_string(),
            payload: payload.to_vec(),
        });
        *request.metadata_mut() = metadata;

        // tokio::time::timeout 包装 gRPC 调用
        let timeout = Duration::from_millis(self.timeout_ms);
        let result =
            block_on(async move { tokio::time::timeout(timeout, client.call(request)).await })
                .ok_or(CrossLangTxError::Timeout)?;

        match result {
            Ok(Ok(resp)) => {
                let tx_resp = resp.into_inner();
                Ok(Self::from_proto_response(tx_resp))
            }
            Ok(Err(status)) => {
                let code = status.code();
                if code == tonic::Code::Unauthenticated || code == tonic::Code::PermissionDenied {
                    Err(CrossLangTxError::AuthFailed)
                } else if code == tonic::Code::DeadlineExceeded {
                    Err(CrossLangTxError::Timeout)
                } else {
                    Err(CrossLangTxError::Transport(format!("gRPC error: {status}")))
                }
            }
            Err(_) => Err(CrossLangTxError::Timeout),
        }
    }
}

impl std::fmt::Debug for TonicGrpcCallHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TonicGrpcCallHandler")
            .field("endpoint", &self.endpoint)
            .field("timeout_ms", &self.timeout_ms)
            .field("protocol_version", &self.protocol_version)
            .finish_non_exhaustive()
    }
}

// ============================================================================
// ReqwestHttpCallHandler — 真实 HTTP/JSON 传输
// ============================================================================

/// HTTP/JSON 请求体
#[derive(serde::Serialize)]
struct HttpCallBody {
    method: String,
    tx_id: String,
    payload: Vec<u8>,
}

/// HTTP/JSON 响应体
#[derive(serde::Deserialize)]
struct HttpCallResponse {
    success: bool,
    #[serde(default)]
    payload: Vec<u8>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    latency_ms: u64,
}

/// 真实 reqwest HTTP/JSON 调用处理器
///
/// 通过 reqwest POST 调用远端参与者的 `{endpoint}/{method}` 端点，
/// 实现 [`RemoteCallHandler`] trait。支持 Token 认证与超时控制。
///
/// # 协议
///
/// 远端参与者需暴露以下 HTTP 端点：
/// - `POST {endpoint}/prepare` — body: `{"method","tx_id","payload"}`
/// - `POST {endpoint}/commit`
/// - `POST {endpoint}/rollback`
/// - `POST {endpoint}/query_status`
///
/// 响应体：`{"success","payload","error","latency_ms"}`
pub struct ReqwestHttpCallHandler {
    /// HTTP 端点基础 URL（如 `http://127.0.0.1:8080/api/tx`）
    endpoint: String,
    /// Bearer Token
    token: String,
    /// 超时（毫秒）
    timeout_ms: u64,
    /// 协议版本
    protocol_version: u32,
    /// 懒创建的 reqwest 客户端
    client: RwLock<Option<reqwest::Client>>,
}

impl ReqwestHttpCallHandler {
    /// 创建新的 HTTP/JSON 调用处理器
    pub fn new(endpoint: String, token: String) -> Self {
        Self {
            endpoint,
            token,
            timeout_ms: 5000,
            protocol_version: COORDINATOR_PROTOCOL_VERSION,
            client: RwLock::new(None),
        }
    }

    /// 设置超时（链式，默认 5000ms）
    #[must_use]
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    /// 设置协议版本（链式）
    #[must_use]
    pub fn with_protocol_version(mut self, version: u32) -> Self {
        self.protocol_version = version;
        self
    }

    /// 返回端点地址
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// 返回协议版本
    pub fn protocol_version(&self) -> u32 {
        self.protocol_version
    }

    /// 传输安全校验（v4.8.0 修复 H-5）
    ///
    /// 参与者使用 Token 认证时，端点必须是 `https://`（TLS 加密）或本机
    /// 回环地址（127.0.0.1 / localhost / [::1]，开发测试用）。
    ///
    /// 修复前生产端点可配置为 `http://` —— Bearer Token 明文传输，
    /// 网络窃听即可获取参与者认证凭据并伪造跨语言事务请求。
    ///
    /// 仅校验 `http://` 前缀端点：`https://` 与非 http scheme 直接放行。
    fn assert_transport_secure(&self) -> Result<(), CrossLangTxError> {
        let lower = self.endpoint.to_ascii_lowercase();
        if !lower.starts_with("http://") {
            // https:// 或非 http scheme（如 grpc://）不在此校验范围
            return Ok(());
        }
        let is_loopback = lower.starts_with("http://127.0.0.1")
            || lower.starts_with("http://localhost")
            || lower.starts_with("http://[::1]")
            || lower.starts_with("http://0.0.0.0");
        if is_loopback {
            return Ok(());
        }
        Err(CrossLangTxError::Transport(format!(
            "insecure transport: endpoint `{}` must use https (TLS) — token authentication over plaintext HTTP leaks credentials (H-5)",
            self.endpoint
        )))
    }

    /// 懒创建 reqwest 客户端
    fn get_or_create_client(&self) -> Result<reqwest::Client, CrossLangTxError> {
        if let Some(client) = self.client.read().clone() {
            return Ok(client);
        }
        // v4.8.0 修复 H-5：首次调用前校验传输安全性（每次调用仅一次，
        // client 缓存后不再重复校验）
        self.assert_transport_secure()?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(self.timeout_ms))
            .build()
            .map_err(|e| CrossLangTxError::Transport(format!("reqwest build failed: {e}")))?;
        *self.client.write() = Some(client.clone());
        Ok(client)
    }
}

impl RemoteCallHandler for ReqwestHttpCallHandler {
    fn call(
        &self,
        method: &str,
        tx_id: &str,
        payload: &[u8],
    ) -> Result<ParticipantResponse, CrossLangTxError> {
        // 协议版本检查
        super::protocol::check_protocol_version(
            COORDINATOR_PROTOCOL_VERSION,
            self.protocol_version,
        )?;

        let client = self.get_or_create_client()?;
        let url = format!("{}/{method}", self.endpoint);
        let body = HttpCallBody {
            method: method.to_string(),
            tx_id: tx_id.to_string(),
            payload: payload.to_vec(),
        };

        let timeout = Duration::from_millis(self.timeout_ms);
        let token = self.token.clone();

        let result = block_on(async move {
            tokio::time::timeout(
                timeout,
                client
                    .post(&url)
                    .header("Authorization", format!("Bearer {token}"))
                    .header("Content-Type", "application/json")
                    .json(&body)
                    .send(),
            )
            .await
        })
        .ok_or(CrossLangTxError::Timeout)?;

        let response = match result {
            Ok(Ok(resp)) => resp,
            Ok(Err(e)) => {
                if e.is_timeout() {
                    return Err(CrossLangTxError::Timeout);
                }
                return Err(CrossLangTxError::Transport(format!(
                    "HTTP send failed: {e}"
                )));
            }
            Err(_) => return Err(CrossLangTxError::Timeout),
        };

        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(CrossLangTxError::AuthFailed);
        }
        if !status.is_success() {
            return Err(CrossLangTxError::Transport(format!("HTTP {status}")));
        }

        // 解析 JSON 响应
        let json_result = block_on(async move { response.json::<HttpCallResponse>().await });
        let http_resp: HttpCallResponse = match json_result {
            Some(Ok(resp)) => resp,
            Some(Err(e)) => {
                return Err(CrossLangTxError::Transport(format!(
                    "JSON parse failed: {e}"
                )))
            }
            None => return Err(CrossLangTxError::Timeout),
        };

        Ok(ParticipantResponse {
            success: http_resp.success,
            payload: http_resp.payload,
            error: http_resp.error,
            latency_ms: http_resp.latency_ms,
        })
    }
}

impl std::fmt::Debug for ReqwestHttpCallHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReqwestHttpCallHandler")
            .field("endpoint", &self.endpoint)
            .field("timeout_ms", &self.timeout_ms)
            .field("protocol_version", &self.protocol_version)
            .finish_non_exhaustive()
    }
}

// ============================================================================
// block_on 辅助 — 在同步上下文中执行 async future
// ============================================================================

/// 在当前 tokio 运行时中 block_on，或创建临时运行时。
///
/// 在已有 runtime 上下文中使用 `block_in_place` + `handle.block_on`，
/// 避免在 async context 中直接 `block_on` 导致 panic。
fn block_on<F>(fut: F) -> Option<F::Output>
where
    F: std::future::Future + Send,
    F::Output: Send,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        return Some(tokio::task::block_in_place(|| handle.block_on(fut)));
    }
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .ok()
        .map(|rt| rt.block_on(fut))
}

// ============================================================================
// 真实 gRPC 服务器（用于测试 + SDK 契约验证）
// ============================================================================

/// 跨语言事务 gRPC 服务器句柄
///
/// 启动一个真实 tonic gRPC 服务器，承载给定的请求处理器，
/// 用于测试和 SDK 契约验证。服务器在独立 tokio task 中运行。
pub struct CrossLangTxServerHandle {
    local_addr: std::net::SocketAddr,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    join: Option<tokio::task::JoinHandle<Result<(), tonic::transport::Error>>>,
}

impl CrossLangTxServerHandle {
    /// 返回服务器实际监听地址
    pub fn local_addr(&self) -> std::net::SocketAddr {
        self.local_addr
    }

    /// 优雅停止服务器
    pub async fn stop(&mut self) -> Result<(), CrossLangTxError> {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(join) = self.join.take() {
            let _ = join.await;
        }
        Ok(())
    }
}

impl Drop for CrossLangTxServerHandle {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(join) = self.join.take() {
            join.abort();
        }
    }
}

/// gRPC 请求处理器 trait（用于服务器端）
pub trait GrpcCallHandler: Send + Sync + 'static {
    fn handle_call(&self, method: &str, tx_id: &str, payload: &[u8]) -> ParticipantResponse;
}

#[tonic::async_trait]
impl proto::cross_lang_tx_service_server::CrossLangTxService for Arc<dyn GrpcCallHandler> {
    async fn call(
        &self,
        request: tonic::Request<CrossLangTxRequest>,
    ) -> Result<tonic::Response<CrossLangTxResponse>, tonic::Status> {
        let req = request.into_inner();
        let resp =
            GrpcCallHandler::handle_call(self.as_ref(), &req.method, &req.tx_id, &req.payload);
        Ok(tonic::Response::new(CrossLangTxResponse {
            success: resp.success,
            payload: resp.payload,
            error: resp.error.unwrap_or_default(),
            latency_ms: resp.latency_ms,
        }))
    }
}

/// 启动真实 tonic gRPC 服务器
///
/// 在 `addr` 上监听，使用给定的 `handler` 处理请求。
/// 必须在 tokio 运行时上下文中调用。
pub async fn start_grpc_server(
    addr: std::net::SocketAddr,
    handler: Arc<dyn GrpcCallHandler>,
) -> Result<CrossLangTxServerHandle, CrossLangTxError> {
    use proto::cross_lang_tx_service_server::CrossLangTxServiceServer;
    use tokio_stream::wrappers::TcpListenerStream;

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| CrossLangTxError::Transport(format!("bind {addr} failed: {e}")))?;
    let local_addr = listener
        .local_addr()
        .map_err(|e| CrossLangTxError::Transport(format!("local_addr: {e}")))?;
    let incoming = TcpListenerStream::new(listener);

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let serve = tonic::transport::Server::builder()
        .add_service(CrossLangTxServiceServer::new(handler))
        .serve_with_incoming_shutdown(incoming, async move {
            let _ = shutdown_rx.await;
        });
    let join = tokio::spawn(serve);

    Ok(CrossLangTxServerHandle {
        local_addr,
        shutdown_tx: Some(shutdown_tx),
        join: Some(join),
    })
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    // ── M1-T1.7: RealTransportConfig 默认值 ──

    #[test]
    fn test_real_transport_config_default() {
        let config = RealTransportConfig::new();
        assert_eq!(config.default_timeout_ms, 5000);
        assert_eq!(config.max_retries, 3);
        assert!(config.tls_config.is_none());
    }

    #[test]
    fn test_real_transport_config_builder() {
        let config = RealTransportConfig::new()
            .with_timeout(10000)
            .with_max_retries(5)
            .with_tls_config(TlsConfig {
                cert: Some(vec![]),
                key: Some(vec![]),
                ca: vec![],
            });
        assert_eq!(config.default_timeout_ms, 10000);
        assert_eq!(config.max_retries, 5);
        assert!(config.tls_config.is_some());
    }

    // ── M1-T2.1/T2.2: TonicGrpcCallHandler 基础 ──

    #[test]
    fn test_tonic_handler_new() {
        let handler = TonicGrpcCallHandler::new(
            "127.0.0.1:50051".to_string(),
            ParticipantAuth::Token("test-token".to_string()),
        );
        assert_eq!(handler.endpoint(), "127.0.0.1:50051");
        assert_eq!(handler.timeout_ms, 5000);
        assert_eq!(handler.protocol_version(), COORDINATOR_PROTOCOL_VERSION);
    }

    #[test]
    fn test_tonic_handler_with_timeout() {
        let handler = TonicGrpcCallHandler::new(
            "127.0.0.1:50051".to_string(),
            ParticipantAuth::Token("t".to_string()),
        )
        .with_timeout(3000);
        assert_eq!(handler.timeout_ms, 3000);
    }

    // ── M1-T2.12: 协议版本不匹配 ──

    #[test]
    fn test_tonic_handler_protocol_version_mismatch() {
        let handler = TonicGrpcCallHandler::new(
            "127.0.0.1:50051".to_string(),
            ParticipantAuth::Token("t".to_string()),
        )
        .with_protocol_version(99);
        let result = handler.call("prepare", "tx-001", &[]);
        assert!(matches!(
            result,
            Err(CrossLangTxError::ProtocolVersionMismatch { .. })
        ));
    }

    // ── M1-T2.9: 真实 gRPC 调用 ──

    /// 测试用 gRPC 请求处理器
    struct TestGrpcHandler {
        prepare_count: AtomicU32,
        commit_count: AtomicU32,
        rollback_count: AtomicU32,
    }

    impl TestGrpcHandler {
        fn new() -> Self {
            Self {
                prepare_count: AtomicU32::new(0),
                commit_count: AtomicU32::new(0),
                rollback_count: AtomicU32::new(0),
            }
        }
    }

    impl GrpcCallHandler for TestGrpcHandler {
        fn handle_call(&self, method: &str, tx_id: &str, _payload: &[u8]) -> ParticipantResponse {
            match method {
                "prepare" => {
                    self.prepare_count.fetch_add(1, Ordering::SeqCst);
                    ParticipantResponse {
                        success: true,
                        payload: format!("prepared:{tx_id}").into_bytes(),
                        error: None,
                        latency_ms: 5,
                    }
                }
                "commit" => {
                    self.commit_count.fetch_add(1, Ordering::SeqCst);
                    ParticipantResponse {
                        success: true,
                        payload: vec![],
                        error: None,
                        latency_ms: 3,
                    }
                }
                "rollback" => {
                    self.rollback_count.fetch_add(1, Ordering::SeqCst);
                    ParticipantResponse {
                        success: true,
                        payload: vec![],
                        error: None,
                        latency_ms: 2,
                    }
                }
                _ => ParticipantResponse {
                    success: false,
                    payload: vec![],
                    error: Some(format!("unknown method: {method}")),
                    latency_ms: 0,
                },
            }
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_tonic_grpc_real_call_prepare() {
        let handler = Arc::new(TestGrpcHandler::new());
        let server = start_grpc_server("127.0.0.1:0".parse().unwrap(), handler.clone())
            .await
            .unwrap();
        let addr = server.local_addr();

        let client =
            TonicGrpcCallHandler::new(addr.to_string(), ParticipantAuth::Token("test".to_string()))
                .with_timeout(2000);

        let result = client.call("prepare", "tx-grpc-001", &[]);
        assert!(result.is_ok(), "prepare should succeed: {:?}", result);
        let resp = result.unwrap();
        assert!(resp.success);
        assert_eq!(
            String::from_utf8(resp.payload).unwrap(),
            "prepared:tx-grpc-001"
        );

        drop(server);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_tonic_grpc_real_call_commit_rollback() {
        let handler = Arc::new(TestGrpcHandler::new());
        let server = start_grpc_server("127.0.0.1:0".parse().unwrap(), handler.clone())
            .await
            .unwrap();
        let addr = server.local_addr();

        let client =
            TonicGrpcCallHandler::new(addr.to_string(), ParticipantAuth::Token("test".to_string()))
                .with_timeout(2000);

        let resp = client.call("commit", "tx-002", &[]).unwrap();
        assert!(resp.success);

        let resp = client.call("rollback", "tx-002", &[]).unwrap();
        assert!(resp.success);

        assert_eq!(handler.commit_count.load(Ordering::SeqCst), 1);
        assert_eq!(handler.rollback_count.load(Ordering::SeqCst), 1);

        drop(server);
    }

    // ── M1-T2.11: gRPC 端点不可达 ──

    #[test]
    fn test_tonic_grpc_unreachable() {
        // 使用一个确定不可达的端口（1 通常被拒绝）
        let client = TonicGrpcCallHandler::new(
            "127.0.0.1:1".to_string(),
            ParticipantAuth::Token("t".to_string()),
        )
        .with_timeout(500);

        let result = client.call("prepare", "tx-003", &[]);
        assert!(result.is_err());
        // 可能是 Transport（连接拒绝）或 Timeout
        assert!(matches!(
            result,
            Err(CrossLangTxError::Transport(_)) | Err(CrossLangTxError::Timeout)
        ));
    }

    // ── M1-T2.10: mTLS 认证（证书配置） ──

    #[test]
    fn test_tonic_handler_mtls_config() {
        // 验证 mTLS 配置可以正确创建 handler
        let handler = TonicGrpcCallHandler::new(
            "127.0.0.1:50052".to_string(),
            ParticipantAuth::Mtls {
                cert: vec![0x01, 0x02],
                key: vec![0x03, 0x04],
                ca: vec![0x05, 0x06],
            },
        )
        .with_timeout(500);

        // handler 创建成功，endpoint 和 auth 正确
        assert_eq!(handler.endpoint(), "127.0.0.1:50052");
        // endpoint_url 应使用 https scheme
        assert_eq!(handler.endpoint_url(), "https://127.0.0.1:50052");
    }

    // ── M1-T2.13: 性能测试（序列化 + 协议处理 ≤ 100ms） ──

    #[tokio::test(flavor = "multi_thread")]
    async fn test_tonic_grpc_latency() {
        let handler = Arc::new(TestGrpcHandler::new());
        let server = start_grpc_server("127.0.0.1:0".parse().unwrap(), handler)
            .await
            .unwrap();
        let addr = server.local_addr();

        let client =
            TonicGrpcCallHandler::new(addr.to_string(), ParticipantAuth::Token("t".to_string()))
                .with_timeout(5000);

        let start = std::time::Instant::now();
        let _ = client.call("prepare", "tx-perf", &[]).unwrap();
        let elapsed = start.elapsed();

        // 含本地 TCP RTT，放宽到 500ms（不含网络 RTT 时 ≤ 100ms）
        assert!(elapsed.as_millis() < 500, "latency {elapsed:?} too high");

        drop(server);
    }

    // ── M1-T3.1/T3.2: ReqwestHttpCallHandler 基础 ──

    #[test]
    fn test_reqwest_handler_new() {
        let handler = ReqwestHttpCallHandler::new(
            "http://127.0.0.1:8080/api/tx".to_string(),
            "test-token".to_string(),
        );
        assert_eq!(handler.endpoint(), "http://127.0.0.1:8080/api/tx");
        assert_eq!(handler.timeout_ms, 5000);
        assert_eq!(handler.protocol_version(), COORDINATOR_PROTOCOL_VERSION);
    }

    #[test]
    fn test_reqwest_handler_with_timeout() {
        let handler =
            ReqwestHttpCallHandler::new("http://127.0.0.1:8080".to_string(), "t".to_string())
                .with_timeout(3000);
        assert_eq!(handler.timeout_ms, 3000);
    }

    // ── v4.8.0 修复 H-5：Token 认证禁止明文 HTTP（非回环）──

    #[test]
    fn test_h5_loopback_http_allowed() {
        let handler =
            ReqwestHttpCallHandler::new("http://127.0.0.1:8080".to_string(), "t".to_string());
        assert!(handler.assert_transport_secure().is_ok());
        let handler =
            ReqwestHttpCallHandler::new("http://localhost:8080".to_string(), "t".to_string());
        assert!(handler.assert_transport_secure().is_ok());
    }

    #[test]
    fn test_h5_https_allowed() {
        let handler = ReqwestHttpCallHandler::new(
            "https://participant.example.com".to_string(),
            "t".to_string(),
        );
        assert!(handler.assert_transport_secure().is_ok());
    }

    #[test]
    fn test_h5_plaintext_remote_rejected() {
        // 修复前（黑帽审计 H-5）：生产端点可用 http:// 明文传输 Bearer Token
        let handler = ReqwestHttpCallHandler::new(
            "http://participant.example.com:8080/api/tx".to_string(),
            "super-secret-token".to_string(),
        );
        let result = handler.assert_transport_secure();
        assert!(
            matches!(result, Err(CrossLangTxError::Transport(_))),
            "非回环 http 端点必须被拒绝（H-5 修复失效）"
        );
    }

    // ── M1-T3.12: 协议版本不匹配 ──

    #[test]
    fn test_reqwest_handler_protocol_version_mismatch() {
        let handler =
            ReqwestHttpCallHandler::new("http://127.0.0.1:8080".to_string(), "t".to_string())
                .with_protocol_version(99);
        let result = handler.call("commit", "tx-001", &[]);
        assert!(matches!(
            result,
            Err(CrossLangTxError::ProtocolVersionMismatch { .. })
        ));
    }

    // ── M1-T3.8: 真实 HTTP 调用 ──

    #[tokio::test(flavor = "multi_thread")]
    async fn test_reqwest_http_real_call() {
        // 启动一个简单的 mock HTTP 服务器
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        async fn handle_connection(
            mut stream: tokio::net::TcpStream,
            prepare_count: Arc<AtomicU32>,
            commit_count: Arc<AtomicU32>,
        ) {
            let mut buf = vec![0u8; 4096];
            let _ = stream.read(&mut buf).await;
            let request_str = String::from_utf8_lossy(&buf);

            let (method_path, response_body) = if request_str.contains("POST /prepare") {
                prepare_count.fetch_add(1, Ordering::SeqCst);
                (
                    "prepare",
                    r#"{"success":true,"payload":[1,2,3],"error":null,"latency_ms":5}"#,
                )
            } else if request_str.contains("POST /commit") {
                commit_count.fetch_add(1, Ordering::SeqCst);
                (
                    "commit",
                    r#"{"success":true,"payload":[],"error":null,"latency_ms":3}"#,
                )
            } else if request_str.contains("POST /rollback") {
                (
                    "rollback",
                    r#"{"success":true,"payload":[],"error":null,"latency_ms":2}"#,
                )
            } else {
                (
                    "unknown",
                    r#"{"success":false,"payload":[],"error":"unknown","latency_ms":0}"#,
                )
            };

            let _ = method_path;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            let _ = stream.write_all(response.as_bytes()).await;
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let prepare_count = Arc::new(AtomicU32::new(0));
        let commit_count = Arc::new(AtomicU32::new(0));

        let pc = prepare_count.clone();
        let cc = commit_count.clone();
        let server_task = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let pc = pc.clone();
                let cc = cc.clone();
                tokio::spawn(handle_connection(stream, pc, cc));
            }
        });

        let client =
            ReqwestHttpCallHandler::new(format!("http://{addr}"), "test-token".to_string())
                .with_timeout(3000);

        let resp = client.call("prepare", "tx-http-001", &[]).unwrap();
        assert!(resp.success);
        assert_eq!(resp.payload, vec![1, 2, 3]);

        let resp = client.call("commit", "tx-http-001", &[]).unwrap();
        assert!(resp.success);

        assert!(prepare_count.load(Ordering::SeqCst) >= 1);
        assert!(commit_count.load(Ordering::SeqCst) >= 1);

        server_task.abort();
    }

    // ── M1-T3.10: HTTP 端点不可达 ──

    #[test]
    fn test_reqwest_http_unreachable() {
        let client = ReqwestHttpCallHandler::new("http://127.0.0.1:1".to_string(), "t".to_string())
            .with_timeout(500);

        let result = client.call("commit", "tx-004", &[]);
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(CrossLangTxError::Transport(_)) | Err(CrossLangTxError::Timeout)
        ));
    }
}
