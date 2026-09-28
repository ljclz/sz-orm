//! v9.2.0 M6-T31：PoolCircuitBreakerLink 自适应缩容/扩容（3 tests）
//!
//! PoolCircuitBreakerLink 始终可用（不依赖 circuit-breaker feature）。

use sz_orm_core::PoolCircuitBreakerLink;

#[test]
fn test_pool_adaptive_shrink_reduces_capacity() {
    let mut link = PoolCircuitBreakerLink::new(100);
    assert_eq!(link.current_capacity(), 100);
    let new_cap = link.shrink_pool(0.5);
    assert_eq!(new_cap, 50);
    assert_eq!(link.current_capacity(), 50);
    assert!(link.is_shrunk());
    assert_eq!(link.shrink_count(), 1);
}

#[test]
fn test_pool_adaptive_expand_restores_capacity() {
    let mut link = PoolCircuitBreakerLink::new(80);
    link.shrink_pool(0.25);
    assert_eq!(link.current_capacity(), 20);
    let restored = link.expand_pool();
    assert_eq!(restored, 80);
    assert_eq!(link.current_capacity(), 80);
    assert!(!link.is_shrunk());
    assert_eq!(link.expand_count(), 1);
}

#[test]
fn test_pool_adaptive_on_circuit_state_change_toggles() {
    let mut link = PoolCircuitBreakerLink::new(100);
    let cap_after_open = link.on_circuit_state_change(true);
    assert_eq!(cap_after_open, 50);
    assert!(link.is_shrunk());
    let cap_after_closed = link.on_circuit_state_change(false);
    assert_eq!(cap_after_closed, 100);
    assert!(!link.is_shrunk());
    assert_eq!(link.shrink_count(), 1);
    assert_eq!(link.expand_count(), 1);
}