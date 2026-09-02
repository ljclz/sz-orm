//! TASK-036 验证测试：决策链路追踪导出

use chrono::Utc;
use std::collections::HashMap;
use sz_orm_agent::tracing::{DecisionTrace, DecisionTracer};
use sz_orm_agent::types::{AgentStep, PerceptionSnapshot};

#[test]
fn test_trace_export() {
    let steps = vec![AgentStep {
        step_number: 1,
        perception: PerceptionSnapshot::default(),
        thought: "test thought".to_string(),
        action: "noop".to_string(),
        action_params: HashMap::new(),
        result: "ok".to_string(),
        success: true,
        timestamp: Utc::now(),
    }];
    let trace = DecisionTracer::export("task-trace", &steps);
    assert_eq!(trace.task_id, "task-trace");
    assert_eq!(trace.total_steps, 1);
    assert_eq!(trace.success_count, 1);
}

#[test]
fn test_trace_json() {
    let trace = DecisionTracer::export("task-json", &[]);
    let json = DecisionTracer::to_json(&trace).unwrap();
    let parsed: DecisionTrace = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.task_id, "task-json");
}
