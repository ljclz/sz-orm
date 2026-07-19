//! 钩子系统 — 软删除 + 全局作用域
//!
//! 演示 HookContext、HookRegistry、ScopeRegistry 的用法。
//! Hookable + SoftDelete + GlobalScope 让 Model 自动获得：
//! - 查询时自动追加 `deleted_at IS NULL`
//! - 删除时改为 `UPDATE SET deleted_at = NOW()`
//!
//! 运行：`cargo run -p sz-orm-examples --bin hooks_soft_delete`

use std::sync::Arc;

use sz_orm_core::hooks::{
    GlobalScope, HookContext, HookEvent, HookRegistry, Hookable, ScopeRegistry, SoftDelete,
    SoftDeleteScope,
};
use sz_orm_core::{DbError, Model, TimestampFields, Value};

// ===== 定义带软删除的 Model =====

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
struct Product {
    id: i64,
    name: String,
    price: f64,
    deleted_at: Option<String>,
}

impl Model for Product {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "products"
    }
    fn pk(&self) -> Self::PrimaryKey {
        self.id
    }
    fn set_pk(&mut self, pk: Self::PrimaryKey) {
        self.id = pk;
    }
    fn timestamp_fields() -> Option<TimestampFields> {
        None
    }
    fn soft_delete_field() -> Option<&'static str> {
        Some("deleted_at")
    }
}

impl SoftDelete for Product {
    fn soft_delete_field() -> &'static str {
        "deleted_at"
    }
    fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }
}

impl Hookable for Product {
    fn before_insert(_ctx: &mut HookContext) -> Result<(), DbError> {
        println!("  [钩子] before_insert: 即将插入商品");
        Ok(())
    }
    fn after_insert(_ctx: &HookContext, _id: &Self::PrimaryKey) -> Result<(), DbError> {
        println!("  [钩子] after_insert: 商品已插入, id={}", _id);
        Ok(())
    }
    fn before_delete(_ctx: &mut HookContext, _id: &Self::PrimaryKey) -> Result<(), DbError> {
        println!("  [钩子] before_delete: 即将删除商品 id={}", _id);
        Ok(())
    }
}

fn main() {
    println!("=== HookContext 构建器 ===");
    let ctx = HookContext::new()
        .with_tenant(42)
        .with_operator(1)
        .with_timestamp(1700000000);
    println!("租户:    {:?}", ctx.tenant_id);
    println!("操作人:  {:?}", ctx.operator_id);
    println!("时间戳:  {}", ctx.timestamp);

    let mut ctx = ctx;
    ctx.set_meta("source", "api");
    ctx.set_meta("ip", "127.0.0.1");
    println!("元数据:  {:?}", ctx.metadata);

    println!("\n=== 软删除全局作用域 ===");
    // (SoftDeleteScope, Product) 实现了 GlobalScope
    let scope_result = <(SoftDeleteScope, Product) as GlobalScope>::apply_scope(&ctx);
    println!("追加 WHERE: {:?}", scope_result);
    // 预期: Some(("deleted_at IS NULL", []))

    println!("\n=== HookRegistry 运行时钩子 ===");
    let registry = HookRegistry::new();

    // 注册运行时钩子
    let call_count = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let counter = Arc::clone(&call_count);
    registry.register(
        HookEvent::BeforeInsert,
        Arc::new(move |_ctx| {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }),
    );

    println!(
        "初始调用次数: {}",
        call_count.load(std::sync::atomic::Ordering::SeqCst)
    );
    registry.dispatch(HookEvent::BeforeInsert, &ctx).unwrap();
    registry.dispatch(HookEvent::BeforeInsert, &ctx).unwrap();
    println!(
        "dispatch 后:  {}",
        call_count.load(std::sync::atomic::Ordering::SeqCst)
    );

    println!("\n=== ScopeRegistry 作用域控制 ===");
    let scope_reg = ScopeRegistry::new();
    println!("soft_delete 启用: {}", scope_reg.is_enabled("soft_delete"));
    scope_reg.disable("soft_delete");
    println!("禁用后:           {}", scope_reg.is_enabled("soft_delete"));

    println!("\n=== without_scope 临时禁用 ===");
    let result = scope_reg.without_scope("tenant", || {
        println!("  闭包内 tenant 状态: {}", scope_reg.is_enabled("tenant"));
        42
    });
    println!("闭包返回: {}", result);
    println!("闭包外 tenant 状态: {}", scope_reg.is_enabled("tenant"));

    println!("\n=== Hookable trait 默认钩子调用 ===");
    println!("执行 before_insert:");
    let mut ctx2 = HookContext::new();
    Product::before_insert(&mut ctx2).unwrap();

    println!("执行 after_insert:");
    Product::after_insert(&ctx2, &100).unwrap();

    println!("执行 before_delete:");
    Product::before_delete(&mut ctx2, &100).unwrap();

    println!("\n=== Value 类型示例（作用域参数）===");
    let v = Value::I64(42);
    println!("Value::I64(42): as_i64() = {:?}", v.as_i64());
}
