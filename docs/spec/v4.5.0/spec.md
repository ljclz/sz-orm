# sz-orm v4.5.0 需求规格说明书

> 版本：v4.5.0（并行查询执行器 + 批量 INSERT/UPDATE/DELETE 优化 + 异步流式结果集）
> 基线：v4.4.0（查询自动优化建议 + 慢查询自动诊断报告 + db-fusion 转正 + 结构化查询日志 + 性能回归基准线 + 查询智能闭环联动，6 项需求 REQ-V44-001~006 全部通过 feature gate 隔离，已验收基线）
> 日期：2026-08-12
> 需求格式：EARS（Ubiquitous / Event-driven / State-driven / Optional / Unwanted）
> 文档定位：需求规格（What to build），不含技术设计（How to build）
> 优先级声明：三项需求均为 P1（查询并行化与批量优化是数据访问层性能的核心提升，三者相互独立可并行推进），按"REQ-V45-001 并行查询执行器（复杂查询场景延迟优化）→ REQ-V45-002 批量写入优化（网络往返优化）→ REQ-V45-003 异步流式结果集（大结果集内存优化）"序推进，三者无强依赖可并行开发
> 需求编号约定：REQ-V45-xxx（v4.5.0 需求项，REQ-V45-001 ~ REQ-V45-003）
> 规划依据：`docs/spec/v4.4.0/` SDD 三阶段文档（spec 870 行 / design 1801 行 / tasks 184 子任务，v4.4.0 已全部完成）+ 2026-08-12 逐项代码验证（file:line 均已实测存在）
> 兼容性铁律：所有新能力通过 feature gate 隔离，默认 feature 行为不变，既有公开 API 完全向后兼容，v4.4.0 已验收测试基线不回退；sz-pay 生产依赖（从 crates.io 拉取 sz-orm-* 6 个包）不得被破坏；五方言覆盖：MySQL/PostgreSQL/SQLite/Oracle/MSSQL
> 范围声明：本版本聚焦查询执行层性能优化（并行化 + 批量化 + 流式化）；更长期（v4.x+ 查询结果缓存增强/分布式查询执行/列式存储集成）在后续版本规划；本版本不涉及 crates.io 发布流程变更
> 边界声明：与 v4.4.0 零重叠（见第 1.4 节），v4.4.0 是"分析/建议/转正"层（优化建议/诊断报告/融合转正/结构化日志/性能基线/闭环联动），v4.5.0 是"执行优化"层（并行执行/批量执行/流式执行）

---

# 1. 组件定位

## 1.1 核心职责

本组件负责交付 sz-orm v4.5.0 的三项查询执行层性能优化能力：(1) 并行查询执行器（新增 `sz-orm-parallel` 包，复用既有连接池 `Pool` `packages/sz-orm-core/src/pool.rs:743` + `Connection` trait `packages/sz-orm-core/src/pool.rs:45` + 自适应执行器 `AdaptiveExecutor` `packages/sz-orm-adaptive/src/executor.rs:120` + tokio 异步运行时，提供并行查询调度器控制并发度避免连接池耗尽 + 查询结果合并器支持多种合并策略 + 并行查询超时控制与单查询失败降级，与既有 AdaptiveExecutor 协同）；(2) 批量 INSERT/UPDATE/DELETE 优化（扩展既有 `sz-orm-batch` 包 `packages/sz-orm-batch/src/lib.rs:43` `BatchOperations` trait + `:83` `DefaultBatchOps`，补齐批量 DELETE + 异步批量执行器 + 五方言批量 SQL 生成 + 事务边界与部分失败回滚 + PostgreSQL COPY 协议，复用既有 `Connection::execute_with_params` `packages/sz-orm-core/src/pool.rs:82` + `Transaction` `packages/sz-orm-core/src/transaction.rs:159`）；(3) 异步流式结果集（新增 `sz-orm-stream` 包，复用既有分页游标 `build_paged_query` `packages/sz-orm-core/src/cursor_stream.rs:29` + 真游标 `stream_cursor` `packages/sz-orm-core/src/stream_api.rs:176` + 分页器 `Paginator` `packages/sz-orm-core/src/paginator.rs:158` + 背压控制器 `BackpressureController` `packages/sz-orm-batch/src/stream.rs:40` + 连接池 `Pool`，提供 keyset pagination + 异步 Stream 背压集成 + 可配置批次大小内存控制 + 连接池集成）。所有能力通过 feature gate 隔离，不破坏现有 API 兼容性与 v4.4.0 已验收基线。

## 1.2 核心输入

1. **v4.4.0 已验收基线**：查询自动优化建议 + 慢查询自动诊断报告 + db-fusion 转正 + 结构化查询日志 + 性能回归基准线 + 查询智能闭环联动，6 项能力全部通过 feature gate 隔离，作为本版本基准。
2. **现有能力清单与缺口证据**：
   - **并行查询执行**：`packages/sz-orm-core/src/pool.rs:743` `pub struct Pool`（自研连接池，AtomicU32 + crossbeam-queue ArrayQueue + Notify，无锁 MPMC）、`:45` `pub trait Connection`（execute/query/begin_transaction/commit/rollback/execute_with_params/query_with_params，异步 trait）、`packages/sz-orm-adaptive/src/executor.rs:120` `pub struct AdaptiveExecutor`（自适应执行器，按 query_key 独立统计，线程安全）、`:16` `pub enum ExecutionPath`（Normal/Paginated/Cached）、`:106` `pub struct QueryOutcome`（查询结果含 rows/elapsed_ms/slow）、workspace 依赖 `tokio = { version = "1.40", features = ["full"] }`（异步运行时）。缺口：无并行查询调度器（多个独立查询并行执行时无并发度控制，可能耗尽连接池），无查询结果合并器（多查询结果需手动合并），无并行查询超时控制与单查询失败降级策略。
   - **批量写入**：`packages/sz-orm-batch/src/lib.rs:43` `pub trait BatchOperations`（batch_insert/batch_update/batch_upsert，**同步**返回 BatchResult）、`:83` `pub struct DefaultBatchOps`（多值 INSERT + CASE WHEN UPDATE + ON DUPLICATE/ON CONFLICT UPSERT）、`:19` `pub struct BatchResult`（inserted/updated/failed/generated_sqls）、`:491` `pub enum RollbackStrategy`（None/Savepoint/PerChunk）、`:482` `pub type ProgressCallback`（进度回调）、`:50` `pub enum UpsertMode`（MysqlOnDuplicate/PostgresOnConflict，**仅两方言**）、`:119` `DEFAULT_CHUNK_SIZE`（分片大小 1000）、`packages/sz-orm-core/src/pool.rs:82` `Connection::execute_with_params`（参数绑定执行，防 SQL 注入）、`packages/sz-orm-core/src/transaction.rs:159` `pub struct Transaction`（事务）、`:527` `TransactionManager`（事务管理器）、`:16` `IsolationLevel`（隔离级别）、`:5 方言 `DbType` `packages/sz-orm-core/src/db_type.rs:11`（MySQL/PostgreSQL/Sqlite/Oracle/SqlServer）。缺口：无批量 DELETE（IN 子句批量删除），`BatchOperations` trait 方法同步返回 BatchResult 仅生成 SQL 不真正执行，`UpsertMode` 仅 MySQL/PostgreSQL 两方言缺 SQLite/Oracle/MSSQL，无异步批量执行器，无事务边界集成，无 PostgreSQL COPY 协议（高效批量导入）。
   - **流式结果集**：`packages/sz-orm-core/src/cursor_stream.rs:29` `pub fn build_paged_query`（五方言分页 SQL 包装：Oracle ROWNUM/SQL Server OFFSET-FETCH/MySQL-PG-SQLite LIMIT-OFFSET）、`:79` `pub fn stream_cursor_paged`（基于分页游标的流式查询执行器，返回 `Pin<Box<dyn futures::Stream>>`，**已实现异步 Stream**，借用 conn）、`packages/sz-orm-core/src/stream_api.rs:176` `pub fn stream_cursor`（**真游标**逐行 fetch，委托 `conn.query_stream_cursor`）、`:50` `pub trait StreamApiExt`（流式 API 扩展）、`packages/sz-orm-core/src/paginator.rs:158` `pub struct Paginator`（分页器）、`:273` `pub trait StreamQueryTrait`（流式查询 trait）、`packages/sz-orm-batch/src/stream.rs:9` `pub struct StreamConfig`（batch_size/max_concurrency/backpressure_threshold）、`:40` `pub struct BackpressureController`（背压控制器，allow_push/push/pop/pending）、`:85` `pub struct BatchSplitter`（批次分割器，**同步** VecDeque）、`:28` `pub struct StreamBatch`（流式批次）。缺口：无 keyset pagination（基于排序键的高效分页，避免 OFFSET 深翻页性能退化），`BackpressureController`/`BatchSplitter` 为同步结构未与异步 Stream 集成，`stream_cursor_paged` 借用 conn 未与连接池集成（每页从池获取连接），无可配置批次大小与内存控制统一抽象。
3. **本机数据库连接信息**：!MySQL 9.6（`mysql://root:test123@127.0.0.1:3306/sz_orm_test`）、PostgreSQL 18（`postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`）、Oracle 23ai Free（`127.0.0.1:1521/freepdb1`）。
4. **sz-pay 生产依赖证据**：sz-pay 从 crates.io 拉取 sz-orm-* 6 个包，作为 API 兼容性验证的下游基准。
5. **五方言覆盖约束**：MySQL/PostgreSQL/SQLite/Oracle/MSSQL，并行查询/批量操作/流式结果集须覆盖全部方言（按方言能力适配，如 PostgreSQL COPY 协议仅 PG 支持）。
6. **既有 feature gate 体系**：`packages/sz-orm-core/Cargo.toml` 已有 40+ feature，v4.3.0 新增 7 个 feature，v4.4.0 新增 6 个 feature（`query-advisor` / `slow-query-diagnosis` / `db-fusion-v2` / `query-logging` / `perf-baseline` / `query-intelligence-loop`），`packages/sz-orm-batch/Cargo.toml` 已有 `batch-stream` feature，作为新能力 feature gate 隔离的基础。

