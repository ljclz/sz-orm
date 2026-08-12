# sz-orm v4.5.0 编码任务规划

> 版本：v4.5.0（并行查询执行器 + 批量 INSERT/UPDATE/DELETE 优化 + 异步流式结果集）
> 基线：v4.4.0（查询自动优化建议 + 慢查询自动诊断报告 + db-fusion 转正 + 结构化查询日志 + 性能回归基准线 + 查询智能闭环联动，6 项需求 REQ-V44-001~006 全部通过 feature gate 隔离，37 任务 / 184 子任务全部已完成并提交，约 7,000+ 个测试通过，14 道门禁全通过）
> 日期：2026-08-12
> 文档定位：编码任务规划（How to execute），对应需求规格 `spec.md`（What to build，688 行）+ 技术设计 `design.md`（How to build，1676 行）
> 任务约束：无 Breaking Change（3 个新 feature gate 隔离，默认全关闭）+ 优先复用既有能力 + 五方言覆盖 + 每项任务附 file:line 代码证据 + unsafe 零容忍 + 禁止占位实现（todo!/unimplemented!/unreachable!）+ 参数化查询强制
> 审计合规铁律：每项任务结论须附真实存在的 file:line 证据，修复后必须运行 `cargo test` 并附输出，禁止未验证即标记 ✅
> 实施顺序：按 design.md §7.2 依赖关系，M0（P0 文档基线，立即）→ M1/M2/M3（P1 三项需求并行开发，主体独立）→ M4（最终集成验证与文档同步，全部完成后）
> 与 v4.4.0 零重叠：v4.4.0 是"分析/建议/转正"层（优化建议/诊断报告/融合转正/结构化日志/性能基线/闭环联动），v4.5.0 是"执行优化"层（并行执行/批量执行/流式执行），新增范围全部落在新包（sz-orm-parallel/sz-orm-stream）或 v4.4.0 不触碰的既有包扩展（sz-orm-batch batch-v2 扩展）

---

# 一、任务总览

## 1.1 里程碑 × 任务数 × 预期工作量

| 里程碑 | 名称 | 对应需求 | 优先级 | 任务数 | 子任务数 | 预期工作量 | 启动时机 |
|--------|------|---------|--------|--------|----------|-----------|---------|
| M0 | 文档基线与准备 | — | P0 | 3 | 12 | 0.5 周 | 立即（v4.4.0 已完成） |
| M1 | 并行查询执行器 | REQ-V45-001 | P1 | 7 | 38 | 2 周 | 立即（新包，独立） |
| M2 | 批量 INSERT/UPDATE/DELETE 优化 | REQ-V45-002 | P1 | 7 | 42 | 2.5 周 | 立即（独立扩展） |
| M3 | 异步流式结果集 | REQ-V45-003 | P1 | 7 | 40 | 2 周 | 立即（新包，独立） |
| M4 | 集成验证与文档同步 | 全局 | P0 | 3 | 16 | 0.5 周 | M1+M2+M3 全部完成后 |
| **合计** | — | **3 项全覆盖** | — | **27** | **148** | **7.5 周** | — |

## 1.2 任务编号约定

- 主任务：`M{里程碑号}-T{任务序号}`（如 M1-T1）
- 子任务：`M{里程碑号}-T{任务序号}.{子任务序号}`（如 M1-T2.1）
- 集成验证任务：每个里程碑末尾固定一个集成测试与门禁验证任务（如 M1-T7）
- 里程碑内需求按 REQ-V45-xxx 序号顺序编排任务

## 1.3 全局约束（适用于所有任务）

1. **feature gate 隔离**：3 个新 feature（`parallel-query` / `batch-v2` / `stream-resultset`），默认全关闭，默认 feature 行为不变
2. **既有 API 不变**：既有公开 API 签名完全向后兼容，sz-pay 既有代码不受影响（sz-pay 从 crates.io 拉取 sz-orm-* 6 个包）
3. **禁止占位实现**：禁止 `todo!`/`unimplemented!`/`unreachable!`
4. **unsafe 零容忍**：所有新增代码无 `unsafe` 块，或必须有 `// SAFETY:` 注释
5. **五方言覆盖**：MySQL/PostgreSQL/SQLite/Oracle/MSSQL（并行查询/批量操作/流式结果集按方言能力适配，如 COPY 仅 PG）
6. **参数化查询强制**：任何 WHERE 条件必须参数化绑定，禁止 SQL 字符串拼接（复用既有 `DefaultBatchOps::quote` `packages/sz-orm-batch/src/lib.rs:177` + `Connection::execute_with_params` `packages/sz-orm-core/src/pool.rs:82`）
7. **审计证据**：每项任务结论附真实存在的 file:line 证据
8. **测试基线不回退**：v4.4.0 已验收测试基线（约 7,000+ 个测试）不回退，v4.5.0 仅增不减
9. **复用优先**：优先复用既有能力，不重复实现（并行查询复用 `Pool` `core/pool.rs:743` / `AdaptiveExecutor` `adaptive/executor.rs:120`；批量优化复用 `DefaultBatchOps` `batch/lib.rs:83` / `Connection::execute_with_params` `pool.rs:82` / `Transaction` `transaction.rs:159` / `RollbackStrategy` `batch/lib.rs:491`；流式结果集复用 `build_paged_query` `cursor_stream.rs:29` / `stream_cursor` `stream_api.rs:176` / `BackpressureController` `batch/stream.rs:40` / `Pool` `pool.rs:743`）
10. **Windows MSVC 编译环境**：RUST_MIN_STACK=134217728, CARGO_INCREMENTAL=0
11. **测试命令**：`cargo test --workspace -j 2 --no-fail-fast`；feature 包测试：`cargo test -p <package> --features <feature>`

## 1.4 里程碑依赖关系

```
M0（P0，文档基线，立即）
M1（P1，并行查询执行器，独立新包）
  - REQ-V45-001 复用既有 sz-orm-core Pool/Connection + sz-orm-adaptive AdaptiveExecutor
M2（P1，批量优化，独立扩展）
  - REQ-V45-002 复用既有 sz-orm-batch DefaultBatchOps + sz-orm-core Connection/Transaction/DbType
M3（P1，流式结果集，独立新包）
  - REQ-V45-003 复用既有 sz-orm-core cursor_stream/stream_api/paginator/Pool + sz-orm-batch BackpressureController
M4（P0，集成验证与文档同步，M1+M2+M3 全部完成后）
  - 依赖 M0~M3 全部完成
```

> **依赖关系说明**：M0 立即启动；M1/M2/M3 三项需求主体相互独立，可并行开发（design.md §7.2 明确声明）；M4 必须在 M1+M2+M3 全部完成后执行。三项需求无强依赖，不存在如 v4.4.0 M2 依赖 M1 的协同关系。

## 1.5 feature gate 定义与测试命令

| feature gate | 所属包 | 依赖 | 测试命令 | 默认 |
|-------------|--------|------|---------|------|
| `parallel-query` | sz-orm-parallel（新包） | tokio + futures + sz-orm-core（Pool/Connection）+ sz-orm-adaptive（AdaptiveExecutor） | `cargo test -p sz-orm-parallel --features parallel-query` | 关闭 |
| `batch-v2` | sz-orm-batch（扩展） | tokio + sz-orm-core（Connection/Transaction/DbType） | `cargo test -p sz-orm-batch --features batch-v2` | 关闭 |
| `stream-resultset` | sz-orm-stream（新包） | tokio + futures + sz-orm-core（cursor_stream/stream_api/paginator/Pool）+ sz-orm-batch（BackpressureController/StreamConfig） | `cargo test -p sz-orm-stream --features stream-resultset` | 关闭 |

---

# 二、M0：文档基线与准备（P0，0.5 周）

**目标**：锁定 v4.4.0 已验收基线，准备 v4.5.0 开发环境（新增 2 包 workspace 注册 + 3 feature gate 骨架 + 版本号升级）。
**对应需求**：—（文档基线与环境准备，非功能需求）
**预期工作量**：0.5 周
**依赖**：无（v4.4.0 已全部完成并提交）

## M0-T1：v4.4.0 完成总结与基线锁定

**任务描述**：总结 v4.4.0 交付成果（37 任务 / 184 子任务全部完成），锁定测试基线（约 7,000+ 个测试通过 + 14 道门禁全通过），作为 v4.5.0 开发的基准。

**涉及文件**：`docs/spec/v4.4.0/tasks.md`（既有，确认全部 `[x]`）、`docs/spec/v4.4.0/spec.md`（既有，870 行）、`docs/spec/v4.4.0/design.md`（既有，1801 行）

**子任务**：
- [ ] M0-T1.1 确认 `docs/spec/v4.4.0/tasks.md` 37 任务 / 184 子任务全部标记 `[x]`（v4.4.0 已完成）
- [ ] M0-T1.2 运行 `cargo test --workspace -j 2 --no-fail-fast` 记录 v4.4.0 测试基线（约 7,000+ 个测试通过）
- [ ] M0-T1.3 运行 14 道门禁全量验证，记录基线通过状态（fmt/check/clippy/test/doc/audit/integration/占位/SQL注入/feature组合/ADR-0001/文档一致性/审计证据/文档同步）
- [ ] M0-T1.4 确认 v4.4.0 6 个 feature gate（`query-advisor`/`slow-query-diagnosis`/`db-fusion-v2`/`query-logging`/`perf-baseline`/`query-intelligence-loop`）默认全关闭，行为不变

**验收标准**：v4.4.0 基线锁定（测试数 + 门禁通过状态 + feature gate 状态），每项附 file:line 或命令输出证据

**依赖**：无

## M0-T2：v4.5.0 开发环境准备

**任务描述**：在 workspace 注册新增 2 包（`sz-orm-parallel` / `sz-orm-stream`）骨架，创建 3 个新 feature gate 占位（默认关闭），升级版本号 4.4.0 → 4.5.0，验证 workspace 编译通过。

**涉及文件**：
- `Cargo.toml`（workspace.members 新增 2 包，workspace.package.version 4.4.0 → 4.5.0）
- `packages/sz-orm-parallel/Cargo.toml`（新建骨架）
- `packages/sz-orm-parallel/src/lib.rs`（新建骨架）
- `packages/sz-orm-stream/Cargo.toml`（新建骨架）
- `packages/sz-orm-stream/src/lib.rs`（新建骨架）
- `packages/sz-orm-batch/Cargo.toml`（扩展：新增 `batch-v2` feature 占位）

**复用标注**：既有 workspace 结构 `Cargo.toml`（58 个成员）；既有 feature gate 模式 `packages/sz-orm-core/Cargo.toml`（40+ feature）；既有 `packages/sz-orm-batch/Cargo.toml`（`batch-stream` feature）

**子任务**：
- [ ] M0-T2.1 创建 `packages/sz-orm-parallel/` 包骨架（`Cargo.toml` + `src/lib.rs` 空 lib），workspace.members 注册
- [ ] M0-T2.2 创建 `packages/sz-orm-stream/` 包骨架（`Cargo.toml` + `src/lib.rs` 空 lib），workspace.members 注册
- [ ] M0-T2.3 `sz-orm-parallel/Cargo.toml` 定义 `parallel-query` feature（默认关闭，依赖占位）
- [ ] M0-T2.4 `sz-orm-stream/Cargo.toml` 定义 `stream-resultset` feature（默认关闭，依赖占位）
- [ ] M0-T2.5 `sz-orm-batch/Cargo.toml` 新增 `batch-v2` feature（默认关闭，依赖占位，与既有 `batch-stream` 独立）
- [ ] M0-T2.6 `Cargo.toml` workspace.package.version 从 `4.4.0` 升级为 `4.5.0`
- [ ] M0-T2.7 验证 `cargo check --workspace` 编译通过（新增 2 包骨架不影响既有编译，workspace 成员 58 → 60）
- [ ] M0-T2.8 验证默认 feature 行为与 v4.4.0 一致（`cargo build --workspace` 行为不变）

**验收标准**：2 新包骨架创建成功，workspace 成员数 60，版本号 4.5.0，workspace 编译通过，默认 feature 行为不变

**依赖**：M0-T1

## M0-T3：基线验证

**任务描述**：运行文档一致性、审计证据、文档同步三道门禁，验证 v4.4.0 基线可被工具消费，v4.5.0 骨架不破坏既有基线。

**涉及文件**：`scripts/check-doc-consistency.py`、`scripts/audit-verify.sh`、`scripts/check-doc-sync.py`

**子任务**：
- [ ] M0-T3.1 运行 `python scripts/check-doc-consistency.py`（门禁 12），验证文档与代码一致
- [ ] M0-T3.2 运行 `bash scripts/audit-verify.sh docs/spec/v4.4.0/tasks.md`（门禁 13），验证 v4.4.0 tasks.md 所有 file:line 引用真实存在
- [ ] M0-T3.3 运行 `python scripts/check-doc-sync.py --diff HEAD`（门禁 14），验证文档与 HEAD 同步

**验收标准**：三道门禁全部通过；v4.4.0 tasks.md 所有 file:line 引用经 audit-verify 验证真实存在

**依赖**：M0-T2

---

# 三、M1：并行查询执行器（REQ-V45-001，P1，2 周）

**目标**：新增 `sz-orm-parallel` 包，`ParallelQueryScheduler` 基于既有连接池 `Pool`（`packages/sz-orm-core/src/pool.rs:743`）+ tokio 异步运行时，将多个独立查询并行执行降低复杂场景整体延迟，通过并发度控制（默认池 max_size 80%）避免连接池耗尽，与既有 `AdaptiveExecutor`（`packages/sz-orm-adaptive/src/executor.rs:120`）协同（单查询仍走自适应路径），`ResultMerger` 支持四种合并策略（First/Union/Join/Map），整体超时与单查询超时控制 + 单查询失败降级（Skip/Abort/Fallback）。
**对应需求**：REQ-V45-001（spec.md §5.1，design.md §2.2.2.1 + §2.1.3.1）
**预期工作量**：2 周
**依赖**：无（M1 为 P1 独立需求，复用既有 sz-orm-core Pool/Connection + sz-orm-adaptive AdaptiveExecutor，新包可与 M2/M3 并行）

## M1-T1：sz-orm-parallel 包搭建 + parallel-query feature gate

**任务描述**：完善 `sz-orm-parallel` 包骨架（M0-T2 已创建），定义 `parallel-query` feature gate 隔离，配置依赖（sz-orm-core + sz-orm-adaptive + tokio + futures + serde），作为并行查询执行器的基础设施。

