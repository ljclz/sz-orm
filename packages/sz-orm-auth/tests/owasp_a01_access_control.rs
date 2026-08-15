#![cfg(feature = "owasp-pentest-suite")]

//! OWASP A01: 失效的访问控制深化渗透测试
//!
//! 对应 REQ-V49-001（OWASP A01 深化）
//!
//! 渗透测试向量：
//! - 垂直越权：普通用户尝试管理员操作
//! - 强制浏览：未授权用户直接访问受保护资源
//! - JWT claims 篡改：篡改 roles/iss/sub 但保留原签名
//! - RBAC 通配符边界：`*` / `read:*` 权限边界

use sz_orm_auth::jwt::{JwtClaims, JwtEncoder};
use sz_orm_auth::{Authorizer, RbacAuthorizer, User};

/// A01-1：垂直越权被拒绝
///
/// 普通用户尝试管理员操作（delete/admin），断言被拒绝。
/// 管理员角色（`*` 通配符）可以执行任意操作。
#[test]
fn a01_vertical_privilege_escalation_rejected() {
    let mut az = RbacAuthorizer::new();
    az.grant("user", "read:own_profile");
    az.grant("user", "write:own_profile");

    let alice = User::new(1, "alice").with_roles(vec!["user".to_string()]);
    let admin = User::new(2, "admin").with_roles(vec!["admin".to_string()]);

    assert!(!az.can(&alice, "delete", "any_resource").unwrap());
    assert!(!az.can(&alice, "admin", "panel").unwrap());
    assert!(!az.can(&alice, "delete", "users").unwrap());

    assert!(az.can(&admin, "delete", "any_resource").unwrap());
    assert!(az.can(&admin, "admin", "panel").unwrap());
}

/// A01-2：强制浏览被拒绝
///
/// 未授权用户直接访问受保护资源（如 migrations 表），断言被拒绝。
#[test]
fn a01_forced_browsing_rejected() {
    let mut az = RbacAuthorizer::new();
    az.grant("user", "read:dashboard");

    let attacker = User::new(1, "attacker").with_roles(vec!["user".to_string()]);

    assert!(!az.can(&attacker, "read", "__sz_orm_migrations").unwrap());
    assert!(!az.can(&attacker, "read", "admin_panel").unwrap());
    assert!(!az.can(&attacker, "write", "__sz_orm_migrations").unwrap());
    assert!(!az.can(&attacker, "delete", "__sz_orm_migrations").unwrap());

    assert!(az.can(&attacker, "read", "dashboard").unwrap());
}

/// A01-3：JWT claims 篡改被拒绝
///
/// 签发合法 JWT，篡改 claims（roles/iss/sub）但保留原签名，断言 decode 拒绝。
/// 深化向量：iss 篡改、sub 篡改、roles 篡改。
#[test]
fn a01_jwt_claims_tampering_rejected() {
    let encoder = JwtEncoder::new("super-secret-key-for-owasp-a01-testing");

    let claims = JwtClaims::new("user-1", 9999999999)
        .with_roles(vec!["user".to_string()])
        .with_issuer("https://auth.example.com")
        .with_user_id(1);
    let token = encoder.encode(&claims).unwrap();

    let parts: Vec<&str> = token.split('.').collect();
    let header_b64 = parts[0];
    let original_claims_b64 = parts[1];
    let signature_b64 = parts[2];

    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    let claims_bytes = URL_SAFE_NO_PAD.decode(original_claims_b64).unwrap();

    let tamper_and_check = |key: &str, value: serde_json::Value| {
        let mut json: serde_json::Value = serde_json::from_slice(&claims_bytes).unwrap();
        json[key] = value;
        let tampered_bytes = serde_json::to_vec(&json).unwrap();
        let tampered_b64 = URL_SAFE_NO_PAD.encode(&tampered_bytes);
        let tampered_token = format!("{}.{}.{}", header_b64, tampered_b64, signature_b64);
        encoder.decode(&tampered_token).is_err()
    };

    assert!(
        tamper_and_check("roles", serde_json::json!(["admin"])),
        "roles 篡改必须被拒绝"
    );
    assert!(
        tamper_and_check("iss", serde_json::json!("https://evil.example.com")),
        "iss 篡改必须被拒绝"
    );
    assert!(
        tamper_and_check("sub", serde_json::json!("admin-1")),
        "sub 篡改必须被拒绝"
    );
    assert!(
        tamper_and_check("user_id", serde_json::json!(999)),
        "user_id 篡改必须被拒绝"
    );

    assert!(encoder.decode(&token).is_ok(), "原始 token 必须仍然有效");
}

/// A01-4：RBAC 通配符边界
///
/// - `read`（action 级）不授予任何资源（M-11 修复）
/// - `*` 全通配符授予所有操作
/// - `read:*` 仅精确匹配 `read:*`，不自动扩展到 `read:posts`（安全默认：粗粒度不隐式降级）
#[test]
fn a01_rbac_wildcard_boundary() {
    let mut az = RbacAuthorizer::new();

    az.grant("operator", "read");
    let op = User::new(1, "op").with_roles(vec!["operator".to_string()]);
    assert!(
        !az.can(&op, "read", "payments").unwrap(),
        "action 级权限不应授予任何资源"
    );

    az.grant("superadmin", "*");
    let superadmin = User::new(2, "super").with_roles(vec!["superadmin".to_string()]);
    assert!(az.can(&superadmin, "delete", "any").unwrap());
    assert!(az.can(&superadmin, "read", "any").unwrap());
    assert!(az.can(&superadmin, "admin", "panel").unwrap());

    az.grant("reader", "read:*");
    let reader = User::new(3, "reader").with_roles(vec!["reader".to_string()]);
    assert!(
        !az.can(&reader, "read", "posts").unwrap(),
        "read:* 不应隐式扩展到 read:posts（安全默认）"
    );
    assert!(
        !az.can(&reader, "write", "posts").unwrap(),
        "read:* 不应授予 write 操作"
    );

    let reader_explicit = User::new(4, "re2").with_roles(vec!["reader".to_string()]);
    let mut az2 = RbacAuthorizer::new();
    az2.grant("reader", "read:posts");
    assert!(az2.can(&reader_explicit, "read", "posts").unwrap());
    assert!(!az2.can(&reader_explicit, "read", "articles").unwrap());
}
