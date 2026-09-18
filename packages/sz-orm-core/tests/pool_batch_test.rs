//! v7.4.0 任务 3.7：Pool 批量获取端到端测试

use sz_orm_core::{Connection, ConnectionFactory, Pool, PoolConfig};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

struct MockConnection {
    connected: bool,
}

impl MockConnection {
    fn new() -> Self {
        Self { connected: true }
    }
}

impl Connection for MockConnection {
    fn execute<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, sz_orm_core::DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(1) })
    }

    fn query<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<sz_orm_core::QueryRows, sz_orm_core::DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(vec![]) })
    }

    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }

    fn commit<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }

    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { true })
    }

    fn close<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>> {
        Box::pin(async move {
            self.connected = false;
            Ok(())
        })
    }
}

struct MockConnectionFactory;

#[async_trait::async_trait]
impl ConnectionFactory for MockConnectionFactory {
    async fn create(&self) -> Result<Box<dyn Connection>, sz_orm_core::DbError> {
        Ok(Box::new(MockConnection::new()))
    }
}

fn create_test_pool(max_size: u32) -> Pool {
    let config = PoolConfig {
        max_size,
        min_idle: 0,
        acquire_timeout: Duration::from_secs(5),
        idle_timeout: Duration::from_secs(60),
        max_lifetime: Duration::from_secs(300),
        connection_timeout: Duration::from_secs(5),
        tls: None,
        query_timeout: None,
        max_rows: None,
        memory_limit: None,
        on_event: None,
        test_before_acquire: false,
        prewarm: false,
    };
    Pool::new(config, Arc::new(MockConnectionFactory)).unwrap()
}

#[tokio::test]
async fn test_acquire_batch_zero() {
    let pool = create_test_pool(4);
    let result = pool.acquire_batch(0).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 0);
}

#[tokio::test]
async fn test_acquire_batch_success() {
    let pool = create_test_pool(4);
    let result = pool.acquire_batch(3).await;
    assert!(result.is_ok());
    assert_eq!(result.as_ref().unwrap().len(), 3);
}

#[tokio::test]
async fn test_acquire_batch_exceeds_max() {
    let pool = create_test_pool(2);
    let result = pool.acquire_batch(3).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_acquire_batch_all_dropped() {
    let pool = create_test_pool(4);
    {
        let conns = pool.acquire_batch(2).await.unwrap();
        assert_eq!(conns.len(), 2);
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    let status = pool.status().await;
    assert_eq!(status.idle, 2, "连接归还后 idle 应为 2");
}

#[tokio::test]
async fn test_acquire_batch_one_at_a_time() {
    let pool = create_test_pool(4);
    let conns = pool.acquire_batch(1).await.unwrap();
    assert_eq!(conns.len(), 1);
    let status = pool.status().await;
    assert_eq!(status.active, 1);
}