**涉及文件**：
- `packages/sz-orm-parallel/Cargo.toml`（完善：依赖 sz-orm-core + sz-orm-adaptive + tokio + futures + serde）
- `packages/sz-orm-parallel/src/lib.rs`（完善：模块声明）
- `Cargo.toml`（workspace.members 已注册，M0-T2）

**复用标注**：既有 `Pool` `packages/sz-orm-core/src/pool.rs:743`；既有 `Connection` trait `packages/sz-orm-core/src/pool.rs:45`；既有 `AdaptiveExecutor` `packages/sz-orm-adaptive/src/executor.rs:120`；workspace 依赖 `tokio = { version = "1.40", features = ["full"] }`（`Cargo.toml:31`）

**feature gate 隔离**：`parallel-query = ["dep:tokio", "dep:futures"]`，默认关闭（sz-orm-core + sz-orm-adaptive 为非 optional 基础依赖）

**子任务**：
- [ ] M1-T1.1 `packages/sz-orm-parallel/Cargo.toml` 配置依赖：`sz-orm-core`（workspace）+ `sz-orm-adaptive`（workspace）+ `tokio`（workspace, optional）+ `futures`（workspace, optional）+ `serde`/`serde_json`
- [ ] M1-T1.2 `[features] parallel-query = ["dep:tokio", "dep:futures"]`，默认关闭
- [ ] M1-T1.3 `src/lib.rs` 声明模块结构（`mod config; mod scheduler; mod merger; mod outcome; mod error;`），`#[cfg(feature = "parallel-query")]` 门控
- [ ] M1-T1.4 验证 `cargo check -p sz-orm-parallel` 编译通过，默认 feature 行为不变（空 lib）
- [ ] M1-T1.5 验证 `cargo check -p sz-orm-parallel --features parallel-query` 编译通过（依赖链路打通）

**验收标准**：包搭建成功，feature gate 默认关闭，依赖链路打通，workspace 集成编译通过

**依赖**：M0-T2

## M1-T2：配置与枚举定义（ParallelQueryConfig/MergeStrategy/FailureStrategy）

**任务描述**：定义 `ParallelQueryConfig` 配置结构、`MergeStrategy` 合并策略枚举、`FailureStrategy` 降级策略枚举、`ParallelQuery` 单查询结构、`QueryFailure` 失败信息、`ParallelQueryError` 错误类型，作为并行查询执行器的数据模型。

**涉及文件**：
- `packages/sz-orm-parallel/src/config.rs`（新建，配置与枚举）
- `packages/sz-orm-parallel/src/error.rs`（新建，错误类型）
- `packages/sz-orm-parallel/src/outcome.rs`（新建，结果结构）

**复用标注**：既有 `QueryOutcome` `packages/sz-orm-adaptive/src/executor.rs:106`（单查询结果 value/rows/elapsed_ms/from_cache/slow）；既有 `Pool` max_size（`packages/sz-orm-core/src/pool.rs:743`，并发度默认 max_size 80%）

**子任务**：
- [ ] M1-T2.1 定义 `pub enum MergeStrategy { First, Union, Join { join_key: String }, Map }`（四种合并策略，`Serialize + Deserialize`）
- [ ] M1-T2.2 定义 `pub enum FailureStrategy { Skip, Abort, Fallback }`（三种降级策略，`Serialize + Deserialize`）
- [ ] M1-T2.3 定义 `pub struct ParallelQueryConfig { pub concurrency: usize, pub overall_timeout_ms: u64, pub per_query_timeout_ms: u64, pub failure_strategy: FailureStrategy, pub merge_strategy: MergeStrategy }`（配置结构）
- [ ] M1-T2.4 实现 `impl ParallelQueryConfig { pub fn new() -> Self }`（默认：concurrency 由池 max_size 80% 计算传入，overall_timeout_ms 30000，per_query_timeout_ms 10000，failure_strategy Skip，merge_strategy First）
- [ ] M1-T2.5 实现链式配置方法：`with_concurrency` / `with_overall_timeout_ms` / `with_per_query_timeout_ms` / `with_failure_strategy` / `with_merge_strategy`
- [ ] M1-T2.6 定义 `pub struct ParallelQuery<T> { pub sql: String, pub params: Vec<Value>, pub query_key: Option<String>, pub fallback_value: Option<T>, _marker: PhantomData<T> }`（单查询结构，参数独立绑定防交叉污染）
- [ ] M1-T2.7 实现 `impl<T> ParallelQuery<T> { pub fn new(sql: impl Into<String>, params: Vec<Value>) -> Self }` + `with_query_key` + `with_fallback_value`
- [ ] M1-T2.8 定义 `pub struct QueryFailure { pub query_index: usize, pub error: String }`（失败信息）
- [ ] M1-T2.9 定义 `pub struct ParallelQueryOutcome<T> { pub results: Vec<Option<QueryOutcome<T>>>, pub failures: Vec<QueryFailure>, pub timed_out: Vec<usize>, pub total_elapsed_ms: u64, pub merged_result: Option<T> }`（执行结果，复用既有 `QueryOutcome` `packages/sz-orm-adaptive/src/executor.rs:106`）
- [ ] M1-T2.10 定义 `pub enum ParallelQueryError { NoQueries, PoolExhausted, OverallTimeout, AllQueriesFailed, MergeFailed }`（错误类型）
- [ ] M1-T2.11 单元测试：`ParallelQueryConfig::new()` 默认值正确（overall_timeout_ms 30000, per_query_timeout_ms 10000, Skip, First）
- [ ] M1-T2.12 单元测试：链式配置 `with_concurrency(5).with_failure_strategy(Abort)` 生效
- [ ] M1-T2.13 单元测试：`MergeStrategy`/`FailureStrategy` 序列化/反序列化往返一致
- [ ] M1-T2.14 边界测试：`concurrency = 0` → `new()` 时由池 max_size 80% 填充，不 panic

**验收标准**：配置与枚举定义完整；链式配置方法正确；复用既有 `QueryOutcome`；序列化/反序列化测试通过

**依赖**：M1-T1

## M1-T3：ParallelQueryScheduler 并行调度器 + parallel 入口

**任务描述**：实现 `ParallelQueryScheduler` 并行调度器，`parallel` 入口将 N 个独立查询并行执行，从既有 `Pool` 获取连接，通过 tokio::join 并行执行，并发度控制避免池耗尽，单查询可走 `AdaptiveExecutor` 自适应路径。

**涉及文件**：
- `packages/sz-orm-parallel/src/scheduler.rs`（新建，并行调度器核心）

**复用标注**：既有 `Pool` `packages/sz-orm-core/src/pool.rs:743`（acquire 获取连接）；既有 `PooledConnection` `packages/sz-orm-core/src/pool.rs:239`（Drop 自动归还）；既有 `Connection::execute_with_params` `packages/sz-orm-core/src/pool.rs:82`（参数绑定执行）；既有 `AdaptiveExecutor` `packages/sz-orm-adaptive/src/executor.rs:120` + `decide` `:157`（单查询自适应路径）；tokio 异步运行时 `Cargo.toml:31`

**子任务**：
- [ ] M1-T3.1 定义 `pub struct ParallelQueryScheduler { pool: Pool, adaptive: Option<Arc<AdaptiveExecutor>> }`（调度器，持有连接池 + 可选自适应执行器）
- [ ] M1-T3.2 实现 `impl ParallelQueryScheduler { pub fn new(pool: Pool) -> Self }`（无自适应执行器，单查询走直连）
- [ ] M1-T3.3 实现 `impl ParallelQueryScheduler { pub fn with_adaptive(pool: Pool, adaptive: Arc<AdaptiveExecutor>) -> Self }`（含自适应执行器，单查询走自适应路径）
- [ ] M1-T3.4 实现 `pub async fn parallel<T>(&self, queries: Vec<ParallelQuery<T>>, config: ParallelQueryConfig) -> Result<ParallelQueryOutcome<T>, ParallelQueryError>`：并行执行入口
- [ ] M1-T3.5 并发度控制：实际并发度 = min(config.concurrency, pool.max_size * 80%)，避免耗尽连接池（复用 `Pool` max_size）
- [ ] M1-T3.6 连接获取：从 `Pool::acquire` 获取并发度个连接，连接获取失败降级为串行执行（降低并发度到可用连接数），不无限等待
- [ ] M1-T3.7 并行执行：`tokio::join!` 并行执行未完成查询，每个查询通过 `Connection::execute_with_params` 执行（参数独立绑定防交叉污染，复用 `packages/sz-orm-core/src/pool.rs:82`）
- [ ] M1-T3.8 自适应协同：若 `adaptive` 为 Some，单查询走 `AdaptiveExecutor::decide`（`packages/sz-orm-adaptive/src/executor.rs:157`）自适应路径（Cached/Paginated/Normal），并行调度器不干预单查询自适应决策
- [ ] M1-T3.9 整体超时控制：`tokio::time::timeout(overall_timeout)` 包裹整个并行执行，超时取消未完成查询释放连接
- [ ] M1-T3.10 单查询超时控制：`tokio::time::timeout(per_query_timeout)` 包裹单查询，超时取消该查询释放连接，`timed_out.push(query_index)`
- [ ] M1-T3.11 连接归还：所有获取的连接通过 `PooledConnection` Drop 自动归还（`packages/sz-orm-core/src/pool.rs:239`），不泄漏连接
- [ ] M1-T3.12 单元测试：传入 3 个独立查询，并发度 2 → 同时执行 2 个查询，第 3 个等待，整体延迟接近最慢 2 个查询之和
- [ ] M1-T3.13 单元测试：并发度 > 池 max_size → 降级为 max_size 并发，不耗尽池
- [ ] M1-T3.14 单元测试：连接池耗尽 → 降级为串行执行，标注"connection pool exhausted, degraded to serial execution"
- [ ] M1-T3.15 单元测试：`adaptive` 为 Some → 单查询走 `AdaptiveExecutor::decide` 自适应路径
- [ ] M1-T3.16 边界测试：传入空查询列表 → 返回 `ParallelQueryError::NoQueries`
- [ ] M1-T3.17 边界测试：`concurrency = 0` → 默认池 max_size 80%，不 panic
- [ ] M1-T3.18 性能测试：并行调度开销 ≤ 1ms（含并发度控制 + 连接获取调度）

**验收标准**：`parallel` 入口完整；并发度控制避免池耗尽；复用既有 `Pool`/`Connection`/`AdaptiveExecutor`；连接不泄漏；自适应协同正确；性能 ≤ 1ms 调度开销

**依赖**：M1-T1、M1-T2

## M1-T4：ResultMerger 结果合并器 + 四种合并策略

**任务描述**：实现 `ResultMerger` 结果合并器，按 `MergeStrategy` 将多个并行查询结果合并为单一结果，支持 First/Union/Join/Map 四种合并策略，合并失败降级返回原始结果列表。

**涉及文件**：
- `packages/sz-orm-parallel/src/merger.rs`（新建，结果合并器）

**复用标注**：既有 `QueryOutcome` `packages/sz-orm-adaptive/src/executor.rs:106`（单查询结果，合并输入）

**子任务**：
- [ ] M1-T4.1 定义 `pub struct ResultMerger;`（结果合并器，无状态）
- [ ] M1-T4.2 实现 `impl ResultMerger { pub fn merge<T>(results: Vec<Option<QueryOutcome<T>>>, strategy: MergeStrategy) -> Result<Option<T>, ParallelQueryError> }`：按策略合并
- [ ] M1-T4.3 策略 First：取首个完成的查询结果（`results.iter().find(|r| r.is_some()).and_then(|r| r.as_ref().unwrap().value.clone())`）
- [ ] M1-T4.4 策略 Union：并集合并所有查询结果（合并所有 `QueryOutcome.value` 为集合）
- [ ] M1-T4.5 策略 Join：按指定 `join_key` 关联多个查询结果（键匹配关联，键不匹配降级）
- [ ] M1-T4.6 策略 Map：映射转换各查询结果后合并（应用 transform 函数）
- [ ] M1-T4.7 合并失败降级：Join 键不匹配 / Map 转换失败 → 降级返回未合并的原始结果列表，标注"merge failed, returning raw results"
- [ ] M1-T4.8 单元测试：3 个查询结果 + `MergeStrategy::First` → 返回首个完成的结果
- [ ] M1-T4.9 单元测试：3 个查询结果 + `MergeStrategy::Union` → 返回 3 个查询结果的并集
- [ ] M1-T4.10 单元测试：Join 策略键匹配 → 关联合并正确
- [ ] M1-T4.11 单元测试：Join 策略键不匹配 → 降级返回原始结果列表，标注"merge failed"
- [ ] M1-T4.12 边界测试：空结果列表 → `merge` 返回 None，不 panic
- [ ] M1-T4.13 边界测试：所有结果为 None（全部失败）→ `merge` 返回 None

**验收标准**：四种合并策略实现正确；合并失败降级处理；复用既有 `QueryOutcome`；边界处理不 panic

**依赖**：M1-T2

## M1-T5：单查询失败降级处理

**任务描述**：实现单查询失败降级策略处理（Skip/Abort/Fallback），不因单查询失败导致整体 panic，按 `FailureStrategy` 处理。

**涉及文件**：
- `packages/sz-orm-parallel/src/scheduler.rs`（扩展：失败降级逻辑）

**复用标注**：既有 `QueryOutcome` `packages/sz-orm-adaptive/src/executor.rs:106`（失败查询 results 为 None）

**子任务**：
- [ ] M1-T5.1 策略 Skip：单查询失败 → `failures.push(QueryFailure { query_index, error })`，`results[i] = None`，继续其他查询
- [ ] M1-T5.2 策略 Abort：单查询失败 → 取消所有未完成查询释放连接，返回 `ParallelQueryError::AllQueriesFailed`
- [ ] M1-T5.3 策略 Fallback：单查询失败 → `results[i] = Some(QueryOutcome { value: fallback_value, ... })`，使用 `ParallelQuery.fallback_value`
- [ ] M1-T5.4 单元测试：3 个查询并行，策略 Skip，第 2 个查询失败 → 跳过失败查询，返回 results[0]=Some, results[1]=None, results[2]=Some + failures 含第 2 个
- [ ] M1-T5.5 单元测试：3 个查询并行，策略 Abort，第 2 个查询失败 → 取消所有查询，返回 `AllQueriesFailed` 错误
- [ ] M1-T5.6 单元测试：3 个查询并行，策略 Fallback，第 2 个查询失败且有 fallback_value → results[1] = Some(fallback_value)
- [ ] M1-T5.7 边界测试：策略 Fallback 但 `fallback_value = None` → 降级为 Skip 行为，标注"fallback value not provided, skipped"
- [ ] M1-T5.8 边界测试：所有查询失败 + 策略 Skip → 返回 results 全 None + failures 含全部，不 panic

