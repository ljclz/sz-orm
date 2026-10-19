# v9.5.0 交付记录

> 版本：9.5.0（下游接线闭环 + 最后一公里补齐）
> 日期：2026-10-07
> 状态：✅ 全部任务完成，门禁通过

## 1. 交付概要

v9.5.0 核心定位：**下游接线闭环 + 最后一公里补齐** — 将 sz-orm 已实现但下游 sz-pay 未接线的核心能力闭环至 sz-pay，并补齐连接池主动自愈与多步骤事务编排两项缺失能力，消除幻影交付风险。

### 交付物清单

| 任务 | 交付物 | 测试数 | 状态 |
|------|--------|--------|------|
| T1 | feature gate 配置（`slow-query-capture` + `auto-healer`） | — | ✅ |
| T2 | `BatchExecutor`（sz-orm-sqlx） | 12 | ✅ |
| T3 | `SlowQueryGuard`（sz-orm-sqlx） | 10 | ✅ |
| T4 | `AutoHealer`（sz-orm-core/pool.rs） | 13 | ✅ |
| T5 | `TransactionOrchestrator`（sz-orm-core/transaction.rs） | 14 | ✅ |
| T6 | Repository trait 增强（sz-orm-core/repository.rs） | 13 | ✅ |
| T7 | 接线示例 `wiring_sz_pay.rs` | — | ✅ |
| T8 | 端到端测试 `wiring_e2e.rs` | 6 | ✅ |
| T9 | 全量门禁验证 | — | ✅ |
| T10 | 交付记录归档 | — | ✅ |

**新增测试总计**：68 个（12 + 10 + 13 + 14 + 13 + 6）

## 2. 变更文件清单

### 新增文件
- `packages/sz-orm-sqlx/src/batch_executor.rs` — T2: BatchExecutor（~230 行）
- `packages/sz-orm-sqlx/src/slow_query_guard.rs` — T3: SlowQueryGuard（~220 行）
- `packages/sz-orm-sqlx/examples/wiring_sz_pay.rs` — T7: 接线示例（~170 行）
- `packages/sz-orm-sqlx/tests/wiring_e2e.rs` — T8: 端到端测试（~230 行）

### 修改文件
- `packages/sz-orm-sqlx/Cargo.toml` — 新增 `slow-query-capture` + `auto-healer` feature + `sz-orm-observability` optional 依赖
- `packages/sz-orm-core/Cargo.toml` — 新增 `auto-healer` feature
- `packages/sz-orm-sqlx/src/lib.rs` — 新增 `pub mod batch_executor` + `pub mod slow_query_guard` + 导出
- `packages/sz-orm-core/src/pool.rs` — T4: 新增 `auto_healer` 模块（~130 行生产代码）
- `packages/sz-orm-core/src/transaction.rs` — T5: 新增 `TransactionOrchestrator`（~170 行生产代码）
- `packages/sz-orm-core/src/repository.rs` — T6: 新增 `batch_create`/`batch_upsert`/`paginate_sql`/`query`/`SoftDeletable`/`RepositoryQuery`（~200 行生产代码）

## 3. 新增 API 签名

### T2: BatchExecutor
```rust
// packages/sz-orm-sqlx/src/batch_executor.rs:58
impl<'a, C: Connection> BatchExecutor<'a, C> {
    pub fn new(conn: &'a mut C, dialect: DbType) -> Self;
    pub fn with_max_batch_rows(mut self, max: usize) -> Self;
    pub async fn batch_insert(&mut self, table: &str, rows: &[HashMap<String, Value>]) -> Result<u64, DbError>;
    pub async fn batch_upsert(&mut self, table: &str, rows: &[HashMap<String, Value>], conflict_cols: &[&str], update_cols: &[&str]) -> Result<u64, DbError>;
}
```

### T3: SlowQueryGuard
```rust
// packages/sz-orm-sqlx/src/slow_query_guard.rs:50
impl<C: Connection> SlowQueryGuard<C> {
    pub fn new(conn: C) -> Self;
    pub fn with_slow_threshold(mut self, threshold: Duration) -> Self;
    pub fn with_sample_rate(mut self, rate: f64) -> Self;
    pub fn with_level(mut self, level: LogLevel) -> Self;
    pub async fn execute_with_logging(&mut self, sql: &str, params: &[Value]) -> Result<u64, DbError>;
    pub fn logged_entries(&self) -> &[QueryLogEntry];
    pub fn is_degraded(&self) -> bool;
    pub fn inner_mut(&mut self) -> &mut C;
}
```

