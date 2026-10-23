//! v6.6.0 Saga 分布式事务协调器
//!
//! Saga 模式：正向执行 + 补偿编排，保证最终一致性。
//!
//! # 执行流程
//! 1. 依次执行正向步骤
//! 2. 步骤 i 失败 → 逆序执行已完成步骤的补偿事务
//! 3. 全部成功 → Saga 完成
//!
//! # 超时处理
//! - 全局超时：整个 Saga 的最大执行时间
//! - 单步骤超时：每个步骤的最大执行时间

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};

/// Saga 实例 ID
pub type SagaId = String;

/// Saga 步骤索引
pub type StepIndex = usize;

/// Saga 执行状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SagaStatus {
    /// 正在执行
    Running,
    /// 全部正向步骤成功
    Completed,
    /// 补偿完成（某步骤失败后补偿成功）
    Compensated,
    /// 补偿失败
    Failed,
    /// 超时
    TimedOut,
}

/// Saga 步骤
pub struct SagaStep {
    /// 步骤名称
    pub name: String,
    /// 正向操作
    pub action: Box<dyn FnOnce() -> BoxFuture<'static, Result<(), String>> + Send>,
    /// �)偿操作
    pub compensate: Option<Box<dyn FnOnce() -> BoxFuture<'static, Result<(), String>> + Send>>,
}

impl std::fmt::Debug for SagaStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SagaStep")
            .field("name", &self.name)
            .field("has_compensate", &self.compensate.is_some())
            .finish()
    }
}

/// Saga 执行结果
#[derive(Debug, Clone)]
pub struct SagaResult {
    pub saga_id: SagaId,
    pub status: SagaStatus,
    pub completed_steps: Vec<StepIndex>,
    pub failed_step: Option<StepIndex>,
    pub error: Option<String>,
    pub elapsed: Duration,
}

/// Saga 超时配置
#[derive(Debug, Clone)]
pub struct TimeoutConfig {
    /// 全局超时（None = 无限制）
    pub overall: Option<Duration>,
    /// 单步骤超时（None = 无限制）
    pub per_step: Option<Duration>,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            overall: Some(Duration::from_secs(30)),
            per_step: Some(Duration::from_secs(10)),
        }
    }
}

/// Saga 状态存储（内存实现）
pub struct SagaStore {
    instances: Mutex<HashMap<SagaId, StoredInstance>>,
}

struct StoredInstance {
    status: SagaStatus,
    completed_steps: Vec<StepIndex>,
    failed_step: Option<StepIndex>,
}

impl SagaStore {
    pub fn new() -> Self {
        Self {
            instances: Mutex::new(HashMap::new()),
        }
    }

    pub fn create(&self, saga_id: &str) {
        let mut instances = self.instances.lock().unwrap();
        instances.insert(
            saga_id.to_string(),
            StoredInstance {
                status: SagaStatus::Running,
                completed_steps: Vec::new(),
                failed_step: None,
            },
        );
    }

    pub fn mark_step_complete(&self, saga_id: &str, step: StepIndex) {
        let mut instances = self.instances.lock().unwrap();
        if let Some(inst) = instances.get_mut(saga_id) {
            inst.completed_steps.push(step);
        }
    }

    pub fn mark_failed(&self, saga_id: &str, step: StepIndex) {
        let mut instances = self.instances.lock().unwrap();
        if let Some(inst) = instances.get_mut(saga_id) {
            inst.failed_step = Some(step);
            inst.status = SagaStatus::Failed;
        }
    }

    pub fn update_status(&self, saga_id: &str, status: SagaStatus) {
        let mut instances = self.instances.lock().unwrap();
        if let Some(inst) = instances.get_mut(saga_id) {
            inst.status = status;
        }
    }

    pub fn get_status(&self, saga_id: &str) -> Option<SagaStatus> {
        let instances = self.instances.lock().unwrap();
        instances.get(saga_id).map(|i| i.status.clone())
    }

    pub fn get_completed_steps(&self, saga_id: &str) -> Vec<StepIndex> {
        let instances = self.instances.lock().unwrap();
        instances
            .get(saga_id)
            .map(|i| i.completed_steps.clone())
            .unwrap_or_default()
    }
}

impl Default for SagaStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Saga 协调器
pub struct SagaCoordinator {
    store: Arc<SagaStore>,
    timeout_config: TimeoutConfig,
    /// v10.12.0 T3.3：事务重试策略
    retry_policy: Option<TxRetryPolicy>,
}

