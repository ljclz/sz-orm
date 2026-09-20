//! 端到端测试：灰度脏数据检测 → 隔离 + 全量回滚

use sz_orm_mig::gray_data_isolation::{ContainmentAction, GrayDataIsolationGuard};

#[tokio::test]
async fn e2e_data_contamination_detected() {
    let guard = GrayDataIsolationGuard::new();
    let report = guard.check_contamination("gray-1", &["orders".to_string()]);

    assert!(report.detected);
    assert_eq!(report.action, ContainmentAction::FullRollback);
    assert!(guard.is_isolated("gray-1"));
}

#[tokio::test]
async fn e2e_no_contamination() {
    let guard = GrayDataIsolationGuard::new();
    let report = guard.check_contamination("gray-1", &[]);

    assert!(!report.detected);
    assert_eq!(report.action, ContainmentAction::None);
    assert!(!guard.contamination_detected());
}

#[tokio::test]
async fn e2e_multiple_contaminations_isolated() {
    let guard = GrayDataIsolationGuard::new();
    guard.check_contamination("gray-1", &["orders".to_string()]);
    guard.check_contamination("gray-2", &["users".to_string()]);

    assert!(guard.is_isolated("gray-1"));
    assert!(guard.is_isolated("gray-2"));
    assert_eq!(guard.isolated_instances().len(), 2);
}
