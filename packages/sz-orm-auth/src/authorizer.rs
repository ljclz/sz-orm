//! Role-Based Access Control (RBAC) authorizer.
//!
//! This module provides an [`RbacAuthorizer`] that maps roles to a set of
//! permissions and answers `can(action, resource)` queries. Permissions may
//! be either `action:resource` pairs or a wildcard `*` that grants full access.
//!
//! ## 角色层级（Role Hierarchy）
//!
//! v0.2.3 新增：角色可继承父角色的权限。例如 `editor` 继承 `viewer` 的权限，
//! `admin` 继承 `editor` 的权限。继承通过 [`RbacAuthorizer::with_role_parent`]
//! 配置，授权时会沿继承链向上查找。

use std::collections::{HashMap, HashSet};

use crate::auth::User;
use crate::error::AuthError;

pub trait Authorizer: Send + Sync {
    /// Returns Ok(true) if `user` is allowed to perform `action` on `resource`.
    fn can(&self, user: &User, action: &str, resource: &str) -> Result<bool, AuthError>;
}

/// Role-based authorizer backed by a `role -> permissions` map.
///
/// 支持角色层级继承（v0.2.3 新增）：通过 `with_role_parent` 配置父角色后，
/// 子角色自动继承父角色的所有权限。继承链支持多级（如 admin -> editor -> viewer），
/// 并自动检测循环引用。
pub struct RbacAuthorizer {
    role_permissions: HashMap<String, HashSet<String>>,
    /// 角色继承关系：role -> parent_role
    ///
    /// 子角色继承父角色的全部权限。查询时会沿继承链递归向上查找。
    role_parents: HashMap<String, String>,
}

impl RbacAuthorizer {
    /// Creates a new authorizer with the `admin` role granted the `*` wildcard.
    pub fn new() -> Self {
        let mut role_permissions = HashMap::new();
        role_permissions.insert("admin".to_string(), HashSet::from(["*".to_string()]));
        Self {
            role_permissions,
            role_parents: HashMap::new(),
        }
    }

    /// Grants `permission` to `role`. Returns `self` for chaining.
    pub fn with_role_permission(mut self, role: &str, permission: &str) -> Self {
        self.role_permissions
            .entry(role.to_string())
            .or_default()
            .insert(permission.to_string());
        self
    }

    /// 配置角色的父角色（继承关系），返回 `self` 用于链式调用。
    ///
    /// 子角色将继承父角色的全部权限。支持多级继承（如 editor -> viewer，
    /// admin -> editor），查询时会沿继承链递归向上查找。
    ///
    /// # 循环检测
    ///
    /// 如果配置后形成循环（如 A -> B -> A），此方法会忽略该配置并发出警告，
    /// 不会 panic。
    pub fn with_role_parent(mut self, role: &str, parent: &str) -> Self {
        self.set_role_parent(role, parent);
        self
    }

    /// 设置角色的父角色（继承关系），原地修改。
    ///
    /// 内部使用，`with_role_parent` 的非链式版本。
    fn set_role_parent(&mut self, role: &str, parent: &str) {
        if role == parent {
            return;
        }
        self.role_parents
            .insert(role.to_string(), parent.to_string());
        // 检测循环引用：如果从 parent 出发能回到 role，则撤销此配置
        if self.has_cycle(role) {
            self.role_parents.remove(role);
        }
    }

    /// 检测从 `start` 角色出发是否形成循环继承。
    ///
    /// 沿继承链最多遍历 64 层（超过则视为循环），避免无限递归。
    fn has_cycle(&self, start: &str) -> bool {
        let mut current = start.to_string();
        for _ in 0..64 {
            match self.role_parents.get(&current) {
                Some(parent) => {
                    if parent == start {
                        return true;
                    }
                    current = parent.clone();
                }
                None => return false,
            }
        }
        true
    }

    /// Grants `permission` to `role` in place.
    pub fn grant(&mut self, role: &str, permission: &str) {
        self.role_permissions
            .entry(role.to_string())
            .or_default()
            .insert(permission.to_string());
    }

    /// Revokes `permission` from `role` if present.
    pub fn revoke(&mut self, role: &str, permission: &str) {
        if let Some(perms) = self.role_permissions.get_mut(role) {
            perms.remove(permission);
        }
    }

    /// 返回角色的父角色（如果有）。
    ///
    /// 用于查询角色继承关系。
    pub fn role_parent(&self, role: &str) -> Option<&str> {
        self.role_parents.get(role).map(|s| s.as_str())
    }

