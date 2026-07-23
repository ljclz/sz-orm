//! # 保留消息存储（Retained Messages）
//!
//! 实现 MQTT 保留消息功能：broker 为每个主题保存最新一条 retain=true 的消息，
//! 新订阅者连接时立即收到匹配的保留消息。
//!
//! ## 主要类型
//!
//! - [`RetainedMessage`] — 保留消息条目
//! - [`RetainedStore`] — 保留消息存储（按主题精确索引，支持通配符匹配查询）

use crate::broker::{MqttMessage, QoS};
use crate::topics::topic_matches;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 保留消息条目
#[derive(Debug, Clone)]
pub struct RetainedMessage {
    /// 主题（保留消息按主题存储，每个主题仅保留最新一条）
    pub topic: String,
    /// 载荷
    pub payload: Vec<u8>,
    /// QoS
    pub qos: QoS,
    /// 存储时间戳（毫秒）
    pub stored_at: i64,
}

impl RetainedMessage {
    pub fn new(topic: impl Into<String>, payload: Vec<u8>, qos: QoS) -> Self {
        Self {
            topic: topic.into(),
            payload,
            qos,
            stored_at: now_ms(),
        }
    }

    /// 文本载荷
    pub fn text_payload(&self) -> Option<&str> {
        std::str::from_utf8(&self.payload).ok()
    }

