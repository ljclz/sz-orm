use std::fmt;

#[derive(Debug)]
pub enum AuthError {
    InvalidCredentials(String),
    TokenExpired(String),
    TokenInvalid(String),
    PermissionDenied(String),
    Config(String),
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthError::InvalidCredentials(msg) => write!(f, "Invalid credentials: {}", msg),
            AuthError::TokenExpired(msg) => write!(f, "Token expired: {}", msg),
            AuthError::TokenInvalid(msg) => write!(f, "Token invalid: {}", msg),
            AuthError::PermissionDenied(msg) => write!(f, "Permission denied: {}", msg),
            AuthError::Config(msg) => write!(f, "Config error: {}", msg),
        }
    }
}

impl std::error::Error for AuthError {}
