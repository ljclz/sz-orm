//! 端到端测试：同事件 60s 内重复触发 3 次仅执行 1 次
//!
//! 验证 IdempotencyDeduplicator 的 60s 滑动窗口去重机制：
//! 同一事件（相同 event_hash）在窗口内重复触发时，
//! 仅首次执行，后续去重。

use std::time::Duration;

use sz_orm_ai::autonomous::IdempotencyDeduplicator;

/// 验证同事件 60s 内重复触发 3 次仅执行 1 次
#[test]
fn e2e_idempotency_same_event_three_times() {
    let mut dedup = IdempotencyDeduplicator::default();

    assert!(!dedup.is_duplicate(42), "第 1 次触发不应去重");
    assert!(dedup.is_duplicate(42), "第 2 次触发应去重");
    assert!(dedup.is_duplicate(42), "第 3 次触发应去重");
}

/// 验证不同事件不去重
#[test]
fn e2e_idempotency_different_events_not_deduplicated() {
    let mut dedup = IdempotencyDeduplicator::default();

    assert!(!dedup.is_duplicate(100), "事件 A 首次不去重");
    assert!(!dedup.is_duplicate(200), "事件 B 首次不去重");
    assert!(dedup.is_duplicate(100), "事件 A 第 2 次去重");
    assert!(dedup.is_duplicate(200), "事件 B 第 2 次去重");
}

/// 验证容量满时 LRU 淘汰
#[test]
fn e2e_idempotency_capacity_eviction() {
    let mut dedup = IdempotencyDeduplicator::new(Duration::from_secs(60), 3);

    assert!(!dedup.is_duplicate(1), "e1 首次不去重");
    assert!(!dedup.is_duplicate(2), "e2 首次不去重");
    assert!(!dedup.is_duplicate(3), "e3 首次不去重");
    assert!(!dedup.is_duplicate(4), "e4 首次不去重（容量满淘汰 e1）");
    assert!(!dedup.is_duplicate(1), "e1 被淘汰后重新不去重");
}

/// 验证窗口过期后同事件不去重
#[test]
fn e2e_idempotency_window_expired() {
    let mut dedup = IdempotencyDeduplicator::new(Duration::from_millis(10), 100);

    assert!(!dedup.is_duplicate(42), "首次不去重");
    assert!(dedup.is_duplicate(42), "窗口内去重");

    std::thread::sleep(Duration::from_millis(20));
    assert!(!dedup.is_duplicate(42), "窗口过期后不去重");
}
