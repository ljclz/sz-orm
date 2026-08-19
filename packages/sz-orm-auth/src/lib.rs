//! # SZ-ORM Auth — Authentication & Authorization
//!
//! Provides JWT token signing/verification and RBAC-based permission control (`Authorizer`/`RbacAuthorizer`),
//! covering user, credential, and role-permission models.
//!
//! ## Main Modules
//!
//! - [`auth`] — User, credential and other basic models
//! - [`jwt`] — JSON Web Token signing and verification
//! - [`authorizer`] — RBAC authorizer (with role hierarchy)
//! - [`oauth2`] — OAuth2 authorization code flow (RFC 6749)
//! - [`mfa`] — Multi-factor authentication (TOTP, RFC 6238)
//! - [`token_store`] — Refresh token storage (rotation + revocation + replay detection)

pub mod auth;
pub mod authorizer;
pub mod error;
pub mod jwt;
pub mod mfa;
pub mod oauth2;
pub mod token_store;

pub use auth::*;
pub use authorizer::{Authorizer, RbacAuthorizer};
pub use error::AuthError;
pub use mfa::{MfaManager, MfaSecret, TotpVerifier};
pub use oauth2::{AuthorizationCode, AuthorizationRequest, OAuth2Server, TokenRequest};
pub use token_store::{StoredToken, TokenFamilyError, TokenStore};

#[cfg(test)]
mod tests {
    use super::*;

    /// Test password verifier (after H-1 fix, authenticate must configure verifier)
    struct MockVerifier;
    impl auth::PasswordVerifier for MockVerifier {
        fn verify_password(&self, _u: &str, _p: &str) -> Result<i64, AuthError> {
            Ok(42)
        }
    }

    #[test]
    fn test_module_exports() {
        // Smoke test ensuring the public API compiles and is reachable.
        let creds = Credentials::new("user", "pass");
        assert_eq!(creds.username, "user");
    }

    #[test]
    fn test_rbac_authorizer_via_lib_root() {
        let authorizer = RbacAuthorizer::new();
        // v4.8.0 修复 M-11：action 级权限不再隐式授予任意资源
        let user = User::new(1, "user").with_permissions(vec!["read".to_string()]);
        let can_read = authorizer.can(&user, "read", "resource");
        assert!(!can_read.unwrap(), "action 级 read 不得授予 read:resource");

        // 显式 action:resource 权限正常放行
        let user2 = User::new(2, "user").with_permissions(vec!["read:resource".to_string()]);
        let can_read = authorizer.can(&user2, "read", "resource");
        let can_delete = authorizer.can(&user2, "delete", "resource");
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
        // v1.2.1 H-1 修复：authenticate 必须配置 password_verifier
        let auth = JwtAuthenticator::new("secret", "issuer", 3600)
            .with_password_verifier(std::sync::Arc::new(MockVerifier));
        let creds = Credentials::new("user", "pass");

        let token = auth.authenticate(&creds).expect("authenticate");
        assert!(!token.access_token.is_empty());

        let user = auth.verify_token(&token.access_token).expect("verify");
        assert_eq!(user.username, "user");
        assert_eq!(user.id, 42);
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
