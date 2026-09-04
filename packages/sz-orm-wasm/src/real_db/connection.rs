//! WasmRealDbConnection — WASM ↔ 后端 DB 代理连接
//!
//! 通过 HTTP 或 WebSocket 代理桥接后端数据库，WASM 端不直接持有 DB 凭据。

use super::protocol::{ProxyRequest, ProxyResponse, SerializationFormat};
use super::WasmRealDbError;

/// 传输方式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmTransport {
    /// HTTP 请求/响应
    Http,
    /// WebSocket 长连接
    WebSocket,
}

/// WASM 真实 DB 连接
///
/// 持有代理 URL、会话 ID、Token，不持有任何 DB 凭据。
/// 所有查询通过代理转发到后端执行。
#[derive(Debug, Clone)]
pub struct WasmRealDbConnection {
    proxy_url: String,
    transport: WasmTransport,
    session_id: String,
    token: String,
    serialization_format: SerializationFormat,
    connected: bool,
}

impl WasmRealDbConnection {
    /// 创建新连接（未连接状态）
    pub fn new(
        proxy_url: &str,
        transport: WasmTransport,
        session_id: &str,
        token: &str,
        serialization_format: SerializationFormat,
    ) -> Self {
        Self {
            proxy_url: proxy_url.to_string(),
            transport,
            session_id: session_id.to_string(),
            token: token.to_string(),
            serialization_format,
            connected: false,
        }
    }

    /// 标记为已连接（实际连接由 transport 层建立）
    pub fn connect(&mut self) -> Result<(), WasmRealDbError> {
        if self.proxy_url.is_empty() {
            return Err(WasmRealDbError::ProxyUnavailable);
        }
        if self.token.is_empty() {
            return Err(WasmRealDbError::AuthFailed);
        }
        self.connected = true;
        Ok(())
    }

    /// 是否已连接
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// 代理 URL
    pub fn proxy_url(&self) -> &str {
        &self.proxy_url
    }

    /// 传输方式
    pub fn transport(&self) -> WasmTransport {
        self.transport
    }

    /// 会话 ID
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Token
    pub fn token(&self) -> &str {
        &self.token
    }

    /// 序列化格式
    pub fn serialization_format(&self) -> SerializationFormat {
        self.serialization_format
    }

    /// 序列化请求为字节
    pub fn serialize_request(&self, request: &ProxyRequest) -> Result<Vec<u8>, WasmRealDbError> {
        request.serialize(self.serialization_format)
    }

    /// 反序列化响应
    pub fn deserialize_response(&self, bytes: &[u8]) -> Result<ProxyResponse, WasmRealDbError> {
        ProxyResponse::deserialize(bytes, self.serialization_format)
    }

    /// 构建代理请求
    pub fn build_request(
        &self,
        query: crate::WasmQuery,
        transaction_id: Option<String>,
    ) -> ProxyRequest {
        ProxyRequest {
            session_id: self.session_id.clone(),
            token: self.token.clone(),
            query,
            transaction_id,
        }
    }

    /// 关闭连接
    pub fn close(&mut self) {
        self.connected = false;
    }

    /// 通过 HTTP 发送请求（reqwest）
    ///
    /// 返回反序列化后的代理响应。
    #[cfg(feature = "wasm-real-db")]
    pub async fn send_request_http(
        &self,
        request: &ProxyRequest,
    ) -> Result<ProxyResponse, WasmRealDbError> {
        if !self.connected {
            return Err(WasmRealDbError::ProxyUnavailable);
        }

        Self::validate_proxy_url(&self.proxy_url)?;

        let body = self.serialize_request(request)?;
        let resp = reqwest::Client::new()
            .post(&self.proxy_url)
            .header("Content-Type", "application/octet-stream")
            .body(body)
            .send()
            .await
            .map_err(|e| WasmRealDbError::QueryFailed {
                reason: e.to_string(),
            })?;

        if !resp.status().is_success() {
            return Err(WasmRealDbError::QueryFailed {
                reason: format!("HTTP {}", resp.status()),
            });
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| WasmRealDbError::SerializationError(e.to_string()))?;
        self.deserialize_response(&bytes)
    }

    /// 验证代理 URL 安全性：只允许 http/https 协议。
    pub fn validate_proxy_url(url: &str) -> Result<(), WasmRealDbError> {
        let lower = url.to_lowercase();
        if !lower.starts_with("http://") && !lower.starts_with("https://") {
            return Err(WasmRealDbError::QueryFailed {
                reason: format!("proxy_url must use http or https protocol, got: {}", url),
            });
        }
        Ok(())
    }
}

impl PartialEq for WasmRealDbConnection {
    fn eq(&self, other: &Self) -> bool {
        self.proxy_url == other.proxy_url
            && self.transport == other.transport
            && self.session_id == other.session_id
            && self.connected == other.connected
    }
}

