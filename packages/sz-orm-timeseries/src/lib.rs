//! SZ-ORM TimescaleDB 扩展
//!
//! 提供时序数据存储、查询和聚合能力，支持三种实现：
//!
//! - **内存实现**（`Memory`）：纯 Rust 时序数据存储，不连接数据库
//! - **Stub 实现**（`Stub`）：生成 TimescaleDB SQL 字符串但不执行
//! - **真实实现**（`RealTimescale`，需启用 `real-timescale` feature）：通过 tokio-postgres 连接 TimescaleDB
//!
//! # 支持的操作
//!
//! | 方法 | SQL 等价 | 说明 |
//! |------|---------|------|
//! | `create_hypertable` | `SELECT create_hypertable(...)` | 创建 hypertable |
//! | `insert_metric` | `INSERT INTO ...` | 插入指标 |
//! | `query_range` | `SELECT ... WHERE timestamp BETWEEN ...` | 范围查询 |
//! | `time_bucket_aggregate` | `SELECT time_bucket(...), AGG(value) ...` | 时间桶聚合 |
//! | `create_continuous_aggregate` | `CREATE MATERIALIZED VIEW ...` | 连续聚合视图 |
//! | `downsample` | `INSERT INTO target SELECT time_bucket ...` | 降采样 |
//! | `drop_metric` | `DROP TABLE ...` | 删除指标 |
//!
//! # 快速入门
//!
//! ```rust
//! use sz_orm_timeseries::{TimeseriesBuilder, TimeseriesExt, TimeseriesProvider, Metric, Aggregation};
//! use chrono::{Utc, TimeZone};
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let wrapper = TimeseriesBuilder::new(TimeseriesProvider::Memory).build()?;
//! wrapper.create_hypertable("cpu_usage", "ts").await?;
//!
//! let now = Utc::now();
//! wrapper.insert_metric(&Metric::new("cpu_usage", now, 0.75)).await?;
//!
//! let buckets = wrapper.time_bucket_aggregate(
//!     "cpu_usage", "1m", Aggregation::Avg,
//!     now - chrono::Duration::minutes(5), now
//! ).await?;
//! println!("buckets: {}", buckets.len());
//! # Ok(())
//! # }
//! ```

pub mod error;
pub mod timeseries;
pub mod types;

pub mod memory;
pub mod stub;

#[cfg(feature = "real-timescale")]
pub mod real_timescale;

pub use error::TimescaleError;
pub use memory::MemoryTimeseries;
pub use stub::StubTimeseries;
pub use timeseries::{TimeseriesBuilder, TimeseriesExt, TimeseriesProvider, TimeseriesWrapper};
pub use types::{Aggregation, DownsampleConfig, Metric, TimeBucket};

#[cfg(feature = "real-timescale")]
pub use timeseries::RealTimescaleConfig;

#[cfg(feature = "real-timescale")]
pub use real_timescale::RealTimescale;