## 1.3 核心输出

1. **并行查询执行器**：sz-orm-parallel（新包，`ParallelQueryScheduler` 并行调度器 + `ResultMerger` 结果合并器 + `MergeStrategy` 合并策略枚举 + `ParallelQueryConfig` 配置 + `ParallelQueryOutcome` 执行结果）+ 与既有 AdaptiveExecutor 协同 API。
2. **批量 INSERT/UPDATE/DELETE 优化**：sz-orm-batch 扩展（`batch_delete` 批量删除 + `BatchExecutor` 异步批量执行器 + SQLite/Oracle/MSSQL 方言批量 SQL 生成 + 事务边界集成 + PostgreSQL COPY 协议支持 + `BatchDialect` 方言抽象）。
3. **异步流式结果集**：sz-orm-stream（新包，`KeysetPaginator` keyset 分页器 + `StreamResultSet` 异步流式结果集 + 背压控制与异步 Stream 集成 + `StreamConfig` 统一配置 + 连接池集成）。
4. **需求追溯矩阵**：本文档第 7 章，建立需求 ↔ 验收条件映射。
5. **验收标准总览**：本文档第 8 章，按需求项汇总验收条件。

## 1.4 职责边界

本组件**不负责**以下事项：

1. **不破坏既有公开 API**：所有新能力通过 feature gate 隔离，既有公开 API 签名保持完全向后兼容。既有 `BatchOperations` trait（`packages/sz-orm-batch/src/lib.rs:43`）保留不动，新增异步批量执行器为独立 API。
2. **不改变既有安全铁律**：任何 WHERE 条件必须参数化，默认禁止 `SELECT *`，N+1 检测自动拦截，批量操作 SQL 生成须参数化（复用既有 `DefaultBatchOps::quote` `packages/sz-orm-batch/src/lib.rs:177` 反引号转义防注入），沿用既有铁律。
3. **不重写 ORM 核心**：`QueryBuilder`（`packages/sz-orm-core/src/query.rs`）运行时构造、`Pool`（`packages/sz-orm-core/src/pool.rs:743`）连接池实现保留不动，并行查询复用既有连接池不重复实现。
4. **不替换既有连接池**：既有 `Pool`（`packages/sz-orm-core/src/pool.rs:743`）/ `Connection` trait（`:45`）/ `PooledConnection`（`:239`）保留，并行查询调度器复用既有连接池，通过并发度控制避免池耗尽，不新建连接池。
5. **不替换既有自适应执行器**：既有 `AdaptiveExecutor`（`packages/sz-orm-adaptive/src/executor.rs:120`）/ `ExecutionPath`（`:16`）/ `QueryOutcome`（`:106`）保留，并行查询执行器与自适应执行器协同（单查询仍可走自适应路径），不修改既有自适应逻辑。
6. **不替换既有批量操作**：既有 `BatchOperations` trait（`packages/sz-orm-batch/src/lib.rs:43`）/ `DefaultBatchOps`（`:83`）/ `BatchResult`（`:19`）/ `RollbackStrategy`（`:491`）/ `ProgressCallback`（`:482`）/ `ConflictTarget` 保留，新增异步批量执行器与批量 DELETE 为扩展，不修改既有同步批量 SQL 生成逻辑。
7. **不替换既有流式游标**：既有 `build_paged_query`（`packages/sz-orm-core/src/cursor_stream.rs:29`）/ `stream_cursor_paged`（`:79`）/ `stream_cursor`（`packages/sz-orm-core/src/stream_api.rs:176`）/ `StreamApiExt`（`:50`）/ `Paginator`（`packages/sz-orm-core/src/paginator.rs:158`）保留，新增 keyset pagination 与异步流式结果集为扩展，不修改既有分页游标逻辑。
8. **不替换既有背压控制器**：既有 `BackpressureController`（`packages/sz-orm-batch/src/stream.rs:40`）/ `StreamConfig`（`:9`）/ `BatchSplitter`（`:85`）保留，异步流式结果集复用既有背压控制器语义，扩展为异步 Stream 集成。
9. **不替换既有事务**：既有 `Transaction`（`packages/sz-orm-core/src/transaction.rs:159`）/ `TransactionManager`（`:527`）/ `IsolationLevel`（`:16`）/ `retry_on_deadlock`（`:466`）保留，批量操作事务边界复用既有事务，不重复实现。
10. **不与 v4.4.0 任务重叠**：v4.4.0 已占用的包/模块（`sz-orm-advisor` 优化建议 / `sz-orm-diagnosis` 诊断 / `sz-orm-fusion` 融合转正 / `sz-orm-observability` 日志 / `sz-orm-explain` 性能基线）本版本不触碰，新增范围全部落在新包（sz-orm-parallel / sz-orm-stream）或 v4.4.0 不触碰的既有包扩展（sz-orm-batch batch-v2 扩展）。
11. **不负责 sz-pay / sz-rust 下游代码修改**：ADR-0001 严禁修改下游/上游仓库，仅保证 API 兼容性。
12. **不降低既有测试覆盖**：v4.5.0 不得使 v4.4.0 已验收测试基线回退，仅增不减。
13. **不引入 unsafe**：所有新增代码无 `unsafe` 块，或必须有 `// SAFETY:` 注释，沿用既有 unsafe 零容忍铁律。
14. **不引入 Breaking Change**：新能力通过 feature gate 隔离，默认全关闭，既有 feature 组合行为不变。
15. **不强制启用新能力**：所有新能力默认关闭或可选启用，避免无配置环境行为变化。
16. **不做分布式查询执行**：跨节点并行查询（如分片查询聚合）需分布式协调器，依赖 sharding 包的成熟度，排除出本版本范围，本版本并行查询仅限单节点多连接并行。
17. **不做列式存储集成**：列式存储需重写结果集编码，无既有代码基础，排除出本版本范围。

---

# 2. 领域术语

**并行查询执行器（Parallel Query Executor）**

: 基于既有连接池 `Pool`（`packages/sz-orm-core/src/pool.rs:743`）与 tokio 异步运行时，将多个独立查询并行执行以降低复杂场景下整体查询延迟，通过并发度控制避免连接池耗尽，复用既有 `AdaptiveExecutor`（`packages/sz-orm-adaptive/src/executor.rs:120`）单查询自适应决策。
: 备注：本版本并行查询仅限单节点多连接并行，不含跨节点分布式查询执行。

**并行查询调度器（Parallel Query Scheduler）**

: 控制多个独立查询的并行执行并发度（同时执行的查询数上限），从既有连接池 `Pool`（`packages/sz-orm-core/src/pool.rs:743`）获取连接，避免并发度过高耗尽连接池，并发度可配（默认不超过连接池 max_size 的 80%）。

**查询结果合并器（Result Merger）**

: 将多个并行查询的结果按合并策略合并为单一结果，`MergeStrategy` 枚举（First 取首个完成 / Union 并集 / Join 关联 / Map 映射转换）抽象合并方式。

**并行查询超时与降级（Parallel Query Timeout & Degradation）**

: 并行查询支持整体超时控制与单查询超时控制，单查询失败时按降级策略处理（Skip 跳过 / Abort 全部中止 / Fallback 返回降级值），不因单查询失败导致整体失败（除非降级策略为 Abort）。

