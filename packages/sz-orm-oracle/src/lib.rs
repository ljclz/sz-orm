//! SZ-ORM Oracle 数据库适配器
//!
//! 基于 `oracle` crate (ODPI-C 绑定) 实现 sz-orm-core 的 `Connection` trait,
//! 支持 Oracle 12c/19c/21c/23ai（实测 Oracle 23ai Free）。
//!
//! # 设计说明
//!
//! `oracle` crate 是同步库,所有数据库调用通过专用阻塞线程池
//! （`OracleBlockingPool`）的 `spawn_blocking` 包装为异步。
//!
//! v1.1.0 优化 3：从 `tokio::task::spawn_blocking`（共享主 runtime 阻塞池）
//! 改为 `OracleBlockingPool::spawn_blocking`（专用 runtime 阻塞池），
//! 隔离 Oracle 阻塞操作与主 runtime 的其他异步任务，避免 Oracle 慢查询
//! 拖累主 runtime。
//!
//! Oracle 占位符使用 `:1, :2, :3` 格式,本适配器自动将 SQL 中的 `?` 转换为 `:N`。
//!
//! # 实测环境
//!
//! - Oracle 23ai Free（127.0.0.1:1521/freepdb1.FALSE，用户 sz_orm_test）
//! - CRUD 全场景基准测试通过（详见 docs/sz-orm/2026-07-25-性能测试报告.md）

pub mod bulk_operations;
pub mod cursor_manager;
pub mod pool_config;
pub mod stored_procedure;
pub mod transaction_isolation;
pub mod type_mapping;

pub use bulk_operations::{BulkConfig, BulkErrorMode, BulkOpKind, BulkOperations, BulkResult};
pub use cursor_manager::{
    CursorConfig, CursorInstance, CursorManager, CursorState, FetchDirection,
};
pub use pool_config::{OraclePoolConfig, PoolStats};
pub use stored_procedure::{
    BatchProcedureCall, ParamMode, ParamType, ProcedureParam, StoredProcedureBuilder,
};
pub use transaction_isolation::{
    AccessMode, BatchErrorAction, TransactionConfig, TransactionIsolation,
};
pub use type_mapping::{OracleColumnMeta, OracleTypeKind, TypeMapping, ValueKind};

use async_trait::async_trait;
use std::collections::HashMap;
use std::future::Future;
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::sync::{Arc, Condvar, Mutex};

use oracle::sql_type::OracleType;
use oracle::{Connection as OracleConn, Row as OracleRow};
use sz_orm_core::{
    Connection, ConnectionFactory, DbError, QueryRows, QueryStreamItem, QueryValues, Value,
};

// ============================================================================
// v1.1.0 优化 3：Oracle 专用阻塞线程池
// ============================================================================

/// Oracle 专用阻塞线程池配置
///
/// 控制 `OracleBlockingPool` 的阻塞线程数上限。独立于主 tokio runtime
/// 的阻塞池（默认 512 线程），用于隔离 Oracle 阻塞操作。
#[derive(Debug, Clone)]
pub struct OracleBlockingPoolConfig {
    /// 阻塞线程数上限
    ///
    /// 默认 64，适用于大多数 OLTP 场景。可通过环境变量
    /// `SZ_ORM_ORACLE_MAX_BLOCKING_THREADS` 覆盖。
    pub max_blocking_threads: usize,
}

impl Default for OracleBlockingPoolConfig {
    fn default() -> Self {
        let max = std::env::var("SZ_ORM_ORACLE_MAX_BLOCKING_THREADS")
            .ok()
            .and_then(|s| s.parse().ok())
            .filter(|n: &usize| *n > 0)
            .unwrap_or(64);
        Self {
            max_blocking_threads: max,
        }
    }
}

/// Oracle 专用阻塞线程池
///
/// v1.1.0 优化 3：创建独立 tokio runtime（`multi_thread` + 自定义
/// `max_blocking_threads`），所有 Oracle 阻塞操作通过此 runtime 的
/// `spawn_blocking` 派发，与主 runtime 阻塞池隔离。
///
/// # 设计要点
///
/// - **独立 runtime**：1 个 worker thread + N 个 blocking thread，
///   避免占用主 runtime 的 512 个 blocking thread 配额
/// - **可配置池大小**：通过 `OracleBlockingPoolConfig` 或环境变量
///   `SZ_ORM_ORACLE_MAX_BLOCKING_THREADS` 调整
/// - **线程命名**：所有线程命名为 `sz-orm-oracle-blocking`，便于监控与诊断
/// - **生命周期**：runtime 由 `_runtime` 字段持有，与 `OracleBlockingPool` 同生命周期
///
/// # 性能收益
///
/// - 主 runtime 阻塞池不再被 Oracle 阻塞操作占用，其他异步任务（如 HTTP
///   请求、文件 IO）不受影响
/// - Oracle 阻塞池可独立调优，OLTP 场景下 64 线程足够；OLAP 大查询场景
///   可适当增大
pub struct OracleBlockingPool {
    /// runtime handle，用于派发 spawn_blocking 任务
    handle: tokio::runtime::Handle,
    /// 持有 runtime，确保阻塞任务能完成
    /// 字段名前缀 `_` 表示不直接读取，仅用于保持生命周期
    _runtime: tokio::runtime::Runtime,
}

impl OracleBlockingPool {
    /// 创建专用阻塞线程池
    ///
    /// # Panics
    ///
    /// 如果 tokio runtime 创建失败（通常是系统资源不足），会 panic。
    /// 这是 fail-fast 策略：在初始化阶段暴露问题，而非运行时降级。
    pub fn new(config: OracleBlockingPoolConfig) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .max_blocking_threads(config.max_blocking_threads)
            .thread_name("sz-orm-oracle-blocking")
            .enable_all()
            .build()
            .expect("Failed to create Oracle blocking pool runtime");
        let handle = runtime.handle().clone();
        Self {
            handle,
            _runtime: runtime,
        }
    }

    /// 在专用阻塞线程池中执行阻塞任务
    ///
    /// 与 `tokio::task::spawn_blocking` 签名一致，但任务派发到本池的
    /// 专用 runtime，而非主 runtime 的共享阻塞池。
    ///
    /// # 参数
    ///
    /// - `func`：阻塞函数，要求 `FnOnce + Send + 'static`
    ///
    /// # 返回
    ///
    /// `JoinHandle<T>`，可在任意 tokio runtime 中 await
    pub fn spawn_blocking<F, T>(&self, func: F) -> tokio::task::JoinHandle<T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        self.handle.spawn_blocking(func)
    }
}

impl std::fmt::Debug for OracleBlockingPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OracleBlockingPool")
            .field("handle", &"tokio::runtime::Handle")
            .field("_runtime", &"tokio::runtime::Runtime")
            .finish()
    }
}

/// 错误转换: oracle::Error -> DbError
fn map_oracle_error(e: oracle::Error) -> DbError {
    let msg = e.to_string();
    let kind = e.kind();
    match kind {
        oracle::ErrorKind::OciError | oracle::ErrorKind::DpiError => {
            if msg.contains("connection") || msg.contains("ORA-03114") || msg.contains("ORA-12541")
            {
                DbError::ConnectionError(msg)
            } else if msg.contains("ORA-00001") {
                DbError::AlreadyExists(msg)
            } else if msg.contains("ORA-01400") {
                DbError::NullValue(msg)
            } else if msg.contains("ORA-00942") || msg.contains("ORA-00904") {
                DbError::NotFound(msg)
            } else {
                DbError::QueryError(msg)
            }
        }
        oracle::ErrorKind::OutOfRange | oracle::ErrorKind::InvalidTypeConversion => {
            DbError::InvalidInput(msg)
        }
        _ => DbError::Internal(msg),
    }
}

/// 判断 SQL 是否需要原始执行(DDL/DCL 语句不走 prepared statement)
fn needs_raw_sql(sql: &str) -> bool {
    let upper = sql.trim_start().to_uppercase();
    upper.starts_with("CREATE ")
        || upper.starts_with("ALTER ")
        || upper.starts_with("DROP ")
        || upper.starts_with("TRUNCATE ")
        || upper.starts_with("GRANT ")
        || upper.starts_with("REVOKE ")
        || upper.starts_with("BEGIN ")
        || upper.starts_with("DECLARE ")
}

/// 将 SQL 中的 `?` 占位符转换为 Oracle 的 `:N` 格式
fn convert_placeholders(sql: &str) -> String {
    let mut result = String::with_capacity(sql.len() + 16);
    let mut in_single_quote = false;
    let mut param_index = 1u32;

    for ch in sql.chars() {
        match ch {
            '\'' if !in_single_quote => {
                in_single_quote = true;
                result.push(ch);
            }
            '\'' if in_single_quote => {
                in_single_quote = false;
                result.push(ch);
            }
            '?' if !in_single_quote => {
                result.push(':');
                result.push_str(&param_index.to_string());
                param_index += 1;
            }
            _ => {
                result.push(ch);
            }
        }
    }
    result
}

