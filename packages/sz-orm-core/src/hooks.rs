//! 钩子系统（Hooks）— 软删除 + 多租户
//!
//! 对应设计文档 3.8 节「钩子系统」。
//!
//! # 核心概念
//!
//! - **HookContext**：钩子执行上下文，包含当前租户、操作人、时间戳等
//! - **Hookable**：可钩选的 Model，支持 before/after insert/update/delete 6 个生命周期
//! - **SoftDelete**：软删除 trait，标记删除而非物理删除
//! - **GlobalScope**：全局查询作用域，自动过滤（如自动排除软删除行、自动追加租户条件）
//! - **TenantScope**：多租户全局作用域，自动追加 `tenant_id = ?` 条件
//!
//! # 使用示例
//!
//! ```no_run
//! use sz_orm_core::hooks::{HookContext, Hookable, SoftDelete, TenantScope, GlobalScope};
//!
//! // 1. 定义带软删除+多租户的 Model
//! // 2. 查询时自动过滤 deleted_at IS NULL AND tenant_id = ?
//! // 3. 删除时自动 UPDATE SET deleted_at = NOW() 而非 DELETE
//! ```

use crate::error::DbError;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

// =====================================================================
// HookContext — 钩子执行上下文
// =====================================================================

/// 钩子执行上下文
///
/// 携带请求级别的元数据，供钩子读取/修改。
#[derive(Debug, Clone, Default)]
pub struct HookContext {
    /// 当前租户 ID（多租户场景）
    pub tenant_id: Option<i64>,
    /// 当前操作人 ID
    pub operator_id: Option<i64>,
    /// 时间戳（Unix 微秒）
    pub timestamp: u64,
    /// 额外元数据
    pub metadata: HashMap<String, String>,
}

impl HookContext {
    /// 创建新的空上下文
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置租户 ID
    pub fn with_tenant(mut self, tenant_id: i64) -> Self {
        self.tenant_id = Some(tenant_id);
        self
    }

    /// 设置操作人 ID
    pub fn with_operator(mut self, operator_id: i64) -> Self {
        self.operator_id = Some(operator_id);
        self
    }

    /// 设置时间戳
    pub fn with_timestamp(mut self, ts: u64) -> Self {
        self.timestamp = ts;
        self
    }

    /// 插入元数据
    pub fn set_meta(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.metadata.insert(key.into(), value.into());
    }

    /// 读取元数据
    pub fn get_meta(&self, key: &str) -> Option<&String> {
        self.metadata.get(key)
    }
}

// =====================================================================
// HookEvent — 钩子事件类型
// =====================================================================

/// 钩子事件类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookEvent {
    BeforeInsert,
    AfterInsert,
    BeforeUpdate,
    AfterUpdate,
    BeforeDelete,
    AfterDelete,
}

impl HookEvent {
    /// 是否为 before 事件
    pub fn is_before(&self) -> bool {
        matches!(
            self,
            HookEvent::BeforeInsert | HookEvent::BeforeUpdate | HookEvent::BeforeDelete
        )
    }

    /// 是否为 after 事件
    pub fn is_after(&self) -> bool {
        matches!(
            self,
            HookEvent::AfterInsert | HookEvent::AfterUpdate | HookEvent::AfterDelete
        )
    }
}

// =====================================================================
// HookResult — 钩子执行结果
// =====================================================================

/// 钩子执行结果
pub type HookResult<T> = Result<T, DbError>;

// =====================================================================
// Hookable — 可钩选 Model trait
// =====================================================================

/// 可钩选 Model trait
///
/// 实现 `Hookable` 的 Model 可以在 insert/update/delete 前后执行自定义逻辑。
/// 默认实现为 no-op，Model 按需 override。
pub trait Hookable: crate::model::Model {
    /// 插入前钩子（默认 no-op）
    fn before_insert(_ctx: &mut HookContext) -> HookResult<()> {
        Ok(())
    }

    /// 插入后钩子（默认 no-op）
    fn after_insert(_ctx: &HookContext, _id: &Self::PrimaryKey) -> HookResult<()> {
        Ok(())
    }

    /// 更新前钩子（默认 no-op）
    fn before_update(_ctx: &mut HookContext, _id: &Self::PrimaryKey) -> HookResult<()> {
        Ok(())
    }

