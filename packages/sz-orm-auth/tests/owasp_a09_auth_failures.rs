#![cfg(feature = "owasp-pentest-suite")]

//! OWASP A09: 身份识别和认证失败渗透测试（auth 包）
//!
//! 对应 REQ-V49-009（OWASP A09 深化）
//!
//! 渗透测试向量：
//! - 会话固定防护：刷新令牌重放被检测，整个家族被撤销
//! - 会话超时：过期 JWT 被 decode 拒绝
//! - 并发会话撤销：revoke_user 撤销用户所有会话
//! - 弱密钥拒绝：JwtAuthenticator::try_new 强制 ≥32 字节密钥
//! - JWT alg=none 绕过阻止：伪造 alg=none token 被 decode 拒绝
//! - OAuth2 授权码重放阻止：一次性消费
//! - MFA 空密钥绕过阻止：TotpVerifier 对空 secret 返回 false
//! - OAuth2 redirect_uri+PKCE 验证：不匹配被拒绝

use sz_orm_auth::jwt::{JwtClaims, JwtEncoder};
use sz_orm_auth::{
    AuthError, AuthorizationRequest, JwtAuthenticator, OAuth2Server, TokenRequest, TokenStore,
    TotpVerifier,
};

/// A09-1：会话固定防护——刷新令牌重放被检测，整个家族被撤销
///
/// 攻击模型：攻击者窃取刷新令牌，在合法用户刷新后尝试重放旧令牌。
/// 防护：TokenStore::refresh 检测到旧令牌已使用，返回 ReplayDetected 并撤销整个家族。
#[test]
fn a09_session_fixation_replay_detected() {
    let store = TokenStore::new();
    store.issue_family("rt_initial", 42).unwrap();

    let result1 = store.refresh("rt_initial", "rt_v2").unwrap();
    assert_eq!(result1.user_id, 42);

    let result2 = store.refresh("rt_initial", "rt_v3");
    assert!(
        matches!(
            result2,
            Err(sz_orm_auth::TokenFamilyError::ReplayDetected(_))
        ),
        "旧刷新令牌重放应被检测并返回 ReplayDetected"
    );

    assert!(!store.is_valid("rt_v2"), "重放后整个家族应被撤销");
    assert!(!store.is_valid("rt_initial"));
}

/// A09-2：会话超时——过期 JWT 被 decode 拒绝
///
/// 攻击模型：攻击者使用过期令牌访问资源。
/// 防护：JwtEncoder::decode 检查 exp，过期返回 TokenExpired。
#[test]
fn a09_session_timeout_expired_jwt_rejected() {
    let encoder = JwtEncoder::new("test-secret-with-sufficient-length-32bytes!");
    let past_exp = 1000;
    let claims = JwtClaims::new("user1", past_exp);
    let token = encoder.encode(&claims).unwrap();

    let result = encoder.decode(&token);
    assert!(
        matches!(result, Err(AuthError::TokenExpired(_))),
        "过期 JWT 应返回 TokenExpired，实际: {:?}",
        result
    );
}

/// A09-3：并发会话撤销——revoke_user 撤销用户所有会话
///
/// 攻击模型：用户账户被入侵，需要立即撤销所有活跃会话。
/// 防护：TokenStore::revoke_user 按 user_id 撤销所有令牌家族。
#[test]
fn a09_concurrent_session_revocation() {
    let store = TokenStore::new();
    store.issue_family("rt_session_1", 99).unwrap();
    store.issue_family("rt_session_2", 99).unwrap();
    store.issue_family("rt_session_3", 99).unwrap();

    assert!(store.is_valid("rt_session_1"));
    assert!(store.is_valid("rt_session_2"));
    assert!(store.is_valid("rt_session_3"));

    let revoked_count = store.revoke_user(99);
    assert_eq!(revoked_count, 3, "应撤销用户 99 的全部 3 个会话");

    assert!(!store.is_valid("rt_session_1"));
    assert!(!store.is_valid("rt_session_2"));
    assert!(!store.is_valid("rt_session_3"));
}

/// A09-4：弱密钥拒绝——JwtAuthenticator::try_new 强制 ≥32 字节密钥
///
/// 攻击模型：开发者使用短密钥（如 "secret"），易被暴力破解。
/// 防护：try_new 对 < 32 字节密钥返回 SecretTooShort。
#[test]
fn a09_weak_secret_rejected() {
    let weak_secret = "short";
    let result = JwtAuthenticator::try_new(weak_secret, "issuer", 3600);
    assert!(
        matches!(result, Err(AuthError::SecretTooShort(_))),
        "短密钥应被拒绝"
    );

    let strong_secret = "this-is-a-very-strong-secret-32+bytes!!";
    assert!(strong_secret.len() >= 32);
    let result2 = JwtAuthenticator::try_new(strong_secret, "issuer", 3600);
    assert!(result2.is_ok(), "≥32 字节密钥应被接受");
}

