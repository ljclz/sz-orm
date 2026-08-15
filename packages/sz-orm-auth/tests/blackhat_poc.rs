//! 黑帽审计 PoC（攻击者视角）——2026-08-14
//!
//! 说明：这些测试断言"攻击行为成立"，测试通过 = 漏洞被证明存在。
//! 对应白帽报告：C-1（OAuth2 授权码可预测）、C-2（JWT 令牌类型混淆）、
//! M-10（TOTP 空密钥恒 "000000"）、M-11（RBAC action 级权限降级越权）。

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::time::{SystemTime, UNIX_EPOCH};
use sz_orm_auth::authorizer::RbacAuthorizer;
use sz_orm_auth::mfa::{MfaManager, MfaSecret, TotpVerifier};
use sz_orm_auth::oauth2::{AuthorizationRequest, OAuth2Server};
use sz_orm_auth::{Authorizer, Credentials, JwtAuthenticator, User};

// ═══════════════════════════════════════════════════════════════════════════
// 回归测试（C-1 修复验证）：OAuth2 授权码不可被时间戳枚举还原
//
// 修复前（v4.8.0 之前行为）：DefaultHasher + 纳秒种子，±1ms 窗口枚举
// 102 万候选 0.84s 即还原授权码（黑帽实证）。修复后使用 OsRng 32 字节
// CSPRNG——时间戳枚举必须失败。
// ═══════════════════════════════════════════════════════════════════════════
#[test]
fn regress_oauth2_code_not_predictable_via_timestamp_enumeration() {
    let mut server = OAuth2Server::new(HashMap::new());
    server.register_client("victim-client", "client-secret");

    let req = AuthorizationRequest::new(
        "victim-client",
        "https://victim.example/callback",
        "read write",
        "state-123",
    );

    let t0 = SystemTime::now();
    let issued = server
        .create_authorization_code(&req, 1001)
        .expect("issue code");
    let t1 = SystemTime::now();
    let target = &issued.code;

    // 修复后授权码为 64 位十六进制（32 字节 CSPRNG）
    assert_eq!(target.len(), 64, "授权码应为 32 字节随机十六进制");
    assert_ne!(
        target,
        &"0".repeat(64),
        "授权码不得为全零（随机源失效信号）"
    );

    // 攻击者枚举窗口 ±1ms、1ns 粒度（修复前此窗口可还原）
    let start_ns = t0.duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let end_ns = t1.duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let window = 1_000_000u128;

    let mut found_seed: Option<u128> = None;
    let mut t = start_ns.saturating_sub(window);
    let end = end_ns + window;
    while t <= end {
        let mut hasher = DefaultHasher::new();
        t.hash(&mut hasher);
        let candidate = format!("{:064x}", hasher.finish());
        if &candidate == target {
            found_seed = Some(t);
            break;
        }
        t += 1;
    }

    assert!(
        found_seed.is_none(),
        "授权码不得被时间戳枚举还原（OsRng 修复失效）"
    );
    println!("[regress-C-1] ✅ 修复验证通过：时间戳枚举无法还原授权码（OsRng 生效）");
}

// ═══════════════════════════════════════════════════════════════════════════
// 回归测试（C-2 修复验证）：令牌类型混淆被切断
//
// 修复前（v4.8.0 之前行为）：访问令牌可送入 refresh 端点无限续期（exp 永续
// 重置，黑帽实证）。修复后：access 令牌被 refresh 端点严格拒绝；仅显式
// token_use="refresh" 的令牌可刷新，且签发结果为 access 令牌。
// ═══════════════════════════════════════════════════════════════════════════
struct AcceptAllVerifier;
impl sz_orm_auth::PasswordVerifier for AcceptAllVerifier {
    fn verify_password(
        &self,
        _username: &str,
        _password: &str,
    ) -> Result<i64, sz_orm_auth::AuthError> {
        Ok(42)
    }
}