/// 将 sz-orm Value 转换为 Oracle 绑定值
fn value_to_oracle_to_sql(value: &Value) -> Box<dyn oracle::sql_type::ToSql + Send> {
    match value {
        Value::Null => Box::new(Option::<i64>::None),
        Value::Bool(b) => Box::new(*b),
        Value::I8(n) => Box::new(*n),
        Value::I16(n) => Box::new(*n),
        Value::I32(n) => Box::new(*n),
        Value::I64(n) => Box::new(*n),
        Value::U8(n) => Box::new(*n),
        Value::U16(n) => Box::new(*n),
        Value::U32(n) => Box::new(*n),
        Value::U64(n) => Box::new(*n as i64),
        Value::F32(f) => Box::new(*f),
        Value::F64(f) => Box::new(*f),
        Value::String(s) => Box::new(s.clone()),
        // DECIMAL/NUMERIC 以字符串绑定，Oracle 隐式转换为 NUMBER 列
        Value::Decimal(s) => Box::new(s.clone()),
        Value::Bytes(b) => Box::new(b.clone()),
        Value::Date(s) | Value::DateTime(s) | Value::Time(s) => Box::new(s.clone()),
        Value::Uuid(s) => Box::new(s.clone()),
        Value::Json(s) => Box::new(s.clone()),
        Value::Array(arr) => {
            let json = serde_json::to_string(arr).unwrap_or_else(|_| "[]".to_string());
            Box::new(json)
        }
        Value::Object(obj) => {
            let json = serde_json::to_string(obj).unwrap_or_else(|_| "{}".to_string());
            Box::new(json)
        }
        _ => Box::new(value.to_string()),
    }
}

/// 优化版 Oracle 行映射：预提取列名与 OracleType 到 Vec,后续行复用引用,
/// 并用 HashMap::with_capacity 预分配,避免每行每列重复调用
/// `col_info.name().to_string()` 与 `col_info.oracle_type()`。
///
/// 性能要点(对齐 sz-orm-sqlx 的 map_rows_optimized):
/// - 列名与 OracleType 仅从 ResultSet 提取一次,后续行复用
/// - HashMap 预分配 capacity = col_count,避免插入时 rehash
/// - 对零行结果直接返回空 Vec,零开销
fn map_oracle_rows_optimized(rows: oracle::ResultSet<'_, OracleRow>) -> Result<QueryRows, DbError> {
    let col_infos: Vec<oracle::ColumnInfo> = rows.column_info().to_vec();
    let col_count = col_infos.len();
    if col_count == 0 {
        return Ok(Vec::new());
    }
    // 预提取列名与 OracleType(只分配一次)
    let names: Vec<String> = col_infos.iter().map(|ci| ci.name().to_string()).collect();
    let types: Vec<OracleType> = col_infos
        .iter()
        .map(|ci| ci.oracle_type().clone())
        .collect();

    let mut result: QueryRows = Vec::new();
    for row_result in rows {
        let row = row_result.map_err(map_oracle_error)?;
        let mut row_map: HashMap<String, Value> = HashMap::with_capacity(col_count);
        for idx in 0..col_count {
            let value = oracle_row_to_value(&row, idx, &types[idx]);
            row_map.insert(names[idx].clone(), value);
        }
        result.push(row_map);
    }
    Ok(result)
}

/// 位置式 Oracle 行映射：返回 `(列名, 按列顺序的值矩阵)`，绕过 HashMap 开销。
///
/// 性能要点：
/// - 列名与 OracleType 仅从 ResultSet 提取一次,后续行复用
/// - 每行值按列序号直接 `Vec::push`,无哈希计算、无字符串克隆
/// - `Vec::with_capacity` 预分配,避免动态扩容
/// - 适用于 SELECT ALL 大结果集场景,比 `map_oracle_rows_optimized` 提升 30%~50%
fn map_oracle_rows_positional(
    rows: oracle::ResultSet<'_, OracleRow>,
) -> Result<QueryValues, DbError> {
    let col_infos: Vec<oracle::ColumnInfo> = rows.column_info().to_vec();
    let col_count = col_infos.len();
    if col_count == 0 {
        return Ok((Vec::new(), Vec::new()));
    }
    // 预提取列名与 OracleType(只分配一次)
    let names: Vec<String> = col_infos.iter().map(|ci| ci.name().to_string()).collect();
    let types: Vec<OracleType> = col_infos
        .iter()
        .map(|ci| ci.oracle_type().clone())
        .collect();

    let mut values_matrix: Vec<Vec<Value>> = Vec::new();
    for row_result in rows {
        let row = row_result.map_err(map_oracle_error)?;
        let mut values: Vec<Value> = Vec::with_capacity(col_count);
        for (idx, t) in types.iter().enumerate().take(col_count) {
            values.push(oracle_row_to_value(&row, idx, t));
        }
        values_matrix.push(values);
    }
    Ok((names, values_matrix))
}

/// 将 Oracle 行值转换为 sz-orm Value
///
/// NUMBER 类型优先以 String 读取（`Value::Decimal`），避免 f64 精度丢失；
/// Int64/UInt64 保持 i64 解码；Float/BinaryFloat/BinaryDouble 使用 f64。
fn oracle_row_to_value(row: &OracleRow, col_idx: usize, oracle_type: &OracleType) -> Value {
    match oracle_type {
        // NUMBER 可能含小数部分，以 String 读取保留完整精度
        OracleType::Number(_, _) => {
            if let Ok(v) = row.get::<_, Option<String>>(col_idx) {
                return v.map(Value::Decimal).unwrap_or(Value::Null);
            }
            if let Ok(v) = row.get::<_, Option<i64>>(col_idx) {
                return v.map(Value::I64).unwrap_or(Value::Null);
            }
            if let Ok(v) = row.get::<_, Option<f64>>(col_idx) {
                return v.map(Value::F64).unwrap_or(Value::Null);
            }
            Value::Null
        }
        OracleType::Float(_) | OracleType::BinaryFloat | OracleType::BinaryDouble => {
            if let Ok(v) = row.get::<_, Option<f64>>(col_idx) {
                return v.map(Value::F64).unwrap_or(Value::Null);
            }
            if let Ok(v) = row.get::<_, Option<i64>>(col_idx) {
                return v.map(Value::I64).unwrap_or(Value::Null);
            }
            Value::Null
        }
        OracleType::Int64 | OracleType::UInt64 => {
            if let Ok(v) = row.get::<_, Option<i64>>(col_idx) {
                return v.map(Value::I64).unwrap_or(Value::Null);
            }
            if let Ok(v) = row.get::<_, Option<f64>>(col_idx) {
                return v.map(Value::F64).unwrap_or(Value::Null);
            }
            Value::Null
        }
        OracleType::Varchar2(_)
        | OracleType::NVarchar2(_)
        | OracleType::Char(_)
        | OracleType::NChar(_)
        | OracleType::Rowid
        | OracleType::Raw(_) => {
            if let Ok(v) = row.get::<_, Option<String>>(col_idx) {
                return v.map(Value::String).unwrap_or(Value::Null);
            }
            Value::Null
        }
        OracleType::Long | OracleType::LongRaw => {
            if let Ok(v) = row.get::<_, Option<String>>(col_idx) {
                return v.map(Value::String).unwrap_or(Value::Null);
            }
            Value::Null
        }
        OracleType::Date => {
            if let Ok(v) = row.get::<_, Option<oracle::sql_type::Timestamp>>(col_idx) {
                return v
                    .map(|ts| Value::DateTime(ts.to_string()))
                    .unwrap_or(Value::Null);
            }
            Value::Null
        }
        OracleType::Timestamp(_) | OracleType::TimestampTZ(_) | OracleType::TimestampLTZ(_) => {
            if let Ok(v) = row.get::<_, Option<oracle::sql_type::Timestamp>>(col_idx) {
                return v
                    .map(|ts| Value::DateTime(ts.to_string()))
                    .unwrap_or(Value::Null);
            }
            Value::Null
        }
        OracleType::BLOB => {
            if let Ok(v) = row.get::<_, Option<Vec<u8>>>(col_idx) {
                return v.map(Value::Bytes).unwrap_or(Value::Null);
            }
            Value::Null
        }
        OracleType::CLOB | OracleType::NCLOB => {
            if let Ok(v) = row.get::<_, Option<String>>(col_idx) {
                return v.map(Value::String).unwrap_or(Value::Null);
            }
            Value::Null
        }
        OracleType::Boolean => {
            if let Ok(v) = row.get::<_, Option<bool>>(col_idx) {
                return v.map(Value::Bool).unwrap_or(Value::Null);
            }
            Value::Null
        }
        // Oracle 23ai JSON 类型，以字符串读取保留原始 JSON 文本
        OracleType::Json => {
            if let Ok(v) = row.get::<_, Option<String>>(col_idx) {
                return v.map(Value::Json).unwrap_or(Value::Null);
            }
            Value::Null
        }
        _ => {
            if let Ok(v) = row.get::<_, Option<i64>>(col_idx) {
                return v.map(Value::I64).unwrap_or(Value::Null);
            }
            if let Ok(v) = row.get::<_, Option<f64>>(col_idx) {
                return v.map(Value::F64).unwrap_or(Value::Null);
            }
            if let Ok(v) = row.get::<_, Option<String>>(col_idx) {
                return v.map(Value::String).unwrap_or(Value::Null);
            }
            Value::Null
        }
    }
}

