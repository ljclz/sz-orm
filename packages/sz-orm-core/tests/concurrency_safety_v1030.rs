//! v10.3.0 并发安全性测试
//!
//! 验证新增 v10.3.0 组件在多线程并发场景下的正确性。

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;

use sz_orm_core::pool::{Connection, ConnectionFactory, Pool, PoolConfig, QueryRows};
use sz_orm_core::profit_collector::{ProfitCollector, ProfitCollectorConfig};
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
        Box::pin(async move { Ok(Vec::new()) })
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

struct MockFac;

#[async_trait]
impl ConnectionFactory for MockFac {
    async fn create(&self) -> Result<Box<dyn Connection>, DbError> {
        Ok(Box::new(MockConnection))
    }
}

#[tokio::test]
async fn test_concurrent_profit_collector_100_threads() {
    let config = ProfitCollectorConfig {
        continuous_collect_enabled: true,
        ..Default::default()
    };
    let collector = Arc::new(ProfitCollector::new(config));
    let hit_count = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();
    for i in 0..100u32 {
        let collector = collector.clone();
        let hit_count = hit_count.clone();
        handles.push(tokio::spawn(async move {
            if i % 2 == 0 {
                collector.record_hit();
            } else {
                collector.record_skip();
            }
            hit_count.fetch_add(1, Ordering::SeqCst);
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let snap = collector.snapshot();
    assert_eq!(snap.hit_count + snap.skip_count, 100);
    assert_eq!(hit_count.load(Ordering::SeqCst), 100);
}

#[tokio::test]
async fn test_concurrent_ops_snapshot_100_threads() {
    let config = PoolConfig {
        profit_collector: ProfitCollectorConfig {
            continuous_collect_enabled: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let pool = Arc::new(Pool::new(config, Arc::new(MockFac)).unwrap());
    let success_count = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();
    for _ in 0..100 {
        let pool = pool.clone();
        let success_count = success_count.clone();
        handles.push(tokio::spawn(async move {
            let snapshot = pool.ops_snapshot();
            assert!(snapshot.snapshot_time > 0);
            success_count.fetch_add(1, Ordering::SeqCst);
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    assert_eq!(success_count.load(Ordering::SeqCst), 100);
}

#[tokio::test]
async fn test_concurrent_unified_heal_event_history() {
    let config = PoolConfig {
        unified_heal: sz_orm_core::unified_heal::UnifiedHealConfig {
            unified_heal_enabled: true,
            self_heal_config: sz_orm_core::self_heal::SelfHealConfig {
                self_heal_enabled: true,
                reconnect_initial_backoff: std::time::Duration::from_millis(1),
                reconnect_max_attempts: 1,
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    };
    let pool = Arc::new(Pool::new(config, Arc::new(MockFac)).unwrap());
    let success_count = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();
    for _ in 0..100 {
        let pool = pool.clone();
        let success_count = success_count.clone();
        handles.push(tokio::spawn(async move {
            let events = pool.unified_heal_event_history();
            assert!(events.len() < 1000, "事件历史长度应合理");
            success_count.fetch_add(1, Ordering::SeqCst);
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    assert_eq!(success_count.load(Ordering::SeqCst), 100);
}

#[tokio::test]
async fn test_concurrent_record_linkage_with_collector() {
    let config = PoolConfig {
        profit_collector: ProfitCollectorConfig {
            continuous_collect_enabled: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let pool = Arc::new(Pool::new(config, Arc::new(MockFac)).unwrap());
    let success_count = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();
    for _ in 0..100 {
        let pool = pool.clone();
        let success_count = success_count.clone();
        handles.push(tokio::spawn(async move {
            pool.record_linkage_hit();
            success_count.fetch_add(1, Ordering::SeqCst);
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let snap = pool.profit_snapshot().unwrap();
    assert_eq!(snap.hit_count, 100);
    assert_eq!(success_count.load(Ordering::SeqCst), 100);
}
