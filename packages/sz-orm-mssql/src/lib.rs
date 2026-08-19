//! SZ-ORM Microsoft SQL Server database adapter
//!
//! Implements the `Connection` trait of sz-orm-core based on the `tiberius` crate
//! (pure Rust TDS protocol implementation), supporting SQL Server 2008+ (TDS 7.3+).
//!
//! # Design notes
//!
//! `tiberius` is a pure Rust asynchronous library that communicates directly with
//! SQL Server via the TDS (Tabular Data Stream) protocol, without requiring native
//! client libraries (such as ODBC/OLEDB).
//!
//! tiberius 0.12 uses `futures_io` traits, so tokio TcpStream must be wrapped via
//! `tokio-util::compat`.
//!
//! SQL Server placeholders use the `@P1, @P2, ...` format; this adapter automatically
//! converts `?` in SQL to `@PN`.
//!
//! # Usage
//!
//! ```ignore
//! use sz_orm_core::{Pool, PoolConfigBuilder};
//! use sz_orm_mssql::{MssqlPoolHandle, MssqlConnectionFactory};
//! use std::sync::Arc;
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let handle = Arc::new(MssqlPoolHandle::connect("server=tcp:localhost,1433;user=sa;password=P@ssw0rd;database=test").await?);
//! let factory = Arc::new(MssqlConnectionFactory::new(handle));
//! let config = PoolConfigBuilder::new().max_size(10).build()?;
//! let pool = Pool::new(config, factory)?;
//! # Ok(())
//! # }
//! ```

pub mod bulk_insert;
pub mod connection_config;
pub mod index_optimization;
pub mod transaction_config;
pub mod tsql_stored_procedure;
pub mod type_mapping;

pub use bulk_insert::{BulkInsert, BulkInsertOptions, BulkInsertResult, BulkInsertStrategy};
pub use connection_config::{
    AuthenticationMode, ConnectionPoolConfig, EncryptionConfig, MssqlConnectionConfig, TlsVersion,
};
pub use index_optimization::{
    IndexOptimizationAdvisor, IndexSuggestion, IndexType, IndexUsageStats, MissingIndexInfo,
};
pub use transaction_config::{DeadlockPriority, TransactionConfig, TransactionIsolation};
pub use tsql_stored_procedure::{
    TSqlBatchExec, TSqlParamDirection, TSqlParameter, TSqlStoredProcedure,
};
pub use type_mapping::{MssqlColumnMeta, MssqlTypeKind, MssqlTypeMapping, ValueKind};

use async_trait::async_trait;
use futures_util::StreamExt;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use tokio::net::TcpStream;
use tokio::sync::Notify;
use tokio_util::compat::TokioAsyncReadCompatExt;

use sz_orm_core::{
    Connection, ConnectionFactory, DbError, QueryRows, QueryStreamItem, QueryValues, Value,
};
use tiberius::{ColumnData, Row as MssqlRow, ToSql};

/// tiberius 0.12 uses futures_io traits; tokio TcpStream must be wrapped via tokio-util compat
type CompatTcpStream = tokio_util::compat::Compat<TcpStream>;

// 连接池内部状态用 `std::sync::Mutex` 保护：只在 `acquire()`（弹出空闲连接/更新
// 计数）与 `Drop`（归还连接）时短暂持锁，不在锁内执行任何 `.await`，因此用同步
// 互斥锁而非 `tokio::sync::Mutex`，避免跨 await 持锁且开销更低。

/// Error conversion: tiberius::error::Error → DbError
fn map_tiberius_error(e: tiberius::error::Error) -> DbError {
    let msg = e.to_string();
    if msg.contains("Connection")
        || msg.contains("connection")
        || msg.contains("Broken pipe")
        || msg.contains("Login failed")
    {
        DbError::ConnectionError(msg)
    } else if msg.contains("timeout") || msg.contains("Timeout") {
        DbError::ConnectionTimeout(msg)
    } else if msg.contains("already exists") || msg.contains("2627") || msg.contains("2601") {
        // SQL Server 2627=唯一约束违反, 2601=重复键索引
        DbError::UniqueViolation(msg)
    } else if msg.contains("547") {
        // SQL Server 547=外键约束违反
        DbError::ForeignKeyViolation(msg)
    } else if msg.contains("515") {
        DbError::NullValue(msg)
    } else if msg.contains("208") || msg.contains("Invalid object name") {
        DbError::NotFound(msg)
    } else if msg.contains("Config") || msg.contains("config") {
        DbError::ConfigError(msg)
    } else {
        DbError::QueryError(msg)
    }
}

/// Determines whether the SQL needs to go through the simple_query path
/// (DDL/DCL/transaction control statements do not use prepared statements)
fn needs_simple_query(sql: &str) -> bool {
    let upper = sql.trim_start().to_uppercase();
    upper.starts_with("CREATE ")
        || upper.starts_with("ALTER ")
        || upper.starts_with("DROP ")
        || upper.starts_with("TRUNCATE ")
        || upper.starts_with("GRANT ")
        || upper.starts_with("REVOKE ")
        || upper.starts_with("USE ")
        || upper.starts_with("SET ")
        || upper.starts_with("BEGIN TRAN")
        || upper.starts_with("COMMIT")
        || upper.starts_with("ROLLBACK")
        || upper.starts_with("SAVE TRAN")
        || upper.starts_with("EXEC ")
        || upper.starts_with("EXECUTE ")
}

/// Converts `?` placeholders in SQL to SQL Server's `@PN` format
///
/// Skips `?` inside string literals; only converts parameter placeholders.
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
                result.push_str(&format!("@P{}", param_index));
                param_index += 1;
            }
            _ => {
                result.push(ch);
            }
        }
    }
    result
}

/// Owned parameter enum that implements tiberius::ToSql for prepared statement binding.
///
/// tiberius 0.12's `Client::execute/query` accepts `&[&dyn ToSql]`.
/// By implementing `ToSql` for `MssqlParamOwned`, it can be passed directly as a parameter.
enum MssqlParamOwned {
    Null,
    Bool(Option<bool>),
    I16(Option<i16>),
    I32(Option<i32>),
    I64(Option<i64>),
    U8(Option<u8>),
    F32(Option<f32>),
    F64(Option<f64>),
    String(String),
    Bytes(Vec<u8>),
}

impl ToSql for MssqlParamOwned {
    fn to_sql(&self) -> ColumnData<'_> {
        match self {
            MssqlParamOwned::Null => ColumnData::I32(None),
            MssqlParamOwned::Bool(b) => ColumnData::Bit(*b),
            MssqlParamOwned::I16(n) => ColumnData::I16(*n),
            MssqlParamOwned::I32(n) => ColumnData::I32(*n),
            MssqlParamOwned::I64(n) => ColumnData::I64(*n),
            MssqlParamOwned::U8(n) => ColumnData::U8(*n),
            MssqlParamOwned::F32(f) => ColumnData::F32(*f),
            MssqlParamOwned::F64(f) => ColumnData::F64(*f),
            MssqlParamOwned::String(s) => {
                ColumnData::String(Some(std::borrow::Cow::Borrowed(s.as_str())))
            }
            MssqlParamOwned::Bytes(b) => {
                ColumnData::Binary(Some(std::borrow::Cow::Borrowed(b.as_slice())))
            }
        }
    }
}

