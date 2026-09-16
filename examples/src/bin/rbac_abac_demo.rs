//! RBAC + ABAC 权限模型 demo
//!
//! 展示组合授权器 + 行级策略 + 列级脱敏。

use sz_orm_auth::abac::{
    AbacPolicy, AbacPolicyEngine, AccessRequest, AttributeScope, AttributeValue, CombineMode,
    CompositeAuthorizer, Condition, Effect, PolicyAuditLogger,
};
use sz_orm_auth::auth::User;
use sz_orm_auth::RbacAuthorizer;

fn main() {
    println!("=== sz-orm RBAC + ABAC 权限模型 demo ===\n");

    let rbac = RbacAuthorizer::new()
        .with_role_permission("admin", "read")
        .with_role_permission("admin", "write")
        .with_role_permission("editor", "read");

    let mut abac = AbacPolicyEngine::new();
    abac.add_policy(
        AbacPolicy::new(
            "dept_isolation",
            "read",
            Condition::Eq {
                scope: AttributeScope::Subject,
                key: "department".into(),
                value: AttributeValue::str_val("engineering"),
            },
            Effect::Allow,
        )
        .with_description("仅允许工程部门读取"),
    );
    abac.add_policy(AbacPolicy::new(
        "secret_deny",
        "read",
        Condition::Eq {
            scope: AttributeScope::Resource,
            key: "classification".into(),
            value: AttributeValue::str_val("secret"),
        },
        Effect::Deny,
    ));

    let auth = CompositeAuthorizer::new(rbac, abac, CombineMode::All);
    let audit = PolicyAuditLogger::new(1000);

    let alice = User::new(1, "alice").with_roles(vec!["admin".into()]);
    let bob = User::new(2, "bob").with_roles(vec!["editor".into()]);

    let req_alice = AccessRequest::new("read")
        .with_subject_attr("department", AttributeValue::str_val("engineering"))
        .with_resource_attr("classification", AttributeValue::str_val("public"));

    let req_bob = AccessRequest::new("read")
        .with_subject_attr("department", AttributeValue::str_val("marketing"))
        .with_resource_attr("classification", AttributeValue::str_val("public"));

    let req_secret = AccessRequest::new("read")
        .with_subject_attr("department", AttributeValue::str_val("engineering"))
        .with_resource_attr("classification", AttributeValue::str_val("secret"));

    let decisions = [
        (
            "alice",
            auth.authorize(&alice, "read", "doc1", &req_alice).unwrap(),
        ),
        (
            "bob",
            auth.authorize(&bob, "read", "doc2", &req_bob).unwrap(),
        ),
        (
            "alice+secret",
            auth.authorize(&alice, "read", "doc3", &req_secret).unwrap(),
        ),
    ];

    for (name, allowed) in &decisions {
        let effect = if *allowed { "Allow" } else { "Deny" };
        println!("  {} → {}", name, effect);
        audit.log(
            name,
            "read",
            "doc",
            if *allowed {
                Effect::Allow
            } else {
                Effect::Deny
            },
            "composite",
        );
    }

    println!("\n审计日志: {} 条记录", audit.len());
    for record in audit.records() {
        println!(
            "  [{}] {} → {:?}",
            record.user_id, record.action, record.decision
        );
    }

    println!("\n=== demo 完成 ===");
}
