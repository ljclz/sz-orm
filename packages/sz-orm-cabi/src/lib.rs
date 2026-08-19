//! # SZ-ORM C ABI — Cross-language FFI Export Layer
//!
//! Provides a unified C ABI interface for Go/Java/C++/Python, exposing the
//! core Model/QueryBuilder/Pool/Transaction APIs of sz-orm-core.
//!
//! ## Safety Guarantees
//!
//! - FFI memory is allocated/freed on the Rust side; the language side only
//!   holds handles
//! - Panics are caught and converted to error codes; no UB crosses the
//!   language boundary
//! - All `unsafe` blocks have `// SAFETY:` comments
//!
//! ## Exported Functions
//!
//! - [`sz_orm_pool_new`]: create a connection pool (real creation, based on
//!   the sz-orm-sqlx SQLite backend)
//! - [`sz_orm_pool_free`]: free a connection pool
//! - [`sz_orm_ping`]: connection pool health check
//! - [`sz_orm_query`]: execute a query and return rows as JSON
//! - [`sz_orm_execute`]: execute a write statement (INSERT/UPDATE/DELETE)
//! - [`sz_orm_version`]: version number

pub mod ffi_memory;
pub mod panic_guard;

use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::Arc;
use std::sync::OnceLock;

use sz_orm_core::{Pool, PoolConfig, PoolConfigBuilder, PooledConnection, Value};

/// Connection pool handle
pub type SzOrmPoolHandle = *mut c_void;

/// Query builder handle
pub type SzOrmQueryBuilderHandle = *mut c_void;

/// Transaction handle
pub type SzOrmTransactionHandle = *mut c_void;

/// Model handle
pub type SzOrmModelHandle = *mut c_void;

/// Error code
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SzOrmErrorCode {
    Ok = 0,
    NotFound = 1,
    ConnectionFailed = 2,
    QueryFailed = 3,
    PoolExhausted = 4,
    TransactionAborted = 5,
    Panic = 6,
    InvalidArgument = 7,
    RuntimeNotInitialized = 8,
    MemoryLeak = 9,
}

impl SzOrmErrorCode {
    pub fn as_i32(self) -> i32 {
        self as i32
    }

    pub fn from_i32(code: i32) -> Self {
        match code {
            0 => SzOrmErrorCode::Ok,
            1 => SzOrmErrorCode::NotFound,
            2 => SzOrmErrorCode::ConnectionFailed,
            3 => SzOrmErrorCode::QueryFailed,
            4 => SzOrmErrorCode::PoolExhausted,
            5 => SzOrmErrorCode::TransactionAborted,
            6 => SzOrmErrorCode::Panic,
            7 => SzOrmErrorCode::InvalidArgument,
            8 => SzOrmErrorCode::RuntimeNotInitialized,
            9 => SzOrmErrorCode::MemoryLeak,
            _ => SzOrmErrorCode::Panic,
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            SzOrmErrorCode::Ok => "success",
            SzOrmErrorCode::NotFound => "resource not found",
            SzOrmErrorCode::ConnectionFailed => "connection failed",
            SzOrmErrorCode::QueryFailed => "query execution failed",
            SzOrmErrorCode::PoolExhausted => "connection pool exhausted",
            SzOrmErrorCode::TransactionAborted => "transaction aborted",
            SzOrmErrorCode::Panic => "internal panic",
            SzOrmErrorCode::InvalidArgument => "invalid argument",
            SzOrmErrorCode::RuntimeNotInitialized => "tokio runtime not initialized",
            SzOrmErrorCode::MemoryLeak => "memory leak detected",
        }
    }
}

/// C ABI compatible connection pool configuration
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PoolConfigC {
    pub max_connections: u32,
    pub min_connections: u32,
    pub connect_timeout_ms: u64,
    pub idle_timeout_ms: u64,
}

impl Default for PoolConfigC {
    fn default() -> Self {
        Self {
            max_connections: 10,
            min_connections: 1,
            connect_timeout_ms: 5000,
            idle_timeout_ms: 300000,
        }
    }
}

/// C ABI compatible query result
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct QueryResultC {
    pub success: i32,
    pub error_code: i32,
    pub rows_affected: u64,
    pub last_insert_id: u64,
}

impl Default for QueryResultC {
    fn default() -> Self {
        Self {
            success: 1,
            error_code: SzOrmErrorCode::Ok.as_i32(),
            rows_affected: 0,
            last_insert_id: 0,
        }
    }
}

impl QueryResultC {
    fn error(code: SzOrmErrorCode) -> Self {
        Self {
            success: 0,
            error_code: code.as_i32(),
            rows_affected: 0,
            last_insert_id: 0,
        }
    }
}

/// Query rows JSON result (allocated on the Rust side; freed via
/// `sz_orm_query_free`)
pub struct QueryJsonResult {
    pub success: i32,
    pub error_code: i32,
    /// C string of the rows JSON (`[{"col":val},...]`); non-null on success
    pub json: *mut c_char,
}

/// Global tokio runtime (the C ABI synchronous interface needs block_on to
/// execute async pool operations)
fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("failed to build tokio runtime")
    })
}

/// Convert `PoolConfigC` to the `PoolConfig` of sz-orm-core
fn to_core_config(c: &PoolConfigC) -> Result<PoolConfig, sz_orm_core::PoolError> {
    PoolConfigBuilder::new()
        .max_size(c.max_connections.max(1))
        .min_idle(c.min_connections.min(c.max_connections.max(1)))
        .acquire_timeout(c.connect_timeout_ms / 1000)
        .idle_timeout(c.idle_timeout_ms / 1000)
        .build()
}

/// Create a connection pool (SQLite backend; real creation)
///
/// `dsn` supports SQLite URLs: `sqlite::memory:`, `sqlite://path/to/db.sqlite`
///
/// # Safety
///
/// SAFETY: `dsn` must be a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_pool_new(
    dsn: *const c_char,
    config: *const PoolConfigC,
) -> SzOrmPoolHandle {
    if dsn.is_null() {
        return std::ptr::null_mut();
    }
    let cfg = if config.is_null() {
        PoolConfigC::default()
    } else {
        // SAFETY: 调用方保证 config 指向有效的 PoolConfigC
        unsafe { *config }
    };
    let core_cfg = match to_core_config(&cfg) {
        Ok(c) => c,
        Err(_) => return std::ptr::null_mut(),
    };
    // SAFETY: 调用方保证 dsn 是有效的 C 字符串
    let dsn = match unsafe { CStr::from_ptr(dsn) }.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return std::ptr::null_mut(),
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime().block_on(async {
            // 连接 SQLite 并构建真实 Pool
            let handle = match sz_orm_sqlx::SqlitePoolHandle::connect(&dsn).await {
                Ok(h) => Arc::new(h),
                Err(_) => return Err(()),
            };
            let factory = Arc::new(sz_orm_sqlx::SqlxSqliteConnectionFactory::new(handle));
            match Pool::new(core_cfg, factory) {
                Ok(pool) => Ok(Box::into_raw(Box::new(pool)) as SzOrmPoolHandle),
                Err(_) => Err(()),
            }
        })
    }));

    match result {
        Ok(Ok(handle)) => handle,
        _ => std::ptr::null_mut(),
    }
}

/// Free a connection pool
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `sz_orm_pool_new` and
/// not yet freed.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_pool_free(handle: SzOrmPoolHandle) {
    if handle.is_null() {
        return;
    }
    // 在 tokio runtime 上下文中 drop Pool：PooledConnection::drop 需要
    // 通过 tokio::runtime::Handle::try_current() spawn 归还任务，
    // 若无 runtime 上下文 sqlx 连接 Drop 会 panic。
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime().block_on(async {
            // SAFETY: 调用方保证 handle 有效；此处消费 Box 触发 Drop
            unsafe {
                drop(Box::from_raw(handle as *mut Pool));
            }
        });
    }));
}

/// Connection pool health check (acquire + ping for real liveness probing)
///
/// Returns 1 = healthy, 0 = unhealthy
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `sz_orm_pool_new`.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_ping(handle: SzOrmPoolHandle) -> i32 {
    if handle.is_null() {
        return 0;
    }
    // SAFETY: 调用方保证 handle 有效
    let pool = unsafe { &*(handle as *const Pool) };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime().block_on(async {
            let mut conn = match pool.acquire().await {
                Ok(c) => c,
                Err(_) => return false,
            };
            conn.ping().await
        })
    }));
    match result {
        Ok(true) => 1,
        _ => 0,
    }
}

/// Execute a query and return rows as JSON
///
/// On success returns a non-null `QueryJsonResult`; the caller frees it with
/// `sz_orm_query_result_free`. Returns `nullptr` on invalid arguments or
/// internal panic.
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `sz_orm_pool_new`;
/// `sql` must be a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_query(
    handle: SzOrmPoolHandle,
    sql: *const c_char,
) -> *mut QueryJsonResult {
    if handle.is_null() || sql.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: 调用方保证 handle 有效
    let pool = unsafe { &*(handle as *const Pool) };
    // SAFETY: 调用方保证 sql 有效
    let sql = match unsafe { CStr::from_ptr(sql) }.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return std::ptr::null_mut(),
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime().block_on(async { pool.query_with_timeout(&sql).await })
    }));

    let (success, error_code, rows_json) = match result {
        Ok(Ok(rows)) => {
            let json = match serde_json::to_string(&rows) {
                Ok(j) => j,
                Err(_) => return err_result(SzOrmErrorCode::QueryFailed),
            };
            (1, SzOrmErrorCode::Ok.as_i32(), json)
        }
        Ok(Err(_)) => return err_result(SzOrmErrorCode::QueryFailed),
        Err(_) => return err_result(SzOrmErrorCode::Panic),
    };

    let json_c = match CString::new(rows_json) {
        Ok(c) => c,
        Err(_) => return err_result(SzOrmErrorCode::QueryFailed),
    };
    let result = Box::new(QueryJsonResult {
        success,
        error_code,
        json: json_c.into_raw(),
    });
    Box::into_raw(result)
}

fn err_result(code: SzOrmErrorCode) -> *mut QueryJsonResult {
    let result = Box::new(QueryJsonResult {
        success: 0,
        error_code: code.as_i32(),
        json: std::ptr::null_mut(),
    });
    Box::into_raw(result)
}

/// Free a query result
///
/// # Safety
///
/// SAFETY: `result` must be a pointer returned by `sz_orm_query` (or nullptr).
#[no_mangle]
pub unsafe extern "C" fn sz_orm_query_result_free(result: *mut QueryJsonResult) {
    if result.is_null() {
        return;
    }
    // SAFETY: 调用方保证 result 来自 sz_orm_query 且未释放
    let result = unsafe { Box::from_raw(result) };
    if !result.json.is_null() {
        // SAFETY: json 由 CString::into_raw 分配，此处配对释放
        unsafe {
            drop(CString::from_raw(result.json));
        }
    }
}

/// Execute a write statement (INSERT/UPDATE/DELETE) and return the number of
/// affected rows
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `sz_orm_pool_new`;
/// `sql` must be a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_execute(
    handle: SzOrmPoolHandle,
    sql: *const c_char,
) -> QueryResultC {
    if handle.is_null() || sql.is_null() {
        return QueryResultC::error(SzOrmErrorCode::InvalidArgument);
    }
    // SAFETY: 调用方保证 handle 有效
    let pool = unsafe { &*(handle as *const Pool) };
    // SAFETY: 调用方保证 sql 有效
    let sql = match unsafe { CStr::from_ptr(sql) }.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return QueryResultC::error(SzOrmErrorCode::InvalidArgument),
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime().block_on(async {
            let mut conn = match pool.acquire().await {
                Ok(c) => c,
                Err(_) => return QueryResultC::error(SzOrmErrorCode::PoolExhausted),
            };
            match conn.execute(&sql).await {
                Ok(rows) => QueryResultC {
                    success: 1,
                    error_code: SzOrmErrorCode::Ok.as_i32(),
                    rows_affected: rows,
                    last_insert_id: 0,
                },
                Err(_) => QueryResultC::error(SzOrmErrorCode::QueryFailed),
            }
        })
    }));

    match result {
        Ok(r) => r,
        Err(_) => QueryResultC::error(SzOrmErrorCode::Panic),
    }
}

