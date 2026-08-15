//! 跨语言 SDK 接入契约
//!
//! 为 Go/Java/C++/Python/JavaScript 各语言提供参与者 SDK 接入契约，
//! 定义统一的端点签名、鉴权方式、序列化格式和协议版本。
//!
//! 复用既有 [`super::observability`] 可观测性 + [`super::COORDINATOR_PROTOCOL_VERSION`]。

use super::observability::{AlertEvent, AlertLevel, CrossLangTxAlerter};
use super::{ParticipantLanguage, ParticipantTransport, COORDINATOR_PROTOCOL_VERSION};
use serde::{Deserialize, Serialize};

// ============================================================================
// SDK 契约数据结构
// ============================================================================

/// 鉴权方案
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthScheme {
    /// mTLS 双向认证
    Mtls,
    /// Bearer Token
    Token,
    /// 无鉴权（仅开发/测试）
    None,
}

/// 序列化格式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SerializationFormat {
    /// Protocol Buffers（gRPC）
    Protobuf,
    /// JSON（HTTP）
    Json,
}

/// SDK 端点签名
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkEndpoints {
    /// prepare 端点
    pub prepare: String,
    /// commit 端点
    pub commit: String,
    /// rollback 端点
    pub rollback: String,
    /// query_status 端点
    pub status: String,
}

impl SdkEndpoints {
    /// 为 gRPC 传输创建端点签名
    ///
    /// gRPC 使用统一 RPC 入口 `Call`，所有方法通过 `method` 字段分发。
    pub fn grpc(endpoint: &str) -> Self {
        Self {
            prepare: format!("grpc://{endpoint}/Call"),
            commit: format!("grpc://{endpoint}/Call"),
            rollback: format!("grpc://{endpoint}/Call"),
            status: format!("grpc://{endpoint}/Call"),
        }
    }

    /// 为 HTTP 传输创建端点签名
    pub fn http(base_url: &str) -> Self {
        Self {
            prepare: format!("{base_url}/prepare"),
            commit: format!("{base_url}/commit"),
            rollback: format!("{base_url}/rollback"),
            status: format!("{base_url}/query_status"),
        }
    }
}

/// 跨语言 SDK 接入契约
///
/// 为每种语言生成完整的接入契约，包括：
/// - 协议版本（对齐 [`COORDINATOR_PROTOCOL_VERSION`]）
/// - 端点签名（prepare/commit/rollback/status）
/// - 鉴权方案
/// - 序列化格式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossLangSdkContract {
    /// 编程语言
    pub language: ParticipantLanguage,
    /// 协议版本
    pub protocol_version: u32,
    /// 端点签名
    pub endpoints: SdkEndpoints,
    /// 鉴权方案
    pub auth_scheme: AuthScheme,
    /// 序列化格式
    pub serialization_format: SerializationFormat,
}

impl CrossLangSdkContract {
    /// 为指定语言生成默认契约
    ///
    /// 协议版本对齐 [`COORDINATOR_PROTOCOL_VERSION`]。
    /// 默认使用 gRPC + Protobuf + Token 认证。
    pub fn for_language(language: ParticipantLanguage) -> Self {
        let (transport, default_endpoint) = Self::default_endpoint_for_language(&language);
        let endpoints = match transport {
            ParticipantTransport::Grpc => SdkEndpoints::grpc(&default_endpoint),
            ParticipantTransport::Http => SdkEndpoints::http(&default_endpoint),
        };
        Self {
            language,
            protocol_version: COORDINATOR_PROTOCOL_VERSION,
            endpoints,
            auth_scheme: AuthScheme::Token,
            serialization_format: SerializationFormat::Protobuf,
        }
    }

    /// 为指定语言 + HTTP 传输生成契约
    pub fn for_language_http(language: ParticipantLanguage, base_url: &str) -> Self {
        Self {
            language,
            protocol_version: COORDINATOR_PROTOCOL_VERSION,
            endpoints: SdkEndpoints::http(base_url),
            auth_scheme: AuthScheme::Token,
            serialization_format: SerializationFormat::Json,
        }
    }

    /// 设置鉴权方案（链式）
    #[must_use]
    pub fn with_auth_scheme(mut self, scheme: AuthScheme) -> Self {
        self.auth_scheme = scheme;
        self
    }

    /// 设置端点签名（链式）
    #[must_use]
    pub fn with_endpoints(mut self, endpoints: SdkEndpoints) -> Self {
        self.endpoints = endpoints;
        self
    }

    /// 为各语言生成默认端点
    fn default_endpoint_for_language(
        language: &ParticipantLanguage,
    ) -> (ParticipantTransport, String) {
        match language {
            ParticipantLanguage::Go => (ParticipantTransport::Grpc, "127.0.0.1:50061".to_string()),
            ParticipantLanguage::Java => (
                ParticipantTransport::Http,
                "http://127.0.0.1:8081/api/tx".to_string(),
            ),
            ParticipantLanguage::Cpp => (ParticipantTransport::Grpc, "127.0.0.1:50062".to_string()),
            ParticipantLanguage::Python => (
                ParticipantTransport::Http,
                "http://127.0.0.1:8082/api/tx".to_string(),
            ),
            ParticipantLanguage::JavaScript => (
                ParticipantTransport::Http,
                "http://127.0.0.1:8083/api/tx".to_string(),
            ),
        }
    }