/// Converts a sz-orm Value into MssqlParamOwned (an owned parameter)
///
/// # Performance notes
///
/// The `String`/`Bytes` branches clone the underlying data. Because `Value` itself
/// holds ownership and `MssqlParamOwned` also needs owned data to cross the async
/// boundary (tiberius's `Client::execute/query` executes asynchronously), this clone
/// is unavoidable.
///
/// To reduce the per-call clone overhead, consider:
/// - Using batch operations (`sz-orm-batch`) to amortize the cost of parameter construction;
/// - Pre-constructing parameters outside the loop and reusing them;
/// - Using pagination or streaming for large text/binary fields.
fn value_to_mssql_param_owned(value: &Value) -> MssqlParamOwned {
    match value {
        Value::Null => MssqlParamOwned::Null,
        Value::Bool(b) => MssqlParamOwned::Bool(Some(*b)),
        Value::I8(n) => MssqlParamOwned::I32(Some(*n as i32)),
        Value::I16(n) => MssqlParamOwned::I16(Some(*n)),
        Value::I32(n) => MssqlParamOwned::I32(Some(*n)),
        Value::I64(n) => MssqlParamOwned::I64(Some(*n)),
        Value::U8(n) => MssqlParamOwned::U8(Some(*n)),
        Value::U16(n) => MssqlParamOwned::I32(Some(*n as i32)),
        Value::U32(n) => MssqlParamOwned::I64(Some(*n as i64)),
        Value::U64(n) => MssqlParamOwned::I64(Some(*n as i64)),
        Value::F32(f) => MssqlParamOwned::F32(Some(*f)),
        Value::F64(f) => MssqlParamOwned::F64(Some(*f)),
        Value::String(s) => MssqlParamOwned::String(s.clone()),
        Value::Bytes(b) => MssqlParamOwned::Bytes(b.clone()),
        // DECIMAL/NUMERIC 以字符串绑定，SQL Server 隐式转换为 DECIMAL 列
        Value::Decimal(s) => MssqlParamOwned::String(s.clone()),
        Value::Date(s) | Value::DateTime(s) | Value::Time(s) => MssqlParamOwned::String(s.clone()),
        Value::Uuid(s) => MssqlParamOwned::String(s.clone()),
        Value::Json(s) => MssqlParamOwned::String(s.clone()),
        Value::Array(arr) => {
            let json = serde_json::to_string(arr).unwrap_or_else(|_| "[]".to_string());
            MssqlParamOwned::String(json)
        }
        Value::Object(obj) => {
            let json = serde_json::to_string(obj).unwrap_or_else(|_| "{}".to_string());
            MssqlParamOwned::String(json)
        }
        // 兼容 #[non_exhaustive] 的 Value：未知变体降级为 Null
        _ => MssqlParamOwned::Null,
    }
}

/// Converts the specified column of a tiberius Row into a sz-orm Value
///
/// tiberius 0.12's `FromSql` is implemented for `&str`/`&[u8]` (borrowed);
/// `String`/`Vec<u8>` only implement `FromSqlOwned`.
/// `try_get` requires `FromSql<'a>`, so `&str`/`&[u8]` are used and then converted to owned.
///
/// DECIMAL/NUMERIC/MONEY/SMALLMONEY are preferentially decoded as
/// `rust_decimal::Decimal` into `Value::Decimal` (string form) to avoid f64 precision loss.
fn row_to_value(row: &MssqlRow, idx: usize) -> Value {
    // 1. 尝试 i64 (覆盖 BIT/TINYINT/SMALLINT/INT/BIGINT)
    if let Ok(v) = row.try_get::<i64, usize>(idx) {
        return v.map(Value::I64).unwrap_or(Value::Null);
    }
    // 2. 尝试 rust_decimal::Decimal (覆盖 DECIMAL/NUMERIC/MONEY/SMALLMONEY，保留精度)
    if let Ok(v) = row.try_get::<rust_decimal::Decimal, usize>(idx) {
        return v
            .map(|d| Value::Decimal(d.to_string()))
            .unwrap_or(Value::Null);
    }
    // 3. 尝试 f64 (覆盖 REAL/FLOAT)
    if let Ok(v) = row.try_get::<f64, usize>(idx) {
        return v.map(Value::F64).unwrap_or(Value::Null);
    }
    // 4. 尝试 bool (覆盖 BIT)
    if let Ok(v) = row.try_get::<bool, usize>(idx) {
        return v.map(Value::Bool).unwrap_or(Value::Null);
    }
    // 5. 尝试 &[u8] (覆盖 BINARY/VARBINARY/IMAGE)
    if let Ok(v) = row.try_get::<&[u8], usize>(idx) {
        return v.map(|b| Value::Bytes(b.to_vec())).unwrap_or(Value::Null);
    }
    // 6. 尝试 &str (覆盖 CHAR/VARCHAR/NCHAR/NVARCHAR/TEXT/NTEXT/DATE/TIME/DATETIME 等)
    if let Ok(v) = row.try_get::<&str, usize>(idx) {
        return v
            .map(|s| Value::String(s.to_string()))
            .unwrap_or(Value::Null);
    }
    Value::Null
}

/// MSSQL connection pool internal state
struct MssqlPoolInner {
    /// Idle connection list: `acquire()` pops from the tail, `Drop` pushes to the tail on return
    idle: Vec<tiberius::Client<CompatTcpStream>>,
    /// Total number of created connections (idle + in-use), used to constrain the pool size upper bound
    total: usize,
}

/// SQL Server connection pool handle
///
/// v1.2.0 fixes P0: replaces the single-connection pseudo-pool (`Mutex<Option<Client>>`,
/// concurrency=1) with a real connection pool (`Mutex<MssqlPoolInner>` holding multiple
/// idle connections + a `Notify` wait mechanism), with concurrency = `max_size` (default 10).
///
/// # Design highlights
///
/// - **Multi-connection concurrency**: `idle` holds multiple idle connections; `acquire()`
///   pops one for the caller's exclusive use, and it is returned via `MssqlClientGuard::drop`
///   after the query completes
/// - **Pool size constraint**: `total` tracks the number of created connections (idle + in-use);
///   `acquire()` creates a new connection when `total < max_size`, otherwise waits on `Notify`
///   for a return
/// - **Lock granularity**: `std::sync::Mutex` is held only briefly during pop/return/counter
///   updates, never across `.await`; `tiberius::Client::connect` performs async connection
///   establishment outside the lock
/// - **Connection reuse**: connections are returned to `idle` on `Drop` without being closed,
///   avoiding repeated connection establishment overhead
pub struct MssqlPoolHandle {
    conn_str: String,
    /// Connection pool internal state (idle connections + total count)
    inner: Mutex<MssqlPoolInner>,
    /// Connection return notification: when the pool is full, `acquire()` awaits `notified().await` here; `Drop` calls `notify_one`
    notify: Notify,
    /// Connection pool size upper bound (default 10)
    max_size: usize,
}

impl MssqlPoolHandle {
    /// Creates a new SQL Server connection pool (pool size upper bound defaults to 10)
    pub async fn connect(conn_str: &str) -> Result<Self, DbError> {
        Self::connect_with_max_size(conn_str, 10).await
    }

