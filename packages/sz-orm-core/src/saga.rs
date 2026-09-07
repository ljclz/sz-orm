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
}

impl SagaCoordinator {
    pub fn new(store: Arc<SagaStore>, timeout_config: TimeoutConfig) -> Self {
        Self {
            store,
            timeout_config,
        }
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
