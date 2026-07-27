//! sqlx 后端适配器实现
//!
//! 为 MySQL、PostgreSQL、SQLite 分别实现 Connection 和 ConnectionFactory。
//! 不使用 sqlx::Any 以避免其类型限制和生命周期问题。
//!
//! 关键设计：
//! Connection trait 已手动解糖（不使用 `#[async_trait]`），所有 async 方法
//! 使用单一生命周期 `'a`（绑定 `&'a mut self` 和 `&'a str`），而非 HRTB。
//! 这样 sqlx::Executor 对 `&'c mut XxxConnection` 的 impl（针对具体 `'c`）
//! 即可满足约束，避免 "implementation of Executor is not general enough" 错误。

use async_trait::async_trait;
use futures::StreamExt;
use sqlx::{Column, Executor, Row, TypeInfo};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;
use sz_orm_core::{ColType, Connection, ConnectionFactory, DbError, QueryRows, QueryValues, Value};

use crate::error::map_sqlx_error;

/// 判断 SQL 是否需要走 raw_sql 路径
/// MySQL prepared statement 协议不支持 BEGIN/COMMIT/ROLLBACK/SAVEPOINT 等命令
fn needs_raw_sql(sql: &str) -> bool {
    let trimmed = sql.trim_start();
    let upper = trimmed.to_uppercase();
    upper.starts_with("BEGIN")
        || upper.starts_with("COMMIT")
        || upper.starts_with("ROLLBACK")
        || upper.starts_with("SAVEPOINT")
        || upper.starts_with("RELEASE")
        || upper.starts_with("SET ")
        || upper.starts_with("USE ")
        || upper.starts_with("START TRANSACTION")
}

// ===================== SQLite 适配器 =====================

// 注：sqlx 0.9 起 Executor trait 要求 'static lifetime，
// 原先的 execute_sqlite_boxed / query_sqlite_boxed 已内联到调用点。
// 见 SqlxSqliteConnection::execute / query 实现。

// 注：原 row_to_value_sqlite（按 type_info().name() 字符串 match）已被
// row_to_value_with_coltype_sqlite（按预解析 ColType 枚举分派）取代，
// 性能更优且避免每行字符串比较。详见该函数。

