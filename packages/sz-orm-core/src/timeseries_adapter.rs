//! # Timeseries Adapter — sz-orm-core 时序数据库适配层
//!
//! v5.0.0 M4：将 sz-orm-timeseries 的 MemoryTimeseries 接入 sz-orm-core，
//! 提供 `timeseries_insert_metric` / `timeseries_query_range` / `timeseries_query_count` 入口。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use chrono::Utc;
use parking_lot::RwLock;
use sz_orm_timeseries::{MemoryTimeseries, Metric, TimeseriesExt};

static TS: OnceLock<RwLock<MemoryTimeseries>> = OnceLock::new();
static QUERY_COUNT: AtomicU64 = AtomicU64::new(0);
static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

fn timeseries() -> &'static RwLock<MemoryTimeseries> {
    TS.get_or_init(|| RwLock::new(MemoryTimeseries::new()))
}

fn runtime() -> &'static tokio::runtime::Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
    })
}

/// 插入指标
pub fn timeseries_insert_metric(metric: &Metric) -> Result<(), sz_orm_timeseries::TimescaleError> {
    QUERY_COUNT.fetch_add(1, Ordering::Relaxed);
    let ts = timeseries().read();
    runtime().block_on(ts.insert_metric(metric))
}

/// 范围查询
pub fn timeseries_query_range(
    metric: &str,
    start: chrono::DateTime<Utc>,
    end: chrono::DateTime<Utc>,
) -> Result<Vec<Metric>, sz_orm_timeseries::TimescaleError> {
    QUERY_COUNT.fetch_add(1, Ordering::Relaxed);
    let ts = timeseries().read();
    runtime().block_on(ts.query_range(metric, start, end))
}

/// 获取查询计数
pub fn timeseries_query_count() -> u64 {
    QUERY_COUNT.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeseries_insert_and_query() {
        let metric = Metric::new("cpu", Utc::now(), 42.0);
        let _ = timeseries_insert_metric(&metric);
        let start = Utc::now() - chrono::Duration::hours(1);
        let end = Utc::now() + chrono::Duration::hours(1);
        let result = timeseries_query_range("cpu", start, end);
        assert!(result.is_ok(), "query_range should return Ok");
    }

    #[test]
    fn test_timeseries_count_increments() {
        let before = timeseries_query_count();
        let metric = Metric::new("mem", Utc::now(), 100.0);
        let _ = timeseries_insert_metric(&metric);
        let after = timeseries_query_count();
        assert!(after > before);
    }
}
