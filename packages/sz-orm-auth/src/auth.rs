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
    pub fn new(secret: impl Into<String>, issuer: impl Into<String>, expiration: u64) -> Self {
        Self {
            encoder: JwtEncoder::new(secret),
            issuer: issuer.into(),
            expiration,
            password_verifier: None,
        }
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

        // v0.2.1 修复 Critical S-1：必须通过 PasswordVerifier 验证密码
        let user_id: i64 = if let Some(verifier) = &self.password_verifier {
            verifier.verify_password(&credentials.username, &credentials.password)?
        } else {
            // 未配置 verifier：保留旧行为（向后兼容）但警告
            // 生产环境必须通过 with_password_verifier() 配置 verifier
            eprintln!(
                "[warn] JwtAuthenticator::authenticate: password_verifier not configured; \
                 accepting credentials without password verification (Critical S-1)"
            );
            0
        };

        let exp = current_timestamp_secs() + (self.expiration as i64);
        let claims = JwtClaims::new(credentials.username.clone(), exp)
            .with_issuer(self.issuer.clone())
            .with_roles(vec!["user".to_string()])
            .with_user_id(user_id);

        let access_token = self.encoder.encode(&claims)?;
        let refresh_claims = JwtClaims::new(credentials.username.clone(), exp + 86400)
            .with_issuer(self.issuer.clone())
            .with_user_id(user_id);
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
        let now = current_timestamp_secs();
        let new_exp = now + (self.expiration as i64);
        let new_claims = JwtClaims::new(claims.sub, new_exp)
            .with_issuer(self.issuer.clone())
            .with_roles(claims.roles)
            .with_permissions(claims.permissions)
            .with_user_id(claims.user_id.unwrap_or(0));
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
        let mut token = Token::new("test", 0);
        // Brand-new token with expires_in=0: expiry = issued_at + 0 = issued_at.
        // is_expired checks `now > expiry`, which is unlikely to be > issued_at within the same millisecond.
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
        let auth = JwtAuthenticator::new("super-secret", "test-issuer", 3600);
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
        let auth = JwtAuthenticator::new("super-secret", "test-issuer", 3600);
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
        let auth_a = JwtAuthenticator::new("secret-a", "issuer", 3600);
        let auth_b = JwtAuthenticator::new("secret-b", "issuer", 3600);

        let token = auth_a
            .authenticate(&Credentials::new("user", "pw"))
            .unwrap();
        let result = auth_b.verify_token(&token.access_token);
        assert!(matches!(result, Err(AuthError::TokenInvalid(_))));
    }

    #[test]
    fn test_jwt_refresh_roundtrip() {
        let auth = JwtAuthenticator::new("super-secret", "issuer", 3600);
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
        let auth = JwtAuthenticator::new("secret", "issuer", 3600);
        let token = auth.authenticate(&Credentials::new("dave", "pw")).unwrap();
        let user = auth.verify_token(&token.access_token).unwrap();
        // authenticate() grants "user" role by default
        assert_eq!(user.roles, vec!["user".to_string()]);
    }
}
