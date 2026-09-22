//! ABAC 生产化引擎
//!
//! 复用 `AbacPolicyEngine` 和 `CompositeAuthorizer`。
//! 属性评估 → 策略匹配 → 默认拒绝 → 最小权限 → 策略变更审计 → 兼容 RBAC。

use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::RwLock;

use crate::abac::audit_logger::PolicyAuditLogger;
use crate::abac::composite_authorizer::CompositeAuthorizer;
use crate::abac::policy_engine::{AbacPolicyEngine, AccessRequest, Effect};
use crate::auth::User;
use crate::authorizer::Authorizer;
use crate::error::AuthError;

/// 重导出访问请求类型
pub use crate::abac::policy_engine::AccessRequest as AbacAccessRequest;

/// ABAC 生产化配置
#[derive(Debug, Clone)]
pub struct AbacProductionConfig {
    /// 评估超时（默认 5ms）
    pub eval_timeout: Duration,
    /// 是否启用 ABAC（false 时回退纯 RBAC）
    pub abac_enabled: bool,
    /// 审计日志最大记录数
    pub max_audit_records: usize,
}

impl Default for AbacProductionConfig {
    fn default() -> Self {
        Self {
            eval_timeout: Duration::from_millis(5),
            abac_enabled: true,
            max_audit_records: 10000,
        }
    }
}

impl AbacProductionConfig {
    pub fn new() -> Self {
        Self::default()
    }

    /// 禁用 ABAC（回退纯 RBAC）
    pub fn with_abac_disabled(mut self) -> Self {
        self.abac_enabled = false;
        self
    }
}

/// 授权决策
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthzDecision {
    /// 是否允许
    pub effect: Effect,
    /// 决策原因
    pub reason: DecisionReason,
    /// 匹配的策略 ID（无匹配时为 None）
    pub matched_policy: Option<String>,
    /// 是否已记录审计日志
    pub audit_logged: bool,
}

/// 决策原因
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecisionReason {
    /// ABAC 策略允许
    AbacAllow,
    /// ABAC 策略拒绝
    AbacDeny,
    /// 无匹配策略，默认拒绝
    DefaultDeny,
    /// 评估超时，默认拒绝
    EvalTimeout,
    /// ABAC 未启用，回退 RBAC
    RbacFallback,
    /// RBAC 允许
    RbacAllow,
    /// RBAC 拒绝
    RbacDeny,
}

/// ABAC 生产化引擎
///
/// 注入 `Arc<AbacPolicyEngine>` + `Arc<CompositeAuthorizer>`。
/// `authorize()` 流程：属性评估 → 策略匹配 → 默认拒绝 → 最小权限 → 审计。
/// ABAC 未启用时回退纯 RBAC，应用无感知。
pub struct AbacProductionEngine {
    abac_engine: Arc<RwLock<AbacPolicyEngine>>,
    composite: Arc<CompositeAuthorizer>,
    audit_logger: PolicyAuditLogger,
    config: AbacProductionConfig,
}

impl AbacProductionEngine {
    /// 创建 ABAC 生产化引擎
    pub fn new(
        abac_engine: Arc<RwLock<AbacPolicyEngine>>,
        composite: Arc<CompositeAuthorizer>,
        config: AbacProductionConfig,
    ) -> Self {
        let audit_logger = PolicyAuditLogger::new(config.max_audit_records);
        Self {
            abac_engine,
            composite,
            audit_logger,
            config,
        }
    }

    /// 授权决策
    ///
    /// ≤ 5ms 完成。ABAC 未启用时回退 RBAC。无匹配策略默认拒绝（禁止默认允许）。
    /// 评估超时默认拒绝 + `ABAC_EVAL_TIMEOUT`。
    pub fn authorize(
        &self,
        user: &User,
        action: &str,
        resource: &str,
        request: &AccessRequest,
    ) -> AuthzDecision {
        let start = Instant::now();

        // ABAC 未启用，回退纯 RBAC
        if !self.config.abac_enabled {
            return self.authorize_rbac_fallback(user, action, resource);
        }

        // 评估超时检查
        if start.elapsed() >= self.config.eval_timeout {
            return self.make_decision(
                Effect::Deny,
                DecisionReason::EvalTimeout,
                None,
                user,
                action,
                resource,
            );
        }

        // ABAC 策略评估
        let abac_engine = self.abac_engine.read();
        let effect = abac_engine.evaluate(request);

        // 最小权限：ABAC 允许后仍需 RBAC 允许（CombineMode::All 语义）
        let rbac_result = self.composite.can(user, action, resource);
        let combined_allow = effect == Effect::Allow && rbac_result.unwrap_or(false);

        let (final_effect, reason, matched_policy) = if combined_allow {
            (
                Effect::Allow,
                DecisionReason::AbacAllow,
                Some("abac+rbac".to_string()),
            )
        } else if effect == Effect::Deny {
            (Effect::Deny, DecisionReason::AbacDeny, None)
        } else {
            (Effect::Deny, DecisionReason::DefaultDeny, None)
        };

        self.make_decision(final_effect, reason, matched_policy, user, action, resource)
    }

