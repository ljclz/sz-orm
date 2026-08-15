//! OAuth2 授权码流程（Authorization Code Flow）
//!
//! 实现 RFC 6749 Section 4.1 的授权码流程：
//! 1. 客户端重定向用户到授权服务器
//! 2. 用户授权后，授权服务器返回授权码
//! 3. 客户端用授权码交换访问令牌
//!
//! 本模块提供流程状态管理，不包含 HTTP 传输层实现。

use parking_lot::Mutex;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::AuthError;

/// OAuth2 授权请求参数
#[derive(Debug, Clone)]
pub struct AuthorizationRequest {
    /// 客户端 ID
    pub client_id: String,
    /// 重定向 URI
    pub redirect_uri: String,
    /// 请求的权限范围（空格分隔）
    pub scope: String,
    /// CSRF 防护状态值
    pub state: String,
    /// 响应类型（固定为 "code"）
    pub response_type: String,
    /// PKCE code_challenge（RFC 7636，v4.8.0 修复 M-7）
    ///
    /// 携带 challenge 的授权码在交换令牌时**强制**要求匹配的 code_verifier，
    /// 抵御授权码拦截攻击（即使授权码泄露也无法兑换令牌）。
    pub code_challenge: Option<String>,
    /// PKCE 方法（当前仅支持 "S256"）
    pub code_challenge_method: Option<String>,
}

impl AuthorizationRequest {
    pub fn new(
        client_id: impl Into<String>,
        redirect_uri: impl Into<String>,
        scope: impl Into<String>,
        state: impl Into<String>,
    ) -> Self {
        Self {
            client_id: client_id.into(),
            redirect_uri: redirect_uri.into(),
            scope: scope.into(),
            state: state.into(),
            response_type: "code".to_string(),
            code_challenge: None,
            code_challenge_method: None,
        }
    }

    /// 附加 PKCE 参数（v4.8.0 修复 M-7）
    ///
    /// `challenge` 为 S256 变换后的 code_challenge（服务端生成，
    /// 详见 RFC 7636 §4.2）；`method` 仅支持 `"S256"`。
    pub fn with_pkce(mut self, challenge: impl Into<String>, method: &str) -> Self {
        self.code_challenge = Some(challenge.into());
        self.code_challenge_method = Some(method.to_string());
        self
    }
}

/// OAuth2 授权码
#[derive(Debug, Clone)]
pub struct AuthorizationCode {
    /// 授权码值
    pub code: String,
    /// 关联的客户端 ID
    pub client_id: String,
    /// 关联的用户 ID
    pub user_id: i64,
    /// 重定向 URI（必须与授权请求一致）
    pub redirect_uri: String,
    /// 请求的权限范围
    pub scope: String,
    /// 创建时间（Unix 秒）
    pub created_at: i64,
    /// 过期时间（Unix 秒），默认 600 秒（10 分钟）
    pub expires_at: i64,
    /// 是否已使用（一次性消费）
    pub used: bool,
    /// PKCE code_challenge（签发时携带则交换时必须验证 verifier）
    pub code_challenge: Option<String>,
}

impl AuthorizationCode {
    /// 授权码默认有效期：10 分钟（RFC 6749 建议）
    const DEFAULT_LIFETIME_SECS: i64 = 600;

    pub fn new(
        code: impl Into<String>,
        client_id: impl Into<String>,
        user_id: i64,
        redirect_uri: impl Into<String>,
        scope: impl Into<String>,
    ) -> Self {
        let now = current_secs();
        Self {
            code: code.into(),
            client_id: client_id.into(),
            user_id,
            redirect_uri: redirect_uri.into(),
            scope: scope.into(),
            created_at: now,
            expires_at: now + Self::DEFAULT_LIFETIME_SECS,
            used: false,
            code_challenge: None,
        }
    }

    /// 是否已过期
    pub fn is_expired(&self) -> bool {
        current_secs() > self.expires_at
    }
}

