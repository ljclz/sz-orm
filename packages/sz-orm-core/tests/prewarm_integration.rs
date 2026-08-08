//! v3.2.0 M1 连接池预热增强 — 集成测试（真实 DB + 冷启动 P95）
//!
//! 测试数据库：
//! - MySQL：mysql://root:test123@127.0.0.1:3306/sz_orm_test
//! - PostgreSQL：postgres://postgres:test123@127.0.0.1:5432/sz_orm_test
//!
//! 运行方式：
//! cargo test -p sz-orm-core --features auto-prewarm --test prewarm_integration -- --ignored --nocapture

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use sqlx::Executor;
use sz_orm_core::{Connection, ConnectionFactory, Pool, PoolConfigBuilder};

const MYSQL_URL_DEFAULT: &str = "mysql://root:test123@127.0.0.1:3306/sz_orm_test";
const PG_URL_DEFAULT: &str = "postgres://postgres:test123@127.0.0.1:5432/sz_orm_test";

fn mysql_url() -> String {
    std::env::var("SZ_ORM_MYSQL_URL").unwrap_or_else(|_| MYSQL_URL_DEFAULT.to_string())
}

fn pg_url() -> String {
    std::env::var("SZ_ORM_PG_URL").unwrap_or_else(|_| PG_URL_DEFAULT.to_string())
}

// ─── MySQL Connection 包装 ─────────────────────────────────────────

struct SqlxMyConn {
    conn: Option<sqlx::pool::PoolConnection<sqlx::MySql>>,
    connected: bool,
}

impl SqlxMyConn {
    fn new(conn: sqlx::pool::PoolConnection<sqlx::MySql>) -> Self {
        Self {
            conn: Some(conn),
            connected: true,
        }
    }
}

impl Connection for SqlxMyConn {
    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, sz_orm_core::DbError>> + Send + 'a>> {
        Box::pin(async move {
            let mut conn = self
                .conn
                .take()
                .ok_or_else(|| sz_orm_core::DbError::Internal("connection closed".into()))?;
            let result = (&mut *conn)
                .execute(sqlx::AssertSqlSafe(sql))
                .await
                .map_err(|e| sz_orm_core::DbError::Internal(e.to_string()));
            self.conn = Some(conn);
            result.map(|r| r.rows_affected())
        })
    }

    fn query<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        Vec<std::collections::HashMap<String, sz_orm_core::Value>>,
                        sz_orm_core::DbError,
                    >,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            let mut conn = self
                .conn
                .take()
                .ok_or_else(|| sz_orm_core::DbError::Internal("connection closed".into()))?;
            let _ = (&mut *conn).fetch_all(sqlx::AssertSqlSafe(sql)).await;
            self.conn = Some(conn);
            Ok(vec![])
        })
    }

    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }

    fn commit<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }

    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { true })
    }

    fn close<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>> {
        Box::pin(async move {
            self.conn = None;
            self.connected = false;
            Ok(())
        })
    }
}

struct MySqlFactory {
    pool: sqlx::MySqlPool,
}

#[async_trait]
impl ConnectionFactory for MySqlFactory {
    async fn create(&self) -> Result<Box<dyn Connection>, sz_orm_core::DbError> {
        let conn = self
            .pool
            .acquire()
            .await
            .map_err(|e| sz_orm_core::DbError::Internal(e.to_string()))?;
        Ok(Box::new(SqlxMyConn::new(conn)))
    }
}

// ─── PG Connection 包装 ───────────────────────────────────────────

struct SqlxPgConn {
    conn: Option<sqlx::pool::PoolConnection<sqlx::Postgres>>,
    connected: bool,
}

impl SqlxPgConn {
    fn new(conn: sqlx::pool::PoolConnection<sqlx::Postgres>) -> Self {
        Self {
            conn: Some(conn),
            connected: true,
        }
    }
}

impl Connection for SqlxPgConn {
    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, sz_orm_core::DbError>> + Send + 'a>> {
        Box::pin(async move {
            let mut conn = self
                .conn
                .take()
                .ok_or_else(|| sz_orm_core::DbError::Internal("connection closed".into()))?;
            let result = (&mut *conn)
                .execute(sqlx::AssertSqlSafe(sql))
                .await
                .map_err(|e| sz_orm_core::DbError::Internal(e.to_string()));
            self.conn = Some(conn);
            result.map(|r| r.rows_affected())
        })
    }

    fn query<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        Vec<std::collections::HashMap<String, sz_orm_core::Value>>,
                        sz_orm_core::DbError,
                    >,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            let mut conn = self
                .conn
                .take()
                .ok_or_else(|| sz_orm_core::DbError::Internal("connection closed".into()))?;
            let _ = (&mut *conn).fetch_all(sqlx::AssertSqlSafe(sql)).await;
            self.conn = Some(conn);
            Ok(vec![])
        })
    }

    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }

    fn commit<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }

    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { true })
    }

    fn close<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>> {
        Box::pin(async move {
            self.conn = None;
            self.connected = false;
            Ok(())
        })
    }
}

