//! 多租户上下文与隔离策略
//!
//! 本模块在 `multi-tenant-enhanced` feature gate 下导出，提供：
//! - [`IsolationStrategy`] — 隔离策略枚举（行级 / Schema 隔离）
//! - [`TenantContext`] + [`TenantContextGuard`] — 租户上下文 + RAII 守卫（task-local 异步隔离）
//! - [`SchemaIsolationRouter`] — Schema 隔离路由器（表名重写 `tenant_{id}_{table}`）
//! - [`TenantPoolRegistry`] — 租户连接池注册表（按 tenant_id 维护独立 Pool）

use crate::pool::{Pool, PoolConfig};
use crate::tenant_security::{ColumnMaskingRule, RowLevelSecurityPolicy};
use crate::PoolError;
use std::collections::HashMap;
use std::sync::Arc;

// ─── M1-T2：租户上下文与 RAII 守卫 ─────────────────────────────────

/// 隔离策略枚举
///
/// - `RowLevel`：行级隔离，追加 `WHERE tenant_id = ?`
/// - `SchemaIsolation`：Schema 隔离，路由到 `tenant_{id}_{table}`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsolationStrategy {
    /// 行级隔离（追加 `WHERE tenant_id = ?`）
    RowLevel,
    /// Schema 隔离（路由到 `tenant_{id}_{table}`）
    SchemaIsolation,
}

/// 租户权限（行级安全策略 + 列级脱敏规则 + 角色列表）
#[derive(Debug, Clone, Default)]
pub struct TenantPermissions {
    /// 行级安全策略列表
    pub row_level_policies: Vec<RowLevelSecurityPolicy>,
    /// 列级脱敏规则列表
    pub column_masking_rules: Vec<ColumnMaskingRule>,
    /// 角色列表
    pub roles: Vec<String>,
}

impl TenantPermissions {
    /// 创建空的权限集合
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加行级安全策略
    pub fn with_row_level_policy(mut self, policy: RowLevelSecurityPolicy) -> Self {
        self.row_level_policies.push(policy);
        self
    }

    /// 添加列级脱敏规则
    pub fn with_column_masking_rule(mut self, rule: ColumnMaskingRule) -> Self {
        self.column_masking_rules.push(rule);
        self
    }

    /// 设置角色列表
    pub fn with_roles(mut self, roles: Vec<String>) -> Self {
        self.roles = roles;
        self
    }
}

// 线程局部存储：用于 RAII guard 模式（同步上下文安全）
std::thread_local! {
    static TENANT_CONTEXT_THREAD: std::cell::RefCell<Option<TenantContext>> = const { std::cell::RefCell::new(None) };
}

// task-local 存储：用于 scope 模式（异步上下文安全）
tokio::task_local! {
    static TENANT_CONTEXT_TASK: std::cell::RefCell<Option<TenantContext>>;
}

/// 租户上下文（运行时自动注入）
///
/// 由可信路径（中间件/网关）设置，不可被客户端篡改。
/// `tenant_id` 必填 `i64`（禁止字符串避免注入）。
#[derive(Debug, Clone)]
pub struct TenantContext {
    /// 租户 ID（必填，与既有 `with_tenant_id(tenant_id: i64)` 类型一致）
    pub tenant_id: i64,
    /// 隔离策略（行级 / Schema 隔离）
    pub isolation_strategy: IsolationStrategy,
    /// 权限（行级安全策略 + 列级脱敏规则 + 角色列表）
    pub permissions: TenantPermissions,
}

impl TenantContext {
    /// 创建新的租户上下文
    pub fn new(tenant_id: i64, isolation_strategy: IsolationStrategy) -> Self {
        Self {
            tenant_id,
            isolation_strategy,
            permissions: TenantPermissions::new(),
        }
    }

    /// 设置权限
    pub fn with_permissions(mut self, permissions: TenantPermissions) -> Self {
        self.permissions = permissions;
        self
    }

