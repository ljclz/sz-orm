//! 行为系统（Behaviors）— 可插拔代码复用单元
//!
//! 对应文档 6.8 节改进项 34（Behaviors 行为系统）+ 35（自动填充时间戳）+ 36（自动填充操作人）。
//!
//! # 核心概念
//!
//! - **Behavior**：可插拔的代码复用单元，订阅一组生命周期事件并自动执行逻辑
//! - **TimestampBehavior**：自动填充 `created_at`/`updated_at` 时间戳
//! - **BlameableBehavior**：自动填充 `created_by`/`updated_by` 操作人 ID
//! - **BehaviorRegistry**：Behavior 注册中心，管理多个 Behavior 的分发
//!
//! # 设计灵感
//!
//! - Yii2 `TimestampBehavior` / `BlameableBehavior` / `AttributeBehavior`
//! - Hibernate `@CreationTimestamp` / `@UpdateTimestamp`
//! - MyBatis-Plus `MetaObjectHandler`
//!
//! # 使用示例
//!
//! ```no_run
//! use sz_orm_core::behaviors::{Behavior, TimestampBehavior, BlameableBehavior, BehaviorRegistry};
//! use sz_orm_core::hooks::HookContext;
//! use sz_orm_core::Value;
//! use std::collections::HashMap;
//!
//! let mut registry = BehaviorRegistry::new();
//! registry.register(Box::new(TimestampBehavior::new("created_at", "updated_at")));
//! registry.register(Box::new(BlameableBehavior::new("created_by", "updated_by")));
//!
//! let ctx = HookContext::default().with_operator(42).with_timestamp(1700000000);
//! let mut attrs = HashMap::new();
//! registry.before_insert(&ctx, &mut attrs).unwrap();
//! assert_eq!(attrs.get("created_at"), Some(&Value::I64(1700000000)));
//! assert_eq!(attrs.get("created_by"), Some(&Value::I64(42)));
//! ```

use crate::error::DbError;
use crate::hooks::HookContext;
use crate::Value;
use parking_lot::RwLock;
use std::collections::HashMap;

/// Behavior 处理结果
pub type BehaviorResult<T> = Result<T, DbError>;

/// 行为 trait — 可插拔代码复用单元
///
/// 每个 Behavior 订阅一组生命周期事件，在事件触发时自动执行逻辑。
/// 默认所有方法都是空实现，Behavior 只需重写关心的方法。
pub trait Behavior: Send + Sync {
    /// Behavior 名称（用于识别、去重、调试）
    fn name(&self) -> &'static str;

    /// 在 insert 前触发（默认空实现）
    fn before_insert(
        &self,
        _ctx: &HookContext,
        _attrs: &mut HashMap<String, Value>,
    ) -> BehaviorResult<()> {
        Ok(())
    }

    /// 在 update 前触发（默认空实现）
    fn before_update(
        &self,
        _ctx: &HookContext,
        _attrs: &mut HashMap<String, Value>,
    ) -> BehaviorResult<()> {
        Ok(())
    }

    /// 在 delete 前触发（默认空实现）
    fn before_delete(
        &self,
        _ctx: &HookContext,
        _attrs: &mut HashMap<String, Value>,
    ) -> BehaviorResult<()> {
        Ok(())
    }

    /// 在 find 后触发（默认空实现，可用于字段后处理）
    fn after_find(
        &self,
        _ctx: &HookContext,
        _attrs: &mut HashMap<String, Value>,
    ) -> BehaviorResult<()> {
        Ok(())
    }
}

// ============================================================================
// TimestampBehavior — 自动填充时间戳
// ============================================================================
//
// 对应：Yii2 `TimestampBehavior` / Hibernate `@CreationTimestamp`+`@UpdateTimestamp`
// / MyBatis-Plus `MetaObjectHandler`
//
// - before_insert：填充 created_at + updated_at
// - before_update：填充 updated_at
//
// 时间戳取自 HookContext.timestamp（Unix 微秒），由调用方保证一致性。

/// 自动填充时间戳 Behavior
///
/// # 示例
///
/// ```
/// use sz_orm_core::behaviors::{Behavior, TimestampBehavior};
/// use sz_orm_core::hooks::HookContext;
/// use sz_orm_core::Value;
/// use std::collections::HashMap;
///
/// let b = TimestampBehavior::new("created_at", "updated_at");
/// let ctx = HookContext::default().with_timestamp(1700000000);
/// let mut attrs = HashMap::new();
/// b.before_insert(&ctx, &mut attrs).unwrap();
/// assert_eq!(attrs.get("created_at"), Some(&Value::I64(1700000000)));
/// assert_eq!(attrs.get("updated_at"), Some(&Value::I64(1700000000)));
/// ```
pub struct TimestampBehavior {
    /// 创建时间字段名（默认 "created_at"）
    pub created_field: &'static str,
    /// 更新时间字段名（默认 "updated_at"）
    pub updated_field: &'static str,
}

impl TimestampBehavior {
    /// 创建默认配置的 TimestampBehavior（字段名 created_at/updated_at）
    pub fn new(created_field: &'static str, updated_field: &'static str) -> Self {
        Self {
            created_field,
            updated_field,
        }
    }

    /// 使用默认字段名（created_at/updated_at）
    pub fn default_fields() -> Self {
        Self::new("created_at", "updated_at")
    }
}

