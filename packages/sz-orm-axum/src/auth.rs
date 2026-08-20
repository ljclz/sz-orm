//! 认证与授权配置。
//!
//! - [`AuthConfig`] — 认证配置
//! - [`AuthResult`] — 认证结果
//! - [`TokenValidator`] — Token 验证器
//! - [`RoleChecker`] — 角色检查器

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

// ============================================================================
// AuthResult — 认证结果
// ============================================================================

/// 认证结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct AuthResult {
    authenticated: bool,
    user_id: Option<String>,
    roles: Vec<String>,
    token: Option<String>,
}


impl AuthResult {
    /// 创建匿名（未认证）结果
    pub fn anonymous() -> Self {
        Self::default()
    }

    /// 创建认证成功结果
    pub fn success(user_id: &str, roles: Vec<String>, token: &str) -> Self {
        Self {
            authenticated: true,
            user_id: Some(user_id.to_string()),
            roles,
            token: Some(token.to_string()),
        }
    }

    /// 创建认证失败结果
    pub fn failure() -> Self {
        Self::default()
    }

    /// 是否已认证
    pub fn is_authenticated(&self) -> bool {
        self.authenticated
    }

    /// 用户 ID
    pub fn user_id(&self) -> Option<&str> {
        self.user_id.as_deref()
    }

    /// 角色
    pub fn roles(&self) -> &[String] {
        &self.roles
    }

    /// Token
    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    /// 是否有指定角色
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }

    /// 是否有任一角色
    pub fn has_any_role(&self, roles: &[&str]) -> bool {
        roles.iter().any(|r| self.has_role(r))
    }

    /// 是否有全部角色
    pub fn has_all_roles(&self, roles: &[&str]) -> bool {
        roles.iter().all(|r| self.has_role(r))
    }
}

// ============================================================================
// AuthConfig — 认证配置
// ============================================================================

/// 认证配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    token_header: String,
    token_prefix: String,
    public_paths: HashSet<String>,
    required_roles: Vec<String>,
    custom_header: Option<String>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            token_header: "Authorization".to_string(),
            token_prefix: "Bearer ".to_string(),
            public_paths: HashSet::new(),
            required_roles: vec![],
            custom_header: None,
        }
    }
}

impl AuthConfig {
    /// 创建默认配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置 Token 头名（链式）
    pub fn token_header(mut self, header: &str) -> Self {
        self.token_header = header.to_string();
        self
    }

    /// 设置 Token 前缀（链式）
    pub fn token_prefix(mut self, prefix: &str) -> Self {
        self.token_prefix = prefix.to_string();
        self
    }

    /// 添加公开路径（链式）
    pub fn public_path(mut self, path: &str) -> Self {
        self.public_paths.insert(path.to_string());
        self
    }

    /// 添加必需角色（链式）
    pub fn required_role(mut self, role: &str) -> Self {
        self.required_roles.push(role.to_string());
        self
    }

    /// 设置自定义头（链式）
    pub fn custom_header(mut self, header: &str) -> Self {
        self.custom_header = Some(header.to_string());
        self
    }

    /// Token 头名
    pub fn token_header_value(&self) -> &str {
        &self.token_header
    }

    /// Token 前缀
    pub fn token_prefix_value(&self) -> &str {
        &self.token_prefix
    }

    /// 是否为公开路径
    pub fn is_public_path(&self, path: &str) -> bool {
        self.public_paths.contains(path)
    }

    /// 必需角色
    pub fn required_roles(&self) -> &[String] {
        &self.required_roles
    }

    /// 是否有必需角色
    pub fn has_required_roles(&self) -> bool {
        !self.required_roles.is_empty()
    }

    /// 自定义头
    pub fn custom_header_value(&self) -> Option<&str> {
        self.custom_header.as_deref()
    }

    /// 从 Authorization 头提取 Token
    pub fn extract_token(&self, header_value: &str) -> Option<String> {
        if let Some(rest) = header_value.strip_prefix(&self.token_prefix) {
            Some(rest.to_string())
        } else if self.token_prefix.is_empty() {
            Some(header_value.to_string())
        } else {
            None
        }
    }

    /// 检查认证结果是否满足角色要求
    pub fn check_roles(&self, auth: &AuthResult) -> bool {
        if !self.has_required_roles() {
            return true;
        }
        auth.has_any_role(
            &self
                .required_roles
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>(),
        )
    }
}

// ============================================================================
// TokenValidator — Token 验证器
// ============================================================================

/// Token 验证器
#[derive(Debug, Clone)]
pub struct TokenValidator {
    min_length: usize,
    max_length: usize,
}

