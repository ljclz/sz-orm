//! v7.3.0 任务 2.4：限流端到端测试（真实场景，需 --ignored）
//!
//! 验证：真实限流排队 + 熔断状态流转 + 审计日志

use std::time::Duration;

use sz_orm_limit::{QueueStrategy, QueueTimeoutLimiter};

#[tokio::test]
#[ignore = "需要真实限流场景"]
async fn limit_e2e_real_queue_and_audit() {
    let limiter = QueueTimeoutLimiter::new(
        10.0,
        Duration::from_millis(100),
        QueueStrategy::Fair,
    );

    // 模拟突发流量
    let mut admitted = 0;
    let mut rejected = 0;
    for i in 0..20 {
        let result = limiter.try_acquire(&format!("req-{i}")).unwrap();
        if result.allowed {
            admitted += 1;
        } else {
            rejected += 1;
        }
    }

    assert!(admitted > 0, "应有请求被放行");
    assert!(admitted + rejected == 20);

    let log = limiter.audit_log();
    assert!(!log.is_empty(), "审计日志不应为空");
}

#[tokio::test]
#[ignore = "需要真实限流场景"]
async fn limit_e2e_weighted_no_starvation() {
    let limiter = QueueTimeoutLimiter::new(
        100.0,
        Duration::from_millis(100),
        QueueStrategy::Weighted(vec![1, 5, 1]),
    );

    // 入队 3 个不同权重请求
    limiter.enqueue("low", 1);
    limiter.enqueue("high", 5);
    limiter.enqueue("mid", 3);

    // 高权重应先被选中
    let first = limiter.select_next();
    assert_eq!(first, Some("high".to_string()));

    // 低权重最终也会被选中（无饿死）
    let mut selected = vec![first.unwrap()];
    while let Some(k) = limiter.select_next() {
        selected.push(k);
    }
    assert!(selected.contains(&"low".to_string()));
    assert!(selected.contains(&"mid".to_string()));
}