//! ABAC 端到端接线测试

use sz_orm_auth::abac::{
    AbacPolicy, AbacPolicyEngine, AccessRequest, AttributeScope, AttributeValue, Condition,
    DecisionCache, Effect, ExprSandbox, PolicyAuditLogger, PolicyHotReloader,
};

#[test]
fn wiring_policy_engine_allow() {
    let mut engine = AbacPolicyEngine::new();
    engine.add_policy(AbacPolicy::new(
        "admin_read",
        "read",
        Condition::Eq {
            scope: AttributeScope::Subject,
            key: "role".into(),
            value: AttributeValue::str_val("admin"),
        },
        Effect::Allow,
    ));
    let req =
        AccessRequest::new("read").with_subject_attr("role", AttributeValue::str_val("admin"));
    assert_eq!(engine.evaluate(&req), Effect::Allow);
}

#[test]
fn wiring_policy_engine_deny() {
    let mut engine = AbacPolicyEngine::new();
    engine.add_policy(AbacPolicy::new(
        "secret_deny",
        "read",
        Condition::Eq {
            scope: AttributeScope::Resource,
            key: "classification".into(),
            value: AttributeValue::str_val("secret"),
        },
        Effect::Deny,
    ));
    let req = AccessRequest::new("read")
        .with_resource_attr("classification", AttributeValue::str_val("secret"));
    assert_eq!(engine.evaluate(&req), Effect::Deny);
}

#[test]
fn wiring_expr_sandbox_validates() {
    assert!(ExprSandbox::validate_key("user_role").is_ok());
    assert!(ExprSandbox::validate_key("user-role").is_err());
    assert!(ExprSandbox::validate_string_value("${injection}").is_err());
}

#[test]
fn wiring_decision_cache() {
    let mut cache = DecisionCache::new(60);
    cache.put("user1", "read", "data", Effect::Allow);
    assert_eq!(cache.get("user1", "read", "data"), Some(Effect::Allow));
}

#[test]
fn wiring_audit_logger() {
    let logger = PolicyAuditLogger::new(100);
    logger.log("user1", "read", "data", Effect::Allow, "p1");
    logger.log("user2", "write", "data", Effect::Deny, "p2");
    assert_eq!(logger.len(), 2);
    let user1 = logger.filter_by_user("user1");
    assert_eq!(user1.len(), 1);
}

#[test]
fn wiring_hot_reloader() {
    let reloader = PolicyHotReloader::new(AbacPolicyEngine::new());
    let v1 = reloader.version();
    reloader.add_policy(AbacPolicy::new(
        "p1",
        "read",
        Condition::Eq {
            scope: AttributeScope::Subject,
            key: "role".into(),
            value: AttributeValue::str_val("admin"),
        },
        Effect::Allow,
    ));
    assert!(reloader.version() > v1);
}

#[test]
fn wiring_complex_policy() {
    let mut engine = AbacPolicyEngine::new();
    engine.add_policy(AbacPolicy::new(
        "dept_and_time",
        "read",
        Condition::And(vec![
            Condition::Eq {
                scope: AttributeScope::Subject,
                key: "department".into(),
                value: AttributeValue::str_val("engineering"),
            },
            Condition::Gt {
                scope: AttributeScope::Environment,
                key: "hour".into(),
                value: 9,
            },
            Condition::Lt {
                scope: AttributeScope::Environment,
                key: "hour".into(),
                value: 18,
            },
        ]),
        Effect::Allow,
    ));
    let req = AccessRequest::new("read")
        .with_subject_attr("department", AttributeValue::str_val("engineering"))
        .with_env_attr("hour", AttributeValue::Int(14));
    assert_eq!(engine.evaluate(&req), Effect::Allow);
    let req_off_hours = AccessRequest::new("read")
        .with_subject_attr("department", AttributeValue::str_val("engineering"))
        .with_env_attr("hour", AttributeValue::Int(22));
    assert_eq!(engine.evaluate(&req_off_hours), Effect::Deny);
}
