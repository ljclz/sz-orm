//! 认证支持：认证配置、令牌验证、角色检查。
//!
//! - [`AuthConfig`] — 认证配置（令牌头名、过期时间、签发者）
//! - [`TokenValidator`] — 令牌验证器（格式检查、过期检查）
//! - [`RoleChecker`] — 角色权限检查器
//! - [`AuthResult`] — 认证结果

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

// ============================================================================
// 认证结果
// ============================================================================

/// 认证结果
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthResult {
    authenticated: bool,
    user_id: Option<String>,
    roles: Vec<String>,
    error: Option<String>,
}

impl AuthResult {
    /// 创建成功认证结果
    pub fn success(user_id: String, roles: Vec<String>) -> Self {
        Self {
            authenticated: true,
            user_id: Some(user_id),
            roles,
            error: None,
        }
    }

    /// 创建失败认证结果
    pub fn failure(error: String) -> Self {
        Self {
            authenticated: false,
            user_id: None,
            roles: vec![],
            error: Some(error),
        }
    }

    /// 创建匿名（未认证）结果
    pub fn anonymous() -> Self {
        Self {
            authenticated: false,
            user_id: None,
            roles: vec![],
            error: None,
        }
    }

    /// 是否已认证
    pub fn is_authenticated(&self) -> bool {
        self.authenticated
    }

    /// 用户 ID
    pub fn user_id(&self) -> Option<&str> {
        self.user_id.as_deref()
    }

    /// 角色列表
    pub fn roles(&self) -> &[String] {
        &self.roles
    }

    /// 错误信息
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// 是否有指定角色
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }

    /// 是否有任一指定角色
    pub fn has_any_role(&self, roles: &[&str]) -> bool {
        roles.iter().any(|r| self.has_role(r))
    }
}

// ============================================================================
// 认证配置
// ============================================================================

/// 认证配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    token_header: String,
    token_prefix: String,
    issuer: String,
    expiry_secs: u64,
    refresh_threshold_secs: u64,
    required_roles: HashSet<String>,
    public_paths: HashSet<String>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            token_header: "Authorization".to_string(),
            token_prefix: "Bearer ".to_string(),
            issuer: "sz-orm".to_string(),
            expiry_secs: 3600,
            refresh_threshold_secs: 300,
            required_roles: HashSet::new(),
            public_paths: HashSet::new(),
        }
    }
}

impl AuthConfig {
    /// 创建默认配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置令牌头名（链式）
    pub fn with_token_header(mut self, header: &str) -> Self {
        self.token_header = header.to_string();
        self
    }

    /// 设置令牌前缀（链式）
    pub fn with_token_prefix(mut self, prefix: &str) -> Self {
        self.token_prefix = prefix.to_string();
        self
    }

    /// 设置签发者（链式）
    pub fn with_issuer(mut self, issuer: &str) -> Self {
        self.issuer = issuer.to_string();
        self
    }

    /// 设置过期时间（秒，链式）
    pub fn with_expiry(mut self, secs: u64) -> Self {
        self.expiry_secs = secs;
        self
    }

    /// 设置刷新阈值（秒，链式）
    pub fn with_refresh_threshold(mut self, secs: u64) -> Self {
        self.refresh_threshold_secs = secs;
        self
    }

    /// 添加必需角色（链式）
    pub fn with_required_role(mut self, role: &str) -> Self {
        self.required_roles.insert(role.to_string());
        self
    }

    /// 添加公共路径（不需要认证，链式）
    pub fn with_public_path(mut self, path: &str) -> Self {
        self.public_paths.insert(path.to_string());
        self
    }

    /// 令牌头名
    pub fn token_header(&self) -> &str {
        &self.token_header
    }

    /// 令牌前缀
    pub fn token_prefix(&self) -> &str {
        &self.token_prefix
    }

    /// 签发者
    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    /// 过期时间
    pub fn expiry_secs(&self) -> u64 {
        self.expiry_secs
    }

    /// 刷新阈值
    pub fn refresh_threshold_secs(&self) -> u64 {
        self.refresh_threshold_secs
    }

    /// 是否是公共路径
    pub fn is_public_path(&self, path: &str) -> bool {
        self.public_paths.contains(path)
    }

