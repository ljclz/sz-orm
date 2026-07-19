//! Hooks 模块契约测试 — 对应 `docs/api-contracts.md` §11
//!
//! 锁定 HookContext、HookEvent、Hookable、HookDispatcher、SoftDelete、TenantModel 契约。

use std::sync::Arc;

use sz_orm_core::hooks::{
    HookContext, HookDispatcher, HookEvent, HookFn, HookRegistry, Hookable, ScopeRegistry,
    SoftDelete, TenantModel,
};
use sz_orm_core::{Model, ModelExt};

// ===== §11.1 HookContext 契约 =====

#[test]
fn test_hook_context_new_is_default_contract() {
    let ctx = HookContext::new();
    assert_eq!(ctx.tenant_id, None);
    assert_eq!(ctx.operator_id, None);
    assert_eq!(ctx.timestamp, 0);
    assert!(ctx.metadata.is_empty());
}

#[test]
fn test_hook_context_with_tenant_contract() {
    let ctx = HookContext::new().with_tenant(42);
    assert_eq!(ctx.tenant_id, Some(42));
}

#[test]
fn test_hook_context_with_operator_contract() {
    let ctx = HookContext::new().with_operator(100);
    assert_eq!(ctx.operator_id, Some(100));
}

#[test]
fn test_hook_context_with_timestamp_contract() {
    let ctx = HookContext::new().with_timestamp(1700000000);
    assert_eq!(ctx.timestamp, 1700000000);
}

#[test]
fn test_hook_context_chaining_contract() {
    let ctx = HookContext::new()
        .with_tenant(1)
        .with_operator(2)
        .with_timestamp(3);
    assert_eq!(ctx.tenant_id, Some(1));
    assert_eq!(ctx.operator_id, Some(2));
    assert_eq!(ctx.timestamp, 3);
}

#[test]
fn test_hook_context_set_get_meta_contract() {
    let mut ctx = HookContext::new();
    ctx.set_meta("request_id", "abc-123");
    ctx.set_meta("user_agent", "test");

    assert_eq!(ctx.get_meta("request_id"), Some(&"abc-123".to_string()));
    assert_eq!(ctx.get_meta("user_agent"), Some(&"test".to_string()));
    assert_eq!(ctx.get_meta("missing"), None);
}

// ===== §11.2 HookEvent 契约 =====

#[test]
fn test_hook_event_is_before_contract() {
    assert!(HookEvent::BeforeInsert.is_before());
    assert!(HookEvent::BeforeUpdate.is_before());
    assert!(HookEvent::BeforeDelete.is_before());
    assert!(HookEvent::BeforeFind.is_before());
    assert!(HookEvent::BeforeValidate.is_before());

    assert!(!HookEvent::AfterInsert.is_before());
    assert!(!HookEvent::AfterUpdate.is_before());
}

#[test]
fn test_hook_event_is_after_contract() {
    assert!(HookEvent::AfterInsert.is_after());
    assert!(HookEvent::AfterUpdate.is_after());
    assert!(HookEvent::AfterDelete.is_after());
    assert!(HookEvent::AfterFind.is_after());

    assert!(!HookEvent::BeforeInsert.is_after());
}

#[test]
fn test_hook_event_before_after_are_mutually_exclusive_contract() {
    // 所有 Before* 事件 is_before()==true, is_after()==false
    // 所有 After* 事件 is_after()==true, is_before()==false
    let all_events = [
        HookEvent::BeforeInsert,
        HookEvent::AfterInsert,
        HookEvent::BeforeUpdate,
        HookEvent::AfterUpdate,
        HookEvent::BeforeDelete,
        HookEvent::AfterDelete,
        HookEvent::BeforeWrite,
        HookEvent::AfterWrite,
        HookEvent::BeforeSave,
        HookEvent::AfterSave,
        HookEvent::BeforeRestore,
        HookEvent::AfterRestore,
        HookEvent::BeforeFind,
        HookEvent::AfterFind,
        HookEvent::BeforeValidate,
        HookEvent::AfterValidate,
    ];
    for e in all_events {
        assert!(
            e.is_before() != e.is_after(),
            "事件 {:?} 的 is_before/is_after 应互斥",
            e
        );
    }
}

// ===== §11.4 HookDispatcher 契约 =====

#[test]
fn test_hook_dispatcher_is_zero_sized_contract() {
    // HookDispatcher 是零大小类型（仅提供静态方法）
    assert_eq!(std::mem::size_of::<HookDispatcher>(), 0);
}

// ===== §11.7 HookRegistry 契约 =====