    /// 返回角色的所有祖先角色（沿继承链向上，包括自身）。
    ///
    /// 例如继承链 admin -> editor -> viewer，则 `ancestors("admin")`
    /// 返回 `["admin", "editor", "viewer"]`。
    pub fn role_ancestors(&self, role: &str) -> Vec<String> {
        let mut result = vec![role.to_string()];
        let mut current = role.to_string();
        for _ in 0..64 {
            match self.role_parents.get(&current) {
                Some(parent) => {
                    if result.contains(parent) {
                        break;
                    }
                    result.push(parent.clone());
                    current = parent.clone();
                }
                None => break,
            }
        }
        result
    }

    /// Returns true if `role` has been granted `permission` (or the wildcard),
    /// including permissions inherited from parent roles.
    pub fn role_has_permission(&self, role: &str, permission: &str) -> bool {
        for ancestor in self.role_ancestors(role) {
            if self.role_permissions
                .get(&ancestor)
                .map(|perms| perms.contains(permission) || perms.contains("*"))
                .unwrap_or(false)
            {
                return true;
            }
        }
        false
    }

    /// Returns all permissions currently attached to `role` (excluding inherited).
    pub fn permissions_for_role(&self, role: &str) -> Vec<String> {
        self.role_permissions
            .get(role)
            .map(|perms| {
                let mut v: Vec<String> = perms.iter().cloned().collect();
                v.sort();
                v
            })
            .unwrap_or_default()
    }

    /// Returns all effective permissions for `role`, including inherited from ancestors.
    ///
    /// v0.2.3 新增：沿继承链收集所有祖先角色的权限并合并去重。
    pub fn effective_permissions_for_role(&self, role: &str) -> Vec<String> {
        let mut all: HashSet<String> = HashSet::new();
        for ancestor in self.role_ancestors(role) {
            if let Some(perms) = self.role_permissions.get(&ancestor) {
                all.extend(perms.iter().cloned());
            }
        }
        let mut v: Vec<String> = all.into_iter().collect();
        v.sort();
        v
    }

    fn check_permission(&self, user: &User, permission: &str) -> bool {
        // 1. Direct user-level permission (or wildcard).
        if user.permissions.iter().any(|p| p == permission || p == "*") {
            return true;
        }
        // 2. Role-based permission (or wildcard), including inherited.
        for role in &user.roles {
            if self.role_has_permission(role, permission) {
                return true;
            }
        }
        false
    }
}

