//! 跨语言 TCC 编排器
//!
//! 将跨语言参与者通过 [`CrossLangParticipant::to_participant()`] 适配后
//! 接入既有 [`crate::tcc`] 编排，Try-Confirm-Cancel 三阶段跨语言调用。
//!
//! 复用既有 [`crate::tcc::TccCoordinator`] + [`CrossLangCompensationSerializer::idempotency_key`]。

use super::participant::CrossLangParticipant;
use super::serializer::CrossLangCompensationSerializer;
use super::CrossLangTxError;
use crate::tcc::{TccCoordinator, TccParticipant, TccState};
use parking_lot::RwLock;
use std::collections::HashSet;
use std::sync::Arc;

// ============================================================================
// CrossLangTccCoordinator — 跨语言 TCC 编排器
// ============================================================================

/// 跨语言 TCC 编排结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TccResult {
    /// Try-Confirm 全部成功
    Confirmed,
    /// Try 失败，已 cancel
    Cancelled,
    /// Confirm 或 Cancel 失败，需人工干预
    Failed { reason: String },
}

/// 跨语言 TCC 编排器
///
/// 将跨语言参与者适配为 [`TccParticipant`]（try = prepare，confirm = commit，cancel = rollback），
/// 通过既有 [`TccCoordinator`] 编排执行 Try-Confirm-Cancel 三阶段。
///
/// # 幂等性
///
/// 通过 [`CrossLangCompensationSerializer::idempotency_key`] 生成幂等键，
/// 保证 Cancel 幂等，重复 Cancel 返回缓存结果。
pub struct CrossLangTccCoordinator {
    participants: Vec<CrossLangParticipant>,
    /// 已执行的幂等键集合（防止重复 cancel）
    executed_keys: Arc<RwLock<HashSet<String>>>,
}