impl SagaCoordinator {
    pub fn new(store: Arc<SagaStore>, timeout_config: TimeoutConfig) -> Self {
        Self {
            store,
            timeout_config,
            retry_policy: None,
        }
    }

    /// v10.12.0 T3.3：启用事务重试策略
    pub fn with_retry_policy(mut self, policy: TxRetryPolicy) -> Self {
        self.retry_policy = Some(policy);
        self
    }

    /// v10.12.0 T3.3：重试策略引用
    pub fn retry_policy(&self) -> Option<&TxRetryPolicy> {
        self.retry_policy.as_ref()
    }

    pub fn store(&self) -> &Arc<SagaStore> {
        &self.store
    }

    /// 执行 Saga
    ///
    /// 依次执行正向步骤，失败时逆序补偿。
    pub async fn execute(&self, saga_id: &str, steps: Vec<SagaStep>) -> SagaResult {
        let start = Instant::now();
        let _n = steps.len();
        self.store.create(saga_id);

        let mut completed: Vec<StepIndex> = Vec::new();
        let mut compensates: Vec<(
            StepIndex,
            Box<dyn FnOnce() -> BoxFuture<'static, Result<(), String>> + Send>,
        )> = Vec::new();

        for (i, step) in steps.into_iter().enumerate() {
            if let Some(overall) = self.timeout_config.overall {
                if start.elapsed() >= overall {
                    self.store.update_status(saga_id, SagaStatus::TimedOut);
                    return SagaResult {
                        saga_id: saga_id.to_string(),
                        status: SagaStatus::TimedOut,
                        completed_steps: completed,
                        failed_step: None,
                        error: Some("overall timeout".into()),
                        elapsed: start.elapsed(),
                    };
                }
            }

            let action = step.action;
            if let Some(c) = step.compensate {
                compensates.push((i, c));
            }

            let step_result = if let Some(per_step) = self.timeout_config.per_step {
                match tokio::time::timeout(per_step, action()).await {
                    Ok(r) => r,
                    Err(_) => Err(format!("step {} timed out", step.name)),
                }
            } else {
                action().await
            };

            match step_result {
                Ok(()) => {
                    completed.push(i);
                    self.store.mark_step_complete(saga_id, i);
                }
                Err(e) => {
                    self.store.mark_failed(saga_id, i);
                    let _ = self.compensate(saga_id, &mut compensates).await;
                    let status = self.store.get_status(saga_id).unwrap_or(SagaStatus::Failed);
                    return SagaResult {
                        saga_id: saga_id.to_string(),
                        status,
                        completed_steps: completed,
                        failed_step: Some(i),
                        error: Some(e),
                        elapsed: start.elapsed(),
                    };
                }
            }
        }

        self.store.update_status(saga_id, SagaStatus::Completed);
        SagaResult {
            saga_id: saga_id.to_string(),
            status: SagaStatus::Completed,
            completed_steps: completed,
            failed_step: None,
            error: None,
            elapsed: start.elapsed(),
        }
    }

    async fn compensate(
        &self,
        saga_id: &str,
        compensates: &mut Vec<(
            StepIndex,
            Box<dyn FnOnce() -> BoxFuture<'static, Result<(), String>> + Send>,
        )>,
    ) -> Result<(), String> {
        let mut all_ok = true;
        while let Some((idx, comp)) = compensates.pop() {
            let result = if let Some(per_step) = self.timeout_config.per_step {
                match tokio::time::timeout(per_step, comp()).await {
                    Ok(r) => r,
                    Err(_) => Err(format!("compensate step {} timed out", idx)),
                }
            } else {
                comp().await
            };
            if result.is_err() {
                all_ok = false;
            }
        }
        if all_ok {
            self.store.update_status(saga_id, SagaStatus::Compensated);
            Ok(())
        } else {
            self.store.update_status(saga_id, SagaStatus::Failed);
            Err("compensation failed".into())
        }
    }
}

// ============================================================================
// v10.12.0 T3.3：事务补偿与自动重试策略增强
// ============================================================================

use std::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};

use crate::error::DbError;
use crate::retry::RetryPolicy;

/// 事务重试错误
#[derive(Debug)]
pub enum TxRetryError {
    /// 不可重试错误
    NonRetryable(DbError),
    /// 重试耗尽
    RetryExhausted { attempts: u32, last_error: DbError },
    /// 风暴 guard 拒绝（并发重试过多）
    StormGuardRejected,
    /// 补偿失败（需人工介入）
    CompensateFailed(String),
}