/// SQLite: 使用预解析的 ColType 进行类型分派（避免每行字符串 match）
///
/// 调用方预先通过 [`ColType::parse_sqlite`] 解析列类型，
/// 后续行直接用枚举分派（编译器优化为跳转表），避免每行每列的字符串比较。
///
/// **注意**：必须使用 `parse_sqlite` 而非通用 `from_type_name`：SQLite 的 INTEGER 类型
/// 实际为 64 位动态存储，通用映射会错误地归类为 I32，导致数值截断。
fn row_to_value_with_coltype_sqlite(
    row: &sqlx::sqlite::SqliteRow,
    ordinal: usize,
    col_type: ColType,
) -> Value {
    match col_type {
        ColType::Bool => match row.try_get::<Option<bool>, usize>(ordinal) {
            Ok(v) => v.map(Value::Bool).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        ColType::I8 => match row.try_get::<Option<i8>, usize>(ordinal) {
            Ok(v) => v.map(Value::I8).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        ColType::I16 => match row.try_get::<Option<i16>, usize>(ordinal) {
            Ok(v) => v.map(Value::I16).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        ColType::I32 => match row.try_get::<Option<i32>, usize>(ordinal) {
            Ok(v) => v.map(Value::I32).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        ColType::I64 => match row.try_get::<Option<i64>, usize>(ordinal) {
            Ok(v) => v.map(Value::I64).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        ColType::U8 => match row.try_get::<Option<u8>, usize>(ordinal) {
            Ok(v) => v.map(Value::U8).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        ColType::U16 => match row.try_get::<Option<u16>, usize>(ordinal) {
            Ok(v) => v.map(Value::U16).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        ColType::U32 => match row.try_get::<Option<u32>, usize>(ordinal) {
            Ok(v) => v.map(Value::U32).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        // SQLite 无原生 u64，按 i64 解码
        ColType::U64 => match row.try_get::<Option<i64>, usize>(ordinal) {
            Ok(v) => v.map(Value::I64).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        ColType::F32 => match row.try_get::<Option<f32>, usize>(ordinal) {
            Ok(v) => v.map(Value::F32).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        ColType::F64 => match row.try_get::<Option<f64>, usize>(ordinal) {
            Ok(v) => v.map(Value::F64).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        ColType::Decimal => match row.try_get::<Option<String>, usize>(ordinal) {
            Ok(v) => v.map(Value::Decimal).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        ColType::String => match row.try_get::<Option<String>, usize>(ordinal) {
            Ok(v) => v.map(Value::String).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        ColType::Bytes => match row.try_get::<Option<Vec<u8>>, usize>(ordinal) {
            Ok(v) => v.map(Value::Bytes).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        // SQLite 中 DATE/DATETIME/TIME/JSON/UUID 通常以 TEXT 存储
        ColType::Date | ColType::DateTime | ColType::Time | ColType::Json | ColType::Uuid => {
            match row.try_get::<Option<String>, usize>(ordinal) {
                Ok(v) => v.map(Value::String).unwrap_or(Value::Null),
                Err(_) => Value::Null,
            }
        }
        ColType::Unknown => {
            // 未知类型，按 bool → i64 → f64 → String 顺序回退
            if let Ok(v) = row.try_get::<Option<bool>, usize>(ordinal) {
                return v.map(Value::Bool).unwrap_or(Value::Null);
            }
            if let Ok(v) = row.try_get::<Option<i64>, usize>(ordinal) {
                return v.map(Value::I64).unwrap_or(Value::Null);
            }
            if let Ok(v) = row.try_get::<Option<f64>, usize>(ordinal) {
                return v.map(Value::F64).unwrap_or(Value::Null);
            }
            if let Ok(v) = row.try_get::<Option<String>, usize>(ordinal) {
                return v.map(Value::String).unwrap_or(Value::Null);
            }
            Value::Null
        }
        // ColType 标记为 #[non_exhaustive]，未来新增变体按 Unknown 处理
        _ => Value::Null,
    }
}

pub struct SqlitePoolHandle {
    pool: sqlx::SqlitePool,
}

impl SqlitePoolHandle {
    pub async fn connect(url: &str) -> Result<Self, DbError> {
        // SQLite 生产配置：WAL + NORMAL 同步 + 5s 忙等 + 256MB mmap
        // - WAL：写不阻塞读，崩溃后通过 WAL 恢复，远比 DELETE 模式安全
        // - Synchronous=Normal：WAL 模式下仅在 checkpoint 时 fsync，性能 ~2x 于 Full
        // - busy_timeout=5s：避免 "database is locked" 误报
        // - mmap_size=256MB：大结果集减少 syscall，提升读取吞吐
        let opts = sqlx::sqlite::SqliteConnectOptions::from_str(url)
            .map_err(map_sqlx_error)?
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
            .busy_timeout(std::time::Duration::from_secs(5))
            .pragma("mmap_size", "268435456");
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(10)
            .acquire_timeout(std::time::Duration::from_secs(30))
            .idle_timeout(Some(std::time::Duration::from_secs(600)))
            .max_lifetime(Some(std::time::Duration::from_secs(1800)))
            .connect_with(opts)
            .await
            .map_err(map_sqlx_error)?;
        Ok(Self { pool })
    }

    pub fn from_pool(pool: sqlx::SqlitePool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &sqlx::SqlitePool {
        &self.pool
    }
}

pub struct SqlxSqliteConnectionFactory {
    pool: Arc<SqlitePoolHandle>,
}

impl SqlxSqliteConnectionFactory {
    pub fn new(pool: Arc<SqlitePoolHandle>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ConnectionFactory for SqlxSqliteConnectionFactory {
    async fn create(&self) -> Result<Box<dyn Connection>, DbError> {
        let conn = self.pool.pool.acquire().await.map_err(map_sqlx_error)?;
        Ok(Box::new(SqlxSqliteConnection {
            conn: Some(conn),
            connected: true,
            in_transaction: false,
        }))
    }
}

pub struct SqlxSqliteConnection {
    conn: Option<sqlx::pool::PoolConnection<sqlx::Sqlite>>,
    connected: bool,
    in_transaction: bool,
}

impl Connection for SqlxSqliteConnection {
    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        Box::pin(async move {
            let mut pool_conn = self
                .conn
                .take()
                .ok_or_else(|| DbError::Internal("connection already closed".to_string()))?;
            // sqlx 0.9: PoolConnection 不再实现 Executor，需通过 DerefMut 解引用到内部连接
            // sqlx 0.9: SqlSafeStr 只对 &'static str 直接实现，非 'static 的 &str 需用 AssertSqlSafe 包装
            let result = if needs_raw_sql(sql) {
                (&mut *pool_conn)
                    .execute(sqlx::raw_sql(sqlx::AssertSqlSafe(sql)))
                    .await
            } else {
                (&mut *pool_conn).execute(sqlx::AssertSqlSafe(sql)).await
            };
            self.conn = Some(pool_conn);

            match result {
                Ok(r) => Ok(r.rows_affected()),
                Err(e) => {
                    let db_err = map_sqlx_error(e);
                    if matches!(db_err, DbError::ConnectionError(_) | DbError::IoError(_)) {
                        self.connected = false;
                    }
                    Err(db_err)
                }
            }
        })
    }

    fn query<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<HashMap<String, Value>>, DbError>> + Send + 'a>>
    {
        Box::pin(async move {
            let mut pool_conn = self
                .conn
                .take()
                .ok_or_else(|| DbError::Internal("connection already closed".to_string()))?;
            // sqlx 0.9: PoolConnection 不再实现 Executor，需通过 DerefMut 解引用到内部连接
            // sqlx 0.9: SqlSafeStr 只对 &'static str 直接实现，非 'static 的 &str 需用 AssertSqlSafe 包装
            let rows_result = (&mut *pool_conn).fetch_all(sqlx::AssertSqlSafe(sql)).await;
            self.conn = Some(pool_conn);

            let rows = rows_result.map_err(map_sqlx_error)?;
            if rows.is_empty() {
                return Ok(Vec::new());
            }
            // 预解析列类型（只在第一行解析，后续行复用，避免每行字符串 match）
            // SQLite 专用解析：INTEGER 映射为 I64（SQLite 整数动态存储，可容纳 64 位）
            let col_types: Vec<ColType> = rows[0]
                .columns()
                .iter()
                .map(|col| ColType::parse_sqlite(col.type_info().name()))
                .collect();
            let mut result = Vec::with_capacity(rows.len());
            for row in &rows {
                let mut record = HashMap::with_capacity(col_types.len());
                for (i, col) in row.columns().iter().enumerate() {
                    let name = col.name().to_string();
                    let value = row_to_value_with_coltype_sqlite(row, i, col_types[i]);
                    record.insert(name, value);
                }
                result.push(record);
            }
            Ok(result)
        })
    }

    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            if self.in_transaction {
                return Err(DbError::Internal("transaction already started".to_string()));
            }
            self.execute("BEGIN").await?;
            self.in_transaction = true;
            Ok(())
        })
    }

    fn commit<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            if self.in_transaction {
                self.execute("COMMIT").await?;
                self.in_transaction = false;
            }
            Ok(())
        })
    }

    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            if self.in_transaction {
                let result = self.execute("ROLLBACK").await;
                self.in_transaction = false;
                result.map(|_| ())
            } else {
                Ok(())
            }
        })
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move {
            match self.execute("SELECT 1").await {
                Ok(_) => true,
                Err(_) => {
                    self.connected = false;
                    false
                }
            }
        })
    }

    fn close<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            if let Some(conn) = self.conn.take() {
                drop(conn);
            }
            self.connected = false;
            self.in_transaction = false;
            Ok(())
        })
    }

    /// SQLite 参数绑定执行（INSERT/UPDATE/DELETE）
    ///
    /// 使用 sqlx prepared statement 绑定参数，避免 SQL 注入与字符串转义开销。
    /// 对 `Value::Bool`/`I8`..=`I64`/`U8`..=`U64`/`F32`/`F64`/`String`/`Bytes` 直接 bind；
    /// 其他类型（Date/DateTime/Json/Array/Object）回退为 `to_string()` 后以 TEXT 绑定。
    fn execute_with_params<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [Value],
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        Box::pin(async move {
            if needs_raw_sql(sql) || params.is_empty() {
                return self.execute(sql).await;
            }
            let mut pool_conn = self
                .conn
                .take()
                .ok_or_else(|| DbError::Internal("connection already closed".to_string()))?;
            let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
            for v in params {
                q = match v {
                    Value::Null => q.bind(None::<i64>),
                    Value::Bool(b) => q.bind(*b),
                    Value::I8(n) => q.bind(*n),
                    Value::I16(n) => q.bind(*n),
                    Value::I32(n) => q.bind(*n),
                    Value::I64(n) => q.bind(*n),
                    Value::U8(n) => q.bind(*n),
                    Value::U16(n) => q.bind(*n),
                    Value::U32(n) => q.bind(*n),
                    Value::U64(n) => q.bind(*n as i64),
                    Value::F32(f) => q.bind(*f),
                    Value::F64(f) => q.bind(*f),
                    Value::String(s) => q.bind(s.as_str()),
                    Value::Decimal(s) => q.bind(s.as_str()),
                    Value::Bytes(b) => q.bind(b.as_slice()),
                    other => q.bind(other.to_string()),
                };
            }
            let result = q.execute(&mut *pool_conn).await;
            self.conn = Some(pool_conn);
            match result {
                Ok(r) => Ok(r.rows_affected()),
                Err(e) => {
                    let db_err = map_sqlx_error(e);
                    if matches!(db_err, DbError::ConnectionError(_) | DbError::IoError(_)) {
                        self.connected = false;
                    }
                    Err(db_err)
                }
            }
        })
    }

    /// SQLite 参数绑定查询（SELECT，HashMap 映射）
    ///
    /// 使用 sqlx prepared statement 绑定参数，结果按预解析 ColType 解码为
    /// `HashMap<String, Value>`。与 `query` 的区别仅在于参数绑定路径。
    fn query_with_params<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [Value],
    ) -> Pin<Box<dyn Future<Output = Result<QueryRows, DbError>> + Send + 'a>> {
        Box::pin(async move {
            if params.is_empty() {
                return self.query(sql).await;
            }
            let mut pool_conn = self
                .conn
                .take()
                .ok_or_else(|| DbError::Internal("connection already closed".to_string()))?;
            let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
            for v in params {
                q = match v {
                    Value::Null => q.bind(None::<i64>),
                    Value::Bool(b) => q.bind(*b),
                    Value::I8(n) => q.bind(*n),
                    Value::I16(n) => q.bind(*n),
                    Value::I32(n) => q.bind(*n),
                    Value::I64(n) => q.bind(*n),
                    Value::U8(n) => q.bind(*n),
                    Value::U16(n) => q.bind(*n),
                    Value::U32(n) => q.bind(*n),
                    Value::U64(n) => q.bind(*n as i64),
                    Value::F32(f) => q.bind(*f),
                    Value::F64(f) => q.bind(*f),
                    Value::String(s) => q.bind(s.as_str()),
                    Value::Decimal(s) => q.bind(s.as_str()),
                    Value::Bytes(b) => q.bind(b.as_slice()),
                    other => q.bind(other.to_string()),
                };
            }
            let rows_result = q.fetch_all(&mut *pool_conn).await;
            self.conn = Some(pool_conn);
            let rows = rows_result.map_err(map_sqlx_error)?;
            if rows.is_empty() {
                return Ok(Vec::new());
            }
            // 预解析列类型（只在第一行解析，后续行复用）
            // SQLite 专用解析：INTEGER → I64
            let col_types: Vec<ColType> = rows[0]
                .columns()
                .iter()
                .map(|col| ColType::parse_sqlite(col.type_info().name()))
                .collect();
            let mut result = Vec::with_capacity(rows.len());
            for row in &rows {
                let mut record = HashMap::with_capacity(col_types.len());
                for (i, col) in row.columns().iter().enumerate() {
                    let name = col.name().to_string();
                    let value = row_to_value_with_coltype_sqlite(row, i, col_types[i]);
                    record.insert(name, value);
                }
                result.push(record);
            }
            Ok(result)
        })
    }

    /// SQLite 位置式查询（SELECT，无参数）
    ///
    /// 绕过 `HashMap<String, Value>` 行映射，返回 `(列名列表, 按列序号的值矩阵)`。
    /// 列名与 ColType 仅 `to_string`/解析一次，后续行复用；每行值按列序号直接 `Vec::push`，
    /// 无哈希计算与字符串克隆。适用于 SELECT ALL 大结果集场景。
    fn query_values<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<QueryValues, DbError>> + Send + 'a>> {
        Box::pin(async move {
            let mut pool_conn = self
                .conn
                .take()
                .ok_or_else(|| DbError::Internal("connection already closed".to_string()))?;
            let rows_result = (&mut *pool_conn).fetch_all(sqlx::AssertSqlSafe(sql)).await;
            self.conn = Some(pool_conn);
            let rows = rows_result.map_err(map_sqlx_error)?;
            if rows.is_empty() {
                return Ok((Vec::new(), Vec::new()));
            }
            let cols = rows[0].columns();
            let mut col_names: Vec<String> = Vec::with_capacity(cols.len());
            let mut col_types: Vec<ColType> = Vec::with_capacity(cols.len());
            for col in cols {
                col_names.push(col.name().to_string());
                col_types.push(ColType::parse_sqlite(col.type_info().name()));
            }
            let mut result_rows: Vec<Vec<Value>> = Vec::with_capacity(rows.len());
            for row in &rows {
                let mut row_values: Vec<Value> = Vec::with_capacity(col_names.len());
                for (idx, _) in col_names.iter().enumerate() {
                    row_values.push(row_to_value_with_coltype_sqlite(row, idx, col_types[idx]));
                }
                result_rows.push(row_values);
            }
            Ok((col_names, result_rows))
        })
    }

    /// SQLite 参数绑定位置式查询（SELECT）
    ///
    /// 叠加 prepared statement + 位置式映射 + ColType 预解析三重优化。
    fn query_values_with_params<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [Value],
    ) -> Pin<Box<dyn Future<Output = Result<QueryValues, DbError>> + Send + 'a>> {
        Box::pin(async move {
            if params.is_empty() {
                return self.query_values(sql).await;
            }
            let mut pool_conn = self
                .conn
                .take()
                .ok_or_else(|| DbError::Internal("connection already closed".to_string()))?;
            let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
            for v in params {
                q = match v {
                    Value::Null => q.bind(None::<i64>),
                    Value::Bool(b) => q.bind(*b),
                    Value::I8(n) => q.bind(*n),
                    Value::I16(n) => q.bind(*n),
                    Value::I32(n) => q.bind(*n),
                    Value::I64(n) => q.bind(*n),
                    Value::U8(n) => q.bind(*n),
                    Value::U16(n) => q.bind(*n),
                    Value::U32(n) => q.bind(*n),
                    Value::U64(n) => q.bind(*n as i64),
                    Value::F32(f) => q.bind(*f),
                    Value::F64(f) => q.bind(*f),
                    Value::String(s) => q.bind(s.as_str()),
                    Value::Decimal(s) => q.bind(s.as_str()),
                    Value::Bytes(b) => q.bind(b.as_slice()),
                    other => q.bind(other.to_string()),
                };
            }
            let rows_result = q.fetch_all(&mut *pool_conn).await;
            self.conn = Some(pool_conn);
            let rows = rows_result.map_err(map_sqlx_error)?;
            if rows.is_empty() {
                return Ok((Vec::new(), Vec::new()));
            }
            let cols = rows[0].columns();
            let mut col_names: Vec<String> = Vec::with_capacity(cols.len());
            let mut col_types: Vec<ColType> = Vec::with_capacity(cols.len());
            for col in cols {
                col_names.push(col.name().to_string());
                col_types.push(ColType::parse_sqlite(col.type_info().name()));
            }
            let mut result_rows: Vec<Vec<Value>> = Vec::with_capacity(rows.len());
            for row in &rows {
                let mut row_values: Vec<Value> = Vec::with_capacity(col_names.len());
                for (idx, _) in col_names.iter().enumerate() {
                    row_values.push(row_to_value_with_coltype_sqlite(row, idx, col_types[idx]));
                }
                result_rows.push(row_values);
            }
            Ok((col_names, result_rows))
        })
    }

    /// SQLite 流式查询：逐行返回结果，避免大结果集 `fetch_all` 内存峰值
    ///
    /// 使用 `sqlx::query::fetch` 获取行流，逐行映射为 `HashMap<String, Value>`。
    /// 流正常消费完毕后连接归还 `self.conn`；若提前 drop 流，连接通过
    /// `PoolConnection::drop` 归还到 sqlx 池，但 `self.conn` 变为 `None`。
    fn query_stream<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn futures::Stream<Item = Result<HashMap<String, Value>, DbError>> + Send + 'a>>
    {
        Box::pin(async_stream::try_stream! {
            let mut pool_conn = self
                .conn
                .take()
                .ok_or_else(|| DbError::Internal("connection already closed".to_string()))?;
            let mut row_stream = sqlx::query(sqlx::AssertSqlSafe(sql)).fetch(&mut *pool_conn);
            let mut col_types: Vec<ColType> = Vec::new();
            let mut col_names: Vec<String> = Vec::new();
            let mut first_row = true;
            while let Some(row_result) = row_stream.next().await {
                let row = row_result.map_err(map_sqlx_error)?;
                if first_row {
                    for col in row.columns() {
                        col_names.push(col.name().to_string());
                        col_types.push(ColType::parse_sqlite(col.type_info().name()));
                    }
                    first_row = false;
                }
                let mut record = HashMap::with_capacity(col_names.len());
                for (i, name) in col_names.iter().enumerate() {
                    let value = row_to_value_with_coltype_sqlite(&row, i, col_types[i]);
                    record.insert(name.clone(), value);
                }
                yield record;
            }
            // 显式释放 row_stream 对 pool_conn 的可变借用，否则借用检查器
            // 无法证明 row_stream 在移动 pool_conn 前已被销毁（E0505）
            drop(row_stream);
            self.conn = Some(pool_conn);
        })
    }
}

