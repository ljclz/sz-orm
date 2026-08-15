//! F4/F5 变异盲区回归测试（2026-08-15 验证报告发现 4）
//!
//! 目标：杀死 cargo-mutants 对 tenant_quota_rls.rs / cache_warmup_protection.rs
//! 生成的语义级存活变异体（110 体中 21 体存活，本文覆盖其中的高价值类）：
//!   - `QuotaEnforcer::record_usage` `+=` → `-=` / `*=`（:255-256）——配额累计运算符无断言
//!   - `replace_placeholders` `&&` → `||`、`==` → `!=`（:641）——RLS 占位符替换逻辑
//!   - `to_audit_context` 删除 4 个非等价 match 分支（:542-545）——审计类型映射
//!   - `PenetrationGuard::might_contain` → `true` / `false`（:241）——穿透防护核心语义
//!   - 顺带守卫：`release_usage` 饱和递减、`check_quota` 无配额放行、多位数占位符
//!
//! 运行：cargo test -p sz-orm-core --features tenant-quota-rls-enhanced,cache-warmup-protection
//!        --test tenant_quota_rls_regression

use std::sync::Arc;

use sz_orm_core::cache_warmup_protection::PenetrationGuard;
use sz_orm_core::process_l1_cache::{ProcessL1Cache, ProcessL1Config};
use sz_orm_core::tenant_quota_rls::{
    EnhancedRlsPolicy, QuotaEnforcer, QuotaResource, TenantAuditEntry, TenantResourceQuota,
};
use sz_orm_core::tenant_security::{
    AuditResult, ParameterizedCondition, Principal, TenantAuditOperation,
};
use sz_orm_core::Value;

fn make_cache() -> Arc<ProcessL1Cache<String>> {
    Arc::new(ProcessL1Cache::new(ProcessL1Config::default()))
}

/// 变异目标：`record_usage` 的 `+=` → `-=` / `*=`。
/// 若运算符被替换，累计值不再等于各次记录的精确和（`-=` 还会触发 u64 下溢 panic）。
#[test]
fn regress_record_usage_accumulates_exactly() {
    let enforcer = QuotaEnforcer::new();
    enforcer.record_usage("t1", QuotaResource::Connection, 3);
    enforcer.record_usage("t1", QuotaResource::Connection, 4);
    assert_eq!(enforcer.current_usage("t1", QuotaResource::Connection), 7);

    enforcer.record_usage("t1", QuotaResource::Storage, 1024);
    enforcer.record_usage("t1", QuotaResource::Storage, 2048);
    assert_eq!(enforcer.current_usage("t1", QuotaResource::Storage), 3072);

    enforcer.record_usage("t1", QuotaResource::Qps, 1);
    enforcer.record_usage("t1", QuotaResource::Qps, 1);
    assert_eq!(enforcer.current_usage("t1", QuotaResource::Qps), 2);
}

/// 顺带守卫：`release_usage` 饱和递减，超过当前使用量归 0 且不下溢。
#[test]
fn regress_release_usage_saturates_at_zero() {
    let enforcer = QuotaEnforcer::new();
    enforcer.record_usage("t1", QuotaResource::Connection, 5);
    enforcer.release_usage("t1", QuotaResource::Connection, 3);
    assert_eq!(enforcer.current_usage("t1", QuotaResource::Connection), 2);
    enforcer.release_usage("t1", QuotaResource::Connection, 8);
    assert_eq!(enforcer.current_usage("t1", QuotaResource::Connection), 0);
}

/// 顺带守卫：未配置配额 / 未配置该资源的配额 → `check_quota` 放行。
#[test]
fn regress_check_quota_no_quota_ok() {
    let enforcer = QuotaEnforcer::new();
    assert!(enforcer
        .check_quota("ghost", QuotaResource::Connection, 999)
        .is_ok());

    let quota = TenantResourceQuota::new("t2").with_max_connections(10);
    enforcer.set_quota(quota);
    assert!(enforcer
        .check_quota("t2", QuotaResource::Connection, 10)
        .is_err());
    assert!(enforcer
        .check_quota("t2", QuotaResource::Storage, 999)
        .is_ok());
}

/// 变异目标：`replace_placeholders` 的 `&&` → `||`、`==` → `!=`。
/// 覆盖三类输入：`$` 后跟非数字（应原样保留）、裸多位数（应原样保留）、
/// 单/多位数占位符（应替换为 `?`）。
#[test]
fn regress_replace_placeholders_semantics() {
    let principal = Principal::new(1, vec!["employee".to_string()]);
    let policy =
        EnhancedRlsPolicy::new("orders", principal).with_condition(ParameterizedCondition::new(
            "tenant_id = $1 AND score > 10 AND tag = $x",
            vec![Value::I64(1)],
        ));
    let combined = policy.combined_condition().unwrap();
    let sql = combined.sql_fragment.as_str();
    assert!(
        sql.contains("tenant_id = ?"),
        "占位符 $1 应替换为 ?，实际: {sql}"
    );
    assert!(!sql.contains("$1"), "$1 不应残留，实际: {sql}");
    assert!(
        sql.contains("score > 10"),
        "裸多位数 10 不应被误替换，实际: {sql}"
    );
    assert!(
        sql.contains("tag = $x"),
        "$ 后非数字应原样保留，实际: {sql}"
    );
}

/// 变异目标：`to_audit_context` 删除 match 分支（"context_set" 分支删除为等价变异，
/// 其余 4 个分支删除后操作类型会错映射为 ContextSet）。
#[test]
fn regress_audit_context_maps_every_operation_kind() {
    let cases = [
        (TenantAuditOperation::ContextSet, "context_set"),
        (TenantAuditOperation::ContextSwitch, "context_switch"),
        (
            TenantAuditOperation::CrossTenantDenied,
            "cross_tenant_denied",
        ),
        (TenantAuditOperation::RowLevelFiltered, "row_level_filtered"),
        (TenantAuditOperation::ColumnMasked, "column_masked"),
    ];
    for (expected, op_str) in cases {
        let entry = TenantAuditEntry::new("42", expected.clone(), AuditResult::Success, "detail");
        assert_eq!(entry.operation, op_str);
        let ctx = entry.to_audit_context();
        assert_eq!(ctx.operation, expected, "操作类型 {op_str} 映射错误");
        assert_eq!(ctx.tenant_id, 42);
        assert_eq!(ctx.result, AuditResult::Success);
    }
}

/// 变异目标：`PenetrationGuard::might_contain` → `true` / `false`。
/// 已注册的键必须返回 true（否则穿透防护误拦真实键），未注册的键必须返回 false（不漏判语义）。
#[test]
fn regress_penetration_guard_might_contain() {
    let guard = PenetrationGuard::new(make_cache(), 1000);
    let pk = Value::I64(42);
    let key = format!("users:{pk:?}");
    guard.register(&key).unwrap();

    assert!(
        guard.might_contain("users", &pk),
        "已注册键 must be found（防误拦）"
    );
    assert!(
        !guard.might_contain("users", &Value::I64(999)),
        "未注册键 must be rejected（不漏判）"
    );
    assert!(
        !guard.might_contain("other_table", &pk),
        "不同表同主键不得误判"
    );
}
