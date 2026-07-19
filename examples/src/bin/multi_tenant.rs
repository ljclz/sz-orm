//! 多租户 — TenantModel + TenantScope
//!
//! 演示如何让 Model 自动获得 `tenant_id = ?` 的查询过滤。
//! 通过实现 TenantModel trait，配合 (TenantScope, M) 全局作用域。
//!
//! 运行：`cargo run -p sz-orm-examples --bin multi_tenant`

use sz_orm_core::hooks::{GlobalScope, HookContext, TenantModel, TenantScope};
use sz_orm_core::{Model, Value};

// ===== 多租户 Model 定义 =====

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
struct Order {
    id: i64,
    tenant_id: i64,
    amount: f64,
    status: String,
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
}

impl TenantModel for Order {
    fn tenant_field() -> &'static str {
        "tenant_id"
    }
    fn tenant_id(&self) -> i64 {
        self.tenant_id
    }
    fn set_tenant_id(&mut self, tenant_id: i64) {
        self.tenant_id = tenant_id;
    }
}

fn main() {
    println!("=== 多租户 Model ===");
    println!("表名:        {}", Order::table_name());
    println!("租户字段:    {}", Order::tenant_field());

    let mut order = Order::default();
    order.set_tenant_id(42);
    println!("租户 ID:     {}", order.tenant_id());

    println!("\n=== TenantScope 应用（有租户上下文）===");
    let ctx_with_tenant = HookContext::new().with_tenant(42);
    let scope_result = <(TenantScope, Order) as GlobalScope>::apply_scope(&ctx_with_tenant);
    println!("ctx.tenant_id = {:?}", ctx_with_tenant.tenant_id);
    println!("追加 WHERE:    {:?}", scope_result);
    // 预期: Some(("tenant_id = ?", [I64(42)]))

    println!("\n=== TenantScope 应用（无租户上下文，跨租户查询）===");
    let ctx_no_tenant = HookContext::new();
    let scope_result = <(TenantScope, Order) as GlobalScope>::apply_scope(&ctx_no_tenant);
    println!("ctx.tenant_id = {:?}", ctx_no_tenant.tenant_id);
    println!("追加 WHERE:    {:?}", scope_result);
    // 预期: None（不追加条件，调用方自行保证安全）

    println!("\n=== 多租户使用模式（伪代码）===");
    println!(
        r#"// 1. 中间件注入租户 ID 到 HookContext
async fn tenant_middleware(req: Request, next: Next) -> Response {{
    let tenant_id = extract_tenant_from_jwt(&req);
    let ctx = HookContext::new().with_tenant(tenant_id);
    // 将 ctx 注入到请求扩展中...
    next.run(req).await
}}

// 2. 查询时自动追加 tenant_id = ?
//    SELECT * FROM orders WHERE status = 'paid' AND tenant_id = ?
//    (TenantScope::apply_scope 返回 ("tenant_id = ?", [I64(42)]))

// 3. 超级管理员跨租户查询（不设置 tenant_id）
//    SELECT * FROM orders WHERE status = 'paid'
//    (TenantScope::apply_scope 返回 None)

// 4. 写入时确保租户隔离
let mut order = Order::default();
order.set_tenant_id(ctx.tenant_id.unwrap());
order.amount = 100.0;
// INSERT INTO orders (tenant_id, amount, ...) VALUES (42, 100.0, ...)
"#
    );

    println!("=== Value 类型验证 ===");
    if let Some((sql, params)) =
        <(TenantScope, Order) as GlobalScope>::apply_scope(&ctx_with_tenant)
    {
        println!("SQL 片段:  {}", sql);
        println!("参数数量:  {}", params.len());
        for (i, p) in params.iter().enumerate() {
            println!("  参数[{}]: as_i64() = {:?}", i, p.as_i64());
        }
        // 验证 Value::I64 包装
        let v = Value::I64(42);
        assert_eq!(v.as_i64(), Some(42));
        println!("Value::I64(42).as_i64() == Some(42) ✓");
    }
}