impl Drop for SqlxSqliteConnection {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            drop(conn);
        }
    }
}

/// SQLite 在线备份（使用 VACUUM INTO，要求 SQLite 3.27+）
///
/// 将当前数据库完整复制到目标路径，原数据库可继续读写（在线备份）。
/// 目标文件若已存在会报错。路径中的单引号会被转义以防 SQL 注入。
pub async fn sqlite_backup(
    conn: &mut SqlxSqliteConnection,
    dest_path: &str,
) -> Result<(), DbError> {
    let escaped_path = dest_path.replace('\'', "''");
    let sql = format!("VACUUM INTO '{}'", escaped_path);
    conn.execute(&sql).await?;
    Ok(())
}

// ===================== MySQL 适配器 =====================

// 注：sqlx 0.9 起 Executor trait 要求 'static lifetime，
// 原先的 execute_mysql_boxed / query_mysql_boxed 已内联到调用点。

fn row_to_value_mysql(row: &sqlx::mysql::MySqlRow, ordinal: usize) -> Value {
    use sqlx::TypeInfo;
    let type_name = row.columns()[ordinal].type_info().name();
    match type_name {
        "BOOLEAN" => match row.try_get::<Option<bool>, usize>(ordinal) {
            Ok(v) => v.map(Value::Bool).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        "TINYINT" | "TINYINT UNSIGNED" => match row.try_get::<Option<i8>, usize>(ordinal) {
            Ok(v) => v.map(Value::I8).unwrap_or(Value::Null),
            Err(_) => match row.try_get::<Option<u8>, usize>(ordinal) {
                Ok(v) => v.map(Value::U8).unwrap_or(Value::Null),
                Err(_) => Value::Null,
            },
        },
        "SMALLINT" | "SMALLINT UNSIGNED" => match row.try_get::<Option<i16>, usize>(ordinal) {
            Ok(v) => v.map(Value::I16).unwrap_or(Value::Null),
            Err(_) => match row.try_get::<Option<u16>, usize>(ordinal) {
                Ok(v) => v.map(Value::U16).unwrap_or(Value::Null),
                Err(_) => Value::Null,
            },
        },
        "INT" | "INT UNSIGNED" | "MEDIUMINT" | "MEDIUMINT UNSIGNED" => {
            match row.try_get::<Option<i32>, usize>(ordinal) {
                Ok(v) => v.map(Value::I32).unwrap_or(Value::Null),
                Err(_) => match row.try_get::<Option<u32>, usize>(ordinal) {
                    Ok(v) => v.map(Value::U32).unwrap_or(Value::Null),
                    Err(_) => Value::Null,
                },
            }
        }
        "BIGINT" | "BIGINT UNSIGNED" => match row.try_get::<Option<i64>, usize>(ordinal) {
            Ok(v) => v.map(Value::I64).unwrap_or(Value::Null),
            Err(_) => match row.try_get::<Option<u64>, usize>(ordinal) {
                Ok(v) => v.map(Value::U64).unwrap_or(Value::Null),
                Err(_) => Value::Null,
            },
        },
        "FLOAT" => match row.try_get::<Option<f32>, usize>(ordinal) {
            Ok(v) => v.map(Value::F32).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        "DOUBLE" => match row.try_get::<Option<f64>, usize>(ordinal) {
            Ok(v) => v.map(Value::F64).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        "VARCHAR" | "TEXT" | "CHAR" | "TINYTEXT" | "MEDIUMTEXT" | "LONGTEXT" | "ENUM" => {
            match row.try_get::<Option<String>, usize>(ordinal) {
                Ok(v) => v.map(Value::String).unwrap_or(Value::Null),
                Err(_) => Value::Null,
            }
        }
        "BLOB" | "TINYBLOB" | "MEDIUMBLOB" | "LONGBLOB" | "BINARY" | "VARBINARY" => {
            match row.try_get::<Option<Vec<u8>>, usize>(ordinal) {
                Ok(v) => v.map(Value::Bytes).unwrap_or(Value::Null),
                Err(_) => Value::Null,
            }
        }
        // DECIMAL/NUMERIC 使用 rust_decimal 解码，以字符串形式保留精度
        "DECIMAL" | "NUMERIC" | "NEWDECIMAL" => {
            match row.try_get::<Option<rust_decimal::Decimal>, usize>(ordinal) {
                Ok(Some(v)) => Value::Decimal(v.to_string()),
                Ok(None) => Value::Null,
                Err(_) => match row.try_get::<Option<String>, usize>(ordinal) {
                    Ok(v) => v.map(Value::String).unwrap_or(Value::Null),
                    Err(_) => Value::Null,
                },
            }
        }
        _ => {
            // 未知类型回退：i64 → f64 → bool → String
            if let Ok(v) = row.try_get::<Option<i64>, usize>(ordinal) {
                return v.map(Value::I64).unwrap_or(Value::Null);
            }
            if let Ok(v) = row.try_get::<Option<f64>, usize>(ordinal) {
                return v.map(Value::F64).unwrap_or(Value::Null);
            }
            if let Ok(v) = row.try_get::<Option<bool>, usize>(ordinal) {
                return v.map(Value::Bool).unwrap_or(Value::Null);
            }
            if let Ok(v) = row.try_get::<Option<String>, usize>(ordinal) {
                return v.map(Value::String).unwrap_or(Value::Null);
            }
            Value::Null
        }
    }
}

/// MySQL: 使用预解析的 ColType 进行类型分派（避免每行字符串 match）
///
/// 与 [`row_to_value_mysql`] 的区别：调用方预先通过 [`ColType::parse_mysql`] 解析列类型，
/// 后续行直接用枚举分派（编译器优化为跳转表），避免每行每列的字符串比较。
///
/// **注意**：必须使用 `parse_mysql` 而非通用 `from_type_name`：MySQL 协议报告的类型名
/// 包含 NEWDECIMAL/YEAR/ENUM/SET 等特有类型，通用映射无法识别。
fn row_to_value_with_coltype_mysql(
    row: &sqlx::mysql::MySqlRow,
    ordinal: usize,
    col_type: ColType,
) -> Value {
    match col_type {
        ColType::Bool => match row.try_get::<Option<bool>, usize>(ordinal) {
            Ok(v) => v.map(Value::Bool).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        ColType::I8 => match row.try_get::<Option<i8>, usize>(ordinal) {
            Ok(v) => v.map(Value::I8).unwrap_or(Value::Null),
            Err(_) => match row.try_get::<Option<u8>, usize>(ordinal) {
                Ok(v) => v.map(Value::U8).unwrap_or(Value::Null),
                Err(_) => Value::Null,
            },
        },
        ColType::I16 => match row.try_get::<Option<i16>, usize>(ordinal) {
            Ok(v) => v.map(Value::I16).unwrap_or(Value::Null),
            Err(_) => match row.try_get::<Option<u16>, usize>(ordinal) {
                Ok(v) => v.map(Value::U16).unwrap_or(Value::Null),
                Err(_) => Value::Null,
            },
        },
        ColType::I32 => match row.try_get::<Option<i32>, usize>(ordinal) {
            Ok(v) => v.map(Value::I32).unwrap_or(Value::Null),
            Err(_) => match row.try_get::<Option<u32>, usize>(ordinal) {
                Ok(v) => v.map(Value::U32).unwrap_or(Value::Null),
                Err(_) => Value::Null,
            },
        },
        ColType::I64 => match row.try_get::<Option<i64>, usize>(ordinal) {
            Ok(v) => v.map(Value::I64).unwrap_or(Value::Null),
            Err(_) => match row.try_get::<Option<u64>, usize>(ordinal) {
                Ok(v) => v.map(Value::U64).unwrap_or(Value::Null),
                Err(_) => Value::Null,
            },
        },
        ColType::U8 => match row.try_get::<Option<u8>, usize>(ordinal) {
            Ok(v) => v.map(Value::U8).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        ColType::U16 => match row.try_get::<Option<u16>, usize>(ordinal) {
            Ok(v) => v.map(Value::U16).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        ColType::U32 => match row.try_get::<Option<u32>, usize>(ordinal) {
            Ok(v) => v.map(Value::U32).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        ColType::U64 => match row.try_get::<Option<u64>, usize>(ordinal) {
            Ok(v) => v.map(Value::U64).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        ColType::F32 => match row.try_get::<Option<f32>, usize>(ordinal) {
            Ok(v) => v.map(Value::F32).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        ColType::F64 => match row.try_get::<Option<f64>, usize>(ordinal) {
            Ok(v) => v.map(Value::F64).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        // DECIMAL/NUMERIC 使用 rust_decimal 解码，以字符串形式保留精度
        ColType::Decimal => match row.try_get::<Option<rust_decimal::Decimal>, usize>(ordinal) {
            Ok(Some(v)) => Value::Decimal(v.to_string()),
            Ok(None) => Value::Null,
            Err(_) => match row.try_get::<Option<String>, usize>(ordinal) {
                Ok(v) => v.map(Value::String).unwrap_or(Value::Null),
                Err(_) => Value::Null,
            },
        },
        ColType::String => match row.try_get::<Option<String>, usize>(ordinal) {
            Ok(v) => v.map(Value::String).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        ColType::Bytes => match row.try_get::<Option<Vec<u8>>, usize>(ordinal) {
            Ok(v) => v.map(Value::Bytes).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        // DATE/DATETIME/TIME/JSON/UUID 在 MySQL 中通常以字符串解码
        ColType::Date | ColType::DateTime | ColType::Time | ColType::Json | ColType::Uuid => {
            match row.try_get::<Option<String>, usize>(ordinal) {
                Ok(v) => v.map(Value::String).unwrap_or(Value::Null),
                Err(_) => Value::Null,
            }
        }
        ColType::Unknown => {
            // 未知类型回退：i64 → f64 → bool → String
            if let Ok(v) = row.try_get::<Option<i64>, usize>(ordinal) {
                return v.map(Value::I64).unwrap_or(Value::Null);
            }
            if let Ok(v) = row.try_get::<Option<f64>, usize>(ordinal) {
                return v.map(Value::F64).unwrap_or(Value::Null);
            }
            if let Ok(v) = row.try_get::<Option<bool>, usize>(ordinal) {
                return v.map(Value::Bool).unwrap_or(Value::Null);
            }
            if let Ok(v) = row.try_get::<Option<String>, usize>(ordinal) {
                return v.map(Value::String).unwrap_or(Value::Null);
            }
            Value::Null
        }
        // ColType 标记为 #[non_exhaustive]，未来新增变体按 Unknown 处理
        _ => Value::Null,
    }
}

pub struct MySqlPoolHandle {
    pool: sqlx::MySqlPool,
}

impl MySqlPoolHandle {
    pub async fn connect(url: &str) -> Result<Self, DbError> {
        let pool = sqlx::pool::PoolOptions::<sqlx::MySql>::new()
            .max_connections(10)
            .acquire_timeout(std::time::Duration::from_secs(30))
            .idle_timeout(Some(std::time::Duration::from_secs(600)))
            .max_lifetime(Some(std::time::Duration::from_secs(1800)))
            .connect(url)
            .await
            .map_err(map_sqlx_error)?;
        Ok(Self { pool })
    }

    pub fn from_pool(pool: sqlx::MySqlPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &sqlx::MySqlPool {
        &self.pool
    }
}

pub struct SqlxMySqlConnectionFactory {
    pool: Arc<MySqlPoolHandle>,
}

impl SqlxMySqlConnectionFactory {
    pub fn new(pool: Arc<MySqlPoolHandle>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ConnectionFactory for SqlxMySqlConnectionFactory {
    async fn create(&self) -> Result<Box<dyn Connection>, DbError> {
        let conn = self.pool.pool.acquire().await.map_err(map_sqlx_error)?;
        Ok(Box::new(SqlxMySqlConnection {
            conn: Some(conn),
            connected: true,
            in_transaction: false,
        }))
    }
}

pub struct SqlxMySqlConnection {
    conn: Option<sqlx::pool::PoolConnection<sqlx::MySql>>,
    connected: bool,
    in_transaction: bool,
}

impl Connection for SqlxMySqlConnection {
    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        Box::pin(async move {
            let mut pool_conn = self
                .conn
                .take()
                .ok_or_else(|| DbError::Internal("connection already closed".to_string()))?;
            // sqlx 0.9: PoolConnection 不再实现 Executor，需通过 DerefMut 解引用到内部连接
            // sqlx 0.9: SqlSafeStr 只对 &'static str 直接实现，非 'static 的 &str 需用 AssertSqlSafe 包装
            let result = if needs_raw_sql(sql) {
                (&mut *pool_conn)
                    .execute(sqlx::raw_sql(sqlx::AssertSqlSafe(sql)))
                    .await
            } else {
                (&mut *pool_conn).execute(sqlx::AssertSqlSafe(sql)).await
            };
            self.conn = Some(pool_conn);

            match result {
                Ok(r) => Ok(r.rows_affected()),
                Err(e) => {
                    let db_err = map_sqlx_error(e);
                    if matches!(db_err, DbError::ConnectionError(_) | DbError::IoError(_)) {
                        self.connected = false;
                    }
                    Err(db_err)
                }
            }
        })
    }

    fn query<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<HashMap<String, Value>>, DbError>> + Send + 'a>>
    {
        Box::pin(async move {
            let mut pool_conn = self
                .conn
                .take()
                .ok_or_else(|| DbError::Internal("connection already closed".to_string()))?;
            // sqlx 0.9: PoolConnection 不再实现 Executor，需通过 DerefMut 解引用到内部连接
            // sqlx 0.9: SqlSafeStr 只对 &'static str 直接实现，非 'static 的 &str 需用 AssertSqlSafe 包装
            let rows_result = (&mut *pool_conn).fetch_all(sqlx::AssertSqlSafe(sql)).await;
            self.conn = Some(pool_conn);

            let rows = rows_result.map_err(map_sqlx_error)?;
            if rows.is_empty() {
                return Ok(Vec::new());
            }
            // 预解析列类型（只在第一行解析，后续行复用，避免每行字符串 match）
            // MySQL 专用解析：覆盖 MySQL 协议特有类型名（NEWDECIMAL/YEAR/ENUM/SET 等）
            let col_types: Vec<ColType> = rows[0]
                .columns()
                .iter()
                .map(|col| ColType::parse_mysql(col.type_info().name()))
                .collect();
            let mut result = Vec::with_capacity(rows.len());
            for row in &rows {
                let mut record = HashMap::with_capacity(col_types.len());
                for (i, col) in row.columns().iter().enumerate() {
                    let name = col.name().to_string();
                    let value = row_to_value_with_coltype_mysql(row, i, col_types[i]);
                    record.insert(name, value);
                }
                result.push(record);
            }
            Ok(result)
        })
    }

    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            if self.in_transaction {
                return Err(DbError::Internal("transaction already started".to_string()));
            }
            self.execute("BEGIN").await?;
            self.in_transaction = true;
            Ok(())
        })
    }

    fn commit<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            if self.in_transaction {
                self.execute("COMMIT").await?;
                self.in_transaction = false;
            }
            Ok(())
        })
    }

    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            if self.in_transaction {
                let result = self.execute("ROLLBACK").await;
                self.in_transaction = false;
                result.map(|_| ())
            } else {
                Ok(())
            }
        })
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move {
            match self.execute("SELECT 1").await {
                Ok(_) => true,
                Err(_) => {
                    self.connected = false;
                    false
                }
            }
        })
    }

    fn close<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            if let Some(conn) = self.conn.take() {
                drop(conn);
            }
            self.connected = false;
            self.in_transaction = false;
            Ok(())
        })
    }

    /// MySQL 参数绑定执行（INSERT/UPDATE/DELETE）
    ///
    /// 使用 sqlx prepared statement 绑定参数。MySQL 协议原生支持 `?` 占位符。
    /// 对 `Value::Bool`/`I8`..=`I64`/`U8`..=`U64`/`F32`/`F64`/`String`/`Bytes` 直接 bind；
    /// 其他类型回退为 `to_string()` 后以 TEXT 绑定。
    fn execute_with_params<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [Value],
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        Box::pin(async move {
            if needs_raw_sql(sql) || params.is_empty() {
                return self.execute(sql).await;
            }
            let mut pool_conn = self
                .conn
                .take()
                .ok_or_else(|| DbError::Internal("connection already closed".to_string()))?;
            let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
            for v in params {
                q = match v {
                    Value::Null => q.bind(None::<i64>),
                    Value::Bool(b) => q.bind(*b),
                    Value::I8(n) => q.bind(*n),
                    Value::I16(n) => q.bind(*n),
                    Value::I32(n) => q.bind(*n),
                    Value::I64(n) => q.bind(*n),
                    Value::U8(n) => q.bind(*n),
                    Value::U16(n) => q.bind(*n),
                    Value::U32(n) => q.bind(*n),
                    Value::U64(n) => q.bind(*n as i64),
                    Value::F32(f) => q.bind(*f),
                    Value::F64(f) => q.bind(*f),
                    Value::String(s) => q.bind(s.as_str()),
                    Value::Decimal(s) => q.bind(s.as_str()),
                    Value::Bytes(b) => q.bind(b.as_slice()),
                    other => q.bind(other.to_string()),
                };
            }
            let result = q.execute(&mut *pool_conn).await;
            self.conn = Some(pool_conn);
            match result {
                Ok(r) => Ok(r.rows_affected()),
                Err(e) => {
                    let db_err = map_sqlx_error(e);
                    if matches!(db_err, DbError::ConnectionError(_) | DbError::IoError(_)) {
                        self.connected = false;
                    }
                    Err(db_err)
                }
            }
        })
    }

    /// MySQL 参数绑定查询（SELECT，HashMap 映射）
    fn query_with_params<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [Value],
    ) -> Pin<Box<dyn Future<Output = Result<QueryRows, DbError>> + Send + 'a>> {
        Box::pin(async move {
            if params.is_empty() {
                return self.query(sql).await;
            }
            let mut pool_conn = self
                .conn
                .take()
                .ok_or_else(|| DbError::Internal("connection already closed".to_string()))?;
            let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
            for v in params {
                q = match v {
                    Value::Null => q.bind(None::<i64>),
                    Value::Bool(b) => q.bind(*b),
                    Value::I8(n) => q.bind(*n),
                    Value::I16(n) => q.bind(*n),
                    Value::I32(n) => q.bind(*n),
                    Value::I64(n) => q.bind(*n),
                    Value::U8(n) => q.bind(*n),
                    Value::U16(n) => q.bind(*n),
                    Value::U32(n) => q.bind(*n),
                    Value::U64(n) => q.bind(*n as i64),
                    Value::F32(f) => q.bind(*f),
                    Value::F64(f) => q.bind(*f),
                    Value::String(s) => q.bind(s.as_str()),
                    Value::Decimal(s) => q.bind(s.as_str()),
                    Value::Bytes(b) => q.bind(b.as_slice()),
                    other => q.bind(other.to_string()),
                };
            }
            let rows_result = q.fetch_all(&mut *pool_conn).await;
            self.conn = Some(pool_conn);
            let rows = rows_result.map_err(map_sqlx_error)?;
            let mut result = Vec::with_capacity(rows.len());
            for row in rows {
                // #13 修复：预分配 HashMap 容量，避免逐列 insert 时 rehash/growth
                let columns = row.columns();
                let mut record = HashMap::with_capacity(columns.len());
                for col in columns {
                    let name = col.name().to_string();
                    let ordinal = col.ordinal();
                    let value = row_to_value_mysql(&row, ordinal);
                    record.insert(name, value);
                }
                result.push(record);
            }
            Ok(result)
        })
    }

    /// MySQL 位置式查询（SELECT，无参数）
    ///
    /// 绕过 HashMap 行映射，返回 `(列名, 按列序号的值矩阵)`。适用于 SELECT ALL 大结果集。
    fn query_values<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<QueryValues, DbError>> + Send + 'a>> {
        Box::pin(async move {
            let mut pool_conn = self
                .conn
                .take()
                .ok_or_else(|| DbError::Internal("connection already closed".to_string()))?;
            let rows_result = (&mut *pool_conn).fetch_all(sqlx::AssertSqlSafe(sql)).await;
            self.conn = Some(pool_conn);
            let rows = rows_result.map_err(map_sqlx_error)?;
            if rows.is_empty() {
                return Ok((Vec::new(), Vec::new()));
            }
            let cols = rows[0].columns();
            let mut col_names: Vec<String> = Vec::with_capacity(cols.len());
            let mut col_types: Vec<ColType> = Vec::with_capacity(cols.len());
            for col in cols {
                col_names.push(col.name().to_string());
                col_types.push(ColType::parse_mysql(col.type_info().name()));
            }
            let mut result_rows: Vec<Vec<Value>> = Vec::with_capacity(rows.len());
            for row in &rows {
                let mut row_values: Vec<Value> = Vec::with_capacity(col_names.len());
                for (idx, _) in col_names.iter().enumerate() {
                    row_values.push(row_to_value_with_coltype_mysql(row, idx, col_types[idx]));
                }
                result_rows.push(row_values);
            }
            Ok((col_names, result_rows))
        })
    }

    /// MySQL 参数绑定位置式查询（SELECT）
    fn query_values_with_params<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [Value],
    ) -> Pin<Box<dyn Future<Output = Result<QueryValues, DbError>> + Send + 'a>> {
        Box::pin(async move {
            if params.is_empty() {
                return self.query_values(sql).await;
            }
            let mut pool_conn = self
                .conn
                .take()
                .ok_or_else(|| DbError::Internal("connection already closed".to_string()))?;
            let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
            for v in params {
                q = match v {
                    Value::Null => q.bind(None::<i64>),
                    Value::Bool(b) => q.bind(*b),
                    Value::I8(n) => q.bind(*n),
                    Value::I16(n) => q.bind(*n),
                    Value::I32(n) => q.bind(*n),
                    Value::I64(n) => q.bind(*n),
                    Value::U8(n) => q.bind(*n),
                    Value::U16(n) => q.bind(*n),
                    Value::U32(n) => q.bind(*n),
                    Value::U64(n) => q.bind(*n as i64),
                    Value::F32(f) => q.bind(*f),
                    Value::F64(f) => q.bind(*f),
                    Value::String(s) => q.bind(s.as_str()),
                    Value::Decimal(s) => q.bind(s.as_str()),
                    Value::Bytes(b) => q.bind(b.as_slice()),
                    other => q.bind(other.to_string()),
                };
            }
            let rows_result = q.fetch_all(&mut *pool_conn).await;
            self.conn = Some(pool_conn);
            let rows = rows_result.map_err(map_sqlx_error)?;
            if rows.is_empty() {
                return Ok((Vec::new(), Vec::new()));
            }
            let cols = rows[0].columns();
            let mut col_names: Vec<String> = Vec::with_capacity(cols.len());
            let mut col_types: Vec<ColType> = Vec::with_capacity(cols.len());
            for col in cols {
                col_names.push(col.name().to_string());
                col_types.push(ColType::parse_mysql(col.type_info().name()));
            }
            let mut result_rows: Vec<Vec<Value>> = Vec::with_capacity(rows.len());
            for row in &rows {
                let mut row_values: Vec<Value> = Vec::with_capacity(col_names.len());
                for (idx, _) in col_names.iter().enumerate() {
                    row_values.push(row_to_value_with_coltype_mysql(row, idx, col_types[idx]));
                }
                result_rows.push(row_values);
            }
            Ok((col_names, result_rows))
        })
    }

    /// MySQL 流式查询：逐行返回结果，避免大结果集 `fetch_all` 内存峰值
    ///
    /// 使用 `sqlx::query::fetch` 获取行流，逐行映射为 `HashMap<String, Value>`。
    /// 流正常消费完毕后连接归还 `self.conn`；若提前 drop 流，连接通过
    /// `PoolConnection::drop` 归还到 sqlx 池，但 `self.conn` 变为 `None`。
    fn query_stream<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn futures::Stream<Item = Result<HashMap<String, Value>, DbError>> + Send + 'a>>
    {
        Box::pin(async_stream::try_stream! {
            let mut pool_conn = self
                .conn
                .take()
                .ok_or_else(|| DbError::Internal("connection already closed".to_string()))?;
            let mut row_stream = sqlx::query(sqlx::AssertSqlSafe(sql)).fetch(&mut *pool_conn);
            let mut col_types: Vec<ColType> = Vec::new();
            let mut col_names: Vec<String> = Vec::new();
            let mut first_row = true;
            while let Some(row_result) = row_stream.next().await {
                let row = row_result.map_err(map_sqlx_error)?;
                if first_row {
                    for col in row.columns() {
                        col_names.push(col.name().to_string());
                        col_types.push(ColType::parse_mysql(col.type_info().name()));
                    }
                    first_row = false;
                }
                let mut record = HashMap::with_capacity(col_names.len());
                for (i, name) in col_names.iter().enumerate() {
                    let value = row_to_value_with_coltype_mysql(&row, i, col_types[i]);
                    record.insert(name.clone(), value);
                }
                yield record;
            }
            // 显式释放 row_stream 对 pool_conn 的可变借用，否则借用检查器
            // 无法证明 row_stream 在移动 pool_conn 前已被销毁（E0505）
            drop(row_stream);
            self.conn = Some(pool_conn);
        })
    }
}

