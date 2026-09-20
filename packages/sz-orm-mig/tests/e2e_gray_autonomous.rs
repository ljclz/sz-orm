//! 跨模块联动 e2e 测试：灰度发布 + 自动回滚 + 自治闭环
//!
//! 验证灰度异常触发自治回滚。
//! 运行：cargo test --workspace --features sz-orm-mig/gray-release,sz-orm-mig/auto-rollback,sz-orm-ai/ai-autonomous-loop --test e2e_gray_autonomous -- --ignored

#![cfg(all(feature = "gray-release", feature = "auto-rollback"))]

use std::collections::HashMap;
use std::time::SystemTime;

use sz_orm_ai::autonomous::{
    ActionBoundary, AnomalyEvent, AutonomousAction, AutonomousPolicy, AutonomousPolicyEngine,
    CircuitBreakerConfig, Severity, TriggerCondition,
};
use sz_orm_mig::auto_rollback::AutoRollbackTrigger;
use sz_orm_mig::gray_release::{GrayReleaseConfig, GrayReleaseOrchestrator, ReleaseStatus};

fn make_policy(name: &str, event_type: &str) -> AutonomousPolicy {
    AutonomousPolicy {
        name: name.to_string(),
        version: "1.0".to_string(),
        trigger: TriggerCondition {
            event_type: event_type.to_string(),
            severity_threshold: Severity::Info,
            context_match: HashMap::new(),
        },
        action: AutonomousAction::AutoRemediation,
        boundary: ActionBoundary {
            min: 0.0,
            max: 100.0,
        },
        circuit_breaker: CircuitBreakerConfig::default(),
        enabled: true,
    }
}

fn make_anomaly_event(event_type: &str, hash: u64) -> AnomalyEvent {
    AnomalyEvent {
        event_type: event_type.to_string(),
        timestamp: SystemTime::now(),
        severity: Severity::Critical,
        context: HashMap::new(),
        event_hash: hash,
    }
}

#[tokio::test]
#[ignore = "跨模块联动 e2e 测试：需 --features gray-release + auto-rollback + ai-autonomous-loop"]
async fn e2e_gray_anomaly_triggers_autonomous_rollback() {
    let mut orch = GrayReleaseOrchestrator::new(GrayReleaseConfig::default());
    let progress = orch.start_release("rel-cross-1");
    assert_eq!(progress.status, ReleaseStatus::Running);
    assert_eq!(progress.current_percentage, 10);

    let rollback_trigger = AutoRollbackTrigger::new(0.05);

    let engine = AutonomousPolicyEngine::new();
    engine.load_from_config(vec![make_policy("gray_rollback_policy", "gray_anomaly")]);

    let error_rate = 0.08;
    let rollback = rollback_trigger.check_and_trigger(error_rate, "rel-cross-1");
    assert!(rollback.is_some(), "错误率 8% 超过阈值 5% 应触发回滚");
    let report = rollback.unwrap();
    assert!(report.success);
    assert!(report.reason.contains("0.08"));

    let event = make_anomaly_event("gray_anomaly", 1001);
    let decision = engine.handle_event(&event, false).await.unwrap();
    assert_eq!(decision.policy_name, "gray_rollback_policy");
    assert_eq!(decision.action, AutonomousAction::AutoRemediation);
    assert!(!decision.dry_run, "应执行实际动作");

    let rollback_result = orch.force_rollback(&report.reason);
    assert!(rollback_result.success);
    assert_eq!(orch.query_progress().status, ReleaseStatus::RolledBack);
}

#[tokio::test]
#[ignore = "跨模块联动 e2e 测试：需 --features gray-release + auto-rollback + ai-autonomous-loop"]
async fn e2e_gray_healthy_no_rollback_autonomous_idle() {
    let mut orch = GrayReleaseOrchestrator::new(GrayReleaseConfig::default());
    orch.start_release("rel-cross-2");

    let rollback_trigger = AutoRollbackTrigger::new(0.05);

    let engine = AutonomousPolicyEngine::new();
    engine.load_from_config(vec![make_policy("gray_rollback_policy", "gray_anomaly")]);

    let error_rate = 0.01;
    let rollback = rollback_trigger.check_and_trigger(error_rate, "rel-cross-2");
    assert!(rollback.is_none(), "错误率 1% 低于阈值 5% 不应触发回滚");

    let advance_result = orch.advance(true);
    assert!(advance_result.is_ok(), "健康判定通过应允许推进");
    assert_eq!(advance_result.unwrap().current_percentage, 20);

    let event = make_anomaly_event("gray_anomaly", 2002);
    let decision = engine.handle_event(&event, true).await.unwrap();
    assert!(decision.dry_run, "无异常时应 dry_run 模式");

    assert_eq!(orch.query_progress().status, ReleaseStatus::Running);
}

#[tokio::test]
#[ignore = "跨模块联动 e2e 测试：需 --features gray-release + auto-rollback + ai-autonomous-loop"]
async fn e2e_rollback_failure_escalates_with_autonomous_audit() {
    let mut orch = GrayReleaseOrchestrator::new(GrayReleaseConfig {
        rollback_threshold: 0.05,
        ..GrayReleaseConfig::default()
    });
    orch.start_release("rel-cross-3");

    let rollback_trigger = AutoRollbackTrigger::new(0.05);

    let engine = AutonomousPolicyEngine::new();
    engine.load_from_config(vec![make_policy("escalation_policy", "rollback_failed")]);

    let rollback = rollback_trigger
        .check_and_trigger(0.10, "rel-cross-3")
        .unwrap();

    use sz_orm_mig::gray_release::RollbackReport;
    let failed_report = RollbackReport {
        release_id: rollback.release_id,
        reason: rollback.reason,
        rolled_back_at: rollback.rolled_back_at,
        success: false,
    };
    let escalation = rollback_trigger.rollback_failed_escalate(&failed_report);
    assert!(escalation.escalated, "回滚失败应升级");
    assert!(escalation.snapshot_preserved, "应保留快照");

    let event = make_anomaly_event("rollback_failed", 3003);
    let decision = engine.handle_event(&event, false).await.unwrap();
    assert_eq!(decision.action, AutonomousAction::AutoRemediation);

    orch.force_rollback("escalated");
    assert_eq!(orch.query_progress().status, ReleaseStatus::RolledBack);
}
