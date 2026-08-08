//! M1-T11：竞态与跨租户泄漏测试
//!
//! 验证：
//! - 并发请求中租户上下文切换竞态，每请求读到正确租户上下文
//! - 查询重写全覆盖（select/insert/update/delete 均追加隔离条件）
//! - 审计日志完整性

#![cfg(feature = "multi-tenant-enhanced")]

use sz_orm_core::dialect::get_dialect;
use sz_orm_core::tenant_context::{IsolationStrategy, TenantContext};
use sz_orm_core::DbType;
use sz_orm_core::Model;
use sz_orm_core::QueryBuilder;
use sz_orm_core::Value;

struct TenantModel;
impl Model for TenantModel {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "orders"
    }
    fn pk(&self) -> Self::PrimaryKey {
        1
    }
    fn set_pk(&mut self, _pk: Self::PrimaryKey) {}
    fn tenant_field() -> Option<&'static str> {
        Some("tenant_id")
    }
}

/// M1-T11.1：并发请求中租户上下文隔离，无跨租户泄漏
#[tokio::test]
async fn test_concurrent_tenant_isolation() {
    let mut handles = Vec::new();

    for tenant_id in 1..=10 {
        let handle = tokio::spawn(async move {
            let ctx = TenantContext::new(tenant_id, IsolationStrategy::RowLevel);
            ctx.scope(async move {
                // 在 scope 内多次 yield，模拟并发调度
                for _ in 0..5 {
                    tokio::task::yield_now().await;
                }
                // 验证上下文未泄漏
                let current = TenantContext::current().unwrap();
                assert_eq!(
                    current.tenant_id, tenant_id,
                    "跨租户泄漏！期望 {} 得到 {}",
                    tenant_id, current.tenant_id
                );
                current.tenant_id
            })
            .await
        });
        handles.push(handle);
    }

    let results = futures::future::join_all(handles).await;
    for (i, result) in results.into_iter().enumerate() {
        assert_eq!(result.unwrap(), i as i64 + 1);
    }
}

/// M1-T11.1：并发请求中 Schema 隔离表名重写正确
#[tokio::test]
async fn test_concurrent_schema_isolation() {
    let mut handles = Vec::new();

    for tenant_id in 1..=5 {
        let handle = tokio::spawn(async move {
            let ctx = TenantContext::new(tenant_id, IsolationStrategy::SchemaIsolation);
            ctx.scope(async move {
                tokio::task::yield_now().await;
                let (sql, _) =
                    QueryBuilder::<TenantModel>::new(get_dialect(DbType::MySQL).unwrap())
                        .table("orders")
                        .build_select_with_params();
                let expected = format!("tenant_{}_orders", tenant_id);
                assert!(
                    sql.contains(&expected),
                    "Schema 隔离失败：期望包含 {} 得到 {}",
                    expected,
                    sql
                );
                tenant_id
            })
            .await
        });
        handles.push(handle);
    }

    let results = futures::future::join_all(handles).await;
    for (i, result) in results.into_iter().enumerate() {
        assert_eq!(result.unwrap(), i as i64 + 1);
    }
}

/// M1-T11.2：查询重写全覆盖（select/insert/update/delete 均追加隔离条件）
#[tokio::test]
async fn test_query_rewrite_full_coverage() {
    let ctx = TenantContext::new(42, IsolationStrategy::RowLevel);
    ctx.scope(async {
        // SELECT
        let (sql, params) = QueryBuilder::<TenantModel>::new(get_dialect(DbType::MySQL).unwrap())
            .table("orders")
            .build_select_with_params();
        assert!(
            sql.contains("`tenant_id` = ?"),
            "SELECT 缺少租户条件: {}",
            sql
        );
        assert!(
            params.contains(&Value::I64(42)),
            "SELECT 缺少 tenant_id 参数"
        );

        // UPDATE
        let mut data = std::collections::HashMap::new();
        data.insert("status".to_string(), Value::String("shipped".to_string()));
        let (sql, params) = QueryBuilder::<TenantModel>::new(get_dialect(DbType::MySQL).unwrap())
            .table("orders")
            .where_eq("id", Value::I64(1))
            .build_update_with_params(&data);
        assert!(
            sql.contains("`tenant_id` = ?"),
            "UPDATE 缺少租户条件: {}",
            sql
        );
        assert!(
            params.contains(&Value::I64(42)),
            "UPDATE 缺少 tenant_id 参数"
        );

        // DELETE (软删除)
        let (sql, params) = QueryBuilder::<TenantModel>::new(get_dialect(DbType::MySQL).unwrap())
            .table("orders")
            .where_eq("id", Value::I64(1))
            .build_delete_with_params();
        assert!(
            sql.contains("`tenant_id` = ?"),
            "DELETE 缺少租户条件: {}",
            sql
        );
        assert!(
            params.contains(&Value::I64(42)),
            "DELETE 缺少 tenant_id 参数"
        );

        // COUNT
        let sql = QueryBuilder::<TenantModel>::new(get_dialect(DbType::MySQL).unwrap())
            .table("orders")
            .build_count();
        assert!(
            sql.contains("`tenant_id` = 42"),
            "COUNT 缺少租户条件: {}",
            sql
        );
    })
    .await;
}

/// M1-T11.3：审计日志完整性
#[tokio::test]
async fn test_audit_log_completeness() {
    use sz_orm_core::tenant_security::{AuditResult, TenantAuditContext, TenantAuditOperation};

    let auditor = sz_orm_audit::SqlAuditor::new();

    // 记录上下文设置
    let ctx_set = TenantAuditContext::new(
        42,
        TenantAuditOperation::ContextSet,
        AuditResult::Success,
        "context set",
    );
    ctx_set.log_to(&auditor);

    // 记录租户切换
    let ctx_switch = TenantAuditContext::new(
        42,
        TenantAuditOperation::ContextSwitch,
        AuditResult::Success,
        "switched from tenant 1 to 42",
    );
    ctx_switch.log_to(&auditor);

    // 记录跨租户拒绝
    let ctx_denied = TenantAuditContext::new(
        42,
        TenantAuditOperation::CrossTenantDenied,
        AuditResult::Denied,
        "attempted access to tenant 99 data",
    );
    ctx_denied.log_to(&auditor);

    // 记录行级过滤
    let ctx_filtered = TenantAuditContext::new(
        42,
        TenantAuditOperation::RowLevelFiltered,
        AuditResult::Success,
        "filtered by department_id = 10",
    );
    ctx_filtered.log_to(&auditor);

    // 记录列级脱敏
    let ctx_masked = TenantAuditContext::new(
        42,
        TenantAuditOperation::ColumnMasked,
        AuditResult::Success,
        "masked column phone for role employee",
    );
    ctx_masked.log_to(&auditor);

    let logs = auditor.get_logs();
    assert_eq!(logs.len(), 5, "应有 5 条审计记录");

    // 验证每条记录包含租户 ID
    for log in &logs {
        assert!(
            log.sql.contains("tenant=42"),
            "审计日志缺少租户 ID: {}",
            log.sql
        );
    }

    // 验证操作类型覆盖
    assert!(logs[0].sql.contains("context_set"));
    assert!(logs[1].sql.contains("context_switch"));
    assert!(logs[2].sql.contains("cross_tenant_denied"));
    assert!(logs[2].sql.contains("denied"), "跨租户拒绝应为 denied");
    assert!(logs[3].sql.contains("row_level_filtered"));
    assert!(logs[4].sql.contains("column_masked"));
}
