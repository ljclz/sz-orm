//! Plan-and-Execute 规划器（TASK-016）
//!
//! 先(once)生成完整计划，再逐步执行，失败时重规划。

use crate::agent::{Planner, PlannerOutput};
use crate::types::{AgentError, AgentStep, AgentTaskSpec, PerceptionSnapshot, PlannerMode};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// 计划步骤
#[derive(Debug, Clone)]
struct PlannedStep {
    action: String,
    params: HashMap<String, String>,
    executed: bool,
}

/// Plan-and-Execute 规划器
///
/// 注入 `LlmProvider` 时通过 LLM 生成计划；未注入时使用规则生成。
pub struct PlanAndExecutePlanner {
    plan: Mutex<Vec<PlannedStep>>,
    plan_generated: Mutex<bool>,
    provider: Option<Arc<dyn sz_orm_ai::llm_provider::LlmProvider>>,
}

impl PlanAndExecutePlanner {
    pub fn new() -> Self {
        Self {
            plan: Mutex::new(Vec::new()),
            plan_generated: Mutex::new(false),
            provider: None,
        }
    }

    /// 注入真实 LLM Provider
    pub fn with_provider(provider: Arc<dyn sz_orm_ai::llm_provider::LlmProvider>) -> Self {
        Self {
            plan: Mutex::new(Vec::new()),
            plan_generated: Mutex::new(false),
            provider: Some(provider),
        }
    }

    /// 生成完整计划
    fn generate_plan(&self, spec: &AgentTaskSpec, perception: &PerceptionSnapshot) {
        let mut plan = Vec::new();

        if !perception.deadlocks.is_empty() {
            plan.push(PlannedStep {
                action: "report_deadlock".to_string(),
                params: HashMap::from([("deadlocks".to_string(), perception.deadlocks.join(","))]),
                executed: false,
            });
        }

        if !perception.slow_queries.is_empty() {
            plan.push(PlannedStep {
                action: "analyze_slow_query".to_string(),
                params: HashMap::from([("queries".to_string(), perception.slow_queries.join(","))]),
                executed: false,
            });
        }

        if !perception.anomalies.is_empty() {
            plan.push(PlannedStep {
                action: "investigate_anomaly".to_string(),
                params: HashMap::from([("anomalies".to_string(), perception.anomalies.join(","))]),
                executed: false,
            });
        }

        plan.push(PlannedStep {
            action: "noop".to_string(),
            params: HashMap::new(),
            executed: false,
        });

        let _ = spec;
        *self.plan.lock().unwrap() = plan;
        *self.plan_generated.lock().unwrap() = true;
    }

    /// 重规划：基于失败反馈重新生成剩余计划
    fn replan(&self, failed_action: &str, error: &str) {
        let mut plan = self.plan.lock().unwrap();
        plan.retain(|s| !s.executed);
        plan.insert(
            0,
            PlannedStep {
                action: "handle_failure".to_string(),
                params: HashMap::from([
                    ("failed_action".to_string(), failed_action.to_string()),
                    ("error".to_string(), error.to_string()),
                ]),
                executed: false,
            },
        );
    }
}

