//! Observer + Event Subscriber — 模型生命周期观察者模式
//!
//! 对应文档 6.8 节改进项 32（Observer）+ 33（Event Subscriber）。
//!
//! # 核心概念
//!
//! - **Observer**：观察者接口，订阅模型生命周期事件（INSERT/UPDATE/DELETE/FIND）
//! - **EventSubscriber**：事件订阅者，按事件类型订阅（比 Observer 更细粒度）
//! - **EventDispatcher**：事件分发器，管理 Observer 与 EventSubscriber 的注册和分发
//!
//! # 与 Behaviors 的区别
//!
//! | 特性 | Behaviors (behaviors.rs) | Observer (本模块) |
//! |------|--------------------------|-------------------|
//! | 注册方式 | Model 内部声明 | 外部注册到 Dispatcher |
//! | 解耦程度 | Model 与 Behavior 强耦合 | 完全解耦，Model 无需感知 |
//! | 适用场景 | 字段自动填充（时间戳/操作人） | 审计日志、缓存失效、外部通知 |
//! | 事件粒度 | 4 个生命周期事件 | 可订阅特定事件类型 |
//!
//! # 设计灵感
//!
//! - Doctrine `EventSubscriber` / `LifecycleCallback`
//! - Hibernate `EntityListener` / `@PostPersist`
//! - Laravel Eloquent `Observer` 类
//! - Rails ActiveRecord `Callbacks` + `Observers`
//!
//! # 使用示例
//!
//! ```
//! use sz_orm_core::observer::{
//!     Event, EventDispatcher, EventSubscriber, Observer, SubscriberResult,
//! };
//! use sz_orm_core::hooks::HookContext;
//! use std::collections::HashMap;
//! use std::sync::{Arc, Mutex};
//! use sz_orm_core::Value;
//!
//! // 1. 审计日志订阅者（订阅所有事件）
//! struct AuditLogSubscriber {
//!     logs: Arc<Mutex<Vec<String>>>,
//! }
//!
//! impl EventSubscriber for AuditLogSubscriber {
//!     fn subscribed_events(&self) -> Vec<Event> {
//!         vec![Event::AfterInsert, Event::AfterUpdate, Event::AfterDelete]
//!     }
//!
//!     fn on_event(&self, event: Event, ctx: &HookContext, attrs: &HashMap<String, Value>) -> SubscriberResult<()> {
//!         let mut logs = self.logs.lock().unwrap();
//!         logs.push(format!("{:?} on attrs with {} fields", event, attrs.len()));
//!         Ok(())
//!     }
//! }
//!
//! // 2. 缓存失效订阅者（仅订阅写入事件）
//! struct CacheInvalidationSubscriber;
//!
//! impl EventSubscriber for CacheInvalidationSubscriber {
//!     fn subscribed_events(&self) -> Vec<Event> {
//!         vec![Event::AfterUpdate, Event::AfterDelete]
//!     }
//!
//!     fn on_event(&self, event: Event, _ctx: &HookContext, attrs: &HashMap<String, Value>) -> SubscriberResult<()> {
//!         // 失效缓存逻辑...
//!         let _ = (event, attrs);
//!         Ok(())
//!     }
//! }
//!
//! // 3. 注册并触发事件
//! let logs = Arc::new(Mutex::new(Vec::new()));
//! let mut dispatcher = EventDispatcher::new();
//! dispatcher.subscribe(Box::new(AuditLogSubscriber { logs: logs.clone() }));
//! dispatcher.subscribe(Box::new(CacheInvalidationSubscriber));
//!
//! let ctx = HookContext::default();
//! let attrs = HashMap::new();
//! dispatcher.dispatch(Event::AfterInsert, &ctx, &attrs);
//!
//! assert_eq!(logs.lock().unwrap().len(), 1);
//! ```

use crate::hooks::HookContext;
use crate::Value;
use std::collections::HashMap;
use std::sync::RwLock;

// ============================================================================
// Event — 事件类型
// ============================================================================

/// 模型生命周期事件类型
///
/// 与 `hooks::HookEvent` 类似但简化为运行时分发用的事件枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Event {
    /// 插入前
    BeforeInsert,
    /// 插入后
    AfterInsert,
    /// 更新前
    BeforeUpdate,
    /// 更新后
    AfterUpdate,
    /// 删除前
    BeforeDelete,
    /// 删除后
    AfterDelete,
    /// 单行查询后
    AfterFind,
    /// 软删除恢复前
    BeforeRestore,
    /// 软删除恢复后
    AfterRestore,
}

impl Event {
    /// 是否为 before 事件
    pub fn is_before(&self) -> bool {
        matches!(
            self,
            Event::BeforeInsert | Event::BeforeUpdate | Event::BeforeDelete | Event::BeforeRestore
        )
    }

    /// 是否为 after 事件
    pub fn is_after(&self) -> bool {
        matches!(
            self,
            Event::AfterInsert
                | Event::AfterUpdate
                | Event::AfterDelete
                | Event::AfterFind
                | Event::AfterRestore
        )
    }

