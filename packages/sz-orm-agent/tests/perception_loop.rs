//! TASK-001 验证测试：感知-决策-执行循环

use std::sync::Arc;
use sz_orm_agent::agent::{AgentDriver, RuleBasedPlanner, SimpleExecutor};
use sz_orm_agent::perception::PerceptionCollector;
use sz_orm_agent::types::*;

#[tokio::test]
async fn test_perception_loop_completes() {
    let driver = AgentDriver::new(
        Arc::new(PerceptionCollector::new()),
        Arc::new(RuleBasedPlanner),
        Arc::new(SimpleExecutor),
    )
    .with_max_steps(3);

    let spec = AgentTaskSpec {
        task_id: "perception-test".to_string(),
        description: "数据库巡检任务".to_string(),
        planner_mode: PlannerMode::RuleBased,
        max_steps: 3,
        created_at: chrono::Utc::now(),
    };

    let steps = driver.run_loop(&spec, Default::default()).await.unwrap();

    assert!(!steps.is_empty(), "至少一轮完整循环");
    assert!(steps.len() <= 3, "不超过最大步数");

    for step in &steps {
        assert!(!step.thought.is_empty(), "思考链非空");
        assert!(!step.action.is_empty(), "行动非空");
    }
}

#[tokio::test]
async fn test_perception_loop_with_anomalies() {
    let driver = AgentDriver::new(
        Arc::new(PerceptionCollector::new()),
        Arc::new(RuleBasedPlanner),
        Arc::new(SimpleExecutor),
    )
    .with_max_steps(2);

    let spec = AgentTaskSpec {
        task_id: "anomaly-test".to_string(),
        description: "异常检测任务".to_string(),
        planner_mode: PlannerMode::RuleBased,
        max_steps: 2,
        created_at: chrono::Utc::now(),
    };

    let signals = sz_orm_agent::agent::PerceptionSignals {
        slow_queries: vec!["slow-1".into()],
        deadlocks: vec!["deadlock-1".into()],
        anomalies: vec!["anomaly-1".into()],
        failure_predictions: vec!["failure-1".into()],
        ..Default::default()
    };

    let steps = driver.run_loop(&spec, signals).await.unwrap();

    assert!(!steps.is_empty());
    assert_eq!(steps[0].action, "report_deadlock");
    assert!(steps[0].success);
}

#[tokio::test]
async fn test_perception_snapshot_health_score() {
    let collector = PerceptionCollector::new();
    let snapshot = collector
        .collect(
            vec!["q".into()],
            std::collections::HashMap::from([("utilization".into(), 0.95)]),
            vec!["d".into()],
            vec!["a".into()],
            vec!["f".into()],
        )
        .await
        .unwrap();

    assert!(snapshot.health_score < 1.0, "有异常时健康评分下降");
    assert!(snapshot.health_score >= 0.0, "健康评分非负");
}