    /// 必需角色数
    pub fn required_role_count(&self) -> usize {
        self.required_roles.len()
    }

    /// 检查是否满足角色要求
    pub fn meets_role_requirements(&self, roles: &[String]) -> bool {
        if self.required_roles.is_empty() {
            return true;
        }
        self.required_roles
            .iter()
            .all(|required| roles.contains(required))
    }

    /// 从 Authorization 头提取令牌
    pub fn extract_token(&self, header_value: &str) -> Option<String> {
        if self.token_prefix.is_empty() {
            return Some(header_value.to_string());
        }
        header_value
            .strip_prefix(&self.token_prefix)
            .map(|s| s.to_string())
    }

    /// 是否需要刷新令牌
    pub fn needs_refresh(&self, remaining_secs: u64) -> bool {
        remaining_secs <= self.refresh_threshold_secs
    }
}

// ============================================================================
// 令牌验证器
// ============================================================================

/// 令牌验证器：检查令牌格式和过期
#[derive(Debug, Clone)]
pub struct TokenValidator {
    config: AuthConfig,
}

impl TokenValidator {
    /// 创建令牌验证器
    pub fn new(config: AuthConfig) -> Self {
        Self { config }
    }

    /// 从配置创建
    pub fn with_defaults() -> Self {
        Self::new(AuthConfig::new())
    }

    /// 验证令牌格式
    pub fn validate_format(&self, token: &str) -> Result<(), String> {
        if token.is_empty() {
            return Err("token is empty".to_string());
        }
        if token.len() < 20 {
            return Err("token too short".to_string());
        }
        if !token
            .chars()
            .all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == '_')
        {
            return Err("token contains invalid characters".to_string());
        }
        Ok(())
    }

    /// 验证令牌过期
    pub fn validate_expiry(&self, issued_at: u64, now: u64) -> Result<(), String> {
        if now < issued_at {
            return Err("token issued in the future".to_string());
        }
        let elapsed = now - issued_at;
        if elapsed > self.config.expiry_secs {
            return Err("token expired".to_string());
        }
        Ok(())
    }

    /// 完整验证（格式 + 过期）
    pub fn validate(&self, token: &str, issued_at: u64, now: u64) -> Result<(), String> {
        self.validate_format(token)?;
        self.validate_expiry(issued_at, now)?;
        Ok(())
    }

    /// 从 Authorization 头验证
    pub fn validate_header(
        &self,
        header_value: &str,
        issued_at: u64,
        now: u64,
    ) -> Result<(), String> {
        let token = self
            .config
            .extract_token(header_value)
            .ok_or_else(|| "invalid authorization header format".to_string())?;
        self.validate(&token, issued_at, now)
    }

    /// 获取配置引用
    pub fn config(&self) -> &AuthConfig {
        &self.config
    }
}

// ============================================================================
// 角色检查器
// ============================================================================

/// 角色权限检查器
#[derive(Debug, Clone, Default)]
pub struct RoleChecker {
    role_permissions: HashMap<String, HashSet<String>>,
}

impl RoleChecker {
    /// 创建空角色检查器
    pub fn new() -> Self {
        Self::default()
    }

    /// 为角色添加权限（链式）
    pub fn with_role_permission(mut self, role: &str, permission: &str) -> Self {
        self.role_permissions
            .entry(role.to_string())
            .or_default()
            .insert(permission.to_string());
        self
    }

    /// 为角色设置权限集合（链式）
    pub fn with_role_permissions(mut self, role: &str, permissions: Vec<String>) -> Self {
        self.role_permissions
            .insert(role.to_string(), permissions.into_iter().collect());
        self
    }

    /// 检查角色是否有权限
    pub fn has_permission(&self, role: &str, permission: &str) -> bool {
        self.role_permissions
            .get(role)
            .map(|perms| perms.contains(permission))
            .unwrap_or(false)
    }

    /// 检查角色集合是否有权限（任一角色有权限即可）
    pub fn any_has_permission(&self, roles: &[String], permission: &str) -> bool {
        roles.iter().any(|r| self.has_permission(r, permission))
    }

