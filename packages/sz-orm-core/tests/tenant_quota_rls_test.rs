#![cfg(feature = "tenant-quota-rls-enhanced")]

use std::collections::HashMap;
use std::sync::Arc;

use sz_orm_core::tenant_quota_rls::*;
use sz_orm_core::tenant_security::{
    AuditResult, ColumnMaskingRule, MaskingFunction, ParameterizedCondition, PermissionPredicate,
    Principal, TenantAuditOperation,
};
use sz_orm_core::Value;

#[test]
fn test_quota_resource_display() {
    assert_eq!(format!("{}", QuotaResource::Connection), "connection");
    assert_eq!(format!("{}", QuotaResource::Qps), "qps");
    assert_eq!(format!("{}", QuotaResource::Storage), "storage");
}

#[test]
fn test_quota_enforce_strategy_default() {
    let s = QuotaEnforceStrategy::default();
    assert_eq!(s, QuotaEnforceStrategy::FailClose);
}

#[test]
fn test_tenant_resource_quota_new() {
    let q = TenantResourceQuota::new("tenant1");
    assert_eq!(q.tenant_id, "tenant1");
    assert!(q.max_connections.is_none());
    assert!(q.max_qps.is_none());
    assert!(q.max_storage.is_none());
}

#[test]
fn test_tenant_resource_quota_with_max_connections() {
    let q = TenantResourceQuota::new("t1").with_max_connections(100);
    assert_eq!(q.max_connections, Some(100));
}

#[test]
fn test_tenant_resource_quota_with_max_qps() {
    let q = TenantResourceQuota::new("t1").with_max_qps(1000);
    assert_eq!(q.max_qps, Some(1000));
}

#[test]
fn test_tenant_resource_quota_with_max_storage() {
    let q = TenantResourceQuota::new("t1").with_max_storage(1024 * 1024 * 1024);
    assert_eq!(q.max_storage, Some(1073741824));
}

#[test]
fn test_tenant_resource_quota_limit() {
    let q = TenantResourceQuota::new("t1")
        .with_max_connections(10)
        .with_max_qps(100)
        .with_max_storage(1000);
    assert_eq!(q.limit(QuotaResource::Connection), Some(10));
    assert_eq!(q.limit(QuotaResource::Qps), Some(100));
    assert_eq!(q.limit(QuotaResource::Storage), Some(1000));
}

#[test]
fn test_tenant_resource_quota_limit_none() {
    let q = TenantResourceQuota::new("t1");
    assert!(q.limit(QuotaResource::Connection).is_none());
}

#[test]
fn test_tenant_resource_quota_is_exceeded() {
    let q = TenantResourceQuota::new("t1").with_max_connections(10);
    assert!(q.is_exceeded(QuotaResource::Connection, 10));
    assert!(q.is_exceeded(QuotaResource::Connection, 15));
    assert!(!q.is_exceeded(QuotaResource::Connection, 5));
}

#[test]
fn test_tenant_resource_quota_is_exceeded_no_limit() {
    let q = TenantResourceQuota::new("t1");
    assert!(!q.is_exceeded(QuotaResource::Connection, 1000));
}

#[test]
fn test_tenant_resource_quota_default() {
    let q = TenantResourceQuota::default();
    assert_eq!(q.tenant_id, "default");
}

#[test]
fn test_quota_error_display() {
    let e = QuotaError::QuotaExceeded {
        tenant_id: "t1".to_string(),
        resource: QuotaResource::Connection,
        limit: 10,
        current: 15,
    };
    let s = format!("{}", e);
    assert!(s.contains("quota exceeded"));
    assert!(s.contains("t1"));
}

#[test]
fn test_quota_error_check_failed_display() {
    let e = QuotaError::QuotaCheckFailed("error".to_string());
    let s = format!("{}", e);
    assert!(s.contains("quota check failed"));
}

#[test]
fn test_quota_error_rls_conflict_display() {
    let e = QuotaError::RlsPolicyConflict("conflict".to_string());
    let s = format!("{}", e);
    assert!(s.contains("RLS policy conflict"));
}

#[test]
fn test_quota_error_audit_failed_display() {
    let e = QuotaError::AuditLogWriteFailed("fail".to_string());
    let s = format!("{}", e);
    assert!(s.contains("audit log write failed"));
}

#[test]
fn test_quota_error_invalid_value_display() {
    let e = QuotaError::InvalidQuotaValue("invalid".to_string());
    let s = format!("{}", e);
    assert!(s.contains("invalid quota value"));
}

