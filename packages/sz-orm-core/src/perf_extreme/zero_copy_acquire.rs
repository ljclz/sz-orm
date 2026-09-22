//! v8.0.0 任务 2.1：零拷贝 acquire 路径
//!
//! 复用既有 `Pool`（`pool.rs:1019`）的 `acquire`，通过 `PerfMetrics` 全局
//! 原子计数器记录 acquire 次数与延迟，消除额外 Arc clone/Box 分配。

use std::sync::Arc;
use std::time::Instant;

use crate::error::PoolError;
use crate::perf_metrics::PerfMetrics;
use crate::pool::{Pool, PooledConnection};

/// 零拷贝 acquire 配置
#[derive(Debug, Clone)]
pub struct ZeroCopyConfig {
    /// P99 延迟目标（纳秒）
    pub p99_target_ns: u64,
}

impl Default for ZeroCopyConfig {
    fn default() -> Self {
        Self { p99_target_ns: 100 }
    }
}

/// 零拷贝 acquire 路径
///
/// 注入 `Arc<Pool>` + `ZeroCopyConfig`，复用既有 `Pool::acquire`，
/// 通过 `PerfMetrics` 全局计数器记录采集延迟与成功/失败次数。
pub struct ZeroCopyAcquire {
    pool: Arc<Pool>,
    config: ZeroCopyConfig,
    metrics: &'static PerfMetrics,
}

impl ZeroCopyAcquire {
    /// 创建零拷贝 acquire 路径
    pub fn new(pool: Arc<Pool>, config: ZeroCopyConfig) -> Self {
        Self {
            pool,
            config,
            metrics: PerfMetrics::global(),
        }
    }

    /// 从 Pool 直接构造（默认配置）
    pub fn from_pool(pool: Arc<Pool>) -> Self {
        Self::new(pool, ZeroCopyConfig::default())
    }

    /// 获取配置引用
    pub fn config(&self) -> &ZeroCopyConfig {
        &self.config
    }

    /// 零拷贝 acquire 连接
    ///
    /// 复用 `Pool::acquire`（`pool.rs:1607`），记录 PerfMetrics。
    /// 池耗尽 → `PoolError::Exhausted`；池关闭 → `PoolError::Closed`。
    pub async fn acquire_zero_copy(&self) -> Result<PooledConnection, PoolError> {
        let start = Instant::now();
        let result = self.pool.acquire().await;
        let elapsed_ns = start.elapsed().as_nanos() as u64;

        match &result {
            Ok(_) => self.metrics.record_pool_acquire(),
            Err(_) => self.metrics.record_pool_acquire_failed(),
        }
        self.metrics
            .record_pool_throughput(1_000_000_000u64.checked_div(elapsed_ns).unwrap_or(0));

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::{Pool, PoolConfigBuilder};
    use std::sync::Arc;

    struct MockConnection {
        connected: bool,
    }

    impl MockConnection {
        fn new() -> Self {
            Self { connected: true }
        }
    }

    impl crate::pool::Connection for MockConnection {
        fn execute<'a>(
            &'a mut self,
            _sql: &'a str,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<u64, crate::DbError>> + Send + 'a>,
        > {
            Box::pin(async move { Ok(1) })
        }
        fn query<'a>(
            &'a mut self,
            _sql: &'a str,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<
                            Vec<std::collections::HashMap<String, crate::value::Value>>,
                            crate::DbError,
                        >,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async move { Ok(vec![]) })
        }
        fn begin_transaction<'a>(
            &'a mut self,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<(), crate::DbError>> + Send + 'a>,
        > {
            Box::pin(async move { Ok(()) })
        }
        fn commit<'a>(
            &'a mut self,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<(), crate::DbError>> + Send + 'a>,
        > {
            Box::pin(async move { Ok(()) })
        }
        fn rollback<'a>(
            &'a mut self,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<(), crate::DbError>> + Send + 'a>,
        > {
            Box::pin(async move { Ok(()) })
        }
        fn is_connected(&self) -> bool {
            self.connected
        }
        fn ping<'a>(
            &'a mut self,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send + 'a>> {
            Box::pin(async move { true })
        }
        fn close<'a>(
            &'a mut self,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<(), crate::DbError>> + Send + 'a>,
        > {
            Box::pin(async move {
                self.connected = false;
                Ok(())
            })
        }
    }

    struct MockConnectionFactory;

    #[async_trait::async_trait]
    impl crate::pool::ConnectionFactory for MockConnectionFactory {
        async fn create(&self) -> Result<Box<dyn crate::pool::Connection>, crate::DbError> {
            Ok(Box::new(MockConnection::new()))
        }
    }

    fn make_pool(max_size: u32) -> Pool {
        let config = PoolConfigBuilder::new().max_size(max_size).build().unwrap();
        Pool::new(config, Arc::new(MockConnectionFactory)).unwrap()
    }

    #[tokio::test]
    async fn acquire_zero_copy_returns_connection() {
        let pool = Arc::new(make_pool(8));
        let zc = ZeroCopyAcquire::from_pool(pool);
        let conn = zc.acquire_zero_copy().await;
        assert!(conn.is_ok(), "acquire_zero_copy 应返回连接");
    }

    #[tokio::test]
    async fn acquire_zero_copy_10000_times() {
        let pool = Arc::new(make_pool(8));
        let zc = ZeroCopyAcquire::from_pool(pool);
        for _ in 0..10000 {
            let _conn = zc.acquire_zero_copy().await.unwrap();
        }
        let snap = PerfMetrics::global().snapshot();
        assert!(snap.pool_acquire_count >= 10000, "应累计 10000 次 acquire");
    }

    #[tokio::test]
    async fn acquire_zero_copy_closed_pool_returns_closed_error() {
        let pool = Arc::new(make_pool(4));
        pool.close_all().await;
        let zc = ZeroCopyAcquire::from_pool(pool);
        let err = zc.acquire_zero_copy().await;
        assert!(matches!(err, Err(PoolError::Closed)));
    }

    #[test]
    fn config_default_p99_target_100ns() {
        let cfg = ZeroCopyConfig::default();
        assert_eq!(cfg.p99_target_ns, 100);
    }

    #[test]
    fn new_with_custom_config() {
        let pool = Arc::new(make_pool(4));
        let cfg = ZeroCopyConfig { p99_target_ns: 200 };
        let zc = ZeroCopyAcquire::new(pool, cfg);
        assert_eq!(zc.config().p99_target_ns, 200);
    }

    #[test]
    fn config_method_returns_reference() {
        let pool = Arc::new(make_pool(4));
        let zc = ZeroCopyAcquire::from_pool(pool);
        assert_eq!(zc.config().p99_target_ns, 100);
    }

    #[tokio::test]
    async fn acquire_zero_copy_exhausted_pool() {
        let pool = Arc::new(make_pool(1));
        let zc = ZeroCopyAcquire::from_pool(pool.clone());
        let _conn = zc.acquire_zero_copy().await.unwrap();
        let err = zc.acquire_zero_copy().await;
        assert!(err.is_err(), "池已耗尽应返回错误");
    }

    #[tokio::test]
    async fn acquire_zero_copy_records_failed_metric_on_error() {
        let pool = Arc::new(make_pool(1));
        pool.close_all().await;
        let zc = ZeroCopyAcquire::from_pool(pool);
        let _ = zc.acquire_zero_copy().await;
        let snap = PerfMetrics::global().snapshot();
        assert!(snap.pool_acquire_failed_count >= 1, "应记录失败计数");
    }
}
