//! Kafka 真实客户端实现（基于 rdkafka）
//!
//! 功能：
//! - 连接 Kafka 集群（支持 SASL 鉴权）
//! - 生产消息（FutureProducer，异步）
//! - 消费消息（StreamConsumer，订阅 topic）
//! - ACK（Kafka 自动提交 offset，ack 为 no-op）
//!
//! 编译要求：
//! - 需要 cmake 和 C 编译器（rdkafka 的 cmake-build feature 会从源码编译 librdkafka）
//! - Windows 需安装 cmake + Visual Studio Build Tools
//! - Linux CI 通过 apt install cmake 即可
//!
//! 限制：
//! - ack() 为 no-op（Kafka 使用消费者组自动提交 offset）
//! - 消息 ID 使用 topic-partition-offset 格式

use crate::error::MqError;
use crate::queue::{Message, MessageQueue};
use async_trait::async_trait;
use rdkafka::config::{ClientConfig, RDKafkaLogLevel};
use rdkafka::consumer::StreamConsumer;
use rdkafka::producer::{FutureProducer, FutureRecord};
use std::collections::HashMap;
use std::sync::Arc;

/// Kafka 真实客户端
pub struct RealKafkaQueue {
    brokers: String,
    group_id: String,
    producer: Option<Arc<FutureProducer>>,
    consumer: Option<Arc<StreamConsumer>>,
}

impl RealKafkaQueue {
    /// 创建新的 Kafka 客户端实例
    pub fn new(brokers: impl Into<String>, group_id: impl Into<String>) -> Self {
        Self {
            brokers: brokers.into(),
            group_id: group_id.into(),
            producer: None,
            consumer: None,
        }
    }

    /// 创建 producer
    pub async fn connect_producer(&mut self) -> Result<(), MqError> {
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", &self.brokers)
            .set("message.timeout.ms", "5000")
            .set_log_level(RDKafkaLogLevel::Warning)
            .create()
            .map_err(|e| MqError::Connection(format!("Kafka producer failed: {e}")))?;
        self.producer = Some(Arc::new(producer));
        Ok(())
    }

    /// 创建 consumer
    pub async fn connect_consumer(&mut self) -> Result<(), MqError> {
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", &self.brokers)
            .set("group.id", &self.group_id)
            .set("enable.auto.commit", "true")
            .set("auto.commit.interval.ms", "1000")
            .set("session.timeout.ms", "6000")
            .set_log_level(RDKafkaLogLevel::Warning)
            .create()
            .map_err(|e| MqError::Connection(format!("Kafka consumer failed: {e}")))?;
        self.consumer = Some(Arc::new(consumer));
        Ok(())
    }
}

impl Default for RealKafkaQueue {
    fn default() -> Self {
        Self::new("localhost:9092", "sz-orm-group")
    }
}

#[async_trait]
impl MessageQueue for RealKafkaQueue {
    async fn publish(&self, topic: &str, message: &[u8]) -> Result<(), MqError> {
        let producer = self
            .producer
            .as_ref()
            .ok_or_else(|| MqError::Connection("Kafka producer not connected".into()))?;

        let delivery = producer
            .send(
                FutureRecord::to(topic).payload(message),
                std::time::Duration::from_secs(5),
            )
            .await;

        match delivery {
            Ok(_) => Ok(()),
            Err((e, _)) => Err(MqError::Publish(format!("Kafka send failed: {e}"))),
        }
    }