#[test]
fn test_quota_enforcer_new() {
    let enforcer = QuotaEnforcer::new();
    assert!(enforcer.get_quota("t1").is_none());
}

#[test]
fn test_quota_enforcer_set_get_quota() {
    let enforcer = QuotaEnforcer::new();
    enforcer.set_quota(TenantResourceQuota::new("t1").with_max_connections(10));
    let q = enforcer.get_quota("t1").unwrap();
    assert_eq!(q.max_connections, Some(10));
}

#[test]
fn test_quota_enforcer_check_quota_no_quota() {
    let enforcer = QuotaEnforcer::new();
    let result = enforcer.check_quota("t1", QuotaResource::Connection, 100);
    assert!(result.is_ok());
}

#[test]
fn test_quota_enforcer_check_quota_within_limit() {
    let enforcer = QuotaEnforcer::new();
    enforcer.set_quota(TenantResourceQuota::new("t1").with_max_connections(10));
    let result = enforcer.check_quota("t1", QuotaResource::Connection, 5);
    assert!(result.is_ok());
}

#[test]
fn test_quota_enforcer_check_quota_exceeded_failclose() {
    let enforcer = QuotaEnforcer::new();
    enforcer.set_quota(TenantResourceQuota::new("t1").with_max_connections(10));
    let result = enforcer.check_quota("t1", QuotaResource::Connection, 10);
    assert!(result.is_err());
}

#[test]
fn test_quota_enforcer_check_quota_exceeded_failopen() {
    let enforcer = QuotaEnforcer::new().with_strategy(QuotaEnforceStrategy::FailOpen);
    enforcer.set_quota(TenantResourceQuota::new("t1").with_max_connections(10));
    let result = enforcer.check_quota("t1", QuotaResource::Connection, 10);
    assert!(result.is_ok());
}

#[test]
fn test_quota_enforcer_check_quota_no_limit() {
    let enforcer = QuotaEnforcer::new();
    enforcer.set_quota(TenantResourceQuota::new("t1"));
    let result = enforcer.check_quota("t1", QuotaResource::Connection, 1000);
    assert!(result.is_ok());
}

#[test]
fn test_quota_enforcer_record_usage() {
    let enforcer = QuotaEnforcer::new();
    enforcer.record_usage("t1", QuotaResource::Connection, 5);
    assert_eq!(enforcer.current_usage("t1", QuotaResource::Connection), 5);
}

#[test]
fn test_quota_enforcer_record_usage_multiple() {
    let enforcer = QuotaEnforcer::new();
    enforcer.record_usage("t1", QuotaResource::Connection, 5);
    enforcer.record_usage("t1", QuotaResource::Connection, 3);
    assert_eq!(enforcer.current_usage("t1", QuotaResource::Connection), 8);
}

#[test]
fn test_quota_enforcer_release_usage() {
    let enforcer = QuotaEnforcer::new();
    enforcer.record_usage("t1", QuotaResource::Connection, 10);
    enforcer.release_usage("t1", QuotaResource::Connection, 3);
    assert_eq!(enforcer.current_usage("t1", QuotaResource::Connection), 7);
}

#[test]
fn test_quota_enforcer_release_usage_saturating() {
    let enforcer = QuotaEnforcer::new();
    enforcer.record_usage("t1", QuotaResource::Connection, 5);
    enforcer.release_usage("t1", QuotaResource::Connection, 10);
    assert_eq!(enforcer.current_usage("t1", QuotaResource::Connection), 0);
}

#[test]
fn test_quota_enforcer_current_usage_no_record() {
    let enforcer = QuotaEnforcer::new();
    assert_eq!(enforcer.current_usage("t1", QuotaResource::Connection), 0);
}

#[test]
fn test_quota_enforcer_check_and_record_ok() {
    let enforcer = QuotaEnforcer::new();
    enforcer.set_quota(TenantResourceQuota::new("t1").with_max_connections(10));
    let result = enforcer.check_and_record("t1", QuotaResource::Connection, 5);
    assert!(result.is_ok());
    assert_eq!(enforcer.current_usage("t1", QuotaResource::Connection), 5);
}

#[test]
fn test_quota_enforcer_check_and_record_exceeded() {
    let enforcer = QuotaEnforcer::new();
    enforcer.set_quota(TenantResourceQuota::new("t1").with_max_connections(10));
    enforcer.record_usage("t1", QuotaResource::Connection, 8);
    let result = enforcer.check_and_record("t1", QuotaResource::Connection, 5);
    assert!(result.is_err());
}

