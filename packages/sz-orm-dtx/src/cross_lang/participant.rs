//! 跨语言参与者适配器
//!
//! 将跨语言参与者（通过 gRPC/HTTP 远程调用）适配为既有 `TransactionParticipant`，
//! 协调器透明编排 Rust 内部参与者与跨语言参与者。

use super::serializer::{CompensationPayload, CrossLangCompensationSerializer};
use super::{CrossLangParticipantDesc, CrossLangParticipantProtocol, CrossLangTxError};
use crate::TransactionParticipant;
use std::sync::Arc;

/// 跨语言参与者适配器
pub struct CrossLangParticipant {
    desc: CrossLangParticipantDesc,
    protocol: Arc<dyn CrossLangParticipantProtocol>,
    timeout_ms: u64,
}

impl CrossLangParticipant {
    pub fn new(
        desc: CrossLangParticipantDesc,
        protocol: Arc<dyn CrossLangParticipantProtocol>,
    ) -> Self {
        Self {
            desc,
            protocol,
            timeout_ms: 5000,
        }
    }

    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    pub fn resource_id(&self) -> &str {
        &self.desc.resource_id
    }

    pub fn desc(&self) -> &CrossLangParticipantDesc {
        &self.desc
    }

    pub fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }

    /// 适配为既有 `TransactionParticipant`
    ///
    /// 将远程调用包装为 `ParticipantCallback` 闭包，
    /// 通过 `with_prepare/with_commit/with_rollback` 注册。
    pub fn to_participant(&self) -> TransactionParticipant {
        let resource_id = self.desc.resource_id.clone();
        let protocol_prepare = self.protocol.clone();

        let participant = TransactionParticipant::new(&resource_id).with_prepare(move || {
            let payload = CrossLangCompensationSerializer::serialize(&CompensationPayload {
                action: "prepare".to_string(),
                target: resource_id.clone(),
                params: serde_json::json!({}),
                idempotency_key: CrossLangCompensationSerializer::idempotency_key(
                    &resource_id,
                    &resource_id,
                    "prepare",
                ),
            })
            .unwrap_or_default();
            match protocol_prepare.prepare(&resource_id, &payload) {
                Ok(resp) if resp.success => Ok(()),
                Ok(resp) => Err(resp.error.unwrap_or_else(|| "prepare failed".to_string())),
                Err(e) => Err(e.to_string()),
            }
        });

        let protocol_commit = self.protocol.clone();
        let resource_id_commit = self.desc.resource_id.clone();
        let participant = participant.with_commit(move || {
            let payload = Vec::new();
            match protocol_commit.commit(&resource_id_commit, &payload) {
                Ok(resp) if resp.success => Ok(()),
                Ok(resp) => Err(resp.error.unwrap_or_else(|| "commit failed".to_string())),
                Err(e) => Err(e.to_string()),
            }
        });

        let protocol_rollback = self.protocol.clone();
        let resource_id_rollback = self.desc.resource_id.clone();
        participant.with_rollback(move || {
            let compensation = CrossLangCompensationSerializer::build_compensation(
                &resource_id_rollback,
                &resource_id_rollback,
                "deduct",
                &resource_id_rollback,
                &serde_json::json!({}),
            );
            let payload =
                CrossLangCompensationSerializer::serialize(&compensation).unwrap_or_default();
            match protocol_rollback.rollback(&resource_id_rollback, &payload) {
                Ok(resp) if resp.success => Ok(()),
                Ok(resp) => Err(resp.error.unwrap_or_else(|| "rollback failed".to_string())),
                Err(e) => Err(e.to_string()),
            }
        })
    }
}

/// 跨语言参与者协调结果
#[derive(Debug, Clone)]
pub struct ParticipantCoordinationResult {
    pub resource_id: String,
    pub success: bool,
    pub latency_ms: u64,
    pub error: Option<String>,
}