    /// 生成 SDK 接入文档（Markdown 格式）
    pub fn to_markdown(&self) -> String {
        let lang = self.language;
        let transport = match self.serialization_format {
            SerializationFormat::Protobuf => "gRPC + Protobuf",
            SerializationFormat::Json => "HTTP + JSON",
        };
        let auth = match self.auth_scheme {
            AuthScheme::Mtls => "mTLS 双向认证",
            AuthScheme::Token => "Bearer Token",
            AuthScheme::None => "无鉴权",
        };
        format!(
            "## {lang} 参与者 SDK 接入契约\n\n\
             - **协议版本**: {}\n\
             - **传输协议**: {transport}\n\
             - **鉴权方案**: {auth}\n\
             - **端点签名**:\n\
               - prepare: `{}`\n\
               - commit: `{}`\n\
               - rollback: `{}`\n\
               - query_status: `{}`\n",
            self.protocol_version,
            self.endpoints.prepare,
            self.endpoints.commit,
            self.endpoints.rollback,
            self.endpoints.status,
        )
    }
}

// ============================================================================
// CrossLangTxTracker — 跨语言事务追踪器（可观测性）
// ============================================================================

/// 跨语言事务追踪 span
#[derive(Debug, Clone)]
pub struct CrossLangTxSpan {
    /// 事务 ID
    pub tx_id: String,
    /// 参与者标识（语言 + 端点）
    pub participant: String,
    /// 执行阶段（prepare/commit/rollback）
    pub phase: String,
    /// 耗时（毫秒）
    pub latency_ms: u64,
    /// 是否成功
    pub success: bool,
    /// 时间戳
    pub timestamp: u64,
}

/// 跨语言事务追踪器
///
/// 复用既有 [`CrossLangTxAlerter`] 记录事务执行 span，
/// 提供事务 ID + 参与者（语言/端点）+ 阶段 + 耗时 + 结果的可观测性。
pub struct CrossLangTxTracker {
    alerter: CrossLangTxAlerter,
    spans: parking_lot::RwLock<Vec<CrossLangTxSpan>>,
}

impl CrossLangTxTracker {
    /// 创建追踪器
    pub fn new() -> Self {
        Self {
            alerter: CrossLangTxAlerter::new(),
            spans: parking_lot::RwLock::new(Vec::new()),
        }
    }

    /// 记录一个 span
    pub fn record_span(&self, span: CrossLangTxSpan) {
        if !span.success {
            let event = AlertEvent {
                level: AlertLevel::Warning,
                tx_id: span.tx_id.clone(),
                participant_id: span.participant.clone(),
                message: format!("{} failed in {} phase", span.participant, span.phase),
                timestamp: span.timestamp,
            };
            // 使用 alerter 的 record_failure 逻辑
            self.alerter.record_failure(
                &span.tx_id,
                &span.participant,
                &super::CrossLangTxError::Transport(format!("{} failed", span.phase)),
            );
            let _ = event; // event 已通过 record_failure 记录
        } else {
            self.alerter.record_success(span.latency_ms);
        }
        self.spans.write().push(span);
    }

    /// 查询指定事务的所有 span
    pub fn spans_for_tx(&self, tx_id: &str) -> Vec<CrossLangTxSpan> {
        self.spans
            .read()
            .iter()
            .filter(|s| s.tx_id == tx_id)
            .cloned()
            .collect()
    }

    /// 返回所有 span
    pub fn all_spans(&self) -> Vec<CrossLangTxSpan> {
        self.spans.read().clone()
    }

    /// 返回指标
    pub fn metrics(&self) -> super::observability::CrossLangTxMetrics {
        self.alerter.metrics()
    }
}