**验收标准**：三种降级策略实现正确；不 panic；边界处理正确

**依赖**：M1-T3

## M1-T6：与 AdaptiveExecutor 协同 + 超时连接释放

**任务描述**：完善与既有 `AdaptiveExecutor` 的协同（单查询仍走自适应路径），确保超时后连接正确释放不泄漏，并行调度器不修改既有自适应决策逻辑。

**涉及文件**：
- `packages/sz-orm-parallel/src/scheduler.rs`（扩展：自适应协同 + 超时释放）

**复用标注**：既有 `AdaptiveExecutor::decide` `packages/sz-orm-adaptive/src/executor.rs:157`（自适应决策 Cached/Paginated/Normal）；既有 `ExecutionPath` `packages/sz-orm-adaptive/src/executor.rs:16`；既有 `PooledConnection` Drop `packages/sz-orm-core/src/pool.rs:239`（超时后 Drop 释放连接）

**子任务**：
- [ ] M1-T6.1 自适应协同：`adaptive` 为 Some 时，单查询走 `AdaptiveExecutor::decide`（`packages/sz-orm-adaptive/src/executor.rs:157`）选择执行路径，并行调度器不干预
- [ ] M1-T6.2 大结果集查询走 `ExecutionPath::Paginated`（`packages/sz-orm-adaptive/src/executor.rs:16`），其他查询走 Normal，并行调度器不修改自适应决策
- [ ] M1-T6.3 超时连接释放：单查询超时 → `tokio::time::timeout` 取消该查询，`PooledConnection` Drop 归还连接（`packages/sz-orm-core/src/pool.rs:239`），不泄漏
- [ ] M1-T6.4 整体超时连接释放：整体超时 → 取消所有未完成查询，所有连接通过 Drop 归还
- [ ] M1-T6.5 单元测试：并行查询含 1 个大结果集查询 → 该查询走 `ExecutionPath::Paginated`，其他走 Normal
- [ ] M1-T6.6 单元测试：单查询超时 5 秒，其中 1 个查询耗时 10 秒 → 该查询 5 秒后取消释放连接，返回其他结果 + 超时标记
- [ ] M1-T6.7 单元测试：整体超时 3 秒，3 个查询各耗时 5 秒 → 3 秒后全部取消释放连接，返回 `OverallTimeout`
- [ ] M1-T6.8 边界测试：超时 0 表示不超时 → 查询执行完成才返回
- [ ] M1-T6.9 连接泄漏验证：并行查询完成后，池可用连接数恢复初始值（所有连接已归还）

**验收标准**：自适应协同正确（不修改既有决策）；超时后连接释放不泄漏；复用既有 `AdaptiveExecutor::decide`/`ExecutionPath`/`PooledConnection` Drop

**依赖**：M1-T3、M1-T5

## M1-T7：M1 集成测试与门禁验证

**任务描述**：M1 里程碑集成测试与门禁验证，确保 REQ-V45-001 全部验收条件满足，默认 feature 行为不变。

**子任务**：
- [ ] M1-T7.1 集成测试：`ParallelQueryScheduler::parallel` 完整流程（3 个独立查询 → 并行执行 → 超时控制 → 失败降级 → 结果合并）
- [ ] M1-T7.2 集成测试：复用既有 `Pool` `packages/sz-orm-core/src/pool.rs:743` + `AdaptiveExecutor` `packages/sz-orm-adaptive/src/executor.rs:120` 真实数据并行查询
- [ ] M1-T7.3 集成测试：四种合并策略（First/Union/Join/Map）完整验证
- [ ] M1-T7.4 集成测试：三种降级策略（Skip/Abort/Fallback）完整验证
- [ ] M1-T7.5 运行 `cargo test -p sz-orm-parallel --features parallel-query`（全部通过）
- [ ] M1-T7.6 `cargo clippy -p sz-orm-parallel --features parallel-query -- -D warnings`（clippy 静态分析）
- [ ] M1-T7.7 `cargo fmt -p sz-orm-parallel -- --check`（fmt 格式检查）
- [ ] M1-T7.8 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-parallel/` 无占位实现
- [ ] M1-T7.9 扫描 `grep -rn 'unsafe' packages/sz-orm-parallel/` 无 unsafe 块（或有 `// SAFETY:` 注释）
- [ ] M1-T7.10 验证默认 feature 行为与 v4.4.0 一致（`cargo build -p sz-orm-parallel` 无并行查询，行为不变）
- [ ] M1-T7.11 验证 `parallel-query` 与既有 feature（`adaptive-query`）组合编译通过

**验收标准**：M1 集成测试通过；clippy/fmt/占位/unsafe 检查通过；默认 feature 行为不变；并行调度 + 四种合并 + 三种降级 + 超时控制 + 自适应协同 + 连接不泄漏全部验证

**依赖**：M1-T1、M1-T2、M1-T3、M1-T4、M1-T5、M1-T6

---

# 四、M2：批量 INSERT/UPDATE/DELETE 优化（REQ-V45-002，P1，2.5 周）

**目标**：扩展既有 `sz-orm-batch` 包，新增 `batch_delete`（IN 子句批量删除）+ `BatchExecutor` 异步批量执行器（通过既有 `Connection::execute_with_params` `packages/sz-orm-core/src/pool.rs:82` 真正执行）+ 五方言批量 SQL 生成（扩展既有 `UpsertMode` `packages/sz-orm-batch/src/lib.rs:50` 至 SQLite/Oracle/MSSQL）+ 事务边界集成（复用既有 `Transaction` `packages/sz-orm-core/src/transaction.rs:159` + `RollbackStrategy` `packages/sz-orm-batch/src/lib.rs:491`）+ PostgreSQL COPY 协议（可选，仅 PG 启用，其他方言降级多值 INSERT）。
**对应需求**：REQ-V45-002（spec.md §5.2，design.md §2.2.2.2 + §2.1.3.2）
**预期工作量**：2.5 周
**依赖**：无（M2 为 P1 独立需求，复用既有 sz-orm-batch + sz-orm-core，扩展包可与 M1/M3 并行）

## M2-T1：batch-v2 feature gate + UpsertMode 五方言扩展

**任务描述**：在 `sz-orm-batch` 新增 `batch-v2` feature gate（M0-T2 已创建占位），配置依赖（tokio + sz-orm-core），扩展既有 `UpsertMode` 枚举新增三方言变体（SQLite/Oracle/MSSQL），既有变体保留。

**涉及文件**：
- `packages/sz-orm-batch/Cargo.toml`（扩展：完善 `batch-v2` feature 依赖）
- `packages/sz-orm-batch/src/lib.rs`（扩展：`UpsertMode` 新增 3 变体，`#[cfg(feature = "batch-v2")]` 门控）

**复用标注**：既有 `UpsertMode` `packages/sz-orm-batch/src/lib.rs:50`（MysqlOnDuplicate/PostgresOnConflict，保留不动）；既有 `DbType` `packages/sz-orm-core/src/db_type.rs:11`（五方言枚举）；既有 `batch-stream` feature（独立，不冲突）

**feature gate 隔离**：`batch-v2 = ["dep:tokio", "dep:sz-orm-core"]`，默认关闭，与既有 `batch-stream` 独立

**子任务**：
- [ ] M2-T1.1 `packages/sz-orm-batch/Cargo.toml` 完善 `batch-v2 = ["dep:tokio", "dep:sz-orm-core"]` feature，新增 `tokio`（workspace, optional）+ `sz-orm-core`（workspace, optional）依赖
- [ ] M2-T1.2 扩展 `UpsertMode` 枚举（`packages/sz-orm-batch/src/lib.rs:50`）新增 `SqliteOnConflict`/`OracleMerge`/`MssqlMerge` 变体，`#[cfg(feature = "batch-v2")]` 门控新变体，既有 `MysqlOnDuplicate`/`PostgresOnConflict` 保留不动
- [ ] M2-T1.3 验证 `cargo check -p sz-orm-batch --features batch-v2` 编译通过（依赖链路打通）
- [ ] M2-T1.4 验证默认 `cargo check -p sz-orm-batch` 编译通过（既有 `UpsertMode` 两方言保留，行为不变）
- [ ] M2-T1.5 验证 `batch-v2` 与既有 `batch-stream` feature 组合编译通过（两 feature 独立）
- [ ] M2-T1.6 单元测试：`UpsertMode::SqliteOnConflict`/`OracleMerge`/`MssqlMerge` 序列化/反序列化正确
- [ ] M2-T1.7 单元测试：既有 `UpsertMode::MysqlOnDuplicate`/`PostgresOnConflict` 行为不变（向后兼容）

**验收标准**：`batch-v2` feature gate 定义；`UpsertMode` 五方言扩展；既有两方言保留；与 `batch-stream` 独立组合编译通过

**依赖**：M0-T2

## M2-T2：BatchDialect 方言抽象 + 五方言批量 SQL 生成

**任务描述**：实现 `BatchDialect` 方言抽象，按 `DbType` 适配五方言批量 INSERT/UPDATE/DELETE/UPSERT SQL 语法（SQLite ON CONFLICT/Oracle MERGE/MSSQL MERGE），复用既有 `DefaultBatchOps` SQL 生成逻辑。

**涉及文件**：
- `packages/sz-orm-batch/src/dialect.rs`（新建，方言抽象）

**复用标注**：既有 `DefaultBatchOps::quote` `packages/sz-orm-batch/src/lib.rs:177`（反引号转义防注入）；既有 `DefaultBatchOps::chunk_indices` `packages/sz-orm-batch/src/lib.rs:164`（分片迭代器）；既有 `DbType` `packages/sz-orm-core/src/db_type.rs:11`；既有 `UpsertMode` `packages/sz-orm-batch/src/lib.rs:50`；既有 `ConflictTarget` `packages/sz-orm-batch/src/lib.rs:503`

**子任务**：
- [ ] M2-T2.1 定义 `pub struct BatchDialect;`（方言抽象，无状态静态方法）
- [ ] M2-T2.2 实现 `pub fn build_batch_insert(db_type: DbType, table: &str, rows: &[Value], chunk: (usize, usize)) -> Result<(String, Vec<Value>), DbError>`：五方言多值 INSERT（语法基本一致，参数化绑定）
- [ ] M2-T2.3 实现 `pub fn build_batch_update(db_type: DbType, table: &str, rows: &[Value], pk: &str, chunk: (usize, usize)) -> Result<(String, Vec<Value>), DbError>`：五方言 CASE WHEN UPDATE
- [ ] M2-T2.4 实现 `pub fn build_batch_delete(db_type: DbType, table: &str, pk: &str, ids: &[Value], chunk: (usize, usize)) -> Result<(String, Vec<Value>), DbError>`：五方言 `WHERE pk IN (?, ?, ...)` 批量 DELETE（复用 `quote` `packages/sz-orm-batch/src/lib.rs:177` 转义）
- [ ] M2-T2.5 实现 `pub fn build_batch_upsert(db_type: DbType, table: &str, rows: &[Value], mode: UpsertMode, chunk: (usize, usize)) -> Result<(String, Vec<Value>), DbError>`：五方言 UPSERT
- [ ] M2-T2.6 UPSERT 方言适配：MySQL `ON DUPLICATE KEY UPDATE`（既有）/ PG `ON CONFLICT DO UPDATE`（既有）/ SQLite `ON CONFLICT DO UPDATE`（新增）/ Oracle `MERGE INTO`（新增）/ MSSQL `MERGE INTO`（新增）
- [ ] M2-T2.7 方言不支持降级：某方言不支持特定操作 → 降级为通用多值 INSERT，标注"dialect N does not support operation M, fallback to generic"
- [ ] M2-T2.8 单元测试：`DbType::Sqlite` 批量 UPSERT → 生成 `ON CONFLICT DO UPDATE` 语法
- [ ] M2-T2.9 单元测试：`DbType::Oracle` 批量 UPSERT → 生成 `MERGE INTO ... WHEN MATCHED THEN UPDATE ... WHEN NOT MATCHED THEN INSERT` 语法
- [ ] M2-T2.10 单元测试：`DbType::SqlServer` 批量 UPSERT → 生成 `MERGE INTO` 语法
- [ ] M2-T2.11 单元测试：五方言批量 DELETE → 生成 `WHERE pk IN (?, ?, ...)` 参数化绑定（复用 `quote` 防注入）
- [ ] M2-T2.12 单元测试：既有 MySQL/PG UPSERT 行为不变（向后兼容）
- [ ] M2-T2.13 边界测试：空 rows → 返回空 SQL 或错误，不 panic
- [ ] M2-T2.14 SQL 注入测试：表名/列名含注入尝试 `' OR 1=1 --` → `quote` 转义后注入失败

**验收标准**：五方言批量 SQL 生成正确；复用既有 `quote`/`chunk_indices`/`DbType`；SQL 注入防护；既有方言行为不变

**依赖**：M2-T1

## M2-T3：batch_delete 批量删除 + BatchDeleteRequest

**任务描述**：扩展既有 `BatchOperations` trait 新增 `batch_delete` 方法（独立方法不破坏既有 trait），实现 `BatchDeleteRequest` 删除请求结构，通过 IN 子句批量删除，复用既有分片逻辑，参数化绑定防 SQL 注入，范围保护拒绝空行/无条件删除。

**涉及文件**：
- `packages/sz-orm-batch/src/delete.rs`（新建，批量删除）
- `packages/sz-orm-batch/src/lib.rs`（扩展：`BatchOperations` trait 新增 `batch_delete` 方法，`#[cfg(feature = "batch-v2")]` 门控）

**复用标注**：既有 `BatchOperations` trait `packages/sz-orm-batch/src/lib.rs:43`（保留不动，新增方法）；既有 `DefaultBatchOps` `packages/sz-orm-batch/src/lib.rs:83`（实现 `batch_delete`）；既有 `DefaultBatchOps.primary_key` `packages/sz-orm-batch/src/lib.rs:84`（主键列名）；既有 `DefaultBatchOps.chunk_size` `packages/sz-orm-batch/src/lib.rs:93`（分片大小）；既有 `DefaultBatchOps::chunk_indices` `packages/sz-orm-batch/src/lib.rs:164`（分片迭代器）；既有 `DefaultBatchOps::quote` `packages/sz-orm-batch/src/lib.rs:177`（转义）；既有 `BatchResult` `packages/sz-orm-batch/src/lib.rs:19`

