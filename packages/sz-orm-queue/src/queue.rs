use crate::error::MqError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;

#[async_trait]
pub trait MessageQueue: Send + Sync {
    async fn publish(&self, topic: &str, message: &[u8]) -> Result<(), MqError>;
    async fn consume(&self, topic: &str) -> Result<Option<Message>, MqError>;
    async fn ack(&self, message_id: &str) -> Result<(), MqError>;
    async fn subscribe(&self, topic: &str) -> Result<(), MqError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub topic: String,
    pub payload: Vec<u8>,
    pub key: Option<String>,
    pub timestamp: i64,
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub id: String,
}

impl Message {
    pub fn new(topic: impl Into<String>, payload: Vec<u8>) -> Self {
        Self {
            topic: topic.into(),
            payload,
            key: None,
            timestamp: current_timestamp(),
            headers: HashMap::new(),
            id: String::new(),
        }
    }

    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    pub fn text(&self) -> Option<&str> {
        std::str::from_utf8(&self.payload).ok()
    }

    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Option<T> {
        serde_json::from_slice(&self.payload).ok()
    }

    pub fn text_message(topic: impl Into<String>, text: impl Into<String>) -> Self {
        Self::new(topic, text.into().into_bytes())
    }

    pub fn json_message<T: serde::Serialize>(
        topic: impl Into<String>,
        data: &T,
    ) -> Result<Self, MqError> {
        let payload = serde_json::to_vec(data)?;
        Ok(Self::new(topic, payload))
    }
}

/// 当前时间戳（毫秒）
///
/// M-10 修复：使用 `unwrap_or_default()` 会在系统时间早于 UNIX_EPOCH 时返回 0，
/// 隐藏了潜在的时钟回拨问题。改为显式 match 并通过 eprintln! 记录事件，
/// 便于在生产环境中排查（可被 stderr 重定向到日志收集系统）。
fn current_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_millis() as i64,
        Err(e) => {
            // 系统时间早于 UNIX_EPOCH（时钟回拨或系统错误）
            // 返回 0 作为兜底，避免 panic；生产环境应监控此事件
            eprintln!(
                "WARN: current_timestamp: system time before UNIX_EPOCH: {} (duration_secs={})",
                e,
                e.duration().as_secs()
            );
            0
        }
    }
}

pub struct QueueConfig {
    pub provider: MqProvider,
    pub brokers: Vec<String>,
    pub group_id: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            provider: MqProvider::Kafka(KafkaConfig::default()),
            brokers: vec!["localhost:9092".to_string()],
            group_id: None,
            username: None,
            password: None,
        }
    }
}

impl QueueConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_provider(mut self, provider: MqProvider) -> Self {
        self.provider = provider;
        self
    }

    pub fn with_brokers(mut self, brokers: Vec<String>) -> Self {
        self.brokers = brokers;
        self
    }

    pub fn with_group(mut self, group: impl Into<String>) -> Self {
        self.group_id = Some(group.into());
        self
    }

    pub fn with_auth(mut self, username: impl Into<String>, password: impl Into<String>) -> Self {
        self.username = Some(username.into());
        self.password = Some(password.into());
        self
    }
}

#[derive(Debug, Clone)]
pub enum MqProvider {
    Kafka(KafkaConfig),
    RabbitMQ(RabbitConfig),
    RocketMQ(RocketConfig),
    ActiveMQ(ActiveConfig),
    Nats(NatsConfig),
    Pulsar(PulsarConfig),
}

