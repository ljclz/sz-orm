//! TASK-020: PermissionAuditor 单元测试
//!
//! 验证权限审计：识别过度授权 + 建议最小权限。

use sz_orm_ai::permission_auditor::{
    DbAccount, PermissionAuditor, PermissionIssueSeverity, QueryUsage,
};

#[test]
fn test_audit_root_account() {
    let auditor = PermissionAuditor::new();
    let account = DbAccount::new("root").with_roles(vec!["root".to_string()]);
    let usage = QueryUsage::from_sql("SELECT id, name FROM users WHERE id = 1");

    let result = auditor.audit_permissions(&account, &usage);

    let super_finding = result
        .findings
        .iter()
        .find(|f| f.title == "使用超级用户账户");
    assert!(super_finding.is_some());
    assert_eq!(
        super_finding.unwrap().severity,
        PermissionIssueSeverity::Critical
    );
    assert!(result.suggested_readonly_account.is_some());
}

#[test]
fn test_audit_normal_account() {
    let auditor = PermissionAuditor::new();
    let account = DbAccount::new("app_user")
        .with_roles(vec!["read_only".to_string()])
        .with_privileges(vec!["SELECT".to_string()]);
    let usage = QueryUsage::from_sql("SELECT id, name FROM users WHERE id = 1");

    let result = auditor.audit_permissions(&account, &usage);

    let super_finding = result
        .findings
        .iter()
        .find(|f| f.title == "使用超级用户账户");
    assert!(super_finding.is_none());
    assert!(result.suggested_readonly_account.is_none());
}

#[test]
fn test_audit_excessive_privileges() {
    let auditor = PermissionAuditor::new();
    let account = DbAccount::new("app_user").with_privileges(vec![
        "SELECT".to_string(),
        "DROP".to_string(),
        "ALTER".to_string(),
    ]);
    let usage = QueryUsage::from_sql("SELECT id FROM users");

    let result = auditor.audit_permissions(&account, &usage);

    let excessive = result.findings.iter().find(|f| f.title == "过度授权");
    assert!(excessive.is_some());
    assert_eq!(
        excessive.unwrap().severity,
        PermissionIssueSeverity::Warning
    );
}

#[test]
fn test_audit_select_star() {
    let auditor = PermissionAuditor::new();
    let account = DbAccount::new("app_user");
    let usage = QueryUsage::from_sql("SELECT * FROM users");

    let result = auditor.audit_permissions(&account, &usage);

    let star_finding = result.findings.iter().find(|f| f.title == "SELECT * 查询");
    assert!(star_finding.is_some());
}

#[test]
fn test_audit_missing_where() {
    let auditor = PermissionAuditor::new();
    let account = DbAccount::new("app_user");
    let usage = QueryUsage::from_sql("SELECT id FROM users");

    let result = auditor.audit_permissions(&account, &usage);

    let where_finding = result
        .findings
        .iter()
        .find(|f| f.title == "无 WHERE 条件的全表查询");
    assert!(where_finding.is_some());
    assert_eq!(
        where_finding.unwrap().severity,
        PermissionIssueSeverity::Info
    );
}

#[test]
fn test_minimal_privileges() {
    let auditor = PermissionAuditor::new();
    let account = DbAccount::new("app_user");
    let usage = QueryUsage::from_sql("SELECT id, name FROM users WHERE id = 1");

    let result = auditor.audit_permissions(&account, &usage);

    assert!(result.minimal_privileges.contains(&"SELECT".to_string()));
    assert!(result
        .minimal_privileges
        .iter()
        .any(|p| p.contains("users.id")));
    assert!(result
        .minimal_privileges
        .iter()
        .any(|p| p.contains("users.name")));
}

#[test]
fn test_security_score_super_user() {
    let auditor = PermissionAuditor::new();
    let root = DbAccount::new("root").with_roles(vec!["root".to_string()]);
    let normal = DbAccount::new("app_user");
    let usage = QueryUsage::from_sql("SELECT id FROM users WHERE id = 1");

    let root_result = auditor.audit_permissions(&root, &usage);
    let normal_result = auditor.audit_permissions(&normal, &usage);

    assert!(root_result.security_score < normal_result.security_score);
    assert!(root_result.security_score < 60.0);
}

#[test]
fn test_suggested_readonly_account() {
    let auditor = PermissionAuditor::new();
    let account = DbAccount::new("root").with_roles(vec!["root".to_string()]);
    let usage = QueryUsage::from_sql("SELECT id FROM users WHERE id = 1");

    let result = auditor.audit_permissions(&account, &usage);

    assert!(result.suggested_readonly_account.is_some());
    let suggested = result.suggested_readonly_account.unwrap();
    assert!(suggested.contains("readonly"));
}

#[test]
fn test_query_usage_from_sql() {
    let usage = QueryUsage::from_sql("SELECT id, name, email FROM users WHERE id = 1");

    assert!(!usage.uses_select_star);
    assert!(usage.has_where);
    assert!(!usage.has_join);
    assert!(usage.tables.contains(&"users".to_string()));
    assert_eq!(usage.referenced_columns.len(), 3);
}

#[test]
fn test_query_usage_select_star() {
    let usage =
        QueryUsage::from_sql("SELECT * FROM users JOIN orders ON users.id = orders.user_id");

    assert!(usage.uses_select_star);
    assert!(usage.has_join);
    assert!(usage.tables.contains(&"users".to_string()));
    assert!(usage.tables.contains(&"orders".to_string()));
    assert!(usage.referenced_columns.is_empty());
}

#[test]
fn test_db_account_builder() {
    let account = DbAccount::new("app_user")
        .with_roles(vec!["reader".to_string(), "analyst".to_string()])
        .with_privileges(vec!["SELECT".to_string(), "SHOW".to_string()]);

    assert_eq!(account.username, "app_user");
    assert!(!account.is_super_user);
    assert_eq!(account.roles.len(), 2);
    assert_eq!(account.granted_privileges.len(), 2);
}

#[test]
fn test_audit_clean_query() {
    let auditor = PermissionAuditor::new();
    let account = DbAccount::new("app_user").with_privileges(vec!["SELECT".to_string()]);
    let usage = QueryUsage::from_sql("SELECT id, name FROM users WHERE id = 1");

    let result = auditor.audit_permissions(&account, &usage);

    let critical_count = result
        .findings
        .iter()
        .filter(|f| f.severity == PermissionIssueSeverity::Critical)
        .count();
    assert_eq!(critical_count, 0);
    assert!(result.security_score > 80.0);
}
