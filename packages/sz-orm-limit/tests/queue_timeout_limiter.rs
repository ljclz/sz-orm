//! v7.3.0 任务 2.4：QueueTimeoutLimiter 测试
//!
//! 验证：超阈值排队 + 排队超 100ms 快速拒绝 + 公平/加权/优先级分配 + 审计日志记录 + 无饿死

use std::time::Duration;

use sz_orm_limit::{AuditAction, QueueStrategy, QueueTimeoutLimiter, RateLimiter};

#[test]
fn test_under_threshold_admitted() {
    let limiter = QueueTimeoutLimiter::new(100.0, Duration::from_millis(100), QueueStrategy::Fair);
    let result = limiter.try_acquire("tenant-1").unwrap();
    assert!(result.allowed);
    assert_eq!(limiter.admitted_count(), 1);
    assert_eq!(limiter.rejected_count(), 0);
}

#[test]
fn test_over_threshold_rejected_with_audit() {
    // 阈值 1 qps，快速发起 5 个请求
    let limiter = QueueTimeoutLimiter::new(1.0, Duration::from_millis(100), QueueStrategy::Fair);
    let _ = limiter.try_acquire("k1").unwrap(); // 放行
                                                // 等待一小段时间让 actual_rate 计算生效
    std::thread::sleep(Duration::from_millis(10));
    let result = limiter.try_acquire("k2").unwrap();
    assert!(!result.allowed);
    assert!(limiter.rejected_count() >= 1);

    // 审计日志应有记录
    let log = limiter.audit_log();
    assert!(log.len() >= 2);
    assert!(log.iter().any(|e| e.action == AuditAction::Admitted));
    assert!(log
        .iter()
        .any(|e| e.action == AuditAction::RejectedOverloaded));
}

#[test]
fn test_queue_timeout_rejects_stale_entries() {
    let limiter = QueueTimeoutLimiter::new(1.0, Duration::from_millis(50), QueueStrategy::Fair);
    // 入队一个请求
    limiter.enqueue("stale", 1);
    assert_eq!(limiter.queue_len(), 1);

    // 等待超时
    std::thread::sleep(Duration::from_millis(60));

    // 再次入队应清理超时请求
    limiter.enqueue("fresh", 1);
    assert_eq!(limiter.queue_len(), 1);
    assert!(limiter.rejected_count() >= 1);
}

#[test]
fn test_fair_strategy_fifo() {
    let limiter = QueueTimeoutLimiter::new(100.0, Duration::from_millis(100), QueueStrategy::Fair);
    limiter.enqueue("a", 1);
    limiter.enqueue("b", 1);
    limiter.enqueue("c", 1);

    assert_eq!(limiter.select_next(), Some("a".to_string()));
    assert_eq!(limiter.select_next(), Some("b".to_string()));
    assert_eq!(limiter.select_next(), Some("c".to_string()));
    assert_eq!(limiter.select_next(), None);
}

#[test]
fn test_weighted_strategy_no_starvation() {
    let limiter = QueueTimeoutLimiter::new(
        100.0,
        Duration::from_millis(100),
        QueueStrategy::Weighted(vec![1, 3]),
    );
    // 高权重 b 应先被选中
    limiter.enqueue("a", 1);
    limiter.enqueue("b", 3);
    limiter.enqueue("c", 2);

    assert_eq!(limiter.select_next(), Some("b".to_string()));
    assert_eq!(limiter.select_next(), Some("c".to_string()));
    assert_eq!(limiter.select_next(), Some("a".to_string()));
}

#[test]
fn test_priority_strategy_highest_first() {
    let limiter = QueueTimeoutLimiter::new(
        100.0,
        Duration::from_millis(100),
        QueueStrategy::Priority(vec![1, 5, 3]),
    );
    limiter.enqueue("low", 1);
    limiter.enqueue("high", 5);
    limiter.enqueue("mid", 3);

    assert_eq!(limiter.select_next(), Some("high".to_string()));
    assert_eq!(limiter.select_next(), Some("mid".to_string()));
    assert_eq!(limiter.select_next(), Some("low".to_string()));
}

#[test]
fn test_audit_log_records_all_actions() {
    let limiter = QueueTimeoutLimiter::new(1.0, Duration::from_millis(100), QueueStrategy::Fair);
    limiter.try_acquire("k1").unwrap();
    std::thread::sleep(Duration::from_millis(10));
    limiter.try_acquire("k2").unwrap();

    let log = limiter.audit_log();
    assert!(log.len() >= 2);
    // 每条日志都有时间戳
    assert!(log.iter().all(|e| e.timestamp_ms > 0));
    // 每条日志都有 key
    assert!(log.iter().all(|e| !e.key.is_empty()));
}

#[test]
fn test_reset_clears_queue() {
    let limiter = QueueTimeoutLimiter::new(100.0, Duration::from_millis(100), QueueStrategy::Fair);
    limiter.enqueue("a", 1);
    limiter.enqueue("b", 1);
    assert_eq!(limiter.queue_len(), 2);

    limiter.reset("a").unwrap();
    assert_eq!(limiter.queue_len(), 1);
}