impl Behavior for TimestampBehavior {
    fn name(&self) -> &'static str {
        "TimestampBehavior"
    }

    fn before_insert(
        &self,
        ctx: &HookContext,
        attrs: &mut HashMap<String, Value>,
    ) -> BehaviorResult<()> {
        let ts = Value::I64(ctx.timestamp as i64);
        attrs.insert(self.created_field.to_string(), ts.clone());
        attrs.insert(self.updated_field.to_string(), ts);
        Ok(())
    }

    fn before_update(
        &self,
        ctx: &HookContext,
        attrs: &mut HashMap<String, Value>,
    ) -> BehaviorResult<()> {
        attrs.insert(
            self.updated_field.to_string(),
            Value::I64(ctx.timestamp as i64),
        );
        Ok(())
    }
}

// ============================================================================
// BlameableBehavior — 自动填充操作人
// ============================================================================
//
// 对应：Yii2 `BlameableBehavior` / Spring Security `AuditorAware`
//
// - before_insert：填充 created_by + updated_by
// - before_update：填充 updated_by
//
// 操作人 ID 取自 HookContext.operator_id。

/// 自动填充操作人 Behavior
///
/// # 示例
///
/// ```
/// use sz_orm_core::behaviors::{Behavior, BlameableBehavior};
/// use sz_orm_core::hooks::HookContext;
/// use sz_orm_core::Value;
/// use std::collections::HashMap;
///
/// let b = BlameableBehavior::new("created_by", "updated_by");
/// let ctx = HookContext::default().with_operator(42);
/// let mut attrs = HashMap::new();
/// b.before_insert(&ctx, &mut attrs).unwrap();
/// assert_eq!(attrs.get("created_by"), Some(&Value::I64(42)));
/// assert_eq!(attrs.get("updated_by"), Some(&Value::I64(42)));
/// ```
pub struct BlameableBehavior {
    /// 创建人字段名（默认 "created_by"）
    pub created_field: &'static str,
    /// 更新人字段名（默认 "updated_by"）
    pub updated_field: &'static str,
}

impl BlameableBehavior {
    /// 创建 BlameableBehavior
    pub fn new(created_field: &'static str, updated_field: &'static str) -> Self {
        Self {
            created_field,
            updated_field,
        }
    }

    /// 使用默认字段名（created_by/updated_by）
    pub fn default_fields() -> Self {
        Self::new("created_by", "updated_by")
    }
}

impl Behavior for BlameableBehavior {
    fn name(&self) -> &'static str {
        "BlameableBehavior"
    }

    fn before_insert(
        &self,
        ctx: &HookContext,
        attrs: &mut HashMap<String, Value>,
    ) -> BehaviorResult<()> {
        if let Some(op) = ctx.operator_id {
            let v = Value::I64(op);
            attrs.insert(self.created_field.to_string(), v.clone());
            attrs.insert(self.updated_field.to_string(), v);
        }
        Ok(())
    }

    fn before_update(
        &self,
        ctx: &HookContext,
        attrs: &mut HashMap<String, Value>,
    ) -> BehaviorResult<()> {
        if let Some(op) = ctx.operator_id {
            attrs.insert(self.updated_field.to_string(), Value::I64(op));
        }
        Ok(())
    }
}

// ============================================================================
// TenantBehavior — 自动填充 tenant_id（S-3：SeaORM 对标短板补全）
// ============================================================================
//
// 对应：Yii2 `TenantBehavior` / Laravel Tenancy `BootTenant`
// / Hibernate `@TenantId`
//
// - before_insert：从 HookContext.tenant_id 读取租户 ID 填充到 attrs
// - before_update：可选校验 tenant_id 不可变更（防跨租户篡改）
//
// 与 hooks::TenantScope（查询时自动追加 tenant_id = ? 过滤）配套，
// 共同实现多租户隔离：写入侧由 TenantBehavior 填充，读取侧由 TenantScope 过滤。

/// 租户隔离行为配置：是否在 update 时强制 tenant_id 不可变更
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TenantUpdatePolicy {
    /// 允许 update 时变更 tenant_id（不推荐，仅在特殊迁移场景使用）
    Allow,
    /// update 时若 attrs 中出现 tenant_id 且与 ctx.tenant_id 不一致则报错（默认）
    #[default]
    DenyMismatch,
    /// update 时静默忽略 attrs 中的 tenant_id（保持原值不变）
    Strip,
}

/// 自动填充 tenant_id Behavior
///
/// # 工作机制
///
/// - `before_insert`：若 `ctx.tenant_id` 为 `Some(tid)`，将 `tid` 写入 `attrs[tenant_field]`；
///   若 `ctx.tenant_id` 为 `None`，按 `skip_when_no_tenant` 配置决定是跳过还是报错。
/// - `before_update`：根据 [`TenantUpdatePolicy`] 处理 attrs 中的 tenant_id：
///   - `DenyMismatch`（默认）：若 attrs 中 tenant_id 与 ctx.tenant_id 不一致则返回 `DbError::TenantError`
///   - `Strip`：从 attrs 中移除 tenant_id（保证不被更新）
///   - `Allow`：不做任何处理
///
/// # 示例
///
/// ```
/// use sz_orm_core::behaviors::{TenantBehavior, TenantUpdatePolicy, Behavior};
/// use sz_orm_core::hooks::HookContext;
/// use sz_orm_core::Value;
/// use std::collections::HashMap;
///
/// let b = TenantBehavior::default_fields();
/// let ctx = HookContext::default().with_tenant(42);
/// let mut attrs = HashMap::new();
/// b.before_insert(&ctx, &mut attrs).unwrap();
/// assert_eq!(attrs.get("tenant_id"), Some(&Value::I64(42)));
/// ```
pub struct TenantBehavior {
    /// 租户字段名（默认 "tenant_id"）
    pub tenant_field: &'static str,
    /// update 时对 tenant_id 的处理策略
    pub update_policy: TenantUpdatePolicy,
    /// ctx.tenant_id 为 None 时的行为：
    /// - true：跳过填充（不写入 tenant_id，允许跨租户写入）
    /// - false：返回 TenantError
    pub skip_when_no_tenant: bool,
}

