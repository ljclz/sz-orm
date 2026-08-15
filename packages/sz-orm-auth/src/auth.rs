use crate::error::AuthError;
use crate::jwt::{JwtClaims, JwtEncoder};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// 密码验证器 trait（v0.2.1 新增，修复 Critical S-1）
///
/// `JwtAuthenticator::authenticate` 调用此 trait 验证密码并获取 user_id。
/// 调用方负责实现真实的密码哈希校验（如 bcrypt/argon2）和用户查询。
///
/// # 示例
///
/// ```ignore
/// use sz_orm_auth::{PasswordVerifier, AuthError};
///
/// struct DbPasswordVerifier;
///
/// impl PasswordVerifier for DbPasswordVerifier {
///     fn verify_password(&self, username: &str, password: &str) -> Result<i64, AuthError> {
///         // 1. 查询数据库获取 stored_hash 和 user_id
///         // 2. 用 bcrypt::verify(password, &stored_hash) 校验
///         // 3. 校验通过返回 Ok(user_id)，否则 Err(AuthError::InvalidCredentials(...))
///         # unimplemented!()
///     }
/// }
/// ```
pub trait PasswordVerifier: Send + Sync {
    /// 验证密码并返回 user_id
    ///
    /// # 返回
    /// - `Ok(user_id)`：密码正确，返回用户 ID
    /// - `Err(AuthError::InvalidCredentials(_))`：密码错误或用户不存在
    fn verify_password(&self, username: &str, password: &str) -> Result<i64, AuthError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

impl Credentials {
    pub fn new(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self {
            username: username.into(),
            password: password.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub token_type: String,
    pub expires_in: u64,
    pub issued_at: i64,
}

impl Token {
    pub fn new(access_token: impl Into<String>, expires_in: u64) -> Self {
        Self {
            access_token: access_token.into(),
            refresh_token: None,
            token_type: "Bearer".to_string(),
            expires_in,
            issued_at: current_timestamp(),
        }
    }

    pub fn with_refresh(mut self, refresh_token: impl Into<String>) -> Self {
        self.refresh_token = Some(refresh_token.into());
        self
    }

    pub fn is_expired(&self) -> bool {
        let now = current_timestamp();
        let expiry = self.issued_at + (self.expires_in as i64 * 1000);
        now > expiry
    }

    pub fn expires_at(&self) -> i64 {
        self.issued_at + (self.expires_in as i64 * 1000)
    }
}

fn current_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn current_timestamp_secs() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

pub struct User {
    pub id: i64,
    pub username: String,
    pub email: Option<String>,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl User {
    pub fn new(id: i64, username: impl Into<String>) -> Self {
        Self {
            id,
            username: username.into(),
            email: None,
            roles: Vec::new(),
            permissions: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    pub fn with_email(mut self, email: impl Into<String>) -> Self {
        self.email = Some(email.into());
        self
    }

    pub fn with_roles(mut self, roles: Vec<String>) -> Self {
        self.roles = roles;
        self
    }

    pub fn with_permissions(mut self, permissions: Vec<String>) -> Self {
        self.permissions = permissions;
        self
    }

    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }

    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.iter().any(|p| p == permission) || self.has_role("admin")
    }
}

/// Authenticator that issues and verifies real HS256 JWTs.
pub struct JwtAuthenticator {
    encoder: JwtEncoder,
    issuer: String,
    expiration: u64,
    /// 可选密码验证器（v0.2.1 新增，修复 Critical S-1）
    ///
    /// - `Some(verifier)`：`authenticate` 调用 `verifier.verify_password` 获取 user_id
    /// - `None`：保留旧行为（不验证密码），但 `eprintln!` 警告生产环境必须配置
    password_verifier: Option<Arc<dyn PasswordVerifier>>,
}

impl JwtAuthenticator {
    /// HS256 最小密钥长度（v4.8.0 修复 M-15）
    ///
    /// 修复前默认路径无密钥强度校验——短密钥/常见口令可被 GPU 离线爆破
    /// （数十亿次/秒），攻击者获取任一令牌即可枚举密钥并任意伪造 claims。
    pub const MIN_SECRET_LEN: usize = 32;

    /// 创建认证器（兼容旧签名，不做密钥强度校验）
    ///
    /// **生产环境必须使用 [`JwtAuthenticator::try_new`]**——本构造器保留
    /// 兼容性（测试/低风险场景），弱密钥风险见 M-15。
    pub fn new(secret: impl Into<String>, issuer: impl Into<String>, expiration: u64) -> Self {
        Self {
            encoder: JwtEncoder::new(secret),
            issuer: issuer.into(),
            expiration,
            password_verifier: None,
        }
    }

    /// 创建认证器并强制校验密钥强度（v4.8.0 修复 M-15）
    ///
    /// secret 长度必须 ≥ [`JwtAuthenticator::MIN_SECRET_LEN`]（32 字节），
    /// 否则返回 [`AuthError::SecretTooShort`]。生产路径应使用本构造器。
    pub fn try_new(
        secret: impl Into<String>,
        issuer: impl Into<String>,
        expiration: u64,
    ) -> Result<Self, AuthError> {
        let secret = secret.into();
        if secret.len() < Self::MIN_SECRET_LEN {
            return Err(AuthError::SecretTooShort(format!(
                "JWT secret must be at least {} bytes (got {})",
                Self::MIN_SECRET_LEN,
                secret.len()
            )));
        }
        Ok(Self {
            encoder: JwtEncoder::new(secret),
            issuer: issuer.into(),
            expiration,
            password_verifier: None,
        })
    }

    /// 配置密码验证器（v0.2.1 新增，修复 Critical S-1）
    ///
    /// 配置后，`authenticate` 会调用 `verifier.verify_password` 验证密码并获取 user_id。
    /// **生产环境必须调用此方法**，否则 `authenticate` 不验证密码（Critical S-1）。
    pub fn with_password_verifier(mut self, verifier: Arc<dyn PasswordVerifier>) -> Self {
        self.password_verifier = Some(verifier);
        self
    }

    pub fn authenticate(&self, credentials: &Credentials) -> Result<Token, AuthError> {
        if credentials.username.is_empty() || credentials.password.is_empty() {
            return Err(AuthError::InvalidCredentials(
                "Username or password is empty".to_string(),
            ));
        }

        // v1.2.1 修复 High H-1（CWE-1188 不安全默认初始化 / CWE-287 认证不当）：
        // 未配置 `password_verifier` 时直接返回 `Err`，拒绝签发 JWT。
        // 原实现（v0.2.1）仅 `eprintln!` 警告后接受任意凭证并签发 `user_id=0` 的 JWT，
        // 开发者遗漏配置时将导致完全认证绕过。stderr 警告在生产环境常被忽略。
        let user_id: i64 = match &self.password_verifier {
            Some(verifier) => {
                verifier.verify_password(&credentials.username, &credentials.password)?
            }
            None => {
                return Err(AuthError::Config(
                    "JwtAuthenticator.password_verifier not configured; \
                     call with_password_verifier() before authenticate() (H-1)"
                        .to_string(),
                ));
            }
        };

        let exp = current_timestamp_secs() + (self.expiration as i64);
        // v4.8.0 修复 Critical C-2：访问/刷新令牌必须带 token_use 类型声明，
        // 防止窃取的访问令牌被 refresh 端点接受导致无限续期（黑帽实证）。
        let claims = JwtClaims::new(credentials.username.clone(), exp)
            .with_issuer(self.issuer.clone())
            .with_roles(vec!["user".to_string()])
            .with_user_id(user_id)
            .with_token_use("access");

        let access_token = self.encoder.encode(&claims)?;
        let refresh_claims = JwtClaims::new(credentials.username.clone(), exp + 86400)
            .with_issuer(self.issuer.clone())
            .with_user_id(user_id)
            .with_token_use("refresh");
        let refresh_token = self.encoder.encode(&refresh_claims)?;

        Ok(Token::new(access_token, self.expiration).with_refresh(refresh_token))
    }

    pub fn verify_token(&self, token: &str) -> Result<User, AuthError> {
        if token.is_empty() {
            return Err(AuthError::TokenInvalid("Token is empty".to_string()));
        }

        let claims = self.encoder.decode(token)?;
        // v0.2.1 修复 Critical S-2：从 claims.user_id 恢复正确的 user.id
        let user_id = claims.user_id.unwrap_or_else(|| {
            eprintln!(
                "[warn] JwtAuthenticator::verify_token: token has no user_id claim \
                 (legacy token); falling back to user.id=0 (Critical S-2)"
            );
            0
        });
        let user = User::new(user_id, claims.sub.clone())
            .with_roles(claims.roles.clone())
            .with_permissions(claims.permissions.clone());
        Ok(user)
    }

    pub fn refresh_token(&self, refresh_token: &str) -> Result<Token, AuthError> {
        if refresh_token.is_empty() {
            return Err(AuthError::TokenInvalid(
                "Refresh token is empty".to_string(),
            ));
        }

        let claims = self.encoder.decode(refresh_token)?;
        // v4.8.0 修复 Critical C-2：严格校验令牌类型——仅接受显式声明
        // token_use="refresh" 的令牌。旧版无类型声明的令牌一律拒绝
        //（访问令牌送入 refresh 端点的类型混淆攻击面被切断）。
        if !claims.is_token_use("refresh") {
            return Err(AuthError::TokenInvalid(
                "Token is not a refresh token (missing token_use=refresh)".to_string(),
            ));
        }

        let now = current_timestamp_secs();
        let new_exp = now + (self.expiration as i64);
        let new_claims = JwtClaims::new(claims.sub, new_exp)
            .with_issuer(self.issuer.clone())
            .with_roles(claims.roles)
            .with_permissions(claims.permissions)
            .with_user_id(claims.user_id.unwrap_or(0))
            .with_token_use("access");
        let new_access_token = self.encoder.encode(&new_claims)?;

        Ok(Token::new(new_access_token, self.expiration))
    }
}

/// Legacy claims struct kept for backward compatibility with the public API.
pub struct Claims {
    pub sub: String,
    pub exp: i64,
    pub iat: i64,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
}

impl Claims {
    pub fn new(subject: impl Into<String>) -> Self {
        Self {
            sub: subject.into(),
            exp: 0,
            iat: current_timestamp(),
            roles: Vec::new(),
            permissions: Vec::new(),
        }
    }

    pub fn with_roles(mut self, roles: Vec<String>) -> Self {
        self.roles = roles;
        self
    }

    pub fn with_permissions(mut self, permissions: Vec<String>) -> Self {
        self.permissions = permissions;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用密码验证器：用户名哈希后取低 32 位作为 user_id（非 0）。
    ///
    /// v1.2.1 修复 H-1 后，`JwtAuthenticator::authenticate` 在未配置 verifier 时
    /// 返回 `Err(AuthError::Config)`，所有调用 `authenticate` 的测试必须配置 verifier。
    struct MockPasswordVerifier;
    impl PasswordVerifier for MockPasswordVerifier {
        fn verify_password(&self, username: &str, _password: &str) -> Result<i64, AuthError> {
            // 简单确定性映射：用户名首字节 + 长度，保证 > 0
            let id = (username.bytes().next().unwrap_or(b'x') as i64) + (username.len() as i64);
            Ok(id)
        }
    }

    /// 构造配置了 MockPasswordVerifier 的 JwtAuthenticator
    fn auth_with_verifier(secret: &str, issuer: &str, exp: u64) -> JwtAuthenticator {
        JwtAuthenticator::new(secret, issuer, exp)
            .with_password_verifier(std::sync::Arc::new(MockPasswordVerifier))
    }

    #[test]
    fn test_credentials_new() {
        let creds = Credentials::new("user", "pass");
        assert_eq!(creds.username, "user");
        assert_eq!(creds.password, "pass");
    }

    #[test]
    fn test_token_new() {
        let token = Token::new("access_token", 3600);
        assert_eq!(token.access_token, "access_token");
        assert_eq!(token.expires_in, 3600);
        assert_eq!(token.token_type, "Bearer");
        assert!(token.refresh_token.is_none());
    }

    #[test]
    fn test_token_with_refresh() {
        let token = Token::new("access", 3600).with_refresh("refresh_token");
        assert_eq!(token.refresh_token, Some("refresh_token".to_string()));
    }

    #[test]
    fn test_token_is_expired() {
        // 使用 expires_in=1（秒）避免时序竞争：expires_in=0 时 expiry=issued_at，
        // 任何毫秒级延迟都会导致 now > expiry，使测试不稳定。
        let mut token = Token::new("test", 1);
        assert!(!token.is_expired());

        token.issued_at = current_timestamp() - 100_000;
        token.expires_in = 1;
        assert!(token.is_expired());
    }

    #[test]
    fn test_user_new() {
        let user = User::new(1, "username");
        assert_eq!(user.id, 1);
        assert_eq!(user.username, "username");
        assert!(user.email.is_none());
    }

    #[test]
    fn test_user_with_email() {
        let user = User::new(1, "user").with_email("user@test.com");
        assert_eq!(user.email, Some("user@test.com".to_string()));
    }

    #[test]
    fn test_user_with_roles() {
        let user = User::new(1, "user").with_roles(vec!["admin".to_string(), "user".to_string()]);
        assert!(user.has_role("admin"));
        assert!(user.has_role("user"));
        assert!(!user.has_role("guest"));
    }

    #[test]
    fn test_user_has_permission() {
        let user =
            User::new(1, "user").with_permissions(vec!["read".to_string(), "write".to_string()]);

        assert!(user.has_permission("read"));
        assert!(user.has_permission("write"));
        assert!(!user.has_permission("delete"));
    }

    #[test]
    fn test_user_admin_has_all() {
        let user = User::new(1, "admin").with_roles(vec!["admin".to_string()]);

        assert!(user.has_permission("anything"));
        assert!(user.has_permission("delete"));
    }

    // ---- Real JWT authenticator tests ----

    #[test]
    fn test_jwt_authenticate_issues_real_jwt() {
        let auth = auth_with_verifier("super-secret", "test-issuer", 3600);
        let creds = Credentials::new("alice", "password123");

        let token = auth.authenticate(&creds).expect("authenticate");
        assert!(!token.access_token.is_empty());
        assert_eq!(token.token_type, "Bearer");
        assert_eq!(token.expires_in, 3600);
        assert!(token.refresh_token.is_some());

        // access_token must be a real 3-part JWT
        let parts: Vec<&str> = token.access_token.split('.').collect();
        assert_eq!(parts.len(), 3, "access token must be a 3-part JWT");
        let refresh_parts: Vec<&str> = token.refresh_token.as_ref().unwrap().split('.').collect();
        assert_eq!(refresh_parts.len(), 3, "refresh token must be a 3-part JWT");
    }

    #[test]
    fn test_jwt_authenticate_rejects_empty_credentials() {
        let auth = JwtAuthenticator::new("secret", "issuer", 3600);
        let creds = Credentials::new("", "");
        let result = auth.authenticate(&creds);
        assert!(matches!(result, Err(AuthError::InvalidCredentials(_))));
    }

    #[test]
    fn test_jwt_verify_roundtrip() {
        let auth = auth_with_verifier("super-secret", "test-issuer", 3600);
        let creds = Credentials::new("bob", "pw");

        let token = auth.authenticate(&creds).expect("authenticate");
        let user = auth.verify_token(&token.access_token).expect("verify");

        assert_eq!(user.username, "bob");
        assert!(user.has_role("user"));
    }

    #[test]
    fn test_jwt_verify_rejects_garbage() {
        let auth = JwtAuthenticator::new("secret", "issuer", 3600);
        let result = auth.verify_token("not.a.jwt");
        assert!(matches!(result, Err(AuthError::TokenInvalid(_))));
    }

    #[test]
    fn test_jwt_verify_rejects_empty() {
        let auth = JwtAuthenticator::new("secret", "issuer", 3600);
        let result = auth.verify_token("");
        assert!(matches!(result, Err(AuthError::TokenInvalid(_))));
    }

    #[test]
    fn test_jwt_verify_rejects_wrong_secret() {
        let auth_a = auth_with_verifier("secret-a", "issuer", 3600);
        let auth_b = JwtAuthenticator::new("secret-b", "issuer", 3600);

        let token = auth_a
            .authenticate(&Credentials::new("user", "pw"))
            .unwrap();
        let result = auth_b.verify_token(&token.access_token);
        assert!(matches!(result, Err(AuthError::TokenInvalid(_))));
    }

    #[test]
    fn test_jwt_refresh_roundtrip() {
        let auth = auth_with_verifier("super-secret", "issuer", 3600);
        let token = auth.authenticate(&Credentials::new("carol", "pw")).unwrap();

        let refreshed = auth
            .refresh_token(token.refresh_token.as_ref().unwrap())
            .expect("refresh");
        // New access token must be a valid JWT and decode to the same subject
        let user = auth
            .verify_token(&refreshed.access_token)
            .expect("verify refreshed");
        assert_eq!(user.username, "carol");
    }

    #[test]
    fn test_jwt_refresh_rejects_empty() {
        let auth = JwtAuthenticator::new("secret", "issuer", 3600);
        let result = auth.refresh_token("");
        assert!(matches!(result, Err(AuthError::TokenInvalid(_))));
    }

    #[test]
    fn test_jwt_refresh_rejects_garbage() {
        let auth = JwtAuthenticator::new("secret", "issuer", 3600);
        let result = auth.refresh_token("garbage");
        assert!(matches!(result, Err(AuthError::TokenInvalid(_))));
    }

    #[test]
    fn test_jwt_claims_carried_to_user() {
        let auth = auth_with_verifier("secret", "issuer", 3600);
        let token = auth.authenticate(&Credentials::new("dave", "pw")).unwrap();
        let user = auth.verify_token(&token.access_token).unwrap();
        // authenticate() grants "user" role by default
        assert_eq!(user.roles, vec!["user".to_string()]);
    }

    /// v1.2.1 新增：验证 H-1 修复 — 未配置 password_verifier 时 authenticate() 必须返回 Err
    #[test]
    fn test_jwt_authenticate_rejects_when_verifier_not_configured() {
        let auth = JwtAuthenticator::new("secret", "issuer", 3600);
        let creds = Credentials::new("alice", "password123");
        let result = auth.authenticate(&creds);
        assert!(
            matches!(result, Err(AuthError::Config(_))),
            "expected Err(AuthError::Config) when password_verifier is None, got {:?}",
            result
        );
    }
}