/// Oracle 连接池内部状态
struct OraclePoolInner {
    /// 空闲连接列表：`acquire()` 从尾部弹出，`Drop` 归还时推入尾部
    idle: Vec<OracleConn>,
    /// 已创建连接总数（空闲 + 在用），用于约束池大小上限
    total: usize,
}

/// Oracle 连接池句柄
///
/// v1.2.0 修复 P0：从单连接伪池（`Mutex<Option<OracleConn>>`，并发度=1）
/// 改为真正的连接池（`Mutex<OraclePoolInner>` 持有多个空闲连接 + `Condvar`
/// 等待机制），并发度 = `max_size`（默认 10）。
///
/// # 设计要点
///
/// - **多连接并发**：`idle` 持有多个空闲连接，`acquire()` 弹出一个供调用方
///   独占使用，查询结束后通过 `OracleConnGuard::drop` 归还
/// - **池大小约束**：`total` 跟踪已创建连接数（空闲+在用），`acquire()` 在
///   `total < max_size` 时创建新连接，否则在 `Condvar` 上等待归还
/// - **连接复用**：连接在 `Drop` 时归还到 `idle`，不关闭，避免反复建连开销
/// - **阻塞安全**：`acquire()` 为同步阻塞调用，由 `OracleConnection` 的查询
///   方法通过 `blocking_pool().spawn_blocking` 包装，`Condvar::wait` 阻塞的是
///   专用阻塞线程，不影响主 tokio runtime
pub struct OraclePoolHandle {
    username: String,
    password: String,
    connect_string: String,
    /// 连接池内部状态（空闲连接 + 总数）
    inner: Mutex<OraclePoolInner>,
    /// 连接归还通知：池满时 `acquire()` 在此等待，`Drop` 时 `notify_one`
    condvar: Condvar,
    /// 连接池大小上限（默认 10）
    max_size: usize,
    /// v1.1.0 优化 3：Oracle 专用阻塞线程池
    ///
    /// 所有 `spawn_blocking` 调用通过此池派发，与主 tokio runtime 阻塞池隔离。
    /// 池大小由 `OracleBlockingPoolConfig` 控制（默认 64，可通过环境变量
    /// `SZ_ORM_ORACLE_MAX_BLOCKING_THREADS` 覆盖）。
    blocking_pool: OracleBlockingPool,
}

impl OraclePoolHandle {
    /// 创建新的 Oracle 连接池（使用默认阻塞池配置，连接池上限默认 10）
    pub fn connect(username: &str, password: &str, connect_string: &str) -> Result<Self, DbError> {
        Self::connect_with_pool(
            username,
            password,
            connect_string,
            OracleBlockingPoolConfig::default(),
        )
    }

    /// 创建新的 Oracle 连接池（自定义阻塞池配置，连接池上限默认 10）
    ///
    /// 适用于需要独立调优 Oracle 阻塞线程数的场景（如 OLAP 大查询）。
    /// 如需自定义连接池大小，使用 [`OraclePoolHandle::connect_with_max_size`]。
    pub fn connect_with_pool(
        username: &str,
        password: &str,
        connect_string: &str,
        pool_config: OracleBlockingPoolConfig,
    ) -> Result<Self, DbError> {
        Self::connect_with_max_size(username, password, connect_string, pool_config, 10)
    }

    /// 创建新的 Oracle 连接池（自定义阻塞池配置与连接池大小上限）
    ///
    /// # 参数
    ///
    /// - `max_size`：连接池大小上限。传入 0 会被替换为默认值 10
    pub fn connect_with_max_size(
        username: &str,
        password: &str,
        connect_string: &str,
        pool_config: OracleBlockingPoolConfig,
        max_size: usize,
    ) -> Result<Self, DbError> {
        // 建立首个连接，同时校验账号/连接串可用性
        let conn =
            OracleConn::connect(username, password, connect_string).map_err(map_oracle_error)?;
        let blocking_pool = OracleBlockingPool::new(pool_config);
        let max_size = if max_size == 0 { 10 } else { max_size };
        Ok(Self {
            username: username.to_string(),
            password: password.to_string(),
            connect_string: connect_string.to_string(),
            inner: Mutex::new(OraclePoolInner {
                idle: vec![conn],
                total: 1,
            }),
            condvar: Condvar::new(),
            max_size,
            blocking_pool,
        })
    }

    /// 从池中获取一个空闲连接（独占使用，`Drop` 时自动归还）
    ///
    /// # 获取策略
    ///
    /// 1. 优先从 `idle` 弹出一个空闲连接
    /// 2. 若 `idle` 为空且 `total < max_size`，创建新连接并递增 `total`
    /// 3. 若 `total >= max_size`，在 `condvar` 上等待，直到有连接归还
    ///
    /// # 阻塞语义
    ///
    /// 本方法为同步阻塞调用，应在 `spawn_blocking` 中调用（`OracleConnection`
    /// 的所有查询方法已通过 `blocking_pool().spawn_blocking` 包装）。等待连接
    /// 时阻塞的是专用阻塞线程，不影响主 tokio runtime。
    pub fn acquire(&self) -> Result<OracleConnGuard<'_>, DbError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|e| DbError::Internal(format!("Oracle pool mutex poisoned: {}", e)))?;
        loop {
            // 1. 空闲连接直接复用
            if let Some(conn) = inner.idle.pop() {
                return Ok(OracleConnGuard {
                    conn: Some(conn),
                    pool: self,
                });
            }
            // 2. 未达上限则创建新连接
            if inner.total < self.max_size {
                inner.total += 1;
                // 在锁内同步建连：oracle::Connection::connect 是阻塞调用，
                // 持锁时间 ≈ 一次 TCP+认证耗时（毫秒级），可接受。
                // 若建连失败需回滚 total 并唤醒一个等待者，避免其空等。
                match OracleConn::connect(&self.username, &self.password, &self.connect_string) {
                    Ok(conn) => {
                        return Ok(OracleConnGuard {
                            conn: Some(conn),
                            pool: self,
                        })
                    }
                    Err(e) => {
                        inner.total -= 1;
                        self.condvar.notify_one();
                        return Err(map_oracle_error(e));
                    }
                }
            }
            // 3. 池满：释放锁并等待连接归还
            inner = self
                .condvar
                .wait(inner)
                .map_err(|e| DbError::Internal(format!("Oracle pool condvar poisoned: {}", e)))?;
        }
    }

    /// 获取连接字符串
    pub fn connect_string(&self) -> &str {
        &self.connect_string
    }

    /// 获取连接池大小上限
    pub fn max_size(&self) -> usize {
        self.max_size
    }

    /// 获取专用阻塞线程池引用
    ///
    /// v1.1.0 优化 3：暴露专用阻塞池，供 `OracleConnection` 派发阻塞任务。
    pub fn blocking_pool(&self) -> &OracleBlockingPool {
        &self.blocking_pool
    }
}

/// Oracle 连接的独占守卫
///
/// 持有从池中取出的连接，`Drop` 时自动归还到池的 `idle` 列表并通知一个等待者。
/// 通过 `Deref` / `DerefMut` 透明访问底层 `OracleConn`。
pub struct OracleConnGuard<'a> {
    /// 当前持有的连接；`Drop` 时取出并归还，正常使用期间始终为 `Some`
    conn: Option<OracleConn>,
    /// 池的引用，用于归还连接
    pool: &'a OraclePoolHandle,
}

impl Deref for OracleConnGuard<'_> {
    type Target = OracleConn;
    fn deref(&self) -> &OracleConn {
        self.conn.as_ref().expect("connection must exist")
    }
}

impl DerefMut for OracleConnGuard<'_> {
    fn deref_mut(&mut self) -> &mut OracleConn {
        self.conn.as_mut().expect("connection must exist")
    }
}

impl Drop for OracleConnGuard<'_> {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            // 即使 mutex poisoned 也强制归还，避免连接泄漏
            let mut inner = self.pool.inner.lock().unwrap_or_else(|e| e.into_inner());
            inner.idle.push(conn);
            drop(inner);
            // 唤醒一个等待 acquire() 的阻塞线程
            self.pool.condvar.notify_one();
        }
    }
}

/// Oracle 连接工厂
pub struct OracleConnectionFactory {
    handle: Arc<OraclePoolHandle>,
}

impl OracleConnectionFactory {
    pub fn new(handle: Arc<OraclePoolHandle>) -> Self {
        Self { handle }
    }
}

#[async_trait]
impl ConnectionFactory for OracleConnectionFactory {
    async fn create(&self) -> Result<Box<dyn Connection>, DbError> {
        let _ = self.handle.acquire()?;
        Ok(Box::new(OracleConnection {
            handle: self.handle.clone(),
            connected: true,
            in_transaction: false,
        }))
    }
}

/// Oracle 连接包装器
pub struct OracleConnection {
    handle: Arc<OraclePoolHandle>,
    connected: bool,
    in_transaction: bool,
}