**批量 DELETE（Batch Delete）**

: 通过 IN 子句（`WHERE pk IN (?, ?, ...)`）批量删除多行，减少网络往返，复用既有 `DefaultBatchOps`（`packages/sz-orm-batch/src/lib.rs:83`）分片逻辑（`chunk_size` `:93`），参数化绑定防 SQL 注入。

**异步批量执行器（Async Batch Executor）**

: 将既有 `BatchOperations` trait（`packages/sz-orm-batch/src/lib.rs:43`，同步生成 SQL）扩展为异步执行器，通过既有 `Connection::execute_with_params`（`packages/sz-orm-core/src/pool.rs:82`）真正执行批量 SQL，支持事务边界与部分失败回滚策略（复用既有 `RollbackStrategy` `packages/sz-orm-batch/src/lib.rs:491`）。
: 备注：既有 `BatchOperations` trait 方法同步返回 `BatchResult` 仅生成 SQL 不执行，本版本补异步执行器。

**批量操作方言抽象（Batch Dialect Abstraction）**

: 将既有 `UpsertMode`（`packages/sz-orm-batch/src/lib.rs:50`，仅 MySQL/PostgreSQL 两方言）扩展为五方言批量 SQL 生成（MySQL/PostgreSQL/SQLite/Oracle/MSSQL），复用既有 `DbType`（`packages/sz-orm-core/src/db_type.rs:11`）方言枚举。

**PostgreSQL COPY 协议（PostgreSQL COPY Protocol）**

: PostgreSQL 原生批量数据导入协议（`COPY table FROM STDIN`），比多值 INSERT 性能更高（跳过 SQL 解析），仅 PostgreSQL 方言支持，其他方言降级为多值 INSERT。

**批量操作事务边界（Batch Transaction Boundary）**

: 批量操作可在事务边界内执行，复用既有 `Transaction`（`packages/sz-orm-core/src/transaction.rs:159`）与 `TransactionManager`（`:527`），部分失败时按 `RollbackStrategy`（`packages/sz-orm-batch/src/lib.rs:491`，None/Savepoint/PerChunk）处理。

**keyset pagination（键集分页）**

: 基于排序键（如自增主键或时间戳）的高效分页，通过 `WHERE key > last_key ORDER BY key LIMIT batch` 获取下一页，避免 OFFSET 深翻页性能退化（OFFSET N 需扫描 N 行，深翻页时性能急剧下降）。
: 备注：既有 `build_paged_query`（`packages/sz-orm-core/src/cursor_stream.rs:29`）使用 LIMIT-OFFSET 分页，本版本补 keyset pagination 用于深翻页场景。

**异步流式结果集（Async Stream ResultSet）**

: 大结果集流式返回，实现异步 Stream trait 逐批 yield，避免一次性加载到内存，复用既有 `stream_cursor_paged`（`packages/sz-orm-core/src/cursor_stream.rs:79`，分页游标 Stream）与 `stream_cursor`（`packages/sz-orm-core/src/stream_api.rs:176`，真游标 Stream），扩展背压控制与连接池集成。

**流式背压控制（!（Stream Backpressure）**

: 消费者处理速度慢于生产者拉取速度时，暂停生产者拉取避免内存积压，复用既有 `BackpressureController`（`packages/sz-orm-batch/src/stream.rs:40`，allow_push/push/pending）语义，扩展为异步 Stream 集成。

**流式内存控制（Stream Memory Control）**

: 可配置批次大小（每批拉取行数）控制流式结果集内存占用，批次大小可配（默认 1000），复用既有 `StreamConfig.batch_size`（`packages/sz-orm-batch/src/stream.rs:11`）。

**v4.5.0 feature gate**

: 控制本版本新能力的 feature gate 集合（`parallel-query` / `batch-v2` / `stream-resultset`），默认关闭，避免无配置环境行为变化。

---

# 3. 角色与边界

## 3.1 核心角色

- **ORM 库维护者**：执行 v4.5.0 三项查询执行层性能优化能力的开发、验证、测试操作者，是新增能力的主要使用者与验收人。
- **下游项目开发者（sz-pay）**：关注 API 兼容性的下游使用者，v4.5.0 不得破坏其既有代码。
- **应用开发者**：使用并行查询执行器优化复杂查询场景延迟，使用批量操作优化写入吞吐量，使用流式结果集处理大结果集，是执行优化能力的主要使用者。
- **DBA / 性能工程师**：评估并行查询并发度对连接池与数据库负载的影响，评估批量操作对数据库写入压力的影响，评估流式结果集对长事务与游标资源的影响。
- **运维/SRE 工程师**：配置并行查询并发度、批量操作分片大小与回滚策略、流式结果集批次大小与背压阈值，监控执行优化能力对系统资源的影响。

## 3.2 外部系统

- **MySQL 9.6 / PostgreSQL 18 / SQLite / Oracle 23ai / MSSQL**：并行查询/批量操作/流式结果集的五方言覆盖目标，PostgreSQL 额外支持 COPY 协议。
- **DPRD**：并行查询调度器从既有连接池 `Pool`（`packages/sz-orm-core/src/pool.rs:743`）获取连接，批量操作与流式结果集通过连接执行 SQL。
- **sz-pay 项目**：API 兼容性验证的下游基准。

## 3.3 交互上下文

```plantuml
@startuml
title sz-orm v4.5.0 三项查询执行层性能优化能力 交互上下文

rectangle "sz-orm v4.5.0" as orm {
  rectangle "并行查询执行器\n(调度器+合并器+超时降级)" as parallel
  rectangle "批量 INSERT/UPDATE/DELETE 优化\n(DELETE+异步执行+五方言+事务+COPY)" as batch
  rectangle "异步流式结果集\n(keyset+背压Stream+内存控制)" as stream
}

actor "ORM 库维护者" as dev
actor "应用开发者" as appdev
actor "DBA/性能工程师" as dba
actor "运维/SRE" as sre
actor "sz-pay 开发者" as downstream

database "MySQL/PG/SQLite\n/Oracle/MSSQL" as db
rectangle "连接池\n(既有 Pool)" as pool

dev --> orm : 开发/验证/测试
appdev --> parallel : 并行查询(多独立查询并行)
parallel --> pool : 复用既有连接池(并发度控制)
pool --> db : 多连接并行执行
appdev --> batch : 批量写入(INSERT/UPDATE/DELETE)
batch --> db : 多值INSERT/CASE WHEN/IN删除/COPY
batch --> db : 事务边界(Savepoint/PerChunk回滚)
appdev --> stream : 流式结果集(大结果集逐批)
stream --> pool : 每批从池获取连接
stream --> db : keyset/LIMIT-OFFSET/真游标
dba --> parallel : 评估并发度对池与DB负载影响
dba --> batch : 评估批量对写入压力影响
dba --> stream : 评估游标对长事务影响
sre --> parallel : 配置并发度
sre --> batch : 配置分片大小+回滚策略
sre --> stream : 配置批次大小+背压阈值
downstream --> orm : API 兼容性验证

@enduml
```

---

# 4. DFX约束

## 4.1 性能

1. **并行查询延迟优化**：N 个独立查询并行执行的整体延迟应接近最慢单查询延迟（而非 N 个查询延迟之和），并发度受连接池 max_size 限制，并行调度开销不超过 1ms（含并发度控制 + 连接获取调度）。
2. **并行查询并发度上限**：并行查询并发度默认不超过连接池 max_size 的 80%（预留连接给其他请求），可配，避免并行查询耗尽连接池。
3. **批量操作网络往返优化**：批量 INSERT/UPDATE/DELETE 应将 N 行数据的网络往返从 N 次降为 ceil(N/chunk_size) 次（默认 chunk_size 1000，复用既有 `DEFAULT_CHUNK_SIZE` `packages/sz-orm-batch/src/lib.rs:119`），批量 SQL 生成开销不超过 10ms（含方言适配 + 分片 + SQL 构造，单批 1000 行）。
4. **PostgreSQL COPY 协议性能**：PostgreSQL COPY 协议批量导入性能应优于多值 INSERT（跳过 SQL 解析），仅 PostgreSQL 方言启用，其他方言降级为多值 INSERT。
5. **流式结果集内存控制**：流式结果集内存占用应不超过批次大小 × 单行大小（默认批次 1000 行），不一次性加载全量结果集，背压触发时暂停生产者拉取。
6. **keyset pagination 深翻页性能**：keyset pagination 深翻页（如第 100 万页）性能应优于 OFFSET 分页（OFFSET 100 万需扫描 100 万行），通过 `WHERE key > last_key` 索引扫描实现。
7. **流式结果集背压开销**：背压控制检查开销不超过 1μs/次（含队列长度检查 + 暂停/恢复判定），不显著影响流式吞吐量。

