//! 跨模块联动 e2e 测试：绿色计算 + 灰度调度
//!
//! 验证绿色调度影响灰度实例选择。
//! 运行：cargo test --workspace --features sz-orm-observability/green-computing,sz-orm-mig/gray-release --test e2e_green_gray -- --ignored

#![cfg(feature = "green-computing")]

use sz_orm_mig::gray_release::{GrayReleaseConfig, GrayReleaseOrchestrator, ReleaseStatus};
use sz_orm_observability::green::{GreenScheduler, InstanceCarbonIntensity};

fn make_instance(id: &str, intensity: f64, region: &str) -> InstanceCarbonIntensity {
    InstanceCarbonIntensity {
        instance_id: id.to_string(),
        carbon_intensity: intensity,
        region: region.to_string(),
    }
}

#[test]
#[ignore = "跨模块联动 e2e 测试：需 --features green-computing + gray-release"]
fn e2e_green_scheduler_selects_lowest_carbon_for_gray() {
    let mut orch = GrayReleaseOrchestrator::new(GrayReleaseConfig::default());
    let progress = orch.start_release("rel-green-1");
    assert_eq!(progress.status, ReleaseStatus::Running);

    let scheduler = GreenScheduler::new(100);

    let instances = vec![
        make_instance("gray-instance-a", 0.8, "region-a"),
        make_instance("gray-instance-b", 0.3, "region-b"),
        make_instance("gray-instance-c", 0.5, "region-c"),
    ];

    let decision = scheduler.schedule(&instances, 50);
    assert_eq!(decision.selected_instance, "gray-instance-b");
    assert!(!decision.fallback, "应选择最低碳强度实例");
    assert!(decision.reason.contains("绿色调度"));

    let advance = orch.advance(true).unwrap();
    assert_eq!(advance.current_percentage, 20);
}

#[test]
#[ignore = "跨模块联动 e2e 测试：需 --features green-computing + gray-release"]
fn e2e_green_scheduler_latency_fallback_during_gray() {
    let mut orch = GrayReleaseOrchestrator::new(GrayReleaseConfig::default());
    orch.start_release("rel-green-2");

    let scheduler = GreenScheduler::new(100);

    let instances = vec![
        make_instance("gray-instance-a", 0.3, "region-a"),
        make_instance("gray-instance-b", 0.8, "region-b"),
    ];

    let decision = scheduler.schedule(&instances, 150);
    assert!(decision.fallback, "延迟超限应回退到负载均衡");
    assert!(decision.reason.contains("GREEN_SCHEDULER_LATENCY_FALLBACK"));

    let advance = orch.advance(true).unwrap();
    assert_eq!(advance.current_percentage, 20, "灰度发布应继续推进");
}

#[test]
#[ignore = "跨模块联动 e2e 测试：需 --features green-computing + gray-release"]
fn e2e_green_gray_release_completion_with_green_scheduling() {
    let mut orch = GrayReleaseOrchestrator::new(GrayReleaseConfig::default());
    orch.start_release("rel-green-3");

    let scheduler = GreenScheduler::new(200);

    let instances = vec![
        make_instance("inst-low-carbon", 0.2, "green-region"),
        make_instance("inst-high-carbon", 0.9, "brown-region"),
    ];

    for _ in 0..9 {
        let decision = scheduler.schedule(&instances, 50);
        assert_eq!(decision.selected_instance, "inst-low-carbon");
        let progress = orch.advance(true).unwrap();
        if progress.current_percentage >= 100 {
            assert_eq!(progress.status, ReleaseStatus::Completed);
            return;
        }
    }
    panic!("灰度发布应在 9 次推进后完成");
}

#[test]
#[ignore = "跨模块联动 e2e 测试：需 --features green-computing + gray-release"]
fn e2e_green_gray_rollback_preserves_green_preference() {
    let mut orch = GrayReleaseOrchestrator::new(GrayReleaseConfig::default());
    orch.start_release("rel-green-4");

    let scheduler = GreenScheduler::new(100);
    let instances = vec![
        make_instance("green-inst", 0.2, "green-region"),
        make_instance("brown-inst", 0.9, "brown-region"),
    ];

    let decision_before = scheduler.schedule(&instances, 50);
    assert_eq!(decision_before.selected_instance, "green-inst");

    let rollback = orch.force_rollback("carbon spike in brown region");
    assert!(rollback.success);

    let decision_after = scheduler.schedule(&instances, 50);
    assert_eq!(
        decision_after.selected_instance, "green-inst",
        "回滚后仍应优先绿色实例"
    );
    assert_eq!(orch.query_progress().status, ReleaseStatus::RolledBack);
}