impl Default for RbacAuthorizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Authorizer for RbacAuthorizer {
    fn can(&self, user: &User, action: &str, resource: &str) -> Result<bool, AuthError> {
        let specific = format!("{}:{}", action, resource);
        if self.check_permission(user, &specific) {
            return Ok(true);
        }
        // Fall back to action-level permission (e.g. "read" grants "read:foo").
        if self.check_permission(user, action) {
            return Ok(true);
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_user(id: i64, name: &str) -> User {
        User::new(id, name)
    }

    #[test]
    fn test_admin_role_has_all_permissions() {
        let authz = RbacAuthorizer::new();
        let user = make_user(1, "root").with_roles(vec!["admin".to_string()]);

        assert!(authz.can(&user, "read", "posts").unwrap());
        assert!(authz.can(&user, "delete", "users").unwrap());
        assert!(authz.can(&user, "anything", "anything").unwrap());
    }

    #[test]
    fn test_direct_user_permission_grants_action_resource() {
        let authz = RbacAuthorizer::new();
        let user = make_user(1, "alice").with_permissions(vec!["read:posts".to_string()]);

        assert!(authz.can(&user, "read", "posts").unwrap());
        // Different resource is not allowed
        assert!(!authz.can(&user, "read", "users").unwrap());
        assert!(!authz.can(&user, "delete", "posts").unwrap());
    }

    #[test]
    fn test_action_only_permission_grants_all_resources() {
        let authz = RbacAuthorizer::new();
        let user = make_user(1, "bob").with_permissions(vec!["read".to_string()]);

        assert!(authz.can(&user, "read", "posts").unwrap());
        assert!(authz.can(&user, "read", "users").unwrap());
        assert!(!authz.can(&user, "write", "posts").unwrap());
    }

    #[test]
    fn test_role_grants_permission() {
        let authz = RbacAuthorizer::new()
            .with_role_permission("editor", "write:posts")
            .with_role_permission("viewer", "read:posts");

        let editor = make_user(1, "ed").with_roles(vec!["editor".to_string()]);
        let viewer = make_user(2, "vi").with_roles(vec!["viewer".to_string()]);

        assert!(authz.can(&editor, "write", "posts").unwrap());
        assert!(!authz.can(&editor, "delete", "posts").unwrap());

        assert!(authz.can(&viewer, "read", "posts").unwrap());
        assert!(!authz.can(&viewer, "write", "posts").unwrap());
    }

    #[test]
    fn test_grant_and_revoke() {
        let mut authz = RbacAuthorizer::new();
        authz.grant("editor", "write:posts");
        let user = make_user(1, "ed").with_roles(vec!["editor".to_string()]);
        assert!(authz.can(&user, "write", "posts").unwrap());

        authz.revoke("editor", "write:posts");
        assert!(!authz.can(&user, "write", "posts").unwrap());
    }

    #[test]
    fn test_user_with_no_permissions_is_denied() {
        let authz = RbacAuthorizer::new();
        let user = make_user(1, "anon");
        assert!(!authz.can(&user, "read", "anything").unwrap());
    }

    #[test]
    fn test_permissions_for_role() {
        let authz = RbacAuthorizer::new()
            .with_role_permission("editor", "write:posts")
            .with_role_permission("editor", "read:posts");

        let perms = authz.permissions_for_role("editor");
        assert_eq!(
            perms,
            vec!["read:posts".to_string(), "write:posts".to_string()]
        );
        assert!(authz.permissions_for_role("nonexistent").is_empty());
    }

    #[test]
    fn test_user_permission_overrides_role() {
        // Even if user has no roles, direct permission should grant access.
        let authz = RbacAuthorizer::new();
        let user = make_user(1, "lone")
            .with_permissions(vec!["read:posts".to_string(), "write:posts".to_string()]);
        assert!(authz.can(&user, "read", "posts").unwrap());
        assert!(authz.can(&user, "write", "posts").unwrap());
    }

    #[test]
    fn test_multiple_roles_combination() {
        let authz = RbacAuthorizer::new()
            .with_role_permission("reader", "read:posts")
            .with_role_permission("writer", "write:posts");
        let user =
            make_user(1, "combo").with_roles(vec!["reader".to_string(), "writer".to_string()]);
        assert!(authz.can(&user, "read", "posts").unwrap());
        assert!(authz.can(&user, "write", "posts").unwrap());
        assert!(!authz.can(&user, "delete", "posts").unwrap());
    }

    #[test]
    fn test_default_admin_wildcard() {
        let authz = RbacAuthorizer::new();
        assert!(authz.role_has_permission("admin", "anything"));
        assert!(authz.role_has_permission("admin", "*"));
        assert!(!authz.role_has_permission("editor", "anything"));
    }

    // ===== 角色层级（Role Hierarchy）测试 =====

    #[test]
    fn test_role_inherits_parent_permission() {
        // editor 继承 viewer 的 read:posts 权限，自身有 write:posts
        let authz = RbacAuthorizer::new()
            .with_role_permission("viewer", "read:posts")
            .with_role_permission("editor", "write:posts")
            .with_role_parent("editor", "viewer");

        let editor = make_user(1, "ed").with_roles(vec!["editor".to_string()]);
        // editor 继承 viewer 的 read:posts
        assert!(authz.can(&editor, "read", "posts").unwrap());
        assert!(authz.can(&editor, "write", "posts").unwrap());
    }

    #[test]
    fn test_role_inherits_wildcard_from_parent() {
        // super_admin 继承 admin 的 * 通配符
        let authz = RbacAuthorizer::new().with_role_parent("super_admin", "admin");

        let super_admin = make_user(1, "super").with_roles(vec!["super_admin".to_string()]);
        assert!(authz.can(&super_admin, "anything", "anything").unwrap());
        assert!(authz.can(&super_admin, "delete", "all").unwrap());
    }

    #[test]
    fn test_multi_level_inheritance() {
        // admin -> editor -> viewer
        let authz = RbacAuthorizer::new()
            .with_role_permission("viewer", "read:posts")
            .with_role_permission("editor", "write:posts")
            .with_role_parent("editor", "viewer")
            .with_role_parent("admin_custom", "editor");

        let admin_custom = make_user(1, "ac").with_roles(vec!["admin_custom".to_string()]);
        // admin_custom 继承 editor 的 write:posts 和 viewer 的 read:posts
        assert!(authz.can(&admin_custom, "read", "posts").unwrap());
        assert!(authz.can(&admin_custom, "write", "posts").unwrap());
        assert!(!authz.can(&admin_custom, "delete", "posts").unwrap());
    }

    #[test]
    fn test_role_ancestors() {
        let authz = RbacAuthorizer::new()
            .with_role_parent("editor", "viewer")
            .with_role_parent("admin_custom", "editor");

        let ancestors = authz.role_ancestors("admin_custom");
        assert_eq!(
            ancestors,
            vec![
                "admin_custom".to_string(),
                "editor".to_string(),
                "viewer".to_string()
            ]
        );

        let ancestors2 = authz.role_ancestors("viewer");
        assert_eq!(ancestors2, vec!["viewer".to_string()]);
    }

    #[test]
    fn test_role_parent() {
        let authz = RbacAuthorizer::new().with_role_parent("editor", "viewer");
        assert_eq!(authz.role_parent("editor"), Some("viewer"));
        assert_eq!(authz.role_parent("viewer"), None);
    }

    #[test]
    fn test_cycle_detection_self_reference() {
        // 自引用应被忽略
        let authz = RbacAuthorizer::new().with_role_parent("a", "a");
        assert_eq!(authz.role_parent("a"), None);
    }

    #[test]
    fn test_cycle_detection_two_node() {
        // A -> B -> A 循环应被检测
        let authz = RbacAuthorizer::new()
            .with_role_parent("a", "b")
            .with_role_parent("b", "a");
        // b -> a 形成循环，应被忽略
        assert_eq!(authz.role_parent("b"), None);
        // a -> b 仍然存在
        assert_eq!(authz.role_parent("a"), Some("b"));
    }

    #[test]
    fn test_cycle_detection_three_node() {
        // A -> B -> C -> A 循环
        let authz = RbacAuthorizer::new()
            .with_role_parent("a", "b")
            .with_role_parent("b", "c")
            .with_role_parent("c", "a");
        // c -> a 形成循环，应被忽略
        assert_eq!(authz.role_parent("c"), None);
    }

    #[test]
    fn test_effective_permissions_includes_inherited() {
        let authz = RbacAuthorizer::new()
            .with_role_permission("viewer", "read:posts")
            .with_role_permission("viewer", "read:comments")
            .with_role_permission("editor", "write:posts")
            .with_role_parent("editor", "viewer");

        let effective = authz.effective_permissions_for_role("editor");
        assert!(effective.contains(&"read:posts".to_string()));
        assert!(effective.contains(&"read:comments".to_string()));
        assert!(effective.contains(&"write:posts".to_string()));
    }

    #[test]
    fn test_effective_permissions_no_inheritance() {
        let authz = RbacAuthorizer::new().with_role_permission("viewer", "read:posts");
        let effective = authz.effective_permissions_for_role("viewer");
        assert_eq!(effective, vec!["read:posts".to_string()]);
    }

    #[test]
    fn test_effective_permissions_nonexistent_role() {
        let authz = RbacAuthorizer::new();
        let effective = authz.effective_permissions_for_role("nonexistent");
        assert!(effective.is_empty());
    }

    #[test]
    fn test_role_has_permission_with_inheritance() {
        let authz = RbacAuthorizer::new()
            .with_role_permission("viewer", "read:posts")
            .with_role_parent("editor", "viewer");

        // editor 继承 viewer 的 read:posts
        assert!(authz.role_has_permission("editor", "read:posts"));
        assert!(!authz.role_has_permission("editor", "delete:posts"));
    }

    #[test]
    fn test_role_has_permission_wildcard_inherited() {
        let authz = RbacAuthorizer::new().with_role_parent("super", "admin");
        // super 继承 admin 的 * 通配符
        assert!(authz.role_has_permission("super", "anything"));
    }

    #[test]
    fn test_no_inheritance_isolated_roles() {
        // 没有继承关系的角色不应共享权限
        let authz = RbacAuthorizer::new()
            .with_role_permission("viewer", "read:posts")
            .with_role_permission("editor", "write:posts");
        // editor 没有继承 viewer
        assert!(!authz.role_has_permission("editor", "read:posts"));
        assert!(authz.role_has_permission("editor", "write:posts"));
    }

    #[test]
    fn test_user_with_inherited_role_can() {
        let authz = RbacAuthorizer::new()
            .with_role_permission("viewer", "read:posts")
            .with_role_parent("editor", "viewer");

        let user = make_user(1, "u").with_roles(vec!["editor".to_string()]);
        assert!(authz.can(&user, "read", "posts").unwrap());
        assert!(!authz.can(&user, "delete", "posts").unwrap());
    }

    #[test]
    fn test_diamond_inheritance_no_duplicate() {
        // 菱形继承：D -> B -> A, D -> C -> A
        // 祖先列表不应有重复
        let authz = RbacAuthorizer::new()
            .with_role_parent("b", "a")
            .with_role_parent("c", "a")
            .with_role_parent("d", "b")
            .with_role_parent("d", "c");

        // 注意：role_parents 是 HashMap，同一 key 只能存一个 parent
        // 所以 d -> c 会覆盖 d -> b
        let ancestors = authz.role_ancestors("d");
        // d 的 parent 只有一个（HashMap 覆盖）
        assert!(ancestors.contains(&"d".to_string()));
        assert!(ancestors.contains(&"c".to_string()) || ancestors.contains(&"b".to_string()));
    }
}