#[derive(Debug, Clone, Default)]
pub struct KafkaConfig {
    pub client_id: Option<String>,
    pub acks: Option<String>,
    pub retries: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct RabbitConfig {
    pub virtual_host: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RocketConfig {
    pub namespace: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ActiveConfig {
    pub broker_url: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct NatsConfig {
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PulsarConfig {
    pub service_url: Option<String>,
}

pub struct InMemoryQueue {
    inner: Arc<RwLock<InMemoryQueueInner>>,
}

struct InMemoryQueueInner {
    queues: HashMap<String, VecDeque<Message>>,
    in_flight: HashMap<String, Message>,
    subscribers: HashMap<String, usize>,
    next_id: u64,
    /// H-3 修复：每个 topic 最大消息数限制（防止 OOM）
    /// 默认 100,000，可通过 `with_max_messages_per_topic` 调整
    max_messages_per_topic: usize,
}

/// 默认每 topic 最大消息数（H-3 修复）
const DEFAULT_MAX_MESSAGES_PER_TOPIC: usize = 100_000;

impl InMemoryQueue {
    pub fn new() -> Self {
        Self::with_max_messages_per_topic(DEFAULT_MAX_MESSAGES_PER_TOPIC)
    }

    /// 创建指定每 topic 最大消息数的队列（H-3 修复）
    ///
    /// 当队列中消息数达到此限制时，`publish` 将返回 `MqError::Publish` 错误。
    /// 默认 100,000，可根据内存容量调整。
    pub fn with_max_messages_per_topic(max: usize) -> Self {
        Self {
            inner: Arc::new(RwLock::new(InMemoryQueueInner {
                queues: HashMap::new(),
                in_flight: HashMap::new(),
                subscribers: HashMap::new(),
                next_id: 1,
                max_messages_per_topic: max,
            })),
        }
    }

    pub async fn message_count(&self, topic: &str) -> usize {
        let inner = self.inner.read().await;
        inner.queues.get(topic).map(|q| q.len()).unwrap_or(0)
    }

    pub async fn subscriber_count(&self, topic: &str) -> usize {
        let inner = self.inner.read().await;
        *inner.subscribers.get(topic).unwrap_or(&0)
    }

    pub async fn in_flight_count(&self) -> usize {
        let inner = self.inner.read().await;
        inner.in_flight.len()
    }
}

impl Default for InMemoryQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MessageQueue for InMemoryQueue {
    async fn publish(&self, topic: &str, message: &[u8]) -> Result<(), MqError> {
        let mut inner = self.inner.write().await;
        // H-3 修复：检查消息数限制，防止 OOM
        let current_count = inner.queues.get(topic).map(|q| q.len()).unwrap_or(0);
        if current_count >= inner.max_messages_per_topic {
            return Err(MqError::Publish(format!(
                "topic '{}' is full: {} >= {} messages (H-3 protection)",
                topic, current_count, inner.max_messages_per_topic
            )));
        }
        let id = format!("msg-{}", inner.next_id);
        // L-2 修复：使用 checked_add 防止 u64 溢出
        // 实际场景下 u64::MAX (1.8e19) 几乎不可能触及，但严谨起见添加检查
        inner.next_id = inner
            .next_id
            .checked_add(1)
            .ok_or_else(|| MqError::Publish("message id overflow: u64::MAX reached".to_string()))?;
        let msg = Message {
            id,
            ..Message::new(topic, message.to_vec())
        };
        inner
            .queues
            .entry(topic.to_string())
            .or_insert_with(VecDeque::new)
            .push_back(msg);
        Ok(())
    }

    async fn consume(&self, topic: &str) -> Result<Option<Message>, MqError> {
        let mut inner = self.inner.write().await;
        let queue = inner
            .queues
            .entry(topic.to_string())
            .or_insert_with(VecDeque::new);
        if let Some(msg) = queue.pop_front() {
            inner.in_flight.insert(msg.id.clone(), msg.clone());
            Ok(Some(msg))
        } else {
            Ok(None)
        }
    }

    async fn ack(&self, message_id: &str) -> Result<(), MqError> {
        let mut inner = self.inner.write().await;
        inner.in_flight.remove(message_id).ok_or_else(|| {
            MqError::NotSupported(format!("Message not found for ack: {}", message_id))
        })?;
        Ok(())
    }

    async fn subscribe(&self, topic: &str) -> Result<(), MqError> {
        let mut inner = self.inner.write().await;
        *inner.subscribers.entry(topic.to_string()).or_insert(0) += 1;
        Ok(())
    }
}

pub struct QueueWrapper {
    queue: Box<dyn MessageQueue>,
}

impl QueueWrapper {
    pub fn new(provider: MqProvider) -> Self {
        let queue: Box<dyn MessageQueue> = match provider {
            MqProvider::Kafka(_) => Box::new(crate::kafka::InMemoryKafkaQueue::new()),
            MqProvider::RabbitMQ(_) => Box::new(crate::rabbitmq::InMemoryRabbitmqQueue::new()),
            MqProvider::RocketMQ(_) => Box::new(crate::rocketmq::InMemoryRocketmqQueue::new()),
            MqProvider::ActiveMQ(_) => Box::new(crate::activemq::InMemoryActivemqQueue::new()),
            MqProvider::Nats(_) => Box::new(crate::nats::InMemoryNatsQueue::new()),
            MqProvider::Pulsar(_) => Box::new(crate::pulsar::InMemoryPulsarQueue::new()),
        };
        Self { queue }
    }

    pub async fn publish(&self, topic: &str, message: &[u8]) -> Result<(), MqError> {
        self.queue.publish(topic, message).await
    }

    pub async fn consume(&self, topic: &str) -> Result<Option<Message>, MqError> {
        self.queue.consume(topic).await
    }

    pub async fn ack(&self, message_id: &str) -> Result<(), MqError> {
        self.queue.ack(message_id).await
    }

    pub async fn subscribe(&self, topic: &str) -> Result<(), MqError> {
        self.queue.subscribe(topic).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_in_memory_queue_basic() {
        let queue = InMemoryQueue::new();
        queue.publish("topic1", b"hello").await.unwrap();
        let msg = queue
            .consume("topic1")
            .await
            .unwrap()
            .expect("msg should exist");
        assert_eq!(msg.payload, b"hello");
        queue.ack(&msg.id).await.unwrap();
    }

    /// L-2 测试：next_id 溢出保护
    ///
    /// 通过将 next_id 设置为 u64::MAX，验证下一次 publish 会返回错误而非 panic
    #[tokio::test]
    async fn test_l2_next_id_overflow_protection() {
        let queue = InMemoryQueue::new();
        // 将 next_id 手动设置为 u64::MAX
        {
            let mut inner = queue.inner.write().await;
            inner.next_id = u64::MAX;
        }
        // 此时 publish 应返回错误（L-2 修复：checked_add 失败）
        let result = queue.publish("topic1", b"msg").await;
        assert!(result.is_err());
        match result {
            Err(MqError::Publish(msg)) => {
                assert!(
                    msg.contains("overflow"),
                    "expected overflow error, got: {}",
                    msg
                );
            }
            _ => panic!("Expected MqError::Publish with overflow message"),
        }
    }

    /// L-2 测试：next_id 在 u64::MAX - 1 时仍可正常递增到 u64::MAX
    #[tokio::test]
    async fn test_l2_next_id_near_max() {
        let queue = InMemoryQueue::new();
        {
            let mut inner = queue.inner.write().await;
            inner.next_id = u64::MAX - 1;
        }
        // 第一次 publish：next_id = u64::MAX - 1 → 成功递增到 u64::MAX
        let result1 = queue.publish("topic1", b"msg1").await;
        assert!(result1.is_ok());
        // 第二次 publish：next_id = u64::MAX → checked_add(1) 溢出，应返回错误
        let result2 = queue.publish("topic1", b"msg2").await;
        assert!(result2.is_err());
    }
}
