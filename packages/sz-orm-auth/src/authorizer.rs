//! Role-Based Access Control (RBAC) authorizer.
//!
//! This module provides an [`RbacAuthorizer`] that maps roles to a set of
//! permissions and answers `can(action, resource)` queries. Permissions may
//! be either `action:resource` pairs or a wildcard `*` that grants full access.

use std::collections::{HashMap, HashSet};

use crate::auth::User;
use crate::error::AuthError;

pub trait Authorizer: Send + Sync {
    /// Returns Ok(true) if `user` is allowed to perform `action` on `resource`.
    fn can(&self, user: &User, action: &str, resource: &str) -> Result<bool, AuthError>;
}

/// Role-based authorizer backed by a `role -> permissions` map.
pub struct RbacAuthorizer {
    role_permissions: HashMap<String, HashSet<String>>,
}

impl RbacAuthorizer {
    /// Creates a new authorizer with the `admin` role granted the `*` wildcard.
    pub fn new() -> Self {
        let mut role_permissions = HashMap::new();
        role_permissions.insert("admin".to_string(), HashSet::from(["*".to_string()]));
        Self { role_permissions }
    }

    /// Grants `permission` to `role`. Returns `self` for chaining.
    pub fn with_role_permission(mut self, role: &str, permission: &str) -> Self {
        self.role_permissions
            .entry(role.to_string())
            .or_default()
            .insert(permission.to_string());
        self
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

    /// Returns true if `role` has been granted `permission` (or the wildcard).
    pub fn role_has_permission(&self, role: &str, permission: &str) -> bool {
        self.role_permissions
            .get(role)
            .map(|perms| perms.contains(permission) || perms.contains("*"))
            .unwrap_or(false)
    }

    /// Returns all permissions currently attached to `role`.
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

    fn check_permission(&self, user: &User, permission: &str) -> bool {
        // 1. Direct user-level permission (or wildcard).
        if user.permissions.iter().any(|p| p == permission || p == "*") {
            return true;
        }
        // 2. Role-based permission (or wildcard).
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
}