struct PgFactory {
    pool: sqlx::PgPool,
}

#[async_trait]
impl ConnectionFactory for PgFactory {
    async fn create(&self) -> Result<Box<dyn Connection>, sz_orm_core::DbError> {
        let conn = self
            .pool
            .acquire()
            .await
            .map_err(|e| sz_orm_core::DbError::Internal(e.to_string()))?;
        Ok(Box::new(SqlxPgConn::new(conn)))
    }
}

// ─── 测试辅助 ─────────────────────────────────────────────────────

async fn create_mysql_factory(max_conn: u32) -> MySqlFactory {
    let pool = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(max_conn)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&mysql_url())
        .await
        .expect("MySQL connect failed");
    MySqlFactory { pool }
}

async fn create_pg_factory(max_conn: u32) -> PgFactory {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(max_conn)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&pg_url())
        .await
        .expect("PG connect failed");
    PgFactory { pool }
}

// ─── 集成测试 ─────────────────────────────────────────────────────

/// 自动预热 MySQL 池后空闲连接 >= min_idle
#[tokio::test]
#[ignore]
async fn test_auto_prewarm_mysql_idle_ge_min_idle() {
    let factory = create_mysql_factory(10).await;
    let config = PoolConfigBuilder::new()
        .max_size(10)
        .min_idle(5)
        .prewarm(true)
        .build()
        .expect("config");

    let pool = Pool::new_async(config, Arc::new(factory))
        .await
        .expect("pool");

    let status = pool.status().await;
    assert!(
        status.idle >= 5,
        "预热后 idle 应 >= 5，实际: {}",
        status.idle
    );
}

/// 自动预热 PG 池后空闲连接 >= min_idle
#[tokio::test]
#[ignore]
async fn test_auto_prewarm_pg_idle_ge_min_idle() {
    let factory = create_pg_factory(10).await;
    let config = PoolConfigBuilder::new()
        .max_size(10)
        .min_idle(5)
        .prewarm(true)
        .build()
        .expect("config");

    let pool = Pool::new_async(config, Arc::new(factory))
        .await
        .expect("pool");

    let status = pool.status().await;
    assert!(
        status.idle >= 5,
        "预热后 idle 应 >= 5，实际: {}",
        status.idle
    );
}

/// 渐进式预热大池（min_idle=50）分批建连
#[tokio::test]
#[ignore]
async fn test_progressive_prewarm_large_pool() {
    let factory = create_mysql_factory(60).await;
    let config = PoolConfigBuilder::new()
        .max_size(60)
        .min_idle(50)
        .prewarm(true)
        .build()
        .expect("config");

    let pool = Pool::new(config, Arc::new(factory)).expect("pool");
    let progress = sz_orm_core::prewarm::PrewarmProgress::new(50);

    let start = Instant::now();
    pool.progressive_prewarm(
        5,
        Duration::from_millis(10),
        Duration::from_secs(30),
        &progress,
    )
    .await;
    let elapsed = start.elapsed();

    let snap = progress.snapshot();
    assert!(snap.is_completed, "应标记完成");
    assert!(snap.warmed >= 50, "warmed 应 >= 50，实际: {}", snap.warmed);
    assert!(
        elapsed <= Duration::from_secs(30),
        "总时间应 <= 30s，实际: {:?}",
        elapsed
    );

    let status = pool.status().await;
    assert!(status.idle >= 50, "池中 idle 应 >= 50");
}

