//! Timeseries 适配层端到端测试
use chrono::Utc;
use sz_orm_core::timeseries_adapter::{
    timeseries_insert_metric, timeseries_query_count, timeseries_query_range,
};
use sz_orm_timeseries::Metric;

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
