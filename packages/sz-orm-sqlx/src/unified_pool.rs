//! UnifiedPool — 统一连接池抽象（v2.2.0 A-3）
//!
//! 包装 `sz_orm_core::Pool`（完整连接池）+ `AnyBackend`，提供 5 后端透明的统一类型。
//! 供 sz-rust AppState 持有单一类型 `Arc<UnifiedPool>`，业务代码无需感知后端类型。
//!
//! # 设计
//!
//! - `UnifiedPool` 是 `Pool` 的 newtype 包装，所有方法委托 `Pool`（零能力丢失）
//! - `from_pool` 提供零成本迁移路径：sz-rust 从 `Arc<Pool>` 迁移到 `Arc<UnifiedPool>`
//! - `connect`/`connect_with_config` 根据 DSN 自动识别后端并创建完整连接池
//!
//! # 用法
//!
//! ```ignore
//! use sz_orm_sqlx::UnifiedPool;
//!
//! // 从 DSN 自动识别后端，创建完整连接池
//! let pool = UnifiedPool::connect("mysql://root:pass@127.0.0.1/db").await?;
//! let mut conn = pool.acquire().await?;
//!
//! // 运行时切换后端
//! let pg_pool = UnifiedPool::connect("postgres://user:pass@127.0.0.1/db").await?;
//! let d = pg_pool.dialect(); // 自动返回 PostgreSqlDialect
//! ```

use std::sync::Arc;
use sz_orm_core::{
    ConnectionFactory, DbError, Dialect, Pool, PoolConfig, PoolConfigBuilder, PoolError,
    PoolStatus, PooledConnection,
};

use crate::any::{
    MySqlPoolHandle, PgPoolHandle, SqlitePoolHandle, SqlxMySqlConnectionFactory,
    SqlxPgConnectionFactory, SqlxSqliteConnectionFactory,
};
use crate::any_driver::AnyBackend;

#[cfg(feature = "oracle")]
use sz_orm_oracle::{OracleConnectionFactory, OraclePoolHandle};

#[cfg(feature = "mssql")]
use sz_orm_mssql::{MssqlConnectionFactory, MssqlPoolHandle};

/// 统一连接池：包装 `Pool` + `AnyBackend`，5 后端透明切换（v2.2.0 新增）
///
/// 供 sz-rust AppState 持有 `Arc<UnifiedPool>`，业务代码无需感知后端类型。
/// 所有方法委托内部 `Pool`，零能力丢失。
pub struct UnifiedPool {
    backend: AnyBackend,
    pool: Pool,
}

impl UnifiedPool {
    /// 连接数据库，根据 DSN 自动识别后端，使用默认 PoolConfig
    ///
    /// 默认配置：max_size=10, timeout=30s
    pub async fn connect(dsn: &str) -> Result<Self, DbError> {
        let config = PoolConfigBuilder::new()
            .build()
            .map_err(DbError::PoolError)?;
        Self::connect_with_config(dsn, config).await
    }

    /// 连接数据库，根据 DSN 自动识别后端，使用自定义 PoolConfig
    pub async fn connect_with_config(dsn: &str, config: PoolConfig) -> Result<Self, DbError> {
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
                    let (username, password, connect_string) =
                        crate::any_driver::parse_oracle_dsn(dsn)?;
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
                    let ado_string = crate::any_driver::parse_mssql_dsn(dsn)?;
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
        let pool = Pool::new(config, factory).map_err(DbError::PoolError)?;
        Ok(Self { backend, pool })
    }

    /// 从已有的 Pool 构造 UnifiedPool（零成本迁移）
    ///
    /// 供 sz-rust 从 `Arc<Pool>` 迁移到 `Arc<UnifiedPool>`：
    /// ```ignore
    /// let unified = UnifiedPool::from_pool(existing_pool, AnyBackend::MySql);
    /// ```
    pub fn from_pool(pool: Pool, backend: AnyBackend) -> Self {
        Self { backend, pool }
    }

    /// 获取后端类型
    #[inline]
    pub fn backend(&self) -> AnyBackend {
        self.backend
    }

    /// 返回对应后端的 Dialect 实例
    #[inline]
    pub fn dialect(&self) -> Box<dyn Dialect> {
        self.backend.dialect()
    }

    /// 获取连接（委托 Pool::acquire）
    #[inline]
    pub async fn acquire(&self) -> Result<PooledConnection, PoolError> {
        self.pool.acquire().await
    }

