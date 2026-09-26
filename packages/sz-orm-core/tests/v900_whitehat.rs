//! M5: known_good 快速路径 whitehat 测试
//!
//! 验证 release 后 acquire 同一连接时 known_good=true 跳过 is_connected 检查，
//! 以及并发 acquire/release 场景下 known_good 标记正确性。

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use sz_orm_core::{Connection, ConnectionFactory, DbError, Pool, PoolConfigBuilder, Value};

/// Mock 连接，记录 is_connected 调用次数
struct CountingMockConnection {
    _id: usize,
    is_connected_calls: Arc<AtomicUsize>,
    connected: bool,
}

impl Connection for CountingMockConnection {
    fn execute<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        Box::pin(async { Ok(1) })
    }

    fn query<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<HashMap<String, Value>>, DbError>> + Send + 'a>>
    {
        Box::pin(async { Ok(vec![]) })
    }

    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }

    fn commit<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }

    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }

    fn is_connected(&self) -> bool {
        self.is_connected_calls.fetch_add(1, Ordering::Relaxed);
        self.connected
    }

    fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async { true })
    }

    fn close<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        self.connected = false;
        Box::pin(async { Ok(()) })
    }
}

struct CountingMockFactory {
    counter: Arc<AtomicUsize>,
    is_connected_calls: Arc<AtomicUsize>,
}

#[async_trait]
impl ConnectionFactory for CountingMockFactory {
    async fn create(&self) -> Result<Box<dyn Connection>, DbError> {
        let id = self.counter.fetch_add(1, Ordering::Relaxed);
        let calls = self.is_connected_calls.clone();
        let conn: Box<dyn Connection> = Box::new(CountingMockConnection {
            _id: id,
            is_connected_calls: calls,
            connected: true,
        });
        Ok(conn)
    }
}

fn make_pool() -> (Pool, Arc<AtomicUsize>) {
    let is_connected_calls = Arc::new(AtomicUsize::new(0));
    let factory = Arc::new(CountingMockFactory {
        counter: Arc::new(AtomicUsize::new(0)),
        is_connected_calls: is_connected_calls.clone(),
    });
    let config = PoolConfigBuilder::new().max_size(5).build().unwrap();
    let pool = Pool::new(config, factory).unwrap();
    (pool, is_connected_calls)
}

#[tokio::test]
async fn test_known_good_always_checks_is_connected_on_reacquire() {
    let (pool, is_connected_calls) = make_pool();

    let conn1 = pool.acquire().await.unwrap();
    assert!(!conn1.known_good);
    pool.release(conn1).await;

    let calls_after_first = is_connected_calls.load(Ordering::Relaxed);

    let conn2 = pool.acquire().await.unwrap();
    assert!(!conn2.known_good, "acquire 后 known_good 应被清除");

    let calls_after_second = is_connected_calls.load(Ordering::Relaxed);
    assert!(
        calls_after_second > calls_after_first,
        "is_connected 应始终被调用（确保网络分区安全）"
    );

    pool.release(conn2).await;
}

#[tokio::test]
async fn test_known_good_set_on_release() {
    let (pool, _is_connected_calls) = make_pool();

    let conn = pool.acquire().await.unwrap();
    assert!(!conn.known_good, "新 acquire 的连接 known_good 应为 false");
    pool.release(conn).await;

    let conn2 = pool.acquire().await.unwrap();
    assert!(!conn2.known_good, "acquire 后 known_good 应被清除");
    pool.release(conn2).await;
}

#[tokio::test]
async fn test_concurrent_acquire_release_known_good() {
    let (pool, _is_connected_calls) = make_pool();

    // 并发 acquire/release 循环
    let pool_clone = pool.clone();
    let handle1 = tokio::spawn(async move {
        for _ in 0..10 {
            let conn = pool_clone.acquire().await.unwrap();
            assert!(!conn.known_good);
            pool_clone.release(conn).await;
        }
    });

    let pool_clone2 = pool.clone();
    let handle2 = tokio::spawn(async move {
        for _ in 0..10 {
            let conn = pool_clone2.acquire().await.unwrap();
            assert!(!conn.known_good);
            pool_clone2.release(conn).await;
        }
    });

    handle1.await.unwrap();
    handle2.await.unwrap();

    // 验证池仍能正常工作
    let conn = pool.acquire().await.unwrap();
    pool.release(conn).await;
}

