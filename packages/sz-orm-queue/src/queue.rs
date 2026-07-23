use crate::error::MqError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Notify, RwLock};

// ============================================================================
// 核心 Trait
// ============================================================================

/// 消息队列统一抽象
///
/// 提供发布/消费/确认/订阅四个核心方法，
/// 以及 nack（重试）和 reject（死信）两个扩展方法（带默认实现，向后兼容）。
#[async_trait]
pub trait MessageQueue: Send + Sync {
    /// 发布消息到指定 topic
    async fn publish(&self, topic: &str, message: &[u8]) -> Result<(), MqError>;

    /// 从指定 topic 消费一条消息（消息进入 in_flight 状态）
    async fn consume(&self, topic: &str) -> Result<Option<Message>, MqError>;

    /// 确认消息已处理完成（从 in_flight 移除）
    async fn ack(&self, message_id: &str) -> Result<(), MqError>;

    /// 订阅 topic
    async fn subscribe(&self, topic: &str) -> Result<(), MqError>;

    /// 消息重回队列尾部（带重试次数追踪）
    ///
    /// - 将消息从 in_flight 移回原 topic 队列尾部，retry_count + 1
    /// - 当 retry_count 达到 max_retries 时自动转入死信队列
    ///
    /// 默认实现返回 NotSupported 错误（保持向后兼容）。
    async fn nack(&self, _message_id: &str) -> Result<(), MqError> {
        Err(MqError::NotSupported("nack not supported".to_string()))
    }

    /// 消息直接进入死信队列（不重试，不增加 retry_count）
    ///
    /// 默认实现返回 NotSupported 错误（保持向后兼容）。
    async fn reject(&self, _message_id: &str) -> Result<(), MqError> {
        Err(MqError::NotSupported("reject not supported".to_string()))
    }
}

// ============================================================================
// 消息
// ============================================================================

/// 消息体
///
/// 包含 topic、payload、key、timestamp、headers、id 以及重试次数 retry_count。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub topic: String,
    pub payload: Vec<u8>,
    pub key: Option<String>,
    pub timestamp: i64,
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub id: String,
    /// 重试次数（nack 时递增，达到 max_retries 后转入死信队列）
    #[serde(default)]
    pub retry_count: u32,
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
            retry_count: 0,
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

// ============================================================================
// 配置
// ============================================================================

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

// ============================================================================
// 重连策略（ReconnectPolicy）
// ============================================================================

/// 重连策略（指数退避）
///
/// 用于 `QueueWrapper::with_reconnect`，在网络错误（`MqError::Connection`）时自动重试。
///
/// - `max_retries`：最大重试次数（默认 5）
/// - `initial_delay_ms`：初始延迟毫秒（默认 100）
/// - `max_delay_ms`：最大延迟毫秒（默认 10000）
/// - `multiplier`：退避倍数（默认 2.0，指数退避）
#[derive(Debug, Clone)]
pub struct ReconnectPolicy {
    /// 最大重试次数（默认 5）
    pub max_retries: u32,
    /// 初始延迟（毫秒，默认 100）
    pub initial_delay_ms: u64,
    /// 最大延迟（毫秒，默认 10000）
    pub max_delay_ms: u64,
    /// 退避倍数（默认 2.0，指数退避）
    pub multiplier: f64,
}

impl Default for ReconnectPolicy {
    fn default() -> Self {
        Self {
            max_retries: 5,
            initial_delay_ms: 100,
            max_delay_ms: 10_000,
            multiplier: 2.0,
        }
    }
}

impl ReconnectPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    /// 计算第 `attempt` 次重试的延迟（指数退避，封顶 max_delay_ms）
    ///
    /// - `attempt = 0`：返回 `initial_delay_ms`
    /// - `attempt = 1`：返回 `initial_delay_ms * multiplier`
    /// - 以此类推，但不超过 `max_delay_ms`
    ///
    /// 使用 `max(0.0)` 防止负数 multiplier 导致负延迟。
    pub fn next_delay(&self, attempt: u32) -> Duration {
        let delay_ms = (self.initial_delay_ms as f64) * self.multiplier.powi(attempt as i32);
        // 防止负数或 NaN，封顶 max_delay_ms
        let delay_ms = delay_ms.max(0.0).min(self.max_delay_ms as f64);
        Duration::from_millis(delay_ms as u64)
    }
}

/// 重连状态追踪
///
/// 用于记录当前重连次数和上次重连时间，供外部监控使用。
#[derive(Debug, Clone, Default)]
pub struct ReconnectState {
    /// 当前重连次数
    pub attempts: u32,
    /// 上次重连时间
    pub last_reconnect: Option<Instant>,
}

// ============================================================================
// 背压策略（BackpressurePolicy）
// ============================================================================

/// 背压策略
///
/// 用于 `InMemoryQueue::with_backpressure`，控制队列满时的行为。
///
/// - `max_queue_size`：每 topic 最大队列长度（默认 10000）
/// - `on_overflow`：队列满时的溢出处理策略
#[derive(Debug, Clone)]
pub struct BackpressurePolicy {
    /// 每 topic 最大队列长度（默认 10000）
    pub max_queue_size: usize,
    /// 队列满时的溢出处理策略
    pub on_overflow: OverflowStrategy,
}

impl Default for BackpressurePolicy {
    fn default() -> Self {
        Self {
            max_queue_size: 10_000,
            on_overflow: OverflowStrategy::Reject,
        }
    }
}

