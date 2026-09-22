//! 决策解释器：对历史自治决策产出人类可理解解释报告

use std::sync::Arc;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use sz_orm_audit::AutonomousDecisionAuditor;

use super::XaiError;

/// XAI 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XaiConfig {
    pub report_version: String,
    pub desensitize_keywords: Vec<String>,
    pub enable_effect_attribution: bool,
}

impl Default for XaiConfig {
    fn default() -> Self {
        Self {
            report_version: "1.0".to_string(),
            desensitize_keywords: vec![
                "password".to_string(),
                "token".to_string(),
                "secret".to_string(),
                "key".to_string(),
                "credential".to_string(),
            ],
            enable_effect_attribution: true,
        }
    }
}

/// 决策依据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionBasis {
    pub policy_name: String,
    pub event_type: String,
    pub event_severity: String,
    pub reasoning: String,
}

/// 候选对比项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateComparison {
    pub action_type: String,
    pub dry_run: bool,
    pub rollback_performed: bool,
}

/// 效果归因
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectAttribution {
    pub execution_success: Option<bool>,
    pub verification_passed: Option<bool>,
    pub degraded_to_rule_mode: bool,
}

/// 决策解释报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplanationReport {
    pub report_id: String,
    pub decision_id: String,
    pub decision_basis: DecisionBasis,
    pub candidate_comparison: CandidateComparison,
    pub effect_attribution: EffectAttribution,
    pub desensitized: bool,
    pub report_version: String,
    pub generated_at: SystemTime,
    pub incomplete: bool,
}

/// 决策解释器
pub struct DecisionExplainer {
    auditor: Arc<AutonomousDecisionAuditor>,
    config: XaiConfig,
}

impl DecisionExplainer {
    pub fn new(auditor: Arc<AutonomousDecisionAuditor>, config: XaiConfig) -> Self {
        Self { auditor, config }
    }

    pub fn auditor(&self) -> &Arc<AutonomousDecisionAuditor> {
        &self.auditor
    }

    /// 生成决策解释报告（≤200ms P99）
    pub fn explain(&self, decision_id: &str) -> Result<ExplanationReport, XaiError> {
        if !self.auditor.is_available() {
            return Err(XaiError::AuditorUnavailable);
        }
        let entries = self.auditor.get_entries();
        let entry = entries.iter().find(|e| e.record_id == decision_id);
        match entry {
            Some(e) => Ok(self.build_report(e)),
            None => Err(XaiError::ExplanationIncomplete(format!(
                "决策 {} 审计记录缺失",
                decision_id
            ))),
        }
    }

    /// 批量解释
    pub fn explain_batch(&self, decision_ids: &[&str]) -> Vec<ExplanationReport> {
        decision_ids
            .iter()
            .filter_map(|id| self.explain(id).ok())
            .collect()
    }

    fn build_report(&self, entry: &sz_orm_audit::AutonomousAuditEntry) -> ExplanationReport {
        let reasoning = self.desensitize(&entry.reasoning);
        ExplanationReport {
            report_id: format!("rpt-{}", entry.record_id),
            decision_id: entry.record_id.clone(),
            decision_basis: DecisionBasis {
                policy_name: entry.policy_name.clone(),
                event_type: entry.event_type.clone(),
                event_severity: entry.event_severity.clone(),
                reasoning,
            },
            candidate_comparison: CandidateComparison {
                action_type: entry.action_type.clone(),
                dry_run: entry.dry_run,
                rollback_performed: entry.rollback_performed,
            },
            effect_attribution: EffectAttribution {
                execution_success: entry.execution_success,
                verification_passed: entry.verification_passed,
                degraded_to_rule_mode: entry.degraded_to_rule_mode,
            },
            desensitized: true,
            report_version: self.config.report_version.clone(),
            generated_at: SystemTime::now(),
            incomplete: false,
        }
    }

    fn desensitize(&self, text: &str) -> String {
        let mut result = text.to_string();
        for keyword in &self.config.desensitize_keywords {
            let lower = result.to_lowercase();
            if lower.contains(keyword) {
                result = result.replace(keyword, &format!("***{}***", keyword));
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sz_orm_audit::AuditEntryBuilder;

    fn make_auditor_with_entry() -> (Arc<AutonomousDecisionAuditor>, String) {
        let auditor = Arc::new(AutonomousDecisionAuditor::new());
        let entry = AuditEntryBuilder::new("latency_spike", "auto_scale_up", "AutoScaling")
            .severity("Critical")
            .reasoning("CPU 超过 90% 阈值，触发自动扩容")
            .build();
        let id = auditor.record(entry).unwrap();
        (auditor, id)
    }

    #[test]
    fn test_explain_normal() {
        let (auditor, id) = make_auditor_with_entry();
        let explainer = DecisionExplainer::new(auditor, XaiConfig::default());
        let report = explainer.explain(&id).unwrap();
        assert!(!report.incomplete);
        assert_eq!(report.decision_id, id);
        assert_eq!(report.decision_basis.policy_name, "auto_scale_up");
        assert!(report.desensitized);
    }

    #[test]
    fn test_explain_desensitize() {
        let auditor = Arc::new(AutonomousDecisionAuditor::new());
        let entry = AuditEntryBuilder::new("auth_failure", "auto_remediation", "AutoRemediation")
            .reasoning("password 重置触发，secret 已轮换")
            .build();
        let id = auditor.record(entry).unwrap();
        let explainer = DecisionExplainer::new(auditor, XaiConfig::default());
        let report = explainer.explain(&id).unwrap();
        assert!(report.decision_basis.reasoning.contains("***password***"));
        assert!(report.decision_basis.reasoning.contains("***secret***"));
    }

    #[test]
    fn test_explain_idempotent() {
        let (auditor, id) = make_auditor_with_entry();
        let explainer = DecisionExplainer::new(auditor, XaiConfig::default());
        let r1 = explainer.explain(&id).unwrap();
        let r2 = explainer.explain(&id).unwrap();
        assert_eq!(r1.decision_id, r2.decision_id);
        assert_eq!(r1.decision_basis.policy_name, r2.decision_basis.policy_name);
        assert_eq!(
            r1.candidate_comparison.action_type,
            r2.candidate_comparison.action_type
        );
    }

    #[test]
    fn test_explain_missing_record() {
        let auditor = Arc::new(AutonomousDecisionAuditor::new());
        let explainer = DecisionExplainer::new(auditor, XaiConfig::default());
        let result = explainer.explain("nonexistent_id");
        assert!(matches!(result, Err(XaiError::ExplanationIncomplete(_))));
    }

    #[test]
    fn test_explain_auditor_unavailable() {
        let (auditor, _) = make_auditor_with_entry();
        auditor.set_available(false);
        let explainer = DecisionExplainer::new(auditor, XaiConfig::default());
        let result = explainer.explain("any_id");
        assert!(matches!(result, Err(XaiError::AuditorUnavailable)));
    }

    #[test]
    fn test_explain_batch() {
        let auditor = Arc::new(AutonomousDecisionAuditor::new());
        let entry1 = AuditEntryBuilder::new("event_a", "policy_a", "AutoRemediation").build();
        let entry2 = AuditEntryBuilder::new("event_b", "policy_b", "AutoScaling").build();
        let id1 = auditor.record(entry1).unwrap();
        let id2 = auditor.record(entry2).unwrap();
        let explainer = DecisionExplainer::new(auditor, XaiConfig::default());
        let reports = explainer.explain_batch(&[&id1, &id2, "nonexistent"]);
        assert_eq!(reports.len(), 2);
    }
}
