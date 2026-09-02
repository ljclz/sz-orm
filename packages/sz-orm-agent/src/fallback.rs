//! LLM 降级规则模式（TASK-018）
//!
//! 当外部 LLM 服务不可用时，Agent 切换为基于规则的决策模式。

use crate::agent::{Planner, PlannerOutput};
use crate::types::{AgentError, AgentStep, AgentTaskSpec, PerceptionSnapshot};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

/// LLM 降级管理器
///
/// 监控 LLM 调用失败次数，超过阈值时自动切换为规则模式。
pub struct LlmFallbackManager {
    /// 连续失败次数
    consecutive_failures: Arc<AtomicU32>,
    /// 降级阈值
    threshold: u32,
    /// 是否已降级
    degraded: Arc<AtomicBool>,
}

impl LlmFallbackManager {
    pub fn new(threshold: u32) -> Self {
        Self {
            consecutive_failures: Arc::new(AtomicU32::new(0)),
            threshold,
            degraded: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 记录 LLM 调用成功
    pub fn record_success(&self) {
        self.consecutive_failures.store(0, Ordering::Relaxed);
        self.degraded.store(false, Ordering::Relaxed);
    }

    /// 记录 LLM 调用失败
    pub fn record_failure(&self) -> bool {
        let count = self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;
        if count >= self.threshold {
            self.degraded.store(true, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    /// 是否已降级
    pub fn is_degraded(&self) -> bool {
        self.degraded.load(Ordering::Relaxed)
    }

    /// 当前连续失败次数
    pub fn failure_count(&self) -> u32 {
        self.consecutive_failures.load(Ordering::Relaxed)
    }
}

/// 规则降级规划器
///
/// 无 LLM，基于规则匹配生成行动。
pub struct RuleBasedFallbackPlanner {
    fallback_manager: Arc<LlmFallbackManager>,
}

impl RuleBasedFallbackPlanner {
    pub fn new(fallback_manager: Arc<LlmFallbackManager>) -> Self {
        Self { fallback_manager }
    }
}

#[async_trait]
impl Planner for RuleBasedFallbackPlanner {
    async fn plan(
        &self,
        perception: &PerceptionSnapshot,
        history: &[AgentStep],
        spec: &AgentTaskSpec,
    ) -> Result<PlannerOutput, AgentError> {
        if !self.fallback_manager.is_degraded() {
            return Err(AgentError::LlmUnavailable("LLM 仍可用，无需降级".into()));
        }

        if history.len() >= spec.max_steps {
            return Err(AgentError::MaxStepsExceeded(spec.task_id.clone()));
        }

        let thought = format!(
            "规则降级模式: 连续失败 {} 次，健康评分 {}",
            self.fallback_manager.failure_count(),
            perception.health_score
        );

        let action = if !perception.deadlocks.is_empty() {
            "report_deadlock"
        } else if !perception.slow_queries.is_empty() {
            "analyze_slow_query"
        } else if !perception.anomalies.is_empty() {
            "investigate_anomaly"
        } else {
            "noop"
        };

        Ok(PlannerOutput {
            thought,
            action: action.to_string(),
            action_params: HashMap::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::PlannerMode;
    use chrono::Utc;

    fn make_spec() -> AgentTaskSpec {
        AgentTaskSpec {
            task_id: "test".to_string(),
            description: "巡检".to_string(),
            planner_mode: PlannerMode::RuleBased,
            max_steps: 10,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn test_fallback_threshold() {
        let manager = LlmFallbackManager::new(3);
        assert!(!manager.record_failure());
        assert!(!manager.record_failure());
        assert!(manager.record_failure(), "第 3 次失败触发降级");
        assert!(manager.is_degraded());
    }

    #[test]
    fn test_fallback_recovery() {
        let manager = LlmFallbackManager::new(2);
        manager.record_failure();
        manager.record_failure();
        assert!(manager.is_degraded());
        manager.record_success();
        assert!(!manager.is_degraded(), "成功后恢复");
        assert_eq!(manager.failure_count(), 0);
    }

    #[tokio::test]
    async fn test_rule_based_planner_when_degraded() {
        let manager = Arc::new(LlmFallbackManager::new(1));
        manager.record_failure();
        assert!(manager.is_degraded());

        let planner = RuleBasedFallbackPlanner::new(manager);
        let spec = make_spec();
        let perception = PerceptionSnapshot {
            slow_queries: vec!["slow".to_string()],
            ..Default::default()
        };

        let output = planner.plan(&perception, &[], &spec).await.unwrap();
        assert!(output.thought.contains("规则降级"));
        assert_eq!(output.action, "analyze_slow_query");
    }

    #[tokio::test]
    async fn test_rule_based_planner_rejects_when_not_degraded() {
        let manager = Arc::new(LlmFallbackManager::new(5));
        let planner = RuleBasedFallbackPlanner::new(manager);
        let spec = make_spec();
        let perception = PerceptionSnapshot::default();

        let result = planner.plan(&perception, &[], &spec).await;
        assert!(result.is_err(), "未降级时应拒绝");
    }
}
