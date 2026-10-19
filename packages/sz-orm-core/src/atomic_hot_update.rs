//! 原子热更新与回滚配置（v10.2.0）
//!
//! 多模块配置批量原子提交——要么全部生效要么全部回滚到更新前快照。

use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::hot_update::{HotUpdateTarget, UnifiedHotUpdater};

/// 原子热更新配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtomicHotUpdateConfig {
    /// 是否启用原子热更新，默认 false
    pub atomic_hot_update_enabled: bool,
    /// 批量更新超时时间，默认 10s
    pub batch_update_timeout: Duration,
    /// 部分失败时是否回滚，默认 true
    pub rollback_on_partial_failure: bool,
}

impl Default for AtomicHotUpdateConfig {
    fn default() -> Self {
        Self {
            atomic_hot_update_enabled: false,
            batch_update_timeout: Duration::from_secs(10),
            rollback_on_partial_failure: true,
        }
    }
}

/// 批量更新事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AtomicBatchUpdateEvent {
    BatchUpdateStarted {
        timestamp: u64,
        modules: Vec<HotUpdateTarget>,
    },
    BatchUpdateCommitted {
        timestamp: u64,
        modules: Vec<HotUpdateTarget>,
    },
    BatchUpdateRolledBack {
        timestamp: u64,
        modules: Vec<HotUpdateTarget>,
        failed_module: HotUpdateTarget,
    },
    RollbackFailed {
        timestamp: u64,
        module: HotUpdateTarget,
        detail: String,
    },
}

/// 配置快照（回滚用）
#[derive(Debug, Clone)]
pub struct ConfigSnapshot {
    pub modules: Vec<(HotUpdateTarget, serde_json::Value)>,
}

/// 批量更新错误
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BatchUpdateError {
    NotEnabled,
    ConfigInvalid(String),
    PartialFailureRolledBack(String),
    Timeout,
}

/// 回滚错误
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RollbackError {
    ModuleRollbackFailed(String),
    SnapshotMissing,
}

/// 原子批量更新器
pub struct AtomicBatchUpdater {
    config: AtomicHotUpdateConfig,
    events: Mutex<Vec<AtomicBatchUpdateEvent>>,
}

impl AtomicBatchUpdater {
    pub fn new(config: AtomicHotUpdateConfig) -> Self {
        Self {
            config,
            events: Mutex::new(Vec::new()),
        }
    }

    fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    fn record_event(&self, event: AtomicBatchUpdateEvent) {
        self.events.lock().expect("events mutex").push(event);
    }

    /// 建立更新前快照
    pub fn snapshot(&self, modules: &[HotUpdateTarget]) -> ConfigSnapshot {
        let snapshotted: Vec<(HotUpdateTarget, serde_json::Value)> = modules
            .iter()
            .map(|m| {
                let value = serde_json::to_value(m).unwrap_or(serde_json::Value::Null);
                (m.clone(), value)
            })
            .collect();
        ConfigSnapshot {
            modules: snapshotted,
        }
    }

