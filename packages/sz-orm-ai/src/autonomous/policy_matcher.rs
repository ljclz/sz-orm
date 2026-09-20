//! 策略匹配器

use super::types::{AnomalyEvent, AutonomousPolicy, TriggerCondition};

/// 策略匹配器：异常事件 → 策略匹配
#[derive(Debug, Clone)]
pub struct PolicyMatcher;

impl PolicyMatcher {
    pub fn new() -> Self {
        Self
    }

    /// 匹配策略：返回第一个匹配的策略
    pub fn match_policy<'a>(
        &self,
        event: &AnomalyEvent,
        policies: &'a [AutonomousPolicy],
    ) -> Option<&'a AutonomousPolicy> {
        policies
            .iter()
            .find(|p| p.enabled && Self::matches_trigger(event, &p.trigger))
    }

    /// 检查事件是否匹配触发条件
    fn matches_trigger(event: &AnomalyEvent, trigger: &TriggerCondition) -> bool {
        if event.event_type != trigger.event_type {
            return false;
        }
        if event.severity < trigger.severity_threshold {
            return false;
        }
        for (key, expected) in &trigger.context_match {
            match event.context.get(key) {
                Some(actual) if actual == expected => {}
                _ => return false,
            }
        }
        true
    }

    /// 匹配所有符合条件的策略
    pub fn match_all_policies<'a>(
        &self,
        event: &AnomalyEvent,
        policies: &'a [AutonomousPolicy],
    ) -> Vec<&'a AutonomousPolicy> {
        policies
            .iter()
            .filter(|p| p.enabled && Self::matches_trigger(event, &p.trigger))
            .collect()
    }
}

impl Default for PolicyMatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::Severity;
    use super::*;
    use std::collections::HashMap;
    use std::time::SystemTime;

    fn make_event(event_type: &str, severity: Severity) -> AnomalyEvent {
        AnomalyEvent {
            event_type: event_type.to_string(),
            timestamp: SystemTime::now(),
            severity,
            context: HashMap::new(),
            event_hash: 1,
        }
    }

    fn make_policy(
        name: &str,
        event_type: &str,
        severity: Severity,
        enabled: bool,
    ) -> AutonomousPolicy {
        AutonomousPolicy {
            name: name.to_string(),
            version: "1.0".to_string(),
            trigger: TriggerCondition {
                event_type: event_type.to_string(),
                severity_threshold: severity,
                context_match: HashMap::new(),
            },
            action: super::super::types::AutonomousAction::AutoRemediation,
            boundary: super::super::types::ActionBoundary {
                min: 0.0,
                max: 100.0,
            },
            circuit_breaker: super::super::types::CircuitBreakerConfig::default(),
            enabled,
        }
    }

    #[test]
    fn test_match_exact_event_type() {
        let matcher = PolicyMatcher::new();
        let event = make_event("high_latency", Severity::Warning);
        let policies = vec![make_policy("p1", "high_latency", Severity::Warning, true)];
        assert!(matcher.match_policy(&event, &policies).is_some());
    }

    #[test]
    fn test_no_match_different_event_type() {
        let matcher = PolicyMatcher::new();
        let event = make_event("high_latency", Severity::Warning);
        let policies = vec![make_policy("p1", "high_cpu", Severity::Warning, true)];
        assert!(matcher.match_policy(&event, &policies).is_none());
    }

    #[test]
    fn test_no_match_lower_severity() {
        let matcher = PolicyMatcher::new();
        let event = make_event("high_latency", Severity::Info);
        let policies = vec![make_policy("p1", "high_latency", Severity::Warning, true)];
        assert!(matcher.match_policy(&event, &policies).is_none());
    }

    #[test]
    fn test_match_higher_severity() {
        let matcher = PolicyMatcher::new();
        let event = make_event("high_latency", Severity::Critical);
        let policies = vec![make_policy("p1", "high_latency", Severity::Warning, true)];
        assert!(matcher.match_policy(&event, &policies).is_some());
    }

    #[test]
    fn test_disabled_policy_not_matched() {
        let matcher = PolicyMatcher::new();
        let event = make_event("high_latency", Severity::Warning);
        let policies = vec![make_policy("p1", "high_latency", Severity::Warning, false)];
        assert!(matcher.match_policy(&event, &policies).is_none());
    }

    #[test]
    fn test_context_match() {
        let matcher = PolicyMatcher::new();
        let mut event = make_event("high_latency", Severity::Warning);
        event.context.insert("db".to_string(), "mysql".to_string());
        let mut policy = make_policy("p1", "high_latency", Severity::Warning, true);
        policy
            .trigger
            .context_match
            .insert("db".to_string(), "mysql".to_string());
        assert!(matcher.match_policy(&event, &[policy]).is_some());
    }

    #[test]
    fn test_context_mismatch() {
        let matcher = PolicyMatcher::new();
        let mut event = make_event("high_latency", Severity::Warning);
        event
            .context
            .insert("db".to_string(), "postgres".to_string());
        let mut policy = make_policy("p1", "high_latency", Severity::Warning, true);
        policy
            .trigger
            .context_match
            .insert("db".to_string(), "mysql".to_string());
        assert!(matcher.match_policy(&event, &[policy]).is_none());
    }
}
