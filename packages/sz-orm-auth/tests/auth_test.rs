use std::sync::Arc;
use sz_orm_auth::*;

struct TestVerifier;
impl auth::PasswordVerifier for TestVerifier {
    fn verify_password(&self, _u: &str, _p: &str) -> Result<i64, AuthError> {
        Ok(1)
    }
}

#[test]
fn test_credentials_new() {
    let creds = Credentials::new("testuser", "testpass");
    assert_eq!(creds.username, "testuser");
    assert_eq!(creds.password, "testpass");
}

#[test]
fn test_user_new() {
    let user = User::new(42, "alice");
    assert_eq!(user.id, 42);
    assert_eq!(user.username, "alice");
}

#[test]
fn test_user_with_permissions() {
    let user = User::new(1, "u").with_permissions(vec!["read".to_string(), "write".to_string()]);
    assert!(user.permissions.contains(&"read".to_string()));
    assert!(user.permissions.contains(&"write".to_string()));
}

#[test]
fn test_user_with_roles() {
    let user = User::new(1, "u").with_roles(vec!["admin".to_string()]);
    assert!(user.roles.contains(&"admin".to_string()));
}

#[test]
fn test_rbac_authorizer_permission_allowed() {
    // v4.8.0 修复 M-11：显式 action:resource 权限正常放行
    let authorizer = RbacAuthorizer::new();
    let user = User::new(1, "u").with_permissions(vec!["read:resource".to_string()]);
    assert!(authorizer.can(&user, "read", "resource").unwrap());
    // action 级权限不再隐式授予任意资源
    let coarse = User::new(2, "v").with_permissions(vec!["read".to_string()]);
    assert!(!authorizer.can(&coarse, "read", "resource").unwrap());
}

#[test]
fn test_rbac_authorizer_permission_denied() {
    let authorizer = RbacAuthorizer::new();
    let user = User::new(1, "u").with_permissions(vec!["read".to_string()]);
    assert!(!authorizer.can(&user, "delete", "resource").unwrap());
}

#[test]
fn test_rbac_authorizer_admin_role() {
    let authorizer = RbacAuthorizer::new();
    let user = User::new(1, "admin").with_roles(vec!["admin".to_string()]);
    assert!(authorizer.can(&user, "anything", "anyresource").unwrap());
}

#[test]
fn test_rbac_authorizer_no_permissions() {
    let authorizer = RbacAuthorizer::new();
    let user = User::new(1, "u");
    assert!(!authorizer.can(&user, "read", "resource").unwrap());
}

#[test]
fn test_jwt_encoder_decode() {
    use jwt::{JwtClaims, JwtEncoder};
    let encoder = JwtEncoder::new("secret-key");
    let claims = JwtClaims::new("user123", 9999999999);
    let token = encoder.encode(&claims).unwrap();
    let decoded = encoder.decode(&token).unwrap();
    assert_eq!(decoded.sub, "user123");
}

#[test]
fn test_jwt_authenticator_flow() {
    let auth = JwtAuthenticator::new("secret", "issuer", 3600)
        .with_password_verifier(Arc::new(TestVerifier));
    let creds = Credentials::new("user", "pass");
    let token = auth.authenticate(&creds).unwrap();
    assert!(!token.access_token.is_empty());
    let user = auth.verify_token(&token.access_token).unwrap();
    assert_eq!(user.username, "user");
}

#[test]
fn test_jwt_authenticator_invalid_token() {
    let auth = JwtAuthenticator::new("secret", "issuer", 3600)
        .with_password_verifier(Arc::new(TestVerifier));
    assert!(auth.verify_token("invalid.token.here").is_err());
}

#[test]
fn test_mfa_secret_new() {
    let secret = MfaSecret::new("user@example.com", "MyApp");
    assert!(!secret.base32_secret.is_empty());
    assert_eq!(secret.account, "user@example.com");
    assert_eq!(secret.issuer, "MyApp");
}

#[test]
fn test_mfa_secret_from_base32() {
    let secret = MfaSecret::from_base32("JBSWY3DPEHPK3PXP", "user", "App");
    assert_eq!(secret.base32_secret, "JBSWY3DPEHPK3PXP");
    assert_eq!(secret.account, "user");
    assert_eq!(secret.issuer, "App");
}