/// Get the version number
#[no_mangle]
pub extern "C" fn sz_orm_version() -> u32 {
    env!("CARGO_PKG_VERSION")
        .split('.')
        .next()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
}

/// C ABI compatible connection pool status snapshot
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct PoolStatsC {
    pub idle: u32,
    pub active: u32,
    pub max: u32,
    pub min: u32,
    pub waiters: u32,
}

/// C ABI compatible connection pool cumulative metrics
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct PoolMetricsC {
    pub acquire_count: u64,
    pub acquire_failed_count: u64,
    pub release_count: u64,
    pub connection_created_count: u64,
    pub connection_closed_count: u64,
}

/// C ABI transaction handle internal storage
struct CabiTransaction {
    conn: Option<PooledConnection>,
    active: bool,
}

/// Get a connection pool status snapshot
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `sz_orm_pool_new`.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_pool_stats(handle: SzOrmPoolHandle) -> PoolStatsC {
    if handle.is_null() {
        return PoolStatsC::default();
    }
    // SAFETY: 调用方保证 handle 有效
    let pool = unsafe { &*(handle as *const Pool) };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime().block_on(async { pool.status().await })
    }));
    match result {
        Ok(s) => PoolStatsC {
            idle: s.idle,
            active: s.active,
            max: s.max,
            min: s.min,
            waiters: s.waiters,
        },
        Err(_) => PoolStatsC::default(),
    }
}

/// Get connection pool cumulative metrics
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `sz_orm_pool_new`.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_pool_metrics(handle: SzOrmPoolHandle) -> PoolMetricsC {
    if handle.is_null() {
        return PoolMetricsC::default();
    }
    // SAFETY: 调用方保证 handle 有效
    let pool = unsafe { &*(handle as *const Pool) };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| pool.pool_metrics()));
    match result {
        Ok(m) => PoolMetricsC {
            acquire_count: m.acquire_count,
            acquire_failed_count: m.acquire_failed_count,
            release_count: m.release_count,
            connection_created_count: m.connection_created_count,
            connection_closed_count: m.connection_closed_count,
        },
        Err(_) => PoolMetricsC::default(),
    }
}

/// Batch-execute multiple SQL statements (semicolon-separated) and return the
/// cumulative number of affected rows
///
/// If any SQL statement fails, an error is returned immediately and
/// subsequent SQL is not executed.
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `sz_orm_pool_new`;
/// `sql` must be a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_execute_batch(
    handle: SzOrmPoolHandle,
    sql: *const c_char,
) -> QueryResultC {
    if handle.is_null() || sql.is_null() {
        return QueryResultC::error(SzOrmErrorCode::InvalidArgument);
    }
    // SAFETY: 调用方保证 handle 有效
    let pool = unsafe { &*(handle as *const Pool) };
    // SAFETY: 调用方保证 sql 有效
    let sql = match unsafe { CStr::from_ptr(sql) }.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return QueryResultC::error(SzOrmErrorCode::InvalidArgument),
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime().block_on(async {
            let mut conn = match pool.acquire().await {
                Ok(c) => c,
                Err(_) => return QueryResultC::error(SzOrmErrorCode::PoolExhausted),
            };
            let mut total_rows: u64 = 0;
            for stmt in sql.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
                match conn.execute(stmt).await {
                    Ok(rows) => total_rows += rows,
                    Err(_) => return QueryResultC::error(SzOrmErrorCode::QueryFailed),
                }
            }
            QueryResultC {
                success: 1,
                error_code: SzOrmErrorCode::Ok.as_i32(),
                rows_affected: total_rows,
                last_insert_id: 0,
            }
        })
    }));

    match result {
        Ok(r) => r,
        Err(_) => QueryResultC::error(SzOrmErrorCode::Panic),
    }
}

/// Query a single row and return the first row as a JSON object (not an array)
///
/// On success returns a non-null `QueryJsonResult`; the caller frees it with
/// `sz_orm_query_result_free`. When there are no result rows, success=1 but
/// json is null.
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `sz_orm_pool_new`;
/// `sql` must be a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_query_one(
    handle: SzOrmPoolHandle,
    sql: *const c_char,
) -> *mut QueryJsonResult {
    if handle.is_null() || sql.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: 调用方保证 handle 有效
    let pool = unsafe { &*(handle as *const Pool) };
    // SAFETY: 调用方保证 sql 有效
    let sql = match unsafe { CStr::from_ptr(sql) }.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return std::ptr::null_mut(),
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime().block_on(async { pool.query_with_timeout(&sql).await })
    }));

    let rows = match result {
        Ok(Ok(rows)) => rows,
        Ok(Err(_)) => return err_result(SzOrmErrorCode::QueryFailed),
        Err(_) => return err_result(SzOrmErrorCode::Panic),
    };

    if rows.is_empty() {
        let result = Box::new(QueryJsonResult {
            success: 1,
            error_code: SzOrmErrorCode::Ok.as_i32(),
            json: std::ptr::null_mut(),
        });
        return Box::into_raw(result);
    }

    let json = match serde_json::to_string(&rows[0]) {
        Ok(j) => j,
        Err(_) => return err_result(SzOrmErrorCode::QueryFailed),
    };

    let json_c = match CString::new(json) {
        Ok(c) => c,
        Err(_) => return err_result(SzOrmErrorCode::QueryFailed),
    };

    let result = Box::new(QueryJsonResult {
        success: 1,
        error_code: SzOrmErrorCode::Ok.as_i32(),
        json: json_c.into_raw(),
    });
    Box::into_raw(result)
}

/// Check whether a table exists (SQLite backend)
///
/// Returns 1 = exists, 0 = does not exist, -1 = error
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `sz_orm_pool_new`;
/// `table` must be a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_table_exists(handle: SzOrmPoolHandle, table: *const c_char) -> i32 {
    if handle.is_null() || table.is_null() {
        return -1;
    }
    // SAFETY: 调用方保证 handle 有效
    let pool = unsafe { &*(handle as *const Pool) };
    // SAFETY: 调用方保证 table 有效
    let table = match unsafe { CStr::from_ptr(table) }.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return -1,
    };

    let sql = format!(
        "SELECT name FROM sqlite_master WHERE type='table' AND name='{}'",
        table.replace('\'', "''")
    );

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime().block_on(async { pool.query_with_timeout(&sql).await })
    }));

    match result {
        Ok(Ok(rows)) => {
            if rows.is_empty() {
                0
            } else {
                1
            }
        }
        _ => -1,
    }
}

/// Count the number of rows in a table
///
/// Returns the row count; returns -1 on error
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `sz_orm_pool_new`;
/// `table` must be a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_count(handle: SzOrmPoolHandle, table: *const c_char) -> i64 {
    if handle.is_null() || table.is_null() {
        return -1;
    }
    // SAFETY: 调用方保证 handle 有效
    let pool = unsafe { &*(handle as *const Pool) };
    // SAFETY: 调用方保证 table 有效
    let table = match unsafe { CStr::from_ptr(table) }.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return -1,
    };

    let sql = format!("SELECT * FROM {}", table);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime().block_on(async { pool.query_with_timeout(&sql).await })
    }));

    match result {
        Ok(Ok(rows)) => rows.len() as i64,
        _ => -1,
    }
}

/// Begin a transaction (holds an exclusive connection until
/// commit/rollback/free)
///
/// Returns a non-null handle on success, nullptr on failure
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `sz_orm_pool_new`.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_transaction_begin(
    handle: SzOrmPoolHandle,
) -> SzOrmTransactionHandle {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: 调用方保证 handle 有效
    let pool = unsafe { &*(handle as *const Pool) };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime().block_on(async {
            let mut conn = match pool.acquire().await {
                Ok(c) => c,
                Err(_) => return Err(()),
            };
            match conn.execute("BEGIN").await {
                Ok(_) => Ok(CabiTransaction {
                    conn: Some(conn),
                    active: true,
                }),
                Err(_) => Err(()),
            }
        })
    }));

    match result {
        Ok(Ok(tx)) => Box::into_raw(Box::new(tx)) as SzOrmTransactionHandle,
        _ => std::ptr::null_mut(),
    }
}

/// Execute SQL within a transaction
///
/// # Safety
///
/// SAFETY: `tx_handle` must be a valid handle returned by
/// `sz_orm_transaction_begin`; `sql` must be a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_transaction_execute(
    tx_handle: SzOrmTransactionHandle,
    sql: *const c_char,
) -> QueryResultC {
    if tx_handle.is_null() || sql.is_null() {
        return QueryResultC::error(SzOrmErrorCode::InvalidArgument);
    }
    // SAFETY: 调用方保证 tx_handle 有效
    let tx = unsafe { &mut *(tx_handle as *mut CabiTransaction) };
    if !tx.active {
        return QueryResultC::error(SzOrmErrorCode::TransactionAborted);
    }
    // SAFETY: 调用方保证 sql 有效
    let sql = match unsafe { CStr::from_ptr(sql) }.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return QueryResultC::error(SzOrmErrorCode::InvalidArgument),
    };

    let conn = match tx.conn.as_mut() {
        Some(c) => c,
        None => return QueryResultC::error(SzOrmErrorCode::TransactionAborted),
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime().block_on(async { conn.execute(&sql).await })
    }));

    match result {
        Ok(Ok(rows)) => QueryResultC {
            success: 1,
            error_code: SzOrmErrorCode::Ok.as_i32(),
            rows_affected: rows,
            last_insert_id: 0,
        },
        Ok(Err(_)) => QueryResultC::error(SzOrmErrorCode::QueryFailed),
        Err(_) => QueryResultC::error(SzOrmErrorCode::Panic),
    }
}

/// Commit a transaction
///
/// Returns 1 = success, 0 = failure
///
/// # Safety
///
/// SAFETY: `tx_handle` must be a valid handle returned by
/// `sz_orm_transaction_begin`.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_transaction_commit(tx_handle: SzOrmTransactionHandle) -> i32 {
    if tx_handle.is_null() {
        return 0;
    }
    // SAFETY: 调用方保证 tx_handle 有效
    let tx = unsafe { &mut *(tx_handle as *mut CabiTransaction) };
    if !tx.active {
        return 0;
    }
    let conn = match tx.conn.as_mut() {
        Some(c) => c,
        None => return 0,
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime().block_on(async { conn.execute("COMMIT").await })
    }));

    match result {
        Ok(Ok(_)) => {
            tx.active = false;
            1
        }
        _ => 0,
    }
}

/// Roll back a transaction
///
/// Returns 1 = success, 0 = failure
///
/// # Safety
///
/// SAFETY: `tx_handle` must be a valid handle returned by
/// `sz_orm_transaction_begin`.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_transaction_rollback(tx_handle: SzOrmTransactionHandle) -> i32 {
    if tx_handle.is_null() {
        return 0;
    }
    // SAFETY: 调用方保证 tx_handle 有效
    let tx = unsafe { &mut *(tx_handle as *mut CabiTransaction) };
    if !tx.active {
        return 0;
    }
    let conn = match tx.conn.as_mut() {
        Some(c) => c,
        None => return 0,
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime().block_on(async { conn.execute("ROLLBACK").await })
    }));

    match result {
        Ok(Ok(_)) => {
            tx.active = false;
            1
        }
        _ => 0,
    }
}

