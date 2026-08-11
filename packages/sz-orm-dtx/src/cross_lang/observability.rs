//! 跨语言事务告警与可观测
//!
//! 提供事务指标采集、告警规则和故障隔离能力。

use super::CrossLangTxError;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// 事务指标
#[derive(Debug, Clone, Default)]
pub struct CrossLangTxMetrics {
    pub total_transactions: u64,
    pub successful_transactions: u64,
    pub failed_transactions: u64,
    pub timed_out_transactions: u64,
    pub total_participants: u64,
    pub compensation_count: u64,
    pub avg_latency_ms: f64,
    pub max_latency_ms: u64,
}

/// 告警级别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertLevel {
    Info,
    Warning,
    Critical,
}

/// 告警事件
#[derive(Debug, Clone)]
pub struct AlertEvent {
    pub level: AlertLevel,
    pub tx_id: String,
    pub participant_id: String,
    pub message: String,
    pub timestamp: u64,
}

/// 告警处理器 trait
pub trait AlertHandler: Send + Sync {
    fn handle_alert(&self, event: &AlertEvent);
}

/// 日志告警处理器
pub struct LogAlertHandler;

impl AlertHandler for LogAlertHandler {
    fn handle_alert(&self, event: &AlertEvent) {
        tracing::warn!(
            level = ?event.level,
            tx_id = %event.tx_id,
            participant = %event.participant_id,
            "cross-lang tx alert: {}",
            event.message
        );
    }
}

/// 跨语言事务可观测器
pub struct CrossLangTxAlerter {
    metrics: RwLock<CrossLangTxMetrics>,
    alert_handlers: RwLock<Vec<Arc<dyn AlertHandler>>>,
    alert_history: RwLock<Vec<AlertEvent>>,
    max_history: usize,
    /// 故障隔离的参与者列表
    isolated_participants: RwLock<HashMap<String, IsolationReason>>,
}

/// 故障隔离原因
#[derive(Debug, Clone)]
pub struct IsolationReason {
    pub participant_id: String,
    pub reason: String,
    pub isolated_at: u64,
}

impl CrossLangTxAlerter {
    pub fn new() -> Self {
        Self {
            metrics: RwLock::new(CrossLangTxMetrics::default()),
            alert_handlers: RwLock::new(vec![Arc::new(LogAlertHandler)]),
            alert_history: RwLock::new(Vec::new()),
            max_history: 1000,
            isolated_participants: RwLock::new(HashMap::new()),
        }
    }

    pub fn with_max_history(mut self, max: usize) -> Self {
        self.max_history = max;
        self
    }

    pub fn add_alert_handler(&self, handler: Arc<dyn AlertHandler>) {
        self.alert_handlers.write().push(handler);
    }

    pub fn record_success(&self, latency_ms: u64) {
        let mut metrics = self.metrics.write();
        metrics.total_transactions += 1;
        metrics.successful_transactions += 1;
        if latency_ms > metrics.max_latency_ms {
            metrics.max_latency_ms = latency_ms;
        }
        let total = metrics.total_transactions;
        let prev_avg = metrics.avg_latency_ms;
        metrics.avg_latency_ms =
            (prev_avg * (total as f64 - 1.0) + latency_ms as f64) / total as f64;
    }

    pub fn record_failure(&self, tx_id: &str, participant_id: &str, error: &CrossLangTxError) {
        let mut metrics = self.metrics.write();
        metrics.total_transactions += 1;
        metrics.failed_transactions += 1;
        if matches!(error, CrossLangTxError::Timeout) {
            metrics.timed_out_transactions += 1;
        }
        if matches!(error, CrossLangTxError::CompensationFailed { .. }) {
            metrics.compensation_count += 1;
        }
        drop(metrics);

        let event = AlertEvent {
            level: match error {
                CrossLangTxError::Timeout => AlertLevel::Warning,
                CrossLangTxError::CompensationFailed { .. } => AlertLevel::Critical,
                _ => AlertLevel::Warning,
            },
            tx_id: tx_id.to_string(),
            participant_id: participant_id.to_string(),
            message: error.to_string(),
            timestamp: 0,
        };
        self.emit_alert(event);
    }

    pub fn record_compensation(&self, tx_id: &str, participant_id: &str) {
        self.metrics.write().compensation_count += 1;
        let event = AlertEvent {
            level: AlertLevel::Info,
            tx_id: tx_id.to_string(),
            participant_id: participant_id.to_string(),
            message: "compensation executed".to_string(),
            timestamp: 0,
        };
        self.emit_alert(event);
    }

