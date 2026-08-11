//! gRPC / HTTP 跨语言参与者协议实现

use super::{
    CrossLangParticipantProtocol, CrossLangTxError, ParticipantResponse,
    COORDINATOR_PROTOCOL_VERSION,
};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// gRPC 参与者协议实现
///
/// 基于 tonic + prost 的 gRPC 传输，复用既有 sz-orm-grpc 模式。
/// 实际的 gRPC 调用通过 `RemoteCallHandler` trait 抽象，便于测试和灵活配置。
pub trait RemoteCallHandler: Send + Sync {
    /// 发起远程调用
    fn call(
        &self,
        method: &str,
        tx_id: &str,
        payload: &[u8],
    ) -> Result<ParticipantResponse, CrossLangTxError>;
}

/// gRPC 参与者协议
pub struct GrpcParticipantProtocol {
    endpoint: String,
    protocol_version: u32,
    handler: Arc<dyn RemoteCallHandler>,
}

impl GrpcParticipantProtocol {
    pub fn new(endpoint: String, handler: Arc<dyn RemoteCallHandler>) -> Self {
        Self {
            endpoint,
            protocol_version: COORDINATOR_PROTOCOL_VERSION,
            handler,
        }
    }

    pub fn with_protocol_version(mut self, version: u32) -> Self {
        self.protocol_version = version;
        self
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

impl CrossLangParticipantProtocol for GrpcParticipantProtocol {
    fn prepare(
        &self,
        tx_id: &str,
        payload: &[u8],
    ) -> Result<ParticipantResponse, CrossLangTxError> {
        self.handler.call("prepare", tx_id, payload)
    }

    fn commit(&self, tx_id: &str, payload: &[u8]) -> Result<ParticipantResponse, CrossLangTxError> {
        self.handler.call("commit", tx_id, payload)
    }

    fn rollback(
        &self,
        tx_id: &str,
        payload: &[u8],
    ) -> Result<ParticipantResponse, CrossLangTxError> {
        self.handler.call("rollback", tx_id, payload)
    }

    fn protocol_version(&self) -> u32 {
        self.protocol_version
    }
}

/// HTTP/JSON 参与者协议
pub struct HttpParticipantProtocol {
    endpoint: String,
    protocol_version: u32,
    handler: Arc<dyn RemoteCallHandler>,
}

impl HttpParticipantProtocol {
    pub fn new(endpoint: String, handler: Arc<dyn RemoteCallHandler>) -> Self {
        Self {
            endpoint,
            protocol_version: COORDINATOR_PROTOCOL_VERSION,
            handler,
        }
    }

    pub fn with_protocol_version(mut self, version: u32) -> Self {
        self.protocol_version = version;
        self
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

impl CrossLangParticipantProtocol for HttpParticipantProtocol {
    fn prepare(
        &self,
        tx_id: &str,
        payload: &[u8],
    ) -> Result<ParticipantResponse, CrossLangTxError> {
        self.handler.call("prepare", tx_id, payload)
    }

    fn commit(&self, tx_id: &str, payload: &[u8]) -> Result<ParticipantResponse, CrossLangTxError> {
        self.handler.call("commit", tx_id, payload)
    }

    fn rollback(
        &self,
        tx_id: &str,
        payload: &[u8],
    ) -> Result<ParticipantResponse, CrossLangTxError> {
        self.handler.call("rollback", tx_id, payload)
    }

    fn protocol_version(&self) -> u32 {
        self.protocol_version
    }
}

/// Mock 远程调用处理器（用于测试）
pub struct MockRemoteCallHandler {
    responses: RwLock<HashMap<String, ParticipantResponse>>,
    call_log: RwLock<Vec<(String, String)>>,
}

impl MockRemoteCallHandler {
    pub fn new() -> Self {
        Self {
            responses: RwLock::new(HashMap::new()),
            call_log: RwLock::new(Vec::new()),
        }
    }

    pub fn with_success_response(method: &str, payload: Vec<u8>) -> Self {
        let mut responses = HashMap::new();
        responses.insert(
            method.to_string(),
            ParticipantResponse {
                success: true,
                payload,
                error: None,
                latency_ms: 10,
            },
        );
        Self {
            responses: RwLock::new(responses),
            call_log: RwLock::new(Vec::new()),
        }
    }

    pub fn set_response(&self, method: &str, response: ParticipantResponse) {
        self.responses.write().insert(method.to_string(), response);
    }

    pub fn call_log(&self) -> Vec<(String, String)> {
        self.call_log.read().clone()
    }
}

impl Default for MockRemoteCallHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl RemoteCallHandler for MockRemoteCallHandler {
    fn call(
        &self,
        method: &str,
        tx_id: &str,
        _payload: &[u8],
    ) -> Result<ParticipantResponse, CrossLangTxError> {
        self.call_log
            .write()
            .push((method.to_string(), tx_id.to_string()));
        self.responses.read().get(method).cloned().ok_or_else(|| {
            CrossLangTxError::RemoteCall(format!("no response for method: {method}"))
        })
    }
}

/// 协议版本兼容性检查
pub fn check_protocol_version(
    coordinator_version: u32,
    participant_version: u32,
) -> Result<(), CrossLangTxError> {
    if coordinator_version != participant_version {
        return Err(CrossLangTxError::ProtocolVersionMismatch {
            coordinator: coordinator_version,
            participant: participant_version,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_grpc_protocol_prepare() {
        let handler = Arc::new(MockRemoteCallHandler::with_success_response(
            "prepare",
            vec![1, 2, 3],
        ));
        let protocol = GrpcParticipantProtocol::new("grpc://localhost:8080".to_string(), handler);
        let resp = protocol.prepare("tx-001", &[0]).unwrap();
        assert!(resp.success);
        assert_eq!(resp.payload, vec![1, 2, 3]);
    }

    #[test]
    fn test_grpc_protocol_version() {
        let handler = Arc::new(MockRemoteCallHandler::new());
        let protocol = GrpcParticipantProtocol::new("grpc://localhost:8080".to_string(), handler);
        assert_eq!(protocol.protocol_version(), COORDINATOR_PROTOCOL_VERSION);
    }

    #[test]
    fn test_grpc_protocol_custom_version() {
        let handler = Arc::new(MockRemoteCallHandler::new());
        let protocol = GrpcParticipantProtocol::new("grpc://localhost:8080".to_string(), handler)
            .with_protocol_version(2);
        assert_eq!(protocol.protocol_version(), 2);
    }

    #[test]
    fn test_http_protocol_prepare() {
        let handler = Arc::new(MockRemoteCallHandler::with_success_response(
            "prepare",
            vec![],
        ));
        let protocol =
            HttpParticipantProtocol::new("http://localhost:8080/api".to_string(), handler);
        let resp = protocol.prepare("tx-002", &[]).unwrap();
        assert!(resp.success);
    }

    #[test]
    fn test_http_protocol_version() {
        let handler = Arc::new(MockRemoteCallHandler::new());
        let protocol =
            HttpParticipantProtocol::new("http://localhost:8080/api".to_string(), handler);
        assert_eq!(protocol.protocol_version(), COORDINATOR_PROTOCOL_VERSION);
    }

    #[test]
    fn test_protocol_version_mismatch() {
        let err = check_protocol_version(1, 2).unwrap_err();
        match err {
            CrossLangTxError::ProtocolVersionMismatch {
                coordinator,
                participant,
            } => {
                assert_eq!(coordinator, 1);
                assert_eq!(participant, 2);
            }
            _ => panic!("expected ProtocolVersionMismatch"),
        }
    }

    #[test]
    fn test_protocol_version_match() {
        check_protocol_version(1, 1).unwrap();
    }

    #[test]
    fn test_mock_call_log() {
        let handler = Arc::new(MockRemoteCallHandler::new());
        handler.set_response(
            "prepare",
            ParticipantResponse {
                success: true,
                payload: vec![],
                error: None,
                latency_ms: 10,
            },
        );
        handler.set_response(
            "commit",
            ParticipantResponse {
                success: true,
                payload: vec![],
                error: None,
                latency_ms: 10,
            },
        );
        let protocol =
            GrpcParticipantProtocol::new("grpc://localhost:8080".to_string(), handler.clone());
        protocol.prepare("tx-003", &[]).unwrap();
        protocol.commit("tx-003", &[]).unwrap();
        let log = handler.call_log();
        assert_eq!(log.len(), 2);
        assert_eq!(log[0], ("prepare".to_string(), "tx-003".to_string()));
        assert_eq!(log[1], ("commit".to_string(), "tx-003".to_string()));
    }

    #[test]
    fn test_grpc_endpoint() {
        let handler = Arc::new(MockRemoteCallHandler::new());
        let protocol = GrpcParticipantProtocol::new("grpc://localhost:9090".to_string(), handler);
        assert_eq!(protocol.endpoint(), "grpc://localhost:9090");
    }

    #[test]
    fn test_http_endpoint() {
        let handler = Arc::new(MockRemoteCallHandler::new());
        let protocol =
            HttpParticipantProtocol::new("http://localhost:9090/api".to_string(), handler);
        assert_eq!(protocol.endpoint(), "http://localhost:9090/api");
    }
}
