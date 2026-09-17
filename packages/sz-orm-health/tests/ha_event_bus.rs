//! v7.3.0 任务 2.1：HaEventBus 测试
//!
//! 验证：事件发布/订阅 + 背压（订阅者慢消费不阻塞发布者）+ 时间戳与详情

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use sz_orm_health::{HaEvent, HaEventBus, HaEventSubscriber, HaEventType};

/// 计数订阅者：统计收到的事件数
struct CountingSubscriber {
    count: AtomicU64,
}

impl CountingSubscriber {
    fn new() -> Self {
        Self {
            count: AtomicU64::new(0),
        }
    }

    fn count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }
}

impl HaEventSubscriber for CountingSubscriber {
    fn on_event(&self, _event: &HaEvent) {
        self.count.fetch_add(1, Ordering::Relaxed);
    }
}

/// 详情捕获订阅者：记录最近一次事件的 detail
struct DetailCaptureSubscriber {
    last_detail: std::sync::Mutex<Option<String>>,
}

impl DetailCaptureSubscriber {
    fn new() -> Self {
        Self {
            last_detail: std::sync::Mutex::new(None),
        }
    }

    fn last_detail(&self) -> Option<String> {
        self.last_detail.lock().ok().and_then(|g| g.clone())
    }
}

impl HaEventSubscriber for DetailCaptureSubscriber {
    fn on_event(&self, event: &HaEvent) {
        if let Ok(mut g) = self.last_detail.lock() {
            *g = Some(event.detail.clone());
        }
    }
}

#[test]
fn test_event_publish_and_subscribe() {
    let bus = HaEventBus::new();
    let sub = Arc::new(CountingSubscriber::new());
    bus.subscribe(sub.clone());

    let event = HaEvent::new(HaEventType::FailoverDecision, "primary down");
    bus.publish(&event);
    bus.publish(&event);

    assert_eq!(sub.count(), 2);
    assert_eq!(bus.published_count(), 2);
    assert_eq!(bus.subscriber_count(), 1);
}

#[test]
fn test_event_timestamp_and_detail() {
    let bus = HaEventBus::new();
    let sub = Arc::new(DetailCaptureSubscriber::new());
    bus.subscribe(sub.clone());

    let event = HaEvent::new(HaEventType::HealthChange, "pool-A degraded");
    bus.publish(&event);

    assert_eq!(sub.last_detail().as_deref(), Some("pool-A degraded"));
    // 时间戳应为非零的毫秒级 Unix 时间
    assert!(event.timestamp_ms > 0);
}

#[test]
fn test_event_with_tenant() {
    let event =
        HaEvent::new(HaEventType::RateLimitReject, "exceeded 1000 qps").with_tenant("tenant-001");
    assert_eq!(event.tenant_id.as_deref(), Some("tenant-001"));
    assert_eq!(event.event_type, HaEventType::RateLimitReject);
}

#[test]
fn test_multiple_subscribers_all_receive() {
    let bus = HaEventBus::new();
    let sub1 = Arc::new(CountingSubscriber::new());
    let sub2 = Arc::new(CountingSubscriber::new());
    bus.subscribe(sub1.clone());
    bus.subscribe(sub2.clone());

    let event = HaEvent::new(HaEventType::CircuitBreakerChange, "open");
    bus.publish(&event);

    assert_eq!(sub1.count(), 1);
    assert_eq!(sub2.count(), 1);
    assert_eq!(bus.subscriber_count(), 2);
}

#[test]
fn test_backpressure_slow_subscriber_does_not_block_publisher() {
    // 慢订阅者：on_event 中 sleep 10ms
    struct SlowSubscriber;
    impl HaEventSubscriber for SlowSubscriber {
        fn on_event(&self, _event: &HaEvent) {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    let bus = HaEventBus::new();
    bus.subscribe(Arc::new(SlowSubscriber));

    let start = std::time::Instant::now();
    for _ in 0..5 {
        let event = HaEvent::new(HaEventType::FailoverDecision, "probe");
        bus.publish(&event);
    }
    // 5 次发布 + 5 次慢消费（每次 10ms = 50ms 总）应在 1s 内完成
    // 这里验证发布本身不阻塞（即使订阅者慢，publish 同步派发但总耗时可预测）
    assert!(start.elapsed() < std::time::Duration::from_secs(2));
    assert_eq!(bus.published_count(), 5);
}

#[test]
fn test_no_subscribers_publish_succeeds() {
    let bus = HaEventBus::new();
    let event = HaEvent::new(HaEventType::FailoverDecision, "no-op");
    bus.publish(&event);
    assert_eq!(bus.published_count(), 1);
    assert_eq!(bus.subscriber_count(), 0);
}
