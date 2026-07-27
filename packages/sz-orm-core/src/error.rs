//! 错误类型与处理
//!
//! 全操作的集中错误类型定义

use std::error::Error;
use std::fmt;
use std::io;
use std::sync::{Arc, OnceLock, RwLock};

/// 错误上报 hook 类型
type ErrorHook = Arc<dyn Fn(&DbError) + Send + Sync>;

/// 全局错误上报 hook 存储（使用 OnceLock 实现 lazy 初始化，无需 once_cell 依赖）
static GLOBAL_ERROR_HOOK: OnceLock<RwLock<Option<ErrorHook>>> = OnceLock::new();

/// 获取全局错误 hook 存储的引用
fn error_hook_storage() -> &'static RwLock<Option<ErrorHook>> {
    GLOBAL_ERROR_HOOK.get_or_init(|| RwLock::new(None))
}

/// 设置全局错误上报 hook
///
/// 调用后，所有通过 `trigger_error_hook` 触发的错误都会被传入此 hook。
pub fn set_error_hook(hook: ErrorHook) {
    *error_hook_storage().write().unwrap() = Some(hook);
}

/// 触发错误 hook（在 DbError 创建/返回时调用）
///
/// 如果未设置 hook 或读取锁失败，则静默跳过。
pub fn trigger_error_hook(err: &DbError) {
    if let Ok(storage) = error_hook_storage().read() {
        if let Some(ref hook) = *storage {
            hook(err);
        }
    }
}

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

    /// 约束冲突（通用回退，无法确定具体类型时使用）
    ConstraintViolation(String),

    /// 唯一约束冲突（UNIQUE constraint）
    UniqueViolation(String),

    /// 外键约束冲突（FOREIGN KEY constraint）
    ForeignKeyViolation(String),

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

    /// #6 修复：带上下文链的错误
    ///
    /// 包装原始错误 + 上下文链，用于在错误传播路径上附加调用方上下文。
    /// 通过 `DbError::with_context("operation")` 创建。
    Contextual {
        /// 原始错误（Box 避免递归类型大小爆炸）
        source: Box<DbError>,
        /// 上下文链头节点
        context: ErrorContext,
    },
}

/// #6 修复：错误上下文链节点
///
/// 构成 `Context → Context → ...` 的单向链表，每一层记录
/// `context`（操作描述）与 `span`（可选 tracing span 名称）。
#[derive(Debug, Clone)]
pub struct ErrorContext {
    /// 当前层上下文描述（如 "fetching user by id"）
    pub context: String,
    /// 可选 tracing span 名（如 "user_service"）
    pub span: Option<String>,
    /// 上一层上下文（None 表示链尾）
    pub previous: Option<Box<ErrorContext>>,
}

impl ErrorContext {
    /// 创建新的上下文节点
    pub fn new(context: impl Into<String>) -> Self {
        Self {
            context: context.into(),
            span: None,
            previous: None,
        }
    }

    /// 附加上一层上下文（消费 self，返回新的链头）
    pub fn with_previous(mut self, prev: ErrorContext) -> Self {
        self.previous = Some(Box::new(prev));
        self
    }

    /// 设置 tracing span 名
    pub fn with_span(mut self, span: impl Into<String>) -> Self {
        self.span = Some(span.into());
        self
    }

    /// 遍历上下文链，从最外层到最内层
    pub fn iter(&self) -> impl Iterator<Item = &ErrorContext> {
        let mut current = Some(self);
        std::iter::from_fn(move || {
            let node = current?;
            let result = node;
            current = node.previous.as_deref();
            Some(result)
        })
    }