    /// 是否为写入事件（INSERT/UPDATE/DELETE）
    pub fn is_write_event(&self) -> bool {
        matches!(
            self,
            Event::BeforeInsert
                | Event::AfterInsert
                | Event::BeforeUpdate
                | Event::AfterUpdate
                | Event::BeforeDelete
                | Event::AfterDelete
        )
    }

    /// 事件名称（用于日志与错误信息）
    pub fn name(&self) -> &'static str {
        match self {
            Event::BeforeInsert => "before_insert",
            Event::AfterInsert => "after_insert",
            Event::BeforeUpdate => "before_update",
            Event::AfterUpdate => "after_update",
            Event::BeforeDelete => "before_delete",
            Event::AfterDelete => "after_delete",
            Event::AfterFind => "after_find",
            Event::BeforeRestore => "before_restore",
            Event::AfterRestore => "after_restore",
        }
    }
}

// ============================================================================
// SubscriberError — 订阅者错误
// ============================================================================

/// 订阅者错误类型
#[derive(Debug)]
pub enum SubscriberError {
    /// 订阅者执行失败（携带错误描述）
    Failed {
        /// 订阅者名称
        subscriber: String,
        /// 错误描述
        reason: String,
    },
    /// 中止后续订阅者执行（用于 veto 模式）
    ///
    /// 例如：before_insert 钩子拒绝该次插入
    Vetoed {
        /// 订阅者名称
        subscriber: String,
        /// 拒绝原因
        reason: String,
    },
}

impl std::fmt::Display for SubscriberError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SubscriberError::Failed { subscriber, reason } => {
                write!(f, "Subscriber `{}` failed: {}", subscriber, reason)
            }
            SubscriberError::Vetoed { subscriber, reason } => {
                write!(f, "Subscriber `{}` vetoed: {}", subscriber, reason)
            }
        }
    }
}

impl std::error::Error for SubscriberError {}

/// 订阅者结果类型
pub type SubscriberResult<T> = Result<T, SubscriberError>;

// ============================================================================
// Observer — 模型观察者 trait
// ============================================================================

/// 模型观察者 trait
///
/// 与 `EventSubscriber` 不同，`Observer` 默认订阅所有事件。
/// 适合需要监控所有生命周期事件的场景（如审计日志）。
///
/// # 实现要点
///
/// - 所有方法默认实现为 no-op，按需 override
/// - 任何方法返回 `Err(SubscriberError::Vetoed)` 会中止 before 事件的后续执行
pub trait Observer: Send + Sync {
    /// 观察者名称（用于日志与错误信息）
    fn name(&self) -> &str {
        "anonymous_observer"
    }

    /// 插入前
    fn before_insert(
        &self,
        _ctx: &HookContext,
        _attrs: &mut HashMap<String, Value>,
    ) -> SubscriberResult<()> {
        Ok(())
    }

    /// 插入后
    fn after_insert(
        &self,
        _ctx: &HookContext,
        _attrs: &HashMap<String, Value>,
    ) -> SubscriberResult<()> {
        Ok(())
    }

    /// 更新前
    fn before_update(
        &self,
        _ctx: &HookContext,
        _attrs: &mut HashMap<String, Value>,
    ) -> SubscriberResult<()> {
        Ok(())
    }

    /// 更新后
    fn after_update(
        &self,
        _ctx: &HookContext,
        _attrs: &HashMap<String, Value>,
    ) -> SubscriberResult<()> {
        Ok(())
    }

    /// 删除前
    fn before_delete(
        &self,
        _ctx: &HookContext,
        _attrs: &HashMap<String, Value>,
    ) -> SubscriberResult<()> {
        Ok(())
    }

    /// 删除后
    fn after_delete(
        &self,
        _ctx: &HookContext,
        _attrs: &HashMap<String, Value>,
    ) -> SubscriberResult<()> {
        Ok(())
    }

    /// 单行查询后
    fn after_find(
        &self,
        _ctx: &HookContext,
        _attrs: &mut HashMap<String, Value>,
    ) -> SubscriberResult<()> {
        Ok(())
    }
}

// ============================================================================
// EventSubscriber — 事件订阅者 trait
// ============================================================================

/// 事件订阅者 trait
///
/// 与 `Observer` 不同，`EventSubscriber` 只接收订阅的特定事件。
/// 适合只关心特定事件的场景（如缓存失效仅关心 UPDATE/DELETE）。
pub trait EventSubscriber: Send + Sync {
    /// 订阅者名称
    fn name(&self) -> &str {
        "anonymous_subscriber"
    }

    /// 返回订阅的事件列表
    ///
    /// 仅当事件在此列表中时，`on_event` 才会被调用。
    fn subscribed_events(&self) -> Vec<Event>;

    /// 事件回调
    ///
    /// # 参数
    /// - `event`：触发的事件
    /// - `ctx`：钩子上下文
    /// - `attrs`：当前属性（before 事件可修改）
    ///
    /// # 返回
    /// - `Ok(())`：继续执行后续订阅者
    /// - `Err(SubscriberError::Vetoed)`：中止 before 事件的后续执行
    /// - `Err(SubscriberError::Failed)`：记录错误，继续执行后续订阅者
    fn on_event(
        &self,
        event: Event,
        ctx: &HookContext,
        attrs: &HashMap<String, Value>,
    ) -> SubscriberResult<()>;
}

