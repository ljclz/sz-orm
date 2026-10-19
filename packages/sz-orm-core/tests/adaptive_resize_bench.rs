//! v9.8.0 连接池自适应调优基准测试。
//!
//! 运行：cargo test -p sz-orm-core --test adaptive_resize_bench -- --ignored

use std::sync::Arc;
use std::time::{Duration, Instant};

use sz_orm_core::{
    AdjustmentRateLimiter, AutoResizeConfig, AutoResizeDecider, Connection, ConnectionFactory,
    LoadSignal, LoadSignalCollector, Pool, PoolConfigBuilder, ResizeDecision, RetestMonitor,
    RetestResult,
};

use async_trait::async_trait;

struct MockConn;

impl Connection for MockConn {
    fn execute<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<u64, sz_orm_core::DbError>> + Send + 'a>,
    > {
        Box::pin(async move { Ok(1) })
    }
    fn query<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<sz_orm_core::QueryRows, sz_orm_core::DbError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move { Ok(sz_orm_core::QueryRows::new()) })
    }
    fn begin_transaction<'a>(
        &'a mut self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>,
    > {
        Box::pin(async move { Ok(()) })
    }
    fn commit<'a>(
        &'a mut self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>,
    > {
        Box::pin(async move { Ok(()) })
    }
    fn rollback<'a>(
        &'a mut self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>,
    > {
        Box::pin(async move { Ok(()) })
    }
    fn is_connected(&self) -> bool {
        true
    }
    fn ping<'a>(
        &'a mut self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { true })
    }
    fn close<'a>(
        &'a mut self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>,
    > {
        Box::pin(async move { Ok(()) })
    }
}

struct MockFactory;

#[async_trait]
impl ConnectionFactory for MockFactory {
    async fn create(&self) -> Result<Box<dyn Connection>, sz_orm_core::DbError> {
        Ok(Box::new(MockConn))
    }
}

#[tokio::test]
#[ignore = "benchmark: adaptive acquire latency stability"]
async fn bench_adaptive_acquire_latency_stability() {
    let config = PoolConfigBuilder::new()
        .max_size(20)
        .min_idle(1)
        .build()
        .unwrap();
    let pool = Pool::new(config, Arc::new(MockFactory)).unwrap();

    let mut p99_before = Vec::new();
    for _ in 0..100 {
        let conn = pool.acquire().await.unwrap();
        pool.release(conn).await;
    }
    let signal = LoadSignalCollector::collect(&pool).await;
    p99_before.push(signal.acquire_p99_ms);

    let resize_config = AutoResizeConfig {
        enabled: true,
        collect_interval: Duration::from_millis(100),
        grow_threshold_ms: 50.0,
        ..AutoResizeConfig::default()
    };
    let _handle = pool.start_adaptive_resize(resize_config);

    tokio::time::sleep(Duration::from_millis(500)).await;

    let mut p99_after = Vec::new();
    for _ in 0..100 {
        let conn = pool.acquire().await.unwrap();
        pool.release(conn).await;
    }
    let signal = LoadSignalCollector::collect(&pool).await;
    p99_after.push(signal.acquire_p99_ms);

    println!(
        "bench_adaptive_acquire_latency_stability: before P99={:?} after P99={:?}",
        p99_before, p99_after
    );
    pool.close_all().await;
}

#[tokio::test]
#[ignore = "benchmark: adaptive grow/shrink convergence"]
async fn bench_adaptive_grow_shrink_convergence() {
    let config = PoolConfigBuilder::new()
        .max_size(30)
        .min_idle(1)
        .build()
        .unwrap();
    let pool = Pool::new(config, Arc::new(MockFactory)).unwrap();

    let decider = AutoResizeDecider::new(
        AutoResizeConfig {
            enabled: true,
            max_step: 5,
            min_capacity: 5,
            ..AutoResizeConfig::default()
        },
        30,
    );

    let high_load = LoadSignal {
        acquire_p50_ms: 80.0,
        acquire_p99_ms: 150.0,
        idle_rate: 0.05,
        active_rate: 0.95,
        timeout_count: 0,
        health_fail_rate: 0.0,
        cross_node_rate: 0.0,
        collected_at: Instant::now(),
    };
    let grow = decider.decide(&high_load, 10);
    assert!(matches!(grow, ResizeDecision::Grow(_)));

    let low_load = LoadSignal {
        acquire_p50_ms: 5.0,
        acquire_p99_ms: 10.0,
        idle_rate: 0.9,
        active_rate: 0.1,
        timeout_count: 0,
        health_fail_rate: 0.0,
        cross_node_rate: 0.0,
        collected_at: Instant::now(),
    };
    let shrink = decider.decide(&low_load, 20);
    assert!(matches!(shrink, ResizeDecision::Shrink(_)));

    println!(
        "bench_adaptive_grow_shrink_convergence: grow={:?} shrink={:?}",
        grow, shrink
    );
    pool.close_all().await;
}

#[tokio::test]
#[ignore = "benchmark: adaptive rollback"]
async fn bench_adaptive_rollback() {
    let monitor = RetestMonitor::new(Duration::from_secs(30), 0.1);
    let before = LoadSignal {
        acquire_p50_ms: 30.0,
        acquire_p99_ms: 50.0,
        idle_rate: 0.1,
        active_rate: 0.9,
        timeout_count: 0,
        health_fail_rate: 0.0,
        cross_node_rate: 0.0,
        collected_at: Instant::now(),
    };
    let after = vec![
        LoadSignal {
            acquire_p50_ms: 70.0,
            acquire_p99_ms: 70.0,
            idle_rate: 0.1,
            active_rate: 0.9,
            timeout_count: 0,
            health_fail_rate: 0.0,
            cross_node_rate: 0.0,
            collected_at: Instant::now(),
        },
        LoadSignal {
            acquire_p50_ms: 75.0,
            acquire_p99_ms: 75.0,
            idle_rate: 0.1,
            active_rate: 0.9,
            timeout_count: 0,
            health_fail_rate: 0.0,
            cross_node_rate: 0.0,
            collected_at: Instant::now(),
        },
    ];
    let result = monitor.evaluate(&before, &after);
    assert!(matches!(result, RetestResult::Degraded { .. }));
    println!("bench_adaptive_rollback: result={:?}", result);
}

#[test]
#[ignore = "benchmark: adaptive storm protection"]
fn bench_adaptive_storm_protection() {
    let limiter = AdjustmentRateLimiter::new(Duration::from_secs(30), Duration::from_secs(60));
    let mut allowed = 0;
    let mut rejected = 0;
    for _ in 0..10 {
        if limiter.allow_adjust() {
            limiter.record_adjust();
            allowed += 1;
        } else {
            rejected += 1;
        }
    }
    assert_eq!(allowed, 1);
    assert_eq!(rejected, 9);
    println!("bench_adaptive_storm_protection: allowed={allowed} rejected={rejected}");
}