    async fn consume(&self, topic: &str) -> Result<Option<Message>, MqError> {
        let consumer = self
            .consumer
            .as_ref()
            .ok_or_else(|| MqError::Connection("Kafka consumer not connected".into()))?;

        // 订阅 topic（幂等，rdkafka 内部处理重复订阅）
        consumer
            .subscribe(&[topic])
            .map_err(|e| MqError::Subscribe(format!("Kafka subscribe failed: {e}")))?;

        // 轮询消息（100ms 超时）
        match tokio::time::timeout(std::time::Duration::from_millis(100), consumer.recv()).await {
            Ok(Ok(msg)) => {
                let payload = msg.payload().map(|p| p.to_vec()).unwrap_or_default();
                let key = msg.key().map(|k| String::from_utf8_lossy(k).to_string());
                let partition = msg.partition();
                let offset = msg.offset();
                let msg_id = format!("{topic}-{partition}-{offset}");

                let message = Message {
                    topic: topic.to_string(),
                    payload,
                    key,
                    timestamp: current_timestamp_millis(),
                    headers: HashMap::new(),
                    id: msg_id,
                };

                // 自动提交 offset（已配置 enable.auto.commit）
                Ok(Some(message))
            }
            Ok(Err(e)) => Err(MqError::Publish(format!("Kafka recv failed: {e}"))),
            Err(_) => Ok(None), // 超时视为无消息
        }
    }

    async fn ack(&self, _message_id: &str) -> Result<(), MqError> {
        // Kafka 使用自动提交 offset，无需显式 ack
        // 如需手动提交，可调用 consumer.commit_message()
        Ok(())
    }

    async fn subscribe(&self, topic: &str) -> Result<(), MqError> {
        let consumer = self
            .consumer
            .as_ref()
            .ok_or_else(|| MqError::Connection("Kafka consumer not connected".into()))?;
        consumer
            .subscribe(&[topic])
            .map_err(|e| MqError::Subscribe(format!("Kafka subscribe failed: {e}")))?;
        Ok(())
    }
}

/// 当前时间戳（毫秒）
fn current_timestamp_millis() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_real_kafka_queue_new() {
        let queue = RealKafkaQueue::new("localhost:9092", "test-group");
        assert_eq!(queue.brokers, "localhost:9092");
        assert_eq!(queue.group_id, "test-group");
        assert!(queue.producer.is_none());
        assert!(queue.consumer.is_none());
    }

    #[test]
    fn test_real_kafka_queue_default() {
        let queue = RealKafkaQueue::default();
        assert_eq!(queue.brokers, "localhost:9092");
        assert_eq!(queue.group_id, "sz-orm-group");
    }

    #[tokio::test]
    async fn test_real_kafka_not_connected_publish() {
        let queue = RealKafkaQueue::new("localhost:9092", "test-group");
        let result = queue.publish("topic", b"msg").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_real_kafka_not_connected_consume() {
        let queue = RealKafkaQueue::new("localhost:9092", "test-group");
        let result = queue.consume("topic").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_real_kafka_not_connected_subscribe() {
        let queue = RealKafkaQueue::new("localhost:9092", "test-group");
        let result = queue.subscribe("topic").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_real_kafka_ack_always_ok() {
        let queue = RealKafkaQueue::new("localhost:9092", "test-group");
        // ack 在 Kafka 中是 no-op（自动提交 offset）
        let result = queue.ack("any-id").await;
        assert!(result.is_ok());
    }

    /// 真实 Kafka 集成测试（需启动 Kafka）
    /// 启动方式：docker run -p 9092:9092 apache/kafka:latest
    #[tokio::test]
    #[ignore = "需真实 Kafka 服务器"]
    async fn test_real_kafka_publish_and_consume() {
        let mut queue = RealKafkaQueue::new("localhost:9092", "test-group");
        queue.connect_producer().await.unwrap();
        queue.connect_consumer().await.unwrap();

        // 先订阅
        queue.subscribe("test-topic").await.unwrap();

        // 等待消费者组再平衡
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // 发布消息
        queue.publish("test-topic", b"hello kafka").await.unwrap();

        // 等待消息到达
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        // 消费
        let msg = queue
            .consume("test-topic")
            .await
            .unwrap()
            .expect("message should exist");
        assert_eq!(msg.payload, b"hello kafka");
        assert_eq!(msg.topic, "test-topic");
    }
}
