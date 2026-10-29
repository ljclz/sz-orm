//! v9.2.0 M6-T28：Pool::acquire/release 状态转换覆盖（4 tests）

mod common;

use common::pool_mock;
use std::time::Duration;
use sz_orm_core::PoolError;

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
        metrics_after.connection_created_count, metrics_before.connection_created_count,
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
    let pool = sz_orm_core::Pool::new(
        config,
        std::sync::Arc::new(pool_mock::MockConnectionFactory),
    )
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

/// P3-2 回归（2026-10-29 全栈评测）：两次连续归还 + 多个等待者，全部必须被服务。
///
/// `tokio::sync::Notify::notify_one` 的许可是**合并**的（无等待者时最多存 1 个），
/// 再叠加 release 路径的 `waiters_count > 0` 抑制，归还风暴并不保证"每次归还
/// 对应一次唤醒"。本测试锁住由此推出的行为契约：等待者靠"等待循环顶部重查空闲队列
/// + 上限 100ms 的有界退避"自愈，必须在 `acquire_timeout`（此处 2s）内全部取得连接，
/// 且不得有多余建连、不得有连接静默泄漏。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_pool_consecutive_releases_coalesced_all_waiters_served() {
    let (pool, factory) = pool_mock::create_counting_pool(2);
    let held1 = pool.acquire().await.unwrap();
    let held2 = pool.acquire().await.unwrap();
    assert_eq!(factory.create_count(), 2, "前置条件：两条连接由本次创建");
    assert_eq!(pool.status().await.idle, 0, "前置条件：池已被占满");

    let mut waiters = Vec::new();
    for _ in 0..4 {
        let p = pool.clone();
        waiters.push(tokio::spawn(async move {
            let conn = p
                .acquire()
                .await
                .expect("等待者必须在 acquire_timeout 内取得连接（通知被合并/抑制时靠退避自愈）");
            p.release(conn).await;
        }));
    }

    // 用 waiters_count 同步：它只在等待者真正 park 时才增加。
    for _ in 0..400 {
        if pool.status().await.waiters >= 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(
        pool.status().await.waiters >= 2,
        "前置条件失效：等待者未登记，本测试没有覆盖到唤醒路径"
    );

    // 连续两次归还，中间不 await 任何内容 —— 唤醒许可最多只剩 1 个。
    pool.release(held1).await;
    pool.release(held2).await;

    for w in waiters {
        w.await.unwrap();
    }
    let status = pool.status().await;
    assert_eq!(status.waiters, 0, "全部等待者必须已退出等待");
    assert!(
        status.idle >= 1,
        "归还的连接必须回到空闲队列（实际 idle={}），不得静默泄漏",
        status.idle
    );
    assert_eq!(
        factory.create_count(),
        2,
        "池已满时等待者不得绕过容量上限另建连接"
    );
}

/// P3-2 回归：无等待者时的归还（notify 被抑制）不得让连接滞留或让后来者饿死。
#[tokio::test]
async fn test_pool_suppressed_notify_late_acquirer_still_served() {
    let pool = pool_mock::create_pool(1);
    let conn = pool.acquire().await.unwrap();
    let before = pool.status().await;
    assert_eq!(
        before.waiters, 0,
        "前置条件：无等待者，本次归还必须走抑制分支（不发 notify）"
    );

    pool.release(conn).await;
    assert_eq!(
        pool.status().await.idle,
        1,
        "被抑制通知的归还仍须把连接放回空闲队列"
    );

    // 此刻没有任何待生效的唤醒许可：后来者必须靠循环顶部重查直接命中空闲连接。
    let late = pool
        .acquire()
        .await
        .expect("抑制通知不得导致后来者等待或超时");
    pool.release(late).await;
    assert_eq!(pool.status().await.idle, 1, "最后一笔归还同样不得泄漏");
}
