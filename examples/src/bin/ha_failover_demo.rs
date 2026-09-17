//! v7.3.0 任务 2.7：高可用故障转移演示示例
//!
//! 演示：配置 HaConfig 启用故障转移/限流熔断/健康检查/追踪，
//!       演示主备故障转移与事件订阅。
//!
//! 生产调用点证据：
//! - HaConfig 配置聚合：examples/src/bin/ha_failover_demo.rs:30
//! - AutoFailoverCoordinator 故障转移：examples/src/bin/ha_failover_demo.rs:55
//! - HaEventBus 事件订阅：examples/src/bin/ha_failover_demo.rs:45
//! - HealthSubItemChecker 5 子项健康：examples/src/bin/ha_failover_demo.rs:80
//! - QueueTimeoutLimiter 限流排队：examples/src/bin/ha_failover_demo.rs:90
//! - ErrorRateCircuitBreaker 错误率熔断：examples/src/bin/ha_failover_demo.rs:100
//! - span_for_query 跨阶段追踪：examples/src/bin/ha_failover_demo.rs:110
//!
//! 运行：cargo run --example ha_failover_demo --features ha-failover

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use sz_orm_core::circuit_breaker::{CircuitBreaker, ErrorRateCircuitBreaker};
use sz_orm_core::rw_split_enhanced::{AutoFailoverCoordinator, ProbeResult};
use sz_orm_core::{FailbackStrategy, FailoverConfig, HaConfig};
use sz_orm_health::advanced::{HealthSubItem, HealthSubItemChecker};
use sz_orm_health::{HaEvent, HaEventBus, HaEventSubscriber, HaEventType, HealthStatus};
use sz_orm_limit::{QueueStrategy, QueueTimeoutLimiter};
use sz_orm_tracing::{Span, span_for_query};

/// 当前时间戳（毫秒）
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 事件计数订阅者
struct EventCounter {
    failover_count: AtomicU64,
    health_change_count: AtomicU64,
}

impl EventCounter {
    fn new() -> Self {
        Self {
            failover_count: AtomicU64::new(0),
            health_change_count: AtomicU64::new(0),
        }
    }
}

