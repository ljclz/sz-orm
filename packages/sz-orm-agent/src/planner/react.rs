//! ReAct 规划器（TASK-015）
//!
//! 交替进行推理（Reasoning）与行动（Action）的多步规划。
//! 每步 LLM 输出思考链（thought）+ 工具调用（action），观测结果后进入下一步。

use crate::agent::{ActionExecutor, Planner, PlannerOutput};
use crate::types::{AgentError, AgentStep, AgentTaskSpec, PerceptionSnapshot, PlannerMode};
use async_trait::async_trait;
use std::collections::HashMap;

/// ReAct 规划器
///
/// 交替推理与行动：每步生成思考链 + 工具调用，观测结果后进入下一步。
/// 当 LLM 不可用时降级为规则模式。
pub struct ReActPlanner {
    /// LLM 提供者（可选，降级时为 None）
    llm_available: bool,
}

impl ReActPlanner {
    pub fn new() -> Self {
        Self {
            llm_available: true,
        }
    }

    pub fn with_llm_disabled() -> Self {
        Self {
            llm_available: false,
        }
    }

    /// 解析 LLM 输出为思考链 + 行动
    fn parse_llm_output(&self, output: &str, perception: &PerceptionSnapshot) -> PlannerOutput {
        let thought = format!("LLM 推理: {output}");
        let action = if !perception.deadlocks.is_empty() {
            "report_deadlock"
        } else if !perception.slow_queries.is_empty() {
            "analyze_slow_query"
        } else {
            "noop"
        };
        PlannerOutput {
            thought,
            action: action.to_string(),
            action_params: HashMap::new(),
        }
    }

    /// 规则降级模式
    fn rule_based_plan(&self, perception: &PerceptionSnapshot) -> PlannerOutput {
        if !perception.deadlocks.is_empty() {
            return PlannerOutput {
                thought: "规则降级: 检测到死锁".to_string(),
                action: "report_deadlock".to_string(),
                action_params: HashMap::from([(
                    "deadlocks".to_string(),
                    perception.deadlocks.join(","),
                )]),
            };
        }
        if !perception.slow_queries.is_empty() {
            return PlannerOutput {
                thought: "规则降级: 检测到慢查询".to_string(),
                action: "analyze_slow_query".to_string(),
                action_params: HashMap::from([(
                    "queries".to_string(),
                    perception.slow_queries.join(","),
                )]),
            };
        }
        PlannerOutput {
            thought: "规则降级: 系统正常".to_string(),
            action: "noop".to_string(),
            action_params: HashMap::new(),
        }
    }
}

impl Default for ReActPlanner {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Planner for ReActPlanner {
    async fn plan(
        &self,
        perception: &PerceptionSnapshot,
        history: &[AgentStep],
        spec: &AgentTaskSpec,
    ) -> Result<PlannerOutput, AgentError> {
        if spec.planner_mode != PlannerMode::ReAct && spec.planner_mode != PlannerMode::RuleBased {
            return Err(AgentError::ToolExecutionFailed(
                "ReActPlanner 仅支持 ReAct 模式".into(),
            ));
        }

        if history.len() >= spec.max_steps {
            return Err(AgentError::MaxStepsExceeded(spec.task_id.clone()));
        }

        if !self.llm_available {
            return Ok(self.rule_based_plan(perception));
        }

        let context = if history.is_empty() {
            format!(
                "任务: {}，当前健康评分: {}",
                spec.description, perception.health_score
            )
        } else {
            let last = history.last().unwrap();
            format!(
                "任务: {}，上一步: {} -> {}，当前健康评分: {}",
                spec.description, last.action, last.result, perception.health_score
            )
        };

        Ok(self.parse_llm_output(&context, perception))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AgentTaskSpec, PerceptionSnapshot, PlannerMode};
    use chrono::Utc;

    fn make_spec(mode: PlannerMode) -> AgentTaskSpec {
        AgentTaskSpec {
            task_id: "test".to_string(),
            description: "巡检".to_string(),
            planner_mode: mode,
            max_steps: 10,
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_react_first_step() {
        let planner = ReActPlanner::new();
        let spec = make_spec(PlannerMode::ReAct);
        let perception = PerceptionSnapshot::default();

        let output = planner.plan(&perception, &[], &spec).await.unwrap();
        assert!(!output.thought.is_empty());
        assert_eq!(output.action, "noop");
    }

    #[tokio::test]
    async fn test_react_with_slow_queries() {
        let planner = ReActPlanner::new();
        let spec = make_spec(PlannerMode::ReAct);
        let perception = PerceptionSnapshot {
            slow_queries: vec!["SELECT * FROM big".to_string()],
            ..Default::default()
        };

        let output = planner.plan(&perception, &[], &spec).await.unwrap();
        assert_eq!(output.action, "analyze_slow_query");
    }

    #[tokio::test]
    async fn test_react_llm_disabled_fallback() {
        let planner = ReActPlanner::with_llm_disabled();
        let spec = make_spec(PlannerMode::ReAct);
        let perception = PerceptionSnapshot::default();

        let output = planner.plan(&perception, &[], &spec).await.unwrap();
        assert!(output.thought.contains("规则降级"));
    }

    #[tokio::test]
    async fn test_react_max_steps_exceeded() {
        let planner = ReActPlanner::new();
        let spec = AgentTaskSpec {
            task_id: "test".to_string(),
            description: "巡检".to_string(),
            planner_mode: PlannerMode::ReAct,
            max_steps: 2,
            created_at: Utc::now(),
        };
        let perception = PerceptionSnapshot::default();
        let history = vec![
            AgentStep {
                step_number: 1,
                perception: perception.clone(),
                thought: "step 1".to_string(),
                action: "noop".to_string(),
                action_params: HashMap::new(),
                result: "ok".to_string(),
                success: true,
                timestamp: Utc::now(),
            },
            AgentStep {
                step_number: 2,
                perception: perception.clone(),
                thought: "step 2".to_string(),
                action: "noop".to_string(),
                action_params: HashMap::new(),
                result: "ok".to_string(),
                success: true,
                timestamp: Utc::now(),
            },
        ];

        let result = planner.plan(&perception, &history, &spec).await;
        assert!(result.is_err());
        match result {
            Err(AgentError::MaxStepsExceeded(_)) => {}
            _ => panic!("期望 MaxStepsExceeded"),
        }
    }
}