// ============================================================================
// EventDispatcher — 事件分发器
// ============================================================================

/// 事件分发器
///
/// 管理 `Observer` 与 `EventSubscriber` 的注册与分发。
///
/// # 分发顺序
///
/// 1. 先按注册顺序调用所有 `Observer` 的对应方法
/// 2. 再按注册顺序调用所有订阅了该事件的 `EventSubscriber`
///
/// # 错误处理
///
/// - `before_*` 事件中任何订阅者返回 `Err(Vetoed)` 会立即中止后续执行
/// - `after_*` 事件中的错误仅记录，不影响后续执行
///
/// # 线程安全
///
/// 内部使用 `RwLock<Vec<...>>`，支持多线程并发。
pub struct EventDispatcher {
    observers: RwLock<Vec<Box<dyn Observer>>>,
    subscribers: RwLock<Vec<Box<dyn EventSubscriber>>>,
    /// 错误收集（非致命错误，不影响流程）
    errors: RwLock<Vec<SubscriberError>>,
    /// 错误缓冲区最大容量（防止内存无限增长）
    ///
    /// 当 errors 长度达到此上限时，新增错误会以 FIFO 方式淘汰最早错误。
    /// 默认 1024，可通过 `with_max_errors` 调整。
    max_errors: usize,
}

/// 默认错误缓冲区容量
const DEFAULT_MAX_ERRORS: usize = 1024;

impl EventDispatcher {
    /// 创建空的事件分发器
    pub fn new() -> Self {
        Self {
            observers: RwLock::new(Vec::new()),
            subscribers: RwLock::new(Vec::new()),
            errors: RwLock::new(Vec::new()),
            max_errors: DEFAULT_MAX_ERRORS,
        }
    }

    /// 设置错误缓冲区最大容量
    ///
    /// 当 errors 达到此容量时，新增错误会淘汰最早错误（FIFO）。
    /// 设置为 0 表示无限制（不推荐，可能导致内存泄漏）。
    pub fn with_max_errors(mut self, max_errors: usize) -> Self {
        self.max_errors = max_errors;
        self
    }

    /// 注册 Observer
    pub fn add_observer(&self, observer: Box<dyn Observer>) {
        self.observers.write().unwrap().push(observer);
    }

    /// 注册 EventSubscriber
    pub fn subscribe(&self, subscriber: Box<dyn EventSubscriber>) {
        self.subscribers.write().unwrap().push(subscriber);
    }

    /// 清空所有注册
    pub fn clear(&self) {
        self.observers.write().unwrap().clear();
        self.subscribers.write().unwrap().clear();
        self.errors.write().unwrap().clear();
    }

    /// 返回已注册的 Observer 数量
    pub fn observer_count(&self) -> usize {
        self.observers.read().unwrap().len()
    }

    /// 返回已注册的 EventSubscriber 数量
    pub fn subscriber_count(&self) -> usize {
        self.subscribers.read().unwrap().len()
    }

    /// 取出收集到的非致命错误（清空内部缓冲）
    pub fn drain_errors(&self) -> Vec<SubscriberError> {
        std::mem::take(&mut *self.errors.write().unwrap())
    }

    /// 返回当前错误缓冲区中的错误数量
    pub fn error_count(&self) -> usize {
        self.errors.read().unwrap().len()
    }

    /// 将本地错误批量写入 errors 缓冲区，遵循 max_errors 限制（FIFO 淘汰）
    ///
    /// - `max_errors = 0` 表示无限制
    /// - 否则当 errors 达到上限时，淘汰最早错误以腾出空间
    fn push_errors(&self, new_errors: Vec<SubscriberError>) {
        if new_errors.is_empty() {
            return;
        }
        let mut errors = self.errors.write().unwrap();
        if self.max_errors == 0 {
            errors.extend(new_errors);
            return;
        }
        for e in new_errors {
            if errors.len() >= self.max_errors {
                // FIFO 淘汰最早错误
                errors.remove(0);
            }
            errors.push(e);
        }
    }

    /// 分发事件（after_* 事件，attrs 不可变）
    ///
    /// 错误仅记录，不影响后续订阅者执行。
    ///
    /// # 实现要点
    ///
    /// - 错误先收集到本地 `Vec`，循环结束后一次性批量写入 `self.errors`，
    ///   避免在持读锁期间获取写锁造成的死锁风险与锁竞争。
    pub fn dispatch(&self, event: Event, ctx: &HookContext, attrs: &HashMap<String, Value>) {
        let mut local_errors: Vec<SubscriberError> = Vec::new();

        // 1. 调用 Observers
        {
            let observers = self.observers.read().unwrap();
            for observer in observers.iter() {
                let result = match event {
                    Event::AfterInsert => observer.after_insert(ctx, attrs),
                    Event::AfterUpdate => observer.after_update(ctx, attrs),
                    Event::AfterDelete => observer.after_delete(ctx, attrs),
                    _ => Ok(()),
                };
                if let Err(e) = result {
                    local_errors.push(e);
                }
            }
        }

        // 2. 调用 EventSubscribers
        {
            let subscribers = self.subscribers.read().unwrap();
            for subscriber in subscribers.iter() {
                if !subscriber.subscribed_events().contains(&event) {
                    continue;
                }
                if let Err(e) = subscriber.on_event(event, ctx, attrs) {
                    local_errors.push(e);
                }
            }
        }

        if !local_errors.is_empty() {
            self.push_errors(local_errors);
        }
    }

