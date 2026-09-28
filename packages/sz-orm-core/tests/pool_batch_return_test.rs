//! v9.2.0 M6-T34：acquire_batch/return_raw 批量获取与裸连接归还（2 tests）
//!
//! 注意：现有 pool_batch_test.rs 已覆盖 acquire_batch 基础场景，
//! 本文件补充 return_raw 归还裸连接的端到端测试。

mod common;

use std::time::Duration;
use common::pool_mock;

#[tokio::test]
async fn test_pool_acquire_batch_then_return_raw_all() {
    let pool = pool_mock::create_pool(4);
    let conns = pool.acquire_batch(3).await.unwrap();
    assert_eq!(conns.len(), 3);
    let raw = conns.into_iter().next().unwrap().into_inner();
    let status_before = pool.status().await;
    pool.return_raw(raw).await.unwrap();
    let status_after = pool.status().await;
    assert_eq!(
        status_after.active,
        status_before.active + 1,
        "return_raw 应重新占用槽位"
    );
}

#[tokio::test]
async fn test_pool_return_raw_rejected_after_close_all() {
    let pool = pool_mock::create_pool(4);
    let conn = pool.acquire().await.unwrap();
    let raw = conn.into_inner();
    pool.close_all().await;
    let result = pool.return_raw(raw).await;
    assert!(
        result.is_err(),
        "close_all 后 return_raw 应失败"
    );
}