//! Error types and handling
//!
//! Centralized error types for all operations

use std::error::Error;
use std::fmt;
use std::io;

/// Database error type
#[derive(Debug)]
pub enum DbError {
    /// Query execution failed
    QueryError(String),

    /// Connection failed
    ConnectionError(String),

    /// Connection refused
    ConnectionRefused(String),

    /// Connection timeout
    ConnectionTimeout(String),

    /// Pool error
    PoolError(PoolError),

    /// Cache error
    CacheError(CacheError),

    /// Transaction error
    TxError(TxError),

    /// Migration error
    MigrationError(String),

    /// Dialect not supported
    Unsupported(String),

    /// Configuration error
    ConfigError(String),

    /// Serialization/Deserialization error
    SerdeError(String),

    /// Not found
    NotFound(String),

    /// Already exists
    AlreadyExists(String),

    /// Constraint violation
    ConstraintViolation(String),

    /// Null value in non-nullable field
    NullValue(String),

    /// Invalid input
    InvalidInput(String),

    /// Internal error
    Internal(String),

    /// Io error
    IoError(String),

    /// 钩子执行失败
    Hook(String),

    /// 多租户错误（如租户 ID 缺失、跨租户访问）
    TenantError(String),
}

impl DbError {
    /// Create a new query error
    pub fn query(s: impl Into<String>) -> Self {
        DbError::QueryError(s.into())
    }

    /// Create a new connection error
    pub fn connection(s: impl Into<String>) -> Self {
        DbError::ConnectionError(s.into())
    }

    /// Create a new not found error
    pub fn not_found(s: impl Into<String>) -> Self {
        DbError::NotFound(s.into())
    }

    /// Check if error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            DbError::ConnectionError(_)
                | DbError::ConnectionTimeout(_)
                | DbError::PoolError(PoolError::Timeout)
        )
    }

    /// Get the error code (for logging/monitoring)
    pub fn error_code(&self) -> &'static str {
        match self {
            DbError::QueryError(_) => "DB001",
            DbError::ConnectionError(_) => "DB002",
            DbError::ConnectionRefused(_) => "DB003",
            DbError::ConnectionTimeout(_) => "DB004",
            DbError::PoolError(e) => e.error_code(),
            DbError::CacheError(e) => e.error_code(),
            DbError::TxError(_) => "DB007",
            DbError::MigrationError(_) => "DB008",
            DbError::Unsupported(_) => "DB009",
            DbError::ConfigError(_) => "DB010",
            DbError::SerdeError(_) => "DB011",
            DbError::NotFound(_) => "DB012",
            DbError::AlreadyExists(_) => "DB013",
            DbError::ConstraintViolation(_) => "DB014",
            DbError::NullValue(_) => "DB015",
            DbError::InvalidInput(_) => "DB016",
            DbError::Internal(_) => "DB017",
            DbError::IoError(_) => "DB018",
            DbError::Hook(_) => "DB019",
            DbError::TenantError(_) => "DB020",
        }
    }
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DbError::QueryError(s) => write!(f, "Query error: {}", s),
            DbError::ConnectionError(s) => write!(f, "Connection error: {}", s),
            DbError::ConnectionRefused(s) => write!(f, "Connection refused: {}", s),
            DbError::ConnectionTimeout(s) => write!(f, "Connection timeout: {}", s),
            DbError::PoolError(e) => write!(f, "Pool error: {}", e),
            DbError::CacheError(e) => write!(f, "Cache error: {}", e),
            DbError::TxError(e) => write!(f, "Transaction error: {}", e),
            DbError::MigrationError(s) => write!(f, "Migration error: {}", s),
            DbError::Unsupported(s) => write!(f, "Unsupported: {}", s),
            DbError::ConfigError(s) => write!(f, "Configuration error: {}", s),
            DbError::SerdeError(s) => write!(f, "Serialization error: {}", s),
            DbError::NotFound(s) => write!(f, "Not found: {}", s),
            DbError::AlreadyExists(s) => write!(f, "Already exists: {}", s),
            DbError::ConstraintViolation(s) => write!(f, "Constraint violation: {}", s),
            DbError::NullValue(s) => write!(f, "Null value: {}", s),
            DbError::InvalidInput(s) => write!(f, "Invalid input: {}", s),
            DbError::Internal(s) => write!(f, "Internal error: {}", s),
            DbError::IoError(s) => write!(f, "IO error: {}", s),
            DbError::Hook(s) => write!(f, "Hook error: {}", s),
            DbError::TenantError(s) => write!(f, "Tenant error: {}", s),
        }
    }
}

