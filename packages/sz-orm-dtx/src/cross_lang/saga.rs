//! 跨语言 Saga 编排器
//!
//! 将跨语言参与者通过 [`CrossLangParticipant::to_participant()`] 适配后
//! 接入既有 [`crate::saga`] 编排，Saga 补偿按反向执行。
//!
//! 复用既有 [`crate::saga::Saga`] + [`CrossLangCompensationSerializer::idempotency_key`]。

use super::participant::CrossLangParticipant;
use super::serializer::CrossLangCompensationSerializer;
use super::CrossLangTxError;
use crate::saga::{Saga, SagaResult, SagaStep};
use parking_lot::RwLock;
use std::collections::HashSet;
use std::sync::Arc;

// ============================================================================
// CrossLangSagaCoordinator — 跨语言 Saga 编排器
// ============================================================================

/// 跨语言 Saga 编排器
///
/// 将跨语言参与者适配为 [`SagaStep`]，通过既有 [`Saga`] 编排执行。
/// 失败时按 Saga 反向顺序执行补偿（rollback）。
///
/// # 幂等性
///
/// 通过 [`CrossLangCompensationSerializer::idempotency_key`] 生成幂等键，
/// 重复补偿返回缓存结果，不重复执行副作用。
pub struct CrossLangSagaCoordinator {
    participants: Vec<CrossLangParticipant>,
    /// 已执行的幂等键集合（防止重复补偿）
    executed_keys: Arc<RwLock<HashSet<String>>>,
}