/// Free a transaction handle (automatically rolls back if the transaction is
/// still active)
///
/// # Safety
///
/// SAFETY: `tx_handle` must be a valid handle returned by
/// `sz_orm_transaction_begin` and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_transaction_free(tx_handle: SzOrmTransactionHandle) {
    if tx_handle.is_null() {
        return;
    }
    // SAFETY: 调用方保证 tx_handle 有效；此处消费 Box
    let mut tx = unsafe { Box::from_raw(tx_handle as *mut CabiTransaction) };
    // 在 tokio runtime 上下文中执行 rollback + drop 连接，
    // 确保 PooledConnection::drop 能 spawn 归还任务到 runtime
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime().block_on(async {
            if tx.active {
                tx.active = false;
                if let Some(conn) = tx.conn.as_mut() {
                    let _ = conn.execute("ROLLBACK").await;
                }
            }
            // 在 tokio 上下文中 take() 触发 PooledConnection::drop
            tx.conn.take();
        });
    }));
}

/// Get the error code description string (the caller must free it with
/// `sz_orm_string_free`)
///
/// # Safety
///
/// SAFETY: the returned pointer must be freed via `sz_orm_string_free`.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_error_description(code: i32) -> *mut c_char {
    let desc = SzOrmErrorCode::from_i32(code).description();
    match CString::new(desc) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Free the string returned by `sz_orm_error_description`
///
/// # Safety
///
/// SAFETY: `s` must be a pointer returned by `sz_orm_error_description`
/// (or nullptr).
#[no_mangle]
pub unsafe extern "C" fn sz_orm_string_free(s: *mut c_char) {
    if s.is_null() {
        return;
    }
    // SAFETY: 调用方保证 s 来自 sz_orm_error_description 且未释放
    unsafe {
        drop(CString::from_raw(s));
    }
}

// ============================================================================
// 模型级 API（REQ-BND-007 ~ REQ-BND-014）
//
// 4 个 pool 上的模型级导出 + 4 个 tx 上的模型级导出。
// 表名/字段名经 validate_identifier 校验（防 SQL 注入），
// 值通过 execute_with_params/query_with_params 参数绑定（防 SQL 注入）。
// ============================================================================

/// Validate the legality of a SQL identifier (table name / field name).
///
/// Rules: non-empty; the first character is a letter or underscore; the
/// remaining characters are letters/digits/underscores. Rejects inputs
/// containing `'`/`;`/`--`/spaces and other SQL injection vectors
/// (REQ-BND-011).
fn validate_identifier(name: &str) -> Result<(), SzOrmErrorCode> {
    if name.is_empty() {
        return Err(SzOrmErrorCode::InvalidArgument);
    }
    let mut chars = name.chars();
    // SAFETY: name 非空，next() 必返回 Some
    let first = chars.next().unwrap();
    if !(first.is_ascii_alphabetic() || first == '_') {
        return Err(SzOrmErrorCode::InvalidArgument);
    }
    for c in chars {
        if !(c.is_ascii_alphanumeric() || c == '_') {
            return Err(SzOrmErrorCode::InvalidArgument);
        }
    }
    Ok(())
}

/// Convert a `serde_json::Value` to an `sz_orm_core::Value` (for binding
/// parameters).
fn json_to_value(v: &serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::I64(i)
            } else if let Some(u) = n.as_u64() {
                Value::U64(u)
            } else if let Some(f) = n.as_f64() {
                Value::F64(f)
            } else {
                Value::Null
            }
        }
        serde_json::Value::String(s) => Value::String(s.clone()),
        serde_json::Value::Array(arr) => Value::Array(arr.iter().map(json_to_value).collect()),
        serde_json::Value::Object(obj) => {
            let mut map = std::collections::HashMap::with_capacity(obj.len());
            for (k, v) in obj {
                map.insert(k.clone(), json_to_value(v));
            }
            Value::Object(map)
        }
    }
}

/// Parse a field-name JSON array `["f1","f2"]` → `Vec<String>` and validate
/// each field name.
fn parse_fields_json(json: &str) -> Result<Vec<String>, SzOrmErrorCode> {
    let arr: Vec<String> =
        serde_json::from_str(json).map_err(|_| SzOrmErrorCode::InvalidArgument)?;
    if arr.is_empty() {
        return Err(SzOrmErrorCode::InvalidArgument);
    }
    for f in &arr {
        validate_identifier(f)?;
    }
    Ok(arr)
}

/// Parse a values JSON array `[v1,v2]` → `Vec<Value>`.
fn parse_values_json(json: &str) -> Result<Vec<Value>, SzOrmErrorCode> {
    let arr: Vec<serde_json::Value> =
        serde_json::from_str(json).map_err(|_| SzOrmErrorCode::InvalidArgument)?;
    Ok(arr.iter().map(json_to_value).collect())
}

/// Parse a set JSON object `{"f":v}` → `Vec<(String, Value)>` and validate
/// the field names.
fn parse_set_json(json: &str) -> Result<Vec<(String, Value)>, SzOrmErrorCode> {
    let obj: std::collections::HashMap<String, serde_json::Value> =
        serde_json::from_str(json).map_err(|_| SzOrmErrorCode::InvalidArgument)?;
    if obj.is_empty() {
        return Err(SzOrmErrorCode::InvalidArgument);
    }
    let mut out = Vec::with_capacity(obj.len());
    for (k, v) in obj {
        validate_identifier(&k)?;
        out.push((k, json_to_value(&v)));
    }
    Ok(out)
}

/// Build an INSERT SQL (parameterized). Table and field names are validated;
/// values are bound via `?` placeholders.
fn build_insert_sql(table: &str, fields: &[String]) -> String {
    let placeholders: Vec<&str> = fields.iter().map(|_| "?").collect();
    format!(
        "INSERT INTO {} ({}) VALUES ({})",
        table,
        fields.join(", "),
        placeholders.join(", ")
    )
}

/// Build an UPDATE SQL (parameterized). Both set values and where parameters
/// are bound via `?` placeholders.
fn build_update_sql(table: &str, set_fields: &[String], where_clause: &str) -> String {
    let sets: Vec<String> = set_fields.iter().map(|f| format!("{} = ?", f)).collect();
    if where_clause.is_empty() {
        format!("UPDATE {} SET {}", table, sets.join(", "))
    } else {
        format!(
            "UPDATE {} SET {} WHERE {}",
            table,
            sets.join(", "),
            where_clause
        )
    }
}

/// Build a DELETE SQL (parameterized). Where parameters are bound via `?`
/// placeholders.
fn build_delete_sql(table: &str, where_clause: &str) -> String {
    if where_clause.is_empty() {
        format!("DELETE FROM {}", table)
    } else {
        format!("DELETE FROM {} WHERE {}", table, where_clause)
    }
}

/// Build a SELECT SQL (parameterized). Where parameters are bound via `?`
/// placeholders.
fn build_select_sql(table: &str, where_clause: &str) -> String {
    if where_clause.is_empty() {
        format!("SELECT * FROM {}", table)
    } else {
        format!("SELECT * FROM {} WHERE {}", table, where_clause)
    }
}

/// Insert a row on a pool (parameterized, REQ-BND-007).
///
/// `fields_json` is a field-name array `["name","age"]`; `values_json` is the
/// corresponding values array `["Alice",30]`.
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `sz_orm_pool_new`;
/// `table`/`fields_json`/`values_json` must be valid NUL-terminated C strings.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_model_insert(
    handle: SzOrmPoolHandle,
    table: *const c_char,
    fields_json: *const c_char,
    values_json: *const c_char,
) -> QueryResultC {
    if handle.is_null() || table.is_null() || fields_json.is_null() || values_json.is_null() {
        return QueryResultC::error(SzOrmErrorCode::InvalidArgument);
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: 调用方保证 handle 有效
        let pool = unsafe { &*(handle as *const Pool) };
        // SAFETY: 调用方保证 C 字符串有效
        let table_s = unsafe { CStr::from_ptr(table) }
            .to_str()
            .map_err(|_| SzOrmErrorCode::InvalidArgument)?;
        let fields_s = unsafe { CStr::from_ptr(fields_json) }
            .to_str()
            .map_err(|_| SzOrmErrorCode::InvalidArgument)?;
        let values_s = unsafe { CStr::from_ptr(values_json) }
            .to_str()
            .map_err(|_| SzOrmErrorCode::InvalidArgument)?;

        validate_identifier(table_s)?;
        let fields = parse_fields_json(fields_s)?;
        let values = parse_values_json(values_s)?;
        if fields.len() != values.len() {
            return Err(SzOrmErrorCode::InvalidArgument);
        }
        let sql = build_insert_sql(table_s, &fields);

        runtime().block_on(async {
            let mut conn = pool
                .acquire()
                .await
                .map_err(|_| SzOrmErrorCode::PoolExhausted)?;
            conn.execute_with_params(&sql, &values)
                .await
                .map(|rows| QueryResultC {
                    success: 1,
                    error_code: SzOrmErrorCode::Ok.as_i32(),
                    rows_affected: rows,
                    last_insert_id: 0,
                })
                .map_err(|_| SzOrmErrorCode::QueryFailed)
        })
    }));

    match result {
        Ok(Ok(r)) => r,
        Ok(Err(code)) => QueryResultC::error(code),
        Err(_) => QueryResultC::error(SzOrmErrorCode::Panic),
    }
}

/// Update rows on a pool (parameterized, REQ-BND-007).
///
/// `set_json` is an object `{"name":"Alice"}`; `where_clause` is a
/// parameterized condition `id = ?`; `where_params_json` is a parameter
/// array `[1]`.
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `sz_orm_pool_new`;
/// `table`/`set_json`/`where_clause`/`where_params_json` must be valid
/// NUL-terminated C strings.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_model_update(
    handle: SzOrmPoolHandle,
    table: *const c_char,
    set_json: *const c_char,
    where_clause: *const c_char,
    where_params_json: *const c_char,
) -> QueryResultC {
    if handle.is_null() || table.is_null() || set_json.is_null() {
        return QueryResultC::error(SzOrmErrorCode::InvalidArgument);
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: 调用方保证 handle 有效
        let pool = unsafe { &*(handle as *const Pool) };
        // SAFETY: 调用方保证 C 字符串有效
        let table_s = unsafe { CStr::from_ptr(table) }
            .to_str()
            .map_err(|_| SzOrmErrorCode::InvalidArgument)?;
        let set_s = unsafe { CStr::from_ptr(set_json) }
            .to_str()
            .map_err(|_| SzOrmErrorCode::InvalidArgument)?;
        let where_s = if where_clause.is_null() {
            String::new()
        } else {
            unsafe { CStr::from_ptr(where_clause) }
                .to_str()
                .map_err(|_| SzOrmErrorCode::InvalidArgument)?
                .to_string()
        };
        let where_params_s = if where_params_json.is_null() {
            "[]"
        } else {
            unsafe { CStr::from_ptr(where_params_json) }
                .to_str()
                .map_err(|_| SzOrmErrorCode::InvalidArgument)?
        };

        validate_identifier(table_s)?;
        let set_pairs = parse_set_json(set_s)?;
        let where_params = parse_values_json(where_params_s)?;
        let set_fields: Vec<String> = set_pairs.iter().map(|(f, _)| f.clone()).collect();
        let mut all_params: Vec<Value> = set_pairs.into_iter().map(|(_, v)| v).collect();
        all_params.extend(where_params);
        let sql = build_update_sql(table_s, &set_fields, &where_s);

        runtime().block_on(async {
            let mut conn = pool
                .acquire()
                .await
                .map_err(|_| SzOrmErrorCode::PoolExhausted)?;
            conn.execute_with_params(&sql, &all_params)
                .await
                .map(|rows| QueryResultC {
                    success: 1,
                    error_code: SzOrmErrorCode::Ok.as_i32(),
                    rows_affected: rows,
                    last_insert_id: 0,
                })
                .map_err(|_| SzOrmErrorCode::QueryFailed)
        })
    }));

    match result {
        Ok(Ok(r)) => r,
        Ok(Err(code)) => QueryResultC::error(code),
        Err(_) => QueryResultC::error(SzOrmErrorCode::Panic),
    }
}

