use sz_orm_audit::*;

fn make_ctx(sql: &str, user: &str, ts: i64) -> SqlAuditContext {
    SqlAuditContext {
        sql: sql.to_string(),
        user: user.to_string(),
        timestamp: ts,
    }
}

#[test]
fn test_sql_auditor_log_and_get() {
    let auditor = SqlAuditor::new();
    auditor.log(&make_ctx("SELECT * FROM users", "alice", 1000));
    auditor.log(&make_ctx("INSERT INTO logs VALUES(1)", "bob", 2000));
    let logs = auditor.get_logs();
    assert_eq!(logs.len(), 2);
    assert_eq!(logs[0].user, "alice");
    assert_eq!(logs[1].user, "bob");
}

#[test]
fn test_sql_auditor_empty() {
    let auditor = SqlAuditor::new();
    assert!(auditor.get_logs().is_empty());
}

#[test]
fn test_sql_auditor_mask_sensitive_password() {
    let auditor = SqlAuditor::new();
    let masked = auditor.mask_sensitive("WHERE password = 'secret123'");
    assert!(masked.contains("******"));
    assert!(!masked.contains("password"));
}

#[test]
fn test_sql_auditor_mask_sensitive_token() {
    let auditor = SqlAuditor::new();
    let masked = auditor.mask_sensitive("SET token = 'abc'");
    assert!(masked.contains("******"));
}

#[test]
fn test_sql_auditor_mask_sensitive_credit_card() {
    let auditor = SqlAuditor::new();
    let masked = auditor.mask_sensitive("credit_card = '1234'");
    assert!(masked.contains("******"));
}

#[test]
fn test_sql_auditor_mask_sensitive_case_insensitive() {
    let auditor = SqlAuditor::new();
    let masked = auditor.mask_sensitive("WHERE PASSWORD = 'x'");
    assert!(masked.contains("******"));
}

#[test]
fn test_sql_auditor_mask_not_in_identifier() {
    let auditor = SqlAuditor::new();
    let masked = auditor.mask_sensitive("SELECT password_hash FROM users");
    assert!(!masked.contains("******"));
    assert!(masked.contains("password_hash"));
}

#[test]
fn test_sql_auditor_mask_no_sensitive() {
    let auditor = SqlAuditor::new();
    let masked = auditor.mask_sensitive("SELECT id, name FROM users");
    assert_eq!(masked, "SELECT id, name FROM users");
}

#[test]
fn test_sql_auditor_log_masks_automatically() {
    let auditor = SqlAuditor::new();
    auditor.log(&make_ctx("WHERE password = 'secret'", "user", 1000));
    let logs = auditor.get_logs();
    assert!(logs[0].sql.contains("******"));
    assert!(!logs[0].sql.contains("password"));
}

#[test]
fn test_audit_rules_default_allows_all() {
    let rules = AuditRules::new();
    assert!(rules.should_audit("SELECT * FROM users"));
    assert!(rules.should_audit("DROP TABLE users"));
}

#[test]
fn test_audit_rules_allow_list() {
    let rules = AuditRules::new().allow("select").allow("insert");
    assert!(rules.should_audit("SELECT * FROM users"));
    assert!(rules.should_audit("INSERT INTO t VALUES(1)"));
    assert!(!rules.should_audit("DROP TABLE users"));
}

#[test]
fn test_audit_rules_deny_list() {
    let rules = AuditRules::new().deny("drop").deny("delete");
    assert!(!rules.should_audit("DROP TABLE users"));
    assert!(!rules.should_audit("DELETE FROM users"));
    assert!(rules.should_audit("SELECT * FROM users"));
}

#[test]
fn test_audit_rules_deny_overrides_allow() {
    let rules = AuditRules::new().allow("select").deny("select");
    assert!(!rules.should_audit("SELECT * FROM users"));
}

#[test]
fn test_audit_rules_case_insensitive() {
    let rules = AuditRules::new().deny("DROP");
    assert!(!rules.should_audit("drop table users"));
}

#[test]
fn test_audit_rules_counts() {
    let rules = AuditRules::new().allow("a").allow("b").deny("c");
    assert_eq!(rules.allow_count(), 2);
    assert_eq!(rules.deny_count(), 1);
}

#[test]
fn test_rotation_policy_none() {
    let policy = RotationPolicy::none();
    assert_eq!(policy.max_entries, 0);
    assert_eq!(policy.max_age_ms, 0);
}

#[test]
fn test_rotation_policy_by_size() {
    let policy = RotationPolicy::by_size(100);
    assert_eq!(policy.max_entries, 100);
    assert_eq!(policy.max_age_ms, 0);
}

#[test]
fn test_rotation_policy_by_age() {
    let policy = RotationPolicy::by_age(60000);
    assert_eq!(policy.max_entries, 0);
    assert_eq!(policy.max_age_ms, 60000);
}

#[test]
fn test_rotation_policy_by_size_and_age() {
    let policy = RotationPolicy::by_size_and_age(100, 60000);
    assert_eq!(policy.max_entries, 100);
    assert_eq!(policy.max_age_ms, 60000);
}