    /// 获取角色所有权限
    pub fn role_permissions(&self, role: &str) -> Vec<&str> {
        self.role_permissions
            .get(role)
            .map(|perms| perms.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }

    /// 注册的角色数
    pub fn role_count(&self) -> usize {
        self.role_permissions.len()
    }

    /// 检查认证结果是否有权限
    pub fn check_auth_result(&self, auth: &AuthResult, permission: &str) -> bool {
        if !auth.is_authenticated() {
            return false;
        }
        self.any_has_permission(auth.roles(), permission)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ----- AuthResult -----

    #[test]
    fn auth_result_success() {
        let result = AuthResult::success("user123".to_string(), vec!["admin".to_string()]);
        assert!(result.is_authenticated());
        assert_eq!(result.user_id(), Some("user123"));
        assert_eq!(result.roles().len(), 1);
        assert!(result.has_role("admin"));
        assert!(!result.has_role("user"));
    }

    #[test]
    fn auth_result_failure() {
        let result = AuthResult::failure("invalid token".to_string());
        assert!(!result.is_authenticated());
        assert_eq!(result.error(), Some("invalid token"));
    }

    #[test]
    fn auth_result_anonymous() {
        let result = AuthResult::anonymous();
        assert!(!result.is_authenticated());
        assert!(result.error().is_none());
    }

    #[test]
    fn auth_result_has_any_role() {
        let result = AuthResult::success(
            "u".to_string(),
            vec!["admin".to_string(), "user".to_string()],
        );
        assert!(result.has_any_role(&["admin", "superadmin"]));
        assert!(!result.has_any_role(&["guest", "anonymous"]));
    }

    // ----- AuthConfig -----

    #[test]
    fn auth_config_default() {
        let config = AuthConfig::new();
        assert_eq!(config.token_header(), "Authorization");
        assert_eq!(config.token_prefix(), "Bearer ");
        assert_eq!(config.issuer(), "sz-orm");
        assert_eq!(config.expiry_secs(), 3600);
    }

    #[test]
    fn auth_config_custom_header() {
        let config = AuthConfig::new().with_token_header("X-API-Key");
        assert_eq!(config.token_header(), "X-API-Key");
    }

    #[test]
    fn auth_config_custom_prefix() {
        let config = AuthConfig::new().with_token_prefix("Token ");
        assert_eq!(config.token_prefix(), "Token ");
    }

    #[test]
    fn auth_config_extract_token_bearer() {
        let config = AuthConfig::new();
        let token = config.extract_token("Bearer abc123");
        assert_eq!(token, Some("abc123".to_string()));
    }

    #[test]
    fn auth_config_extract_token_no_prefix() {
        let config = AuthConfig::new().with_token_prefix("");
        let token = config.extract_token("abc123");
        assert_eq!(token, Some("abc123".to_string()));
    }

    #[test]
    fn auth_config_extract_token_invalid() {
        let config = AuthConfig::new();
        assert!(config.extract_token("abc123").is_none());
    }

    #[test]
    fn auth_config_public_path() {
        let config = AuthConfig::new().with_public_path("/health");
        assert!(config.is_public_path("/health"));
        assert!(!config.is_public_path("/api/users"));
    }

    #[test]
    fn auth_config_required_roles() {
        let config = AuthConfig::new()
            .with_required_role("admin")
            .with_required_role("editor");
        assert_eq!(config.required_role_count(), 2);
        assert!(config.meets_role_requirements(&["admin".to_string(), "editor".to_string()]));
        assert!(!config.meets_role_requirements(&["admin".to_string()]));
    }

    #[test]
    fn auth_config_no_required_roles() {
        let config = AuthConfig::new();
        assert!(config.meets_role_requirements(&[]));
    }

    #[test]
    fn auth_config_needs_refresh() {
        let config = AuthConfig::new().with_refresh_threshold(300);
        assert!(config.needs_refresh(200));
        assert!(config.needs_refresh(300));
        assert!(!config.needs_refresh(301));
    }

    // ----- TokenValidator -----

    #[test]
    fn token_validator_format_valid() {
        let validator = TokenValidator::with_defaults();
        assert!(validator
            .validate_format("abcdefghijklmnopqrstuvwxyz123456")
            .is_ok());
    }

    #[test]
    fn token_validator_format_empty() {
        let validator = TokenValidator::with_defaults();
        assert!(validator.validate_format("").is_err());
    }

    #[test]
    fn token_validator_format_too_short() {
        let validator = TokenValidator::with_defaults();
        assert!(validator.validate_format("short").is_err());
    }

    #[test]
    fn token_validator_format_invalid_chars() {
        let validator = TokenValidator::with_defaults();
        assert!(validator
            .validate_format("token with spaces and stuff!!")
            .is_err());
    }

    #[test]
    fn token_validator_expiry_valid() {
        let validator = TokenValidator::with_defaults();
        assert!(validator.validate_expiry(1000, 2000).is_ok());
    }

    #[test]
    fn token_validator_expiry_expired() {
        let validator = TokenValidator::with_defaults();
        assert!(validator.validate_expiry(0, 4000).is_err());
    }

    #[test]
    fn token_validator_expiry_future() {
        let validator = TokenValidator::with_defaults();
        assert!(validator.validate_expiry(2000, 1000).is_err());
    }

    #[test]
    fn token_validator_full_valid() {
        let validator = TokenValidator::with_defaults();
        assert!(validator
            .validate("abcdefghijklmnopqrstuvwxyz123456", 1000, 2000)
            .is_ok());
    }

    #[test]
    fn token_validator_header_valid() {
        let validator = TokenValidator::with_defaults();
        let header = "Bearer abcdefghijklmnopqrstuvwxyz123456";
        assert!(validator.validate_header(header, 1000, 2000).is_ok());
    }

    #[test]
    fn token_validator_header_invalid_format() {
        let validator = TokenValidator::with_defaults();
        assert!(validator
            .validate_header("InvalidHeader", 1000, 2000)
            .is_err());
    }

    // ----- RoleChecker -----

    #[test]
    fn role_checker_empty() {
        let checker = RoleChecker::new();
        assert_eq!(checker.role_count(), 0);
        assert!(!checker.has_permission("admin", "read"));
    }

    #[test]
    fn role_checker_with_permission() {
        let checker = RoleChecker::new()
            .with_role_permission("admin", "read")
            .with_role_permission("admin", "write");
        assert!(checker.has_permission("admin", "read"));
        assert!(checker.has_permission("admin", "write"));
        assert!(!checker.has_permission("admin", "delete"));
        assert_eq!(checker.role_count(), 1);
    }

    #[test]
    fn role_checker_with_permissions_batch() {
        let checker = RoleChecker::new()
            .with_role_permissions("editor", vec!["read".to_string(), "write".to_string()]);
        assert!(checker.has_permission("editor", "read"));
        assert!(checker.has_permission("editor", "write"));
    }

    #[test]
    fn role_checker_any_has_permission() {
        let checker = RoleChecker::new()
            .with_role_permission("admin", "delete")
            .with_role_permission("user", "read");
        let roles = vec!["user".to_string(), "guest".to_string()];
        assert!(checker.any_has_permission(&roles, "read"));
        assert!(!checker.any_has_permission(&roles, "delete"));
    }

    #[test]
    fn role_checker_role_permissions() {
        let checker = RoleChecker::new()
            .with_role_permission("admin", "read")
            .with_role_permission("admin", "write");
        let perms = checker.role_permissions("admin");
        assert_eq!(perms.len(), 2);
    }

    #[test]
    fn role_checker_role_permissions_nonexistent() {
        let checker = RoleChecker::new();
        let perms = checker.role_permissions("nonexistent");
        assert!(perms.is_empty());
    }

    #[test]
    fn role_checker_check_auth_result() {
        let checker = RoleChecker::new().with_role_permission("admin", "delete");
        let auth = AuthResult::success("u".to_string(), vec!["admin".to_string()]);
        assert!(checker.check_auth_result(&auth, "delete"));
        assert!(!checker.check_auth_result(&auth, "execute"));
    }

    #[test]
    fn role_checker_check_unauthenticated() {
        let checker = RoleChecker::new().with_role_permission("admin", "delete");
        let auth = AuthResult::anonymous();
        assert!(!checker.check_auth_result(&auth, "delete"));
    }
}
