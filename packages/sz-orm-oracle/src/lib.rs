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
}