#[test]
fn test_rotating_auditor_basic() {
    let auditor = RotatingAuditor::with_max_entries(10);
    assert!(auditor.is_empty());
    auditor.log(&make_ctx("SELECT 1", "user", 1000));
    assert_eq!(auditor.len(), 1);
    assert!(!auditor.is_empty());
}

#[test]
fn test_rotating_auditor_rotation_by_size() {
    let auditor = RotatingAuditor::with_max_entries(3);
    auditor.log(&make_ctx("SQL1", "u", 1000));
    auditor.log(&make_ctx("SQL2", "u", 1000));
    auditor.log(&make_ctx("SQL3", "u", 1000));
    auditor.log(&make_ctx("SQL4", "u", 1000));
    assert_eq!(auditor.rotation_count(), 1);
    assert_eq!(auditor.len(), 1);
}

#[test]
fn test_rotating_auditor_with_rules() {
    let rules = AuditRules::new().deny("drop");
    let auditor = RotatingAuditor::new(RotationPolicy::none(), rules);
    let logged = auditor.log(&make_ctx("DROP TABLE x", "u", 1000));
    assert!(!logged);
    assert_eq!(auditor.len(), 0);
    let logged2 = auditor.log(&make_ctx("SELECT 1", "u", 1000));
    assert!(logged2);
    assert_eq!(auditor.len(), 1);
}

#[test]
fn test_rotating_auditor_manual_rotate() {
    let auditor = RotatingAuditor::with_max_entries(100);
    auditor.log(&make_ctx("SQL1", "u", 1000));
    auditor.log(&make_ctx("SQL2", "u", 1000));
    let cleared = auditor.rotate();
    assert_eq!(cleared, 2);
    assert!(auditor.is_empty());
    assert_eq!(auditor.rotation_count(), 1);
}

#[test]
fn test_async_audit_writer() {
    let writer = AsyncAuditWriter::new();
    writer.log(&make_ctx("SELECT 1", "user", 1000)).unwrap();
    writer.log(&make_ctx("SELECT 2", "user", 2000)).unwrap();
    let logs = writer.shutdown().unwrap();
    assert_eq!(logs.len(), 2);
}

#[test]
fn test_async_audit_writer_masks_sensitive() {
    let writer = AsyncAuditWriter::new();
    writer
        .log(&make_ctx("WHERE password = 'x'", "user", 1000))
        .unwrap();
    let logs = writer.shutdown().unwrap();
    assert!(logs[0].sql.contains("******"));
}

#[test]
fn test_async_audit_writer_double_shutdown() {
    let writer = AsyncAuditWriter::new();
    writer.shutdown().unwrap();
    let result = writer.shutdown();
    assert!(result.is_err());
}

#[test]
fn test_audit_query_by_user() {
    let query = AuditQuery::new().by_user("alice");
    assert_eq!(query.user, Some("alice".to_string()));
}

#[test]
fn test_audit_query_by_time_range() {
    let query = AuditQuery::new().by_time_range(1000, 2000);
    assert_eq!(query.from_ts, Some(1000));
    assert_eq!(query.to_ts, Some(2000));
}

#[test]
fn test_audit_query_by_sql_contains() {
    let query = AuditQuery::new().by_sql_contains("INSERT");
    assert_eq!(query.sql_contains, Some("INSERT".to_string()));
}

#[test]
fn test_audit_query_with_limit() {
    let query = AuditQuery::new().with_limit(10);
    assert_eq!(query.limit, 10);
}

#[test]
fn test_audit_query_filter() {
    let logs = vec![
        make_ctx("SELECT 1", "alice", 1000),
        make_ctx("INSERT 1", "bob", 2000),
        make_ctx("SELECT 2", "alice", 3000),
    ];
    let query = AuditQuery::new().by_user("alice");
    let filtered = query.filter(&logs);
    assert_eq!(filtered.len(), 2);
}

#[test]
fn test_audit_query_filter_with_limit() {
    let logs = vec![
        make_ctx("SQL1", "u", 1000),
        make_ctx("SQL2", "u", 2000),
        make_ctx("SQL3", "u", 3000),
    ];
    let query = AuditQuery::new().with_limit(2);
    let filtered = query.filter(&logs);
    assert_eq!(filtered.len(), 2);
}

#[test]
fn test_audit_query_filter_time_range() {
    let logs = vec![
        make_ctx("SQL1", "u", 1000),
        make_ctx("SQL2", "u", 2000),
        make_ctx("SQL3", "u", 3000),
    ];
    let query = AuditQuery::new().by_time_range(1500, 2500);
    let filtered = query.filter(&logs);
    assert_eq!(filtered.len(), 1);
}

#[test]
fn test_audit_query_filter_sql_contains() {
    let logs = vec![
        make_ctx("SELECT * FROM t", "u", 1000),
        make_ctx("INSERT INTO t", "u", 2000),
    ];
    let query = AuditQuery::new().by_sql_contains("insert");
    let filtered = query.filter(&logs);
    assert_eq!(filtered.len(), 1);
}
