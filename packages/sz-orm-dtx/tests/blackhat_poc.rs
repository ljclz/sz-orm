//! 黑帽审计回归测试（DTX）——2026-08-14
//!
//! 断言防御生效（测试通过 = 修复有效）：
//! - H-3：补偿/预备幂等键绑定真实 tx_id（修复前跨事务恒等，资金一致性破坏）
//! - M-13：补偿失败不标记幂等（修复前重试被静默吞掉，补偿永久丢失）

use parking_lot::RwLock;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use sz_orm_dtx::cross_lang::participant::CrossLangParticipant;
use sz_orm_dtx::cross_lang::protocol::{GrpcParticipantProtocol, RemoteCallHandler};
use sz_orm_dtx::cross_lang::serializer::{CompensationPayload, CrossLangCompensationSerializer};
use sz_orm_dtx::cross_lang::tcc::CrossLangTccCoordinator;
use sz_orm_dtx::cross_lang::{
    CrossLangParticipantDesc, CrossLangTxError, ParticipantAuth, ParticipantLanguage,
    ParticipantResponse, ParticipantTransport, COORDINATOR_PROTOCOL_VERSION,
};

fn make_participant(handler: Arc<dyn RemoteCallHandler>, id: &str) -> CrossLangParticipant {
    let desc = CrossLangParticipantDesc {
        resource_id: id.to_string(),
        language: ParticipantLanguage::Go,
        transport: ParticipantTransport::Grpc,
        endpoint: "grpc://localhost:8080".to_string(),
        auth: ParticipantAuth::Token("t".to_string()),
        protocol_version: COORDINATOR_PROTOCOL_VERSION,
    };
    CrossLangParticipant::new(
        desc,
        Arc::new(GrpcParticipantProtocol::new(
            "localhost:8080".to_string(),
            handler,
        )),
    )
}

/// 捕获 rollback 载荷的 handler
struct CaptureRollbackHandler {
    rollback_payloads: RwLock<Vec<Vec<u8>>>,
}

impl CaptureRollbackHandler {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            rollback_payloads: RwLock::new(Vec::new()),
        })
    }
}

