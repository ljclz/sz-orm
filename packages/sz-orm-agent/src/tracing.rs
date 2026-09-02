//! Agent 决策链路结构化追踪导出（TASK-036）
//!
//! 每步感知输入/思考链/工具调用/结果输出为结构化 JSON 记录。

use crate::types::AgentStep;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 决策链路记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionTrace {
    pub task_id: String,
    pub steps: Vec<StepTrace>,
    pub total_steps: usize,
    pub success_count: usize,
    pub failure_count: usize,
}

/// 单步追踪
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepTrace {
    pub step_number: usize,
    pub perception: PerceptionTrace,
    pub thought: String,
    pub action: String,
    pub action_params: HashMap<String, String>,
    pub result: String,
    pub success: bool,
    pub timestamp: String,
}

/// 感知追踪
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerceptionTrace {
    pub health_score: f64,
    pub slow_query_count: usize,
    pub deadlock_count: usize,
    pub anomaly_count: usize,
    pub failure_prediction_count: usize,
}

/// 决策链路追踪器
pub struct DecisionTracer;

impl DecisionTracer {
    /// 从 Agent 步骤生成结构化追踪
    pub fn export(task_id: &str, steps: &[AgentStep]) -> DecisionTrace {
        let step_traces: Vec<StepTrace> = steps
            .iter()
            .map(|s| StepTrace {
                step_number: s.step_number,
                perception: PerceptionTrace {
                    health_score: s.perception.health_score,
                    slow_query_count: s.perception.slow_queries.len(),
                    deadlock_count: s.perception.deadlocks.len(),
                    anomaly_count: s.perception.anomalies.len(),
                    failure_prediction_count: s.perception.failure_predictions.len(),
                },
                thought: s.thought.clone(),
                action: s.action.clone(),
                action_params: s.action_params.clone(),
                result: s.result.clone(),
                success: s.success,
                timestamp: s.timestamp.to_rfc3339(),
            })
            .collect();

        let success_count = step_traces.iter().filter(|t| t.success).count();
        let failure_count = step_traces.len() - success_count;

        DecisionTrace {
            task_id: task_id.to_string(),
            total_steps: step_traces.len(),
            steps: step_traces,
            success_count,
            failure_count,
        }
    }

    /// 导出为 JSON 字符串
    pub fn to_json(trace: &DecisionTrace) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(trace)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AgentStep, PerceptionSnapshot};
    use chrono::Utc;
    use std::collections::HashMap;

    fn make_step(n: usize, success: bool) -> AgentStep {
        AgentStep {
            step_number: n,
            perception: PerceptionSnapshot {
                slow_queries: vec!["q".to_string()],
                ..Default::default()
            },
            thought: format!("thought {n}"),
            action: format!("action_{n}"),
            action_params: HashMap::from([("key".to_string(), "value".to_string())]),
            result: format!("result {n}"),
            success,
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn test_export_trace() {
        let steps = vec![make_step(1, true), make_step(2, true), make_step(3, false)];
        let trace = DecisionTracer::export("task-1", &steps);

        assert_eq!(trace.task_id, "task-1");
        assert_eq!(trace.total_steps, 3);
        assert_eq!(trace.success_count, 2);
        assert_eq!(trace.failure_count, 1);
    }

    #[test]
    fn test_export_json_parseable() {
        let steps = vec![make_step(1, true)];
        let trace = DecisionTracer::export("task-2", &steps);
        let json = DecisionTracer::to_json(&trace).unwrap();

        let parsed: DecisionTrace = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.task_id, "task-2");
        assert_eq!(parsed.total_steps, 1);
    }

    #[test]
    fn test_perception_trace_counts() {
        let steps = vec![make_step(1, true)];
        let trace = DecisionTracer::export("task-3", &steps);
        assert_eq!(trace.steps[0].perception.slow_query_count, 1);
        assert_eq!(trace.steps[0].perception.deadlock_count, 0);
    }

    #[test]
    fn test_empty_steps() {
        let trace = DecisionTracer::export("task-empty", &[]);
        assert_eq!(trace.total_steps, 0);
        assert_eq!(trace.success_count, 0);
    }
}