impl Default for CrossLangTxTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ── M1-T6.3: for_language 契约生成 ──

    #[test]
    fn test_contract_for_go() {
        let contract = CrossLangSdkContract::for_language(ParticipantLanguage::Go);
        assert_eq!(contract.language, ParticipantLanguage::Go);
        assert_eq!(contract.protocol_version, COORDINATOR_PROTOCOL_VERSION);
        assert_eq!(contract.auth_scheme, AuthScheme::Token);
        assert_eq!(contract.serialization_format, SerializationFormat::Protobuf);
    }

    #[test]
    fn test_contract_for_java() {
        let contract = CrossLangSdkContract::for_language(ParticipantLanguage::Java);
        assert_eq!(contract.language, ParticipantLanguage::Java);
        assert_eq!(contract.protocol_version, COORDINATOR_PROTOCOL_VERSION);
    }

    #[test]
    fn test_contract_for_cpp() {
        let contract = CrossLangSdkContract::for_language(ParticipantLanguage::Cpp);
        assert_eq!(contract.language, ParticipantLanguage::Cpp);
        assert_eq!(contract.protocol_version, COORDINATOR_PROTOCOL_VERSION);
    }

    #[test]
    fn test_contract_for_python() {
        let contract = CrossLangSdkContract::for_language(ParticipantLanguage::Python);
        assert_eq!(contract.language, ParticipantLanguage::Python);
    }

    #[test]
    fn test_contract_for_javascript() {
        let contract = CrossLangSdkContract::for_language(ParticipantLanguage::JavaScript);
        assert_eq!(contract.language, ParticipantLanguage::JavaScript);
    }

    // ── M1-T6.4: 五语言契约生成 ──

    #[test]
    fn test_all_five_language_contracts() {
        let languages = [
            ParticipantLanguage::Go,
            ParticipantLanguage::Java,
            ParticipantLanguage::Cpp,
            ParticipantLanguage::Python,
            ParticipantLanguage::JavaScript,
        ];
        for lang in &languages {
            let contract = CrossLangSdkContract::for_language(*lang);
            assert_eq!(contract.protocol_version, COORDINATOR_PROTOCOL_VERSION);
            assert!(!contract.endpoints.prepare.is_empty());
            assert!(!contract.endpoints.commit.is_empty());
            assert!(!contract.endpoints.rollback.is_empty());
            assert!(!contract.endpoints.status.is_empty());
        }
    }

    // ── HTTP 契约 ──

    #[test]
    fn test_contract_http() {
        let contract = CrossLangSdkContract::for_language_http(
            ParticipantLanguage::Java,
            "http://localhost:8080/api/tx",
        );
        assert_eq!(contract.serialization_format, SerializationFormat::Json);
        assert_eq!(contract.auth_scheme, AuthScheme::Token);
        assert_eq!(
            contract.endpoints.prepare,
            "http://localhost:8080/api/tx/prepare"
        );
    }

    // ── 链式配置 ──

    #[test]
    fn test_contract_builder() {
        let contract = CrossLangSdkContract::for_language(ParticipantLanguage::Go)
            .with_auth_scheme(AuthScheme::Mtls);
        assert_eq!(contract.auth_scheme, AuthScheme::Mtls);
    }

    // ── Markdown 文档生成 ──

    #[test]
    fn test_contract_to_markdown() {
        let contract = CrossLangSdkContract::for_language(ParticipantLanguage::Go);
        let md = contract.to_markdown();
        assert!(md.contains("go"));
        assert!(md.contains("gRPC + Protobuf"));
        assert!(md.contains("Bearer Token"));
    }

    // ── SdkEndpoints ──

    #[test]
    fn test_sdk_endpoints_grpc() {
        let endpoints = SdkEndpoints::grpc("127.0.0.1:50051");
        assert!(endpoints.prepare.contains("grpc://127.0.0.1:50051/Call"));
        assert!(endpoints.commit.contains("grpc://127.0.0.1:50051/Call"));
    }

    #[test]
    fn test_sdk_endpoints_http() {
        let endpoints = SdkEndpoints::http("http://localhost:8080/api");
        assert_eq!(endpoints.prepare, "http://localhost:8080/api/prepare");
        assert_eq!(endpoints.commit, "http://localhost:8080/api/commit");
        assert_eq!(endpoints.rollback, "http://localhost:8080/api/rollback");
        assert_eq!(endpoints.status, "http://localhost:8080/api/query_status");
    }

    // ── M1-T6.7: 可观测性追踪 ──

    #[test]
    fn test_tx_tracker_record_span() {
        let tracker = CrossLangTxTracker::new();
        let span = CrossLangTxSpan {
            tx_id: "tx5".to_string(),
            participant: "go".to_string(),
            phase: "prepare".to_string(),
            latency_ms: 42,
            success: true,
            timestamp: 1000,
        };
        tracker.record_span(span);

        let spans = tracker.spans_for_tx("tx5");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].participant, "go");
        assert_eq!(spans[0].phase, "prepare");
        assert_eq!(spans[0].latency_ms, 42);
        assert!(spans[0].success);
    }

    #[test]
    fn test_tx_tracker_failure_span() {
        let tracker = CrossLangTxTracker::new();
        let span = CrossLangTxSpan {
            tx_id: "tx-fail".to_string(),
            participant: "java".to_string(),
            phase: "commit".to_string(),
            latency_ms: 10,
            success: false,
            timestamp: 2000,
        };
        tracker.record_span(span);

        let metrics = tracker.metrics();
        assert_eq!(metrics.failed_transactions, 1);
    }

    #[test]
    fn test_tx_tracker_multiple_spans() {
        let tracker = CrossLangTxTracker::new();
        for i in 0..3 {
            tracker.record_span(CrossLangTxSpan {
                tx_id: "tx-multi".to_string(),
                participant: format!("p{i}"),
                phase: "prepare".to_string(),
                latency_ms: 5,
                success: true,
                timestamp: i,
            });
        }
        let spans = tracker.spans_for_tx("tx-multi");
        assert_eq!(spans.len(), 3);
    }
}
