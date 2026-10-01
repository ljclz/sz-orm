//! v9.2.0 M6-T33：health_check/close_all/shutdown/shutdown_with_timeout 生命周期（4 tests）

mod common;

use common::pool_mock;
use std::time::Duration;
use sz_orm_core::PoolError;

#[tokio::test]
async fn test_pool_health_check_returns_zero_when_all_alive() {
    let pool = pool_mock::create_pool(4);
    pool.warmup(2).await.unwrap();
    let removed = pool.health_check().await;
    assert_eq!(removed, 0, "存活连接不应被移除");
    assert_eq!(pool.status().await.idle, 2);
}

#[tokio::test]
async fn test_pool_close_all_rejects_new_acquire() {
    let pool = pool_mock::create_pool(4);
    let conn = pool.acquire().await.unwrap();
    pool.release(conn).await;
    pool.close_all().await;
    let result = pool.acquire().await;
    assert!(
        matches!(result, Err(PoolError::Closed)),
        "close_all 后 acquire 应返回 Closed"
    );
}

#[tokio::test]
async fn test_pool_shutdown_closes_idle_connections() {
    let pool = pool_mock::create_pool(4);
    pool.warmup(2).await.unwrap();
    assert_eq!(pool.status().await.idle, 2);
    pool.shutdown().await;
    let result = pool.acquire().await;
    assert!(matches!(result, Err(PoolError::Closed)));
}

#[tokio::test]
async fn test_pool_shutdown_with_timeout_completes() {
    let pool = pool_mock::create_pool(4);
    let conn = pool.acquire().await.unwrap();
    pool.release(conn).await;
    pool.shutdown_with_timeout(Duration::from_millis(200)).await;
    let result = pool.acquire().await;
    assert!(matches!(result, Err(PoolError::Closed)));
}