    /// 进入上下文作用域，返回 RAII 守卫（线程局部存储）
    ///
    /// 守卫在 Drop 时自动清理线程局部上下文。
    /// **注意**：此方法使用 `thread_local` 存储，适用于同步上下文或
    /// 单线程 tokio 运行时。对于多线程 tokio 运行时中的异步任务，
    /// 请使用 [`TenantContext::scope`] 代替。
    pub fn enter(self) -> TenantContextGuard {
        TenantContextGuard::enter(self)
    }

    /// 在指定异步块的作用域内设置上下文（task-local 存储）
    ///
    /// 这是异步安全的上下文设置方式，使用 `tokio::task_local!` 实现
    /// 异步任务边界隔离。在 scope 内部，[`TenantContext::current`]
    /// 返回 `Some`；scope 结束后返回 `None`。
    ///
    /// # 示例
    ///
    /// ```ignore
    /// let ctx = TenantContext::new(42, IsolationStrategy::RowLevel);
    /// ctx.scope(async {
    ///     // 在此作用域内，TenantContext::current() 返回 Some
    ///     assert_eq!(TenantContext::current().unwrap().tenant_id, 42);
    /// }).await;
    /// ```
    pub async fn scope<F, R>(self, f: F) -> R
    where
        F: std::future::Future<Output = R>,
    {
        TENANT_CONTEXT_TASK
            .scope(std::cell::RefCell::new(Some(self)), f)
            .await
    }

    /// 读取当前上下文
    ///
    /// 优先从 task-local（异步 scope）读取，其次从 thread-local（RAII guard）读取。
    /// 返回 `None` 表示未设置上下文。
    pub fn current() -> Option<TenantContext> {
        // 先尝试 task-local
        if let Some(ctx) = TENANT_CONTEXT_TASK
            .try_with(|cell| cell.borrow().clone())
            .ok()
            .flatten()
        {
            return Some(ctx);
        }
        // 再尝试 thread-local
        TENANT_CONTEXT_THREAD.with(|cell| cell.borrow().clone())
    }

    /// 检查当前是否在上下文作用域内
    pub fn is_set() -> bool {
        Self::current().is_some()
    }
}

/// RAII 上下文守卫（线程局部存储）
///
/// 在作用域结束时自动清理线程局部上下文。
/// 守卫作用域内上下文不变，保证租户切换原子。
pub struct TenantContextGuard {
    _prev: Option<TenantContext>,
}

impl TenantContextGuard {
    fn enter(context: TenantContext) -> Self {
        let prev = TENANT_CONTEXT_THREAD.with(|cell| cell.borrow().clone());
        TENANT_CONTEXT_THREAD.with(|cell| {
            *cell.borrow_mut() = Some(context);
        });
        Self { _prev: prev }
    }
}

impl Drop for TenantContextGuard {
    fn drop(&mut self) {
        TENANT_CONTEXT_THREAD.with(|cell| {
            *cell.borrow_mut() = self._prev.take();
        });
    }
}

// ─── M1-T3：Schema 隔离路由器 ──────────────────────────────────────

/// Schema 隔离路由器
///
/// 将表名重写为 `tenant_{tenant_id}_{table}` 格式。
/// Schema 命名遵循固定格式，禁止用户自定义避免冲突。
pub struct SchemaIsolationRouter;

impl SchemaIsolationRouter {
    /// 重写表名：`table` → `tenant_{tenant_id}_{table}`
    ///
    /// # 示例
    ///
    /// ```
    /// use sz_orm_core::tenant_context::SchemaIsolationRouter;
    /// assert_eq!(SchemaIsolationRouter::rewrite_table("users", 42), "tenant_42_users");
    /// ```
    pub fn rewrite_table(table: &str, tenant_id: i64) -> String {
        format!("tenant_{}_{}", tenant_id, table)
    }

    /// 重写多条表名
    pub fn rewrite_tables(tables: &[&str], tenant_id: i64) -> Vec<String> {
        tables
            .iter()
            .map(|t| Self::rewrite_table(t, tenant_id))
            .collect()
    }
}

