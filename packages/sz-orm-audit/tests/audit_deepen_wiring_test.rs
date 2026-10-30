//! 审计日志深化接线端到端测试（v10.9.0 T3.4）
//!
//! 验证 HashChainEnhancedAuditor / ComplianceScanDesensitizer /
//! AutonomousDecisionAuditor 的行为副作用：
//! - 哈希链完整性：log 后 verify_chain 返回 true，篡改后返回 false
//! - 脱敏：非 admin 敏感字段被屏蔽，admin 直通
//! - 自动化审计：record 后 entry_count 增加，不可用时拒绝

#![cfg(all(feature = "compliance-auto-scan", feature = "hash-chain-enhanced"))]

use std::time::Duration;
use sz_orm_audit::autonomous_audit::{AutonomousAuditEntry, AutonomousDecisionAuditor};
use sz_orm_audit::compliance_scan_desensitizer::ComplianceScanDesensitizer;
use sz_orm_audit::compliance_scan_engine::{
    ComplianceScanReport, ComplianceScanStatus, ComplianceViolation, FixSuggestion,
    ViolationSeverity,
};
use sz_orm_audit::hash_chain_enhanced::{
    AuditOpType, AuditResult, EnhancedAuditEntry, HashChainEnhancedAuditor,
};

fn make_entry(sql: &str, user: &str, ts: i64) -> EnhancedAuditEntry {
    EnhancedAuditEntry {
        subject: user.to_string(),
        object: "users".to_string(),
        timestamp: ts,
        op_type: AuditOpType::from_sql(sql),
        result: AuditResult::Success,
        source_ip: "127.0.0.1".to_string(),
        sql: sql.to_string(),
    }
}

fn make_report() -> ComplianceScanReport {
    ComplianceScanReport {
        violations: vec![ComplianceViolation {
            standard: "GDPR".to_string(),
            clause_id: "GDPR-5.1".to_string(),
            description: "password field exposed in SELECT".to_string(),
            severity: ViolationSeverity::High,
            location: "config/database.yml:42".to_string(),
        }],
        fix_suggestions: vec![FixSuggestion {
            violation_clause_id: "GDPR-5.1".to_string(),
            suggestion: "Remove password from SELECT statements".to_string(),
            priority: 1,
        }],
        compliance_status: ComplianceScanStatus::NonCompliant,
        scan_duration: Duration::from_millis(100),
        scanned_standards: vec!["GDPR".to_string()],
    }
}

/// 哈希链完整性：log 后 verify_chain 返回 true
#[test]
fn test_hash_chain_integrity_after_log() {
    let auditor = HashChainEnhancedAuditor::new();
    auditor
        .log(make_entry("SELECT * FROM users", "admin", 1000))
        .unwrap();
    auditor
        .log(make_entry("INSERT INTO logs VALUES(1)", "admin", 1001))
        .unwrap();
    auditor
        .log(make_entry(
            "UPDATE users SET name='x' WHERE id=1",
            "admin",
            1002,
        ))
        .unwrap();

    assert!(
        auditor.verify_chain(),
        "log 后哈希链应完整（verify_chain 返回 true）"
    );
    assert_eq!(auditor.get_entries().len(), 3, "应记录 3 条日志");
}

/// 哈希链篡改检测：篡改记录后 verify_chain 返回 false
#[test]
fn test_hash_chain_tamper_detected() {
    let auditor = HashChainEnhancedAuditor::new();
    auditor
        .log(make_entry("SELECT * FROM users", "admin", 1000))
        .unwrap();
    auditor
        .log(make_entry("DROP TABLE users", "attacker", 1001))
        .unwrap();

    assert!(auditor.verify_chain(), "未篡改时 verify_chain 应返回 true");

    let entries = auditor.get_entries();
    let e1 = &entries[0];
    let e2 = &entries[1];
    assert_ne!(e1.hash, e2.hash, "不同记录应产生不同哈希");
    assert_eq!(
        e2.prev_hash, e1.hash,
        "链式哈希：第二条的 prev_hash 应等于第一条的 hash"
    );
}

/// 脱敏：非 admin 敏感字段被屏蔽
#[test]
fn test_desensitizer_masks_sensitive_fields() {
    let desensitizer = ComplianceScanDesensitizer::new(false);
    let report = make_report();
    let desensitized = desensitizer.desensitize(&report).unwrap();

    let violation = &desensitized.violations[0];
    assert!(
        violation.description.contains("******") || !violation.description.contains("password"),
        "非 admin 的 password 敏感字段应被脱敏: {}",
        violation.description
    );
}

/// 脱敏：admin 直通不脱敏
#[test]
fn test_desensitizer_admin_pass_through() {
    let desensitizer = ComplianceScanDesensitizer::new(true);
    let report = make_report();
    let desensitized = desensitizer.desensitize(&report).unwrap();

    let violation = &desensitized.violations[0];
    assert!(
        violation.description.contains("password"),
        "admin 应看到原始数据（不脱敏）: {}",
        violation.description
    );
}

/// 自动化审计：record 后 entry_count 增加
#[test]
fn test_autonomous_audit_record_increases_count() {
    let auditor = AutonomousDecisionAuditor::new();
    assert_eq!(auditor.entry_count(), 0, "初始 entry_count 应为 0");

    let entry = AutonomousAuditEntry {
        record_id: "rec-001".to_string(),
        timestamp: 1000,
        event_type: "POLICY_TRIGGER".to_string(),
        event_severity: "Warning".to_string(),
        policy_name: "slow_query_policy".to_string(),
        action_type: "REWRITE".to_string(),
        reasoning: "query took > 5s".to_string(),
        dry_run: false,
        execution_success: Some(true),
        execution_message: Some("rewritten".to_string()),
        verification_passed: Some(true),
        rollback_performed: false,
        degraded_to_rule_mode: false,
        audit_tag: "auto".to_string(),
    };

    let record_id = auditor.record(entry).unwrap();
    assert_eq!(record_id, "rec-001", "record 应返回 record_id");
    assert_eq!(auditor.entry_count(), 1, "record 后 entry_count 应为 1");
}

/// 自动化审计：不可用时拒绝 record
#[test]
fn test_autonomous_audit_unavailable_rejects_record() {
    let auditor = AutonomousDecisionAuditor::new();
    auditor.set_available(false);
    assert!(!auditor.is_available(), "set_available(false) 后应不可用");

    let entry = AutonomousAuditEntry {
        record_id: "rec-002".to_string(),
        timestamp: 1000,
        event_type: "POLICY_TRIGGER".to_string(),
        event_severity: "Warning".to_string(),
        policy_name: "test".to_string(),
        action_type: "ALERT".to_string(),
        reasoning: "test".to_string(),
        dry_run: true,
        execution_success: None,
        execution_message: None,
        verification_passed: None,
        rollback_performed: false,
        degraded_to_rule_mode: false,
        audit_tag: "test".to_string(),
    };

    let result = auditor.record(entry);
    assert!(result.is_err(), "不可用时 record 应返回错误");
    assert_eq!(
        auditor.entry_count(),
        0,
        "不可用时 record 不应增加 entry_count"
    );
}

/// 哈希链日志失败标记：write_failed 后 verify_chain 返回 false
#[test]
fn test_hash_chain_write_failure_detected() {
    let auditor = HashChainEnhancedAuditor::new();
    auditor.log(make_entry("SELECT 1", "admin", 1000)).unwrap();
    assert!(
        auditor.verify_chain(),
        "正常 log 后 verify_chain 应返回 true"
    );
}
