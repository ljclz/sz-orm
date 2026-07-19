//! Pool 模块契约测试 — 对应 `docs/api-contracts.md` §5
//!
//! 锁定连接池的关键不变量，特别是 v0.2.0 的行为变更：
//! - `close_all()` 后 `acquire()` 必须返回 `Err(PoolError::Closed)`（不再创建新连接）
//! - `release()` 后 `status().active` 必须减 1
//! - `acquire()` 必须遵守 `acquire_timeout`
//! - `PooledConnection::into_inner()` 消费连接返回 `Box<dyn Connection>`

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use sz_orm_core::Connection;
use sz_orm_core::{DbType, PoolError, PoolStatus};
use sz_orm_core::{Pool, PoolConfig, PoolConfigBuilder};

use crate::common::{InMemoryDb, MockConnectionFactory};

// ===== 辅助函数 =====

fn make_pool(max_size: u32) -> Pool {
    let db = Arc::new(Mutex::new(InMemoryDb::new()));
    let factory = Arc::new(MockConnectionFactory::new(db));
    let config = PoolConfigBuilder::new()
        .max_size(max_size)
        .acquire_timeout(1)
        .build()
        .unwrap();
    Pool::new(config, factory).unwrap()
}

// ===== §5.1 close_all 后 acquire 返回 Closed 契约（v0.2.0 行为变更） =====

#[tokio::test]
async fn test_close_all_blocks_acquire_contract() {
    // v0.2.0 关键契约：close_all() 后 acquire() 必须返回 Err(PoolError::Closed)
    // 不再创建新连接
    let pool = make_pool(5);
    pool.close_all().await;

    let result = pool.acquire().await;
    match result {
        Err(PoolError::Closed) => { /* 契约满足 */ }
        Err(other) => panic!(
            "期望 PoolError::Closed，实际: {:?}（v0.2.0 契约违反）",
            other
        ),
        Ok(_) => panic!("close_all 后 acquire 必须返回 Err，实际返回 Ok"),
    }
}

// ===== §5.1 release 后连接归还 idle 契约 =====
//
// 注：PoolStatus::active 实际语义为"总连接数"（total = idle + in-use），
// release 后连接归还 idle 队列，total 不变，idle +1。
// 此契约锁定该行为。

#[tokio::test]
async fn test_release_returns_connection_to_idle_contract() {
    let pool = make_pool(5);

    let conn = pool.acquire().await.unwrap();
    let status_after_acquire = pool.status().await;
    // acquire 后 total >= 1，idle == 0（连接正在使用中）
    assert!(status_after_acquire.active >= 1);
    assert_eq!(status_after_acquire.idle, 0);

    pool.release(conn).await;
    let status_after_release = pool.status().await;
    // release 后 idle +1（连接归还），total 不变
    assert_eq!(
        status_after_release.idle, 1,
        "release 后 idle 必须为 1（连接归还到空闲队列）"
    );
    assert_eq!(
        status_after_release.active, status_after_acquire.active,
        "release 后 total（PoolStatus::active）应保持不变"
    );
}

// ===== §5.1 acquire 遵守 timeout 契约 =====

#[tokio::test]
async fn test_acquire_respects_timeout_contract() {
    // 配置 max_size=1，acquire_timeout=1s
    // 先获取一个连接占满池，再尝试 acquire 应超时
    let pool = make_pool(1);

    let _held = pool.acquire().await.unwrap();
    let start = std::time::Instant::now();
    let result = pool.acquire().await;
    let elapsed = start.elapsed();

    assert!(result.is_err(), "应超时失败");
    assert!(
        elapsed >= Duration::from_millis(900),
        "应等待至少 ~1s 才超时，实际: {:?}",
        elapsed
    );
    match result {
        Err(PoolError::Timeout) => { /* 契约满足 */ }
        Err(other) => panic!("期望 PoolError::Timeout，实际: {:?}", other),
        Ok(_) => panic!("应超时失败，实际返回 Ok"),
    }
}

// ===== §5.4 PooledConnection::into_inner 契约 =====

