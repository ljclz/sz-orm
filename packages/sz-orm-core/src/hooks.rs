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
///
/// 在 think-orm 风格的 6 个细粒度 insert/update/delete 事件基础上，
/// 进一步提供 6 个通用写入事件（write/save/restore）：
/// - `BeforeWrite` / `AfterWrite`：任何写入操作（insert/update）前后触发
/// - `BeforeSave` / `AfterSave`：插入或更新保存前后触发（与 write 等价，命名借用 Rails/ActiveRecord）
/// - `BeforeRestore` / `AfterRestore`：软删除恢复前后触发
///
/// 此外还提供 4 个查询/验证事件：
/// - `BeforeFind` / `AfterFind`：单行 SELECT 前后触发（可用于查询缓存、审计）
/// - `BeforeValidate` / `AfterValidate`：数据验证前后触发（写入前的业务规则校验）
///
/// 触发顺序示例（执行 INSERT 时）：
/// `BeforeWrite` → `BeforeSave` → `BeforeValidate` → `BeforeInsert` → (INSERT) → `AfterInsert` → `AfterSave` → `AfterWrite`
///
/// 触发顺序示例（执行 SELECT 时）：
/// `BeforeFind` → (SELECT) → `AfterFind`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookEvent {
    BeforeInsert,
    AfterInsert,
    BeforeUpdate,
    AfterUpdate,
    BeforeDelete,
    AfterDelete,
    /// 通用写入前：insert 或 update 前均触发
    BeforeWrite,
    /// 通用写入后：insert 或 update 后均触发
    AfterWrite,
    /// 保存前（与 BeforeWrite 等价，命名风格不同）
    BeforeSave,
    /// 保存后（与 AfterWrite 等价，命名风格不同）
    AfterSave,
    /// 软删除恢复前
    BeforeRestore,
    /// 软删除恢复后
    AfterRestore,
    /// 查询前（单行 SELECT 前触发，可用于查询缓存预热、审计日志）
    BeforeFind,
    /// 查询后（单行 SELECT 后触发，可用于查询结果后处理、缓存填充）
    AfterFind,
    /// 数据验证前（写入前的业务规则校验，在 before_insert/before_update 之前触发）
    BeforeValidate,
    /// 数据验证后（验证完成后触发，可清理临时状态）
    AfterValidate,
}

impl HookEvent {
    /// 是否为 before 事件
    pub fn is_before(&self) -> bool {
        matches!(
            self,
            HookEvent::BeforeInsert
                | HookEvent::BeforeUpdate
                | HookEvent::BeforeDelete
                | HookEvent::BeforeWrite
                | HookEvent::BeforeSave
                | HookEvent::BeforeRestore
                | HookEvent::BeforeFind
                | HookEvent::BeforeValidate
        )
    }

    /// 是否为 after 事件
    pub fn is_after(&self) -> bool {
        matches!(
            self,
            HookEvent::AfterInsert
                | HookEvent::AfterUpdate
                | HookEvent::AfterDelete
                | HookEvent::AfterWrite
                | HookEvent::AfterSave
                | HookEvent::AfterRestore
                | HookEvent::AfterFind
                | HookEvent::AfterValidate
        )
    }

    /// 是否为通用写入事件（write/save）
    pub fn is_write_level(&self) -> bool {
        matches!(
            self,
            HookEvent::BeforeWrite
                | HookEvent::AfterWrite
                | HookEvent::BeforeSave
                | HookEvent::AfterSave
        )
    }

    /// 是否为查询事件（find）
    pub fn is_find_level(&self) -> bool {
        matches!(self, HookEvent::BeforeFind | HookEvent::AfterFind)
    }

    /// 是否为验证事件（validate）
    pub fn is_validate_level(&self) -> bool {
        matches!(self, HookEvent::BeforeValidate | HookEvent::AfterValidate)
    }

