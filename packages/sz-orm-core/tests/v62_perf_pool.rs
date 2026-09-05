#![allow(deprecated)]

//! v6.2 性能优化：池性能集成测试
//!
//! 验证 Pool 复用率、调优建议、预热容错、并发 set_max_size。
//! 标注 `#[ignore]` 用 `--ignored` 触发。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use sz_orm_core::{Connection, ConnectionFactory, Pool, PoolConfig};

/// Mock 连接
struct MockConnection {
    connected: bool,
}

#[async_trait::async_trait]
impl Connection for MockConnection {
    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<u64, sz_orm_core::DbError>> + Send + 'a>,
    > {
        let _ = sql;
        Box::pin(async move { Ok(1) })
    }
    fn query<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<sz_orm_core::QueryRows, sz_orm_core::DbError>,
                > + Send
                + 'a,
        >,
    > {
        let _ = sql;
        Box::pin(async { Ok(sz_orm_core::QueryRows::new()) })
    }
    fn begin_transaction<'a>(
        &'a mut self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>,
    > {
        Box::pin(async { Ok(()) })
    }
    fn commit<'a>(
        &'a mut self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>,
    > {
        Box::pin(async { Ok(()) })
    }
    fn rollback<'a>(
        &'a mut self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>,
    > {
        Box::pin(async { Ok(()) })
    }
    fn is_connected(&self) -> bool {
        self.connected
    }
    fn ping<'a>(
        &'a mut self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send + 'a>> {
        Box::pin(async { true })
    }
    fn close<'a>(
        &'a mut self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>,
    > {
        Box::pin(async move {
            self.connected = false;
            Ok(())
        })
    }
}

/// 始终成功的连接工厂
struct MockConnectionFactory;

#[async_trait::async_trait]
impl ConnectionFactory for MockConnectionFactory {
    async fn create(&self) -> Result<Box<dyn Connection>, sz_orm_core::DbError> {
        Ok(Box::new(MockConnection { connected: true }))
    }
}

/// 前 N 次成功，之后失败的连接工厂
struct PartialFailingFactory {
    success_count: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl ConnectionFactory for PartialFailingFactory {
    async fn create(&self) -> Result<Box<dyn Connection>, sz_orm_core::DbError> {
        let n = self.success_count.fetch_sub(1, Ordering::Relaxed);
        if n > 0 {
            Ok(Box::new(MockConnection { connected: true }))
        } else {
            Err(sz_orm_core::DbError::ConnectionError(
                "simulated failure".to_string(),
            ))
        }
    }
}

/// 验证稳态运行 1000 次 acquire/release 后复用率 ≥ 0.9
#[tokio::test]
#[ignore]
async fn pool_reuse_rate_real_db() {
    let config = PoolConfig {
        max_size: 10,
        min_idle: 2,
        ..PoolConfig::default()
    };
    let factory = Arc::new(MockConnectionFactory);
    let pool = Pool::new(config, factory).expect("pool");

    for _ in 0..1000 {
        let conn = pool.acquire().await.expect("acquire");
        pool.release(conn).await;
    }

    let metrics = pool.pool_metrics();
    let reuse_rate = metrics.connection_reuse_rate();
    assert!(
        reuse_rate >= 0.9,
        "复用率应 ≥ 0.9，实际: {reuse_rate}"
    );

    pool.close_all().await;
}

/// 验证真实负载下 suggest_tuning() 不 panic
#[tokio::test]
#[ignore]
async fn pool_suggest_tuning_real_db() {
    let config = PoolConfig {
        max_size: 10,
        min_idle: 2,
        ..PoolConfig::default()
    };
    let factory = Arc::new(MockConnectionFactory);
    let pool = Pool::new(config, factory).expect("pool");

    for _ in 0..100 {
        let conn = pool.acquire().await.expect("acquire");
        pool.release(conn).await;
    }

    let advice = pool.suggest_tuning();
    assert!(
        !advice.reason.is_empty(),
        "reason 不应为空"
    );

    pool.close_all().await;
}

/// 验证预热部分失败时池仍可服务
#[tokio::test]
#[ignore]
async fn pool_prewarm_partial_failure() {
    let config = PoolConfig {
        max_size: 10,
        min_idle: 5,
        prewarm: true,
        ..PoolConfig::default()
    };
    let factory = Arc::new(PartialFailingFactory {
        success_count: Arc::new(AtomicUsize::new(3)),
    });
    let pool = Pool::new(config, factory).expect("pool");

    let conn = pool.acquire().await;
    assert!(
        conn.is_ok(),
        "池应仍可服务即使预热部分失败"
    );

    pool.close_all().await;
}

/// 验证并发 acquire 进行中调用 set_max_size 不出错
#[tokio::test]
#[ignore]
async fn pool_set_max_size_concurrent() {
    let config = PoolConfig {
        max_size: 10,
        min_idle: 2,
        ..PoolConfig::default()
    };
    let factory = Arc::new(MockConnectionFactory);
    let pool = Arc::new(Pool::new(config, factory).expect("pool"));

    let pool1 = Arc::clone(&pool);
    let handle = tokio::spawn(async move {
        for _ in 0..100 {
            let conn = pool1.acquire().await.expect("acquire");
            pool1.release(conn).await;
        }
    });

    pool.set_max_size(5);
    pool.set_max_size(20);

    handle.await.expect("task completed");

    pool.close_all().await;
}