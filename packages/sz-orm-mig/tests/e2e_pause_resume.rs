//! 端到端测试：灰度暂停 → 恢复 → 从暂停点继续

use sz_orm_mig::gray_release::{GrayReleaseConfig, GrayReleaseOrchestrator, ReleaseStatus};

#[tokio::test]
async fn e2e_pause_resume_preserves_percentage() {
    let mut orch = GrayReleaseOrchestrator::new(GrayReleaseConfig::default());
    orch.start_release("rel-1");
    orch.advance(true).unwrap();
    assert_eq!(orch.query_progress().current_percentage, 20);

    orch.pause().unwrap();
    assert_eq!(orch.query_progress().status, ReleaseStatus::Paused);
    assert_eq!(orch.query_progress().current_percentage, 20);

    orch.resume().unwrap();
    assert_eq!(orch.query_progress().status, ReleaseStatus::Running);
    assert_eq!(orch.query_progress().current_percentage, 20);
}

#[tokio::test]
async fn e2e_pause_not_running_rejected() {
    let mut orch = GrayReleaseOrchestrator::new(GrayReleaseConfig::default());
    orch.start_release("rel-1");
    orch.pause().unwrap();

    assert!(orch.pause().is_err());
}

#[tokio::test]
async fn e2e_resume_not_paused_rejected() {
    let mut orch = GrayReleaseOrchestrator::new(GrayReleaseConfig::default());
    orch.start_release("rel-1");

    assert!(orch.resume().is_err());
}