**子任务**：
- [ ] M2-T3.1 定义 `pub struct BatchDeleteRequest { pub table: String, pub primary_key: String, pub ids: Vec<Value> }`（删除请求，`ids` 非空防误删）
- [ ] M2-T3.2 实现 `impl BatchDeleteRequest { pub fn new(table: impl Into<String>, primary_key: impl Into<String>, ids: Vec<Value>) -> Result<Self, BatchError> }`：校验 `ids` 非空（空则 `BatchError::EmptyIds`）+ `primary_key` 非空（空则 `BatchError::MissingPrimaryKey`）
- [ ] M2-T3.3 扩展 `BatchOperations` trait 新增 `fn batch_delete(&self, request: &BatchDeleteRequest) -> BatchResult` 方法（`#[cfg(feature = "batch-v2")]` 门控，既有方法保留不动）
- [ ] M2-T3.4 实现 `DefaultBatchOps::batch_delete`：按 `chunk_size` 分片（复用 `chunk_indices` `packages/sz-orm-batch/src/lib.rs:164`）生成多条 `DELETE FROM table WHERE pk IN (?, ?, ...)` SQL，参数化绑定
- [ ] M2-T3.5 范围保护：`ids` 为空 → 拒绝执行返回空 `BatchResult { failed: 0, generated_sqls: [] }`；`primary_key` 为空 → 返回错误"batch_delete requires primary key values"
- [ ] M2-T3.6 单元测试：传入 2500 行删除，chunk_size 1000 → 生成 3 条 DELETE SQL（1000 + 1000 + 500），`WHERE pk IN (?, ?, ...)` 参数化绑定
- [ ] M2-T3.7 单元测试：`BatchDeleteRequest::new` 空 ids → 返回 `BatchError::EmptyIds`
- [ ] M2-T3.8 单元测试：`BatchDeleteRequest::new` 空 primary_key → 返回 `BatchError::MissingPrimaryKey`
- [ ] M2-T3.9 边界测试：ids 数量 = chunk_size → 生成 1 条 DELETE SQL
- [ ] M2-T3.10 SQL 注入测试：表名含注入尝试 → `quote` 转义后注入失败

**验收标准**：`batch_delete` 实现正确；复用既有分片/转义；参数化绑定防注入；范围保护拒绝空行/无条件删除；既有 `BatchOperations` trait 保留

**依赖**：M2-T1、M2-T2

## M2-T4：BatchExecutor 异步批量执行器 + BatchExecutorConfig

**任务描述**：实现 `BatchExecutor` 异步批量执行器，通过既有 `Connection::execute_with_params` 真正执行批量 SQL（既有 `BatchOperations` trait 同步返回 `BatchResult` 仅生成 SQL），复用既有 `DefaultBatchOps` SQL 生成 + `BatchResult` 返回 + 进度回调。

**涉及文件**：
- `packages/sz-orm-batch/src/executor.rs`（新建，异步批量执行器）

**复用标注**：既有 `DefaultBatchOps` `packages/sz-orm-batch/src/lib.rs:83`（SQL 生成）；既有 `Connection::execute_with_params` `packages/sz-orm-core/src/pool.rs:82`（参数绑定执行）；既有 `BatchResult` `packages/sz-orm-batch/src/lib.rs:19`（inserted/updated/failed/generated_sqls）；既有 `ProgressCallback` `packages/sz-orm-batch/src/lib.rs:482`；既有 `BatchProgress` `packages/sz-orm-batch/src/lib.rs:451`；既有 `DEFAULT_CHUNK_SIZE` `packages/sz-orm-batch/src/lib.rs:119`（1000）；既有 `RollbackStrategy` `packages/sz-orm-batch/src/lib.rs:491`

**子任务**：
- [ ] M2-T4.1 定义 `pub struct BatchExecutorConfig { pub chunk_size: usize, pub rollback_strategy: RollbackStrategy, pub progress_callback: Option<ProgressCallback>, pub use_copy_protocol: bool, pub transaction: Option<Transaction> }`（执行配置）
- [ ] M2-T4.2 实现 `impl BatchExecutorConfig { pub fn new() -> Self }`（默认：chunk_size 1000 复用 `DEFAULT_CHUNK_SIZE` `packages/sz-orm-batch/src/lib.rs:119`，rollback_strategy None，use_copy_protocol false，transaction None）
- [ ] M2-T4.3 实现链式配置：`with_chunk_size` / `with_rollback_strategy` / `with_progress_callback` / `with_copy_protocol` / `with_transaction`
- [ ] M2-T4.4 定义 `pub struct BatchExecutor { ops: DefaultBatchOps, db_type: DbType }`（异步执行器）
- [ ] M2-T4.5 实现 `impl BatchExecutor { pub fn new(db_type: DbType) -> Self }` + `with_ops(db_type, ops)`
- [ ] M2-T4.6 实现 `pub async fn execute_batch_insert(&self, conn: &mut dyn Connection, table: &str, rows: Vec<Value>, config: &BatchExecutorConfig) -> Result<BatchResult, DbError>`：复用 `DefaultBatchOps` 生成 SQL + `Connection::execute_with_params`（`packages/sz-orm-core/src/pool.rs:82`）执行 + 进度回调触发
- [ ] M2-T4.7 实现 `pub async fn execute_batch_update(&self, conn: &mut dyn Connection, table: &str, rows: Vec<Value>, config: &BatchExecutorConfig) -> Result<BatchResult, DbError>`
- [ ] M2-T4.8 实现 `pub async fn execute_batch_delete(&self, conn: &mut dyn Connection, request: &BatchDeleteRequest, config: &BatchExecutorConfig) -> Result<BatchResult, DbError>`
- [ ] M2-T4.9 实现 `pub async fn execute_batch_upsert(&self, conn: &mut dyn Connection, table: &str, rows: Vec<Value>, config: &BatchExecutorConfig) -> Result<BatchResult, DbError>`
- [ ] M2-T4.10 进度回调：每分片执行前后触发 `ProgressCallback`（Started → ProcessingChunk → ChunkCompleted → Finished，复用 `BatchProgress` `packages/sz-orm-batch/src/lib.rs:451`）
- [ ] M2-T4.11 空行处理：`rows` 为空 → 返回空 `BatchResult`，不执行 SQL
- [ ] M2-T4.12 单元测试：`execute_batch_insert` 2500 行，chunk_size 1000 → 3 分片执行，返回 `BatchResult { inserted: 2500, generated_sqls: [3 条] }`
- [ ] M2-T4.13 单元测试：进度回调触发顺序 Started → ProcessingChunk × 3 → ChunkCompleted × 3 → Finished
- [ ] M2-T4.14 单元测试：空 rows → 返回空 `BatchResult`，不执行 SQL
- [ ] M2-T4.15 边界测试：分片失败 → failed 计入 `BatchResult.failed`，不 panic

**验收标准**：`BatchExecutor` 异步执行器完整；复用既有 `DefaultBatchOps`/`execute_with_params`/`BatchResult`/`ProgressCallback`；进度回调正确；空行处理正确

**依赖**：M2-T1、M2-T2、M2-T3

## M2-T5：事务边界与部分失败回滚

**任务描述**：扩展 `BatchExecutor` 支持事务边界，复用既有 `Transaction` + `TransactionManager` + `retry_on_deadlock`，部分分片失败时按 `RollbackStrategy`（None/Savepoint/PerChunk）处理，保证事务一致性。

**涉及文件**：
- `packages/sz-orm-batch/src/executor.rs`（扩展：事务边界逻辑）

**复用标注**：既有 `Transaction` `packages/sz-orm-core/src/transaction.rs:159`（conn + state + options + savepoint_counter）；既有 `TransactionManager` `packages/sz-orm-core/src/transaction.rs:527`；既有 `retry_on_deadlock` `packages/sz-orm-core/src/transaction.rs:466`（死锁检测 + 指数退避重试）；既有 `IsolationLevel` `packages/sz-orm-core/src/transaction.rs:16`；既有 `RollbackStrategy` `packages/sz-orm-batch/src/lib.rs:491`（None/Savepoint/PerChunk）

**子任务**：
- [ ] M2-T5.1 事务边界执行：`config.transaction` 为 Some → 在事务边界内执行分片，复用 `Transaction` `packages/sz-orm-core/src/transaction.rs:159`
- [ ] M2-T5.2 策略 None：失败分片计入 `failed` 不影响已成功分片，继续执行后续分片
- [ ] M2-T5.3 策略 Savepoint：每分片前生成 `SAVEPOINT sp_N`，分片失败 → `ROLLBACK TO SAVEPOINT sp_N`，已成功分片保留，继续后续分片
- [ ] M2-T5.4 策略 PerChunk：任一分片失败 → 整批中止，后续分片不再执行，已成功分片保留
- [ ] M2-T5.5 事务提交：全部分片成功 → `commit`；全部分片失败 + 策略 Abort → `rollback`
- [ ] M2-T5.6 死锁重试：事务死锁 → 复用 `retry_on_deadlock` `packages/sz-orm-core/src/transaction.rs:466` 重试，重试超限返回错误"batch transaction deadlock, retried N times, failed"
- [ ] M2-T5.7 单元测试：批量 INSERT 3 分片，第 2 分片失败，`RollbackStrategy::Savepoint` → 第 1 分片保留，第 2 分片回滚到 savepoint，第 3 分片继续，返回 `BatchResult { inserted: 2000, failed: 1000 }`
- [ ] M2-T5.8 单元测试：`RollbackStrategy::PerChunk` → 第 1 分片保留，第 2 分片失败后中止，第 3 分片不执行
- [ ] M2-T5.9 单元测试：`RollbackStrategy::None` → 第 2 分片失败计入 failed，第 1/3 分片成功
- [ ] M2-T5.10 单元测试：事务死锁 → `retry_on_deadlock` 重试成功
- [ ] M2-T5.11 边界测试：无事务（`config.transaction = None`）→ 分片独立执行，失败计入 failed
- [ ] M2-T5.12 边界测试：全部分片失败 + PerChunk → 第 1 分片失败即中止

**验收标准**：事务边界 + 三种回滚策略正确；复用既有 `Transaction`/`retry_on_deadlock`/`RollbackStrategy`；死锁重试；事务一致性保证

**依赖**：M2-T4

## M2-T6：PostgreSQL COPY 协议

**任务描述**：实现 PostgreSQL COPY 协议批量导入（`COPY table FROM STDIN`，比多值 INSERT 性能更高，跳过 SQL 解析），仅 PostgreSQL 方言支持，其他方言降级为多值 INSERT，COPY 失败降级重试。

**涉及文件**：
- `packages/sz-orm-batch/src/copy.rs`（新建，PG COPY 协议）

**复用标注**：既有 `Connection` trait `packages/sz-orm-core/src/pool.rs:45`（PG COPY 协议扩展方法）；既有 `DbType` `packages/sz-orm-core/src/db_type.rs:11`（方言判定）；既有 `BatchResult` `packages/sz-orm-batch/src/lib.rs:19`

**子任务**：
- [ ] M2-T6.1 定义 `pub struct CopyProtocolExecutor { db_type: DbType }`（PG COPY 执行器）
- [ ] M2-T6.2 实现 `impl CopyProtocolExecutor { pub fn new(db_type: DbType) -> Self }`
- [ ] M2-T6.3 实现 `pub async fn execute_copy(&self, conn: &mut dyn Connection, table: &str, rows: &[Value]) -> Result<BatchResult, DbError>`：`COPY table (col1, col2, ...) FROM STDIN WITH (FORMAT csv)` 原生批量导入
- [ ] M2-T6.4 方言判定：仅 `DbType::PostgreSQL` + `use_copy_protocol == true` 启用 COPY，其他方言降级多值 INSERT，标注"COPY not supported, fallback to multi-value INSERT"
- [ ] M2-T6.5 COPY 失败降级：COPY 协议执行失败（连接中断/数据格式错误）→ 降级为多值 INSERT 重试，标注"COPY protocol failed, retried with multi-value INSERT"
- [ ] M2-T6.6 `BatchExecutor::execute_batch_insert` 集成：`use_copy_protocol == true` 且 `DbType::PostgreSQL` → 调用 `CopyProtocolExecutor::execute_copy`，否则多值 INSERT
- [ ] M2-T6.7 单元测试：`DbType::PostgreSQL` + COPY 启用 + 批量 INSERT 10000 行 → 使用 COPY 协议导入
- [ ] M2-T6.8 单元测试：`DbType::MySQL` + COPY 启用 → 降级为多值 INSERT，标注"COPY not supported, fallback"
- [ ] M2-T6.9 单元测试：COPY 协议失败 → 降级多值 INSERT 重试，标注"COPY failed, fallback"
- [ ] M2-T6.10 边界测试：空 rows → 不执行 COPY，返回空 `BatchResult`
- [ ] M2-T6.11 性能测试：PG COPY 10000 行导入性能优于多值 INSERT（跳过 SQL 解析）

**验收标准**：PG COPY 协议正确；方言判定 + 降级处理；COPY 失败降级重试；性能优于多值 INSERT

**依赖**：M2-T4

## M2-T7：M2 集成测试与门禁验证

**任务描述**：M2 里程碑集成测试与门禁验证，确保 REQ-V45-002 全部验收条件满足，默认 feature 行为不变。

