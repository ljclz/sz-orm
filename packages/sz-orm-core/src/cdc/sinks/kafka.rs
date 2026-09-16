//! CDC Kafka Sink — Kafka 投递 + 事件脱敏
//!
//! 将 ChangeEvent 序列化为 JSON 并投递到 Kafka。
//! 使用可注入的 KafkaProducer trait，用户可接入 rdkafka/kafka-rust 等真实客户端。

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::RwLock;
use serde_json::Value;

use super::super::checkpoint::CdcError;
use super::super::dispatcher::CdcSink;
use super::super::event::ChangeEvent;

/// Kafka 生产者 trait（用户注入真实实现）
#[async_trait]
pub trait KafkaProducer: Send + Sync {
    /// 发送消息到指定 topic
    async fn send(&self, topic: &str, key: &str, payload: &str) -> Result<(), String>;
}

/// 内存缓冲生产者（测试用）
pub struct BufferedProducer {
    messages: RwLock<Vec<(String, String, String)>>,
}

impl BufferedProducer {
    /// 创建缓冲生产者
    pub fn new() -> Self {
        Self {
            messages: RwLock::new(Vec::new()),
        }
    }

    /// 获取已发送消息
    pub fn messages(&self) -> Vec<(String, String, String)> {
        self.messages.read().clone()
    }

    /// 消息数
    pub fn count(&self) -> usize {
        self.messages.read().len()
    }
}

impl Default for BufferedProducer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl KafkaProducer for BufferedProducer {
    async fn send(&self, topic: &str, key: &str, payload: &str) -> Result<(), String> {
        self.messages
            .write()
            .push((topic.to_string(), key.to_string(), payload.to_string()));
        Ok(())
    }
}

/// Kafka Sink 配置
#[derive(Debug, Clone)]
pub struct KafkaSinkConfig {
    /// Kafka topic
    pub topic: String,
    /// 脱敏字段名集合
    pub masked_fields: HashSet<String>,
    /// 是否启用脱敏
    pub enable_masking: bool,
}

impl KafkaSinkConfig {
    /// 创建配置
    pub fn new(topic: &str) -> Self {
        Self {
            topic: topic.to_string(),
            masked_fields: HashSet::new(),
            enable_masking: false,
        }
    }

    /// 启用脱敏
    pub fn with_masking(mut self, fields: Vec<String>) -> Self {
        self.enable_masking = true;
        self.masked_fields = fields.into_iter().collect();
        self
    }
}

/// Kafka Sink — 将 ChangeEvent 投递到 Kafka
pub struct KafkaSink {
    producer: Arc<dyn KafkaProducer>,
    config: KafkaSinkConfig,
}

impl KafkaSink {
    /// 创建 Kafka Sink
    pub fn new(producer: Arc<dyn KafkaProducer>, config: KafkaSinkConfig) -> Self {
        Self { producer, config }
    }

    /// 生成消息 key（表名 + 事件 ID）
    fn make_key(&self, event: &ChangeEvent) -> String {
        format!("{}:{}", event.source_table, event.event_id)
    }

    /// 序列化事件为 JSON（可选脱敏）
    fn serialize_event(&self, event: &ChangeEvent) -> String {
        if !self.config.enable_masking {
            return serde_json::to_string(event).unwrap_or_else(|_| "{}".to_string());
        }
        let mut json = serde_json::to_value(event).unwrap_or(Value::Null);
        if let Value::Object(ref mut map) = json {
            if let Some(Value::Object(ref mut row_map)) = map.get_mut("row_data") {
                for field in &self.config.masked_fields {
                    if row_map.contains_key(field) {
                        row_map.insert(field.clone(), Value::String("***".to_string()));
                    }
                }
            }
            map.insert("masked".to_string(), Value::Bool(true));
        }
        serde_json::to_string(&json).unwrap_or_else(|_| "{}".to_string())
    }
}

#[async_trait]
impl CdcSink for KafkaSink {
    async fn handle(&self, event: &ChangeEvent) -> Result<(), CdcError> {
        let key = self.make_key(event);
        let payload = self.serialize_event(event);
        self.producer
            .send(&self.config.topic, &key, &payload)
            .await
            .map_err(CdcError::SinkError)?;
        Ok(())
    }
}

/// 对 ChangeEvent 执行字段脱敏
pub fn mask_event_fields(event: &ChangeEvent, fields: &HashSet<String>) -> ChangeEvent {
    let mut masked = event.clone();
    if let Value::Object(ref mut row) = masked.row_data {
        for field in fields {
            if row.contains_key(field) {
                row.insert(field.clone(), Value::String("***".to_string()));
            }
        }
    }
    masked.masked = true;
    masked
}

#[cfg(test)]
mod tests {
    use super::super::super::event::{ChangeEventType, ChangePosition};
    use super::*;

    fn make_event(table: &str, row_data: Value) -> ChangeEvent {
        ChangeEvent::new(
            ChangeEventType::Insert,
            "source_db",
            table,
            row_data,
            ChangePosition::SqliteHook { seq: 1 },
            1000,
        )
    }

