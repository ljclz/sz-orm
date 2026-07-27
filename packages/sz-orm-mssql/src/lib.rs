//! SZ-ORM Microsoft SQL Server 数据库适配器
//!
//! 基于 `tiberius` crate (纯 Rust TDS 协议实现) 实现 sz-orm-core 的 `Connection` trait,
//! 支持 SQL Server 2008+ (TDS 7.3+)。
//!
//! # 设计说明
//!
//! `tiberius` 是纯 Rust 异步库,基于 TDS (Tabular Data Stream) 协议直接与 SQL Server
//! 通信,无需原生客户端库 (如 ODBC/OLEDB)。
//!
//! tiberius 0.12 使用 `futures_io` traits,需通过 `tokio-util::compat` 包装 tokio TcpStream。
//!
//! SQL Server 占位符使用 `@P1, @P2, ...` 格式,本适配器自动将 SQL 中的 `?`
//! 转换为 `@PN`。
//!
//! # 用法
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

use async_trait::async_trait;
use futures_util::StreamExt;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use tokio::net::TcpStream;
use tokio::sync::Notify;
use tokio_util::compat::TokioAsyncReadCompatExt;

use sz_orm_core::{Connection, ConnectionFactory, DbError, QueryRows, QueryValues, Value};
use tiberius::{ColumnData, Row as MssqlRow, ToSql};

/// tiberius 0.12 使用 futures_io traits,需通过 tokio-util compat 包装 tokio TcpStream
type CompatTcpStream = tokio_util::compat::Compat<TcpStream>;

// 连接池内部状态用 `std::sync::Mutex` 保护：只在 `acquire()`（弹出空闲连接/更新
// 计数）与 `Drop`（归还连接）时短暂持锁，不在锁内执行任何 `.await`，因此用同步
// 互斥锁而非 `tokio::sync::Mutex`，避免跨 await 持锁且开销更低。

/// 错误转换: tiberius::error::Error → DbError
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

/// 判断 SQL 是否需要走 simple_query 路径 (DDL/DCL/事务控制语句不走 prepared statement)
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

/// 将 SQL 中的 `?` 占位符转换为 SQL Server 的 `@PN` 格式
///
/// 跳过字符串字面量内的 `?`,仅转换参数占位符。
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

/// 拥有所有权的参数枚举,实现 tiberius::ToSql 用于 prepared statement 绑定。
///
/// tiberius 0.12 的 `Client::execute/query` 接受 `&[&dyn ToSql]`,
/// 通过为 `MssqlParamOwned` 实现 `ToSql`,可直接作为参数传入。
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

/// 将 sz-orm Value 转换为 MssqlParamOwned (拥有所有权的参数)
///
/// # 性能说明
///
/// `String`/`Bytes` 分支会 clone 底层数据。由于 `Value` 本身持有所有权，
/// 而 `MssqlParamOwned` 也需要 owned 数据以跨越 async 边界（tiberius 的
/// `Client::execute/query` 异步执行），此处的 clone 无法避免。
///
/// 如需降低单次调用的 clone 开销，建议：
/// - 使用批量操作（`sz-orm-batch`）摊薄每次参数构造的成本；
/// - 在循环外预构造参数并复用；
/// - 对大文本/二进制字段考虑分页或流式处理。
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

/// 将 tiberius Row 中指定列转换为 sz-orm Value
///
/// tiberius 0.12 的 `FromSql` 为 `&str`/`&[u8]` 实现 (借用),`String`/`Vec<u8>` 仅实现 `FromSqlOwned`。
/// `try_get` 需要 `FromSql<'a>`,因此使用 `&str`/`&[u8]` 并转为 owned。
///
/// DECIMAL/NUMERIC/MONEY/SMALLMONEY 优先以 `rust_decimal::Decimal` 解码为
/// `Value::Decimal`(字符串形式),避免 f64 精度丢失。
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

/// MSSQL 连接池内部状态
struct MssqlPoolInner {
    /// 空闲连接列表：`acquire()` 从尾部弹出，`Drop` 归还时推入尾部
    idle: Vec<tiberius::Client<CompatTcpStream>>,
    /// 已创建连接总数（空闲 + 在用），用于约束池大小上限
    total: usize,
}