## 4.2 可靠性

1. **并行查询单查询失败降级**：并行查询中单查询失败时，按降级策略处理（Skip 跳过该查询结果 / Abort 全部中止 / Fallback 返回降级值），不因单查询失败导致整体 panic，除非降级策略为 Abort。
2. **并行查询超时控制**：并行查询支持整体超时与单查询超时，超时后取消未完成查询（释放连接），不泄漏连接。
3. **并行查询连接池保护**：并行查询并发度受连接池 max_size 限制，连接获取失败时降级为串行执行（降低并发度），不无限等待连接。
4. **批量操作部分失败回滚**：批量操作部分分片失败时，按 `RollbackStrategy`（`packages/sz-orm-batch/src/lib.rs:491`）处理：None 失败分片计入 failed 不影响已成功分片；Savepoint 每分片前生成 SAVEPOINT 失败回滚到 savepoint；PerChunk 任一分片失败整批中止后续分块不再执行。
5. **批量操作事务一致性**：批量操作在事务边界内执行时，须保证原子性（全成功提交 / 全失败回滚），部分失败时按 `RollbackStrategy` 处理，不产生部分提交的脏数据。
6. **批量操作 idempotency**：批量 INSERT 分片失败重试时，已成功分片不重复插入（通过主键冲突检测或 UPSERT 语义），避免重复数据。
7. **流式结果集连接池集成**：流式结果集每批从连接池获取连接，批次完成后归还连接，不长期占用连接（除非真游标模式需保持连接），连接获取失败时降级为等待重试。
8. **流式结果集背压不丢失数据**：背压触发暂停生产者时，已拉取数据须保留在内存队列中不丢失，消费者处理完后恢复生产者拉取。
9. **流式结果集游标资源释放**：流式结果集消费完成或提前中止时，须释放游标资源（真游标模式关闭服务端游标，分页模式无游标资源），不泄漏游标。
10. **v4.4.0 测试基线不回退**：v4.5.0 不得使 v4.4.0 已验收测试基线回退，仅增不减。

## 4.3 安全性

1. **批量操作参数化**：批量操作 SQL 生成须参数化绑定（复用既有 `DefaultBatchOps::quote` `packages/sz-orm-batch/src/lib.rs:177` 反引号转义 + `Connection::execute_with_params` `packages/sz-orm-core/src/pool.rs:82` 参数绑定），禁止 SQL 字符串拼接，防止 SQL 注入。
2. **批量 DELETE 范围保护**：批量 DELETE 须校验删除条件（禁止无条件全表删除），删除行数须不超过传入行数，防止误删。
3. **并行查询参数隔离**：并行查询各查询参数独立绑定，不交叉污染，单查询参数泄露不影响其他查询。
4. **流式结果集不泄露连接**：流式结果集连接须在消费完成或中止后归还连接池，不泄露给消费者，连接凭据不暴露。
5. **审计证据要求**：每项需求结论须附 file:line 证据，遵循 AGENTS.md 审计合规铁律。

## 4.4 可维护性

1. **并行查询可配置**：并行查询并发度、超时、降级策略须可配置，不强制干扰开发者，默认值保守（并发度 = 池 max_size 80%，超时 30 秒，降级 Skip）。
2. **批量操作可配置**：批量操作分片大小、回滚策略、进度回调、UPSERT 模式、COPY 协议开关须可配置（复用既有 `DefaultBatchOps` 配置链 `packages/sz-orm-batch/src/lib.rs:134`），不强制干扰开发者。
3. **流式结果集可配置**：流式结果集批次大小、背压阈值、分页策略（keyset/LIMIT-OFFSET/真游标）须可配置，不强制干扰开发者。
4. **五方言一致**：新增能力在 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 五方言上行为一致（并行查询/批量操作/流式结果集按方言能力适配，如 COPY 仅 PG）。
5. **批量操作审计可追溯**：批量操作须返回 `BatchResult`（复用既有 `BatchResult.generated_sqls` `packages/sz-orm-batch/src/lib.rs:23`，含生成的 SQL 供审计），异步执行器须记录执行结果（成功/失败/影响行数）。

## 4.5 兼容性

1. **API 向后兼容**：所有新能力通过 feature gate 隔离，默认 feature 行为不变，既有公开 API 完全向后兼容。
2. **sz-pay 不破坏**：sz-pay 从 crates.io 拉取的 sz-orm-* 6 个包既有用法不受影响。
3. **既有连接池保留**：既有 `Pool`（`packages/sz-orm-core/src/pool.rs:743`）/ `Connection` trait（`:45`）/ `PooledConnection`（`:239`）保留不动，并行查询复用既有连接池。
4. **既有自适应执行器保留**：既有 `AdaptiveExecutor`（`packages/sz-orm-adaptive/src/executor.rs:120`）/ `ExecutionPath`（`:16`）/ `QueryOutcome`（`:106`）保留不动，并行查询与自适应执行器协同。
5. **既有批量操作保留**：既有 `BatchOperations` trait（`packages/sz-orm-batch/src/lib.rs:43`）/ `DefaultBatchOps`（`:83`）/ `BatchResult`（`:19`）/ `RollbackStrategy`（`:491`）/ `ProgressCallback`（`:482`）/ `UpsertMode`（`:50`）/ `ConflictTarget` 保留不动，新增异步执行器与批量 DELETE 为扩展。
6. **既有流式游标保留**：既有 `build_paged_query`（`packages/sz-orm-core/src/cursor_stream.rs:29`）/ `stream_cursor_paged`（`:79`）/ `stream_cursor`（`packages/sz-orm-core/src/stream_api.rs:176`）/ `StreamApiExt`（`:50`）/ `Paginator`（`packages/sz-orm-core/src/paginator.rs:158`）保留不动，新增 keyset pagination 与异步流式结果集为扩展。
7. **既有背压控制器保留**：既有 `BackpressureController`（`packages/sz-orm-batch/src/stream.rs:40`）/ `StreamConfig`（`:9`）/ `BatchSplitter`（`:85`）保留不动，异步流式结果集复用既有背压语义。
8. **既有事务保留**：既有 `Transaction`（`packages/sz-orm-core/src/transaction.rs:159`）/ `TransactionManager`（`:527`）/ `IsolationLevel`（`:16`）保留不动，批量操作复用既有事务。
9. **既有 feature 组合不破坏**：v4.5.0 新增 feature（`parallel-query` / `batch-v2` / `stream-resultset`）与既有 feature（含 v4.3.0 7 个 + v4.4.0 6 个 + sz-orm-batch `batch-stream`）任意组合编译通过。

---

# 5. 核心能力

## 5.1 并行查询执行器（REQ-V45-001，P1）

### 5.1.1 业务规则

1. **并行查询调度器**（EARS: Ubiquitous）
   系统应当提供 `sz-orm-parallel` 包，`ParallelQueryScheduler` 将多个独立查询并行执行，从既有连接池 `Pool`（`packages/sz-orm-core/src/pool.rs:743`）获取连接，通过并发度控制（同时执行的查询数上限）避免连接池耗尽，并发度默认不超过连接池 max_size 的 80%，可配。
   a. 验收条件：[传入 3 个独立查询，并发度 2，连接池 max_size 10] → [同时执行 2 个查询，第 3 个等待，整体延迟接近最慢 2 个查询之和而非 3 个之和]
2. **复用既有连接池与异步运行时**（EARS: Ubiquitous）
   系统应当复用既有连接池 `Pool`（`packages/sz-orm-core/src/pool.rs:743`）与 `Connection` trait（`:45`，异步 execute/query）+ tokio 异步运行时（workspace 依赖），不新建连接池，不引入新异步运行时。
   a. 验收条件：[并行查询执行] → [从既有 `Pool::acquire` 获取连接，通过 tokio::join 并行执行，不新建连接池]
3. **与既有 AdaptiveExecutor 协同**（EARS: Ubiquitous）
   系统应当与既有自适应执行器 `AdaptiveExecutor`（`packages/sz-orm-adaptive/src/executor.rs:120`）协同，并行查询中每个单查询仍可走自适应路径（`ExecutionPath` Normal/Paginated/Cached `:16`），并行调度器不修改既有自适应决策逻辑。
   a. 验收条件：[并行查询含 1 个大结果集查询] → [该查询走 `ExecutionPath::Paginated` 自适应路径，其他查询走 Normal，并行调度器不干预单查询自适应决策]
