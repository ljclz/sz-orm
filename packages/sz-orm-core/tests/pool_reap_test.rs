//! v9.2.0 M6-T29：reap_idle/reap_idle_selective 选择性回收（2 tests）

mod common;

use common::pool_mock;
use std::time::Duration;

#[tokio::test]
async fn test_pool_reap_idle_removes_expired() {
    let config = sz_orm_core::PoolConfig {
        max_size: 4,
        min_idle: 0,
        acquire_timeout: Duration::from_secs(2),
        idle_timeout: Duration::from_millis(50),
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
    let pool = sz_orm_core::Pool::new(
        config,
        std::sync::Arc::new(pool_mock::MockConnectionFactory),
    )
    .unwrap();
    let conn = pool.acquire().await.unwrap();
    pool.release(conn).await;
    assert_eq!(pool.status().await.idle, 1);
    tokio::time::sleep(Duration::from_millis(80)).await;
    pool.reap_idle().await;
    assert_eq!(pool.status().await.idle, 0, "空闲过久的连接应被回收");
}

#[tokio::test]
async fn test_pool_reap_idle_keeps_fresh() {
    let config = sz_orm_core::PoolConfig {
        max_size: 4,
        min_idle: 0,
        acquire_timeout: Duration::from_secs(2),
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
    let pool = sz_orm_core::Pool::new(
        config,
        std::sync::Arc::new(pool_mock::MockConnectionFactory),
    )
    .unwrap();
    let conn = pool.acquire().await.unwrap();
    pool.release(conn).await;
    assert_eq!(pool.status().await.idle, 1);
    pool.reap_idle().await;
    assert_eq!(pool.status().await.idle, 1, "未过期连接应保留");
}