impl Drop for SqlxMySqlConnection {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            drop(conn);
        }
    }
}

/// MySQL 批量导入（多行 INSERT，作为 LOAD DATA LOCAL INFILE 的安全替代）
///
/// 构建多行 INSERT 语句：`INSERT INTO t (c1, c2) VALUES (?, ?), (?, ?), ...`
/// 使用参数绑定避免 SQL 注入。MySQL 原生支持 `?` 占位符。
/// 当数据量极大时建议分批调用（每批 1000~10000 行），避免 SQL 过长或超出
/// `max_allowed_packet` 限制。
pub async fn mysql_bulk_insert(
    conn: &mut SqlxMySqlConnection,
    table: &str,
    columns: &[&str],
    rows: &[Vec<Value>],
) -> Result<u64, DbError> {
    if rows.is_empty() {
        return Ok(0);
    }
    let col_list = columns.join(", ");
    let cols_per_row = columns.len();
    // MySQL 占位符 ? 每列一个，跨行复用相同模式
    let row_placeholder = format!("({})", vec!["?"; cols_per_row].join(", "));
    let placeholders = vec![row_placeholder; rows.len()].join(", ");
    let sql = format!(
        "INSERT INTO {} ({}) VALUES {}",
        table, col_list, placeholders
    );
    // 展平所有行的值为单一参数数组
    let mut params: Vec<Value> = Vec::with_capacity(rows.len() * cols_per_row);
    for row in rows {
        for v in row {
            params.push(v.clone());
        }
    }
    conn.execute_with_params(&sql, &params).await
}

