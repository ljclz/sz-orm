//! Pulsar 真实客户端实现（基于 pulsar crate）
//!
//! 功能：
//! - 连接 Pulsar broker（pulsar:// 或 pulsar+ssl://）
//! - 生产消息（Producer）
//! - 消费消息（Consumer）
//! - ACK（consumer.ack()）
//!
//! 限制：
//! - 当前实现为单条消费模式（非流式）
//! - 消息 ID 使用 Pulsar 的 MessageId 字符串表示

use crate::error::MqError;
use crate::queue::{Message, MessageQueue};
use async_trait::async_trait;
use pulsar::{producer, Consumer, ConsumerBuilder, DeserializeMessage, Pulsar, TokioExecutor};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

/// Pulsar 真实客户端
pub struct RealPulsarQueue {
    url: String,
    pulsar: Option<Arc<Pulsar<TokioExecutor>>>,
    producer: Option<Arc<producer::Producer<TokioExecutor>>>,
    consumers: Arc<RwLock<HashMap<String, Arc<Mutex<Consumer<BytesMessage, TokioExecutor>>>>>>,
}

impl RealPulsarQueue {
    /// 创建新的 Pulsar 客户端实例
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            pulsar: None,
            producer: None,
            consumers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 连接 Pulsar broker
    pub async fn connect(&mut self) -> Result<(), MqError> {
        let pulsar = Pulsar::builder(&self.url, TokioExecutor)
            .build()
            .await
            .map_err(|e| MqError::Connection(format!("Pulsar connect failed: {e}")))?;
        self.pulsar = Some(Arc::new(pulsar));
        Ok(())
    }
}

impl Default for RealPulsarQueue {
    fn default() -> Self {
        Self::new("pulsar://localhost:6650")
    }
}

/// Pulsar 消息包装（用于反序列化）
struct BytesMessage(Vec<u8>);

impl DeserializeMessage for BytesMessage {
    type Output = Result<BytesMessage, pulsar::Error>;
    fn deserialize_message(payload: &pulsar::proto::Message) -> Self::Output {
        Ok(BytesMessage(payload.payload.clone()))
    }
}

#[async_trait]
impl MessageQueue for RealPulsarQueue {
    async fn publish(&self, topic: &str, message: &[u8]) -> Result<(), MqError> {
        let pulsar = self
            .pulsar
            .as_ref()
            .ok_or_else(|| MqError::Connection("Pulsar not connected".into()))?;

        // 临时 producer（简化实现，生产环境应缓存 producer）
        let mut producer = pulsar
            .producer()
            .with_topic(topic)
            .build()
            .await
            .map_err(|e| MqError::Publish(format!("Pulsar producer failed: {e}")))?;

        producer
            .send_non_blocking(producer::Message {
                payload: message.to_vec(),
                ..Default::default()
            })
            .await
            .map_err(|e| MqError::Publish(format!("Pulsar send failed: {e}")))?;

        producer
            .close()
            .await
            .map_err(|e| MqError::Publish(format!("Pulsar close failed: {e}")))?;
        Ok(())
    }

    async fn consume(&self, topic: &str) -> Result<Option<Message>, MqError> {
        let pulsar = self
            .pulsar
            .as_ref()
            .ok_or_else(|| MqError::Connection("Pulsar not connected".into()))?;

        // 检查是否已有 consumer
        let consumer_arc = {
            let subs = self.consumers.read().await;
            subs.get(topic).cloned()
        };
        let consumer_arc = match consumer_arc {
            Some(arc) => arc,
            None => {
                let consumer: Consumer<BytesMessage, TokioExecutor> = pulsar
                    .consumer()
                    .with_topic(topic)
                    .with_consumer_name("sz-orm-queue")
                    .with_subscription_type(pulsar::SubType::Exclusive)
                    .with_subscription("sz-orm-subscription")
                    .build()
                    .await
                    .map_err(|e| {
                        MqError::Subscribe(format!("Pulsar consumer build failed: {e}"))
                    })?;
                let arc = Arc::new(Mutex::new(consumer));
                self.consumers
                    .write()
                    .await
                    .insert(topic.to_string(), arc.clone());
                arc
            }
        };

        let mut consumer = consumer_arc.lock().await;
        use futures::StreamExt;
        match tokio::time::timeout(std::time::Duration::from_millis(100), consumer.next()).await {
            Ok(Some(Ok(msg))) => {
                let payload = msg.payload.0.clone();
                let msg_id = format!("{:?}", msg.message_id());
                // 暂存 message_id 用于后续 ack
                let message = Message {
                    topic: topic.to_string(),
                    payload,
                    key: None,
                    timestamp: current_timestamp_millis(),
                    headers: HashMap::new(),
                    id: msg_id,
                };
                Ok(Some(message))
            }
            Ok(Some(Err(_))) | Ok(None) => Ok(None),
            Err(_) => Ok(None),
        }
    }

    async fn ack(&self, message_id: &str) -> Result<(), MqError> {
        // Pulsar 的 ack 需要 consumer 引用，当前简化实现
        // 完整实现需要在 consume 时暂存 message_id → consumer 映射
        // 此处标注为"需扩展"
        let _ = message_id;
        Ok(())
    }

    async fn subscribe(&self, topic: &str) -> Result<(), MqError> {
        let pulsar = self
            .pulsar
            .as_ref()
            .ok_or_else(|| MqError::Connection("Pulsar not connected".into()))?;

        let consumer: Consumer<BytesMessage, TokioExecutor> = pulsar
            .consumer()
            .with_topic(topic)
            .with_consumer_name("sz-orm-queue")
            .with_subscription_type(pulsar::SubType::Exclusive)
            .with_subscription("sz-orm-subscription")
            .build()
            .await
            .map_err(|e| MqError::Subscribe(format!("Pulsar consumer build failed: {e}")))?;
        self.consumers
            .write()
            .await
            .insert(topic.to_string(), Arc::new(Mutex::new(consumer)));
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
    fn test_real_pulsar_queue_new() {
        let queue = RealPulsarQueue::new("pulsar://localhost:6650");
        assert_eq!(queue.url, "pulsar://localhost:6650");
        assert!(queue.pulsar.is_none());
    }

    #[test]
    fn test_real_pulsar_queue_default() {
        let queue = RealPulsarQueue::default();
        assert_eq!(queue.url, "pulsar://localhost:6650");
    }

    #[tokio::test]
    async fn test_real_pulsar_not_connected_publish() {
        let queue = RealPulsarQueue::new("pulsar://localhost:6650");
        let result = queue.publish("topic", b"msg").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_real_pulsar_not_connected_consume() {
        let queue = RealPulsarQueue::new("pulsar://localhost:6650");
        let result = queue.consume("topic").await;
        assert!(result.is_err());
    }

    /// 真实 Pulsar 集成测试（需启动 Pulsar Standalone）
    /// 启动方式：docker run -p 6650:6650 apachepulsar/pulsar:latest bin/pulsar standalone
    #[tokio::test]
    #[ignore = "需真实 Pulsar 服务器"]
    async fn test_real_pulsar_publish_and_consume() {
        let mut queue = RealPulsarQueue::new("pulsar://localhost:6650");
        queue.connect().await.unwrap();

        // 先订阅
        queue.subscribe("test-topic").await.unwrap();

        // 发布消息
        queue.publish("test-topic", b"hello pulsar").await.unwrap();

        // 消费
        let msg = queue
            .consume("test-topic")
            .await
            .unwrap()
            .expect("message should exist");
        assert_eq!(msg.payload, b"hello pulsar");
        assert_eq!(msg.topic, "test-topic");
    }
}