4. **查询结果合并器**（EARS: Ubiquitous）
   系统应当提供 `ResultMerger` 将多个并行查询的结果按 `MergeStrategy` 合并为单一结果：`First`（取首个完成的结果）/ `Union`（并集合并）/ `Join`（按指定键关联）/ `Map`（映射转换），合并策略可配。
   a. 验收条件：[3 个查询并行，MergeStrategy::First] → [返回首个完成的查询结果]；[MergeStrategy::Union] → [返回 3 个查询结果的并集]
5. **并行查询超时控制**（EARS: State-driven）
   当并行查询超过整体超时或单查询超过单查询超时时，系统应当取消未完成查询（释放连接），返回已完成查询结果 + 超时标记，不泄漏连接，整体超时与单查询超时均可配（默认整体 30 秒，单查询 10 秒）。
   a. 验收条件：[3 个查询并行，单查询超时 5 秒，其中 1 个查询耗时 10 秒] → [该查询 5 秒后取消释放连接，返回其他 2 个查询结果 + 超时标记]
6. **单查询失败降级**（EARS: Event-driven）
   如果并行查询中单查询失败，则系统应当按降级策略处理：`Skip`（跳过该查询结果，继续其他查询）/ `Abort`（取消所有查询，返回错误）/ `Fallback`（返回降级值），默认 `Skip`，不因单查询失败导致整体 panic。
   a. 验收条件：[3 个查询并行，降级策略 Skip，其中 1 个查询失败] → [跳过失败查询，返回其他 2 个查询结果 + 失败标记]；[降级策略 Abort] → [取消所有查询，返回错误]
7. **禁止项**（EARS: Unwanted）
   如果并行查询执行器影响默认 feature 编译或耗尽连接池，则系统应当通过 `parallel-query` feature gate 隔离，默认不启用并行查询，且并发度受连接池 max_size 限制不耗尽池。
   a. 验收条件：[`cargo build` 默认编译] → [无并行查询执行器，行为与 v4.4.0 一致]

### 5.1.2 交互流程

```plantuml
@startuml
title 并行查询执行器 交互流程
actor "应用开发者" as appdev
participant "sz-orm-parallel\n(并行调度器)" as scheduler
participant "Pool\n(既有连接池)" as pool
participant "AdaptiveExecutor\n(既有自适应)" as adaptive
database "DB" as db

appdev -> scheduler : parallel(queries, config)
scheduler -> pool : acquire(并发度个连接)
pool --> scheduler : N 个连接
par 并行执行
  scheduler -> adaptive : execute(query_1)
  adaptive -> db : 查询_1(自适应路径)
  db --> adaptive : 结果_1
  adaptive --> scheduler : QueryOutcome_1
else
  scheduler -> adaptive : execute(query_2)
  adaptive -> db : 查询_2(自适应路径)
  db --> adaptive : 结果_2
  adaptive --> scheduler : QueryOutcome_2
end
alt 全部成功
  scheduler -> scheduler : ResultMerger.merge(结果们, strategy)
  scheduler --> appdev : ParallelQueryOutcome(合并结果)
else 单查询失败
  scheduler -> scheduler : 降级策略(Skip/Abort/Fallback)
  alt Skip
    scheduler --> appdev : ParallelQueryOutcome(成功结果 + 失败标记)
  else Abort
    scheduler --> appdev : Error(全部中止)
  end
end
@enduml
```

### 5.1.3 异常场景

1. **连接池耗尽**
   a. 触发条件：并行查询并发度超过连接池可用连接数，且其他请求占用连接
   b. 系统行为：降级为串行执行（降低并发度到可用连接数），不无限等待连接，超时后返回错误
   c. 用户感知：结果标注"connection pool exhausted, degraded to serial execution"或错误"parallel query timeout waiting for connection"
2. **单查询超时**
   a. 触发条件：单查询执行时间超过单查询超时阈值
   b. 系统行为：取消该查询（释放连接），按降级策略处理，其他查询继续
   c. 用户感知：结果标注"query N timed out after X seconds, skipped/aborted/fallback"
3. **结果合并失败**
   a. 触发条件：合并策略无法合并结果（如 Join 键不匹配、Map 转换失败）
   b. 系统行为：降级为返回未合并的原始结果列表，标注"merge failed, returning raw results"
   c. 用户感知：结果标注"merge strategy failed, raw results returned"

## 5.2 批量 INSERT/UPDATE/DELETE 优化（REQ-V45-002，P1）

### 5.2.1 业务规则

1. **批量 DELETE**（EARS: Ubiquitous）
   系统应当扩展既有 `BatchOperations` trait（`packages/sz-orm-batch/src/lib.rs:43`），新增 `batch_delete` 方法，通过 IN 子句（`WHERE pk IN (?, ?, ...)`）批量删除多行，复用既有 `DefaultBatchOps` 分片逻辑（`chunk_size` `:93`）按 chunk_size 分片生成多条 DELETE，参数化绑定防 SQL 注入。
   a. 验收条件：[传入 2500 行删除，chunk_size 1000] → [生成 3 条 DELETE SQL（1000 + 1000 + 500），WHERE pk IN (?, ?, ...) 参数化绑定]
2. **异步批量执行器**（EARS: Ubiquitous）
   系统应当提供 `BatchExecutor` 异步批量执行器，通过既有 `Connection::execute_with_params`（`packages/sz-orm-core/src/pool.rs:82`）真正执行批量 SQL（既有 `BatchOperations` trait `packages/sz-orm-batch/src/lib.rs:43` 同步返回 `BatchResult` 仅生成 SQL），返回 `BatchResult`（复用既有 `BatchResult` `:19`，含 inserted/updated/failed/generated_sqls）。
   a. 验收条件：[`BatchExecutor::execute_batch_insert(conn, table, rows).await`] → [通过 `execute_with_params` 执行生成的 SQL，返回 `BatchResult` 含实际插入行数]
3. **五方言批量 SQL 生成**（EARS: Ubiquitous）
   系统应当将既有 `UpsertMode`（`packages/sz-orm-batch/src/lib.rs:50`，仅 MySQL/PostgreSQL 两方言）扩展为五方言批量 SQL 生成（MySQL/PostgreSQL/SQLite/Oracle/MSSQL），复用既有 `DbType`（`packages/sz-orm-core/src/db_type.rs:11`）方言枚举，按方言适配批量 INSERT/UPDATE/DELETE/UPSERT SQL 语法（如 SQLite `ON CONFLICT`/Oracle `MERGE`/MSSQL `MERGE`）。
   a. 验收条件：[DbType::Sqlite 批量 UPSERT] → [生成 `ON CONFLICT DO UPDATE` 语法]；[DbType::Oracle 批量 UPSERT] → [生成 `MERGE` 语法]；[DbType::SqlServer 批量 UPSERT] → [生成 `MERGE` 语法]
4. **PostgreSQL COPY 协议**（EARS: Optional）
   当方言为 PostgreSQL 且启用 COPY 协议时，系统可以使用 `COPY table FROM STDIN` 原生批量导入协议（比多值 INSERT 性能更高，跳过 SQL 解析），其他方言降级为多值 INSERT。
   a. 验收条件：[DbType::PostgreSQL + COPY 启用 + 批量 INSERT 10000 行] → [使用 COPY 协议导入，性能优于多值 INSERT]；[DbType::MySQL + COPY 启用] → [降级为多值 INSERT，标注"COPY not supported, fallback to multi-value INSERT"]
5. **事务边界与部分失败回滚**（EARS: Ubiquitous）
   系统应当支持批量操作在事务边界内执行，复用既有 `Transaction`（`packages/sz-orm-core/src/transaction.rs:159`）与 `TransactionManager`（`:527`），部分分片失败时按既有 `RollbackStrategy`（`packages/sz-orm-batch/src/lib.rs:491`）处理：None 失败分片计入 failed 不影响已成功分片；Savepoint 每分片前生成 SAVEPOINT 失败回滚到 savepoint；PerChunk 任一分片失败整批中止。
   a. 验收条件：[批量 INSERT 3 分片，第 2 分片失败，RollbackStrategy::Savepoint] → [第 1 分片保留，第 2 分片回滚到 savepoint，第 3 分片继续，返回 BatchResult { inserted: 2000, failed: 1000 }]；[RollbackStrategy::PerChunk] → [第 1 分片保留，第 2 分片失败后中止，第 3 分片不执行]
6. **复用既有分片与进度回调**（EARS: Ubiquitous）
   系统应当复用既有 `DefaultBatchOps` 分片逻辑（`chunk_indices` `packages/sz-orm-batch/src/lib.rs:164`）与进度回调（`ProgressCallback` `:482` / `BatchProgress` `:451`），异步批量执行器在每分片执行前后触发进度回调（`BatchStage::ProcessingChunk` / `ChunkCompleted`）。
   a. 验收条件：[批量 INSERT 3 分片 + 进度回调] → [回调触发 Started → ProcessingChunk → ChunkCompleted × 3 → Finished]