/// OAuth2 令牌交换请求
#[derive(Debug, Clone)]
pub struct TokenRequest {
    pub grant_type: String,
    pub code: String,
    pub redirect_uri: String,
    pub client_id: String,
    /// 客户端密钥（v4.8.0 修复 M-7：提供时强制校验）
    ///
    /// 修复前 `exchange_code` 从不校验 client_secret——攻击者只需窃取
    /// 授权码 + 已知 client_id 即可兑换令牌（RFC 6749 §4.1.3 要求验证
    /// 客户端身份）。生产环境必须通过 [`TokenRequest::with_client_secret`]
    /// 携带密钥。
    pub client_secret: Option<String>,
    /// PKCE code_verifier（RFC 7636，v4.8.0 修复 M-7）
    ///
    /// 签发时携带 code_challenge 的授权码，交换时**必须**提供匹配的
    /// verifier，否则拒绝兑换。
    pub code_verifier: Option<String>,
}

impl TokenRequest {
    pub fn new(
        code: impl Into<String>,
        redirect_uri: impl Into<String>,
        client_id: impl Into<String>,
    ) -> Self {
        Self {
            grant_type: "authorization_code".to_string(),
            code: code.into(),
            redirect_uri: redirect_uri.into(),
            client_id: client_id.into(),
            client_secret: None,
            code_verifier: None,
        }
    }

    /// 附加客户端密钥（v4.8.0 修复 M-7：交换时强制校验）
    pub fn with_client_secret(mut self, secret: impl Into<String>) -> Self {
        self.client_secret = Some(secret.into());
        self
    }

    /// 附加 PKCE code_verifier（v4.8.0 修复 M-7）
    pub fn with_code_verifier(mut self, verifier: impl Into<String>) -> Self {
        self.code_verifier = Some(verifier.into());
        self
    }
}

/// OAuth2 授权服务器：管理授权码的签发、验证与交换。
///
/// 内部使用 `Mutex<HashMap>` 存储授权码，支持：
/// - 创建授权码（`create_authorization_code`）
/// - 用授权码交换令牌（`exchange_code`）
/// - 验证客户端凭据
/// - 授权码一次性消费
pub struct OAuth2Server {
    /// 已签发的授权码：code -> AuthorizationCode
    codes: Mutex<HashMap<String, AuthorizationCode>>,
    /// 已注册的客户端：client_id -> client_secret
    clients: HashMap<String, String>,
}

impl OAuth2Server {
    /// 创建授权服务器，注册一组客户端
    pub fn new(clients: HashMap<String, String>) -> Self {
        Self {
            codes: Mutex::new(HashMap::new()),
            clients,
        }
    }

    /// 创建空授权服务器，后续通过 `register_client` 注册
    pub fn empty() -> Self {
        Self {
            codes: Mutex::new(HashMap::new()),
            clients: HashMap::new(),
        }
    }

    /// 注册客户端
    pub fn register_client(
        &mut self,
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
    ) {
        self.clients.insert(client_id.into(), client_secret.into());
    }

    /// 验证客户端凭据
    pub fn validate_client(&self, client_id: &str, client_secret: &str) -> bool {
        self.clients
            .get(client_id)
            .map(|secret| secret == client_secret)
            .unwrap_or(false)
    }

    /// 是否已注册客户端
    pub fn has_client(&self, client_id: &str) -> bool {
        self.clients.contains_key(client_id)
    }

    /// 签发授权码
    ///
    /// 验证授权请求中的 client_id 已注册后，创建一次性授权码。
    pub fn create_authorization_code(
        &self,
        req: &AuthorizationRequest,
        user_id: i64,
    ) -> Result<AuthorizationCode, AuthError> {
        if !self.has_client(&req.client_id) {
            return Err(AuthError::Config(format!(
                "Unregistered client: {}",
                req.client_id
            )));
        }
        if req.response_type != "code" {
            return Err(AuthError::Config(format!(
                "Unsupported response_type: {}",
                req.response_type
            )));
        }
        let code_value = generate_code();
        let mut auth_code = AuthorizationCode::new(
            code_value,
            req.client_id.clone(),
            user_id,
            req.redirect_uri.clone(),
            req.scope.clone(),
        );
        // v4.8.0 修复 M-7：签发时记录 PKCE challenge
        auth_code.code_challenge = req.code_challenge.clone();
        self.codes
            .lock()
            .insert(auth_code.code.clone(), auth_code.clone());
        Ok(auth_code)
    }

