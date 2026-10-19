//! v10.2.0 边界条件测试
//!
//! 验证 v10.2.0 新增组件在边界条件下的行为正确性。
//! 纯逻辑测试，不需要真实 DB。

use std::sync::Arc;
use std::time::Duration;

use sz_orm_core::atomic_hot_update::{AtomicBatchUpdater, AtomicHotUpdateConfig, BatchUpdateError};
use sz_orm_core::auto_wire::AutoWireConfig;
use sz_orm_core::hot_update::HotUpdateTarget;
use sz_orm_core::profit_quant::{
    ProfitError, ProfitQuantConfig, ProfitQuantEngine, WorkloadMetrics,
};
use sz_orm_core::self_heal::{SelfHealConfig, SelfHealEngine};

#[tokio::test]
async fn test_boundary_profit_quant_empty_workload() {
    let engine = ProfitQuantEngine::new(ProfitQuantConfig::default());
    let result = engine
        .run_benchmark(
            "",
            |_w| async {
                Ok(WorkloadMetrics {
                    throughput: 100.0,
                    latency_p99: 100.0,
                    cache_hit_zero_copy_ratio: 0.5,
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
        .await;
    assert!(
        matches!(result, Err(ProfitError::EmptyWorkload)),
        "empty workload must be rejected"
    );
}

#[test]
fn test_boundary_self_heal_zero_max_attempts() {
    let config = SelfHealConfig {
        self_heal_enabled: true,
        reconnect_max_attempts: 0,
        ..Default::default()
    };
    let engine = SelfHealEngine::new(config);
    assert!(!engine.is_valid(), "max_attempts=0 must be invalid");
}

#[tokio::test]
async fn test_boundary_atomic_batch_empty_updates() {
    let updater = AtomicBatchUpdater::new(AtomicHotUpdateConfig::default());
    let result = updater
        .batch_update(Vec::new(), |_u| async { Ok(()) })
        .await;
    assert!(result.is_ok(), "empty updates should succeed");
    let history = updater.event_history();
    assert!(
        history.iter().any(|e| matches!(
            e,
            sz_orm_core::atomic_hot_update::AtomicBatchUpdateEvent::BatchUpdateCommitted { .. }
        )),
        "must have committed event"
    );
}

#[tokio::test]
async fn test_boundary_atomic_batch_single_module() {
    let updater = AtomicBatchUpdater::new(AtomicHotUpdateConfig {
        atomic_hot_update_enabled: true,
        ..Default::default()
    });
    let updates = vec![HotUpdateTarget::SlowQueryThreshold { threshold_ms: 300 }];
    let result = updater
        .batch_update(updates, |target| {
            let target = target.clone();
            async move { sz_orm_core::hot_update::UnifiedHotUpdater::validate(&target) }
        })
        .await;
    assert!(result.is_ok(), "single module batch update must succeed");
    let history = updater.event_history();
    assert!(history.iter().any(|e| matches!(
        e,
        sz_orm_core::atomic_hot_update::AtomicBatchUpdateEvent::BatchUpdateCommitted { .. }
    )));
}

#[test]
fn test_boundary_loop_health_zero_events() {
    use sz_orm_core::performance_linkage::LoopHealthConfig;
    let config = LoopHealthConfig::default();
    assert!(!config.loop_health_enabled, "default disabled");
    let enabled_config = LoopHealthConfig {
        loop_health_enabled: true,
        ..Default::default()
    };
    assert!(enabled_config.loop_health_enabled, "explicitly enabled");
}

#[tokio::test]
async fn test_boundary_auto_wire_no_factory() {
    use async_trait::async_trait;
    use std::future::Future;
    use std::pin::Pin;
    use sz_orm_core::pool::{Connection, ConnectionFactory, QueryRows};
    use sz_orm_core::pool::{Pool, PoolConfig};
    use sz_orm_core::DbError;

    struct MockConn;
    impl Connection for MockConn {
        fn execute<'a>(
            &'a mut self,
            _sql: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
            Box::pin(async { Ok(1) })
        }
        fn query<'a>(
            &'a mut self,
            _sql: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<QueryRows, DbError>> + Send + 'a>> {
            Box::pin(async { Ok(vec![]) })
        }
        fn begin_transaction<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
            Box::pin(async { Ok(()) })
        }
        fn commit<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
            Box::pin(async { Ok(()) })
        }
        fn rollback<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
            Box::pin(async { Ok(()) })
        }
        fn is_connected(&self) -> bool {
            true
        }
        fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
            Box::pin(async { true })
        }
        fn close<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
            Box::pin(async { Ok(()) })
        }
    }
    struct MockFac;
    #[async_trait]
    impl ConnectionFactory for MockFac {
        async fn create(&self) -> Result<Box<dyn Connection>, DbError> {
            Ok(Box::new(MockConn))
        }
    }

    let config = PoolConfig {
        auto_wire: AutoWireConfig {
            auto_wire_enabled: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let pool = Pool::new(config, Arc::new(MockFac)).unwrap();
    let status = pool.auto_wire_status();
    assert!(
        status.wired_components.is_empty(),
        "no factory → no wired components"
    );
    assert!(
        status.failed_components.is_empty(),
        "no factory → no failed components"
    );
}

#[tokio::test]
async fn test_boundary_self_heal_jitter_boundary() {
    let config = SelfHealConfig {
        self_heal_enabled: true,
        reconnect_jitter_range: 0.0,
        reconnect_initial_backoff: Duration::from_millis(100),
        ..Default::default()
    };
    let engine = SelfHealEngine::new(config);
    let backoff = engine.compute_backoff(0);
    assert_eq!(
        backoff,
        Duration::from_millis(100),
        "zero jitter → exact backoff"
    );
}

#[tokio::test]
async fn test_boundary_atomic_batch_not_enabled() {
    use async_trait::async_trait;
    use std::future::Future;
    use std::pin::Pin;
    use sz_orm_core::pool::{Connection, ConnectionFactory, QueryRows};
    use sz_orm_core::pool::{Pool, PoolConfig};
    use sz_orm_core::DbError;

    struct MockConn;
    impl Connection for MockConn {
        fn execute<'a>(
            &'a mut self,
            _sql: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
            Box::pin(async { Ok(1) })
        }
        fn query<'a>(
            &'a mut self,
            _sql: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<QueryRows, DbError>> + Send + 'a>> {
            Box::pin(async { Ok(vec![]) })
        }
        fn begin_transaction<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
            Box::pin(async { Ok(()) })
        }
        fn commit<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
            Box::pin(async { Ok(()) })
        }
        fn rollback<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
            Box::pin(async { Ok(()) })
        }
        fn is_connected(&self) -> bool {
            true
        }
        fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
            Box::pin(async { true })
        }
        fn close<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
            Box::pin(async { Ok(()) })
        }
    }
    struct MockFac;
    #[async_trait]
    impl ConnectionFactory for MockFac {
        async fn create(&self) -> Result<Box<dyn Connection>, DbError> {
            Ok(Box::new(MockConn))
        }
    }

    let pool = Pool::new(PoolConfig::default(), Arc::new(MockFac)).unwrap();
    let result = pool
        .batch_hot_update(vec![HotUpdateTarget::SlowQueryThreshold {
            threshold_ms: 200,
        }])
        .await;
    assert!(
        matches!(result, Err(BatchUpdateError::NotEnabled)),
        "disabled → NotEnabled"
    );
}
