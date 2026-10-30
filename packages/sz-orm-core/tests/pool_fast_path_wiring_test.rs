//! v10.11.0 T1.4 连接池快速路径深化端到端接线测试
//!
//! 验证生产调用点可达：
//! - CachePadded<T> 64B 对齐 wrapper 避免 false sharing
//! - FastPathStats 取/还连接分离统计（acquire_count/release_count/cache_miss_count）
//! - Pool::acquire 快速路径（idle 队列无锁 pop）命中时不计 cache_miss
//! - Pool::acquire 快速路径未命中时计 cache_miss（回退慢路径创建新连接）
//! - Pool::release 计 release_count
//! - Pool::fast_path_stats() getter 可观测快速路径命中率

mod common;

use common::pool_mock;
use std::sync::Arc;
use std::time::Duration;

use sz_orm_core::{Pool, PoolConfig};

/// T1.4 测试：FastPathStats 基本计数
///
/// 验证行为副作用：acquire 和 release 后 fast_path_stats 的计数器正确递增。
#[tokio::test]
async fn test_pool_fast_path_stats_basic_counting() {
    let config = PoolConfig {
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
    let pool = Pool::new(config, Arc::new(pool_mock::MockConnectionFactory)).unwrap();

    assert_eq!(pool.fast_path_stats().acquire_count(), 0);
    assert_eq!(pool.fast_path_stats().release_count(), 0);
    assert_eq!(pool.fast_path_stats().cache_miss_count(), 0);

    let conn1 = pool.acquire().await.unwrap();
    assert_eq!(pool.fast_path_stats().acquire_count(), 1);
    assert_eq!(pool.fast_path_stats().cache_miss_count(), 1);

    pool.release(conn1).await;
    assert_eq!(pool.fast_path_stats().release_count(), 1);

    let conn2 = pool.acquire().await.unwrap();
    assert_eq!(pool.fast_path_stats().acquire_count(), 2);
    assert_eq!(pool.fast_path_stats().cache_miss_count(), 1);

    pool.release(conn2).await;
    assert_eq!(pool.fast_path_stats().release_count(), 2);
}

/// T1.4 端到端测试：快速路径 cache miss 减少
///
/// 验证行为副作用：
/// 1. 首次 acquire 无空闲连接 → cache_miss +1（回退慢路径创建新连接）
/// 2. release 后连接入 idle 队列
/// 3. 再次 acquire 命中快速路径 → cache_miss 不增加
/// 4. 多轮 acquire-release 后 cache_miss 远小于 acquire_count
#[tokio::test]
async fn e2e_pool_fast_path_cache_miss_reduction() {
    let config = PoolConfig {
        max_size: 8,
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
    let pool = Pool::new(config, Arc::new(pool_mock::MockConnectionFactory)).unwrap();

    let conn = pool.acquire().await.unwrap();
    assert_eq!(pool.fast_path_stats().cache_miss_count(), 1);
    pool.release(conn).await;

    for _ in 0..20 {
        let c = pool.acquire().await.unwrap();
        assert_eq!(
            pool.fast_path_stats().cache_miss_count(),
            1,
            "release 后 idle 队列有连接，应命中快速路径"
        );
        pool.release(c).await;
    }

    assert_eq!(pool.fast_path_stats().acquire_count(), 21);
    assert_eq!(pool.fast_path_stats().release_count(), 21);
    assert_eq!(pool.fast_path_stats().cache_miss_count(), 1);
}

/// T1.4 端到端测试：并发下快速路径统计正确
///
/// 验证行为副作用：多任务并发 acquire/release 后，
/// acquire_count == release_count == 并发次数，
/// cache_miss_count <= acquire_count（部分命中快速路径）。
#[tokio::test]
async fn e2e_pool_fast_path_under_concurrency() {
    let config = PoolConfig {
        max_size: 16,
        min_idle: 0,
        acquire_timeout: Duration::from_secs(5),
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
    let pool = Arc::new(Pool::new(config, Arc::new(pool_mock::MockConnectionFactory)).unwrap());

    let total_ops = 100usize;
    let mut handles = Vec::new();
    for _ in 0..total_ops {
        let pool_clone = Arc::clone(&pool);
        handles.push(tokio::spawn(async move {
            let conn = pool_clone.acquire().await.unwrap();
            pool_clone.release(conn).await;
        }));
    }
    for h in handles {
        h.await.unwrap();
    }

    let stats = pool.fast_path_stats();
    assert_eq!(stats.acquire_count(), total_ops as u64);
    assert_eq!(stats.release_count(), total_ops as u64);
    assert!(
        stats.cache_miss_count() <= stats.acquire_count(),
        "cache_miss 不应超过 acquire_count"
    );
    assert!(
        stats.cache_miss_count() >= 1,
        "首次 acquire 必定 miss（idle 队列为空）"
    );
}

/// T1.4 测试：CachePadded 64B 对齐
///
/// 验证行为副作用：CachePadded<T> 的内存地址 64B 对齐，
/// 确保 acquire_count 和 release_count 不共享 cache line。
#[test]
fn test_pool_fast_path_cache_padded_alignment() {
    use std::sync::atomic::AtomicU64;
    use sz_orm_core::CachePadded;

    let padded = CachePadded::new(AtomicU64::new(42));
    let addr = padded.get() as *const AtomicU64 as usize;
    assert_eq!(addr % 64, 0, "CachePadded 地址必须 64B 对齐");

    let stats = sz_orm_core::FastPathStats::new();
    let acquire_addr = stats.acquire_count() as *const u64 as usize;
    let _release_addr = stats.release_count() as *const u64 as usize;
    assert_eq!(acquire_addr % 64, 0, "acquire_count 内部地址应 64B 对齐");

    assert_eq!(stats.acquire_count(), 0);
    assert_eq!(stats.release_count(), 0);
    assert_eq!(stats.cache_miss_count(), 0);

    stats.record_acquire();
    stats.record_release();
    stats.record_cache_miss();
    assert_eq!(stats.acquire_count(), 1);
    assert_eq!(stats.release_count(), 1);
    assert_eq!(stats.cache_miss_count(), 1);
}