// ===================== PostgreSQL 适配器 =====================

// 注：sqlx 0.9 起 Executor trait 要求 'static lifetime，
// 原先的 execute_pg_boxed / query_pg_boxed 已内联到调用点。

fn row_to_value_pg(row: &sqlx::postgres::PgRow, ordinal: usize) -> Value {
    use sqlx::TypeInfo;
    let type_name = row.columns()[ordinal].type_info().name();
    match type_name {
        "BOOL" => match row.try_get::<Option<bool>, usize>(ordinal) {
            Ok(v) => v.map(Value::Bool).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        "INT2" => match row.try_get::<Option<i16>, usize>(ordinal) {
            Ok(v) => v.map(Value::I16).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        "INT4" | "OID" => match row.try_get::<Option<i32>, usize>(ordinal) {
            Ok(v) => v.map(Value::I32).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        "INT8" => match row.try_get::<Option<i64>, usize>(ordinal) {
            Ok(v) => v.map(Value::I64).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        "FLOAT4" => match row.try_get::<Option<f32>, usize>(ordinal) {
            Ok(v) => v.map(Value::F32).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        "FLOAT8" => match row.try_get::<Option<f64>, usize>(ordinal) {
            Ok(v) => v.map(Value::F64).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        "TEXT" | "VARCHAR" | "CHAR" | "NAME" => match row.try_get::<Option<String>, usize>(ordinal)
        {
            Ok(v) => v.map(Value::String).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        "BYTEA" => match row.try_get::<Option<Vec<u8>>, usize>(ordinal) {
            Ok(v) => v.map(Value::Bytes).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        "NUMERIC" => match row.try_get::<Option<rust_decimal::Decimal>, usize>(ordinal) {
            Ok(Some(v)) => Value::Decimal(v.to_string()),
            Ok(None) => Value::Null,
            Err(_) => match row.try_get::<Option<String>, usize>(ordinal) {
                Ok(v) => v.map(Value::String).unwrap_or(Value::Null),
                Err(_) => Value::Null,
            },
        },
        // UUID：使用 sqlx::types::Uuid（16 字节）解码，避免 36 字符字符串的内存浪费
        "UUID" => match row.try_get::<Option<sqlx::types::Uuid>, usize>(ordinal) {
            Ok(v) => v
                .map(|uuid| Value::String(uuid.to_string()))
                .unwrap_or(Value::Null),
            Err(_) => match row.try_get::<Option<String>, usize>(ordinal) {
                Ok(v) => v.map(Value::String).unwrap_or(Value::Null),
                Err(_) => Value::Null,
            },
        },
        _ => {
            // 未知类型回退
            if let Ok(v) = row.try_get::<Option<i64>, usize>(ordinal) {
                return v.map(Value::I64).unwrap_or(Value::Null);
            }
            if let Ok(v) = row.try_get::<Option<f64>, usize>(ordinal) {
                return v.map(Value::F64).unwrap_or(Value::Null);
            }
            if let Ok(v) = row.try_get::<Option<bool>, usize>(ordinal) {
                return v.map(Value::Bool).unwrap_or(Value::Null);
            }
            if let Ok(v) = row.try_get::<Option<String>, usize>(ordinal) {
                return v.map(Value::String).unwrap_or(Value::Null);
            }
            Value::Null
        }
    }
}

/// PostgreSQL: 使用预解析的 ColType 进行类型分派（避免每行字符串 match）
///
/// 与 [`row_to_value_pg`] 的区别：调用方预先通过 [`ColType::parse_postgres`] 解析列类型，
/// 后续行直接用枚举分派（编译器优化为跳转表），避免每行每列的字符串比较。
///
/// **注意**：必须使用 `parse_postgres` 而非通用 `from_type_name`：PostgreSQL 使用
/// PG 内部类型名（INT4/INT8/FLOAT8/BPCHAR/JSONB/TIMESTAMPTZ 等），通用映射无法识别。
fn row_to_value_with_coltype_pg(
    row: &sqlx::postgres::PgRow,
    ordinal: usize,
    col_type: ColType,
) -> Value {
    match col_type {
        ColType::Bool => match row.try_get::<Option<bool>, usize>(ordinal) {
            Ok(v) => v.map(Value::Bool).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        ColType::I8 => match row.try_get::<Option<i8>, usize>(ordinal) {
            Ok(v) => v.map(Value::I8).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        ColType::I16 => match row.try_get::<Option<i16>, usize>(ordinal) {
            Ok(v) => v.map(Value::I16).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        // PostgreSQL OID 在 parse_postgres 中归为 I32
        ColType::I32 => match row.try_get::<Option<i32>, usize>(ordinal) {
            Ok(v) => v.map(Value::I32).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        ColType::I64 => match row.try_get::<Option<i64>, usize>(ordinal) {
            Ok(v) => v.map(Value::I64).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        // PostgreSQL 无原生无符号类型（u8/u16/u32）：
        // - ColType::U8/U16/U32 仅在 parse_postgres 误判时出现（PG 无 UNSIGNED 关键字）
        // - 退化为最小可容纳的有符号类型：U8→i16、U16→i32、U32→i64
        // - 数值正确性保持，仅在 Value 枚举上为有符号变体（与 sqlx Type<Postgres> 实现一致）
        ColType::U8 => match row.try_get::<Option<i16>, usize>(ordinal) {
            Ok(v) => v.map(Value::I16).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        ColType::U16 => match row.try_get::<Option<i32>, usize>(ordinal) {
            Ok(v) => v.map(Value::I32).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        ColType::U32 => match row.try_get::<Option<i64>, usize>(ordinal) {
            Ok(v) => v.map(Value::I64).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        // PostgreSQL 无原生 U64，按 i64 解码
        ColType::U64 => match row.try_get::<Option<i64>, usize>(ordinal) {
            Ok(v) => v.map(Value::I64).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        ColType::F32 => match row.try_get::<Option<f32>, usize>(ordinal) {
            Ok(v) => v.map(Value::F32).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        ColType::F64 => match row.try_get::<Option<f64>, usize>(ordinal) {
            Ok(v) => v.map(Value::F64).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        // NUMERIC/DECIMAL/MONEY 使用 rust_decimal 解码，以字符串形式保留精度
        ColType::Decimal => match row.try_get::<Option<rust_decimal::Decimal>, usize>(ordinal) {
            Ok(Some(v)) => Value::Decimal(v.to_string()),
            Ok(None) => Value::Null,
            Err(_) => match row.try_get::<Option<String>, usize>(ordinal) {
                Ok(v) => v.map(Value::String).unwrap_or(Value::Null),
                Err(_) => Value::Null,
            },
        },
        ColType::String => match row.try_get::<Option<String>, usize>(ordinal) {
            Ok(v) => v.map(Value::String).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        ColType::Bytes => match row.try_get::<Option<Vec<u8>>, usize>(ordinal) {
            Ok(v) => v.map(Value::Bytes).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        // DATE/DATETIME/TIME/JSON 在 PostgreSQL 中通常以字符串解码
        ColType::Date | ColType::DateTime | ColType::Time | ColType::Json => {
            match row.try_get::<Option<String>, usize>(ordinal) {
                Ok(v) => v.map(Value::String).unwrap_or(Value::Null),
                Err(_) => Value::Null,
            }
        }
        // UUID：使用 sqlx::types::Uuid（16 字节）解码，避免 36 字符字符串的内存浪费
        ColType::Uuid => match row.try_get::<Option<sqlx::types::Uuid>, usize>(ordinal) {
            Ok(v) => v
                .map(|uuid| Value::String(uuid.to_string()))
                .unwrap_or(Value::Null),
            Err(_) => match row.try_get::<Option<String>, usize>(ordinal) {
                Ok(v) => v.map(Value::String).unwrap_or(Value::Null),
                Err(_) => Value::Null,
            },
        },
        ColType::Unknown => {
            // 未知类型回退：i64 → f64 → bool → String
            if let Ok(v) = row.try_get::<Option<i64>, usize>(ordinal) {
                return v.map(Value::I64).unwrap_or(Value::Null);
            }
            if let Ok(v) = row.try_get::<Option<f64>, usize>(ordinal) {
                return v.map(Value::F64).unwrap_or(Value::Null);
            }
            if let Ok(v) = row.try_get::<Option<bool>, usize>(ordinal) {
                return v.map(Value::Bool).unwrap_or(Value::Null);
            }
            if let Ok(v) = row.try_get::<Option<String>, usize>(ordinal) {
                return v.map(Value::String).unwrap_or(Value::Null);
            }
            Value::Null
        }
        // ColType 标记为 #[non_exhaustive]，未来新增变体按 Unknown 处理
        _ => Value::Null,
    }
}

pub struct PgPoolHandle {
    pool: sqlx::PgPool,
}

impl PgPoolHandle {
    pub async fn connect(url: &str) -> Result<Self, DbError> {
        let pool = sqlx::pool::PoolOptions::<sqlx::Postgres>::new()
            .max_connections(10)
            .acquire_timeout(std::time::Duration::from_secs(30))
            .idle_timeout(Some(std::time::Duration::from_secs(600)))
            .max_lifetime(Some(std::time::Duration::from_secs(1800)))
            .connect(url)
            .await
            .map_err(map_sqlx_error)?;
        Ok(Self { pool })
    }

    pub fn from_pool(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &sqlx::PgPool {
        &self.pool
    }
}

pub struct SqlxPgConnectionFactory {
    pool: Arc<PgPoolHandle>,
}

impl SqlxPgConnectionFactory {
    pub fn new(pool: Arc<PgPoolHandle>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ConnectionFactory for SqlxPgConnectionFactory {
    async fn create(&self) -> Result<Box<dyn Connection>, DbError> {
        let conn = self.pool.pool.acquire().await.map_err(map_sqlx_error)?;
        Ok(Box::new(SqlxPgConnection {
            conn: Some(conn),
            connected: true,
            in_transaction: false,
        }))
    }
}

pub struct SqlxPgConnection {
    conn: Option<sqlx::pool::PoolConnection<sqlx::Postgres>>,
    connected: bool,
    in_transaction: bool,
}

impl Connection for SqlxPgConnection {
    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        Box::pin(async move {
            let mut pool_conn = self
                .conn
                .take()
                .ok_or_else(|| DbError::Internal("connection already closed".to_string()))?;
            // sqlx 0.9: PoolConnection 不再实现 Executor，需通过 DerefMut 解引用到内部连接
            // sqlx 0.9: SqlSafeStr 只对 &'static str 直接实现，非 'static 的 &str 需用 AssertSqlSafe 包装
            let result = if needs_raw_sql(sql) {
                (&mut *pool_conn)
                    .execute(sqlx::raw_sql(sqlx::AssertSqlSafe(sql)))
                    .await
            } else {
                (&mut *pool_conn).execute(sqlx::AssertSqlSafe(sql)).await
            };
            self.conn = Some(pool_conn);

            match result {
                Ok(r) => Ok(r.rows_affected()),
                Err(e) => {
                    let db_err = map_sqlx_error(e);
                    if matches!(db_err, DbError::ConnectionError(_) | DbError::IoError(_)) {
                        self.connected = false;
                    }
                    Err(db_err)
                }
            }
        })
    }

    fn query<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<HashMap<String, Value>>, DbError>> + Send + 'a>>
    {
        Box::pin(async move {
            let mut pool_conn = self
                .conn
                .take()
                .ok_or_else(|| DbError::Internal("connection already closed".to_string()))?;
            // sqlx 0.9: PoolConnection 不再实现 Executor，需通过 DerefMut 解引用到内部连接
            // sqlx 0.9: SqlSafeStr 只对 &'static str 直接实现，非 'static 的 &str 需用 AssertSqlSafe 包装
            let rows_result = (&mut *pool_conn).fetch_all(sqlx::AssertSqlSafe(sql)).await;
            self.conn = Some(pool_conn);

            let rows = rows_result.map_err(map_sqlx_error)?;
            if rows.is_empty() {
                return Ok(Vec::new());
            }
            // 预解析列类型（只在第一行解析，后续行复用，避免每行字符串 match）
            // PostgreSQL 专用解析：覆盖 PG 内部类型名（INT4/INT8/FLOAT8/BPCHAR/JSONB 等）
            let col_types: Vec<ColType> = rows[0]
                .columns()
                .iter()
                .map(|col| ColType::parse_postgres(col.type_info().name()))
                .collect();
            let mut result = Vec::with_capacity(rows.len());
            for row in &rows {
                let mut record = HashMap::with_capacity(col_types.len());
                for (i, col) in row.columns().iter().enumerate() {
                    let name = col.name().to_string();
                    let value = row_to_value_with_coltype_pg(row, i, col_types[i]);
                    record.insert(name, value);
                }
                result.push(record);
            }
            Ok(result)
        })
    }

    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            if self.in_transaction {
                return Err(DbError::Internal("transaction already started".to_string()));
            }
            self.execute("BEGIN").await?;
            self.in_transaction = true;
            Ok(())
        })
    }

    fn commit<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            if self.in_transaction {
                self.execute("COMMIT").await?;
                self.in_transaction = false;
            }
            Ok(())
        })
    }

    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            if self.in_transaction {
                let result = self.execute("ROLLBACK").await;
                self.in_transaction = false;
                result.map(|_| ())
            } else {
                Ok(())
            }
        })
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move {
            match self.execute("SELECT 1").await {
                Ok(_) => true,
                Err(_) => {
                    self.connected = false;
                    false
                }
            }
        })
    }

    fn close<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            if let Some(conn) = self.conn.take() {
                drop(conn);
            }
            self.connected = false;
            self.in_transaction = false;
            Ok(())
        })
    }

    /// PostgreSQL 参数绑定执行（INSERT/UPDATE/DELETE）
    ///
    /// 使用 sqlx prepared statement 绑定参数。PostgreSQL 协议使用 `$1, $2, ...` 占位符。
    ///
    /// # 类型映射说明
    ///
    /// PostgreSQL 不支持无符号整数类型（u8/u16/u32/u64），因此无符号 Value 绑定
    /// 时按"最小可容纳的有符号类型"转换：
    /// - `U8`  → `i16`（PostgreSQL `SMALLINT`）
    /// - `U16` → `i32`（PostgreSQL `INTEGER`）
    /// - `U32` → `i64`（PostgreSQL `BIGINT`）
    /// - `U64` → `i64`（可能截断，仅适用于 < `i64::MAX` 的值）
    ///
    /// 其他类型直接 bind；未知类型回退为 `to_string()` 后以 TEXT 绑定。
    fn execute_with_params<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [Value],
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        Box::pin(async move {
            if needs_raw_sql(sql) || params.is_empty() {
                return self.execute(sql).await;
            }
            let mut pool_conn = self
                .conn
                .take()
                .ok_or_else(|| DbError::Internal("connection already closed".to_string()))?;
            let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
            for v in params {
                q = match v {
                    Value::Null => q.bind(None::<i64>),
                    Value::Bool(b) => q.bind(*b),
                    Value::I8(n) => q.bind(*n),
                    Value::I16(n) => q.bind(*n),
                    Value::I32(n) => q.bind(*n),
                    Value::I64(n) => q.bind(*n),
                    // PostgreSQL 无 u8/u16/u32/u64：按最小可容纳有符号类型转换
                    Value::U8(n) => q.bind(*n as i16),
                    Value::U16(n) => q.bind(*n as i32),
                    Value::U32(n) => q.bind(*n as i64),
                    Value::U64(n) => q.bind(*n as i64),
                    Value::F32(f) => q.bind(*f),
                    Value::F64(f) => q.bind(*f),
                    Value::String(s) => q.bind(s.as_str()),
                    Value::Decimal(s) => q.bind(s.as_str()),
                    Value::Bytes(b) => q.bind(b.as_slice()),
                    other => q.bind(other.to_string()),
                };
            }
            let result = q.execute(&mut *pool_conn).await;
            self.conn = Some(pool_conn);
            match result {
                Ok(r) => Ok(r.rows_affected()),
                Err(e) => {
                    let db_err = map_sqlx_error(e);
                    if matches!(db_err, DbError::ConnectionError(_) | DbError::IoError(_)) {
                        self.connected = false;
                    }
                    Err(db_err)
                }
            }
        })
    }

    /// PostgreSQL 参数绑定查询（SELECT，HashMap 映射）
    ///
    /// 类型映射规则与 [`Self::execute_with_params`] 相同。
    fn query_with_params<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [Value],
    ) -> Pin<Box<dyn Future<Output = Result<QueryRows, DbError>> + Send + 'a>> {
        Box::pin(async move {
            if params.is_empty() {
                return self.query(sql).await;
            }
            let mut pool_conn = self
                .conn
                .take()
                .ok_or_else(|| DbError::Internal("connection already closed".to_string()))?;
            let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
            for v in params {
                q = match v {
                    Value::Null => q.bind(None::<i64>),
                    Value::Bool(b) => q.bind(*b),
                    Value::I8(n) => q.bind(*n),
                    Value::I16(n) => q.bind(*n),
                    Value::I32(n) => q.bind(*n),
                    Value::I64(n) => q.bind(*n),
                    Value::U8(n) => q.bind(*n as i16),
                    Value::U16(n) => q.bind(*n as i32),
                    Value::U32(n) => q.bind(*n as i64),
                    Value::U64(n) => q.bind(*n as i64),
                    Value::F32(f) => q.bind(*f),
                    Value::F64(f) => q.bind(*f),
                    Value::String(s) => q.bind(s.as_str()),
                    Value::Decimal(s) => q.bind(s.as_str()),
                    Value::Bytes(b) => q.bind(b.as_slice()),
                    other => q.bind(other.to_string()),
                };
            }
            let rows_result = q.fetch_all(&mut *pool_conn).await;
            self.conn = Some(pool_conn);
            let rows = rows_result.map_err(map_sqlx_error)?;
            if rows.is_empty() {
                return Ok(Vec::new());
            }
            // 预解析列类型（只在第一行解析，后续行复用，避免每行字符串 match）
            let col_types: Vec<ColType> = rows[0]
                .columns()
                .iter()
                .map(|col| ColType::parse_postgres(col.type_info().name()))
                .collect();
            let mut result = Vec::with_capacity(rows.len());
            for row in &rows {
                let mut record = HashMap::with_capacity(col_types.len());
                for (i, col) in row.columns().iter().enumerate() {
                    let name = col.name().to_string();
                    let value = row_to_value_with_coltype_pg(row, i, col_types[i]);
                    record.insert(name, value);
                }
                result.push(record);
            }
            Ok(result)
        })
    }

    /// PostgreSQL 位置式查询（SELECT，无参数）
    ///
    /// 绕过 HashMap 行映射，返回 `(列名, 按列序号的值矩阵)`。适用于 SELECT ALL 大结果集。
    fn query_values<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<QueryValues, DbError>> + Send + 'a>> {
        Box::pin(async move {
            let mut pool_conn = self
                .conn
                .take()
                .ok_or_else(|| DbError::Internal("connection already closed".to_string()))?;
            let rows_result = (&mut *pool_conn).fetch_all(sqlx::AssertSqlSafe(sql)).await;
            self.conn = Some(pool_conn);
            let rows = rows_result.map_err(map_sqlx_error)?;
            if rows.is_empty() {
                return Ok((Vec::new(), Vec::new()));
            }
            let cols = rows[0].columns();
            let mut col_names: Vec<String> = Vec::with_capacity(cols.len());
            for col in cols {
                col_names.push(col.name().to_string());
            }
            let mut result_rows: Vec<Vec<Value>> = Vec::with_capacity(rows.len());
            for row in rows {
                let mut row_values: Vec<Value> = Vec::with_capacity(col_names.len());
                for (idx, _) in col_names.iter().enumerate() {
                    let ordinal = row.columns()[idx].ordinal();
                    row_values.push(row_to_value_pg(&row, ordinal));
                }
                result_rows.push(row_values);
            }
            Ok((col_names, result_rows))
        })
    }

    /// PostgreSQL 参数绑定位置式查询（SELECT）
    ///
    /// 类型映射规则与 [`Self::execute_with_params`] 相同。
    fn query_values_with_params<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [Value],
    ) -> Pin<Box<dyn Future<Output = Result<QueryValues, DbError>> + Send + 'a>> {
        Box::pin(async move {
            if params.is_empty() {
                return self.query_values(sql).await;
            }
            let mut pool_conn = self
                .conn
                .take()
                .ok_or_else(|| DbError::Internal("connection already closed".to_string()))?;
            let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
            for v in params {
                q = match v {
                    Value::Null => q.bind(None::<i64>),
                    Value::Bool(b) => q.bind(*b),
                    Value::I8(n) => q.bind(*n),
                    Value::I16(n) => q.bind(*n),
                    Value::I32(n) => q.bind(*n),
                    Value::I64(n) => q.bind(*n),
                    Value::U8(n) => q.bind(*n as i16),
                    Value::U16(n) => q.bind(*n as i32),
                    Value::U32(n) => q.bind(*n as i64),
                    Value::U64(n) => q.bind(*n as i64),
                    Value::F32(f) => q.bind(*f),
                    Value::F64(f) => q.bind(*f),
                    Value::String(s) => q.bind(s.as_str()),
                    Value::Decimal(s) => q.bind(s.as_str()),
                    Value::Bytes(b) => q.bind(b.as_slice()),
                    other => q.bind(other.to_string()),
                };
            }
            let rows_result = q.fetch_all(&mut *pool_conn).await;
            self.conn = Some(pool_conn);
            let rows = rows_result.map_err(map_sqlx_error)?;
            if rows.is_empty() {
                return Ok((Vec::new(), Vec::new()));
            }
            let cols = rows[0].columns();
            let mut col_names: Vec<String> = Vec::with_capacity(cols.len());
            for col in cols {
                col_names.push(col.name().to_string());
            }
            let mut result_rows: Vec<Vec<Value>> = Vec::with_capacity(rows.len());
            for row in rows {
                let mut row_values: Vec<Value> = Vec::with_capacity(col_names.len());
                for (idx, _) in col_names.iter().enumerate() {
                    let ordinal = row.columns()[idx].ordinal();
                    row_values.push(row_to_value_pg(&row, ordinal));
                }
                result_rows.push(row_values);
            }
            Ok((col_names, result_rows))
        })
    }

    /// PostgreSQL 流式查询：逐行返回结果，避免大结果集 `fetch_all` 内存峰值
    ///
    /// 使用 `sqlx::query::fetch` 获取行流，逐行映射为 `HashMap<String, Value>`。
    /// 流正常消费完毕后连接归还 `self.conn`；若提前 drop 流，连接通过
    /// `PoolConnection::drop` 归还到 sqlx 池，但 `self.conn` 变为 `None`。
    ///
    /// 注意：PostgreSQL 的 `fetch` 返回 `Pin<Box<dyn Stream<...>>>`（boxed trait object），
    /// 其析构函数可能持有 `pool_conn` 的借用，导致无法直接 `self.conn = Some(pool_conn)`。
    /// 通过显式 `drop(row_stream)` 提前结束借用，再归还连接。
    fn query_stream<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn futures::Stream<Item = Result<HashMap<String, Value>, DbError>> + Send + 'a>>
    {
        Box::pin(async_stream::try_stream! {
            let mut pool_conn = self
                .conn
                .take()
                .ok_or_else(|| DbError::Internal("connection already closed".to_string()))?;
            let mut row_stream = sqlx::query(sqlx::AssertSqlSafe(sql)).fetch(&mut *pool_conn);
            // 流式查询无法预解析第一行列类型（流首行未知），使用每行 columns() 长度预分配 HashMap
            // 仍然使用 row_to_value_pg（按列类型字符串 match）以保证正确性
            while let Some(row_result) = row_stream.next().await {
                let row = row_result.map_err(map_sqlx_error)?;
                let cols = row.columns();
                let mut record = HashMap::with_capacity(cols.len());
                for (i, col) in cols.iter().enumerate() {
                    let name = col.name().to_string();
                    // 流式场景下每行都解析 ColType 反而增加开销，直接使用 row_to_value_pg
                    let value = row_to_value_pg(&row, i);
                    record.insert(name, value);
                }
                yield record;
            }
            // 显式 drop row_stream 以释放对 pool_conn 的借用
            // PostgreSQL 的 fetch 流是 boxed trait object，析构函数可能持有借用
            drop(row_stream);
            self.conn = Some(pool_conn);
        })
    }
}

impl Drop for SqlxPgConnection {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            drop(conn);
        }
    }
}

// ===================== PostgreSQL 扩展功能 =====================

/// PostgreSQL 扩展功能 trait
///
/// 提供 PostgreSQL 特有的 LISTEN/NOTIFY 通道通信与 COPY FROM STDIN 批量导入功能。
/// 这些功能在其他数据库（MySQL/SQLite）中没有对应实现，因此单独定义为扩展 trait，
/// 仅由 [`SqlxPgConnection`] 实现。
///
/// # LISTEN/NOTIFY
///
/// PostgreSQL 的轻量级进程间通信机制：
/// - `LISTEN channel`：订阅通道
/// - `NOTIFY channel, payload`：向通道发送通知
///
/// 接收通知需配合 `PgListener` 或轮询 `pg_notification` 系列函数。
///
/// # COPY FROM STDIN
///
/// PostgreSQL 的高性能批量导入协议，比逐行 INSERT 快 10~100 倍。
/// 数据通过专用协议流式传输，绕过 SQL 解析器。
#[async_trait]
pub trait PgExtensions: Send + Sync {
    /// LISTEN 通道：订阅指定通道的通知
    ///
    /// # 参数
    ///
    /// - `channel`: 通道名（仅允许字母、数字、下划线，防止 SQL 注入）
    ///
    /// # 错误
    ///
    /// - 通道名包含非法字符时返回 `DbError::Internal`
    /// - 连接已关闭时返回 `DbError::Internal`
    async fn listen(&mut self, channel: &str) -> Result<(), DbError>;

    /// NOTIFY 通道：向指定通道发送通知
    ///
    /// # 参数
    ///
    /// - `channel`: 通道名（仅允许字母、数字、下划线）
    /// - `payload`: 通知载荷字符串（单引号自动转义）
    ///
    /// # 错误
    ///
    /// - 通道名包含非法字符时返回 `DbError::Internal`
    async fn notify(&mut self, channel: &str, payload: &str) -> Result<(), DbError>;

    /// COPY FROM STDIN：批量导入数据
    ///
    /// # 参数
    ///
    /// - `sql`: COPY 语句，如 `COPY mytable (col1, col2) FROM STDIN WITH (FORMAT csv, HEADER true)`
    /// - `data`: 完整的导入数据字节流
    ///
    /// # 返回
    ///
    /// 返回受影响的行数。
    ///
    /// # 性能
    ///
    /// 比逐行 INSERT 快 10~100 倍，适用于大批量数据导入（10 万行以上）。
    async fn copy_from_stdin(&mut self, sql: &str, data: &[u8]) -> Result<u64, DbError>;
}

/// 校验 PostgreSQL 通道名合法性（仅允许字母、数字、下划线）
///
/// LISTEN/NOTIFY 的通道名不能通过参数化绑定，必须拼接 SQL，
/// 因此需严格校验防止 SQL 注入。
fn validate_pg_channel_name(channel: &str) -> Result<(), DbError> {
    if channel.is_empty() {
        return Err(DbError::Internal(
            "PG channel name must not be empty".to_string(),
        ));
    }
    if !channel
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(DbError::Internal(format!(
            "invalid PG channel name: {} (only alphanumeric and underscore allowed)",
            channel
        )));
    }
    Ok(())
}

#[async_trait]
impl PgExtensions for SqlxPgConnection {
    async fn listen(&mut self, channel: &str) -> Result<(), DbError> {
        validate_pg_channel_name(channel)?;
        // LISTEN 是简单 SQL 命令，channel 已校验，可安全拼接
        self.execute(&format!("LISTEN {}", channel)).await?;
        Ok(())
    }

    async fn notify(&mut self, channel: &str, payload: &str) -> Result<(), DbError> {
        validate_pg_channel_name(channel)?;
        // payload 通过 PostgreSQL 字符串字面量转义（单引号加倍）防止注入
        let escaped_payload = payload.replace('\'', "''");
        self.execute(&format!("NOTIFY {}, '{}'", channel, escaped_payload))
            .await?;
        Ok(())
    }

    async fn copy_from_stdin(&mut self, sql: &str, data: &[u8]) -> Result<u64, DbError> {
        let mut pool_conn = self
            .conn
            .take()
            .ok_or_else(|| DbError::Internal("connection already closed".to_string()))?;
        // sqlx 0.9: 通过 DerefMut 访问 PgConnection 的 copy_in_raw 方法
        // 启动 COPY 协议，返回 PgCopyIn 流式写入句柄
        // 注意：直接使用 *pool_conn 解引用，避免 clippy::needless_borrow 警告
        let mut copy = (*pool_conn)
            .copy_in_raw(sql)
            .await
            .map_err(map_sqlx_error)?;
        // 一次性发送全部数据（非流式，适用于数据可完整载入内存的场景）
        copy.send(data).await.map_err(map_sqlx_error)?;
        // 完成 COPY 操作，sqlx 0.9 的 PgCopyIn::finish() 直接返回 u64（受影响行数）
        let result = copy.finish().await.map_err(map_sqlx_error)?;
        self.conn = Some(pool_conn);
        Ok(result)
    }
}

/// PostgreSQL 批量导入（多行 INSERT，作为 COPY 协议的简化替代）
///
/// 构建多行 INSERT 语句：`INSERT INTO t (c1, c2) VALUES ($1, $2), ($3, $4), ...`
/// 使用参数绑定避免 SQL 注入。PostgreSQL 占位符 `$N` 跨行递增。
///
/// 当数据量极大（>10 万行）时，建议分批调用或使用
/// [`PgExtensions::copy_from_stdin`] 走 COPY 协议以获得更高吞吐。
pub async fn pg_bulk_insert(
    conn: &mut SqlxPgConnection,
    table: &str,
    columns: &[&str],
    rows: &[Vec<Value>],
) -> Result<u64, DbError> {
    if rows.is_empty() {
        return Ok(0);
    }
    let col_list = columns.join(", ");
    let cols_per_row = columns.len();
    // PostgreSQL 占位符 $1, $2, ... 跨行递增
    let placeholders: Vec<String> = rows
        .iter()
        .enumerate()
        .map(|(row_idx, _)| {
            let base = row_idx * cols_per_row;
            let ph: Vec<String> = (0..cols_per_row)
                .map(|i| format!("${}", base + i + 1))
                .collect();
            format!("({})", ph.join(", "))
        })
        .collect();
    let sql = format!(
        "INSERT INTO {} ({}) VALUES {}",
        table,
        col_list,
        placeholders.join(", ")
    );
    // 展平所有行的值为单一参数数组
    let mut params: Vec<Value> = Vec::with_capacity(rows.len() * cols_per_row);
    for row in rows {
        for v in row {
            params.push(v.clone());
        }
    }
    conn.execute_with_params(&sql, &params).await
}