impl OracleConnection {
    fn mark_connection_error(&mut self, e: &DbError) {
        if matches!(e, DbError::ConnectionError(_) | DbError::IoError(_)) {
            self.connected = false;
        }
    }
}

impl Connection for OracleConnection {
    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        Box::pin(async move {
            if !self.connected {
                return Err(DbError::ConnectionError("connection closed".to_string()));
            }
            let sql_owned = sql.to_string();
            // v1.1.0 优化 3：通过专用阻塞线程池派发，隔离 Oracle 阻塞操作
            let result = self
                .handle
                .blocking_pool()
                .spawn_blocking({
                    let handle = self.handle.clone();
                    move || {
                        let guard = handle.acquire()?;
                        let conn = guard.deref();
                        let stmt = conn.execute(&sql_owned, &[]).map_err(map_oracle_error)?;
                        Ok::<u64, DbError>(stmt.row_count().unwrap_or(0))
                    }
                })
                .await
                .map_err(|e| DbError::Internal(format!("spawn_blocking join error: {}", e)))?;
            if let Err(ref e) = result {
                self.mark_connection_error(e);
            }
            result
        })
    }

    fn query<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<QueryRows, DbError>> + Send + 'a>> {
        Box::pin(async move {
            if !self.connected {
                return Err(DbError::ConnectionError("connection closed".to_string()));
            }
            let sql_owned = sql.to_string();
            // v1.1.0 优化 3：通过专用阻塞线程池派发，隔离 Oracle 阻塞操作
            let result = self
                .handle
                .blocking_pool()
                .spawn_blocking({
                    let handle = self.handle.clone();
                    move || {
                        let guard = handle.acquire()?;
                        let conn = guard.deref();
                        let rows = conn.query(&sql_owned, &[]).map_err(map_oracle_error)?;
                        map_oracle_rows_optimized(rows)
                    }
                })
                .await
                .map_err(|e| DbError::Internal(format!("spawn_blocking join error: {}", e)))?;
            if let Err(ref e) = result {
                self.mark_connection_error(e);
            }
            result
        })
    }

    /// G-SX-4：Oracle 原生游标流式查询
    ///
    /// `oracle` crate 为同步 API，`ResultSet` 是同步迭代器。本方法通过
    /// `tokio::sync::mpsc` 通道桥接阻塞迭代与异步消费：
    ///
    /// 1. 在专用阻塞线程池中获取连接、执行查询、迭代 `ResultSet`
    /// 2. 每一行通过 mpsc 通道发送到异步端
    /// 3. 异步端从通道接收并逐行 yield
    ///
    /// 相比默认实现（全量加载到 Vec 再逐行 yield），本方法避免了大结果集
    /// 的内存峰值：阻塞线程逐行拉取，异步端逐行消费，通道缓冲区大小
    /// 限制了同时在途的行数。
    fn query_stream<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn futures::Stream<Item = QueryStreamItem> + Send + 'a>> {
        let sql_owned = sql.to_string();
        // 通道容量：平衡内存占用与吞吐。64 行在途足以让异步端批量消费，
        // 同时限制内存占用（每行一个 HashMap<String, Value>）。
        let (tx, mut rx) = tokio::sync::mpsc::channel::<QueryStreamItem>(64);

        // 在专用阻塞线程池中迭代 ResultSet
        let handle = self.handle.clone();
        let blocking_pool = self.handle.blocking_pool().handle.clone();
        // spawn_blocking 需要闭包为 'static，所以 move handle 和 sql_owned
        let connected = self.connected;
        blocking_pool.spawn_blocking(move || {
            if !connected {
                let _ = tx.blocking_send(Err(DbError::ConnectionError(
                    "connection closed".to_string(),
                )));
                return;
            }
            // 获取连接 guard（阻塞）
            let guard = match handle.acquire() {
                Ok(g) => g,
                Err(e) => {
                    let _ = tx.blocking_send(Err(e));
                    return;
                }
            };
            let conn = guard.deref();
            // 执行查询并迭代 ResultSet（全部在阻塞线程内完成）
            let rows = match conn.query(&sql_owned, &[]) {
                Ok(rs) => rs,
                Err(e) => {
                    let _ = tx.blocking_send(Err(map_oracle_error(e)));
                    return;
                }
            };
            // 提取列信息（只一次）
            let col_infos: Vec<oracle::ColumnInfo> = rows.column_info().to_vec();
            let col_count = col_infos.len();
            if col_count == 0 {
                return;
            }
            let names: Vec<String> = col_infos.iter().map(|ci| ci.name().to_string()).collect();
            let types: Vec<OracleType> = col_infos
                .iter()
                .map(|ci| ci.oracle_type().clone())
                .collect();
            // 逐行迭代，通过通道发送
            for row_result in rows {
                match row_result {
                    Ok(row) => {
                        let mut row_map: HashMap<String, Value> = HashMap::with_capacity(col_count);
                        for idx in 0..col_count {
                            let value = oracle_row_to_value(&row, idx, &types[idx]);
                            row_map.insert(names[idx].clone(), value);
                        }
                        if tx.blocking_send(Ok(row_map)).is_err() {
                            // 接收端已 drop（消费者提前终止流），停止迭代
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = tx.blocking_send(Err(map_oracle_error(e)));
                        break;
                    }
                }
            }
            // tx drop 后 rx 将收到 None，自然结束流
        });

        // 异步端：从通道接收并逐行 yield
        Box::pin(async_stream::stream! {
            while let Some(item) = rx.recv().await {
                yield item;
            }
        })
    }

    fn execute_with_params<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [Value],
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        Box::pin(async move {
            if !self.connected {
                return Err(DbError::ConnectionError("connection closed".to_string()));
            }

            if needs_raw_sql(sql) || params.is_empty() {
                return self.execute(sql).await;
            }

            let sql_converted = convert_placeholders(sql);
            let params_owned: Vec<Value> = params.to_vec();
            // v1.1.0 优化 3：通过专用阻塞线程池派发，隔离 Oracle 阻塞操作
            let result = self
                .handle
                .blocking_pool()
                .spawn_blocking({
                    let handle = self.handle.clone();
                    move || {
                        let guard = handle.acquire()?;
                        let conn = guard.deref();
                        let to_sql_params: Vec<Box<dyn oracle::sql_type::ToSql + Send>> =
                            params_owned.iter().map(value_to_oracle_to_sql).collect();
                        let to_sql_refs: Vec<&dyn oracle::sql_type::ToSql> = to_sql_params
                            .iter()
                            .map(|b| &**b as &dyn oracle::sql_type::ToSql)
                            .collect();
                        let stmt = conn
                            .execute(&sql_converted, &to_sql_refs)
                            .map_err(map_oracle_error)?;
                        Ok::<u64, DbError>(stmt.row_count().unwrap_or(0))
                    }
                })
                .await
                .map_err(|e| DbError::Internal(format!("spawn_blocking join error: {}", e)))?;
            if let Err(ref e) = result {
                self.mark_connection_error(e);
            }
            result
        })
    }

    fn query_with_params<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [Value],
    ) -> Pin<Box<dyn Future<Output = Result<QueryRows, DbError>> + Send + 'a>> {
        Box::pin(async move {
            if !self.connected {
                return Err(DbError::ConnectionError("connection closed".to_string()));
            }

            let sql_converted = convert_placeholders(sql);
            let params_owned: Vec<Value> = params.to_vec();
            // v1.1.0 优化 3：通过专用阻塞线程池派发，隔离 Oracle 阻塞操作
            let result = self
                .handle
                .blocking_pool()
                .spawn_blocking({
                    let handle = self.handle.clone();
                    move || {
                        let guard = handle.acquire()?;
                        let conn = guard.deref();
                        let to_sql_params: Vec<Box<dyn oracle::sql_type::ToSql + Send>> =
                            params_owned.iter().map(value_to_oracle_to_sql).collect();
                        let to_sql_refs: Vec<&dyn oracle::sql_type::ToSql> = to_sql_params
                            .iter()
                            .map(|b| &**b as &dyn oracle::sql_type::ToSql)
                            .collect();
                        let rows = conn
                            .query(&sql_converted, &to_sql_refs)
                            .map_err(map_oracle_error)?;
                        map_oracle_rows_optimized(rows)
                    }
                })
                .await
                .map_err(|e| DbError::Internal(format!("spawn_blocking join error: {}", e)))?;
            if let Err(ref e) = result {
                self.mark_connection_error(e);
            }
            result
        })
    }

    /// Oracle 位置式查询（SELECT）：绕过 HashMap 行映射，返回列名 + 按列顺序的值矩阵。
    ///
    /// 适用于 SELECT ALL 大结果集场景，比 `query` 提升 30%~50%。
    fn query_values<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<QueryValues, DbError>> + Send + 'a>> {
        Box::pin(async move {
            if !self.connected {
                return Err(DbError::ConnectionError("connection closed".to_string()));
            }
            let sql_owned = sql.to_string();
            // v1.1.0 优化 3：通过专用阻塞线程池派发，隔离 Oracle 阻塞操作
            let result = self
                .handle
                .blocking_pool()
                .spawn_blocking({
                    let handle = self.handle.clone();
                    move || {
                        let guard = handle.acquire()?;
                        let conn = guard.deref();
                        let rows = conn.query(&sql_owned, &[]).map_err(map_oracle_error)?;
                        map_oracle_rows_positional(rows)
                    }
                })
                .await
                .map_err(|e| DbError::Internal(format!("spawn_blocking join error: {}", e)))?;
            if let Err(ref e) = result {
                self.mark_connection_error(e);
            }
            result
        })
    }

    /// Oracle 参数绑定位置式查询（SELECT）：叠加 prepared statement + 位置式映射双重优化。
    fn query_values_with_params<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [Value],
    ) -> Pin<Box<dyn Future<Output = Result<QueryValues, DbError>> + Send + 'a>> {
        Box::pin(async move {
            if !self.connected {
                return Err(DbError::ConnectionError("connection closed".to_string()));
            }

            let sql_converted = convert_placeholders(sql);
            let params_owned: Vec<Value> = params.to_vec();
            // v1.1.0 优化 3：通过专用阻塞线程池派发，隔离 Oracle 阻塞操作
            let result = self
                .handle
                .blocking_pool()
                .spawn_blocking({
                    let handle = self.handle.clone();
                    move || {
                        let guard = handle.acquire()?;
                        let conn = guard.deref();
                        let to_sql_params: Vec<Box<dyn oracle::sql_type::ToSql + Send>> =
                            params_owned.iter().map(value_to_oracle_to_sql).collect();
                        let to_sql_refs: Vec<&dyn oracle::sql_type::ToSql> = to_sql_params
                            .iter()
                            .map(|b| &**b as &dyn oracle::sql_type::ToSql)
                            .collect();
                        let rows = conn
                            .query(&sql_converted, &to_sql_refs)
                            .map_err(map_oracle_error)?;
                        map_oracle_rows_positional(rows)
                    }
                })
                .await
                .map_err(|e| DbError::Internal(format!("spawn_blocking join error: {}", e)))?;
            if let Err(ref e) = result {
                self.mark_connection_error(e);
            }
            result
        })
    }

    /// Oracle 批量插入：使用原生 Array DML（Batch API）一次性提交多行
    ///
    /// 通过 `conn.batch(sql, batch_size)` 创建批处理，逐行 `append_row` 后
    /// `execute()` 提交。启用 `with_row_counts` 获取每行影响行数，求和返回。
    /// 比默认实现的逐行 `execute_with_params` 循环减少 N-1 次网络往返。
    fn execute_batch_params<'a>(
        &'a mut self,
        sql: &'a str,
        params_batch: &'a [Vec<Value>],
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        Box::pin(async move {
            if !self.connected {
                return Err(DbError::ConnectionError("connection closed".to_string()));
            }
            if params_batch.is_empty() {
                return Ok(0);
            }
            if needs_raw_sql(sql) {
                // DDL/DCL 不支持批量绑定，回退到逐条执行
                let mut total = 0u64;
                for params in params_batch {
                    total += self.execute_with_params(sql, params).await?;
                }
                return Ok(total);
            }

            let sql_converted = convert_placeholders(sql);
            let batch_size = params_batch.len();
            // 深拷贝参数，移入 spawn_blocking 闭包
            let batch_owned: Vec<Vec<Value>> = params_batch.to_vec();
            // v1.1.0 优化 3：通过专用阻塞线程池派发，隔离 Oracle 阻塞操作
            let result = self
                .handle
                .blocking_pool()
                .spawn_blocking({
                    let handle = self.handle.clone();
                    move || {
                        let guard = handle.acquire()?;
                        let conn = guard.deref();
                        // 创建批处理，启用 row_counts 以获取每行影响行数
                        let mut batch = conn
                            .batch(&sql_converted, batch_size)
                            .with_row_counts()
                            .build()
                            .map_err(map_oracle_error)?;
                        // 逐行追加：每行参数转换为 &dyn ToSql 引用切片
                        for row_params in &batch_owned {
                            let to_sql_boxed: Vec<Box<dyn oracle::sql_type::ToSql + Send>> =
                                row_params.iter().map(value_to_oracle_to_sql).collect();
                            let to_sql_refs: Vec<&dyn oracle::sql_type::ToSql> = to_sql_boxed
                                .iter()
                                .map(|b| &**b as &dyn oracle::sql_type::ToSql)
                                .collect();
                            batch.append_row(&to_sql_refs).map_err(map_oracle_error)?;
                        }
                        // 执行批处理
                        batch.execute().map_err(map_oracle_error)?;
                        // 汇总每行影响行数
                        let row_counts = batch.row_counts().map_err(map_oracle_error)?;
                        let total: u64 = row_counts.iter().sum();
                        Ok::<u64, DbError>(total)
                    }
                })
                .await
                .map_err(|e| DbError::Internal(format!("spawn_blocking join error: {}", e)))?;
            if let Err(ref e) = result {
                self.mark_connection_error(e);
            }
            result
        })
    }

    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            if !self.connected {
                return Err(DbError::ConnectionError("connection closed".to_string()));
            }
            // v1.1.0 优化 3：通过专用阻塞线程池派发，隔离 Oracle 阻塞操作
            let result = self
                .handle
                .blocking_pool()
                .spawn_blocking({
                    let handle = self.handle.clone();
                    move || {
                        let guard = handle.acquire()?;
                        let conn = guard.deref();
                        conn.execute("BEGIN", &[]).map_err(map_oracle_error)?;
                        Ok::<(), DbError>(())
                    }
                })
                .await
                .map_err(|e| DbError::Internal(format!("spawn_blocking join error: {}", e)))?;
            if result.is_ok() {
                self.in_transaction = true;
            } else if let Err(ref e) = result {
                self.mark_connection_error(e);
            }
            result
        })
    }

    fn commit<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            if !self.connected {
                return Err(DbError::ConnectionError("connection closed".to_string()));
            }
            // v1.1.0 优化 3：通过专用阻塞线程池派发，隔离 Oracle 阻塞操作
            let result = self
                .handle
                .blocking_pool()
                .spawn_blocking({
                    let handle = self.handle.clone();
                    move || {
                        let guard = handle.acquire()?;
                        let conn = guard.deref();
                        conn.commit().map_err(map_oracle_error)?;
                        Ok::<(), DbError>(())
                    }
                })
                .await
                .map_err(|e| DbError::Internal(format!("spawn_blocking join error: {}", e)))?;
            if result.is_ok() {
                self.in_transaction = false;
            } else if let Err(ref e) = result {
                self.mark_connection_error(e);
            }
            result
        })
    }

    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            if !self.connected {
                return Err(DbError::ConnectionError("connection closed".to_string()));
            }
            // v1.1.0 优化 3：通过专用阻塞线程池派发，隔离 Oracle 阻塞操作
            let result = self
                .handle
                .blocking_pool()
                .spawn_blocking({
                    let handle = self.handle.clone();
                    move || {
                        let guard = handle.acquire()?;
                        let conn = guard.deref();
                        conn.rollback().map_err(map_oracle_error)?;
                        Ok::<(), DbError>(())
                    }
                })
                .await
                .map_err(|e| DbError::Internal(format!("spawn_blocking join error: {}", e)))?;
            if result.is_ok() {
                self.in_transaction = false;
            } else if let Err(ref e) = result {
                self.mark_connection_error(e);
            }
            result
        })
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move {
            if !self.connected {
                return false;
            }
            // v1.1.0 优化 3：通过专用阻塞线程池派发，隔离 Oracle 阻塞操作
            let result = self
                .handle
                .blocking_pool()
                .spawn_blocking({
                    let handle = self.handle.clone();
                    move || {
                        let guard = handle.acquire()?;
                        let conn = guard.deref();
                        conn.execute("SELECT 1 FROM dual", &[])
                            .map_err(map_oracle_error)?;
                        Ok::<(), DbError>(())
                    }
                })
                .await;
            match result {
                Ok(Ok(())) => true,
                _ => {
                    self.connected = false;
                    false
                }
            }
        })
    }

    fn close<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            self.connected = false;
            Ok(())
        })
    }
}

