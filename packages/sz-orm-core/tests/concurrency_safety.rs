//! 并发正确性测试（v10.1.0 M6.1）
//!
//! 验证 Pool/Cache/LinkageMetrics 在高并发下的正确性。

use async_trait::async_trait;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use sz_orm_core::pool::{Connection, ConnectionFactory, Pool, PoolConfigBuilder, QueryRows};
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
async fn test_concurrent_acquire_release_100_threads() {
    let config = PoolConfigBuilder::new()
        .max_size(8)
        .acquire_timeout(5)
        .build()
        .expect("config valid");
    let pool = Pool::new(config, Arc::new(MockFactory)).expect("pool creation");

    let pool_clone = pool.clone();
    let success_count = Arc::new(AtomicU64::new(0));

    let mut handles = Vec::new();
    for _ in 0..100 {
        let pool = pool_clone.clone();
        let success = success_count.clone();
        handles.push(tokio::spawn(async move {
            let conn = pool.acquire().await;
            if let Ok(conn) = conn {
                success.fetch_add(1, Ordering::SeqCst);
                drop(conn);
            }
        }));
    }

    for handle in handles {
        handle.await.expect("task panicked");
    }

    assert_eq!(
        success_count.load(Ordering::SeqCst),
        100,
        "all 100 tasks should acquire successfully"
    );
}

#[tokio::test]
async fn test_concurrent_linkage_metrics() {
    let config = PoolConfigBuilder::new()
        .max_size(4)
        .acquire_timeout(5)
        .performance_linkage(sz_orm_core::PerformanceLinkageConfig {
            enabled: true,
            ..Default::default()
        })
        .build()
        .expect("config valid");
    let pool = Pool::new(config, Arc::new(MockFactory)).expect("pool creation");

    let pool_clone = pool.clone();
    let mut handles = Vec::new();
    for _ in 0..100 {
        let pool = pool_clone.clone();
        handles.push(tokio::spawn(async move {
            pool.record_linkage_hit();
        }));
    }

    for handle in handles {
        handle.await.expect("task panicked");
    }

    let metrics = pool.linkage_metrics();
    assert_eq!(
        metrics.hit_count, 100,
        "hit_count should be 100 after 100 concurrent record_hit calls"
    );
}

#[tokio::test]
async fn test_concurrent_linkage_metrics_mix_hit_skip() {
    let config = PoolConfigBuilder::new()
        .max_size(4)
        .acquire_timeout(5)
        .performance_linkage(sz_orm_core::PerformanceLinkageConfig {
            enabled: true,
            ..Default::default()
        })
        .build()
        .expect("config valid");
    let pool = Pool::new(config, Arc::new(MockFactory)).expect("pool creation");

    let pool_clone = pool.clone();
    let mut handles = Vec::new();
    for i in 0..100u64 {
        let pool = pool_clone.clone();
        handles.push(tokio::spawn(async move {
            if i % 2 == 0 {
                pool.record_linkage_hit();
            } else {
                pool.record_linkage_skip();
            }
        }));
    }

    for handle in handles {
        handle.await.expect("task panicked");
    }

    let metrics = pool.linkage_metrics();
    assert_eq!(metrics.hit_count, 50, "50 hits expected");
    assert_eq!(metrics.skip_count, 50, "50 skips expected");
}

#[tokio::test]
async fn test_concurrent_apply_hot_update_validation() {
    let config = PoolConfigBuilder::new()
        .max_size(4)
        .acquire_timeout(5)
        .build()
        .expect("config valid");
    let pool = Pool::new(config, Arc::new(MockFactory)).expect("pool creation");

    let pool_clone = pool.clone();
    let mut handles = Vec::new();
    for i in 0..50u32 {
        let pool = pool_clone.clone();
        handles.push(tokio::spawn(async move {
            let target = sz_orm_core::HotUpdateTarget::AlertThreshold {
                warn: i,
                critical: i + 10,
            };
            pool.apply_hot_update(target)
        }));
    }

    let mut ok_count = 0;
    for handle in handles {
        let result = handle.await.expect("task panicked");
        if result.is_ok() {
            ok_count += 1;
        }
    }
    assert_eq!(
        ok_count, 50,
        "all 50 valid hot updates should pass validation"
    );
}
