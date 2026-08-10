use sz_orm_health::*;

#[test]
fn test_health_status_default() {
    let status = HealthStatus::default();
    assert_eq!(status, HealthStatus::Unknown);
}

#[test]
fn test_health_status_equality() {
    assert_eq!(HealthStatus::Healthy, HealthStatus::Healthy);
    assert_ne!(HealthStatus::Healthy, HealthStatus::Unhealthy);
    assert_ne!(HealthStatus::Unhealthy, HealthStatus::Unknown);
}

#[test]
fn test_health_report_new() {
    let report = HealthReport::new("pool1");
    assert_eq!(report.pool_name, "pool1");
    assert_eq!(report.status, HealthStatus::Unknown);
    assert_eq!(report.connection_count, 0);
    assert_eq!(report.slow_queries, 0);
}

#[test]
fn test_health_report_set_healthy() {
    let report = HealthReport::new("pool1").set_healthy();
    assert_eq!(report.status, HealthStatus::Healthy);
}

#[test]
fn test_health_report_set_status() {
    let report = HealthReport::new("pool1").set_status(HealthStatus::Unhealthy);
    assert_eq!(report.status, HealthStatus::Unhealthy);
}

#[test]
fn test_health_report_with_message() {
    let report = HealthReport::new("pool1").set_healthy().with_message("OK");
    assert_eq!(report.message, "OK");
}

#[test]
fn test_health_report_with_connection_count() {
    let report = HealthReport::new("pool1")
        .set_healthy()
        .with_connection_count(10);
    assert_eq!(report.connection_count, 10);
}

#[test]
fn test_health_report_with_slow_queries() {
    let report = HealthReport::new("pool1")
        .set_healthy()
        .with_slow_queries(5);
    assert_eq!(report.slow_queries, 5);
}

#[test]
fn test_health_report_serialization() {
    let report = HealthReport::new("pool1").set_healthy();
    let json = serde_json::to_string(&report).unwrap();
    let deserialized: HealthReport = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.pool_name, "pool1");
    assert_eq!(deserialized.status, HealthStatus::Healthy);
}

#[test]
fn test_health_report_with_sla_metrics() {
    let report = HealthReport::new("pool1")
        .set_healthy()
        .with_error_rate(0.01)
        .with_latency_p50(10.5)
        .with_latency_p95(50.0)
        .with_latency_p99(100.0);
    assert_eq!(report.error_rate, Some(0.01));
    assert_eq!(report.p50_ms, Some(10.5));
    assert_eq!(report.p95_ms, Some(50.0));
    assert_eq!(report.p99_ms, Some(100.0));
}

#[test]
fn test_health_report_with_saturation() {
    let report = HealthReport::new("pool1")
        .set_healthy()
        .with_saturation(0.8);
    assert_eq!(report.saturation, Some(0.8));
}

#[test]
fn test_health_report_with_uptime_ratio() {
    let report = HealthReport::new("pool1")
        .set_healthy()
        .with_uptime_ratio(0.999);
    assert_eq!(report.uptime_ratio, Some(0.999));
}

#[test]
fn test_health_report_chain_builders() {
    let report = HealthReport::new("pool")
        .set_healthy()
        .with_connection_count(5)
        .with_slow_queries(2)
        .with_message("all good")
        .with_error_rate(0.0)
        .with_latency_p50(1.0)
        .with_latency_p95(5.0)
        .with_latency_p99(10.0)
        .with_saturation(0.5)
        .with_uptime_ratio(1.0);
    assert_eq!(report.pool_name, "pool");
    assert_eq!(report.status, HealthStatus::Healthy);
    assert_eq!(report.connection_count, 5);
    assert_eq!(report.slow_queries, 2);
    assert_eq!(report.message, "all good");
}