    fn emit_alert(&self, event: AlertEvent) {
        let handlers = self.alert_handlers.read();
        for handler in handlers.iter() {
            handler.handle_alert(&event);
        }
        let mut history = self.alert_history.write();
        history.push(event);
        if history.len() > self.max_history {
            history.remove(0);
        }
    }

    pub fn metrics(&self) -> CrossLangTxMetrics {
        self.metrics.read().clone()
    }

    pub fn alert_history(&self) -> Vec<AlertEvent> {
        self.alert_history.read().clone()
    }

    /// 隔离故障参与者
    pub fn isolate_participant(&self, participant_id: &str, reason: &str, timestamp: u64) {
        self.isolated_participants.write().insert(
            participant_id.to_string(),
            IsolationReason {
                participant_id: participant_id.to_string(),
                reason: reason.to_string(),
                isolated_at: timestamp,
            },
        );
        let event = AlertEvent {
            level: AlertLevel::Critical,
            tx_id: String::new(),
            participant_id: participant_id.to_string(),
            message: format!("participant isolated: {reason}"),
            timestamp,
        };
        self.emit_alert(event);
    }

    /// 解除隔离
    pub fn release_participant(&self, participant_id: &str) -> bool {
        self.isolated_participants
            .write()
            .remove(participant_id)
            .is_some()
    }

    /// 检查参与者是否被隔离
    pub fn is_isolated(&self, participant_id: &str) -> bool {
        self.isolated_participants
            .read()
            .contains_key(participant_id)
    }

    /// 获取所有被隔离的参与者
    pub fn isolated_participants(&self) -> Vec<IsolationReason> {
        self.isolated_participants
            .read()
            .values()
            .cloned()
            .collect()
    }
}

impl Default for CrossLangTxAlerter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_success() {
        let alerter = CrossLangTxAlerter::new();
        alerter.record_success(100);
        alerter.record_success(200);
        let metrics = alerter.metrics();
        assert_eq!(metrics.total_transactions, 2);
        assert_eq!(metrics.successful_transactions, 2);
        assert_eq!(metrics.max_latency_ms, 200);
        assert!((metrics.avg_latency_ms - 150.0).abs() < 0.01);
    }

    #[test]
    fn test_record_failure() {
        let alerter = CrossLangTxAlerter::new();
        alerter.record_failure("tx-1", "p-1", &CrossLangTxError::Timeout);
        let metrics = alerter.metrics();
        assert_eq!(metrics.failed_transactions, 1);
        assert_eq!(metrics.timed_out_transactions, 1);
    }

    #[test]
    fn test_record_compensation() {
        let alerter = CrossLangTxAlerter::new();
        alerter.record_compensation("tx-1", "p-1");
        let metrics = alerter.metrics();
        assert_eq!(metrics.compensation_count, 1);
    }

    #[test]
    fn test_alert_history() {
        let alerter = CrossLangTxAlerter::new();
        alerter.record_failure("tx-1", "p-1", &CrossLangTxError::Timeout);
        alerter.record_failure("tx-2", "p-2", &CrossLangTxError::AuthFailed);
        let history = alerter.alert_history();
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn test_isolate_participant() {
        let alerter = CrossLangTxAlerter::new();
        alerter.isolate_participant("p-1", "repeated failures", 1000);
        assert!(alerter.is_isolated("p-1"));
        assert!(!alerter.is_isolated("p-2"));
    }

    #[test]
    fn test_release_participant() {
        let alerter = CrossLangTxAlerter::new();
        alerter.isolate_participant("p-1", "failure", 1000);
        assert!(alerter.release_participant("p-1"));
        assert!(!alerter.is_isolated("p-1"));
    }

    #[test]
    fn test_isolated_participants_list() {
        let alerter = CrossLangTxAlerter::new();
        alerter.isolate_participant("p-1", "failure", 1000);
        alerter.isolate_participant("p-2", "timeout", 2000);
        let isolated = alerter.isolated_participants();
        assert_eq!(isolated.len(), 2);
    }

    #[test]
    fn test_max_history_limit() {
        let alerter = CrossLangTxAlerter::new().with_max_history(3);
        for i in 0..5 {
            alerter.record_failure(&format!("tx-{i}"), "p-1", &CrossLangTxError::Timeout);
        }
        let history = alerter.alert_history();
        assert!(history.len() <= 3);
    }

    #[test]
    fn test_compensation_failed_alert_level() {
        let alerter = CrossLangTxAlerter::new();
        alerter.record_failure(
            "tx-1",
            "p-1",
            &CrossLangTxError::CompensationFailed {
                participant: "p-1".to_string(),
            },
        );
        let history = alerter.alert_history();
        assert_eq!(history[0].level, AlertLevel::Critical);
    }
}
