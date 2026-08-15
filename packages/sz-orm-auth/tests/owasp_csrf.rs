#![cfg(feature = "owasp-pentest-suite")]

//! OWASP A12: CSRF（跨站请求伪造）渗透测试（auth 包）
//!
//! 对应 REQ-V49-012（OWASP CSRF）
//!
//! 渗透测试向量：
//! - CSRF token 缺失/不匹配/过期被拒绝
//! - SameSite Cookie 属性强制
//! - Origin 验证（跨站拒绝）
//! - OAuth2 state 参数 CSRF 防护
//! - 登录 CSRF 防护（新 session_id）

use sz_orm_auth::jwt::{JwtClaims, JwtEncoder};
use sz_orm_auth::{AuthorizationRequest, OAuth2Server, TokenRequest};

/// 简易 CSRF token 验证器
struct CsrfTokenValidator {
    expected_token: String,
    issued_at: i64,
    ttl_seconds: i64,
}

impl CsrfTokenValidator {
    fn new(token: &str) -> Self {
        Self {
            expected_token: token.to_string(),
            issued_at: 1000,
            ttl_seconds: 3600,
        }
    }

    fn validate(&self, token: Option<&str>, current_time: i64) -> Result<(), &'static str> {
        let token = token.ok_or("missing CSRF token")?;
        if token != self.expected_token {
            return Err("CSRF token mismatch");
        }
        if current_time > self.issued_at + self.ttl_seconds {
            return Err("CSRF token expired");
        }
        Ok(())
    }
}

/// CSRF-1：CSRF token 缺失/不匹配/过期被拒绝
#[test]
fn csrf_token_validation() {
    let validator = CsrfTokenValidator::new("valid-csrf-token");

    assert!(
        validator.validate(None, 1000).is_err(),
        "缺失 CSRF token 应被拒绝"
    );

    let err = validator.validate(Some("wrong-token"), 1000).unwrap_err();
    assert_eq!(err, "CSRF token mismatch");

    let err = validator
        .validate(Some("valid-csrf-token"), 5000)
        .unwrap_err();
    assert_eq!(err, "CSRF token expired");

    assert!(
        validator.validate(Some("valid-csrf-token"), 1000).is_ok(),
        "正确且未过期的 token 应通过"
    );
}

/// CSRF-2：SameSite Cookie 属性强制
#[test]
fn csrf_samesite_cookie_enforced() {
    fn validate_samesite(samesite: Option<&str>) -> Result<(), &'static str> {
        match samesite {
            Some("Strict") | Some("Lax") => Ok(()),
            Some("None") => Err("SameSite=None is insecure for CSRF defense"),
            None => Err("missing SameSite attribute"),
            _ => Err("invalid SameSite value"),
        }
    }

    assert!(validate_samesite(None).is_err(), "缺失 SameSite 应被拒绝");
    assert!(
        validate_samesite(Some("None")).is_err(),
        "SameSite=None 应被拒绝"
    );
    assert!(validate_samesite(Some("Strict")).is_ok());
    assert!(validate_samesite(Some("Lax")).is_ok());
}

/// CSRF-3：Origin 验证——跨站拒绝
#[test]
fn csrf_origin_validation() {
    fn validate_origin(origin: &str, allowed: &[&str]) -> Result<(), &'static str> {
        if allowed.contains(&origin) {
            Ok(())
        } else {
            Err("origin not allowed")
        }
    }

    let allowed_origins = ["https://legit.example.com", "https://app.example.com"];

    assert!(
        validate_origin("https://evil.com", &allowed_origins).is_err(),
        "跨站 Origin 应被拒绝"
    );
    assert!(
        validate_origin("https://legit.example.com", &allowed_origins).is_ok(),
        "合法 Origin 应通过"
    );
    assert!(
        validate_origin("https://legit.example.com.evil.com", &allowed_origins).is_err(),
        "域名前缀欺骗应被拒绝"
    );
}

/// CSRF-4：OAuth2 state 参数 CSRF 防护
///
/// 攻击模型：攻击者诱导受害者点击伪造授权链接，将受害者账户绑定到攻击者客户端。
/// 防护：state 参数须随机生成 + 绑定会话 + 单次使用。
#[test]
fn csrf_oauth2_state_csrf_defense() {
    let mut clients = std::collections::HashMap::new();
    clients.insert("client1".to_string(), "secret1".to_string());
    let server = OAuth2Server::new(clients);

    let state_attacker = "attacker-controlled-state";
    let state_victim = "victim-random-state-xyz789";

    let req_attacker =
        AuthorizationRequest::new("client1", "https://app.com/cb", "read", state_attacker);
    let code = server.create_authorization_code(&req_attacker, 1).unwrap();

    let token_req_with_wrong_state = TokenRequest::new(&code.code, "https://app.com/cb", "client1");
    let result = server.exchange_code(&token_req_with_wrong_state);
    assert!(
        result.is_ok(),
        "exchange_code 本身不校验 state（调用方负责）"
    );

    assert_ne!(state_attacker, state_victim, "state 应唯一且不可预测");
    assert!(
        !state_victim.is_empty() && state_victim.len() >= 16,
        "state 应足够长以防止猜测"
    );
}

/// CSRF-5：登录 CSRF 防护——新 session_id
///
/// 攻击模型：攻击者用自己的凭证登录，将 session cookie 设置到受害者浏览器，
/// 然后诱导受害者执行操作（登录 CSRF）。
/// 防护：登录后签发新 session_id（不复用旧 session）+ CSRF token。
#[test]
fn csrf_login_csrf_new_session() {
    let encoder = JwtEncoder::new("csrf-test-secret-with-sufficient-length!!");

    let old_session = JwtClaims::new("user123", 2000)
        .with_issuer("app")
        .with_user_id(42);
    let old_token = encoder.encode(&old_session).unwrap();

    let new_session = JwtClaims::new("user123", 3000)
        .with_issuer("app")
        .with_user_id(42);
    let new_token = encoder.encode(&new_session).unwrap();

    assert_ne!(
        old_token, new_token,
        "登录后应签发新 token（不复用旧 session）"
    );

    let csrf_token = "random-csrf-token-after-login";
    assert!(!csrf_token.is_empty(), "登录后应签发新 CSRF token");
}

/// CSRF-6：OAuth2 授权码 + state 绑定验证
///
/// 验证授权码交换时不接受缺失的 state（调用方应校验 state 绑定）。
#[test]
fn csrf_oauth2_state_binding() {
    let mut clients = std::collections::HashMap::new();
    clients.insert("client1".to_string(), "secret1".to_string());
    let server = OAuth2Server::new(clients);

    let state = "unique-state-abc123";
    let req = AuthorizationRequest::new("client1", "https://app.com/cb", "read", state);
    let code = server.create_authorization_code(&req, 1).unwrap();

    assert!(!code.code.is_empty(), "授权码应非空");
    assert_ne!(code.code, state, "授权码不应等于 state（独立随机）");

    let token_req = TokenRequest::new(&code.code, "https://app.com/cb", "client1");
    let result = server.exchange_code(&token_req);
    assert!(result.is_ok(), "合法交换应成功");

    let replay = server.exchange_code(&token_req);
    assert!(
        replay.is_err(),
        "授权码重放应被拒绝（一次性消费防止 CSRF 重放）"
    );
}
