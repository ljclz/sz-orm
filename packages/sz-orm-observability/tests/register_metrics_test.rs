#![cfg(feature = "prometheus-exporter")]

use sz_orm_observability::prometheus_exporter::{register_sz_orm_metrics, PrometheusExporter};

#[test]
fn test_register_sz_orm_metrics() {
    let exporter = PrometheusExporter::new();
    register_sz_orm_metrics(&exporter);
    let output = exporter.export();
    assert!(output.contains("sz_orm_pool_active"));
    assert!(output.contains("sz_orm_cache_l1_hit_rate"));
    assert!(output.contains("sz_orm_query_qps"));
    assert!(output.contains("sz_orm_circuit_state"));
    assert!(output.contains("sz_orm_ai_decision_latency_ms"));
    assert!(output.contains("sz_orm_tracing_span_count"));
}

#[test]
fn test_metrics_no_sensitive_labels() {
    let exporter = PrometheusExporter::new();
    register_sz_orm_metrics(&exporter);
    let output = exporter.export();
    assert!(!output.contains("password"));
    assert!(!output.contains("token"));
    assert!(!output.contains("secret"));
}