/// 溢出处理策略
///
/// - `Block`：阻塞等待（async，直到队列有空间）
/// - `DropOldest`：丢弃最旧消息后插入新的
/// - `DropNewest`：丢弃新消息（返回 Ok，不插入）
/// - `Reject`：拒绝（返回 Err）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverflowStrategy {
    /// 阻塞等待（async，直到队列有空间）
    Block,
    /// 丢弃最旧消息后插入新的
    DropOldest,
    /// 丢弃新消息（返回 Ok，不插入）
    DropNewest,
    /// 拒绝（返回 Err）
    Reject,
}

// ============================================================================
// InMemoryQueue
// ============================================================================

pub struct InMemoryQueue {
    inner: Arc<RwLock<InMemoryQueueInner>>,
}

impl Clone for InMemoryQueue {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

struct InMemoryQueueInner {
    /// 按 topic 分组的就绪队列
    queues: HashMap<String, VecDeque<Message>>,
    /// 已消费未确认的消息（按 message_id 索引）
    in_flight: HashMap<String, Message>,
    /// 按 topic 分组的订阅者计数
    subscribers: HashMap<String, usize>,
    /// 下一个消息 ID（自增）
    next_id: u64,
    /// H-3 修复：每个 topic 最大消息数限制（防止 OOM）
    /// 默认 100,000，可通过 `with_max_messages_per_topic` 调整
    max_messages_per_topic: usize,
    /// 死信队列：按 topic 分组
    dead_letters: HashMap<String, VecDeque<Message>>,
    /// 最大重试次数（默认 3，nack 达到此值后转入死信队列）
    max_retries: u32,
    /// 背压策略（None 时使用 max_messages_per_topic + Reject 行为，保持向后兼容）
    backpressure: Option<BackpressurePolicy>,
    /// Block 策略的通知器（按 topic）
    notify: HashMap<String, Arc<Notify>>,
}

/// 默认每 topic 最大消息数（H-3 修复）
const DEFAULT_MAX_MESSAGES_PER_TOPIC: usize = 100_000;

/// 默认最大重试次数
const DEFAULT_MAX_RETRIES: u32 = 3;

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
                dead_letters: HashMap::new(),
                max_retries: DEFAULT_MAX_RETRIES,
                backpressure: None,
                notify: HashMap::new(),
            })),
        }
    }

    /// 创建指定最大重试次数的队列
    ///
    /// - `max_retries = 3`（默认）：允许 3 次 nack 重试，第 3 次 nack 转入死信队列
    /// - `max_retries = 0`：不允许重试，第一次 nack 即转入死信队列
    pub fn with_max_retries(max_retries: u32) -> Self {
        Self {
            inner: Arc::new(RwLock::new(InMemoryQueueInner {
                queues: HashMap::new(),
                in_flight: HashMap::new(),
                subscribers: HashMap::new(),
                next_id: 1,
                max_messages_per_topic: DEFAULT_MAX_MESSAGES_PER_TOPIC,
                dead_letters: HashMap::new(),
                max_retries,
                backpressure: None,
                notify: HashMap::new(),
            })),
        }
    }

    /// 创建带背压策略的队列
    ///
    /// `policy.max_queue_size` 将同时设置 `max_messages_per_topic`，
    /// 确保背压策略与 H-3 限制一致。
    pub fn with_backpressure(policy: BackpressurePolicy) -> Self {
        let max = policy.max_queue_size;
        Self {
            inner: Arc::new(RwLock::new(InMemoryQueueInner {
                queues: HashMap::new(),
                in_flight: HashMap::new(),
                subscribers: HashMap::new(),
                next_id: 1,
                max_messages_per_topic: max,
                dead_letters: HashMap::new(),
                max_retries: DEFAULT_MAX_RETRIES,
                backpressure: Some(policy),
                notify: HashMap::new(),
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

    /// 死信队列中的消息数（指定 topic）
    ///
    /// 如果 topic 不存在死信队列，返回 0。
    pub async fn dead_letter_count(&self, topic: &str) -> usize {
        let inner = self.inner.read().await;
        inner
            .dead_letters
            .get(topic)
            .map(|q| q.len())
            .unwrap_or(0)
    }

    /// 消费一条死信消息（从死信队列头部弹出）
    ///
    /// 注意：此操作不会增加 in_flight 计数，死信消息不再走正常 ack 流程。
    /// 返回 `None` 表示该 topic 没有死信消息。
    pub async fn consume_dead_letter(&self, topic: &str) -> Option<Message> {
        let mut inner = self.inner.write().await;
        if let Some(dq) = inner.dead_letters.get_mut(topic) {
            return dq.pop_front();
        }
        None
    }

    /// 将死信消息重新放回原 topic 队列（重置 retry_count = 0）
    ///
    /// 在所有 topic 的死信队列中查找指定 `message_id`。
    /// 找到后从死信队列移除，重置 retry_count，放回原 topic 队列尾部。
    pub async fn requeue_dead_letter(&self, message_id: &str) -> Result<(), MqError> {
        let mut inner = self.inner.write().await;
        // 遍历所有 topic 的死信队列查找消息
        for dq in inner.dead_letters.values_mut() {
            let mut found_idx = None;
            for (idx, m) in dq.iter().enumerate() {
                if m.id == message_id {
                    found_idx = Some(idx);
                    break;
                }
            }
            if let Some(idx) = found_idx {
                let mut msg = dq.remove(idx).expect("checked: idx exists");
                // 重置重试次数
                msg.retry_count = 0;
                // 重新放入原 topic 队列尾部
                inner
                    .queues
                    .entry(msg.topic.clone())
                    .or_insert_with(VecDeque::new)
                    .push_back(msg);
                return Ok(());
            }
        }
        Err(MqError::NotSupported(format!(
            "Dead letter not found: {}",
            message_id
        )))
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
        // 获取背压策略（如果有）；None 时使用 Reject（保持 H-3 向后兼容）
        let strategy = {
            let inner = self.inner.read().await;
            inner
                .backpressure
                .as_ref()
                .map(|p| p.on_overflow)
                .unwrap_or(OverflowStrategy::Reject)
        };

        match strategy {
            OverflowStrategy::Block => self.publish_with_block(topic, message).await,
            _ => self.publish_immediate(topic, message, strategy).await,
        }
    }

    async fn consume(&self, topic: &str) -> Result<Option<Message>, MqError> {
        let mut inner = self.inner.write().await;
        let queue = inner
            .queues
            .entry(topic.to_string())
            .or_insert_with(VecDeque::new);
        if let Some(msg) = queue.pop_front() {
            inner.in_flight.insert(msg.id.clone(), msg.clone());
            // 通知等待的 publisher（Block 策略）：队列有空间了
            if let Some(notify) = inner.notify.get(topic) {
                notify.notify_one();
            }
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

    async fn nack(&self, message_id: &str) -> Result<(), MqError> {
        let mut inner = self.inner.write().await;
        let mut msg = inner.in_flight.remove(message_id).ok_or_else(|| {
            MqError::NotSupported(format!("Message not found for nack: {}", message_id))
        })?;

        // 增加重试次数（saturating_add 防止 u32 溢出）
        msg.retry_count = msg.retry_count.saturating_add(1);

        // 检查是否达到最大重试次数：达到则转入死信队列
        if msg.retry_count >= inner.max_retries {
            inner
                .dead_letters
                .entry(msg.topic.clone())
                .or_insert_with(VecDeque::new)
                .push_back(msg);
        } else {
            // 未达到上限：重回原 topic 队列尾部等待再次消费
            // 注意：nack 增加了队列长度，不通知 Block publisher（避免唤醒后立即又满）
            inner
                .queues
                .entry(msg.topic.clone())
                .or_insert_with(VecDeque::new)
                .push_back(msg);
        }
        Ok(())
    }

    async fn reject(&self, message_id: &str) -> Result<(), MqError> {
        let mut inner = self.inner.write().await;
        let msg = inner.in_flight.remove(message_id).ok_or_else(|| {
            MqError::NotSupported(format!("Message not found for reject: {}", message_id))
        })?;
        // 直接进入死信队列（不增加 retry_count）
        inner
            .dead_letters
            .entry(msg.topic.clone())
            .or_insert_with(VecDeque::new)
            .push_back(msg);
        Ok(())
    }
}

impl InMemoryQueue {
    /// 立即模式 publish：根据溢出策略处理满队列
    ///
    /// 用于 Reject / DropOldest / DropNewest 策略。
    async fn publish_immediate(
        &self,
        topic: &str,
        message: &[u8],
        strategy: OverflowStrategy,
    ) -> Result<(), MqError> {
        let mut inner = self.inner.write().await;
        // H-3 修复：检查消息数限制，防止 OOM
        let current_count = inner.queues.get(topic).map(|q| q.len()).unwrap_or(0);
        if current_count >= inner.max_messages_per_topic {
            match strategy {
                OverflowStrategy::DropOldest => {
                    // 弹出最旧消息后插入新的
                    let queue = inner
                        .queues
                        .entry(topic.to_string())
                        .or_insert_with(VecDeque::new);
                    queue.pop_front();
                }
                OverflowStrategy::DropNewest => {
                    // 丢弃新消息，直接返回 Ok
                    return Ok(());
                }
                OverflowStrategy::Reject => {
                    return Err(MqError::Publish(format!(
                        "topic '{}' is full: {} >= {} messages (H-3 protection)",
                        topic, current_count, inner.max_messages_per_topic
                    )));
                }
                OverflowStrategy::Block => {
                    unreachable!("Block strategy handled by publish_with_block")
                }
            }
        }
        // 生成消息 ID 并插入
        let id = format!("msg-{}", inner.next_id);
        // L-2 修复：使用 checked_add 防止 u64 溢出
        inner.next_id = inner
            .next_id
            .checked_add(1)
            .ok_or_else(|| MqError::Publish("message id overflow: u64::MAX reached".to_string()))?;
        let msg = Message {
            id,
            retry_count: 0,
            ..Message::new(topic, message.to_vec())
        };
        inner
            .queues
            .entry(topic.to_string())
            .or_insert_with(VecDeque::new)
            .push_back(msg);
        Ok(())
    }

    /// 阻塞模式 publish：队列满时等待，直到有空间
    ///
    /// 用于 Block 策略。使用 `tokio::sync::Notify` 实现等待/通知。
    async fn publish_with_block(
        &self,
        topic: &str,
        message: &[u8],
    ) -> Result<(), MqError> {
        loop {
            let notify = {
                let mut inner = self.inner.write().await;
                let current_count = inner.queues.get(topic).map(|q| q.len()).unwrap_or(0);
                if current_count < inner.max_messages_per_topic {
                    // 有空间，插入并返回
                    let id = format!("msg-{}", inner.next_id);
                    inner.next_id = inner.next_id.checked_add(1).ok_or_else(|| {
                        MqError::Publish("message id overflow: u64::MAX reached".to_string())
                    })?;
                    let msg = Message {
                        id,
                        retry_count: 0,
                        ..Message::new(topic, message.to_vec())
                    };
                    inner
                        .queues
                        .entry(topic.to_string())
                        .or_insert_with(VecDeque::new)
                        .push_back(msg);
                    return Ok(());
                }
                // 队列满，获取 Notify 引用（按 topic 隔离）
                inner
                    .notify
                    .entry(topic.to_string())
                    .or_insert_with(|| Arc::new(Notify::new()))
                    .clone()
            };
            // 释放写锁后等待通知（避免长时间持锁）
            // Notify 内部使用 permit 机制，不会丢失通知
            notify.notified().await;
        }
    }
}

// ============================================================================
// QueueWrapper
// ============================================================================

pub struct QueueWrapper {
    queue: Box<dyn MessageQueue>,
    /// 重连策略（None 表示不重试，直接返回错误）
    reconnect: Option<ReconnectPolicy>,
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
        Self {
            queue,
            reconnect: None,
        }
    }

    /// 从已有 queue 创建 wrapper（用于测试自定义 MessageQueue 实现）
    #[cfg(test)]
    pub(crate) fn with_queue(queue: Box<dyn MessageQueue>) -> Self {
        Self {
            queue,
            reconnect: None,
        }
    }

    /// 设置重连策略
    ///
    /// 启用后，`publish` / `consume` 在遇到 `MqError::Connection` 错误时
    /// 会按指数退避策略自动重试，最多重试 `policy.max_retries` 次。
    pub fn with_reconnect(mut self, policy: ReconnectPolicy) -> Self {
        self.reconnect = Some(policy);
        self
    }

    pub async fn publish(&self, topic: &str, message: &[u8]) -> Result<(), MqError> {
        if let Some(policy) = &self.reconnect {
            let mut attempts = 0u32;
            loop {
                match self.queue.publish(topic, message).await {
                    Ok(()) => return Ok(()),
                    Err(MqError::Connection(_)) if attempts < policy.max_retries => {
                        let delay = policy.next_delay(attempts);
                        tokio::time::sleep(delay).await;
                        attempts += 1;
                    }
                    Err(e) => return Err(e),
                }
            }
        } else {
            self.queue.publish(topic, message).await
        }
    }

    pub async fn consume(&self, topic: &str) -> Result<Option<Message>, MqError> {
        if let Some(policy) = &self.reconnect {
            let mut attempts = 0u32;
            loop {
                match self.queue.consume(topic).await {
                    Ok(msg) => return Ok(msg),
                    Err(MqError::Connection(_)) if attempts < policy.max_retries => {
                        let delay = policy.next_delay(attempts);
                        tokio::time::sleep(delay).await;
                        attempts += 1;
                    }
                    Err(e) => return Err(e),
                }
            }
        } else {
            self.queue.consume(topic).await
        }
    }

    pub async fn ack(&self, message_id: &str) -> Result<(), MqError> {
        self.queue.ack(message_id).await
    }

    pub async fn subscribe(&self, topic: &str) -> Result<(), MqError> {
        self.queue.subscribe(topic).await
    }

    /// 消息重回队列尾部（带重试次数追踪）
    ///
    /// 委托给底层 queue 的 nack 实现。重连策略不应用于 nack。
    pub async fn nack(&self, message_id: &str) -> Result<(), MqError> {
        self.queue.nack(message_id).await
    }

    /// 消息直接进入死信队列
    ///
    /// 委托给底层 queue 的 reject 实现。重连策略不应用于 reject。
    pub async fn reject(&self, message_id: &str) -> Result<(), MqError> {
        self.queue.reject(message_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    // ========================================================================
    // 既有测试（保持不变）
    // ========================================================================

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
    #[tokio::test]
    async fn test_l2_next_id_overflow_protection() {
        let queue = InMemoryQueue::new();
        {
            let mut inner = queue.inner.write().await;
            inner.next_id = u64::MAX;
        }
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
        let result1 = queue.publish("topic1", b"msg1").await;
        assert!(result1.is_ok());
        let result2 = queue.publish("topic1", b"msg2").await;
        assert!(result2.is_err());
    }

    // ========================================================================
    // nack / reject / 死信队列测试
    // ========================================================================

    /// nack 将消息重回队列尾部，并增加 retry_count
    #[tokio::test]
    async fn test_nack_requeues_message_with_retry_count() {
        let queue = InMemoryQueue::new();
        queue.publish("topic", b"msg1").await.unwrap();
        let msg = queue.consume("topic").await.unwrap().unwrap();
        assert_eq!(msg.retry_count, 0);

        // nack 后消息重回队列
        queue.nack(&msg.id).await.unwrap();
        assert_eq!(queue.message_count("topic").await, 1);
        assert_eq!(queue.in_flight_count().await, 0);

        // 再次消费，retry_count 应为 1
        let msg2 = queue.consume("topic").await.unwrap().unwrap();
        assert_eq!(msg2.id, msg.id);
        assert_eq!(msg2.retry_count, 1);
    }

    /// nack 多次后 retry_count 持续递增（未达上限前）
    ///
    /// 注意：consume 时看到的 retry_count 是上一次 nack 后的值（即本次 nack 之前的值）。
    /// - 第 1 次 consume：retry_count = 0（刚 publish）
    /// - nack → retry_count = 1，重回队列
    /// - 第 2 次 consume：retry_count = 1
    /// - nack → retry_count = 2，重回队列
    /// - 以此类推
    #[tokio::test]
    async fn test_nack_increments_retry_count() {
        let queue = InMemoryQueue::with_max_retries(10);
        queue.publish("topic", b"data").await.unwrap();

        for expected_retry in 0..5u32 {
            let msg = queue.consume("topic").await.unwrap().unwrap();
            assert_eq!(
                msg.retry_count, expected_retry,
                "consume should show retry_count before this iteration's nack"
            );
            queue.nack(&msg.id).await.unwrap();
        }
        // 消息仍在就绪队列中（未达 max_retries=10）
        assert_eq!(queue.message_count("topic").await, 1);
        assert_eq!(queue.dead_letter_count("topic").await, 0);
    }

    /// nack 达到 max_retries 后自动转入死信队列
    #[tokio::test]
    async fn test_nack_max_retries_sends_to_dlx() {
        // max_retries = 3：第 3 次 nack 后转入 DLX
        let queue = InMemoryQueue::with_max_retries(3);
        queue.publish("topic", b"payload").await.unwrap();

        // 第 1 次 nack：retry_count = 1，重回队列
        let msg = queue.consume("topic").await.unwrap().unwrap();
        queue.nack(&msg.id).await.unwrap();
        assert_eq!(queue.message_count("topic").await, 1);
        assert_eq!(queue.dead_letter_count("topic").await, 0);

        // 第 2 次 nack：retry_count = 2，重回队列
        let msg = queue.consume("topic").await.unwrap().unwrap();
        assert_eq!(msg.retry_count, 1);
        queue.nack(&msg.id).await.unwrap();
        assert_eq!(queue.message_count("topic").await, 1);
        assert_eq!(queue.dead_letter_count("topic").await, 0);

        // 第 3 次 nack：retry_count = 3，达到 max_retries，转入 DLX
        let msg = queue.consume("topic").await.unwrap().unwrap();
        assert_eq!(msg.retry_count, 2);
        queue.nack(&msg.id).await.unwrap();
        assert_eq!(queue.message_count("topic").await, 0);
        assert_eq!(queue.dead_letter_count("topic").await, 1);
    }

    /// max_retries = 0 时，第一次 nack 立即转入死信队列
    #[tokio::test]
    async fn test_nack_max_retries_zero_sends_to_dlx_immediately() {
        let queue = InMemoryQueue::with_max_retries(0);
        queue.publish("topic", b"msg").await.unwrap();
        let msg = queue.consume("topic").await.unwrap().unwrap();
        assert_eq!(msg.retry_count, 0);

        // nack 后 retry_count 变为 1，1 >= 0 → 立即转入 DLX
        queue.nack(&msg.id).await.unwrap();

        assert_eq!(queue.message_count("topic").await, 0);
        assert_eq!(queue.dead_letter_count("topic").await, 1);

        // 验证 DLX 中的消息 retry_count = 1
        let dlq_msg = queue
            .consume_dead_letter("topic")
            .await
            .expect("should have dead letter");
        assert_eq!(dlq_msg.retry_count, 1);
    }

    /// nack 不存在的 message_id 返回错误
    #[tokio::test]
    async fn test_nack_unknown_message_id_returns_error() {
        let queue = InMemoryQueue::new();
        let result = queue.nack("nonexistent-id").await;
        assert!(result.is_err());
        match result {
            Err(MqError::NotSupported(msg)) => {
                assert!(msg.contains("not found for nack"));
            }
            _ => panic!("Expected MqError::NotSupported"),
        }
    }

    /// reject 将消息直接送入死信队列
    #[tokio::test]
    async fn test_reject_sends_to_dead_letter_queue() {
        let queue = InMemoryQueue::new();
        queue.publish("topic", b"bad-msg").await.unwrap();
        let msg = queue.consume("topic").await.unwrap().unwrap();

        queue.reject(&msg.id).await.unwrap();

        assert_eq!(queue.message_count("topic").await, 0);
        assert_eq!(queue.in_flight_count().await, 0);
        assert_eq!(queue.dead_letter_count("topic").await, 1);
    }

    /// reject 不增加 retry_count
    #[tokio::test]
    async fn test_reject_does_not_increment_retry_count() {
        let queue = InMemoryQueue::new();
        queue.publish("topic", b"msg").await.unwrap();
        let msg = queue.consume("topic").await.unwrap().unwrap();
        assert_eq!(msg.retry_count, 0);

        queue.reject(&msg.id).await.unwrap();

        let dlq_msg = queue
            .consume_dead_letter("topic")
            .await
            .expect("should have dead letter");
        assert_eq!(dlq_msg.retry_count, 0, "reject should not increment retry_count");
    }

    /// reject 不存在的 message_id 返回错误
    #[tokio::test]
    async fn test_reject_unknown_message_id_returns_error() {
        let queue = InMemoryQueue::new();
        let result = queue.reject("nonexistent-id").await;
        assert!(result.is_err());
    }

    /// 空队列 reject 返回错误（in_flight 为空）
    #[tokio::test]
    async fn test_reject_empty_in_flight_returns_error() {
        let queue = InMemoryQueue::new();
        // 没有任何消息在 in_flight 中
        let result = queue.reject("any-id").await;
        assert!(result.is_err());
        assert_eq!(queue.dead_letter_count("topic").await, 0);
    }

    /// dead_letter_count 对不存在的 topic 返回 0
    #[tokio::test]
    async fn test_dead_letter_count_empty_topic() {
        let queue = InMemoryQueue::new();
        assert_eq!(queue.dead_letter_count("no-such-topic").await, 0);
    }

    /// dead_letter_count 在 reject 后正确计数
    #[tokio::test]
    async fn test_dead_letter_count_after_reject() {
        let queue = InMemoryQueue::new();
        queue.publish("topic", b"m1").await.unwrap();
        queue.publish("topic", b"m2").await.unwrap();

        let m1 = queue.consume("topic").await.unwrap().unwrap();
        queue.reject(&m1.id).await.unwrap();
        assert_eq!(queue.dead_letter_count("topic").await, 1);

        let m2 = queue.consume("topic").await.unwrap().unwrap();
        queue.reject(&m2.id).await.unwrap();
        assert_eq!(queue.dead_letter_count("topic").await, 2);
    }

    /// consume_dead_letter 弹出最旧的死信消息
    #[tokio::test]
    async fn test_consume_dead_letter() {
        let queue = InMemoryQueue::new();
        queue.publish("topic", b"first").await.unwrap();
        queue.publish("topic", b"second").await.unwrap();

        let m1 = queue.consume("topic").await.unwrap().unwrap();
        queue.reject(&m1.id).await.unwrap();
        let m2 = queue.consume("topic").await.unwrap().unwrap();
        queue.reject(&m2.id).await.unwrap();

        // FIFO 顺序
        let d1 = queue.consume_dead_letter("topic").await.expect("should have dead letter");
        assert_eq!(d1.payload, b"first");
        let d2 = queue.consume_dead_letter("topic").await.expect("should have dead letter");
        assert_eq!(d2.payload, b"second");

        // 死信队列已空
        assert!(queue.consume_dead_letter("topic").await.is_none());
    }

    /// consume_dead_letter 对不存在的 topic 返回 None
    #[tokio::test]
    async fn test_consume_dead_letter_empty() {
        let queue = InMemoryQueue::new();
        assert!(queue.consume_dead_letter("no-such-topic").await.is_none());
    }

    /// requeue_dead_letter 将死信消息放回原队列，并重置 retry_count
    #[tokio::test]
    async fn test_requeue_dead_letter_resets_retry_count() {
        let queue = InMemoryQueue::with_max_retries(2);
        queue.publish("topic", b"msg").await.unwrap();

        // 第 1 次 nack：retry_count = 1，重回队列
        let m1 = queue.consume("topic").await.unwrap().unwrap();
        queue.nack(&m1.id).await.unwrap();
        // 第 2 次 nack：retry_count = 2，达到 max_retries=2，转入 DLX
        let m2 = queue.consume("topic").await.unwrap().unwrap();
        assert_eq!(m2.retry_count, 1);
        queue.nack(&m2.id).await.unwrap();

        assert_eq!(queue.dead_letter_count("topic").await, 1);
        assert_eq!(queue.message_count("topic").await, 0);

        // 重新入队，retry_count 应重置为 0
        queue.requeue_dead_letter(&m1.id).await.unwrap();
        assert_eq!(queue.dead_letter_count("topic").await, 0);
        assert_eq!(queue.message_count("topic").await, 1);

        // 验证 retry_count 已重置
        let m3 = queue.consume("topic").await.unwrap().unwrap();
        assert_eq!(m3.id, m1.id);
        assert_eq!(m3.retry_count, 0, "retry_count should be reset after requeue");
    }

    /// requeue_dead_letter 对不存在的 message_id 返回错误
    #[tokio::test]
    async fn test_requeue_dead_letter_not_found() {
        let queue = InMemoryQueue::new();
        let result = queue.requeue_dead_letter("nonexistent-id").await;
        assert!(result.is_err());
        match result {
            Err(MqError::NotSupported(msg)) => {
                assert!(msg.contains("Dead letter not found"));
            }
            _ => panic!("Expected MqError::NotSupported"),
        }
    }

    // ========================================================================
    // ReconnectPolicy 测试
    // ========================================================================

    /// ReconnectPolicy 默认值
    #[test]
    fn test_reconnect_policy_default_values() {
        let policy = ReconnectPolicy::default();
        assert_eq!(policy.max_retries, 5);
        assert_eq!(policy.initial_delay_ms, 100);
        assert_eq!(policy.max_delay_ms, 10_000);
        assert!((policy.multiplier - 2.0).abs() < f64::EPSILON);
    }

    /// ReconnectPolicy 指数退避计算
    #[test]
    fn test_reconnect_policy_next_delay_exponential() {
        let policy = ReconnectPolicy {
            max_retries: 5,
            initial_delay_ms: 100,
            max_delay_ms: 10_000,
            multiplier: 2.0,
        };
        // attempt 0: 100 * 2^0 = 100
        assert_eq!(policy.next_delay(0), Duration::from_millis(100));
        // attempt 1: 100 * 2^1 = 200
        assert_eq!(policy.next_delay(1), Duration::from_millis(200));
        // attempt 2: 100 * 2^2 = 400
        assert_eq!(policy.next_delay(2), Duration::from_millis(400));
        // attempt 3: 100 * 2^3 = 800
        assert_eq!(policy.next_delay(3), Duration::from_millis(800));
    }

    /// ReconnectPolicy 延迟封顶 max_delay_ms
    #[test]
    fn test_reconnect_policy_next_delay_capped_at_max() {
        let policy = ReconnectPolicy {
            max_retries: 10,
            initial_delay_ms: 100,
            max_delay_ms: 1000,
            multiplier: 2.0,
        };
        // attempt 4: 100 * 2^4 = 1600 > 1000 → 封顶为 1000
        assert_eq!(policy.next_delay(4), Duration::from_millis(1000));
        // attempt 10: 同样封顶
        assert_eq!(policy.next_delay(10), Duration::from_millis(1000));
    }

    /// ReconnectPolicy attempt = 0 返回 initial_delay_ms
    #[test]
    fn test_reconnect_policy_zero_attempt() {
        let policy = ReconnectPolicy {
            max_retries: 3,
            initial_delay_ms: 500,
            max_delay_ms: 10_000,
            multiplier: 3.0,
        };
        assert_eq!(policy.next_delay(0), Duration::from_millis(500));
    }

    // ========================================================================
    // QueueWrapper 重连测试（使用 Mock 队列）
    // ========================================================================

    /// 模拟连接错误的队列（用于测试重连）
    ///
    /// 在第 `succeed_on_attempt` 次调用 publish 时返回 Ok，之前返回 Connection 错误。
    struct FailingQueue {
        call_count: AtomicU32,
        succeed_on_attempt: u32,
    }

    impl FailingQueue {
        fn new(succeed_on_attempt: u32) -> Self {
            Self {
                call_count: AtomicU32::new(0),
                succeed_on_attempt,
            }
        }
    }

    #[async_trait]
    impl MessageQueue for FailingQueue {
        async fn publish(&self, _topic: &str, _message: &[u8]) -> Result<(), MqError> {
            let attempt = self.call_count.fetch_add(1, Ordering::SeqCst) + 1;
            if attempt >= self.succeed_on_attempt {
                Ok(())
            } else {
                Err(MqError::Connection("simulated connection error".to_string()))
            }
        }

        async fn consume(&self, _topic: &str) -> Result<Option<Message>, MqError> {
            Err(MqError::Connection("simulated connection error".to_string()))
        }

        async fn ack(&self, _message_id: &str) -> Result<(), MqError> {
            Err(MqError::Connection("simulated".to_string()))
        }

        async fn subscribe(&self, _topic: &str) -> Result<(), MqError> {
            Err(MqError::Connection("simulated".to_string()))
        }
    }

    /// 模拟非连接错误的队列（用于测试不重试非连接错误）
    struct PublishErrorQueue;

    #[async_trait]
    impl MessageQueue for PublishErrorQueue {
        async fn publish(&self, _topic: &str, _message: &[u8]) -> Result<(), MqError> {
            Err(MqError::Publish("non-connection error".to_string()))
        }

        async fn consume(&self, _topic: &str) -> Result<Option<Message>, MqError> {
            Err(MqError::Publish("non-connection error".to_string()))
        }

        async fn ack(&self, _message_id: &str) -> Result<(), MqError> {
            Ok(())
        }

        async fn subscribe(&self, _topic: &str) -> Result<(), MqError> {
            Ok(())
        }
    }

    /// 重连：在 Connection 错误时自动重试，最终成功
    #[tokio::test]
    async fn test_reconnect_retries_on_connection_error() {
        // 第 3 次调用成功（前 2 次失败）
        let failing = FailingQueue::new(3);
        let wrapper = QueueWrapper::with_queue(Box::new(failing))
            .with_reconnect(ReconnectPolicy {
                max_retries: 5,
                initial_delay_ms: 1, // 测试用短延迟
                max_delay_ms: 10,
                multiplier: 2.0,
            });

        let result = wrapper.publish("topic", b"data").await;
        assert!(result.is_ok(), "should succeed after retries");
    }

    /// 重连：达到 max_retries 后放弃，返回错误
    #[tokio::test]
    async fn test_reconnect_gives_up_after_max_retries() {
        // 永不成功
        let failing = FailingQueue::new(u32::MAX);
        let wrapper = QueueWrapper::with_queue(Box::new(failing))
            .with_reconnect(ReconnectPolicy {
                max_retries: 2,
                initial_delay_ms: 1,
                max_delay_ms: 10,
                multiplier: 2.0,
            });

        let result = wrapper.publish("topic", b"data").await;
        assert!(result.is_err());
        match result {
            Err(MqError::Connection(_)) => {}
            _ => panic!("Expected MqError::Connection"),
        }
    }

    /// 重连：非 Connection 错误不触发重试
    #[tokio::test]
    async fn test_reconnect_no_retry_on_non_connection_error() {
        let wrapper = QueueWrapper::with_queue(Box::new(PublishErrorQueue))
            .with_reconnect(ReconnectPolicy {
                max_retries: 5,
                initial_delay_ms: 1,
                max_delay_ms: 10,
                multiplier: 2.0,
            });

        let result = wrapper.publish("topic", b"data").await;
        assert!(result.is_err());
        match result {
            Err(MqError::Publish(msg)) => {
                assert!(msg.contains("non-connection error"));
            }
            _ => panic!("Expected MqError::Publish"),
        }
    }

    // ========================================================================
    // Backpressure 测试
    // ========================================================================

    /// DropOldest 策略：队列满时丢弃最旧消息
    #[tokio::test]
    async fn test_backpressure_drop_oldest() {
        let queue = InMemoryQueue::with_backpressure(BackpressurePolicy {
            max_queue_size: 2,
            on_overflow: OverflowStrategy::DropOldest,
        });

        queue.publish("topic", b"m1").await.unwrap();
        queue.publish("topic", b"m2").await.unwrap();
        // 队列已满（2 条），第 3 条触发 DropOldest：丢弃 m1，插入 m3
        queue.publish("topic", b"m3").await.unwrap();

        assert_eq!(queue.message_count("topic").await, 2);

        // 验证最旧消息 m1 已被丢弃
        let m1 = queue.consume("topic").await.unwrap().unwrap();
        assert_eq!(m1.payload, b"m2", "oldest should be dropped");
        let m2 = queue.consume("topic").await.unwrap().unwrap();
        assert_eq!(m2.payload, b"m3");
    }

    /// DropNewest 策略：队列满时丢弃新消息（返回 Ok）
    #[tokio::test]
    async fn test_backpressure_drop_newest() {
        let queue = InMemoryQueue::with_backpressure(BackpressurePolicy {
            max_queue_size: 1,
            on_overflow: OverflowStrategy::DropNewest,
        });

        queue.publish("topic", b"m1").await.unwrap();
        // 队列已满，第 2 条触发 DropNewest：丢弃 m2，返回 Ok
        let result = queue.publish("topic", b"m2").await;
        assert!(result.is_ok());

        assert_eq!(queue.message_count("topic").await, 1);
        // 验证保留的是 m1（旧消息）
        let m = queue.consume("topic").await.unwrap().unwrap();
        assert_eq!(m.payload, b"m1", "newest should be dropped");
    }

    /// Reject 策略：队列满时返回错误（与 H-3 行为一致）
    #[tokio::test]
    async fn test_backpressure_reject() {
        let queue = InMemoryQueue::with_backpressure(BackpressurePolicy {
            max_queue_size: 1,
            on_overflow: OverflowStrategy::Reject,
        });

        queue.publish("topic", b"m1").await.unwrap();
        // 队列已满，第 2 条触发 Reject：返回错误
        let result = queue.publish("topic", b"m2").await;
        assert!(result.is_err());

        assert_eq!(queue.message_count("topic").await, 1);
    }

    /// Block 策略：队列满时阻塞，consume 后解除阻塞
    #[tokio::test]
    async fn test_backpressure_block_unblocks_on_consume() {
        let queue = InMemoryQueue::with_backpressure(BackpressurePolicy {
            max_queue_size: 1,
            on_overflow: OverflowStrategy::Block,
        });
        queue.publish("topic", b"m1").await.unwrap();

        // 在另一个任务中尝试 publish（应阻塞）
        let queue_clone = queue.clone();
        let handle = tokio::spawn(async move {
            queue_clone.publish("topic", b"m2").await
        });

        // 等待 50ms，确认任务仍在阻塞
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!handle.is_finished(), "publish should be blocked");

        // consume 一条消息，释放空间
        queue.consume("topic").await.unwrap();

        // 阻塞的 publish 应能完成
        let result = tokio::time::timeout(Duration::from_secs(1), handle)
            .await
            .expect("publish should complete after consume");
        assert!(result.is_ok(), "publish should succeed: {:?}", result);

        // 验证 m2 已入队
        assert_eq!(queue.message_count("topic").await, 1);
        let m = queue.consume("topic").await.unwrap().unwrap();
        assert_eq!(m.payload, b"m2");
    }

    /// Block 策略：队列持续满时，publish 在超时下保持阻塞
    #[tokio::test]
    async fn test_backpressure_block_times_out_when_queue_stays_full() {
        let queue = InMemoryQueue::with_backpressure(BackpressurePolicy {
            max_queue_size: 1,
            on_overflow: OverflowStrategy::Block,
        });
        queue.publish("topic", b"m1").await.unwrap();

        // 尝试 publish，应阻塞；用 timeout 验证它不会立即返回
        let result = tokio::time::timeout(
            Duration::from_millis(100),
            queue.publish("topic", b"m2"),
        )
        .await;

        // 应超时（队列持续满）
        assert!(result.is_err(), "publish should block and time out");

        // 队列仍只有 1 条消息
        assert_eq!(queue.message_count("topic").await, 1);
    }

    /// Block 策略：不同 topic 独立阻塞（互不影响）
    #[tokio::test]
    async fn test_backpressure_block_isolated_per_topic() {
        let queue = InMemoryQueue::with_backpressure(BackpressurePolicy {
            max_queue_size: 1,
            on_overflow: OverflowStrategy::Block,
        });
        queue.publish("topic-a", b"a1").await.unwrap();
        queue.publish("topic-b", b"b1").await.unwrap();

        // topic-a 满，topic-b 满
        // 向 topic-a publish 应阻塞
        let queue_clone = queue.clone();
        let handle = tokio::spawn(async move {
            queue_clone.publish("topic-a", b"a2").await
        });

        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!handle.is_finished(), "topic-a publish should be blocked");

        // consume topic-b 不应解除 topic-a 的阻塞
        queue.consume("topic-b").await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!handle.is_finished(), "topic-a publish should still be blocked after topic-b consume");

        // consume topic-a 才能解除阻塞
        queue.consume("topic-a").await.unwrap();
        let result = tokio::time::timeout(Duration::from_secs(1), handle)
            .await
            .expect("publish should complete after topic-a consume");
        assert!(result.is_ok());
    }
}