    /// 格式化为多行字符串（每行一个上下文层）
    pub fn format_chain(&self) -> String {
        self.iter()
            .enumerate()
            .map(|(i, ctx)| {
                if let Some(ref span) = ctx.span {
                    format!("  [{}] {} (span: {})", i, ctx.context, span)
                } else {
                    format!("  [{}] {}", i, ctx.context)
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
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

    /// #6 修复：附加错误上下文（消费 self，返回带上下文的新错误）
    ///
    /// 用于在错误传播路径上附加调用方上下文，形成 `error.with_context("operation")`
    /// 链式调用。多次调用会形成上下文链表。
    ///
    /// # 示例
    ///
    /// ```ignore
    /// fn fetch_user(id: i64) -> Result<User, DbError> {
    ///     db.query("SELECT * FROM users WHERE id = ?", &[id.into()])
    ///         .await
    ///         .map_err(|e| e.with_context(format!("fetching user id={}", id)))?;
    ///     // ...
    /// }
    /// ```
    pub fn with_context(self, context: impl Into<String>) -> Self {
        let new_ctx = ErrorContext::new(context);
        // 若已是 Contextual，将原 context 链作为 previous
        match self {
            DbError::Contextual {
                source,
                context: existing_ctx,
            } => {
                let new_ctx = new_ctx.with_previous(existing_ctx);
                DbError::Contextual {
                    source,
                    context: new_ctx,
                }
            }
            other => DbError::Contextual {
                source: Box::new(other),
                context: new_ctx,
            },
        }
    }

    /// #6 修复：附加错误上下文（含 tracing span 名）
    pub fn with_context_in_span(self, context: impl Into<String>, span: impl Into<String>) -> Self {
        let new_ctx = ErrorContext::new(context).with_span(span);
        match self {
            DbError::Contextual {
                source,
                context: existing_ctx,
            } => {
                let new_ctx = new_ctx.with_previous(existing_ctx);
                DbError::Contextual {
                    source,
                    context: new_ctx,
                }
            }
            other => DbError::Contextual {
                source: Box::new(other),
                context: new_ctx,
            },
        }
    }

    /// #6 修复：获取错误上下文链（None 表示无附加上下文）
    pub fn context(&self) -> Option<&ErrorContext> {
        match self {
            DbError::Contextual { context, .. } => Some(context),
            _ => None,
        }
    }

    /// #6 修复：格式化错误上下文链为多行字符串
    pub fn format_context_chain(&self) -> String {
        match self {
            DbError::Contextual { context, .. } => context.format_chain(),
            _ => String::new(),
        }
    }

    /// #6 修复：剥离上下文链，返回原始错误引用
    pub fn root_cause(&self) -> &DbError {
        match self {
            DbError::Contextual { source, .. } => source.root_cause(),
            other => other,
        }
    }

    /// 该错误是否可重试
    pub fn is_retryable(&self) -> bool {
        self.root_cause_is_retryable()
    }

    /// 内部方法：检查根错误是否可重试
    fn root_cause_is_retryable(&self) -> bool {
        match self {
            DbError::Contextual { source, .. } => source.root_cause_is_retryable(),
            DbError::ConnectionError(_)
            | DbError::ConnectionTimeout(_)
            | DbError::PoolError(PoolError::Timeout) => true,
            _ => false,
        }
    }

    /// 获取错误码（用于日志/监控）
    pub fn error_code(&self) -> &'static str {
        match self {
            DbError::Contextual { source, .. } => source.error_code(),
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
            DbError::UniqueViolation(_) => "DB022",
            DbError::ForeignKeyViolation(_) => "DB023",
            DbError::NullValue(_) => "DB015",
            DbError::InvalidInput(_) => "DB016",
            DbError::Internal(_) => "DB017",
            DbError::IoError(_) => "DB018",
            DbError::Hook(_) => "DB019",
            DbError::TenantError(_) => "DB020",
            DbError::Validation(_) => "DB021",
        }
    }

    /// 映射到 HTTP 状态码（RFC 7231）
    ///
    /// 用于在 HTTP 服务（如 axum/actix）中根据数据库错误返回合适的 HTTP 状态码。
    /// - 400 Bad Request：非法输入、参数校验失败、配置错误
    /// - 404 Not Found：资源未找到
    /// - 409 Conflict：资源已存在、约束冲突（唯一/外键/非空/通用）
    /// - 422 Unprocessable Entity：序列化/反序列化错误
    /// - 500 Internal Server Error：查询失败、内部错误、钩子失败、迁移失败、IO 错误、事务错误、租户错误
    /// - 501 Not Implemented：方言/功能不支持
    /// - 502 Bad Gateway：连接错误、连接被拒绝
    /// - 503 Service Unavailable：连接池耗尽/关闭、缓存错误
    /// - 504 Gateway Timeout：连接超时、连接池获取超时
    pub fn http_status(&self) -> u16 {
        match self {
            DbError::Contextual { source, .. } => source.http_status(),
            DbError::InvalidInput(_) | DbError::Validation(_) | DbError::ConfigError(_) => 400,
            DbError::NotFound(_) => 404,
            DbError::AlreadyExists(_)
            | DbError::ConstraintViolation(_)
            | DbError::UniqueViolation(_)
            | DbError::ForeignKeyViolation(_)
            | DbError::NullValue(_) => 409,
            DbError::SerdeError(_) => 422,
            DbError::Unsupported(_) => 501,
            DbError::ConnectionError(_) | DbError::ConnectionRefused(_) => 502,
            DbError::ConnectionTimeout(_) => 504,
            DbError::PoolError(e) => match e {
                PoolError::Timeout => 504,
                PoolError::Exhausted | PoolError::Closed | PoolError::ConnectionFailed(_) => 503,
                _ => 500,
            },
            DbError::CacheError(_) => 503,
            // QueryError/Internal/Hook/MigrationError/IoError/TxError/TenantError 均为服务端内部错误
            _ => 500,
        }
    }

    /// 映射到 gRPC 状态码
    ///
    /// 用于在 gRPC 服务（tonic）中根据数据库错误返回合适的 gRPC 状态码。
    /// 参考：https://grpc.io/docs/guides/status-codes/
    /// - 2 UNKNOWN：查询失败、内部错误、钩子失败、迁移失败、IO 错误
    /// - 3 INVALID_ARGUMENT：非法输入、参数校验失败、配置错误
    /// - 4 DEADLINE_EXCEEDED：连接超时、连接池获取超时
    /// - 5 NOT_FOUND：资源未找到
    /// - 6 ALREADY_EXISTS：资源已存在、唯一约束冲突
    /// - 7 PERMISSION_DENIED：租户错误（跨租户访问）
    /// - 8 RESOURCE_EXHAUSTED：连接池耗尽/关闭、缓存错误
    /// - 9 FAILED_PRECONDITION：约束冲突（通用/外键/非空）、事务错误
    /// - 12 UNIMPLEMENTED：方言/功能不支持
    /// - 13 INTERNAL：序列化/反序列化错误
    /// - 14 UNAVAILABLE：连接错误、连接被拒绝、连接创建失败
    pub fn grpc_status_code(&self) -> u32 {
        match self {
            DbError::Contextual { source, .. } => source.grpc_status_code(),
            DbError::InvalidInput(_) | DbError::Validation(_) | DbError::ConfigError(_) => 3,
            DbError::ConnectionTimeout(_) => 4,
            DbError::PoolError(PoolError::Timeout) => 4,
            DbError::NotFound(_) => 5,
            DbError::AlreadyExists(_) | DbError::UniqueViolation(_) => 6,
            DbError::TenantError(_) => 7,
            DbError::PoolError(PoolError::Exhausted) | DbError::PoolError(PoolError::Closed) => 8,
            DbError::CacheError(_) => 8,
            DbError::ConstraintViolation(_)
            | DbError::ForeignKeyViolation(_)
            | DbError::NullValue(_)
            | DbError::TxError(_) => 9,
            DbError::Unsupported(_) => 12,
            DbError::SerdeError(_) => 13,
            DbError::ConnectionError(_)
            | DbError::ConnectionRefused(_)
            | DbError::PoolError(PoolError::ConnectionFailed(_)) => 14,
            // QueryError/Internal/Hook/MigrationError/IoError/PoolError(其他) → UNKNOWN
            _ => 2,
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
            DbError::UniqueViolation(s) => write!(f, "Unique constraint violation: {}", s),
            DbError::ForeignKeyViolation(s) => write!(f, "Foreign key constraint violation: {}", s),
            DbError::NullValue(s) => write!(f, "Null value: {}", s),
            DbError::InvalidInput(s) => write!(f, "Invalid input: {}", s),
            DbError::Internal(s) => write!(f, "Internal error: {}", s),
            DbError::IoError(s) => write!(f, "IO error: {}", s),
            DbError::Hook(s) => write!(f, "Hook error: {}", s),
            DbError::TenantError(s) => write!(f, "Tenant error: {}", s),
            DbError::Validation(s) => write!(f, "Validation error: {}", s),
            DbError::Contextual {
                context, source, ..
            } => write!(f, "{}: {}", context.context, source),
        }
    }
}

impl Error for DbError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            DbError::PoolError(e) => Some(e),
            DbError::CacheError(e) => Some(e),
            DbError::TxError(e) => Some(e),
            // #6 修复：暴露 Contextual 包装的原始错误，使 std::error::Error::source()
            // 链式遍历可透过 Contextual 层到达根因
            DbError::Contextual { source, .. } => Some(source.as_ref()),
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

    /// #88 修复：断路器已跳闸，拒绝请求以防级联失败
    ///
    /// 当 `circuit-breaker` feature 启用且 `CircuitBreaker` 处于 `Open` 状态时，
    /// `acquire`/`query_with_timeout` 等方法会返回此错误，避免对下游数据库
    /// 造成更大压力。
    CircuitOpen,

    /// #93 修复：限流器拒绝请求
    ///
    /// 当 `rate-limit` feature 启用且 `RateLimiter` 拒绝当前 key 时返回。
    /// `remaining` 为本次窗口剩余配额（已为 0），`reset_at` 为窗口重置时间戳（毫秒）。
    RateLimited { remaining: u64, reset_at: i64 },
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
            PoolError::CircuitOpen => "PL009",
            PoolError::RateLimited { .. } => "PL010",
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
            PoolError::CircuitOpen => write!(f, "Circuit breaker open"),
            PoolError::RateLimited {
                remaining,
                reset_at,
            } => write!(
                f,
                "Rate limited (remaining: {}, reset_at: {})",
                remaining, reset_at
            ),
        }
    }
}

