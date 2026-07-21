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
use sqlx::{Column, Executor, Row};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use sz_orm_core::{Connection, ConnectionFactory, DbError, Value};

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

/// 将 SqliteRow 转换为 Value（按列序号）
/// 使用列类型信息决定解码类型，避免 bool/int 混淆
fn row_to_value_sqlite(row: &sqlx::sqlite::SqliteRow, ordinal: usize) -> Value {
    use sqlx::TypeInfo;
    let type_name = row.columns()[ordinal].type_info().name();
    match type_name {
        "BOOLEAN" => match row.try_get::<Option<bool>, usize>(ordinal) {
            Ok(v) => v.map(Value::Bool).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        "INTEGER" => match row.try_get::<Option<i64>, usize>(ordinal) {
            Ok(v) => v.map(Value::I64).unwrap_or(Value::Null),
            Err(_) => match row.try_get::<Option<i32>, usize>(ordinal) {
                Ok(v) => v.map(Value::I32).unwrap_or(Value::Null),
                Err(_) => Value::Null,
            },
        },
        "REAL" => match row.try_get::<Option<f64>, usize>(ordinal) {
            Ok(v) => v.map(Value::F64).unwrap_or(Value::Null),
            Err(_) => match row.try_get::<Option<f32>, usize>(ordinal) {
                Ok(v) => v.map(Value::F32).unwrap_or(Value::Null),
                Err(_) => Value::Null,
            },
        },
        "TEXT" => match row.try_get::<Option<String>, usize>(ordinal) {
            Ok(v) => v.map(Value::String).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        "BLOB" => match row.try_get::<Option<Vec<u8>>, usize>(ordinal) {
            Ok(v) => v.map(Value::Bytes).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
        _ => {
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
    }
}

pub struct SqlitePoolHandle {
    pool: sqlx::SqlitePool,
}

impl SqlitePoolHandle {
    pub async fn connect(url: &str) -> Result<Self, DbError> {
        let pool = sqlx::pool::PoolOptions::<sqlx::Sqlite>::new()
            .max_connections(10)
            .acquire_timeout(std::time::Duration::from_secs(30))
            .idle_timeout(Some(std::time::Duration::from_secs(600)))
            .max_lifetime(Some(std::time::Duration::from_secs(1800)))
            .connect(url)
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
            let mut result = Vec::with_capacity(rows.len());
            for row in rows {
                let mut record = HashMap::new();
                for col in row.columns() {
                    let name = col.name().to_string();
                    let ordinal = col.ordinal();
                    let value = row_to_value_sqlite(&row, ordinal);
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
}

impl Drop for SqlxSqliteConnection {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            drop(conn);
        }
    }
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
        // DECIMAL/NUMERIC 使用 rust_decimal 解码
        "DECIMAL" | "NUMERIC" | "NEWDECIMAL" => {
            match row.try_get::<Option<rust_decimal::Decimal>, usize>(ordinal) {
                Ok(Some(v)) => Value::F64(v.to_string().parse::<f64>().unwrap_or(0.0)),
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
            let mut result = Vec::with_capacity(rows.len());
            for row in rows {
                let mut record = HashMap::new();
                for col in row.columns() {
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
}

impl Drop for SqlxMySqlConnection {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            drop(conn);
        }
    }
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
            Ok(Some(v)) => Value::F64(v.to_string().parse::<f64>().unwrap_or(0.0)),
            Ok(None) => Value::Null,
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
            let mut result = Vec::with_capacity(rows.len());
            for row in rows {
                let mut record = HashMap::new();
                for col in row.columns() {
                    let name = col.name().to_string();
                    let ordinal = col.ordinal();
                    let value = row_to_value_pg(&row, ordinal);
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
}

impl Drop for SqlxPgConnection {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            drop(conn);
        }
    }
}