7. **批量 DELETE 范围保护**（EARS: Unwanted）
   如果批量 DELETE 传入空行列表或无条件删除，则系统应当拒绝执行（返回 `BatchResult { failed: 0, generated_sqls: [] }` 或错误），防止误删全表。
   a. 验收条件：[batch_delete 传入空行] → [返回空 BatchResult 不生成 SQL]；[batch_delete 未指定主键] → [返回错误"batch_delete requires primary key values"]
8. **禁止项**（EARS: Unwanted）
   如果批量优化影响默认 feature 编译或破坏既有 `BatchOperations` trait，则系统应当通过 `batch-v2` feature gate 隔离，默认不启用异步执行器与批量 DELETE，且既有 `BatchOperations` trait 保留不动。
   a. 验收条件：[`cargo build` 默认编译] → [无异步批量执行器与批量 DELETE，既有 `BatchOperations` trait 行为与 v4.4.0 一致]

### 5.2.2 交互流程

```plantuml
@startuml
title 批量 INSERT/UPDATE/DELETE 优化 交互流程
actor "应用开发者" as appdev
participant "sz-orm-batch\n(异步执行器)" as executor
participant "DefaultBatchOps\n(既有SQL生成)" as ops
participant "Connection\n(既有)" as conn
participant "Transaction\n(既有)" as tx
database "DB" as db

appdev -> executor : execute_batch_insert(conn, table, rows, config)
executor -> ops : batch_insert(table, rows) [复用既有SQL生成]
ops --> executor : BatchResult { generated_sqls }
executor -> ops : chunk_indices(rows.len()) [复用既有分片]
alt 无事务
  loop 每个分片
    executor -> conn : execute_with_params(sql, params) [参数化绑定]
    conn -> db : 执行批量SQL
    db --> conn : 影响行数
    conn --> executor : inserted/failed
    executor -> appdev : ProgressCallback(ChunkCompleted)
  end
else 有事务 + Savepoint
  executor -> tx : begin_transaction
  loop 每个分片
    executor -> tx : SAVEPOINT sp_N
    executor -> conn : execute_with_params(sql, params)
    alt 分片成功
      conn --> executor : inserted
    else 分片失败
      executor -> tx : ROLLBACK TO SAVEPOINT sp_N
    end
  end
  executor -> tx : commit
end
executor --> appdev : BatchResult { inserted, updated, failed, generated_sqls }
@enduml
```

### 5.2.3 异常场景

1. **部分分片失败**
   a. 触发条件：批量操作中某分片执行失败（如主键冲突、字段约束违反）
   b. 系统行为：按 `RollbackStrategy` 处理（None 计入 failed / Savepoint 回滚该分片 / PerChunk 中止后续），不 panic
   c. 用户感知：`BatchResult { failed: N, generated_sqls: [...] }`，标注失败分片
2. **事务死锁**
   a. 触发条件：批量操作事务与其他事务死锁
   b. 系统行为：复用既有 `retry_on_deadlock`（`packages/sz-orm-core/src/transaction.rs:466`）重试，重试次数超限后返回错误
   c. 用户感知：错误"batch transaction deadlock, retried N times, failed"
3. **PostgreSQL COPY 协议失败**
   a. 触发条件：COPY 协议执行失败（如连接中断、数据格式错误）
   b. 系统行为：降级为多值 INSERT 重试，标注"COPY failed, fallback to multi-value INSERT"
   c. 用户感知：结果标注"COPY protocol failed, retried with multi-value INSERT"
4. **方言不支持**
   a. 触发条件：某方言不支持特定批量操作（如 SQLite 不支持 COPY、Oracle MERGE 语法限制）
   b. 系统行为：降级为通用多值 INSERT，标注"dialect N does not support operation M, fallback to generic"
   c. 用户感知：结果标注"dialect fallback to generic batch operation"

## 5.3 异步流式结果集（REQ-V45-003，P1）

### 5.3.1 业务规则

1. **异步 Stream 结果集**（EARS: Ubiquitous）
   系统应当提供 `sz-orm-stream` 包，`StreamResultSet` 实现异步 Stream trait 逐批 yield 结果，避免一次性加载全量结果集到内存，复用既有 `stream_cursor_paged`（`packages/sz-orm-core/src/cursor_stream.rs:79`，分页游标 Stream）与 `stream_cursor`（`packages/sz-orm-core/src/stream_api.rs:176`，真游标 Stream）。
   a. 验收条件：[查询返回 100 万行，批次大小 1000] → [StreamResultSet 逐批 yield 1000 行，内存占用约 1000 行大小，不一次性加载 100 万行]
2. **keyset pagination**（EARS: Ubiquitous）
   系统应当提供 `KeysetPaginator` 基于排序键的高效分页，通过 `WHERE key > last_key ORDER BY key LIMIT batch` 获取下一页，避免 OFFSET 深翻页性能退化（既有 `build_paged_query` `packages/sz-orm-core/src/cursor_stream.rs:29` 使用 LIMIT-OFFSET），keyset 须指定排序列与最后键值。
   a. 验收条件：[keyset pagination 第 100 万页，排序键 id] → [通过 `WHERE id > last_id ORDER BY id LIMIT 1000` 索引扫描获取，性能优于 OFFSET 1000000]
3. **分页策略可选**（EARS: Ubiquitous）
   系统应当支持三种分页策略可选：`Keyset`（keyset pagination，深翻页高效）/ `LimitOffset`（既有 LIMIT-OFFSET 分页，复用 `build_paged_query` `packages/sz-orm-core/src/cursor_stream.rs:29`）/ `ServerCursor`（真游标，复用 `stream_cursor` `packages/sz-orm-core/src/stream_api.rs:176`，需数据库支持），默认 `LimitOffset`。
   a. 验收条件：[分页策略 Keyset] → [使用 keyset pagination]；[分页策略 ServerCursor + PostgreSQL] → [使用真游标]；[分页策略 ServerCursor + 不支持游标的方言] → [降级为 LimitOffset，标注"server cursor not supported, fallback to limit-offset"]
4. **背压控制与异步 Stream 集成**（EARS: Ubiquitous）
   系统应当复用既有 `BackpressureController`（`packages/sz-orm-batch/src/stream.rs:40`，allow_push/push/pending）语义，扩展为异步 Stream 集成：消费者处理速度慢于生产者拉取速度时，暂停生产者拉取（不拉取下一批），避免内存积压，背压阈值可配（复用既有 `StreamConfig.backpressure_threshold` `:13`，默认 10000）。
   a. 验收条件：[生产者拉取 10 批，消费者处理 1 批，背压阈值 10000 行] → [生产者拉取到积压超 10000 行时暂停，消费者处理完后恢复]
5. **可配置批次大小与内存控制**（EARS: Ubiquitous）
   系统应当支持可配置批次大小（每批拉取行数）控制流式结果集内存占用，批次大小可配（复用既有 `StreamConfig.batch_size` `packages/sz-orm-batch/src/stream.rs:11`，默认 1000），内存占用不超过批次大小 × 单行大小。
   a. 验收条件：[批次大小 500，单行 1KB] → [内存占用约 500KB，不一次性加载全量]
6. **连接池集成**（EARS: Ubiquitous）
   系统应当将流式结果集与既有连接池 `Pool`（`packages/sz-orm-core/src/pool.rs:743`）集成，每批从池获取连接执行查询，批次完成后归还连接（分页模式），真游标模式保持连接至消费完成，连接获取失败时降级为等待重试。
   a. 验收条件：[分页模式流式查询 10 批] → [每批从 Pool::acquire 获取连接，批次完成后 Pool::release 归还，不长期占用连接]；[真游标模式] → [保持连接至 Stream 消费完成]
7. **复用既有分页 SQL 包装**（EARS: Ubiquitous）
   系统应当复用既有 `build_paged_query`（`packages/sz-orm-core/src/cursor_stream.rs:29`，五方言分页 SQL 包装：Oracle ROWNUM/SQL Server OFFSET-FETCH/MySQL-PG-SQLite LIMIT-OFFSET）生成 LimitOffset 分页 SQL，不重复实现方言适配。
   a. 验收条件：[LimitOffset 分页 + DbType::Oracle] → [复用 `build_paged_query` 生成 ROWNUM 子查询包装]
8. **游标资源释放**（EARS: State-driven）
   当流式结果集消费完成或提前中止时，系统应当释放游标资源（真游标模式关闭服务端游标，分页模式无游标资源）并归还连接，不泄漏游标与连接。
   a. 验收条件：[真游标模式 Stream 消费完成] → [关闭服务端游标 + 归还连接]；[提前 drop StreamResultSet] → [释放游标 + 归还连接（Drop 语义）]
