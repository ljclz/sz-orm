#![cfg(all(feature = "owasp-pentest-suite", feature = "multi-tenant-enhanced"))]

//! OWASP A01: 失效的访问控制深化渗透测试（core 包）
//!
//! 对应 REQ-V49-001（OWASP A01 深化）
//!
//! 渗透测试向量：
//! - 水平越权：租户 1 和租户 2 的 Schema 隔离
//! - IDOR：查询附加 tenant_id 参数化条件，阻止跨用户访问

use sz_orm_core::dialect::get_dialect;
use sz_orm_core::tenant_context::{IsolationStrategy, SchemaIsolationRouter, TenantContext};
use sz_orm_core::{DbType, Model, QueryBuilder, Value};

#[derive(Debug, Clone, Default)]
struct Order {
    id: i64,
}

impl Model for Order {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "orders"
    }
    fn pk(&self) -> Self::PrimaryKey {
        self.id
    }
    fn set_pk(&mut self, pk: Self::PrimaryKey) {
        self.id = pk;
    }
    fn tenant_field() -> Option<&'static str> {
        Some("tenant_id")
    }
}

/// A01-5：水平越权隔离
///
/// 租户 1 和租户 2 的表名通过 Schema 隔离路由器重写，
/// 断言生成的表名含 `tenant_1_` / `tenant_2_` 前缀，互相隔离。
#[test]
fn a01_horizontal_privilege_isolation() {
    let ctx1 = TenantContext::new(1, IsolationStrategy::SchemaIsolation);
    let ctx2 = TenantContext::new(2, IsolationStrategy::SchemaIsolation);

    let table1 = SchemaIsolationRouter::rewrite_table("orders", ctx1.tenant_id);
    let table2 = SchemaIsolationRouter::rewrite_table("orders", ctx2.tenant_id);

    assert!(
        table1.contains("tenant_1_"),
        "租户 1 表名必须含 tenant_1_ 前缀"
    );
    assert!(
        !table1.contains("tenant_2_"),
        "租户 1 表名不得含 tenant_2_ 前缀"
    );
    assert!(
        table2.contains("tenant_2_"),
        "租户 2 表名必须含 tenant_2_ 前缀"
    );
    assert!(
        !table2.contains("tenant_1_"),
        "租户 2 表名不得含 tenant_1_ 前缀"
    );
    assert_ne!(table1, table2, "不同租户的表名必须不同");
}

/// A01-6：IDOR 被阻止
///
/// 用户 A（tenant_id=1）查询 orders where id=2，
/// 断言查询附加 `tenant_id` 参数化条件，IDOR 被阻止。
#[test]
fn a01_insecure_direct_object_reference_blocked() {
    let qb = QueryBuilder::<Order>::new(get_dialect(DbType::MySQL).unwrap())
        .table("orders")
        .with_tenant_id(1)
        .where_eq("id", Value::I64(2));
    let (sql, params) = qb.build_select_with_params();

    assert!(
        !sql.contains("tenant_id = 1"),
        "tenant_id 必须参数化，不得为字面量"
    );

    let has_tenant_param = params.iter().any(|p| matches!(p, Value::I64(1)));
    assert!(has_tenant_param, "params 必须含 tenant_id = 1");

    let has_id_param = params.iter().any(|p| matches!(p, Value::I64(2)));
    assert!(has_id_param, "params 必须含 id = 2");
}
