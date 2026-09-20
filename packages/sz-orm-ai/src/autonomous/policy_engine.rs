//! 自治策略引擎

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use parking_lot::RwLock;

use super::boundary_validator::BoundaryValidator;
use super::circuit_breaker::AutonomousCircuitBreaker;
use super::idempotency::IdempotencyDeduplicator;
use super::policy_matcher::PolicyMatcher;
use super::types::*;

/// 自治策略引擎
pub struct AutonomousPolicyEngine {
    policies: Arc<RwLock<Vec<AutonomousPolicy>>>,
    matcher: PolicyMatcher,
    boundary_validator: BoundaryValidator,
    circuit_breakers: Arc<RwLock<HashMap<String, AutonomousCircuitBreaker>>>,
    idempotency: Arc<RwLock<IdempotencyDeduplicator>>,
    audit_available: Arc<RwLock<bool>>,
    in_progress: Arc<RwLock<Option<AutonomousDecision>>>,
}

impl AutonomousPolicyEngine {
    pub fn new() -> Self {
        Self {
            policies: Arc::new(RwLock::new(Vec::new())),
            matcher: PolicyMatcher::new(),
            boundary_validator: BoundaryValidator::new(),
            circuit_breakers: Arc::new(RwLock::new(HashMap::new())),
            idempotency: Arc::new(RwLock::new(IdempotencyDeduplicator::default())),
            audit_available: Arc::new(RwLock::new(true)),
            in_progress: Arc::new(RwLock::new(None)),
        }
    }

    /// 从配置加载策略
    pub fn load_from_config(&self, policies: Vec<AutonomousPolicy>) {
        let mut guard = self.policies.write();
        *guard = policies;
    }

    /// 热更新策略（5s 内生效）
    pub fn hot_reload(&self, policies: Vec<AutonomousPolicy>) {
        let mut guard = self.policies.write();
        *guard = policies;
    }

    /// 获取当前策略列表
    pub fn policies(&self) -> Vec<AutonomousPolicy> {
        self.policies.read().clone()
    }

    /// 设置审计可用性
    pub fn set_audit_available(&self, available: bool) {
        *self.audit_available.write() = available;
    }

    /// 手动熔断指定策略
    pub fn manual_circuit_break(&self, policy_name: &str) {
        let mut guard = self.circuit_breakers.write();
        guard
            .entry(policy_name.to_string())
            .or_default()
            .manual_break();
    }

    /// 中止正在执行的动作
    pub fn abort_in_progress(&self) -> Option<AutonomousDecision> {
        self.in_progress.write().take()
    }

    /// 处理异常事件（dry_run 模式仅产出决策不执行）
    pub async fn handle_event(
        &self,
        event: &AnomalyEvent,
        dry_run: bool,
    ) -> Result<AutonomousDecision, AutonomousError> {
        if !*self.audit_available.read() {
            return Err(AutonomousError::AuditUnavailable);
        }
        {
            let mut dedup = self.idempotency.write();
            if dedup.is_duplicate(event.event_hash) {
                return Ok(AutonomousDecision {
                    policy_name: "deduplicated".to_string(),
                    action: AutonomousAction::AutoRemediation,
                    reasoning: "事件重复，已去重".to_string(),
                    dry_run: true,
                    timestamp: Instant::now(),
                });
            }
        }
        let policies = self.policies.read();
        let policy = self
            .matcher
            .match_policy(event, &policies)
            .ok_or_else(|| AutonomousError::PolicyNotFound(event.event_type.clone()))?;
        {
            let mut breakers = self.circuit_breakers.write();
            let breaker = breakers
                .entry(policy.name.clone())
                .or_insert_with(|| AutonomousCircuitBreaker::new(policy.circuit_breaker.clone()));
            if !breaker.allow_request() {
                return Err(AutonomousError::CircuitBroken(policy.name.clone()));
            }
        }
        let decision = AutonomousDecision {
            policy_name: policy.name.clone(),
            action: policy.action.clone(),
            reasoning: format!("策略 {} 匹配事件 {}", policy.name, event.event_type),
            dry_run,
            timestamp: Instant::now(),
        };
        self.boundary_validator
            .validate(&policy.action, 1.0, &policy.boundary)?;
        if !dry_run {
            *self.in_progress.write() = Some(decision.clone());
        }
        Ok(decision)
    }