    /// 是否为细粒度事件（v0.2.0+ 新增的事件）
    pub fn is_fine_grained(&self) -> bool {
        self.is_write_level()
            || self.is_find_level()
            || self.is_validate_level()
            || matches!(self, HookEvent::BeforeRestore | HookEvent::AfterRestore)
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
///
/// # 细粒度事件（v0.2.0+）
///
/// 在原 6 个 insert/update/delete 钩子之外，新增 6 个通用钩子：
/// - `before_write` / `after_write`：任何写入（insert 或 update）前后均触发
/// - `before_save` / `after_save`：保存前后触发（与 write 等价，命名风格不同）
/// - `before_restore` / `after_restore`：软删除恢复前后触发
///
/// 调用方应在执行 INSERT 前依次调用 `before_write` → `before_save` → `before_insert`，
/// INSERT 完成后依次调用 `after_insert` → `after_save` → `after_write`。
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

    /// 通用写入前钩子：insert 或 update 前均触发（默认 no-op）
    ///
    /// 适合用于审计日志、统一字段填充（如 updated_at = now()）等场景。
    fn before_write(_ctx: &mut HookContext) -> HookResult<()> {
        Ok(())
    }

    /// 通用写入后钩子：insert 或 update 后均触发（默认 no-op）
    fn after_write(_ctx: &HookContext, _id: &Self::PrimaryKey) -> HookResult<()> {
        Ok(())
    }

    /// 保存前钩子（与 `before_write` 等价，命名风格不同，默认 no-op）
    fn before_save(_ctx: &mut HookContext) -> HookResult<()> {
        Ok(())
    }

    /// 保存后钩子（与 `after_write` 等价，命名风格不同，默认 no-op）
    fn after_save(_ctx: &HookContext, _id: &Self::PrimaryKey) -> HookResult<()> {
        Ok(())
    }

    /// 软删除恢复前钩子（默认 no-op）
    ///
    /// 当软删除行被恢复（`UPDATE deleted_at = NULL`）时触发。
    fn before_restore(_ctx: &mut HookContext, _id: &Self::PrimaryKey) -> HookResult<()> {
        Ok(())
    }

    /// 软删除恢复后钩子（默认 no-op）
    fn after_restore(_ctx: &HookContext, _id: &Self::PrimaryKey) -> HookResult<()> {
        Ok(())
    }

    /// 单行查询前钩子（默认 no-op）
    ///
    /// 在执行 `SELECT * FROM ... WHERE pk = ?` 前触发。
    /// 适合用于查询缓存预热、查询审计日志、强制查询条件注入等。
    fn before_find(_ctx: &mut HookContext, _id: &Self::PrimaryKey) -> HookResult<()> {
        Ok(())
    }

    /// 单行查询后钩子（默认 no-op）
    ///
    /// 在 `SELECT * FROM ... WHERE pk = ?` 返回结果后触发。
    /// 适合用于查询结果缓存填充、行级权限校验等。
    fn after_find(_ctx: &HookContext, _id: &Self::PrimaryKey) -> HookResult<()> {
        Ok(())
    }

    /// 数据验证前钩子（默认 no-op）
    ///
    /// 在写入前的业务规则校验之前触发，调用顺序：
    /// `before_write` → `before_save` → `before_validate` → `validate` → `after_validate` → `before_insert`
    ///
    /// 适合用于字段非空校验、字段格式校验、跨字段一致性校验等。
    /// 失败时返回 `Err(DbError::Validation(...))`，会短路后续 before_insert。
    fn before_validate(_ctx: &mut HookContext) -> HookResult<()> {
        Ok(())
    }

    /// 数据验证逻辑（默认 no-op）
    ///
    /// 在 `before_validate` 之后、`after_validate` 之前调用。
    /// Model 可按需 override 此方法以实现实际的业务规则校验。
    /// 失败时返回 `Err(DbError::Validation(...))`，会短路后续 `after_validate` 与 `before_insert`。
    fn validate(_ctx: &mut HookContext) -> HookResult<()> {
        Ok(())
    }

    /// 数据验证后钩子（默认 no-op）
    ///
    /// 验证成功后触发，可用于清理临时状态、记录验证日志。
    fn after_validate(_ctx: &HookContext) -> HookResult<()> {
        Ok(())
    }
}

/// 钩子执行辅助工具
///
/// 封装常见的钩子触发顺序，避免业务代码手动逐个调用。
pub struct HookDispatcher;

impl HookDispatcher {
    /// INSERT 操作的完整钩子序列：
    /// `before_write` → `before_save` → `before_validate` → `validate` → `after_validate`
    /// → `before_insert` → (执行) → `after_insert` → `after_save` → `after_write`
    ///
    /// `f` 为执行实际 INSERT 操作的闭包，返回插入后的主键。
    pub fn insert<M, F>(ctx: &mut HookContext, f: F) -> HookResult<M::PrimaryKey>
    where
        M: Hookable,
        F: FnOnce(&mut HookContext) -> HookResult<M::PrimaryKey>,
    {
        M::before_write(ctx)?;
        M::before_save(ctx)?;
        M::before_validate(ctx)?;
        M::validate(ctx)?;
        M::after_validate(ctx)?;
        M::before_insert(ctx)?;
        let id = f(ctx)?;
        M::after_insert(ctx, &id)?;
        M::after_save(ctx, &id)?;
        M::after_write(ctx, &id)?;
        Ok(id)
    }