/// Delete rows on a pool (parameterized, REQ-BND-007).
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `sz_orm_pool_new`;
/// `table`/`where_clause`/`where_params_json` must be valid NUL-terminated C
/// strings.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_model_delete(
    handle: SzOrmPoolHandle,
    table: *const c_char,
    where_clause: *const c_char,
    where_params_json: *const c_char,
) -> QueryResultC {
    if handle.is_null() || table.is_null() {
        return QueryResultC::error(SzOrmErrorCode::InvalidArgument);
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: 调用方保证 handle 有效
        let pool = unsafe { &*(handle as *const Pool) };
        // SAFETY: 调用方保证 C 字符串有效
        let table_s = unsafe { CStr::from_ptr(table) }
            .to_str()
            .map_err(|_| SzOrmErrorCode::InvalidArgument)?;
        let where_s = if where_clause.is_null() {
            String::new()
        } else {
            unsafe { CStr::from_ptr(where_clause) }
                .to_str()
                .map_err(|_| SzOrmErrorCode::InvalidArgument)?
                .to_string()
        };
        let where_params_s = if where_params_json.is_null() {
            "[]"
        } else {
            unsafe { CStr::from_ptr(where_params_json) }
                .to_str()
                .map_err(|_| SzOrmErrorCode::InvalidArgument)?
        };

        validate_identifier(table_s)?;
        let where_params = parse_values_json(where_params_s)?;
        let sql = build_delete_sql(table_s, &where_s);

        runtime().block_on(async {
            let mut conn = pool
                .acquire()
                .await
                .map_err(|_| SzOrmErrorCode::PoolExhausted)?;
            conn.execute_with_params(&sql, &where_params)
                .await
                .map(|rows| QueryResultC {
                    success: 1,
                    error_code: SzOrmErrorCode::Ok.as_i32(),
                    rows_affected: rows,
                    last_insert_id: 0,
                })
                .map_err(|_| SzOrmErrorCode::QueryFailed)
        })
    }));

    match result {
        Ok(Ok(r)) => r,
        Ok(Err(code)) => QueryResultC::error(code),
        Err(_) => QueryResultC::error(SzOrmErrorCode::Panic),
    }
}

/// Query rows on a pool and return a JSON row-array string (parameterized,
/// REQ-BND-007).
///
/// On success returns a non-null `*mut c_char` (the caller frees it with
/// `sz_orm_string_free`); null indicates failure. Returns `[]` when no rows
/// match.
///
/// # Safety
///
/// SAFETY: `handle` must be a valid handle returned by `sz_orm_pool_new`;
/// `table`/`where_clause`/`where_params_json` must be valid NUL-terminated C
/// strings.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_model_find(
    handle: SzOrmPoolHandle,
    table: *const c_char,
    where_clause: *const c_char,
    where_params_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || table.is_null() {
        return std::ptr::null_mut();
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: 调用方保证 handle 有效
        let pool = unsafe { &*(handle as *const Pool) };
        // SAFETY: 调用方保证 C 字符串有效
        let table_s = unsafe { CStr::from_ptr(table) }
            .to_str()
            .map_err(|_| SzOrmErrorCode::InvalidArgument)?;
        let where_s = if where_clause.is_null() {
            String::new()
        } else {
            unsafe { CStr::from_ptr(where_clause) }
                .to_str()
                .map_err(|_| SzOrmErrorCode::InvalidArgument)?
                .to_string()
        };
        let where_params_s = if where_params_json.is_null() {
            "[]"
        } else {
            unsafe { CStr::from_ptr(where_params_json) }
                .to_str()
                .map_err(|_| SzOrmErrorCode::InvalidArgument)?
        };

        validate_identifier(table_s)?;
        let where_params = parse_values_json(where_params_s)?;
        let sql = build_select_sql(table_s, &where_s);

        runtime().block_on(async {
            let mut conn = pool
                .acquire()
                .await
                .map_err(|_| SzOrmErrorCode::PoolExhausted)?;
            conn.query_with_params(&sql, &where_params)
                .await
                .map_err(|_| SzOrmErrorCode::QueryFailed)
        })
    }));

    let rows = match result {
        Ok(Ok(rows)) => rows,
        _ => return std::ptr::null_mut(),
    };

    let json = match serde_json::to_string(&rows) {
        Ok(j) => j,
        Err(_) => return std::ptr::null_mut(),
    };
    match CString::new(json) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Insert a row within a transaction (parameterized, REQ-BND-014).
///
/// # Safety
///
/// SAFETY: `tx_handle` must be a valid handle returned by
/// `sz_orm_transaction_begin`; `table`/`fields_json`/`values_json` must be
/// valid NUL-terminated C strings.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_model_insert_tx(
    tx_handle: SzOrmTransactionHandle,
    table: *const c_char,
    fields_json: *const c_char,
    values_json: *const c_char,
) -> QueryResultC {
    if tx_handle.is_null() || table.is_null() || fields_json.is_null() || values_json.is_null() {
        return QueryResultC::error(SzOrmErrorCode::InvalidArgument);
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: 调用方保证 tx_handle 有效
        let tx = unsafe { &mut *(tx_handle as *mut CabiTransaction) };
        if !tx.active {
            return Err(SzOrmErrorCode::TransactionAborted);
        }
        // SAFETY: 调用方保证 C 字符串有效
        let table_s = unsafe { CStr::from_ptr(table) }
            .to_str()
            .map_err(|_| SzOrmErrorCode::InvalidArgument)?;
        let fields_s = unsafe { CStr::from_ptr(fields_json) }
            .to_str()
            .map_err(|_| SzOrmErrorCode::InvalidArgument)?;
        let values_s = unsafe { CStr::from_ptr(values_json) }
            .to_str()
            .map_err(|_| SzOrmErrorCode::InvalidArgument)?;

        validate_identifier(table_s)?;
        let fields = parse_fields_json(fields_s)?;
        let values = parse_values_json(values_s)?;
        if fields.len() != values.len() {
            return Err(SzOrmErrorCode::InvalidArgument);
        }
        let sql = build_insert_sql(table_s, &fields);

        let conn = tx.conn.as_mut().ok_or(SzOrmErrorCode::TransactionAborted)?;
        runtime().block_on(async {
            conn.execute_with_params(&sql, &values)
                .await
                .map(|rows| QueryResultC {
                    success: 1,
                    error_code: SzOrmErrorCode::Ok.as_i32(),
                    rows_affected: rows,
                    last_insert_id: 0,
                })
                .map_err(|_| SzOrmErrorCode::QueryFailed)
        })
    }));

    match result {
        Ok(Ok(r)) => r,
        Ok(Err(code)) => QueryResultC::error(code),
        Err(_) => QueryResultC::error(SzOrmErrorCode::Panic),
    }
}

/// Update rows within a transaction (parameterized, REQ-BND-014).
///
/// # Safety
///
/// SAFETY: `tx_handle` must be a valid handle returned by
/// `sz_orm_transaction_begin`;
/// `table`/`set_json`/`where_clause`/`where_params_json` must be valid
/// NUL-terminated C strings.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_model_update_tx(
    tx_handle: SzOrmTransactionHandle,
    table: *const c_char,
    set_json: *const c_char,
    where_clause: *const c_char,
    where_params_json: *const c_char,
) -> QueryResultC {
    if tx_handle.is_null() || table.is_null() || set_json.is_null() {
        return QueryResultC::error(SzOrmErrorCode::InvalidArgument);
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: 调用方保证 tx_handle 有效
        let tx = unsafe { &mut *(tx_handle as *mut CabiTransaction) };
        if !tx.active {
            return Err(SzOrmErrorCode::TransactionAborted);
        }
        // SAFETY: 调用方保证 C 字符串有效
        let table_s = unsafe { CStr::from_ptr(table) }
            .to_str()
            .map_err(|_| SzOrmErrorCode::InvalidArgument)?;
        let set_s = unsafe { CStr::from_ptr(set_json) }
            .to_str()
            .map_err(|_| SzOrmErrorCode::InvalidArgument)?;
        let where_s = if where_clause.is_null() {
            String::new()
        } else {
            unsafe { CStr::from_ptr(where_clause) }
                .to_str()
                .map_err(|_| SzOrmErrorCode::InvalidArgument)?
                .to_string()
        };
        let where_params_s = if where_params_json.is_null() {
            "[]"
        } else {
            unsafe { CStr::from_ptr(where_params_json) }
                .to_str()
                .map_err(|_| SzOrmErrorCode::InvalidArgument)?
        };

        validate_identifier(table_s)?;
        let set_pairs = parse_set_json(set_s)?;
        let where_params = parse_values_json(where_params_s)?;
        let set_fields: Vec<String> = set_pairs.iter().map(|(f, _)| f.clone()).collect();
        let mut all_params: Vec<Value> = set_pairs.into_iter().map(|(_, v)| v).collect();
        all_params.extend(where_params);
        let sql = build_update_sql(table_s, &set_fields, &where_s);

        let conn = tx.conn.as_mut().ok_or(SzOrmErrorCode::TransactionAborted)?;
        runtime().block_on(async {
            conn.execute_with_params(&sql, &all_params)
                .await
                .map(|rows| QueryResultC {
                    success: 1,
                    error_code: SzOrmErrorCode::Ok.as_i32(),
                    rows_affected: rows,
                    last_insert_id: 0,
                })
                .map_err(|_| SzOrmErrorCode::QueryFailed)
        })
    }));

    match result {
        Ok(Ok(r)) => r,
        Ok(Err(code)) => QueryResultC::error(code),
        Err(_) => QueryResultC::error(SzOrmErrorCode::Panic),
    }
}

/// Delete rows within a transaction (parameterized, REQ-BND-014).
///
/// # Safety
///
/// SAFETY: `tx_handle` must be a valid handle returned by
/// `sz_orm_transaction_begin`; `table`/`where_clause`/`where_params_json` must
/// be valid NUL-terminated C strings.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_model_delete_tx(
    tx_handle: SzOrmTransactionHandle,
    table: *const c_char,
    where_clause: *const c_char,
    where_params_json: *const c_char,
) -> QueryResultC {
    if tx_handle.is_null() || table.is_null() {
        return QueryResultC::error(SzOrmErrorCode::InvalidArgument);
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: 调用方保证 tx_handle 有效
        let tx = unsafe { &mut *(tx_handle as *mut CabiTransaction) };
        if !tx.active {
            return Err(SzOrmErrorCode::TransactionAborted);
        }
        // SAFETY: 调用方保证 C 字符串有效
        let table_s = unsafe { CStr::from_ptr(table) }
            .to_str()
            .map_err(|_| SzOrmErrorCode::InvalidArgument)?;
        let where_s = if where_clause.is_null() {
            String::new()
        } else {
            unsafe { CStr::from_ptr(where_clause) }
                .to_str()
                .map_err(|_| SzOrmErrorCode::InvalidArgument)?
                .to_string()
        };
        let where_params_s = if where_params_json.is_null() {
            "[]"
        } else {
            unsafe { CStr::from_ptr(where_params_json) }
                .to_str()
                .map_err(|_| SzOrmErrorCode::InvalidArgument)?
        };

        validate_identifier(table_s)?;
        let where_params = parse_values_json(where_params_s)?;
        let sql = build_delete_sql(table_s, &where_s);

        let conn = tx.conn.as_mut().ok_or(SzOrmErrorCode::TransactionAborted)?;
        runtime().block_on(async {
            conn.execute_with_params(&sql, &where_params)
                .await
                .map(|rows| QueryResultC {
                    success: 1,
                    error_code: SzOrmErrorCode::Ok.as_i32(),
                    rows_affected: rows,
                    last_insert_id: 0,
                })
                .map_err(|_| SzOrmErrorCode::QueryFailed)
        })
    }));

    match result {
        Ok(Ok(r)) => r,
        Ok(Err(code)) => QueryResultC::error(code),
        Err(_) => QueryResultC::error(SzOrmErrorCode::Panic),
    }
}