**子任务**：
- [ ] M2-T7.1 集成测试：`BatchExecutor::execute_batch_insert/update/delete/upsert` 完整流程（SQL 生成 → 分片 → 执行 → 进度回调 → BatchResult）
- [ ] M2-T7.2 集成测试：五方言批量 SQL 生成（MySQL/PG/SQLite/Oracle/MSSQL UPSERT + DELETE）
- [ ] M2-T7.3 集成测试：事务边界 + 三种回滚策略（None/Savepoint/PerChunk）完整验证
- [ ] M2-T7.4 集成测试：PostgreSQL COPY 协议 + 降级处理
- [ ] M2-T7.5 集成测试：复用既有 `DefaultBatchOps` `packages/sz-orm-batch/src/lib.rs:83` + `Connection::execute_with_params` `packages/sz-orm-core/src/pool.rs:82` + `Transaction` `packages/sz-orm-core/src/transaction.rs:159`
- [ ] M2-T7.6 运行 `cargo test -p sz-orm-batch --features batch-v2`（全部通过）
- [ ] M2-T7.7 `cargo clippy -p sz-orm-batch --features batch-v2 -- -D warnings`
- [ ] M2-T7.8 `cargo fmt -p sz-orm-batch -- --check`
- [ ] M2-T7.9 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-batch/src/executor.rs packages/sz-orm-batch/src/dialect.rs packages/sz-orm-batch/src/delete.rs packages/sz-orm-batch/src/copy.rs` 无占位实现
- [ ] M2-T7.10 扫描 `grep -rn 'unsafe' packages/sz-orm-batch/src/executor.rs packages/sz-orm-batch/src/dialect.rs packages/sz-orm-batch/src/delete.rs packages/sz-orm-batch/src/copy.rs` 无 unsafe 块
- [ ] M2-T7.11 验证默认 feature 行为与 v4.4.0 一致（`cargo build -p sz-orm-batch` 既有 `BatchOperations` trait 行为不变）
- [ ] M2-T7.12 验证 `batch-v2` 与既有 `batch-stream` feature 组合编译通过

**验收标准**：M2 集成测试通过；门禁通过；默认行为不变；batch_delete + 异步执行器 + 五方言 + 事务边界 + PG COPY + 范围保护 + 参数化绑定全部验证

**依赖**：M2-T1、M2-T2、M2-T3、M2-T4、M2-T5、M2-T6

---

# 五、M3：异步流式结果集（REQ-V45-003，P1，2 周）

**目标**：新增 `sz-orm-stream` 包，`StreamResultSet` 实现异步 Stream trait 逐批 yield 避免一次性加载全量，`KeysetPaginator` keyset pagination（`WHERE key > last_key ORDER BY key LIMIT batch`，深翻页高效），三种分页策略可选（Keyset/LimitOffset/ServerCursor，LimitOffset 复用既有 `build_paged_query` `packages/sz-orm-core/src/cursor_stream.rs:29`，ServerCursor 复用既有 `stream_cursor` `packages/sz-orm-core/src/stream_api.rs:176`），背压控制与异步 Stream 集成（复用既有 `BackpressureController` `packages/sz-orm-batch/src/stream.rs:40` 语义），连接池集成（每批从池获取连接，批次完成归还）。
**对应需求**：REQ-V45-003（spec.md §5.3，design.md §2.2.2.3 + §2.1.3.3）
**预期工作量**：2 周
**依赖**：无（M3 为 P1 独立需求，复用既有 sz-orm-core cursor_stream/stream_api/paginator/Pool + sz-orm-batch BackpressureController，新包可与 M1/M2 并行）

## M3-T1：sz-orm-stream 包搭建 + stream-resultset feature gate

**任务描述**：完善 `sz-orm-stream` 包骨架（M0-T2 已创建），定义 `stream-resultset` feature gate 隔离，配置依赖（sz-orm-core + sz-orm-batch + tokio + futures + serde），作为异步流式结果集的基础设施。

**涉及文件**：
- `packages/sz-orm-stream/Cargo.toml`（完善：依赖 sz-orm-core + sz-orm-batch + tokio + futures + serde）
- `packages/sz-orm-stream/src/lib.rs`（完善：模块声明）
- `Cargo.toml`（workspace.members 已注册，M0-T2）

**复用标注**：既有 `build_paged_query` `packages/sz-orm-core/src/cursor_stream.rs:29`；既有 `stream_cursor` `packages/sz-orm-core/src/stream_api.rs:176`；既有 `BackpressureController` `packages/sz-orm-batch/src/stream.rs:40`；既有 `Pool` `packages/sz-orm-core/src/pool.rs:743`

**feature gate 隔离**：`stream-resultset = ["dep:tokio", "dep:futures"]`，默认关闭（sz-orm-core + sz-orm-batch 为非 optional 基础依赖）

**子任务**：
- [ ] M3-T1.1 `packages/sz-orm-stream/Cargo.toml` 配置依赖：`sz-orm-core`（workspace）+ `sz-orm-batch`（workspace）+ `tokio`（workspace, optional）+ `futures`（workspace, optional）+ `serde`/`serde_json`
- [ ] M3-T1.2 `[features] stream-resultset = ["dep:tokio", "dep:futures"]`，默认关闭
- [ ] M3-T1.3 `src/lib.rs` 声明模块结构（`mod config; mod result_set; mod keyset; mod backpressure;`），`#[cfg(feature = "stream-resultset")]` 门控
- [ ] M3-T1.4 验证 `cargo check -p sz-orm-stream` 编译通过，默认 feature 行为不变（空 lib）
- [ ] M3-T1.5 验证 `cargo check -p sz-orm-stream --features stream-resultset` 编译通过（依赖链路打通）

**验收标准**：包搭建成功，feature gate 默认关闭，依赖链路打通，workspace 集成编译通过

**依赖**：M0-T2

## M3-T2：配置与枚举定义（StreamResultSetConfig/PaginationStrategy/OrderDirection）

**任务描述**：定义 `StreamResultSetConfig` 流式配置、`PaginationStrategy` 分页策略枚举、`OrderDirection` 排序方向枚举，作为异步流式结果集的数据模型。

**涉及文件**：
- `packages/sz-orm-stream/src/config.rs`（新建，配置与枚举）

**复用标注**：既有 `StreamConfig.batch_size` `packages/sz-orm-batch/src/stream.rs:11`（批次大小默认 1000）；既有 `StreamConfig.backpressure_threshold` `packages/sz-orm-batch/src/stream.rs:13`（背压阈值默认 10000）；既有 `DbType` `packages/sz-orm-core/src/db_type.rs:11`

**子任务**：
- [ ] M3-T2.1 定义 `pub enum PaginationStrategy { Keyset, LimitOffset, ServerCursor }`（三种分页策略，`Serialize + Deserialize`）
- [ ] M3-T2.2 定义 `pub enum OrderDirection { Asc, Desc }`（排序方向，默认 Asc）
- [ ] M3-T2.3 定义 `pub struct StreamResultSetConfig { pub batch_size: usize, pub backpressure_threshold: usize, pub pagination_strategy: PaginationStrategy, pub keyset_column: Option<String>, pub db_type: DbType }`（流式配置）
- [ ] M3-T2.4 实现 `impl StreamResultSetConfig { pub fn new(db_type: DbType) -> Self }`（默认：batch_size 1000 复用 `StreamConfig.batch_size` `packages/sz-orm-batch/src/stream.rs:11`，backpressure_threshold 10000 复用 `:13`，pagination_strategy LimitOffset）
- [ ] M3-T2.5 实现链式配置：`with_batch_size` / `with_backpressure_threshold` / `with_pagination_strategy` / `with_keyset_column`
- [ ] M3-T2.6 单元测试：`StreamResultSetConfig::new(DbType::PostgreSQL)` 默认值正确（batch_size 1000, backpressure_threshold 10000, LimitOffset）
- [ ] M3-T2.7 单元测试：链式配置 `with_batch_size(500).with_pagination_strategy(Keyset).with_keyset_column("id")` 生效
- [ ] M3-T2.8 单元测试：`PaginationStrategy`/`OrderDirection` 序列化/反序列化往返一致
- [ ] M3-T2.9 边界测试：`pagination_strategy = Keyset` 但 `keyset_column = None` → 运行时错误"keyset pagination requires keyset_column"

**验收标准**：配置与枚举定义完整；链式配置方法正确；复用既有 `StreamConfig` 默认值

**依赖**：M3-T1

## M3-T3：KeysetPaginator keyset 分页器

**任务描述**：实现 `KeysetPaginator` keyset 分页器，生成 `WHERE key > last_key ORDER BY key LIMIT batch` keyset 分页 SQL，避免 OFFSET 深翻页性能退化，支持 Asc/Desc 排序方向。

**涉及文件**：
- `packages/sz-orm-stream/src/keyset.rs`（新建，keyset 分页器）

**复用标注**：既有 `build_paged_query` `packages/sz-orm-core/src/cursor_stream.rs:29`（LimitOffset 策略复用，keyset 为新增不修改既有）

**子任务**：
- [ ] M3-T3.1 定义 `pub struct KeysetPaginator { key_column: String, last_key: Option<Value>, batch_size: usize, order_direction: OrderDirection }`（keyset 分页器状态）
- [ ] M3-T3.2 实现 `impl KeysetPaginator { pub fn new(key_column: impl Into<String>, batch_size: usize) -> Self }`（默认 Asc）
- [ ] M3-T3.3 实现 `with_order_direction(mut self, direction: OrderDirection) -> Self`
- [ ] M3-T3.4 实现 `pub fn build_next_page_sql(&self, base_sql: &str) -> String`：生成 `WHERE key > last_key ORDER BY key LIMIT batch`（Asc）或 `WHERE key < last_key ORDER BY key DESC LIMIT batch`（Desc），首次无 `WHERE key >` 条件
- [ ] M3-T3.5 实现 `pub fn update_last_key(&mut self, last_row: &RowResult)`：更新 `last_key` 为本批最后一行的键值
- [ ] M3-T3.6 实现 `pub fn has_more(&self) -> bool`：是否还有更多数据（基于上次查询结果判定）
- [ ] M3-T3.7 单元测试：首次 `build_next_page_sql` → `SELECT * FROM t ORDER BY id LIMIT 1000`（无 WHERE 条件）
- [ ] M3-T3.8 单元测试：`update_last_key` 后 `build_next_page_sql` → `SELECT * FROM t WHERE id > 1000 ORDER BY id LIMIT 1000`
- [ ] M3-T3.9 单元测试：Desc 排序 → `WHERE id < last_key ORDER BY id DESC LIMIT batch`
- [ ] M3-T3.10 单元测试：keyset 第 100 万页 → `WHERE id > last_id ORDER BY id LIMIT 1000` 索引扫描，性能优于 OFFSET 1000000
- [ ] M3-T3.11 边界测试：`last_key = None`（首次）→ 不生成 WHERE 条件
- [ ] M3-T3.12 边界测试：空批结果 → `has_more` 返回 false

**验收标准**：keyset 分页 SQL 生成正确；Asc/Desc 支持；深翻页高效（索引扫描 vs OFFSET 全表扫描）

**依赖**：M3-T2

## M3-T4：AsyncBackpressureController 异步背压控制器

**任务描述**：实现 `AsyncBackpressureController` 异步背压控制器，复用既有 `BackpressureController` 语义，扩展为异步 Stream 集成（消费者慢于生产者时暂停生产者拉取），背压检查开销 ≤ 1μs/次。

**涉及文件**：
- `packages/sz-orm-stream/src/backpressure.rs`（新建，异步背压控制器）

**复用标注**：既有 `BackpressureController` `packages/sz-orm-batch/src/stream.rs:40`（allow_push/push/pop/pending，同步结构，复用语义）；既有 `StreamConfig.backpressure_threshold` `packages/sz-orm-batch/src/stream.rs:13`（默认 10000）

**子任务**：
- [ ] M3-T4.1 定义 `pub struct AsyncBackpressureController { threshold: usize, current: Arc<AtomicUsize>, notify: Arc<Notify> }`（异步背压，`AtomicUsize` 无锁 + `Notify` 异步通知）
- [ ] M3-T4.2 实现 `impl AsyncBackpressureController { pub fn new(threshold: usize) -> Self }`（复用 `StreamConfig.backpressure_threshold` `packages/sz-orm-batch/src/stream.rs:13` 默认 10000）
- [ ] M3-T4.3 实现 `pub async fn allow_push(&self) -> bool`：检查 `current < threshold`，false 时等待消费者处理（`notify.notified().await`）
- [ ] M3-T4.4 实现 `pub fn push(&self)`：入队 `current.fetch_add(1)`
- [ ] M3-T4.5 实现 `pub fn pop(&self)`：出队 `current.fetch_sub(1)` + `notify.notify_one()` 唤醒生产者
- [ ] M3-T4.6 实现 `pub fn pending(&self) -> usize`：当前积压量 `current.load()`
- [ ] M3-T4.7 单元测试：`pending < threshold` → `allow_push` 返回 true
- [ ] M3-T4.8 单元测试：`pending >= threshold` → `allow_push` 等待消费者处理（返回 Pending）
- [ ] M3-T4.9 单元测试：`pop` 后 `notify` 唤醒等待的生产者
- [ ] M3-T4.10 边界测试：`threshold = 0` → 始终暂停生产者（极端背压）
- [ ] M3-T4.11 性能测试：背压检查开销 ≤ 1μs/次（含队列长度检查 + 暂停/恢复判定）

**验收标准**：异步背压控制器正确；复用既有 `BackpressureController` 语义；异步 Stream 集成；性能 ≤ 1μs/次

**依赖**：M3-T1

## M3-T5：StreamResultSet 异步流式结果集 + Stream trait 实现

**任务描述**：实现 `StreamResultSet` 异步流式结果集，实现异步 Stream trait 逐批 yield 结果，按 `PaginationStrategy` 选择分页策略（Keyset/LimitOffset/ServerCursor），复用既有 `build_paged_query`/`stream_cursor` 语义，背压控制集成。

**涉及文件**：
- `packages/sz-orm-stream/src/result_set.rs`（新建，异步流式结果集核心）

**复用标注**：既有 `build_paged_query` `packages/sz-orm-core/src/cursor_stream.rs:29`（LimitOffset 策略复用，五方言分页 SQL）；既有 `stream_cursor_paged` `packages/sz-orm-core/src/cursor_stream.rs:79`（分页游标 Stream 语义）；既有 `stream_cursor` `packages/sz-orm-core/src/stream_api.rs:176`（ServerCursor 策略复用，真游标 Stream）；既有 `Paginator` `packages/sz-orm-core/src/paginator.rs:158`；既有 `Pool` `packages/sz-orm-core/src/pool.rs:743`；既有 `PooledConnection` `packages/sz-orm-core/src/pool.rs:239`