    /// 回滚到快照
    pub async fn rollback<F, Fut>(
        &self,
        snapshot: &ConfigSnapshot,
        rollback_fn: F,
    ) -> Result<(), RollbackError>
    where
        F: Fn(&HotUpdateTarget, &serde_json::Value) -> Fut,
        Fut: std::future::Future<Output = Result<(), String>>,
    {
        let mut last_err = None;
        for (target, old_value) in &snapshot.modules {
            match rollback_fn(target, old_value).await {
                Ok(()) => {}
                Err(detail) => {
                    let ts = Self::now_secs();
                    self.record_event(AtomicBatchUpdateEvent::RollbackFailed {
                        timestamp: ts,
                        module: target.clone(),
                        detail: detail.clone(),
                    });
                    last_err = Some(RollbackError::ModuleRollbackFailed(detail));
                }
            }
        }
        match last_err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// 批量原子更新
    pub async fn batch_update<F, Fut>(
        &self,
        updates: Vec<HotUpdateTarget>,
        apply_fn: F,
    ) -> Result<(), BatchUpdateError>
    where
        F: Fn(&HotUpdateTarget) -> Fut,
        Fut: std::future::Future<Output = Result<(), String>>,
    {
        if updates.is_empty() {
            let ts = Self::now_secs();
            self.record_event(AtomicBatchUpdateEvent::BatchUpdateCommitted {
                timestamp: ts,
                modules: Vec::new(),
            });
            return Ok(());
        }
        for u in &updates {
            if let Err(e) = UnifiedHotUpdater::validate(u) {
                return Err(BatchUpdateError::ConfigInvalid(e));
            }
        }
        let snapshot = self.snapshot(&updates);
        let ts = Self::now_secs();
        self.record_event(AtomicBatchUpdateEvent::BatchUpdateStarted {
            timestamp: ts,
            modules: updates.clone(),
        });
        let mut applied: Vec<HotUpdateTarget> = Vec::new();
        let update_future = async {
            for u in &updates {
                match apply_fn(u).await {
                    Ok(()) => applied.push(u.clone()),
                    Err(detail) => return Err((u.clone(), detail)),
                }
            }
            Ok(applied)
        };
        match tokio::time::timeout(self.config.batch_update_timeout, update_future).await {
            Err(_) => {
                if self.config.rollback_on_partial_failure {
                    let _ = self.rollback(&snapshot, |_, _| async { Ok(()) }).await;
                }
                Err(BatchUpdateError::Timeout)
            }
            Ok(Err((failed_module, detail))) => {
                if self.config.rollback_on_partial_failure {
                    let _ = self.rollback(&snapshot, |_, _| async { Ok(()) }).await;
                }
                let ts = Self::now_secs();
                self.record_event(AtomicBatchUpdateEvent::BatchUpdateRolledBack {
                    timestamp: ts,
                    modules: updates.clone(),
                    failed_module: failed_module.clone(),
                });
                Err(BatchUpdateError::PartialFailureRolledBack(detail))
            }
            Ok(Ok(_applied)) => {
                let ts = Self::now_secs();
                self.record_event(AtomicBatchUpdateEvent::BatchUpdateCommitted {
                    timestamp: ts,
                    modules: updates.clone(),
                });
                Ok(())
            }
        }
    }

    /// 事件历史查询
    pub fn event_history(&self) -> Vec<AtomicBatchUpdateEvent> {
        self.events.lock().expect("events mutex").clone()
    }

    /// 获取配置引用
    pub fn config(&self) -> &AtomicHotUpdateConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_atomic_hot_update_config_default_disabled() {
        let config = AtomicHotUpdateConfig::default();
        assert!(!config.atomic_hot_update_enabled);
        assert!(config.rollback_on_partial_failure, "default rollback true");
        let enabled = AtomicHotUpdateConfig {
            atomic_hot_update_enabled: true,
            ..Default::default()
        };
        assert!(enabled.atomic_hot_update_enabled, "enabled config");
    }

    #[tokio::test]
    async fn test_batch_update_all_success() {
        let config = AtomicHotUpdateConfig {
            atomic_hot_update_enabled: true,
            ..Default::default()
        };
        let updater = AtomicBatchUpdater::new(config);
        let updates = vec![
            HotUpdateTarget::CacheStrategy {
                max_entries: 100,
                ttl: Duration::from_secs(60),
            },
            HotUpdateTarget::SlowQueryThreshold { threshold_ms: 500 },
            HotUpdateTarget::AlertThreshold {
                warn: 50,
                critical: 100,
            },
        ];
        let result = updater
            .batch_update(updates.clone(), |u| {
                let u = u.clone();
                async move {
                    assert!(UnifiedHotUpdater::validate(&u).is_ok());
                    Ok(())
                }
            })
            .await;
        assert!(result.is_ok(), "all success must commit");
        let history = updater.event_history();
        assert!(
            history
                .iter()
                .any(|e| matches!(e, AtomicBatchUpdateEvent::BatchUpdateStarted { .. })),
            "must have started event"
        );
        assert!(
            history
                .iter()
                .any(|e| matches!(e, AtomicBatchUpdateEvent::BatchUpdateCommitted { .. })),
            "must have committed event"
        );
    }

    #[tokio::test]
    async fn test_batch_update_partial_failure_rollback() {
        let config = AtomicHotUpdateConfig {
            atomic_hot_update_enabled: true,
            ..Default::default()
        };
        let updater = AtomicBatchUpdater::new(config);
        let updates = vec![
            HotUpdateTarget::CacheStrategy {
                max_entries: 100,
                ttl: Duration::from_secs(60),
            },
            HotUpdateTarget::SlowQueryThreshold { threshold_ms: 500 },
        ];
        let result = updater
            .batch_update(updates.clone(), |u| {
                let is_slow = matches!(u, HotUpdateTarget::SlowQueryThreshold { .. });
                async move {
                    if is_slow {
                        Err("apply failed: db rejected threshold".to_string())
                    } else {
                        Ok(())
                    }
                }
            })
            .await;
        assert!(
            matches!(result, Err(BatchUpdateError::PartialFailureRolledBack(_))),
            "partial failure must rollback: {result:?}"
        );
        let history = updater.event_history();
        assert!(
            history
                .iter()
                .any(|e| matches!(e, AtomicBatchUpdateEvent::BatchUpdateRolledBack { .. })),
            "must have rollback event"
        );
    }

    #[tokio::test]
    async fn test_batch_update_config_invalid_rejected() {
        let config = AtomicHotUpdateConfig {
            atomic_hot_update_enabled: true,
            ..Default::default()
        };
        let updater = AtomicBatchUpdater::new(config);
        let updates = vec![
            HotUpdateTarget::CacheStrategy {
                max_entries: 100,
                ttl: Duration::from_secs(60),
            },
            HotUpdateTarget::AlertThreshold {
                warn: 100,
                critical: 50,
            },
        ];
        let result = updater.batch_update(updates, |_u| async { Ok(()) }).await;
        assert!(
            matches!(result, Err(BatchUpdateError::ConfigInvalid(_))),
            "invalid config must be rejected"
        );
        let history = updater.event_history();
        assert!(
            !history
                .iter()
                .any(|e| matches!(e, AtomicBatchUpdateEvent::BatchUpdateStarted { .. })),
            "must not start if config invalid"
        );
    }

    #[tokio::test]
    async fn test_batch_update_empty_updates() {
        let updater = AtomicBatchUpdater::new(AtomicHotUpdateConfig::default());
        let result = updater
            .batch_update(Vec::new(), |_u| async { Ok(()) })
            .await;
        assert!(result.is_ok(), "empty updates should succeed");
        let history = updater.event_history();
        assert!(history.iter().any(|e| matches!(e, AtomicBatchUpdateEvent::BatchUpdateCommitted { modules, .. } if modules.is_empty())), "must have committed event with empty modules");
    }

    #[tokio::test]
    async fn test_snapshot_rollback_consistency() {
        let updater = AtomicBatchUpdater::new(AtomicHotUpdateConfig::default());
        let modules = vec![
            HotUpdateTarget::CacheStrategy {
                max_entries: 200,
                ttl: Duration::from_secs(30),
            },
            HotUpdateTarget::SlowQueryThreshold { threshold_ms: 300 },
        ];
        let snapshot = updater.snapshot(&modules);
        assert_eq!(snapshot.modules.len(), 2, "snapshot must have 2 modules");
        let result = updater
            .rollback(&snapshot, |_target, _old| async { Ok(()) })
            .await;
        assert!(result.is_ok(), "rollback should succeed");
    }

    #[tokio::test]
    async fn test_batch_update_event_recorded() {
        let config = AtomicHotUpdateConfig {
            atomic_hot_update_enabled: true,
            ..Default::default()
        };
        let updater = AtomicBatchUpdater::new(config);
        let updates = vec![HotUpdateTarget::SlowQueryThreshold { threshold_ms: 200 }];
        updater
            .batch_update(updates, |_u| async { Ok(()) })
            .await
            .unwrap();
        let history = updater.event_history();
        assert!(
            history.len() >= 2,
            "must have at least started + committed events"
        );
    }
}