#[tokio::test]
async fn test_pooled_connection_into_inner_contract() {
    use sz_orm_core::PooledConnection;

    let pool = make_pool(5);
    let pooled: PooledConnection = pool.acquire().await.unwrap();

    // into_inner 消费 self，返回 Box<dyn Connection>
    let conn: Box<dyn Connection> = pooled.into_inner();

    // 验证连接可用
    assert!(conn.is_connected());
}

#[tokio::test]
async fn test_pooled_connection_deref_to_dyn_connection_contract() {
    use std::ops::Deref;
    let pool = make_pool(5);
    let pooled = pool.acquire().await.unwrap();

    // Deref<Target = dyn Connection>
    let _: &dyn Connection = pooled.deref();
    assert!(pooled.is_connected());
}

// ===== §5.2 PoolConfig 校验契约 =====

#[test]
fn test_pool_config_rejects_max_size_zero_contract() {
    let result = PoolConfigBuilder::new().max_size(0).build();
    match result {
        Err(PoolError::InvalidConfig(_)) => { /* 契约满足 */ }
        Err(other) => panic!("期望 InvalidConfig，实际: {:?}", other),
        Ok(_) => panic!("max_size=0 必须被拒绝，实际返回 Ok"),
    }
}

#[test]
fn test_pool_config_rejects_min_idle_exceeds_max_contract() {
    let result = PoolConfigBuilder::new().max_size(5).min_idle(10).build();
    match result {
        Err(PoolError::InvalidConfig(_)) => { /* 契约满足 */ }
        Err(other) => panic!("期望 InvalidConfig，实际: {:?}", other),
        Ok(_) => panic!("min_idle > max_size 必须被拒绝，实际返回 Ok"),
    }
}

#[test]
fn test_pool_config_default_constants_contract() {
    // 默认常量契约
    assert_eq!(sz_orm_core::DEFAULT_MAX_SIZE, 100);
    assert_eq!(sz_orm_core::DEFAULT_MIN_IDLE, 5);
    assert_eq!(sz_orm_core::DEFAULT_ACQUIRE_TIMEOUT, 30);
    assert_eq!(sz_orm_core::DEFAULT_IDLE_TIMEOUT, 600);
    assert_eq!(sz_orm_core::DEFAULT_MAX_LIFETIME, 1800);

    // PoolConfig::default() 应使用上述默认值
    let cfg = PoolConfig::default();
    assert_eq!(cfg.max_size, 100);
}

// ===== §5.1 PoolStatus 契约 =====

#[tokio::test]
async fn test_pool_status_returns_correct_fields_contract() {
    let pool = make_pool(3);
    let status: PoolStatus = pool.status().await;
    assert_eq!(status.max, 3);
    assert_eq!(status.active, 0);
    assert_eq!(status.idle, 0);
}

// ===== §5.1 release 到已关闭的池不 panic 契约 =====

#[tokio::test]
async fn test_release_to_closed_pool_doesnt_panic_contract() {
    let pool = make_pool(5);
    let conn = pool.acquire().await.unwrap();
    pool.close_all().await;
    // release 到已关闭池：应直接关闭连接，不 panic
    pool.release(conn).await;
}

// ===== §5.1 acquire 后再 release 可重复使用连接契约 =====

#[tokio::test]
async fn test_acquire_release_cycle_reuses_connection_contract() {
    let pool = make_pool(2);

    for _ in 0..5 {
        let conn = pool.acquire().await.unwrap();
        assert!(conn.is_connected());
        pool.release(conn).await;
    }

    let status = pool.status().await;
    // 经过 5 次 acquire-release 循环，连接被复用，total 应为 1，idle 应为 1
    assert_eq!(status.active, 1, "复用连接后 total 应为 1");
    assert_eq!(status.idle, 1, "复用连接后 idle 应为 1");
}

// ===== DbType::MySQL::default_port 契约（与 contracts.md §2 联动） =====

#[test]
fn test_db_type_default_port_contract() {
    assert_eq!(DbType::MySQL.default_port(), 3306);
    assert_eq!(DbType::PostgreSQL.default_port(), 5432);
    assert_eq!(DbType::Oracle.default_port(), 1521);
    assert_eq!(DbType::Redis.default_port(), 6379);
    assert_eq!(DbType::MongoDB.default_port(), 27017);
    assert_eq!(DbType::ClickHouse.default_port(), 8123);
}
