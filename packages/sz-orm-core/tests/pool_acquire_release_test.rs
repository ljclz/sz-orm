//! v9.2.0 M6-T28：Pool::acquire/release 状态转换覆盖（4 tests）

mod common;

use std::time::Duration;
use sz_orm_core::PoolError;
use common::pool_mock;

#[tokio::test]
async fn test_pool_acquire_release_idle_reuse() {
    let pool = pool_mock::create_pool(4);
    let conn = pool.acquire().await.unwrap();
    pool.release(conn).await;
    let metrics_before = pool.pool_metrics();
    let conn2 = pool.acquire().await.unwrap();
    let metrics_after = pool.pool_metrics();
    pool.release(conn2).await;
    assert_eq!(
        metrics_after.connection_created_count,
        metrics_before.connection_created_count,
        "复用空闲连接不应新建"
    );
    assert!(metrics_after.acquire_count >= metrics_before.acquire_count);
}

#[tokio::test]
async fn test_pool_acquire_create_new_when_empty() {
    let (pool, factory) = pool_mock::create_counting_pool(4);
    assert_eq!(factory.create_count(), 0);
    let conn = pool.acquire().await.unwrap();
    assert_eq!(factory.create_count(), 1, "池空时 acquire 应创建新连接");
    pool.release(conn).await;
}

#[tokio::test]
async fn test_pool_acquire_wait_timeout_when_exhausted() {
    let config = sz_orm_core::PoolConfig {
        max_size: 1,
        min_idle: 0,
        acquire_timeout: Duration::from_millis(100),
        idle_timeout: Duration::from_secs(60),
        max_lifetime: Duration::from_secs(300),
        connection_timeout: Duration::from_secs(2),
        tls: None,
        query_timeout: None,
        max_rows: None,
        memory_limit: None,
        on_event: None,
        test_before_acquire: false,
        prewarm: false,
    };
    let pool = sz_orm_core::Pool::new(config, std::sync::Arc::new(pool_mock::MockConnectionFactory))
        .unwrap();
    let conn = pool.acquire().await.unwrap();
    let result = pool.acquire().await;
    assert!(matches!(result, Err(PoolError::Timeout)), "池满应超时");
    pool.release(conn).await;
}

#[tokio::test]
async fn test_pool_acquire_release_full_cycle() {
    let pool = pool_mock::create_pool(2);
    let conn1 = pool.acquire().await.unwrap();
    let conn2 = pool.acquire().await.unwrap();
    let status = pool.status().await;
    assert_eq!(status.active, 2);
    pool.release(conn1).await;
    pool.release(conn2).await;
    let status = pool.status().await;
    assert_eq!(status.idle, 2, "归还后空闲应为 2");
    let conn3 = pool.acquire().await.unwrap();
    let conn4 = pool.acquire().await.unwrap();
    pool.release(conn3).await;
    pool.release(conn4).await;
    let metrics = pool.pool_metrics();
    assert!(metrics.acquire_count >= 4);
    assert!(metrics.release_count >= 4);
}