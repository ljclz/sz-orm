//! # SZ-ORM C ABI — 跨语言 FFI 导出层
//!
//! 为 Go/Java/C++/Python 提供统一的 C ABI 接口，暴露 sz-orm-core 的
//! Model/QueryBuilder/Pool/Transaction 核心 API。
//!
//! ## 安全保证
//!
//! - FFI 内存由 Rust 侧分配/释放，语言侧仅持有句柄
//! - panic 捕获转换为错误码，不跨语言边界 UB
//! - unsafe 块均有 `// SAFETY:` 注释
//!
//! ## 导出函数
//!
//! - [`sz_orm_pool_new`]：创建连接池（真实创建，基于 sz-orm-sqlx SQLite 后端）
//! - [`sz_orm_pool_free`]：释放连接池
//! - [`sz_orm_ping`]：连接池健康检查
//! - [`sz_orm_query`]：执行查询，返回行 JSON
//! - [`sz_orm_execute`]：执行写语句（INSERT/UPDATE/DELETE）
//! - [`sz_orm_version`]：版本号

pub mod ffi_memory;
pub mod panic_guard;

use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::Arc;
use std::sync::OnceLock;

use sz_orm_core::{Pool, PoolConfig, PoolConfigBuilder, PooledConnection};

/// 连接池句柄
pub type SzOrmPoolHandle = *mut c_void;

/// 查询构建器句柄
pub type SzOrmQueryBuilderHandle = *mut c_void;

/// 事务句柄
pub type SzOrmTransactionHandle = *mut c_void;

/// 模型句柄
pub type SzOrmModelHandle = *mut c_void;

/// 错误码
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

/// C ABI 兼容的连接池配置
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

/// C ABI 兼容的查询结果
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

/// 查询行 JSON 结果（Rust 侧分配，`sz_orm_query_free` 释放）
pub struct QueryJsonResult {
    pub success: i32,
    pub error_code: i32,
    /// 行 JSON 的 C 字符串（`[{"col":val},...]`），成功时非空
    pub json: *mut c_char,
}

/// 全局 tokio runtime（C ABI 同步接口需要 block_on 执行异步池操作）
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

/// 将 PoolConfigC 转换为 sz-orm-core 的 PoolConfig
fn to_core_config(c: &PoolConfigC) -> Result<PoolConfig, sz_orm_core::PoolError> {
    PoolConfigBuilder::new()
        .max_size(c.max_connections.max(1))
        .min_idle(c.min_connections.min(c.max_connections.max(1)))
        .acquire_timeout(c.connect_timeout_ms / 1000)
        .idle_timeout(c.idle_timeout_ms / 1000)
        .build()
}

/// 创建连接池（SQLite 后端，真实创建）
///
/// dsn 支持 SQLite URL：`sqlite::memory:`、`sqlite://path/to/db.sqlite`
///
/// # Safety
///
/// SAFETY: `dsn` 必须是有效的 NUL 结尾 C 字符串。
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

/// 释放连接池
///
/// # Safety
///
/// SAFETY: `handle` 必须是 `sz_orm_pool_new` 返回的有效句柄，且未被释放过。
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

/// 连接池健康检查（acquire + ping 真实探活）
///
/// 返回 1 = 健康，0 = 不健康
///
/// # Safety
///
/// SAFETY: `handle` 必须是 `sz_orm_pool_new` 返回的有效句柄。
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

/// 执行查询，返回行 JSON
///
/// 成功时返回非空 `QueryJsonResult`，调用方用 `sz_orm_query_result_free` 释放。
/// 返回 `nullptr` 表示参数无效或内部 panic。
///
/// # Safety
///
/// SAFETY: `handle` 必须是 `sz_orm_pool_new` 返回的有效句柄；
/// `sql` 必须是有效的 NUL 结尾 C 字符串。
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

/// 释放查询结果
///
/// # Safety
///
/// SAFETY: `result` 必须是 `sz_orm_query` 返回的指针（或 nullptr）。
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

/// 执行写语句（INSERT/UPDATE/DELETE），返回影响行数
///
/// # Safety
///
/// SAFETY: `handle` 必须是 `sz_orm_pool_new` 返回的有效句柄；
/// `sql` 必须是有效的 NUL 结尾 C 字符串。
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

