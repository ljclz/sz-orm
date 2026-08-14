//! 安全攻击性测试：多租户越权攻击向量（门禁 21 安全攻击测试）
//!
//! 需要 `multi-tenant-enhanced` feature：`cargo test -p sz-orm-core --features multi-tenant-enhanced --test security_attacks`
//!
//! 攻击面：
//!   1. 跨租户表名直接访问（Schema 隔离绕过尝试）——记录已知边界
//!   2. tenant_id 必须参数化（SQL 注入面：值永不为字面量）
//!   3. 无租户上下文时的行为（越权前提）——记录"调用方负责"边界
//!   4. Schema 路由边界值（负数/超大 tenant_id）——不 panic、格式稳定

#![cfg(feature = "multi-tenant-enhanced")]

use sz_orm_core::dialect::get_dialect;
use sz_orm_core::tenant_context::{IsolationStrategy, TenantContext};
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

fn builder() -> QueryBuilder<Order> {
    QueryBuilder::<Order>::new(get_dialect(DbType::MySQL).unwrap())
}

#[test]
fn attack_tenant_id_never_inlined_as_literal() {
    // 攻击：tenant_id 若以字面量拼进 SQL，可被注入（1 OR 1=1 等）
    // 防御断言：构建出的 SQL 必须使用参数占位符，值只出现在 params 中
    let qb = builder()
        .table("orders")
        .with_tenant_id(42)
        .where_eq("status", Value::String("active".to_string()));
    let (sql, params) = qb.build_select_with_params();
    // tenant_id 条件必须参数化（? 占位），42 不得以字面量出现在 SQL
    assert!(
        !sql.contains("42"),
        "tenant_id 必须以参数传递，不得内联字面量: {sql}"
    );
    assert!(
        sql.matches('?').count() >= 1,
        "tenant_id 条件应有参数占位符: {sql}"
    );
    assert!(
        params.iter().any(|v| v.as_i64() == Some(42)),
        "params 中应包含 tenant_id=42 的参数值"
    );
}

#[tokio::test]
async fn attack_cross_tenant_table_access_attempt() {
    // 攻击：租户 42 的代码直接写 table("orders") → Schema 隔离下应重写为 tenant_42_orders
    // （防御成立的前提：所有表访问都经 QueryBuilder 的 table() 入口）
    let ctx = TenantContext::new(42, IsolationStrategy::SchemaIsolation);
    let result = ctx
        .scope(async {
            let qb = builder().table("orders").with_tenant_id(42);
            let (sql, _) = qb.build_select_with_params();
            sql
        })
        .await;
    assert!(
        result.contains("tenant_42_orders"),
        "Schema 隔离下表名应重写为 tenant_42_orders: {result}"
    );
    // 攻击变体：直接构造 tenant_99_orders（模拟读取他租户表名）
    // 已知边界：若调用方绕过 table() 入口直接拼表名，隔离不生效——
    // 文档化该边界（防御依赖"全表访问经 QueryBuilder"约定）
    let direct = builder().table("tenant_99_orders").with_tenant_id(42);
    let (sql, _) = direct.build_select_with_params();
    assert!(
        sql.contains("tenant_99_orders"),
        "已知边界：直接指定表名可绕过 Schema 重写（调用方须遵守 table() 入口约定）"
    );
}

#[tokio::test]
async fn attack_missing_context_no_tenant_filter() {
    // 攻击前提：无 TenantContext、未显式 with_tenant_id → 查询不含租户条件
    // 这是设计行为（README：跨租户查询调用方需自行确保安全），测试固化该边界，
    // 防止未来"意外默认注入"或"意外强制"改变行为而不自知
    let qb = builder()
        .table("orders")
        .where_eq("status", Value::String("active".to_string()));
    let (sql, _) = qb.build_select_with_params();
    assert!(
        !sql.contains("tenant"),
        "无上下文时不得隐式注入租户条件（已知边界）: {sql}"
    );
}

#[test]
fn attack_schema_router_edge_values() {
    // 攻击/边界：负数、超大、0 tenant_id 不得 panic 或产生畸形表名
    for tid in [0i64, -1, i64::MAX, i64::MIN] {
        let name = sz_orm_core::tenant_context::SchemaIsolationRouter::rewrite_table("orders", tid);
        assert!(
            name.starts_with("tenant_") && name.ends_with("_orders"),
            "表名格式异常: {name} (tenant_id={tid})"
        );
        assert!(
            !name.contains("..") && !name.contains('/'),
            "表名不得含路径分隔符（注入面）: {name}"
        );
    }
}