#[tokio::test]
async fn test_known_good_does_not_bypass_expiry() {
    let is_connected_calls = Arc::new(AtomicUsize::new(0));
    let factory = Arc::new(CountingMockFactory {
        counter: Arc::new(AtomicUsize::new(0)),
        is_connected_calls: is_connected_calls.clone(),
    });
    // max_lifetime=1s 使连接快速过期
    let config = PoolConfigBuilder::new()
        .max_size(2)
        .max_lifetime(1)
        .build()
        .unwrap();
    let pool = Pool::new(config, factory).unwrap();

    let conn = pool.acquire().await.unwrap();
    pool.release(conn).await;

    // 等待过期
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

    // acquire 应发现连接过期，创建新连接
    let conn2 = pool.acquire().await.unwrap();
    assert!(!conn2.known_good);
    pool.release(conn2).await;
}

#[tokio::test]
async fn test_known_good_does_not_bypass_idle_timeout() {
    let is_connected_calls = Arc::new(AtomicUsize::new(0));
    let factory = Arc::new(CountingMockFactory {
        counter: Arc::new(AtomicUsize::new(0)),
        is_connected_calls: is_connected_calls.clone(),
    });
    // idle_timeout=1s 使连接快速空闲超时
    let config = PoolConfigBuilder::new()
        .max_size(2)
        .idle_timeout(1)
        .build()
        .unwrap();
    let pool = Pool::new(config, factory).unwrap();

    let conn = pool.acquire().await.unwrap();
    pool.release(conn).await;

    // 等待空闲超时
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

    // acquire 应发现连接空闲超时，创建新连接
    let conn2 = pool.acquire().await.unwrap();
    assert!(!conn2.known_good);
    pool.release(conn2).await;
}

#[tokio::test]
async fn test_known_good_multiple_cycles() {
    let (pool, _is_connected_calls) = make_pool();

    // 多次 acquire/release 循环
    for i in 0..20 {
        let conn = pool.acquire().await.unwrap();
        assert!(
            !conn.known_good,
            "第 {} 次 acquire 后 known_good 应为 false",
            i
        );
        pool.release(conn).await;
    }
}
// === M9: reap_idle_selective 测试 ===

#[tokio::test]
async fn test_reap_idle_selective_keeps_valid_connections() {
    let (pool, _is_connected_calls) = make_pool();

    // 创建几个连接并归还
    let conn1 = pool.acquire().await.unwrap();
    pool.release(conn1).await;
    let conn2 = pool.acquire().await.unwrap();
    pool.release(conn2).await;

    // reap_idle 应保留未过期连接
    pool.reap_idle().await;

    // 仍能 acquire 到连接
    let conn = pool.acquire().await.unwrap();
    pool.release(conn).await;
}

#[tokio::test]
async fn test_reap_idle_selective_removes_expired() {
    let is_connected_calls = Arc::new(AtomicUsize::new(0));
    let factory = Arc::new(CountingMockFactory {
        counter: Arc::new(AtomicUsize::new(0)),
        is_connected_calls: is_connected_calls.clone(),
    });
    let config = PoolConfigBuilder::new()
        .max_size(5)
        .idle_timeout(1)
        .build()
        .unwrap();
    let pool = Pool::new(config, factory).unwrap();

    // 创建并归还连接
    let conn = pool.acquire().await.unwrap();
    pool.release(conn).await;

    // 等待空闲超时
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

    // reap_idle 应回收过期连接
    pool.reap_idle().await;

    // acquire 应创建新连接
    let conn2 = pool.acquire().await.unwrap();
    pool.release(conn2).await;
}

#[tokio::test]
async fn test_reap_idle_selective_no_connections() {
    let (pool, _is_connected_calls) = make_pool();
    // 空池 reap_idle 不应 panic
    pool.reap_idle().await;
}

#[tokio::test]
async fn test_reap_idle_selective_concurrent_safety() {
    let (pool, _is_connected_calls) = make_pool();

    // 并发 acquire/release + reap_idle
    let pool_clone1 = pool.clone();
    let handle1 = tokio::spawn(async move {
        for _ in 0..5 {
            let conn = pool_clone1.acquire().await.unwrap();
            pool_clone1.release(conn).await;
        }
    });

    let pool_clone2 = pool.clone();
    let handle2 = tokio::spawn(async move {
        for _ in 0..5 {
            pool_clone2.reap_idle().await;
        }
    });

    handle1.await.unwrap();
    handle2.await.unwrap();

    // 池仍能正常工作
    let conn = pool.acquire().await.unwrap();
    pool.release(conn).await;
}
