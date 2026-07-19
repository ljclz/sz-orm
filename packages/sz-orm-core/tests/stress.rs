//! Stress 测试套件
//!
//! 针对连接池、事务、查询的高并发和长时间运行进行压力测试
//! 目标：发现资源泄漏、状态不一致、死锁、活锁等问题
//!
//! 不变量：
//! - 任意时刻 active_count <= max_size（active_count = idle + borrowed）
//! - close_all 后所有 release 的连接被关闭，active_count 减少
//! - acquire_timeout 后必须返回 PoolError::Timeout
//! - 高频 acquire/release 下最终 active_count == idle_count（无借出）
//!
//! 注意：PoolStatus.active 是"池中总连接数"（idle + borrowed），
//!       PoolStatus.idle 是"空闲连接数"，
//!       借出连接数 = active - idle

mod common;

use common::{MockConnectionFactory, Rng};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use sz_orm_core::TransactOptions;
use sz_orm_core::Transaction;
use sz_orm_core::{Pool, PoolConfigBuilder, PoolError};
use tokio::sync::Mutex;

/// 辅助：构建一个连接池（使用 leak 获得 'static 引用，供 tokio::spawn 使用）
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

/// 辅助：断言没有借出连接（active == idle）
async fn assert_no_borrowed(pool: &Pool, msg: &str) {
    let status = pool.status().await;
    assert_eq!(
        status.active, status.idle,
        "{}: active={}, idle={}",
        msg, status.active, status.idle
    );
}

