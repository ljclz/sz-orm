//! RBAC + ABAC 组合授权器
//!
//! 将 RBAC 和 ABAC 授权器组合使用，支持 AND/OR 模式。

use crate::auth::User;
use crate::authorizer::{Authorizer, RbacAuthorizer};
use crate::error::AuthError;

use super::policy_engine::{AbacPolicyEngine, AccessRequest, Effect};

/// 组合模式
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CombineMode {
    /// RBAC 和 ABAC 都必须允许
    All,
    /// RBAC 或 ABAC 任一允许即可
    Any,
}

/// RBAC + ABAC 组合授权器
pub struct CompositeAuthorizer {
    rbac: RbacAuthorizer,
    abac: AbacPolicyEngine,
    mode: CombineMode,
}

impl CompositeAuthorizer {
    /// 创建组合授权器
    pub fn new(rbac: RbacAuthorizer, abac: AbacPolicyEngine, mode: CombineMode) -> Self {
        Self { rbac, abac, mode }
    }

    /// 评估授权
    pub fn authorize(
        &self,
        user: &User,
        action: &str,
        resource: &str,
        request: &AccessRequest,
    ) -> Result<bool, AuthError> {
        let rbac_result = self.rbac.can(user, action, resource)?;
        let abac_result = self.abac.evaluate(request) == Effect::Allow;

        let allowed = match self.mode {
            CombineMode::All => rbac_result && abac_result,
            CombineMode::Any => rbac_result || abac_result,
        };
        Ok(allowed)
    }

    /// 组合模式
    pub fn mode(&self) -> &CombineMode {
        &self.mode
    }
}

impl Authorizer for CompositeAuthorizer {
    fn can(&self, user: &User, action: &str, resource: &str) -> Result<bool, AuthError> {
        self.rbac.can(user, action, resource)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::User;

    fn make_user(role: &str) -> User {
        User::new(1, "test_user").with_roles(vec![role.to_string()])
    }

    fn make_request(action: &str) -> AccessRequest {
        AccessRequest::new(action).with_subject_attr("role", AttributeValue::str_val("admin"))
    }

    use super::super::policy_engine::AttributeValue;

    #[test]
    fn test_all_mode_both_allow() {
        let rbac = RbacAuthorizer::new().with_role_permission("admin", "read");
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
        let auth = CompositeAuthorizer::new(rbac, abac, CombineMode::All);
        let user = make_user("admin");
        let req = make_request("read");
        assert!(auth.authorize(&user, "read", "data", &req).unwrap());
    }

    #[test]
    fn test_all_mode_rbac_denies() {
        let rbac = RbacAuthorizer::new();
        let abac = AbacPolicyEngine::new();
        let auth = CompositeAuthorizer::new(rbac, abac, CombineMode::All);
        let user = make_user("guest");
        let req = make_request("read");
        assert!(!auth.authorize(&user, "read", "data", &req).unwrap());
    }

    #[test]
    fn test_any_mode_rbac_allows() {
        let rbac = RbacAuthorizer::new().with_role_permission("admin", "read");
        let abac = AbacPolicyEngine::new();
        let auth = CompositeAuthorizer::new(rbac, abac, CombineMode::Any);
        let user = make_user("admin");
        let req = AccessRequest::new("read");
        assert!(auth.authorize(&user, "read", "data", &req).unwrap());
    }

    #[test]
    fn test_any_mode_abac_allows() {
        let rbac = RbacAuthorizer::new();
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
        let auth = CompositeAuthorizer::new(rbac, abac, CombineMode::Any);
        let user = make_user("admin");
        let req = make_request("read");
        assert!(auth.authorize(&user, "read", "data", &req).unwrap());
    }

    #[test]
    fn test_any_mode_both_deny() {
        let rbac = RbacAuthorizer::new();
        let abac = AbacPolicyEngine::new();
        let auth = CompositeAuthorizer::new(rbac, abac, CombineMode::Any);
        let user = make_user("guest");
        let req = AccessRequest::new("read");
        assert!(!auth.authorize(&user, "read", "data", &req).unwrap());
    }

    #[test]
    fn test_all_mode_abac_denies() {
        let rbac = RbacAuthorizer::new().with_role_permission("admin", "read");
        let abac = AbacPolicyEngine::new();
        let auth = CompositeAuthorizer::new(rbac, abac, CombineMode::All);
        let user = make_user("admin");
        let req = AccessRequest::new("read");
        assert!(!auth.authorize(&user, "read", "data", &req).unwrap());
    }

    #[test]
    fn test_mode_access() {
        let rbac = RbacAuthorizer::new();
        let abac = AbacPolicyEngine::new();
        let auth = CompositeAuthorizer::new(rbac, abac, CombineMode::All);
        assert_eq!(auth.mode(), &CombineMode::All);
    }

    use super::super::policy_engine::{AbacPolicy, AttributeScope, Condition, Effect};
}