    /// Creates a new SQL Server connection pool (custom pool size upper bound)
    ///
    /// # Parameters
    ///
    /// - `max_size`: Pool size upper bound. Passing 0 is replaced with the default value 10
    pub async fn connect_with_max_size(conn_str: &str, max_size: usize) -> Result<Self, DbError> {
        let config = tiberius::Config::from_ado_string(conn_str)
            .map_err(|e| DbError::ConfigError(e.to_string()))?;
        let tcp = tokio::net::TcpStream::connect(config.get_addr())
            .await
            .map_err(|e| DbError::ConnectionError(e.to_string()))?;
        tcp.set_nodelay(true).ok();
        let client = tiberius::Client::connect(config, tcp.compat())
            .await
            .map_err(map_tiberius_error)?;
        let max_size = if max_size == 0 { 10 } else { max_size };
        Ok(Self {
            conn_str: conn_str.to_string(),
            inner: Mutex::new(MssqlPoolInner {
                idle: vec![client],
                total: 1,
            }),
            notify: Notify::new(),
            max_size,
        })
    }

    /// Acquires an idle connection from the pool (for exclusive use; automatically returned on `Drop`)
    ///
    /// # Acquire strategy
    ///
    /// 1. First pops an idle connection from `idle`
    /// 2. If `idle` is empty and `total < max_size`, creates a new connection and increments `total`
    /// 3. If `total >= max_size`, waits on `notify` until a connection is returned
    ///
    /// # Lock granularity
    ///
    /// `std::sync::Mutex` is held only briefly when popping a connection / updating counters;
    /// async connection establishment (`TcpStream::connect` / `Client::connect`) is performed
    /// outside the lock via `.await`, without blocking other callers' `acquire()`.
    pub async fn acquire(&self) -> Result<MssqlClientGuard<'_>, DbError> {
        loop {
            // 短暂持锁：弹出空闲连接或决定是否创建新连接
            let create_new = {
                let mut inner = self
                    .inner
                    .lock()
                    .map_err(|e| DbError::Internal(format!("MSSQL pool mutex poisoned: {}", e)))?;
                if let Some(client) = inner.idle.pop() {
                    return Ok(MssqlClientGuard {
                        client: Some(client),
                        pool: self,
                    });
                }
                if inner.total < self.max_size {
                    // 预占一个名额，锁外再建连
                    inner.total += 1;
                    true
                } else {
                    false
                }
            };

            if create_new {
                // 锁外异步建连：不阻塞其他 acquire()
                match self.build_client().await {
                    Ok(client) => {
                        return Ok(MssqlClientGuard {
                            client: Some(client),
                            pool: self,
                        });
                    }
                    Err(e) => {
                        // 建连失败：回滚 total 并唤醒一个等待者，避免其空等
                        let mut inner = self.inner.lock().map_err(|e| {
                            DbError::Internal(format!("MSSQL pool mutex poisoned: {}", e))
                        })?;
                        inner.total -= 1;
                        drop(inner);
                        self.notify.notify_one();
                        return Err(e);
                    }
                }
            } else {
                // 池满：等待连接归还
                self.notify.notified().await;
            }
        }
    }

    /// Establishes a new tiberius client (private helper method)
    async fn build_client(&self) -> Result<tiberius::Client<CompatTcpStream>, DbError> {
        let config = tiberius::Config::from_ado_string(&self.conn_str)
            .map_err(|e| DbError::ConfigError(e.to_string()))?;
        let tcp = tokio::net::TcpStream::connect(config.get_addr())
            .await
            .map_err(|e| DbError::ConnectionError(e.to_string()))?;
        tcp.set_nodelay(true).ok();
        tiberius::Client::connect(config, tcp.compat())
            .await
            .map_err(map_tiberius_error)
    }

    pub fn conn_str(&self) -> &str {
        &self.conn_str
    }

    /// Returns the connection pool size upper bound
    pub fn max_size(&self) -> usize {
        self.max_size
    }
}

/// Exclusive guard for a SQL Server connection
///
/// Holds the client taken from the pool; on `Drop` it is automatically returned to the
/// pool's `idle` list and one waiter is notified. The underlying `tiberius::Client` is
/// accessed transparently via `Deref` / `DerefMut`.
pub struct MssqlClientGuard<'a> {
    /// The currently held client; taken out and returned on `Drop`, always `Some` during normal use
    client: Option<tiberius::Client<CompatTcpStream>>,
    /// Reference to the pool, used to return the connection
    pool: &'a MssqlPoolHandle,
}

impl std::ops::Deref for MssqlClientGuard<'_> {
    type Target = tiberius::Client<CompatTcpStream>;
    fn deref(&self) -> &tiberius::Client<CompatTcpStream> {
        self.client.as_ref().expect("connection must exist")
    }
}

impl std::ops::DerefMut for MssqlClientGuard<'_> {
    fn deref_mut(&mut self) -> &mut tiberius::Client<CompatTcpStream> {
        self.client.as_mut().expect("connection must exist")
    }
}

impl Drop for MssqlClientGuard<'_> {
    fn drop(&mut self) {
        if let Some(client) = self.client.take() {
            // 即使 mutex poisoned 也强制归还，避免连接泄漏
            let mut inner = self.pool.inner.lock().unwrap_or_else(|e| e.into_inner());
            inner.idle.push(client);
            drop(inner);
            // 唤醒一个等待 acquire() 的任务
            self.pool.notify.notify_one();
        }
    }
}

/// SQL Server connection factory
pub struct MssqlConnectionFactory {
    handle: Arc<MssqlPoolHandle>,
}

impl MssqlConnectionFactory {
    pub fn new(handle: Arc<MssqlPoolHandle>) -> Self {
        Self { handle }
    }
}

#[async_trait]
impl ConnectionFactory for MssqlConnectionFactory {
    async fn create(&self) -> Result<Box<dyn Connection>, DbError> {
        let _ = self.handle.acquire().await?;
        Ok(Box::new(MssqlConnection {
            handle: self.handle.clone(),
            connected: true,
            in_transaction: false,
        }))
    }
}

/// SQL Server connection wrapper
pub struct MssqlConnection {
    handle: Arc<MssqlPoolHandle>,
    connected: bool,
    in_transaction: bool,
}

impl MssqlConnection {
    /// Marks a connection error (called after the guard is released)
    fn mark_connection_error_after(&mut self, e: &DbError) {
        if matches!(e, DbError::ConnectionError(_) | DbError::IoError(_)) {
            self.connected = false;
        }
    }
}

