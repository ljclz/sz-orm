//! RBAC + ABAC 组合授权接线测试

use sz_orm_auth::abac::{
    AbacPolicy, AbacPolicyEngine, AccessRequest, AttributeScope, AttributeValue, CombineMode,
    CompositeAuthorizer, Condition, Effect,
};
use sz_orm_auth::auth::User;
use sz_orm_auth::{Authorizer, RbacAuthorizer};

fn make_admin_user() -> User {
    User::new(1, "alice").with_roles(vec!["admin".into()])
}

fn make_guest_user() -> User {
    User::new(2, "bob").with_roles(vec!["guest".into()])
}

fn make_request(action: &str, role: &str) -> AccessRequest {
    AccessRequest::new(action).with_subject_attr("role", AttributeValue::str_val(role))
}

#[test]
fn wiring_composite_all_mode_both_allow() {
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
    let user = make_admin_user();
    let req = make_request("read", "admin");
    assert!(auth.authorize(&user, "read", "data", &req).unwrap());
}

#[test]
fn wiring_composite_all_mode_rbac_denies() {
    let rbac = RbacAuthorizer::new();
    let abac = AbacPolicyEngine::new();
    let auth = CompositeAuthorizer::new(rbac, abac, CombineMode::All);
    let user = make_guest_user();
    let req = make_request("read", "guest");
    assert!(!auth.authorize(&user, "read", "data", &req).unwrap());
}

#[test]
fn wiring_composite_any_mode_abac_allows() {
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
    let user = make_admin_user();
    let req = make_request("read", "admin");
    assert!(auth.authorize(&user, "read", "data", &req).unwrap());
}

#[test]
fn wiring_composite_authorizer_trait() {
    let rbac = RbacAuthorizer::new().with_role_permission("admin", "read");
    let abac = AbacPolicyEngine::new();
    let auth = CompositeAuthorizer::new(rbac, abac, CombineMode::All);
    let user = make_admin_user();
    assert!(auth.can(&user, "read", "data").unwrap());
}

#[test]
fn wiring_composite_any_mode_both_deny() {
    let rbac = RbacAuthorizer::new();
    let abac = AbacPolicyEngine::new();
    let auth = CompositeAuthorizer::new(rbac, abac, CombineMode::Any);
    let user = make_guest_user();
    let req = make_request("read", "guest");
    assert!(!auth.authorize(&user, "read", "data", &req).unwrap());
}