/// A09-5：JWT alg=none 绕过阻止——伪造 alg=none token 被 decode 拒绝
///
/// 攻击模型：攻击者将 JWT header 的 alg 改为 "none" 以绕过签名验证。
/// 防护：decode 先做签名常量时间比较，再检查 alg == "HS256"，
/// 任何 alg=none token 因签名不匹配被拒绝。
#[test]
fn a09_jwt_alg_none_bypass_blocked() {
    let encoder = JwtEncoder::new("test-secret-with-sufficient-length-32bytes!");

    let alg_none_token = "eyJhbGciOiJub25lIiwidHlwIjoiSldUIn0.eyJzdWIiOiJhZG1pbiIsImV4cCI6OTk5OTk5OTk5OSwiaWF0IjoxMDAwfQ.";
    let result = encoder.decode(alg_none_token);
    assert!(
        result.is_err(),
        "alg=none 伪造 token 应被拒绝，实际: {:?}",
        result
    );
}

/// A09-6：OAuth2 授权码重放阻止——一次性消费
///
/// 攻击模型：攻击者截获授权码后重放以获取令牌。
/// 防护：exchange_code 标记授权码为已使用，第二次调用返回错误。
#[test]
fn a09_oauth2_authorization_code_replay_blocked() {
    let mut clients = std::collections::HashMap::new();
    clients.insert("client1".to_string(), "secret1".to_string());
    let server = OAuth2Server::new(clients);

    let req = AuthorizationRequest::new("client1", "https://app.com/cb", "read", "state123");
    let code = server.create_authorization_code(&req, 1).unwrap();

    let token_req = TokenRequest::new(&code.code, "https://app.com/cb", "client1");
    let result1 = server.exchange_code(&token_req);
    assert!(result1.is_ok(), "首次兑换应成功");

    let result2 = server.exchange_code(&token_req);
    assert!(
        result2.is_err(),
        "授权码重放应被拒绝（一次性消费），实际: {:?}",
        result2
    );
}

/// A09-7：MFA 空密钥绕过阻止——TotpVerifier 对空 secret 返回 false
///
/// 攻击模型：攻击者利用空密钥生成恒定 TOTP 码 "000000" 绕过 MFA。
/// 防护：TotpVerifier::verify_at 对空 base32 secret 返回 false。
#[test]
fn a09_mfa_empty_secret_bypass_blocked() {
    let verifier = TotpVerifier::new();
    let result = verifier.verify_at("", "000000", 1700000000);
    assert!(!result, "空 base32 secret 的 TOTP 验证应返回 false");

    let result2 = verifier.verify_at("", "123456", 1700000000);
    assert!(!result2, "空 secret 对任意码都应返回 false");
}

/// A09-8：OAuth2 redirect_uri+PKCE 验证——不匹配被拒绝
///
/// 攻击模型：攻击者篡改 redirect_uri 以截获授权码回调；
/// 或不提供 PKCE verifier 以绕过码交换保护。
/// 防护：exchange_code 严格校验 redirect_uri 一致性和 PKCE verifier。
#[test]
fn a09_oauth2_redirect_uri_and_pkce_enforced() {
    let mut clients = std::collections::HashMap::new();
    clients.insert("client1".to_string(), "secret1".to_string());
    let server = OAuth2Server::new(clients);

    let challenge = "E9Melhoa2OwvFrEMTJguCHaoinK1rN6Mi5Y1QQJ0m2_3Q";
    let req = AuthorizationRequest::new("client1", "https://app.com/cb", "read", "state")
        .with_pkce(challenge, "S256");
    let code = server.create_authorization_code(&req, 1).unwrap();

    let wrong_redirect = TokenRequest::new(&code.code, "https://evil.com/cb", "client1");
    assert!(
        server.exchange_code(&wrong_redirect).is_err(),
        "redirect_uri 不匹配应被拒绝"
    );

    let req2 = AuthorizationRequest::new("client1", "https://app.com/cb", "read", "state2")
        .with_pkce(challenge, "S256");
    let code2 = server.create_authorization_code(&req2, 2).unwrap();

    let no_verifier = TokenRequest::new(&code2.code, "https://app.com/cb", "client1");
    assert!(
        server.exchange_code(&no_verifier).is_err(),
        "缺少 PKCE verifier 应被拒绝"
    );

    let wrong_verifier = TokenRequest::new(&code2.code, "https://app.com/cb", "client1")
        .with_code_verifier("attacker-guessed-verifier");
    assert!(
        server.exchange_code(&wrong_verifier).is_err(),
        "错误 PKCE verifier 应被拒绝"
    );
}
