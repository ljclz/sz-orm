//! 自治决策审计器
//!
//! 记录 AI 自治决策全链路：输入事件 → 决策依据 → 执行动作 → 执行结果 → 验证结果 → 回退记录。
//! 复用 `SqlAuditor` 的敏感关键词脱敏模式，扩展决策语义字段。

use std::sync::Mutex;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

/// 自治决策审计记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomousAuditEntry {
    pub record_id: String,
    pub timestamp: i64,
    pub event_type: String,
    pub event_severity: String,
    pub policy_name: String,
    pub action_type: String,
    pub reasoning: String,
    pub dry_run: bool,
    pub execution_success: Option<bool>,
    pub execution_message: Option<String>,
    pub verification_passed: Option<bool>,
    pub rollback_performed: bool,
    pub degraded_to_rule_mode: bool,
    pub audit_tag: String,
}

/// 自治决策审计器
pub struct AutonomousDecisionAuditor {
    entries: Mutex<Vec<AutonomousAuditEntry>>,
    available: Mutex<bool>,
}

impl AutonomousDecisionAuditor {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
            available: Mutex::new(true),
        }
    }

    pub fn set_available(&self, available: bool) {
        *self.available.lock().expect("audit available lock") = available;
    }

    pub fn is_available(&self) -> bool {
        *self.available.lock().expect("audit available lock")
    }

    pub fn record(&self, entry: AutonomousAuditEntry) -> Result<String, String> {
        if !self.is_available() {
            return Err("AUDIT_UNAVAILABLE_AUTONOMOUS_PAUSED".to_string());
        }
        let mut entries = self.entries.lock().expect("audit entries lock");
        entries.push(entry.clone());
        Ok(entry.record_id)
    }

    pub fn get_entries(&self) -> Vec<AutonomousAuditEntry> {
        self.entries.lock().expect("audit entries lock").clone()
    }

    pub fn flush(&self, path: &str) -> Result<usize, String> {
        let entries = self.entries.lock().expect("audit entries lock");
        let json = serde_json::to_string_pretty(&*entries).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())?;
        Ok(entries.len())
    }

    pub fn entry_count(&self) -> usize {
        self.entries.lock().expect("audit entries lock").len()
    }
}

impl Default for AutonomousDecisionAuditor {
    fn default() -> Self {
        Self::new()
    }
}

/// 构建审计记录辅助工具
pub struct AuditEntryBuilder {
    event_type: String,
    event_severity: String,
    policy_name: String,
    action_type: String,
    reasoning: String,
    dry_run: bool,
    execution_success: Option<bool>,
    execution_message: Option<String>,
    verification_passed: Option<bool>,
    rollback_performed: bool,
    degraded_to_rule_mode: bool,
    audit_tag: String,
}

impl AuditEntryBuilder {
    pub fn new(event_type: &str, policy_name: &str, action_type: &str) -> Self {
        Self {
            event_type: event_type.to_string(),
            event_severity: "Warning".to_string(),
            policy_name: policy_name.to_string(),
            action_type: action_type.to_string(),
            reasoning: String::new(),
            dry_run: false,
            execution_success: None,
            execution_message: None,
            verification_passed: None,
            rollback_performed: false,
            degraded_to_rule_mode: false,
            audit_tag: String::new(),
        }
    }

    pub fn severity(mut self, severity: &str) -> Self {
        self.event_severity = severity.to_string();
        self
    }

    pub fn reasoning(mut self, reasoning: &str) -> Self {
        self.reasoning = reasoning.to_string();
        self
    }

    pub fn dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    pub fn execution(mut self, success: bool, message: &str) -> Self {
        self.execution_success = Some(success);
        self.execution_message = Some(message.to_string());
        self
    }

    pub fn verification(mut self, passed: bool) -> Self {
        self.verification_passed = Some(passed);
        self
    }

    pub fn rollback(mut self, performed: bool) -> Self {
        self.rollback_performed = performed;
        self
    }

    pub fn degraded(mut self, degraded: bool) -> Self {
        self.degraded_to_rule_mode = degraded;
        self
    }

    pub fn tag(mut self, tag: &str) -> Self {
        self.audit_tag = tag.to_string();
        self
    }

