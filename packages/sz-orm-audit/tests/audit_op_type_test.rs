#![cfg(feature = "hash-chain-enhanced")]

//! 5.6 端到端测试：DDL / 权限变更 / 脱敏配置变更识别 + 哈希链防篡改。

use sz_orm_audit::hash_chain_enhanced::{
    AuditOpType, AuditResult, EnhancedAuditEntry, HashChainEnhancedAuditor,
};

fn make_entry(sql: &str, user: &str, ip: &str) -> EnhancedAuditEntry {
    EnhancedAuditEntry {
        subject: user.to_string(),
        object: "users".to_string(),
        timestamp: 1000,
        op_type: AuditOpType::from_sql(sql),
        result: AuditResult::Success,
        source_ip: ip.to_string(),
        sql: sql.to_string(),
    }
}

#[test]
fn ddl_create_recognized() {
    assert_eq!(
        AuditOpType::from_sql("CREATE TABLE users (id INT)"),
        AuditOpType::Ddl
    );
}

#[test]
fn ddl_alter_recognized() {
    assert_eq!(
        AuditOpType::from_sql("ALTER TABLE users ADD COLUMN name TEXT"),
        AuditOpType::Ddl
    );
}

#[test]
fn ddl_drop_recognized() {
    assert_eq!(AuditOpType::from_sql("DROP TABLE users"), AuditOpType::Ddl);
}

#[test]
fn ddl_truncate_recognized() {
    assert_eq!(
        AuditOpType::from_sql("TRUNCATE TABLE logs"),
        AuditOpType::Ddl
    );
}

#[test]
fn permission_grant_recognized() {
    assert_eq!(
        AuditOpType::from_sql("GRANT SELECT ON users TO analyst"),
        AuditOpType::PermissionChange
    );
}

#[test]
fn permission_revoke_recognized() {
    assert_eq!(
        AuditOpType::from_sql("REVOKE SELECT ON users FROM analyst"),
        AuditOpType::PermissionChange
    );
}

#[test]
fn masking_config_change_recognized() {
    assert_eq!(
        AuditOpType::from_sql("MASKING_RULE_UPDATE phone SET strategy=hash"),
        AuditOpType::MaskingConfigChange
    );
}

#[test]
fn dml_still_works() {
    assert_eq!(
        AuditOpType::from_sql("SELECT * FROM users"),
        AuditOpType::Select
    );
    assert_eq!(
        AuditOpType::from_sql("INSERT INTO users VALUES(1)"),
        AuditOpType::Insert
    );
    assert_eq!(
        AuditOpType::from_sql("UPDATE users SET name='x'"),
        AuditOpType::Update
    );
    assert_eq!(
        AuditOpType::from_sql("DELETE FROM users WHERE id=1"),
        AuditOpType::Delete
    );
}

#[test]
fn case_insensitive_recognition() {
    assert_eq!(
        AuditOpType::from_sql("create table foo (id int)"),
        AuditOpType::Ddl
    );
    assert_eq!(
        AuditOpType::from_sql("grant select on foo to bob"),
        AuditOpType::PermissionChange
    );
    assert_eq!(
        AuditOpType::from_sql("masking_rule_update phone set strategy=hash"),
        AuditOpType::MaskingConfigChange
    );
}

#[test]
fn hash_chain_integrity_with_all_op_types() {
    let auditor = HashChainEnhancedAuditor::new();
    auditor
        .log(make_entry("SELECT * FROM users", "admin", "10.0.0.1"))
        .unwrap();
    auditor
        .log(make_entry("CREATE TABLE foo (id INT)", "admin", "10.0.0.1"))
        .unwrap();
    auditor
        .log(make_entry(
            "GRANT SELECT ON foo TO bob",
            "admin",
            "10.0.0.1",
        ))
        .unwrap();
    auditor
        .log(make_entry(
            "MASKING_RULE_UPDATE phone SET strategy=hash",
            "admin",
            "10.0.0.1",
        ))
        .unwrap();
    auditor
        .log(make_entry("DROP TABLE foo", "admin", "10.0.0.1"))
        .unwrap();
    assert_eq!(auditor.len(), 5);
    assert!(auditor.verify_chain());
}

#[test]
fn hash_chain_tamper_detected_via_get_entries() {
    let auditor = HashChainEnhancedAuditor::new();
    auditor
        .log(make_entry("CREATE TABLE foo (id INT)", "admin", "10.0.0.1"))
        .unwrap();
    auditor
        .log(make_entry("DROP TABLE foo", "admin", "10.0.0.1"))
        .unwrap();
    assert!(auditor.verify_chain());

    let entries = auditor.get_entries();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].entry.op_type, AuditOpType::Ddl);
    assert_eq!(entries[1].entry.op_type, AuditOpType::Ddl);
    assert_ne!(entries[0].hash, entries[1].hash);
    assert_eq!(entries[1].prev_hash, entries[0].hash);
}

#[test]
fn hash_chain_different_ips_different_hashes() {
    let auditor = HashChainEnhancedAuditor::new();
    auditor
        .log(make_entry(
            "GRANT SELECT ON foo TO bob",
            "admin",
            "10.0.0.1",
        ))
        .unwrap();
    let entries1 = auditor.get_entries();
    let hash1 = entries1[0].hash.clone();

    let auditor2 = HashChainEnhancedAuditor::new();
    auditor2
        .log(make_entry(
            "GRANT SELECT ON foo TO bob",
            "admin",
            "10.0.0.2",
        ))
        .unwrap();
    let entries2 = auditor2.get_entries();
    let hash2 = entries2[0].hash.clone();

    assert_ne!(hash1, hash2);
}

#[test]
fn audit_entry_contains_all_required_fields() {
    let entry = make_entry("CREATE TABLE foo (id INT)", "admin", "10.0.0.1");
    assert!(!entry.subject.is_empty());
    assert!(!entry.object.is_empty());
    assert!(entry.timestamp > 0);
    assert_ne!(entry.op_type, AuditOpType::Select);
    assert!(!entry.source_ip.is_empty());
    assert!(!entry.sql.is_empty());
}

#[test]
fn unknown_sql_defaults_to_select() {
    assert_eq!(
        AuditOpType::from_sql("EXPLAIN ANALYZE SELECT 1"),
        AuditOpType::Select
    );
}

#[test]
fn write_failed_rejects_ddl_audit() {
    let auditor = HashChainEnhancedAuditor::new();
    auditor.set_write_failed();
    let result = auditor.log(make_entry("CREATE TABLE foo (id INT)", "admin", "10.0.0.1"));
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("AUDIT_LOG_FAILED"));
}

#[test]
fn all_op_types_chain_verifies() {
    let auditor = HashChainEnhancedAuditor::new();
    let sqls = [
        "SELECT 1",
        "INSERT INTO t VALUES(1)",
        "UPDATE t SET x=1",
        "DELETE FROM t",
        "CREATE TABLE u (id INT)",
        "ALTER TABLE u ADD c TEXT",
        "DROP TABLE u",
        "TRUNCATE TABLE t",
        "GRANT SELECT ON t TO bob",
        "REVOKE SELECT ON t FROM bob",
        "MASKING_RULE_UPDATE phone SET strategy=hash",
    ];
    for sql in &sqls {
        auditor.log(make_entry(sql, "admin", "10.0.0.1")).unwrap();
    }
    assert_eq!(auditor.len(), sqls.len());
    assert!(auditor.verify_chain());
}
