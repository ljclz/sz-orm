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
use std::collections::HashMap;
use std::sync::RwLock;

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
        let mut guards = self.behaviors.write().unwrap();
        guards.push(behavior);
    }

    /// 按 name 移除已注册的 Behavior
    pub fn unregister(&self, name: &str) -> bool {
        let mut guards = self.behaviors.write().unwrap();
        let before = guards.len();
        guards.retain(|b| b.name() != name);
        guards.len() < before
    }

    /// 已注册的 Behavior 数量
    pub fn count(&self) -> usize {
        self.behaviors.read().unwrap().len()
    }

    /// 列出所有已注册 Behavior 的名称
    pub fn names(&self) -> Vec<&'static str> {
        self.behaviors
            .read()
            .unwrap()
            .iter()
            .map(|b| b.name())
            .collect()
    }

    /// 分发 before_insert 事件
    pub fn before_insert(
        &self,
        ctx: &HookContext,
        attrs: &mut HashMap<String, Value>,
    ) -> BehaviorResult<()> {
        let guards = self.behaviors.read().unwrap();
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
        let guards = self.behaviors.read().unwrap();
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
        let guards = self.behaviors.read().unwrap();
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
        let guards = self.behaviors.read().unwrap();
        for b in guards.iter() {
            b.after_find(ctx, attrs)?;
        }
        Ok(())
    }

    /// 清空所有已注册的 Behavior
    pub fn clear(&self) {
        self.behaviors.write().unwrap().clear();
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