/// SQL Server 连接池句柄
///
/// v1.2.0 修复 P0：从单连接伪池（`Mutex<Option<Client>>`，并发度=1）改为
/// 真正的连接池（`Mutex<MssqlPoolInner>` 持有多个空闲连接 + `Notify` 等待
/// 机制），并发度 = `max_size`（默认 10）。
///
/// # 设计要点
///
/// - **多连接并发**：`idle` 持有多个空闲连接，`acquire()` 弹出一个供调用方
///   独占使用，查询结束后通过 `MssqlClientGuard::drop` 归还
/// - **池大小约束**：`total` 跟踪已创建连接数（空闲+在用），`acquire()` 在
///   `total < max_size` 时创建新连接，否则在 `Notify` 上等待归还
/// - **锁粒度**：`std::sync::Mutex` 仅在弹出/归还/计数更新时短暂持有，不跨
///   `.await`；`tiberius::Client::connect` 是异步建连，在锁外执行
/// - **连接复用**：连接在 `Drop` 时归还到 `idle`，不关闭，避免反复建连开销
pub struct MssqlPoolHandle {
    conn_str: String,
    /// 连接池内部状态（空闲连接 + 总数）
    inner: Mutex<MssqlPoolInner>,
    /// 连接归还通知：池满时 `acquire()` 在此 `notified().await`，`Drop` 时 `notify_one`
    notify: Notify,
    /// 连接池大小上限（默认 10）
    max_size: usize,
}

impl MssqlPoolHandle {
    /// 创建新的 SQL Server 连接池（连接池上限默认 10）
    pub async fn connect(conn_str: &str) -> Result<Self, DbError> {
        Self::connect_with_max_size(conn_str, 10).await
    }

    /// 创建新的 SQL Server 连接池（自定义连接池大小上限）
    ///
    /// # 参数
    ///
    /// - `max_size`：连接池大小上限。传入 0 会被替换为默认值 10
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

    /// 从池中获取一个空闲连接（独占使用，`Drop` 时自动归还）
    ///
    /// # 获取策略
    ///
    /// 1. 优先从 `idle` 弹出一个空闲连接
    /// 2. 若 `idle` 为空且 `total < max_size`，创建新连接并递增 `total`
    /// 3. 若 `total >= max_size`，在 `notify` 上等待，直到有连接归还
    ///
    /// # 锁粒度
    ///
    /// `std::sync::Mutex` 仅在弹出连接/更新计数时短暂持有，异步建连
    /// （`TcpStream::connect` / `Client::connect`）在锁外 `.await`，
    /// 不阻塞其他调用方的 `acquire()`。
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

    /// 建立一个新的 tiberius 客户端（私有辅助方法）
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

    /// 获取连接池大小上限
    pub fn max_size(&self) -> usize {
        self.max_size
    }
}

/// SQL Server 连接的独占守卫
///
/// 持有从池中取出的客户端，`Drop` 时自动归还到池的 `idle` 列表并通知一个等待者。
/// 通过 `Deref` / `DerefMut` 透明访问底层 `tiberius::Client`。
pub struct MssqlClientGuard<'a> {
    /// 当前持有的客户端；`Drop` 时取出并归还，正常使用期间始终为 `Some`
    client: Option<tiberius::Client<CompatTcpStream>>,
    /// 池的引用，用于归还连接
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

/// SQL Server 连接工厂
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

/// SQL Server 连接包装器
pub struct MssqlConnection {
    handle: Arc<MssqlPoolHandle>,
    connected: bool,
    in_transaction: bool,
}

impl MssqlConnection {
    /// 标记连接错误（在 guard 释放后调用）
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

    /// 位置式查询（SELECT，无参数）：返回 `(列名, 按列顺序的值矩阵)`
    ///
    /// 绕过 `HashMap<String, Value>` 行映射，列名仅 `to_string` 一次后复用，
    /// 每行值按列序号直接 `Vec::push`，无哈希计算与字符串克隆。
    /// 适用于 SELECT ALL 大结果集场景，相比 [`query`](Connection::query) 可获得 30%~50% 性能提升。
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

    /// 参数绑定位置式查询（SELECT）：叠加 prepared statement + 位置式映射双重优化
    ///
    /// 无参数时回退到 [`query_values`](Connection::query_values)。
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
// 单元测试
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
}