    /// 更新后钩子（默认 no-op）
    fn after_update(_ctx: &HookContext, _id: &Self::PrimaryKey) -> HookResult<()> {
        Ok(())
    }

    /// 删除前钩子（默认 no-op）
    fn before_delete(_ctx: &mut HookContext, _id: &Self::PrimaryKey) -> HookResult<()> {
        Ok(())
    }

    /// 删除后钩子（默认 no-op）
    fn after_delete(_ctx: &HookContext, _id: &Self::PrimaryKey) -> HookResult<()> {
        Ok(())
    }
}

// =====================================================================
// SoftDelete — 软删除 trait
// =====================================================================

/// 软删除 trait
///
/// 实现此 trait 的 Model 在调用 delete 时，实际执行
/// `UPDATE SET {field} = NOW() WHERE pk = ?` 而非 `DELETE`。
pub trait SoftDelete: crate::model::Model {
    /// 软删除字段名（如 `deleted_at`）
    fn soft_delete_field() -> &'static str;

    /// 是否已软删除（由 Model 数据行判断）
    fn is_deleted(&self) -> bool;
}

// =====================================================================
// GlobalScope — 全局查询作用域
// =====================================================================

/// 全局查询作用域
///
/// 应用到所有查询的隐式过滤条件。
/// 典型实现：软删除作用域（`deleted_at IS NULL`）、多租户作用域（`tenant_id = ?`）。
///
/// 注：此 trait 不要求实现 `Model`，因为作用域本身只是一个标记类型，
/// 真正的 Model 由泛型参数 `M` 携带。
pub trait GlobalScope {
    /// 作用域名称（用于调试）
    fn scope_name() -> &'static str;

    /// 返回需要追加的 WHERE 条件 SQL 片段
    ///
    /// 返回 `None` 表示无附加条件。
    /// 返回 `Some((sql, params))` 表示追加 `AND {sql}`，绑定 `params`。
    fn apply_scope(ctx: &HookContext) -> Option<(String, Vec<crate::value::Value>)>;
}

// =====================================================================
// SoftDeleteScope — 软删除全局作用域
// =====================================================================

/// 软删除全局作用域
///
/// 自动追加 `AND {soft_delete_field} IS NULL` 到所有查询。
/// 需配合 `SoftDelete` trait 使用。
pub struct SoftDeleteScope;

impl<M: SoftDelete> GlobalScope for (SoftDeleteScope, M) {
    fn scope_name() -> &'static str {
        "soft_delete"
    }

    fn apply_scope(_ctx: &HookContext) -> Option<(String, Vec<crate::value::Value>)> {
        // 使用完全限定语法避免与 Model::soft_delete_field 歧义
        let field = <M as SoftDelete>::soft_delete_field();
        Some((format!("{} IS NULL", field), vec![]))
    }
}

// =====================================================================
// TenantScope — 多租户全局作用域
// =====================================================================

/// 多租户全局作用域
///
/// 自动追加 `AND tenant_id = ?` 到所有查询，绑定 `ctx.tenant_id`。
/// 若 `ctx.tenant_id` 为 None，则不追加条件（允许跨租户查询，需调用方自行保证安全）。
pub struct TenantScope;

/// 多租户 Model trait
///
/// 实现此 trait 的 Model 自动获得 `TenantScope` 全局作用域。
pub trait TenantModel: crate::model::Model {
    /// 租户字段名（默认 `tenant_id`）
    fn tenant_field() -> &'static str {
        "tenant_id"
    }

    /// 获取当前行的租户 ID
    fn tenant_id(&self) -> i64;

    /// 设置租户 ID
    fn set_tenant_id(&mut self, tenant_id: i64);
}

impl<M: TenantModel> GlobalScope for (TenantScope, M) {
    fn scope_name() -> &'static str {
        "tenant"
    }

    fn apply_scope(ctx: &HookContext) -> Option<(String, Vec<crate::value::Value>)> {
        ctx.tenant_id.map(|tid| {
            (
                format!("{} = ?", <M as TenantModel>::tenant_field()),
                vec![crate::value::Value::I64(tid)],
            )
        })
    }
}

// =====================================================================
// HookRegistry — 钩子注册表（运行时钩子）
// =====================================================================

/// 运行时钩子函数类型
pub type HookFn = Arc<dyn Fn(&HookContext) -> HookResult<()> + Send + Sync>;