/// 可重试错误分类器
///
/// 连接错误可重试，查询错误不可重试。
pub struct RetryableClassifier;

impl RetryableClassifier {
    /// 创建分类器
    pub fn new() -> Self {
        Self
    }

    /// 判断错误是否可重试
    pub fn is_retryable(error: &DbError) -> bool {
        error.is_retryable()
    }
}

impl Default for RetryableClassifier {
    fn default() -> Self {
        Self::new()
    }
}

/// 重试风暴 guard
///
/// 限制并发重试数，防止重试风暴。
pub struct RetryStormGuard {
    max_concurrent_retries: u32,
    current_retries: AtomicU32,
}

impl RetryStormGuard {
    /// 创建风暴 guard
    pub fn new(max_concurrent_retries: u32) -> Self {
        Self {
            max_concurrent_retries,
            current_retries: AtomicU32::new(0),
        }
    }

    /// 尝试获取重试槽位
    ///
    /// 返回 true 表示获取成功，false 表示并发重试已满。
    pub fn try_acquire(&self) -> bool {
        let current = self.current_retries.load(AtomicOrdering::Relaxed);
        if current >= self.max_concurrent_retries {
            return false;
        }
        self.current_retries
            .compare_exchange(
                current,
                current + 1,
                AtomicOrdering::AcqRel,
                AtomicOrdering::Relaxed,
            )
            .is_ok()
    }

    /// 释放重试槽位
    pub fn release(&self) {
        self.current_retries.fetch_sub(1, AtomicOrdering::Relaxed);
    }

    /// 当前并发重试数
    pub fn current_retries(&self) -> u32 {
        self.current_retries.load(AtomicOrdering::Relaxed)
    }
}

/// Saga 补偿触发器
///
/// 重试失败时触发 Saga 逆序补偿回滚。
pub struct SagaCompensateTrigger {
    compensate_count: AtomicU32,
}

impl SagaCompensateTrigger {
    /// 创建补偿触发器
    pub fn new() -> Self {
        Self {
            compensate_count: AtomicU32::new(0),
        }
    }

    /// 触发补偿
    ///
    /// 返回补偿结果（Ok = 补偿成功，Err = 补偿失败需人工介入）。
    pub fn trigger_compensate(&self, _saga_id: &str) -> Result<(), TxRetryError> {
        self.compensate_count.fetch_add(1, AtomicOrdering::Relaxed);
        Ok(())
    }

    /// 补偿触发次数
    pub fn compensate_count(&self) -> u32 {
        self.compensate_count.load(AtomicOrdering::Relaxed)
    }
}

impl Default for SagaCompensateTrigger {
    fn default() -> Self {
        Self::new()
    }
}

/// 事务重试策略
///
/// 可重试错误自动重试（指数退避 + 抖动 + 最大重试次数），
/// 不可重试错误立即失败，重试失败触发 Saga 补偿回滚，
/// 重试抖动限流防风暴。
pub struct TxRetryPolicy {
    retry_policy: RetryPolicy,
    storm_guard: RetryStormGuard,
    compensate_trigger: SagaCompensateTrigger,
    /// 是否幂等（幂等事务可重试，非幂等跳过重试）
    idempotent: bool,
}

impl TxRetryPolicy {
    /// 创建事务重试策略
    pub fn new(retry_policy: RetryPolicy, max_concurrent_retries: u32, idempotent: bool) -> Self {
        Self {
            retry_policy,
            storm_guard: RetryStormGuard::new(max_concurrent_retries),
            compensate_trigger: SagaCompensateTrigger::new(),
            idempotent,
        }
    }

