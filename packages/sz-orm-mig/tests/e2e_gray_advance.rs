//! 端到端测试：灰度 10% → 健康判定 → 推进 20%

use sz_orm_mig::gray_health_judge::{GrayHealthJudge, HealthMetrics};
use sz_orm_mig::gray_release::{GrayReleaseConfig, GrayReleaseOrchestrator, ReleaseStatus};

#[tokio::test]
async fn e2e_gray_advance_10_to_20() {
    let mut orch = GrayReleaseOrchestrator::new(GrayReleaseConfig::default());
    let progress = orch.start_release("rel-1");
    assert_eq!(progress.current_percentage, 10);

    let judge = GrayHealthJudge::new(
        orch.config().health_judge_conditions.clone(),
        orch.config().rollback_threshold,
    );
    let metrics = HealthMetrics {
        error_rate: 0.001,
        latency_ms: 100.0,
        custom: std::collections::HashMap::new(),
    };
    let result = judge.judge(&metrics);
    assert!(result.healthy);

    let progress = orch.advance(true).unwrap();
    assert_eq!(progress.current_percentage, 20);
}

#[tokio::test]
async fn e2e_gray_advance_unhealthy_blocked() {
    let mut orch = GrayReleaseOrchestrator::new(GrayReleaseConfig::default());
    orch.start_release("rel-1");

    let result = orch.advance(false);
    assert!(result.is_err());
    assert_eq!(orch.query_progress().current_percentage, 10);
}

#[tokio::test]
async fn e2e_gray_advance_full_to_completion() {
    let mut orch = GrayReleaseOrchestrator::new(GrayReleaseConfig::default());
    orch.start_release("rel-1");

    for _ in 0..9 {
        orch.advance(true).unwrap();
    }
    assert_eq!(orch.query_progress().status, ReleaseStatus::Completed);
}