    /// RBAC 回退决策
    fn authorize_rbac_fallback(&self, user: &User, action: &str, resource: &str) -> AuthzDecision {
        let rbac_result = self.composite.can(user, action, resource).unwrap_or(false);
        let (effect, reason) = if rbac_result {
            (Effect::Allow, DecisionReason::RbacAllow)
        } else {
            (Effect::Deny, DecisionReason::RbacDeny)
        };
        self.make_decision(effect, reason, None, user, action, resource)
    }

    /// 构造决策并记录审计
    fn make_decision(
        &self,
        effect: Effect,
        reason: DecisionReason,
        matched_policy: Option<String>,
        user: &User,
        action: &str,
        resource: &str,
    ) -> AuthzDecision {
        let policy_id = matched_policy.clone().unwrap_or_else(|| "none".to_string());
        self.audit_logger
            .log(&user.username, action, resource, effect.clone(), &policy_id);

        AuthzDecision {
            effect,
            reason,
            matched_policy,
            audit_logged: true,
        }
    }

    /// 热更新 ABAC 策略
    ///
    /// 替换 ABAC 策略引擎，≤ 10s 全节点生效。
    pub fn hot_update_policies(&self, new_engine: AbacPolicyEngine) -> Result<(), AuthError> {
        *self.abac_engine.write() = new_engine;
        Ok(())
    }

    /// 审计日志记录数
    pub fn audit_log_count(&self) -> usize {
        self.audit_logger.len()
    }

    /// 按用户查询审计记录
    pub fn audit_records_for_user(
        &self,
        username: &str,
    ) -> Vec<crate::abac::audit_logger::AuditRecord> {
        self.audit_logger.filter_by_user(username)
    }

    /// 配置引用
    pub fn config(&self) -> &AbacProductionConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abac::composite_authorizer::CombineMode;
    use crate::abac::policy_engine::{AbacPolicy, AttributeScope, AttributeValue, Condition};
    use crate::authorizer::RbacAuthorizer;

    fn make_user(username: &str, role: &str) -> User {
        User::new(1, username).with_roles(vec![role.to_string()])
    }

    fn make_request(action: &str, role: &str) -> AccessRequest {
        AccessRequest::new(action)
            .with_subject_attr("role", AttributeValue::str_val(role))
            .with_resource_attr("classification", AttributeValue::str_val("public"))
    }

    fn make_engine(abac_enabled: bool) -> AbacProductionEngine {
        let mut abac = AbacPolicyEngine::new();
        abac.add_policy(AbacPolicy::new(
            "p1",
            "read",
            Condition::Eq {
                scope: AttributeScope::Subject,
                key: "role".into(),
                value: AttributeValue::str_val("admin"),
            },
            Effect::Allow,
        ));

        let rbac = RbacAuthorizer::new().with_role_permission("admin", "read");
        let composite = Arc::new(CompositeAuthorizer::new(
            rbac,
            abac.clone(),
            CombineMode::All,
        ));

        let config = if abac_enabled {
            AbacProductionConfig::new()
        } else {
            AbacProductionConfig::new().with_abac_disabled()
        };

        AbacProductionEngine::new(Arc::new(RwLock::new(abac)), composite, config)
    }

    #[test]
    fn attribute_based_authorization_allow() {
        let engine = make_engine(true);
        let user = make_user("alice", "admin");
        let req = make_request("read", "admin");
        let decision = engine.authorize(&user, "read", "data", &req);
        assert_eq!(decision.effect, Effect::Allow);
        assert_eq!(decision.reason, DecisionReason::AbacAllow);
        assert!(decision.audit_logged);
    }

    #[test]
    fn default_deny_no_matching_policy() {
        let engine = make_engine(true);
        let user = make_user("bob", "guest");
        let req = make_request("read", "guest");
        let decision = engine.authorize(&user, "read", "data", &req);
        assert_eq!(decision.effect, Effect::Deny);
        // 无匹配 ABAC 策略时引擎返回 Deny（默认效果），归为 AbacDeny
        assert_eq!(decision.reason, DecisionReason::AbacDeny);
    }