#[cfg(test)]
fn futures_executor_block_on<F: std::future::Future>(f: F) -> F::Output {
    use std::pin::Pin;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    fn dummy_raw_waker() -> RawWaker {
        fn no_op(_: *const ()) {}
        fn clone(_: *const ()) -> RawWaker {
            dummy_raw_waker()
        }
        static V_TABLE: RawWakerVTable = RawWakerVTable::new(clone, no_op, no_op, no_op);
        RawWaker::new(std::ptr::null(), &V_TABLE)
    }

    let waker = unsafe { Waker::from_raw(dummy_raw_waker()) };
    let mut cx = Context::from_waker(&waker);
    let mut future = Box::pin(f);
    loop {
        match Pin::as_mut(&mut future).poll(&mut cx) {
            Poll::Ready(val) => return val,
            Poll::Pending => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WasmQuery;

    fn make_connection() -> WasmRealDbConnection {
        WasmRealDbConnection::new(
            "https://proxy.example.com/db",
            WasmTransport::Http,
            "sess-001",
            "token-xyz",
            SerializationFormat::Json,
        )
    }

    #[test]
    fn test_connection_new_not_connected() {
        let conn = make_connection();
        assert!(!conn.is_connected());
        assert_eq!(conn.proxy_url(), "https://proxy.example.com/db");
        assert_eq!(conn.transport(), WasmTransport::Http);
        assert_eq!(conn.session_id(), "sess-001");
        assert_eq!(conn.token(), "token-xyz");
        assert_eq!(conn.serialization_format(), SerializationFormat::Json);
    }

    #[test]
    fn test_validate_proxy_url_safe() {
        assert!(WasmRealDbConnection::validate_proxy_url("http://localhost:8080").is_ok());
        assert!(WasmRealDbConnection::validate_proxy_url("https://proxy.example.com/db").is_ok());
        assert!(WasmRealDbConnection::validate_proxy_url("HTTP://LOCALHOST:8080").is_ok());
    }

    #[test]
    fn test_validate_proxy_url_unsafe() {
        assert!(WasmRealDbConnection::validate_proxy_url("file:///etc/passwd").is_err());
        assert!(WasmRealDbConnection::validate_proxy_url("ftp://evil.com").is_err());
        assert!(WasmRealDbConnection::validate_proxy_url("gopher://evil.com").is_err());
        assert!(WasmRealDbConnection::validate_proxy_url("javascript:alert(1)").is_err());
    }

    #[test]
    fn test_connection_connect_success() {
        let mut conn = make_connection();
        assert!(conn.connect().is_ok());
        assert!(conn.is_connected());
    }

    #[test]
    fn test_connection_connect_empty_url() {
        let mut conn = WasmRealDbConnection::new(
            "",
            WasmTransport::Http,
            "sess",
            "token",
            SerializationFormat::Json,
        );
        assert!(matches!(
            conn.connect(),
            Err(WasmRealDbError::ProxyUnavailable)
        ));
    }

    #[test]
    fn test_connection_connect_empty_token() {
        let mut conn = WasmRealDbConnection::new(
            "https://proxy.example.com",
            WasmTransport::Http,
            "sess",
            "",
            SerializationFormat::Json,
        );
        assert!(matches!(conn.connect(), Err(WasmRealDbError::AuthFailed)));
    }

    #[test]
    fn test_connection_close() {
        let mut conn = make_connection();
        conn.connect().unwrap();
        assert!(conn.is_connected());
        conn.close();
        assert!(!conn.is_connected());
    }

    #[test]
    fn test_build_request() {
        let conn = make_connection();
        let query = WasmQuery::with_params(
            "SELECT * FROM users WHERE id = ?",
            vec![serde_json::json!(1)],
        );
        let req = conn.build_request(query, None);
        assert_eq!(req.session_id, "sess-001");
        assert_eq!(req.token, "token-xyz");
        assert_eq!(req.query.sql, "SELECT * FROM users WHERE id = ?");
        assert_eq!(req.query.params.len(), 1);
        assert!(req.transaction_id.is_none());
    }

    #[test]
    fn test_build_request_with_transaction() {
        let conn = make_connection();
        let query = WasmQuery::new("SELECT 1");
        let req = conn.build_request(query, Some("tx-123".to_string()));
        assert_eq!(req.transaction_id, Some("tx-123".to_string()));
    }

    #[test]
    fn test_serialize_deserialize_roundtrip() {
        let conn = make_connection();
        let query = WasmQuery::with_params(
            "INSERT INTO users (name) VALUES (?)",
            vec![serde_json::json!("Alice")],
        );
        let req = conn.build_request(query, None);
        let bytes = conn.serialize_request(&req).unwrap();
        assert!(!bytes.is_empty());
    }

    #[test]
    fn test_msgpack_connection() {
        let conn = WasmRealDbConnection::new(
            "https://proxy.example.com/db",
            WasmTransport::WebSocket,
            "sess-002",
            "token-abc",
            SerializationFormat::MessagePack,
        );
        assert_eq!(
            conn.serialization_format(),
            SerializationFormat::MessagePack
        );
        assert_eq!(conn.transport(), WasmTransport::WebSocket);
    }

    #[test]
    fn test_connection_equality() {
        let conn1 = make_connection();
        let conn2 = make_connection();
        assert_eq!(conn1, conn2);
    }

    #[test]
    fn test_send_request_not_connected() {
        let conn = make_connection();
        let query = WasmQuery::new("SELECT 1");
        let req = conn.build_request(query, None);
        let result = futures_executor_block_on(conn.send_request_http(&req));
        assert!(matches!(result, Err(WasmRealDbError::ProxyUnavailable)));
    }
}
