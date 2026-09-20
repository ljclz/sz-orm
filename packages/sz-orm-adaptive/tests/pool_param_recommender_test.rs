//! v7.5.0 PoolParamRecommender 端到端测试

use sz_orm_adaptive::param_tuner::{PoolParamRecommender, WorkloadStats};

#[test]
fn test_recommend_normal_workload() {
    let stats = WorkloadStats {
        avg_concurrent_queries: 20.0,
        avg_query_duration_ms: 50.0,
        peak_qps: 500.0,
        db_max_connections: 100,
    };
    let params = PoolParamRecommender::recommend(&stats);
    assert!(params.capacity >= 4);
    assert!(params.capacity <= 80);
    assert!(!params.rationale.is_empty());
}

#[test]
fn test_recommend_high_concurrency() {
    let stats = WorkloadStats {
        avg_concurrent_queries: 100.0,
        avg_query_duration_ms: 200.0,
        peak_qps: 2000.0,
        db_max_connections: 200,
    };
    let params = PoolParamRecommender::recommend(&stats);
    assert!(params.acquire_timeout_ms >= 5000);
    assert!(params.health_check_interval_secs <= 10);
}

#[test]
fn test_recommend_low_concurrency() {
    let stats = WorkloadStats {
        avg_concurrent_queries: 5.0,
        avg_query_duration_ms: 10.0,
        peak_qps: 50.0,
        db_max_connections: 50,
    };
    let params = PoolParamRecommender::recommend(&stats);
    assert!(params.acquire_timeout_ms <= 5000);
    assert!(params.health_check_interval_secs >= 10);
}

#[test]
fn test_recommend_caps_at_db_limit() {
    let stats = WorkloadStats {
        avg_concurrent_queries: 200.0,
        avg_query_duration_ms: 1000.0,
        peak_qps: 10000.0,
        db_max_connections: 20,
    };
    let params = PoolParamRecommender::recommend(&stats);
    assert!(params.capacity <= 16);
    assert!(params.rationale.contains("POOL_TUNING_UNSAFE") || params.capacity <= 16);
}

#[test]
fn test_recommend_rationale_explains() {
    let stats = WorkloadStats {
        avg_concurrent_queries: 10.0,
        avg_query_duration_ms: 100.0,
        peak_qps: 200.0,
        db_max_connections: 100,
    };
    let params = PoolParamRecommender::recommend(&stats);
    assert!(params.rationale.contains("capacity="));
    assert!(params.rationale.contains("peak_qps="));
}

#[test]
fn test_recommend_long_query_idle_timeout() {
    let stats = WorkloadStats {
        avg_concurrent_queries: 10.0,
        avg_query_duration_ms: 600.0,
        peak_qps: 100.0,
        db_max_connections: 100,
    };
    let params = PoolParamRecommender::recommend(&stats);
    assert_eq!(params.idle_timeout_secs, 60);
}

#[test]
fn test_recommend_short_query_idle_timeout() {
    let stats = WorkloadStats {
        avg_concurrent_queries: 10.0,
        avg_query_duration_ms: 100.0,
        peak_qps: 100.0,
        db_max_connections: 100,
    };
    let params = PoolParamRecommender::recommend(&stats);
    assert_eq!(params.idle_timeout_secs, 300);
}