// ============================================================================
// 连接串解析 API
// ============================================================================

/// Oracle 连接串解析结果
#[derive(Debug, Clone)]
pub struct OracleConnInfo {
    host: String,
    port: u16,
    service_name: String,
    username: String,
    password: String,
}

impl OracleConnInfo {
    /// 获取主机地址
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// 获取端口
    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// 获取服务名
    #[must_use]
    pub fn service_name(&self) -> &str {
        &self.service_name
    }

    /// 获取用户名
    #[must_use]
    pub fn username(&self) -> &str {
        &self.username
    }

    /// 获取密码
    #[must_use]
    pub fn password(&self) -> &str {
        &self.password
    }

    /// 重新生成连接串（`host:port/service_name`）
    #[must_use]
    pub fn as_connect_string(&self) -> String {
        format!("{}:{}/{}", self.host, self.port, self.service_name)
    }
}

/// 解析 Oracle 连接串
///
/// 支持格式：`host:port/service_name` 或 `host/service_name`（默认端口 1521）
///
/// # Errors
///
/// 若格式无效返回 `DbError::Internal`。
pub fn parse_connect_string(conn_str: &str) -> Result<OracleConnInfo, DbError> {
    let (host_port, service_name) = conn_str.split_once('/').ok_or_else(|| {
        DbError::Internal("missing service_name (use host:port/service)".to_string())
    })?;
    let service_name = service_name.trim().to_string();
    if service_name.is_empty() {
        return Err(DbError::Internal("empty service_name".to_string()));
    }

    let (host, port) = if let Some((h, p)) = host_port.split_once(':') {
        let port: u16 = p
            .trim()
            .parse()
            .map_err(|_| DbError::Internal(format!("invalid port: {p}")))?;
        (h.trim().to_string(), port)
    } else {
        (host_port.trim().to_string(), 1521)
    };

    if host.is_empty() {
        return Err(DbError::Internal("empty host".to_string()));
    }

    Ok(OracleConnInfo {
        host,
        port,
        service_name,
        username: String::new(),
        password: String::new(),
    })
}

