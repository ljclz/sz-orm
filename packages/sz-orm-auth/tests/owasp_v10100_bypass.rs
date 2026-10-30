#![cfg(feature = "owasp-pentest-suite")]

//! OWASP v10.10.0 防御绕过渗透测试（T4.5）
//!
//! 新增 4 个防御绕过变体：角色操纵、哈希碰撞尝试、
//! 时序攻击、JWT 算法混淆。
//! 复用既有 owasp-pentest-suite feature gate。

use sz_orm_auth::jwt::{JwtClaims, JwtEncoder};
use sz_orm_auth::{Authorizer, RbacAuthorizer, User};

/// 绕过-1：角色操纵变体（大小写/空格/前缀注入）
///
/// 攻击模型：攻击者尝试通过角色名变体（大小写/空格/前缀注入）
/// 绕过 RBAC 检查。RBAC 应精确匹配，不忽略大小写或前缀。
#[test]
fn test_owasp_v10100_a01_bypass_role_manipulation_variant() {
    let mut az = RbacAuthorizer::new();
    az.grant("admin", "delete:users");
    az.grant("admin", "read:all");

    let variants = [
        "Admin",
        "ADMIN",
        " admin",
        "admin ",
        "admin\x00",
        "administrator",
    ];
    for variant in &variants {
        let attacker = User::new(1, "attacker").with_roles(vec![variant.to_string()]);
        assert!(
            !az.can(&attacker, "delete", "users").unwrap_or(true),
            "角色变体 '{}' 不应获得 admin 权限",
            variant
        );
    }

    let real_admin = User::new(2, "admin").with_roles(vec!["admin".to_string()]);
    assert!(az.can(&real_admin, "delete", "users").unwrap());
}

/// 绕过-2：哈希碰撞尝试变体
///
/// 攻击模型：攻击者尝试构造不同的 JWT claims 产生相同签名。
/// HMAC-SHA256 抗碰撞，不同 claims 必须产生不同 token。
#[test]
fn test_owasp_v10100_a01_bypass_hash_collision_variant() {
    let encoder = JwtEncoder::new("super-secret-key-for-owasp-v10100-bypass-testing");

    let claims_a = JwtClaims::new("user-a", 9999999999).with_user_id(1);
    let claims_b = JwtClaims::new("user-b", 9999999999).with_user_id(2);
    let claims_c = JwtClaims::new("user-a", 9999999999).with_user_id(1);

    let token_a = encoder.encode(&claims_a).unwrap();
    let token_b = encoder.encode(&claims_b).unwrap();
    let token_c = encoder.encode(&claims_c).unwrap();

    assert_ne!(token_a, token_b, "不同 claims 必须产生不同 token");
    assert_eq!(token_a, token_c, "相同 claims 必须产生相同 token");

    let decoded_a = encoder.decode(&token_a).unwrap();
    let decoded_b = encoder.decode(&token_b).unwrap();
    assert_ne!(decoded_a.sub, decoded_b.sub, "解码后 subject 必须不同");
}

/// 绕过-3：时序攻击变体（JWT 签名验证不泄露信息）
///
/// 攻击模型：攻击者通过测量验证响应时间推断签名正确性。
/// 防御：无论 token 格式如何错误，decode 都应返回 Err（不泄露差异）。
#[test]
fn test_owasp_v10100_a01_bypass_timing_attack_variant() {
    let encoder = JwtEncoder::new("super-secret-key-for-owasp-v10100-timing-test");

    let claims = JwtClaims::new("user-1", 9999999999).with_roles(vec!["user".to_string()]);
    let valid_token = encoder.encode(&claims).unwrap();

    let tampered_tokens = [
        format!("{}.{}.{}", "invalid", "invalid", "invalid"),
        valid_token[..valid_token.len() - 1].to_string(),
        format!(
            "{}.{}.x",
            valid_token.split('.').next().unwrap(),
            valid_token.split('.').nth(1).unwrap()
        ),
        String::new(),
        "a.b.c.d".to_string(),
    ];

    for tampered in &tampered_tokens {
        let result = encoder.decode(tampered);
        assert!(result.is_err(), "篡改 token 必须被拒绝: {}", tampered);
    }

    assert!(encoder.decode(&valid_token).is_ok(), "合法 token 必须通过");
}

/// 绕过-4：JWT 算法混淆变体（alg:none / alg:RS256）
///
/// 攻击模型：攻击者将 JWT header 的 alg 改为 "none" 或 "RS256"，
/// 试图绕过 HMAC 签名验证。decode 必须拒绝非 HS256 算法。
#[test]
fn test_owasp_v10100_a01_bypass_jwt_algorithm_confusion() {
    let encoder = JwtEncoder::new("super-secret-key-for-owasp-v10100-alg-confusion");

    let claims = JwtClaims::new("user-1", 9999999999).with_roles(vec!["admin".to_string()]);
    let valid_token = encoder.encode(&claims).unwrap();

    let parts: Vec<&str> = valid_token.split('.').collect();
    let claims_b64 = parts[1];
    let signature_b64 = parts[2];

    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;

    let forge_with_alg = |alg: &str| -> String {
        let header = serde_json::json!({"alg": alg, "typ": "JWT"});
        let header_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).unwrap());
        format!("{}.{}.{}", header_b64, claims_b64, signature_b64)
    };

    let alg_none_token = forge_with_alg("none");
    let alg_rs256_token = forge_with_alg("RS256");
    let alg_empty_token = forge_with_alg("");

    assert!(
        encoder.decode(&alg_none_token).is_err(),
        "alg:none 必须被拒绝"
    );
    assert!(
        encoder.decode(&alg_rs256_token).is_err(),
        "alg:RS256 必须被拒绝"
    );
    assert!(
        encoder.decode(&alg_empty_token).is_err(),
        "alg:'' 必须被拒绝"
    );
}