/// Query rows within a transaction and return a JSON row-array string
/// (parameterized, REQ-BND-014).
///
/// On success returns a non-null `*mut c_char` (the caller frees it with
/// `sz_orm_string_free`); null indicates failure.
///
/// # Safety
///
/// SAFETY: `tx_handle` must be a valid handle returned by
/// `sz_orm_transaction_begin`; `table`/`where_clause`/`where_params_json` must
/// be valid NUL-terminated C strings.
#[no_mangle]
pub unsafe extern "C" fn sz_orm_model_find_tx(
    tx_handle: SzOrmTransactionHandle,
    table: *const c_char,
    where_clause: *const c_char,
    where_params_json: *const c_char,
) -> *mut c_char {
    if tx_handle.is_null() || table.is_null() {
        return std::ptr::null_mut();
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: 调用方保证 tx_handle 有效
        let tx = unsafe { &mut *(tx_handle as *mut CabiTransaction) };
        if !tx.active {
            return Err(SzOrmErrorCode::TransactionAborted);
        }
        // SAFETY: 调用方保证 C 字符串有效
        let table_s = unsafe { CStr::from_ptr(table) }
            .to_str()
            .map_err(|_| SzOrmErrorCode::InvalidArgument)?;
        let where_s = if where_clause.is_null() {
            String::new()
        } else {
            unsafe { CStr::from_ptr(where_clause) }
                .to_str()
                .map_err(|_| SzOrmErrorCode::InvalidArgument)?
                .to_string()
        };
        let where_params_s = if where_params_json.is_null() {
            "[]"
        } else {
            unsafe { CStr::from_ptr(where_params_json) }
                .to_str()
                .map_err(|_| SzOrmErrorCode::InvalidArgument)?
        };

        validate_identifier(table_s)?;
        let where_params = parse_values_json(where_params_s)?;
        let sql = build_select_sql(table_s, &where_s);

        let conn = tx.conn.as_mut().ok_or(SzOrmErrorCode::TransactionAborted)?;
        runtime().block_on(async {
            conn.query_with_params(&sql, &where_params)
                .await
                .map_err(|_| SzOrmErrorCode::QueryFailed)
        })
    }));

    let rows = match result {
        Ok(Ok(rows)) => rows,
        _ => return std::ptr::null_mut(),
    };

    let json = match serde_json::to_string(&rows) {
        Ok(j) => j,
        Err(_) => return std::ptr::null_mut(),
    };
    match CString::new(json) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    fn test_pool() -> SzOrmPoolHandle {
        let dsn = CString::new("sqlite::memory:").unwrap();
        // 强制 max_connections=1：SQLite :memory: 每连接独立内存数据库，
        // 池只有 1 个连接时所有操作共享同一内存数据库
        let config = PoolConfigC {
            max_connections: 1,
            min_connections: 1,
            ..Default::default()
        };
        // SAFETY: dsn 是有效 C 字符串
        unsafe { sz_orm_pool_new(dsn.as_ptr(), &config) }
    }

    #[test]
    fn test_error_code_roundtrip() {
        for code in 0..=9 {
            let ec = SzOrmErrorCode::from_i32(code);
            assert_eq!(ec.as_i32(), code);
        }
    }

    #[test]
    fn test_error_code_unknown_defaults_to_panic() {
        let ec = SzOrmErrorCode::from_i32(999);
        assert_eq!(ec, SzOrmErrorCode::Panic);
    }

    #[test]
    fn test_error_code_description() {
        assert_eq!(SzOrmErrorCode::Ok.description(), "success");
        assert_eq!(SzOrmErrorCode::Panic.description(), "internal panic");
        assert_eq!(
            SzOrmErrorCode::ConnectionFailed.description(),
            "connection failed"
        );
    }

    #[test]
    fn test_pool_config_default() {
        let config = PoolConfigC::default();
        assert_eq!(config.max_connections, 10);
        assert_eq!(config.min_connections, 1);
        assert_eq!(config.connect_timeout_ms, 5000);
        assert_eq!(config.idle_timeout_ms, 300000);
    }

    #[test]
    fn test_query_result_default() {
        let result = QueryResultC::default();
        assert_eq!(result.success, 1);
        assert_eq!(result.error_code, 0);
        assert_eq!(result.rows_affected, 0);
    }

    #[test]
    fn test_pool_new_creates_real_pool() {
        let pool = test_pool();
        assert!(!pool.is_null(), "pool_new should create a real pool");
        // SAFETY: pool 由 test_pool 创建
        unsafe { sz_orm_pool_free(pool) };
    }

    #[test]
    fn test_ping_healthy() {
        let pool = test_pool();
        assert!(!pool.is_null());
        // SAFETY: pool 有效
        let healthy = unsafe { sz_orm_ping(pool) };
        assert_eq!(healthy, 1, "fresh pool should be healthy");
        // SAFETY: pool 有效
        unsafe { sz_orm_pool_free(pool) };
    }

    #[test]
    fn test_execute_and_query_roundtrip() {
        let pool = test_pool();
        assert!(!pool.is_null());

        // 建表 + 插入
        let create =
            CString::new("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)").unwrap();
        // SAFETY: pool/sql 有效
        let r = unsafe { sz_orm_execute(pool, create.as_ptr()) };
        assert_eq!(
            r.success, 1,
            "CREATE should succeed, got code {}",
            r.error_code
        );

        let insert = CString::new("INSERT INTO users (name) VALUES ('Alice'), ('Bob')").unwrap();
        // SAFETY: pool/sql 有效
        let r = unsafe { sz_orm_execute(pool, insert.as_ptr()) };
        assert_eq!(r.success, 1);
        assert_eq!(r.rows_affected, 2, "two rows inserted");

        // 查询
        let q = CString::new("SELECT id, name FROM users ORDER BY id").unwrap();
        // SAFETY: pool/sql 有效
        let result = unsafe { sz_orm_query(pool, q.as_ptr()) };
        assert!(!result.is_null());
        // SAFETY: result 非空
        let result = unsafe { Box::from_raw(result) };
        assert_eq!(result.success, 1);
        let json = if result.json.is_null() {
            String::new()
        } else {
            // SAFETY: json 有效 C 字符串
            unsafe { CStr::from_ptr(result.json) }
                .to_string_lossy()
                .into_owned()
        };
        assert!(
            json.contains("Alice"),
            "query rows should contain Alice: {json}"
        );
        assert!(
            json.contains("Bob"),
            "query rows should contain Bob: {json}"
        );

        // SAFETY: result 由 sz_orm_query 分配，此处配对释放
        unsafe { sz_orm_query_result_free(Box::into_raw(result)) };
        // SAFETY: pool 有效
        unsafe { sz_orm_pool_free(pool) };
    }

    #[test]
    fn test_query_invalid_sql_returns_error() {
        let pool = test_pool();
        assert!(!pool.is_null());
        let q = CString::new("SELECT * FROM nonexistent_table").unwrap();
        // SAFETY: pool/sql 有效
        let result = unsafe { sz_orm_query(pool, q.as_ptr()) };
        assert!(!result.is_null());
        // SAFETY: result 非空
        let result = unsafe { Box::from_raw(result) };
        assert_eq!(result.success, 0, "invalid SQL should fail");
        assert_eq!(result.error_code, SzOrmErrorCode::QueryFailed.as_i32());
        // SAFETY: result 配对释放
        unsafe { sz_orm_query_result_free(Box::into_raw(result)) };
        // SAFETY: pool 有效
        unsafe { sz_orm_pool_free(pool) };
    }

    #[test]
    fn test_pool_new_null_dsn_returns_null() {
        // SAFETY: null 指针合法调用，应返回 null
        let pool = unsafe { sz_orm_pool_new(std::ptr::null(), std::ptr::null()) };
        assert!(pool.is_null());
    }

    #[test]
    fn test_execute_null_returns_invalid_argument() {
        // SAFETY: null 指针合法调用
        let r = unsafe { sz_orm_execute(std::ptr::null_mut(), std::ptr::null()) };
        assert_eq!(r.success, 0);
        assert_eq!(r.error_code, SzOrmErrorCode::InvalidArgument.as_i32());
    }

    #[test]
    fn test_version() {
        let v = sz_orm_version();
        assert!(v >= 1, "version should be >= 1, got {v}");
    }

    // ===== 新增测试：PoolStatsC / PoolMetricsC =====

    #[test]
    fn test_pool_stats_c_default() {
        let s = PoolStatsC::default();
        assert_eq!(s.idle, 0);
        assert_eq!(s.active, 0);
        assert_eq!(s.max, 0);
        assert_eq!(s.min, 0);
        assert_eq!(s.waiters, 0);
    }

    #[test]
    fn test_pool_metrics_c_default() {
        let m = PoolMetricsC::default();
        assert_eq!(m.acquire_count, 0);
        assert_eq!(m.acquire_failed_count, 0);
        assert_eq!(m.release_count, 0);
        assert_eq!(m.connection_created_count, 0);
        assert_eq!(m.connection_closed_count, 0);
    }

    #[test]
    fn test_pool_stats_null_returns_default() {
        // SAFETY: null 指针合法调用
        let s = unsafe { sz_orm_pool_stats(std::ptr::null_mut()) };
        assert_eq!(s.idle, 0);
        assert_eq!(s.max, 0);
    }

    #[test]
    fn test_pool_metrics_null_returns_default() {
        // SAFETY: null 指针合法调用
        let m = unsafe { sz_orm_pool_metrics(std::ptr::null_mut()) };
        assert_eq!(m.acquire_count, 0);
    }

    #[test]
    fn test_pool_stats_real_pool() {
        let pool = test_pool();
        assert!(!pool.is_null());
        // SAFETY: pool 有效
        let s = unsafe { sz_orm_pool_stats(pool) };
        assert!(s.max >= 1, "max should be >= 1, got {}", s.max);
        // SAFETY: pool 有效
        unsafe { sz_orm_pool_free(pool) };
    }

    #[test]
    fn test_pool_metrics_real_pool() {
        let pool = test_pool();
        assert!(!pool.is_null());
        // 先 ping 一次产生 acquire 指标
        // SAFETY: pool 有效
        let _ = unsafe { sz_orm_ping(pool) };
        // SAFETY: pool 有效
        let m = unsafe { sz_orm_pool_metrics(pool) };
        assert!(
            m.acquire_count >= 1,
            "acquire_count should be >= 1 after ping"
        );
        // SAFETY: pool 有效
        unsafe { sz_orm_pool_free(pool) };
    }

    // ===== 新增测试：sz_orm_execute_batch =====

    #[test]
    fn test_execute_batch_null_returns_error() {
        // SAFETY: null 指针合法调用
        let r = unsafe { sz_orm_execute_batch(std::ptr::null_mut(), std::ptr::null()) };
        assert_eq!(r.success, 0);
        assert_eq!(r.error_code, SzOrmErrorCode::InvalidArgument.as_i32());
    }

    #[test]
    fn test_execute_batch_multiple_statements() {
        let pool = test_pool();
        assert!(!pool.is_null());
        let batch = CString::new(
            "CREATE TABLE batch_t (id INTEGER PRIMARY KEY, val TEXT);\
             INSERT INTO batch_t (val) VALUES ('a');\
             INSERT INTO batch_t (val) VALUES ('b');\
             INSERT INTO batch_t (val) VALUES ('c')",
        )
        .unwrap();
        // SAFETY: pool/sql 有效
        let r = unsafe { sz_orm_execute_batch(pool, batch.as_ptr()) };
        assert_eq!(r.success, 1, "batch should succeed");
        assert_eq!(r.rows_affected, 3, "3 rows inserted");
        // SAFETY: pool 有效
        unsafe { sz_orm_pool_free(pool) };
    }

    #[test]
    fn test_execute_batch_empty_statements() {
        let pool = test_pool();
        assert!(!pool.is_null());
        let batch = CString::new("  ;  ;  ").unwrap();
        // SAFETY: pool/sql 有效
        let r = unsafe { sz_orm_execute_batch(pool, batch.as_ptr()) };
        assert_eq!(r.success, 1, "empty batch should succeed");
        assert_eq!(r.rows_affected, 0);
        // SAFETY: pool 有效
        unsafe { sz_orm_pool_free(pool) };
    }

    // ===== 新增测试：sz_orm_query_one =====

    #[test]
    fn test_query_one_returns_first_row() {
        let pool = test_pool();
        assert!(!pool.is_null());
        let create = CString::new("CREATE TABLE qo_t (id INTEGER PRIMARY KEY, name TEXT)").unwrap();
        // SAFETY: pool/sql 有效
        unsafe { sz_orm_execute(pool, create.as_ptr()) };
        let insert = CString::new("INSERT INTO qo_t (name) VALUES ('Alice'), ('Bob')").unwrap();
        // SAFETY: pool/sql 有效
        unsafe { sz_orm_execute(pool, insert.as_ptr()) };

        let q = CString::new("SELECT name FROM qo_t ORDER BY id").unwrap();
        // SAFETY: pool/sql 有效
        let result = unsafe { sz_orm_query_one(pool, q.as_ptr()) };
        assert!(!result.is_null());
        // SAFETY: result 非空
        let result = unsafe { Box::from_raw(result) };
        assert_eq!(result.success, 1);
        assert!(!result.json.is_null());
        // SAFETY: json 有效
        let json = unsafe { CStr::from_ptr(result.json) }
            .to_string_lossy()
            .into_owned();
        assert!(json.contains("Alice"), "first row should be Alice: {json}");
        // SAFETY: result 配对释放
        unsafe { sz_orm_query_result_free(Box::into_raw(result)) };
        // SAFETY: pool 有效
        unsafe { sz_orm_pool_free(pool) };
    }

    #[test]
    fn test_query_one_empty_result() {
        let pool = test_pool();
        assert!(!pool.is_null());
        let create = CString::new("CREATE TABLE empty_t (id INTEGER)").unwrap();
        // SAFETY: pool/sql 有效
        unsafe { sz_orm_execute(pool, create.as_ptr()) };

        let q = CString::new("SELECT * FROM empty_t").unwrap();
        // SAFETY: pool/sql 有效
        let result = unsafe { sz_orm_query_one(pool, q.as_ptr()) };
        assert!(!result.is_null());
        // SAFETY: result 非空
        let result = unsafe { Box::from_raw(result) };
        assert_eq!(result.success, 1);
        assert!(result.json.is_null(), "empty result should have null json");
        // SAFETY: result 配对释放
        unsafe { sz_orm_query_result_free(Box::into_raw(result)) };
        // SAFETY: pool 有效
        unsafe { sz_orm_pool_free(pool) };
    }

    #[test]
    fn test_query_one_null_returns_null() {
        // SAFETY: null 指针合法调用
        let result = unsafe { sz_orm_query_one(std::ptr::null_mut(), std::ptr::null()) };
        assert!(result.is_null());
    }

    // ===== 新增测试：sz_orm_table_exists =====

    #[test]
    fn test_table_exists_true() {
        let pool = test_pool();
        assert!(!pool.is_null());
        let create = CString::new("CREATE TABLE exists_t (id INTEGER)").unwrap();
        // SAFETY: pool/sql 有效
        unsafe { sz_orm_execute(pool, create.as_ptr()) };

        let table = CString::new("exists_t").unwrap();
        // SAFETY: pool/table 有效
        let exists = unsafe { sz_orm_table_exists(pool, table.as_ptr()) };
        assert_eq!(exists, 1, "table should exist");
        // SAFETY: pool 有效
        unsafe { sz_orm_pool_free(pool) };
    }

    #[test]
    fn test_table_exists_false() {
        let pool = test_pool();
        assert!(!pool.is_null());
        let table = CString::new("nonexistent_table").unwrap();
        // SAFETY: pool/table 有效
        let exists = unsafe { sz_orm_table_exists(pool, table.as_ptr()) };
        assert_eq!(exists, 0, "table should not exist");
        // SAFETY: pool 有效
        unsafe { sz_orm_pool_free(pool) };
    }

    #[test]
    fn test_table_exists_null_returns_error() {
        // SAFETY: null 指针合法调用
        let exists = unsafe { sz_orm_table_exists(std::ptr::null_mut(), std::ptr::null()) };
        assert_eq!(exists, -1);
    }

    // ===== 新增测试：sz_orm_count =====

    #[test]
    fn test_count_returns_row_count() {
        let pool = test_pool();
        assert!(!pool.is_null());
        let create =
            CString::new("CREATE TABLE count_t (id INTEGER PRIMARY KEY, val TEXT)").unwrap();
        // SAFETY: pool/sql 有效
        unsafe { sz_orm_execute(pool, create.as_ptr()) };
        let insert1 = CString::new("INSERT INTO count_t (val) VALUES ('x')").unwrap();
        // SAFETY: pool/sql 有效
        let r1 = unsafe { sz_orm_execute(pool, insert1.as_ptr()) };
        assert_eq!(r1.rows_affected, 1, "first insert should affect 1 row");
        let insert2 = CString::new("INSERT INTO count_t (val) VALUES ('y')").unwrap();
        // SAFETY: pool/sql 有效
        let r2 = unsafe { sz_orm_execute(pool, insert2.as_ptr()) };
        assert_eq!(r2.rows_affected, 1, "second insert should affect 1 row");
        let insert3 = CString::new("INSERT INTO count_t (val) VALUES ('z')").unwrap();
        // SAFETY: pool/sql 有效
        let r3 = unsafe { sz_orm_execute(pool, insert3.as_ptr()) };
        assert_eq!(r3.rows_affected, 1, "third insert should affect 1 row");

        // 用 query 验证实际行数
        let q = CString::new("SELECT * FROM count_t").unwrap();
        // SAFETY: pool/sql 有效
        let result = unsafe { sz_orm_query(pool, q.as_ptr()) };
        assert!(!result.is_null());
        // SAFETY: result 非空
        let result = unsafe { Box::from_raw(result) };
        assert_eq!(result.success, 1);
        let json = if result.json.is_null() {
            String::new()
        } else {
            // SAFETY: json 有效
            unsafe { CStr::from_ptr(result.json) }
                .to_string_lossy()
                .into_owned()
        };
        // SAFETY: result 配对释放
        unsafe { sz_orm_query_result_free(Box::into_raw(result)) };
        assert!(
            json.contains("x") && json.contains("y") && json.contains("z"),
            "all 3 rows should be present: {json}"
        );

        let table = CString::new("count_t").unwrap();
        // SAFETY: pool/table 有效
        let count = unsafe { sz_orm_count(pool, table.as_ptr()) };
        assert_eq!(count, 3, "should have 3 rows (json={json})");
        // SAFETY: pool 有效
        unsafe { sz_orm_pool_free(pool) };
    }

    #[test]
    fn test_count_empty_table() {
        let pool = test_pool();
        assert!(!pool.is_null());
        let create = CString::new("CREATE TABLE empty_count (id INTEGER)").unwrap();
        // SAFETY: pool/sql 有效
        unsafe { sz_orm_execute(pool, create.as_ptr()) };

        let table = CString::new("empty_count").unwrap();
        // SAFETY: pool/table 有效
        let count = unsafe { sz_orm_count(pool, table.as_ptr()) };
        assert_eq!(count, 0, "empty table should have 0 rows");
        // SAFETY: pool 有效
        unsafe { sz_orm_pool_free(pool) };
    }

    #[test]
    fn test_count_null_returns_error() {
        // SAFETY: null 指针合法调用
        let count = unsafe { sz_orm_count(std::ptr::null_mut(), std::ptr::null()) };
        assert_eq!(count, -1);
    }

    // ===== 新增测试：事务 API =====

    #[test]
    fn test_transaction_begin_returns_handle() {
        let pool = test_pool();
        assert!(!pool.is_null());
        // SAFETY: pool 有效
        let tx = unsafe { sz_orm_transaction_begin(pool) };
        assert!(!tx.is_null(), "transaction begin should return handle");
        // SAFETY: tx 有效
        unsafe { sz_orm_transaction_free(tx) };
        // SAFETY: pool 有效
        unsafe { sz_orm_pool_free(pool) };
    }

    #[test]
    fn test_transaction_begin_null_returns_null() {
        // SAFETY: null 指针合法调用
        let tx = unsafe { sz_orm_transaction_begin(std::ptr::null_mut()) };
        assert!(tx.is_null());
    }

    #[test]
    fn test_transaction_commit() {
        let pool = test_pool();
        assert!(!pool.is_null());
        let create =
            CString::new("CREATE TABLE tx_commit (id INTEGER PRIMARY KEY, v TEXT)").unwrap();
        // SAFETY: pool/sql 有效
        unsafe { sz_orm_execute(pool, create.as_ptr()) };

        // SAFETY: pool 有效
        let tx = unsafe { sz_orm_transaction_begin(pool) };
        assert!(!tx.is_null());
        let insert = CString::new("INSERT INTO tx_commit (v) VALUES ('a')").unwrap();
        // SAFETY: tx/sql 有效
        let r = unsafe { sz_orm_transaction_execute(tx, insert.as_ptr()) };
        assert_eq!(r.success, 1, "insert in tx should succeed");
        // SAFETY: tx 有效
        let committed = unsafe { sz_orm_transaction_commit(tx) };
        assert_eq!(committed, 1, "commit should succeed");
        // SAFETY: tx 有效
        unsafe { sz_orm_transaction_free(tx) };

        // 验证数据已提交
        let table = CString::new("tx_commit").unwrap();
        // SAFETY: pool/table 有效
        let count = unsafe { sz_orm_count(pool, table.as_ptr()) };
        assert_eq!(count, 1, "committed data should be visible");
        // SAFETY: pool 有效
        unsafe { sz_orm_pool_free(pool) };
    }

    #[test]
    fn test_transaction_rollback() {
        let pool = test_pool();
        assert!(!pool.is_null());
        let create = CString::new("CREATE TABLE tx_rb (id INTEGER PRIMARY KEY, v TEXT)").unwrap();
        // SAFETY: pool/sql 有效
        unsafe { sz_orm_execute(pool, create.as_ptr()) };

        // SAFETY: pool 有效
        let tx = unsafe { sz_orm_transaction_begin(pool) };
        assert!(!tx.is_null());
        let insert = CString::new("INSERT INTO tx_rb (v) VALUES ('x')").unwrap();
        // SAFETY: tx/sql 有效
        let r = unsafe { sz_orm_transaction_execute(tx, insert.as_ptr()) };
        assert_eq!(r.success, 1);
        // SAFETY: tx 有效
        let rolled = unsafe { sz_orm_transaction_rollback(tx) };
        assert_eq!(rolled, 1, "rollback should succeed");
        // SAFETY: tx 有效
        unsafe { sz_orm_transaction_free(tx) };

        // 验证数据已回滚
        let table = CString::new("tx_rb").unwrap();
        // SAFETY: pool/table 有效
        let count = unsafe { sz_orm_count(pool, table.as_ptr()) };
        assert_eq!(count, 0, "rolled back data should not be visible");
        // SAFETY: pool 有效
        unsafe { sz_orm_pool_free(pool) };
    }

    #[test]
    fn test_transaction_free_auto_rollback() {
        let pool = test_pool();
        assert!(!pool.is_null());
        let create = CString::new("CREATE TABLE tx_auto (id INTEGER PRIMARY KEY, v TEXT)").unwrap();
        // SAFETY: pool/sql 有效
        unsafe { sz_orm_execute(pool, create.as_ptr()) };

        // SAFETY: pool 有效
        let tx = unsafe { sz_orm_transaction_begin(pool) };
        assert!(!tx.is_null());
        let insert = CString::new("INSERT INTO tx_auto (v) VALUES ('y')").unwrap();
        // SAFETY: tx/sql 有效
        let r = unsafe { sz_orm_transaction_execute(tx, insert.as_ptr()) };
        assert_eq!(r.success, 1);
        // 不调用 commit/rollback，直接 free → 自动回滚
        // SAFETY: tx 有效
        unsafe { sz_orm_transaction_free(tx) };

        let table = CString::new("tx_auto").unwrap();
        // SAFETY: pool/table 有效
        let count = unsafe { sz_orm_count(pool, table.as_ptr()) };
        assert_eq!(count, 0, "auto-rollback should discard data");
        // SAFETY: pool 有效
        unsafe { sz_orm_pool_free(pool) };
    }

    #[test]
    fn test_transaction_execute_null_returns_error() {
        // SAFETY: null 指针合法调用
        let r = unsafe { sz_orm_transaction_execute(std::ptr::null_mut(), std::ptr::null()) };
        assert_eq!(r.success, 0);
        assert_eq!(r.error_code, SzOrmErrorCode::InvalidArgument.as_i32());
    }

    #[test]
    fn test_transaction_commit_null_returns_zero() {
        // SAFETY: null 指针合法调用
        let r = unsafe { sz_orm_transaction_commit(std::ptr::null_mut()) };
        assert_eq!(r, 0);
    }

    #[test]
    fn test_transaction_rollback_null_returns_zero() {
        // SAFETY: null 指针合法调用
        let r = unsafe { sz_orm_transaction_rollback(std::ptr::null_mut()) };
        assert_eq!(r, 0);
    }

    #[test]
    fn test_transaction_free_null_is_noop() {
        // SAFETY: null 指针合法调用
        unsafe { sz_orm_transaction_free(std::ptr::null_mut()) };
    }

    #[test]
    fn test_transaction_execute_after_commit_returns_error() {
        let pool = test_pool();
        assert!(!pool.is_null());
        // SAFETY: pool 有效
        let tx = unsafe { sz_orm_transaction_begin(pool) };
        assert!(!tx.is_null());
        // SAFETY: tx 有效
        let committed = unsafe { sz_orm_transaction_commit(tx) };
        assert_eq!(committed, 1);
        // 提交后再 execute 应返回 TransactionAborted
        let sql = CString::new("SELECT 1").unwrap();
        // SAFETY: tx/sql 有效
        let r = unsafe { sz_orm_transaction_execute(tx, sql.as_ptr()) };
        assert_eq!(r.success, 0);
        assert_eq!(r.error_code, SzOrmErrorCode::TransactionAborted.as_i32());
        // SAFETY: tx 有效
        unsafe { sz_orm_transaction_free(tx) };
        // SAFETY: pool 有效
        unsafe { sz_orm_pool_free(pool) };
    }

    // ===== 新增测试：sz_orm_error_description / sz_orm_string_free =====

    #[test]
    fn test_error_description_ok() {
        // SAFETY: 返回的字符串需释放
        let s = unsafe { sz_orm_error_description(SzOrmErrorCode::Ok.as_i32()) };
        assert!(!s.is_null());
        // SAFETY: s 有效
        let desc = unsafe { CStr::from_ptr(s) }.to_string_lossy().into_owned();
        assert_eq!(desc, "success");
        // SAFETY: s 配对释放
        unsafe { sz_orm_string_free(s) };
    }

    #[test]
    fn test_error_description_all_codes() {
        for code in 0..=9 {
            // SAFETY: 返回的字符串需释放
            let s = unsafe { sz_orm_error_description(code) };
            assert!(!s.is_null(), "code {} should return non-null", code);
            // SAFETY: s 有效
            let desc = unsafe { CStr::from_ptr(s) }.to_string_lossy().into_owned();
            assert!(!desc.is_empty(), "code {} should have non-empty desc", code);
            // SAFETY: s 配对释放
            unsafe { sz_orm_string_free(s) };
        }
    }

    #[test]
    fn test_string_free_null_is_noop() {
        // SAFETY: null 指针合法调用
        unsafe { sz_orm_string_free(std::ptr::null_mut()) };
    }

    // ===== 新增测试：模型级 API（REQ-BND-007 ~ REQ-BND-014）=====

    #[test]
    fn test_validate_identifier_legal() {
        assert_eq!(validate_identifier("users"), Ok(()));
        assert_eq!(validate_identifier("_users"), Ok(()));
        assert_eq!(validate_identifier("user_1"), Ok(()));
        assert_eq!(validate_identifier("UserTable"), Ok(()));
        assert_eq!(validate_identifier("a1b2c3"), Ok(()));
    }

    #[test]
    fn test_validate_identifier_illegal() {
        assert_eq!(
            validate_identifier("users; DROP--"),
            Err(SzOrmErrorCode::InvalidArgument)
        );
        assert_eq!(
            validate_identifier("ta'ble"),
            Err(SzOrmErrorCode::InvalidArgument)
        );
        assert_eq!(
            validate_identifier("a b"),
            Err(SzOrmErrorCode::InvalidArgument)
        );
        assert_eq!(
            validate_identifier(""),
            Err(SzOrmErrorCode::InvalidArgument)
        );
        assert_eq!(
            validate_identifier("1table"),
            Err(SzOrmErrorCode::InvalidArgument)
        );
        assert_eq!(
            validate_identifier("table;drop"),
            Err(SzOrmErrorCode::InvalidArgument)
        );
    }

    /// Model test helper: create a table with name/age columns
    fn model_test_pool() -> SzOrmPoolHandle {
        let pool = test_pool();
        assert!(!pool.is_null());
        let create =
            CString::new("CREATE TABLE m_t (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)")
                .unwrap();
        // SAFETY: pool/sql 有效
        let r = unsafe { sz_orm_execute(pool, create.as_ptr()) };
        assert_eq!(r.success, 1, "CREATE should succeed");
        pool
    }

    /// Free the string pointer returned by model_find
    unsafe fn free_find_str(ptr: *mut c_char) -> String {
        if ptr.is_null() {
            return String::new();
        }
        // SAFETY: ptr 有效
        let s = unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned();
        // SAFETY: ptr 配对释放
        unsafe { sz_orm_string_free(ptr) };
        s
    }

    #[test]
    fn test_model_insert_then_find_roundtrip() {
        let pool = model_test_pool();
        let table = CString::new("m_t").unwrap();
        let fields = CString::new(r#"["name","age"]"#).unwrap();
        let values = CString::new(r#"["Alice",30]"#).unwrap();
        // SAFETY: pool/table/fields/values 有效
        let r =
            unsafe { sz_orm_model_insert(pool, table.as_ptr(), fields.as_ptr(), values.as_ptr()) };
        assert_eq!(r.success, 1, "insert should succeed, code={}", r.error_code);
        assert_eq!(r.rows_affected, 1);

        let where_clause = CString::new("name = ?").unwrap();
        let where_params = CString::new(r#"["Alice"]"#).unwrap();
        // SAFETY: pool/table/where 有效
        let ptr = unsafe {
            sz_orm_model_find(
                pool,
                table.as_ptr(),
                where_clause.as_ptr(),
                where_params.as_ptr(),
            )
        };
        assert!(!ptr.is_null(), "find should return non-null");
        // SAFETY: ptr 有效
        let json = unsafe { free_find_str(ptr) };
        assert!(json.contains("Alice"), "find should contain Alice: {json}");
        assert!(json.contains("30"), "find should contain age 30: {json}");
        // SAFETY: pool 有效
        unsafe { sz_orm_pool_free(pool) };
    }

    #[test]
    fn test_model_update_then_find() {
        let pool = model_test_pool();
        let table = CString::new("m_t").unwrap();
        let fields = CString::new(r#"["name","age"]"#).unwrap();
        let values = CString::new(r#"["Bob",25]"#).unwrap();
        // SAFETY: pool/table/fields/values 有效
        let r =
            unsafe { sz_orm_model_insert(pool, table.as_ptr(), fields.as_ptr(), values.as_ptr()) };
        assert_eq!(r.success, 1);

        let set_json = CString::new(r#"{"age":26}"#).unwrap();
        let where_clause = CString::new("name = ?").unwrap();
        let where_params = CString::new(r#"["Bob"]"#).unwrap();
        // SAFETY: pool/table/set/where 有效
        let r = unsafe {
            sz_orm_model_update(
                pool,
                table.as_ptr(),
                set_json.as_ptr(),
                where_clause.as_ptr(),
                where_params.as_ptr(),
            )
        };
        assert_eq!(r.success, 1, "update should succeed, code={}", r.error_code);
        assert_eq!(r.rows_affected, 1);

        // SAFETY: pool/table/where 有效
        let ptr = unsafe {
            sz_orm_model_find(
                pool,
                table.as_ptr(),
                where_clause.as_ptr(),
                where_params.as_ptr(),
            )
        };
        assert!(!ptr.is_null());
        // SAFETY: ptr 有效
        let json = unsafe { free_find_str(ptr) };
        assert!(
            json.contains("26"),
            "find after update should contain age 26: {json}"
        );
        assert!(
            !json.contains("25"),
            "find should not contain old age 25: {json}"
        );
        // SAFETY: pool 有效
        unsafe { sz_orm_pool_free(pool) };
    }

    #[test]
    fn test_model_delete_then_find_empty() {
        let pool = model_test_pool();
        let table = CString::new("m_t").unwrap();
        let fields = CString::new(r#"["name","age"]"#).unwrap();
        let values = CString::new(r#"["Carol",40]"#).unwrap();
        // SAFETY: pool/table/fields/values 有效
        let r =
            unsafe { sz_orm_model_insert(pool, table.as_ptr(), fields.as_ptr(), values.as_ptr()) };
        assert_eq!(r.success, 1);

        let where_clause = CString::new("name = ?").unwrap();
        let where_params = CString::new(r#"["Carol"]"#).unwrap();
        // SAFETY: pool/table/where 有效
        let r = unsafe {
            sz_orm_model_delete(
                pool,
                table.as_ptr(),
                where_clause.as_ptr(),
                where_params.as_ptr(),
            )
        };
        assert_eq!(r.success, 1, "delete should succeed, code={}", r.error_code);
        assert_eq!(r.rows_affected, 1);

        // SAFETY: pool/table/where 有效
        let ptr = unsafe {
            sz_orm_model_find(
                pool,
                table.as_ptr(),
                where_clause.as_ptr(),
                where_params.as_ptr(),
            )
        };
        assert!(
            !ptr.is_null(),
            "find after delete should return non-null (empty array)"
        );
        // SAFETY: ptr 有效
        let json = unsafe { free_find_str(ptr) };
        assert!(
            json == "[]" || json == "null" || json.is_empty(),
            "find after delete should return empty array, got: {json}"
        );
        // SAFETY: pool 有效
        unsafe { sz_orm_pool_free(pool) };
    }

    #[test]
    fn test_model_insert_illegal_table_returns_error() {
        let pool = model_test_pool();
        let table = CString::new("m_t; DROP--").unwrap();
        let fields = CString::new(r#"["name"]"#).unwrap();
        let values = CString::new(r#"["X"]"#).unwrap();
        // SAFETY: pool/table/fields/values 有效（table 含注入向量）
        let r =
            unsafe { sz_orm_model_insert(pool, table.as_ptr(), fields.as_ptr(), values.as_ptr()) };
        assert_eq!(r.success, 0, "illegal table should fail");
        assert_eq!(r.error_code, SzOrmErrorCode::InvalidArgument.as_i32());
        // SAFETY: pool 有效
        unsafe { sz_orm_pool_free(pool) };
    }

    #[test]
    fn test_model_insert_illegal_field_returns_error() {
        let pool = model_test_pool();
        let table = CString::new("m_t").unwrap();
        let fields = CString::new(r#"["name;drop"]"#).unwrap();
        let values = CString::new(r#"["X"]"#).unwrap();
        // SAFETY: pool/table/fields/values 有效（fields 含注入向量）
        let r =
            unsafe { sz_orm_model_insert(pool, table.as_ptr(), fields.as_ptr(), values.as_ptr()) };
        assert_eq!(r.success, 0, "illegal field should fail");
        assert_eq!(r.error_code, SzOrmErrorCode::InvalidArgument.as_i32());
        // SAFETY: pool 有效
        unsafe { sz_orm_pool_free(pool) };
    }

    #[test]
    fn test_model_insert_null_handle_returns_error() {
        let table = CString::new("m_t").unwrap();
        let fields = CString::new(r#"["name"]"#).unwrap();
        let values = CString::new(r#"["X"]"#).unwrap();
        // SAFETY: null 指针合法调用
        let r = unsafe {
            sz_orm_model_insert(
                std::ptr::null_mut(),
                table.as_ptr(),
                fields.as_ptr(),
                values.as_ptr(),
            )
        };
        assert_eq!(r.success, 0);
        assert_eq!(r.error_code, SzOrmErrorCode::InvalidArgument.as_i32());
    }

    #[test]
    fn test_model_insert_fields_values_mismatch_returns_error() {
        let pool = model_test_pool();
        let table = CString::new("m_t").unwrap();
        let fields = CString::new(r#"["name","age"]"#).unwrap();
        let values = CString::new(r#"["X"]"#).unwrap();
        // SAFETY: pool/table/fields/values 有效（长度不匹配）
        let r =
            unsafe { sz_orm_model_insert(pool, table.as_ptr(), fields.as_ptr(), values.as_ptr()) };
        assert_eq!(r.success, 0, "mismatched fields/values should fail");
        assert_eq!(r.error_code, SzOrmErrorCode::InvalidArgument.as_i32());
        // SAFETY: pool 有效
        unsafe { sz_orm_pool_free(pool) };
    }

    #[test]
    fn test_model_find_no_match_returns_empty_array() {
        let pool = model_test_pool();
        let table = CString::new("m_t").unwrap();
        let where_clause = CString::new("name = ?").unwrap();
        let where_params = CString::new(r#"["NonExistent"]"#).unwrap();
        // SAFETY: pool/table/where 有效
        let ptr = unsafe {
            sz_orm_model_find(
                pool,
                table.as_ptr(),
                where_clause.as_ptr(),
                where_params.as_ptr(),
            )
        };
        assert!(
            !ptr.is_null(),
            "find no match should return non-null (empty array)"
        );
        // SAFETY: ptr 有效
        let json = unsafe { free_find_str(ptr) };
        assert!(
            json == "[]" || json == "null" || json.is_empty(),
            "find no match should return empty array, got: {json}"
        );
        // SAFETY: pool 有效
        unsafe { sz_orm_pool_free(pool) };
    }

    #[test]
    fn test_model_find_all_no_where_clause() {
        let pool = model_test_pool();
        let table = CString::new("m_t").unwrap();
        let fields = CString::new(r#"["name","age"]"#).unwrap();
        let v1 = CString::new(r#"["A",1]"#).unwrap();
        let v2 = CString::new(r#"["B",2]"#).unwrap();
        // SAFETY: pool/table/fields/values 有效
        unsafe { sz_orm_model_insert(pool, table.as_ptr(), fields.as_ptr(), v1.as_ptr()) };
        unsafe { sz_orm_model_insert(pool, table.as_ptr(), fields.as_ptr(), v2.as_ptr()) };

        // 无 where 子句，查询全部
        // SAFETY: pool/table 有效，where_clause/where_params 为 null
        let ptr =
            unsafe { sz_orm_model_find(pool, table.as_ptr(), std::ptr::null(), std::ptr::null()) };
        assert!(!ptr.is_null());
        // SAFETY: ptr 有效
        let json = unsafe { free_find_str(ptr) };
        assert!(json.contains("A"), "find all should contain A: {json}");
        assert!(json.contains("B"), "find all should contain B: {json}");
        // SAFETY: pool 有效
        unsafe { sz_orm_pool_free(pool) };
    }

    // ===== 事务内模型操作测试（REQ-BND-014）=====

    #[test]
    fn test_model_insert_tx_rollback_then_find_empty() {
        let pool = model_test_pool();
        // SAFETY: pool 有效
        let tx = unsafe { sz_orm_transaction_begin(pool) };
        assert!(!tx.is_null());

        let table = CString::new("m_t").unwrap();
        let fields = CString::new(r#"["name","age"]"#).unwrap();
        let values = CString::new(r#"["TxUser",99]"#).unwrap();
        // SAFETY: tx/table/fields/values 有效
        let r =
            unsafe { sz_orm_model_insert_tx(tx, table.as_ptr(), fields.as_ptr(), values.as_ptr()) };
        assert_eq!(
            r.success, 1,
            "insert_tx should succeed, code={}",
            r.error_code
        );

        // SAFETY: tx 有效
        let rolled = unsafe { sz_orm_transaction_rollback(tx) };
        assert_eq!(rolled, 1, "rollback should succeed");
        // SAFETY: tx 有效
        unsafe { sz_orm_transaction_free(tx) };

        // 回滚后 find 应返回空
        let where_clause = CString::new("name = ?").unwrap();
        let where_params = CString::new(r#"["TxUser"]"#).unwrap();
        // SAFETY: pool/table/where 有效
        let ptr = unsafe {
            sz_orm_model_find(
                pool,
                table.as_ptr(),
                where_clause.as_ptr(),
                where_params.as_ptr(),
            )
        };
        assert!(!ptr.is_null());
        // SAFETY: ptr 有效
        let json = unsafe { free_find_str(ptr) };
        assert!(
            json == "[]" || json == "null" || json.is_empty(),
            "find after rollback should be empty, got: {json}"
        );
        // SAFETY: pool 有效
        unsafe { sz_orm_pool_free(pool) };
    }

    #[test]
    fn test_model_insert_tx_commit_then_find() {
        let pool = model_test_pool();
        // SAFETY: pool 有效
        let tx = unsafe { sz_orm_transaction_begin(pool) };
        assert!(!tx.is_null());

        let table = CString::new("m_t").unwrap();
        let fields = CString::new(r#"["name","age"]"#).unwrap();
        let values = CString::new(r#"["CommitUser",50]"#).unwrap();
        // SAFETY: tx/table/fields/values 有效
        let r =
            unsafe { sz_orm_model_insert_tx(tx, table.as_ptr(), fields.as_ptr(), values.as_ptr()) };
        assert_eq!(r.success, 1);

        // SAFETY: tx 有效
        let committed = unsafe { sz_orm_transaction_commit(tx) };
        assert_eq!(committed, 1, "commit should succeed");
        // SAFETY: tx 有效
        unsafe { sz_orm_transaction_free(tx) };

        // 提交后 find 应返回数据
        let where_clause = CString::new("name = ?").unwrap();
        let where_params = CString::new(r#"["CommitUser"]"#).unwrap();
        // SAFETY: pool/table/where 有效
        let ptr = unsafe {
            sz_orm_model_find(
                pool,
                table.as_ptr(),
                where_clause.as_ptr(),
                where_params.as_ptr(),
            )
        };
        assert!(!ptr.is_null());
        // SAFETY: ptr 有效
        let json = unsafe { free_find_str(ptr) };
        assert!(
            json.contains("CommitUser"),
            "find after commit should contain CommitUser: {json}"
        );
        // SAFETY: pool 有效
        unsafe { sz_orm_pool_free(pool) };
    }

    #[test]
    fn test_model_update_tx_and_delete_tx() {
        let pool = model_test_pool();
        let table = CString::new("m_t").unwrap();
        let fields = CString::new(r#"["name","age"]"#).unwrap();
        let values = CString::new(r#"["TxDel",10]"#).unwrap();
        // SAFETY: pool/table/fields/values 有效
        unsafe { sz_orm_model_insert(pool, table.as_ptr(), fields.as_ptr(), values.as_ptr()) };

        // 事务内 update
        // SAFETY: pool 有效
        let tx = unsafe { sz_orm_transaction_begin(pool) };
        assert!(!tx.is_null());
        let set_json = CString::new(r#"{"age":11}"#).unwrap();
        let where_clause = CString::new("name = ?").unwrap();
        let where_params = CString::new(r#"["TxDel"]"#).unwrap();
        // SAFETY: tx/table/set/where 有效
        let r = unsafe {
            sz_orm_model_update_tx(
                tx,
                table.as_ptr(),
                set_json.as_ptr(),
                where_clause.as_ptr(),
                where_params.as_ptr(),
            )
        };
        assert_eq!(
            r.success, 1,
            "update_tx should succeed, code={}",
            r.error_code
        );
        // SAFETY: tx 有效
        assert_eq!(unsafe { sz_orm_transaction_commit(tx) }, 1);
        // SAFETY: tx 有效
        unsafe { sz_orm_transaction_free(tx) };

        // 验证 update 生效
        // SAFETY: pool/table/where 有效
        let ptr = unsafe {
            sz_orm_model_find(
                pool,
                table.as_ptr(),
                where_clause.as_ptr(),
                where_params.as_ptr(),
            )
        };
        // SAFETY: ptr 有效
        let json = unsafe { free_find_str(ptr) };
        assert!(
            json.contains("11"),
            "find after update_tx should contain age 11: {json}"
        );

        // 事务内 delete
        // SAFETY: pool 有效
        let tx2 = unsafe { sz_orm_transaction_begin(pool) };
        assert!(!tx2.is_null());
        // SAFETY: tx2/table/where 有效
        let r = unsafe {
            sz_orm_model_delete_tx(
                tx2,
                table.as_ptr(),
                where_clause.as_ptr(),
                where_params.as_ptr(),
            )
        };
        assert_eq!(
            r.success, 1,
            "delete_tx should succeed, code={}",
            r.error_code
        );
        // SAFETY: tx2 有效
        assert_eq!(unsafe { sz_orm_transaction_commit(tx2) }, 1);
        // SAFETY: tx2 有效
        unsafe { sz_orm_transaction_free(tx2) };

        // 验证 delete 生效
        // SAFETY: pool/table/where 有效
        let ptr = unsafe {
            sz_orm_model_find(
                pool,
                table.as_ptr(),
                where_clause.as_ptr(),
                where_params.as_ptr(),
            )
        };
        // SAFETY: ptr 有效
        let json = unsafe { free_find_str(ptr) };
        assert!(
            json == "[]" || json == "null" || json.is_empty(),
            "find after delete_tx should be empty, got: {json}"
        );
        // SAFETY: pool 有效
        unsafe { sz_orm_pool_free(pool) };
    }

    #[test]
    fn test_model_find_tx_in_transaction() {
        let pool = model_test_pool();
        // SAFETY: pool 有效
        let tx = unsafe { sz_orm_transaction_begin(pool) };
        assert!(!tx.is_null());

        let table = CString::new("m_t").unwrap();
        let fields = CString::new(r#"["name","age"]"#).unwrap();
        let values = CString::new(r#"["InTx",77]"#).unwrap();
        // SAFETY: tx/table/fields/values 有效
        unsafe { sz_orm_model_insert_tx(tx, table.as_ptr(), fields.as_ptr(), values.as_ptr()) };

        // 事务内 find 应能看到未提交数据
        let where_clause = CString::new("name = ?").unwrap();
        let where_params = CString::new(r#"["InTx"]"#).unwrap();
        // SAFETY: tx/table/where 有效
        let ptr = unsafe {
            sz_orm_model_find_tx(
                tx,
                table.as_ptr(),
                where_clause.as_ptr(),
                where_params.as_ptr(),
            )
        };
        assert!(!ptr.is_null(), "find_tx should return non-null");
        // SAFETY: ptr 有效
        let json = unsafe { free_find_str(ptr) };
        assert!(
            json.contains("InTx"),
            "find_tx in transaction should see uncommitted data: {json}"
        );

        // SAFETY: tx 有效
        unsafe { sz_orm_transaction_rollback(tx) };
        // SAFETY: tx 有效
        unsafe { sz_orm_transaction_free(tx) };
        // SAFETY: pool 有效
        unsafe { sz_orm_pool_free(pool) };
    }

    #[test]
    fn test_model_insert_tx_null_handle_returns_error() {
        let table = CString::new("m_t").unwrap();
        let fields = CString::new(r#"["name"]"#).unwrap();
        let values = CString::new(r#"["X"]"#).unwrap();
        // SAFETY: null 指针合法调用
        let r = unsafe {
            sz_orm_model_insert_tx(
                std::ptr::null_mut(),
                table.as_ptr(),
                fields.as_ptr(),
                values.as_ptr(),
            )
        };
        assert_eq!(r.success, 0);
        assert_eq!(r.error_code, SzOrmErrorCode::InvalidArgument.as_i32());
    }
}