impl Default for TokenValidator {
    fn default() -> Self {
        Self {
            min_length: 16,
            max_length: 512,
        }
    }
}

impl TokenValidator {
    /// 创建验证器
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置最小长度（链式）
    pub fn min_length(mut self, n: usize) -> Self {
        self.min_length = n;
        self
    }

    /// 设置最大长度（链式）
    pub fn max_length(mut self, n: usize) -> Self {
        self.max_length = n;
        self
    }

    /// 验证 Token 格式
    pub fn validate_format(&self, token: &str) -> bool {
        let len = token.len();
        (self.min_length..=self.max_length).contains(&len)
    }

    /// 验证 Token 头
    pub fn validate_header(&self, header: &str, config: &AuthConfig) -> Option<String> {
        let token = config.extract_token(header)?;
        if self.validate_format(&token) {
            Some(token)
        } else {
            None
        }
    }
}

// ============================================================================
// RoleChecker — 角色检查器
// ============================================================================

/// 角色检查器
#[derive(Debug, Clone, Default)]
pub struct RoleChecker {
    role_permissions: HashMap<String, HashSet<String>>,
}

impl RoleChecker {
    /// 创建检查器
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加角色权限（链式）
    pub fn role_permission(mut self, role: &str, permission: &str) -> Self {
        self.role_permissions
            .entry(role.to_string())
            .or_default()
            .insert(permission.to_string());
        self
    }

    /// 添加角色权限集合（链式）
    pub fn role_permissions(mut self, role: &str, permissions: Vec<String>) -> Self {
        self.role_permissions
            .entry(role.to_string())
            .or_default()
            .extend(permissions);
        self
    }

    /// 检查角色是否有权限
    pub fn has_permission(&self, role: &str, permission: &str) -> bool {
        self.role_permissions
            .get(role)
            .map(|perms| perms.contains(permission))
            .unwrap_or(false)
    }

    /// 检查是否有任一权限
    pub fn has_any_permission(&self, role: &str, permissions: &[&str]) -> bool {
        permissions.iter().any(|p| self.has_permission(role, p))
    }

