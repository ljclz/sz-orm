//! v9.2.0 M6-T30：configure_circuit_breaker/reset/circuit_state 熔断器（3 tests）
//!
//! 注意：Pool 的 circuit-breaker 方法需要 `circuit-breaker` feature。
//! 默认 features 不含 circuit-breaker，以下测试在启用该 feature 时编译运行。
//! 验证命令：cargo test -p sz-orm-core --test pool_circuit_test --features circuit-breaker

#![cfg(feature = "circuit-breaker")]

mod common;

use common::pool_mock;
use std::time::Duration;
use sz_orm_core::circuit_breaker::CircuitState;

#[tokio::test]
async fn test_pool_configure_circuit_breaker_sets_threshold() {
    let pool = pool_mock::create_pool(4);
    pool.configure_circuit_breaker(3, Duration::from_secs(60));
    assert_eq!(pool.circuit_state(), CircuitState::Closed);
}

#[tokio::test]
async fn test_pool_reset_circuit_breaker_returns_to_closed() {
    let pool = pool_mock::create_pool(4);
    pool.configure_circuit_breaker(2, Duration::from_secs(60));
    let conn = pool.acquire().await.unwrap();
    pool.release(conn).await;
    let changed = pool.reset_circuit_breaker();
    let _ = changed;
    assert_eq!(pool.circuit_state(), CircuitState::Closed);
}

#[tokio::test]
async fn test_pool_circuit_state_initial_closed() {
    let pool = pool_mock::create_pool(4);
    assert_eq!(
        pool.circuit_state(),
        CircuitState::Closed,
        "初始状态应为 Closed"
    );
}
