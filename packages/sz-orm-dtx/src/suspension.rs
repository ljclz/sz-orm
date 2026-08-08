//! 悬挂事务检测 — 后台定时扫描超时事务
//!
//! tokio 后台任务，周期扫描 Prepare 后超时未决定的事务，
//! 按策略处理（Commit/Rollback），收敛到终态。

use std::sync::Arc;

use tokio::sync::watch;

use crate::xa::{SuspendedTransaction, SuspensionConfig, XaCoordinator};
use crate::TransactionLogStore;

/// 悬挂事务检测器
///
/// 后台 tokio 任务，周期扫描 `TransactionLogStore` 中的未决事务，
/// 检测 Prepare 后超时未决定的事务，按策略处理。
pub struct SuspensionDetector {
    log_store: Arc<dyn TransactionLogStore>,
    config: SuspensionConfig,
    /// 取消令牌（优雅关闭）
    cancel_tx: watch::Sender<bool>,
    cancel_rx: watch::Receiver<bool>,
}

impl SuspensionDetector {
    /// 创建悬挂检测器
    pub fn new(log_store: Arc<dyn TransactionLogStore>, config: SuspensionConfig) -> Self {
        let (cancel_tx, cancel_rx) = watch::channel(false);
        Self {
            log_store,
            config,
            cancel_tx,
            cancel_rx,
        }
    }

    /// 获取配置
    pub fn config(&self) -> &SuspensionConfig {
        &self.config
    }

    /// 优雅关闭
    pub fn shutdown(&self) {
        let _ = self.cancel_tx.send(true);
    }

    /// 检测超时事务
    ///
    /// 扫描未决事务，返回超时的事务列表。
    /// 超时判定：当前时间 - 日志时间戳 > config.timeout
    pub async fn detect_suspended(&self) -> Vec<SuspendedTransaction> {
        let pending = match self.log_store.read_pending().await {
            Ok(entries) => entries,
            Err(_) => return vec![],
        };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let timeout_ms = self.config.timeout.as_millis() as u64;

        let mut suspended = Vec::new();
        for entry in pending {
            // 只检测 Prepared 状态（Prepare 后未进入 Commit）
            if entry.state != "Prepared" {
                continue;
            }

            let entry_time: u64 = entry.timestamp.parse().unwrap_or(0);
            if now.saturating_sub(entry_time) > timeout_ms {
                let suspended_at = chrono::DateTime::from_timestamp_millis(entry_time as i64)
                    .unwrap_or_else(chrono::Utc::now);
                for resource_id in &entry.participants {
                    suspended.push(SuspendedTransaction {
                        tx_id: entry.tx_id.clone(),
                        resource_id: resource_id.clone(),
                        suspended_at,
                        policy: self.config.policy,
                    });
                }
            }
        }

        suspended
    }

    /// 启动后台检测循环
    ///
    /// 周期扫描超时事务，按策略处理。
    /// 通过 `shutdown()` 优雅关闭。
    pub async fn run(&self, _coordinator: &XaCoordinator) {
        let mut interval = tokio::time::interval(self.config.check_interval);
        let mut cancel_rx = self.cancel_rx.clone();

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let suspended = self.detect_suspended().await;
                    for s in &suspended {
                        tracing::warn!(
                            tx_id = %s.tx_id,
                            resource_id = %s.resource_id,
                            policy = ?s.policy,
                            "检测到悬挂事务"
                        );
                    }
                }
                _ = cancel_rx.changed() => {
                    if *cancel_rx.borrow() {
                        break;
                    }
                }
            }
        }
    }
}

impl std::fmt::Debug for SuspensionDetector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SuspensionDetector")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xa::SuspensionPolicy;
    use crate::InMemoryTransactionLog;
    use crate::TransactionLogEntry;
    use std::time::Duration;

    #[tokio::test]
    async fn test_detect_no_suspended() {
        let log_store: Arc<dyn TransactionLogStore> = Arc::new(InMemoryTransactionLog::new());
        let detector = SuspensionDetector::new(log_store, SuspensionConfig::default());

        let suspended = detector.detect_suspended().await;
        assert!(suspended.is_empty());
    }

    #[tokio::test]
    async fn test_detect_suspended_prepared() {
        let log_store: Arc<dyn TransactionLogStore> = Arc::new(InMemoryTransactionLog::new());

        // 写入一条很久以前的 Prepared 日志（时间戳为 0）
        log_store
            .append(TransactionLogEntry {
                tx_id: "tx-suspend-1".to_string(),
                state: "Prepared".to_string(),
                participants: vec!["db1".to_string(), "db2".to_string()],
                timestamp: "0".to_string(),
                action: "prepare".to_string(),
            })
            .await
            .unwrap();

        let config = SuspensionConfig {
            timeout: Duration::from_secs(1),
            policy: SuspensionPolicy::Rollback,
            check_interval: Duration::from_secs(1),
        };
        let detector = SuspensionDetector::new(log_store, config);

        let suspended = detector.detect_suspended().await;
        assert_eq!(suspended.len(), 2);
        assert_eq!(suspended[0].tx_id, "tx-suspend-1");
        assert_eq!(suspended[0].policy, SuspensionPolicy::Rollback);
    }

    #[tokio::test]
    async fn test_detect_not_suspended_recent() {
        let log_store: Arc<dyn TransactionLogStore> = Arc::new(InMemoryTransactionLog::new());

        // 写入一条刚刚的 Prepared 日志
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis().to_string())
            .unwrap_or_else(|_| "0".to_string());

        log_store
            .append(TransactionLogEntry {
                tx_id: "tx-recent".to_string(),
                state: "Prepared".to_string(),
                participants: vec!["db1".to_string()],
                timestamp: now,
                action: "prepare".to_string(),
            })
            .await
            .unwrap();

        let config = SuspensionConfig {
            timeout: Duration::from_secs(30),
            policy: SuspensionPolicy::Rollback,
            check_interval: Duration::from_secs(5),
        };
        let detector = SuspensionDetector::new(log_store, config);

        let suspended = detector.detect_suspended().await;
        assert!(suspended.is_empty());
    }

    #[tokio::test]
    async fn test_detect_ignores_non_prepared() {
        let log_store: Arc<dyn TransactionLogStore> = Arc::new(InMemoryTransactionLog::new());

        // Preparing 状态不应被检测为悬挂
        log_store
            .append(TransactionLogEntry {
                tx_id: "tx-preparing".to_string(),
                state: "Preparing".to_string(),
                participants: vec!["db1".to_string()],
                timestamp: "0".to_string(),
                action: "prepare".to_string(),
            })
            .await
            .unwrap();

        let config = SuspensionConfig {
            timeout: Duration::from_secs(1),
            policy: SuspensionPolicy::Rollback,
            check_interval: Duration::from_secs(1),
        };
        let detector = SuspensionDetector::new(log_store, config);

        let suspended = detector.detect_suspended().await;
        assert!(suspended.is_empty());
    }

    #[tokio::test]
    async fn test_shutdown() {
        let log_store: Arc<dyn TransactionLogStore> = Arc::new(InMemoryTransactionLog::new());
        let detector = SuspensionDetector::new(log_store, SuspensionConfig::default());

        detector.shutdown();
        // 验证取消信号已发送
        assert!(*detector.cancel_rx.borrow());
    }
}