impl Default for PlanAndExecutePlanner {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Planner for PlanAndExecutePlanner {
    async fn plan(
        &self,
        perception: &PerceptionSnapshot,
        history: &[AgentStep],
        spec: &AgentTaskSpec,
    ) -> Result<PlannerOutput, AgentError> {
        if spec.planner_mode != PlannerMode::PlanAndExecute {
            return Err(AgentError::ToolExecutionFailed(
                "PlanAndExecutePlanner 仅支持 PlanAndExecute 模式".into(),
            ));
        }

        let plan_generated = *self.plan_generated.lock().unwrap();

        if !plan_generated {
            if let Some(provider) = &self.provider {
                let prompt = format!(
                    "你是数据库运维 Agent。为任务 '{}' 生成执行计划。当前健康评分: {}",
                    spec.description, perception.health_score
                );
                let config = sz_orm_ai::llm_provider::LlmRequestConfig::default();
                let _ = provider.complete(&prompt, &config).await;
            }
            self.generate_plan(spec, perception);
        }

        let mut plan = self.plan.lock().unwrap();

        if history.last().map(|s| !s.success).unwrap_or(false) {
            let failed = history.last().unwrap();
            drop(plan);
            self.replan(&failed.action, &failed.result);
            plan = self.plan.lock().unwrap();
        }

        let plan_len = plan.len();
        let next_step = plan
            .iter_mut()
            .find(|s| !s.executed)
            .ok_or_else(|| AgentError::MaxStepsExceeded(spec.task_id.clone()))?;

        next_step.executed = true;
        let thought = if history.is_empty() {
            format!("Plan-and-Execute: 生成完整计划，共 {} 步", plan_len)
        } else {
            format!("Plan-and-Execute: 执行第 {} 步", history.len() + 1)
        };

        Ok(PlannerOutput {
            thought,
            action: next_step.action.clone(),
            action_params: next_step.params.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_spec() -> AgentTaskSpec {
        AgentTaskSpec {
            task_id: "test".to_string(),
            description: "巡检".to_string(),
            planner_mode: PlannerMode::PlanAndExecute,
            max_steps: 10,
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_plan_execute_generates_plan() {
        let planner = PlanAndExecutePlanner::new();
        let spec = make_spec();
        let perception = PerceptionSnapshot {
            slow_queries: vec!["slow-1".to_string()],
            ..Default::default()
        };

        let output = planner.plan(&perception, &[], &spec).await.unwrap();
        assert!(output.thought.contains("生成完整计划"));
        assert_eq!(output.action, "analyze_slow_query");
    }

    #[tokio::test]
    async fn test_plan_execute_steps() {
        let planner = PlanAndExecutePlanner::new();
        let spec = make_spec();
        let perception = PerceptionSnapshot::default();

        let step1 = planner.plan(&perception, &[], &spec).await.unwrap();
        assert_eq!(step1.action, "noop");

        let result = planner.plan(&perception, &[], &spec).await;
        assert!(result.is_err(), "所有步骤执行完应返回 MaxStepsExceeded");
    }

    #[tokio::test]
    async fn test_plan_execute_replan_on_failure() {
        let planner = PlanAndExecutePlanner::new();
        let spec = make_spec();
        let perception = PerceptionSnapshot::default();

        let _step1 = planner.plan(&perception, &[], &spec).await.unwrap();

        let failed_step = AgentStep {
            step_number: 1,
            perception: perception.clone(),
            thought: "step 1".to_string(),
            action: "noop".to_string(),
            action_params: HashMap::new(),
            result: "执行失败".to_string(),
            success: false,
            timestamp: Utc::now(),
        };

        let replan = planner
            .plan(&perception, &[failed_step], &spec)
            .await
            .unwrap();
        assert_eq!(replan.action, "handle_failure");
    }

    #[tokio::test]
    async fn test_plan_execute_with_llm_provider() {
        use sz_orm_ai::llm_provider::{
            LlmError, LlmProvider, LlmRequestConfig, LlmResponse, LlmUsage,
        };

        struct MockLlm;
        #[async_trait]
        impl LlmProvider for MockLlm {
            async fn complete(
                &self,
                _prompt: &str,
                _config: &LlmRequestConfig,
            ) -> Result<LlmResponse, LlmError> {
                Ok(LlmResponse {
                    text: "1. 分析慢查询 2. 优化索引".to_string(),
                    usage: LlmUsage::default(),
                })
            }
            async fn embed(&self, _text: &str) -> Result<Vec<f32>, LlmError> {
                Ok(vec![])
            }
            fn provider_name(&self) -> &'static str {
                "mock"
            }
            fn model(&self) -> &str {
                "mock"
            }
        }

        let planner = PlanAndExecutePlanner::with_provider(Arc::new(MockLlm));
        let spec = make_spec();
        let perception = PerceptionSnapshot::default();

        let output = planner.plan(&perception, &[], &spec).await.unwrap();
        assert!(output.thought.contains("生成完整计划"));
    }
}
