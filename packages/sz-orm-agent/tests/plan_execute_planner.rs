//! TASK-016 验证测试：Plan-and-Execute 规划器

use chrono::Utc;
use sz_orm_agent::agent::Planner;
use sz_orm_agent::planner::plan_execute::PlanAndExecutePlanner;
use sz_orm_agent::types::{AgentTaskSpec, PerceptionSnapshot, PlannerMode};

#[tokio::test]
async fn test_plan_execute_generates_plan() {
    let planner = PlanAndExecutePlanner::new();
    let spec = AgentTaskSpec {
        task_id: "pne-test".to_string(),
        description: "巡检".to_string(),
        planner_mode: PlannerMode::PlanAndExecute,
        max_steps: 10,
        created_at: Utc::now(),
    };
    let output = planner
        .plan(&PerceptionSnapshot::default(), &[], &spec)
        .await
        .unwrap();
    assert!(output.thought.contains("生成完整计划"));
}