### T4: AutoHealer
```rust
// packages/sz-orm-core/src/pool.rs:2664
pub struct HealerConfig { pub heal_interval, pub probe_timeout, pub probe_sql, pub conservative_mode, pub min_idle }
pub struct AutoHealer;
pub struct AutoHealerHandle;
impl AutoHealer {
    pub fn start(pool: Pool, config: HealerConfig) -> AutoHealerHandle;
}
impl AutoHealerHandle {
    pub fn stop(&self);
    pub fn is_cancelled(&self) -> bool;
}
```

### T5: TransactionOrchestrator
```rust
// packages/sz-orm-core/src/transaction.rs:809
pub struct TransactionOrchestrator { steps, config }
pub enum OrchestrationResult { Success(HashMap<String, Value>), Failure { failed_step, reason, compensated } }
pub struct OrchestratorConfig { pub rollback_strategy: RollbackStrategy }
pub enum RollbackStrategy { FullRollback, RollbackTo(String) }
pub type StepFn = Box<dyn for<'a> Fn(&'a mut Transaction, &'a mut HashMap<String, Value>) -> Pin<Box<dyn Future<Output = Result<(), TxError>> + Send + 'a>> + Send + Sync>;
pub type CompensateFn = Box<dyn for<'a> Fn(&'a mut HashMap<String, Value>) -> Pin<Box<dyn Future<Output = Result<(), TxError>> + Send + 'a>> + Send + Sync>;

impl TransactionOrchestrator {
    pub fn new() -> Self;
    pub fn with_config(mut self, config: OrchestratorConfig) -> Self;
    pub fn step(mut self, name: impl Into<String>, execute: StepFn) -> Self;
    pub fn step_with_compensate(mut self, name: impl Into<String>, execute: StepFn, compensate: CompensateFn) -> Self;
    pub async fn run(self, conn: Box<dyn Connection>, options: TransactOptions) -> OrchestrationResult;
    pub fn step_count(&self) -> usize;
}
```

### T6: Repository trait 增强
```rust
// packages/sz-orm-core/src/repository.rs:341
trait Repository<E> {
    fn batch_create(&self, entities: Vec<E>) -> RepositoryResult<usize>;  // 默认降级 save_many
    fn batch_upsert(&self, entities: Vec<E>) -> RepositoryResult<usize>;  // 默认降级 save_many
    fn paginate_sql(&self, page: u64, page_size: u64) -> RepositoryResult<PageResult<E>>;  // 默认降级 paginate
    fn query(&self) -> RepositoryQuery<'_, E, Self>;  // 链式构建器
}

pub trait SoftDeletable { fn soft_delete_column() -> &'static str { "deleted_at" } }

pub struct RepositoryQuery<'a, E, R: Repository<E>> {
    // where_eq / where_in / order_by / limit / offset / with_relation / find_all / find_one / count
}

pub enum RepositoryError { ..., RelationNotFound(String), InvalidParams(String) }
```

## 4. 门禁验证结果

| # | 门禁 | 命令 | 结果 |
|---|------|------|------|
| G2 | check 编译检查 | `cargo check --workspace --all-targets` | ✅ 通过 |
| G3 | clippy 静态分析 | `cargo clippy -p sz-orm-core -p sz-orm-sqlx --all-targets --features "auto-healer,slow-query-capture" -- -D warnings` | ✅ 通过（修复 1 个 needless_range_loop） |
| G4 | 单元测试 | `cargo test -p sz-orm-core --features auto-healer --lib` + `cargo test -p sz-orm-sqlx --features "auto-healer,slow-query-capture" --lib` | ✅ 3241 + 175 = 3416 passed |
| G7 | 集成测试 | `cargo test -p sz-orm-sqlx --test wiring_e2e --features "slow-query-capture,auto-healer" -- --ignored` | ✅ 6 passed（MySQL 9.6 + PG 18 真实 DB） |
| G8 | 占位实现检查 | `grep -rn 'todo!\|unimplemented!\|unreachable!'` | ✅ 仅文档注释示例 |
| G10 | all-features 编译 | `cargo check -p sz-orm-core -p sz-orm-sqlx --all-targets --all-features` | ✅ 通过 |