    /// UPDATE 操作的完整钩子序列（同 INSERT，含 validate）
    pub fn update<M, F>(ctx: &mut HookContext, id: &M::PrimaryKey, f: F) -> HookResult<()>
    where
        M: Hookable,
        F: FnOnce(&mut HookContext) -> HookResult<()>,
    {
        M::before_write(ctx)?;
        M::before_save(ctx)?;
        M::before_validate(ctx)?;
        M::validate(ctx)?;
        M::after_validate(ctx)?;
        M::before_update(ctx, id)?;
        f(ctx)?;
        M::after_update(ctx, id)?;
        M::after_save(ctx, id)?;
        M::after_write(ctx, id)?;
        Ok(())
    }

    /// DELETE 操作的完整钩子序列
    pub fn delete<M, F>(ctx: &mut HookContext, id: &M::PrimaryKey, f: F) -> HookResult<()>
    where
        M: Hookable,
        F: FnOnce(&mut HookContext) -> HookResult<()>,
    {
        M::before_delete(ctx, id)?;
        f(ctx)?;
        M::after_delete(ctx, id)?;
        Ok(())
    }

    /// RESTORE 操作（软删除恢复）的完整钩子序列
    pub fn restore<M, F>(ctx: &mut HookContext, id: &M::PrimaryKey, f: F) -> HookResult<()>
    where
        M: Hookable,
        F: FnOnce(&mut HookContext) -> HookResult<()>,
    {
        M::before_restore(ctx, id)?;
        f(ctx)?;
        M::after_restore(ctx, id)?;
        Ok(())
    }

    /// FIND 操作（单行查询）的完整钩子序列：
    /// `before_find` → (执行 SELECT) → `after_find`
    ///
    /// `f` 为执行实际 SELECT 操作的闭包，返回查询到的实例。
    pub fn find<M, F>(ctx: &mut HookContext, id: &M::PrimaryKey, f: F) -> HookResult<()>
    where
        M: Hookable,
        F: FnOnce(&mut HookContext) -> HookResult<()>,
    {
        M::before_find(ctx, id)?;
        f(ctx)?;
        M::after_find(ctx, id)?;
        Ok(())
    }