    /// 转换为 MqttMessage（retain=true）
    pub fn to_mqtt_message(&self) -> MqttMessage {
        MqttMessage::new(self.topic.clone(), self.payload.clone())
            .with_qos(self.qos)
            .retain()
    }
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// 保留消息存储
#[derive(Debug, Default)]
pub struct RetainedStore {
    messages: Arc<RwLock<HashMap<String, RetainedMessage>>>,
}

impl RetainedStore {
    pub fn new() -> Self {
        Self {
            messages: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 存储一条保留消息。若该主题已有保留消息，将被覆盖。
    /// 注意：根据 MQTT 规范，零字节载荷的保留消息应删除该主题的保留消息。
    pub async fn store(
        &self,
        topic: impl Into<String>,
        payload: Vec<u8>,
        qos: QoS,
    ) -> StoreAction {
        let topic = topic.into();
        // 零字节载荷 -> 删除该主题的保留消息
        if payload.is_empty() {
            let mut messages = self.messages.write().await;
            let removed = messages.remove(&topic);
            return StoreAction::Replaced {
                topic,
                previous: removed,
            };
        }
        let msg = RetainedMessage::new(topic.clone(), payload, qos);
        let mut messages = self.messages.write().await;
        let previous = messages.insert(topic, msg);
        StoreAction::Replaced {
            topic: String::new(), // 已移入 msg
            previous,
        }
    }

    /// 直接从 MqttMessage 存储（仅当 retain=true 时才存储）
    pub async fn store_from_message(&self, msg: &MqttMessage) -> StoreAction {
        if !msg.retain {
            return StoreAction::IgnoredNotRetained;
        }
        self.store(&msg.topic, msg.payload.clone(), msg.qos).await
    }

    /// 获取指定主题的保留消息（精确匹配）
    pub async fn get(&self, topic: &str) -> Option<RetainedMessage> {
        let messages = self.messages.read().await;
        messages.get(topic).cloned()
    }

    /// 删除指定主题的保留消息
    pub async fn remove(&self, topic: &str) -> Option<RetainedMessage> {
        let mut messages = self.messages.write().await;
        messages.remove(topic)
    }

    /// 清空所有保留消息
    pub async fn clear(&self) {
        let mut messages = self.messages.write().await;
        messages.clear();
    }

    /// 当前保留消息数量
    pub async fn count(&self) -> usize {
        let messages = self.messages.read().await;
        messages.len()
    }

    /// 查询匹配给定订阅过滤器（支持通配符）的所有保留消息
    pub async fn matching(&self, filter: &str) -> Vec<RetainedMessage> {
        let messages = self.messages.read().await;
        let mut matched: Vec<RetainedMessage> = messages
            .values()
            .filter(|m| topic_matches(&m.topic, filter))
            .cloned()
            .collect();
        // 按主题排序以保证测试稳定性
        matched.sort_by(|a, b| a.topic.cmp(&b.topic));
        matched
    }

    /// 返回所有保留消息（按主题排序）
    pub async fn all(&self) -> Vec<RetainedMessage> {
        let messages = self.messages.read().await;
        let mut all: Vec<RetainedMessage> = messages.values().cloned().collect();
        all.sort_by(|a, b| a.topic.cmp(&b.topic));
        all
    }

    /// 查询指定主题是否存在保留消息
    pub async fn contains(&self, topic: &str) -> bool {
        let messages = self.messages.read().await;
        messages.contains_key(topic)
    }

    /// 列出所有保留消息的主题（排序）
    pub async fn topics(&self) -> Vec<String> {
        let messages = self.messages.read().await;
        let mut topics: Vec<String> = messages.keys().cloned().collect();
        topics.sort();
        topics
    }
}

/// 存储动作的返回结果
#[derive(Debug)]
pub enum StoreAction {
    /// 新增或覆盖了已有保留消息（previous 为被覆盖的旧消息）
    Replaced {
        topic: String,
        previous: Option<RetainedMessage>,
    },
    /// 消息非 retain，未存储
    IgnoredNotRetained,
}

impl StoreAction {
    /// 是否实际写入了存储
    pub fn stored(&self) -> bool {
        matches!(self, StoreAction::Replaced { .. })
    }

    /// 是否覆盖了已有消息
    pub fn replaced_existing(&self) -> bool {
        matches!(self, StoreAction::Replaced { previous: Some(_), .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_store_new_message() {
        let store = RetainedStore::new();
        let action = store
            .store("config/version", b"1.0".to_vec(), QoS::AtLeastOnce)
            .await;
        assert!(action.stored());
        assert!(!action.replaced_existing());
        assert_eq!(store.count().await, 1);
    }

    #[tokio::test]
    async fn test_store_overwrites_previous() {
        let store = RetainedStore::new();
        store
            .store("t", b"v1".to_vec(), QoS::AtMostOnce)
            .await;
        let action = store
            .store("t", b"v2".to_vec(), QoS::AtLeastOnce)
            .await;
        assert!(action.replaced_existing());
        let msg = store.get("t").await.expect("should exist");
        assert_eq!(msg.text_payload(), Some("v2"));
        assert_eq!(msg.qos, QoS::AtLeastOnce);
        assert_eq!(store.count().await, 1);
    }

    #[tokio::test]
    async fn test_store_empty_payload_removes_topic() {
        let store = RetainedStore::new();
        store
            .store("t", b"v1".to_vec(), QoS::AtMostOnce)
            .await;
        assert_eq!(store.count().await, 1);
        // 零字节载荷删除
        store.store("t", vec![], QoS::AtMostOnce).await;
        assert_eq!(store.count().await, 0);
        assert!(!store.contains("t").await);
    }

    #[tokio::test]
    async fn test_store_from_message_retain_true() {
        let store = RetainedStore::new();
        let msg = MqttMessage::new("t", b"p".to_vec())
            .with_qos(QoS::ExactlyOnce)
            .retain();
        let action = store.store_from_message(&msg).await;
        assert!(action.stored());
        assert_eq!(store.count().await, 1);
    }

    #[tokio::test]
    async fn test_store_from_message_retain_false_ignored() {
        let store = RetainedStore::new();
        let msg = MqttMessage::new("t", b"p".to_vec()); // retain=false
        let action = store.store_from_message(&msg).await;
        assert!(!action.stored());
        assert_eq!(store.count().await, 0);
    }

    #[tokio::test]
    async fn test_get_returns_none_for_missing() {
        let store = RetainedStore::new();
        assert!(store.get("nope").await.is_none());
    }

    #[tokio::test]
    async fn test_remove() {
        let store = RetainedStore::new();
        store
            .store("t", b"v".to_vec(), QoS::AtMostOnce)
            .await;
        let removed = store.remove("t").await;
        assert!(removed.is_some());
        assert_eq!(store.count().await, 0);
    }

    #[tokio::test]
    async fn test_remove_missing_returns_none() {
        let store = RetainedStore::new();
        assert!(store.remove("nope").await.is_none());
    }

    #[tokio::test]
    async fn test_clear() {
        let store = RetainedStore::new();
        store
            .store("a", b"1".to_vec(), QoS::AtMostOnce)
            .await;
        store
            .store("b", b"2".to_vec(), QoS::AtMostOnce)
            .await;
        assert_eq!(store.count().await, 2);
        store.clear().await;
        assert_eq!(store.count().await, 0);
    }

    #[tokio::test]
    async fn test_contains() {
        let store = RetainedStore::new();
        store
            .store("t", b"v".to_vec(), QoS::AtMostOnce)
            .await;
        assert!(store.contains("t").await);
        assert!(!store.contains("other").await);
    }

    #[tokio::test]
    async fn test_topics_sorted() {
        let store = RetainedStore::new();
        store
            .store("c", b"1".to_vec(), QoS::AtMostOnce)
            .await;
        store
            .store("a", b"2".to_vec(), QoS::AtMostOnce)
            .await;
        store
            .store("b", b"3".to_vec(), QoS::AtMostOnce)
            .await;
        assert_eq!(store.topics().await, vec!["a", "b", "c"]);
    }

    #[tokio::test]
    async fn test_matching_exact_filter() {
        let store = RetainedStore::new();
        store
            .store("home/temp", b"23".to_vec(), QoS::AtMostOnce)
            .await;
        store
            .store("office/temp", b"25".to_vec(), QoS::AtMostOnce)
            .await;
        let matched = store.matching("home/temp").await;
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].topic, "home/temp");
    }

    #[tokio::test]
    async fn test_matching_plus_wildcard() {
        let store = RetainedStore::new();
        store
            .store("home/living/temp", b"23".to_vec(), QoS::AtMostOnce)
            .await;
        store
            .store("home/kitchen/temp", b"20".to_vec(), QoS::AtMostOnce)
            .await;
        store
            .store("office/temp", b"25".to_vec(), QoS::AtMostOnce)
            .await;
        let matched = store.matching("home/+/temp").await;
        assert_eq!(matched.len(), 2);
        assert_eq!(matched[0].topic, "home/kitchen/temp");
        assert_eq!(matched[1].topic, "home/living/temp");
    }

    #[tokio::test]
    async fn test_matching_hash_wildcard() {
        let store = RetainedStore::new();
        store
            .store("home/living/temp", b"1".to_vec(), QoS::AtMostOnce)
            .await;
        store
            .store("home/kitchen/humidity", b"2".to_vec(), QoS::AtMostOnce)
            .await;
        store
            .store("office/temp", b"3".to_vec(), QoS::AtMostOnce)
            .await;
        let matched = store.matching("home/#").await;
        assert_eq!(matched.len(), 2);
    }

    #[tokio::test]
    async fn test_matching_root_hash() {
        let store = RetainedStore::new();
        store
            .store("a/b/c", b"1".to_vec(), QoS::AtMostOnce)
            .await;
        store
            .store("x/y", b"2".to_vec(), QoS::AtMostOnce)
            .await;
        let matched = store.matching("#").await;
        assert_eq!(matched.len(), 2);
    }

    #[tokio::test]
    async fn test_matching_no_results() {
        let store = RetainedStore::new();
        store
            .store("a/b", b"1".to_vec(), QoS::AtMostOnce)
            .await;
        let matched = store.matching("c/#").await;
        assert!(matched.is_empty());
    }

    #[tokio::test]
    async fn test_all_sorted_by_topic() {
        let store = RetainedStore::new();
        store
            .store("z", b"1".to_vec(), QoS::AtMostOnce)
            .await;
        store
            .store("a", b"2".to_vec(), QoS::AtMostOnce)
            .await;
        store
            .store("m", b"3".to_vec(), QoS::AtMostOnce)
            .await;
        let all = store.all().await;
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].topic, "a");
        assert_eq!(all[1].topic, "m");
        assert_eq!(all[2].topic, "z");
    }

    #[tokio::test]
    async fn test_retained_message_to_mqtt_message_sets_retain() {
        let rm = RetainedMessage::new("t", b"p".to_vec(), QoS::ExactlyOnce);
        let msg = rm.to_mqtt_message();
        assert!(msg.retain);
        assert_eq!(msg.topic, "t");
        assert_eq!(msg.qos, QoS::ExactlyOnce);
    }

    #[tokio::test]
    async fn test_store_action_replaced_existing_with_topic() {
        let store = RetainedStore::new();
        let action = store
            .store("t", b"v1".to_vec(), QoS::AtMostOnce)
            .await;
        assert!(!action.replaced_existing());
        let action2 = store
            .store("t", b"v2".to_vec(), QoS::AtMostOnce)
            .await;
        assert!(action2.replaced_existing());
    }

    #[tokio::test]
    async fn test_store_from_message_empty_payload_removes() {
        let store = RetainedStore::new();
        let msg = MqttMessage::new("t", b"v".to_vec()).retain();
        store.store_from_message(&msg).await;
        assert_eq!(store.count().await, 1);
        // 零字节载荷 + retain=true -> 删除
        let empty = MqttMessage::new("t", vec![]).retain();
        store.store_from_message(&empty).await;
        assert_eq!(store.count().await, 0);
    }

    #[test]
    fn test_retained_message_text_payload() {
        let rm = RetainedMessage::new("t", b"hello".to_vec(), QoS::AtMostOnce);
        assert_eq!(rm.text_payload(), Some("hello"));
    }

    #[test]
    fn test_retained_message_text_payload_invalid_utf8() {
        let rm = RetainedMessage::new("t", vec![0xFF, 0xFE], QoS::AtMostOnce);
        assert_eq!(rm.text_payload(), None);
    }
}