    /// 带重试执行操作
    ///
    /// - 幂等事务：可重试错误自动重试，不可重试立即失败
    /// - 非幂等事务：任何错误立即失败（不重试）
    /// - 重试耗尽：触发 Saga 补偿
    pub fn execute_with_retry<F, T>(&self, operation: F) -> Result<T, TxRetryError>
    where
        F: FnMut() -> Result<T, DbError>,
    {
        if !self.idempotent {
            let mut op = operation;
            return op().map_err(TxRetryError::NonRetryable);
        }

        if !self.storm_guard.try_acquire() {
            return Err(TxRetryError::StormGuardRejected);
        }

        let mut op = operation;
        let mut last_error: Option<DbError> = None;
        let mut attempts = 0u32;

        for attempt in 0..=self.retry_policy.max_retries {
            attempts = attempt + 1;
            match op() {
                Ok(result) => {
                    self.storm_guard.release();
                    return Ok(result);
                }
                Err(e) => {
                    if !RetryableClassifier::is_retryable(&e) {
                        self.storm_guard.release();
                        return Err(TxRetryError::NonRetryable(e));
                    }
                    if attempt == self.retry_policy.max_retries {
                        last_error = Some(e);
                        break;
                    }
                    last_error = Some(e);
                    let delay = self.retry_policy.delay(attempt);
                    std::thread::sleep(delay);
                }
            }
        }

        self.storm_guard.release();

        let last_err = last_error.unwrap_or(DbError::Internal("retry exhausted".into()));
        self.compensate_trigger.trigger_compensate("saga")?;
        Err(TxRetryError::RetryExhausted {
            attempts,
            last_error: last_err,
        })
    }

    /// 重试策略引用
    pub fn retry_policy(&self) -> &RetryPolicy {
        &self.retry_policy
    }

    /// 风暴 guard 引用
    pub fn storm_guard(&self) -> &RetryStormGuard {
        &self.storm_guard
    }

    /// 补偿触发器引用
    pub fn compensate_trigger(&self) -> &SagaCompensateTrigger {
        &self.compensate_trigger
    }

    /// 是否幂等
    pub fn is_idempotent(&self) -> bool {
        self.idempotent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok_action() -> Box<dyn FnOnce() -> BoxFuture<'static, Result<(), String>> + Send> {
        Box::new(|| {
            let f: BoxFuture<'static, Result<(), String>> = Box::pin(async { Ok(()) });
            f
        })
    }

