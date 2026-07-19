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
use sz_orm_core::{Connection, ConnectionFactory, DbError, QueryRows};

use crate::any::{
    MySqlPoolHandle, PgPoolHandle, SqlitePoolHandle, SqlxMySqlConnectionFactory,
    SqlxPgConnectionFactory, SqlxSqliteConnectionFactory,
};

/// 数据库后端类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnyBackend {
    /// MySQL / MariaDB
    MySql,
    /// PostgreSQL
    Postgres,
    /// SQLite
    Sqlite,
}

impl AnyBackend {
    /// 从 DSN 自动识别后端类型
    ///
    /// # 支持的 scheme
    ///
    /// - `mysql://` / `mariadb://` → MySQL
    /// - `postgres://` / `postgresql://` → Postgres
    /// - `sqlite://` / `sqlite:` → Sqlite
    pub fn from_dsn(dsn: &str) -> Result<Self, DbError> {
        if dsn.starts_with("mysql://") || dsn.starts_with("mariadb://") {
            Ok(AnyBackend::MySql)
        } else if dsn.starts_with("postgres://") || dsn.starts_with("postgresql://") {
            Ok(AnyBackend::Postgres)
        } else if dsn.starts_with("sqlite://") || dsn.starts_with("sqlite:") {
            Ok(AnyBackend::Sqlite)
        } else {
            Err(DbError::ConnectionRefused(format!(
                "未知的 DSN scheme: {}（支持 mysql/postgres/sqlite）",
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

    /// 创建一个新连接
    pub async fn create(&self) -> Result<AnyConnection, DbError> {
        let conn = self.factory.create().await?;
        Ok(AnyConnection {
            backend: self.backend,
            inner: conn,
        })
    }
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
    fn test_any_backend_from_dsn_unknown() {
        let result = AnyBackend::from_dsn("redis://127.0.0.1");
        assert!(result.is_err());
        if let Err(e) = result {
            let msg = format!("{}", e);
            assert!(msg.contains("redis") || msg.contains("未知"));
        }
    }

    #[test]
    fn test_any_backend_name() {
        assert_eq!(AnyBackend::MySql.name(), "mysql");
        assert_eq!(AnyBackend::Postgres.name(), "postgres");
        assert_eq!(AnyBackend::Sqlite.name(), "sqlite");
    }

    #[test]
    fn test_any_backend_equality() {
        assert_eq!(AnyBackend::MySql, AnyBackend::MySql);
        assert_ne!(AnyBackend::MySql, AnyBackend::Postgres);
        assert_ne!(AnyBackend::Postgres, AnyBackend::Sqlite);
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
}
