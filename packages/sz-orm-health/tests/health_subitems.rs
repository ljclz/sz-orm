//! v7.3.0 任务 2.5：HealthSubItem 5 子项测试
//!
//! 验证：5 子项独立判定 + 汇总状态 + 子项变更触发事件 + 订阅者收到事件

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use sz_orm_health::{
    advanced::{HealthSubItem, HealthSubItemChecker},
    HaEvent, HaEventBus, HaEventSubscriber, HaEventType, HealthStatus,
};

/// 计数订阅者
struct CountingSubscriber {
    count: AtomicU64,
}

impl CountingSubscriber {
    fn new() -> Self {
        Self {
            count: AtomicU64::new(0),
        }
    }
}

impl HaEventSubscriber for CountingSubscriber {
    fn on_event(&self, _event: &HaEvent) {
        self.count.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn test_five_sub_items_independent_judgment() {
    let checker = HealthSubItemChecker::new();
    checker.check_and_update(HealthSubItem::Connection, HealthStatus::Healthy, "10/100");
    checker.check_and_update(HealthSubItem::PoolSaturation, HealthStatus::Healthy, "10%");
    checker.check_and_update(
        HealthSubItem::PrimaryReplica,
        HealthStatus::Healthy,
        "lag=0",
    );
    checker.check_and_update(HealthSubItem::CacheHitRate, HealthStatus::Healthy, "95%");
    checker.check_and_update(
        HealthSubItem::RateLimitCircuit,
        HealthStatus::Healthy,
        "closed",
    );

    let report = checker.report();
    assert_eq!(report.sub_item_count(), 5);
    assert_eq!(report.overall, HealthStatus::Healthy);
}

#[test]
fn test_overall_unhealthy_if_any_sub_item_unhealthy() {
    let checker = HealthSubItemChecker::new();
    checker.check_and_update(HealthSubItem::Connection, HealthStatus::Healthy, "ok");
    checker.check_and_update(
        HealthSubItem::PoolSaturation,
        HealthStatus::Unhealthy,
        "95% saturated",
    );

    let report = checker.report();
    assert_eq!(report.overall, HealthStatus::Unhealthy);
}

#[test]
fn test_overall_unknown_if_any_sub_item_unknown() {
    let checker = HealthSubItemChecker::new();
    checker.check_and_update(HealthSubItem::Connection, HealthStatus::Healthy, "ok");
    checker.check_and_update(
        HealthSubItem::CacheHitRate,
        HealthStatus::Unknown,
        "not measured",
    );

    let report = checker.report();
    assert_eq!(report.overall, HealthStatus::Unknown);
}

#[test]
fn test_sub_item_change_triggers_event() {
    let bus = Arc::new(HaEventBus::new());
    let sub = Arc::new(CountingSubscriber::new());
    bus.subscribe(sub.clone());

    let checker = HealthSubItemChecker::with_event_bus(bus);
    checker.check_and_update(HealthSubItem::Connection, HealthStatus::Healthy, "ok");
    checker.check_and_update(
        HealthSubItem::PoolSaturation,
        HealthStatus::Unhealthy,
        "overloaded",
    );

    // 订阅者应收到 2 个事件
    assert_eq!(sub.count.load(Ordering::Relaxed), 2);
}

#[test]
fn test_subscriber_receives_health_change_events() {
    let bus = Arc::new(HaEventBus::new());

    /// 捕获事件类型的订阅者
    struct EventCapture {
        events: std::sync::Mutex<Vec<HaEventType>>,
    }
    impl HaEventSubscriber for EventCapture {
        fn on_event(&self, event: &HaEvent) {
            if let Ok(mut g) = self.events.lock() {
                g.push(event.event_type);
            }
        }
    }

    let capture = Arc::new(EventCapture {
        events: std::sync::Mutex::new(Vec::new()),
    });
    bus.subscribe(capture.clone());

    let checker = HealthSubItemChecker::with_event_bus(bus);
    checker.check_and_update(HealthSubItem::Connection, HealthStatus::Healthy, "ok");
    checker.check_and_update(
        HealthSubItem::PrimaryReplica,
        HealthStatus::Unhealthy,
        "lag=100",
    );

    let events = capture.events.lock().unwrap();
    assert_eq!(events.len(), 2);
    assert!(events.iter().all(|&e| e == HaEventType::HealthChange));
}

#[test]
fn test_report_details_preserved() {
    let checker = HealthSubItemChecker::new();
    checker.check_and_update(
        HealthSubItem::CacheHitRate,
        HealthStatus::Healthy,
        "hit=950 miss=50",
    );

    let report = checker.report();
    let detail = report.details.get(&HealthSubItem::CacheHitRate).unwrap();
    assert!(detail.contains("hit=950"));
    assert!(detail.contains("miss=50"));
}
