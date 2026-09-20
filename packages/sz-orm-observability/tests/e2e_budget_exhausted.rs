//! 端到端测试：错误预算耗尽 → 禁止非关键变更

use sz_orm_observability::slo_automation::ErrorBudgetTracker;

#[tokio::test]
async fn e2e_budget_exhausted_blocks_change() {
    let mut tracker = ErrorBudgetTracker::new(100.0);
    assert!(tracker.allow_non_critical_change());
    tracker.update(100.0);
    assert!(!tracker.allow_non_critical_change());
}
