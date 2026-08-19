//! SZ-ORM sqlx adapter
//!
//! Provides Connection and ConnectionFactory implementations for sz-orm-core,
//! supporting MySQL, PostgreSQL, and SQLite.
//!
//! Does not use sqlx::Any; instead implements each backend separately to avoid type limitations and lifetime issues.
//!
//! # Examples
//!
//! ```no_run
//! use sz_orm_core::{Pool, PoolConfigBuilder};
//! use sz_orm_sqlx::{SqlitePoolHandle, SqlxSqliteConnectionFactory};
//! use std::sync::Arc;
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let pool_handle = SqlitePoolHandle::connect("sqlite::memory:").await?;
//! let factory = Arc::new(SqlxSqliteConnectionFactory::new(Arc::new(pool_handle)));
//! let config = PoolConfigBuilder::new().max_size(10).build()?;
//! let pool = Pool::new(config, factory)?;
//!
//! let mut conn = pool.acquire().await?;
//! let rows = conn.query("SELECT 1 as one").await?;
//! assert_eq!(rows.len(), 1);
//! # Ok(())
//! # }
//! ```

mod any;
pub mod any_driver;
pub mod enhanced;
mod error;
#[cfg(feature = "dialect-saphana-driver")]
pub mod saphana_adapter;
pub mod unified_pool;

pub use any::{
    mysql_bulk_insert, pg_bulk_insert, sqlite_backup, MySqlPoolHandle, PgExtensions, PgPoolHandle,
    SqlitePoolHandle, SqlxMySqlConnection, SqlxMySqlConnectionFactory, SqlxPgConnection,
    SqlxPgConnectionFactory, SqlxSqliteConnection, SqlxSqliteConnectionFactory,
};
pub use any_driver::{AnyBackend, AnyConnection, AnyPool};
pub use enhanced::{
    CacheStats, EnhancedPoolConfig, EnhancedPoolConfigBuilder, PreparedStatementCache,
    TransactionIsolation,
};
pub use error::map_sqlx_error;
pub use unified_pool::UnifiedPool;

pub use sz_orm_core;