    /// 分发 before 事件（attrs 可变）
    ///
    /// 任何订阅者返回 `Err(Vetoed)` 会立即中止并返回错误。
    ///
    /// # 实现要点
    ///
    /// - Vetoed 时直接返回该错误，**不会再次调用 `on_event`**（避免订阅者副作用翻倍）
    /// - 错误先收集到本地 `Vec`，避免持读锁时获取写锁造成死锁
    pub fn dispatch_before_mut(
        &self,
        event: Event,
        ctx: &HookContext,
        attrs: &mut HashMap<String, Value>,
    ) -> SubscriberResult<()> {
        let mut local_errors: Vec<SubscriberError> = Vec::new();
        let mut vetoed: Option<SubscriberError> = None;

        // 1. 调用 Observers
        {
            let observers = self.observers.read().unwrap();
            for observer in observers.iter() {
                let result = match event {
                    Event::BeforeInsert => observer.before_insert(ctx, attrs),
                    Event::BeforeUpdate => observer.before_update(ctx, attrs),
                    _ => Ok(()),
                };
                match result {
                    Ok(()) => {}
                    Err(e @ SubscriberError::Vetoed { .. }) => {
                        vetoed = Some(e);
                        break;
                    }
                    Err(e) => local_errors.push(e),
                }
            }
        }

        // 2. 调用 EventSubscribers（仅当未被 vetoed）
        if vetoed.is_none() {
            let subscribers = self.subscribers.read().unwrap();
            for subscriber in subscribers.iter() {
                if !subscriber.subscribed_events().contains(&event) {
                    continue;
                }
                match subscriber.on_event(event, ctx, attrs) {
                    Ok(()) => {}
                    Err(e @ SubscriberError::Vetoed { .. }) => {
                        vetoed = Some(e);
                        break;
                    }
                    Err(e) => local_errors.push(e),
                }
            }
        }

        if !local_errors.is_empty() {
            self.push_errors(local_errors);
        }

        if let Some(e) = vetoed {
            return Err(e);
        }
        Ok(())
    }

    /// 分发 after_find 事件（attrs 可变，用于修改读出的数据）
    ///
    /// # 实现要点
    ///
    /// - 错误先收集到本地 `Vec`，避免持读锁时获取写锁造成死锁
    pub fn dispatch_after_find(
        &self,
        ctx: &HookContext,
        attrs: &mut HashMap<String, Value>,
    ) -> SubscriberResult<()> {
        let mut local_errors: Vec<SubscriberError> = Vec::new();

        {
            let observers = self.observers.read().unwrap();
            for observer in observers.iter() {
                if let Err(e) = observer.after_find(ctx, attrs) {
                    local_errors.push(e);
                }
            }
        }

        {
            let subscribers = self.subscribers.read().unwrap();
            for subscriber in subscribers.iter() {
                if !subscriber.subscribed_events().contains(&Event::AfterFind) {
                    continue;
                }
                if let Err(e) = subscriber.on_event(Event::AfterFind, ctx, attrs) {
                    local_errors.push(e);
                }
            }
        }

        if !local_errors.is_empty() {
            self.push_errors(local_errors);
        }

        Ok(())
    }
}

impl Default for EventDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 内置订阅者实现
// ============================================================================

// -------------------- AuditLogSubscriber --------------------

