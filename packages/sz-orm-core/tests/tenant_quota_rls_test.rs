#![cfg(feature = "tenant-quota-rls-enhanced")]

use sz_orm_core::tenant_quota_rls::*;

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