#[test]
fn test_mfa_secret_to_uri() {
    let secret = MfaSecret::from_base32("JBSWY3DPEHPK3PXP", "user", "MyApp");
    let uri = secret.to_uri();
    assert!(uri.starts_with("otpauth://totp/"));
    assert!(uri.contains("JBSWY3DPEHPK3PXP"));
}

#[test]
fn test_totp_verifier_generate_code() {
    let verifier = TotpVerifier::new();
    let secret = MfaSecret::new("user", "App");
    let code = verifier.generate_at(&secret.base32_secret, 1234567890);
    assert_eq!(code.len(), 6);
}

#[test]
fn test_totp_verifier_valid_code() {
    let verifier = TotpVerifier::new();
    let secret = MfaSecret::new("user", "App");
    let timestamp: u64 = 1234567890;
    let code = verifier.generate_at(&secret.base32_secret, timestamp);
    assert!(verifier.verify_at(&secret.base32_secret, &code, timestamp));
}

#[test]
fn test_totp_verifier_invalid_code() {
    let verifier = TotpVerifier::new();
    let secret = MfaSecret::new("user", "App");
    assert!(!verifier.verify_at(&secret.base32_secret, "000000", 1234567890));
}

#[test]
fn test_totp_verifier_with_time_step() {
    let verifier = TotpVerifier::new().with_time_step(60);
    let secret = MfaSecret::new("user", "App");
    let code = verifier.generate_at(&secret.base32_secret, 1234567890);
    assert_eq!(code.len(), 6);
}

#[test]
fn test_totp_verifier_with_drift() {
    let verifier = TotpVerifier::new().with_drift(2);
    let secret = MfaSecret::new("user", "App");
    let ts: u64 = 1234567890;
    let code = verifier.generate_at(&secret.base32_secret, ts);
    assert!(verifier.verify_at(&secret.base32_secret, &code, ts));
}

#[test]
fn test_oauth2_authorization_request() {
    let req = AuthorizationRequest::new(
        "client123",
        "https://example.com/cb",
        "read write",
        "state123",
    );
    assert_eq!(req.client_id, "client123");
    assert_eq!(req.redirect_uri, "https://example.com/cb");
    assert_eq!(req.scope, "read write");
    assert_eq!(req.state, "state123");
    assert_eq!(req.response_type, "code");
}

#[test]
fn test_token_store_new() {
    let store = TokenStore::new();
    let _ = store.with_refresh_lifetime(3600);
}

#[test]
fn test_token_store_issue_family() {
    let store = TokenStore::new();
    let token = store.issue_family("refresh_token_1", 42).unwrap();
    assert_eq!(token.token, "refresh_token_1");
    assert_eq!(token.user_id, 42);
    assert!(!token.used);
    assert!(!token.revoked);
}

#[test]
fn test_token_store_refresh() {
    let store = TokenStore::new();
    store.issue_family("old_refresh", 1).unwrap();
    let new_token = store.refresh("old_refresh", "new_refresh").unwrap();
    assert_eq!(new_token.token, "new_refresh");
    assert_eq!(new_token.user_id, 1);
}

#[test]
fn test_token_store_refresh_replay_detected() {
    let store = TokenStore::new();
    store.issue_family("refresh_1", 1).unwrap();
    store.refresh("refresh_1", "refresh_2").unwrap();
    let result = store.refresh("refresh_1", "refresh_3");
    assert!(result.is_err());
}

#[test]
fn test_token_store_refresh_not_found() {
    let store = TokenStore::new();
    let result = store.refresh("nonexistent", "new_token");
    assert!(result.is_err());
}

#[test]
fn test_stored_token_is_valid() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let token = StoredToken::new("token", "family", 1, now + 3600);
    assert!(token.is_valid());
}

#[test]
fn test_stored_token_expired() {
    let token = StoredToken::new("token", "family", 1, 1);
    assert!(token.is_expired());
    assert!(!token.is_valid());
}