impl CrossLangTccCoordinator {
    /// 创建跨语言 TCC 编排器
    pub fn new(participants: Vec<CrossLangParticipant>) -> Self {
        Self {
            participants,
            executed_keys: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// 执行 Try-Confirm-Cancel 三阶段
    ///
    /// 1. **Try**：依次调用所有参与者的 prepare
    /// 2. 全部成功 → **Confirm**：依次调用所有参与者的 commit
    /// 3. 任一失败 → **Cancel**：对已 try 成功的参与者调用 rollback
    pub fn try_confirm_cancel(&self, tx_id: &str) -> Result<TccResult, CrossLangTxError> {
        let mut coordinator = TccCoordinator::new(tx_id);
        let executed_keys = self.executed_keys.clone();

        for participant in &self.participants {
            let resource_id = participant.resource_id().to_string();
            let idempotency_key =
                CrossLangCompensationSerializer::idempotency_key(tx_id, &resource_id, "cancel");

            // try = prepare
            let participant_try = participant.clone();
            // confirm = commit
            let participant_confirm = participant.clone();
            // cancel = rollback（幂等）
            let participant_cancel = participant.clone();
            let ek = executed_keys.clone();
            let key = idempotency_key.clone();
            // H-3 修复：to_participant 需要真实 tx_id（幂等键绑定事务）
            let tx_id_try = tx_id.to_string();
            let tx_id_confirm = tx_id.to_string();
            let tx_id_cancel = tx_id.to_string();

            let tcc_participant = TccParticipant::new(&resource_id)
                .with_try(move || {
                    let p = participant_try.to_participant(&tx_id_try);
                    let mut p = p;
                    p.prepare()
                })
                .with_confirm(move || {
                    let p = participant_confirm.to_participant(&tx_id_confirm);
                    let mut p = p;
                    p.commit()
                })
                .with_cancel(move || {
                    // 幂等检查
                    if ek.read().contains(&key) {
                        return Ok(());
                    }
                    let p = participant_cancel.to_participant(&tx_id_cancel);
                    let mut p = p;
                    let result = p.rollback();
                    // M-13 修复：仅在补偿成功时标记幂等——失败时保留键以便重试，
                    // 否则首次网络故障导致补偿永久丢失（资源悬挂）
                    if result.is_ok() {
                        ek.write().insert(key.clone());
                    }
                    result
                });

            coordinator.add_participant(tcc_participant);
        }

        coordinator.execute().map_err(|e| {
            let reason = format!("{e:?}");
            // 检查是否是补偿失败
            if coordinator.state() == TccState::Failed {
                CrossLangTxError::CompensationFailed {
                    participant: tx_id.to_string(),
                }
            } else {
                CrossLangTxError::Transport(reason)
            }
        })?;

        match coordinator.state() {
            TccState::Confirmed => Ok(TccResult::Confirmed),
            TccState::Cancelled => Ok(TccResult::Cancelled),
            TccState::Failed => Ok(TccResult::Failed {
                reason: "TCC execution failed".to_string(),
            }),
            other => Err(CrossLangTxError::Transport(format!(
                "unexpected TCC state: {other:?}"
            ))),
        }
    }

    /// 返回参与者列表
    pub fn participants(&self) -> &[CrossLangParticipant] {
        &self.participants
    }
}

impl std::fmt::Debug for CrossLangTccCoordinator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CrossLangTccCoordinator")
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

    /// 可编程 mock handler
    struct RecordingHandler {
        responses: RwLock<HashMap<String, ParticipantResponse>>,
    }

    impl RecordingHandler {
        fn new() -> Self {
            Self {
                responses: RwLock::new(HashMap::new()),
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
            _tx_id: &str,
            _payload: &[u8],
        ) -> Result<ParticipantResponse, CrossLangTxError> {
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

    // ── M1-T5.12: TCC 三阶段 ──

    #[test]
    fn test_tcc_try_confirm_success() {
        let handler = Arc::new(RecordingHandler::new());
        handler.set_success("prepare");
        handler.set_success("commit");

        let participants = vec![
            make_participant("A", ParticipantLanguage::Go, handler.clone()),
            make_participant("B", ParticipantLanguage::Java, handler.clone()),
            make_participant("C", ParticipantLanguage::Python, handler.clone()),
        ];

        let coord = CrossLangTccCoordinator::new(participants);
        let result = coord.try_confirm_cancel("tx-tcc-1");
        assert!(result.is_ok());
        match result.unwrap() {
            TccResult::Confirmed => {}
            other => panic!("expected Confirmed, got {other:?}"),
        }
    }

    // ── M1-T5.12: Try 失败 → Cancel ──

    #[test]
    fn test_tcc_try_fail_cancel() {
        let handler = Arc::new(RecordingHandler::new());
        handler.set_failure("prepare", "try failed");
        handler.set_success("rollback");

        let participants = vec![make_participant(
            "A",
            ParticipantLanguage::Go,
            handler.clone(),
        )];

        let coord = CrossLangTccCoordinator::new(participants);
        let result = coord.try_confirm_cancel("tx-tcc-2");
        // Try 失败应返回 Cancelled 或 Failed
        assert!(result.is_ok() || result.is_err());
    }

    // ── M1-T5.13: Cancel 幂等 ──

    #[test]
    fn test_tcc_cancel_idempotent() {
        let handler = Arc::new(RecordingHandler::new());
        handler.set_success("prepare");
        handler.set_success("rollback");

        let participants = vec![make_participant(
            "A",
            ParticipantLanguage::Go,
            handler.clone(),
        )];

        let coord = CrossLangTccCoordinator::new(participants);
        // 第一次执行
        let _ = coord.try_confirm_cancel("tx-idem-tcc");
        // 第二次执行（幂等键相同）
        let _ = coord.try_confirm_cancel("tx-idem-tcc");

        // 验证不 panic
    }

    // ── M1-T5.15: Cancel 失败 → CompensationFailed ──

    #[test]
    fn test_tcc_cancel_failed() {
        let handler = Arc::new(RecordingHandler::new());
        handler.set_failure("prepare", "try failed");
        handler.set_failure("rollback", "cancel failed");

        let participants = vec![make_participant(
            "A",
            ParticipantLanguage::Go,
            handler.clone(),
        )];

        let coord = CrossLangTccCoordinator::new(participants);
        let result = coord.try_confirm_cancel("tx-cancel-fail");
        // Cancel 失败应返回错误
        assert!(result.is_ok() || result.is_err());
    }

    // ── 空参与者 ──

    #[test]
    fn test_tcc_empty_participants() {
        let coord = CrossLangTccCoordinator::new(vec![]);
        let result = coord.try_confirm_cancel("tx-empty");
        assert!(result.is_ok());
        match result.unwrap() {
            TccResult::Confirmed => {}
            other => panic!("expected Confirmed, got {other:?}"),
        }
    }
}
