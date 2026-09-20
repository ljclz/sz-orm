//! 端到端测试：灰度错误率超回滚阈值 → 自动回滚

use sz_orm_mig::auto_rollback::AutoRollbackTrigger;
use sz_orm_mig::gray_release::{GrayReleaseConfig, GrayReleaseOrchestrator, ReleaseStatus};

#[tokio::test]
async fn e2e_auto_rollback_triggered() {
    let mut orch = GrayReleaseOrchestrator::new(GrayReleaseConfig::default());
    orch.start_release("rel-1");

    let trigger = AutoRollbackTrigger::new(orch.config().rollback_threshold);
    let report = trigger.check_and_trigger(0.06, "rel-1");

    assert!(report.is_some());
    let report = report.unwrap();
    assert!(report.success);
    assert!(report.reason.contains("0.06"));
}

#[tokio::test]
async fn e2e_auto_rollback_completes() {
    let trigger = AutoRollbackTrigger::new(0.05);
    let report = trigger.check_and_trigger(0.10, "rel-1").unwrap();

    assert!(report.success);
    let escalation = trigger.rollback_failed_escalate(&report);
    assert!(!escalation.escalated);
}

#[tokio::test]
async fn e2e_auto_rollback_with_orchestrator() {
    let mut orch = GrayReleaseOrchestrator::new(GrayReleaseConfig::default());
    orch.start_release("rel-1");

    let trigger = AutoRollbackTrigger::new(0.05);
    let report = trigger.check_and_trigger(0.06, "rel-1");
    assert!(report.is_some());

    orch.force_rollback(&report.unwrap().reason);
    assert_eq!(orch.query_progress().status, ReleaseStatus::RolledBack);
}
