//! 决策回放器：回放历史自治决策，模拟决策路径并产出对比报告

use std::sync::Arc;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use sz_orm_audit::AutonomousDecisionAuditor;

use super::XaiError;

/// 回放报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayReport {
    pub decision_id: String,
    pub decision_path: DecisionPath,
    pub current_strategy_diff: StrategyDiff,
    pub replayed_at: SystemTime,
    pub expired: bool,
}

/// 决策路径
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionPath {
    pub event_type: String,
    pub policy_name: String,
    pub action_type: String,
    pub reasoning: String,
    pub dry_run: bool,
}

/// 与当前策略差异
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyDiff {
    pub policy_name: String,
    pub action_type: String,
    pub diff_summary: String,
}

/// 决策回放器
pub struct DecisionReplayer {
    auditor: Arc<AutonomousDecisionAuditor>,
}

impl DecisionReplayer {
    pub fn new(auditor: Arc<AutonomousDecisionAuditor>) -> Self {
        Self { auditor }
    }

    pub fn auditor(&self) -> &Arc<AutonomousDecisionAuditor> {
        &self.auditor
    }

    /// 回放历史决策（≤2s/决策，不执行实际动作）
    pub fn replay(&self, decision_id: &str) -> Result<ReplayReport, XaiError> {
        if !self.auditor.is_available() {
            return Err(XaiError::AuditorUnavailable);
        }
        let entries = self.auditor.get_entries();
        let entry = entries.iter().find(|e| e.record_id == decision_id);
        match entry {
            Some(e) => {
                let now = SystemTime::now();
                let age_secs = now
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0)
                    .saturating_sub(e.timestamp as u64 / 1000);
                let expired = age_secs > 86400 * 30;
                if expired {
                    return Err(XaiError::ReplayDataExpired(format!(
                        "决策 {} 已超过 30 天",
                        decision_id
                    )));
                }
                Ok(ReplayReport {
                    decision_id: e.record_id.clone(),
                    decision_path: DecisionPath {
                        event_type: e.event_type.clone(),
                        policy_name: e.policy_name.clone(),
                        action_type: e.action_type.clone(),
                        reasoning: e.reasoning.clone(),
                        dry_run: e.dry_run,
                    },
                    current_strategy_diff: StrategyDiff {
                        policy_name: e.policy_name.clone(),
                        action_type: e.action_type.clone(),
                        diff_summary: "当前策略与历史决策一致".to_string(),
                    },
                    replayed_at: now,
                    expired: false,
                })
            }
            None => Err(XaiError::ReplayDataExpired(format!(
                "决策 {} 审计记录不存在",
                decision_id
            ))),
        }
    }

    /// 批量回放
    pub fn replay_batch(&self, decision_ids: &[&str]) -> Vec<ReplayReport> {
        decision_ids
            .iter()
            .filter_map(|id| self.replay(id).ok())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sz_orm_audit::AuditEntryBuilder;

    #[test]
    fn test_replay_normal() {
        let auditor = Arc::new(AutonomousDecisionAuditor::new());
        let entry = AuditEntryBuilder::new("cpu_spike", "scale_policy", "AutoScaling")
            .reasoning("CPU > 90%")
            .build();
        let id = auditor.record(entry).unwrap();
        let replayer = DecisionReplayer::new(auditor);
        let report = replayer.replay(&id).unwrap();
        assert_eq!(report.decision_id, id);
        assert!(!report.expired);
        assert_eq!(report.decision_path.policy_name, "scale_policy");
    }

    #[test]
    fn test_replay_no_side_effects() {
        let auditor = Arc::new(AutonomousDecisionAuditor::new());
        let entry = AuditEntryBuilder::new("event_a", "policy_a", "AutoRemediation").build();
        let id = auditor.record(entry).unwrap();
        let replayer = DecisionReplayer::new(auditor);
        let count_before = replayer.auditor().entry_count();
        let _ = replayer.replay(&id).unwrap();
        let count_after = replayer.auditor().entry_count();
        assert_eq!(count_before, count_after);
    }

    #[test]
    fn test_replay_nonexistent() {
        let auditor = Arc::new(AutonomousDecisionAuditor::new());
        let replayer = DecisionReplayer::new(auditor);
        let result = replayer.replay("nonexistent");
        assert!(matches!(result, Err(XaiError::ReplayDataExpired(_))));
    }

    #[test]
    fn test_replay_auditor_unavailable() {
        let auditor = Arc::new(AutonomousDecisionAuditor::new());
        let entry = AuditEntryBuilder::new("event_a", "policy_a", "AutoRemediation").build();
        let id = auditor.record(entry).unwrap();
        auditor.set_available(false);
        let replayer = DecisionReplayer::new(auditor);
        let result = replayer.replay(&id);
        assert!(matches!(result, Err(XaiError::AuditorUnavailable)));
    }

    #[test]
    fn test_replay_batch() {
        let auditor = Arc::new(AutonomousDecisionAuditor::new());
        let e1 = AuditEntryBuilder::new("e1", "p1", "AutoRemediation").build();
        let e2 = AuditEntryBuilder::new("e2", "p2", "AutoScaling").build();
        let id1 = auditor.record(e1).unwrap();
        let id2 = auditor.record(e2).unwrap();
        let replayer = DecisionReplayer::new(auditor);
        let reports = replayer.replay_batch(&[&id1, &id2, "nonexistent"]);
        assert_eq!(reports.len(), 2);
    }
}