impl RemoteCallHandler for CaptureRollbackHandler {
    fn call(
        &self,
        method: &str,
        _tx_id: &str,
        payload: &[u8],
    ) -> Result<ParticipantResponse, CrossLangTxError> {
        if method == "rollback" {
            self.rollback_payloads.write().push(payload.to_vec());
        }
        Ok(ParticipantResponse {
            success: true,
            payload: vec![],
            error: None,
            latency_ms: 1,
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 回归测试（H-3 修复验证）：补偿幂等键绑定真实 tx_id
//
// 修复前（黑帽实证）：tx_id 与 participant_id 都被传成 resource_id，
// 同一资源的所有事务补偿键恒为 `{resource_id}:{resource_id}:refund`——
// 事务 B 的退款被远端当作事务 A 的重复补偿丢弃。
// 修复后：幂等键 = `{tx_id}:{participant_id}:{action}`，跨事务唯一。
// ═══════════════════════════════════════════════════════════════════════════
#[test]
fn regress_h3_compensation_idempotency_key_bound_to_tx_id() {
    let handler = CaptureRollbackHandler::new();
    let participant = make_participant(handler.clone(), "account-A");

    // 两个不同事务对同一参与者执行 rollback
    let mut tx1 = participant.to_participant("tx-1001");
    tx1.rollback().expect("rollback tx1");
    let mut tx2 = participant.to_participant("tx-1002");
    tx2.rollback().expect("rollback tx2");

    let payloads = handler.rollback_payloads.read();
    assert_eq!(payloads.len(), 2, "两次 rollback 必须都发出");

    let comp1: CompensationPayload = serde_json::from_slice(&payloads[0]).expect("parse comp1");
    let comp2: CompensationPayload = serde_json::from_slice(&payloads[1]).expect("parse comp2");

    println!("[regress-H-3] tx1 补偿幂等键 = {}", comp1.idempotency_key);
    println!("[regress-H-3] tx2 补偿幂等键 = {}", comp2.idempotency_key);
    assert_eq!(
        comp1.idempotency_key, "tx-1001:account-A:refund",
        "补偿幂等键必须绑定 tx_id（H-3 修复失效）"
    );
    assert_eq!(comp2.idempotency_key, "tx-1002:account-A:refund");
    assert_ne!(
        comp1.idempotency_key, comp2.idempotency_key,
        "不同事务的补偿幂等键必须不同"
    );
    println!("[regress-H-3] ✅ 跨事务补偿幂等键唯一（资金一致性修复生效）");
}

// ═══════════════════════════════════════════════════════════════════════════
// 回归测试（M-13 修复验证）：补偿失败后重试不被幂等键吞掉
//
// 修复前（黑帽实证）：无论 rollback 成功与否都插入幂等键——首次网络故障
// 导致补偿永久丢失（资源悬挂）。
// 修复后：仅成功时标记幂等；失败保留键以便重试。
// ═══════════════════════════════════════════════════════════════════════════
struct FlakyRollbackHandler {
    rollback_calls: AtomicU32,
}

impl RemoteCallHandler for FlakyRollbackHandler {
    fn call(
        &self,
        method: &str,
        _tx_id: &str,
        _payload: &[u8],
    ) -> Result<ParticipantResponse, CrossLangTxError> {
        match method {
            "prepare" => {
                // inventory-B 的 prepare 恒失败 → 触发 TCC cancel 阶段
                if let Ok(comp) = serde_json::from_slice::<CompensationPayload>(_payload) {
                    if comp.target == "inventory-B" {
                        return Err(CrossLangTxError::Transport("prepare failed".to_string()));
                    }
                }
                Ok(ParticipantResponse {
                    success: true,
                    payload: vec![],
                    error: None,
                    latency_ms: 1,
                })
            }
            "commit" => Ok(ParticipantResponse {
                success: true,
                payload: vec![],
                error: None,
                latency_ms: 1,
            }),
            "rollback" => {
                // 第一次 rollback 模拟网络故障失败，之后成功
                if self.rollback_calls.fetch_add(1, Ordering::SeqCst) == 0 {
                    Err(CrossLangTxError::Timeout)
                } else {
                    Ok(ParticipantResponse {
                        success: true,
                        payload: vec![],
                        error: None,
                        latency_ms: 1,
                    })
                }
            }
            other => Err(CrossLangTxError::Transport(format!(
                "unexpected method {other}"
            ))),
        }
    }
}

#[test]
fn regress_m13_failed_compensation_retried() {
    let flaky = Arc::new(FlakyRollbackHandler {
        rollback_calls: AtomicU32::new(0),
    });

    // 两个参与者：P1 成功，P2 失败 → 触发 cancel 阶段（P1 被补偿）
    let p1 = make_participant(flaky.clone(), "account-A");
    let p2 = make_participant(flaky.clone(), "inventory-B");
    let coordinator = CrossLangTccCoordinator::new(vec![p1, p2]);

    // 第一次执行：P2 prepare 失败 → cancel P1 → 首次 rollback 网络失败
    let r1 = coordinator.try_confirm_cancel("tx-m13");
    assert!(matches!(
        r1,
        Err(CrossLangTxError::CompensationFailed { .. })
    ));
    let calls_after_first = flaky.rollback_calls.load(Ordering::SeqCst);
    assert_eq!(calls_after_first, 1, "首次执行必须恰好 1 次 rollback 调用");

    // 第二次执行：重试同一事务——修复前幂等键已标记导致 rollback 被吞；
    // 修复后必须再次尝试补偿
    let _r2 = coordinator.try_confirm_cancel("tx-m13");
    let calls_after_second = flaky.rollback_calls.load(Ordering::SeqCst);
    assert_eq!(
        calls_after_second, 2,
        "补偿失败后重试必须再次尝试 rollback（M-13 修复失效：{} 次）",
        calls_after_second
    );
    println!("[regress-M-13] ✅ 失败补偿未被幂等键吞掉（第 2 次重试再次尝试 rollback）");
}

// ═══════════════════════════════════════════════════════════════════════════
// 回归测试（H-3 补充）：prepare 幂等键同样绑定 tx_id
// ═══════════════════════════════════════════════════════════════════════════
#[test]
fn regress_h3_prepare_idempotency_key_bound_to_tx_id() {
    // 序列化器层直接验证：prepare 与补偿的幂等键均含真实 tx_id
    let key_a = CrossLangCompensationSerializer::idempotency_key("tx-A", "res-1", "prepare");
    let key_b = CrossLangCompensationSerializer::idempotency_key("tx-B", "res-1", "prepare");
    assert_eq!(key_a, "tx-A:res-1:prepare");
    assert_ne!(key_a, key_b, "不同事务的 prepare 幂等键必须不同");
    println!("[regress-H-3] ✅ prepare 幂等键绑定 tx_id");
}
