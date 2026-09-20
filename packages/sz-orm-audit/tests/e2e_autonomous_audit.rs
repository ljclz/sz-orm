//! 端到端测试：自治决策审计器
//!
//! 验证 AutonomousDecisionAuditor 的 record/get_entries/entry_count/flush/set_available 全链路。

use sz_orm_audit::{AuditEntryBuilder, AutonomousDecisionAuditor};

#[test]
fn e2e_autonomous_audit_record_and_query() {
    let auditor = AutonomousDecisionAuditor::new();
    let entry = AuditEntryBuilder::new("high_cpu", "p_auto_remediation", "AutoRemediation")
        .severity("Critical")
        .reasoning("CPU > 90% 持续 5 分钟")
        .execution(true, "已扩容 2 实例")
        .verification(true)
        .tag("AUTONOMOUS_SUCCESS")
        .build();

    let record_id = auditor.record(entry).expect("record 应成功");
    assert!(!record_id.is_empty(), "record_id 不应为空");
    assert_eq!(auditor.entry_count(), 1);

    let entries = auditor.get_entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].event_type, "high_cpu");
    assert_eq!(entries[0].policy_name, "p_auto_remediation");
    assert_eq!(entries[0].action_type, "AutoRemediation");
    assert_eq!(entries[0].execution_success, Some(true));
    assert_eq!(entries[0].verification_passed, Some(true));
}

#[test]
fn e2e_autonomous_audit_pause_and_resume() {
    let auditor = AutonomousDecisionAuditor::new();
    assert!(auditor.is_available());

    auditor.set_available(false);
    assert!(!auditor.is_available());

    let entry = AuditEntryBuilder::new("high_mem", "p_auto_scaling", "AutoScaling").build();
    let result = auditor.record(entry);
    assert!(result.is_err(), "暂停状态下应拒绝记录");
    assert_eq!(result.unwrap_err(), "AUDIT_UNAVAILABLE_AUTONOMOUS_PAUSED");

    auditor.set_available(true);
    let entry2 = AuditEntryBuilder::new("high_mem", "p_auto_scaling", "AutoScaling").build();
    assert!(auditor.record(entry2).is_ok(), "恢复后应接受记录");
    assert_eq!(auditor.entry_count(), 1);
}

#[test]
fn e2e_autonomous_audit_rollback_and_degraded() {
    let auditor = AutonomousDecisionAuditor::new();
    let entry = AuditEntryBuilder::new("pool_exhausted", "p_pool_heal", "AutoRemediation")
        .execution(false, "修复超时")
        .rollback(true)
        .degraded(true)
        .tag("DEGRADED_RULE_MODE")
        .build();

    auditor.record(entry).unwrap();
    let entries = auditor.get_entries();
    assert!(entries[0].rollback_performed, "应记录回退已执行");
    assert!(entries[0].degraded_to_rule_mode, "应记录降级到规则模式");
    assert_eq!(entries[0].execution_success, Some(false));
    assert_eq!(entries[0].audit_tag, "DEGRADED_RULE_MODE");
}

#[test]
fn e2e_autonomous_audit_flush_and_cleanup() {
    let auditor = AutonomousDecisionAuditor::new();
    let entry = AuditEntryBuilder::new("disk_full", "p_disk_cleanup", "AutoRemediation")
        .execution(true, "已清理临时文件")
        .verification(true)
        .build();
    auditor.record(entry).unwrap();

    let path = std::env::temp_dir().join("e2e_autonomous_audit_flush_test.json");
    let count = auditor.flush(path.to_str().unwrap()).expect("flush 应成功");
    assert_eq!(count, 1, "应 flush 1 条记录");
    assert!(path.exists(), "文件应已写入");

    let content = std::fs::read_to_string(&path).expect("应能读取文件");
    assert!(content.contains("disk_full"), "文件应包含事件类型");
    assert!(content.contains("p_disk_cleanup"), "文件应包含策略名");

    std::fs::remove_file(&path).expect("测试结束后必须删除文件");
    assert!(!path.exists(), "文件应已删除");
}
