//! Any driver — 一套代码多 DB 后端透明切换（SQLx 风格）
//!
//! SQLx 提供 `sqlx::Any` 适配器，让同一份代码可以在 MySQL/PostgreSQL/SQLite
//! 之间透明切换。SZ-ORM 在 `sz-orm-sqlx` 已有各后端独立实现，
//! 此模块在上层提供统一的 [`AnyConnection`] 和 [`AnyPool`] 抽象，
//! 让运行时切换数据库后端成为可能。
//!
//! # 设计
//!
//! - [`AnyBackend`]：枚举后端类型
//! - [`AnyPool`]：持有具体后端的 `Box<dyn ConnectionFactory>`
//! - [`AnyConnection`]：持有具体后端的 `Box<dyn Connection>`
//! - 通过 DSN 自动识别后端类型，运行时透明切换
//!
//! # 用法
//!
//! ```ignore
//! use sz_orm_sqlx::any_driver::{AnyBackend, AnyPool};
//!
//! // 从 DSN 自动识别后端
//! let pool = AnyPool::connect("mysql://root:pass@127.0.0.1/db").await?;
//! let mut conn = pool.create().await?;
//! let rows = conn.query("SELECT 1").await?;
//!
//! // 运行时切换后端
//! let pg_pool = AnyPool::connect("postgres://user:pass@127.0.0.1/db").await?;
//! let mut pg_conn = pg_pool.create().await?;
//! let rows = pg_conn.query("SELECT 1").await?;
//! ```

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use sz_orm_core::{
    Connection, ConnectionFactory, DbError, Dialect, MySqlDialect, OracleDialect,
    PostgreSqlDialect, QueryRows, SqlServerDialect, SqliteDialect,
};

use crate::any::{
    MySqlPoolHandle, PgPoolHandle, SqlitePoolHandle, SqlxMySqlConnectionFactory,
    SqlxPgConnectionFactory, SqlxSqliteConnectionFactory,
};

#[cfg(feature = "oracle")]
use sz_orm_oracle::{OracleConnectionFactory, OraclePoolHandle};

#[cfg(feature = "mssql")]
use sz_orm_mssql::{MssqlConnectionFactory, MssqlPoolHandle};

/// 数据库后端类型
///
/// v2.2.0 新增 `Oracle` 和 `Mssql` 变体（需启用 `oracle`/`mssql` feature）。
/// `#[non_exhaustive]` 标注确保外部 crate match 时必须使用 wildcard 臂，
/// 未来新增变体不会破坏现有代码。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AnyBackend {
    /// MySQL / MariaDB
    MySql,
    /// PostgreSQL
    Postgres,
    /// SQLite
    Sqlite,
    /// Oracle（v2.2.0 新增，需启用 `oracle` feature）
    Oracle,
    /// SQL Server / MSSQL（v2.2.0 新增，需启用 `mssql` feature）
    Mssql,
}

impl AnyBackend {
    /// 从 DSN 自动识别后端类型
    ///
    /// # 支持的 scheme
    ///
    /// - `mysql://` / `mariadb://` → MySQL
    /// - `postgres://` / `postgresql://` → Postgres
    /// - `sqlite://` / `sqlite:` → Sqlite
    /// - `oracle://` → Oracle（v2.2.0 新增）
    /// - `mssql://` / `sqlserver://` → Mssql（v2.2.0 新增）
    pub fn from_dsn(dsn: &str) -> Result<Self, DbError> {
        if dsn.starts_with("mysql://") || dsn.starts_with("mariadb://") {
            Ok(AnyBackend::MySql)
        } else if dsn.starts_with("postgres://") || dsn.starts_with("postgresql://") {
            Ok(AnyBackend::Postgres)
        } else if dsn.starts_with("sqlite://") || dsn.starts_with("sqlite:") {
            Ok(AnyBackend::Sqlite)
        } else if dsn.starts_with("oracle://") {
            Ok(AnyBackend::Oracle)
        } else if dsn.starts_with("mssql://") || dsn.starts_with("sqlserver://") {
            Ok(AnyBackend::Mssql)
        } else {
            Err(DbError::ConnectionRefused(format!(
                "未知的 DSN scheme: {}（支持 mysql/postgres/sqlite/oracle/mssql）",
                dsn
            )))
        }
    }