impl Connection for MssqlConnection {
    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        Box::pin(async move {
            if !self.connected {
                return Err(DbError::ConnectionError("connection closed".to_string()));
            }
            let result = {
                let mut guard = self.handle.acquire().await?;
                let client = &mut *guard;
                if needs_simple_query(sql) {
                    // DDL/事务控制：simple_query 不返回 rows_affected，消费流即可
                    let mut stream = client.simple_query(sql).await.map_err(map_tiberius_error)?;
                    while stream.next().await.is_some() {}
                    Ok(1u64)
                } else {
                    // DML 无参数：用 execute 获取 rows_affected
                    let exec_result = client.execute(sql, &[]).await.map_err(map_tiberius_error)?;
                    Ok(exec_result.total())
                }
            };
            match result {
                Ok(n) => Ok(n),
                Err(e) => {
                    self.mark_connection_error_after(&e);
                    Err(e)
                }
            }
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
            let result = {
                let mut guard = self.handle.acquire().await?;
                let client = &mut *guard;
                let mut stream = client.simple_query(sql).await.map_err(map_tiberius_error)?;
                let mut result_rows: QueryRows = Vec::new();
                let mut col_names: Vec<String> = Vec::new();
                let mut col_set = false;
                while let Some(item_result) = stream.next().await {
                    match item_result {
                        Ok(tiberius::QueryItem::Row(row)) => {
                            if !col_set {
                                for col in row.columns() {
                                    col_names.push(col.name().to_string());
                                }
                                col_set = true;
                            }
                            let mut row_map: HashMap<String, Value> = HashMap::new();
                            for (i, col_name) in col_names.iter().enumerate() {
                                let value = row_to_value(&row, i);
                                row_map.insert(col_name.clone(), value);
                            }
                            result_rows.push(row_map);
                        }
                        Ok(tiberius::QueryItem::Metadata(_)) => {
                            // 元数据项，忽略（列信息从第一行提取）
                        }
                        Err(e) => {
                            return Err(map_tiberius_error(e));
                        }
                    }
                }
                Ok(result_rows)
            };
            match result {
                Ok(rows) => Ok(rows),
                Err(e) => {
                    self.mark_connection_error_after(&e);
                    Err(e)
                }
            }
        })
    }

    /// G-SX-4: MSSQL native cursor streaming query
    ///
    /// tiberius `simple_query` returns an async stream of `QueryItem::Row`; this method
    /// directly yields each row, avoiding the default implementation that collects all
    /// rows into a Vec before returning them one by one.
    /// Suitable for large result sets, significantly reducing peak memory.
    fn query_stream<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn futures::Stream<Item = QueryStreamItem> + Send + 'a>> {
        Box::pin(async_stream::try_stream! {
            if !self.connected {
                Err(DbError::ConnectionError("connection closed".to_string()))?;
            }
            let mut guard = self.handle.acquire().await?;
            let client = &mut *guard;
            let mut stream = client.simple_query(sql).await.map_err(map_tiberius_error)?;
            let mut col_names: Vec<String> = Vec::new();
            let mut col_set = false;
            while let Some(item_result) = stream.next().await {
                match item_result {
                    Ok(tiberius::QueryItem::Row(row)) => {
                        if !col_set {
                            for col in row.columns() {
                                col_names.push(col.name().to_string());
                            }
                            col_set = true;
                        }
                        let mut row_map: HashMap<String, Value> = HashMap::new();
                        for (i, col_name) in col_names.iter().enumerate() {
                            let value = row_to_value(&row, i);
                            row_map.insert(col_name.clone(), value);
                        }
                        yield row_map;
                    }
                    Ok(tiberius::QueryItem::Metadata(_)) => {
                        // 元数据项，忽略（列信息从第一行提取）
                    }
                    Err(e) => {
                        Err(map_tiberius_error(e))?;
                    }
                }
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
            if params.is_empty() || needs_simple_query(sql) {
                return self.execute(sql).await;
            }
            let sql_converted = convert_placeholders(sql);
            let params_owned: Vec<MssqlParamOwned> =
                params.iter().map(value_to_mssql_param_owned).collect();
            let params_ref: Vec<&dyn ToSql> =
                params_owned.iter().map(|p| p as &dyn ToSql).collect();

            let result = {
                let mut guard = self.handle.acquire().await?;
                let client = &mut *guard;
                let exec_result = client
                    .execute(&sql_converted, &params_ref)
                    .await
                    .map_err(map_tiberius_error)?;
                Ok(exec_result.total())
            };
            match result {
                Ok(n) => Ok(n),
                Err(e) => {
                    self.mark_connection_error_after(&e);
                    Err(e)
                }
            }
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
            let params_owned: Vec<MssqlParamOwned> =
                params.iter().map(value_to_mssql_param_owned).collect();
            let params_ref: Vec<&dyn ToSql> =
                params_owned.iter().map(|p| p as &dyn ToSql).collect();

            let result = {
                let mut guard = self.handle.acquire().await?;
                let client = &mut *guard;
                let mut stream = client
                    .query(&sql_converted, &params_ref)
                    .await
                    .map_err(map_tiberius_error)?;
                let mut result_rows: QueryRows = Vec::new();
                let mut col_names: Vec<String> = Vec::new();
                let mut col_set = false;
                while let Some(item_result) = stream.next().await {
                    match item_result {
                        Ok(tiberius::QueryItem::Row(row)) => {
                            if !col_set {
                                for col in row.columns() {
                                    col_names.push(col.name().to_string());
                                }
                                col_set = true;
                            }
                            let mut row_map: HashMap<String, Value> = HashMap::new();
                            for (i, col_name) in col_names.iter().enumerate() {
                                let value = row_to_value(&row, i);
                                row_map.insert(col_name.clone(), value);
                            }
                            result_rows.push(row_map);
                        }
                        Ok(tiberius::QueryItem::Metadata(_)) => {
                            // 元数据项，忽略
                        }
                        Err(e) => {
                            return Err(map_tiberius_error(e));
                        }
                    }
                }
                Ok(result_rows)
            };
            match result {
                Ok(rows) => Ok(rows),
                Err(e) => {
                    self.mark_connection_error_after(&e);
                    Err(e)
                }
            }
        })
    }

    /// Positional query (SELECT, no parameters): returns `(column names, value matrix in column order)`
    ///
    /// Bypasses the `HashMap<String, Value>` row mapping; column names are `to_string`-ed once
    /// and reused, and each row's values are pushed directly by column index into a `Vec`,
    /// with no hashing or string cloning.
    /// Suitable for SELECT ALL large result sets; compared to [`query`](Connection::query) this
    /// can deliver a 30%~50% performance improvement.
    fn query_values<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<QueryValues, DbError>> + Send + 'a>> {
        Box::pin(async move {
            if !self.connected {
                return Err(DbError::ConnectionError("connection closed".to_string()));
            }
            let result = {
                let mut guard = self.handle.acquire().await?;
                let client = &mut *guard;
                let mut stream = client.simple_query(sql).await.map_err(map_tiberius_error)?;
                let mut col_names: Vec<String> = Vec::new();
                let mut result_rows: Vec<Vec<Value>> = Vec::new();
                let mut col_set = false;
                while let Some(item_result) = stream.next().await {
                    match item_result {
                        Ok(tiberius::QueryItem::Row(row)) => {
                            if !col_set {
                                for col in row.columns() {
                                    col_names.push(col.name().to_string());
                                }
                                col_set = true;
                            }
                            let mut row_values: Vec<Value> = Vec::with_capacity(col_names.len());
                            for (i, _) in col_names.iter().enumerate() {
                                row_values.push(row_to_value(&row, i));
                            }
                            result_rows.push(row_values);
                        }
                        Ok(tiberius::QueryItem::Metadata(_)) => {
                            // 元数据项，忽略（列信息从第一行提取）
                        }
                        Err(e) => {
                            return Err(map_tiberius_error(e));
                        }
                    }
                }
                Ok::<QueryValues, DbError>((col_names, result_rows))
            };
            match result {
                Ok(values) => Ok(values),
                Err(e) => {
                    self.mark_connection_error_after(&e);
                    Err(e)
                }
            }
        })
    }

    /// Parameter-bound positional query (SELECT): combines prepared statement + positional mapping dual optimization
    ///
    /// Falls back to [`query_values`](Connection::query_values) when there are no parameters.
    fn query_values_with_params<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [Value],
    ) -> Pin<Box<dyn Future<Output = Result<QueryValues, DbError>> + Send + 'a>> {
        Box::pin(async move {
            if !self.connected {
                return Err(DbError::ConnectionError("connection closed".to_string()));
            }
            if params.is_empty() {
                return self.query_values(sql).await;
            }
            let sql_converted = convert_placeholders(sql);
            let params_owned: Vec<MssqlParamOwned> =
                params.iter().map(value_to_mssql_param_owned).collect();
            let params_ref: Vec<&dyn ToSql> =
                params_owned.iter().map(|p| p as &dyn ToSql).collect();

            let result = {
                let mut guard = self.handle.acquire().await?;
                let client = &mut *guard;
                let mut stream = client
                    .query(&sql_converted, &params_ref)
                    .await
                    .map_err(map_tiberius_error)?;
                let mut col_names: Vec<String> = Vec::new();
                let mut result_rows: Vec<Vec<Value>> = Vec::new();
                let mut col_set = false;
                while let Some(item_result) = stream.next().await {
                    match item_result {
                        Ok(tiberius::QueryItem::Row(row)) => {
                            if !col_set {
                                for col in row.columns() {
                                    col_names.push(col.name().to_string());
                                }
                                col_set = true;
                            }
                            let mut row_values: Vec<Value> = Vec::with_capacity(col_names.len());
                            for (i, _) in col_names.iter().enumerate() {
                                row_values.push(row_to_value(&row, i));
                            }
                            result_rows.push(row_values);
                        }
                        Ok(tiberius::QueryItem::Metadata(_)) => {
                            // 元数据项，忽略
                        }
                        Err(e) => {
                            return Err(map_tiberius_error(e));
                        }
                    }
                }
                Ok::<QueryValues, DbError>((col_names, result_rows))
            };
            match result {
                Ok(values) => Ok(values),
                Err(e) => {
                    self.mark_connection_error_after(&e);
                    Err(e)
                }
            }
        })
    }

    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            if !self.connected {
                return Err(DbError::ConnectionError("connection closed".to_string()));
            }
            let result = {
                let mut guard = self.handle.acquire().await?;
                let client = &mut *guard;
                client
                    .simple_query("BEGIN TRANSACTION")
                    .await
                    .map_err(map_tiberius_error)?;
                Ok::<(), DbError>(())
            };
            match result {
                Ok(_) => {
                    self.in_transaction = true;
                    Ok(())
                }
                Err(e) => {
                    self.mark_connection_error_after(&e);
                    Err(e)
                }
            }
        })
    }

    fn commit<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            if !self.connected {
                return Err(DbError::ConnectionError("connection closed".to_string()));
            }
            let result = {
                let mut guard = self.handle.acquire().await?;
                let client = &mut *guard;
                client
                    .simple_query("COMMIT")
                    .await
                    .map_err(map_tiberius_error)?;
                Ok::<(), DbError>(())
            };
            match result {
                Ok(_) => {
                    self.in_transaction = false;
                    Ok(())
                }
                Err(e) => {
                    self.mark_connection_error_after(&e);
                    Err(e)
                }
            }
        })
    }

    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            if !self.connected {
                return Err(DbError::ConnectionError("connection closed".to_string()));
            }
            let result = {
                let mut guard = self.handle.acquire().await?;
                let client = &mut *guard;
                client
                    .simple_query("ROLLBACK")
                    .await
                    .map_err(map_tiberius_error)?;
                Ok::<(), DbError>(())
            };
            match result {
                Ok(_) => {
                    self.in_transaction = false;
                    Ok(())
                }
                Err(e) => {
                    self.mark_connection_error_after(&e);
                    Err(e)
                }
            }
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
            self.query("SELECT 1").await.is_ok()
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
// Connection string parsing API
// ============================================================================