**子任务**：
- [ ] M3-T5.1 定义 `enum StreamState { NotStarted, Paging { offset: u64 }, Keyset { paginator: KeysetPaginator }, ServerCursor { conn: PooledConnection }, Done }`（流式状态机）
- [ ] M3-T5.2 定义 `pub struct StreamResultSet<'a> { pool: &'a Pool, sql: String, params: Vec<Value>, config: StreamResultSetConfig, state: StreamState, backpressure: AsyncBackpressureController }`（异步流式结果集）
- [ ] M3-T5.3 实现 `impl<'a> StreamResultSet<'a> { pub fn new(pool: &'a Pool, sql: impl Into<String>, config: StreamResultSetConfig) -> Self }`
- [ ] M3-T5.4 实现 `with_params(mut self, params: Vec<Value>) -> Self`
- [ ] M3-T5.5 实现 `pub fn stream_query(self) -> Pin<Box<dyn Stream<Item = Result<Vec<RowResult>, DbError>> + Send + 'a>>`：返回异步 Stream
- [ ] M3-T5.6 实现 `Stream::poll_next`：背压检查 → 分页策略选择 → 连接获取 → 查询执行 → 结果 yield → 连接归还
- [ ] M3-T5.7 策略 Keyset：`KeysetPaginator::build_next_page_sql` 生成 keyset SQL → 从 `Pool::acquire` 获取连接执行 → `update_last_key` → 归还连接
- [ ] M3-T5.8 策略 LimitOffset：复用 `build_paged_query` `packages/sz-orm-core/src/cursor_stream.rs:29` 生成五方言分页 SQL → 从 `Pool::acquire` 获取连接执行 → `offset += batch_size` → 归还连接
- [ ] M3-T5.9 策略 ServerCursor：方言支持 → 保持连接（首次 acquire）+ `stream_cursor` `packages/sz-orm-core/src/stream_api.rs:176` 真游标 FETCH batch；方言不支持 → 降级 LimitOffset，标注"server cursor not supported, fallback to limit-offset"
- [ ] M3-T5.10 背压控制：每次拉取前 `backpressure.allow_push()`，false 时返回 `Poll::Pending` 等待消费者处理
- [ ] M3-T5.11 结果为空 → 标记 `StreamState::Done`，返回 `Poll::Ready(None)`
- [ ] M3-T5.12 单元测试：查询返回 100 万行，批次 1000 → StreamResultSet 逐批 yield 1000 行，不一次性加载
- [ ] M3-T5.13 单元测试：策略 Keyset → 使用 `WHERE key > last_key ORDER BY key LIMIT batch`
- [ ] M3-T5.14 单元测试：策略 LimitOffset + `DbType::Oracle` → 复用 `build_paged_query` 生成 ROWNUM 子查询
- [ ] M3-T5.15 单元测试：策略 ServerCursor + PostgreSQL → 使用真游标
- [ ] M3-T5.16 单元测试：策略 ServerCursor + SQLite → 降级 LimitOffset，标注"server cursor not supported"
- [ ] M3-T5.17 边界测试：空 SQL → 返回错误
- [ ] M3-T5.18 边界测试：`batch_size = 0` → 返回错误

**验收标准**：`StreamResultSet` 异步 Stream trait 实现正确；三种分页策略可选；复用既有 `build_paged_query`/`stream_cursor`；背压控制集成；逐批 yield 不一次性加载

**依赖**：M3-T2、M3-T3、M3-T4

## M3-T6：连接池集成 + 游标资源释放

**任务描述**：完善流式结果集与连接池集成（每批从池获取连接，批次完成归还），游标资源释放（消费完成或提前 drop 释放游标 + 归还连接，Drop 语义），不泄漏连接与游标。

**涉及文件**：
- `packages/sz-orm-stream/src/result_set.rs`（扩展：连接池集成 + Drop 语义）

**复用标注**：既有 `Pool::acquire` `packages/sz-orm-core/src/pool.rs:743`；既有 `PooledConnection` Drop `packages/sz-orm-core/src/pool.rs:239`（自动归还连接）；既有 `stream_cursor` Drop `packages/sz-orm-core/src/stream_api.rs:176`（关闭 DB 游标）

**子任务**：
- [ ] M3-T6.1 分页模式连接池集成：每批从 `Pool::acquire` 获取连接，批次完成后 `PooledConnection` Drop 归还（`packages/sz-orm-core/src/pool.rs:239`），不长期占用连接
- [ ] M3-T6.2 真游标模式连接保持：首次 `Pool::acquire` 获取连接保持至 Stream 消费完成，消费完成后归还
- [ ] M3-T6.3 连接获取失败降级：池无可用连接且超时 → 降级为等待重试（可配重试次数），超限后 yield 错误并结束 Stream
- [ ] M3-T6.4 游标资源释放：真游标模式消费完成 → 关闭服务端游标 + 归还连接（复用 `stream_cursor` Drop `packages/sz-orm-core/src/stream_api.rs:176`）
- [ ] M3-T6.5 分页模式无游标资源：分页模式（Keyset/LimitOffset）无服务端游标，仅需归还连接
- [ ] M3-T6.6 Drop 语义：提前 drop `StreamResultSet` → 释放游标 + 归还连接，不泄漏
- [ ] M3-T6.7 实现 `impl Drop for StreamResultSet`：Drop 时释放游标 + 归还连接（真游标模式）
- [ ] M3-T6.8 单元测试：分页模式流式查询 10 批 → 每批从 Pool::acquire 获取连接，批次完成后归还，不长期占用
- [ ] M3-T6.9 单元测试：真游标模式 → 保持连接至 Stream 消费完成
- [ ] M3-T6.10 单元测试：真游标模式消费完成 → 关闭游标 + 归还连接
- [ ] M3-T6.11 单元测试：提前 drop StreamResultSet → 释放游标 + 归还连接（Drop 语义）
- [ ] M3-T6.12 单元测试：连接获取失败 → 降级等待重试，超限 yield 错误结束 Stream
- [ ] M3-T6.13 连接泄漏验证：流式查询完成后，池可用连接数恢复初始值（所有连接已归还）

**验收标准**：连接池集成正确；游标资源释放（Drop 语义）；不泄漏连接与游标；复用既有 `Pool`/`PooledConnection` Drop/`stream_cursor` Drop

**依赖**：M3-T5

## M3-T7：M3 集成测试与门禁验证

**任务描述**：M3 里程碑集成测试与门禁验证，确保 REQ-V45-003 全部验收条件满足，默认 feature 行为不变。

**子任务**：
- [ ] M3-T7.1 集成测试：`StreamResultSet::stream_query` 完整流程（背压检查 → 分页策略 → 连接获取 → 查询 → yield → 连接归还）
- [ ] M3-T7.2 集成测试：三种分页策略（Keyset/LimitOffset/ServerCursor）完整验证
- [ ] M3-T7.3 集成测试：背压控制（生产者快于消费者 → 暂停生产者，不内存溢出）
- [ ] M3-T7.4 集成测试：连接池集成 + 游标资源释放（Drop 语义）
- [ ] M3-T7.5 集成测试：复用既有 `build_paged_query` `packages/sz-orm-core/src/cursor_stream.rs:29` + `stream_cursor` `packages/sz-orm-core/src/stream_api.rs:176` + `BackpressureController` `packages/sz-orm-batch/src/stream.rs:40` + `Pool` `packages/sz-orm-core/src/pool.rs:743`
- [ ] M3-T7.6 运行 `cargo test -p sz-orm-stream --features stream-resultset`（全部通过）
- [ ] M3-T7.7 `cargo clippy -p sz-orm-stream --features stream-resultset -- -D warnings`
- [ ] M3-T7.8 `cargo fmt -p sz-orm-stream -- --check`
- [ ] M3-T7.9 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-stream/` 无占位实现
- [ ] M3-T7.10 扫描 `grep -rn 'unsafe' packages/sz-orm-stream/` 无 unsafe 块
- [ ] M3-T7.11 验证默认 feature 行为与 v4.4.0 一致（`cargo build -p sz-orm-stream` 无 StreamResultSet，行为不变）
- [ ] M3-T7.12 验证 `stream-resultset` 与既有 feature（`batch-stream`）组合编译通过

**验收标准**：M3 集成测试通过；门禁通过；默认行为不变；StreamResultSet + KeysetPaginator + 三种分页策略 + 背压控制 + 连接池集成 + 游标释放全部验证

**依赖**：M3-T1、M3-T2、M3-T3、M3-T4、M3-T5、M3-T6

---

# 六、M4：集成验证与文档同步（P0，0.5 周）

**目标**：M1+M2+M3 全部完成后，进行 workspace 全量集成测试、feature 全组合编译验证、文档同步与版本号更新，确保 v4.5.0 整体交付质量。
**对应需求**：全局（集成验证与文档同步，非功能需求）
**预期工作量**：0.5 周
**依赖**：M0、M1、M2、M3 全部完成

## M4-T1：workspace 全量集成测试

**任务描述**：运行 workspace 全量测试 + 14 道门禁全量验证，确保 v4.5.0 三项需求集成后整体通过，v4.4.0 测试基线不回退。

**涉及文件**：`Cargo.toml`（workspace 全量）、`packages/sz-orm-parallel/`、`packages/sz-orm-batch/`、`packages/sz-orm-stream/`

**子任务**：
- [ ] M4-T1.1 运行 `cargo test --workspace -j 2 --no-fail-fast`（全量测试通过，v4.4.0 基线不回退）
- [ ] M4-T1.2 运行 `cargo test -p sz-orm-parallel --features parallel-query` + `cargo test -p sz-orm-batch --features batch-v2` + `cargo test -p sz-orm-stream --features stream-resultset`（三项需求 feature 测试通过）
- [ ] M4-T1.3 门禁 1：`cargo fmt --all -- --check`（fmt 格式检查）
- [ ] M4-T1.4 门禁 2：`cargo check --workspace --all-targets`（编译检查）
- [ ] M4-T1.5 门禁 3：`cargo clippy --workspace --all-targets -- -D warnings`（clippy 静态分析）
- [ ] M4-T1.6 门禁 4：`cargo test --workspace`（单元/集成测试）
- [ ] M4-T1.7 门禁 5：`cargo doc --workspace --no-deps --all-features`（文档构建）
- [ ] M4-T1.8 门禁 8：扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' --include='*.rs' packages/sz-orm-parallel/ packages/sz-orm-batch/src/executor.rs packages/sz-orm-batch/src/dialect.rs packages/sz-orm-batch/src/delete.rs packages/sz-orm-batch/src/copy.rs packages/sz-orm-stream/` 无占位实现
- [ ] M4-T1.9 门禁 10：`cargo check --workspace --all-targets --all-features`（feature 全组合编译）
- [ ] M4-T1.10 验证 v4.4.0 测试基线不回退（v4.5.0 测试数 ≥ v4.4.0 测试数）

**验收标准**：workspace 全量测试通过；14 道门禁全通过；v4.4.0 测试基线不回退；三项需求 feature 测试通过

**依赖**：M1-T7、M2-T7、M3-T7

## M4-T2：feature 全组合编译验证

**任务描述**：验证 v4.5.0 3 个新 feature 与既有 feature（v4.3.0 7 个 + v4.4.0 6 个 + sz-orm-batch `batch-stream`）任意组合编译通过，确保无 feature 冲突。

**涉及文件**：`packages/sz-orm-parallel/Cargo.toml`、`packages/sz-orm-batch/Cargo.toml`、`packages/sz-orm-stream/Cargo.toml`

**子任务**：
- [ ] M4-T2.1 验证默认（无 feature）编译通过，行为与 v4.4.0 一致：`cargo build --workspace`
- [ ] M4-T2.2 验证单 feature 编译：`cargo build -p sz-orm-parallel --features parallel-query` / `cargo build -p sz-orm-batch --features batch-v2` / `cargo build -p sz-orm-stream --features stream-resultset`
- [ ] M4-T2.3 验证 v4.5.0 三 feature 组合编译：`cargo build --features sz-orm-parallel/parallel-query,sz-orm-batch/batch-v2,sz-orm-stream/stream-resultset`
- [ ] M4-T2.4 验证 v4.5.0 + v4.4.0 feature 组合编译：`cargo build --features sz-orm-parallel/parallel-query,sz-orm-advisor/query-advisor,sz-orm-diagnosis/slow-query-diagnosis,...`
- [ ] M4-T2.5 验证 v4.5.0 + v4.3.0 feature 组合编译：`cargo build --features sz-orm-parallel/parallel-query,sz-orm-explain/explain-analyzer,...`
- [ ] M4-T2.6 验证 `batch-v2` + 既有 `batch-stream` 组合编译：`cargo build -p sz-orm-batch --features batch-stream,batch-v2`
- [ ] M4-T2.7 验证全 feature 组合编译：`cargo build --workspace --all-features`

**验收标准**：所有 feature 组合编译通过；无 feature 冲突；默认行为与 v4.4.0 一致

**依赖**：M4-T1

## M4-T3：文档同步与版本号更新

**任务描述**：更新 v4.5.0 相关文档（AGENTS.md 工作空间成员数 + feature gate 数 + 版本号）、运行文档一致性门禁，确保文档与代码同步。

**涉及文件**：`AGENTS.md`（工作空间成员 58 → 60，版本 4.4.0 → 4.5.0，新增 3 feature）、`docs/spec/v4.5.0/tasks.md`（本文件，标记任务完成）

**子任务**：
- [ ] M4-T3.1 更新 `AGENTS.md` 工作空间成员数 58 → 60（新增 sz-orm-parallel + sz-orm-stream）
- [ ] M4-T3.2 更新 `AGENTS.md` 版本号 4.4.0 → 4.5.0，新增 3 feature（parallel-query/batch-v2/stream-resultset）
- [ ] M4-T3.3 更新 `AGENTS.md` 模块路径新增 `sz-orm-parallel`/`sz-orm-stream` 包说明
- [ ] M4-T3.4 运行 `python scripts/check-doc-consistency.py`（门禁 12，文档与代码一致）
- [ ] M4-T3.5 运行 `python scripts/check-doc-sync.py --diff HEAD`（门禁 14，文档与 HEAD 同步）
- [ ] M4-T3.6 确认 `docs/spec/v4.5.0/tasks.md` 27 任务 / 148 子任务全部标记 `[x]`

**验收标准**：AGENTS.md 更新（成员 60 + 版本 4.5.0 + 3 feature）；文档一致性门禁通过；tasks.md 全部完成

**依赖**：M4-T2

---

# 七、任务依赖关系图

