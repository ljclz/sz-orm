use sz_orm_auth::AuthError;
use std::error::Error;

#[test]
fn test_invalid_credentials() {
    let err = AuthError::InvalidCredentials("bad user".into());
    assert_eq!(err.to_string(), "Invalid credentials: bad user");
}

#[test]
fn test_token_expired() {
    let err = AuthError::TokenExpired("jwt".into());
    assert_eq!(err.to_string(), "Token expired: jwt");
}

#[test]
fn test_token_invalid() {
    let err = AuthError::TokenInvalid("sig".into());
    assert_eq!(err.to_string(), "Token invalid: sig");
}

#[test]
fn test_permission_denied() {
    let err = AuthError::PermissionDenied("admin".into());
    assert_eq!(err.to_string(), "Permission denied: admin");
}

#[test]
fn test_config_error() {
    let err = AuthError::Config("missing".into());
    assert_eq!(err.to_string(), "Config error: missing");
}

#[test]
fn test_secret_too_short() {
    let err = AuthError::SecretTooShort("8".into());
    assert_eq!(err.to_string(), "Secret too short: 8");
}

#[test]
fn test_error_trait_impl() {
    let err = AuthError::InvalidCredentials("x".into());
    assert!(err.source().is_none());
}