/// SQL Server connection string parsing result
#[derive(Debug, Clone)]
pub struct MssqlConnInfo {
    server: String,
    port: u16,
    database: String,
    user: String,
    password: String,
}

impl MssqlConnInfo {
    /// Returns the server address
    #[must_use]
    pub fn server(&self) -> &str {
        &self.server
    }

    /// Returns the port
    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Returns the database name
    #[must_use]
    pub fn database(&self) -> &str {
        &self.database
    }

    /// Returns the user name
    #[must_use]
    pub fn user(&self) -> &str {
        &self.user
    }

    /// Returns the password
    #[must_use]
    pub fn password(&self) -> &str {
        &self.password
    }

    /// Regenerates the DSN connection string
    #[must_use]
    pub fn as_dsn(&self) -> String {
        format!(
            "server={};port={};database={};user={};password={}",
            self.server, self.port, self.database, self.user, self.password
        )
    }
}

/// Parses a SQL Server connection string
///
/// Supported formats: `server=host;port=1433;database=db;user=sa;password=pwd`
/// or `server=host,port=1433;database=db;user=sa;password=pwd`
///
/// # Errors
///
/// Returns `DbError::Internal` if the `server` or `database` field is missing.
pub fn parse_conn_str(conn_str: &str) -> Result<MssqlConnInfo, DbError> {
    let mut server = String::new();
    let mut port: u16 = 1433;
    let mut database = String::new();
    let mut user = String::new();
    let mut password = String::new();

    for part in conn_str
        .split(';')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        let (key, val) = part
            .split_once('=')
            .or_else(|| part.split_once(','))
            .ok_or_else(|| DbError::Internal(format!("invalid conn_str segment: {part}")))?;
        let key = key.trim().to_lowercase();
        let val = val.trim();
        match key.as_str() {
            "server" => server = val.to_string(),
            "port" => port = val.parse().unwrap_or(1433),
            "database" | "db" => database = val.to_string(),
            "user" | "uid" => user = val.to_string(),
            "password" | "pwd" => password = val.to_string(),
            _ => {}
        }
    }

    if server.is_empty() {
        return Err(DbError::Internal("missing server in conn_str".to_string()));
    }
    if database.is_empty() {
        return Err(DbError::Internal(
            "missing database in conn_str".to_string(),
        ));
    }

    Ok(MssqlConnInfo {
        server,
        port,
        database,
        user,
        password,
    })
}

// ============================================================================
// SQL Server dialect helper API
// ============================================================================

/// SQL Server dialect helper
pub struct MssqlDialect;

impl MssqlDialect {
    /// Constructs a dialect helper
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Quotes an identifier (SQL Server uses square brackets `[name]`)
    #[must_use]
    pub fn quote_identifier(&self, name: &str) -> String {
        format!("[{}]", name.replace(']', "]]"))
    }

    /// Generates a LIMIT/OFFSET clause (SQL Server uses `OFFSET ... FETCH NEXT` syntax)
    ///
    /// - `limit` only: `TOP N` (compatible with SQL Server 2008+)
    /// - `limit` + `offset`: `OFFSET N ROWS FETCH NEXT M ROWS ONLY` (SQL Server 2012+)
    /// - Neither: empty string
    #[must_use]
    pub fn limit_clause(&self, limit: Option<u64>, offset: Option<u64>) -> String {
        match (limit, offset) {
            (Some(l), None) => format!("TOP {l} "),
            (Some(l), Some(o)) => {
                format!("OFFSET {o} ROWS FETCH NEXT {l} ROWS ONLY")
            }
            (None, Some(o)) => format!("OFFSET {o} ROWS"),
            (None, None) => String::new(),
        }
    }

    /// Generates a parameter placeholder (`@P1`, `@P2`, ...)
    #[must_use]
    pub fn placeholder(&self, index: usize) -> String {
        format!("@P{index}")
    }

    /// Checks whether the keyword is a SQL Server reserved word
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
                | "INNER"
                | "LEFT"
                | "RIGHT"
                | "OUTER"
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
                | "TOP"
                | "OFFSET"
                | "FETCH"
                | "NEXT"
                | "ROWS"
                | "ONLY"
                | "BEGIN"
                | "COMMIT"
                | "ROLLBACK"
                | "TRANSACTION"
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
                | "DEALLOCATE"
                | "TRIGGER"
                | "PRIMARY"
                | "FOREIGN"
                | "KEY"
                | "REFERENCES"
                | "CONSTRAINT"
                | "DEFAULT"
                | "CHECK"
                | "UNIQUE"
                | "CLUSTERED"
                | "NONCLUSTERED"
        )
    }
}