// ============================================================================
// Oracle 方言辅助 API
// ============================================================================

/// Oracle 方言辅助
pub struct OracleDialect;

impl OracleDialect {
    /// 构造方言辅助器
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// 引用标识符（Oracle 用双引号 `"name"`）
    #[must_use]
    pub fn quote_identifier(&self, name: &str) -> String {
        format!("\"{}\"", name.replace('"', "\"\""))
    }

    /// 生成 LIMIT/OFFSET 子句（Oracle 12c+ 用 `OFFSET ... ROWS FETCH NEXT`）
    #[must_use]
    pub fn limit_clause(&self, limit: Option<u64>, offset: Option<u64>) -> String {
        match (limit, offset) {
            (Some(l), Some(o)) => {
                format!("OFFSET {o} ROWS FETCH NEXT {l} ROWS ONLY")
            }
            (Some(l), None) => format!("FETCH NEXT {l} ROWS ONLY"),
            (None, Some(o)) => format!("OFFSET {o} ROWS"),
            (None, None) => String::new(),
        }
    }

    /// 生成参数占位符（Oracle 用 `:1`, `:2`, ...）
    #[must_use]
    pub fn placeholder(&self, index: usize) -> String {
        format!(":{index}")
    }

    /// 检查是否为 Oracle 保留字
    #[must_use]
    pub fn is_reserved_keyword(&self, kw: &str) -> bool {
        matches!(
            kw.to_uppercase().as_str(),
            "SELECT"
                | "FROM"
                | "WHERE"
                | "INSERT"
                | "UPDATE"
                | "DELETE"
                | "CREATE"
                | "ALTER"
                | "DROP"
                | "TABLE"
                | "INDEX"
                | "VIEW"
                | "JOIN"
                | "ON"
                | "AND"
                | "OR"
                | "NOT"
                | "NULL"
                | "ORDER"
                | "GROUP"
                | "HAVING"
                | "BY"
                | "AS"
                | "DISTINCT"
                | "OFFSET"
                | "FETCH"
                | "NEXT"
                | "ROWS"
                | "ONLY"
                | "BEGIN"
                | "END"
                | "COMMIT"
                | "ROLLBACK"
                | "INTO"
                | "VALUES"
                | "SET"
                | "EXEC"
                | "PROCEDURE"
                | "FUNCTION"
                | "RETURN"
                | "IF"
                | "ELSE"
                | "WHILE"
                | "DECLARE"
                | "CURSOR"
                | "OPEN"
                | "CLOSE"
                | "PRIMARY"
                | "FOREIGN"
                | "KEY"
                | "REFERENCES"
                | "CONSTRAINT"
                | "DEFAULT"
                | "CHECK"
                | "UNIQUE"
                | "TRIGGER"
                | "SEQUENCE"
                | "SYSDATE"
                | "ROWNUM"
                | "LEVEL"
                | "CONNECT"
                | "START"
                | "WITH"
                | "MERGE"
                | "MATCHED"
                | "USING"
                | "WHEN"
                | "THEN"
                | "CASE"
        )
    }
}

impl Default for OracleDialect {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Oracle 类型枚举 API
// ============================================================================

/// Oracle 数据类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OracleDataType {
    Number,
    Varchar2,
    Nvarchar2,
    Char,
    Nchar,
    Clob,
    Nclob,
    Blob,
    Raw,
    Long,
    LongRaw,
    Date,
    Timestamp,
    TimestampTz,
    TimestampLtz,
    IntervalYearToMonth,
    IntervalDayToSecond,
    BinaryFloat,
    BinaryDouble,
    Rowid,
    Urowid,
    Xmltype,
    Null,
}

impl OracleDataType {
    /// 返回 SQL 类型名
    #[must_use]
    pub fn as_sql_type(&self) -> &'static str {
        match self {
            OracleDataType::Number => "NUMBER",
            OracleDataType::Varchar2 => "VARCHAR2",
            OracleDataType::Nvarchar2 => "NVARCHAR2",
            OracleDataType::Char => "CHAR",
            OracleDataType::Nchar => "NCHAR",
            OracleDataType::Clob => "CLOB",
            OracleDataType::Nclob => "NCLOB",
            OracleDataType::Blob => "BLOB",
            OracleDataType::Raw => "RAW",
            OracleDataType::Long => "LONG",
            OracleDataType::LongRaw => "LONG RAW",
            OracleDataType::Date => "DATE",
            OracleDataType::Timestamp => "TIMESTAMP",
            OracleDataType::TimestampTz => "TIMESTAMP WITH TIME ZONE",
            OracleDataType::TimestampLtz => "TIMESTAMP WITH LOCAL TIME ZONE",
            OracleDataType::IntervalYearToMonth => "INTERVAL YEAR TO MONTH",
            OracleDataType::IntervalDayToSecond => "INTERVAL DAY TO SECOND",
            OracleDataType::BinaryFloat => "BINARY_FLOAT",
            OracleDataType::BinaryDouble => "BINARY_DOUBLE",
            OracleDataType::Rowid => "ROWID",
            OracleDataType::Urowid => "UROWID",
            OracleDataType::Xmltype => "XMLTYPE",
            OracleDataType::Null => "NULL",
        }
    }

    /// 是否为数值类型
    #[must_use]
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            OracleDataType::Number | OracleDataType::BinaryFloat | OracleDataType::BinaryDouble
        )
    }

    /// 是否为字符串类型
    #[must_use]
    pub fn is_string(&self) -> bool {
        matches!(
            self,
            OracleDataType::Varchar2
                | OracleDataType::Nvarchar2
                | OracleDataType::Char
                | OracleDataType::Nchar
                | OracleDataType::Long
                | OracleDataType::Xmltype
        )
    }

    /// 是否为二进制类型
    #[must_use]
    pub fn is_binary(&self) -> bool {
        matches!(
            self,
            OracleDataType::Raw | OracleDataType::LongRaw | OracleDataType::Blob
        )
    }

    /// 是否为时间类型
    #[must_use]
    pub fn is_temporal(&self) -> bool {
        matches!(
            self,
            OracleDataType::Date
                | OracleDataType::Timestamp
                | OracleDataType::TimestampTz
                | OracleDataType::TimestampLtz
        )
    }

    /// 是否为 LOB 类型
    #[must_use]
    pub fn is_lob(&self) -> bool {
        matches!(
            self,
            OracleDataType::Clob | OracleDataType::Nclob | OracleDataType::Blob
        )
    }

    /// 从类型名解析
    #[must_use]
    pub fn parse_name(name: &str) -> Self {
        let upper = name.to_uppercase();
        if upper.starts_with("TIMESTAMP") {
            if upper.contains("LOCAL") {
                return OracleDataType::TimestampLtz;
            }
            if upper.contains("TIME ZONE") || upper.contains("TZ") {
                return OracleDataType::TimestampTz;
            }
            return OracleDataType::Timestamp;
        }
        if upper.starts_with("INTERVAL") {
            if upper.contains("YEAR") {
                return OracleDataType::IntervalYearToMonth;
            }
            return OracleDataType::IntervalDayToSecond;
        }
        match upper.as_str() {
            "NUMBER" | "NUMERIC" | "DECIMAL" | "DEC" | "INTEGER" | "INT" | "FLOAT" | "REAL" => {
                OracleDataType::Number
            }
            "VARCHAR2" | "VARCHAR" => OracleDataType::Varchar2,
            "NVARCHAR2" => OracleDataType::Nvarchar2,
            "CHAR" => OracleDataType::Char,
            "NCHAR" => OracleDataType::Nchar,
            "CLOB" => OracleDataType::Clob,
            "NCLOB" => OracleDataType::Nclob,
            "BLOB" => OracleDataType::Blob,
            "RAW" => OracleDataType::Raw,
            "LONG" => OracleDataType::Long,
            "LONG RAW" => OracleDataType::LongRaw,
            "DATE" => OracleDataType::Date,
            "BINARY_FLOAT" => OracleDataType::BinaryFloat,
            "BINARY_DOUBLE" => OracleDataType::BinaryDouble,
            "ROWID" => OracleDataType::Rowid,
            "UROWID" => OracleDataType::Urowid,
            "XMLTYPE" => OracleDataType::Xmltype,
            "NULL" => OracleDataType::Null,
            _ => OracleDataType::Varchar2,
        }
    }
}

