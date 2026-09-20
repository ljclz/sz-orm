//! 端到端测试：灰度发布进度追踪器
//!
//! 验证 GrayReleaseProgressTracker 的初始化、推进、暂停、恢复、完成全链路。

use sz_orm_mig::gray_progress_tracker::GrayReleaseProgressTracker;
use sz_orm_mig::gray_release::ReleaseStatus;

#[test]
fn e2e_progress_tracker_full_lifecycle() {
    let tracker = GrayReleaseProgressTracker::new();
    tracker.init("rel-progress-1");
    let p = tracker.query_progress();
    assert_eq!(p.status, ReleaseStatus::Running);
    assert_eq!(p.current_percentage, 0);

    tracker.update_percentage(20);
    assert_eq!(tracker.query_progress().current_percentage, 20);

    tracker.pause();
    assert_eq!(tracker.query_progress().status, ReleaseStatus::Paused);

    tracker.resume();
    assert_eq!(tracker.query_progress().status, ReleaseStatus::Running);

    tracker.update_percentage(100);
    tracker.complete();
    assert_eq!(tracker.query_progress().status, ReleaseStatus::Completed);
    assert_eq!(tracker.history().len(), 6);
}

#[test]
fn e2e_progress_tracker_rollback() {
    let tracker = GrayReleaseProgressTracker::new();
    tracker.init("rel-progress-2");
    tracker.update_percentage(30);
    tracker.rollback();
    assert_eq!(tracker.query_progress().status, ReleaseStatus::RolledBack);
    assert_eq!(tracker.history().len(), 3);
}