#[test]
fn test_hook_registry_new_contract() {
    let _registry = HookRegistry::new();
}

#[test]
fn test_hook_registry_register_and_dispatch_contract() {
    use std::sync::atomic::{AtomicU32, Ordering};
    let registry = HookRegistry::new();
    let counter = Arc::new(AtomicU32::new(0));
    let counter_clone = counter.clone();

    let hook: HookFn = Arc::new(move |_ctx| {
        counter_clone.fetch_add(1, Ordering::SeqCst);
        Ok(())
    });
    registry.register(HookEvent::BeforeInsert, hook);

    let ctx = HookContext::new();
    registry.dispatch(HookEvent::BeforeInsert, &ctx).unwrap();
    assert_eq!(counter.load(Ordering::SeqCst), 1);

    // 派发其他事件不应触发该钩子
    registry.dispatch(HookEvent::BeforeUpdate, &ctx).unwrap();
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

// ===== §11.7 ScopeRegistry 契约 =====

#[test]
fn test_scope_registry_new_contract() {
    let _registry = ScopeRegistry::new();
}

// ===== §11.5 SoftDelete trait 契约（编译时验证） =====

#[derive(Clone, Default)]
#[allow(dead_code)]
struct SoftDeletableUser {
    id: i64,
    name: String,
    deleted_at: Option<i64>,
}

impl Model for SoftDeletableUser {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "users"
    }
    fn pk(&self) -> Self::PrimaryKey {
        self.id
    }
    fn set_pk(&mut self, pk: Self::PrimaryKey) {
        self.id = pk;
    }
    // Model::soft_delete_field 返回 Option<&'static str>
    fn soft_delete_field() -> Option<&'static str> {
        Some("deleted_at")
    }
}

impl ModelExt for SoftDeletableUser {
    fn columns() -> Vec<&'static str> {
        vec!["id", "name", "deleted_at"]
    }
    fn fillable() -> Vec<&'static str> {
        vec!["name"]
    }
}

impl SoftDelete for SoftDeletableUser {
    // SoftDelete::soft_delete_field 返回 &'static str（与 Model 不同）
    fn soft_delete_field() -> &'static str {
        "deleted_at"
    }

    fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }
}

#[test]
fn test_soft_delete_trait_compiles_contract() {
    // SoftDelete trait 实现应编译通过
    let _user = SoftDeletableUser::default();
    // 使用完全限定语法避免与 Model::soft_delete_field 歧义
    assert_eq!(
        <SoftDeletableUser as SoftDelete>::soft_delete_field(),
        "deleted_at"
    );
    assert_eq!(
        <SoftDeletableUser as Model>::soft_delete_field(),
        Some("deleted_at")
    );
}

// ===== §11.6 TenantModel trait 契约（编译时验证） =====

#[derive(Clone, Default)]
#[allow(dead_code)]
struct TenantUser {
    id: i64,
    tenant_id: i64,
    name: String,
}

impl Model for TenantUser {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "tenant_users"
    }
    fn pk(&self) -> Self::PrimaryKey {
        self.id
    }
    fn set_pk(&mut self, pk: Self::PrimaryKey) {
        self.id = pk;
    }
}

impl ModelExt for TenantUser {
    fn columns() -> Vec<&'static str> {
        vec!["id", "tenant_id", "name"]
    }
    fn fillable() -> Vec<&'static str> {
        vec!["name"]
    }
}

impl TenantModel for TenantUser {
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

#[test]
fn test_tenant_model_trait_compiles_contract() {
    let mut user = TenantUser::default();
    assert_eq!(<TenantUser as TenantModel>::tenant_field(), "tenant_id");
    assert_eq!(user.tenant_id(), 0);
    user.set_tenant_id(42);
    assert_eq!(user.tenant_id(), 42);
}

// ===== §11.3 Hookable trait 契约（编译时验证） =====

#[derive(Clone, Default)]
#[allow(dead_code)]
struct HookableUser {
    id: i64,
    name: String,
}

impl Model for HookableUser {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "hookable_users"
    }
    fn pk(&self) -> Self::PrimaryKey {
        self.id
    }
    fn set_pk(&mut self, pk: Self::PrimaryKey) {
        self.id = pk;
    }
}

impl ModelExt for HookableUser {
    fn columns() -> Vec<&'static str> {
        vec!["id", "name"]
    }
    fn fillable() -> Vec<&'static str> {
        vec!["name"]
    }
}

impl Hookable for HookableUser {}

#[test]
fn test_hookable_trait_compiles_contract() {
    // Hookable trait 默认实现应编译通过
    let _user = HookableUser::default();
}