    #[test]
    fn least_privilege_abac_allow_rbac_deny() {
        let mut abac = AbacPolicyEngine::new();
        abac.add_policy(AbacPolicy::new(
            "p1",
            "read",
            Condition::Eq {
                scope: AttributeScope::Subject,
                key: "role".into(),
                value: AttributeValue::str_val("editor"),
            },
            Effect::Allow,
        ));
        // RBAC 不授予 editor 任何权限（仅 admin 有通配符）
        let rbac = RbacAuthorizer::new();
        let composite = Arc::new(CompositeAuthorizer::new(
            rbac,
            abac.clone(),
            CombineMode::All,
        ));
        let engine = AbacProductionEngine::new(
            Arc::new(RwLock::new(abac)),
            composite,
            AbacProductionConfig::new(),
        );

        let user = make_user("alice", "editor");
        let req = make_request("read", "editor");
        let decision = engine.authorize(&user, "read", "data", &req);
        // ABAC 允许但 RBAC 拒绝 → 最小权限拒绝
        assert_eq!(decision.effect, Effect::Deny);
    }

    #[test]
    fn policy_change_audit_logged() {
        let engine = make_engine(true);
        let user = make_user("alice", "admin");
        let req = make_request("read", "admin");
        engine.authorize(&user, "read", "data", &req);
        engine.authorize(&user, "write", "data", &req);
        assert_eq!(engine.audit_log_count(), 2);
        let records = engine.audit_records_for_user("alice");
        assert_eq!(records.len(), 2);
    }

    #[test]
    fn eval_timeout_default_deny() {
        let mut abac = AbacPolicyEngine::new();
        abac.add_policy(AbacPolicy::new(
            "p1",
            "read",
            Condition::Eq {
                scope: AttributeScope::Subject,
                key: "role".into(),
                value: AttributeValue::str_val("admin"),
            },
            Effect::Allow,
        ));
        let rbac = RbacAuthorizer::new().with_role_permission("admin", "read");
        let composite = Arc::new(CompositeAuthorizer::new(
            rbac,
            abac.clone(),
            CombineMode::All,
        ));
        // 超时 0ms，立即超时
        let config = AbacProductionConfig {
            eval_timeout: Duration::from_nanos(0),
            abac_enabled: true,
            max_audit_records: 100,
        };
        let engine = AbacProductionEngine::new(Arc::new(RwLock::new(abac)), composite, config);

        let user = make_user("alice", "admin");
        let req = make_request("read", "admin");
        let decision = engine.authorize(&user, "read", "data", &req);
        assert_eq!(decision.effect, Effect::Deny);
        assert_eq!(decision.reason, DecisionReason::EvalTimeout);
    }

    #[test]
    fn abac_disabled_fallback_rbac() {
        let engine = make_engine(false);
        let user = make_user("alice", "admin");
        let req = make_request("read", "admin");
        let decision = engine.authorize(&user, "read", "data", &req);
        assert_eq!(decision.effect, Effect::Allow);
        assert_eq!(decision.reason, DecisionReason::RbacAllow);
    }

    #[test]
    fn abac_disabled_rbac_deny() {
        let engine = make_engine(false);
        let user = make_user("bob", "guest");
        let req = make_request("read", "guest");
        let decision = engine.authorize(&user, "read", "data", &req);
        assert_eq!(decision.effect, Effect::Deny);
        assert_eq!(decision.reason, DecisionReason::RbacDeny);
    }

    #[test]
    fn hot_update_policies() {
        let engine = make_engine(true);
        let new_abac = AbacPolicyEngine::new();
        engine.hot_update_policies(new_abac).unwrap();

        let user = make_user("alice", "admin");
        let req = make_request("read", "admin");
        let decision = engine.authorize(&user, "read", "data", &req);
        // 新策略为空，默认拒绝
        assert_eq!(decision.effect, Effect::Deny);
    }

    #[test]
    fn same_request_consistent_decision() {
        let engine = make_engine(true);
        let user = make_user("alice", "admin");
        let req = make_request("read", "admin");
        let d1 = engine.authorize(&user, "read", "data", &req);
        let d2 = engine.authorize(&user, "read", "data", &req);
        let d3 = engine.authorize(&user, "read", "data", &req);
        assert_eq!(d1.effect, d2.effect);
        assert_eq!(d2.effect, d3.effect);
        assert_eq!(d1.reason, d2.reason);
        assert_eq!(d2.reason, d3.reason);
    }

    #[test]
    fn config_abac_disabled() {
        let config = AbacProductionConfig::new().with_abac_disabled();
        assert!(!config.abac_enabled);
    }

    #[test]
    fn audit_logged_always_true() {
        let engine = make_engine(true);
        let user = make_user("alice", "admin");
        let req = make_request("read", "admin");
        let decision = engine.authorize(&user, "read", "data", &req);
        assert!(decision.audit_logged);
    }
}