impl Default for MssqlDialect {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// SQL Server type enum API
// ============================================================================

/// SQL Server data types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MssqlType {
    Bigint,
    Binary,
    Bit,
    Char,
    Date,
    Datetime,
    Datetime2,
    Datetimeoffset,
    Decimal,
    Float,
    Image,
    Int,
    Money,
    Nchar,
    Ntext,
    Numeric,
    Nvarchar,
    Real,
    Smalldatetime,
    Smallint,
    Smallmoney,
    Text,
    Time,
    Tinyint,
    Uniqueidentifier,
    Varbinary,
    Varchar,
    Xml,
    Variant,
    Null,
}

impl MssqlType {
    /// Returns the SQL type name
    #[must_use]
    pub fn as_sql_type(&self) -> &'static str {
        match self {
            MssqlType::Bigint => "BIGINT",
            MssqlType::Binary => "BINARY",
            MssqlType::Bit => "BIT",
            MssqlType::Char => "CHAR",
            MssqlType::Date => "DATE",
            MssqlType::Datetime => "DATETIME",
            MssqlType::Datetime2 => "DATETIME2",
            MssqlType::Datetimeoffset => "DATETIMEOFFSET",
            MssqlType::Decimal => "DECIMAL",
            MssqlType::Float => "FLOAT",
            MssqlType::Image => "IMAGE",
            MssqlType::Int => "INT",
            MssqlType::Money => "MONEY",
            MssqlType::Nchar => "NCHAR",
            MssqlType::Ntext => "NTEXT",
            MssqlType::Numeric => "NUMERIC",
            MssqlType::Nvarchar => "NVARCHAR",
            MssqlType::Real => "REAL",
            MssqlType::Smalldatetime => "SMALLDATETIME",
            MssqlType::Smallint => "SMALLINT",
            MssqlType::Smallmoney => "SMALLMONEY",
            MssqlType::Text => "TEXT",
            MssqlType::Time => "TIME",
            MssqlType::Tinyint => "TINYINT",
            MssqlType::Uniqueidentifier => "UNIQUEIDENTIFIER",
            MssqlType::Varbinary => "VARBINARY",
            MssqlType::Varchar => "VARCHAR",
            MssqlType::Xml => "XML",
            MssqlType::Variant => "SQL_VARIANT",
            MssqlType::Null => "NULL",
        }
    }

    /// Whether it is a numeric type
    #[must_use]
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            MssqlType::Bigint
                | MssqlType::Bit
                | MssqlType::Decimal
                | MssqlType::Float
                | MssqlType::Int
                | MssqlType::Money
                | MssqlType::Numeric
                | MssqlType::Real
                | MssqlType::Smallint
                | MssqlType::Smallmoney
                | MssqlType::Tinyint
        )
    }

    /// Whether it is a string type
    #[must_use]
    pub fn is_string(&self) -> bool {
        matches!(
            self,
            MssqlType::Char
                | MssqlType::Nchar
                | MssqlType::Ntext
                | MssqlType::Nvarchar
                | MssqlType::Text
                | MssqlType::Varchar
                | MssqlType::Xml
        )
    }

    /// Whether it is a binary type
    #[must_use]
    pub fn is_binary(&self) -> bool {
        matches!(
            self,
            MssqlType::Binary | MssqlType::Image | MssqlType::Varbinary
        )
    }

    /// Whether it is a temporal type
    #[must_use]
    pub fn is_temporal(&self) -> bool {
        matches!(
            self,
            MssqlType::Date
                | MssqlType::Datetime
                | MssqlType::Datetime2
                | MssqlType::Datetimeoffset
                | MssqlType::Smalldatetime
                | MssqlType::Time
        )
    }

    /// Parses from a type name
    #[must_use]
    pub fn parse_name(name: &str) -> Self {
        match name.to_uppercase().as_str() {
            "BIGINT" => MssqlType::Bigint,
            "BINARY" => MssqlType::Binary,
            "BIT" => MssqlType::Bit,
            "CHAR" => MssqlType::Char,
            "DATE" => MssqlType::Date,
            "DATETIME" => MssqlType::Datetime,
            "DATETIME2" => MssqlType::Datetime2,
            "DATETIMEOFFSET" => MssqlType::Datetimeoffset,
            "DECIMAL" => MssqlType::Decimal,
            "FLOAT" => MssqlType::Float,
            "IMAGE" => MssqlType::Image,
            "INT" | "INTEGER" => MssqlType::Int,
            "MONEY" => MssqlType::Money,
            "NCHAR" => MssqlType::Nchar,
            "NTEXT" => MssqlType::Ntext,
            "NUMERIC" => MssqlType::Numeric,
            "NVARCHAR" => MssqlType::Nvarchar,
            "REAL" => MssqlType::Real,
            "SMALLDATETIME" => MssqlType::Smalldatetime,
            "SMALLINT" => MssqlType::Smallint,
            "SMALLMONEY" => MssqlType::Smallmoney,
            "TEXT" => MssqlType::Text,
            "TIME" => MssqlType::Time,
            "TINYINT" => MssqlType::Tinyint,
            "UNIQUEIDENTIFIER" => MssqlType::Uniqueidentifier,
            "VARBINARY" => MssqlType::Varbinary,
            "VARCHAR" => MssqlType::Varchar,
            "XML" => MssqlType::Xml,
            "SQL_VARIANT" => MssqlType::Variant,
            "NULL" => MssqlType::Null,
            _ => MssqlType::Variant,
        }
    }
}

// ============================================================================
// SQL Server error category API
// ============================================================================

/// SQL Server error categories
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MssqlErrorCategory {
    /// Primary/unique key conflict (error codes 2627/2601)
    DuplicateKey,
    /// Constraint violation (error code 547)
    ConstraintViolation,
    /// NULL value violates a non-null constraint (error code 515)
    NullViolation,
    /// Object does not exist (error code 208)
    InvalidObject,
    /// Deadlock (error code 1205)
    Deadlock,
    /// Timeout (error code -2)
    Timeout,
    /// Other errors
    Other,
}

impl MssqlErrorCategory {
    /// Categorizes from a SQL Server error code
    #[must_use]
    pub fn from_code(code: i32) -> Self {
        match code {
            2627 | 2601 => MssqlErrorCategory::DuplicateKey,
            547 => MssqlErrorCategory::ConstraintViolation,
            515 => MssqlErrorCategory::NullViolation,
            208 => MssqlErrorCategory::InvalidObject,
            1205 => MssqlErrorCategory::Deadlock,
            -2 => MssqlErrorCategory::Timeout,
            _ => MssqlErrorCategory::Other,
        }
    }

    /// Error description
    #[must_use]
    pub fn description(&self) -> &'static str {
        match self {
            MssqlErrorCategory::DuplicateKey => "duplicate key violation",
            MssqlErrorCategory::ConstraintViolation => "constraint violation",
            MssqlErrorCategory::NullViolation => "NULL value in non-null column",
            MssqlErrorCategory::InvalidObject => "invalid object name",
            MssqlErrorCategory::Deadlock => "deadlock detected",
            MssqlErrorCategory::Timeout => "query timeout",
            MssqlErrorCategory::Other => "unknown error",
        }
    }

    /// Whether it is retriable (deadlock/timeout)
    #[must_use]
    pub fn is_retriable(&self) -> bool {
        matches!(
            self,
            MssqlErrorCategory::Deadlock | MssqlErrorCategory::Timeout
        )
    }
}