/// Stress 1：高并发 acquire/release 循环
/// 验证：active_count 始终 <= max_size，最终无借出；无连接泄漏
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stress_pool_concurrent_acquire_release() {
    let pool = make_pool(20, 10);
    let total_ops = Arc::new(AtomicU64::new(0));
    let max_active_observed = Arc::new(AtomicU32::new(0));

    let mut handles = Vec::new();
    for _ in 0..8 {
        let total_ops = total_ops.clone();
        let max_active = max_active_observed.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..100 {
                let conn = match pool.acquire().await {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let status = pool.status().await;
                max_active.fetch_max(status.active, Ordering::Relaxed);
                tokio::task::yield_now().await;
                pool.release(conn).await;
                total_ops.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    assert!(
        total_ops.load(Ordering::Relaxed) > 0,
        "should complete some ops"
    );
    assert!(
        max_active_observed.load(Ordering::Relaxed) <= 20,
        "active_count must never exceed max_size; observed {}",
        max_active_observed.load(Ordering::Relaxed)
    );
    assert_no_borrowed(pool, "after all ops").await;
}

/// Stress 2：max_size 限制测试
/// 验证：当所有连接被借出时，新 acquire 会超时
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stress_pool_max_size_enforcement() {
    let pool = make_pool(5, 1);
    // 借出所有连接
    let mut held_conns = Vec::new();
    for _ in 0..5 {
        held_conns.push(pool.acquire().await.unwrap());
    }
    let status = pool.status().await;
    assert_eq!(status.active, 5);
    assert_eq!(status.idle, 0);

    // 新 acquire 必然超时
    let start = Instant::now();
    let result = pool.acquire().await;
    let elapsed = start.elapsed();
    assert!(result.is_err(), "should timeout when pool exhausted");
    assert!(
        matches!(result, Err(PoolError::Timeout)),
        "expected Timeout error, got {:?}",
        result.as_ref().err()
    );
    assert!(
        elapsed >= Duration::from_millis(900),
        "should wait at least ~1s before timeout, got {:?}",
        elapsed
    );
    assert!(
        elapsed < Duration::from_millis(2000),
        "should not wait too long, got {:?}",
        elapsed
    );

    // 释放一个连接
    pool.release(held_conns.pop().unwrap()).await;
    // 现在 acquire 应该立即成功
    let start = Instant::now();
    let conn = pool.acquire().await;
    let elapsed = start.elapsed();
    assert!(conn.is_ok(), "acquire should succeed after release");
    assert!(
        elapsed < Duration::from_millis(200),
        "acquire should be fast after release, got {:?}",
        elapsed
    );

    // 清理
    if let Ok(c) = conn {
        pool.release(c).await;
    }
    for c in held_conns {
        pool.release(c).await;
    }
    assert_no_borrowed(pool, "after cleanup").await;
}

/// Stress 3：突发大量并发任务
/// 验证：在突发负载下不会出现死锁或状态错乱
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn stress_pool_burst_load() {
    let pool = make_pool(10, 5);
    let success_count = Arc::new(AtomicU64::new(0));
    let timeout_count = Arc::new(AtomicU64::new(0));

    let mut handles = Vec::new();
    for _ in 0..100 {
        let sc = success_count.clone();
        let tc = timeout_count.clone();
        handles.push(tokio::spawn(async move {
            // 持有连接 30ms 模拟工作
            match pool.acquire().await {
                Ok(conn) => {
                    tokio::time::sleep(Duration::from_millis(30)).await;
                    pool.release(conn).await;
                    sc.fetch_add(1, Ordering::Relaxed);
                }
                Err(PoolError::Timeout) => {
                    tc.fetch_add(1, Ordering::Relaxed);
                }
                Err(e) => panic!("unexpected error: {:?}", e),
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    let total = success_count.load(Ordering::Relaxed) + timeout_count.load(Ordering::Relaxed);
    assert_eq!(total, 100, "all tasks must complete");
    // 10 个连接，每个 30ms，5 秒内可以完成 ~1600 个操作
    // 100 个任务应该都能成功
    assert!(
        success_count.load(Ordering::Relaxed) >= 80,
        "at least 80 should succeed, got {}",
        success_count.load(Ordering::Relaxed)
    );

    assert_no_borrowed(pool, "after burst load").await;
}

/// Stress 4：长时间运行事务
/// 验证：长事务期间其他连接仍可正常工作
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stress_long_transaction() {
    let pool = make_pool(5, 5);

    // 启动一个长事务
    let conn = pool.acquire().await.unwrap();
    let mut tx = Transaction::new(conn.into_inner(), TransactOptions::default());
    tx.execute("INSERT INTO t VALUES (1)").await.unwrap();
    tx.execute("INSERT INTO t VALUES (2)").await.unwrap();

    // 长事务期间，其他连接应该能正常工作
    let success = Arc::new(AtomicU64::new(0));
    let mut handles = Vec::new();
    for _ in 0..4 {
        let s = success.clone();
        handles.push(tokio::spawn(async move {
            if let Ok(c) = pool.acquire().await {
                tokio::time::sleep(Duration::from_millis(10)).await;
                pool.release(c).await;
                s.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }
    assert!(
        success.load(Ordering::Relaxed) >= 3,
        "other tasks should succeed during long tx"
    );

    // 提交长事务
    tx.commit().await.unwrap();
    // Transaction 持有的 conn 不会自动归还到池中
    // 需要手动取出并归还（这里通过 drop 释放，连接不会回到池中）
    drop(tx);

    let status = pool.status().await;
    // 长事务的连接未归还，所以 active - idle == 1
    assert_eq!(status.active - status.idle, 1, "long-tx conn not returned");
}

/// Stress 5：混合工作负载（短查询 + 长事务 + 写操作）
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stress_mixed_workload() {
    let pool = make_pool(15, 5);
    let completed = Arc::new(AtomicU64::new(0));

    let mut handles = Vec::new();
    for i in 0..60 {
        let completed = completed.clone();
        handles.push(tokio::spawn(async move {
            match i % 3 {
                0 => {
                    // 短查询
                    if let Ok(conn) = pool.acquire().await {
                        tokio::task::yield_now().await;
                        pool.release(conn).await;
                    }
                }
                1 => {
                    // 长事务
                    if let Ok(conn) = pool.acquire().await {
                        let mut tx =
                            Transaction::new(conn.into_inner(), TransactOptions::default());
                        let _ = tx.execute("INSERT").await;
                        let _ = tx.commit().await;
                        drop(tx);
                    }
                }
                _ => {
                    // 写操作
                    if let Ok(conn) = pool.acquire().await {
                        drop(conn);
                    }
                }
            }
            completed.fetch_add(1, Ordering::Relaxed);
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    assert_eq!(completed.load(Ordering::Relaxed), 60);
    // 注意：Transaction drop 后连接不会自动归还，但 i%3==1 的分支连接通过 tx 持有
    // tx.commit 后 tx 仍持有 conn，drop(tx) 后 conn 被 drop（不归还池）
    // 所以 active - idle 可能 > 0
    let status = pool.status().await;
    assert!(status.active <= 15, "active must not exceed max_size");
}

/// Stress 6：连接池耗尽后恢复
/// 验证：池耗尽 → 超时 → 释放连接 → 新 acquire 成功
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stress_pool_exhaustion_recovery() {
    let pool = make_pool(3, 1);

    // 借出所有
    let mut held = Vec::new();
    for _ in 0..3 {
        held.push(pool.acquire().await.unwrap());
    }

    // 验证 acquire 会超时
    let result = tokio::time::timeout(Duration::from_secs(2), pool.acquire()).await;
    assert!(result.is_ok(), "acquire should complete (with timeout)");
    assert!(result.unwrap().is_err(), "should be Timeout error");

    // 释放一个
    pool.release(held.pop().unwrap()).await;

    // 现在 acquire 应该立即成功
    let start = Instant::now();
    let conn = pool.acquire().await;
    let elapsed = start.elapsed();
    assert!(conn.is_ok(), "acquire should succeed after release");
    assert!(
        elapsed < Duration::from_millis(200),
        "acquire should be fast after release, got {:?}",
        elapsed
    );

    // 清理
    if let Ok(c) = conn {
        pool.release(c).await;
    }
    for c in held {
        pool.release(c).await;
    }
    assert_no_borrowed(pool, "after recovery").await;
}

/// Stress 7：高频 acquire/release 切换
/// 验证：在极高频率下状态仍然一致
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stress_high_frequency_acquire_release() {
    let pool = make_pool(8, 5);
    let iterations = Arc::new(AtomicU64::new(0));

    let mut handles = Vec::new();
    for _ in 0..4 {
        let iters = iterations.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..500 {
                if let Ok(conn) = pool.acquire().await {
                    pool.release(conn).await;
                    iters.fetch_add(1, Ordering::Relaxed);
                }
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    assert!(
        iterations.load(Ordering::Relaxed) > 0,
        "should complete some iterations"
    );
    assert_no_borrowed(pool, "after high frequency").await;
}

/// Stress 8：close_all 后所有 release 的连接被关闭
/// 验证：close_all + release 后 active_count 减少到 0
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stress_close_all_drops_released() {
    let pool = make_pool(10, 5);

    // 借出 5 个连接
    let mut held = Vec::new();
    for _ in 0..5 {
        held.push(pool.acquire().await.unwrap());
    }
    let status = pool.status().await;
    assert_eq!(status.active, 5);
    assert_eq!(status.idle, 0);

    // close_all（只关闭 idle，不关闭已借出的）
    pool.close_all().await;
    let status = pool.status().await;
    assert_eq!(status.idle, 0);
    assert_eq!(
        status.active, 5,
        "close_all should not affect borrowed conns"
    );

    // 归还连接：池已关闭，应直接关闭连接并减少 active_count
    for conn in held {
        pool.release(conn).await;
    }
    let status = pool.status().await;
    assert_eq!(
        status.idle, 0,
        "released conns should be closed, not returned to idle"
    );
    assert_eq!(
        status.active, 0,
        "active should be 0 after release on closed pool"
    );
}

/// Stress 9：长时间运行，验证状态一致性
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stress_long_running_steady_state() {
    let pool = make_pool(10, 5);
    let total = Arc::new(AtomicU64::new(0));

    // 持续运行 1 秒
    let deadline = Instant::now() + Duration::from_secs(1);
    let mut handles = Vec::new();
    for _ in 0..4 {
        let total = total.clone();
        handles.push(tokio::spawn(async move {
            while Instant::now() < deadline {
                if let Ok(conn) = pool.acquire().await {
                    tokio::task::yield_now().await;
                    pool.release(conn).await;
                    total.fetch_add(1, Ordering::Relaxed);
                }
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    let ops = total.load(Ordering::Relaxed);
    assert!(ops > 100, "should complete many ops in 1s, got {}", ops);

    assert_no_borrowed(pool, "after steady state").await;
}

/// Stress 10：并发 reap_idle
/// 验证：并发 reap_idle 不会导致状态错乱
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stress_concurrent_reap_idle() {
    let pool = make_pool(20, 5);

    // 创建一些空闲连接
    let mut conns = Vec::new();
    for _ in 0..10 {
        conns.push(pool.acquire().await.unwrap());
    }
    for c in conns.drain(..) {
        pool.release(c).await;
    }

    // 并发 reap_idle
    let mut handles = Vec::new();
    for _ in 0..2 {
        handles.push(tokio::spawn(async move {
            for _ in 0..5 {
                pool.reap_idle().await;
                tokio::task::yield_now().await;
            }
        }));
    }
    // 同时有任务在 acquire/release
    for _ in 0..2 {
        handles.push(tokio::spawn(async move {
            for _ in 0..50 {
                if let Ok(conn) = pool.acquire().await {
                    pool.release(conn).await;
                }
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    assert_no_borrowed(pool, "after concurrent reap").await;
    let status = pool.status().await;
    assert!(status.idle <= 20);
}

/// Stress 11：使用 FaultyConnectionFactory 测试故障下的压力行为
/// 验证：工厂故障时不会导致状态不一致
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stress_with_factory_faults() {
    use common::FaultyConnectionFactory;
    use sz_orm_core::ConnectionFactory;
    let db = Arc::new(Mutex::new(common::InMemoryDb::new()));
    let factory: Arc<dyn ConnectionFactory> = Arc::new(FaultyConnectionFactory::new(db, 5));
    let config = PoolConfigBuilder::new()
        .max_size(10)
        .min_idle(0)
        .acquire_timeout(5)
        .build()
        .unwrap();
    let pool: &'static Pool = &*Box::leak(Box::new(Pool::new(config, factory).unwrap()));

    let success = Arc::new(AtomicU64::new(0));
    let failure = Arc::new(AtomicU64::new(0));

    let mut handles = Vec::new();
    for _ in 0..10 {
        let s = success.clone();
        let f = failure.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..10 {
                match pool.acquire().await {
                    Ok(conn) => {
                        pool.release(conn).await;
                        s.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(_) => {
                        f.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    let total = success.load(Ordering::Relaxed) + failure.load(Ordering::Relaxed);
    assert_eq!(total, 100, "all ops must complete");

    assert_no_borrowed(pool, "after factory faults").await;
}

/// Stress 12：随机延迟下的压力测试
/// 验证：在随机持有时间下，池状态始终一致
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stress_random_hold_times() {
    let pool = make_pool(12, 5);
    let mut rng = Rng::new(42);
    let total = Arc::new(AtomicU64::new(0));

    let mut delays = Vec::new();
    for _ in 0..150 {
        delays.push(rng.next_usize(20));
    }
    let delays = Arc::new(Mutex::new(delays));

    let mut handles = Vec::new();
    for _ in 0..4 {
        let total = total.clone();
        let delays = delays.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..30 {
                let delay_ms = {
                    let mut d = delays.lock().await;
                    if d.is_empty() {
                        5
                    } else {
                        d.pop().unwrap() as u64
                    }
                };
                if let Ok(conn) = pool.acquire().await {
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    pool.release(conn).await;
                    total.fetch_add(1, Ordering::Relaxed);
                }
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    assert!(
        total.load(Ordering::Relaxed) > 0,
        "should complete some ops"
    );
    assert_no_borrowed(pool, "after random hold times").await;
    let status = pool.status().await;
    assert!(status.idle <= 12);
}
