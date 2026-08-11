//! Subscription 支持：基于 CDC ChangeEvent 推送数据变更

use std::time::Instant;

use parking_lot::RwLock;

use super::error::TicketError;

/// 订阅事件类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubscriptionEventType {
    UserUpdated,
    UserCreated,
    UserDeleted,
    DataChanged,
}

/// 订阅事件
#[derive(Debug, Clone)]
pub struct SubscriptionEvent {
    pub event_type: SubscriptionEventType,
    pub table: String,
    pub payload: serde_json::Value,
    pub timestamp: Instant,
}

/// 订阅状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubscriptionStatus {
    Active,
    Disconnected,
}

/// 订阅信息
#[derive(Debug, Clone)]
pub struct SubscriptionInfo {
    pub id: String,
    pub event_type: SubscriptionEventType,
    pub status: SubscriptionStatus,
    pub created_at: Instant,
    pub events_sent: u64,
}

/// Subscription 数据源（基于 CDC ChangeEvent）
pub struct SubscriptionSource {
    subscriptions: RwLock<Vec<SubscriptionInfo>>,
    event_buffer: RwLock<Vec<SubscriptionEvent>>,
    buffer_capacity: usize,
}

impl SubscriptionSource {
    pub fn new() -> Self {
        Self::with_capacity(10_000)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            subscriptions: RwLock::new(Vec::new()),
            event_buffer: RwLock::new(Vec::new()),
            buffer_capacity: capacity,
        }
    }

    /// 订阅事件
    pub fn subscribe(&self, event_type: SubscriptionEventType) -> String {
        let id = format!("sub-{}", Instant::now().elapsed().as_nanos());
        let info = SubscriptionInfo {
            id: id.clone(),
            event_type,
            status: SubscriptionStatus::Active,
            created_at: Instant::now(),
            events_sent: 0,
        };
        self.subscriptions.write().push(info);
        id
    }

    /// 取消订阅
    pub fn unsubscribe(&self, sub_id: &str) {
        let mut subs = self.subscriptions.write();
        if let Some(sub) = subs.iter_mut().find(|s| s.id == sub_id) {
            sub.status = SubscriptionStatus::Disconnected;
        }
    }

    /// 推送事件（从 CDC ChangeEvent 转换）
    pub fn push_event(&self, event: SubscriptionEvent) -> Result<(), TicketError> {
        let mut buffer = self.event_buffer.write();
        if buffer.len() >= self.buffer_capacity {
            buffer.remove(0);
        }
        buffer.push(event);
        let mut subs = self.subscriptions.write();
        for sub in subs.iter_mut() {
            if sub.status == SubscriptionStatus::Active {
                sub.events_sent += 1;
            }
        }
        Ok(())
    }

    /// 获取缓冲的事件
    pub fn buffered_events(&self) -> Vec<SubscriptionEvent> {
        self.event_buffer.read().clone()
    }

    /// 获取活跃订阅数
    pub fn active_subscription_count(&self) -> usize {
        self.subscriptions
            .read()
            .iter()
            .filter(|s| s.status == SubscriptionStatus::Active)
            .count()
    }

    /// 获取所有订阅
    pub fn subscriptions(&self) -> Vec<SubscriptionInfo> {
        self.subscriptions.read().clone()
    }

    /// 清理断开的订阅
    pub fn cleanup_disconnected(&self) {
        let mut subs = self.subscriptions.write();
        subs.retain(|s| s.status == SubscriptionStatus::Active);
    }

    /// 从 CDC ChangeEvent 创建 SubscriptionEvent
    pub fn from_cdc_change(op: &str, table: &str, payload: serde_json::Value) -> SubscriptionEvent {
        let event_type = match op {
            "Insert" => SubscriptionEventType::UserCreated,
            "Update" => SubscriptionEventType::UserUpdated,
            "Delete" => SubscriptionEventType::UserDeleted,
            _ => SubscriptionEventType::DataChanged,
        };
        SubscriptionEvent {
            event_type,
            table: table.to_string(),
            payload,
            timestamp: Instant::now(),
        }
    }
}

impl Default for SubscriptionSource {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_subscribe_and_unsubscribe() {
        let source = SubscriptionSource::new();
        let sub_id = source.subscribe(SubscriptionEventType::UserUpdated);
        assert_eq!(source.active_subscription_count(), 1);

        source.unsubscribe(&sub_id);
        assert_eq!(source.active_subscription_count(), 0);
    }

    #[test]
    fn test_push_event() {
        let source = SubscriptionSource::new();
        source.subscribe(SubscriptionEventType::UserUpdated);

        let event = SubscriptionEvent {
            event_type: SubscriptionEventType::UserUpdated,
            table: "users".to_string(),
            payload: json!({"id": 1, "name": "updated"}),
            timestamp: Instant::now(),
        };
        source.push_event(event).unwrap();

        assert_eq!(source.buffered_events().len(), 1);
        let subs = source.subscriptions();
        assert_eq!(subs[0].events_sent, 1);
    }

    #[test]
    fn test_from_cdc_change() {
        let event = SubscriptionSource::from_cdc_change("Update", "users", json!({"id": 1}));
        assert_eq!(event.event_type, SubscriptionEventType::UserUpdated);
        assert_eq!(event.table, "users");

        let event = SubscriptionSource::from_cdc_change("Insert", "users", json!({"id": 1}));
        assert_eq!(event.event_type, SubscriptionEventType::UserCreated);

        let event = SubscriptionSource::from_cdc_change("Delete", "users", json!({"id": 1}));
        assert_eq!(event.event_type, SubscriptionEventType::UserDeleted);
    }

    #[test]
    fn test_buffer_capacity() {
        let source = SubscriptionSource::with_capacity(2);
        for i in 0..3 {
            let event = SubscriptionEvent {
                event_type: SubscriptionEventType::DataChanged,
                table: "t".to_string(),
                payload: json!(i),
                timestamp: Instant::now(),
            };
            source.push_event(event).unwrap();
        }
        assert_eq!(source.buffered_events().len(), 2);
    }

    #[test]
    fn test_cleanup_disconnected() {
        let source = SubscriptionSource::new();
        let id1 = source.subscribe(SubscriptionEventType::UserUpdated);
        let _id2 = source.subscribe(SubscriptionEventType::UserCreated);
        source.unsubscribe(&id1);

        assert_eq!(source.subscriptions().len(), 2);
        source.cleanup_disconnected();
        assert_eq!(source.subscriptions().len(), 1);
    }

    #[test]
    fn test_multiple_subscriptions_receive_events() {
        let source = SubscriptionSource::new();
        source.subscribe(SubscriptionEventType::UserUpdated);
        source.subscribe(SubscriptionEventType::UserUpdated);

        let event = SubscriptionEvent {
            event_type: SubscriptionEventType::UserUpdated,
            table: "users".to_string(),
            payload: json!({}),
            timestamp: Instant::now(),
        };
        source.push_event(event).unwrap();

        let subs = source.subscriptions();
        assert_eq!(subs[0].events_sent, 1);
        assert_eq!(subs[1].events_sent, 1);
    }

    #[test]
    fn test_disconnected_subscription_not_counted() {
        let source = SubscriptionSource::new();
        let id = source.subscribe(SubscriptionEventType::UserUpdated);
        source.unsubscribe(&id);
        assert_eq!(source.active_subscription_count(), 0);
    }
}