9. **禁止项**（EARS: Unwanted）
   如果异步流式结果集影响默认 feature 编译或一次性加载全量结果集，则系统应当通过 `stream-resultset` feature gate 隔离，默认不启用流式结果集，且逐批 yield 不一次性加载。
   a. 验收条件：[`cargo build` 默认编译] → [无 StreamResultSet，行为与 v4.4.0 一致]

### 5.3.2 交互流程

```plantuml
@startuml
title 异步流式结果集 交互流程
actor "应用开发者" as appdev
participant "sz-orm-stream\n(StreamResultSet)" as stream
participant "Pool\n(既有连接池)" as pool
participant "build_paged_query\n(既有分页SQL)" as paged
participant "BackpressureController\n(既有背压)" as backpressure
database "DB" as db

appdev -> stream : stream_query(sql, config)
stream -> backpressure : BackpressureController::new(threshold)
alt 分页模式 (Keyset/LimitOffset)
  loop 每批
    stream -> backpressure : allow_push() [背压检查]
    alt 允许拉取
      stream -> pool : acquire() [获取连接]
      pool --> stream : conn
      alt Keyset
        stream -> stream : WHERE key > last_key ORDER BY key LIMIT batch
      else LimitOffset
        stream -> paged : build_paged_query(db_type, sql, offset, batch) [复用既有]
        paged --> stream : 分页SQL
      end
      stream -> db : 执行分页查询
      db --> stream : 批次结果
      stream -> pool : release(conn) [归还连接]
      stream -> backpressure : push() [入队]
      stream --> appdev : yield 批次结果
    else 背压暂停
      stream -> stream : 等待消费者处理
    end
  end
else 真游标模式 (ServerCursor)
  stream -> pool : acquire() [获取连接保持]
  stream -> db : DECLARE CURSOR
  loop 每批
    stream -> db : FETCH batch
    db --> stream : 批次结果
    stream --> appdev : yield 批次结果
  end
  stream -> db : CLOSE CURSOR
  stream -> pool : release(conn) [归还连接]
end
@enduml
```

### 5.3.3 异常场景

1. **连接获取失败**
   a. 触发条件：连接池无可用连接且超时
   b. 系统行为：降级为等待重试（可配重试次数），超限后 yield 错误并结束 Stream
   c. 用户感知：Stream yield `Err("connection pool timeout")` 后结束
2. **分页查询失败**
   a. 触发条件：某批分页查询执行失败（如 SQL 错误、连接中断）
   b. 系统行为：yield 错误并结束 Stream，释放连接，不继续拉取
   c. 用户感知：Stream yield `Err("query failed: ...")` 后结束
3. **keyset 排序列无索引**
   a. 触发条件：keyset pagination 排序列无索引，导致全表扫描
   b. 系统行为：正常执行（性能退化由数据库处理），结果标注"keyset column has no index, performance may degrade"
   c. 用户感知：结果标注"warning: keyset column N has no index"
4. **真游标不支持**
   a. 触发条件：分页策略 ServerCursor 但方言不支持真游标（如 SQLite）
   b. 系统行为：降级为 LimitOffset 分页，标注"server cursor not supported, fallback to limit-offset"
   c. 用户感知：结果标注"server cursor not supported by dialect N, fallback to limit-offset"
5. **Stream 提前 drop**
   a. 触发条件：消费者提前 drop StreamResultSet（未消费完）
   b. 系统行为：Drop 语义释放游标资源 + 归还连接，不泄漏
   c. 用户感知：无（资源自动释放）

---

# 6. 数据约束

## 6.1 ParallelQueryConfig（并行查询配置）

1. **concurrency**：并行并发度上限，usize，必填，默认连接池 max_size 的 80%，不超过 max_size。
2. **overall_timeout_ms**：整体超时（毫秒），u64，必填，默认 30000（30 秒），0 表示不超时。
3. **per_query_timeout_ms**：单查询超时（毫秒），u64，必填，默认 10000（10 秒），0 表示不超时。
4. **failure_strategy**：单查询失败降级策略，`FailureStrategy` 枚举（Skip/Abort/Fallback），必填，默认 Skip。
5. **merge_strategy**：结果合并策略，`MergeStrategy` 枚举（First/Union/Join/Map），必填，默认 First。

## 6.2 ParallelQueryOutcome（并行查询结果）

1. **results**：各查询结果，`Vec<Option<QueryOutcome>>`，必填（失败查询为 None）。
2. **failures**：失败查询信息，`Vec<QueryFailure>`（含查询索引 + 错误信息），必填（可为空）。
3. **timed_out**：超时查询索引，`Vec<usize>`，必填（可为空）。
4. **total_elapsed_ms**：整体耗时（毫秒），u64，必填。
5. **merged_result**：合并后结果，`Option<Value>`，可选（合并策略为 First 时有值）。

## 6.3 MergeStrategy（合并策略）

1. **First**：取首个完成的查询结果。
2. **Union**：并集合并所有查询结果。
3. **Join**：按指定键关联多个查询结果（须指定 join_key）。
4. **Map**：映射转换各查询结果后合并（须指定 transform 函数）。

## 6.4 BatchExecutorConfig（异步批量执行配置）

1. **chunk_size**：分片大小，usize，必填，默认 1000（复用既有 `DEFAULT_CHUNK_SIZE` `packages/sz-orm-batch/src/lib.rs:119`）。
2. **rollback_strategy**：回滚策略，`RollbackStrategy`（既有 `packages/sz-orm-batch/src/lib.rs:491`，None/Savepoint/PerChunk），必填，默认 None。
3. **progress_callback**：进度回调，`Option<ProgressCallback>`（既有 `:482`），可选。
4. **use_copy_protocol**：是否使用 PostgreSQL COPY 协议，bool，必填，默认 false。
5. **transaction**：事务边界，`Option<Transaction>`（既有 `packages/sz-orm-core/src/transaction.rs:159`），可选（None 表示无事务边界）。

## 6.5 BatchDeleteRequest（批量删除请求）

1. **table**：表名，String，必填。
2. **primary_key**：主键列名，String，必填（复用既有 `DefaultBatchOps.primary_key` `packages/sz-orm-batch/src/lib.rs:84`）。
3. **ids**：待删除主键值列表，`Vec<Value>`，必填，非空（空则拒绝执行防误删）。

## 6.6 StreamResultSetConfig（流式结果集配置）

1. **batch_size**：批次大小（每批拉取行数），usize，必填，默认 1000（复用既有 `StreamConfig.batch_size` `packages/sz-orm-batch/src/stream.rs:11`）。
2. **backpressure_threshold**：背压阈值（队列最大积压量），usize，必填，默认 10000（复用既有 `StreamConfig.backpressure_threshold` `:13`）。
3. **pagination_strategy**：分页策略，`PaginationStrategy` 枚举（Keyset/LimitOffset/ServerCursor），必填，默认 LimitOffset。
4. **keyset_column**：keyset 排序列，`Option<String>`，可选（分页策略为 Keyset 时必填）。
5. **db_type**：数据库方言，`DbType`（既有 `packages/sz-orm-core/src/db_type.rs:11`），必填。

## 6.7 KeysetPaginator（keyset 分页器）

1. **key_column**：排序键列名，String，必填。
2. **last_key**：最后键值，`Option<Value>`，必填（首次为 None，后续为上一批最后一行的键值）。
3. **batch_size**：批次大小，usize，必填。
4. **order_direction**：排序方向，`OrderDirection` 枚举（Asc/Desc），必填，默认 Asc。

## 6.8 PaginationStrategy（分页策略）

1. **Keyset**：keyset pagination（基于排序键，深翻页高效）。
2. **LimitOffset**：LIMIT-OFFSET 分页（复用既有 `build_paged_query` `packages/sz-orm-core/src/cursor_stream.rs:29`）。
3. **ServerCursor**：服务端真游标（复用既有 `stream_cursor` `packages/sz-orm-core/src/stream_api.rs:176`，需数据库支持）。

---

# 7. 需求追溯矩阵