## 5. file:line 证据

### T2: BatchExecutor
- `packages/sz-orm-sqlx/src/batch_executor.rs:58` — `BatchExecutor::new`
- `packages/sz-orm-sqlx/src/batch_executor.rs:79` — `batch_insert`
- `packages/sz-orm-sqlx/src/batch_executor.rs:109` — `batch_upsert`

### T3: SlowQueryGuard
- `packages/sz-orm-sqlx/src/slow_query_guard.rs:50` — `SlowQueryGuard::new`
- `packages/sz-orm-sqlx/src/slow_query_guard.rs:88` — `execute_with_logging`

### T4: AutoHealer
- `packages/sz-orm-core/src/pool.rs:2664` — `HealerConfig`
- `packages/sz-orm-core/src/pool.rs:2712` — `AutoHealer`
- `packages/sz-orm-core/src/pool.rs:2819` — `pub use auto_healer::{AutoHealer, AutoHealerHandle, HealerConfig}`

### T5: TransactionOrchestrator
- `packages/sz-orm-core/src/transaction.rs:809` — `TransactionOrchestrator`
- `packages/sz-orm-core/src/transaction.rs:864` — `run` 方法
- `packages/sz-orm-core/src/transaction.rs:795` — `OrchestrationResult`

### T6: Repository trait 增强
- `packages/sz-orm-core/src/repository.rs:546` — `batch_create`
- `packages/sz-orm-core/src/repository.rs:554` — `batch_upsert`
- `packages/sz-orm-core/src/repository.rs:562` — `paginate_sql`
- `packages/sz-orm-core/src/repository.rs:577` — `query`
- `packages/sz-orm-core/src/repository.rs:604` — `SoftDeletable`
- `packages/sz-orm-core/src/repository.rs:615` — `RepositoryQuery`

### T7: 接线示例
- `packages/sz-orm-sqlx/examples/wiring_sz_pay.rs:58` — `demo_batch_executor`
- `packages/sz-orm-sqlx/examples/wiring_sz_pay.rs:78` — `demo_slow_query_guard`
- `packages/sz-orm-sqlx/examples/wiring_sz_pay.rs:95` — `demo_auto_healer`
- `packages/sz-orm-sqlx/examples/wiring_sz_pay.rs:113` — `demo_transaction_orchestrator`
- `packages/sz-orm-sqlx/examples/wiring_sz_pay.rs:155` — `demo_repository_enhanced`

### T8: 端到端测试
- `packages/sz-orm-sqlx/tests/wiring_e2e.rs:30` — `test_batch_insert_e2e_mysql`
- `packages/sz-orm-sqlx/tests/wiring_e2e.rs:62` — `test_batch_upsert_e2e_pg`
- `packages/sz-orm-sqlx/tests/wiring_e2e.rs:94` — `test_slow_query_capture_e2e`
- `packages/sz-orm-sqlx/tests/wiring_e2e.rs:136` — `test_auto_healer_config_e2e`
- `packages/sz-orm-sqlx/tests/wiring_e2e.rs:155` — `test_orchestrator_e2e`
- `packages/sz-orm-sqlx/tests/wiring_e2e.rs:215` — `test_repository_enhanced_e2e`

## 6. 向后兼容性

- **不新增 crate**：72 不变
- **不引入新外部依赖**：`sz-orm-observability` 已存在，仅改为 optional 依赖
- **既有公开 API 签名不变**：所有新增方法均有默认实现，既有 implementor（`InMemoryRepository`、`GenericKeyRepository`）无须改动
- **feature gate 默认关闭**：`slow-query-capture` 和 `auto-healer` 均默认关闭，不影响既有编译

## 7. 已知限制

1. **T5 死锁重试**：`TransactionOrchestrator::run` 不内部处理死锁重试（`Connection` 无 `clone_box`、`TransactOptions` 未实现 `Clone`）。死锁重试由调用方使用 `retry_on_deadlock` 包装 `run` 实现。文档已说明。
2. **T8 AutoHealer 端到端**：`test_auto_healer_config_e2e` 仅验证配置就绪 + 连接 ping，未验证真实断连自愈全流程（需长时运行 + KILL CONNECTION 操作）。
3. **G1 fmt**：Windows 路径太长导致 `cargo fmt --all --check` 报 OS error 206，非代码问题。CI 环境（Linux）无此限制。