#[test]
fn test_quota_enforcer_record_usage_qps() {
    let enforcer = QuotaEnforcer::new();
    enforcer.record_usage("t1", QuotaResource::Qps, 100);
    assert_eq!(enforcer.current_usage("t1", QuotaResource::Qps), 100);
}

#[test]
fn test_quota_enforcer_record_usage_storage() {
    let enforcer = QuotaEnforcer::new();
    enforcer.record_usage("t1", QuotaResource::Storage, 1024);
    assert_eq!(enforcer.current_usage("t1", QuotaResource::Storage), 1024);
}

#[test]
fn test_quota_enforcer_default() {
    let enforcer = QuotaEnforcer::default();
    assert!(enforcer.get_quota("t1").is_none());
}

// ============================================================================
// QuotaEnforcer 审计路径（v4.7.0）
// ============================================================================

#[test]
fn test_quota_enforcer_set_audit_logger() {
    let enforcer = QuotaEnforcer::new();
    let logger = Arc::new(TenantAuditLogger::new());
    enforcer.set_audit_logger(Some(logger));
    enforcer.set_audit_logger(None);
    // 不 panic 即通过
    assert!(enforcer.get_quota("t1").is_none());
}

#[test]
fn test_quota_enforcer_audit_logs_on_exceeded() {
    let enforcer = QuotaEnforcer::new();
    let logger = Arc::new(TenantAuditLogger::new());
    enforcer.set_audit_logger(Some(logger.clone()));
    enforcer.set_quota(TenantResourceQuota::new("t1").with_max_connections(1));
    let result = enforcer.check_and_record("t1", QuotaResource::Connection, 2);
    assert!(result.is_err());
    assert_eq!(logger.log_count("t1"), 1);
    let logs = logger.all_logs();
    assert_eq!(logs[0].operation, "quota_exceeded");
    assert_eq!(logs[0].result, "rejected");
    assert_eq!(logs[0].quota_resource, Some(QuotaResource::Connection));
}

#[test]
fn test_quota_enforcer_audit_no_log_when_within_limit() {
    let enforcer = QuotaEnforcer::new();
    let logger = Arc::new(TenantAuditLogger::new());
    enforcer.set_audit_logger(Some(logger.clone()));
    enforcer.set_quota(TenantResourceQuota::new("t1").with_max_connections(10));
    let result = enforcer.check_and_record("t1", QuotaResource::Connection, 5);
    assert!(result.is_ok());
    assert_eq!(logger.log_count("t1"), 0);
}

// ============================================================================
// EnhancedRlsPolicy（多条件组合 + 列级脱敏联动）
// ============================================================================

#[test]
fn test_enhanced_rls_policy_new() {
    let p = EnhancedRlsPolicy::new("orders", Principal::new(1, vec!["admin".to_string()]));
    assert_eq!(p.table, "orders");
    assert!(p.conditions.is_empty());
    assert!(p.masking_rules.is_empty());
    assert_eq!(p.principal.tenant_id, 1);
    assert_eq!(p.principal.roles, vec!["admin"]);
}

#[test]
fn test_enhanced_rls_policy_with_condition() {
    let p = EnhancedRlsPolicy::new("orders", Principal::new(1, vec![])).with_condition(
        ParameterizedCondition::new("tenant_id = $1", vec![Value::from(1)]),
    );
    assert_eq!(p.conditions.len(), 1);
    assert_eq!(p.conditions[0].sql_fragment, "tenant_id = $1");
}

#[test]
fn test_enhanced_rls_policy_with_masking_rule() {
    let rule = ColumnMaskingRule::new(
        "orders",
        "phone",
        MaskingFunction::Phone,
        PermissionPredicate::all(),
    );
    let p = EnhancedRlsPolicy::new("orders", Principal::new(1, vec![])).with_masking_rule(rule);
    assert_eq!(p.masking_rules.len(), 1);
    assert_eq!(p.masking_rules[0].column, "phone");
}

#[test]
fn test_enhanced_rls_policy_combined_condition() {
    let p = EnhancedRlsPolicy::new("orders", Principal::new(1, vec![]))
        .with_condition(ParameterizedCondition::new(
            "tenant_id = $1",
            vec![Value::from(1)],
        ))
        .with_condition(ParameterizedCondition::new(
            "dept_id IN ($1,$2)",
            vec![Value::from(10), Value::from(20)],
        ));
    let combined = p.combined_condition().unwrap();
    // `$N` 占位符统一替换为 `?`
    assert_eq!(combined.sql_fragment, "tenant_id = ? AND dept_id IN (?,?)");
    assert_eq!(combined.params.len(), 3);
}