/// 获取版本号
#[no_mangle]
pub extern "C" fn sz_orm_version() -> u32 {
    env!("CARGO_PKG_VERSION")
        .split('.')
        .next()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
}

/// C ABI 兼容的连接池状态快照
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct PoolStatsC {
    pub idle: u32,
    pub active: u32,
    pub max: u32,
    pub min: u32,
    pub waiters: u32,
}

/// C ABI 兼容的连接池累计指标
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct PoolMetricsC {
    pub acquire_count: u64,
    pub acquire_failed_count: u64,
    pub release_count: u64,
    pub connection_created_count: u64,
    pub connection_closed_count: u64,
}

/// C ABI 事务句柄内部存储
struct CabiTransaction {
    conn: Option<PooledConnection>,
    active: bool,
}

/// 获取连接池状态快照
///
/// # Safety
///
/// SAFETY: `handle` 必须是 `sz_orm_pool_new` 返回的有效句柄。
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

/// 获取连接池累计指标
///
/// # Safety
///
/// SAFETY: `handle` 必须是 `sz_orm_pool_new` 返回的有效句柄。
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

/// 批量执行多条 SQL（分号分隔），返回累计影响行数
///
/// 任一条 SQL 失败则立即返回错误，后续 SQL 不执行。
///
/// # Safety
///
/// SAFETY: `handle` 必须是 `sz_orm_pool_new` 返回的有效句柄；
/// `sql` 必须是有效的 NUL 结尾 C 字符串。
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

/// 查询单行，返回第一行 JSON 对象（非数组）
///
/// 成功时返回非空 `QueryJsonResult`，调用方用 `sz_orm_query_result_free` 释放。
/// 无结果行时 success=1 但 json 为 null。
///
/// # Safety
///
/// SAFETY: `handle` 必须是 `sz_orm_pool_new` 返回的有效句柄；
/// `sql` 必须是有效的 NUL 结尾 C 字符串。
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

/// 检查表是否存在（SQLite 后端）
///
/// 返回 1 = 存在，0 = 不存在，-1 = 错误
///
/// # Safety
///
/// SAFETY: `handle` 必须是 `sz_orm_pool_new` 返回的有效句柄；
/// `table` 必须是有效的 NUL 结尾 C 字符串。
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

/// 统计表行数
///
/// 返回行数，错误返回 -1
///
/// # Safety
///
/// SAFETY: `handle` 必须是 `sz_orm_pool_new` 返回的有效句柄；
/// `table` 必须是有效的 NUL 结尾 C 字符串。
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

/// 开始事务（独占一个连接直到 commit/rollback/free）
///
/// 返回非空句柄表示成功，nullptr 表示失败
///
/// # Safety
///
/// SAFETY: `handle` 必须是 `sz_orm_pool_new` 返回的有效句柄。
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

/// 在事务中执行 SQL
///
/// # Safety
///
/// SAFETY: `tx_handle` 必须是 `sz_orm_transaction_begin` 返回的有效句柄；
/// `sql` 必须是有效的 NUL 结尾 C 字符串。
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

/// 提交事务
///
/// 返回 1 = 成功，0 = 失败
///
/// # Safety
///
/// SAFETY: `tx_handle` 必须是 `sz_orm_transaction_begin` 返回的有效句柄。
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

/// 回滚事务
///
/// 返回 1 = 成功，0 = 失败
///
/// # Safety
///
/// SAFETY: `tx_handle` 必须是 `sz_orm_transaction_begin` 返回的有效句柄。
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

/// 释放事务句柄（若事务仍活跃则自动回滚）
///
/// # Safety
///
/// SAFETY: `tx_handle` 必须是 `sz_orm_transaction_begin` 返回的有效句柄，且未被释放过。
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

/// 获取错误码描述字符串（调用方需用 `sz_orm_string_free` 释放）
///
/// # Safety
///
/// SAFETY: 返回的指针需通过 `sz_orm_string_free` 释放。
#[no_mangle]
pub unsafe extern "C" fn sz_orm_error_description(code: i32) -> *mut c_char {
    let desc = SzOrmErrorCode::from_i32(code).description();
    match CString::new(desc) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// 释放由 `sz_orm_error_description` 返回的字符串
///
/// # Safety
///
/// SAFETY: `s` 必须是 `sz_orm_error_description` 返回的指针（或 nullptr）。
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
}