impl Error for DbError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            DbError::PoolError(e) => Some(e),
            DbError::CacheError(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for DbError {
    fn from(err: io::Error) -> Self {
        DbError::IoError(err.to_string())
    }
}

impl From<serde_json::Error> for DbError {
    fn from(err: serde_json::Error) -> Self {
        DbError::SerdeError(err.to_string())
    }
}

impl From<std::num::TryFromIntError> for DbError {
    fn from(err: std::num::TryFromIntError) -> Self {
        DbError::Internal(err.to_string())
    }
}

impl From<std::string::FromUtf8Error> for DbError {
    fn from(err: std::string::FromUtf8Error) -> Self {
        DbError::Internal(err.to_string())
    }
}

/// Pool specific errors
#[derive(Debug)]
pub enum PoolError {
    /// Connection pool exhausted
    Exhausted,

    /// Connection acquire timeout
    Timeout,

    /// Connection already acquired
    AlreadyAcquired,

    /// Connection not acquired
    NotAcquired,

    /// Invalid configuration
    InvalidConfig(String),

    /// Internal error
    Internal(String),
}

impl PoolError {
    pub fn error_code(&self) -> &'static str {
        match self {
            PoolError::Exhausted => "PL001",
            PoolError::Timeout => "PL002",
            PoolError::AlreadyAcquired => "PL003",
            PoolError::NotAcquired => "PL004",
            PoolError::InvalidConfig(_) => "PL005",
            PoolError::Internal(_) => "PL006",
        }
    }
}

impl fmt::Display for PoolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PoolError::Exhausted => write!(f, "Connection pool exhausted"),
            PoolError::Timeout => write!(f, "Connection acquire timeout"),
            PoolError::AlreadyAcquired => write!(f, "Connection already acquired"),
            PoolError::NotAcquired => write!(f, "Connection not acquired"),
            PoolError::InvalidConfig(s) => write!(f, "Invalid pool config: {}", s),
            PoolError::Internal(s) => write!(f, "Internal pool error: {}", s),
        }
    }
}

impl Error for PoolError {}

/// Cache specific errors
#[derive(Debug)]
pub enum CacheError {
    /// Key not found
    NotFound(String),

    /// Serialization error
    SerializationError(String),

    /// Deserialization error
    DeserializationError(String),

    /// Connection error
    ConnectionError(String),

    /// Timeout
    Timeout(String),

    /// Internal error
    Internal(String),
}

impl CacheError {
    pub fn error_code(&self) -> &'static str {
        match self {
            CacheError::NotFound(_) => "CH001",
            CacheError::SerializationError(_) => "CH002",
            CacheError::DeserializationError(_) => "CH003",
            CacheError::ConnectionError(_) => "CH004",
            CacheError::Timeout(_) => "CH005",
            CacheError::Internal(_) => "CH006",
        }
    }
}

impl fmt::Display for CacheError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CacheError::NotFound(s) => write!(f, "Cache key not found: {}", s),
            CacheError::SerializationError(s) => write!(f, "Cache serialization error: {}", s),
            CacheError::DeserializationError(s) => write!(f, "Cache deserialization error: {}", s),
            CacheError::ConnectionError(s) => write!(f, "Cache connection error: {}", s),
            CacheError::Timeout(s) => write!(f, "Cache timeout: {}", s),
            CacheError::Internal(s) => write!(f, "Cache internal error: {}", s),
        }
    }
}

impl Error for CacheError {}

impl<T> From<std::sync::PoisonError<T>> for CacheError {
    fn from(err: std::sync::PoisonError<T>) -> Self {
        CacheError::Internal(format!("RwLock poisoned: {}", err))
    }
}

/// Transaction specific errors
#[derive(Debug)]
pub enum TxError {
    /// Transaction not started
    NotStarted,

    /// Transaction already started
    AlreadyStarted,

    /// Transaction commit failed
    CommitFailed(String),

    /// Transaction rollback failed
    RollbackFailed(String),

    /// Savepoint error
    SavepointError(String),

    /// Nested transaction not supported
    NestedNotSupported,
}

impl fmt::Display for TxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TxError::NotStarted => write!(f, "Transaction not started"),
            TxError::AlreadyStarted => write!(f, "Transaction already started"),
            TxError::CommitFailed(s) => write!(f, "Transaction commit failed: {}", s),
            TxError::RollbackFailed(s) => write!(f, "Transaction rollback failed: {}", s),
            TxError::SavepointError(s) => write!(f, "Savepoint error: {}", s),
            TxError::NestedNotSupported => write!(f, "Nested transactions not supported"),
        }
    }
}

impl Error for TxError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_db_error_display() {
        let err = DbError::query("test");
        assert_eq!(format!("{}", err), "Query error: test");

        let err = DbError::not_found("user");
        assert_eq!(format!("{}", err), "Not found: user");
    }

    #[test]
    fn test_db_error_code() {
        let err = DbError::query("test");
        assert_eq!(err.error_code(), "DB001");

        let err = DbError::PoolError(PoolError::Timeout);
        assert_eq!(err.error_code(), "PL002");
    }

    #[test]
    fn test_db_error_source() {
        let err = DbError::PoolError(PoolError::Timeout);
        assert!(err.source().is_some());
    }

    #[test]
    fn test_pool_error() {
        let err = PoolError::Timeout;
        assert_eq!(format!("{}", err), "Connection acquire timeout");
        assert_eq!(err.error_code(), "PL002");
    }

    #[test]
    fn test_cache_error() {
        let err = CacheError::NotFound("key".to_string());
        assert_eq!(format!("{}", err), "Cache key not found: key");
        assert_eq!(err.error_code(), "CH001");
    }
}