    /// 仅触发验证钩子序列（不执行实际写入）：
    /// `before_validate` → `validate` → `after_validate`
    ///
    /// 适用于调用方需独立校验数据、不立刻写入的场景。
    pub fn validate<M>(ctx: &mut HookContext) -> HookResult<()>
    where
        M: Hookable,
    {
        M::before_validate(ctx)?;
        M::validate(ctx)?;
        M::after_validate(ctx)?;
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

        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 2);
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
        registry.register(HookEvent::BeforeInsert, Arc::new(|_ctx| Ok(())));
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
        assert_eq!(called.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    // ===== 细粒度钩子（v0.2.0+）测试 =====

    #[test]
    fn hook_event_is_write_level() {
        assert!(HookEvent::BeforeWrite.is_write_level());
        assert!(HookEvent::AfterWrite.is_write_level());
        assert!(HookEvent::BeforeSave.is_write_level());
        assert!(HookEvent::AfterSave.is_write_level());
        assert!(!HookEvent::BeforeInsert.is_write_level());
        assert!(!HookEvent::AfterDelete.is_write_level());
        assert!(!HookEvent::BeforeRestore.is_write_level());
    }

    #[test]
    fn hook_event_before_after_covers_new_variants() {
        assert!(HookEvent::BeforeWrite.is_before());
        assert!(HookEvent::BeforeSave.is_before());
        assert!(HookEvent::BeforeRestore.is_before());
        assert!(HookEvent::AfterWrite.is_after());
        assert!(HookEvent::AfterSave.is_after());
        assert!(HookEvent::AfterRestore.is_after());
        assert!(!HookEvent::AfterWrite.is_before());
        assert!(!HookEvent::BeforeWrite.is_after());
    }

    #[test]
    fn hook_registry_supports_new_events() {
        let registry = HookRegistry::new();
        let counter = Arc::new(std::sync::atomic::AtomicU32::new(0));

        for event in [
            HookEvent::BeforeWrite,
            HookEvent::AfterWrite,
            HookEvent::BeforeSave,
            HookEvent::AfterSave,
            HookEvent::BeforeRestore,
            HookEvent::AfterRestore,
        ] {
            let c = Arc::clone(&counter);
            registry.register(
                event,
                Arc::new(move |_ctx| {
                    c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Ok(())
                }),
            );
        }

        let ctx = HookContext::new();
        for event in [
            HookEvent::BeforeWrite,
            HookEvent::AfterWrite,
            HookEvent::BeforeSave,
            HookEvent::AfterSave,
            HookEvent::BeforeRestore,
            HookEvent::AfterRestore,
        ] {
            registry.dispatch(event, &ctx).unwrap();
        }

        assert_eq!(
            counter.load(std::sync::atomic::Ordering::SeqCst),
            6,
            "所有细粒度事件均应被正确注册与触发"
        );
    }

    // ===== HookDispatcher 测试 =====

    struct DispatchTestModel;
    impl crate::model::Model for DispatchTestModel {
        type PrimaryKey = i64;
        fn table_name() -> &'static str {
            "dispatch_test"
        }
        fn pk(&self) -> Self::PrimaryKey {
            0
        }
        fn set_pk(&mut self, _pk: Self::PrimaryKey) {}
    }

    // 全局计数器：用于在 after_* 钩子中记录调用次数（&HookContext 不可变）
    static DISPATCH_CALLS: std::sync::OnceLock<Arc<std::sync::atomic::AtomicU32>> =
        std::sync::OnceLock::new();

    fn dispatch_calls() -> Arc<std::sync::atomic::AtomicU32> {
        DISPATCH_CALLS
            .get_or_init(|| Arc::new(std::sync::atomic::AtomicU32::new(0)))
            .clone()
    }