```
M0（P0，文档基线，立即）
  ├─ M0-T1（v4.4.0 基线锁定）
  ├─ M0-T2（v4.5.0 环境准备，新增 2 包 + 3 feature + 版本号）
  └─ M0-T3（基线验证）

M1（P1，并行查询执行器，独立新包，M0-T2 后启动）
  ├─ M1-T1（包搭建 + feature gate）← M0-T2
  ├─ M1-T2（配置与枚举）← M1-T1
  ├─ M1-T3（ParallelQueryScheduler 调度器）← M1-T1, M1-T2
  ├─ M1-T4（ResultMerger 合并器）← M1-T2
  ├─ M1-T5（失败降级处理）← M1-T3
  ├─ M1-T6（自适应协同 + 超时释放）← M1-T3, M1-T5
  └─ M1-T7（集成测试与门禁）← M1-T1~M1-T6

M2（P1，批量优化，独立扩展，M0-T2 后启动）
  ├─ M2-T1（batch-v2 feature + UpsertMode 五方言）← M0-T2
  ├─ M2-T2（BatchDialect 方言抽象）← M2-T1
  ├─ M2-T3（batch_delete + BatchDeleteRequest）← M2-T1, M2-T2
  ├─ M2-T4（BatchExecutor 异步执行器）← M2-T1, M2-T2, M2-T3
  ├─ M2-T5（事务边界与回滚）← M2-T4
  ├─ M2-T6（PostgreSQL COPY 协议）← M2-T4
  └─ M2-T7（集成测试与门禁）← M2-T1~M2-T6

M3（P1，流式结果集，独立新包，M0-T2 后启动）
  ├─ M3-T1（包搭建 + feature gate）← M0-T2
  ├─ M3-T2（配置与枚举）← M3-T1
  ├─ M3-T3（KeysetPaginator keyset 分页）← M3-T2
  ├─ M3-T4（AsyncBackpressureController 异步背压）← M3-T1
  ├─ M3-T5（StreamResultSet + Stream trait）← M3-T2, M3-T3, M3-T4
  ├─ M3-T6（连接池集成 + 游标释放）← M3-T5
  └─ M3-T7（集成测试与门禁）← M3-T1~M3-T6

M4（P0，集成验证与文档同步，M1+M2+M3 全部完成后）
  ├─ M4-T1（workspace 全量集成测试）← M1-T7, M2-T7, M3-T7
  ├─ M4-T2（feature 全组合编译验证）← M4-T1
  └─ M4-T3（文档同步与版本号更新）← M4-T2
```

> **并行开发说明**：M1/M2/M3 三项需求主体相互独立（design.md §7.2），可并行开发。M0 完成后，M1/M2/M3 同时启动；M1/M2/M3 全部完成后，M4 启动。不存在如 v4.4.0 M2 依赖 M1 的协同关系。

---

# 八、风险与缓解措施

| 风险 ID | 风险描述 | 影响 | 概率 | 缓解措施 | 责任任务 |
|---------|---------|------|------|---------|---------|
| R-001 | 并行查询耗尽连接池（并发度过高） | 高 | 中 | 并发度默认池 max_size 80% 预留连接，可配，连接获取失败降级串行执行，整体超时控制 | M1-T3 |
| R-002 | 并行查询单查询失败导致整体 panic | 高 | 低 | 单查询失败按 `FailureStrategy` 降级（Skip/Abort/Fallback），不 panic，默认 Skip | M1-T5 |
| R-003 | 并行查询超时后连接泄漏 | 高 | 低 | 超时取消未完成查询释放连接（tokio::time::timeout + Drop 归还），`PooledConnection` Drop 自动归还 | M1-T6 |
| R-004 | 结果合并失败（Join 键不匹配/Map 转换失败） | 中 | 中 | 合并失败降级返回未合并原始结果列表，标注"merge failed, returning raw results" | M1-T4 |
| R-005 | 批量 DELETE 误删全表（空条件/无条件删除） | 高 | 低 | `BatchDeleteRequest::ids` 非空校验（空则拒绝），`primary_key` 非空校验，禁止无条件删除 | M2-T3 |
| R-006 | 批量操作部分失败数据不一致 | 高 | 中 | 事务边界 + `RollbackStrategy`（None/Savepoint/PerChunk），Savepoint 每分片前 SAVEPOINT 失败回滚，PerChunk 任一失败整批中止 | M2-T5 |
| R-007 | 批量操作事务死锁 | 中 | 中 | 复用既有 `retry_on_deadlock` `packages/sz-orm-core/src/transaction.rs:466` 死锁检测 + 指数退避重试，重试超限返回错误 | M2-T5 |
| R-008 | PostgreSQL COPY 协议失败 | 中 | 低 | COPY 失败降级为多值 INSERT 重试，标注"COPY failed, fallback to multi-value INSERT" | M2-T6 |
| R-009 | 五方言批量 SQL 语法差异导致执行失败 | 中 | 中 | `BatchDialect` 方言抽象按 `DbType` 适配，不支持方言降级通用多值 INSERT，标注"dialect fallback" | M2-T2 |
| R-010 | 流式结果集内存积压（生产者快于消费者） | 中 | 中 | `AsyncBackpressureController` 背压控制，阈值可配（默认 10000），超阈值暂停生产者拉取 | M3-T4 |
| R-011 | 流式结果集连接泄漏（提前 drop 未归还连接） | 高 | 低 | `StreamResultSet` Drop 语义释放游标 + 归还连接，`PooledConnection` Drop 自动归还，真游标模式 Drop 关闭游标 | M3-T6 |
| R-012 | keyset pagination 排序列无索引性能退化 | 中 | 中 | 正常执行（性能退化由数据库处理），结果标注"keyset column has no index, performance may degrade" | M3-T3 |
| R-013 | 真游标方言不支持 | 低 | 中 | ServerCursor 不支持方言（SQLite/MSSQL）降级 LimitOffset，标注"server cursor not supported, fallback to limit-offset" | M3-T5 |
| R-014 | 新增 feature 与既有 feature 组合编译失败 | 高 | 低 | 门禁 10 全组合编译 + feature 依赖关系验证（M4-T2） | M4-T2 |
| R-015 | sz-pay 既有代码因 API 变更破坏 | 高 | 低 | 无 Breaking Change，3 个 feature gate 隔离默认关闭，既有公开 API 完全向后兼容，sz-pay 回归测试 | M4-T1 |
| R-016 | 批量操作 SQL 注入（列名/表名拼接） | 高 | 低 | 复用既有 `DefaultBatchOps::quote` `packages/sz-orm-batch/src/lib.rs:177` 反引号转义 + `Connection::execute_with_params` `packages/sz-orm-core/src/pool.rs:82` 参数绑定，禁止 SQL 字符串拼接 | M2-T2 |
| R-017 | 并行查询参数交叉污染 | 中 | 低 | 各查询参数独立绑定（`ParallelQuery.params` 独立 Vec），不共享参数缓冲区 | M1-T3 |

---

# 九、验收标准总览

## 9.1 REQ-V45-001 并行查询执行器（P1，M1）

1. `ParallelQueryScheduler` 将多个独立查询并行执行，并发度控制避免连接池耗尽（默认池 max_size 80%）— M1-T3
2. 复用既有 `Pool` `packages/sz-orm-core/src/pool.rs:743` + `Connection` trait `packages/sz-orm-core/src/pool.rs:45` + tokio 异步运行时，不新建连接池 — M1-T1/T3
3. 与既有 `AdaptiveExecutor` `packages/sz-orm-adaptive/src/executor.rs:120` 协同，单查询仍走自适应路径，不修改既有自适应决策 — M1-T6
4. `ResultMerger` 支持四种合并策略（First/Union/Join/Map）— M1-T4
5. 整体超时与单查询超时控制，超时取消未完成查询释放连接 — M1-T3/T6
6. 单查询失败降级（Skip/Abort/Fallback），不 panic — M1-T5
7. `parallel-query` feature gate 隔离，默认关闭 — M1-T1/T7

## 9.2 REQ-V45-002 批量 INSERT/UPDATE/DELETE 优化（P1，M2）

1. `batch_delete` 通过 IN 子句批量删除，复用既有分片逻辑，参数化绑定 — M2-T3
2. `BatchExecutor` 异步执行器通过 `execute_with_params` `packages/sz-orm-core/src/pool.rs:82` 真正执行批量 SQL — M2-T4
3. 五方言批量 SQL 生成（MySQL/PostgreSQL/SQLite/Oracle/MSSQL），复用 `DbType` `packages/sz-orm-core/src/db_type.rs:11` — M2-T2
4. PostgreSQL COPY 协议（可选），其他方言降级为多值 INSERT — M2-T6
5. 事务边界 + 部分失败回滚（复用 `Transaction` `packages/sz-orm-core/src/transaction.rs:159` + `RollbackStrategy` `packages/sz-orm-batch/src/lib.rs:491`）— M2-T5
6. 复用既有分片与进度回调（`chunk_indices` `packages/sz-orm-batch/src/lib.rs:164` + `ProgressCallback` `packages/sz-orm-batch/src/lib.rs:482`）— M2-T4
7. 批量 DELETE 范围保护（拒绝空行/无条件删除）— M2-T3
8. `batch-v2` feature gate 隔离，默认关闭，既有 `BatchOperations` trait `packages/sz-orm-batch/src/lib.rs:43` 保留 — M2-T1/T7

## 9.3 REQ-V45-003 异步流式结果集（P1，M3）

1. `StreamResultSet` 实现异步 Stream trait 逐批 yield，避免一次性加载全量 — M3-T5
2. `KeysetPaginator` keyset pagination（`WHERE key > last_key`，深翻页高效）— M3-T3
3. 三种分页策略可选（Keyset/LimitOffset/ServerCursor），默认 LimitOffset — M3-T5
4. 背压控制与异步 Stream 集成（复用 `BackpressureController` `packages/sz-orm-batch/src/stream.rs:40`，暂停生产者避免积压）— M3-T4
5. 可配置批次大小控制内存（默认 1000，复用 `StreamConfig.batch_size` `packages/sz-orm-batch/src/stream.rs:11`）— M3-T2
6. 连接池集成（每批从池获取连接，批次完成归还，复用 `Pool` `packages/sz-orm-core/src/pool.rs:743`）— M3-T6
7. 复用既有 `build_paged_query` `packages/sz-orm-core/src/cursor_stream.rs:29` 五方言分页 SQL 包装 — M3-T5
8. 游标资源释放（消费完成或提前 drop 释放游标 + 归还连接）— M3-T6
9. `stream-resultset` feature gate 隔离，默认关闭 — M3-T1/T7

## 9.4 全局验收

1. 无 Breaking Change，3 feature gate 隔离默认全关闭，既有公开 API 完全向后兼容 — M1-T7/M2-T7/M3-T7
2. v4.4.0 测试基线不回退（约 7,000+ 个测试仅增不减）— M4-T1
3. 14 道门禁全通过 — M4-T1
4. feature 全组合编译通过 — M4-T2
5. 五方言覆盖（MySQL/PostgreSQL/SQLite/Oracle/MSSQL）— M2-T2/M3-T5
6. 禁止占位实现（todo!/unimplemented!/unreachable!）— M1-T7/M2-T7/M3-T7
7. unsafe 零容忍 — M1-T7/M2-T7/M3-T7
8. 参数化查询强制（复用 `DefaultBatchOps::quote` + `Connection::execute_with_params`）— M2-T2/T3
9. 审计证据（每项结论附 file:line 证据）— 全任务
10. 与 v4.4.0 零重叠（执行优化层 vs 分析/建议/转正层）— 全任务

---

# 十、feature gate 总览

| feature gate | 所属包 | 控制能力 | 默认 | 对应需求 | 测试命令 |
|-------------|--------|---------|------|---------|---------|
| `parallel-query` | sz-orm-parallel（新包）+ sz-orm-core + sz-orm-adaptive（只读复用） | 并行查询执行器（调度器 + 合并器 + 超时降级） | 关闭 | REQ-V45-001 | `cargo test -p sz-orm-parallel --features parallel-query` |
| `batch-v2` | sz-orm-batch（扩展）+ sz-orm-core（只读复用 Connection/Transaction/DbType） | 批量 INSERT/UPDATE/DELETE 优化（DELETE + 异步执行 + 五方言 + 事务 + COPY） | 关闭 | REQ-V45-002 | `cargo test -p sz-orm-batch --features batch-v2` |
| `stream-resultset` | sz-orm-stream（新包）+ sz-orm-core（只读复用 cursor_stream/stream_api/paginator/Pool）+ sz-orm-batch（只读复用 BackpressureController/StreamConfig） | 异步流式结果集（keyset + 背压 Stream + 内存控制 + 连接池集成） | 关闭 | REQ-V45-003 | `cargo test -p sz-orm-stream --features stream-resultset` |

**feature 组合兼容性**：

| feature 组合 | 编译预期 | 验证命令 |
|-------------|---------|---------|
| 默认（无 feature） | 编译通过，行为与 v4.4.0 一致 | `cargo build --workspace` |
| 单 feature：parallel-query | 编译通过 | `cargo build -p sz-orm-parallel --features parallel-query` |
| 单 feature：batch-v2 | 编译通过 | `cargo build -p sz-orm-batch --features batch-v2` |
| 单 feature：stream-resultset | 编译通过 | `cargo build -p sz-orm-stream --features stream-resultset` |
| v4.5.0 三 feature 组合 | 编译通过 | `cargo build --features sz-orm-parallel/parallel-query,sz-orm-batch/batch-v2,sz-orm-stream/stream-resultset` |
| v4.5.0 + v4.4.0 feature 组合 | 编译通过 | `cargo build --features sz-orm-parallel/parallel-query,sz-orm-advisor/query-advisor,...` |
| v4.5.0 + v4.3.0 feature 组合 | 编译通过 | `cargo build --features sz-orm-parallel/parallel-query,sz-orm-explain/explain-analyzer,...` |
| 既有 batch-stream + batch-v2 | 编译通过（两 feature 独立） | `cargo build -p sz-orm-batch --features batch-stream,batch-v2` |
| 全 feature 组合 | 编译通过 | `cargo build --workspace --all-features` |

---

# 十一、复用点清单

## 11.1 REQ-V45-001 并行查询执行器复用点（10 项）

| 复用项 | 复用位置 | 用途 | 证据验证 |
|--------|---------|------|---------|
| `Pool` | `packages/sz-orm-core/src/pool.rs:743` | 并行查询从既有连接池获取连接，不新建连接池 | ✅ 已验证（pub struct Pool，AtomicU32 + ArrayQueue + Notify） |
| `Connection` trait | `packages/sz-orm-core/src/pool.rs:45` | 并行查询各查询通过既有 Connection trait 执行 | ✅ 已验证（pub trait Connection: Send + Sync，异步 trait） |
| `Connection::execute_with_params` | `packages/sz-orm-core/src/pool.rs:82` | 参数绑定执行，防 SQL 注入 | ✅ 已验证（默认实现返回 NotImplemented，支持适配器覆盖） |
| `PooledConnection` | `packages/sz-orm-core/src/pool.rs:239` | Drop 自动归还连接池，避免连接泄漏 | ✅ 已验证（pub struct PooledConnection，Drop 归还） |
| `AdaptiveExecutor` | `packages/sz-orm-adaptive/src/executor.rs:120` | 单查询自适应路径，并行调度器不修改既有自适应决策 | ✅ 已验证（pub struct AdaptiveExecutor，按 query_key 独立统计） |
| `ExecutionPath` | `packages/sz-orm-adaptive/src/executor.rs:16` | 单查询执行路径（Normal/Paginated/Cached） | ✅ 已验证（pub enum ExecutionPath） |
| `QueryOutcome` | `packages/sz-orm-adaptive/src/executor.rs:106` | 单查询结果（value/rows/elapsed_ms/from_cache/slow） | ✅ 已验证（pub struct QueryOutcome<T>） |
| `AdaptiveExecutor::decide` | `packages/sz-orm-adaptive/src/executor.rs:157` | 自适应决策（按统计选择执行路径） | ✅ 已验证（pub fn decide） |
| `QueryStats` | `packages/sz-orm-adaptive/src/stats.rs:11` | 运行时统计（AtomicU64 无锁） | ✅ 已验证（pub struct QueryStats） |
| tokio 异步运行时 | `Cargo.toml:31`（workspace 依赖） | tokio::join! 并行执行 + tokio::time::timeout 超时控制 | ✅ 已验证（tokio = { version = "1.40", features = ["full"] }） |