// ============================================================================
// Oracle 错误分类 API
// ============================================================================

/// Oracle 错误分类（基于 ORA- 错误码）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OracleErrorCategory {
    /// 唯一约束违反（ORA-00001）
    DuplicateKey,
    /// 外键约束违反（ORA-02291）
    ForeignKeyViolation,
    /// 检查约束违反（ORA-02290）
    CheckConstraintViolation,
    /// 值过大（ORA-01401）
    ValueTooLarge,
    /// 无效 SQL（ORA-00900）
    InvalidSql,
    /// 对象不存在（ORA-00942）
    ObjectNotFound,
    /// 死锁（ORA-00060）
    Deadlock,
    /// 资源忙（ORA-00054）
    ResourceBusy,
    /// 超时
    Timeout,
    /// 其他
    Other,
}

impl OracleErrorCategory {
    /// 从 Oracle 错误码分类
    #[must_use]
    pub fn from_code(code: i32) -> Self {
        match code {
            1 => OracleErrorCategory::DuplicateKey,
            2291 => OracleErrorCategory::ForeignKeyViolation,
            2290 => OracleErrorCategory::CheckConstraintViolation,
            1401 => OracleErrorCategory::ValueTooLarge,
            900 => OracleErrorCategory::InvalidSql,
            942 => OracleErrorCategory::ObjectNotFound,
            60 => OracleErrorCategory::Deadlock,
            54 => OracleErrorCategory::ResourceBusy,
            _ => OracleErrorCategory::Other,
        }
    }

    /// 错误描述
    #[must_use]
    pub fn description(&self) -> &'static str {
        match self {
            OracleErrorCategory::DuplicateKey => "unique constraint violated",
            OracleErrorCategory::ForeignKeyViolation => "foreign key constraint violated",
            OracleErrorCategory::CheckConstraintViolation => "check constraint violated",
            OracleErrorCategory::ValueTooLarge => "value too large for column",
            OracleErrorCategory::InvalidSql => "invalid SQL statement",
            OracleErrorCategory::ObjectNotFound => "table or view does not exist",
            OracleErrorCategory::Deadlock => "deadlock detected",
            OracleErrorCategory::ResourceBusy => "resource busy",
            OracleErrorCategory::Timeout => "operation timed out",
            OracleErrorCategory::Other => "unknown error",
        }
    }

    /// 是否可重试（死锁/资源忙/超时）
    #[must_use]
    pub fn is_retriable(&self) -> bool {
        matches!(
            self,
            OracleErrorCategory::Deadlock
                | OracleErrorCategory::ResourceBusy
                | OracleErrorCategory::Timeout
        )
    }
}

// ============================================================================
// PL/SQL 调用辅助 API
// ============================================================================

/// PL/SQL 调用构建器
#[derive(Debug, Clone)]
pub struct PlSqlCall {
    name: String,
    is_function: bool,
    return_type: String,
    params: Vec<(String, String)>,
}

impl PlSqlCall {
    /// 构建存储过程调用
    #[must_use]
    pub fn procedure(name: &str) -> Self {
        Self {
            name: name.to_string(),
            is_function: false,
            return_type: String::new(),
            params: Vec::new(),
        }
    }

    /// 构建函数调用
    #[must_use]
    pub fn function(name: &str, return_type: &str) -> Self {
        Self {
            name: name.to_string(),
            is_function: true,
            return_type: return_type.to_string(),
            params: Vec::new(),
        }
    }

    /// 添加参数
    #[must_use]
    pub fn param(mut self, name: &str, value: &str) -> Self {
        self.params.push((name.to_string(), value.to_string()));
        self
    }