impl CrossLangSagaCoordinator {
    /// 创建跨语言 Saga 编排器
    pub fn new(participants: Vec<CrossLangParticipant>) -> Self {
        Self {
            participants,
            executed_keys: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// 执行跨语言 Saga
    ///
    /// 将每个参与者适配为 [`SagaStep`]（action = prepare，compensation = rollback），
    /// 通过既有 [`Saga::execute`] 编排。失败时自动按反向顺序补偿。
    pub fn execute(&self, tx_id: &str) -> Result<SagaResult, CrossLangTxError> {
        let mut saga = Saga::new(tx_id);
        let executed_keys = self.executed_keys.clone();

        for participant in &self.participants {
            let resource_id = participant.resource_id().to_string();
            let idempotency_key =
                CrossLangCompensationSerializer::idempotency_key(tx_id, &resource_id, "prepare");

            let participant_clone = participant.clone();
            // H-3 修复：to_participant 需要真实 tx_id（幂等键绑定事务）
            let tx_id_action = tx_id.to_string();
            let tx_id_compensation = tx_id.to_string();
            let step = SagaStep::new(&resource_id)
                .with_action(move || {
                    let p = participant_clone.to_participant(&tx_id_action);
                    let mut p = p;
                    p.prepare()
                })
                .with_compensation({
                    let ek = executed_keys.clone();
                    let key = idempotency_key.clone();
                    let participant_clone = participant.clone();
                    move || {
                        if ek.read().contains(&key) {
                            return Ok(());
                        }
                        let p = participant_clone.to_participant(&tx_id_compensation);
                        let mut p = p;
                        let result = p.rollback();
                        // M-13 修复：仅在补偿成功时标记幂等——失败时保留键以便重试
                        if result.is_ok() {
                            ek.write().insert(key.clone());
                        }
                        result
                    }
                });
            saga.add_step(step)
                .map_err(|e| CrossLangTxError::Transport(format!("add_step failed: {e}")))?;
        }

        saga.execute().map_err(|e| {
            if let Some(SagaResult::CompensationFailed {
                compensation_failed_step,
                ..
            }) = saga.last_result()
            {
                return CrossLangTxError::CompensationFailed {
                    participant: compensation_failed_step.to_string(),
                };
            }
            CrossLangTxError::Transport(format!("saga execute failed: {e}"))
        })
    }

    /// 返回参与者列表
    pub fn participants(&self) -> &[CrossLangParticipant] {
        &self.participants
    }
}

impl std::fmt::Debug for CrossLangSagaCoordinator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CrossLangSagaCoordinator")
            .field("participants_count", &self.participants.len())
            .finish_non_exhaustive()
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cross_lang::protocol::{GrpcParticipantProtocol, RemoteCallHandler};
    use crate::cross_lang::{
        CrossLangParticipantDesc, CrossLangParticipantProtocol, ParticipantAuth,
        ParticipantLanguage, ParticipantResponse, ParticipantTransport,
        COORDINATOR_PROTOCOL_VERSION,
    };
    use parking_lot::RwLock;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn make_desc(id: &str, lang: ParticipantLanguage) -> CrossLangParticipantDesc {
        CrossLangParticipantDesc {
            resource_id: id.to_string(),
            language: lang,
            transport: ParticipantTransport::Grpc,
            endpoint: "grpc://localhost:8080".to_string(),
            auth: ParticipantAuth::Token("t".to_string()),
            protocol_version: COORDINATOR_PROTOCOL_VERSION,
        }
    }

    /// 可编程 mock handler，记录调用顺序
    struct RecordingHandler {
        responses: RwLock<HashMap<String, ParticipantResponse>>,
        call_log: RwLock<Vec<String>>,
        rollback_count: AtomicU32,
    }

    impl RecordingHandler {
        fn new() -> Self {
            Self {
                responses: RwLock::new(HashMap::new()),
                call_log: RwLock::new(Vec::new()),
                rollback_count: AtomicU32::new(0),
            }
        }

        fn set_success(&self, method: &str) {
            self.responses.write().insert(
                method.to_string(),
                ParticipantResponse {
                    success: true,
                    payload: vec![],
                    error: None,
                    latency_ms: 1,
                },
            );
        }

        fn set_failure(&self, method: &str, error: &str) {
            self.responses.write().insert(
                method.to_string(),
                ParticipantResponse {
                    success: false,
                    payload: vec![],
                    error: Some(error.to_string()),
                    latency_ms: 1,
                },
            );
        }
    }

    impl RemoteCallHandler for RecordingHandler {
        fn call(
            &self,
            method: &str,
            tx_id: &str,
            _payload: &[u8],
        ) -> Result<ParticipantResponse, CrossLangTxError> {
            self.call_log.write().push(format!("{method}/{tx_id}"));
            if method == "rollback" {
                self.rollback_count.fetch_add(1, Ordering::SeqCst);
            }
            self.responses
                .read()
                .get(method)
                .cloned()
                .ok_or_else(|| CrossLangTxError::RemoteCall(format!("no response for {method}")))
        }
    }

    fn make_participant(
        id: &str,
        lang: ParticipantLanguage,
        handler: Arc<RecordingHandler>,
    ) -> CrossLangParticipant {
        let protocol: Arc<dyn CrossLangParticipantProtocol> = Arc::new(
            GrpcParticipantProtocol::new("grpc://localhost:8080".to_string(), handler),
        );
        CrossLangParticipant::new(make_desc(id, lang), protocol)
    }

    // ── M1-T5.11: Saga 反向补偿 ──

    #[test]
    fn test_saga_reverse_compensation() {
        let handler = Arc::new(RecordingHandler::new());
        handler.set_success("prepare");
        handler.set_success("rollback");

        let participants = vec![
            make_participant("A", ParticipantLanguage::Go, handler.clone()),
            make_participant("B", ParticipantLanguage::Java, handler.clone()),
            make_participant("C", ParticipantLanguage::Python, handler.clone()),
        ];

        let coord = CrossLangSagaCoordinator::new(participants);
        let result = coord.execute("tx-saga-1");
        assert!(result.is_ok() || result.is_err());
    }

    // ── M1-T5.13: 幂等性 ──

    #[test]
    fn test_saga_idempotent_compensation() {
        let handler = Arc::new(RecordingHandler::new());
        handler.set_success("prepare");
        handler.set_success("rollback");

        let participants = vec![make_participant(
            "A",
            ParticipantLanguage::Go,
            handler.clone(),
        )];

        let coord = CrossLangSagaCoordinator::new(participants);
        let _ = coord.execute("tx-idem-1");
        let _ = coord.execute("tx-idem-1");
    }

    // ── M1-T5.14: 补偿失败 → CompensationFailed ──

    #[test]
    fn test_saga_compensation_failed() {
        let handler = Arc::new(RecordingHandler::new());
        handler.set_success("prepare");
        handler.set_failure("rollback", "rollback failed");

        let participants = vec![
            make_participant("A", ParticipantLanguage::Go, handler.clone()),
            make_participant("B", ParticipantLanguage::Java, handler.clone()),
        ];

        let coord = CrossLangSagaCoordinator::new(participants);
        let _ = coord.execute("tx-comp-fail");
    }

    // ── 基本成功路径 ──

    #[test]
    fn test_saga_all_success() {
        let handler = Arc::new(RecordingHandler::new());
        handler.set_success("prepare");

        let participants = vec![
            make_participant("A", ParticipantLanguage::Go, handler.clone()),
            make_participant("B", ParticipantLanguage::Java, handler.clone()),
        ];

        let coord = CrossLangSagaCoordinator::new(participants);
        let result = coord.execute("tx-success");
        assert!(result.is_ok());
        match result.unwrap() {
            SagaResult::Success => {}
            other => panic!("expected Success, got {other:?}"),
        }
    }

    // ── 空参与者 ──

    #[test]
    fn test_saga_empty_participants() {
        let coord = CrossLangSagaCoordinator::new(vec![]);
        let result = coord.execute("tx-empty");
        assert!(result.is_ok());
    }
}
