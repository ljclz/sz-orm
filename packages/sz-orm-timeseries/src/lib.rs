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
pub mod extensions;
pub mod timeseries;
pub mod types;

pub mod memory;
mod safety;
pub mod stub;

#[cfg(feature = "real-timescale")]
pub mod real_timescale;

pub use error::TimescaleError;
pub use extensions::{
    parse_bucket_to_secs, secs_to_bucket_string, CompressionConfig, CompressionPolicyRegistry,
    CompressionStats, CompressionStatus, ContinuousAggregateDef, ContinuousAggregateRegistry,
    GapfillFiller, GapfillStrategy, RefreshPolicy, RetentionPolicy, RetentionPolicyRegistry,
    TimeBucketAligner,
};
pub use memory::MemoryTimeseries;
pub use stub::StubTimeseries;
pub use timeseries::{TimeseriesBuilder, TimeseriesExt, TimeseriesProvider, TimeseriesWrapper};
pub use types::{Aggregation, DownsampleConfig, Metric, TimeBucket};

#[cfg(feature = "real-timescale")]
pub use timeseries::RealTimescaleConfig;

#[cfg(feature = "real-timescale")]
pub use real_timescale::RealTimescale;

/// M-17 修复：查询时间范围最大跨度（秒）
///
/// 限制单次 `query_range` / `time_bucket_aggregate` 的时间范围不能超过 366 天，
/// 防止调用方误用（如查询 100 年的数据）导致 OOM 或数据库性能问题。
///
/// - 366 天 ≈ 1 年（含闰年），足以覆盖常见的监控/分析场景
/// - 366 * 86400 = 31,622,400 秒
pub const MAX_QUERY_RANGE_SECS: i64 = 366 * 86400;

/// M-17 修复：校验时间范围是否在合理跨度内
///
/// - `start >= end`：返回 `InvalidTimeRange` 错误
/// - `(end - start) > MAX_QUERY_RANGE_SECS`：返回 `InvalidTimeRange` 错误
/// - 其他情况：返回 Ok
pub fn validate_time_range(
    start: chrono::DateTime<chrono::Utc>,
    end: chrono::DateTime<chrono::Utc>,
) -> Result<(), TimescaleError> {
    if start >= end {
        return Err(TimescaleError::InvalidTimeRange {
            start: start.to_rfc3339(),
            end: end.to_rfc3339(),
        });
    }
    let duration_secs = (end - start).num_seconds();
    if duration_secs > MAX_QUERY_RANGE_SECS {
        return Err(TimescaleError::InvalidTimeRange {
            start: start.to_rfc3339(),
            end: end.to_rfc3339(),
        });
    }
    Ok(())
}
