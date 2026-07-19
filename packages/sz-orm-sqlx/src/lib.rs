//! SZ-ORM sqlx 适配器
//!
//! 为 sz-orm-core 提供 Connection 和 ConnectionFactory 的实现，
//! 支持 MySQL、PostgreSQL、SQLite 三种数据库。
//!
//! 不使用 sqlx::Any，而为每种后端单独实现，避免类型限制和生命周期问题。
//!
//! # 示例
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
mod error;

pub use any::{
    MySqlPoolHandle, PgPoolHandle, SqlitePoolHandle, SqlxMySqlConnection,
    SqlxMySqlConnectionFactory, SqlxPgConnection, SqlxPgConnectionFactory, SqlxSqliteConnection,
    SqlxSqliteConnectionFactory,
};
pub use error::map_sqlx_error;

pub use sz_orm_core;
