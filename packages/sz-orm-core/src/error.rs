//! 错误类型与处理
//!
//! 全操作的集中错误类型定义

use std::error::Error;
use std::fmt;
use std::io;

/// 数据库错误类型
#[derive(Debug)]
pub enum DbError {
    /// 查询执行失败
    QueryError(String),

    /// 连接失败
    ConnectionError(String),

    /// 连接被拒绝
    ConnectionRefused(String),

    /// 连接超时
    ConnectionTimeout(String),

    /// 连接池错误
    PoolError(PoolError),

    /// 缓存错误
    CacheError(CacheError),

    /// 事务错误
    TxError(TxError),

    /// 迁移错误
    MigrationError(String),

    /// 方言不支持
    Unsupported(String),

    /// 配置错误
    ConfigError(String),

    /// 序列化/反序列化错误
    SerdeError(String),

    /// 未找到
    NotFound(String),

    /// 已存在
    AlreadyExists(String),

    /// 约束冲突
    ConstraintViolation(String),

    /// 非空字段出现 null 值
    NullValue(String),

    /// 输入非法
    InvalidInput(String),

    /// 内部错误
    Internal(String),

    /// IO 错误
    IoError(String),

    /// 钩子执行失败
    Hook(String),

    /// 多租户错误（如租户 ID 缺失、跨租户访问）
    TenantError(String),

    /// 数据验证失败（业务规则校验未通过，由 before_validate 钩子触发）
    Validation(String),
}

impl DbError {
    /// 新建查询错误
    pub fn query(s: impl Into<String>) -> Self {
        DbError::QueryError(s.into())
    }

    /// 新建连接错误
    pub fn connection(s: impl Into<String>) -> Self {
        DbError::ConnectionError(s.into())
    }

    /// 新建未找到错误
    pub fn not_found(s: impl Into<String>) -> Self {
        DbError::NotFound(s.into())
    }

    /// 该错误是否可重试
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            DbError::ConnectionError(_)
                | DbError::ConnectionTimeout(_)
                | DbError::PoolError(PoolError::Timeout)
        )
    }

    /// 获取错误码（用于日志/监控）
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
            DbError::Validation(_) => "DB021",
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
            DbError::Validation(s) => write!(f, "Validation error: {}", s),
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

impl<T> From<std::sync::PoisonError<T>> for DbError {
    fn from(err: std::sync::PoisonError<T>) -> Self {
        DbError::Internal(format!("RwLock/Mutex poisoned: {}", err))
    }
}

/// 连接池特有错误
#[derive(Debug)]
pub enum PoolError {
    /// 连接池耗尽
    Exhausted,

    /// 获取连接超时
    Timeout,

    /// 连接已被获取
    AlreadyAcquired,

    /// 连接未被获取
    NotAcquired,

    /// 配置非法
    InvalidConfig(String),

    /// 内部错误
    Internal(String),

    /// 连接池已关闭（close_all 后拒绝新 acquire）
    Closed,

    /// 连接创建失败（保留原始错误信息）
    ConnectionFailed(String),
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
            PoolError::Closed => "PL007",
            PoolError::ConnectionFailed(_) => "PL008",
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
            PoolError::Closed => write!(f, "Connection pool closed"),
            PoolError::ConnectionFailed(s) => write!(f, "Connection failed: {}", s),
        }
    }
}

impl Error for PoolError {}

/// 缓存特有错误
#[derive(Debug)]
pub enum CacheError {
    /// 键不存在
    NotFound(String),

    /// 序列化错误
    SerializationError(String),

    /// 反序列化错误
    DeserializationError(String),

    /// 连接错误
    ConnectionError(String),

    /// 超时
    Timeout(String),

    /// 内部错误
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

/// 事务状态
///
/// 定义在 `error` 模块以避免 `transaction` ↔ `error` 循环依赖，
/// `transaction` 模块通过 `pub use` 重导出本类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransactionState {
    #[default]
    Active,
    Committed,
    RolledBack,
}

impl fmt::Display for TransactionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransactionState::Active => write!(f, "Active"),
            TransactionState::Committed => write!(f, "Committed"),
            TransactionState::RolledBack => write!(f, "RolledBack"),
        }
    }
}

/// 事务特有错误
#[derive(Debug)]
pub enum TxError {
    /// 事务未开始
    NotStarted,

    /// 事务已开始
    AlreadyStarted,

    /// 事务提交失败
    CommitFailed(String),

    /// 事务回滚失败
    RollbackFailed(String),

    /// 保存点错误
    SavepointError(String),

    /// 不支持嵌套事务
    NestedNotSupported,

    /// 事务不在 Active 状态（用于 execute/query 等操作前置校验）
    NotActive(TransactionState),

    /// 保存点名称非法（包含不支持的字符或以数字开头）
    InvalidSavepointName(String),

    /// 连接已被取走（take_connection 重复调用，或操作时连接已释放）
    ConnectionTaken,

    /// H-8 修复：嵌套事务深度超过限制
    ///
    /// `current_depth` 为当前已嵌套深度（含本次），`max_depth` 为配置的最大深度。
    MaxNestingDepthExceeded { current_depth: u32, max_depth: u32 },

    /// M-8 修复：死锁检测
    ///
    /// 当事务执行过程中检测到死锁（数据库返回死锁错误码）时返回。
    /// 调用方可使用 `retry_on_deadlock` 包装器自动重试。
    DeadlockDetected { attempt: u32, max_attempts: u32 },
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
            TxError::NotActive(state) => {
                write!(f, "Transaction not active (current state: {})", state)
            }
            TxError::InvalidSavepointName(name) => {
                write!(
                    f,
                    "Invalid savepoint name '{}': must be non-empty, start with a letter or underscore, and contain only ASCII alphanumeric or underscore",
                    name
                )
            }
            TxError::ConnectionTaken => write!(f, "Transaction connection already taken"),
            TxError::MaxNestingDepthExceeded {
                current_depth,
                max_depth,
            } => write!(
                f,
                "Transaction nesting depth {} exceeds maximum allowed {}",
                current_depth, max_depth
            ),
            TxError::DeadlockDetected {
                attempt,
                max_attempts,
            } => write!(
                f,
                "Deadlock detected on attempt {} of {}",
                attempt, max_attempts
            ),
        }
    }
}

impl Error for TxError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        // TxError 各变体仅承载 String 描述或状态枚举（无嵌套 Error 对象），故无 source 可委托
        None
    }
}

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
