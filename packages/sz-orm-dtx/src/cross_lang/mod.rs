//! 跨语言分布式事务参与者协议
//!
//! 定义异构语言服务（Go/Java/C++/Python/JS）作为 Saga/TCC/XA 事务参与者的
//! 标准接入协议，通过 gRPC 或 HTTP/JSON 与 Rust 协调器通信。

pub mod observability;
pub mod participant;
pub mod protocol;
pub mod real_transport;
pub mod recovery;
pub mod registry;
pub mod saga;
pub mod sdk_contract;
pub mod serializer;
pub mod tcc;

use serde::{Deserialize, Serialize};

/// 跨语言参与者编程语言
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ParticipantLanguage {
    Go,
    Java,
    Cpp,
    Python,
    JavaScript,
}

impl std::fmt::Display for ParticipantLanguage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParticipantLanguage::Go => write!(f, "go"),
            ParticipantLanguage::Java => write!(f, "java"),
            ParticipantLanguage::Cpp => write!(f, "cpp"),
            ParticipantLanguage::Python => write!(f, "python"),
            ParticipantLanguage::JavaScript => write!(f, "javascript"),
        }
    }
}

/// 参与者传输协议
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParticipantTransport {
    Grpc,
    Http,
}

/// 参与者鉴权方式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParticipantAuth {
    /// mTLS 双向认证
    Mtls {
        cert: Vec<u8>,
        key: Vec<u8>,
        ca: Vec<u8>,
    },
    /// Token 认证
    Token(String),
}

/// 跨语言参与者描述信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossLangParticipantDesc {
    /// 资源标识符
    pub resource_id: String,
    /// 编程语言
    pub language: ParticipantLanguage,
    /// 传输协议
    pub transport: ParticipantTransport,
    /// 服务端点地址（如 `grpc://host:port` 或 `http://host:port/path`）
    pub endpoint: String,
    /// 鉴权凭据
    pub auth: ParticipantAuth,
    /// 协议版本号
    pub protocol_version: u32,
}

/// 参与者响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticipantResponse {
    /// 是否成功
    pub success: bool,
    /// 响应负载
    pub payload: Vec<u8>,
    /// 错误信息
    pub error: Option<String>,
    /// 响应耗时（毫秒）
    pub latency_ms: u64,
}

/// 跨语言事务错误
#[derive(Debug, Clone, thiserror::Error)]
pub enum CrossLangTxError {
    #[error("participant call timeout")]
    Timeout,
    #[error("authentication failed")]
    AuthFailed,
    #[error("protocol version mismatch: coordinator={coordinator}, participant={participant}")]
    ProtocolVersionMismatch { coordinator: u32, participant: u32 },
    #[error("recovery conflict detected")]
    RecoveryConflict,
    #[error("transport error: {0}")]
    Transport(String),
    #[error("compensation failed for participant: {participant}")]
    CompensationFailed { participant: String },
    #[error("remote call error: {0}")]
    RemoteCall(String),
}

/// 跨语言参与者协议 trait
///
/// 定义标准接口，gRPC 和 HTTP 双实现。
pub trait CrossLangParticipantProtocol: Send + Sync {
    /// 预备阶段：通知参与者预留资源
    fn prepare(&self, tx_id: &str, payload: &[u8])
        -> Result<ParticipantResponse, CrossLangTxError>;

    /// 提交阶段：通知参与者确认操作
    fn commit(&self, tx_id: &str, payload: &[u8]) -> Result<ParticipantResponse, CrossLangTxError>;

    /// 回滚阶段：通知参与者补偿/回滚
    fn rollback(
        &self,
        tx_id: &str,
        payload: &[u8],
    ) -> Result<ParticipantResponse, CrossLangTxError>;

    /// 返回协议版本号
    fn protocol_version(&self) -> u32;
}

/// 当前协调器协议版本
pub const COORDINATOR_PROTOCOL_VERSION: u32 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_participant_language_display() {
        assert_eq!(ParticipantLanguage::Go.to_string(), "go");
        assert_eq!(ParticipantLanguage::Java.to_string(), "java");
        assert_eq!(ParticipantLanguage::Cpp.to_string(), "cpp");
        assert_eq!(ParticipantLanguage::Python.to_string(), "python");
        assert_eq!(ParticipantLanguage::JavaScript.to_string(), "javascript");
    }

    #[test]
    fn test_participant_desc_serialization() {
        let desc = CrossLangParticipantDesc {
            resource_id: "order-service".to_string(),
            language: ParticipantLanguage::Go,
            transport: ParticipantTransport::Grpc,
            endpoint: "grpc://localhost:8080".to_string(),
            auth: ParticipantAuth::Token("secret-token".to_string()),
            protocol_version: 1,
        };
        let json = serde_json::to_string(&desc).unwrap();
        let decoded: CrossLangParticipantDesc = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.resource_id, "order-service");
        assert_eq!(decoded.language, ParticipantLanguage::Go);
        assert_eq!(decoded.protocol_version, 1);
    }

    #[test]
    fn test_participant_response_serialization() {
        let resp = ParticipantResponse {
            success: true,
            payload: vec![1, 2, 3],
            error: None,
            latency_ms: 42,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let decoded: ParticipantResponse = serde_json::from_str(&json).unwrap();
        assert!(decoded.success);
        assert_eq!(decoded.payload, vec![1, 2, 3]);
        assert_eq!(decoded.latency_ms, 42);
    }

    #[test]
    fn test_cross_lang_tx_error_display() {
        let err = CrossLangTxError::Timeout;
        assert_eq!(err.to_string(), "participant call timeout");

        let err = CrossLangTxError::ProtocolVersionMismatch {
            coordinator: 1,
            participant: 2,
        };
        assert!(err.to_string().contains("coordinator=1"));
        assert!(err.to_string().contains("participant=2"));
    }

    #[test]
    fn test_participant_auth_mtls_serialization() {
        let auth = ParticipantAuth::Mtls {
            cert: vec![0x01, 0x02],
            key: vec![0x03, 0x04],
            ca: vec![0x05, 0x06],
        };
        let json = serde_json::to_string(&auth).unwrap();
        let decoded: ParticipantAuth = serde_json::from_str(&json).unwrap();
        match decoded {
            ParticipantAuth::Mtls { cert, key, ca } => {
                assert_eq!(cert, vec![0x01, 0x02]);
                assert_eq!(key, vec![0x03, 0x04]);
                assert_eq!(ca, vec![0x05, 0x06]);
            }
            _ => panic!("expected Mtls"),
        }
    }
}