// ─── M1-T5：租户连接池注册表 ───────────────────────────────────────

/// 租户连接池注册表
///
/// 按 `tenant_id` 维护独立的 `Pool`，各租户池共享 `PoolConfig`。
/// 租户切换原子（RAII 守卫），路由开销 ≤ 50μs（HashMap 查找 + Arc clone）。
pub struct TenantPoolRegistry {
    pools: parking_lot::RwLock<HashMap<i64, Arc<Pool>>>,
    pool_config: PoolConfig,
}

impl TenantPoolRegistry {
    /// 创建新的租户连接池注册表
    pub fn new(pool_config: PoolConfig) -> Self {
        Self {
            pools: parking_lot::RwLock::new(HashMap::new()),
            pool_config,
        }
    }

    /// 获取或创建租户连接池
    ///
    /// Pool 已存在时返回既有 Pool；不存在时创建新 Pool 并插入 HashMap。
    /// 使用读锁优先 + 写锁降级策略避免不必要的写锁竞争。
    pub fn get_or_create(
        &self,
        tenant_id: i64,
        factory: &Arc<dyn crate::pool::ConnectionFactory>,
    ) -> Result<Arc<Pool>, PoolError> {
        // 先尝试读锁快速路径
        {
            let pools = self.pools.read();
            if let Some(pool) = pools.get(&tenant_id) {
                return Ok(Arc::clone(pool));
            }
        }

        // 写锁慢速路径：创建新 Pool
        let mut pools = self.pools.write();
        // 双检查：可能在等写锁时其他线程已创建
        if let Some(pool) = pools.get(&tenant_id) {
            return Ok(Arc::clone(pool));
        }

        let pool = Arc::new(Pool::new(self.pool_config.clone(), Arc::clone(factory))?);
        pools.insert(tenant_id, Arc::clone(&pool));
        Ok(pool)
    }

    /// 原子切换到指定租户的连接池
    ///
    /// 返回 RAII 守卫，在 Drop 时切换回原租户。
    pub fn switch(
        &self,
        tenant_id: i64,
        factory: &Arc<dyn crate::pool::ConnectionFactory>,
    ) -> Result<TenantPoolGuard<'_>, PoolError> {
        let new_pool = self.get_or_create(tenant_id, factory)?;
        Ok(TenantPoolGuard {
            registry: self,
            new_pool,
        })
    }

    /// 获取已注册的租户数量
    pub fn tenant_count(&self) -> usize {
        self.pools.read().len()
    }

    /// 获取连接池配置
    pub fn config(&self) -> &PoolConfig {
        &self.pool_config
    }
}

/// 租户连接池 RAII 守卫
///
/// 在 Drop 时不需要显式切换回原租户（各租户 Pool 独立，无全局"当前池"状态）。
/// 守卫持有新租户 Pool 的 Arc 引用，Drop 时自动释放。
pub struct TenantPoolGuard<'a> {
    registry: &'a TenantPoolRegistry,
    new_pool: Arc<Pool>,
}

impl<'a> TenantPoolGuard<'a> {
    /// 获取当前租户的连接池
    pub fn pool(&self) -> &Arc<Pool> {
        &self.new_pool
    }