impl TenantBehavior {
    /// 创建 TenantBehavior
    pub fn new(
        tenant_field: &'static str,
        update_policy: TenantUpdatePolicy,
        skip_when_no_tenant: bool,
    ) -> Self {
        Self {
            tenant_field,
            update_policy,
            skip_when_no_tenant,
        }
    }

    /// 使用默认字段名（tenant_id）+ 默认策略（DenyMismatch + skip_when_no_tenant=true）
    pub fn default_fields() -> Self {
        Self::new("tenant_id", TenantUpdatePolicy::default(), true)
    }

    /// 设置 update 策略（builder 风格）
    pub fn with_update_policy(mut self, policy: TenantUpdatePolicy) -> Self {
        self.update_policy = policy;
        self
    }

    /// 设置 ctx.tenant_id 为 None 时的行为（builder 风格）
    pub fn with_skip_when_no_tenant(mut self, skip: bool) -> Self {
        self.skip_when_no_tenant = skip;
        self
    }
}

impl Behavior for TenantBehavior {
    fn name(&self) -> &'static str {
        "TenantBehavior"
    }

    fn before_insert(
        &self,
        ctx: &HookContext,
        attrs: &mut HashMap<String, Value>,
    ) -> BehaviorResult<()> {
        match ctx.tenant_id {
            Some(tid) => {
                attrs.insert(self.tenant_field.to_string(), Value::I64(tid));
                Ok(())
            }
            None => {
                if self.skip_when_no_tenant {
                    Ok(())
                } else {
                    Err(DbError::TenantError(format!(
                        "TenantBehavior::before_insert: ctx.tenant_id is None, \
                         cannot auto-fill `{}`; set skip_when_no_tenant=true or \
                         provide tenant_id in HookContext",
                        self.tenant_field
                    )))
                }
            }
        }
    }

    fn before_update(
        &self,
        ctx: &HookContext,
        attrs: &mut HashMap<String, Value>,
    ) -> BehaviorResult<()> {
        match self.update_policy {
            TenantUpdatePolicy::Allow => Ok(()),
            TenantUpdatePolicy::Strip => {
                attrs.remove(self.tenant_field);
                Ok(())
            }
            TenantUpdatePolicy::DenyMismatch => {
                if let Some(existing) = attrs.get(self.tenant_field) {
                    match (existing, ctx.tenant_id) {
                        // ctx 中有 tenant_id：必须与 attrs 一致
                        (Value::I64(a), Some(b)) if *a == b => Ok(()),
                        (Value::I64(a), Some(b)) => Err(DbError::TenantError(format!(
                            "TenantBehavior::before_update: tenant_id mismatch — \
                             attrs.{}={}, ctx.tenant_id={}; update rejected to prevent \
                             cross-tenant tampering",
                            self.tenant_field, a, b
                        ))),
                        // ctx 中无 tenant_id：不允许显式更新 tenant_id
                        (_, None) => Err(DbError::TenantError(format!(
                            "TenantBehavior::before_update: attrs contains `{}` but \
                             ctx.tenant_id is None; remove `{}` from update payload or \
                             set ctx.tenant_id",
                            self.tenant_field, self.tenant_field
                        ))),
                        // 非 I64 类型的 tenant_id 视为类型不匹配
                        (other, _) => Err(DbError::TenantError(format!(
                            "TenantBehavior::before_update: attrs.{} expected I64, got {:?}",
                            self.tenant_field, other
                        ))),
                    }
                } else {
                    Ok(())
                }
            }
        }
    }
}

// ============================================================================
// AttributeBehavior — 通用属性自动设置
// ============================================================================
//
// 对应：Yii2 `AttributeBehavior`
//
// 允许用户注册自定义闭包，在指定事件触发时设置属性值。

/// 通用属性 Behavior — 在指定事件触发时通过闭包设置属性
///
/// # 示例
///
/// ```
/// use sz_orm_core::behaviors::{AttributeBehavior, BehaviorRegistry, Behavior};
/// use sz_orm_core::hooks::{HookContext, HookEvent};
/// use sz_orm_core::Value;
/// use std::collections::HashMap;
///
/// let mut registry = BehaviorRegistry::new();
/// // 在 before_insert 时设置 uuid 字段
/// registry.register(Box::new(AttributeBehavior::new(
///     "uuid_gen",
///     HookEvent::BeforeInsert,
///     "uuid",
///     |_ctx| Value::String("auto-uuid".to_string()),
/// )));
///
/// let ctx = HookContext::default();
/// let mut attrs = HashMap::new();
/// registry.before_insert(&ctx, &mut attrs).unwrap();
/// assert_eq!(attrs.get("uuid"), Some(&Value::String("auto-uuid".to_string())));
/// ```
pub struct AttributeBehavior {
    /// Behavior 名称
    pub name_str: &'static str,
    /// 订阅的事件（仅在该事件触发时执行）
    pub event: crate::hooks::HookEvent,
    /// 目标字段名
    pub target_field: &'static str,
    /// 值生成闭包
    pub generator: Box<dyn Fn(&HookContext) -> Value + Send + Sync>,
}