#[test]
fn regress_access_token_rejected_by_refresh_endpoint() {
    let auth = JwtAuthenticator::new(
        "blackhat-poc-secret-key-0123456789abcdef",
        "issuer-test",
        3600,
    )
    .with_password_verifier(std::sync::Arc::new(AcceptAllVerifier));

    let token = auth
        .authenticate(&Credentials::new("victim", "anything"))
        .expect("issue access token");

    // 1) 攻击面切断：访问令牌送入 refresh 端点必须被拒绝
    let attack = auth.refresh_token(&token.access_token);
    assert!(
        attack.is_err(),
        "访问令牌不得被 refresh 端点接受（类型混淆修复失效）"
    );
    println!(
        "[regress-C-2] ✅ 攻击面切断：访问令牌被 refresh 拒绝：{:?}",
        attack.err().map(|e| format!("{e:?}"))
    );

    // 2) 正常路径：真实 refresh 令牌仍可刷新，且产物为 access 令牌
    let refresh = token.refresh_token.expect("token must carry refresh token");
    let renewed = auth.refresh_token(&refresh).expect("refresh should work");
    assert!(renewed.access_token != refresh, "刷新必须签发新的访问令牌");
    // 新访问令牌可被 verify_token 接受
    assert!(
        auth.verify_token(&renewed.access_token).is_ok(),
        "刷新产物必须是有效访问令牌"
    );
    // 3) 类型声明实证：刷新令牌 claims 带 token_use="refresh"，访问令牌带 "access"
    let encoder = sz_orm_auth::jwt::JwtEncoder::new("blackhat-poc-secret-key-0123456789abcdef");
    let refresh_claims = encoder.decode(&refresh).expect("decode refresh");
    assert_eq!(
        refresh_claims.token_use.as_deref(),
        Some("refresh"),
        "刷新令牌必须声明 token_use=refresh"
    );
    let access_claims = encoder.decode(&token.access_token).expect("decode access");
    assert_eq!(
        access_claims.token_use.as_deref(),
        Some("access"),
        "访问令牌必须声明 token_use=access"
    );
    println!("[regress-C-2] ✅ 访问/刷新令牌类型声明正确区分（token_use claim 生效）");
}

// ═══════════════════════════════════════════════════════════════════════════
// 回归测试（M-10 修复验证）：TOTP 空密钥被拒绝
//
// 修复前（黑帽实证）：空 base32 密钥生成恒 "000000" 且 verify 放行，
// 空密钥账户一次即过 MFA。修复后：verify 入口拒绝空密钥。
// ═══════════════════════════════════════════════════════════════════════════
#[test]
fn regress_totp_empty_secret_rejected() {
    // 第一层：TotpVerifier 直接空密钥——验证必须失败
    let verifier = TotpVerifier::new();
    assert!(
        !verifier.verify("", "000000"),
        "空密钥的恒定码必须被 verify 拒绝（M-10 修复失效）"
    );
    assert!(!verifier.verify("", "123456"), "空密钥下任何码都必须被拒绝");

    // 第二层：MfaManager 绑定空密钥用户——MFA 必须拒绝
    let mgr = MfaManager::new();
    mgr.bind_secret(
        "user-with-empty-key",
        MfaSecret::from_base32("", "acc", "iss"),
    );
    let ok = mgr
        .verify("user-with-empty-key", "000000")
        .expect("verify should not error");
    assert!(!ok, "空密钥用户 MFA 必须拒绝 '000000'");
    println!("[regress-M-10] ✅ 空密钥 TOTP 绕过被切断");

    // 正常路径不受影响：真实密钥仍可生成-验证
    let real = MfaSecret::from_base32("JBSWY3DPEHPK3PXP", "acc", "iss");
    let code = verifier.generate_now(&real.base32_secret);
    assert!(
        verifier.verify(&real.base32_secret, &code),
        "正常密钥验证必须通过"
    );
    println!("[regress-M-10] ✅ 正常 TOTP 流程不受影响");
}

// ═══════════════════════════════════════════════════════════════════════════
// 回归测试（M-11 修复验证）：RBAC action 级权限不再越权
//
// 修复前（黑帽实证）：grant("operator","read") 隐式授予 read:任意资源。
// 修复后：仅显式 `action:resource` 或 `action:*` / `*` 通配符放行。
// ═══════════════════════════════════════════════════════════════════════════
#[test]
fn regress_rbac_action_level_permission_no_longer_grants_all_resources() {
    let mut az = RbacAuthorizer::new();
    az.grant("operator", "read"); // 运维按习惯配置粗粒度权限

    let user = User::new(1, "alice").with_roles(vec!["operator".to_string()]);

    // 越权面必须被切断
    let unauth_resources = ["payments", "users_salary", "medical_records", "auth_tokens"];
    for resource in unauth_resources {
        let allowed = az.can(&user, "read", resource).expect("can ok");
        assert!(!allowed, "action 级 read 不得隐式授予 read:{resource}");
    }
    println!("[regress-M-11] ✅ action 级权限不再授予任何资源");

    // 显式授权仍工作：read:posts 只放行 read:posts
    let mut az2 = RbacAuthorizer::new();
    az2.grant("operator", "read:posts");
    let user2 = User::new(1, "bob").with_roles(vec!["operator".to_string()]);
    assert!(az2.can(&user2, "read", "posts").expect("ok"));
    assert!(!az2.can(&user2, "read", "users").expect("ok"));
    println!("[regress-M-11] ✅ 显式 action:resource 授权不受影响");
}