    /// 后端名称
    pub fn name(&self) -> &'static str {
        match self {
            AnyBackend::MySql => "mysql",
            AnyBackend::Postgres => "postgres",
            AnyBackend::Sqlite => "sqlite",
            AnyBackend::Oracle => "oracle",
            AnyBackend::Mssql => "mssql",
        }
    }

    /// 返回对应后端的 Dialect 实例（v2.2.0 新增）
    ///
    /// - MySql → [`MySqlDialect`]
    /// - Postgres → [`PostgreSqlDialect`]
    /// - Sqlite → [`SqliteDialect`]
    /// - Oracle → [`OracleDialect`]
    /// - Mssql → [`SqlServerDialect`]
    pub fn dialect(&self) -> Box<dyn Dialect> {
        match self {
            AnyBackend::MySql => Box::new(MySqlDialect),
            AnyBackend::Postgres => Box::new(PostgreSqlDialect),
            AnyBackend::Sqlite => Box::new(SqliteDialect),
            AnyBackend::Oracle => Box::new(OracleDialect),
            AnyBackend::Mssql => Box::new(SqlServerDialect),
        }
    }
}

/// 后端无关的连接工厂
pub struct AnyPool {
    backend: AnyBackend,
    factory: Arc<dyn ConnectionFactory>,
}

impl AnyPool {
    /// 连接数据库，根据 DSN 自动识别后端
    ///
    /// # 错误
    ///
    /// - DSN scheme 不识别 → [`DbError::ConnectionRefused`]
    /// - 连接失败 → [`DbError::ConnectionError`]
    /// - Oracle/MSSQL 后端未启用对应 feature → [`DbError::ConnectionRefused`] 含提示
    pub async fn connect(dsn: &str) -> Result<Self, DbError> {
        let backend = AnyBackend::from_dsn(dsn)?;
        let factory: Arc<dyn ConnectionFactory> = match backend {
            AnyBackend::MySql => {
                let handle = Arc::new(MySqlPoolHandle::connect(dsn).await?);
                Arc::new(SqlxMySqlConnectionFactory::new(handle))
            }
            AnyBackend::Postgres => {
                let handle = Arc::new(PgPoolHandle::connect(dsn).await?);
                Arc::new(SqlxPgConnectionFactory::new(handle))
            }
            AnyBackend::Sqlite => {
                let handle = Arc::new(SqlitePoolHandle::connect(dsn).await?);
                Arc::new(SqlxSqliteConnectionFactory::new(handle))
            }
            AnyBackend::Oracle => {
                #[cfg(feature = "oracle")]
                {
                    let (username, password, connect_string) = parse_oracle_dsn(dsn)?;
                    let handle = Arc::new(OraclePoolHandle::connect(
                        &username,
                        &password,
                        &connect_string,
                    )?);
                    Arc::new(OracleConnectionFactory::new(handle))
                }
                #[cfg(not(feature = "oracle"))]
                {
                    return Err(DbError::ConnectionRefused(
                        "Oracle 后端未启用，请在 Cargo.toml 中添加 features = [\"oracle\"]"
                            .to_string(),
                    ));
                }
            }
            AnyBackend::Mssql => {
                #[cfg(feature = "mssql")]
                {
                    let ado_string = parse_mssql_dsn(dsn)?;
                    let handle = Arc::new(MssqlPoolHandle::connect(&ado_string).await?);
                    Arc::new(MssqlConnectionFactory::new(handle))
                }
                #[cfg(not(feature = "mssql"))]
                {
                    return Err(DbError::ConnectionRefused(
                        "MSSQL 后端未启用，请在 Cargo.toml 中添加 features = [\"mssql\"]"
                            .to_string(),
                    ));
                }
            }
        };
        Ok(Self { backend, factory })
    }

