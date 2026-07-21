//! ActiveMQ 真实客户端实现（基于 lapin AMQP 1.0 协议）
//!
//! ActiveMQ 5.14+ 支持 AMQP 1.0 协议，可通过 lapin 客户端连接。
//! ActiveMQ Artemis 原生支持 AMQP 1.0。
//!
//! 连接 URL 格式：
//! - amqp://localhost:5672 （ActiveMQ 5.x 需启用 AMQP connector）
//! - amqp://localhost:61616 （ActiveMQ Artemis 默认 AMQP 端口）
//!
//! 限制：
//! - 复用 lapin 的 AMQP 0.9.1 实现，ActiveMQ 5.x 的 AMQP 1.0 支持有限
//! - 推荐使用 ActiveMQ Artemis（原生 AMQP 1.0）

use crate::error::MqError;
use crate::queue::{Message, MessageQueue};
use async_trait::async_trait;
use futures::StreamExt;
use lapin::{
    options::*, types::FieldTable, BasicProperties, Channel, Connection, ConnectionProperties,
    Consumer,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

/// ActiveMQ 真实客户端（通过 AMQP 1.0 协议）
pub struct RealActivemqQueue {
    broker_url: String,
    connection: Option<Arc<Connection>>,
    channel: Option<Channel>,
    consumers: Arc<RwLock<HashMap<String, Arc<Mutex<Consumer>>>>>,
    in_flight: Arc<RwLock<HashMap<String, u64>>>,
}

impl RealActivemqQueue {
    /// 创建新的 ActiveMQ 客户端实例
    pub fn new(broker_url: impl Into<String>) -> Self {
        Self {
            broker_url: broker_url.into(),
            connection: None,
            channel: None,
            consumers: Arc::new(RwLock::new(HashMap::new())),
            in_flight: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 连接 ActiveMQ 服务器
    pub async fn connect(&mut self) -> Result<(), MqError> {
        let conn = Connection::connect(&self.broker_url, ConnectionProperties::default())
            .await
            .map_err(|e| MqError::Connection(format!("ActiveMQ connect failed: {e}")))?;
        let channel = conn
            .create_channel()
            .await
            .map_err(|e| MqError::Connection(format!("ActiveMQ channel failed: {e}")))?;
        self.connection = Some(Arc::new(conn));
        self.channel = Some(channel);
        Ok(())
    }

    /// M-15 修复：重新连接 ActiveMQ 服务器
    ///
    /// 当连接断开或长时间出错时，调用方应调用此方法重建连接和 channel。
    ///
    /// # 说明
    ///
    /// - ActiveMQ 使用 AMQP 协议（通过 lapin），与 RabbitMQ 类似
    /// - lapin 内部通过 heartbeat 检测连接状态，但不会自动重连
    /// - 此方法会清除旧连接、channel、consumers 和 in_flight，然后重新建立连接
    /// - 重连后需要重新订阅所有 topic
    pub async fn reconnect(&mut self) -> Result<(), MqError> {
        // 清除旧状态
        self.channel = None;
        self.connection = None;
        self.consumers.write().await.clear();
        self.in_flight.write().await.clear();
        // 重建连接
        self.connect().await
    }
}

impl Default for RealActivemqQueue {
    fn default() -> Self {
        Self::new("amqp://localhost:61616")
    }
}

#[async_trait]
impl MessageQueue for RealActivemqQueue {
    async fn publish(&self, topic: &str, message: &[u8]) -> Result<(), MqError> {
        let channel = self
            .channel
            .as_ref()
            .ok_or_else(|| MqError::Connection("ActiveMQ not connected".into()))?;
        channel
            .queue_declare(
                topic.into(),
                QueueDeclareOptions::default(),
                FieldTable::default(),
            )
            .await
            .map_err(|e| MqError::Publish(format!("ActiveMQ queue declare failed: {e}")))?;
        channel
            .basic_publish(
                "".into(),
                topic.into(),
                BasicPublishOptions::default(),
                message,
                BasicProperties::default(),
            )
            .await
            .map_err(|e| MqError::Publish(format!("ActiveMQ publish failed: {e}")))?
            .await
            .map_err(|e| MqError::Publish(format!("ActiveMQ confirm failed: {e}")))?;
        Ok(())
    }

    async fn consume(&self, topic: &str) -> Result<Option<Message>, MqError> {
        let channel = self
            .channel
            .as_ref()
            .ok_or_else(|| MqError::Connection("ActiveMQ not connected".into()))?;

        // 先检查是否已有 consumer，避免在 async 中 block_on 死锁
        let consumer_arc = {
            let subs = self.consumers.read().await;
            subs.get(topic).cloned()
        };
        let consumer_arc = match consumer_arc {
            Some(arc) => arc,
            None => {
                // 创建新 consumer
                channel
                    .queue_declare(
                        topic.into(),
                        QueueDeclareOptions::default(),
                        FieldTable::default(),
                    )
                    .await
                    .map_err(|e| {
                        MqError::Subscribe(format!("ActiveMQ queue declare failed: {e}"))
                    })?;
                let consumer = channel
                    .basic_consume(
                        topic.into(),
                        topic.into(),
                        BasicConsumeOptions::default(),
                        FieldTable::default(),
                    )
                    .await
                    .map_err(|e| MqError::Subscribe(format!("ActiveMQ consume failed: {e}")))?;
                let arc = Arc::new(Mutex::new(consumer));
                self.consumers
                    .write()
                    .await
                    .insert(topic.to_string(), arc.clone());
                arc
            }
        };

        let mut consumer = consumer_arc.lock().await;
        match tokio::time::timeout(std::time::Duration::from_millis(100), consumer.next()).await {
            Ok(Some(Ok(delivery))) => {
                let delivery_tag = delivery.delivery_tag;
                let msg_id = format!("activemq-{delivery_tag}");
                let message = Message {
                    topic: topic.to_string(),
                    payload: delivery.data.clone(),
                    key: None,
                    timestamp: current_timestamp_millis(),
                    headers: HashMap::new(),
                    id: msg_id.clone(),
                };
                self.in_flight.write().await.insert(msg_id, delivery_tag);
                Ok(Some(message))
            }
            Ok(Some(Err(_))) | Ok(None) => Ok(None),
            Err(_) => Ok(None),
        }
    }

    async fn ack(&self, message_id: &str) -> Result<(), MqError> {
        let channel = self
            .channel
            .as_ref()
            .ok_or_else(|| MqError::Connection("ActiveMQ not connected".into()))?;
        let mut in_flight = self.in_flight.write().await;
        let delivery_tag = in_flight
            .remove(message_id)
            .ok_or_else(|| MqError::Publish(format!("unknown message id: {message_id}")))?;
        channel
            .basic_ack(delivery_tag, BasicAckOptions::default())
            .await
            .map_err(|e| MqError::Publish(format!("ActiveMQ ack failed: {e}")))?;
        Ok(())
    }

    async fn subscribe(&self, topic: &str) -> Result<(), MqError> {
        let channel = self
            .channel
            .as_ref()
            .ok_or_else(|| MqError::Connection("ActiveMQ not connected".into()))?;
        // 声明队列（幂等）
        channel
            .queue_declare(
                topic.into(),
                QueueDeclareOptions::default(),
                FieldTable::default(),
            )
            .await
            .map_err(|e| MqError::Subscribe(format!("ActiveMQ queue declare failed: {e}")))?;
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
    fn test_real_activemq_queue_new() {
        let queue = RealActivemqQueue::new("amqp://localhost:61616");
        assert_eq!(queue.broker_url, "amqp://localhost:61616");
        assert!(queue.connection.is_none());
    }

    #[test]
    fn test_real_activemq_queue_default() {
        let queue = RealActivemqQueue::default();
        assert_eq!(queue.broker_url, "amqp://localhost:61616");
    }

    #[tokio::test]
    async fn test_real_activemq_not_connected_publish() {
        let queue = RealActivemqQueue::new("amqp://localhost:61616");
        let result = queue.publish("topic", b"msg").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_real_activemq_not_connected_ack() {
        let queue = RealActivemqQueue::new("amqp://localhost:61616");
        let result = queue.ack("any-id").await;
        assert!(result.is_err());
    }

    /// 真实 ActiveMQ 集成测试（需启动 ActiveMQ Artemis）
    /// 启动方式：docker run -p 61616:61616 vromero/activemq-artemis
    #[tokio::test]
    #[ignore = "需真实 ActiveMQ Artemis 服务器"]
    async fn test_real_activemq_publish_and_consume() {
        let mut queue = RealActivemqQueue::new("amqp://localhost:61616");
        queue.connect().await.unwrap();

        // 先订阅
        queue.subscribe("test-queue").await.unwrap();

        // 发布消息
        queue
            .publish("test-queue", b"hello activemq")
            .await
            .unwrap();

        // 消费
        let msg = queue
            .consume("test-queue")
            .await
            .unwrap()
            .expect("message should exist");
        assert_eq!(msg.payload, b"hello activemq");
        assert_eq!(msg.topic, "test-queue");

        // ACK
        queue.ack(&msg.id).await.unwrap();
    }
}