    /// 调整连接池大小（委托 Pool::resize）
    #[inline]
    pub fn resize(&self, new_max: usize) {
        self.pool.resize(new_max);
    }

    /// 关闭所有连接（委托 Pool::close_all）
    #[inline]
    pub async fn close_all(&self) {
        self.pool.close_all().await;
    }

    /// 获取连接池状态（委托 Pool::status）
    #[inline]
    pub async fn status(&self) -> PoolStatus {
        self.pool.status().await
    }

    /// 预热连接池（委托 Pool::prewarm）
    #[inline]
    pub async fn prewarm(&self) {
        self.pool.prewarm().await;
    }

    /// v3.2.0：渐进式分批预热（委托 Pool::progressive_prewarm）
    #[cfg(feature = "auto-prewarm")]
    pub async fn progressive_prewarm(
        &self,
        batch_size: u32,
        interval: std::time::Duration,
        total_timeout: std::time::Duration,
        progress: &sz_orm_core::prewarm::PrewarmProgress,
    ) {
        self.pool
            .progressive_prewarm(batch_size, interval, total_timeout, progress)
            .await;
    }
}

/// v3.2.0：多池注册表 — 统一管理多个后端的 UnifiedPool
#[cfg(feature = "auto-prewarm")]
pub struct MultiPoolRegistry {
    pools: Vec<(String, UnifiedPool)>,
}

#[cfg(feature = "auto-prewarm")]
impl MultiPoolRegistry {
    pub fn new() -> Self {
        Self { pools: Vec::new() }
    }

    pub fn register(&mut self, name: impl Into<String>, pool: UnifiedPool) {
        self.pools.push((name.into(), pool));
    }

    /// 并行预热所有注册的池
    pub async fn unified_prewarm_all(&self) -> sz_orm_core::prewarm::PrewarmSummary {
        use std::time::Instant;
        use sz_orm_core::prewarm::{BackendPrewarmResult, PrewarmSummary};

        let mut summary = PrewarmSummary::new();
        for (name, pool) in &self.pools {
            let start = Instant::now();
            let min_idle = pool.pool.config().min_idle;
            let progress = sz_orm_core::prewarm::PrewarmProgress::new(min_idle);
            pool.prewarm().await;
            let snap = progress.snapshot();
            summary.add(BackendPrewarmResult {
                backend: name.clone(),
                warmed: snap.warmed,
                failed: snap.failed,
                elapsed: start.elapsed(),
                errors: vec![],
            });
        }
        summary
    }
}

#[cfg(feature = "auto-prewarm")]
impl Default for MultiPoolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for UnifiedPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnifiedPool")
            .field("backend", &self.backend)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_unified_pool_sqlite_connect() {
        let pool = UnifiedPool::connect("sqlite::memory:").await.unwrap();
        assert_eq!(pool.backend(), AnyBackend::Sqlite);

        let mut conn = pool.acquire().await.unwrap();
        conn.execute("CREATE TABLE t (id INTEGER PRIMARY KEY)")
            .await
            .unwrap();
        conn.execute("INSERT INTO t (id) VALUES (1)").await.unwrap();
        let rows = conn.query("SELECT * FROM t").await.unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[tokio::test]
    async fn test_unified_pool_dialect() {
        let pool = UnifiedPool::connect("sqlite::memory:").await.unwrap();
        let d = pool.dialect();
        assert_eq!(d.db_type(), sz_orm_core::DbType::Sqlite);
    }

    #[tokio::test]
    async fn test_unified_pool_from_pool() {
        let handle = Arc::new(SqlitePoolHandle::connect("sqlite::memory:").await.unwrap());
        let factory = Arc::new(SqlxSqliteConnectionFactory::new(handle));
        let config = PoolConfigBuilder::new().build().unwrap();
        let pool = Pool::new(config, factory).unwrap();

        let unified = UnifiedPool::from_pool(pool, AnyBackend::Sqlite);
        assert_eq!(unified.backend(), AnyBackend::Sqlite);

        let mut conn = unified.acquire().await.unwrap();
        conn.execute("SELECT 1").await.unwrap();
    }

    #[tokio::test]
    async fn test_unified_pool_resize_and_close() {
        let pool = UnifiedPool::connect("sqlite::memory:").await.unwrap();
        pool.resize(20);
        let status = pool.status().await;
        assert_eq!(status.max, 20);
        pool.close_all().await;
    }

    #[tokio::test]
    async fn test_unified_pool_invalid_dsn() {
        let result = UnifiedPool::connect("invalid://dsn").await;
        assert!(result.is_err());
    }
}
