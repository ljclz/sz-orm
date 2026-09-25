use sz_orm_timeseries::TimescaleError;
use std::error::Error;

#[test]
fn test_not_found() {
    let err = TimescaleError::NotFound("metric".into());
    assert_eq!(err.to_string(), "not found: metric");
}

#[test]
fn test_invalid_time_range() {
    let err = TimescaleError::InvalidTimeRange { start: "2026-01-02".into(), end: "2026-01-01".into() };
    assert_eq!(err.to_string(), "invalid time range: start 2026-01-02 >= end 2026-01-01");
}

#[test]
fn test_unsupported_aggregation() {
    let err = TimescaleError::UnsupportedAggregation("median".into());
    assert_eq!(err.to_string(), "unsupported aggregation: median");
}

#[test]
fn test_query_error() {
    let err = TimescaleError::Query("fail".into());
    assert_eq!(err.to_string(), "query error: fail");
}

#[test]
fn test_connection_error() {
    let err = TimescaleError::Connection("refused".into());
    assert_eq!(err.to_string(), "connection error: refused");
}

#[test]
fn test_invalid_config() {
    let err = TimescaleError::InvalidConfig("bad".into());
    assert_eq!(err.to_string(), "invalid config: bad");
}

#[test]
fn test_from_io_error() {
    let io_err = std::io::Error::new(std::io::ErrorKind::Other, "test");
    let err: TimescaleError = io_err.into();
    assert!(err.to_string().contains("query error"));
}

#[test]
fn test_error_trait_impl() {
    let err = TimescaleError::NotFound("x".into());
    assert!(err.source().is_none());
}