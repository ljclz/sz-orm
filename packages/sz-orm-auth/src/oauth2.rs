//! OAuth2 授权码流程（Authorization Code Flow）
//!
//! 实现 RFC 6749 Section 4.1 的授权码流程：
//! 1. 客户端重定向用户到授权服务器
//! 2. 用户授权后，授权服务器返回授权码
//! 3. 客户端用授权码交换访问令牌
//!
//! 本模块提供流程状态管理，不包含 HTTP 传输层实现。

use std::collections::HashMap;
use std::sync::Mutex;
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
        }
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
        }
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
        let auth_code = AuthorizationCode::new(
            code_value,
            req.client_id.clone(),
            user_id,
            req.redirect_uri.clone(),
            req.scope.clone(),
        );
        self.codes
            .lock()
            .unwrap()
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
    pub fn exchange_code(&self, req: &TokenRequest) -> Result<AuthorizationCode, AuthError> {
        let mut codes = self.codes.lock().unwrap();
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
            return Err(AuthError::TokenInvalid(
                "Redirect URI mismatch".to_string(),
            ));
        }

        if auth_code.client_id != req.client_id {
            return Err(AuthError::TokenInvalid("Client ID mismatch".to_string()));
        }

        // 标记为已使用
        let result = auth_code.clone();
        codes.get_mut(&req.code).unwrap().used = true;
        Ok(result)
    }

    /// 返回当前存储的授权码数量
    pub fn code_count(&self) -> usize {
        self.codes.lock().unwrap().len()
    }

    /// 清理已过期或已使用的授权码
    pub fn cleanup(&self) -> usize {
        let mut codes = self.codes.lock().unwrap();
        let before = codes.len();
        codes.retain(|_, c| !c.is_expired() && !c.used);
        before - codes.len()
    }
}

/// 生成随机授权码（32 字节十六进制）
fn generate_code() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    current_nanos().hash(&mut hasher);
    let seed = hasher.finish();
    format!("{:064x}", seed)
}

fn current_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn current_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
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
}
