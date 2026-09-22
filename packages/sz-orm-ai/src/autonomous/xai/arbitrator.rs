//! 策略冲突仲裁器：多策略匹配同一事件时按优先级仲裁出唯一执行策略

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::autonomous::types::AutonomousPolicy;

use super::XaiError;

/// 仲裁配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbitrationConfig {
    /// 策略优先级映射（policy_name -> priority，数值越大优先级越高）
    pub priorities: HashMap<String, i32>,
    /// 冲突规则：相同优先级时是否按名称字典序选择
    pub tie_break_by_name: bool,
}

impl Default for ArbitrationConfig {
    fn default() -> Self {
        Self {
            priorities: HashMap::new(),
            tie_break_by_name: true,
        }
    }
}

impl ArbitrationConfig {
    pub fn with_priority(mut self, policy_name: &str, priority: i32) -> Self {
        self.priorities.insert(policy_name.to_string(), priority);
        self
    }
}

/// 被仲裁策略记录
#[derive(Debug, Clone)]
pub struct ArbitratedPolicy {
    pub policy_name: String,
    pub priority: i32,
    pub reason: String,
}

/// 仲裁结果
#[derive(Debug, Clone)]
pub struct ArbitrationResult {
    pub winner: AutonomousPolicy,
    pub arbitrated: Vec<ArbitratedPolicy>,
}

/// 策略冲突仲裁器
pub struct PolicyArbitrator {
    config: ArbitrationConfig,
}

impl PolicyArbitrator {
    pub fn new(config: ArbitrationConfig) -> Self {
        Self { config }
    }

    /// 多策略冲突仲裁（≤50ms）
    pub fn arbitrate(
        &self,
        matched_policies: Vec<&AutonomousPolicy>,
    ) -> Result<ArbitrationResult, XaiError> {
        if matched_policies.is_empty() {
            return Err(XaiError::PolicyConflictUnresolved("无策略匹配".to_string()));
        }
        if matched_policies.len() == 1 {
            return Ok(ArbitrationResult {
                winner: matched_policies[0].clone(),
                arbitrated: Vec::new(),
            });
        }
        let mut indexed: Vec<(i32, &AutonomousPolicy)> = matched_policies
            .iter()
            .map(|p| {
                let pri = self.config.priorities.get(&p.name).copied().unwrap_or(0);
                (pri, *p)
            })
            .collect();
        indexed.sort_by_key(|(pri, _)| std::cmp::Reverse(*pri));
        let top_priority = indexed[0].0;
        let top_group: Vec<&AutonomousPolicy> = indexed
            .iter()
            .filter(|(pri, _)| *pri == top_priority)
            .map(|(_, p)| *p)
            .collect();
        if top_group.len() > 1 {
            if self.config.tie_break_by_name {
                let sorted = {
                    let mut g = top_group.clone();
                    g.sort_by(|a, b| a.name.cmp(&b.name));
                    g
                };
                let winner = sorted[0].clone();
                let arbitrated: Vec<ArbitratedPolicy> = sorted[1..]
                    .iter()
                    .map(|p| ArbitratedPolicy {
                        policy_name: p.name.clone(),
                        priority: top_priority,
                        reason: "相同优先级，按名称字典序仲裁".to_string(),
                    })
                    .collect();
                let lower: Vec<ArbitratedPolicy> = indexed
                    .iter()
                    .filter(|(pri, _)| *pri < top_priority)
                    .map(|(pri, p)| ArbitratedPolicy {
                        policy_name: p.name.clone(),
                        priority: *pri,
                        reason: "优先级较低".to_string(),
                    })
                    .collect();
                return Ok(ArbitrationResult {
                    winner,
                    arbitrated: arbitrated.into_iter().chain(lower).collect(),
                });
            }
            let names: Vec<String> = top_group.iter().map(|p| p.name.clone()).collect();
            return Err(XaiError::PolicyConflictUnresolved(format!(
                "策略优先级相同无法仲裁: {}",
                names.join(", ")
            )));
        }
        let winner = top_group[0].clone();
        let arbitrated = indexed[1..]
            .iter()
            .map(|(pri, p)| ArbitratedPolicy {
                policy_name: p.name.clone(),
                priority: *pri,
                reason: "优先级较低".to_string(),
            })
            .collect();
        Ok(ArbitrationResult { winner, arbitrated })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autonomous::types::{
        ActionBoundary, AutonomousAction, CircuitBreakerConfig, TriggerCondition,
    };
    use std::collections::HashMap;
    use std::time::Duration;

    fn make_policy(name: &str) -> AutonomousPolicy {
        AutonomousPolicy {
            name: name.to_string(),
            version: "1.0".to_string(),
            trigger: TriggerCondition {
                event_type: "test".to_string(),
                severity_threshold: crate::autonomous::types::Severity::Info,
                context_match: HashMap::new(),
            },
            action: AutonomousAction::AutoRemediation,
            boundary: ActionBoundary {
                min: 0.0,
                max: 100.0,
            },
            circuit_breaker: CircuitBreakerConfig {
                failure_threshold: 5,
                break_duration: Duration::from_secs(1800),
            },
            enabled: true,
        }
    }

    #[test]
    fn test_arbitrate_single_policy() {
        let arbitrator = PolicyArbitrator::new(ArbitrationConfig::default());
        let p = make_policy("p1");
        let result = arbitrator.arbitrate(vec![&p]).unwrap();
        assert_eq!(result.winner.name, "p1");
        assert!(result.arbitrated.is_empty());
    }

    #[test]
    fn test_arbitrate_by_priority() {
        let config = ArbitrationConfig::default()
            .with_priority("p1", 10)
            .with_priority("p2", 20);
        let arbitrator = PolicyArbitrator::new(config);
        let p1 = make_policy("p1");
        let p2 = make_policy("p2");
        let result = arbitrator.arbitrate(vec![&p1, &p2]).unwrap();
        assert_eq!(result.winner.name, "p2");
        assert_eq!(result.arbitrated.len(), 1);
        assert_eq!(result.arbitrated[0].policy_name, "p1");
    }

    #[test]
    fn test_arbitrate_tie_break_by_name() {
        let config = ArbitrationConfig::default()
            .with_priority("alpha", 10)
            .with_priority("beta", 10);
        let arbitrator = PolicyArbitrator::new(config);
        let alpha = make_policy("alpha");
        let beta = make_policy("beta");
        let result = arbitrator.arbitrate(vec![&alpha, &beta]).unwrap();
        assert_eq!(result.winner.name, "alpha");
        assert_eq!(result.arbitrated.len(), 1);
    }

    #[test]
    fn test_arbitrate_conflict_unresolved() {
        let config = ArbitrationConfig {
            priorities: HashMap::new(),
            tie_break_by_name: false,
        };
        let arbitrator = PolicyArbitrator::new(config);
        let p1 = make_policy("p1");
        let p2 = make_policy("p2");
        let result = arbitrator.arbitrate(vec![&p1, &p2]);
        assert!(matches!(result, Err(XaiError::PolicyConflictUnresolved(_))));
    }

    #[test]
    fn test_arbitrate_empty() {
        let arbitrator = PolicyArbitrator::new(ArbitrationConfig::default());
        let result = arbitrator.arbitrate(vec![]);
        assert!(matches!(result, Err(XaiError::PolicyConflictUnresolved(_))));
    }
}