#[test]
fn test_enhanced_rls_policy_combined_condition_none() {
    let p = EnhancedRlsPolicy::new("orders", Principal::new(1, vec![]));
    assert!(p.combined_condition().is_none());
}

#[test]
fn test_enhanced_rls_policy_to_legacy_policy() {
    let p = EnhancedRlsPolicy::new("orders", Principal::new(1, vec![])).with_condition(
        ParameterizedCondition::new("tenant_id = $1", vec![Value::from(1)]),
    );
    let legacy = p.to_legacy_policy().unwrap();
    assert_eq!(legacy.table, "orders");
    assert_eq!(legacy.filter_condition.sql_fragment, "tenant_id = ?");
    assert_eq!(legacy.principal.tenant_id, 1);
}

#[test]
fn test_enhanced_rls_policy_to_legacy_no_conditions_error() {
    let p = EnhancedRlsPolicy::new("orders", Principal::new(1, vec![]));
    let err = p.to_legacy_policy().unwrap_err();
    assert!(matches!(err, QuotaError::RlsPolicyConflict(msg) if msg.contains("no conditions")));
}

// ============================================================================
// RlsPolicyEnhancer（策略注册 + 查询增强 + 行级脱敏）
// ============================================================================

#[test]
fn test_rls_policy_enhancer_new_default() {
    let enhancer = RlsPolicyEnhancer::new();
    assert!(enhancer.get_policy("orders").is_none());
    assert!(enhancer.masking_rules("orders").is_empty());
    let enhancer = RlsPolicyEnhancer::default();
    assert!(enhancer.get_policy("orders").is_none());
}

#[test]
fn test_rls_policy_enhancer_with_policy_and_get() {
    let enhancer = RlsPolicyEnhancer::new();
    let policy = EnhancedRlsPolicy::new("orders", Principal::new(1, vec![])).with_condition(
        ParameterizedCondition::new("tenant_id = $1", vec![Value::from(1)]),
    );
    enhancer.with_policy(policy).unwrap();
    let got = enhancer.get_policy("orders").unwrap();
    assert_eq!(got.table, "orders");
    assert_eq!(got.conditions.len(), 1);
}

#[test]
fn test_rls_policy_enhancer_with_policy_duplicate_error() {
    let enhancer = RlsPolicyEnhancer::new();
    enhancer
        .with_policy(EnhancedRlsPolicy::new("orders", Principal::new(1, vec![])))
        .unwrap();
    let dup = EnhancedRlsPolicy::new("orders", Principal::new(1, vec![]));
    let err = enhancer.with_policy(dup).unwrap_err();
    assert!(matches!(err, QuotaError::RlsPolicyConflict(msg) if msg.contains("already exists")));
}

#[test]
fn test_rls_policy_enhancer_enhance_query_match() {
    let enhancer = RlsPolicyEnhancer::new();
    let policy = EnhancedRlsPolicy::new("orders", Principal::new(1, vec![])).with_condition(
        ParameterizedCondition::new("tenant_id = $1", vec![Value::from(1)]),
    );
    enhancer.with_policy(policy).unwrap();
    let cond = enhancer.enhance_query("orders", "1").unwrap().unwrap();
    assert_eq!(cond.sql_fragment, "tenant_id = ?");
    assert_eq!(cond.params.len(), 1);
}

#[test]
fn test_rls_policy_enhancer_enhance_query_no_policy() {
    let enhancer = RlsPolicyEnhancer::new();
    let result = enhancer.enhance_query("orders", "1").unwrap();
    assert!(result.is_none());
}

#[test]
fn test_rls_policy_enhancer_enhance_query_tenant_mismatch() {
    let enhancer = RlsPolicyEnhancer::new();
    enhancer
        .with_policy(EnhancedRlsPolicy::new("orders", Principal::new(1, vec![])))
        .unwrap();
    let err = enhancer.enhance_query("orders", "2").unwrap_err();
    assert!(
        matches!(err, QuotaError::RlsPolicyConflict(msg) if msg.contains("tenant_id mismatch"))
    );
}

#[test]
fn test_rls_policy_enhancer_mask_row() {
    let enhancer = RlsPolicyEnhancer::new();
    let rule = ColumnMaskingRule::new(
        "orders",
        "phone",
        MaskingFunction::Phone,
        PermissionPredicate::all(),
    );
    let policy =
        EnhancedRlsPolicy::new("orders", Principal::new(1, vec![])).with_masking_rule(rule);
    enhancer.with_policy(policy).unwrap();

    let mut row = HashMap::new();
    row.insert("phone".to_string(), "13800138000".to_string());
    enhancer.mask_row("orders", &mut row);
    assert_ne!(row["phone"], "13800138000", "phone 列应被脱敏");

    // 未注册脱敏规则的列不受影响
    let mut row2 = HashMap::new();
    row2.insert("name".to_string(), "alice".to_string());
    enhancer.mask_row("orders", &mut row2);
    assert_eq!(row2["name"], "alice");
}