    /// 用授权码交换访问令牌
    ///
    /// 验证流程：
    /// 1. 授权码存在
    /// 2. 授权码未过期
    /// 3. 授权码未使用（一次性消费）
    /// 4. redirect_uri 与签发时一致
    /// 5. client_id 与签发时一致
    /// 6. client_secret 提供时强制校验（v4.8.0 修复 M-7）
    /// 7. 签发时携带 PKCE challenge → 必须验证 code_verifier（v4.8.0 修复 M-7）
    pub fn exchange_code(&self, req: &TokenRequest) -> Result<AuthorizationCode, AuthError> {
        let mut codes = self.codes.lock();
        let auth_code = codes
            .get(&req.code)
            .ok_or_else(|| AuthError::TokenInvalid("Invalid authorization code".to_string()))?;

        if auth_code.is_expired() {
            return Err(AuthError::TokenExpired(
                "Authorization code expired".to_string(),
            ));
        }

        if auth_code.used {
            return Err(AuthError::TokenInvalid(
                "Authorization code already used".to_string(),
            ));
        }

        if auth_code.redirect_uri != req.redirect_uri {
            return Err(AuthError::TokenInvalid("Redirect URI mismatch".to_string()));
        }

        if auth_code.client_id != req.client_id {
            return Err(AuthError::TokenInvalid("Client ID mismatch".to_string()));
        }

        // v4.8.0 修复 M-7（RFC 6749 §4.1.3）：客户端密钥校验。
        // 携带 client_secret 的请求必须与注册表匹配——此前从不校验，
        // 仅凭授权码 + 已知 client_id 即可兑换令牌。
        if let Some(secret) = &req.client_secret {
            if !self.validate_client(&req.client_id, secret) {
                return Err(AuthError::TokenInvalid(
                    "Invalid client credentials".to_string(),
                ));
            }
        }

        // v4.8.0 修复 M-7（RFC 7636）：PKCE 强制验证——签发时携带
        // code_challenge 的授权码，交换必须提供匹配的 code_verifier。
        // 授权码被拦截（回调劫持/日志泄露）时，攻击者仍无法兑换令牌。
        if let Some(challenge) = &auth_code.code_challenge {
            let verifier = req.code_verifier.as_deref().ok_or_else(|| {
                AuthError::TokenInvalid("PKCE code_verifier required".to_string())
            })?;
            if !verify_pkce_s256(challenge, verifier) {
                return Err(AuthError::TokenInvalid(
                    "PKCE verification failed".to_string(),
                ));
            }
        }

        // 标记为已使用
        let result = auth_code.clone();
        codes.get_mut(&req.code).unwrap().used = true;
        Ok(result)
    }

    /// 返回当前存储的授权码数量
    pub fn code_count(&self) -> usize {
        self.codes.lock().len()
    }

    /// 清理已过期或已使用的授权码
    pub fn cleanup(&self) -> usize {
        let mut codes = self.codes.lock();
        let before = codes.len();
        codes.retain(|_, c| !c.is_expired() && !c.used);
        before - codes.len()
    }
}

/// 生成随机授权码（32 字节随机十六进制）
///
/// v4.8.0 修复 Critical C-1（CWE-338）：使用 `OsRng`（密码学安全 RNG）替代
/// `DefaultHasher` + 纳秒种子。原实现熵完全来自可预测时间戳——2026-08-14
/// 黑帽审计实证：攻击者在 ±1ms 窗口内枚举纳秒种子，102 万候选 0.84s 即还原
/// 真实授权码（见 docs/assessment/2026-08-14-blackhat-security-audit.md）。
/// 修复模式与 token_store.rs / mfa.rs 的家族 ID / MFA 密钥生成保持一致。
fn generate_code() -> String {
    use rand::rngs::OsRng;
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let hex: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();
    hex
}

