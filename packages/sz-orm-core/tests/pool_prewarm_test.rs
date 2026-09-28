//! v9.2.0 M6-T32：prewarm/progressive_prewarm/warmup 预热（3 tests）

mod common;

use std::time::Duration;
use common::pool_mock;

#[tokio::test]
async fn test_pool_prewarm_creates_min_idle_connections() {
    let config = pool_mock::prewarm_config(8, 3);
    let pool = sz_orm_core::Pool::new(config, std::sync::Arc::new(pool_mock::MockConnectionFactory))
        .unwrap();
    pool.prewarm().await;
    let status = pool.status().await;
    assert_eq!(status.idle, 3, "prewarm 应创建 min_idle 个连接");
    assert_eq!(status.active, 3);
}

#[tokio::test]
async fn test_pool_warmup_creates_specified_connections() {
    let pool = pool_mock::create_pool(8);
    pool.warmup(4).await.unwrap();
    let status = pool.status().await;
    assert_eq!(status.idle, 4, "warmup(4) 应创建 4 个连接");
    assert_eq!(status.active, 4);
}

#[tokio::test]
async fn test_pool_warmup_respects_max_size() {
    let pool = pool_mock::create_pool(3);
    pool.warmup(10).await.unwrap();
    let status = pool.status().await;
    assert_eq!(
        status.active,
        3,
        "warmup 不应超过 max_size"
    );
}