    #[tokio::test]
    async fn test_kafka_sink_basic_send() {
        let producer = Arc::new(BufferedProducer::new());
        let sink = KafkaSink::new(producer.clone(), KafkaSinkConfig::new("test_topic"));
        let event = make_event("users", serde_json::json!({"id": 1, "name": "Alice"}));
        sink.handle(&event).await.unwrap();
        assert_eq!(producer.count(), 1);
        let msgs = producer.messages();
        assert_eq!(msgs[0].0, "test_topic");
        assert!(msgs[0].2.contains("Alice"));
    }

    #[tokio::test]
    async fn test_kafka_sink_key_format() {
        let producer = Arc::new(BufferedProducer::new());
        let sink = KafkaSink::new(producer.clone(), KafkaSinkConfig::new("topic"));
        let event = make_event("users", serde_json::json!({"id": 1}));
        sink.handle(&event).await.unwrap();
        let msgs = producer.messages();
        assert!(msgs[0].1.starts_with("users:"));
    }

    #[tokio::test]
    async fn test_kafka_sink_multiple_events() {
        let producer = Arc::new(BufferedProducer::new());
        let sink = KafkaSink::new(producer.clone(), KafkaSinkConfig::new("topic"));
        for i in 0..5 {
            let event = make_event("users", serde_json::json!({"id": i}));
            sink.handle(&event).await.unwrap();
        }
        assert_eq!(producer.count(), 5);
    }

    #[tokio::test]
    async fn test_kafka_sink_with_masking() {
        let producer = Arc::new(BufferedProducer::new());
        let config = KafkaSinkConfig::new("topic").with_masking(vec!["password".into()]);
        let sink = KafkaSink::new(producer.clone(), config);
        let event = make_event(
            "users",
            serde_json::json!({"id": 1, "password": "secret123"}),
        );
        sink.handle(&event).await.unwrap();
        let msgs = producer.messages();
        assert!(msgs[0].2.contains("***"));
        assert!(!msgs[0].2.contains("secret123"));
    }

    #[tokio::test]
    async fn test_kafka_sink_masking_preserves_other_fields() {
        let producer = Arc::new(BufferedProducer::new());
        let config = KafkaSinkConfig::new("topic").with_masking(vec!["ssn".into()]);
        let sink = KafkaSink::new(producer.clone(), config);
        let event = make_event(
            "users",
            serde_json::json!({"id": 1, "name": "Alice", "ssn": "123-45-6789"}),
        );
        sink.handle(&event).await.unwrap();
        let msgs = producer.messages();
        assert!(msgs[0].2.contains("Alice"));
        assert!(!msgs[0].2.contains("123-45-6789"));
    }

    #[tokio::test]
    async fn test_kafka_sink_no_masking_when_disabled() {
        let producer = Arc::new(BufferedProducer::new());
        let sink = KafkaSink::new(producer.clone(), KafkaSinkConfig::new("topic"));
        let event = make_event("users", serde_json::json!({"id": 1, "password": "secret"}));
        sink.handle(&event).await.unwrap();
        let msgs = producer.messages();
        assert!(msgs[0].2.contains("secret"));
    }

    #[tokio::test]
    async fn test_kafka_sink_error_propagation() {
        struct FailingProducer;
        #[async_trait]
        impl KafkaProducer for FailingProducer {
            async fn send(&self, _: &str, _: &str, _: &str) -> Result<(), String> {
                Err("kafka unavailable".to_string())
            }
        }
        let sink = KafkaSink::new(Arc::new(FailingProducer), KafkaSinkConfig::new("topic"));
        let event = make_event("users", serde_json::json!({"id": 1}));
        let result = sink.handle(&event).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_kafka_sink_topic_in_message() {
        let producer = Arc::new(BufferedProducer::new());
        let sink = KafkaSink::new(producer.clone(), KafkaSinkConfig::new("my_topic"));
        let event = make_event("users", serde_json::json!({"id": 1}));
        sink.handle(&event).await.unwrap();
        assert_eq!(producer.messages()[0].0, "my_topic");
    }

    #[test]
    fn test_mask_event_fields_function() {
        let mut fields = HashSet::new();
        fields.insert("password".to_string());
        let event = make_event("users", serde_json::json!({"id": 1, "password": "secret"}));
        let masked = mask_event_fields(&event, &fields);
        assert!(masked.masked);
        assert_eq!(masked.row_data["password"], serde_json::json!("***"));
    }

    #[test]
    fn test_mask_event_fields_preserves_unmasked() {
        let mut fields = HashSet::new();
        fields.insert("ssn".to_string());
        let event = make_event(
            "users",
            serde_json::json!({"id": 42, "name": "Bob", "ssn": "111-22-3333"}),
        );
        let masked = mask_event_fields(&event, &fields);
        assert_eq!(masked.row_data["id"], serde_json::json!(42));
        assert_eq!(masked.row_data["name"], serde_json::json!("Bob"));
        assert_eq!(masked.row_data["ssn"], serde_json::json!("***"));
    }

    #[test]
    fn test_mask_event_fields_empty_set() {
        let fields = HashSet::new();
        let event = make_event("users", serde_json::json!({"id": 1}));
        let masked = mask_event_fields(&event, &fields);
        assert!(masked.masked);
        assert_eq!(masked.row_data["id"], serde_json::json!(1));
    }
}
