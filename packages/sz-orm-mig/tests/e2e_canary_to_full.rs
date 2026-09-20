//! 端到端测试：金丝雀 1 实例 → 健康判定 → 全量推广

use sz_orm_mig::canary_release::CanaryReleaseManager;
use sz_orm_mig::gray_release::ReleaseStatus;

#[tokio::test]
async fn e2e_canary_to_full_promotion() {
    let manager = CanaryReleaseManager::new(10);
    manager.add_canary("instance-1");
    assert_eq!(manager.canary_count(), 1);

    let progress = manager.start_canary("rel-1");
    assert_eq!(progress.status, ReleaseStatus::Running);
    assert_eq!(progress.current_percentage, 10);

    let progress = manager.promote_to_full(true).unwrap();
    assert_eq!(progress.status, ReleaseStatus::Completed);
    assert_eq!(progress.current_percentage, 100);
}

#[tokio::test]
async fn e2e_canary_unhealthy_no_promotion() {
    let manager = CanaryReleaseManager::new(10);
    manager.add_canary("instance-1");
    manager.start_canary("rel-1");

    assert!(manager.promote_to_full(false).is_err());
    assert_eq!(manager.query_progress().status, ReleaseStatus::Running);
}

#[tokio::test]
async fn e2e_canary_rollback() {
    let manager = CanaryReleaseManager::new(10);
    manager.add_canary("instance-1");
    manager.start_canary("rel-1");

    manager.rollback_canary("unhealthy").unwrap();
    assert_eq!(manager.query_progress().status, ReleaseStatus::RolledBack);
    assert_eq!(manager.canary_count(), 0);
}