/// 钩子注册表
///
/// 支持运行时注册全局钩子函数，按事件类型分组。
/// 与 `Hookable` trait 互补：trait 用于编译期已知钩子，注册表用于运行时插件。
pub struct HookRegistry {
    hooks: RwLock<HashMap<HookEvent, Vec<HookFn>>>,
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl HookRegistry {
    /// 创建空注册表
    pub fn new() -> Self {
        Self {
            hooks: RwLock::new(HashMap::new()),
        }
    }

    /// 注册钩子
    pub fn register(&self, event: HookEvent, hook: HookFn) {
        // lock poisoned 时降级为 no-op，避免级联 panic。
        if let Ok(mut hooks) = self.hooks.write() {
            hooks.entry(event).or_default().push(hook);
        }
    }

    /// 执行某事件的所有钩子
    ///
    /// 按注册顺序执行，任一钩子失败则立即返回错误。
    pub fn dispatch(&self, event: HookEvent, ctx: &HookContext) -> HookResult<()> {
        let hooks = match self.hooks.read() {
            Ok(h) => h,
            Err(_) => return Ok(()), // lock poisoned → no-op
        };
        if let Some(fns) = hooks.get(&event) {
            for f in fns {
                f(ctx)?;
            }
        }
        Ok(())
    }

    /// 清除某事件的所有钩子
    pub fn clear(&self, event: HookEvent) {
        if let Ok(mut hooks) = self.hooks.write() {
            hooks.remove(&event);
        }
    }

    /// 清除所有钩子
    pub fn clear_all(&self) {
        if let Ok(mut hooks) = self.hooks.write() {
            hooks.clear();
        }
    }

    /// 获取某事件的钩子数量
    pub fn count(&self, event: HookEvent) -> usize {
        self.hooks
            .read()
            .map(|h| h.get(&event).map(|v| v.len()).unwrap_or(0))
            .unwrap_or(0)
    }
}

// =====================================================================
// ScopeRegistry — 全局作用域注册表
// =====================================================================

/// 全局作用域注册表
///
/// 管理多个 GlobalScope 的启用/禁用状态。
/// 典型用法：临时禁用软删除作用域以查询已删除行（`without_scope`）。
pub struct ScopeRegistry {
    disabled: RwLock<Vec<String>>,
}

impl Default for ScopeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ScopeRegistry {
    /// 创建空注册表（所有作用域默认启用）
    pub fn new() -> Self {
        Self {
            disabled: RwLock::new(Vec::new()),
        }
    }

    /// 禁用指定作用域
    pub fn disable(&self, scope_name: impl Into<String>) {
        if let Ok(mut disabled) = self.disabled.write() {
            let name = scope_name.into();
            if !disabled.contains(&name) {
                disabled.push(name);
            }
        }
    }

    /// 启用指定作用域
    pub fn enable(&self, scope_name: &str) {
        if let Ok(mut disabled) = self.disabled.write() {
            disabled.retain(|n| n != scope_name);
        }
    }

    /// 检查作用域是否启用
    pub fn is_enabled(&self, scope_name: &str) -> bool {
        self.disabled
            .read()
            .map(|d| !d.iter().any(|n| n == scope_name))
            .unwrap_or(true)
    }

    /// 在闭包内临时禁用作用域
    ///
    /// ```no_run
    /// # use sz_orm_core::hooks::ScopeRegistry;
    /// let registry = ScopeRegistry::new();
    /// registry.without_scope("soft_delete", || {
    ///     // 此处查询会包含已软删除的行
    /// });
    /// ```
    pub fn without_scope<F, R>(&self, scope_name: &str, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        self.disable(scope_name);
        let result = f();
        self.enable(scope_name);
        result
    }
}

// =====================================================================
// 测试
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_context_builder() {
        let ctx = HookContext::new()
            .with_tenant(42)
            .with_operator(1)
            .with_timestamp(1700000000);

        assert_eq!(ctx.tenant_id, Some(42));
        assert_eq!(ctx.operator_id, Some(1));
        assert_eq!(ctx.timestamp, 1700000000);
    }

    #[test]
    fn hook_context_metadata() {
        let mut ctx = HookContext::new();
        ctx.set_meta("source", "api");
        ctx.set_meta("ip", "127.0.0.1");

        assert_eq!(ctx.get_meta("source"), Some(&"api".to_string()));
        assert_eq!(ctx.get_meta("ip"), Some(&"127.0.0.1".to_string()));
        assert_eq!(ctx.get_meta("missing"), None);
    }