    /// 记录执行结果（更新熔断器）
    pub fn record_outcome(&self, policy_name: &str, success: bool) {
        let mut breakers = self.circuit_breakers.write();
        let breaker = breakers.entry(policy_name.to_string()).or_default();
        if success {
            breaker.record_success();
        } else {
            breaker.record_failure();
        }
        *self.in_progress.write() = None;
    }
}

impl Default for AutonomousPolicyEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for AutonomousPolicyEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AutonomousPolicyEngine")
            .field("policies", &self.policies.read().len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    fn make_event(event_type: &str, hash: u64) -> AnomalyEvent {
        AnomalyEvent {
            event_type: event_type.to_string(),
            timestamp: SystemTime::now(),
            severity: Severity::Warning,
            context: HashMap::new(),
            event_hash: hash,
        }
    }

    fn make_policy(name: &str, event_type: &str) -> AutonomousPolicy {
        AutonomousPolicy {
            name: name.to_string(),
            version: "1.0".to_string(),
            trigger: TriggerCondition {
                event_type: event_type.to_string(),
                severity_threshold: Severity::Info,
                context_match: HashMap::new(),
            },
            action: AutonomousAction::AutoRemediation,
            boundary: ActionBoundary {
                min: 0.0,
                max: 100.0,
            },
            circuit_breaker: CircuitBreakerConfig::default(),
            enabled: true,
        }
    }

    #[tokio::test]
    async fn test_handle_event_matching_policy() {
        let engine = AutonomousPolicyEngine::new();
        engine.load_from_config(vec![make_policy("p1", "high_latency")]);
        let event = make_event("high_latency", 1);
        let decision = engine.handle_event(&event, true).await.unwrap();
        assert_eq!(decision.policy_name, "p1");
        assert!(decision.dry_run);
    }

    #[tokio::test]
    async fn test_handle_event_no_matching_policy() {
        let engine = AutonomousPolicyEngine::new();
        engine.load_from_config(vec![make_policy("p1", "high_cpu")]);
        let event = make_event("high_latency", 1);
        let result = engine.handle_event(&event, true).await;
        assert!(matches!(result, Err(AutonomousError::PolicyNotFound(_))));
    }

    #[tokio::test]
    async fn test_handle_event_audit_unavailable() {
        let engine = AutonomousPolicyEngine::new();
        engine.set_audit_available(false);
        let event = make_event("high_latency", 1);
        let result = engine.handle_event(&event, true).await;
        assert!(matches!(result, Err(AutonomousError::AuditUnavailable)));
    }

    #[tokio::test]
    async fn test_handle_event_deduplicated() {
        let engine = AutonomousPolicyEngine::new();
        engine.load_from_config(vec![make_policy("p1", "high_latency")]);
        let event = make_event("high_latency", 42);
        engine.handle_event(&event, true).await.unwrap();
        let decision = engine.handle_event(&event, true).await.unwrap();
        assert_eq!(decision.policy_name, "deduplicated");
    }

    #[tokio::test]
    async fn test_hot_reload() {
        let engine = AutonomousPolicyEngine::new();
        engine.load_from_config(vec![make_policy("p1", "high_latency")]);
        engine.hot_reload(vec![make_policy("p2", "high_cpu")]);
        assert_eq!(engine.policies().len(), 1);
        assert_eq!(engine.policies()[0].name, "p2");
    }

    #[tokio::test]
    async fn test_manual_circuit_break() {
        let engine = AutonomousPolicyEngine::new();
        engine.load_from_config(vec![make_policy("p1", "high_latency")]);
        engine.manual_circuit_break("p1");
        let event = make_event("high_latency", 1);
        let result = engine.handle_event(&event, true).await;
        assert!(matches!(result, Err(AutonomousError::CircuitBroken(_))));
    }

    #[tokio::test]
    async fn test_record_outcome_success_resets_breaker() {
        let engine = AutonomousPolicyEngine::new();
        engine.load_from_config(vec![make_policy("p1", "high_latency")]);
        engine.record_outcome("p1", false);
        engine.record_outcome("p1", false);
        engine.record_outcome("p1", true);
        let event = make_event("high_latency", 1);
        let result = engine.handle_event(&event, true).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_abort_in_progress() {
        let engine = AutonomousPolicyEngine::new();
        engine.load_from_config(vec![make_policy("p1", "high_latency")]);
        let event = make_event("high_latency", 1);
        engine.handle_event(&event, false).await.unwrap();
        assert!(engine.abort_in_progress().is_some());
        assert!(engine.abort_in_progress().is_none());
    }
}