impl Error for PoolError {}

/// 缓存特有错误
#[derive(Debug, Clone)]
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
    fn test_db_error_contextual_source_chain() {
        // #6 修复验证：Contextual 错误的 source() 应直接指向根错误，
        // 上下文链通过 ErrorContext.previous 维护（不通过 source() 链）
        let root = DbError::QueryError("table not found".to_string());
        let wrapped = root.with_context("fetching user");
        let outer = wrapped.with_context("user_service.fetch");

        // 1. std::error::Error::source() 应直接返回根 QueryError（跳过 Contextual 层）
        let source1 = outer.source().expect("outer should have source");
        // source1 应为根 QueryError，不再有 source
        assert!(source1.source().is_none());

        // 2. 上下文链应有两层：外层 "user_service.fetch"，内层 "fetching user"
        let ctx_chain = outer.context().expect("outer should have context");
        assert_eq!(ctx_chain.context, "user_service.fetch");
        let inner_ctx = ctx_chain
            .previous
            .as_ref()
            .expect("should have previous context");
        assert_eq!(inner_ctx.context, "fetching user");
        assert!(inner_ctx.previous.is_none());

        // 3. root_cause() 应返回根 QueryError
        let root_cause = outer.root_cause();
        assert!(matches!(root_cause, DbError::QueryError(_)));
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

    #[test]
    fn test_error_hook_set_and_trigger() {
        use std::sync::atomic::{AtomicU32, Ordering};
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();
        set_error_hook(Arc::new(move |_err: &DbError| {
            c.fetch_add(1, Ordering::SeqCst);
        }));
        let err = DbError::query("hook test");
        trigger_error_hook(&err);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_error_hook_no_hook_silent() {
        // 不设置 hook 时 trigger 应静默跳过（不 panic）
        let err = DbError::query("no hook");
        trigger_error_hook(&err);
    }

    // ===== HTTP 状态码映射测试 =====

    #[test]
    fn test_http_status_bad_request() {
        assert_eq!(DbError::InvalidInput("bad".into()).http_status(), 400);
        assert_eq!(DbError::Validation("fail".into()).http_status(), 400);
        assert_eq!(DbError::ConfigError("cfg".into()).http_status(), 400);
    }

    #[test]
    fn test_http_status_not_found() {
        assert_eq!(DbError::NotFound("user".into()).http_status(), 404);
    }

    #[test]
    fn test_http_status_conflict() {
        assert_eq!(DbError::AlreadyExists("x".into()).http_status(), 409);
        assert_eq!(DbError::ConstraintViolation("c".into()).http_status(), 409);
        assert_eq!(DbError::UniqueViolation("u".into()).http_status(), 409);
        assert_eq!(DbError::ForeignKeyViolation("f".into()).http_status(), 409);
        assert_eq!(DbError::NullValue("n".into()).http_status(), 409);
    }

    #[test]
    fn test_http_status_unprocessable() {
        assert_eq!(DbError::SerdeError("s".into()).http_status(), 422);
    }

    #[test]
    fn test_http_status_internal_server_error() {
        assert_eq!(DbError::QueryError("q".into()).http_status(), 500);
        assert_eq!(DbError::Internal("i".into()).http_status(), 500);
        assert_eq!(DbError::Hook("h".into()).http_status(), 500);
        assert_eq!(DbError::MigrationError("m".into()).http_status(), 500);
        assert_eq!(DbError::IoError("io".into()).http_status(), 500);
        assert_eq!(DbError::TxError(TxError::NotStarted).http_status(), 500);
        assert_eq!(DbError::TenantError("t".into()).http_status(), 500);
    }

    #[test]
    fn test_http_status_not_implemented() {
        assert_eq!(DbError::Unsupported("feat".into()).http_status(), 501);
    }

    #[test]
    fn test_http_status_bad_gateway() {
        assert_eq!(DbError::ConnectionError("c".into()).http_status(), 502);
        assert_eq!(DbError::ConnectionRefused("r".into()).http_status(), 502);
    }

    #[test]
    fn test_http_status_service_unavailable() {
        assert_eq!(DbError::PoolError(PoolError::Exhausted).http_status(), 503);
        assert_eq!(DbError::PoolError(PoolError::Closed).http_status(), 503);
        assert_eq!(
            DbError::PoolError(PoolError::ConnectionFailed("f".into())).http_status(),
            503
        );
        assert_eq!(
            DbError::CacheError(CacheError::Internal("e".into())).http_status(),
            503
        );
    }

    #[test]
    fn test_http_status_gateway_timeout() {
        assert_eq!(DbError::ConnectionTimeout("t".into()).http_status(), 504);
        assert_eq!(DbError::PoolError(PoolError::Timeout).http_status(), 504);
    }

    // ===== gRPC 状态码映射测试 =====

    #[test]
    fn test_grpc_status_invalid_argument() {
        assert_eq!(DbError::InvalidInput("bad".into()).grpc_status_code(), 3);
        assert_eq!(DbError::Validation("fail".into()).grpc_status_code(), 3);
        assert_eq!(DbError::ConfigError("cfg".into()).grpc_status_code(), 3);
    }

    #[test]
    fn test_grpc_status_deadline_exceeded() {
        assert_eq!(DbError::ConnectionTimeout("t".into()).grpc_status_code(), 4);
        assert_eq!(DbError::PoolError(PoolError::Timeout).grpc_status_code(), 4);
    }

    #[test]
    fn test_grpc_status_not_found() {
        assert_eq!(DbError::NotFound("user".into()).grpc_status_code(), 5);
    }

    #[test]
    fn test_grpc_status_already_exists() {
        assert_eq!(DbError::AlreadyExists("x".into()).grpc_status_code(), 6);
        assert_eq!(DbError::UniqueViolation("u".into()).grpc_status_code(), 6);
    }

    #[test]
    fn test_grpc_status_permission_denied() {
        assert_eq!(DbError::TenantError("t".into()).grpc_status_code(), 7);
    }

    #[test]
    fn test_grpc_status_resource_exhausted() {
        assert_eq!(
            DbError::PoolError(PoolError::Exhausted).grpc_status_code(),
            8
        );
        assert_eq!(DbError::PoolError(PoolError::Closed).grpc_status_code(), 8);
        assert_eq!(
            DbError::CacheError(CacheError::Internal("e".into())).grpc_status_code(),
            8
        );
    }

    #[test]
    fn test_grpc_status_failed_precondition() {
        assert_eq!(
            DbError::ConstraintViolation("c".into()).grpc_status_code(),
            9
        );
        assert_eq!(
            DbError::ForeignKeyViolation("f".into()).grpc_status_code(),
            9
        );
        assert_eq!(DbError::NullValue("n".into()).grpc_status_code(), 9);
        assert_eq!(DbError::TxError(TxError::NotStarted).grpc_status_code(), 9);
    }

    #[test]
    fn test_grpc_status_unimplemented() {
        assert_eq!(DbError::Unsupported("feat".into()).grpc_status_code(), 12);
    }

    #[test]
    fn test_grpc_status_internal() {
        assert_eq!(DbError::SerdeError("s".into()).grpc_status_code(), 13);
    }

    #[test]
    fn test_grpc_status_unavailable() {
        assert_eq!(DbError::ConnectionError("c".into()).grpc_status_code(), 14);
        assert_eq!(
            DbError::ConnectionRefused("r".into()).grpc_status_code(),
            14
        );
        assert_eq!(
            DbError::PoolError(PoolError::ConnectionFailed("f".into())).grpc_status_code(),
            14
        );
    }

    #[test]
    fn test_grpc_status_unknown() {
        assert_eq!(DbError::QueryError("q".into()).grpc_status_code(), 2);
        assert_eq!(DbError::Internal("i".into()).grpc_status_code(), 2);
        assert_eq!(DbError::Hook("h".into()).grpc_status_code(), 2);
        assert_eq!(DbError::MigrationError("m".into()).grpc_status_code(), 2);
        assert_eq!(DbError::IoError("io".into()).grpc_status_code(), 2);
        // PoolError 的其他变体（AlreadyAcquired/NotAcquired/InvalidConfig/Internal）→ UNKNOWN
        assert_eq!(
            DbError::PoolError(PoolError::AlreadyAcquired).grpc_status_code(),
            2
        );
        assert_eq!(
            DbError::PoolError(PoolError::NotAcquired).grpc_status_code(),
            2
        );
        assert_eq!(
            DbError::PoolError(PoolError::InvalidConfig("x".into())).grpc_status_code(),
            2
        );
        assert_eq!(
            DbError::PoolError(PoolError::Internal("y".into())).grpc_status_code(),
            2
        );
    }
}