    impl Hookable for DispatchTestModel {
        fn before_write(ctx: &mut HookContext) -> HookResult<()> {
            ctx.set_meta("before_write", "1");
            Ok(())
        }
        fn before_save(ctx: &mut HookContext) -> HookResult<()> {
            ctx.set_meta("before_save", "1");
            Ok(())
        }
        fn before_validate(ctx: &mut HookContext) -> HookResult<()> {
            ctx.set_meta("before_validate", "1");
            Ok(())
        }
        fn after_validate(ctx: &HookContext) -> HookResult<()> {
            assert_eq!(ctx.get_meta("before_validate"), Some(&"1".to_string()));
            ctx_set_meta_for_after(ctx, "after_validate", "1");
            Ok(())
        }
        fn before_insert(ctx: &mut HookContext) -> HookResult<()> {
            ctx.set_meta("before_insert", "1");
            Ok(())
        }
        fn after_insert(ctx: &HookContext, _id: &Self::PrimaryKey) -> HookResult<()> {
            assert_eq!(ctx.get_meta("before_write"), Some(&"1".to_string()));
            assert_eq!(ctx.get_meta("before_save"), Some(&"1".to_string()));
            assert_eq!(ctx.get_meta("before_insert"), Some(&"1".to_string()));
            assert_eq!(ctx.get_meta("before_validate"), Some(&"1".to_string()));
            Ok(())
        }
        fn after_save(_ctx: &HookContext, _id: &Self::PrimaryKey) -> HookResult<()> {
            dispatch_calls().fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
        fn after_write(_ctx: &HookContext, _id: &Self::PrimaryKey) -> HookResult<()> {
            dispatch_calls().fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
        fn before_find(ctx: &mut HookContext, _id: &Self::PrimaryKey) -> HookResult<()> {
            ctx.set_meta("before_find", "1");
            Ok(())
        }
        fn after_find(ctx: &HookContext, _id: &Self::PrimaryKey) -> HookResult<()> {
            assert_eq!(ctx.get_meta("before_find"), Some(&"1".to_string()));
            ctx_set_meta_for_after(ctx, "after_find", "1");
            Ok(())
        }
    }

    // after_* 钩子接收 &HookContext（不可变），无法直接 set_meta
    // 使用 AtomicU32 计数器记录 after_* 调用次数（无锁，无线程安全问题）
    static AFTER_VALIDATE_COUNT: std::sync::atomic::AtomicU32 =
        std::sync::atomic::AtomicU32::new(0);
    static AFTER_FIND_COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

    fn ctx_set_meta_for_after(_ctx: &HookContext, key: &str, _value: &str) {
        match key {
            "after_validate" => {
                AFTER_VALIDATE_COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
            "after_find" => {
                AFTER_FIND_COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
            _ => {}
        }
    }

    fn after_call_was(key: &str) -> bool {
        match key {
            "after_validate" => AFTER_VALIDATE_COUNT.load(std::sync::atomic::Ordering::SeqCst) > 0,
            "after_find" => AFTER_FIND_COUNT.load(std::sync::atomic::Ordering::SeqCst) > 0,
            _ => false,
        }
    }

    fn reset_after_calls() {
        AFTER_VALIDATE_COUNT.store(0, std::sync::atomic::Ordering::SeqCst);
        AFTER_FIND_COUNT.store(0, std::sync::atomic::Ordering::SeqCst);
    }

    // 串行化锁：全局静态计数器是共享的，并行测试会互相干扰
    static HOOK_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn hook_dispatcher_insert_full_sequence() {
        let _guard = HOOK_TEST_LOCK.lock().unwrap();
        dispatch_calls().store(0, std::sync::atomic::Ordering::SeqCst);
        reset_after_calls();
        let mut ctx = HookContext::new();
        let id = HookDispatcher::insert::<DispatchTestModel, _>(&mut ctx, |_ctx| Ok(42_i64));
        assert!(id.is_ok());
        assert_eq!(id.unwrap(), 42);
        // 验证 before 钩子都已执行
        assert_eq!(ctx.get_meta("before_write"), Some(&"1".to_string()));
        assert_eq!(ctx.get_meta("before_save"), Some(&"1".to_string()));
        assert_eq!(ctx.get_meta("before_insert"), Some(&"1".to_string()));
        // 验证 before_validate + after_validate 都已执行
        assert_eq!(ctx.get_meta("before_validate"), Some(&"1".to_string()));
        assert!(after_call_was("after_validate"));
        // 验证 after_save + after_write 都已执行
        assert_eq!(
            dispatch_calls().load(std::sync::atomic::Ordering::SeqCst),
            2
        );
    }

    #[test]
    fn hook_dispatcher_insert_short_circuit_on_before_write_error() {
        struct ErrorModel;
        impl crate::model::Model for ErrorModel {
            type PrimaryKey = i64;
            fn table_name() -> &'static str {
                "error_model"
            }
            fn pk(&self) -> Self::PrimaryKey {
                0
            }
            fn set_pk(&mut self, _pk: Self::PrimaryKey) {}
        }
        impl Hookable for ErrorModel {
            fn before_write(_ctx: &mut HookContext) -> HookResult<()> {
                Err(DbError::Hook("before_write failed".into()))
            }
        }

        let mut ctx = HookContext::new();
        let result = HookDispatcher::insert::<ErrorModel, _>(&mut ctx, |_ctx| Ok(1_i64));
        assert!(result.is_err());
        // before_write 失败，不应执行实际操作
    }

    #[test]
    fn hook_dispatcher_insert_short_circuit_on_before_validate_error() {
        struct ValidationFailModel;
        impl crate::model::Model for ValidationFailModel {
            type PrimaryKey = i64;
            fn table_name() -> &'static str {
                "validation_fail"
            }
            fn pk(&self) -> Self::PrimaryKey {
                0
            }
            fn set_pk(&mut self, _pk: Self::PrimaryKey) {}
        }
        impl Hookable for ValidationFailModel {
            fn before_validate(_ctx: &mut HookContext) -> HookResult<()> {
                Err(DbError::Validation("name is required".into()))
            }
        }

        let mut ctx = HookContext::new();
        let called = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let c = Arc::clone(&called);
        let result = HookDispatcher::insert::<ValidationFailModel, _>(&mut ctx, move |_ctx| {
            c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(1_i64)
        });
        assert!(result.is_err());
        // before_validate 失败，实际 INSERT 不应执行
        assert_eq!(
            called.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "before_validate 失败应短路 INSERT 操作"
        );
        // 错误类型应是 Validation
        match result.unwrap_err() {
            DbError::Validation(msg) => assert_eq!(msg, "name is required"),
            other => panic!("期望 Validation 错误，得到 {:?}", other),
        }
    }

    #[test]
    fn hook_dispatcher_update_full_sequence() {
        let _guard = HOOK_TEST_LOCK.lock().unwrap();
        dispatch_calls().store(0, std::sync::atomic::Ordering::SeqCst);
        reset_after_calls();
        let mut ctx = HookContext::new();
        let result =
            HookDispatcher::update::<DispatchTestModel, _>(&mut ctx, &42_i64, |_ctx| Ok(()));
        assert!(result.is_ok());
        // update 也会触发 after_save + after_write
        assert_eq!(
            dispatch_calls().load(std::sync::atomic::Ordering::SeqCst),
            2
        );
        // update 也会触发 validate
        assert_eq!(ctx.get_meta("before_validate"), Some(&"1".to_string()));
        assert!(after_call_was("after_validate"));
    }

    #[test]
    fn hook_dispatcher_delete_full_sequence() {
        let mut ctx = HookContext::new();
        let result =
            HookDispatcher::delete::<DispatchTestModel, _>(&mut ctx, &42_i64, |_ctx| Ok(()));
        assert!(result.is_ok());
    }

    #[test]
    fn hook_dispatcher_restore_full_sequence() {
        let mut ctx = HookContext::new();
        let result =
            HookDispatcher::restore::<DispatchTestModel, _>(&mut ctx, &42_i64, |_ctx| Ok(()));
        assert!(result.is_ok());
    }

    #[test]
    fn hook_dispatcher_find_full_sequence() {
        let _guard = HOOK_TEST_LOCK.lock().unwrap();
        dispatch_calls().store(0, std::sync::atomic::Ordering::SeqCst);
        reset_after_calls();
        let mut ctx = HookContext::new();
        let called = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let c = Arc::clone(&called);
        let result = HookDispatcher::find::<DispatchTestModel, _>(&mut ctx, &42_i64, move |_ctx| {
            c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        });
        assert!(result.is_ok());
        assert_eq!(
            called.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "SELECT 操作应执行一次"
        );
        // 验证 before_find + after_find 都已执行
        assert_eq!(ctx.get_meta("before_find"), Some(&"1".to_string()));
        assert!(after_call_was("after_find"));
    }

    #[test]
    fn hook_dispatcher_find_short_circuit_on_before_find_error() {
        struct FindFailModel;
        impl crate::model::Model for FindFailModel {
            type PrimaryKey = i64;
            fn table_name() -> &'static str {
                "find_fail"
            }
            fn pk(&self) -> Self::PrimaryKey {
                0
            }
            fn set_pk(&mut self, _pk: Self::PrimaryKey) {}
        }
        impl Hookable for FindFailModel {
            fn before_find(_ctx: &mut HookContext, _id: &Self::PrimaryKey) -> HookResult<()> {
                Err(DbError::Hook("before_find blocked".into()))
            }
        }

        let mut ctx = HookContext::new();
        let called = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let c = Arc::clone(&called);
        let result = HookDispatcher::find::<FindFailModel, _>(&mut ctx, &1_i64, move |_ctx| {
            c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(
            called.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "before_find 失败应短路 SELECT"
        );
    }

    #[test]
    fn hook_dispatcher_validate_standalone() {
        reset_after_calls();
        let mut ctx = HookContext::new();
        let result = HookDispatcher::validate::<DispatchTestModel>(&mut ctx);
        assert!(result.is_ok());
        assert_eq!(ctx.get_meta("before_validate"), Some(&"1".to_string()));
        assert!(after_call_was("after_validate"));
    }

    #[test]
    fn hook_event_is_find_level_and_is_validate_level() {
        assert!(HookEvent::BeforeFind.is_find_level());
        assert!(HookEvent::AfterFind.is_find_level());
        assert!(HookEvent::BeforeValidate.is_validate_level());
        assert!(HookEvent::AfterValidate.is_validate_level());
        assert!(!HookEvent::BeforeInsert.is_find_level());
        assert!(!HookEvent::BeforeInsert.is_validate_level());
        assert!(!HookEvent::BeforeWrite.is_find_level());
        assert!(!HookEvent::BeforeWrite.is_validate_level());
    }

    #[test]
    fn hook_event_is_fine_grained_covers_all_v02_events() {
        // v0.2.0+ 新增的事件均应被识别为细粒度
        assert!(HookEvent::BeforeWrite.is_fine_grained());
        assert!(HookEvent::AfterWrite.is_fine_grained());
        assert!(HookEvent::BeforeSave.is_fine_grained());
        assert!(HookEvent::AfterSave.is_fine_grained());
        assert!(HookEvent::BeforeRestore.is_fine_grained());
        assert!(HookEvent::AfterRestore.is_fine_grained());
        assert!(HookEvent::BeforeFind.is_fine_grained());
        assert!(HookEvent::AfterFind.is_fine_grained());
        assert!(HookEvent::BeforeValidate.is_fine_grained());
        assert!(HookEvent::AfterValidate.is_fine_grained());
        // 原生 6 事件不应标记为细粒度
        assert!(!HookEvent::BeforeInsert.is_fine_grained());
        assert!(!HookEvent::AfterInsert.is_fine_grained());
        assert!(!HookEvent::BeforeUpdate.is_fine_grained());
        assert!(!HookEvent::AfterUpdate.is_fine_grained());
        assert!(!HookEvent::BeforeDelete.is_fine_grained());
        assert!(!HookEvent::AfterDelete.is_fine_grained());
    }

    #[test]
    fn hook_registry_supports_find_and_validate_events() {
        let registry = HookRegistry::new();
        let counter = Arc::new(std::sync::atomic::AtomicU32::new(0));

        for event in [
            HookEvent::BeforeFind,
            HookEvent::AfterFind,
            HookEvent::BeforeValidate,
            HookEvent::AfterValidate,
        ] {
            let c = Arc::clone(&counter);
            registry.register(
                event,
                Arc::new(move |_ctx| {
                    c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Ok(())
                }),
            );
        }

        let ctx = HookContext::new();
        for event in [
            HookEvent::BeforeFind,
            HookEvent::AfterFind,
            HookEvent::BeforeValidate,
            HookEvent::AfterValidate,
        ] {
            registry.dispatch(event, &ctx).unwrap();
        }

        assert_eq!(
            counter.load(std::sync::atomic::Ordering::SeqCst),
            4,
            "find/validate 钩子应能被注册与触发"
        );
    }

    #[test]
    fn db_error_validation_error_code_and_display() {
        let err = DbError::Validation("name required".into());
        assert_eq!(err.error_code(), "DB021");
        assert_eq!(format!("{}", err), "Validation error: name required");
    }
}