// ============================================================================
// TenantAuditEntry / TenantAuditLogger（租户级审计）
// ============================================================================

#[test]
fn test_tenant_audit_entry_new() {
    let entry = TenantAuditEntry::new(
        "t1",
        TenantAuditOperation::ContextSet,
        AuditResult::Success,
        "detail",
    );
    assert_eq!(entry.tenant_id, "t1");
    assert_eq!(entry.operation, "context_set");
    assert_eq!(entry.result, "success");
    assert_eq!(entry.detail, "detail");
    assert!(entry.table.is_none());
    assert!(entry.quota_resource.is_none());
}

#[test]
fn test_tenant_audit_entry_with_table_and_resource() {
    let entry = TenantAuditEntry::new(
        "t1",
        TenantAuditOperation::ContextSwitch,
        AuditResult::Denied,
        "d",
    )
    .with_table("orders")
    .with_quota_resource(QuotaResource::Qps);
    assert_eq!(entry.table.as_deref(), Some("orders"));
    assert_eq!(entry.quota_resource, Some(QuotaResource::Qps));
}

#[test]
fn test_tenant_audit_entry_to_audit_context_success() {
    let entry = TenantAuditEntry::new(
        "1",
        TenantAuditOperation::ContextSet,
        AuditResult::Success,
        "d",
    );
    let ctx = entry.to_audit_context();
    assert_eq!(ctx.tenant_id, 1);
    assert_eq!(ctx.operation, TenantAuditOperation::ContextSet);
    assert_eq!(ctx.result, AuditResult::Success);
}

#[test]
fn test_tenant_audit_entry_to_audit_context_denied() {
    let entry = TenantAuditEntry::new(
        "2",
        TenantAuditOperation::CrossTenantDenied,
        AuditResult::Denied,
        "d",
    );
    let ctx = entry.to_audit_context();
    assert_eq!(ctx.tenant_id, 2);
    assert_eq!(ctx.operation, TenantAuditOperation::CrossTenantDenied);
    assert_eq!(ctx.result, AuditResult::Denied);
}

#[test]
fn test_tenant_audit_logger_log_and_get() {
    let logger = TenantAuditLogger::new();
    logger
        .log(TenantAuditEntry::new(
            "t1",
            TenantAuditOperation::ContextSet,
            AuditResult::Success,
            "d",
        ))
        .unwrap();
    logger
        .log(TenantAuditEntry::new(
            "t2",
            TenantAuditOperation::ContextSet,
            AuditResult::Success,
            "d",
        ))
        .unwrap();
    assert_eq!(logger.get_logs("t1").len(), 1);
    assert_eq!(logger.log_count("t1"), 1);
    assert_eq!(logger.log_count("t2"), 1);
    assert_eq!(logger.log_count("t3"), 0);
}

#[test]
fn test_tenant_audit_logger_all_logs() {
    let logger = TenantAuditLogger::new();
    logger
        .log(TenantAuditEntry::new(
            "t1",
            TenantAuditOperation::ContextSet,
            AuditResult::Success,
            "d",
        ))
        .unwrap();
    assert_eq!(logger.all_logs().len(), 1);
}

#[test]
fn test_tenant_audit_logger_filter_by_operation() {
    let logger = TenantAuditLogger::new();
    logger
        .log(TenantAuditEntry::new(
            "t1",
            TenantAuditOperation::ContextSet,
            AuditResult::Success,
            "d",
        ))
        .unwrap();
    logger
        .log(TenantAuditEntry::new(
            "t1",
            TenantAuditOperation::ColumnMasked,
            AuditResult::Success,
            "d",
        ))
        .unwrap();
    assert_eq!(logger.filter_by_operation("t1", "column_masked").len(), 1);
    assert_eq!(logger.filter_by_operation("t1", "context_set").len(), 1);
    assert_eq!(logger.filter_by_operation("t2", "context_set").len(), 0);
}

#[test]
fn test_tenant_audit_logger_default() {
    let logger = TenantAuditLogger::default();
    assert!(logger.all_logs().is_empty());
    let dbg = format!("{:?}", logger);
    assert!(dbg.contains("log_count"));
}