impl AttributeBehavior {
    /// 创建 AttributeBehavior
    pub fn new(
        name: &'static str,
        event: crate::hooks::HookEvent,
        target_field: &'static str,
        generator: impl Fn(&HookContext) -> Value + Send + Sync + 'static,
    ) -> Self {
        Self {
            name_str: name,
            event,
            target_field,
            generator: Box::new(generator),
        }
    }
}

impl Behavior for AttributeBehavior {
    fn name(&self) -> &'static str {
        self.name_str
    }

    fn before_insert(
        &self,
        ctx: &HookContext,
        attrs: &mut HashMap<String, Value>,
    ) -> BehaviorResult<()> {
        if self.event == crate::hooks::HookEvent::BeforeInsert
            || self.event == crate::hooks::HookEvent::BeforeWrite
            || self.event == crate::hooks::HookEvent::BeforeSave
        {
            let v = (self.generator)(ctx);
            attrs.insert(self.target_field.to_string(), v);
        }
        Ok(())
    }

    fn before_update(
        &self,
        ctx: &HookContext,
        attrs: &mut HashMap<String, Value>,
    ) -> BehaviorResult<()> {
        if self.event == crate::hooks::HookEvent::BeforeUpdate
            || self.event == crate::hooks::HookEvent::BeforeWrite
            || self.event == crate::hooks::HookEvent::BeforeSave
        {
            let v = (self.generator)(ctx);
            attrs.insert(self.target_field.to_string(), v);
        }
        Ok(())
    }

    fn after_find(
        &self,
        ctx: &HookContext,
        attrs: &mut HashMap<String, Value>,
    ) -> BehaviorResult<()> {
        if self.event == crate::hooks::HookEvent::AfterFind {
            let v = (self.generator)(ctx);
            attrs.insert(self.target_field.to_string(), v);
        }
        Ok(())
    }
}

// ============================================================================
// BehaviorRegistry — Behavior 注册中心
// ============================================================================

/// Behavior 注册中心 — 管理多个 Behavior 的注册与分发
///
/// 线程安全：内部使用 RwLock，可在多线程环境下共享。
///
/// # 示例
///
/// ```
/// use sz_orm_core::behaviors::{BehaviorRegistry, TimestampBehavior, BlameableBehavior, Behavior};
/// use sz_orm_core::hooks::HookContext;
/// use sz_orm_core::Value;
/// use std::collections::HashMap;
///
/// let mut registry = BehaviorRegistry::new();
/// registry.register(Box::new(TimestampBehavior::default_fields()));
/// registry.register(Box::new(BlameableBehavior::default_fields()));
///
/// let ctx = HookContext::default().with_operator(100).with_timestamp(1700000000);
/// let mut attrs = HashMap::new();
/// registry.before_insert(&ctx, &mut attrs).unwrap();
/// assert_eq!(attrs.get("created_at"), Some(&Value::I64(1700000000)));
/// assert_eq!(attrs.get("created_by"), Some(&Value::I64(100)));
/// ```
pub struct BehaviorRegistry {
    behaviors: RwLock<Vec<Box<dyn Behavior>>>,
}

impl BehaviorRegistry {
    /// 创建空的 BehaviorRegistry
    pub fn new() -> Self {
        Self {
            behaviors: RwLock::new(Vec::new()),
        }
    }

    /// 注册一个 Behavior
    pub fn register(&self, behavior: Box<dyn Behavior>) {
        let mut guards = self.behaviors.write();
        guards.push(behavior);
    }

    /// 按 name 移除已注册的 Behavior
    pub fn unregister(&self, name: &str) -> bool {
        let mut guards = self.behaviors.write();
        let before = guards.len();
        guards.retain(|b| b.name() != name);
        guards.len() < before
    }

    /// 已注册的 Behavior 数量
    pub fn count(&self) -> usize {
        self.behaviors.read().len()
    }

    /// 列出所有已注册 Behavior 的名称
    pub fn names(&self) -> Vec<&'static str> {
        self.behaviors.read().iter().map(|b| b.name()).collect()
    }

    /// 分发 before_insert 事件
    pub fn before_insert(
        &self,
        ctx: &HookContext,
        attrs: &mut HashMap<String, Value>,
    ) -> BehaviorResult<()> {
        let guards = self.behaviors.read();
        for b in guards.iter() {
            b.before_insert(ctx, attrs)?;
        }
        Ok(())
    }

    /// 分发 before_update 事件
    pub fn before_update(
        &self,
        ctx: &HookContext,
        attrs: &mut HashMap<String, Value>,
    ) -> BehaviorResult<()> {
        let guards = self.behaviors.read();
        for b in guards.iter() {
            b.before_update(ctx, attrs)?;
        }
        Ok(())
    }

    /// 分发 before_delete 事件
    pub fn before_delete(
        &self,
        ctx: &HookContext,
        attrs: &mut HashMap<String, Value>,
    ) -> BehaviorResult<()> {
        let guards = self.behaviors.read();
        for b in guards.iter() {
            b.before_delete(ctx, attrs)?;
        }
        Ok(())
    }

    /// 分发 after_find 事件
    pub fn after_find(
        &self,
        ctx: &HookContext,
        attrs: &mut HashMap<String, Value>,
    ) -> BehaviorResult<()> {
        let guards = self.behaviors.read();
        for b in guards.iter() {
            b.after_find(ctx, attrs)?;
        }
        Ok(())
    }

    /// 清空所有已注册的 Behavior
    pub fn clear(&self) {
        self.behaviors.write().clear();
    }
}

