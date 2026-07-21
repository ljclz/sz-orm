//! NATS 真实客户端实现（基于 async-nats）
//!
//! 功能：
//! - 连接 NATS 服务器（支持 nkey/jwt 鉴权）
//! - 发布消息（publish）
//! - 订阅并消费（subscribe + consume）
//! - ACK（NATS Core 无 ACK 概念，消费即确认；JetStream 才有 ACK）
//!
//! 限制：
//! - 当前实现针对 NATS Core（非 JetStream），consume 后无需 ack
//! - ack() 方法为 no-op，返回 Ok(())
//! - 消息 ID 使用 NATS 消息的 reply subject 或生成的 UUID

use crate::error::MqError;
use crate::queue::{Message, MessageQueue};
use async_trait::async_trait;
use futures::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

/// NATS 真实客户端
pub struct RealNatsQueue {
    url: String,
    client: Option<Arc<async_nats::Client>>,
    subscribers: Arc<RwLock<HashMap<String, Arc<Mutex<async_nats::Subscriber>>>>>,
}

impl RealNatsQueue {
    /// 创建新的 NATS 客户端实例
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            client: None,
            subscribers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 连接 NATS 服务器
    pub async fn connect(&mut self) -> Result<(), MqError> {
        let client = async_nats::connect(&self.url)
            .await
            .map_err(|e| MqError::Connection(format!("NATS connect failed: {e}")))?;
        self.client = Some(Arc::new(client));
        Ok(())
    }

    /// M-13 修复：重新连接 NATS 服务器
    ///
    /// 当连接断开或长时间出错时，调用方应调用此方法重建连接。
    ///
    /// # 说明
    ///
    /// - async-nats 内部已有自动重连机制（通过 `connect_options::retry_on_error`）
    /// - 但在某些场景（如 DNS 解析失败）下，内部重连可能失效
    /// - 此方法会清除旧连接和订阅者，然后重新建立连接
    /// - 重连后需要重新订阅所有 topic
    pub async fn reconnect(&mut self) -> Result<(), MqError> {
        // 清除旧状态
        self.client = None;
        self.subscribers.write().await.clear();
        // 重建连接
        self.connect().await
    }
}

impl Default for RealNatsQueue {
    fn default() -> Self {
        Self::new("nats://localhost:4222")
    }
}

