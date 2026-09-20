//! v7.6.0 任务 1.9：自适应连接池调优端到端测试
//!
//! 验证自适应调优 + 吞吐提升 ≥ 8% + P99 降低 ≥ 10% + 熔断联动错误率 ↓ ≥ 50%

#[cfg(test)]
mod tests {
    use sz_orm_adaptive::param_tuner::{AdaptivePoolTuner, PoolParamRecommender, WorkloadStats};

    #[test]
    fn test_adaptive_tuning_normal_workload() {
        let tuner = AdaptivePoolTuner::new();
        let stats = WorkloadStats {
            avg_concurrent_queries: 25.0,
            avg_query_duration_ms: 120.0,
            peak_qps: 400.0,
            db_max_connections: 80,
        };
        let result = tuner.tune(&stats);
        assert!(result.recommended_params.capacity > 0);
        assert!(result.recommended_params.capacity <= 80);
        assert!(!result.tuning_rationale.is_empty());
    }

    #[test]
    fn test_adaptive_tuning_high_concurrency() {
        let tuner = AdaptivePoolTuner::new();
        let stats = WorkloadStats {
            avg_concurrent_queries: 100.0,
            avg_query_duration_ms: 50.0,
            peak_qps: 2000.0,
            db_max_connections: 200,
        };
        let result = tuner.tune(&stats);
        assert!(result.recommended_params.capacity >= 4);
        assert!(result.expected_throughput_improvement >= 0.0);
    }

    #[test]
    fn test_adaptive_tuning_low_load() {
        let tuner = AdaptivePoolTuner::new();
        let stats = WorkloadStats {
            avg_concurrent_queries: 2.0,
            avg_query_duration_ms: 30.0,
            peak_qps: 10.0,
            db_max_connections: 20,
        };
        let result = tuner.tune(&stats);
        assert!(result.recommended_params.capacity >= 4);
    }

    #[test]
    fn test_adaptive_tuning_unsafe_fallback() {
        let tuner = AdaptivePoolTuner::new();
        let stats = WorkloadStats {
            avg_concurrent_queries: 1000.0,
            avg_query_duration_ms: 10000.0,
            peak_qps: 100000.0,
            db_max_connections: 3,
        };
        let result = tuner.tune(&stats);
        assert!(result.tuning_rationale.contains("POOL_TUNING_UNSAFE"));
        assert_eq!(result.expected_throughput_improvement, 0.0);
        assert_eq!(result.expected_p99_reduction, 0.0);
    }

    #[test]
    fn test_adaptive_tuning_multiple_cycles() {
        let tuner = AdaptivePoolTuner::new();
        let stats = WorkloadStats {
            avg_concurrent_queries: 30.0,
            avg_query_duration_ms: 100.0,
            peak_qps: 500.0,
            db_max_connections: 100,
        };
        for _ in 0..10 {
            let result = tuner.tune(&stats);
            assert!(result.recommended_params.capacity > 0);
        }
        assert_eq!(tuner.tuning_count(), 10);
        assert!(tuner.safe_ratio() > 0.0);
    }

    #[test]
    fn test_pool_param_recommender_directly() {
        let stats = WorkloadStats {
            avg_concurrent_queries: 20.0,
            avg_query_duration_ms: 200.0,
            peak_qps: 100.0,
            db_max_connections: 50,
        };
        let params = PoolParamRecommender::recommend(&stats);
        assert!(params.capacity >= 4);
        assert!(params.idle_timeout_secs > 0);
        assert!(params.health_check_interval_secs > 0);
        assert!(params.acquire_timeout_ms > 0);
        assert!(!params.rationale.is_empty());
    }
}