    #[test]
    fn hook_event_is_before_after() {
        assert!(HookEvent::BeforeInsert.is_before());
        assert!(!HookEvent::BeforeInsert.is_after());
        assert!(HookEvent::AfterInsert.is_after());
        assert!(!HookEvent::AfterInsert.is_before());
    }

    #[test]
    fn hook_registry_register_and_dispatch() {
        let registry = HookRegistry::new();
        let counter = Arc::new(std::sync::atomic::AtomicU32::new(0));

        let c = Arc::clone(&counter);
        registry.register(
            HookEvent::BeforeInsert,
            Arc::new(move |_ctx| {
                c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            }),
        );

        let ctx = HookContext::new();
        registry.dispatch(HookEvent::BeforeInsert, &ctx).unwrap();
        registry.dispatch(HookEvent::BeforeInsert, &ctx).unwrap();

        assert_eq!(
            counter.load(std::sync::atomic::Ordering::SeqCst),
            2
        );
    }

    #[test]
    fn hook_registry_dispatch_no_hooks() {
        let registry = HookRegistry::new();
        let ctx = HookContext::new();
        // 无钩子时 dispatch 应返回 Ok
        assert!(registry.dispatch(HookEvent::BeforeInsert, &ctx).is_ok());
    }

    #[test]
    fn hook_registry_clear() {
        let registry = HookRegistry::new();
        registry.register(
            HookEvent::BeforeInsert,
            Arc::new(|_ctx| Ok(())),
        );
        assert_eq!(registry.count(HookEvent::BeforeInsert), 1);

        registry.clear(HookEvent::BeforeInsert);
        assert_eq!(registry.count(HookEvent::BeforeInsert), 0);
    }

    #[test]
    fn hook_registry_clear_all() {
        let registry = HookRegistry::new();
        registry.register(HookEvent::BeforeInsert, Arc::new(|_ctx| Ok(())));
        registry.register(HookEvent::AfterInsert, Arc::new(|_ctx| Ok(())));
        registry.register(HookEvent::BeforeUpdate, Arc::new(|_ctx| Ok(())));

        registry.clear_all();
        assert_eq!(registry.count(HookEvent::BeforeInsert), 0);
        assert_eq!(registry.count(HookEvent::AfterInsert), 0);
        assert_eq!(registry.count(HookEvent::BeforeUpdate), 0);
    }

    #[test]
    fn scope_registry_enable_disable() {
        let registry = ScopeRegistry::new();

        assert!(registry.is_enabled("soft_delete"));
        assert!(registry.is_enabled("tenant"));

        registry.disable("soft_delete");
        assert!(!registry.is_enabled("soft_delete"));
        assert!(registry.is_enabled("tenant"));

        registry.enable("soft_delete");
        assert!(registry.is_enabled("soft_delete"));
    }

    #[test]
    fn scope_registry_without_scope() {
        let registry = ScopeRegistry::new();
        assert!(registry.is_enabled("soft_delete"));

        let result = registry.without_scope("soft_delete", || {
            assert!(!registry.is_enabled("soft_delete"));
            42
        });

        assert_eq!(result, 42);
        assert!(registry.is_enabled("soft_delete"));
    }

    #[test]
    fn hook_registry_short_circuit_on_error() {
        let registry = HookRegistry::new();
        let called = Arc::new(std::sync::atomic::AtomicU32::new(0));

        let c1 = Arc::clone(&called);
        registry.register(
            HookEvent::BeforeInsert,
            Arc::new(move |_ctx| {
                c1.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            }),
        );

        registry.register(
            HookEvent::BeforeInsert,
            Arc::new(|_ctx| Err(DbError::Hook("second hook failed".into()))),
        );

        let c3 = Arc::clone(&called);
        registry.register(
            HookEvent::BeforeInsert,
            Arc::new(move |_ctx| {
                c3.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            }),
        );

        let ctx = HookContext::new();
        let result = registry.dispatch(HookEvent::BeforeInsert, &ctx);

        assert!(result.is_err());
        // 第一个钩子执行，第二个返回错误，第三个不应执行
        assert_eq!(
            called.load(std::sync::atomic::Ordering::SeqCst),
            1
        );
    }
}