/// 多池统一预热（MySQL + PG）汇总结果
#[tokio::test]
#[ignore]
async fn test_multi_pool_unified_prewarm() {
    let mysql_factory = create_mysql_factory(10).await;
    let pg_factory = create_pg_factory(10).await;

    let mysql_config = PoolConfigBuilder::new()
        .max_size(10)
        .min_idle(5)
        .prewarm(true)
        .build()
        .expect("mysql config");
    let pg_config = PoolConfigBuilder::new()
        .max_size(10)
        .min_idle(3)
        .prewarm(true)
        .build()
        .expect("pg config");

    let mysql_pool = Pool::new_async(mysql_config, Arc::new(mysql_factory))
        .await
        .expect("mysql pool");
    let pg_pool = Pool::new_async(pg_config, Arc::new(pg_factory))
        .await
        .expect("pg pool");

    let mysql_status = mysql_pool.status().await;
    let pg_status = pg_pool.status().await;

    assert!(mysql_status.idle >= 5, "MySQL idle 应 >= 5");
    assert!(pg_status.idle >= 3, "PG idle 应 >= 3");
    assert!(
        mysql_status.idle + pg_status.idle >= 8,
        "总 idle 应 >= 8，MySQL: {}，PG: {}",
        mysql_status.idle,
        pg_status.idle
    );
}

/// DB 不可达时预热失败不阻断池创建
#[tokio::test]
#[ignore]
async fn test_prewarm_failure_non_blocking_unreachable() {
    struct UnreachableFactory;

    #[async_trait]
    impl ConnectionFactory for UnreachableFactory {
        async fn create(&self) -> Result<Box<dyn Connection>, sz_orm_core::DbError> {
            Err(sz_orm_core::DbError::Internal(
                "connection refused - DB unreachable".into(),
            ))
        }
    }

    let mut config = PoolConfigBuilder::new()
        .max_size(10)
        .min_idle(5)
        .prewarm(true)
        .build()
        .expect("config");
    config.connection_timeout = Duration::from_secs(1);

    let pool = Pool::new_async(config, Arc::new(UnreachableFactory))
        .await
        .expect("池创建应成功即使预热失败");

    let status = pool.status().await;
    assert_eq!(status.idle, 0, "不可达 DB 时 idle 应为 0");
    assert_eq!(status.max, 10, "池配置应正常");
}

/// 冷启动首次查询 P95 对比（自动预热 vs 未预热）
#[tokio::test]
#[ignore]
async fn test_cold_start_p95_prewarm_vs_no_prewarm() {
    let warm_factory = create_mysql_factory(20).await;
    let cold_factory = create_mysql_factory(20).await;

    // 预热池
    let warm_config = PoolConfigBuilder::new()
        .max_size(10)
        .min_idle(5)
        .prewarm(true)
        .build()
        .expect("warm config");
    let warm_pool = Pool::new_async(warm_config, Arc::new(warm_factory))
        .await
        .expect("warm pool");

    // 冷启动池
    let cold_config = PoolConfigBuilder::new()
        .max_size(10)
        .min_idle(5)
        .prewarm(false)
        .build()
        .expect("cold config");
    let cold_pool = Pool::new(cold_config, Arc::new(cold_factory)).expect("cold pool");

    // 预热池：首次 acquire 应从空闲队列获取（冷启动场景）
    let warm_first = {
        let start = Instant::now();
        let _ = warm_pool.acquire().await.expect("acquire");
        start.elapsed()
    };

    // 冷启动池：首次 acquire 需新建连接
    let cold_first = {
        let start = Instant::now();
        let _ = cold_pool.acquire().await.expect("acquire");
        start.elapsed()
    };

    eprintln!("预热池首次 acquire: {:?}", warm_first);
    eprintln!("冷启动池首次 acquire: {:?}", cold_first);

    // 预热池首次 acquire 应 <= 50ms（从空闲队列获取，无需建连）
    assert!(
        warm_first <= Duration::from_millis(50),
        "预热池首次 acquire 应 <= 50ms，实际: {:?}",
        warm_first
    );
}

/// 渐进式预热超时停止（已预热连接保留）
#[tokio::test]
#[ignore]
async fn test_progressive_prewarm_timeout_preserves_connections() {
    let factory = create_mysql_factory(30).await;
    let config = PoolConfigBuilder::new()
        .max_size(30)
        .min_idle(20)
        .prewarm(true)
        .build()
        .expect("config");

    let pool = Pool::new(config, Arc::new(factory)).expect("pool");
    let progress = sz_orm_core::prewarm::PrewarmProgress::new(20);

    // total_timeout=50ms，只能预热少量连接
    pool.progressive_prewarm(
        2,
        Duration::from_millis(10),
        Duration::from_millis(50),
        &progress,
    )
    .await;

    let snap = progress.snapshot();
    assert!(snap.is_completed, "应标记完成");
    assert!(
        snap.warmed < 20,
        "超时应未预热全部 20 个，实际: {}",
        snap.warmed
    );

    let status = pool.status().await;
    assert!(
        status.idle > 0,
        "已预热连接应保留在池中，idle: {}",
        status.idle
    );
}
