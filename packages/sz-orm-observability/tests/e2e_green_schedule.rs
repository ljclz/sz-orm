//! 端到端测试：绿色调度优先低排放实例

use sz_orm_observability::green::{GreenScheduler, InstanceCarbonIntensity};

#[tokio::test]
async fn e2e_green_schedule_lowest_carbon() {
    let scheduler = GreenScheduler::new(100);
    let instances = vec![
        InstanceCarbonIntensity {
            instance_id: "a".into(),
            carbon_intensity: 0.8,
            region: "us".into(),
        },
        InstanceCarbonIntensity {
            instance_id: "b".into(),
            carbon_intensity: 0.3,
            region: "eu".into(),
        },
    ];
    let decision = scheduler.schedule(&instances, 50);
    assert_eq!(decision.selected_instance, "b");
}