| 需求 ID | 优先级 | 需求名称 | 验收条件数 | feature gate | 复用既有代码 |
|---------|--------|---------|-----------|-------------|-------------|
| REQ-V45-001 | P1 | 并行查询执行器 | 7 | `parallel-query` | `Pool` `packages/sz-orm-core/src/pool.rs:743` / `Connection` `packages/sz-orm-core/src/pool.rs:45` / `AdaptiveExecutor` `packages/sz-orm-adaptive/src/executor.rs:120` / `ExecutionPath` `packages/sz-orm-adaptive/src/executor.rs:16` / `QueryOutcome` `packages/sz-orm-adaptive/src/executor.rs:106` |
| REQ-V45-002 | P1 | 批量 INSERT/UPDATE/DELETE 优化 | 8 | `batch-v2` | `BatchOperations` `packages/sz-orm-batch/src/lib.rs:43` / `DefaultBatchOps` `packages/sz-orm-batch/src/lib.rs:83` / `BatchResult` `packages/sz-orm-batch/src/lib.rs:19` / `RollbackStrategy` `packages/sz-orm-batch/src/lib.rs:491` / `ProgressCallback` `packages/sz-orm-batch/src/lib.rs:482` / `UpsertMode` `packages/sz-orm-batch/src/lib.rs:50` / `Connection::execute_with_params` `packages/sz-orm-core/src/pool.rs:82` / `Transaction` `packages/sz-orm-core/src/transaction.rs:159` / `TransactionManager` `packages/sz-orm-core/src/transaction.rs:527` / `retry_on_deadlock` `packages/sz-orm-core/src/transaction.rs:466` / `DbType` `packages/sz-orm-core/src/db_type.rs:11` |
| REQ-V45-003 | P1 | 异步流式结果集 | 9 | `stream-resultset` | `build_paged_query` `packages/sz-orm-core/src/cursor_stream.rs:29` / `stream_cursor_paged` `packages/sz-orm-core/src/cursor_stream.rs:79` / `stream_cursor` `packages/sz-orm-core/src/stream_api.rs:176` / `StreamApiExt` `packages/sz-orm-core/src/stream_api.rs:50` / `Paginator` `packages/sz-orm-core/src/paginator.rs:158` / `BackpressureController` `packages/sz-orm-batch/src/stream.rs:40` / `StreamConfig` `packages/sz-orm-batch/src/stream.rs:9` / `Pool` `packages/sz-orm-core/src/pool.rs:743` / `DbType` `packages/sz-orm-core/src/db_type.rs:11` |

---

# 8. 验收标准总览

## 8.1 REQ-V45-001 并行查询执行器（P1）

1. `ParallelQueryScheduler` 将多个独立查询并行执行，并发度控制避免连接池耗尽（默认池 max_size 80%）
2. 复用既有 `Pool` + `Connection` trait + tokio 异步运行时，不新建连接池
3. 与既有 `AdaptiveExecutor` 协同，单查询仍走自适应路径，不修改既有自适应决策
4. `ResultMerger` 支持四种合并策略（First/Union/Join/Map）
5. 整体超时与单查询超时控制，超时取消未完成查询释放连接
6. 单查询失败降级（Skip/Abort/Fallback），不 panic
7. `parallel-query` feature gate 隔离，默认关闭

## 8.2 REQ-V45-002 批量 INSERT/UPDATE/DELETE 优化（P1）

1. `batch_delete` 通过 IN 子句批量删除，复用既有分片逻辑，参数化绑定
2. `BatchExecutor` 异步执行器通过 `execute_with_params` 真正执行批量 SQL
3. 五方言批量 SQL 生成（MySQL/PostgreSQL/SQLite/Oracle/MSSQL），复用 `DbType`
4. PostgreSQL COPY 协议（可选），其他方言降级为多值 INSERT
5. 事务边界 + 部分失败回滚（复用 `Transaction` + `RollbackStrategy`）
6. 复用既有分片与进度回调（`chunk_indices` + `ProgressCallback`）
7. 批量 DELETE 范围保护（拒绝空行/无条件删除）
8. `batch-v2` feature gate 隔离，默认关闭，既有 `BatchOperations` trait 保留

## 8.3 REQ-V45-003 异步流式结果集（P1）

1. `StreamResultSet` 实现异步 Stream trait 逐批 yield，避免一次性加载全量
2. `KeysetPaginator` keyset pagination（`WHERE key > last_key`，深翻页高效）
3. 三种分页策略可选（Keyset/LimitOffset/ServerCursor），默认 LimitOffset
4. 背压控制与异步 Stream 集成（复用 `BackpressureController`，暂停生产者避免积压）
5. 可配置批次大小控制内存（默认 1000，复用 `StreamConfig.batch_size`）
6. 连接池集成（每批从池获取连接，批次完成归还，复用 `Pool`）
7. 复用既有 `build_paged_query` 五方言分页 SQL 包装
8. 游标资源释放（消费完成或提前 drop 释放游标 + 归还连接）
9. `stream-resultset` feature gate 隔离，默认关闭

---

# 9. feature gate 总览

| feature gate | 所属包 | 控制能力 | 默认 | 对应需求 |
|-------------|--------|---------|------|---------|
| `parallel-query` | sz-orm-parallel（新包）+ sz-orm-core + sz-orm-adaptive（只读复用） | 并行查询执行器（调度器 + 合并器 + 超时降级） | 关闭 | REQ-V45-001 |
| `batch-v2` | sz-orm-batch（扩展）+ sz-orm-core（只读复用 Connection/Transaction/DbType） | 批量 INSERT/UPDATE/DELETE 优化（DELETE + 异步执行 + 五方言 + 事务 + COPY） | 关闭 | REQ-V45-002 |
| `stream-resultset` | sz-orm-stream（新包）+ sz-orm-core（只读复用 cursor_stream/stream_api/paginator/Pool）+ sz-orm-batch（只读复用 BackpressureController/StreamConfig） | 异步流式结果集（keyset + 背压 Stream + 内存控制 + 连接池集成） | 关闭 | REQ-V45-003 |

---

# 10. 与 v4.4.0 的关系

## 10.1 零重叠声明

v4.5.0 与 v4.4.0 零重叠：

| v4.4.0 能力（分析/建议/转正层） | v4.5.0 能力（执行优化层） | 关系 |
|-------------------------------|-------------------------|------|
| 查询自动优化建议（`sz-orm-advisor`） | 并行查询执行器（`sz-orm-parallel`） | v4.5.0 复用 v4.4.0 优化建议可选联动（并行查询结果可触发建议生成），不重复实现建议 |
| 慢查询自动诊断（`sz-orm-diagnosis`） | 并行查询执行器 / 批量操作 / 流式结果集 | v4.5.0 执行优化可被 v4.4.0 诊断观测（慢查询诊断可标注并行/批量/流式执行），不重复实现诊断 |
| db-fusion 转正（`sz-orm-fusion`） | 并行查询执行器 | v4.5.0 并行查询可并行执行融合查询，不修改既有融合逻辑 |
| 结构化查询日志（`sz-orm-observability`） | 并行查询 / 批量操作 / 流式结果集 | v4.5.0 执行优化可被 v4.4.0 日志观测，不重复实现日志 |
| 性能回归基准线（`sz-orm-explain`） | 并行查询 / 批量操作 / 流式结果集 | v4.5.0 执行优化性能可被 v4.4.0 基线比对，不重复实现基线 |
| 查询智能闭环联动（`sz-orm-advisor`） | 并行查询执行器 | v4.5.0 并行查询可接入闭环（可选），不修改既有闭环 |

## 10.2 依赖关系

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

## 10.3 新增包

| 包名 | 对应需求 | 依赖 | 说明 |
|------|---------|------|------|
| `sz-orm-parallel` | REQ-V45-001 | sz-orm-core（Pool/Connection 只读复用）+ sz-orm-adaptive（AdaptiveExecutor 只读复用）+ tokio | 并行查询执行器（调度器 + 合并器 + 超时降级） |
| `sz-orm-stream` | REQ-V45-003 | sz-orm-core（cursor_stream/stream_api/paginator/Pool 只读复用）+ sz-orm-batch（BackpressureController/StreamConfig 只读复用） | 异步流式结果集（keyset + 背压 Stream + 内存控制） |

## 10.4 扩展包

| 包名 | 对应需求 | 扩展内容 |
|------|---------|---------|
| `sz-orm-batch` | REQ-V45-002 | 批量 DELETE + 异步批量执行器 + 五方言批量 SQL + 事务边界 + PostgreSQL COPY 协议（`batch-v2` feature） |

---

> 文档生成依据：`docs/spec/v4.4.0/` SDD 三阶段文档（spec 870 行 / design 1801 行 / tasks 184 子任务，v4.4.0 已全部完成）+ 2026-08-12 逐项代码验证（所有 file:line 证据均已实测存在）
> 审计合规：本文档所有 file:line 证据均引用真实存在的代码，遵循 AGENTS.md 审计合规铁律
> 下一阶段：spec-design-agent 生成 `design.md`（技术设计），spec-task-agent 生成 `tasks.md`（编码任务规划）