// ============================================================================
// Unit tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_placeholders_simple() {
        let sql = "SELECT * FROM users WHERE id = ? AND name = ?";
        let converted = convert_placeholders(sql);
        assert_eq!(
            converted,
            "SELECT * FROM users WHERE id = @P1 AND name = @P2"
        );
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
            "SELECT * FROM users WHERE name = 'What?' AND id = @P1"
        );
    }

    #[test]
    fn test_convert_placeholders_many() {
        let sql = "INSERT INTO t (a, b, c, d) VALUES (?, ?, ?, ?)";
        let converted = convert_placeholders(sql);
        assert_eq!(
            converted,
            "INSERT INTO t (a, b, c, d) VALUES (@P1, @P2, @P3, @P4)"
        );
    }

    #[test]
    fn test_convert_placeholders_empty() {
        let converted = convert_placeholders("");
        assert_eq!(converted, "");
    }

    #[test]
    fn test_needs_simple_query_create() {
        assert!(needs_simple_query("CREATE TABLE users (id INT)"));
    }

    #[test]
    fn test_needs_simple_query_alter() {
        assert!(needs_simple_query(
            "ALTER TABLE users ADD COLUMN name VARCHAR(100)"
        ));
    }

    #[test]
    fn test_needs_simple_query_transaction() {
        assert!(needs_simple_query("BEGIN TRANSACTION"));
        assert!(needs_simple_query("COMMIT"));
        assert!(needs_simple_query("ROLLBACK"));
        assert!(needs_simple_query("SAVE TRAN sp1"));
    }

    #[test]
    fn test_needs_simple_query_dml() {
        assert!(!needs_simple_query("SELECT * FROM users"));
        assert!(!needs_simple_query("INSERT INTO users VALUES (1)"));
        assert!(!needs_simple_query("UPDATE users SET name = 'x'"));
        assert!(!needs_simple_query("DELETE FROM users"));
    }

    #[test]
    fn test_value_to_mssql_int() {
        let v = Value::I64(42);
        let param = value_to_mssql_param_owned(&v);
        match param {
            MssqlParamOwned::I64(Some(42)) => {}
            _ => panic!("expected I64(Some(42))"),
        }
    }

    #[test]
    fn test_value_to_mssql_string() {
        let v = Value::String("hello".to_string());
        let param = value_to_mssql_param_owned(&v);
        match param {
            MssqlParamOwned::String(s) => assert_eq!(s, "hello"),
            _ => panic!("expected String"),
        }
    }

    #[test]
    fn test_value_to_mssql_null() {
        let v = Value::Null;
        let param = value_to_mssql_param_owned(&v);
        assert!(matches!(param, MssqlParamOwned::Null));
    }

    #[test]
    fn test_value_to_mssql_float() {
        // 使用 1.5 避免触发 clippy::approx_constant (3.14 ≈ PI)
        let v = Value::F64(1.5);
        let param = value_to_mssql_param_owned(&v);
        if let MssqlParamOwned::F64(Some(f)) = param {
            assert!((f - 1.5).abs() < 1e-10);
        } else {
            panic!("expected F64");
        }
    }

    #[test]
    fn test_value_to_mssql_bool() {
        let v = Value::Bool(true);
        let param = value_to_mssql_param_owned(&v);
        assert!(matches!(param, MssqlParamOwned::Bool(Some(true))));
    }

    #[test]
    fn test_value_to_mssql_bytes() {
        let v = Value::Bytes(vec![1, 2, 3]);
        let param = value_to_mssql_param_owned(&v);
        if let MssqlParamOwned::Bytes(b) = param {
            assert_eq!(b, vec![1, 2, 3]);
        } else {
            panic!("expected Bytes");
        }
    }

    #[test]
    fn test_to_sql_null() {
        let param = MssqlParamOwned::Null;
        let col = param.to_sql();
        assert!(matches!(col, ColumnData::I32(None)));
    }

    #[test]
    fn test_to_sql_string() {
        let param = MssqlParamOwned::String("test".to_string());
        let col = param.to_sql();
        match col {
            ColumnData::String(Some(s)) => assert_eq!(s, "test"),
            _ => panic!("expected String(Some)"),
        }
    }

    #[test]
    fn test_value_mapping_u16_as_i32() {
        let param = value_to_mssql_param_owned(&Value::U16(65535));
        assert!(matches!(param, MssqlParamOwned::I32(Some(65535))));
    }

    #[test]
    fn test_value_mapping_u64_as_i64() {
        let big: u64 = 1 << 40;
        let param = value_to_mssql_param_owned(&Value::U64(big));
        assert!(matches!(param, MssqlParamOwned::I64(Some(v)) if v == big as i64));
    }

    #[test]
    fn test_value_mapping_decimal_as_string() {
        let param = value_to_mssql_param_owned(&Value::Decimal("123.45".to_string()));
        assert!(matches!(param, MssqlParamOwned::String(_)));
    }

    #[test]
    fn test_value_mapping_json_as_string() {
        let param = value_to_mssql_param_owned(&Value::Json(r#"{"a":1}"#.to_string()));
        assert!(matches!(param, MssqlParamOwned::String(_)));
    }

    #[test]
    fn test_value_mapping_array_as_json_string() {
        let param = value_to_mssql_param_owned(&Value::Array(vec![Value::I32(1)]));
        match param {
            MssqlParamOwned::String(s) => assert!(s.contains("1")),
            _ => panic!("expected String"),
        }
    }

    #[test]
    fn test_value_mapping_null() {
        let param = value_to_mssql_param_owned(&Value::Null);
        assert!(matches!(param, MssqlParamOwned::Null));
    }

    #[test]
    fn test_value_mapping_i64_roundtrip() {
        let param = value_to_mssql_param_owned(&Value::I64(-42));
        assert!(matches!(param, MssqlParamOwned::I64(Some(-42))));
    }

    // ===== 连接串解析测试 =====

    #[test]
    fn test_parse_conn_str_full() {
        let info =
            parse_conn_str("server=localhost;port=1433;database=testdb;user=sa;password=pwd123")
                .unwrap();
        assert_eq!(info.server(), "localhost");
        assert_eq!(info.port(), 1433);
        assert_eq!(info.database(), "testdb");
        assert_eq!(info.user(), "sa");
        assert_eq!(info.password(), "pwd123");
    }

    #[test]
    fn test_parse_conn_str_default_port() {
        let info =
            parse_conn_str("server=192.168.1.1;database=mydb;user=admin;password=secret").unwrap();
        assert_eq!(info.server(), "192.168.1.1");
        assert_eq!(info.port(), 1433, "default port should be 1433");
    }

    #[test]
    fn test_parse_conn_str_missing_server() {
        let result = parse_conn_str("database=testdb;user=sa;password=pwd");
        assert!(result.is_err(), "missing server should error");
    }

    #[test]
    fn test_parse_conn_str_missing_database() {
        let result = parse_conn_str("server=localhost;user=sa;password=pwd");
        assert!(result.is_err(), "missing database should error");
    }

    #[test]
    fn test_parse_conn_str_as_dsn_roundtrip() {
        let original = "server=host;port=1434;database=db;user=u;password=p";
        let info = parse_conn_str(original).unwrap();
        let dsn = info.as_dsn();
        let info2 = parse_conn_str(&dsn).unwrap();
        assert_eq!(info.server(), info2.server());
        assert_eq!(info.port(), info2.port());
        assert_eq!(info.database(), info2.database());
    }

    #[test]
    fn test_parse_conn_str_alternate_keys() {
        let info = parse_conn_str("server=h;database=d;uid=user1;pwd=pass1").unwrap();
        assert_eq!(info.user(), "user1");
        assert_eq!(info.password(), "pass1");
    }

    // ===== MssqlDialect 测试 =====

    #[test]
    fn test_dialect_quote_identifier() {
        let d = MssqlDialect::new();
        assert_eq!(d.quote_identifier("users"), "[users]");
        assert_eq!(d.quote_identifier("order"), "[order]");
    }

    #[test]
    fn test_dialect_quote_identifier_escape_bracket() {
        let d = MssqlDialect::new();
        assert_eq!(d.quote_identifier("a]b"), "[a]]b]");
    }

    #[test]
    fn test_dialect_limit_clause_top() {
        let d = MssqlDialect::new();
        assert_eq!(d.limit_clause(Some(10), None), "TOP 10 ");
    }

    #[test]
    fn test_dialect_limit_clause_offset_fetch() {
        let d = MssqlDialect::new();
        assert_eq!(
            d.limit_clause(Some(10), Some(20)),
            "OFFSET 20 ROWS FETCH NEXT 10 ROWS ONLY"
        );
    }

    #[test]
    fn test_dialect_limit_clause_empty() {
        let d = MssqlDialect::new();
        assert_eq!(d.limit_clause(None, None), "");
    }

    #[test]
    fn test_dialect_placeholder() {
        let d = MssqlDialect::new();
        assert_eq!(d.placeholder(1), "@P1");
        assert_eq!(d.placeholder(3), "@P3");
    }

    #[test]
    fn test_dialect_is_reserved_keyword() {
        let d = MssqlDialect::new();
        assert!(d.is_reserved_keyword("SELECT"));
        assert!(d.is_reserved_keyword("select"));
        assert!(d.is_reserved_keyword("TABLE"));
        assert!(!d.is_reserved_keyword("users"));
        assert!(!d.is_reserved_keyword("my_column"));
    }

    #[test]
    fn test_dialect_default() {
        let d = MssqlDialect;
        assert_eq!(d.placeholder(1), "@P1");
    }

    // ===== MssqlType 测试 =====

    #[test]
    fn test_mssql_type_as_sql_type() {
        assert_eq!(MssqlType::Int.as_sql_type(), "INT");
        assert_eq!(MssqlType::Varchar.as_sql_type(), "VARCHAR");
        assert_eq!(MssqlType::Datetime2.as_sql_type(), "DATETIME2");
        assert_eq!(
            MssqlType::Uniqueidentifier.as_sql_type(),
            "UNIQUEIDENTIFIER"
        );
    }

    #[test]
    fn test_mssql_type_is_numeric() {
        assert!(MssqlType::Int.is_numeric());
        assert!(MssqlType::Bigint.is_numeric());
        assert!(MssqlType::Decimal.is_numeric());
        assert!(MssqlType::Money.is_numeric());
        assert!(!MssqlType::Varchar.is_numeric());
        assert!(!MssqlType::Date.is_numeric());
    }

    #[test]
    fn test_mssql_type_is_string() {
        assert!(MssqlType::Varchar.is_string());
        assert!(MssqlType::Nvarchar.is_string());
        assert!(MssqlType::Text.is_string());
        assert!(MssqlType::Xml.is_string());
        assert!(!MssqlType::Int.is_string());
    }

    #[test]
    fn test_mssql_type_is_binary() {
        assert!(MssqlType::Varbinary.is_binary());
        assert!(MssqlType::Binary.is_binary());
        assert!(MssqlType::Image.is_binary());
        assert!(!MssqlType::Int.is_binary());
    }

    #[test]
    fn test_mssql_type_is_temporal() {
        assert!(MssqlType::Date.is_temporal());
        assert!(MssqlType::Datetime.is_temporal());
        assert!(MssqlType::Smalldatetime.is_temporal());
        assert!(MssqlType::Time.is_temporal());
        assert!(!MssqlType::Int.is_temporal());
    }

    #[test]
    fn test_mssql_type_parse_name() {
        assert_eq!(MssqlType::parse_name("INT"), MssqlType::Int);
        assert_eq!(MssqlType::parse_name("varchar"), MssqlType::Varchar);
        assert_eq!(MssqlType::parse_name("DATETIME2"), MssqlType::Datetime2);
        assert_eq!(MssqlType::parse_name("INTEGER"), MssqlType::Int);
        assert_eq!(MssqlType::parse_name("unknown"), MssqlType::Variant);
    }

    #[test]
    fn test_mssql_type_money_and_smalldatetime() {
        assert!(MssqlType::Money.is_numeric());
        assert!(MssqlType::Smallmoney.is_numeric());
        assert!(MssqlType::Smalldatetime.is_temporal());
        assert_eq!(MssqlType::Money.as_sql_type(), "MONEY");
        assert_eq!(MssqlType::Smalldatetime.as_sql_type(), "SMALLDATETIME");
    }

    #[test]
    fn test_mssql_type_uniqueidentifier() {
        assert!(!MssqlType::Uniqueidentifier.is_numeric());
        assert!(!MssqlType::Uniqueidentifier.is_string());
        assert!(!MssqlType::Uniqueidentifier.is_binary());
        assert_eq!(
            MssqlType::Uniqueidentifier.as_sql_type(),
            "UNIQUEIDENTIFIER"
        );
    }

    // ===== MssqlErrorCategory 测试 =====

    #[test]
    fn test_error_category_duplicate_key() {
        assert_eq!(
            MssqlErrorCategory::from_code(2627),
            MssqlErrorCategory::DuplicateKey
        );
        assert_eq!(
            MssqlErrorCategory::from_code(2601),
            MssqlErrorCategory::DuplicateKey
        );
    }

    #[test]
    fn test_error_category_constraint() {
        assert_eq!(
            MssqlErrorCategory::from_code(547),
            MssqlErrorCategory::ConstraintViolation
        );
    }

    #[test]
    fn test_error_category_deadlock_and_timeout() {
        assert_eq!(
            MssqlErrorCategory::from_code(1205),
            MssqlErrorCategory::Deadlock
        );
        assert_eq!(
            MssqlErrorCategory::from_code(-2),
            MssqlErrorCategory::Timeout
        );
    }

    #[test]
    fn test_error_category_description() {
        assert_eq!(
            MssqlErrorCategory::DuplicateKey.description(),
            "duplicate key violation"
        );
        assert_eq!(
            MssqlErrorCategory::Deadlock.description(),
            "deadlock detected"
        );
    }

    #[test]
    fn test_error_category_is_retriable() {
        assert!(MssqlErrorCategory::Deadlock.is_retriable());
        assert!(MssqlErrorCategory::Timeout.is_retriable());
        assert!(!MssqlErrorCategory::DuplicateKey.is_retriable());
        assert!(!MssqlErrorCategory::Other.is_retriable());
    }

    #[test]
    fn test_error_category_other() {
        assert_eq!(
            MssqlErrorCategory::from_code(999),
            MssqlErrorCategory::Other
        );
        assert_eq!(MssqlErrorCategory::Other.description(), "unknown error");
    }
}
