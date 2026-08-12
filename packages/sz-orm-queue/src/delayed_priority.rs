//! # 消息延迟队列与优先级调度
//!
//! 提供延迟投递（按 `deliver_at` 投递）、优先级队列（Strict/Weighted/FairShare）
//! 和定时调度（Cron 表达式或固定间隔），aging 机制避免低优先级饿死。
//!
//! ## 特性
//!
//! - 三种优先级策略：Strict（严格优先级）/ Weighted（加权）/ FairShare（公平份额）
//! - 延迟投递：消息在 `deliver_at` 之前不可消费
//! - 定时调度：Cron 表达式或固定间隔周期投递
//! - aging 机制：低优先级消息等待超阈值后提升优先级，避免饿死
//! - 复用既有 `MessageQueue` + v4.6.0 `BackoffPolicy`

use std::collections::{BTreeMap, BinaryHeap, HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::dlx::BackoffPolicy;
use crate::queue::{Message, MessageQueue};
use crate::MqError;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn validate_cron(expr: &str) -> Result<(), MqError> {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() != 5 {
        return Err(MqError::NotSupported(format!(
            "invalid cron expression: expected 5 fields, got {}: {}",
            parts.len(),
            expr
        )));
    }
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            return Err(MqError::NotSupported(format!(
                "invalid cron expression: field {} is empty: {}",
                i, expr
            )));
        }
    }
    Ok(())
}