/// 审计日志订阅者
///
/// 记录所有写入操作到内部日志缓冲，可用于调试或审计。
///
/// # 示例
///
/// ```
/// use sz_orm_core::observer::{EventDispatcher, AuditLogSubscriber, Event};
/// use sz_orm_core::hooks::HookContext;
/// use std::collections::HashMap;
///
/// let audit = AuditLogSubscriber::new();
/// let mut dispatcher = EventDispatcher::new();
/// dispatcher.subscribe(Box::new(audit.clone()));
///
/// let ctx = HookContext::default();
/// let attrs = HashMap::new();
/// dispatcher.dispatch(Event::AfterInsert, &ctx, &attrs);
///
/// assert_eq!(audit.logs().lock().unwrap().len(), 1);
/// ```
#[derive(Clone)]
pub struct AuditLogSubscriber {
    logs: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

impl AuditLogSubscriber {
    /// 创建审计日志订阅者
    pub fn new() -> Self {
        Self {
            logs: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    /// 获取日志列表（用于断言）
    pub fn logs(&self) -> &std::sync::Arc<std::sync::Mutex<Vec<String>>> {
        &self.logs
    }
}

impl Default for AuditLogSubscriber {
    fn default() -> Self {
        Self::new()
    }
}

impl EventSubscriber for AuditLogSubscriber {
    fn name(&self) -> &str {
        "audit_log"
    }

    fn subscribed_events(&self) -> Vec<Event> {
        vec![Event::AfterInsert, Event::AfterUpdate, Event::AfterDelete]
    }

    fn on_event(
        &self,
        event: Event,
        ctx: &HookContext,
        attrs: &HashMap<String, Value>,
    ) -> SubscriberResult<()> {
        let mut logs = self.logs.lock().unwrap();
        logs.push(format!(
            "event={} operator={:?} field_count={}",
            event.name(),
            ctx.operator_id,
            attrs.len()
        ));
        Ok(())
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    // ===== Event 测试 =====

    #[test]
    fn test_event_is_before_after() {
        assert!(Event::BeforeInsert.is_before());
        assert!(!Event::BeforeInsert.is_after());
        assert!(Event::AfterInsert.is_after());
        assert!(!Event::AfterInsert.is_before());
    }

    #[test]
    fn test_event_is_write_event() {
        assert!(Event::BeforeInsert.is_write_event());
        assert!(Event::AfterUpdate.is_write_event());
        assert!(Event::BeforeDelete.is_write_event());
        assert!(!Event::AfterFind.is_write_event());
    }

    #[test]
    fn test_event_name() {
        assert_eq!(Event::BeforeInsert.name(), "before_insert");
        assert_eq!(Event::AfterDelete.name(), "after_delete");
        assert_eq!(Event::AfterFind.name(), "after_find");
    }

    // ===== EventDispatcher 基础测试 =====

    #[test]
    fn test_new_dispatcher_is_empty() {
        let d = EventDispatcher::new();
        assert_eq!(d.observer_count(), 0);
        assert_eq!(d.subscriber_count(), 0);
    }

    #[test]
    fn test_add_observer() {
        struct DummyObserver;
        impl Observer for DummyObserver {}

        let d = EventDispatcher::new();
        d.add_observer(Box::new(DummyObserver));
        assert_eq!(d.observer_count(), 1);
    }

    #[test]
    fn test_subscribe() {
        struct DummySubscriber;
        impl EventSubscriber for DummySubscriber {
            fn subscribed_events(&self) -> Vec<Event> {
                vec![Event::AfterInsert]
            }
            fn on_event(
                &self,
                _event: Event,
                _ctx: &HookContext,
                _attrs: &HashMap<String, Value>,
            ) -> SubscriberResult<()> {
                Ok(())
            }
        }

        let d = EventDispatcher::new();
        d.subscribe(Box::new(DummySubscriber));
        assert_eq!(d.subscriber_count(), 1);
    }

    #[test]
    fn test_clear() {
        struct DummyObserver;
        impl Observer for DummyObserver {}

        let d = EventDispatcher::new();
        d.add_observer(Box::new(DummyObserver));
        d.clear();
        assert_eq!(d.observer_count(), 0);
    }

    // ===== Observer 触发测试 =====

    /// 计数 Observer，用于测试
    struct CountingObserver {
        insert_count: Arc<Mutex<u32>>,
        update_count: Arc<Mutex<u32>>,
        delete_count: Arc<Mutex<u32>>,
    }

    impl Observer for CountingObserver {
        fn name(&self) -> &str {
            "counting"
        }

        fn after_insert(
            &self,
            _ctx: &HookContext,
            _attrs: &HashMap<String, Value>,
        ) -> SubscriberResult<()> {
            *self.insert_count.lock().unwrap() += 1;
            Ok(())
        }

        fn after_update(
            &self,
            _ctx: &HookContext,
            _attrs: &HashMap<String, Value>,
        ) -> SubscriberResult<()> {
            *self.update_count.lock().unwrap() += 1;
            Ok(())
        }

        fn after_delete(
            &self,
            _ctx: &HookContext,
            _attrs: &HashMap<String, Value>,
        ) -> SubscriberResult<()> {
            *self.delete_count.lock().unwrap() += 1;
            Ok(())
        }
    }

    #[test]
    fn test_observer_triggered_on_dispatch() {
        let insert = Arc::new(Mutex::new(0u32));
        let update = Arc::new(Mutex::new(0u32));
        let delete = Arc::new(Mutex::new(0u32));

        let observer = CountingObserver {
            insert_count: insert.clone(),
            update_count: update.clone(),
            delete_count: delete.clone(),
        };

        let d = EventDispatcher::new();
        d.add_observer(Box::new(observer));

        let ctx = HookContext::default();
        let attrs = HashMap::new();

        d.dispatch(Event::AfterInsert, &ctx, &attrs);
        d.dispatch(Event::AfterInsert, &ctx, &attrs);
        d.dispatch(Event::AfterUpdate, &ctx, &attrs);
        d.dispatch(Event::AfterDelete, &ctx, &attrs);

        assert_eq!(*insert.lock().unwrap(), 2);
        assert_eq!(*update.lock().unwrap(), 1);
        assert_eq!(*delete.lock().unwrap(), 1);
    }

    #[test]
    fn test_observer_before_event_can_modify_attrs() {
        struct TimestampInjector;
        impl Observer for TimestampInjector {
            fn before_insert(
                &self,
                _ctx: &HookContext,
                attrs: &mut HashMap<String, Value>,
            ) -> SubscriberResult<()> {
                attrs.insert(
                    "created_at".to_string(),
                    Value::String("2026-07-19".to_string()),
                );
                Ok(())
            }
        }

        let d = EventDispatcher::new();
        d.add_observer(Box::new(TimestampInjector));

        let ctx = HookContext::default();
        let mut attrs = HashMap::new();
        d.dispatch_before_mut(Event::BeforeInsert, &ctx, &mut attrs)
            .unwrap();

        assert_eq!(
            attrs.get("created_at"),
            Some(&Value::String("2026-07-19".to_string()))
        );
    }

    // ===== EventSubscriber 测试 =====

    /// 只订阅 AfterInsert 的订阅者
    struct InsertOnlySubscriber {
        called: Arc<Mutex<u32>>,
    }

    impl EventSubscriber for InsertOnlySubscriber {
        fn name(&self) -> &str {
            "insert_only"
        }

        fn subscribed_events(&self) -> Vec<Event> {
            vec![Event::AfterInsert]
        }

        fn on_event(
            &self,
            _event: Event,
            _ctx: &HookContext,
            _attrs: &HashMap<String, Value>,
        ) -> SubscriberResult<()> {
            *self.called.lock().unwrap() += 1;
            Ok(())
        }
    }

    #[test]
    fn test_subscriber_only_called_for_subscribed_events() {
        let called = Arc::new(Mutex::new(0u32));
        let subscriber = InsertOnlySubscriber {
            called: called.clone(),
        };

        let d = EventDispatcher::new();
        d.subscribe(Box::new(subscriber));

        let ctx = HookContext::default();
        let attrs = HashMap::new();

        // AfterInsert 应触发
        d.dispatch(Event::AfterInsert, &ctx, &attrs);
        // AfterUpdate 不应触发（未订阅）
        d.dispatch(Event::AfterUpdate, &ctx, &attrs);
        // AfterDelete 不应触发
        d.dispatch(Event::AfterDelete, &ctx, &attrs);
        // 再触发一次 AfterInsert
        d.dispatch(Event::AfterInsert, &ctx, &attrs);

        assert_eq!(*called.lock().unwrap(), 2);
    }

    #[test]
    fn test_subscriber_veto_aborts_before_event() {
        struct VetoSubscriber;
        impl EventSubscriber for VetoSubscriber {
            fn name(&self) -> &str {
                "veto"
            }
            fn subscribed_events(&self) -> Vec<Event> {
                vec![Event::BeforeInsert]
            }
            fn on_event(
                &self,
                _event: Event,
                _ctx: &HookContext,
                _attrs: &HashMap<String, Value>,
            ) -> SubscriberResult<()> {
                Err(SubscriberError::Vetoed {
                    subscriber: "veto".to_string(),
                    reason: "Business rule violation".to_string(),
                })
            }
        }

        let d = EventDispatcher::new();
        d.subscribe(Box::new(VetoSubscriber));

        let ctx = HookContext::default();
        let mut attrs = HashMap::new();
        let result = d.dispatch_before_mut(Event::BeforeInsert, &ctx, &mut attrs);

        assert!(matches!(result, Err(SubscriberError::Vetoed { .. })));
    }

    #[test]
    fn test_subscriber_failed_does_not_abort_after_event() {
        struct FailingSubscriber;
        impl EventSubscriber for FailingSubscriber {
            fn name(&self) -> &str {
                "failing"
            }
            fn subscribed_events(&self) -> Vec<Event> {
                vec![Event::AfterInsert]
            }
            fn on_event(
                &self,
                _event: Event,
                _ctx: &HookContext,
                _attrs: &HashMap<String, Value>,
            ) -> SubscriberResult<()> {
                Err(SubscriberError::Failed {
                    subscriber: "failing".to_string(),
                    reason: "Connection lost".to_string(),
                })
            }
        }

        struct CountingSubscriber {
            called: Arc<Mutex<u32>>,
        }
        impl EventSubscriber for CountingSubscriber {
            fn name(&self) -> &str {
                "counting"
            }
            fn subscribed_events(&self) -> Vec<Event> {
                vec![Event::AfterInsert]
            }
            fn on_event(
                &self,
                _event: Event,
                _ctx: &HookContext,
                _attrs: &HashMap<String, Value>,
            ) -> SubscriberResult<()> {
                *self.called.lock().unwrap() += 1;
                Ok(())
            }
        }

        let called = Arc::new(Mutex::new(0u32));
        let d = EventDispatcher::new();
        d.subscribe(Box::new(FailingSubscriber));
        d.subscribe(Box::new(CountingSubscriber {
            called: called.clone(),
        }));

        let ctx = HookContext::default();
        let attrs = HashMap::new();
        d.dispatch(Event::AfterInsert, &ctx, &attrs);

        // 即使 FailingSubscriber 失败，CountingSubscriber 仍应被调用
        assert_eq!(*called.lock().unwrap(), 1);
    }

    // ===== AuditLogSubscriber 测试 =====

    #[test]
    fn test_audit_log_subscriber() {
        let audit = AuditLogSubscriber::new();
        let audit_clone = audit.clone();

        let d = EventDispatcher::new();
        d.subscribe(Box::new(audit_clone));

        let ctx = HookContext {
            operator_id: Some(42),
            ..Default::default()
        };
        let mut attrs = HashMap::new();
        attrs.insert("name".to_string(), Value::String("alice".to_string()));

        d.dispatch(Event::AfterInsert, &ctx, &attrs);
        d.dispatch(Event::AfterUpdate, &ctx, &attrs);
        d.dispatch(Event::AfterDelete, &ctx, &attrs);
        // AfterFind 不在订阅列表，不应记录
        d.dispatch_after_find(&ctx, &mut attrs).unwrap();

        let logs = audit.logs().lock().unwrap();
        assert_eq!(logs.len(), 3);
        assert!(logs[0].contains("event=after_insert"));
        assert!(logs[0].contains("operator=Some(42)"));
        assert!(logs[0].contains("field_count=1"));
    }

    // ===== 多订阅者协同测试 =====

    #[test]
    fn test_multiple_subscribers_and_observers() {
        let sub1_called = Arc::new(Mutex::new(0u32));
        let sub2_called = Arc::new(Mutex::new(0u32));
        let obs_called = Arc::new(Mutex::new(0u32));

        struct Sub1(Arc<Mutex<u32>>);
        impl EventSubscriber for Sub1 {
            fn name(&self) -> &str {
                "sub1"
            }
            fn subscribed_events(&self) -> Vec<Event> {
                vec![Event::AfterInsert]
            }
            fn on_event(
                &self,
                _e: Event,
                _c: &HookContext,
                _a: &HashMap<String, Value>,
            ) -> SubscriberResult<()> {
                *self.0.lock().unwrap() += 1;
                Ok(())
            }
        }

        struct Sub2(Arc<Mutex<u32>>);
        impl EventSubscriber for Sub2 {
            fn name(&self) -> &str {
                "sub2"
            }
            fn subscribed_events(&self) -> Vec<Event> {
                vec![Event::AfterInsert, Event::AfterUpdate]
            }
            fn on_event(
                &self,
                _e: Event,
                _c: &HookContext,
                _a: &HashMap<String, Value>,
            ) -> SubscriberResult<()> {
                *self.0.lock().unwrap() += 1;
                Ok(())
            }
        }

        struct Obs(Arc<Mutex<u32>>);
        impl Observer for Obs {
            fn name(&self) -> &str {
                "obs"
            }
            fn after_insert(
                &self,
                _c: &HookContext,
                _a: &HashMap<String, Value>,
            ) -> SubscriberResult<()> {
                *self.0.lock().unwrap() += 1;
                Ok(())
            }
        }

        let d = EventDispatcher::new();
        d.subscribe(Box::new(Sub1(sub1_called.clone())));
        d.subscribe(Box::new(Sub2(sub2_called.clone())));
        d.add_observer(Box::new(Obs(obs_called.clone())));

        let ctx = HookContext::default();
        let attrs = HashMap::new();

        d.dispatch(Event::AfterInsert, &ctx, &attrs);

        assert_eq!(*sub1_called.lock().unwrap(), 1);
        assert_eq!(*sub2_called.lock().unwrap(), 1);
        assert_eq!(*obs_called.lock().unwrap(), 1);
    }

    // ===== 错误收集测试 =====

    #[test]
    fn test_drain_errors() {
        struct ErrSub;
        impl EventSubscriber for ErrSub {
            fn name(&self) -> &str {
                "err"
            }
            fn subscribed_events(&self) -> Vec<Event> {
                vec![Event::AfterInsert]
            }
            fn on_event(
                &self,
                _e: Event,
                _c: &HookContext,
                _a: &HashMap<String, Value>,
            ) -> SubscriberResult<()> {
                Err(SubscriberError::Failed {
                    subscriber: "err".to_string(),
                    reason: "test".to_string(),
                })
            }
        }

        let d = EventDispatcher::new();
        d.subscribe(Box::new(ErrSub));

        let ctx = HookContext::default();
        let attrs = HashMap::new();
        d.dispatch(Event::AfterInsert, &ctx, &attrs);
        d.dispatch(Event::AfterInsert, &ctx, &attrs);

        let errors = d.drain_errors();
        assert_eq!(errors.len(), 2);
        assert!(matches!(errors[0], SubscriberError::Failed { .. }));

        // drain 后内部应为空
        let errors = d.drain_errors();
        assert!(errors.is_empty());
    }

    // ===== max_errors 限制测试（防内存无限增长） =====

    #[test]
    fn test_max_errors_limits_buffer_size() {
        struct ErrSub;
        impl EventSubscriber for ErrSub {
            fn name(&self) -> &str {
                "err"
            }
            fn subscribed_events(&self) -> Vec<Event> {
                vec![Event::AfterInsert]
            }
            fn on_event(
                &self,
                _e: Event,
                _c: &HookContext,
                _a: &HashMap<String, Value>,
            ) -> SubscriberResult<()> {
                Err(SubscriberError::Failed {
                    subscriber: "err".to_string(),
                    reason: "test".to_string(),
                })
            }
        }

        // 设置 max_errors = 3，触发 5 次错误，应只保留最新 3 个
        let d = EventDispatcher::new().with_max_errors(3);
        d.subscribe(Box::new(ErrSub));

        let ctx = HookContext::default();
        let attrs = HashMap::new();
        for _ in 0..5 {
            d.dispatch(Event::AfterInsert, &ctx, &attrs);
        }

        assert_eq!(d.error_count(), 3);
        let errors = d.drain_errors();
        assert_eq!(errors.len(), 3);
    }

    #[test]
    fn test_max_errors_zero_means_unlimited() {
        struct ErrSub;
        impl EventSubscriber for ErrSub {
            fn name(&self) -> &str {
                "err"
            }
            fn subscribed_events(&self) -> Vec<Event> {
                vec![Event::AfterInsert]
            }
            fn on_event(
                &self,
                _e: Event,
                _c: &HookContext,
                _a: &HashMap<String, Value>,
            ) -> SubscriberResult<()> {
                Err(SubscriberError::Failed {
                    subscriber: "err".to_string(),
                    reason: "test".to_string(),
                })
            }
        }

        let d = EventDispatcher::new().with_max_errors(0);
        d.subscribe(Box::new(ErrSub));

        let ctx = HookContext::default();
        let attrs = HashMap::new();
        for _ in 0..10 {
            d.dispatch(Event::AfterInsert, &ctx, &attrs);
        }

        assert_eq!(d.error_count(), 10);
    }

    #[test]
    fn test_max_errors_fifo_eviction_order() {
        // 验证 FIFO 淘汰：保留的是最新错误
        struct CounterSub(Arc<Mutex<u32>>);
        impl EventSubscriber for CounterSub {
            fn name(&self) -> &str {
                "counter"
            }
            fn subscribed_events(&self) -> Vec<Event> {
                vec![Event::AfterInsert]
            }
            fn on_event(
                &self,
                _e: Event,
                _c: &HookContext,
                _a: &HashMap<String, Value>,
            ) -> SubscriberResult<()> {
                let mut n = self.0.lock().unwrap();
                *n += 1;
                Err(SubscriberError::Failed {
                    subscriber: "counter".to_string(),
                    reason: format!("call-{}", *n),
                })
            }
        }

        let counter = Arc::new(Mutex::new(0u32));
        let d = EventDispatcher::new().with_max_errors(2);
        d.subscribe(Box::new(CounterSub(counter.clone())));

        let ctx = HookContext::default();
        let attrs = HashMap::new();
        for _ in 0..4 {
            d.dispatch(Event::AfterInsert, &ctx, &attrs);
        }

        let errors = d.drain_errors();
        assert_eq!(errors.len(), 2);
        // 应保留最新的两个（call-3, call-4）
        match &errors[0] {
            SubscriberError::Failed { reason, .. } => assert_eq!(reason, "call-3"),
            other => panic!("expected Failed, got {:?}", other),
        }
        match &errors[1] {
            SubscriberError::Failed { reason, .. } => assert_eq!(reason, "call-4"),
            other => panic!("expected Failed, got {:?}", other),
        }
    }

    // ===== before 事件 Veto 测试 =====

    #[test]
    fn test_veto_aborts_subsequent_observers() {
        let second_called = Arc::new(Mutex::new(0u32));

        struct VetoObs;
        impl Observer for VetoObs {
            fn name(&self) -> &str {
                "veto"
            }
            fn before_insert(
                &self,
                _c: &HookContext,
                _a: &mut HashMap<String, Value>,
            ) -> SubscriberResult<()> {
                Err(SubscriberError::Vetoed {
                    subscriber: "veto".to_string(),
                    reason: "no".to_string(),
                })
            }
        }

        struct CountingObs(Arc<Mutex<u32>>);
        impl Observer for CountingObs {
            fn name(&self) -> &str {
                "counting"
            }
            fn before_insert(
                &self,
                _c: &HookContext,
                _a: &mut HashMap<String, Value>,
            ) -> SubscriberResult<()> {
                *self.0.lock().unwrap() += 1;
                Ok(())
            }
        }

        let d = EventDispatcher::new();
        d.add_observer(Box::new(VetoObs));
        d.add_observer(Box::new(CountingObs(second_called.clone())));

        let ctx = HookContext::default();
        let mut attrs = HashMap::new();
        let result = d.dispatch_before_mut(Event::BeforeInsert, &ctx, &mut attrs);

        assert!(result.is_err());
        // 第二个 observer 不应被调用
        assert_eq!(*second_called.lock().unwrap(), 0);
    }

    // ===== Display 测试 =====

    #[test]
    fn test_error_display() {
        let e = SubscriberError::Failed {
            subscriber: "test".to_string(),
            reason: "boom".to_string(),
        };
        assert!(e.to_string().contains("test"));
        assert!(e.to_string().contains("boom"));

        let e = SubscriberError::Vetoed {
            subscriber: "vetoer".to_string(),
            reason: "rejected".to_string(),
        };
        assert!(e.to_string().contains("vetoer"));
        assert!(e.to_string().contains("rejected"));
    }
}