    /// 获取角色权限
    pub fn role_permissions_value(&self, role: &str) -> Vec<String> {
        self.role_permissions
            .get(role)
            .map(|perms| perms.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// 检查认证结果是否有权限
    pub fn check_permission(&self, auth: &AuthResult, permission: &str) -> bool {
        if !auth.is_authenticated() {
            return false;
        }
        auth.roles()
            .iter()
            .any(|r| self.has_permission(r, permission))
    }

    /// 检查认证结果是否有任一权限
    pub fn check_any_permission(&self, auth: &AuthResult, permissions: &[&str]) -> bool {
        permissions.iter().any(|p| self.check_permission(auth, p))
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ----- AuthResult -----

    #[test]
    fn auth_result_anonymous() {
        let r = AuthResult::anonymous();
        assert!(!r.is_authenticated());
        assert!(r.user_id().is_none());
        assert!(r.roles().is_empty());
    }

    #[test]
    fn auth_result_success() {
        let r = AuthResult::success("user1", vec!["admin".to_string()], "token123");
        assert!(r.is_authenticated());
        assert_eq!(r.user_id(), Some("user1"));
        assert_eq!(r.token(), Some("token123"));
    }

    #[test]
    fn auth_result_failure() {
        let r = AuthResult::failure();
        assert!(!r.is_authenticated());
    }

    #[test]
    fn auth_result_has_role() {
        let r = AuthResult::success("u", vec!["admin".to_string(), "user".to_string()], "t");
        assert!(r.has_role("admin"));
        assert!(r.has_role("user"));
        assert!(!r.has_role("guest"));
    }

    #[test]
    fn auth_result_has_any_role() {
        let r = AuthResult::success("u", vec!["user".to_string()], "t");
        assert!(r.has_any_role(&["admin", "user"]));
        assert!(!r.has_any_role(&["admin", "guest"]));
    }

    #[test]
    fn auth_result_has_all_roles() {
        let r = AuthResult::success("u", vec!["admin".to_string(), "user".to_string()], "t");
        assert!(r.has_all_roles(&["admin", "user"]));
        assert!(!r.has_all_roles(&["admin", "guest"]));
    }

    // ----- AuthConfig -----

    #[test]
    fn auth_config_default() {
        let c = AuthConfig::new();
        assert_eq!(c.token_header_value(), "Authorization");
        assert_eq!(c.token_prefix_value(), "Bearer ");
    }

    #[test]
    fn auth_config_extract_token() {
        let c = AuthConfig::new();
        assert_eq!(c.extract_token("Bearer abc123"), Some("abc123".to_string()));
        assert_eq!(c.extract_token("abc123"), None);
    }

    #[test]
    fn auth_config_custom_prefix() {
        let c = AuthConfig::new().token_prefix("Token ");
        assert_eq!(c.extract_token("Token abc123"), Some("abc123".to_string()));
    }

    #[test]
    fn auth_config_public_path() {
        let c = AuthConfig::new().public_path("/health");
        assert!(c.is_public_path("/health"));
        assert!(!c.is_public_path("/api"));
    }

    #[test]
    fn auth_config_required_roles() {
        let c = AuthConfig::new().required_role("admin");
        assert!(c.has_required_roles());
        assert_eq!(c.required_roles(), &["admin".to_string()]);
    }

    #[test]
    fn auth_config_check_roles() {
        let c = AuthConfig::new().required_role("admin");
        let auth = AuthResult::success("u", vec!["admin".to_string()], "t");
        assert!(c.check_roles(&auth));
    }

    #[test]
    fn auth_config_check_roles_fail() {
        let c = AuthConfig::new().required_role("admin");
        let auth = AuthResult::success("u", vec!["user".to_string()], "t");
        assert!(!c.check_roles(&auth));
    }

    #[test]
    fn auth_config_no_required_roles() {
        let c = AuthConfig::new();
        let auth = AuthResult::anonymous();
        assert!(c.check_roles(&auth));
    }

    // ----- TokenValidator -----

    #[test]
    fn token_validator_format() {
        let v = TokenValidator::new();
        assert!(v.validate_format("abcdefghijklmnop"));
        assert!(!v.validate_format("short"));
    }

    #[test]
    fn token_validator_custom_length() {
        let v = TokenValidator::new().min_length(5).max_length(10);
        assert!(v.validate_format("12345"));
        assert!(v.validate_format("1234567890"));
        assert!(!v.validate_format("1234"));
        assert!(!v.validate_format("12345678901"));
    }

    #[test]
    fn token_validator_header() {
        let v = TokenValidator::new().min_length(3);
        let c = AuthConfig::new();
        assert_eq!(
            v.validate_header("Bearer abc123", &c),
            Some("abc123".to_string())
        );
        assert_eq!(v.validate_header("Bearer ab", &c), None);
        assert_eq!(v.validate_header("invalid", &c), None);
    }

    // ----- RoleChecker -----

    #[test]
    fn role_checker_empty() {
        let c = RoleChecker::new();
        assert!(!c.has_permission("admin", "read"));
    }

    #[test]
    fn role_checker_permission() {
        let c = RoleChecker::new().role_permission("admin", "read");
        assert!(c.has_permission("admin", "read"));
        assert!(!c.has_permission("admin", "write"));
    }

    #[test]
    fn role_checker_permissions_batch() {
        let c = RoleChecker::new()
            .role_permissions("admin", vec!["read".to_string(), "write".to_string()]);
        assert!(c.has_permission("admin", "read"));
        assert!(c.has_permission("admin", "write"));
    }

    #[test]
    fn role_checker_any_permission() {
        let c = RoleChecker::new().role_permission("admin", "read");
        assert!(c.has_any_permission("admin", &["read", "write"]));
        assert!(!c.has_any_permission("admin", &["write", "delete"]));
    }

    #[test]
    fn role_checker_check_auth_result() {
        let c = RoleChecker::new().role_permission("admin", "read");
        let auth = AuthResult::success("u", vec!["admin".to_string()], "t");
        assert!(c.check_permission(&auth, "read"));
        assert!(!c.check_permission(&auth, "write"));
    }

    #[test]
    fn role_checker_check_unauthenticated() {
        let c = RoleChecker::new().role_permission("admin", "read");
        let auth = AuthResult::anonymous();
        assert!(!c.check_permission(&auth, "read"));
    }

    #[test]
    fn role_checker_any_permission_auth() {
        let c = RoleChecker::new()
            .role_permission("admin", "read")
            .role_permission("user", "write");
        let auth = AuthResult::success("u", vec!["admin".to_string()], "t");
        assert!(c.check_any_permission(&auth, &["read", "write"]));
    }

    #[test]
    fn role_checker_role_permissions_value() {
        let c = RoleChecker::new()
            .role_permissions("admin", vec!["read".to_string(), "write".to_string()]);
        let perms = c.role_permissions_value("admin");
        assert_eq!(perms.len(), 2);
    }

    #[test]
    fn role_checker_nonexistent_role() {
        let c = RoleChecker::new();
        assert!(c.role_permissions_value("nonexistent").is_empty());
    }
}