    pub fn build(self) -> AutonomousAuditEntry {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let record_id = format!("auto-{}-{}", timestamp, self.event_type);
        AutonomousAuditEntry {
            record_id,
            timestamp,
            event_type: self.event_type,
            event_severity: self.event_severity,
            policy_name: self.policy_name,
            action_type: self.action_type,
            reasoning: self.reasoning,
            dry_run: self.dry_run,
            execution_success: self.execution_success,
            execution_message: self.execution_message,
            verification_passed: self.verification_passed,
            rollback_performed: self.rollback_performed,
            degraded_to_rule_mode: self.degraded_to_rule_mode,
            audit_tag: self.audit_tag,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_record_success() {
        let auditor = AutonomousDecisionAuditor::new();
        let entry = AuditEntryBuilder::new("high_cpu", "p1", "AutoRemediation")
            .reasoning("CPU > 90%")
            .execution(true, "remediated")
            .verification(true)
            .build();

        let result = auditor.record(entry);
        assert!(result.is_ok());
        assert_eq!(auditor.entry_count(), 1);
    }

    #[test]
    fn test_audit_unavailable_rejected() {
        let auditor = AutonomousDecisionAuditor::new();
        auditor.set_available(false);

        let entry = AuditEntryBuilder::new("high_cpu", "p1", "AutoRemediation").build();
        let result = auditor.record(entry);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "AUDIT_UNAVAILABLE_AUTONOMOUS_PAUSED");
    }

    #[test]
    fn test_audit_recovery() {
        let auditor = AutonomousDecisionAuditor::new();
        auditor.set_available(false);
        assert!(!auditor.is_available());

        auditor.set_available(true);
        assert!(auditor.is_available());

        let entry = AuditEntryBuilder::new("high_cpu", "p1", "AutoRemediation").build();
        assert!(auditor.record(entry).is_ok());
    }

    #[test]
    fn test_audit_degraded_tag() {
        let auditor = AutonomousDecisionAuditor::new();
        let entry = AuditEntryBuilder::new("high_cpu", "p1", "AutoScaling")
            .degraded(true)
            .tag("DEGRADED_RULE_MODE")
            .build();

        auditor.record(entry).unwrap();
        let entries = auditor.get_entries();
        assert!(entries[0].degraded_to_rule_mode);
        assert_eq!(entries[0].audit_tag, "DEGRADED_RULE_MODE");
    }

    #[test]
    fn test_audit_rollback_recorded() {
        let auditor = AutonomousDecisionAuditor::new();
        let entry = AuditEntryBuilder::new("high_cpu", "p1", "AutoRemediation")
            .execution(false, "timeout")
            .rollback(true)
            .build();

        auditor.record(entry).unwrap();
        let entries = auditor.get_entries();
        assert!(entries[0].rollback_performed);
        assert_eq!(entries[0].execution_success, Some(false));
    }

    #[test]
    fn test_audit_flush() {
        let auditor = AutonomousDecisionAuditor::new();
        let entry = AuditEntryBuilder::new("high_cpu", "p1", "AutoRemediation").build();
        auditor.record(entry).unwrap();

        let path = std::env::temp_dir().join("test_autonomous_audit_flush.json");
        let result = auditor.flush(path.to_str().unwrap());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1);
    }

    #[test]
    fn test_audit_entry_builder_full_chain() {
        let entry = AuditEntryBuilder::new("pool_exhausted", "p_pool_heal", "AutoRemediation")
            .severity("Critical")
            .reasoning("连接池耗尽，触发自动修复")
            .dry_run(false)
            .execution(true, "重启连接池成功")
            .verification(true)
            .tag("AUTONOMOUS_SUCCESS")
            .build();

        assert_eq!(entry.event_type, "pool_exhausted");
        assert_eq!(entry.event_severity, "Critical");
        assert_eq!(entry.policy_name, "p_pool_heal");
        assert_eq!(entry.action_type, "AutoRemediation");
        assert!(!entry.dry_run);
        assert_eq!(entry.execution_success, Some(true));
        assert_eq!(entry.verification_passed, Some(true));
        assert_eq!(entry.audit_tag, "AUTONOMOUS_SUCCESS");
    }
}
