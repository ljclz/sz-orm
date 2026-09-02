//! TASK-015 验证测试：ReAct 规划器

use chrono::Utc;
use sz_orm_agent::agent::Planner;
use sz_orm_agent::planner::react::ReActPlanner;
use sz_orm_agent::types::{AgentTaskSpec, PerceptionSnapshot, PlannerMode};

fn make_spec() -> AgentTaskSpec {
    AgentTaskSpec {
        task_id: "react-test".to_string(),
        description: "巡检".to_string(),
        planner_mode: PlannerMode::ReAct,
        max_steps: 10,
        created_at: Utc::now(),
    }
}

#[tokio::test]
async fn test_react_first_step() {
    let planner = ReActPlanner::new();
    let output = planner
        .plan(&PerceptionSnapshot::default(), &[], &make_spec())
        .await
        .unwrap();
    assert!(!output.thought.is_empty());
}

#[tokio::test]
async fn test_react_llm_disabled() {
    let planner = ReActPlanner::with_llm_disabled();
    let output = planner
        .plan(&PerceptionSnapshot::default(), &[], &make_spec())
        .await
        .unwrap();
    assert!(output.thought.contains("规则降级"));
}
