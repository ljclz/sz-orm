//! 端到端测试：审计日志不可用时拒绝执行所有动作
//!
//! 验证当审计日志不可用时，AutonomousPolicyEngine 进入安全模式，
//! 拒绝执行所有自治动作并返回 AuditUnavailable 错误。

use std::collections::HashMap;
use std::time::SystemTime;

use sz_orm_ai::autonomous::{AnomalyEvent, AutonomousError, AutonomousPolicyEngine, Severity};

fn make_event() -> AnomalyEvent {
    AnomalyEvent {
        event_type: "high_cpu".to_string(),
        timestamp: SystemTime::now(),
        severity: Severity::Critical,
        context: HashMap::new(),
        event_hash: 1,
    }
}

/// 验证审计不可用时拒绝执行所有自治动作
#[tokio::test]
async fn e2e_audit_unavailable_rejected() {
    let engine = AutonomousPolicyEngine::new();
    engine.set_audit_available(false);

    let event = make_event();
    let result = engine.handle_event(&event, false).await;

    assert!(
        matches!(result, Err(AutonomousError::AuditUnavailable)),
        "审计不可用时应返回 AuditUnavailable 错误"
    );
}

/// 验证审计不可用时 dry_run 也被拒绝
#[tokio::test]
async fn e2e_audit_unavailable_dry_run_also_rejected() {
    let engine = AutonomousPolicyEngine::new();
    engine.set_audit_available(false);

    let event = make_event();
    let result = engine.handle_event(&event, true).await;

    assert!(
        matches!(result, Err(AutonomousError::AuditUnavailable)),
        "审计不可用时 dry_run 也应被拒绝"
    );
}

/// 验证审计恢复后正常执行
#[tokio::test]
async fn e2e_audit_recovered_normal_execution() {
    let engine = AutonomousPolicyEngine::new();
    engine.set_audit_available(false);

    let event = make_event();
    assert!(engine.handle_event(&event, true).await.is_err());

    engine.set_audit_available(true);
    let result = engine.handle_event(&event, true).await;
    assert!(
        result.is_ok() || matches!(result, Err(AutonomousError::PolicyNotFound(_))),
        "审计恢复后不应返回 AuditUnavailable 错误"
    );
}

/// 验证安全模式下 abort_in_progress 仍可用
#[tokio::test]
async fn e2e_audit_unavailable_abort_still_works() {
    let engine = AutonomousPolicyEngine::new();
    engine.set_audit_available(false);

    let result = engine.abort_in_progress();
    assert!(
        result.is_none(),
        "安全模式下 abort_in_progress 应仍可用（返回 None 表示无正在执行的动作）"
    );
}
