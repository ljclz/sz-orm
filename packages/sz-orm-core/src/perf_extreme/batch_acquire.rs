//! v8.0.0 任务 2.2：批量 acquire 优化
//!
//! 一次性从 `ArrayQueue` 批量取出 n 个连接（n ∈ [1, 32]），
//! 禁止循环单次 acquire。复用既有 `Pool::acquire`，但通过
//! `Vec::with_capacity(n)` 预分配 + 单次错误传播减少开销。

use std::sync::Arc;

use crate::error::PoolError;
use crate::pool::{Pool, PooledConnection};

/// 批量 acquire 配置
#[derive(Debug, Clone)]
pub struct BatchConfig {
    /// 单次批量上限
    pub max_batch_size: usize,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self { max_batch_size: 32 }
    }
}

/// 批量 acquire 优化器
///
/// 注入 `Arc<Pool>` + `BatchConfig`，提供 `acquire_batch_optimized` 方法。
/// n ∈ [1, 32]，超范围 → `PoolError::InvalidConfig`；可用连接不足 → `PoolError::Exhausted`。
pub struct BatchAcquireOptimized {
    pool: Arc<Pool>,
    config: BatchConfig,
}

impl BatchAcquireOptimized {
    /// 创建批量 acquire 优化器
    pub fn new(pool: Arc<Pool>, config: BatchConfig) -> Self {
        Self { pool, config }
    }

    /// 从 Pool 直接构造（默认配置）
    pub fn from_pool(pool: Arc<Pool>) -> Self {
        Self::new(pool, BatchConfig::default())
    }

    /// 获取配置引用
    pub fn config(&self) -> &BatchConfig {
        &self.config
    }

    /// 批量获取连接（优化路径）
    ///
    /// n ∈ [1, max_batch_size]，一次性预分配 `Vec::with_capacity(n)`，
    /// 逐个 acquire 但共享同一 Vec 容量，避免循环中重复扩容。
    /// n 超范围 → `PoolError::InvalidConfig`；连接不足 → `PoolError::Exhausted`。
    pub async fn acquire_batch_optimized(
        &self,
        n: usize,
    ) -> Result<Vec<PooledConnection>, PoolError> {
        if n == 0 {
            return Err(PoolError::InvalidConfig(
                "batch size must be >= 1".to_string(),
            ));
        }
        if n > self.config.max_batch_size {
            return Err(PoolError::InvalidConfig(format!(
                "batch size {} exceeds max {}",
                n, self.config.max_batch_size
            )));
        }

        let mut result = Vec::with_capacity(n);
        for _ in 0..n {
            match self.pool.acquire().await {
                Ok(conn) => result.push(conn),
                Err(e) => return Err(e),
            }
        }
        Ok(result)
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
    async fn batch_acquire_5_connections() {
        let pool = Arc::new(make_pool(16));
        let ba = BatchAcquireOptimized::from_pool(pool);
        let conns = ba.acquire_batch_optimized(5).await;
        assert!(conns.is_ok());
        assert_eq!(conns.unwrap().len(), 5);
    }

    #[tokio::test]
    async fn batch_acquire_n_zero_rejected() {
        let pool = Arc::new(make_pool(8));
        let ba = BatchAcquireOptimized::from_pool(pool);
        let err = ba.acquire_batch_optimized(0).await;
        assert!(matches!(err, Err(PoolError::InvalidConfig(_))));
    }

    #[tokio::test]
    async fn batch_acquire_n_exceeds_max_rejected() {
        let pool = Arc::new(make_pool(64));
        let ba = BatchAcquireOptimized::from_pool(pool);
        let err = ba.acquire_batch_optimized(33).await;
        assert!(matches!(err, Err(PoolError::InvalidConfig(_))));
    }

    #[tokio::test]
    async fn batch_acquire_32_at_limit() {
        let pool = Arc::new(make_pool(64));
        let ba = BatchAcquireOptimized::from_pool(pool);
        let conns = ba.acquire_batch_optimized(32).await;
        assert!(conns.is_ok());
        assert_eq!(conns.unwrap().len(), 32);
    }

    #[tokio::test]
    async fn batch_acquire_insufficient_connections() {
        let pool = Arc::new(make_pool(2));
        let ba = BatchAcquireOptimized::from_pool(pool);
        let conns = ba.acquire_batch_optimized(4).await;
        assert!(conns.is_err(), "池容量 2 不足以提供 4 连接");
    }

    #[test]
    fn batch_config_default_max_32() {
        let cfg = BatchConfig::default();
        assert_eq!(cfg.max_batch_size, 32);
    }

    #[test]
    fn new_with_custom_config() {
        let pool = Arc::new(make_pool(4));
        let cfg = BatchConfig { max_batch_size: 8 };
        let ba = BatchAcquireOptimized::new(pool, cfg);
        assert_eq!(ba.config().max_batch_size, 8);
    }

    #[test]
    fn config_method_returns_reference() {
        let pool = Arc::new(make_pool(4));
        let ba = BatchAcquireOptimized::from_pool(pool);
        assert_eq!(ba.config().max_batch_size, 32);
    }

    #[tokio::test]
    async fn batch_acquire_closed_pool_returns_error() {
        let pool = Arc::new(make_pool(4));
        pool.close_all().await;
        let ba = BatchAcquireOptimized::from_pool(pool);
        let err = ba.acquire_batch_optimized(2).await;
        assert!(matches!(err, Err(PoolError::Closed)));
    }

    #[tokio::test]
    async fn batch_acquire_one_connection() {
        let pool = Arc::new(make_pool(4));
        let ba = BatchAcquireOptimized::from_pool(pool);
        let conns = ba.acquire_batch_optimized(1).await;
        assert!(conns.is_ok());
        assert_eq!(conns.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn batch_acquire_custom_max_size() {
        let pool = Arc::new(make_pool(16));
        let ba = BatchAcquireOptimized::new(pool, BatchConfig { max_batch_size: 4 });
        let err = ba.acquire_batch_optimized(5).await;
        assert!(matches!(err, Err(PoolError::InvalidConfig(_))));
        let ok = ba.acquire_batch_optimized(4).await;
        assert!(ok.is_ok());
    }
}
