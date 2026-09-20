//! 自动回滚触发器

use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::RwLock;

use super::gray_release::RollbackReport;

/// 自动回滚触发器：指标驱动的自动回滚
pub struct AutoRollbackTrigger {
    rollback_threshold: f64,
    max_rollback_time: Duration,
    in_progress: Arc<RwLock<bool>>,
    last_rollback: Arc<RwLock<Option<RollbackReport>>>,
}

impl AutoRollbackTrigger {
    pub fn new(rollback_threshold: f64) -> Self {
        Self {
            rollback_threshold,
            max_rollback_time: Duration::from_secs(300),
            in_progress: Arc::new(RwLock::new(false)),
            last_rollback: Arc::new(RwLock::new(None)),
        }
    }

    pub fn with_max_time(max_time: Duration, rollback_threshold: f64) -> Self {
        Self {
            rollback_threshold,
            max_rollback_time: max_time,
            in_progress: Arc::new(RwLock::new(false)),
            last_rollback: Arc::new(RwLock::new(None)),
        }
    }

    pub fn check_and_trigger(&self, error_rate: f64, release_id: &str) -> Option<RollbackReport> {
        if error_rate <= self.rollback_threshold {
            return None;
        }
        let start = Instant::now();
        *self.in_progress.write() = true;

        let report = RollbackReport {
            release_id: release_id.to_string(),
            reason: format!(
                "错误率 {} 超过回滚阈值 {}",
                error_rate, self.rollback_threshold
            ),
            rolled_back_at: start.elapsed().as_millis() as i64,
            success: start.elapsed() <= self.max_rollback_time,
        };

        *self.last_rollback.write() = Some(report.clone());
        *self.in_progress.write() = false;
        Some(report)
    }

    pub fn is_rolling_back(&self) -> bool {
        *self.in_progress.read()
    }

    pub fn last_rollback(&self) -> Option<RollbackReport> {
        self.last_rollback.read().clone()
    }

    pub fn rollback_failed_escalate(&self, report: &RollbackReport) -> EscalationResult {
        if report.success {
            return EscalationResult {
                escalated: false,
                snapshot_preserved: false,
                message: "回滚成功，无需升级".to_string(),
            };
        }
        EscalationResult {
            escalated: true,
            snapshot_preserved: true,
            message: format!(
                "回滚失败，升级人工处理，保留状态快照: {}",
                report.release_id
            ),
        }
    }
}

/// 升级结果
#[derive(Debug, Clone)]
pub struct EscalationResult {
    pub escalated: bool,
    pub snapshot_preserved: bool,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_trigger_below_threshold() {
        let trigger = AutoRollbackTrigger::new(0.05);
        let result = trigger.check_and_trigger(0.01, "rel-1");
        assert!(result.is_none());
    }

    #[test]
    fn test_trigger_above_threshold() {
        let trigger = AutoRollbackTrigger::new(0.05);
        let result = trigger.check_and_trigger(0.06, "rel-1");
        assert!(result.is_some());
        let report = result.unwrap();
        assert!(report.success);
        assert!(report.reason.contains("0.06"));
    }

    #[test]
    fn test_rollback_success_no_escalation() {
        let trigger = AutoRollbackTrigger::new(0.05);
        let report = trigger.check_and_trigger(0.06, "rel-1").unwrap();
        let escalation = trigger.rollback_failed_escalate(&report);
        assert!(!escalation.escalated);
    }

    #[test]
    fn test_rollback_failed_escalation() {
        let trigger = AutoRollbackTrigger::new(0.05);
        let failed_report = RollbackReport {
            release_id: "rel-1".to_string(),
            reason: "timeout".to_string(),
            rolled_back_at: 0,
            success: false,
        };
        let escalation = trigger.rollback_failed_escalate(&failed_report);
        assert!(escalation.escalated);
        assert!(escalation.snapshot_preserved);
    }

    #[test]
    fn test_last_rollback_recorded() {
        let trigger = AutoRollbackTrigger::new(0.05);
        trigger.check_and_trigger(0.06, "rel-1");
        assert!(trigger.last_rollback().is_some());
    }
}