    /// 从已有的连接工厂构造
    pub fn from_factory(backend: AnyBackend, factory: Arc<dyn ConnectionFactory>) -> Self {
        Self { backend, factory }
    }

    /// 获取后端类型
    pub fn backend(&self) -> AnyBackend {
        self.backend
    }

    /// 返回对应后端的 Dialect 实例（v2.2.0 新增）
    ///
    /// 委托 [`AnyBackend::dialect()`]，根据后端自动选择方言。
    pub fn dialect(&self) -> Box<dyn Dialect> {
        self.backend.dialect()
    }

    /// 创建一个新连接
    pub async fn create(&self) -> Result<AnyConnection, DbError> {
        let conn = self.factory.create().await?;
        Ok(AnyConnection {
            backend: self.backend,
            inner: conn,
        })
    }
}

/// 解析 Oracle DSN 为 (username, password, connect_string)
///
/// 格式：`oracle://user:pass@host:port/service`
///
/// 返回：username="user", password="pass", connect_string="host:port/service"
#[allow(dead_code)]
pub(crate) fn parse_oracle_dsn(dsn: &str) -> Result<(String, String, String), DbError> {
    let rest = dsn
        .strip_prefix("oracle://")
        .ok_or_else(|| DbError::ConnectionRefused(format!("无效的 Oracle DSN: {}", dsn)))?;
    parse_user_pass_host(rest, "Oracle")
}

/// 解析 MSSQL DSN 为 ADO.NET 连接字符串
///
/// 格式：`mssql://user:pass@host:port/database` 或 `sqlserver://user:pass@host:port/database`
///
/// 返回：`Server=host,port;Database=database;User Id=user;Password=pass;`
#[allow(dead_code)]
pub(crate) fn parse_mssql_dsn(dsn: &str) -> Result<String, DbError> {
    let rest = dsn
        .strip_prefix("mssql://")
        .or_else(|| dsn.strip_prefix("sqlserver://"))
        .ok_or_else(|| DbError::ConnectionRefused(format!("无效的 MSSQL DSN: {}", dsn)))?;
    let (username, password, host_port_db) = parse_user_pass_host(rest, "MSSQL")?;
    let (host_port, database) = host_port_db
        .split_once('/')
        .ok_or_else(|| DbError::ConnectionRefused(format!("MSSQL DSN 缺少 database: {}", dsn)))?;
    let (host, port) = host_port.split_once(':').unwrap_or((host_port, "1433"));
    Ok(format!(
        "Server={},{};Database={};User Id={};Password={};",
        host, port, database, username, password
    ))
}

/// 通用 DSN 解析：`user:pass@host:port/...` → (user, pass, host:port/...)
#[allow(dead_code)]
fn parse_user_pass_host(rest: &str, backend: &str) -> Result<(String, String, String), DbError> {
    let (userinfo, hostinfo) = rest
        .split_once('@')
        .ok_or_else(|| DbError::ConnectionRefused(format!("{} DSN 缺少 @: {}", backend, rest)))?;
    let (username, password) = userinfo.split_once(':').ok_or_else(|| {
        DbError::ConnectionRefused(format!("{} DSN 缺少 password: {}", backend, rest))
    })?;
    Ok((
        username.to_string(),
        password.to_string(),
        hostinfo.to_string(),
    ))
}

/// 后端无关的连接
pub struct AnyConnection {
    backend: AnyBackend,
    inner: Box<dyn Connection>,
}

impl AnyConnection {
    /// 获取后端类型
    pub fn backend(&self) -> AnyBackend {
        self.backend
    }
}