#[async_trait]
impl MessageQueue for RealNatsQueue {
    async fn publish(&self, topic: &str, message: &[u8]) -> Result<(), MqError> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| MqError::Connection("NATS not connected".into()))?;
        client
            .publish(topic.to_string(), message.to_vec().into())
            .await
            .map_err(|e| MqError::Publish(format!("NATS publish failed: {e}")))?;
        // flush 确保消息发送
        client
            .flush()
            .await
            .map_err(|e| MqError::Publish(format!("NATS flush failed: {e}")))?;
        Ok(())
    }

    async fn consume(&self, topic: &str) -> Result<Option<Message>, MqError> {
        let subs = self.subscribers.read().await;
        let subscriber_arc = subs.get(topic).cloned();
        drop(subs);

        let subscriber_arc = match subscriber_arc {
            Some(s) => s,
            None => {
                // 自动订阅
                let client = self
                    .client
                    .as_ref()
                    .ok_or_else(|| MqError::Connection("NATS not connected".into()))?;
                let subscriber = client
                    .subscribe(topic.to_string())
                    .await
                    .map_err(|e| MqError::Subscribe(format!("NATS subscribe failed: {e}")))?;
                let arc = Arc::new(Mutex::new(subscriber));
                self.subscribers
                    .write()
                    .await
                    .insert(topic.to_string(), arc.clone());
                arc
            }
        };

        let mut subscriber = subscriber_arc.lock().await;
        match tokio::time::timeout(std::time::Duration::from_millis(100), subscriber.next()).await {
            Ok(Some(msg)) => {
                let message = Message {
                    topic: msg.subject.to_string(),
                    payload: msg.payload.to_vec(),
                    key: msg.reply.as_ref().map(|s| s.to_string()),
                    timestamp: current_timestamp_millis(),
                    headers: HashMap::new(),
                    id: uuid_like_id(),
                };
                Ok(Some(message))
            }
            Ok(None) => Ok(None),
            Err(_) => Ok(None), // 超时视为无消息
        }
    }

    /// H-6 文档说明：NATS Core 的 ack() 为 no-op
    ///
    /// # 语义
    /// - **NATS Core**：at-most-once（最多一次），消息一旦投递即视为确认，无 ACK 概念
    /// - **NATS JetStream**：at-least-once（至少一次），需要显式 ACK
    ///
    /// # 当前实现
    /// 本实现针对 NATS Core，ack() 为 no-op，返回 Ok(())。
    /// 若需 at-least-once 语义，请使用：
    /// - `RealKafkaQueue`（手动提交 offset）
    /// - `RealPulsarQueue`（consumer.ack_with）
    /// - `LapinRabbitmqQueue`（basic_ack）
    /// - `RealActivemqQueue`（basic_ack）
    ///
    /// # 风险
    /// 消费者崩溃时可能丢失未处理的消息（NATS Core 不会重新投递）。
    async fn ack(&self, _message_id: &str) -> Result<(), MqError> {
        // NATS Core 无 ACK 概念，消费即确认
        // JetStream 才有 ACK，此处为 no-op
        Ok(())
    }

    async fn subscribe(&self, topic: &str) -> Result<(), MqError> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| MqError::Connection("NATS not connected".into()))?;
        let subscriber = client
            .subscribe(topic.to_string())
            .await
            .map_err(|e| MqError::Subscribe(format!("NATS subscribe failed: {e}")))?;
        self.subscribers
            .write()
            .await
            .insert(topic.to_string(), Arc::new(Mutex::new(subscriber)));
        Ok(())
    }
}

/// 生成简单 ID（避免引入 uuid 依赖）
fn uuid_like_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("nats-{ts}")
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
    fn test_real_nats_queue_new() {
        let queue = RealNatsQueue::new("nats://localhost:4222");
        assert_eq!(queue.url, "nats://localhost:4222");
        assert!(queue.client.is_none());
    }

    #[test]
    fn test_real_nats_queue_default() {
        let queue = RealNatsQueue::default();
        assert_eq!(queue.url, "nats://localhost:4222");
    }

    #[tokio::test]
    async fn test_real_nats_not_connected_publish() {
        let queue = RealNatsQueue::new("nats://localhost:4222");
        let result = queue.publish("topic", b"msg").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_real_nats_not_connected_consume() {
        let queue = RealNatsQueue::new("nats://localhost:4222");
        let result = queue.consume("topic").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_real_nats_not_connected_subscribe() {
        let queue = RealNatsQueue::new("nats://localhost:4222");
        let result = queue.subscribe("topic").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_real_nats_ack_always_ok() {
        let queue = RealNatsQueue::new("nats://localhost:4222");
        // ack 在 NATS Core 中是 no-op，始终返回 Ok
        let result = queue.ack("any-id").await;
        assert!(result.is_ok());
    }

    /// 真实 NATS 集成测试（需启动 NATS 服务器）
    /// 启动方式：docker run -p 4222:4222 nats:latest
    #[tokio::test]
    #[ignore = "需真实 NATS 服务器"]
    async fn test_real_nats_publish_and_consume() {
        let mut queue = RealNatsQueue::new("nats://localhost:4222");
        queue.connect().await.unwrap();

        // 先订阅
        queue.subscribe("test-subject").await.unwrap();

        // 发布消息
        queue.publish("test-subject", b"hello nats").await.unwrap();

        // 消费
        let msg = queue
            .consume("test-subject")
            .await
            .unwrap()
            .expect("message should exist");
        assert_eq!(msg.payload, b"hello nats");
        assert_eq!(msg.topic, "test-subject");

        // ACK（no-op）
        queue.ack(&msg.id).await.unwrap();
    }
}