    /// 生成 PL/SQL 匿名块
    #[must_use]
    pub fn build(&self) -> String {
        let param_binds: Vec<String> = self
            .params
            .iter()
            .map(|(n, _)| format!("{n} => {n}_val"))
            .collect();
        let param_assigns: Vec<String> = self
            .params
            .iter()
            .map(|(n, v)| format!("{n}_val VARCHAR2(4000) := '{v}';"))
            .collect();

        if self.is_function {
            format!(
                "DECLARE\n  {}\n  result {};\nBEGIN\n  result := {}({});\nEND;",
                param_assigns.join("\n  "),
                self.return_type,
                self.name,
                param_binds.join(", ")
            )
        } else {
            format!(
                "DECLARE\n  {}\nBEGIN\n  {}({});\nEND;",
                param_assigns.join("\n  "),
                self.name,
                param_binds.join(", ")
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_placeholders_simple() {
        let sql = "SELECT * FROM users WHERE id = ? AND name = ?";
        let converted = convert_placeholders(sql);
        assert_eq!(converted, "SELECT * FROM users WHERE id = :1 AND name = :2");
    }

    #[test]
    fn test_convert_placeholders_none() {
        let sql = "SELECT * FROM users";
        let converted = convert_placeholders(sql);
        assert_eq!(converted, sql);
    }

    #[test]
    fn test_convert_placeholders_in_string_literal() {
        let sql = "SELECT * FROM users WHERE name = 'What?' AND id = ?";
        let converted = convert_placeholders(sql);
        assert_eq!(
            converted,
            "SELECT * FROM users WHERE name = 'What?' AND id = :1"
        );
    }

    #[test]
    fn test_convert_placeholders_many() {
        let sql = "INSERT INTO t (a, b, c, d) VALUES (?, ?, ?, ?)";
        let converted = convert_placeholders(sql);
        assert_eq!(
            converted,
            "INSERT INTO t (a, b, c, d) VALUES (:1, :2, :3, :4)"
        );
    }

    #[test]
    fn test_convert_placeholders_empty() {
        let converted = convert_placeholders("");
        assert_eq!(converted, "");
    }

    #[test]
    fn test_needs_raw_sql_create() {
        assert!(needs_raw_sql("CREATE TABLE users (id INT)"));
    }

    #[test]
    fn test_needs_raw_sql_alter() {
        assert!(needs_raw_sql(
            "ALTER TABLE users ADD COLUMN name VARCHAR(100)"
        ));
    }

    #[test]
    fn test_needs_raw_sql_dml() {
        assert!(!needs_raw_sql("SELECT * FROM users"));
        assert!(!needs_raw_sql("INSERT INTO users VALUES (1)"));
        assert!(!needs_raw_sql("UPDATE users SET name = 'x'"));
        assert!(!needs_raw_sql("DELETE FROM users"));
    }

    #[test]
    fn test_value_to_oracle_int() {
        let v = Value::I64(42);
        let _param = value_to_oracle_to_sql(&v);
    }

    #[test]
    fn test_value_to_oracle_string() {
        let v = Value::String("hello".to_string());
        let _param = value_to_oracle_to_sql(&v);
    }

    #[test]
    fn test_value_to_oracle_null() {
        let v = Value::Null;
        let _param = value_to_oracle_to_sql(&v);
    }

    #[test]
    fn test_value_to_oracle_float() {
        // 使用 1.5 避免触发 clippy::approx_constant (3.14 ≈ PI)
        let v = Value::F64(1.5);
        let _param = value_to_oracle_to_sql(&v);
    }

    #[test]
    fn test_value_to_oracle_bool() {
        let v = Value::Bool(true);
        let _param = value_to_oracle_to_sql(&v);
    }

    #[test]
    fn test_value_to_oracle_bytes() {
        let v = Value::Bytes(vec![1, 2, 3]);
        let _param = value_to_oracle_to_sql(&v);
    }

    #[test]
    fn test_value_to_oracle_array() {
        let v = Value::Array(vec![Value::I32(1), Value::I32(2)]);
        let _param = value_to_oracle_to_sql(&v);
    }

    #[test]
    fn test_value_to_oracle_object() {
        let v = Value::Object(std::collections::HashMap::from([(
            "a".to_string(),
            Value::I32(1),
        )]));
        let _param = value_to_oracle_to_sql(&v);
    }

    #[test]
    fn test_value_to_oracle_u64_boundary() {
        // u64 超出 i64 范围时降级为 i64 转换（不 panic）
        let v = Value::U64(u64::MAX);
        let _param = value_to_oracle_to_sql(&v);
    }

    #[test]
    fn test_value_to_oracle_decimal_string() {
        let v = Value::Decimal("123.45".to_string());
        let _param = value_to_oracle_to_sql(&v);
    }

    #[test]
    fn test_blocking_pool_config_defaults() {
        let cfg = OracleBlockingPoolConfig::default();
        assert!(cfg.max_blocking_threads > 0);
    }

    #[test]
    fn test_blocking_pool_new_and_drop() {
        let cfg = OracleBlockingPoolConfig::default();
        let pool = OracleBlockingPool::new(cfg);
        // 未连接时 spawn_blocking 仍可排队（真实连接需 Oracle 服务，此处仅验证结构）
        let _ = pool;
    }

    // ===== 连接串解析测试 =====

    #[test]
    fn test_parse_connect_string_full() {
        let info = parse_connect_string("localhost:1521/ORCLPDB1").unwrap();
        assert_eq!(info.host(), "localhost");
        assert_eq!(info.port(), 1521);
        assert_eq!(info.service_name(), "ORCLPDB1");
    }

    #[test]
    fn test_parse_connect_string_default_port() {
        let info = parse_connect_string("dbserver/MYSID").unwrap();
        assert_eq!(info.host(), "dbserver");
        assert_eq!(info.port(), 1521, "default port should be 1521");
        assert_eq!(info.service_name(), "MYSID");
    }

    #[test]
    fn test_parse_connect_string_missing_service() {
        let result = parse_connect_string("localhost:1521");
        assert!(result.is_err(), "missing service_name should error");
    }

    #[test]
    fn test_parse_connect_string_empty_host() {
        let result = parse_connect_string(":1521/SVC");
        assert!(result.is_err(), "empty host should error");
    }

    #[test]
    fn test_parse_connect_string_invalid_port() {
        let result = parse_connect_string("host:abc/SVC");
        assert!(result.is_err(), "invalid port should error");
    }

    #[test]
    fn test_parse_connect_string_roundtrip() {
        let info = parse_connect_string("host:1522/SVC").unwrap();
        let cs = info.as_connect_string();
        let info2 = parse_connect_string(&cs).unwrap();
        assert_eq!(info.host(), info2.host());
        assert_eq!(info.port(), info2.port());
        assert_eq!(info.service_name(), info2.service_name());
    }

    // ===== OracleDialect 测试 =====

    #[test]
    fn test_oracle_dialect_quote_identifier() {
        let d = OracleDialect::new();
        assert_eq!(d.quote_identifier("users"), "\"users\"");
        assert_eq!(d.quote_identifier("select"), "\"select\"");
    }

    #[test]
    fn test_oracle_dialect_quote_escape() {
        let d = OracleDialect::new();
        assert_eq!(d.quote_identifier("a\"b"), "\"a\"\"b\"");
    }

    #[test]
    fn test_oracle_dialect_limit_clause() {
        let d = OracleDialect::new();
        assert_eq!(
            d.limit_clause(Some(10), Some(20)),
            "OFFSET 20 ROWS FETCH NEXT 10 ROWS ONLY"
        );
        assert_eq!(d.limit_clause(Some(10), None), "FETCH NEXT 10 ROWS ONLY");
        assert_eq!(d.limit_clause(None, None), "");
    }

    #[test]
    fn test_oracle_dialect_placeholder() {
        let d = OracleDialect::new();
        assert_eq!(d.placeholder(1), ":1");
        assert_eq!(d.placeholder(3), ":3");
    }

    #[test]
    fn test_oracle_dialect_reserved_keyword() {
        let d = OracleDialect::new();
        assert!(d.is_reserved_keyword("SELECT"));
        assert!(d.is_reserved_keyword("rownum"));
        assert!(!d.is_reserved_keyword("my_table"));
    }

    // ===== OracleType 测试 =====

    #[test]
    fn test_oracle_type_as_sql_type() {
        assert_eq!(OracleDataType::Number.as_sql_type(), "NUMBER");
        assert_eq!(OracleDataType::Varchar2.as_sql_type(), "VARCHAR2");
        assert_eq!(
            OracleDataType::TimestampTz.as_sql_type(),
            "TIMESTAMP WITH TIME ZONE"
        );
    }

    #[test]
    fn test_oracle_type_is_numeric() {
        assert!(OracleDataType::Number.is_numeric());
        assert!(OracleDataType::BinaryFloat.is_numeric());
        assert!(OracleDataType::BinaryDouble.is_numeric());
        assert!(!OracleDataType::Varchar2.is_numeric());
    }

    #[test]
    fn test_oracle_type_is_string() {
        assert!(OracleDataType::Varchar2.is_string());
        assert!(OracleDataType::Nvarchar2.is_string());
        assert!(OracleDataType::Xmltype.is_string());
        assert!(!OracleDataType::Number.is_string());
    }

    #[test]
    fn test_oracle_type_is_binary() {
        assert!(OracleDataType::Raw.is_binary());
        assert!(OracleDataType::Blob.is_binary());
        assert!(OracleDataType::LongRaw.is_binary());
        assert!(!OracleDataType::Number.is_binary());
    }

    #[test]
    fn test_oracle_type_is_temporal() {
        assert!(OracleDataType::Date.is_temporal());
        assert!(OracleDataType::Timestamp.is_temporal());
        assert!(OracleDataType::TimestampTz.is_temporal());
        assert!(!OracleDataType::Number.is_temporal());
    }

    #[test]
    fn test_oracle_type_is_lob() {
        assert!(OracleDataType::Clob.is_lob());
        assert!(OracleDataType::Blob.is_lob());
        assert!(OracleDataType::Nclob.is_lob());
        assert!(!OracleDataType::Varchar2.is_lob());
    }

    #[test]
    fn test_oracle_type_parse_name() {
        assert_eq!(OracleDataType::parse_name("NUMBER"), OracleDataType::Number);
        assert_eq!(
            OracleDataType::parse_name("varchar2"),
            OracleDataType::Varchar2
        );
        assert_eq!(
            OracleDataType::parse_name("TIMESTAMP WITH TIME ZONE"),
            OracleDataType::TimestampTz
        );
        assert_eq!(
            OracleDataType::parse_name("INTEGER"),
            OracleDataType::Number
        );
        assert_eq!(
            OracleDataType::parse_name("unknown"),
            OracleDataType::Varchar2
        );
    }

    #[test]
    fn test_oracle_type_parse_timestamp_variants() {
        assert_eq!(
            OracleDataType::parse_name("TIMESTAMP"),
            OracleDataType::Timestamp
        );
        assert_eq!(
            OracleDataType::parse_name("TIMESTAMP WITH LOCAL TIME ZONE"),
            OracleDataType::TimestampLtz
        );
    }

    // ===== OracleErrorCategory 测试 =====

    #[test]
    fn test_oracle_error_category_duplicate_key() {
        assert_eq!(
            OracleErrorCategory::from_code(1),
            OracleErrorCategory::DuplicateKey
        );
    }

    #[test]
    fn test_oracle_error_category_fk_and_check() {
        assert_eq!(
            OracleErrorCategory::from_code(2291),
            OracleErrorCategory::ForeignKeyViolation
        );
        assert_eq!(
            OracleErrorCategory::from_code(2290),
            OracleErrorCategory::CheckConstraintViolation
        );
    }

    #[test]
    fn test_oracle_error_category_object_not_found() {
        assert_eq!(
            OracleErrorCategory::from_code(942),
            OracleErrorCategory::ObjectNotFound
        );
    }

    #[test]
    fn test_oracle_error_category_deadlock() {
        assert_eq!(
            OracleErrorCategory::from_code(60),
            OracleErrorCategory::Deadlock
        );
        assert!(OracleErrorCategory::Deadlock.is_retriable());
    }

    #[test]
    fn test_oracle_error_category_description() {
        assert_eq!(
            OracleErrorCategory::DuplicateKey.description(),
            "unique constraint violated"
        );
        assert_eq!(
            OracleErrorCategory::ObjectNotFound.description(),
            "table or view does not exist"
        );
    }

    #[test]
    fn test_oracle_error_category_is_retriable() {
        assert!(OracleErrorCategory::Deadlock.is_retriable());
        assert!(OracleErrorCategory::ResourceBusy.is_retriable());
        assert!(!OracleErrorCategory::DuplicateKey.is_retriable());
        assert!(!OracleErrorCategory::Other.is_retriable());
    }

    #[test]
    fn test_oracle_error_category_other() {
        assert_eq!(
            OracleErrorCategory::from_code(9999),
            OracleErrorCategory::Other
        );
    }

    // ===== PlSqlCall 测试 =====

    #[test]
    fn test_plsql_procedure_build() {
        let call = PlSqlCall::procedure("my_proc")
            .param("p1", "value1")
            .param("p2", "value2");
        let sql = call.build();
        assert!(sql.contains("BEGIN"));
        assert!(sql.contains("my_proc"));
        assert!(sql.contains("p1 => p1_val"));
        assert!(sql.contains("END;"));
    }

    #[test]
    fn test_plsql_function_build() {
        let call = PlSqlCall::function("my_func", "NUMBER").param("x", "42");
        let sql = call.build();
        assert!(sql.contains("result NUMBER"));
        assert!(sql.contains("result := my_func"));
    }

    #[test]
    fn test_plsql_procedure_no_params() {
        let call = PlSqlCall::procedure("simple_proc");
        let sql = call.build();
        assert!(sql.contains("simple_proc()"));
    }

    #[test]
    fn test_plsql_function_with_multiple_params() {
        let call = PlSqlCall::function("calc", "NUMBER")
            .param("a", "1")
            .param("b", "2")
            .param("c", "3");
        let sql = call.build();
        assert!(sql.contains("result NUMBER"));
        assert!(sql.contains("calc(a => a_val, b => b_val, c => c_val)"));
        assert!(sql.contains("a_val VARCHAR2(4000) := '1';"));
    }
}