/// 协调跨语言参与者执行指定操作
pub fn coordinate_participant(
    participant: &CrossLangParticipant,
    tx_id: &str,
    action: &str,
    payload: &[u8],
) -> ParticipantCoordinationResult {
    let protocol = &participant.protocol;
    let result = match action {
        "prepare" => protocol.prepare(tx_id, payload),
        "commit" => protocol.commit(tx_id, payload),
        "rollback" => protocol.rollback(tx_id, payload),
        _ => Err(CrossLangTxError::RemoteCall(format!(
            "unknown action: {action}"
        ))),
    };

    match result {
        Ok(resp) => ParticipantCoordinationResult {
            resource_id: participant.desc.resource_id.clone(),
            success: resp.success,
            latency_ms: resp.latency_ms,
            error: resp.error,
        },
        Err(e) => ParticipantCoordinationResult {
            resource_id: participant.desc.resource_id.clone(),
            success: false,
            latency_ms: 0,
            error: Some(e.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cross_lang::protocol::{
        GrpcParticipantProtocol, HttpParticipantProtocol, MockRemoteCallHandler,
    };
    use crate::cross_lang::{ParticipantAuth, ParticipantLanguage, ParticipantTransport};

    fn make_desc() -> CrossLangParticipantDesc {
        CrossLangParticipantDesc {
            resource_id: "test-participant".to_string(),
            language: ParticipantLanguage::Go,
            transport: ParticipantTransport::Grpc,
            endpoint: "grpc://localhost:8080".to_string(),
            auth: ParticipantAuth::Token("token".to_string()),
            protocol_version: 1,
        }
    }

    #[test]
    fn test_cross_lang_participant_new() {
        let handler = Arc::new(MockRemoteCallHandler::new());
        let protocol: Arc<dyn CrossLangParticipantProtocol> = Arc::new(
            GrpcParticipantProtocol::new("grpc://localhost:8080".to_string(), handler),
        );
        let participant = CrossLangParticipant::new(make_desc(), protocol);
        assert_eq!(participant.resource_id(), "test-participant");
        assert_eq!(participant.timeout_ms(), 5000);
    }

    #[test]
    fn test_with_timeout() {
        let handler = Arc::new(MockRemoteCallHandler::new());
        let protocol: Arc<dyn CrossLangParticipantProtocol> = Arc::new(
            GrpcParticipantProtocol::new("grpc://localhost:8080".to_string(), handler),
        );
        let participant = CrossLangParticipant::new(make_desc(), protocol).with_timeout(3000);
        assert_eq!(participant.timeout_ms(), 3000);
    }

    #[test]
    fn test_coordinate_prepare_success() {
        let handler = Arc::new(MockRemoteCallHandler::with_success_response(
            "prepare",
            vec![],
        ));
        let protocol: Arc<dyn CrossLangParticipantProtocol> = Arc::new(
            GrpcParticipantProtocol::new("grpc://localhost:8080".to_string(), handler),
        );
        let participant = CrossLangParticipant::new(make_desc(), protocol);
        let result = coordinate_participant(&participant, "tx-001", "prepare", &[]);
        assert!(result.success);
        assert_eq!(result.resource_id, "test-participant");
    }

    #[test]
    fn test_coordinate_commit_success() {
        let handler = Arc::new(MockRemoteCallHandler::with_success_response(
            "commit",
            vec![],
        ));
        let protocol: Arc<dyn CrossLangParticipantProtocol> = Arc::new(
            GrpcParticipantProtocol::new("grpc://localhost:8080".to_string(), handler),
        );
        let participant = CrossLangParticipant::new(make_desc(), protocol);
        let result = coordinate_participant(&participant, "tx-001", "commit", &[]);
        assert!(result.success);
    }

    #[test]
    fn test_coordinate_rollback_success() {
        let handler = Arc::new(MockRemoteCallHandler::with_success_response(
            "rollback",
            vec![],
        ));
        let protocol: Arc<dyn CrossLangParticipantProtocol> = Arc::new(
            GrpcParticipantProtocol::new("grpc://localhost:8080".to_string(), handler),
        );
        let participant = CrossLangParticipant::new(make_desc(), protocol);
        let result = coordinate_participant(&participant, "tx-001", "rollback", &[]);
        assert!(result.success);
    }

    #[test]
    fn test_coordinate_unknown_action() {
        let handler = Arc::new(MockRemoteCallHandler::new());
        let protocol: Arc<dyn CrossLangParticipantProtocol> = Arc::new(
            GrpcParticipantProtocol::new("grpc://localhost:8080".to_string(), handler),
        );
        let participant = CrossLangParticipant::new(make_desc(), protocol);
        let result = coordinate_participant(&participant, "tx-001", "unknown", &[]);
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[test]
    fn test_coordinate_remote_call_error() {
        let handler = Arc::new(MockRemoteCallHandler::new());
        let protocol: Arc<dyn CrossLangParticipantProtocol> = Arc::new(
            GrpcParticipantProtocol::new("grpc://localhost:8080".to_string(), handler),
        );
        let participant = CrossLangParticipant::new(make_desc(), protocol);
        let result = coordinate_participant(&participant, "tx-001", "prepare", &[]);
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[test]
    fn test_http_participant_coordinate() {
        let handler = Arc::new(MockRemoteCallHandler::with_success_response(
            "prepare",
            vec![],
        ));
        let protocol: Arc<dyn CrossLangParticipantProtocol> = Arc::new(
            HttpParticipantProtocol::new("http://localhost:8080/api".to_string(), handler),
        );
        let participant = CrossLangParticipant::new(make_desc(), protocol);
        let result = coordinate_participant(&participant, "tx-001", "prepare", &[]);
        assert!(result.success);
    }
}