## 11.2 REQ-V45-002 批量 INSERT/UPDATE/DELETE 优化复用点（19 项）

| 复用项 | 复用位置 | 用途 | 证据验证 |
|--------|---------|------|---------|
| `BatchOperations` trait | `packages/sz-orm-batch/src/lib.rs:43` | 既有批量操作 trait（batch_insert/update/upsert），保留不动 | ✅ 已验证（pub trait BatchOperations: Send + Sync） |
| `DefaultBatchOps` | `packages/sz-orm-batch/src/lib.rs:83` | 既有默认批量实现，复用 SQL 生成逻辑 | ✅ 已验证（pub struct DefaultBatchOps） |
| `DefaultBatchOps.primary_key` | `packages/sz-orm-batch/src/lib.rs:84` | 主键列名（默认 "id"），batch_delete 复用 | ✅ 已验证（pub primary_key: String） |
| `DefaultBatchOps.chunk_size` | `packages/sz-orm-batch/src/lib.rs:93` | 分片大小（默认 1000），batch_delete 复用 | ✅ 已验证（pub chunk_size: usize） |
| `DEFAULT_CHUNK_SIZE` | `packages/sz-orm-batch/src/lib.rs:119` | 默认分片大小常量 1000 | ✅ 已验证（pub const DEFAULT_CHUNK_SIZE: usize = 1000） |
| `DefaultBatchOps::chunk_indices` | `packages/sz-orm-batch/src/lib.rs:164` | 分片迭代器，batch_delete 复用分片逻辑 | ✅ 已验证（fn chunk_indices 返回 (start, end) 迭代器） |
| `DefaultBatchOps::quote` | `packages/sz-orm-batch/src/lib.rs:177` | 反引号转义防 SQL 注入，batch_delete 复用 | ✅ 已验证（fn quote，反引号转义为双反引号） |
| `BatchResult` | `packages/sz-orm-batch/src/lib.rs:19` | 批量结果（inserted/updated/failed/generated_sqls），异步执行器复用 | ✅ 已验证（pub struct BatchResult） |
| `UpsertMode` | `packages/sz-orm-batch/src/lib.rs:50` | 既有两方言 UPSERT 模式，扩展为五方言 | ✅ 已验证（pub enum UpsertMode，MysqlOnDuplicate/PostgresOnConflict） |
| `BatchProgress` | `packages/sz-orm-batch/src/lib.rs:451` | 批量进度，异步执行器复用进度回调 | ✅ 已验证（pub struct BatchProgress） |
| `ProgressCallback` | `packages/sz-orm-batch/src/lib.rs:482` | 进度回调类型，异步执行器复用 | ✅ 已验证（pub type ProgressCallback = Arc<dyn Fn(BatchProgress) + Send + Sync>） |
| `RollbackStrategy` | `packages/sz-orm-batch/src/lib.rs:491` | 回滚策略（None/Savepoint/PerChunk），异步执行器复用 | ✅ 已验证（pub enum RollbackStrategy） |
| `ConflictTarget` | `packages/sz-orm-batch/src/lib.rs:503` | UPSERT 冲突目标，五方言扩展复用 | ✅ 已验证（pub enum ConflictTarget） |
| `Connection::execute_with_params` | `packages/sz-orm-core/src/pool.rs:82` | 异步执行器通过参数绑定执行批量 SQL | ✅ 已验证（参数绑定执行，防 SQL 注入） |
| `Transaction` | `packages/sz-orm-core/src/transaction.rs:159` | 事务边界，异步执行器复用 | ✅ 已验证（pub struct Transaction，conn + state + options + savepoint_counter） |
| `TransactionManager` | `packages/sz-orm-core/src/transaction.rs:527` | 事务管理器，按名称管理多个事务 | ✅ 已验证（pub struct TransactionManager） |
| `retry_on_deadlock` | `packages/sz-orm-core/src/transaction.rs:466` | 死锁重试，批量操作事务死锁复用 | ✅ 已验证（pub async fn retry_on_deadlock，死锁检测 + 指数退避） |
| `IsolationLevel` | `packages/sz-orm-core/src/transaction.rs:16` | 事务隔离级别 | ✅ 已验证（pub enum IsolationLevel） |
| `DbType` | `packages/sz-orm-core/src/db_type.rs:11` | 数据库方言枚举，五方言适配复用 | ✅ 已验证（pub enum DbType，#[non_exhaustive]） |

## 11.3 REQ-V45-003 异步流式结果集复用点（13 项）

| 复用项 | 复用位置 | 用途 | 证据验证 |
|--------|---------|------|---------|
| `build_paged_query` | `packages/sz-orm-core/src/cursor_stream.rs:29` | 五方言分页 SQL 包装，LimitOffset 策略复用 | ✅ 已验证（pub fn build_paged_query，Oracle ROWNUM/SQL Server OFFSET-FETCH/MySQL-PG-SQLite LIMIT-OFFSET） |
| `stream_cursor_paged` | `packages/sz-orm-core/src/cursor_stream.rs:79` | 分页游标 Stream，StreamResultSet 复用语义 | ✅ 已验证（pub fn stream_cursor_paged，返回 Pin<Box<dyn Stream>>，借用 conn） |
| `stream_cursor` | `packages/sz-orm-core/src/stream_api.rs:176` | 真游标 Stream，ServerCursor 策略复用 | ✅ 已验证（pub fn stream_cursor，委托 conn.query_stream_cursor） |
| `StreamApiExt` | `packages/sz-orm-core/src/stream_api.rs:50` | 流式 API 扩展 trait | ✅ 已验证（pub trait StreamApiExt<M: Model>） |
| `Paginator` | `packages/sz-orm-core/src/paginator.rs:158` | 既有分页器，LimitOffset 复用 | ✅ 已验证（pub struct Paginator<'a, C>，fetch_page） |
| `BackpressureController` | `packages/sz-orm-batch/src/stream.rs:40` | 既有背压控制器，异步背压复用语义 | ✅ 已验证（pub struct BackpressureController，allow_push/push/pop/pending） |
| `StreamConfig` | `packages/sz-orm-batch/src/stream.rs:9` | 既有流式配置，StreamResultSetConfig 复用默认值 | ✅ 已验证（pub struct StreamConfig，batch_size/max_concurrency/backpressure_threshold） |
| `StreamConfig.batch_size` | `packages/sz-orm-batch/src/stream.rs:11` | 批次大小默认 1000 | ✅ 已验证（pub batch_size: usize） |
| `StreamConfig.backpressure_threshold` | `packages/sz-orm-batch/src/stream.rs:13` | 背压阈值默认 10000 | ✅ 已验证（pub backpressure_threshold: usize） |
| `StreamBatch` | `packages/sz-orm-batch/src/stream.rs:30` | 流式批次结构 | ✅ 已验证（pub struct StreamBatch<T>，batch_index/records/is_last） |
| `Pool` | `packages/sz-orm-core/src/pool.rs:743` | 连接池集成，每批从池获取连接 | ✅ 已验证（同 REQ-V45-001） |
| `PooledConnection` | `packages/sz-orm-core/src/pool.rs:239` | Drop 自动归还连接池，游标资源释放复用 | ✅ 已验证（同 REQ-V45-001） |
| `DbType` | `packages/sz-orm-core/src/db_type.rs:11` | 数据库方言，分页策略方言适配复用 | ✅ 已验证（同 REQ-V45-002） |

## 11.4 复用统计

| 需求 | 复用点数 | 新增点数 | 复用率 |
|------|---------|---------|--------|
| REQ-V45-001 并行查询 | 10 | 6（ParallelQueryScheduler/ResultMerger/ParallelQueryConfig/ParallelQueryOutcome/MergeStrategy/FailureStrategy） | 62.5% |
| REQ-V45-002 批量优化 | 19 | 5（BatchExecutor/BatchExecutorConfig/BatchDialect/BatchDeleteRequest/CopyProtocolExecutor + UpsertMode 扩展 3 变体） | 79.2% |
| REQ-V45-003 流式结果集 | 13 | 6（StreamResultSet/KeysetPaginator/StreamResultSetConfig/PaginationStrategy/OrderDirection/AsyncBackpressureController） | 68.4% |
| **合计** | **42** | **17** | **71.2%** |

> **复用率说明**：v4.5.0 整体复用率 71.2%，优先复用既有能力（连接池/Connection trait/AdaptiveExecutor/DefaultBatchOps/Transaction/build_paged_query/BackpressureController 等），不重复实现，符合 spec.md §1.4 复用优先约束。

---

# 十二、与 v4.4.0 的关系

## 12.1 零重叠声明

v4.5.0 与 v4.4.0 零重叠：

| v4.4.0 能力（分析/建议/转正层） | v4.5.0 能力（执行优化层） | 关系 |
|-------------------------------|-------------------------|------|
| 查询自动优化建议（`sz-orm-advisor`） | 并行查询执行器（`sz-orm-parallel`） | v4.5.0 复用 v4.4.0 优化建议可选联动（并行查询结果可触发建议生成），不重复实现建议 |
| 慢查询自动诊断（`sz-orm-diagnosis`） | 并行查询执行器 / 批量操作 / 流式结果集 | v4.5.0 执行优化可被 v4.4.0 诊断观测（慢查询诊断可标注并行/批量/流式执行），不重复实现诊断 |
| db-fusion 转正（`sz-orm-fusion`） | 并行查询执行器 | v4.5.0 并行查询可并行执行融合查询，不修改既有融合逻辑 |
| 结构化查询日志（`sz-orm-observability`） | 并行查询 / 批量操作 / 流式结果集 | v4.5.0 执行优化可被 v4.4.0 日志观测，不重复实现日志 |
| 性能回归基准线（`sz-orm-explain`） | 并行查询 / 批量操作 / 流式结果集 | v4.5.0 执行优化性能可被 v4.4.0 基线比对，不重复实现基线 |
| 查询智能闭环联动（`sz-orm-advisor`） | 并行查询执行器 | v4.5.0 并行查询可接入闭环（可选），不修改既有闭环 |

## 12.2 依赖关系

```
v4.4.0 已验收基线（6 个 feature gate）
  │
  ├─ adaptive-query ───→ REQ-V45-001 并行查询（复用 AdaptiveExecutor/ExecutionPath/QueryOutcome）
  │
  └─ (其他 v4.4.0 feature) ──→ 无 v4.5.0 强依赖（v4.5.0 三项需求主体独立）

v4.5.0 三项需求相互独立，可并行开发：
  ├─ REQ-V45-001 并行查询（新包 sz-orm-parallel，复用 sz-orm-core Pool/Connection + sz-orm-adaptive）
  ├─ REQ-V45-002 批量优化（扩展 sz-orm-batch，复用 sz-orm-core Connection/Transaction/DbType）
  └─ REQ-V45-003 流式结果集（新包 sz-orm-stream，复用 sz-orm-core cursor_stream/stream_api/paginator/Pool + sz-orm-batch BackpressureController）
```

## 12.3 新增包

| 包名 | 对应需求 | 依赖 | 说明 |
|------|---------|------|------|
| `sz-orm-parallel` | REQ-V45-001 | sz-orm-core（Pool/Connection 只读复用）+ sz-orm-adaptive（AdaptiveExecutor 只读复用）+ tokio + futures | 并行查询执行器（调度器 + 合并器 + 超时降级） |
| `sz-orm-stream` | REQ-V45-003 | sz-orm-core（cursor_stream/stream_api/paginator/Pool 只读复用）+ sz-orm-batch（BackpressureController/StreamConfig 只读复用）+ tokio + futures | 异步流式结果集（keyset + 背压 Stream + 内存控制） |

## 12.4 扩展包

| 包名 | 对应需求 | 扩展内容 |
|------|---------|---------|
| `sz-orm-batch` | REQ-V45-002 | 批量 DELETE + 异步批量执行器 + 五方言批量 SQL + 事务边界 + PostgreSQL COPY 协议（`batch-v2` feature） |

## 12.5 版本号变更

| 项目 | v4.4.0 | v4.5.0 | 变更类型 |
|------|--------|--------|---------|
| workspace.package.version | 4.4.0 | 4.5.0 | minor 版本号升级 |
| workspace 成员数 | 58 | 60 | 新增 2 包（sz-orm-parallel + sz-orm-stream） |
| feature gate 数 | v4.4.0 6 个 + 既有 | v4.5.0 3 个 + v4.4.0 6 个 + 既有 | 新增 3 feature（parallel-query/batch-v2/stream-resultset） |
| sz-orm-batch feature | batch-stream | batch-stream + batch-v2 | 扩展 1 feature |

---

> 文档生成依据：`docs/spec/v4.5.0/spec.md`（需求规格，688 行）+ `docs/spec/v4.5.0/design.md`（技术设计，1676 行）+ `docs/spec/v4.4.0/tasks.md`（v4.4.0 任务规划，184 子任务已完成）+ 2026-08-12 逐项代码验证（所有 file:line 证据均已实测存在）
> 审计合规：本文档所有 file:line 证据均引用真实存在的代码，遵循 AGENTS.md 审计合规铁律
> 任务约束：27 任务 / 148 子任务，每项任务附输入/输出/验收标准/复用点（file:line 证据），任务粒度 1-4 小时可完成，依赖关系清晰，里程碑划分合理（M0 文档基线 → M1~M3 三项需求并行 → M4 集成验证）
> 下一阶段：编码实施（按 tasks.md 任务顺序执行，M0 → M1/M2/M3 并行 → M4）