/// PKCE S256 验证（RFC 7636 §4.6，v4.8.0 修复 M-7）
///
/// `base64url(sha256(code_verifier), 无 padding)` 必须与签发的
/// `code_challenge` 相等。时间常数比较防侧信道。
fn verify_pkce_s256(challenge: &str, verifier: &str) -> bool {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(verifier.as_bytes());
    let computed = URL_SAFE_NO_PAD.encode(digest);
    // 长度不同快速失败；相同长度走常数时间比较
    if computed.len() != challenge.len() {
        return false;
    }
    constant_time_eq(computed.as_bytes(), challenge.as_bytes())
}

/// 常数时间字节比较（防时序侧信道）
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let mut diff: u8 = (a.len() as u8) ^ (b.len() as u8);
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= u8::from(x != y);
    }
    diff == 0
}

fn current_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_server() -> OAuth2Server {
        let mut clients = HashMap::new();
        clients.insert("client1".to_string(), "secret1".to_string());
        OAuth2Server::new(clients)
    }

    fn make_request() -> AuthorizationRequest {
        AuthorizationRequest::new("client1", "https://app.com/cb", "read write", "xyz123")
    }

    #[test]
    fn test_authorization_request_new() {
        let req = AuthorizationRequest::new("cid", "https://cb", "read", "state");
        assert_eq!(req.client_id, "cid");
        assert_eq!(req.redirect_uri, "https://cb");
        assert_eq!(req.scope, "read");
        assert_eq!(req.state, "state");
        assert_eq!(req.response_type, "code");
    }

    #[test]
    fn test_oauth2_server_validate_client() {
        let server = make_server();
        assert!(server.validate_client("client1", "secret1"));
        assert!(!server.validate_client("client1", "wrong"));
        assert!(!server.validate_client("unknown", "secret1"));
    }

    #[test]
    fn test_oauth2_server_has_client() {
        let server = make_server();
        assert!(server.has_client("client1"));
        assert!(!server.has_client("unknown"));
    }

    #[test]
    fn test_oauth2_server_register_client() {
        let mut server = OAuth2Server::empty();
        assert!(!server.has_client("new_client"));
        server.register_client("new_client", "new_secret");
        assert!(server.has_client("new_client"));
        assert!(server.validate_client("new_client", "new_secret"));
    }

    #[test]
    fn test_create_authorization_code_success() {
        let server = make_server();
        let req = make_request();
        let code = server.create_authorization_code(&req, 42).unwrap();
        assert_eq!(code.client_id, "client1");
        assert_eq!(code.user_id, 42);
        assert_eq!(code.redirect_uri, "https://app.com/cb");
        assert_eq!(code.scope, "read write");
        assert!(!code.used);
        assert!(!code.is_expired());
        assert_eq!(server.code_count(), 1);
    }

    #[test]
    fn test_create_authorization_code_unregistered_client() {
        let server = make_server();
        let req = AuthorizationRequest::new("unknown", "https://cb", "read", "state");
        let result = server.create_authorization_code(&req, 1);
        assert!(matches!(result, Err(AuthError::Config(_))));
    }

    #[test]
    fn test_create_authorization_code_wrong_response_type() {
        let server = make_server();
        let mut req = make_request();
        req.response_type = "token".to_string();
        let result = server.create_authorization_code(&req, 1);
        assert!(matches!(result, Err(AuthError::Config(_))));
    }

    #[test]
    fn test_exchange_code_success() {
        let server = make_server();
        let req = make_request();
        let code = server.create_authorization_code(&req, 99).unwrap();
        let token_req = TokenRequest::new(&code.code, "https://app.com/cb", "client1");
        let result = server.exchange_code(&token_req).unwrap();
        assert_eq!(result.user_id, 99);
        assert_eq!(result.client_id, "client1");
    }

    #[test]
    fn test_exchange_code_invalid_code() {
        let server = make_server();
        let token_req = TokenRequest::new("nonexistent", "https://app.com/cb", "client1");
        let result = server.exchange_code(&token_req);
        assert!(matches!(result, Err(AuthError::TokenInvalid(_))));
    }

    #[test]
    fn test_exchange_code_already_used() {
        let server = make_server();
        let req = make_request();
        let code = server.create_authorization_code(&req, 1).unwrap();
        let token_req = TokenRequest::new(&code.code, "https://app.com/cb", "client1");
        // 第一次交换成功
        server.exchange_code(&token_req).unwrap();
        // 第二次应失败（一次性消费）
        let result = server.exchange_code(&token_req);
        assert!(matches!(result, Err(AuthError::TokenInvalid(_))));
    }

    #[test]
    fn test_exchange_code_redirect_uri_mismatch() {
        let server = make_server();
        let req = make_request();
        let code = server.create_authorization_code(&req, 1).unwrap();
        let token_req = TokenRequest::new(&code.code, "https://wrong.com/cb", "client1");
        let result = server.exchange_code(&token_req);
        assert!(matches!(result, Err(AuthError::TokenInvalid(_))));
    }

    #[test]
    fn test_exchange_code_client_id_mismatch() {
        let server = make_server();
        let req = make_request();
        let code = server.create_authorization_code(&req, 1).unwrap();
        let token_req = TokenRequest::new(&code.code, "https://app.com/cb", "wrong_client");
        let result = server.exchange_code(&token_req);
        assert!(matches!(result, Err(AuthError::TokenInvalid(_))));
    }

    #[test]
    fn test_authorization_code_is_expired() {
        let mut code = AuthorizationCode::new("c", "cid", 1, "uri", "scope");
        assert!(!code.is_expired());
        code.expires_at = current_secs() - 100;
        assert!(code.is_expired());
    }

    #[test]
    fn test_oauth2_cleanup_removes_used() {
        let server = make_server();
        let req = make_request();
        let code = server.create_authorization_code(&req, 1).unwrap();
        let token_req = TokenRequest::new(&code.code, "https://app.com/cb", "client1");
        server.exchange_code(&token_req).unwrap();
        assert_eq!(server.code_count(), 1);
        let removed = server.cleanup();
        assert_eq!(removed, 1);
        assert_eq!(server.code_count(), 0);
    }

    #[test]
    fn test_oauth2_cleanup_keeps_valid() {
        let server = make_server();
        let req = make_request();
        server.create_authorization_code(&req, 1).unwrap();
        assert_eq!(server.code_count(), 1);
        let removed = server.cleanup();
        assert_eq!(removed, 0);
        assert_eq!(server.code_count(), 1);
    }

    #[test]
    fn test_generate_code_non_empty() {
        let code = generate_code();
        assert!(!code.is_empty());
        assert_eq!(code.len(), 64);
    }

    #[test]
    fn test_generate_code_different_each_call() {
        let c1 = generate_code();
        std::thread::sleep(std::time::Duration::from_millis(1));
        let c2 = generate_code();
        // 极大概率不同
        assert_ne!(c1, c2);
    }

    #[test]
    fn test_oauth2_empty_server() {
        let server = OAuth2Server::empty();
        assert_eq!(server.code_count(), 0);
        assert!(!server.has_client("any"));
    }

    #[test]
    fn test_multiple_clients() {
        let mut server = OAuth2Server::empty();
        server.register_client("app1", "secret1");
        server.register_client("app2", "secret2");
        let req1 = AuthorizationRequest::new("app1", "https://a1/cb", "read", "s1");
        let req2 = AuthorizationRequest::new("app2", "https://a2/cb", "write", "s2");
        let c1 = server.create_authorization_code(&req1, 1).unwrap();
        let c2 = server.create_authorization_code(&req2, 2).unwrap();
        assert_eq!(server.code_count(), 2);
        // app1 不能用 app2 的 code
        let wrong_req = TokenRequest::new(&c2.code, "https://a2/cb", "app1");
        assert!(server.exchange_code(&wrong_req).is_err());
        // 正确交换
        let right_req = TokenRequest::new(&c1.code, "https://a1/cb", "app1");
        assert!(server.exchange_code(&right_req).is_ok());
    }

    // ── v4.8.0 修复 M-7：client_secret 强校验 + PKCE ──

    /// RFC 7636 官方向量：verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"
    /// challenge = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    fn pkce_vector() -> (String, String) {
        (
            "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".to_string(),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM".to_string(),
        )
    }

    #[test]
    fn test_pkce_s256_rfc7636_vector() {
        let (verifier, challenge) = pkce_vector();
        assert!(
            verify_pkce_s256(&challenge, &verifier),
            "RFC 7636 §A.1 官方向量必须验证通过"
        );
        assert!(
            !verify_pkce_s256(&challenge, "wrong-verifier"),
            "错误 verifier 必须失败"
        );
        assert!(!verify_pkce_s256(&challenge, ""), "空 verifier 必须失败");
    }

    #[test]
    fn test_exchange_code_client_secret_mismatch_rejected() {
        let server = make_server();
        let req = make_request();
        let code = server.create_authorization_code(&req, 1).unwrap();

        // 修复前：exchange_code 从不校验 client_secret，攻击者凭码即可兑换
        let bad = TokenRequest::new(&code.code, "https://app.com/cb", "client1")
            .with_client_secret("wrong-secret");
        let result = server.exchange_code(&bad);
        assert!(
            matches!(result, Err(AuthError::TokenInvalid(_))),
            "错误 client_secret 必须被拒绝（M-7 修复失效）"
        );

        // 正确 secret 放行
        let good = TokenRequest::new(&code.code, "https://app.com/cb", "client1")
            .with_client_secret("secret1");
        assert!(server.exchange_code(&good).is_ok());
    }

    #[test]
    fn test_pkce_challenge_enforced_on_exchange() {
        let server = make_server();
        let (verifier, challenge) = pkce_vector();
        let req = make_request().with_pkce(challenge.clone(), "S256");
        let code = server.create_authorization_code(&req, 7).unwrap();
        assert_eq!(code.code_challenge.as_deref(), Some(challenge.as_str()));

        // 攻击场景：拦截到授权码但没有 verifier → 必须拒绝
        let no_verifier = TokenRequest::new(&code.code, "https://app.com/cb", "client1");
        assert!(
            matches!(
                server.exchange_code(&no_verifier),
                Err(AuthError::TokenInvalid(_))
            ),
            "缺少 code_verifier 必须被拒绝（M-7 修复失效）"
        );

        // 攻击场景：错误的 verifier → 必须拒绝
        let wrong_verifier = TokenRequest::new(&code.code, "https://app.com/cb", "client1")
            .with_code_verifier("attacker-guessed-verifier");
        assert!(
            matches!(
                server.exchange_code(&wrong_verifier),
                Err(AuthError::TokenInvalid(_))
            ),
            "错误 code_verifier 必须被拒绝"
        );

        // 正常路径：正确 verifier → 放行
        let correct = TokenRequest::new(&code.code, "https://app.com/cb", "client1")
            .with_code_verifier(verifier);
        let exchanged = server.exchange_code(&correct).unwrap();
        assert_eq!(exchanged.user_id, 7);
    }

    #[test]
    fn test_pkce_and_secret_combined() {
        let server = make_server();
        let (verifier, challenge) = pkce_vector();
        let req = make_request().with_pkce(challenge, "S256");
        let code = server.create_authorization_code(&req, 5).unwrap();

        // 全参数正确
        let ok = TokenRequest::new(&code.code, "https://app.com/cb", "client1")
            .with_client_secret("secret1")
            .with_code_verifier(verifier.clone());
        assert!(server.exchange_code(&ok).is_ok());

        // secret 错误 + verifier 正确
        let bad_secret = TokenRequest::new(&code.code, "https://app.com/cb", "client1")
            .with_client_secret("nope")
            .with_code_verifier(verifier);
        assert!(server.exchange_code(&bad_secret).is_err());
    }
}
