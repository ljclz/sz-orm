//! v8.8.0 白帽测试 — 条件 notify 竞态 / ping 采样 / reap_idle UAF
//!
//! 验证 v8.8.0 连接池优化未引入安全漏洞：
//! - 多线程并发 acquire/release 无死锁、无 use-after-free、无数据竞争
//! - ping 采样（空闲 < 30s 跳过 ping）不导致认证绕过
//! - reap_idle 关闭过期连接后无悬垂引用

#![allow(dead_code)]
mod common;

use common::MockConnectionFactory;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use sz_orm_core::{Pool, PoolConfigBuilder};
use tokio::sync::Mutex;

fn make_pool(max_size: u32, acquire_timeout_secs: u64) -> &'static Pool {
    let db = Arc::new(Mutex::new(common::InMemoryDb::new()));
    let factory = Arc::new(MockConnectionFactory::new(db));
    let config = PoolConfigBuilder::new()
        .max_size(max_size)
        .min_idle(0)
        .acquire_timeout(acquire_timeout_secs)
        .build()
        .unwrap();
    let pool = Pool::new(config, factory).unwrap();
    Box::leak(Box::new(pool))
}

fn make_pool_with_ping(max_size: u32, acquire_timeout_secs: u64) -> &'static Pool {
    let db = Arc::new(Mutex::new(common::InMemoryDb::new()));
    let factory = Arc::new(MockConnectionFactory::new(db));
    let config = PoolConfigBuilder::new()
        .max_size(max_size)
        .min_idle(0)
        .acquire_timeout(acquire_timeout_secs)
        .test_before_acquire(true)
        .build()
        .unwrap();
    let pool = Pool::new(config, factory).unwrap();
    Box::leak(Box::new(pool))
}

// ── 条件 notify 竞态测试 ──────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn whitehat_concurrent_acquire_release_no_deadlock() {
    let pool = make_pool(20, 10);
    let total_ops = Arc::new(AtomicU64::new(0));
    let mut handles = Vec::new();

    for _ in 0..16 {
        let total_ops = total_ops.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..200 {
                let conn = pool.acquire().await.expect("acquire must succeed");
                tokio::task::yield_now().await;
                pool.release(conn).await;
                total_ops.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    assert_eq!(total_ops.load(Ordering::SeqCst), 16 * 200);
    let status = pool.status().await;
    assert_eq!(
        status.active, status.idle,
        "no borrowed connections after all tasks done"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn whitehat_concurrent_acquire_release_no_count_corruption() {
    let pool = make_pool(10, 10);
    let max_active = Arc::new(AtomicU64::new(0));
    let mut handles = Vec::new();

    for _ in 0..8 {
        let max_active = max_active.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..100 {
                let conn = pool.acquire().await.expect("acquire");
                let status = pool.status().await;
                max_active.fetch_max(status.active as u64, Ordering::Relaxed);
                pool.release(conn).await;
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    let max_observed = max_active.load(Ordering::SeqCst);
    assert!(
        max_observed <= 10,
        "active count must never exceed max_size, got {max_observed}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn whitehat_conditional_notify_does_not_drop_wakeup() {
    let pool = make_pool(2, 5);
    let acquired = Arc::new(AtomicU64::new(0));

    let mut holders = Vec::new();
    for _ in 0..2 {
        let conn = pool.acquire().await.expect("acquire both connections");
        holders.push(conn);
    }

    let acquired_clone = acquired.clone();
    let waiter = tokio::spawn(async move {
        let conn = pool.acquire().await.expect("acquire after release");
        acquired_clone.fetch_add(1, Ordering::SeqCst);
        pool.release(conn).await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    pool.release(holders.pop().unwrap()).await;

    tokio::time::timeout(Duration::from_secs(3), waiter)
        .await
        .expect("waiter must be woken by conditional notify")
        .unwrap();

    assert_eq!(acquired.load(Ordering::SeqCst), 1);
    for h in holders {
        pool.release(h).await;
    }
}

// ── ping 采样测试 ─────────────────────────────────────────────────

#[tokio::test]
async fn whitehat_ping_sampling_recent_connection_still_usable() {
    let pool = make_pool_with_ping(5, 10);

    let conn = pool.acquire().await.expect("acquire");
    pool.release(conn).await;

    tokio::time::sleep(Duration::from_millis(10)).await;

    let conn2 = pool
        .acquire()
        .await
        .expect("recent connection must be reusable without ping");
    assert!(conn2.is_connected(), "connection must be alive");
    pool.release(conn2).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn whitehat_ping_sampling_concurrent_no_auth_bypass() {
    let pool = make_pool_with_ping(10, 10);
    let success_count = Arc::new(AtomicU64::new(0));
    let mut handles = Vec::new();

    for _ in 0..8 {
        let success_count = success_count.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..50 {
                let conn = pool.acquire().await.expect("acquire");
                assert!(
                    conn.is_connected(),
                    "connection must be alive (no auth bypass)"
                );
                pool.release(conn).await;
                success_count.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    assert_eq!(success_count.load(Ordering::SeqCst), 8 * 50);
}

// ── reap_idle UAF 测试 ────────────────────────────────────────────

#[tokio::test]
async fn whitehat_reap_idle_no_use_after_free() {
    let pool = make_pool(10, 10);

    let mut conns = Vec::new();
    for _ in 0..5 {
        conns.push(pool.acquire().await.expect("acquire"));
    }
    for conn in conns {
        pool.release(conn).await;
    }

    pool.reap_idle().await;

    let status = pool.status().await;
    assert!(status.active <= 5, "reap_idle must not corrupt pool state");

    let conn = pool
        .acquire()
        .await
        .expect("pool must be usable after reap_idle");
    assert!(
        conn.is_connected(),
        "connection after reap_idle must be valid"
    );
    pool.release(conn).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn whitehat_reap_idle_concurrent_no_uaf() {
    let pool = make_pool(20, 10);
    let ops = Arc::new(AtomicU64::new(0));
    let mut handles = Vec::new();

    for _ in 0..4 {
        let ops = ops.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..100 {
                let conn = pool.acquire().await.expect("acquire");
                tokio::task::yield_now().await;
                pool.release(conn).await;
                ops.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    let reaper = tokio::spawn(async move {
        for _ in 0..10 {
            tokio::time::sleep(Duration::from_millis(5)).await;
            pool.reap_idle().await;
        }
    });

    for h in handles {
        h.await.unwrap();
    }
    reaper.await.unwrap();

    assert_eq!(ops.load(Ordering::SeqCst), 4 * 100);
    let status = pool.status().await;
    assert!(
        status.active <= 20,
        "pool state must be consistent after concurrent reap_idle"
    );
}
