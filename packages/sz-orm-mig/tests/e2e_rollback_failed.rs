//! 端到端测试：回滚执行失败 → 升级人工，保留当前状态快照

use sz_orm_mig::auto_rollback::AutoRollbackTrigger;
use sz_orm_mig::gray_release::RollbackReport;

#[tokio::test]
async fn e2e_rollback_failed_escalates() {
    let trigger = AutoRollbackTrigger::new(0.05);
    let failed_report = RollbackReport {
        release_id: "rel-1".to_string(),
        reason: "rollback timeout".to_string(),
        rolled_back_at: 0,
        success: false,
    };

    let escalation = trigger.rollback_failed_escalate(&failed_report);
    assert!(escalation.escalated);
    assert!(escalation.snapshot_preserved);
    assert!(escalation.message.contains("升级人工处理"));
}

#[tokio::test]
async fn e2e_rollback_success_no_escalation() {
    let trigger = AutoRollbackTrigger::new(0.05);
    let success_report = RollbackReport {
        release_id: "rel-1".to_string(),
        reason: "error rate exceeded".to_string(),
        rolled_back_at: 0,
        success: true,
    };

    let escalation = trigger.rollback_failed_escalate(&success_report);
    assert!(!escalation.escalated);
    assert!(!escalation.snapshot_preserved);
}

#[tokio::test]
async fn e2e_rollback_triggered_then_failed_escalates() {
    let trigger = AutoRollbackTrigger::new(0.05);
    let report = trigger.check_and_trigger(0.10, "rel-1").unwrap();

    let failed_report = RollbackReport {
        success: false,
        ..report
    };
    let escalation = trigger.rollback_failed_escalate(&failed_report);
    assert!(escalation.escalated);
}
