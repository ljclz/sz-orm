#![cfg(feature = "dynamic-pool-tuning")]

use sz_orm_adaptive::param_tuner::*;

#[tokio::test]
async fn e2e_dynamic_pool_tuner_seconds_level_response() {
    let tuner = DynamicPoolTuner::new();
    let stats = WorkloadStats {
        avg_concurrent_queries: 50.0,
        avg_query_duration_ms: 100.0,
        peak_qps: 1000.0,
        db_max_connections: 200,
    };
    let result = tuner.tune_dynamic(&stats).await;
    assert!(result.load_response_time_ms <= 1000.0);
    assert!(result.multi_pool_isolated);
    assert!(result.throughput_improvement >= 6.0);
    assert!(result.p99_improvement >= 8.0);
}

#[tokio::test]
async fn e2e_multi_pool_isolator_three_pools() {
    let pools = vec![
        (
            PoolType::ReadWrite,
            IsolatedPoolConfig {
                capacity: 100,
                ..Default::default()
            },
        ),
        (
            PoolType::Transaction,
            IsolatedPoolConfig {
                capacity: 30,
                ..Default::default()
            },
        ),
        (
            PoolType::Batch,
            IsolatedPoolConfig {
                capacity: 10,
                ..Default::default()
            },
        ),
    ];
    let isolator = MultiPoolIsolator::new(pools);
    assert!(isolator.is_isolated());
    assert_eq!(isolator.pool_count(), 3);
    assert_eq!(
        isolator.get_pool(PoolType::ReadWrite).unwrap().capacity,
        100
    );
    assert_eq!(
        isolator.get_pool(PoolType::Transaction).unwrap().capacity,
        30
    );
    assert_eq!(isolator.get_pool(PoolType::Batch).unwrap().capacity, 10);
}

#[tokio::test]
async fn e2e_dynamic_pool_tuner_unsafe_alert() {
    let tuner = DynamicPoolTuner::new();
    let stats = WorkloadStats {
        avg_concurrent_queries: 1000.0,
        avg_query_duration_ms: 10000.0,
        peak_qps: 100000.0,
        db_max_connections: 3,
    };
    let result = tuner.tune_dynamic(&stats).await;
    assert!(result.unsafe_alert.is_some());
    assert_eq!(result.unsafe_alert.as_ref().unwrap(), "POOL_TUNING_UNSAFE");
}