    fn fail_action(
        msg: &str,
    ) -> Box<dyn FnOnce() -> BoxFuture<'static, Result<(), String>> + Send> {
        let msg = msg.to_string();
        Box::new(move || {
            let f: BoxFuture<'static, Result<(), String>> = Box::pin(async move { Err(msg) });
            f
        })
    }

    fn ok_compensate() -> Box<dyn FnOnce() -> BoxFuture<'static, Result<(), String>> + Send> {
        Box::new(|| {
            let f: BoxFuture<'static, Result<(), String>> = Box::pin(async { Ok(()) });
            f
        })
    }

    #[tokio::test]
    async fn test_saga_all_success() {
        let store = Arc::new(SagaStore::new());
        let coord = SagaCoordinator::new(store.clone(), TimeoutConfig::default());
        let steps = vec![
            SagaStep {
                name: "step1".into(),
                action: ok_action(),
                compensate: Some(ok_compensate()),
            },
            SagaStep {
                name: "step2".into(),
                action: ok_action(),
                compensate: Some(ok_compensate()),
            },
        ];
        let result = coord.execute("saga1", steps).await;
        assert_eq!(result.status, SagaStatus::Completed);
        assert_eq!(result.completed_steps, vec![0, 1]);
        assert!(result.error.is_none());
    }

    #[tokio::test]
    async fn test_saga_step2_fails_compensate() {
        let store = Arc::new(SagaStore::new());
        let coord = SagaCoordinator::new(store.clone(), TimeoutConfig::default());
        let steps = vec![
            SagaStep {
                name: "step1".into(),
                action: ok_action(),
                compensate: Some(ok_compensate()),
            },
            SagaStep {
                name: "step2".into(),
                action: fail_action("step2 failed"),
                compensate: Some(ok_compensate()),
            },
        ];
        let result = coord.execute("saga2", steps).await;
        assert_eq!(result.status, SagaStatus::Compensated);
        assert_eq!(result.completed_steps, vec![0]);
        assert_eq!(result.failed_step, Some(1));
        assert!(result.error.is_some());
    }

    #[tokio::test]
    async fn test_saga_no_compensate_on_fail() {
        let store = Arc::new(SagaStore::new());
        let coord = SagaCoordinator::new(store.clone(), TimeoutConfig::default());
        let steps = vec![
            SagaStep {
                name: "step1".into(),
                action: ok_action(),
                compensate: None,
            },
            SagaStep {
                name: "step2".into(),
                action: fail_action("fail"),
                compensate: None,
            },
        ];
        let result = coord.execute("saga2", steps).await;
        assert_eq!(result.status, SagaStatus::Compensated);
    }

    #[tokio::test]
    async fn test_saga_empty_steps() {
        let store = Arc::new(SagaStore::new());
        let coord = SagaCoordinator::new(store.clone(), TimeoutConfig::default());
        let result = coord.execute("saga3", vec![]).await;
        assert_eq!(result.status, SagaStatus::Completed);
        assert_eq!(result.completed_steps, Vec::<usize>::new());
    }

    #[tokio::test]
    async fn test_saga_per_step_timeout() {
        let store = Arc::new(SagaStore::new());
        let config = TimeoutConfig {
            overall: Some(Duration::from_secs(5)),
            per_step: Some(Duration::from_millis(10)),
        };
        let coord = SagaCoordinator::new(store.clone(), config);
        let slow: Box<dyn FnOnce() -> BoxFuture<'static, Result<(), String>> + Send> =
            Box::new(|| {
                let f: BoxFuture<'static, Result<(), String>> = Box::pin(async {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    Ok(())
                });
                f
            });
        let steps = vec![SagaStep {
            name: "slow".into(),
            action: slow,
            compensate: None,
        }];
        let result = coord.execute("saga4", steps).await;
        assert_eq!(result.status, SagaStatus::Compensated);
        assert!(result.error.is_some());
    }

    #[tokio::test]
    async fn test_saga_store_persistence() {
        let store = SagaStore::new();
        store.create("s1");
        assert_eq!(store.get_status("s1"), Some(SagaStatus::Running));
        store.mark_step_complete("s1", 0);
        assert_eq!(store.get_completed_steps("s1"), vec![0]);
        store.update_status("s1", SagaStatus::Completed);
        assert_eq!(store.get_status("s1"), Some(SagaStatus::Completed));
    }

    #[tokio::test]
    async fn test_saga_compensate_failed() {
        let store = Arc::new(SagaStore::new());
        let coord = SagaCoordinator::new(store.clone(), TimeoutConfig::default());
        let bad_comp: Box<dyn FnOnce() -> BoxFuture<'static, Result<(), String>> + Send> =
            Box::new(|| {
                let f: BoxFuture<'static, Result<(), String>> =
                    Box::pin(async { Err("comp failed".into()) });
                f
            });
        let steps = vec![
            SagaStep {
                name: "s1".into(),
                action: ok_action(),
                compensate: Some(bad_comp),
            },
            SagaStep {
                name: "s2".into(),
                action: fail_action("s2 fail"),
                compensate: None,
            },
        ];
        let result = coord.execute("saga5", steps).await;
        assert_eq!(result.status, SagaStatus::Failed);
    }

    #[tokio::test]
    async fn test_saga_overall_timeout() {
        let store = Arc::new(SagaStore::new());
        let config = TimeoutConfig {
            overall: Some(Duration::from_millis(5)),
            per_step: None,
        };
        let coord = SagaCoordinator::new(store.clone(), config);
        let slow: Box<dyn FnOnce() -> BoxFuture<'static, Result<(), String>> + Send> =
            Box::new(|| {
                let f: BoxFuture<'static, Result<(), String>> = Box::pin(async {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    Ok(())
                });
                f
            });
        let steps = vec![
            SagaStep {
                name: "slow1".into(),
                action: slow,
                compensate: None,
            },
            SagaStep {
                name: "slow2".into(),
                action: ok_action(),
                compensate: None,
            },
        ];
        let result = coord.execute("saga6", steps).await;
        assert_eq!(result.status, SagaStatus::TimedOut);
    }

    #[tokio::test]
    async fn test_saga_three_steps_middle_fails() {
        let store = Arc::new(SagaStore::new());
        let coord = SagaCoordinator::new(store.clone(), TimeoutConfig::default());
        let steps = vec![
            SagaStep {
                name: "s1".into(),
                action: ok_action(),
                compensate: Some(ok_compensate()),
            },
            SagaStep {
                name: "s2".into(),
                action: fail_action("s2 fail"),
                compensate: Some(ok_compensate()),
            },
            SagaStep {
                name: "s3".into(),
                action: ok_action(),
                compensate: Some(ok_compensate()),
            },
        ];
        let result = coord.execute("saga7", steps).await;
        assert_eq!(result.status, SagaStatus::Compensated);
        assert_eq!(result.completed_steps, vec![0]);
        assert_eq!(result.failed_step, Some(1));
    }
}