impl Default for BehaviorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::HookEvent;

    // ===== TimestampBehavior 测试 =====

    #[test]
    fn test_timestamp_behavior_before_insert() {
        let b = TimestampBehavior::default_fields();
        let ctx = HookContext::default().with_timestamp(1700000000);
        let mut attrs = HashMap::new();
        b.before_insert(&ctx, &mut attrs).unwrap();
        assert_eq!(attrs.get("created_at"), Some(&Value::I64(1700000000)));
        assert_eq!(attrs.get("updated_at"), Some(&Value::I64(1700000000)));
    }

    #[test]
    fn test_timestamp_behavior_before_update() {
        let b = TimestampBehavior::default_fields();
        let ctx = HookContext::default().with_timestamp(1800000000);
        let mut attrs = HashMap::new();
        b.before_update(&ctx, &mut attrs).unwrap();
        // update 不应填充 created_at
        assert!(!attrs.contains_key("created_at"));
        assert_eq!(attrs.get("updated_at"), Some(&Value::I64(1800000000)));
    }

    #[test]
    fn test_timestamp_behavior_custom_fields() {
        let b = TimestampBehavior::new("create_time", "update_time");
        let ctx = HookContext::default().with_timestamp(100);
        let mut attrs = HashMap::new();
        b.before_insert(&ctx, &mut attrs).unwrap();
        assert_eq!(attrs.get("create_time"), Some(&Value::I64(100)));
        assert_eq!(attrs.get("update_time"), Some(&Value::I64(100)));
    }

    #[test]
    fn test_timestamp_behavior_name() {
        let b = TimestampBehavior::default_fields();
        assert_eq!(b.name(), "TimestampBehavior");
    }

    // ===== BlameableBehavior 测试 =====

    #[test]
    fn test_blameable_behavior_before_insert() {
        let b = BlameableBehavior::default_fields();
        let ctx = HookContext::default().with_operator(42);
        let mut attrs = HashMap::new();
        b.before_insert(&ctx, &mut attrs).unwrap();
        assert_eq!(attrs.get("created_by"), Some(&Value::I64(42)));
        assert_eq!(attrs.get("updated_by"), Some(&Value::I64(42)));
    }

    #[test]
    fn test_blameable_behavior_before_update() {
        let b = BlameableBehavior::default_fields();
        let ctx = HookContext::default().with_operator(99);
        let mut attrs = HashMap::new();
        b.before_update(&ctx, &mut attrs).unwrap();
        assert!(!attrs.contains_key("created_by"));
        assert_eq!(attrs.get("updated_by"), Some(&Value::I64(99)));
    }

    #[test]
    fn test_blameable_behavior_no_operator_skips() {
        // 未设置 operator_id 时不应填充
        let b = BlameableBehavior::default_fields();
        let ctx = HookContext::default(); // 无 operator
        let mut attrs = HashMap::new();
        b.before_insert(&ctx, &mut attrs).unwrap();
        assert!(!attrs.contains_key("created_by"));
        assert!(!attrs.contains_key("updated_by"));
    }

    #[test]
    fn test_blameable_behavior_name() {
        let b = BlameableBehavior::default_fields();
        assert_eq!(b.name(), "BlameableBehavior");
    }

    // ===== TenantBehavior 测试（S-3）=====

    #[test]
    fn test_tenant_behavior_default_policy() {
        assert_eq!(
            TenantUpdatePolicy::default(),
            TenantUpdatePolicy::DenyMismatch
        );
    }

    #[test]
    fn test_tenant_behavior_before_insert_fills_tenant_id() {
        let b = TenantBehavior::default_fields();
        let ctx = HookContext::default().with_tenant(42);
        let mut attrs = HashMap::new();
        b.before_insert(&ctx, &mut attrs).unwrap();
        assert_eq!(attrs.get("tenant_id"), Some(&Value::I64(42)));
    }

    #[test]
    fn test_tenant_behavior_before_insert_overwrites_existing() {
        // 即使 attrs 已有 tenant_id，也以 ctx.tenant_id 为准（防止业务层伪造）
        let b = TenantBehavior::default_fields();
        let ctx = HookContext::default().with_tenant(99);
        let mut attrs = HashMap::new();
        attrs.insert("tenant_id".to_string(), Value::I64(1)); // 业务层伪造
        b.before_insert(&ctx, &mut attrs).unwrap();
        assert_eq!(attrs.get("tenant_id"), Some(&Value::I64(99)));
    }

    #[test]
    fn test_tenant_behavior_before_insert_no_tenant_skips_by_default() {
        // 默认 skip_when_no_tenant=true：ctx 无 tenant_id 时跳过
        let b = TenantBehavior::default_fields();
        let ctx = HookContext::default(); // 无 tenant_id
        let mut attrs = HashMap::new();
        let result = b.before_insert(&ctx, &mut attrs);
        assert!(result.is_ok());
        assert!(!attrs.contains_key("tenant_id"));
    }

    #[test]
    fn test_tenant_behavior_before_insert_no_tenant_errors_when_configured() {
        // skip_when_no_tenant=false：ctx 无 tenant_id 时返回 TenantError
        let b = TenantBehavior::default_fields().with_skip_when_no_tenant(false);
        let ctx = HookContext::default();
        let mut attrs = HashMap::new();
        let result = b.before_insert(&ctx, &mut attrs);
        match result {
            Err(DbError::TenantError(msg)) => {
                assert!(msg.contains("ctx.tenant_id is None"));
                assert!(msg.contains("tenant_id"));
            }
            other => panic!("expected TenantError, got {:?}", other),
        }
        assert!(!attrs.contains_key("tenant_id"));
    }

    #[test]
    fn test_tenant_behavior_custom_field_name() {
        let b = TenantBehavior::new("org_id", TenantUpdatePolicy::default(), true);
        let ctx = HookContext::default().with_tenant(7);
        let mut attrs = HashMap::new();
        b.before_insert(&ctx, &mut attrs).unwrap();
        assert_eq!(attrs.get("org_id"), Some(&Value::I64(7)));
        assert!(!attrs.contains_key("tenant_id"));
    }

    #[test]
    fn test_tenant_behavior_name() {
        let b = TenantBehavior::default_fields();
        assert_eq!(b.name(), "TenantBehavior");
    }

    // --- before_update 策略测试 ---

    #[test]
    fn test_tenant_behavior_update_deny_mismatch_match_ok() {
        // attrs.tenant_id == ctx.tenant_id：允许 update
        let b = TenantBehavior::default_fields(); // DenyMismatch
        let ctx = HookContext::default().with_tenant(42);
        let mut attrs = HashMap::new();
        attrs.insert("tenant_id".to_string(), Value::I64(42));
        let result = b.before_update(&ctx, &mut attrs);
        assert!(result.is_ok());
        assert_eq!(attrs.get("tenant_id"), Some(&Value::I64(42))); // 未被移除
    }

    #[test]
    fn test_tenant_behavior_update_deny_mismatch_mismatch_rejected() {
        // attrs.tenant_id != ctx.tenant_id：拒绝 update
        let b = TenantBehavior::default_fields();
        let ctx = HookContext::default().with_tenant(42);
        let mut attrs = HashMap::new();
        attrs.insert("tenant_id".to_string(), Value::I64(99)); // 跨租户篡改
        let result = b.before_update(&ctx, &mut attrs);
        match result {
            Err(DbError::TenantError(msg)) => {
                assert!(msg.contains("mismatch"));
                assert!(msg.contains("99"));
                assert!(msg.contains("42"));
            }
            other => panic!("expected TenantError, got {:?}", other),
        }
    }

    #[test]
    fn test_tenant_behavior_update_deny_mismatch_no_ctx_tenant_rejected() {
        // ctx.tenant_id=None 但 attrs 有 tenant_id：拒绝
        let b = TenantBehavior::default_fields();
        let ctx = HookContext::default();
        let mut attrs = HashMap::new();
        attrs.insert("tenant_id".to_string(), Value::I64(1));
        let result = b.before_update(&ctx, &mut attrs);
        match result {
            Err(DbError::TenantError(msg)) => {
                assert!(msg.contains("ctx.tenant_id is None"));
            }
            other => panic!("expected TenantError, got {:?}", other),
        }
    }

    #[test]
    fn test_tenant_behavior_update_deny_mismatch_no_attrs_tenant_ok() {
        // attrs 中没有 tenant_id：允许 update（不影响原值）
        let b = TenantBehavior::default_fields();
        let ctx = HookContext::default().with_tenant(42);
        let mut attrs = HashMap::new();
        attrs.insert("name".to_string(), Value::String("updated".into()));
        let result = b.before_update(&ctx, &mut attrs);
        assert!(result.is_ok());
        assert!(!attrs.contains_key("tenant_id"));
    }

    #[test]
    fn test_tenant_behavior_update_strip_removes_tenant_id() {
        // Strip 策略：从 attrs 中移除 tenant_id
        let b = TenantBehavior::default_fields().with_update_policy(TenantUpdatePolicy::Strip);
        let ctx = HookContext::default().with_tenant(42);
        let mut attrs = HashMap::new();
        attrs.insert("tenant_id".to_string(), Value::I64(99));
        attrs.insert("name".to_string(), Value::String("x".into()));
        let result = b.before_update(&ctx, &mut attrs);
        assert!(result.is_ok());
        assert!(
            !attrs.contains_key("tenant_id"),
            "Strip should remove tenant_id"
        );
        assert!(attrs.contains_key("name"), "other fields should remain");
    }

    #[test]
    fn test_tenant_behavior_update_strip_no_tenant_id_no_op() {
        // Strip 策略：attrs 中没有 tenant_id，无操作
        let b = TenantBehavior::default_fields().with_update_policy(TenantUpdatePolicy::Strip);
        let ctx = HookContext::default();
        let mut attrs = HashMap::new();
        attrs.insert("name".to_string(), Value::String("x".into()));
        let result = b.before_update(&ctx, &mut attrs);
        assert!(result.is_ok());
    }

    #[test]
    fn test_tenant_behavior_update_allow_no_check() {
        // Allow 策略：不做任何检查（即使不一致也允许）
        let b = TenantBehavior::default_fields().with_update_policy(TenantUpdatePolicy::Allow);
        let ctx = HookContext::default().with_tenant(42);
        let mut attrs = HashMap::new();
        attrs.insert("tenant_id".to_string(), Value::I64(999));
        let result = b.before_update(&ctx, &mut attrs);
        assert!(result.is_ok());
        assert_eq!(attrs.get("tenant_id"), Some(&Value::I64(999))); // 保留原值
    }

    #[test]
    fn test_tenant_behavior_update_wrong_type_rejected() {
        // attrs.tenant_id 不是 I64 类型：拒绝（类型不匹配）
        let b = TenantBehavior::default_fields();
        let ctx = HookContext::default().with_tenant(42);
        let mut attrs = HashMap::new();
        attrs.insert("tenant_id".to_string(), Value::String("forty-two".into()));
        let result = b.before_update(&ctx, &mut attrs);
        match result {
            Err(DbError::TenantError(msg)) => {
                assert!(msg.contains("expected I64"));
            }
            other => panic!("expected TenantError, got {:?}", other),
        }
    }

    // --- 集成：BehaviorRegistry + TenantBehavior ---

    #[test]
    fn test_registry_with_tenant_behavior_insert() {
        let r = BehaviorRegistry::new();
        r.register(Box::new(TenantBehavior::default_fields()));
        r.register(Box::new(TimestampBehavior::default_fields()));

        let ctx = HookContext::default().with_tenant(7).with_timestamp(1000);
        let mut attrs = HashMap::new();
        r.before_insert(&ctx, &mut attrs).unwrap();
        assert_eq!(attrs.get("tenant_id"), Some(&Value::I64(7)));
        assert_eq!(attrs.get("created_at"), Some(&Value::I64(1000)));
    }

    #[test]
    fn test_registry_with_tenant_behavior_update_strip() {
        let r = BehaviorRegistry::new();
        r.register(Box::new(
            TenantBehavior::default_fields().with_update_policy(TenantUpdatePolicy::Strip),
        ));

        let ctx = HookContext::default().with_tenant(7);
        let mut attrs = HashMap::new();
        attrs.insert("tenant_id".to_string(), Value::I64(99));
        attrs.insert("name".to_string(), Value::String("updated".into()));
        r.before_update(&ctx, &mut attrs).unwrap();
        // Strip 应移除 tenant_id
        assert!(!attrs.contains_key("tenant_id"));
        assert!(attrs.contains_key("name"));
    }

    #[test]
    fn test_registry_unregister_tenant_behavior() {
        let r = BehaviorRegistry::new();
        r.register(Box::new(TenantBehavior::default_fields()));
        assert_eq!(r.count(), 1);
        assert!(r.unregister("TenantBehavior"));
        assert_eq!(r.count(), 0);
    }

    #[test]
    fn test_combined_tenant_timestamp_blameable_insert() {
        // 模拟真实场景：同时使用 Tenant + Timestamp + Blameable
        let r = BehaviorRegistry::new();
        r.register(Box::new(TenantBehavior::default_fields()));
        r.register(Box::new(TimestampBehavior::default_fields()));
        r.register(Box::new(BlameableBehavior::default_fields()));

        let ctx = HookContext::default()
            .with_tenant(42)
            .with_operator(1)
            .with_timestamp(1700000000);
        let mut attrs = HashMap::new();
        r.before_insert(&ctx, &mut attrs).unwrap();
        assert_eq!(attrs.get("tenant_id"), Some(&Value::I64(42)));
        assert_eq!(attrs.get("created_at"), Some(&Value::I64(1700000000)));
        assert_eq!(attrs.get("created_by"), Some(&Value::I64(1)));
    }

    #[test]
    fn test_tenant_behavior_prevents_cross_tenant_tampering() {
        // 安全场景：恶意用户尝试在 update 时将 tenant_id 改为其他租户
        let r = BehaviorRegistry::new();
        r.register(Box::new(TenantBehavior::default_fields())); // DenyMismatch

        // 正常租户 42 的用户尝试把记录的 tenant_id 改为 99
        let ctx = HookContext::default().with_tenant(42);
        let mut attrs = HashMap::new();
        attrs.insert("tenant_id".to_string(), Value::I64(99)); // 试图迁移到租户 99
        attrs.insert("data".to_string(), Value::String("evil".into()));

        let result = r.before_update(&ctx, &mut attrs);
        assert!(result.is_err(), "cross-tenant tampering should be rejected");
    }

    // ===== AttributeBehavior 测试 =====

    #[test]
    fn test_attribute_behavior_before_insert() {
        let b = AttributeBehavior::new("uuid_gen", HookEvent::BeforeInsert, "uuid", |_ctx| {
            Value::String("auto-uuid".to_string())
        });
        let ctx = HookContext::default();
        let mut attrs = HashMap::new();
        b.before_insert(&ctx, &mut attrs).unwrap();
        assert_eq!(
            attrs.get("uuid"),
            Some(&Value::String("auto-uuid".to_string()))
        );
    }

    #[test]
    fn test_attribute_behavior_event_filter() {
        // 注册 BeforeInsert 事件，但触发 before_update，不应执行
        let b = AttributeBehavior::new("test", HookEvent::BeforeInsert, "field", |_ctx| {
            Value::I64(1)
        });
        let ctx = HookContext::default();
        let mut attrs = HashMap::new();
        b.before_update(&ctx, &mut attrs).unwrap();
        assert!(!attrs.contains_key("field"));
    }

    // ===== BehaviorRegistry 测试 =====

    #[test]
    fn test_registry_register_and_count() {
        let r = BehaviorRegistry::new();
        assert_eq!(r.count(), 0);
        r.register(Box::new(TimestampBehavior::default_fields()));
        assert_eq!(r.count(), 1);
        r.register(Box::new(BlameableBehavior::default_fields()));
        assert_eq!(r.count(), 2);
    }

    #[test]
    fn test_registry_unregister_by_name() {
        let r = BehaviorRegistry::new();
        r.register(Box::new(TimestampBehavior::default_fields()));
        r.register(Box::new(BlameableBehavior::default_fields()));
        assert_eq!(r.count(), 2);

        let removed = r.unregister("TimestampBehavior");
        assert!(removed);
        assert_eq!(r.count(), 1);

        // 不存在的 name 返回 false
        let removed2 = r.unregister("NonExistent");
        assert!(!removed2);
    }

    #[test]
    fn test_registry_names() {
        let r = BehaviorRegistry::new();
        r.register(Box::new(TimestampBehavior::default_fields()));
        r.register(Box::new(BlameableBehavior::default_fields()));
        let names = r.names();
        assert!(names.contains(&"TimestampBehavior"));
        assert!(names.contains(&"BlameableBehavior"));
    }

    #[test]
    fn test_registry_before_insert_dispatches_all() {
        let r = BehaviorRegistry::new();
        r.register(Box::new(TimestampBehavior::default_fields()));
        r.register(Box::new(BlameableBehavior::default_fields()));

        let ctx = HookContext::default()
            .with_operator(100)
            .with_timestamp(1700000000);
        let mut attrs = HashMap::new();
        r.before_insert(&ctx, &mut attrs).unwrap();

        // 两个 Behavior 都应执行
        assert_eq!(attrs.get("created_at"), Some(&Value::I64(1700000000)));
        assert_eq!(attrs.get("created_by"), Some(&Value::I64(100)));
    }

    #[test]
    fn test_registry_before_update_dispatches_all() {
        let r = BehaviorRegistry::new();
        r.register(Box::new(TimestampBehavior::default_fields()));
        r.register(Box::new(BlameableBehavior::default_fields()));

        let ctx = HookContext::default()
            .with_operator(200)
            .with_timestamp(1800000000);
        let mut attrs = HashMap::new();
        r.before_update(&ctx, &mut attrs).unwrap();

        // update 只填充 updated_* 字段
        assert!(!attrs.contains_key("created_at"));
        assert_eq!(attrs.get("updated_at"), Some(&Value::I64(1800000000)));
        assert!(!attrs.contains_key("created_by"));
        assert_eq!(attrs.get("updated_by"), Some(&Value::I64(200)));
    }

    #[test]
    fn test_registry_clear() {
        let r = BehaviorRegistry::new();
        r.register(Box::new(TimestampBehavior::default_fields()));
        r.register(Box::new(BlameableBehavior::default_fields()));
        assert_eq!(r.count(), 2);

        r.clear();
        assert_eq!(r.count(), 0);
    }

    #[test]
    fn test_registry_default() {
        let r = BehaviorRegistry::default();
        assert_eq!(r.count(), 0);
    }

    #[test]
    fn test_registry_empty_dispatches_no_op() {
        // 空 registry 分发事件应该是 no-op
        let r = BehaviorRegistry::new();
        let ctx = HookContext::default();
        let mut attrs = HashMap::new();
        assert!(r.before_insert(&ctx, &mut attrs).is_ok());
        assert!(r.before_update(&ctx, &mut attrs).is_ok());
        assert!(r.before_delete(&ctx, &mut attrs).is_ok());
        assert!(r.after_find(&ctx, &mut attrs).is_ok());
        assert!(attrs.is_empty());
    }

    #[test]
    fn test_combined_timestamp_and_blameable() {
        // 模拟真实场景：同时使用 TimestampBehavior + BlameableBehavior
        let r = BehaviorRegistry::new();
        r.register(Box::new(TimestampBehavior::default_fields()));
        r.register(Box::new(BlameableBehavior::default_fields()));

        // 模拟 insert
        let ctx1 = HookContext::default().with_operator(1).with_timestamp(1000);
        let mut attrs1 = HashMap::new();
        r.before_insert(&ctx1, &mut attrs1).unwrap();
        assert_eq!(attrs1.get("created_at"), Some(&Value::I64(1000)));
        assert_eq!(attrs1.get("updated_at"), Some(&Value::I64(1000)));
        assert_eq!(attrs1.get("created_by"), Some(&Value::I64(1)));
        assert_eq!(attrs1.get("updated_by"), Some(&Value::I64(1)));

        // 模拟 update（不同操作人、不同时间）
        let ctx2 = HookContext::default().with_operator(2).with_timestamp(2000);
        let mut attrs2 = HashMap::new();
        r.before_update(&ctx2, &mut attrs2).unwrap();
        assert!(!attrs2.contains_key("created_at"));
        assert_eq!(attrs2.get("updated_at"), Some(&Value::I64(2000)));
        assert!(!attrs2.contains_key("created_by"));
        assert_eq!(attrs2.get("updated_by"), Some(&Value::I64(2)));
    }
}