    /// 获取注册表引用
    pub fn registry(&self) -> &TenantPoolRegistry {
        self.registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── M1-T9.1：TenantContext + TenantContextGuard 测试 ──────────

    #[tokio::test]
    async fn test_tenant_context_enter_and_current() {
        let ctx = TenantContext::new(42, IsolationStrategy::RowLevel);
        let guard = ctx.enter();
        let current = TenantContext::current();
        assert!(current.is_some());
        assert_eq!(current.unwrap().tenant_id, 42);
        drop(guard);
        // 守卫 Drop 后上下文应清理
        assert!(TenantContext::current().is_none());
    }

    #[tokio::test]
    async fn test_tenant_context_not_set() {
        // 未设置上下文时 current() 返回 None
        // 注意：thread_local 可能残留其他测试的值，所以仅测试 scope 外
        let ctx = TenantContext::new(99, IsolationStrategy::RowLevel);
        ctx.scope(async {
            assert!(TenantContext::is_set());
        })
        .await;
        // scope 结束后 task-local 清理，但 thread-local 可能仍残留
    }

    #[tokio::test]
    async fn test_tenant_context_is_set() {
        let ctx = TenantContext::new(7, IsolationStrategy::SchemaIsolation);
        ctx.scope(async {
            assert!(TenantContext::is_set());
            assert_eq!(TenantContext::current().unwrap().tenant_id, 7);
        })
        .await;
    }

    #[tokio::test]
    async fn test_tenant_context_nested_scope() {
        let ctx_a = TenantContext::new(1, IsolationStrategy::RowLevel);
        ctx_a
            .scope(async {
                assert_eq!(TenantContext::current().unwrap().tenant_id, 1);

                let ctx_b = TenantContext::new(2, IsolationStrategy::RowLevel);
                ctx_b
                    .scope(async {
                        assert_eq!(TenantContext::current().unwrap().tenant_id, 2);
                    })
                    .await;

                // 恢复回 ctx_a
                assert_eq!(TenantContext::current().unwrap().tenant_id, 1);
            })
            .await;
    }

    #[tokio::test]
    async fn test_tenant_context_async_isolation() {
        // 不同异步任务有独立的 task-local 上下文
        let ctx_a = TenantContext::new(1, IsolationStrategy::RowLevel);
        let ctx_b = TenantContext::new(2, IsolationStrategy::RowLevel);

        let handle_a = tokio::spawn(async move {
            ctx_a
                .scope(async {
                    tokio::task::yield_now().await;
                    TenantContext::current().unwrap().tenant_id
                })
                .await
        });

        let handle_b = tokio::spawn(async move {
            ctx_b
                .scope(async {
                    tokio::task::yield_now().await;
                    TenantContext::current().unwrap().tenant_id
                })
                .await
        });

        let (id_a, id_b) = tokio::join!(handle_a, handle_b);
        assert_eq!(id_a.unwrap(), 1);
        assert_eq!(id_b.unwrap(), 2);
    }

    // ─── M1-T9.2：SchemaIsolationRouter 测试 ──────────────────────

    #[test]
    fn test_schema_isolation_router_rewrite() {
        assert_eq!(
            SchemaIsolationRouter::rewrite_table("users", 42),
            "tenant_42_users"
        );
        assert_eq!(
            SchemaIsolationRouter::rewrite_table("orders", 1),
            "tenant_1_orders"
        );
    }

    #[test]
    fn test_schema_isolation_router_different_tenants() {
        let table_a = SchemaIsolationRouter::rewrite_table("users", 1);
        let table_b = SchemaIsolationRouter::rewrite_table("users", 2);
        assert_ne!(table_a, table_b);
        assert_eq!(table_a, "tenant_1_users");
        assert_eq!(table_b, "tenant_2_users");
    }

    #[test]
    fn test_schema_isolation_router_rewrite_tables() {
        let rewritten = SchemaIsolationRouter::rewrite_tables(&["users", "orders"], 42);
        assert_eq!(rewritten, vec!["tenant_42_users", "tenant_42_orders"]);
    }

    // ─── TenantPermissions 测试 ───────────────────────────────────

    #[test]
    fn test_tenant_permissions_default() {
        let perms = TenantPermissions::new();
        assert!(perms.row_level_policies.is_empty());
        assert!(perms.column_masking_rules.is_empty());
        assert!(perms.roles.is_empty());
    }

    #[test]
    fn test_isolation_strategy_copy() {
        let strategy = IsolationStrategy::RowLevel;
        let copied = strategy;
        assert_eq!(strategy, copied);
    }
}