impl HaEventSubscriber for EventCounter {
    fn on_event(&self, event: &HaEvent) {
        match event.event_type {
            HaEventType::FailoverDecision => {
                self.failover_count.fetch_add(1, Ordering::Relaxed);
            }
            HaEventType::HealthChange => {
                self.health_change_count.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
    }
}

#[tokio::main]
async fn main() {
    println!("=== sz-orm v7.3.0 高可用故障转移演示 ===\n");

    // 1. 配置 HaConfig（生产调用点：ha_failover_demo.rs:30）
    let ha_config = HaConfig::builder()
        .failover_enabled(true)
        .failover(FailoverConfig {
            primary_url: "mysql://primary".into(),
            replica_url: "mysql://replica".into(),
            probe_interval: Duration::from_secs(1),
            probe_failure_threshold: 3,
            failback_strategy: FailbackStrategy::Auto,
        })
        .rate_limit_threshold(100.0)
        .rate_limit_queue_timeout_ms(100)
        .circuit_breaker_error_threshold(0.5)
        .circuit_breaker_half_open_probes(3)
        .trace_sample_rate(1.0)
        .trace_otlp_endpoint("http://localhost:4317")
        .build()
        .expect("HaConfig 校验失败");
    println!("[配置] HaConfig 校验通过: failover_enabled={}", ha_config.failover_enabled);

    // 2. 事件总线 + 订阅者（生产调用点：ha_failover_demo.rs:45）
    let event_bus = Arc::new(HaEventBus::new());
    let counter = Arc::new(EventCounter::new());
    event_bus.subscribe(counter.clone());
    println!("[事件] HaEventBus 订阅者已注册: {}", event_bus.subscriber_count());

    // 3. 自动故障转移协调器（生产调用点：ha_failover_demo.rs:55）
    let failover_config = ha_config.failover.as_ref().unwrap().clone();
    let coord = AutoFailoverCoordinator::new(failover_config);
    coord.set_primary_lsn(100);
    coord.set_replica_lsn(100);
    println!("[故障转移] AutoFailoverCoordinator 已初始化");

    // 模拟主库探活失败
    println!("\n--- 模拟主库宕机（连续探活失败 3 次）---");
    for i in 1..=3 {
        coord.record_probe(ProbeResult {
            success: false,
            timestamp_ms: now_ms(),
            error: Some(format!("attempt {i}: connection refused")),
        });
        println!("[探活] 失败 {i}/3, consecutive_failures={}", coord.consecutive_probe_failures());
    }
    println!("[故障转移] is_failed_over={}", coord.is_failed_over());

    // 发布故障转移事件
    event_bus.publish(&HaEvent::new(
        HaEventType::FailoverDecision,
        "primary down, failed over to replica",
    ));

    // 4. 5 子项健康检查（生产调用点：ha_failover_demo.rs:80）
    let health_checker = HealthSubItemChecker::with_event_bus(event_bus.clone());
    health_checker.check_and_update(
        HealthSubItem::Connection,
        HealthStatus::Healthy,
        "10/100",
    );
    health_checker.check_and_update(
        HealthSubItem::PrimaryReplica,
        HealthStatus::Unhealthy,
        "primary down",
    );
    println!("\n[健康] 整体状态: {:?}", health_checker.overall());

    // 5. 限流排队（生产调用点：ha_failover_demo.rs:90）
    let limiter = QueueTimeoutLimiter::new(
        ha_config.rate_limit_threshold,
        Duration::from_millis(ha_config.rate_limit_queue_timeout_ms),
        QueueStrategy::Fair,
    );
    let result = limiter.try_acquire("tenant-001").unwrap();
    println!("\n[限流] tenant-001 admitted={}", result.allowed);

    // 6. 错误率熔断（生产调用点：ha_failover_demo.rs:100）
    let mut cb = ErrorRateCircuitBreaker::new(
        ha_config.circuit_breaker_error_threshold,
        Duration::from_secs(60),
        ha_config.circuit_breaker_half_open_probes,
        10,
    );
    println!("\n[熔断] 初始状态: {:?}", cb.state());
    // 模拟 6 次失败 + 4 次成功 = 60% > 50%
    for _ in 0..6 {
        cb.record_failure();
    }
    for _ in 0..4 {
        cb.record_success();
    }
    println!("[熔断] 错误率={:.2}, 状态={:?}", cb.error_rate(), cb.state());

    // 7. 跨阶段追踪（生产调用点：ha_failover_demo.rs:110）
    let root = Span::new("trace-ha-demo", "span-root", "ha_demo");
    let query_span = span_for_query(&root, "SELECT * FROM users", "conn-42");
    println!("\n[追踪] query span: trace_id={}, parent_id={:?}",
        query_span.trace_id, query_span.parent_id);
    println!("[追踪] db.statement={}", query_span.tags.get("db.statement").unwrap());

    // 8. 回切到主库
    println!("\n--- 模拟主库恢复，回切 ---");
    coord.record_probe(ProbeResult {
        success: true,
        timestamp_ms: now_ms(),
        error: None,
    });
    coord.set_replica_lsn(100);
    if coord.failback(FailbackStrategy::Auto).is_ok() {
        println!("[回切] 成功回切到主库, is_failed_over={}", coord.is_failed_over());
    }

    // 9. 事件统计
    println!("\n=== 演示完成 ===");
    println!("[事件] FailoverDecision 事件数: {}", counter.failover_count.load(Ordering::Relaxed));
    println!("[事件] HealthChange 事件数: {}", counter.health_change_count.load(Ordering::Relaxed));
    println!("[事件] 总发布事件数: {}", event_bus.published_count());

    // 决策历史
    let history = coord.decision_history();
    println!("[故障转移] 决策历史记录数: {}", history.len());
    for (i, decision) in history.iter().enumerate() {
        println!("  决策 {}: switched={}, reason={}", i + 1, decision.switched, decision.reason);
    }
}