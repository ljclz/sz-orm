//! # SZ-ORM Auth — 认证授权
//!
//! 提供 JWT 令牌签发/校验与基于 RBAC 的权限控制（`Authorizer`/`RbacAuthorizer`），
//! 涵盖用户、凭证与角色权限模型。
//!
//! ## 主要模块
//!
//! - [`auth`] — 用户、凭证等基础模型
//! - [`jwt`] — JSON Web Token 签发与校验
//! - [`authorizer`] — RBAC 授权器

pub mod auth;
pub mod authorizer;
pub mod error;
pub mod jwt;

pub use auth::*;
pub use authorizer::{Authorizer, RbacAuthorizer};
pub use error::AuthError;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_exports() {
        // Smoke test ensuring the public API compiles and is reachable.
        let creds = Credentials::new("user", "pass");
        assert_eq!(creds.username, "user");
    }

    #[test]
    fn test_rbac_authorizer_via_lib_root() {
        let authorizer = RbacAuthorizer::new();
        let user = User::new(1, "user").with_permissions(vec!["read".to_string()]);

        let can_read = authorizer.can(&user, "read", "resource");
        let can_delete = authorizer.can(&user, "delete", "resource");

        assert!(can_read.unwrap());
        assert!(!can_delete.unwrap());
    }

    #[test]
    fn test_rbac_authorizer_admin_via_lib_root() {
        let authorizer = RbacAuthorizer::new();
        let user = User::new(1, "admin").with_roles(vec!["admin".to_string()]);

        let can_do_anything = authorizer.can(&user, "delete", "anything");

        assert!(can_do_anything.unwrap());
    }

    #[test]
    fn test_jwt_authenticator_via_lib_root() {
        let auth = JwtAuthenticator::new("secret", "issuer", 3600);
        let creds = Credentials::new("user", "pass");

        let token = auth.authenticate(&creds).expect("authenticate");
        assert!(!token.access_token.is_empty());

        let user = auth.verify_token(&token.access_token).expect("verify");
        assert_eq!(user.username, "user");
    }

    #[test]
    fn test_jwt_encoder_via_lib_root() {
        use jwt::{JwtClaims, JwtEncoder};
        let encoder = JwtEncoder::new("lib-secret");
        let claims = JwtClaims::new("lib-user", 9_999_999_999);
        let token = encoder.encode(&claims).expect("encode");
        let decoded = encoder.decode(&token).expect("decode");
        assert_eq!(decoded.sub, "lib-user");
    }
}
