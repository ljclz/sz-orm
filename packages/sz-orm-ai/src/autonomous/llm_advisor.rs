//! LLM 顾问（复杂场景 LLM 推理，3s 超时降级到规则决策）

use std::time::{Duration, Instant};

use super::types::{AnomalyEvent, AutonomousAction, LlmSuggestion};

/// LLM 顾问
pub struct LlmAdvisor {
    timeout: Duration,
}

impl LlmAdvisor {
    pub fn new() -> Self {
        Self {
            timeout: Duration::from_secs(3),
        }
    }

    pub fn with_timeout(timeout: Duration) -> Self {
        Self { timeout }
    }

    /// 获取 LLM 建议（超时降级到规则模式）
    pub async fn advise(&self, event: &AnomalyEvent) -> LlmSuggestion {
        let start = Instant::now();
        match self.call_llm(event).await {
            Some(suggestion) => suggestion,
            None => {
                let degraded = start.elapsed() >= self.timeout;
                LlmSuggestion {
                    action: self.rule_based_action(event),
                    reasoning: if degraded {
                        "DEGRADED_RULE_MODE: LLM 超时降级".to_string()
                    } else {
                        "DEGRADED_RULE_MODE: LLM 不可用".to_string()
                    },
                    confidence: 0.5,
                    degraded: true,
                }
            }
        }
    }

    /// 模拟 LLM 调用（实际实现会调用 LlmRouter::complete()）
    async fn call_llm(&self, _event: &AnomalyEvent) -> Option<LlmSuggestion> {
        None
    }

    /// 规则决策模式
    fn rule_based_action(&self, event: &AnomalyEvent) -> AutonomousAction {
        match event.event_type.as_str() {
            "high_latency" | "slow_query" => AutonomousAction::SlowQueryGovernance,
            "high_cpu" | "high_memory" => AutonomousAction::AutoScaling,
            "pool_exhausted" | "connection_leak" => AutonomousAction::AutoRemediation,
            _ => AutonomousAction::AutoTuning,
        }
    }
}

impl Default for LlmAdvisor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::time::SystemTime;

    fn make_event(event_type: &str) -> AnomalyEvent {
        AnomalyEvent {
            event_type: event_type.to_string(),
            timestamp: SystemTime::now(),
            severity: super::super::types::Severity::Warning,
            context: HashMap::new(),
            event_hash: 1,
        }
    }

    #[tokio::test]
    async fn test_llm_degrade_to_rule_mode() {
        let advisor = LlmAdvisor::new();
        let event = make_event("high_latency");
        let suggestion = advisor.advise(&event).await;
        assert!(suggestion.degraded);
        assert!(suggestion.reasoning.contains("DEGRADED_RULE_MODE"));
    }

    #[tokio::test]
    async fn test_rule_based_action_mapping() {
        let advisor = LlmAdvisor::new();
        let event = make_event("high_latency");
        let suggestion = advisor.advise(&event).await;
        assert_eq!(suggestion.action, AutonomousAction::SlowQueryGovernance);
    }

    #[tokio::test]
    async fn test_rule_based_action_high_cpu() {
        let advisor = LlmAdvisor::new();
        let event = make_event("high_cpu");
        let suggestion = advisor.advise(&event).await;
        assert_eq!(suggestion.action, AutonomousAction::AutoScaling);
    }

    #[tokio::test]
    async fn test_rule_based_action_pool_exhausted() {
        let advisor = LlmAdvisor::new();
        let event = make_event("pool_exhausted");
        let suggestion = advisor.advise(&event).await;
        assert_eq!(suggestion.action, AutonomousAction::AutoRemediation);
    }

    #[tokio::test]
    async fn test_rule_based_action_default() {
        let advisor = LlmAdvisor::new();
        let event = make_event("unknown_event");
        let suggestion = advisor.advise(&event).await;
        assert_eq!(suggestion.action, AutonomousAction::AutoTuning);
    }
}