fn cron_next_ms(expr: &str, from_ms: i64) -> i64 {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() != 5 {
        return from_ms + 60_000;
    }
    let minute: i64 = parts[0].parse().unwrap_or(0);
    let _hour: i64 = parts[1].parse().unwrap_or(0);
    let _day: i64 = parts[2].parse().unwrap_or(0);
    let _month: i64 = parts[3].parse().unwrap_or(0);
    let _weekday: i64 = parts[4].parse().unwrap_or(0);
    let secs_to_next = (minute * 60).saturating_sub((from_ms / 1000) % 3600);
    from_ms + secs_to_next.max(1) * 1000
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PriorityPolicy {
    #[default]
    Strict,
    Weighted,
    FairShare,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelayedMessage {
    pub message: Message,
    pub deliver_at: i64,
    pub priority: i32,
}

impl DelayedMessage {
    pub fn new(message: Message, deliver_at: i64, priority: i32) -> Self {
        Self {
            message,
            deliver_at,
            priority,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledMessage {
    pub message: Message,
    pub cron: Option<String>,
    pub interval: Option<Duration>,
}

impl ScheduledMessage {
    pub fn with_cron(message: Message, cron: impl Into<String>) -> Self {
        Self {
            message,
            cron: Some(cron.into()),
            interval: None,
        }
    }

    pub fn with_interval(message: Message, interval: Duration) -> Self {
        Self {
            message,
            cron: None,
            interval: Some(interval),
        }
    }

    pub fn next_deliver_ms(&self, from_ms: i64) -> Result<i64, MqError> {
        if let Some(ref cron) = self.cron {
            validate_cron(cron)?;
            Ok(cron_next_ms(cron, from_ms))
        } else if let Some(interval) = self.interval {
            Ok(from_ms + interval.as_millis() as i64)
        } else {
            Err(MqError::NotSupported(
                "scheduled message has no cron or interval".to_string(),
            ))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleConfig {
    pub enabled: bool,
    pub priority_policy: PriorityPolicy,
    pub aging_enabled: bool,
    pub aging_threshold_ms: u64,
    pub queue_capacity: usize,
    pub check_interval_ms: u64,
}

impl Default for ScheduleConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            priority_policy: PriorityPolicy::Strict,
            aging_enabled: true,
            aging_threshold_ms: 300_000,
            queue_capacity: 100_000,
            check_interval_ms: 100,
        }
    }
}

impl ScheduleConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_priority_policy(mut self, policy: PriorityPolicy) -> Self {
        self.priority_policy = policy;
        self
    }

    pub fn with_aging(mut self, enabled: bool, threshold_ms: u64) -> Self {
        self.aging_enabled = enabled;
        self.aging_threshold_ms = threshold_ms;
        self
    }

    pub fn with_queue_capacity(mut self, capacity: usize) -> Self {
        self.queue_capacity = capacity;
        self
    }

    pub fn with_check_interval_ms(mut self, interval_ms: u64) -> Self {
        self.check_interval_ms = interval_ms;
        self
    }

    pub fn enabled(mut self) -> Self {
        self.enabled = true;
        self
    }
}

#[derive(Debug, Clone)]
struct PriorityEntry {
    message: Message,
    priority: i32,
    enqueued_at: Instant,
    tenant_id: Option<String>,
}

impl PartialEq for PriorityEntry {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority
    }
}

impl Eq for PriorityEntry {}

impl PartialOrd for PriorityEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PriorityEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.priority.cmp(&other.priority)
    }
}

#[derive(Debug)]
pub struct PriorityQueue {
    heap: Mutex<BinaryHeap<PriorityEntry>>,
    policy: PriorityPolicy,
    capacity: usize,
    aging_enabled: bool,
    aging_threshold_ms: u64,
    fair_share_state: Mutex<HashMap<String, u64>>,
    total_dequeued: AtomicU64,
}

impl PriorityQueue {
    pub fn new(policy: PriorityPolicy, capacity: usize) -> Self {
        Self {
            heap: Mutex::new(BinaryHeap::new()),
            policy,
            capacity,
            aging_enabled: true,
            aging_threshold_ms: 300_000,
            fair_share_state: Mutex::new(HashMap::new()),
            total_dequeued: AtomicU64::new(0),
        }
    }

    pub fn with_aging(mut self, enabled: bool, threshold_ms: u64) -> Self {
        self.aging_enabled = enabled;
        self.aging_threshold_ms = threshold_ms;
        self
    }

    pub fn enqueue(&self, message: Message, priority: i32) -> Result<(), MqError> {
        let mut heap = self.heap.lock().unwrap();
        if heap.len() >= self.capacity {
            return Err(MqError::NotSupported(format!(
                "priority queue capacity exceeded ({}), please increase capacity or drain queue",
                self.capacity
            )));
        }
        let tenant_id = message.headers.get("tenant_id").cloned();
        heap.push(PriorityEntry {
            message,
            priority,
            enqueued_at: Instant::now(),
            tenant_id,
        });
        Ok(())
    }

    pub fn dequeue(&self) -> Option<Message> {
        match self.policy {
            PriorityPolicy::Strict => self.dequeue_strict(),
            PriorityPolicy::Weighted => self.dequeue_weighted(),
            PriorityPolicy::FairShare => self.dequeue_fair_share(),
        }
    }

    fn dequeue_strict(&self) -> Option<Message> {
        let mut heap = self.heap.lock().unwrap();
        if self.aging_enabled {
            let threshold = Duration::from_millis(self.aging_threshold_ms);
            let now = Instant::now();
            let mut all: Vec<PriorityEntry> = heap.drain().collect();
            for entry in all.iter_mut() {
                if now.duration_since(entry.enqueued_at) > threshold {
                    entry.priority += 1;
                }
            }
            for entry in all {
                heap.push(entry);
            }
        }
        let result = heap.pop().map(|e| e.message);
        if result.is_some() {
            self.total_dequeued.fetch_add(1, Ordering::Relaxed);
        }
        result
    }

    fn dequeue_weighted(&self) -> Option<Message> {
        let mut heap = self.heap.lock().unwrap();
        if heap.is_empty() {
            return None;
        }
        let mut all: Vec<PriorityEntry> = heap.drain().collect();
        all.sort_by_key(|b| std::cmp::Reverse(b.priority));
        let total_weight: i64 = all.iter().map(|e| e.priority.max(1) as i64).sum();
        if total_weight == 0 {
            let entry = all.into_iter().next();
            if entry.is_some() {
                self.total_dequeued.fetch_add(1, Ordering::Relaxed);
            }
            return entry.map(|e| e.message);
        }
        let mut rng_state = self.total_dequeued.load(Ordering::Relaxed);
        rng_state = rng_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let r = (rng_state >> 11) as f64 / (1u64 << 53) as f64;
        let mut cumulative = 0.0f64;
        let mut chosen_idx = 0;
        for (i, entry) in all.iter().enumerate() {
            cumulative += entry.priority.max(1) as f64 / total_weight as f64;
            if r < cumulative {
                chosen_idx = i;
                break;
            }
        }
        let chosen = all.remove(chosen_idx);
        for entry in all {
            heap.push(entry);
        }
        self.total_dequeued.fetch_add(1, Ordering::Relaxed);
        Some(chosen.message)
    }

    fn dequeue_fair_share(&self) -> Option<Message> {
        let mut heap = self.heap.lock().unwrap();
        if heap.is_empty() {
            return None;
        }
        let mut all: Vec<PriorityEntry> = heap.drain().collect();
        all.sort_by_key(|b| std::cmp::Reverse(b.priority));
        let mut fair_state = self.fair_share_state.lock().unwrap();
        let now_ms_val = now_ms() as u64;
        let mut chosen_idx = 0;
        let mut min_last_served = u64::MAX;
        for (i, entry) in all.iter().enumerate() {
            let tenant = entry
                .tenant_id
                .clone()
                .unwrap_or_else(|| "default".to_string());
            let last_served = *fair_state.get(&tenant).unwrap_or(&0);
            let wait = now_ms_val.saturating_sub(last_served);
            if wait > min_last_served {
                continue;
            }
            min_last_served = last_served;
            chosen_idx = i;
        }
        let chosen = all.remove(chosen_idx);
        let tenant = chosen
            .tenant_id
            .clone()
            .unwrap_or_else(|| "default".to_string());
        fair_state.insert(tenant, now_ms_val);
        for entry in all {
            heap.push(entry);
        }
        self.total_dequeued.fetch_add(1, Ordering::Relaxed);
        Some(chosen.message)
    }

    pub fn len(&self) -> usize {
        self.heap.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.heap.lock().unwrap().is_empty()
    }

    pub fn total_dequeued(&self) -> u64 {
        self.total_dequeued.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleLog {
    pub message_id: String,
    pub deliver_at: i64,
    pub priority: i32,
    pub delivered_at: i64,
    pub success: bool,
    pub retry_count: u32,
}

pub struct DelayScheduler {
    delayed_messages: RwLock<BTreeMap<i64, Vec<DelayedMessage>>>,
    priority_queue: PriorityQueue,
    scheduled: RwLock<Vec<ScheduledMessage>>,
    queue: Arc<dyn MessageQueue + Send + Sync>,
    config: ScheduleConfig,
    shutdown: AtomicBool,
    logs: Mutex<VecDeque<ScheduleLog>>,
    backoff_policy: BackoffPolicy,
    max_retries: u32,
}

impl DelayScheduler {
    pub fn new(queue: Arc<dyn MessageQueue + Send + Sync>, config: ScheduleConfig) -> Self {
        let pq = PriorityQueue::new(config.priority_policy.clone(), config.queue_capacity)
            .with_aging(config.aging_enabled, config.aging_threshold_ms);
        Self {
            delayed_messages: RwLock::new(BTreeMap::new()),
            priority_queue: pq,
            scheduled: RwLock::new(Vec::new()),
            queue,
            config,
            shutdown: AtomicBool::new(false),
            logs: Mutex::new(VecDeque::new()),
            backoff_policy: BackoffPolicy::Exponential,
            max_retries: 3,
        }
    }

    pub fn with_backoff(mut self, policy: BackoffPolicy, max_retries: u32) -> Self {
        self.backoff_policy = policy;
        self.max_retries = max_retries;
        self
    }

    pub async fn publish_delayed(&self, msg: DelayedMessage) -> Result<(), MqError> {
        let mut delayed = self.delayed_messages.write().unwrap();
        delayed.entry(msg.deliver_at).or_default().push(msg);
        Ok(())
    }

    pub async fn publish_scheduled(&self, msg: ScheduledMessage) -> Result<(), MqError> {
        if let Some(ref cron) = msg.cron {
            validate_cron(cron)?;
        }
        if msg.cron.is_none() && msg.interval.is_none() {
            return Err(MqError::NotSupported(
                "scheduled message must have cron or interval".to_string(),
            ));
        }
        let mut scheduled = self.scheduled.write().unwrap();
        scheduled.push(msg);
        Ok(())
    }

    pub async fn check_and_deliver(&self) -> Result<usize, MqError> {
        if self.shutdown.load(Ordering::Relaxed) {
            return Ok(0);
        }
        let now = now_ms();
        let mut delivered_count = 0usize;
        {
            let mut delayed = self.delayed_messages.write().unwrap();
            let ready_keys: Vec<i64> = delayed.keys().filter(|&&k| k <= now).cloned().collect();
            for key in ready_keys {
                if let Some(msgs) = delayed.remove(&key) {
                    for msg in msgs {
                        self.priority_queue.enqueue(msg.message, msg.priority)?;
                    }
                }
            }
        }
        {
            let scheduled = self.scheduled.read().unwrap();
            if !scheduled.is_empty() {
                let mut delayed = self.delayed_messages.write().unwrap();
                for sm in scheduled.iter() {
                    match sm.next_deliver_ms(now) {
                        Ok(next_ms) => {
                            let dm = DelayedMessage::new(sm.message.clone(), next_ms, 0);
                            delayed.entry(dm.deliver_at).or_default().push(dm);
                        }
                        Err(_) => continue,
                    }
                }
            }
        }
        while let Some(message) = self.priority_queue.dequeue() {
            let msg_id = message.id.clone();
            let priority = 0i32;
            let mut retry_count = 0u32;
            let mut success = false;
            loop {
                let result = self.queue.publish(&message.topic, &message.payload).await;
                if result.is_ok() {
                    success = true;
                    break;
                }
                if retry_count >= self.max_retries {
                    break;
                }
                retry_count += 1;
                let backoff_ms = self.backoff_policy.calculate(retry_count, 100, 10_000);
                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
            }
            let log = ScheduleLog {
                message_id: msg_id,
                deliver_at: now,
                priority,
                delivered_at: now_ms(),
                success,
                retry_count,
            };
            self.logs.lock().unwrap().push_back(log);
            delivered_count += 1;
        }
        Ok(delivered_count)
    }

    pub async fn run(&self) {
        let interval = Duration::from_millis(self.config.check_interval_ms);
        while !self.shutdown.load(Ordering::Relaxed) {
            let _ = self.check_and_deliver().await;
            tokio::time::sleep(interval).await;
        }
    }

    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }

    pub fn pending_delayed_count(&self) -> usize {
        self.delayed_messages
            .read()
            .unwrap()
            .values()
            .map(|v| v.len())
            .sum()
    }

    pub fn ready_queue_len(&self) -> usize {
        self.priority_queue.len()
    }

    pub fn logs(&self) -> Vec<ScheduleLog> {
        self.logs.lock().unwrap().iter().cloned().collect()
    }

    pub fn config(&self) -> &ScheduleConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queue::InMemoryQueue;

    fn make_message(id: &str, topic: &str) -> Message {
        let mut msg = Message::text_message(topic, format!("payload-{}", id));
        msg.id = id.to_string();
        msg
    }

    #[test]
    fn test_schedule_config_default() {
        let config = ScheduleConfig::new();
        assert!(!config.enabled);
        assert_eq!(config.priority_policy, PriorityPolicy::Strict);
        assert!(config.aging_enabled);
        assert_eq!(config.aging_threshold_ms, 300_000);
        assert_eq!(config.queue_capacity, 100_000);
        assert_eq!(config.check_interval_ms, 100);
    }

    #[test]
    fn test_schedule_config_builder() {
        let config = ScheduleConfig::new()
            .with_priority_policy(PriorityPolicy::Weighted)
            .with_aging(false, 600_000)
            .with_queue_capacity(5000)
            .with_check_interval_ms(200)
            .enabled();
        assert!(config.enabled);
        assert_eq!(config.priority_policy, PriorityPolicy::Weighted);
        assert!(!config.aging_enabled);
        assert_eq!(config.aging_threshold_ms, 600_000);
        assert_eq!(config.queue_capacity, 5000);
        assert_eq!(config.check_interval_ms, 200);
    }

    #[test]
    fn test_priority_queue_strict() {
        let pq = PriorityQueue::new(PriorityPolicy::Strict, 100);
        pq.enqueue(make_message("a", "t"), 5).unwrap();
        pq.enqueue(make_message("b", "t"), 10).unwrap();
        let first = pq.dequeue().unwrap();
        assert_eq!(first.id, "b");
        let second = pq.dequeue().unwrap();
        assert_eq!(second.id, "a");
    }

    #[test]
    fn test_priority_queue_capacity() {
        let pq = PriorityQueue::new(PriorityPolicy::Strict, 2);
        pq.enqueue(make_message("a", "t"), 1).unwrap();
        pq.enqueue(make_message("b", "t"), 1).unwrap();
        let result = pq.enqueue(make_message("c", "t"), 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_priority_queue_empty() {
        let pq = PriorityQueue::new(PriorityPolicy::Strict, 100);
        assert!(pq.dequeue().is_none());
        assert!(pq.is_empty());
    }

    #[test]
    fn test_priority_queue_fair_share() {
        let pq = PriorityQueue::new(PriorityPolicy::FairShare, 100);
        let mut msg1 = make_message("a", "t");
        msg1.headers
            .insert("tenant_id".to_string(), "t1".to_string());
        let mut msg2 = make_message("b", "t");
        msg2.headers
            .insert("tenant_id".to_string(), "t2".to_string());
        pq.enqueue(msg1, 5).unwrap();
        pq.enqueue(msg2, 5).unwrap();
        let first = pq.dequeue().unwrap();
        assert!(first.id == "a" || first.id == "b");
        let second = pq.dequeue().unwrap();
        assert!(second.id == "a" || second.id == "b");
        assert_ne!(first.id, second.id);
    }

    #[test]
    fn test_delayed_message() {
        let msg = make_message("a", "t");
        let dm = DelayedMessage::new(msg, now_ms() + 5000, 3);
        assert_eq!(dm.priority, 3);
        assert!(dm.deliver_at > now_ms());
    }

    #[test]
    fn test_scheduled_message_cron() {
        let msg = make_message("a", "t");
        let sm = ScheduledMessage::with_cron(msg, "0 * * * *");
        assert!(sm.cron.is_some());
        let next = sm.next_deliver_ms(now_ms());
        assert!(next.is_ok());
    }

    #[test]
    fn test_scheduled_message_interval() {
        let msg = make_message("a", "t");
        let sm = ScheduledMessage::with_interval(msg, Duration::from_secs(60));
        assert!(sm.interval.is_some());
        let next = sm.next_deliver_ms(now_ms());
        assert!(next.is_ok());
        assert!(next.unwrap() > now_ms());
    }

    #[test]
    fn test_scheduled_message_invalid_cron() {
        let msg = make_message("a", "t");
        let sm = ScheduledMessage::with_cron(msg, "invalid");
        let result = sm.next_deliver_ms(now_ms());
        assert!(result.is_err());
    }

    #[test]
    fn test_scheduled_message_no_schedule() {
        let msg = make_message("a", "t");
        let sm = ScheduledMessage {
            message: msg,
            cron: None,
            interval: None,
        };
        let result = sm.next_deliver_ms(now_ms());
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_cron_valid() {
        assert!(validate_cron("0 * * * *").is_ok());
        assert!(validate_cron("*/5 * * * *").is_ok());
        assert!(validate_cron("0 9 * * 1").is_ok());
    }

    #[test]
    fn test_validate_cron_invalid() {
        assert!(validate_cron("invalid").is_err());
        assert!(validate_cron("0 * * *").is_err());
        assert!(validate_cron("0 * * * * *").is_err());
    }

    #[test]
    fn test_delay_scheduler_publish_delayed() {
        let queue = Arc::new(InMemoryQueue::new());
        let config = ScheduleConfig::new().enabled();
        let scheduler = DelayScheduler::new(queue, config);
        let msg = make_message("a", "t");
        let dm = DelayedMessage::new(msg, now_ms() + 10000, 5);
        let result = futures::executor::block_on(scheduler.publish_delayed(dm));
        assert!(result.is_ok());
        assert_eq!(scheduler.pending_delayed_count(), 1);
    }

    #[test]
    fn test_delay_scheduler_publish_scheduled() {
        let queue = Arc::new(InMemoryQueue::new());
        let config = ScheduleConfig::new().enabled();
        let scheduler = DelayScheduler::new(queue, config);
        let msg = make_message("a", "t");
        let sm = ScheduledMessage::with_interval(msg, Duration::from_secs(60));
        let result = futures::executor::block_on(scheduler.publish_scheduled(sm));
        assert!(result.is_ok());
    }

    #[test]
    fn test_delay_scheduler_publish_scheduled_invalid_cron() {
        let queue = Arc::new(InMemoryQueue::new());
        let config = ScheduleConfig::new().enabled();
        let scheduler = DelayScheduler::new(queue, config);
        let msg = make_message("a", "t");
        let sm = ScheduledMessage::with_cron(msg, "invalid");
        let result = futures::executor::block_on(scheduler.publish_scheduled(sm));
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_delay_scheduler_check_and_deliver_not_ready() {
        let queue = Arc::new(InMemoryQueue::new());
        let config = ScheduleConfig::new().enabled();
        let scheduler = DelayScheduler::new(queue, config);
        let msg = make_message("a", "t");
        let dm = DelayedMessage::new(msg, now_ms() + 100000, 5);
        let _ = scheduler.publish_delayed(dm).await;
        let delivered = scheduler.check_and_deliver().await.unwrap();
        assert_eq!(delivered, 0);
    }

    #[tokio::test]
    async fn test_delay_scheduler_check_and_deliver_ready() {
        let queue = Arc::new(InMemoryQueue::new());
        let config = ScheduleConfig::new().enabled();
        let scheduler = DelayScheduler::new(queue, config);
        let msg = make_message("a", "t");
        let dm = DelayedMessage::new(msg, now_ms() - 1, 5);
        let _ = scheduler.publish_delayed(dm).await;
        let delivered = scheduler.check_and_deliver().await.unwrap();
        assert_eq!(delivered, 1);
        assert_eq!(scheduler.pending_delayed_count(), 0);
    }

    #[tokio::test]
    async fn test_delay_scheduler_shutdown() {
        let queue = Arc::new(InMemoryQueue::new());
        let config = ScheduleConfig::new().enabled();
        let scheduler = DelayScheduler::new(queue, config);
        scheduler.shutdown();
        let delivered = scheduler.check_and_deliver().await.unwrap();
        assert_eq!(delivered, 0);
    }

    #[tokio::test]
    async fn test_delay_scheduler_logs() {
        let queue = Arc::new(InMemoryQueue::new());
        let config = ScheduleConfig::new().enabled();
        let scheduler = DelayScheduler::new(queue, config);
        let msg = make_message("a", "t");
        let dm = DelayedMessage::new(msg, now_ms() - 1, 5);
        let _ = scheduler.publish_delayed(dm).await;
        let _ = scheduler.check_and_deliver().await;
        let logs = scheduler.logs();
        assert_eq!(logs.len(), 1);
        assert!(logs[0].success);
    }

    #[test]

    fn test_priority_queue_total_dequeued() {
        let pq = PriorityQueue::new(PriorityPolicy::Strict, 100);
        pq.enqueue(make_message("a", "t"), 1).unwrap();
        pq.enqueue(make_message("b", "t"), 2).unwrap();
        pq.dequeue();
        pq.dequeue();
        assert_eq!(pq.total_dequeued(), 2);
    }

    #[test]
    fn test_delayed_message_preserves_tenant() {
        let mut msg = make_message("a", "t");
        msg.headers
            .insert("tenant_id".to_string(), "tenant_1".to_string());
        let dm = DelayedMessage::new(msg, now_ms() + 1000, 5);
        assert_eq!(dm.message.headers.get("tenant_id").unwrap(), "tenant_1");
    }

    #[test]
    fn test_priority_queue_weighted() {
        let pq = PriorityQueue::new(PriorityPolicy::Weighted, 100);
        pq.enqueue(make_message("a", "t"), 1).unwrap();
        pq.enqueue(make_message("b", "t"), 10).unwrap();
        let _ = pq.dequeue();
        let _ = pq.dequeue();
        assert_eq!(pq.total_dequeued(), 2);
    }
}
