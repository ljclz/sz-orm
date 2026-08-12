//! # 死信队列自动重投递（DLX Auto-Redelivery）
//!
//! 提供 `RedeliveryScheduler` 自动重投递调度器，死信消息按 `BackoffPolicy`
//! 退避策略自动调度重投递，支持四种路由策略。
//!
//! ## 特性
//!
//! - 四种退避策略：Fixed / Exponential / Linear / RandomJitter
//! - 四种路由策略：RequeueToOriginal / ForwardToDlxTopic / ForwardToDlxQueue / Drop
//! - 重投递次数上限保护
//! - 复用既有 `InMemoryQueue.dead_letters` / `requeue_dead_letter`

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::queue::{InMemoryQueue, Message, MessageQueue};
use crate::MqError;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn random_jitter_factor() -> f64 {
    use std::cell::Cell;
    thread_local! {
        static STATE: Cell<u64> = const { Cell::new(12345) };
    }
    STATE.with(|s| {
        let mut seed = s.get();
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        s.set(seed);
        let normalized = (seed >> 11) as f64 / (1u64 << 53) as f64;
        0.5 + normalized
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackoffPolicy {
    Fixed,
    Exponential,
    Linear,
    RandomJitter,
}

impl BackoffPolicy {
    pub fn calculate(&self, retry_count: u32, initial_ms: u64, max_ms: u64) -> u64 {
        match self {
            BackoffPolicy::Fixed => initial_ms,
            BackoffPolicy::Exponential => {
                if retry_count == 0 {
                    return initial_ms;
                }
                if retry_count >= 64 {
                    return max_ms;
                }
                let shift = retry_count.min(63);
                let result = initial_ms.checked_shl(shift).unwrap_or(max_ms);
                result.min(max_ms)
            }
            BackoffPolicy::Linear => {
                let count = retry_count.max(1) as u64;
                initial_ms.saturating_mul(count).min(max_ms)
            }
            BackoffPolicy::RandomJitter => {
                let factor = random_jitter_factor();
                let base = (initial_ms as f64 * factor) as u64;
                base.min(max_ms)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DlxRoutingStrategy {
    RequeueToOriginal,
    ForwardToDlxTopic,
    ForwardToDlxQueue,
    Drop,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DlxConfig {
    pub enabled: bool,
    pub backoff_policy: BackoffPolicy,
    pub initial_backoff_ms: u64,
    pub max_backoff_ms: u64,
    pub max_redelivery_count: u32,
    pub routing_strategy: DlxRoutingStrategy,
    pub dlx_topic: Option<String>,
    pub dlx_queue: Option<String>,
}

impl Default for DlxConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl DlxConfig {
    pub fn new() -> Self {
        Self {
            enabled: false,
            backoff_policy: BackoffPolicy::Exponential,
            initial_backoff_ms: 1000,
            max_backoff_ms: 60000,
            max_redelivery_count: 10,
            routing_strategy: DlxRoutingStrategy::RequeueToOriginal,
            dlx_topic: None,
            dlx_queue: None,
        }
    }

    pub fn with_backoff_policy(mut self, policy: BackoffPolicy) -> Self {
        self.backoff_policy = policy;
        self
    }

    pub fn with_initial_backoff_ms(mut self, ms: u64) -> Self {
        self.initial_backoff_ms = ms;
        self
    }

    pub fn with_max_backoff_ms(mut self, ms: u64) -> Self {
        self.max_backoff_ms = ms;
        self
    }

    pub fn with_max_redelivery_count(mut self, count: u32) -> Self {
        self.max_redelivery_count = count;
        self
    }

    pub fn with_routing_strategy(mut self, strategy: DlxRoutingStrategy) -> Self {
        self.routing_strategy = strategy;
        self
    }

    pub fn with_dlx_topic(mut self, topic: impl Into<String>) -> Self {
        self.dlx_topic = Some(topic.into());
        self
    }

    pub fn with_dlx_queue(mut self, queue: impl Into<String>) -> Self {
        self.dlx_queue = Some(queue.into());
        self
    }

    pub fn enabled(mut self) -> Self {
        self.enabled = true;
        self
    }
}

#[derive(Debug, Clone)]
pub struct DlxEntry {
    pub message: Message,
    pub redelivery_count: u32,
    pub last_redelivery_at: u64,
    pub next_redelivery_at: u64,
}

impl DlxEntry {
    pub fn new(message: Message) -> Self {
        let now = now_ms();
        Self {
            message,
            redelivery_count: 0,
            last_redelivery_at: 0,
            next_redelivery_at: now,
        }
    }

    pub fn should_redeliver(&self, max_count: u32) -> bool {
        self.redelivery_count < max_count
    }

    fn record_redelivery(&mut self) {
        self.redelivery_count = self.redelivery_count.saturating_add(1);
        self.last_redelivery_at = now_ms();
    }

    fn schedule_next(&mut self, backoff_ms: u64) {
        self.next_redelivery_at = now_ms().saturating_add(backoff_ms);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RedeliveryOutcome {
    Requeued,
    ForwardedToDlxTopic,
    ForwardedToDlxQueue,
    Dropped,
    LimitReached,
    Skipped(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedeliveryLog {
    pub message_id: String,
    pub redelivery_count: u32,
    pub backoff_ms: u64,
    pub routing_strategy: DlxRoutingStrategy,
    pub outcome: RedeliveryOutcome,
    pub timestamp: u64,
}

pub struct RedeliveryScheduler {
    queue: Arc<InMemoryQueue>,
    config: DlxConfig,
    running: Arc<AtomicBool>,
    logs: Arc<Mutex<VecDeque<RedeliveryLog>>>,
    check_interval_ms: AtomicU64,
}

impl RedeliveryScheduler {
    pub fn new(queue: Arc<InMemoryQueue>, config: DlxConfig) -> Self {
        Self {
            queue,
            config,
            running: Arc::new(AtomicBool::new(false)),
            logs: Arc::new(Mutex::new(VecDeque::new())),
            check_interval_ms: AtomicU64::new(100),
        }
    }

    pub fn with_check_interval(self, interval_ms: u64) -> Self {
        self.check_interval_ms.store(interval_ms, Ordering::Relaxed);
        self
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    pub async fn start(&self) -> Result<(), MqError> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        Ok(())
    }

    pub async fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    fn calculate_backoff(&self, retry_count: u32) -> u64 {
        self.config.backoff_policy.calculate(
            retry_count,
            self.config.initial_backoff_ms,
            self.config.max_backoff_ms,
        )
    }

    pub async fn collect_dead_letters(&self) -> Vec<DlxEntry> {
        let messages = self.queue.collect_all_dead_letters().await;
        messages.into_iter().map(DlxEntry::new).collect()
    }

    pub async fn schedule_redelivery(
        &self,
        entry: &mut DlxEntry,
    ) -> Result<RedeliveryOutcome, MqError> {
        if !entry.should_redeliver(self.config.max_redelivery_count) {
            return Ok(RedeliveryOutcome::LimitReached);
        }

        let backoff = self.calculate_backoff(entry.redelivery_count);

        tokio::time::sleep(tokio::time::Duration::from_millis(backoff)).await;

        let outcome = self.execute_routing(&entry.message).await?;

        entry.record_redelivery();
        let next_backoff = self.calculate_backoff(entry.redelivery_count);
        entry.schedule_next(next_backoff);

        self.log_redelivery(&entry.message, entry.redelivery_count, backoff, &outcome);

        Ok(outcome)
    }

    async fn execute_routing(&self, message: &Message) -> Result<RedeliveryOutcome, MqError> {
        match self.config.routing_strategy {
            DlxRoutingStrategy::RequeueToOriginal => {
                self.queue.requeue_dead_letter(&message.id).await?;
                Ok(RedeliveryOutcome::Requeued)
            }
            DlxRoutingStrategy::ForwardToDlxTopic => {
                let topic = self.config.dlx_topic.as_ref().ok_or_else(|| {
                    MqError::NotSupported("dlx_topic required but not configured".to_string())
                })?;
                self.queue.publish(topic, &message.payload).await?;
                self.remove_from_dead_letters(&message.id).await;
                Ok(RedeliveryOutcome::ForwardedToDlxTopic)
            }
            DlxRoutingStrategy::ForwardToDlxQueue => {
                let queue_name = self.config.dlx_queue.as_ref().ok_or_else(|| {
                    MqError::NotSupported("dlx_queue required but not configured".to_string())
                })?;
                self.queue.publish(queue_name, &message.payload).await?;
                self.remove_from_dead_letters(&message.id).await;
                Ok(RedeliveryOutcome::ForwardedToDlxQueue)
            }
            DlxRoutingStrategy::Drop => {
                self.remove_from_dead_letters(&message.id).await;
                Ok(RedeliveryOutcome::Dropped)
            }
        }
    }

    async fn remove_from_dead_letters(&self, message_id: &str) {
        self.queue.remove_dead_letter(message_id).await;
    }

    fn log_redelivery(
        &self,
        message: &Message,
        count: u32,
        backoff_ms: u64,
        outcome: &RedeliveryOutcome,
    ) {
        let log = RedeliveryLog {
            message_id: message.id.clone(),
            redelivery_count: count,
            backoff_ms,
            routing_strategy: self.config.routing_strategy.clone(),
            outcome: outcome.clone(),
            timestamp: now_ms(),
        };
        let mut logs = self.logs.lock().unwrap();
        logs.push_back(log);
        if logs.len() > 10000 {
            logs.pop_front();
        }
    }

    pub fn get_logs(&self) -> Vec<RedeliveryLog> {
        let logs = self.logs.lock().unwrap();
        logs.iter().cloned().collect()
    }

    pub async fn run_once(&self) -> Result<usize, MqError> {
        if !self.running.load(Ordering::Relaxed) {
            return Ok(0);
        }

        let mut entries = self.collect_dead_letters().await;
        if entries.is_empty() {
            return Ok(0);
        }

        let mut processed = 0usize;
        for entry in entries.iter_mut() {
            if !self.running.load(Ordering::Relaxed) {
                break;
            }
            let result = self.schedule_redelivery(entry).await;
            if result.is_ok() {
                processed += 1;
            }
        }
        Ok(processed)
    }

    pub async fn run_loop(&self) -> Result<(), MqError> {
        let interval = self.check_interval_ms.load(Ordering::Relaxed);
        while self.running.load(Ordering::Relaxed) {
            let _ = self.run_once().await;
            tokio::time::sleep(tokio::time::Duration::from_millis(interval)).await;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dlx_config_default() {
        let config = DlxConfig::new();
        assert!(!config.enabled);
        assert_eq!(config.backoff_policy, BackoffPolicy::Exponential);
        assert_eq!(config.initial_backoff_ms, 1000);
        assert_eq!(config.max_backoff_ms, 60000);
        assert_eq!(config.max_redelivery_count, 10);
        assert_eq!(
            config.routing_strategy,
            DlxRoutingStrategy::RequeueToOriginal
        );
        assert!(config.dlx_topic.is_none());
        assert!(config.dlx_queue.is_none());
    }

    #[test]
    fn test_dlx_config_builder() {
        let config = DlxConfig::new()
            .enabled()
            .with_backoff_policy(BackoffPolicy::Fixed)
            .with_initial_backoff_ms(500)
            .with_max_backoff_ms(30000)
            .with_max_redelivery_count(5)
            .with_routing_strategy(DlxRoutingStrategy::ForwardToDlxTopic)
            .with_dlx_topic("orders.dlx")
            .with_dlx_queue("orders.dlx.queue");

        assert!(config.enabled);
        assert_eq!(config.backoff_policy, BackoffPolicy::Fixed);
        assert_eq!(config.initial_backoff_ms, 500);
        assert_eq!(config.max_backoff_ms, 30000);
        assert_eq!(config.max_redelivery_count, 5);
        assert_eq!(
            config.routing_strategy,
            DlxRoutingStrategy::ForwardToDlxTopic
        );
        assert_eq!(config.dlx_topic.as_deref(), Some("orders.dlx"));
        assert_eq!(config.dlx_queue.as_deref(), Some("orders.dlx.queue"));
    }

    #[test]
    fn test_backoff_fixed() {
        let policy = BackoffPolicy::Fixed;
        assert_eq!(policy.calculate(0, 1000, 60000), 1000);
        assert_eq!(policy.calculate(5, 1000, 60000), 1000);
        assert_eq!(policy.calculate(100, 1000, 60000), 1000);
    }

    #[test]
    fn test_backoff_exponential() {
        let policy = BackoffPolicy::Exponential;
        assert_eq!(policy.calculate(0, 1000, 60000), 1000);
        assert_eq!(policy.calculate(1, 1000, 60000), 2000);
        assert_eq!(policy.calculate(2, 1000, 60000), 4000);
        assert_eq!(policy.calculate(3, 1000, 60000), 8000);
    }

    #[test]
    fn test_backoff_exponential_capped() {
        let policy = BackoffPolicy::Exponential;
        assert_eq!(policy.calculate(20, 1000, 60000), 60000);
        assert_eq!(policy.calculate(100, 1000, 60000), 60000);
    }

    #[test]
    fn test_backoff_linear() {
        let policy = BackoffPolicy::Linear;
        assert_eq!(policy.calculate(1, 1000, 60000), 1000);
        assert_eq!(policy.calculate(2, 1000, 60000), 2000);
        assert_eq!(policy.calculate(5, 1000, 60000), 5000);
        assert_eq!(policy.calculate(100, 1000, 60000), 60000);
    }

    #[test]
    fn test_backoff_random_jitter() {
        let policy = BackoffPolicy::RandomJitter;
        let v = policy.calculate(1, 1000, 60000);
        assert!(
            v >= 500 && v <= 1500,
            "jitter should be in [500, 1500], got {v}"
        );
    }

    #[test]
    fn test_dlx_entry_new() {
        let msg = Message::new("test", vec![1, 2, 3]);
        let entry = DlxEntry::new(msg);
        assert_eq!(entry.redelivery_count, 0);
        assert_eq!(entry.last_redelivery_at, 0);
        assert!(entry.next_redelivery_at > 0);
    }

    #[test]
    fn test_dlx_entry_should_redeliver() {
        let msg = Message::new("test", vec![]);
        let mut entry = DlxEntry::new(msg);
        assert!(entry.should_redeliver(10));
        assert!(entry.should_redeliver(1));

        entry.redelivery_count = 10;
        assert!(!entry.should_redeliver(10));
        assert!(entry.should_redeliver(11));
    }

    #[test]
    fn test_redelivery_outcome_serde() {
        let outcomes = vec![
            RedeliveryOutcome::Requeued,
            RedeliveryOutcome::ForwardedToDlxTopic,
            RedeliveryOutcome::ForwardedToDlxQueue,
            RedeliveryOutcome::Dropped,
            RedeliveryOutcome::LimitReached,
            RedeliveryOutcome::Skipped("message not found".to_string()),
        ];
        for o in &outcomes {
            let json = serde_json::to_string(o).unwrap();
            let decoded: RedeliveryOutcome = serde_json::from_str(&json).unwrap();
            assert_eq!(*o, decoded);
        }
    }

    #[tokio::test]
    async fn test_scheduler_start_stop() {
        let queue = Arc::new(InMemoryQueue::new());
        let config = DlxConfig::new().enabled();
        let scheduler = RedeliveryScheduler::new(queue, config);

        assert!(!scheduler.is_running());
        scheduler.start().await.unwrap();
        assert!(scheduler.is_running());
        scheduler.stop().await;
        assert!(!scheduler.is_running());
    }

    #[tokio::test]
    async fn test_scheduler_start_idempotent() {
        let queue = Arc::new(InMemoryQueue::new());
        let config = DlxConfig::new().enabled();
        let scheduler = RedeliveryScheduler::new(queue, config);

        scheduler.start().await.unwrap();
        scheduler.start().await.unwrap();
        assert!(scheduler.is_running());
        scheduler.stop().await;
    }

    #[tokio::test]
    async fn test_scheduler_collect_dead_letters_empty() {
        let queue = Arc::new(InMemoryQueue::new());
        let config = DlxConfig::new().enabled();
        let scheduler = RedeliveryScheduler::new(queue, config);

        let entries = scheduler.collect_dead_letters().await;
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn test_scheduler_collect_dead_letters_with_messages() {
        let queue = Arc::new(InMemoryQueue::with_max_retries(1));
        queue.publish("orders", b"order-1").await.unwrap();
        let msg = queue.consume("orders").await.unwrap().unwrap();
        queue.nack(&msg.id).await.unwrap();

        let config = DlxConfig::new().enabled();
        let scheduler = RedeliveryScheduler::new(queue, config);

        let entries = scheduler.collect_dead_letters().await;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].message.payload, b"order-1");
    }

    #[tokio::test]
    async fn test_redelivery_requeue_to_original() {
        let queue = Arc::new(InMemoryQueue::with_max_retries(1));
        queue.publish("orders", b"order-1").await.unwrap();
        let msg = queue.consume("orders").await.unwrap().unwrap();
        queue.nack(&msg.id).await.unwrap();
        assert_eq!(queue.dead_letter_count("orders").await, 1);

        let config = DlxConfig::new()
            .enabled()
            .with_routing_strategy(DlxRoutingStrategy::RequeueToOriginal)
            .with_initial_backoff_ms(10)
            .with_max_backoff_ms(100);
        let scheduler = RedeliveryScheduler::new(queue.clone(), config);
        scheduler.start().await.unwrap();

        let mut entries = scheduler.collect_dead_letters().await;
        assert_eq!(entries.len(), 1);
        let outcome = scheduler
            .schedule_redelivery(&mut entries[0])
            .await
            .unwrap();
        assert_eq!(outcome, RedeliveryOutcome::Requeued);

        assert_eq!(queue.dead_letter_count("orders").await, 0);
        assert_eq!(queue.message_count("orders").await, 1);
        scheduler.stop().await;
    }

    #[tokio::test]
    async fn test_redelivery_forward_to_dlx_topic() {
        let queue = Arc::new(InMemoryQueue::with_max_retries(1));
        queue.publish("orders", b"order-1").await.unwrap();
        let msg = queue.consume("orders").await.unwrap().unwrap();
        queue.nack(&msg.id).await.unwrap();

        let config = DlxConfig::new()
            .enabled()
            .with_routing_strategy(DlxRoutingStrategy::ForwardToDlxTopic)
            .with_dlx_topic("orders.dlx")
            .with_initial_backoff_ms(10)
            .with_max_backoff_ms(100);
        let scheduler = RedeliveryScheduler::new(queue.clone(), config);
        scheduler.start().await.unwrap();

        let mut entries = scheduler.collect_dead_letters().await;
        let outcome = scheduler
            .schedule_redelivery(&mut entries[0])
            .await
            .unwrap();
        assert_eq!(outcome, RedeliveryOutcome::ForwardedToDlxTopic);
        assert_eq!(queue.message_count("orders.dlx").await, 1);
        scheduler.stop().await;
    }

    #[tokio::test]
    async fn test_redelivery_forward_to_dlx_queue() {
        let queue = Arc::new(InMemoryQueue::with_max_retries(1));
        queue.publish("orders", b"order-1").await.unwrap();
        let msg = queue.consume("orders").await.unwrap().unwrap();
        queue.nack(&msg.id).await.unwrap();

        let config = DlxConfig::new()
            .enabled()
            .with_routing_strategy(DlxRoutingStrategy::ForwardToDlxQueue)
            .with_dlx_queue("orders.dlx.queue")
            .with_initial_backoff_ms(10)
            .with_max_backoff_ms(100);
        let scheduler = RedeliveryScheduler::new(queue.clone(), config);
        scheduler.start().await.unwrap();

        let mut entries = scheduler.collect_dead_letters().await;
        let outcome = scheduler
            .schedule_redelivery(&mut entries[0])
            .await
            .unwrap();
        assert_eq!(outcome, RedeliveryOutcome::ForwardedToDlxQueue);
        assert_eq!(queue.message_count("orders.dlx.queue").await, 1);
        scheduler.stop().await;
    }

    #[tokio::test]
    async fn test_redelivery_drop() {
        let queue = Arc::new(InMemoryQueue::with_max_retries(1));
        queue.publish("orders", b"order-1").await.unwrap();
        let msg = queue.consume("orders").await.unwrap().unwrap();
        queue.nack(&msg.id).await.unwrap();
        assert_eq!(queue.dead_letter_count("orders").await, 1);

        let config = DlxConfig::new()
            .enabled()
            .with_routing_strategy(DlxRoutingStrategy::Drop)
            .with_initial_backoff_ms(10)
            .with_max_backoff_ms(100);
        let scheduler = RedeliveryScheduler::new(queue.clone(), config);
        scheduler.start().await.unwrap();

        let mut entries = scheduler.collect_dead_letters().await;
        let outcome = scheduler
            .schedule_redelivery(&mut entries[0])
            .await
            .unwrap();
        assert_eq!(outcome, RedeliveryOutcome::Dropped);
        assert_eq!(queue.dead_letter_count("orders").await, 0);
        scheduler.stop().await;
    }

    #[tokio::test]
    async fn test_redelivery_limit_reached() {
        let queue = Arc::new(InMemoryQueue::with_max_retries(1));
        queue.publish("orders", b"order-1").await.unwrap();
        let msg = queue.consume("orders").await.unwrap().unwrap();
        queue.nack(&msg.id).await.unwrap();

        let config = DlxConfig::new()
            .enabled()
            .with_max_redelivery_count(0)
            .with_initial_backoff_ms(10)
            .with_max_backoff_ms(100);
        let scheduler = RedeliveryScheduler::new(queue.clone(), config);
        scheduler.start().await.unwrap();

        let mut entries = scheduler.collect_dead_letters().await;
        let outcome = scheduler
            .schedule_redelivery(&mut entries[0])
            .await
            .unwrap();
        assert_eq!(outcome, RedeliveryOutcome::LimitReached);
        scheduler.stop().await;
    }

    #[tokio::test]
    async fn test_forward_to_dlx_topic_without_config_errors() {
        let queue = Arc::new(InMemoryQueue::with_max_retries(1));
        queue.publish("orders", b"order-1").await.unwrap();
        let msg = queue.consume("orders").await.unwrap().unwrap();
        queue.nack(&msg.id).await.unwrap();

        let config = DlxConfig::new()
            .enabled()
            .with_routing_strategy(DlxRoutingStrategy::ForwardToDlxTopic)
            .with_initial_backoff_ms(10)
            .with_max_backoff_ms(100);
        let scheduler = RedeliveryScheduler::new(queue.clone(), config);
        scheduler.start().await.unwrap();

        let mut entries = scheduler.collect_dead_letters().await;
        let result = scheduler.schedule_redelivery(&mut entries[0]).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("dlx_topic required"));
        scheduler.stop().await;
    }

    #[tokio::test]
    async fn test_redelivery_logging() {
        let queue = Arc::new(InMemoryQueue::with_max_retries(1));
        queue.publish("orders", b"order-1").await.unwrap();
        let msg = queue.consume("orders").await.unwrap().unwrap();
        queue.nack(&msg.id).await.unwrap();

        let config = DlxConfig::new()
            .enabled()
            .with_routing_strategy(DlxRoutingStrategy::Drop)
            .with_initial_backoff_ms(10)
            .with_max_backoff_ms(100);
        let scheduler = RedeliveryScheduler::new(queue.clone(), config);
        scheduler.start().await.unwrap();

        let mut entries = scheduler.collect_dead_letters().await;
        let _ = scheduler
            .schedule_redelivery(&mut entries[0])
            .await
            .unwrap();

        let logs = scheduler.get_logs();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].redelivery_count, 1);
        assert_eq!(logs[0].outcome, RedeliveryOutcome::Dropped);
        scheduler.stop().await;
    }

    #[tokio::test]
    async fn test_run_once_empty_queue() {
        let queue = Arc::new(InMemoryQueue::new());
        let config = DlxConfig::new().enabled();
        let scheduler = RedeliveryScheduler::new(queue, config);
        scheduler.start().await.unwrap();

        let processed = scheduler.run_once().await.unwrap();
        assert_eq!(processed, 0);
        scheduler.stop().await;
    }

    #[tokio::test]
    async fn test_run_once_not_running() {
        let queue = Arc::new(InMemoryQueue::new());
        let config = DlxConfig::new().enabled();
        let scheduler = RedeliveryScheduler::new(queue, config);

        let processed = scheduler.run_once().await.unwrap();
        assert_eq!(processed, 0);
    }

    #[test]
    fn test_backoff_policy_serde() {
        let policies = vec![
            BackoffPolicy::Fixed,
            BackoffPolicy::Exponential,
            BackoffPolicy::Linear,
            BackoffPolicy::RandomJitter,
        ];
        for p in &policies {
            let json = serde_json::to_string(p).unwrap();
            let decoded: BackoffPolicy = serde_json::from_str(&json).unwrap();
            assert_eq!(*p, decoded);
        }
    }

    #[test]
    fn test_routing_strategy_serde() {
        let strategies = vec![
            DlxRoutingStrategy::RequeueToOriginal,
            DlxRoutingStrategy::ForwardToDlxTopic,
            DlxRoutingStrategy::ForwardToDlxQueue,
            DlxRoutingStrategy::Drop,
        ];
        for s in &strategies {
            let json = serde_json::to_string(s).unwrap();
            let decoded: DlxRoutingStrategy = serde_json::from_str(&json).unwrap();
            assert_eq!(*s, decoded);
        }
    }
}
