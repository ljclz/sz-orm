//! SZ-ORM TimescaleDB Extension
//!
//! Provides time-series data storage, querying, and aggregation capabilities, supporting three implementations:
//!
//! - **In-memory implementation** (`Memory`): Pure Rust time-series data storage, no database connection
//! - **Stub implementation** (`Stub`): Generates TimescaleDB SQL string but does not execute
//! - **Real implementation** (`RealTimescale`, requires `real-timescale` feature): Connects to TimescaleDB via tokio-postgres
//!
//! # Supported Operations
//!
//! | Method | SQL equivalent | Description |
//! |------|---------|------|
//! | `create_hypertable` | `SELECT create_hypertable(...)` | Create hypertable |
//! | `insert_metric` | `INSERT INTO ...` | Insert metric |
//! | `query_range` | `SELECT ... WHERE timestamp BETWEEN ...` | Range query |
//! | `time_bucket_aggregate` | `SELECT time_bucket(...), AGG(value) ...` | Time bucket aggregation |
//! | `create_continuous_aggregate` | `CREATE MATERIALIZED VIEW ...` | Continuous aggregate view |
//! | `downsample` | `INSERT INTO target SELECT time_bucket ...` | Downsample |
//! | `drop_metric` | `DROP TABLE ...` | Drop metric |
//!
//! # Quick Start
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

/// M-17 fix: Maximum time range span for queries (seconds)
///
/// Limits single `query_range` / `time_bucket_aggregate` time range to not exceed 366 days,
/// preventing caller misuse (e.g. querying 100 years of data) from causing OOM or database performance issues.
///
/// - 366 days ≈ 1 year (including leap year), sufficient for common monitoring/analysis scenarios
/// - 366 * 86400 = 31,622,400 seconds
pub const MAX_QUERY_RANGE_SECS: i64 = 366 * 86400;

/// M-17 fix: Validate whether time range is within reasonable span
///
/// - `start >= end`: Returns `InvalidTimeRange` error
/// - `(end - start) > MAX_QUERY_RANGE_SECS`: Returns `InvalidTimeRange` error
/// - Other cases: Returns Ok
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