impl Connection for AnyConnection {
    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        self.inner.execute(sql)
    }

    fn query<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<QueryRows, DbError>> + Send + 'a>> {
        self.inner.query(sql)
    }

    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        self.inner.begin_transaction()
    }

    fn commit<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        self.inner.commit()
    }

    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        self.inner.rollback()
    }

    fn is_connected(&self) -> bool {
        self.inner.is_connected()
    }

    fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        self.inner.ping()
    }

    fn close<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        self.inner.close()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_any_backend_from_dsn_mysql() {
        assert_eq!(
            AnyBackend::from_dsn("mysql://root:pass@127.0.0.1/db").unwrap(),
            AnyBackend::MySql
        );
        assert_eq!(
            AnyBackend::from_dsn("mariadb://root:pass@127.0.0.1/db").unwrap(),
            AnyBackend::MySql
        );
    }

    #[test]
    fn test_any_backend_from_dsn_postgres() {
        assert_eq!(
            AnyBackend::from_dsn("postgres://user:pass@127.0.0.1/db").unwrap(),
            AnyBackend::Postgres
        );
        assert_eq!(
            AnyBackend::from_dsn("postgresql://user:pass@127.0.0.1/db").unwrap(),
            AnyBackend::Postgres
        );
    }

    #[test]
    fn test_any_backend_from_dsn_sqlite() {
        assert_eq!(
            AnyBackend::from_dsn("sqlite::memory:").unwrap(),
            AnyBackend::Sqlite
        );
        assert_eq!(
            AnyBackend::from_dsn("sqlite://./test.db").unwrap(),
            AnyBackend::Sqlite
        );
    }

    #[test]
    fn test_any_backend_from_dsn_oracle() {
        assert_eq!(
            AnyBackend::from_dsn("oracle://sys:test123@127.0.0.1:1521/freepdb1").unwrap(),
            AnyBackend::Oracle
        );
    }

    #[test]
    fn test_any_backend_from_dsn_mssql() {
        assert_eq!(
            AnyBackend::from_dsn("mssql://sa:test123@127.0.0.1:1433/testdb").unwrap(),
            AnyBackend::Mssql
        );
    }

    #[test]
    fn test_any_backend_from_dsn_sqlserver() {
        assert_eq!(
            AnyBackend::from_dsn("sqlserver://sa:test123@127.0.0.1:1433/testdb").unwrap(),
            AnyBackend::Mssql
        );
    }

    #[test]
    fn test_any_backend_from_dsn_unknown_v22() {
        let result = AnyBackend::from_dsn("redis://127.0.0.1");
        assert!(result.is_err());
        if let Err(e) = result {
            let msg = format!("{}", e);
            assert!(msg.contains("mysql"));
            assert!(msg.contains("postgres"));
            assert!(msg.contains("sqlite"));
            assert!(msg.contains("oracle"));
            assert!(msg.contains("mssql"));
        }
    }

    #[test]
    fn test_any_backend_name_v22() {
        assert_eq!(AnyBackend::MySql.name(), "mysql");
        assert_eq!(AnyBackend::Postgres.name(), "postgres");
        assert_eq!(AnyBackend::Sqlite.name(), "sqlite");
        assert_eq!(AnyBackend::Oracle.name(), "oracle");
        assert_eq!(AnyBackend::Mssql.name(), "mssql");
    }

    #[test]
    fn test_any_backend_equality() {
        assert_eq!(AnyBackend::MySql, AnyBackend::MySql);
        assert_ne!(AnyBackend::MySql, AnyBackend::Postgres);
        assert_ne!(AnyBackend::Postgres, AnyBackend::Sqlite);
        assert_ne!(AnyBackend::Oracle, AnyBackend::Mssql);
        assert_ne!(AnyBackend::Oracle, AnyBackend::MySql);
    }

    #[test]
    fn test_parse_oracle_dsn() {
        let (user, pass, cs) =
            parse_oracle_dsn("oracle://sys:test123@127.0.0.1:1521/freepdb1").unwrap();
        assert_eq!(user, "sys");
        assert_eq!(pass, "test123");
        assert_eq!(cs, "127.0.0.1:1521/freepdb1");
    }

    #[test]
    fn test_parse_mssql_dsn() {
        let ado = parse_mssql_dsn("mssql://sa:test123@127.0.0.1:1433/testdb").unwrap();
        assert!(ado.contains("Server=127.0.0.1,1433"));
        assert!(ado.contains("Database=testdb"));
        assert!(ado.contains("User Id=sa"));
        assert!(ado.contains("Password=test123"));
    }

    #[test]
    fn test_parse_mssql_dsn_sqlserver_scheme() {
        let ado = parse_mssql_dsn("sqlserver://sa:pass@localhost/testdb").unwrap();
        assert!(ado.contains("Server=localhost,1433"));
        assert!(ado.contains("Database=testdb"));
    }

    // ---- 真实 SQLite 集成测试 ----

    #[tokio::test]
    async fn test_any_pool_sqlite_connect_and_query() {
        let pool = AnyPool::connect("sqlite::memory:").await.unwrap();
        assert_eq!(pool.backend(), AnyBackend::Sqlite);

        let mut conn = pool.create().await.unwrap();
        assert_eq!(conn.backend(), AnyBackend::Sqlite);

        // 创建表并插入数据
        conn.execute("CREATE TABLE test_any (id INTEGER PRIMARY KEY, name TEXT)")
            .await
            .unwrap();
        conn.execute("INSERT INTO test_any (name) VALUES ('Alice')")
            .await
            .unwrap();
        conn.execute("INSERT INTO test_any (name) VALUES ('Bob')")
            .await
            .unwrap();

        // 查询验证
        let rows = conn
            .query("SELECT * FROM test_any ORDER BY id")
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].get("name").and_then(|v| v.as_str()), Some("Alice"));
        assert_eq!(rows[1].get("name").and_then(|v| v.as_str()), Some("Bob"));
    }

    #[tokio::test]
    async fn test_any_pool_sqlite_transaction_commit() {
        let pool = AnyPool::connect("sqlite::memory:").await.unwrap();
        let mut conn = pool.create().await.unwrap();

        conn.execute("CREATE TABLE tx_test (id INTEGER PRIMARY KEY, val INTEGER)")
            .await
            .unwrap();

        // 事务提交
        conn.begin_transaction().await.unwrap();
        conn.execute("INSERT INTO tx_test (val) VALUES (1)")
            .await
            .unwrap();
        conn.execute("INSERT INTO tx_test (val) VALUES (2)")
            .await
            .unwrap();
        conn.commit().await.unwrap();

        let rows = conn.query("SELECT * FROM tx_test").await.unwrap();
        assert_eq!(rows.len(), 2);
    }

    #[tokio::test]
    async fn test_any_pool_sqlite_transaction_rollback() {
        let pool = AnyPool::connect("sqlite::memory:").await.unwrap();
        let mut conn = pool.create().await.unwrap();

        conn.execute("CREATE TABLE tx_rb (id INTEGER PRIMARY KEY, val INTEGER)")
            .await
            .unwrap();

        conn.begin_transaction().await.unwrap();
        conn.execute("INSERT INTO tx_rb (val) VALUES (1)")
            .await
            .unwrap();
        conn.rollback().await.unwrap();

        let rows = conn.query("SELECT * FROM tx_rb").await.unwrap();
        assert_eq!(rows.len(), 0);
    }

    #[tokio::test]
    async fn test_any_pool_sqlite_ping() {
        let pool = AnyPool::connect("sqlite::memory:").await.unwrap();
        let mut conn = pool.create().await.unwrap();
        let ok = conn.ping().await;
        assert!(ok);
        assert!(conn.is_connected());
    }

    #[tokio::test]
    async fn test_any_pool_invalid_dsn() {
        let result = AnyPool::connect("invalid://dsn").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_any_pool_sqlite_count_query() {
        let pool = AnyPool::connect("sqlite::memory:").await.unwrap();
        let mut conn = pool.create().await.unwrap();

        conn.execute("CREATE TABLE cnt (id INTEGER PRIMARY KEY)")
            .await
            .unwrap();
        for i in 1..=5 {
            conn.execute(&format!("INSERT INTO cnt (id) VALUES ({})", i))
                .await
                .unwrap();
        }

        // 通过 SELECT * 验证行数（避开 sqlx 适配器中 COUNT(*) 类型推断的既有问题）
        let rows = conn.query("SELECT * FROM cnt").await.unwrap();
        assert_eq!(rows.len(), 5);
    }

    // ---- v2.2.0 A-2: Dialect 与 AnyPool 集成测试 ----

    #[test]
    fn test_any_backend_dialect_mapping() {
        use sz_orm_core::DbType;

        let mysql_d = AnyBackend::MySql.dialect();
        assert_eq!(mysql_d.db_type(), DbType::MySQL);

        let pg_d = AnyBackend::Postgres.dialect();
        assert_eq!(pg_d.db_type(), DbType::PostgreSQL);

        let sqlite_d = AnyBackend::Sqlite.dialect();
        assert_eq!(sqlite_d.db_type(), DbType::Sqlite);

        let oracle_d = AnyBackend::Oracle.dialect();
        assert_eq!(oracle_d.db_type(), DbType::Oracle);

        let mssql_d = AnyBackend::Mssql.dialect();
        assert_eq!(mssql_d.db_type(), DbType::SqlServer);
    }

    #[test]
    fn test_oracle_dialect_pagination() {
        let d = AnyBackend::Oracle.dialect();
        let sql = d.build_pagination("SELECT * FROM users", 2, 10);
        let upper = sql.to_uppercase();
        assert!(
            upper.contains("OFFSET") || upper.contains("FETCH") || upper.contains("ROWNUM"),
            "Oracle 分页 SQL 应含 OFFSET/FETCH/ROWNUM，实际: {}",
            sql
        );
        assert!(
            !upper.contains("LIMIT"),
            "Oracle 分页 SQL 不应含 LIMIT，实际: {}",
            sql
        );
    }

    #[test]
    fn test_mssql_dialect_pagination() {
        let d = AnyBackend::Mssql.dialect();
        let sql = d.build_pagination("SELECT * FROM users", 2, 10);
        let upper = sql.to_uppercase();
        assert!(
            upper.contains("OFFSET") || upper.contains("FETCH"),
            "MSSQL 分页 SQL 应含 OFFSET/FETCH，实际: {}",
            sql
        );
        assert!(
            !upper.contains("LIMIT"),
            "MSSQL 分页 SQL 不应含 LIMIT，实际: {}",
            sql
        );
    }

    #[test]
    fn test_oracle_dialect_no_placeholder() {
        let d = AnyBackend::Oracle.dialect();
        // 调用各 Dialect trait 方法，确认不 panic（无 todo!/unimplemented!）
        let _ = d.db_type();
        let _ = d.quote("col");
        let _ = d.quote_checked("col").unwrap();
        let _ = d.escape_string("val");
        let _ = d.supports_returning();
        let _ = d.build_pagination("SELECT 1", 1, 10);
        let _ = d.json_type();
        let _ = d.json_extract("col", "$.key");
        let _ = d.full_text_search(&["col"], "kw");
        let _ = d.bool_to_int("expr");
        let _ = d.concat(&["a", "b"]);
    }

    #[test]
    fn test_mssql_dialect_no_placeholder() {
        let d = AnyBackend::Mssql.dialect();
        let _ = d.db_type();
        let _ = d.quote("col");
        let _ = d.quote_checked("col").unwrap();
        let _ = d.escape_string("val");
        let _ = d.supports_returning();
        let _ = d.build_pagination("SELECT 1", 1, 10);
        let _ = d.json_type();
        let _ = d.json_extract("col", "$.key");
        let _ = d.full_text_search(&["col"], "kw");
        let _ = d.bool_to_int("expr");
        let _ = d.concat(&["a", "b"]);
    }

    #[tokio::test]
    async fn test_any_pool_dialect_sqlite() {
        let pool = AnyPool::connect("sqlite::memory:").await.unwrap();
        let d = pool.dialect();
        assert_eq!(d.db_type(), sz_orm_core::DbType::Sqlite);
    }
}
