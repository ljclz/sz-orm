//! v10.2.0 并发安全性测试
//!
//! 验证新增 v10.2.0 组件在多线程并发场景下的正确性。

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use async_trait::async_trait;

use sz_orm_core::atomic_hot_update::{AtomicBatchUpdater, AtomicHotUpdateConfig};
use sz_orm_core::hot_update::HotUpdateTarget;
use sz_orm_core::pool::{Connection, ConnectionFactory, Pool, PoolConfig, QueryRows};
use sz_orm_core::profit_quant::{ProfitQuantConfig, ProfitQuantEngine, WorkloadMetrics};
use sz_orm_core::self_heal::{SelfHealConfig, SelfHealEngine};
use sz_orm_core::DbError;

struct MockConnection;

impl Connection for MockConnection {
    fn execute<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(1) })
    }

    fn query<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<QueryRows, DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(vec![]) })
    }

    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }

    fn commit<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }

    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }

    fn is_connected(&self) -> bool {
        true
    }

    fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { true })
    }

    fn close<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }
}

struct MockFactory;

#[async_trait]
impl ConnectionFactory for MockFactory {
    async fn create(&self) -> Result<Box<dyn Connection>, DbError> {
        Ok(Box::new(MockConnection))
    }
}

#[tokio::test]
async fn test_concurrent_auto_wire_status_100_threads() {
    let config = PoolConfig::default();
    let pool = Arc::new(Pool::new(config, Arc::new(MockFactory)).unwrap());
    let mut handles = Vec::new();
    for _ in 0..100 {
        let p = pool.clone();
        handles.push(tokio::spawn(async move { p.auto_wire_status() }));
    }
    let first = handles.remove(0).await.unwrap();
    for handle in handles {
        let status = handle.await.unwrap();
        assert_eq!(
            status.wired_components, first.wired_components,
            "consistent status"
        );
    }
}

#[tokio::test]
async fn test_concurrent_self_heal_event_history() {
    let config = SelfHealConfig {
        self_heal_enabled: true,
        reconnect_initial_backoff: std::time::Duration::from_millis(1),
        reconnect_max_attempts: 1,
        ..Default::default()
    };
    let engine = Arc::new(SelfHealEngine::new(config));
    let mut handles = Vec::new();
    for i in 0..100 {
        let e = engine.clone();
        handles.push(tokio::spawn(async move {
            e.auto_reconnect(move || async move {
                if i % 2 == 0 {
                    Ok(())
                } else {
                    Err("fail".to_string())
                }
            })
            .await
        }));
    }
    for handle in handles {
        let _ = handle.await.unwrap();
    }
    let history = engine.event_history();
    assert!(
        history.len() >= 100,
        "must have at least 100 events: {}",
        history.len()
    );
}

#[tokio::test]
async fn test_concurrent_atomic_batch_update() {
    let config = AtomicHotUpdateConfig {
        atomic_hot_update_enabled: true,
        ..Default::default()
    };
    let updater = Arc::new(AtomicBatchUpdater::new(config));
    let success_count = Arc::new(AtomicU32::new(0));
    let mut handles = Vec::new();
    for _ in 0..10 {
        let u = updater.clone();
        let sc = success_count.clone();
        handles.push(tokio::spawn(async move {
            let updates = vec![HotUpdateTarget::SlowQueryThreshold { threshold_ms: 200 }];
            let result = u
                .batch_update(updates, |target| {
                    let target = target.clone();
                    async move { sz_orm_core::hot_update::UnifiedHotUpdater::validate(&target) }
                })
                .await;
            if result.is_ok() {
                sc.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }
    for handle in handles {
        handle.await.unwrap();
    }
    assert_eq!(
        success_count.load(Ordering::Relaxed),
        10,
        "all 10 concurrent batch updates must succeed"
    );
    let history = updater.event_history();
    assert!(
        history.len() >= 20,
        "must have started + committed events: {}",
        history.len()
    );
}

#[tokio::test]
async fn test_concurrent_profit_quant_benchmark() {
    let config = ProfitQuantConfig {
        profit_quant_enabled: true,
        ..Default::default()
    };
    let engine = Arc::new(ProfitQuantEngine::new(config));
    let mut handles = Vec::new();
    for i in 0..20 {
        let e = engine.clone();
        handles.push(tokio::spawn(async move {
            let workload = format!("workload_{i}");
            e.run_benchmark(
                &workload,
                |_w| async {
                    Ok(WorkloadMetrics {
                        throughput: 120.0,
                        latency_p99: 80.0,
                        cache_hit_zero_copy_ratio: 0.9,
                    })
                },
                |_w| async {
                    Ok(WorkloadMetrics {
                        throughput: 100.0,
                        latency_p99: 100.0,
                        cache_hit_zero_copy_ratio: 0.5,
                    })
                },
            )
            .await
        }));
    }
    for (i, handle) in handles.into_iter().enumerate() {
        let report = handle.await.unwrap().unwrap();
        assert!(
            (report.throughput_uplift_pct - 20.0).abs() < 1e-6,
            "benchmark {i} uplift 20%"
        );
    }
}
