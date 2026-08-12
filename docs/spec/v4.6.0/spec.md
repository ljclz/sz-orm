# sz-orm v4.6.0 需求规格说明书

> 版本：v4.6.0（消息死信队列自动重投递 + 迁移回滚自动化 + 批量事务原子性保证 + 异常检测 + 存储成本分析 + 连接级多租户隔离 + 进程级 L1 缓存）
> 基线：v4.5.0（并行查询执行器 + 批量 INSERT/UPDATE/DELETE 优化 + 异步流式结果集，3 项需求 REQ-V45-001~003 全部通过 feature gate 隔离，已验收基线）
> 日期：2026-08-12
> 需求格式：EARS（Ubiquitous / Event-driven / State-driven / Optional / Unwanted）
> 文档定位：需求规格（What to build），不含技术设计（How to build）
> 优先级声明：7 项需求中 5 项 P1（REQ-V46-001 DLX 自动重投递 / REQ-V46-002 零停机回滚 / REQ-V46-003 批量事务原子性 / REQ-V46-006 连接级多租户 / REQ-V46-007 进程级 L1 缓存）+ 2 项 P2（REQ-V46-004 异常检测 / REQ-V46-005 存储成本分析），按"REQ-V46-001 DLX → REQ-V46-002 零停机回滚 → REQ-V46-003 批量原子性 → REQ-V46-006 连接级多租户 → REQ-V46-007 进程级 L1 → REQ-V46-004 异常检测 → REQ-V46-005 成本分析"序推进，7 项无强依赖可并行开发
> 需求编号约定：REQ-V46-xxx（v4.6.0 需求项，REQ-V46-001 ~ REQ-V46-007）
> 规划依据：`docs/spec/v4.5.0/` SDD 三阶段文档（spec 688 行 / design 98714 字节 / tasks 96260 字节，v4.5.0 已全部完成）+ 2026-08-12 逐项代码验证（file:line 均已实测存在）+ 对比分析文档 5.2/5.4 节 16 项缺失能力筛选
> 兼容性铁律：所有新能力通过 feature gate 隔离，默认 feature 行为不变，既有公开 API 完全向后兼容，v4.5.0 已验收测试基线不回退；sz-pay 生产依赖（从 crates.io 拉取 sz-orm-* 6 个包）不得被破坏；五方言覆盖：MySQL/PostgreSQL/SQLite/Oracle/MSSQL
> 范围声明：本版本聚焦可靠性增强与运维智能化（消息可靠投递 + 迁移安全回滚 + 批量原子性 + 异常自检 + 成本自优 + 多租户隔离 + 缓存升级）；更长期（v4.x+ 跨语言分布式事务/低代码双向同步/OpenAPI 反向生成/WASM 真实连接）在后续版本规划；本版本不涉及 crates.io 发布流程变更
> 边界声明：与 v4.5.0 零重叠（见第 1.4 节），v4.5.0 是"执行优化"层（并行执行/批量执行/流式执行），v4.6.0 是"可靠性 + 运维智能化"层（消息可靠/迁移安全/批量原子/异常自检/成本自优/租户隔离/缓存升级）

---

# 1. 组件定位

## 1.1 核心职责

本组件负责交付 sz-orm v4.6.0 的七项可靠性增强与运维智能化能力：(1) 消息死信队列自动重投递（扩展既有 `sz-orm-queue` 包 `packages/sz-orm-queue/src/queue.rs:18` `MessageQueue` trait + `:339` `InMemoryQueue` + `:364` `dead_letters` + `:484` `requeue_dead_letter`，补齐自动重投递调度器 + 退避策略 + 定时重投递 + DLX 路由策略，复用既有 `nack` `:37` / `reject` `:44` / `max_retries` `:366`）；(2) 迁移回滚自动化（扩展既有 `sz-orm-core` 迁移管理 `packages/sz-orm-core/src/migration.rs:10` `Migration` + `:587` `rollback` + `:677` `down`，补齐零停机回滚策略 + 健康检查触发 + 回滚窗口 + 回滚验证，复用既有 `MigrationResolver` `:62` / `FileMigrationResolver` `:68` / `MigrationContext` `:193`）；(3) 批量事务原子性保证（扩展既有 `sz-orm-batch` 包 `packages/sz-orm-batch/src/executor.rs:18` `BatchExecutorConfig` + `:93` `BatchExecutionResult`，复用既有 `sz-orm-dtx` 包 `packages/sz-orm-dtx/src/saga.rs:377` `Saga` + `:255` `SagaStep` + `packages/sz-orm-dtx/src/lib.rs:270` `DistributedTransaction`，补齐 all-or-nothing 原子语义 + 跨批次原子提交 + Saga 补偿）；(4) 异常检测（扩展既有 `sz-orm-observability` 包 `packages/sz-orm-observability/src/lib.rs:259` `MetricsRegistry` + `packages/sz-orm-observability/src/slo.rs:223` `SloMonitor`，补齐异常检测算法 + 告警联动 + 异常标注，复用既有 `QueryLogger` `packages/sz-orm-observability/src/query_logger.rs:73`）；(5) 存储成本分析与优化建议（扩展既有 `sz-orm-storage` 包 `packages/sz-orm-storage/src/storage.rs:14` `Storage` trait + `packages/sz-orm-storage/src/advanced.rs:438` `BucketLifecycle`，补齐成本分析 + 成本优化建议 + 成本报表，复用既有 `LifecycleRule` `:400` / `LifecycleAction` `:378`）；(6) 连接级多租户隔离（扩展既有 `sz-orm-core` 连接池 `packages/sz-orm-core/src/pool.rs:743` `Pool` + `:45` `Connection` trait，复用既有 `TenantContext` `packages/sz-orm-core/src/tenant_context.rs:80` + `IsolationStrategy` `:22` + `TenantPoolRegistry` `:224`，补齐连接绑定租户 + 连接亲和性 + 连接级隔离，避免每租户独立池的资源开销）；(7) 进程级 L1 缓存（扩展既有 `sz-orm-core` 缓存 `packages/sz-orm-core/src/l1_cache.rs:87` `L1Cache` + `packages/sz-orm-core/src/l2_cache.rs:517` `L2Cache`，复用既有 `CacheKey` `packages/sz-orm-core/src/l2_cache.rs:143` + `CacheCoherenceProtocol` `packages/sz-orm-core/src/cache_coherence.rs:103`，补齐进程级 L1 缓存 + 跨 Session Identity Map + 线程安全）。所有能力通过 feature gate 隔离，不破坏现有 API 兼容性与 v4.5.0 已验收基线。

## 1.2 核心输入

1. **v4.5.0 已验收基线**：并行查询执行器 + 批量 INSERT/UPDATE/DELETE 优化 + 异步流式结果集，3 项能力全部通过 feature gate 隔离，作为本版本基准。
2. **现有能力清单与缺口证据**：
   - **消息死信队列**：`packages/sz-orm-queue/src/queue.rs:18` `pub trait MessageQueue`（publish/consume/ack/subscribe/nack/reject，nack/reject 带默认实现返回 NotSupported）、`:57` `pub struct Message`（topic/payload/key/timestamp/headers/id/retry_count）、`:339` `pub struct InMemoryQueue`（内置死信队列）、`:364` `dead_letters: HashMap<String, VecDeque<Message>>`（死信存储）、`:366` `max_retries: u32`（最大重试次数）、`:377` `DEFAULT_MAX_RETRIES: u32 = 3`、`:37` `nack`（重入队列尾部，retry_count + 1，达 max_retries 转死信）、`:44` `reject`（直接进死信）、`:484` `requeue_dead_letter`（**手动**将死信放回原队列并重置 retry_count）、`:463` `dead_letter_count` / `:472` `consume_dead_letter`。缺口：`requeue_dead_letter` 是**手动**调用，无自动重投递调度器，无退避策略（exponential backoff），无定时重投递（scheduled redelivery），无 DLX 路由策略（死信转发到另一个 topic/queue）。
   - **迁移管理**：`packages/sz-orm-core/src/migration.rs:10` `pub struct Migration`（version/name/sql_up/sql_down/batch/executed_at）、`:18` `pub sql_down: String`（反向 SQL）、`:62` `pub trait MigrationResolver`（resolve 返回迁移列表）、`:68` `pub struct FileMigrationResolver`（文件迁移解析器）、`:193` `pub struct MigrationContext`（迁移上下文）、`:587` `pub async fn rollback`（回滚指定版本，执行 down SQL）、`:677` `pub async fn down`（回滚到指定版本，执行该版本之后所有迁移的 down SQL）、`:747` `pub struct MigrationProgress`。缺口：无零停机回滚策略（shadow table / reverse migration / blue-green rollback），无健康检查触发自动回滚（部署后健康检查失败自动回滚），无回滚窗口（rollback window，超时后不可回滚），无回滚验证（回滚后数据一致性校验）。
   - **批量执行器**：`packages/sz-orm-batch/src/executor.rs:18` `pub struct BatchExecutorConfig`（chunk_size/rollback_strategy/progress_callback/use_copy_protocol）、`:68` `pub enum BatchExecutorError`（ExecutionFailed/AllChunksFailed/TransactionRolledBack）、`:93` `pub struct BatchExecutionResult`（base/chunk_results/in_transaction/rolled_back）、`:106` `pub struct ChunkExecutionDetail`（分片执行详情）、`:163` `execute_batch_insert` / `:250` `execute_batch_update` / `:320` `execute_batch_delete` / `:373` `execute_batch_upsert`、`packages/sz-orm-batch/src/lib.rs:518` `pub enum RollbackStrategy`（None/Savepoint/PerChunk）、`packages/sz-orm-dtx/src/saga.rs:377` `pub struct Saga`（Saga 模式，步骤 + 补偿）、`:255` `pub struct SagaStep`（Saga 步骤，action + compensation）、`:507` `pub fn execute`（Saga 执行）、`packages/sz-orm-dtx/src/lib.rs:270` `pub struct DistributedTransaction`（分布式事务 2PC）、`:334` `prepare` / `:372` `commit` / `:407` `rollback`。缺口：`RollbackStrategy::None` 允许部分成功（非 all-or-nothing），无跨批次原子提交（多批次作为一个原子事务），无 Saga 补偿模式批量操作（失败时按 Saga 补偿回滚已成功批次），无 2PC 协调批量提交（多连接批量操作原子提交）。
   - **可观测性**：`packages/sz-orm-observability/src/lib.rs:259` `pub struct MetricsRegistry`（Counter/Gauge/Histogram 注册中心）、`:75` `pub enum MetricKind`（Counter/Gauge/Histogram）、`:284` `register_counter` / `:317` `register_gauge` / `:350` `register_histogram`、`packages/sz-orm-observability/src/slo.rs:223` `pub struct SloMonitor`（SLO 燃烧率监控）、`:52` `pub struct SloConfig`、`:98` `pub struct SloBurnRate`、`packages/sz-orm-observability/src/query_logger.rs:73` `pub struct QueryLogger`（结构化查询日志）、`:46` `pub struct QueryLogEntry`、`:35` `pub enum LogLevel`。缺口：无异常检测算法（statistical anomaly / threshold / trend），无告警联动（检测到异常后触发告警），无异常标注（在指标/日志上标注异常）。
   - **存储**：`packages/sz-orm-storage/src/storage.rs:14` `pub trait Storage`（put/get/delete/list 统一抽象）、`:22` `pub struct StorageBuilder`（构建器）、`:287` `pub enum StorageProvider`（Local/S3/AliyunOss/TencentCos/HuaweiObs/Upyun/QiniuKodo 7 provider）、`packages/sz-orm-storage/src/advanced.rs:438` `pub struct BucketLifecycle`（生命周期管理）、`:400` `pub struct LifecycleRule`（生命周期规则）、`:378` `pub enum LifecycleAction`（生命周期动作）、`:64` `pub struct MultipartUpload`（分片上传）、`:219` `pub struct ResumableUploadManager`（断点续传）、`:622` `pub struct CdnRefresher`（CDN 刷新）。缺口：无成本分析（按 provider / bucket / tier 统计存储成本），无成本优化建议（冷数据降级、生命周期优化、低频访问数据降级到低成本 tier），无成本报表（周期性成本报表生成）。
   - **多租户**：`packages/sz-orm-core/src/pool.rs:743` `pub struct Pool`（自研连接池）、`:45` `pub trait Connection`（execute/query/begin_transaction/commit/rollback）、`:239` `pub struct PooledConnection`（池化连接）、`:1268` `pub async fn acquire`（获取连接）、`packages/sz-orm-core/src/tenant_context.rs:80` `pub struct TenantContext`（租户上下文）、`:22` `pub enum IsolationStrategy`（RowLevel/SchemaIsolation）、`:166` `pub struct TenantContextGuard`（RAII 守卫）、`:194` `pub struct SchemaIsolationRouter`（Schema 隔离路由器）、`:224` `pub struct TenantPoolRegistry`（**按 tenant_id 维护独立 Pool**）、`packages/sz-orm-core/src/tenant_security.rs:67` `pub struct RowLevelSecurityPolicy`（行级安全策略）、`:155` `pub struct ColumnMaskingRule`（列级脱敏规则）。缺口：`TenantPoolRegistry` 是**每租户独立池**（资源开销大，N 个租户 = N 个池），无连接级隔离（同一连接池中连接绑定到特定租户，通过 `SET app.tenant_id = ?` 或类似机制），无连接亲和性（同一租户请求复用绑定到该租户的连接）。
   - **L1 缓存**：`packages/sz-orm-core/src/l1_cache.rs:87` `pub struct L1Cache<T>`（**Session 级** Identity Map + LRU + AtomicU64 统计）、`:47` `pub struct L1CacheStats`（hits/misses/entry_count/evict_count）、`:106` `pub fn new` / `:126` `pub fn put` / `:149` `pub fn get`、`:84` **非 `Send + Sync`，不跨线程共享**（Session 内使用）、`packages/sz-orm-core/src/l2_cache.rs:517` `pub struct L2Cache`（**进程级**跨 Session 共享）、`:143` `pub struct CacheKey`（统一缓存键）、`:740` `pub fn invalidate_table`（表级失效）、`packages/sz-orm-core/src/cache_coherence.rs:103` `pub struct CacheCoherenceProtocol`（缓存一致性协议）、`:12` `pub enum MesiState`（MESI 状态机）、`:25` `pub enum ConsistencyStrategy`（一致性策略）。缺口：L1 是 **Session 级**（非 Send + Sync，不跨 Session 共享），无进程级 L1（跨 Session 共享 Identity Map，线程安全），L1 与 L2 职责重叠（进程级 L1 可作为 L2 的 Identity Map 层，减少 L2 锁竞争）。
3. **本机数据库连接信息**：MySQL 9.6（`mysql://root:test123@127.0.0.1:3306/sz_orm_test`）、PostgreSQL 18（`postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`）、Oracle 23ai Free（`127.0.0.1:1521/freepdb1`）。
4. **sz-pay 生产依赖证据**：sz-pay 从 crates.io 拉取 sz-orm-* 6 个包，作为 API 兼容性验证的下游基准。
5. **五方言覆盖约束**：MySQL/PostgreSQL/SQLite/Oracle/MSSQL，迁移回滚/批量原子性/连接级多租户须覆盖全部方言（按方言能力适配，如 `SET app.tenant_id` 仅 PostgreSQL/MySQL 支持，其他方言降级为 Schema 隔离）。
6. **既有 feature gate 体系**：`packages/sz-orm-core/Cargo.toml` 已有 40+ feature（含 `multi-tenant-enhanced` `:34` / `l1-cache` `:64` / `migration-branch` `:128` / `migration-dry-run` `:120` / `cache-coherence` `:126` / `dist-cache` `:36`），`packages/sz-orm-batch/Cargo.toml` 已有 `batch-v2` `:25`，`packages/sz-orm-queue/Cargo.toml` 已有 `cdc` `:34` / `message-tracing` `:36`，`packages/sz-orm-observability/Cargo.toml` 已有 `query-logging` `:26` / `service-mesh` `:24`，`packages/sz-orm-storage/Cargo.toml` 已有 `storage-lifecycle` `:17` / `real-cloud` `:20`，作为新能力 feature gate 隔离的基础。

## 1.3 核心输出

1. **消息死信队列自动重投递**：sz-orm-queue 扩展（`DlxConfig` DLX 配置 + `RedeliveryStrategy` 重投递策略 + `BackoffPolicy` 退避策略 + `RedeliveryScheduler` 自动重投递调度器 + `DlxEntry` 死信条目 + `RedeliveryOutcome` 重投递结果）。
2. **迁移回滚自动化**：sz-orm-core 扩展（`ZeroDowntimeRollbackStrategy` 零停机回滚策略 + `RollbackPlan` 回滚计划 + `RollbackWindow` 回滚窗口 + `HealthCheck` 健康检查 + `AutoRollbackTrigger` 自动回滚触发器 + `RollbackExecutor` 回滚执行器）。
3. **批量事务原子性保证**：sz-orm-batch 扩展（`AtomicityGuarantee` 原子性保证枚举 + `BatchTransactionCoordinator` 批量事务协调器 + `SagaCompensator` Saga 补偿器 + `BatchAtomicConfig` 原子配置，复用 sz-orm-dtx Saga/2PC）。
4. **异常检测**：sz-orm-observability 扩展（`AnomalyDetector` 异常检测器 + `AnomalyAlgorithm` 异常检测算法枚举 + `AnomalyAlert` 异常告警 + `AnomalyConfig` 异常配置 + `ThresholdDetector` 阈值检测器 + `TrendDetector` 趋势检测器 + `StatisticalDetector` 统计检测器）。
5. **存储成本分析与优化建议**：sz-orm-storage 扩展（`CostAnalyzer` 成本分析器 + `CostReport` 成本报表 + `CostOptimizationSuggestion` 成本优化建议 + `StorageTiering` 存储分层 + `CostConfig` 成本配置）。
6. **连接级多租户隔离**：sz-orm-core 扩展（`ConnectionTenantBinder` 连接租户绑定器 + `TenantConnectionGuard` 租户连接守卫 + `ConnectionAffinityPolicy` 连接亲和策略 + `ConnectionLevelIsolation` 连接级隔离枚举）。
7. **进程级 L1 缓存**：sz-orm-core 扩展（`ProcessL1Cache` 进程级 L1 缓存 + `ProcessL1Config` 进程级 L1 配置 + `CrossSessionIdentityMap` 跨 Session Identity Map）。
8. **需求追溯矩阵**：本文档第 7 章，建立需求 ↔ 验收条件映射。
9. **验收标准总览**：本文档第 8 章，按需求项汇总验收条件。

## 1.4 职责边界

本组件**不负责**以下事项：

1. **不破坏既有公开 API**：所有新能力通过 feature gate 隔离，既有公开 API 签名保持完全向后兼容。既有 `MessageQueue` trait（`packages/sz-orm-queue/src/queue.rs:18`）保留不动，新增 DLX 自动重投递为扩展 API。
2. **不改变既有安全铁律**：任何 WHERE 条件必须参数化，默认禁止 `SELECT *`，N+1 检测自动拦截，多租户隔离须防止租户越权，沿用既有铁律。
3. **不重写消息队列核心**：既有 `MessageQueue` trait（`packages/sz-orm-queue/src/queue.rs:18`）/ `InMemoryQueue`（`:339`）/ `Message`（`:57`）/ `nack`（`:37`）/ `reject`（`:44`）/ `requeue_dead_letter`（`:484`）保留不动，DLX 自动重投递为扩展，不修改既有死信队列逻辑。
4. **不重写迁移核心**：既有 `Migration`（`packages/sz-orm-core/src/migration.rs:10`）/ `rollback`（`:587`）/ `down`（`:677`）/ `MigrationResolver`（`:62`）/ `FileMigrationResolver`（`:68`）/ `MigrationContext`（`:193`）保留不动，零停机回滚为扩展，不修改既有回滚逻辑。
5. **不重写批量执行器**：既有 `BatchExecutor`（`packages/sz-orm-batch/src/executor.rs`）/ `BatchExecutorConfig`（`:18`）/ `BatchExecutionResult`（`:93`）/ `RollbackStrategy`（`packages/sz-orm-batch/src/lib.rs:518`）保留不动，批量事务原子性为扩展，不修改既有批量执行逻辑。
6. **不重写分布式事务**：既有 `Saga`（`packages/sz-orm-dtx/src/saga.rs:377`）/ `SagaStep`（`:255`）/ `DistributedTransaction`（`packages/sz-orm-dtx/src/lib.rs:270`）保留不动，批量事务原子性复用 sz-orm-dtx Saga/2PC，不修改既有分布式事务逻辑。
7. **不重写可观测性核心**：既有 `MetricsRegistry`（`packages/sz-orm-observability/src/lib.rs:259`）/ `SloMonitor`（`packages/sz-orm-observability/src/slo.rs:223`）/ `QueryLogger`（`packages/sz-orm-observability/src/query_logger.rs:73`）保留不动，异常检测为扩展，不修改既有指标/日志逻辑。
8. **不重写存储核心**：既有 `Storage` trait（`packages/sz-orm-storage/src/storage.rs:14`）/ `StorageBuilder`（`:22`）/ `BucketLifecycle`（`packages/sz-orm-storage/src/advanced.rs:438`）/ `LifecycleRule`（`:400`）保留不动，成本分析为扩展，不修改既有存储/生命周期逻辑。
9. **不重写连接池**：既有 `Pool`（`packages/sz-orm-core/src/pool.rs:743`）/ `Connection` trait（`:45`）/ `PooledConnection`（`:239`）保留不动，连接级多租户隔离复用既有连接池，不新建连接池。
10. **不重写多租户核心**：既有 `TenantContext`（`packages/sz-orm-core/src/tenant_context.rs:80`）/ `IsolationStrategy`（`:22`）/ `TenantPoolRegistry`（`:224`）/ `TenantContextGuard`（`:166`）保留不动，连接级多租户隔离为扩展，不修改既有租户上下文逻辑。
11. **不重写 L1 缓存核心**：既有 `L1Cache`（`packages/sz-orm-core/src/l1_cache.rs:87`）/ `L1CacheStats`（`:47`）保留不动，进程级 L1 缓存为新增 API，不修改既有 Session 级 L1 逻辑。
12. **不重写 L2 缓存核心**：既有 `L2Cache`（`packages/sz-orm-core/src/l2_cache.rs:517`）/ `CacheKey`（`:143`）/ `CacheCoherenceProtocol`（`packages/sz-orm-core/src/cache_coherence.rs:103`）保留不动，进程级 L1 缓存与 L2 协同，不修改既有 L2 逻辑。
13. **不与 v4.5.0 任务重叠**：v4.5.0 已占用的包/模块（`sz-orm-parallel` 并行查询 / `sz-orm-stream` 流式结果集 / `sz-orm-batch` batch-v2 批量执行器）本版本不触碰其新增逻辑，新增范围全部落在既有包扩展（sz-orm-queue / sz-orm-core / sz-orm-batch / sz-orm-observability / sz-orm-storage）。
14. **不负责 sz-pay / sz-rust 下游代码修改**：ADR-0001 严禁修改下游/上游仓库，仅保证 API 兼容性。
15. **不降低既有测试覆盖**：v4.6.0 不得使 v4.5.0 已验收测试基线回退，仅增不减。
16. **不引入 unsafe**：所有新增代码无 `unsafe` 块，或必须有 `// SAFETY:` 注释，沿用既有 unsafe 零容忍铁律。
17. **不引入 Breaking Change**：新能力通过 feature gate 隔离，默认全关闭，既有 feature 组合行为不变。
18. **不强制启用新能力**：所有新能力默认关闭或可选启用，避免无配置环境行为变化。
19. **不做跨语言分布式事务**：跨语言（Java/Go/C++）互操作的分布式事务需跨语言协议适配，依赖 sz-orm-dtx 的 cross_lang 模块成熟度，排除出本版本范围，本版本批量事务原子性仅限 Rust 内多连接原子提交。
20. **不做低代码双向同步**：低代码 ↔ 代码双向同步需可视化设计器与代码生成器深度集成，无既有代码基础，排除出本版本范围。
21. **不做 OpenAPI 反向生成**：OpenAPI → ORM 反向生成需 OpenAPI 规范解析与 ORM 模型生成，无既有代码基础，排除出本版本范围。
22. **不做 WASM 真实数据库连接**：WASM 真实数据库连接需 WASM 兼容的数据库驱动，无既有代码基础，排除出本版本范围。
23. **不做 Informix/SAP HANA/Firebird 真实驱动**：需集成第三方 C 库驱动，违反"不需要外部 C 库或第三方驱动依赖"约束，排除出本版本范围。

---

# 2. 领域术语

**消息死信队列自动重投递（DLX Auto Redelivery）**

: 扩展既有 `MessageQueue` trait（`packages/sz-orm-queue/src/queue.rs:18`）的死信队列能力，当消息通过 `nack`（`:37`）达到 `max_retries`（`:366`）或 `reject`（`:44`）进入死信队列后，按退避策略自动调度重投递（无需手动调用 `requeue_dead_letter` `:484`），支持 DLX 路由（死信转发到另一个 topic/queue）。
: 备注：既有 `requeue_dead_letter` 是手动调用，本版本补自动重投递调度器。

**退避策略（Backoff Policy）**

: 死信消息重投递的退避策略，`BackoffPolicy` 枚举（Fixed 固定间隔 / Exponential 指数退避 / Linear 线性退避 / RandomJitter 随机抖动），避免立即重投递导致再次失败。

**DLX 路由策略（DLX Routing Strategy）**

: 死信消息的路由策略，`DlxRoutingStrategy` 枚举（RequeueToOriginal 放回原队列 / ForwardToDlxTopic 转发到死信 topic / ForwardToDlxQueue 转发到死信 queue / Drop 丢弃），可配。

**零停机回滚（Zero-Downtime Rollback）**

: 扩展既有迁移回滚（`packages/sz-orm-core/src/migration.rs:587` `rollback` / `:677` `down`），通过 shadow table（影子表）或 reverse migration（反向迁移）实现零停机回滚，回滚过程中服务不中断，回滚完成后切换流量。

**回滚窗口（Rollback Window）**

: 部署后可自动回滚的时间窗口，超时后不可自动回滚（需手动回滚），避免长时间后回滚导致数据丢失，可配（默认 5 分钟）。

**健康检查触发自动回滚（Health Check Triggered Auto Rollback）**

: 部署后持续健康检查，若健康检查失败（如错误率超阈值、响应时间超阈值），在回滚窗口内自动触发回滚，无需人工干预。

**批量事务原子性保证（Batch Transaction Atomicity Guarantee）**

: 扩展既有批量执行器（`packages/sz-orm-batch/src/executor.rs`），提供 all-or-nothing 原子语义（全成功提交 / 全失败回滚），复用既有 `sz-orm-dtx` 包 `Saga`（`packages/sz-orm-dtx/src/saga.rs:377`）补偿模式与 `DistributedTransaction`（`packages/sz-orm-dtx/src/lib.rs:270`）2PC 协调，确保跨批次操作的原子性。

**Saga 补偿模式批量操作（Saga Compensation Batch）**

: 批量操作按 Saga 模式执行，每个批次作为一个 Saga 步骤（`SagaStep` `packages/sz-orm-dtx/src/saga.rs:255`），失败时按 Saga 补偿回滚已成功批次，实现最终一致性。

**异常检测（Anomaly Detection）**

: 扩展既有可观测性（`packages/sz-orm-observability/src/lib.rs:259` `MetricsRegistry`），基于指标历史数据检测异常（statistical anomaly 统计异常 / threshold 阈值 / trend 趋势），检测到异常后触发告警并在指标/日志上标注异常。

**异常检测算法（Anomaly Detection Algorithm）**

: `AnomalyAlgorithm` 枚举（Threshold 阈值检测 / Trend 趋势检测 / Statistical 统计检测 / ZScore Z-Score 检测 / IQR 四分位距检测），可配，按算法检测指标异常。

**存储成本分析（Storage Cost Analysis）**

: 扩展既有存储（`packages/sz-orm-storage/src/storage.rs:14` `Storage` trait），按 provider / bucket / tier 统计存储成本（容量成本 + 请求成本 + 流量成本），生成成本报表，提供成本优化建议（冷数据降级、生命周期优化、低频访问数据降级到低成本 tier）。

**成本优化建议（Cost Optimization Suggestion）**

: 基于成本分析结果生成的优化建议，`CostOptimizationSuggestion` 枚举（TierDowngrade 存储层降级 / LifecycleOptimize 生命周期优化 / DeleteExpired 删除过期数据 / CompressCold 冷数据压缩），可配。

**连接级多租户隔离（Connection-Level Multi-Tenant Isolation）**

: 扩展既有连接池（`packages/sz-orm-core/src/pool.rs:743` `Pool`）与多租户（`packages/sz-orm-core/src/tenant_context.rs:80` `TenantContext`），在同一连接池中连接绑定到特定租户（通过 `SET app.tenant_id = ?` 或类似机制），避免 `TenantPoolRegistry`（`:224`）每租户独立池的资源开销，支持连接亲和性（同一租户请求复用绑定到该租户的连接）。

**连接亲和性（Connection Affinity）**

: 同一租户的请求优先复用绑定到该租户的连接，减少连接上的租户上下文切换开销，`ConnectionAffinityPolicy` 枚举（Strict 严格亲和 / Preferred 优先亲和 / None 无亲和），可配。

**进程级 L1 缓存（Process-Level L1 Cache）**

: 扩展既有 L1 缓存（`packages/sz-orm-core/src/l1_cache.rs:87` `L1Cache`，Session 级），升级为进程级（跨 Session 共享 Identity Map，线程安全 `Send + Sync`），与既有 L2 缓存（`packages/sz-orm-core/src/l2_cache.rs:517` `L2Cache`）协同，减少 L2 锁竞争。

**跨 Session Identity Map（Cross-Session Identity Map）**

: 进程级 L1 缓存提供的跨 Session Identity Map 语义，相同主键的多次查询（跨 Session）返回相同 `Arc<T>` 引用（`Arc::ptr_eq` 为 true），线程安全。

**v4.6.0 feature gate**

: 控制本版本新能力的 feature gate 集合（`dlx-auto-redelivery` / `zero-downtime-rollback` / `batch-atomic` / `anomaly-detection` / `cost-analysis` / `connection-level-tenant` / `process-l1-cache`），默认关闭，避免无配置环境行为变化。

---

# 3. 角色与边界

## 3.1 核心角色

- **ORM 库维护者**：执行 v4.6.0 七项可靠性增强与运维智能化能力的开发、验证、测试操作者，是新增能力的主要使用者与验收人。
- **下游项目开发者（sz-pay）**：关注 API 兼容性的下游使用者，v4.6.0 不得破坏其既有代码。
- **应用开发者**：使用 DLX 自动重投递保证消息可靠投递，使用零停机回滚保证迁移安全，使用批量事务原子性保证数据一致性，使用异常检测监控业务异常，使用成本分析优化存储成本，使用连接级多租户隔离多租户数据，使用进程级 L1 缓存提升查询性能。
- **DBA / 性能工程师**：评估零停机回滚对数据库的影响，评估批量事务原子性对数据库事务负载的影响，评估连接级多租户对连接池的影响，评估进程级 L1 缓存对内存与命中率的影响。
- **运维/SRE 工程师**：配置 DLX 自动重投递策略与退避策略，配置零停机回滚窗口与健康检查，配置异常检测算法与告警阈值，配置成本分析周期与优化建议，监控可靠性增强能力对系统的影响。
- **FinOps / 成本工程师**：使用存储成本分析评估存储成本，使用成本优化建议降低存储成本，监控成本报表。

## 3.2 外部系统

- **MySQL 9.6 / PostgreSQL 18 / SQLite / Oracle 23ai / MSSQL**：迁移回滚/批量原子性/连接级多租户的五方言覆盖目标，PostgreSQL/MySQL 额外支持 `SET app.tenant_id` 连接级隔离。
- **DBRD**：批量事务原子性通过既有连接池 `Pool`（`packages/sz-orm-core/src/pool.rs:743`）获取连接执行 SQL，连接级多租户在连接上设置租户上下文。
- **消息队列 provider**：DLX 自动重投递复用既有 6 provider（RabbitMQ/Kafka/NATS/ActiveMQ/Pulsar/RocketMQ）。
- **对象存储 provider**：成本分析复用既有 7 provider（Local/S3/Aliyun/Tencent/Huawei/Upyun/Qiniu）。
- **sz-pay 项目**：API 兼容性验证的下游基准。

## 3.3 交互上下文

```plantuml
@startuml
title sz-orm v4.6.0 七项可靠性增强与运维智能化能力 交互上下文

rectangle "sz-orm v4.6.0" as orm {
  rectangle "DLX 自动重投递\n(调度器+退避+路由)" as dlx
  rectangle "零停机回滚\n(shadow+健康检查+窗口)" as rollback
  rectangle "批量事务原子性\n(all-or-nothing+Saga+2PC)" as atomic
  rectangle "异常检测\n(算法+告警+标注)" as anomaly
  rectangle "存储成本分析\n(分析+建议+报表)" as cost
  rectangle "连接级多租户隔离\n(绑定+亲和+SET)" as tenant
  rectangle "进程级 L1 缓存\n(跨Session+Identity Map)" as l1
}

actor "ORM 库维护者" as dev
actor "应用开发者" as appdev
actor "DBA/性能工程师" as dba
actor "运维/SRE" as sre
actor "FinOps/成本工程师" as finops
actor "sz-pay 开发者" as downstream

database "MySQL/PG/SQLite\n/Oracle/MSSQL" as db
rectangle "连接池\n(既有 Pool)" as pool
rectangle "消息队列\n(既有 6 provider)" as mq
rectangle "对象存储\n(既有 7 provider)" as storage
rectangle "sz-orm-dtx\n(既有 Saga/2PC)" as dtx

dev --> orm : 开发/验证/测试
appdev --> dlx : 消息可靠投递(死信自动重投)
dlx --> mq : 复用既有死信队列
appdev --> rollback : 迁移安全(零停机回滚)
rollback --> db : shadow table/reverse migration
appdev --> atomic : 批量原子性(all-or-nothing)
atomic --> dtx : 复用 Saga/2PC
atomic --> db : 跨批次原子提交
appdev --> anomaly : 异常自检(检测+告警)
anomaly --> db : 指标历史数据
finops --> cost : 成本分析(报表+建议)
cost --> storage : 复用既有 7 provider
appdev --> tenant : 多租户隔离(连接级)
tenant --> pool : 复用既有连接池
tenant --> db : SET app.tenant_id=?
appdev --> l1 : 缓存升级(进程级 L1)
l1 --> db : L1→L2→DB 查询协作
dba --> rollback : 评估回滚对DB影响
dba --> atomic : 评估事务负载影响
dba --> tenant : 评估连接池影响
dba --> l1 : 评估内存与命中率影响
sre --> dlx : 配置退避策略
sre --> rollback : 配置回滚窗口+健康检查
sre --> anomaly : 配置算法+告警阈值
sre --> cost : 配置分析周期
downstream --> orm : API 兼容性验证

@enduml
```

---

# 4. DFX约束

## 4.1 性能

1. **DLX 自动重投递调度开销**：自动重投递调度器检查开销不超过 1ms/次（含退避计算 + 调度判定），不显著影响消息队列吞吐量。
2. **零停机回滚切换时间**：零停机回滚的流量切换时间不超过 5 秒（shadow table 模式，含数据校验 + 切换 + 连接刷新），回滚过程中服务不中断。
3. **批量事务原子性开销**：批量事务原子性协调开销不超过 10ms（含 Saga 步骤编排 + 补偿回滚判定），跨批次原子提交的性能损耗不超过 5%（相比非原子批量执行）。
4. **异常检测开销**：异常检测算法执行开销不超过 1ms/指标/次（含算法计算 + 阈值比较 + 告警判定），不显著影响指标采集性能。
5. **成本分析开销**：成本分析报表生成开销不超过 5 秒（含 7 provider 成本统计 + 报表生成），可周期性执行（默认每日一次）。
6. **连接级多租户隔离开销**：连接租户绑定开销不超过 0.5ms/次（含 `SET app.tenant_id` 执行 + 亲和性判定），不显著影响连接获取性能。
7. **进程级 L1 缓存命中率**：进程级 L1 缓存命中率应不低于既有 Session 级 L1（跨 Session 共享扩大命中范围），缓存查找开销不超过 100ns/次（含 `RwLock` 读锁 + HashMap 查找）。

## 4.2 可靠性

1. **DLX 自动重投递不丢失消息**：死信消息自动重投递时，消息须保留在死信队列中直到重投递成功，重投递失败时按退避策略重试，不丢失消息。
2. **DLX 重投递次数上限**：自动重投递次数有上限（可配，默认 10 次），超过上限后按 DLX 路由策略处理（Drop 丢弃 / ForwardToDlxTopic 转发 / 保持死信），不无限重投递。
3. **零停机回滚数据一致性**：零停机回滚后数据须与回滚前一致（shadow table 模式数据校验），回滚失败时保持原状态不产生脏数据。
4. **回滚窗口超时保护**：超过回滚窗口后不可自动回滚（需手动回滚），避免长时间后回滚导致数据丢失。
5. **健康检查触发回滚可靠性**：健康检查连续失败 N 次（可配，默认 3 次）后才触发自动回滚，避免单次抖动误触发回滚。
6. **批量事务 all-or-nothing**：批量事务原子性保证 all-or-nothing 语义，全成功提交 / 全失败回滚，不产生部分提交的脏数据。
7. **Saga 补偿回滚可靠性**：Saga 补偿回滚时，已成功批次的补偿操作须执行成功，补偿失败时记录补偿日志供人工干预，不静默忽略。
8. **异常检测不误报**：异常检测算法须配置合理的阈值/窗口，避免正常波动被误报为异常，异常告警须附异常证据（指标值/阈值/时间窗口）。
9. **成本分析数据准确**：成本分析数据须准确反映实际存储成本（按 provider API 返回的计费数据），不估算不编造。
10. **连接级多租户隔离防越权**：连接级多租户隔离须防止租户越权（连接绑定的 tenant_id 不可被篡改，查询自动注入 tenant_id 过滤），不泄露跨租户数据。
11. **连接亲和性不泄漏连接**：连接亲和性复用绑定到租户的连接时，连接须在请求完成后归还连接池，不长期占用连接。
12. **进程级 L1 缓存一致性**：进程级 L1 缓存须与 L2 缓存保持一致性（L1 失效时 L2 同步失效），不返回过期数据，复用既有 `CacheCoherenceProtocol`（`packages/sz-orm-core/src/cache_coherence.rs:103`）。
13. **进程级 L1 缓存线程安全**：进程级 L1 缓存须线程安全（`Send + Sync`），跨线程共享不数据竞争。
14. **v4.5.0 测试基线不回退**：v4.6.0 不得使 v4.5.0 已验收测试基线回退，仅增不减。

## 4.3 安全性

1. **DLX 死信消息不泄露**：死信消息须保留原始消息的脱敏状态（复用既有 `sz-orm-masking` 脱敏），不泄露敏感数据。
2. **零停机回滚 SQL 参数化**：回滚 SQL 须参数化绑定（复用既有 `Connection::execute_with_params` `packages/sz-orm-core/src/pool.rs:82`），禁止 SQL 字符串拼接，防止 SQL 注入。
3. **批量事务参数化**：批量事务原子性操作须参数化绑定，禁止 SQL 字符串拼接，防止 SQL 注入。
4. **异常检测不泄露敏感指标**：异常检测告警须脱敏敏感指标（如用户数据），不泄露敏感信息。
5. **成本分析不泄露凭据**：成本分析须使用既有存储凭据（`StorageBuilder` `packages/sz-orm-storage/src/storage.rs:22`），不暴露凭据，不重新输入凭据。
6. **连接级多租户隔离防注入**：连接级多租户隔离的 `SET app.tenant_id = ?` 须参数化绑定，tenant_id 不可被客户端篡改（由可信路径设置），防止 SQL 注入与越权。
7. **进程级 L1 缓存不泄露数据**：进程级 L1 缓存须按租户隔离（多租户环境下，不同租户的缓存项隔离），不泄露跨租户数据。
8. **审计证据要求**：每项需求结论须附 file:line 证据，遵循 AGENTS.md 审计合规铁律。

## 4.4 可维护性

1. **DLX 自动重投递可配置**：退避策略、重投递次数上限、DLX 路由策略须可配置，不强制干扰开发者，默认值保守（Fixed 退避 1 秒，上限 10 次，RequeueToOriginal 路由）。
2. **零停机回滚可配置**：回滚窗口、健康检查阈值、健康检查连续失败次数须可配置，不强制干扰开发者，默认值保守（窗口 5 分钟，错误率 5%，连续失败 3 次）。
3. **批量事务原子性可配置**：原子性保证级别（AllOrNothing / BestEffort / SagaCompensation）、Saga 补偿策略须可配置，不强制干扰开发者。
4. **异常检测可配置**：异常检测算法、阈值/窗口、告警通道须可配置，不强制干扰开发者。
5. **成本分析可配置**：成本分析周期、优化建议类型、报表格式须可配置，不强制干扰开发者。
6. **连接级多租户可配置**：连接亲和策略、隔离机制（SET app.tenant_id / Schema 隔离）须可配置，不强制干扰开发者。
7. **进程级 L1 缓存可配置**：进程级 L1 容量、TTL、失效策略须可配置，不强制干扰开发者，默认值保守（容量 10000，TTL 5 分钟）。
8. **五方言一致**：新增能力在 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 五方言上行为一致（迁移回滚/批量原子性/连接级多租户按方言能力适配，如 `SET app.tenant_id` 仅 PG/MySQL 支持）。
9. **可靠性增强审计可追溯**：DLX 自动重投递须记录重投递日志（消息 ID + 重投递次数 + 退避时间 + 结果），零停机回滚须记录回滚日志（版本 + 策略 + 耗时 + 结果），批量事务原子性须记录原子提交日志（批次列表 + 提交/回滚结果），供审计追溯。

## 4.5 兼容性

1. **API 向后兼容**：所有新能力通过 feature gate 隔离，默认 feature 行为不变，既有公开 API 完全向后兼容。
2. **sz-pay 不破坏**：sz-pay 从 crates.io 拉取的 sz-orm-* 6 个包既有用法不受影响。
3. **既有消息队列保留**：既有 `MessageQueue` trait（`packages/sz-orm-queue/src/queue.rs:18`）/ `InMemoryQueue`（`:339`）/ `Message`（`:57`）/ `nack`（`:37`）/ `reject`（`:44`）/ `requeue_dead_letter`（`:484`）保留不动，DLX 自动重投递为扩展。
4. **既有迁移管理保留**：既有 `Migration`（`packages/sz-orm-core/src/migration.rs:10`）/ `rollback`（`:587`）/ `down`（`:677`）/ `MigrationResolver`（`:62`）/ `FileMigrationResolver`（`:68`）保留不动，零停机回滚为扩展。
5. **既有批量执行器保留**：既有 `BatchExecutor`（`packages/sz-orm-batch/src/executor.rs`）/ `BatchExecutorConfig`（`:18`）/ `BatchExecutionResult`（`:93`）/ `RollbackStrategy`（`packages/sz-orm-batch/src/lib.rs:518`）保留不动，批量事务原子性为扩展。
6. **既有分布式事务保留**：既有 `Saga`（`packages/sz-orm-dtx/src/saga.rs:377`）/ `SagaStep`（`:255`）/ `DistributedTransaction`（`packages/sz-orm-dtx/src/lib.rs:270`）保留不动，批量事务原子性复用 sz-orm-dtx。
7. **既有可观测性保留**：既有 `MetricsRegistry`（`packages/sz-orm-observability/src/lib.rs:259`）/ `SloMonitor`（`packages/sz-orm-observability/src/slo.rs:223`）/ `QueryLogger`（`packages/sz-orm-observability/src/query_logger.rs:73`）保留不动，异常检测为扩展。
8. **既有存储保留**：既有 `Storage` trait（`packages/sz-orm-storage/src/storage.rs:14`）/ `StorageBuilder`（`:22`）/ `BucketLifecycle`（`packages/sz-orm-storage/src/advanced.rs:438`）/ `LifecycleRule`（`:400`）保留不动，成本分析为扩展。
9. **既有连接池保留**：既有 `Pool`（`packages/sz-orm-core/src/pool.rs:743`）/ `Connection` trait（`:45`）/ `PooledConnection`（`:239`）保留不动，连接级多租户隔离复用既有连接池。
10. **既有多租户保留**：既有 `TenantContext`（`packages/sz-orm-core/src/tenant_context.rs:80`）/ `IsolationStrategy`（`:22`）/ `TenantPoolRegistry`（`:224`）/ `TenantContextGuard`（`:166`）保留不动，连接级多租户隔离为扩展。
11. **既有 L1 缓存保留**：既有 `L1Cache`（`packages/sz-orm-core/src/l1_cache.rs:87`）/ `L1CacheStats`（`:47`）保留不动，进程级 L1 缓存为新增 API。
12. **既有 L2 缓存保留**：既有 `L2Cache`（`packages/sz-orm-core/src/l2_cache.rs:517`）/ `CacheKey`（`:143`）/ `CacheCoherenceProtocol`（`packages/sz-orm-core/src/cache_coherence.rs:103`）保留不动，进程级 L1 缓存与 L2 协同。
13. **既有 feature 组合不破坏**：v4.6.0 新增 feature（`dlx-auto-redelivery` / `zero-downtime-rollback` / `batch-atomic` / `anomaly-detection` / `cost-analysis` / `connection-level-tenant` / `process-l1-cache`）与既有 feature（含 v4.3.0 7 个 + v4.4.0 6 个 + v4.5.0 3 个）任意组合编译通过。

---

# 5. 核心能力

## 5.1 消息死信队列自动重投递（REQ-V46-001，P1）

### 5.1.1 业务规则

1. **DLX 自动重投递调度器**（EARS: Ubiquitous）
   系统应当扩展既有 `sz-orm-queue` 包，提供 `RedeliveryScheduler` 自动重投递调度器，当消息通过既有 `nack`（`packages/sz-orm-queue/src/queue.rs:37`）达到 `max_retries`（`:366`）或 `reject`（`:44`）进入死信队列后，按退避策略自动调度重投递，无需手动调用既有 `requeue_dead_letter`（`:484`）。
   a. 验收条件：[消息 nack 达到 max_retries 进入死信队列 + 启用自动重投递] → [RedeliveryScheduler 按退避策略自动调度重投递，无需手动调用 requeue_dead_letter]
2. **退避策略**（EARS: Ubiquitous）
   系统应当提供 `BackoffPolicy` 退避策略枚举（Fixed 固定间隔 / Exponential 指数退避 / Linear 线性退避 / RandomJitter 随机抖动），死信消息重投递按退避策略计算下次重投递时间，避免立即重投递导致再次失败，退避策略可配（默认 Exponential，初始 1 秒，最大 60 秒）。
   a. 验收条件：[BackoffPolicy::Exponential，初始 1 秒，重投递失败 3 次] → [重投递间隔 1s → 2s → 4s，不超过最大 60s]
3. **DLX 路由策略**（EARS: Ubiquitous）
   系统应当提供 `DlxRoutingStrategy` DLX 路由策略枚举（RequeueToOriginal 放回原队列 / ForwardToDlxTopic 转发到死信 topic / ForwardToDlxQueue 转发到死信 queue / Drop 丢弃），死信消息按路由策略处理，路由策略可配（默认 RequeueToOriginal）。
   a. 验收条件：[DlxRoutingStrategy::ForwardToDlxTopic + 死信 topic "orders.dlx"] → [死信消息转发到 "orders.dlx" topic]；[DlxRoutingStrategy::Drop] → [死信消息丢弃不重投递]
4. **重投递次数上限**（EARS: State-driven）
   当自动重投递次数达到上限时，系统应当按 DLX 路由策略处理（Drop 丢弃 / ForwardToDlxTopic 转发 / 保持死信），不无限重投递，重投递次数上限可配（默认 10 次）。
   a. 验收条件：[重投递次数上限 10 + 消息重投递失败 10 次] → [按路由策略处理，不再重投递，记录日志"redelivery limit reached for message X"]
5. **复用既有死信队列与消息结构**（EARS: Ubiquitous）
   系统应当复用既有 `InMemoryQueue.dead_letters`（`packages/sz-orm-queue/src/queue.rs:364`）/ `Message.retry_count`（`:67`）/ `max_retries`（`:366`）/ `DEFAULT_MAX_RETRIES`（`:377`），不重复实现死信存储，自动重投递调度器基于既有死信队列扩展。
   a. 验收条件：[自动重投递调度器] → [复用既有 dead_letters 存储，不新建死信存储结构]
6. **重投递日志可追溯**（EARS: Ubiquitous）
   系统应当记录重投递日志（消息 ID + 重投递次数 + 退避时间 + 路由策略 + 结果），供审计追溯，复用既有 `message_tracing` 模块（`packages/sz-orm-queue/src/message_tracing.rs`，feature gate `message-tracing`）。
   a. 验收条件：[消息重投递 3 次] → [记录 3 条重投递日志，含消息 ID + 次数 + 退避时间 + 结果]
7. **禁止项**（EARS: Unwanted）
   如果 DLX 自动重投递影响默认 feature 编译或破坏既有 `MessageQueue` trait，则系统应当通过 `dlx-auto-redelivery` feature gate 隔离，默认不启用自动重投递，且既有 `MessageQueue` trait 与 `requeue_dead_letter` 保留不动。
   a. 验收条件：[`cargo build` 默认编译] → [无自动重投递调度器，行为与 v4.5.0 一致，既有 requeue_dead_letter 手动调用仍可用]

### 5.1.2 交互流程

```plantuml
@startuml
title 消息死信队列自动重投递 交互流程
actor "应用开发者" as appdev
participant "sz-orm-queue\n(RedeliveryScheduler)" as scheduler
participant "InMemoryQueue\n(既有死信队列)" as queue
participant "BackoffPolicy\n(退避策略)" as backoff
queue "消息队列 provider" as mq

appdev -> queue : nack(message_id) [复用既有]
queue -> queue : retry_count + 1
alt retry_count >= max_retries
  queue -> queue : 进入死信队列 dead_letters
  queue -> scheduler : 通知死信消息进入
  scheduler -> backoff : 计算退避时间(retry_count)
  backoff --> scheduler : backoff_time
  scheduler -> scheduler : 等待 backoff_time
  alt 重投递次数 < 上限
    alt 路由策略 RequeueToOriginal
      scheduler -> queue : requeue_dead_letter(message_id) [复用既有]
      queue -> mq : 消息重回原队列
    else 路由策略 ForwardToDlxTopic
      scheduler -> mq : publish(dlx_topic, message)
    else 路由策略 Drop
      scheduler -> queue : 丢弃死信
    end
    scheduler -> scheduler : 记录重投递日志
  else 重投递次数 >= 上限
    scheduler -> scheduler : 按路由策略处理 + 记录"redelivery limit reached"
  end
else retry_count < max_retries
  queue -> mq : 消息重回原队列尾部
end
@enduml
```

### 5.1.3 异常场景

1. **重投递消息不存在**
   a. 触发条件：自动重投递调度器尝试重投递时，死信消息已被手动消费或删除
   b. 系统行为：跳过该消息，记录日志"dead letter message X not found, skipped"
   c. 用户感知：日志标注"redelivery skipped, message not found"
2. **重投递目标队列不可用**
   a. 触发条件：重投递目标队列（原队列或 DLX topic）不可用（如连接断开）
   b. 系统行为：按退避策略重试，重试次数超限后保持死信，记录日志
   c. 用户感知：日志标注"redelivery failed, target unavailable, retried N times"
3. **退避策略计算异常**
   a. 触发条件：退避策略计算溢出（如 Exponential 指数过大）
   b. 系统行为：降级为最大退避时间（60 秒），记录日志"backoff calculation overflow, fallback to max"
   c. 用户感知：日志标注"backoff fallback to max 60s"

## 5.2 迁移回滚自动化（REQ-V46-002，P1）

### 5.2.1 业务规则

1. **零停机回滚策略**（EARS: Ubiquitous）
   系统应当扩展既有 `sz-orm-core` 迁移管理，提供 `ZeroDowntimeRollbackStrategy` 零停机回滚策略枚举（ShadowTable 影子表 / ReverseMigration 反向迁移 / BlueGreen 蓝绿回滚），通过 shadow table 或 reverse migration 实现零停机回滚，回滚过程中服务不中断，回滚完成后切换流量，策略可配（默认 ShadowTable）。
   a. 验收条件：[ShadowTable 策略 + 回滚到版本 V] → [在 shadow table 执行 down SQL，校验数据一致性，切换流量到 shadow table，服务不中断]
2. **复用既有迁移回滚**（EARS: Ubiquitous）
   系统应当复用既有 `Migration.sql_down`（`packages/sz-orm-core/src/migration.rs:18`）/ `rollback`（`:587`）/ `down`（`:677`）/ `MigrationResolver`（`:62`）/ `FileMigrationResolver`（`:68`），零停机回滚基于既有回滚逻辑扩展，不重复实现回滚 SQL 执行。
   a. 验收条件：[零停机回滚] → [复用既有 rollback/down 执行 down SQL，不新建回滚执行逻辑]
3. **回滚窗口**（EARS: State-driven）
   当部署后时间超过回滚窗口时，系统应当拒绝自动回滚（返回错误"rollback window expired, manual rollback required"），避免长时间后回滚导致数据丢失，回滚窗口可配（默认 5 分钟）。
   a. 验收条件：[回滚窗口 5 分钟 + 部署后 6 分钟健康检查失败] → [拒绝自动回滚，返回错误"rollback window expired"]
4. **健康检查触发自动回滚**（EARS: Event-driven）
   如果部署后健康检查连续失败 N 次（可配，默认 3 次），则系统应当自动触发回滚，在回滚窗口内执行零停机回滚，无需人工干预，健康检查阈值（错误率/响应时间）可配。
   a. 验收条件：[健康检查连续失败 3 次 + 错误率 > 5% + 在回滚窗口内] → [自动触发零停机回滚，记录日志"auto rollback triggered by health check failure"]
5. **回滚数据一致性校验**（EARS: Ubiquitous）
   系统应当在校验回滚后数据一致性（shadow table 模式校验 shadow table 与原表数据一致），校验失败时保持原状态不切换流量，记录错误"rollback data consistency check failed"。
   a. 验收条件：[ShadowTable 回滚 + 数据校验失败] → [保持原状态不切换流量，记录错误"consistency check failed"]
6. **回滚日志可追溯**（EARS: Ubiquitous）
   系统应当记录回滚日志（回滚版本 + 策略 + 触发原因 + 耗时 + 结果），供审计追溯。
   a. 验收条件：[自动回滚触发] → [记录回滚日志，含版本 + 策略 + 触发原因 + 耗时 + 结果]
7. **禁止项**（EARS: Unwanted）
   如果零停机回滚影响默认 feature 编译或破坏既有迁移管理，则系统应当通过 `zero-downtime-rollback` feature gate 隔离，默认不启用零停机回滚，且既有 `rollback`/`down` 保留不动。
   a. 验收条件：[`cargo build` 默认编译] → [无零停机回滚，行为与 v4.5.0 一致，既有 rollback/down 手动调用仍可用]

### 5.2.2 交互流程

```plantuml
@startuml
title 迁移回滚自动化 交互流程
actor "运维/SRE" as sre
participant "AutoRollbackTrigger\n(自动回滚触发器)" as trigger
participant "HealthCheck\n(健康检查)" as health
participant "RollbackExecutor\n(回滚执行器)" as executor
participant "MigrationContext\n(既有迁移)" as mig
database "DB" as db

sre -> trigger : 配置(窗口+健康检查阈值+策略)
loop 部署后持续健康检查
  trigger -> health : check()
  alt 健康检查通过
    health --> trigger : OK
  else 健康检查连续失败 N 次
    health --> trigger : Fail(错误率/响应时间)
    alt 在回滚窗口内
      trigger -> executor : execute_rollback(策略, 目标版本)
      alt ShadowTable 策略
        executor -> mig : down(target_version) [复用既有]
        mig -> db : 在 shadow table 执行 down SQL
        executor -> db : 校验 shadow table 与原表数据一致性
        alt 校验通过
          executor -> db : 切换流量到 shadow table
          executor -> sre : 回滚成功
        else 校验失败
          executor -> sre : 保持原状态 + 错误"consistency check failed"
        end
      else ReverseMigration 策略
        executor -> mig : rollback(version) [复用既有]
        mig -> db : 执行 down SQL
      end
      executor -> executor : 记录回滚日志
    else 超过回滚窗口
      trigger -> sre : 拒绝自动回滚"rollback window expired"
    end
  end
end
@enduml
```

### 5.2.3 异常场景

1. **回滚窗口超时**
   a. 触发条件：部署后健康检查失败，但已超过回滚窗口
   b. 系统行为：拒绝自动回滚，返回错误"rollback window expired, manual rollback required"
   c. 用户感知：错误"rollback window expired, manual rollback required"
2. **数据一致性校验失败**
   a. 触发条件：ShadowTable 回滚后数据一致性校验失败
   b. 系统行为：保持原状态不切换流量，记录错误"consistency check failed"
   c. 用户感知：错误"rollback data consistency check failed, kept original state"
3. **回滚 SQL 执行失败**
   a. 触发条件：回滚 SQL（down SQL）执行失败（如 SQL 错误、连接中断）
   b. 系统行为：中止回滚，记录错误，保持原状态
   c. 用户感知：错误"rollback SQL execution failed: ..."
4. **健康检查抖动误触发**
   a. 触发条件：健康检查单次失败但立即恢复（抖动）
   b. 系统行为：连续失败 N 次后才触发回滚，单次抖动不触发
   c. 用户感知：无（抖动被过滤）

## 5.3 批量事务原子性保证（REQ-V46-003，P1）

### 5.3.1 业务规则

1. **all-or-nothing 原子语义**（EARS: Ubiquitous）
   系统应当扩展既有 `sz-orm-batch` 批量执行器，提供 `AtomicityGuarantee` 原子性保证枚举（AllOrNothing 全成功提交/全失败回滚 / BestEffort 尽力而为/部分成功 / SagaCompensation Saga 补偿），`AllOrNothing` 模式保证跨批次操作的原子性，不产生部分提交的脏数据。
   a. 验收条件：[AllOrNothing + 3 批次 + 第 2 批次失败] → [全部回滚，返回错误"atomicity violated, all batches rolled back"，不产生部分提交]
2. **复用既有批量执行器与分布式事务**（EARS: Ubiquitous）
   系统应当复用既有 `BatchExecutor`（`packages/sz-orm-batch/src/executor.rs`）/ `BatchExecutorConfig`（`:18`）/ `BatchExecutionResult`（`:93`）/ `RollbackStrategy`（`packages/sz-orm-batch/src/lib.rs:518`）+ `Saga`（`packages/sz-orm-dtx/src/saga.rs:377`）/ `SagaStep`（`:255`）/ `DistributedTransaction`（`packages/sz-orm-dtx/src/lib.rs:270`），不重复实现批量执行与分布式事务逻辑。
   a. 验收条件：[批量事务原子性] → [复用既有 BatchExecutor 执行批次 + 复用 Saga/2PC 协调原子提交，不新建执行逻辑]
3. **Saga 补偿模式批量操作**（EARS: Ubiquitous）
   系统应当支持 `SagaCompensation` 模式，每个批次作为一个 `SagaStep`（`packages/sz-orm-dtx/src/saga.rs:255`），批次成功时执行下一批次，批次失败时按 Saga 补偿回滚已成功批次（补偿操作为批次的反向操作），实现最终一致性。
   a. 验收条件：[SagaCompensation + 3 批次 + 第 2 批次失败] → [第 1 批次补偿回滚，第 2/3 批次不执行，返回 Saga 结果含补偿日志]
4. **跨批次原子提交**（EARS: Ubiquitous）
   系统应当支持跨批次原子提交，多个批次作为一个原子事务提交（复用既有 `DistributedTransaction` 2PC `packages/sz-orm-dtx/src/lib.rs:270`，prepare 全成功后 commit，任一 prepare 失败后 rollback），确保跨批次操作的原子性。
   a. 验收条件：[3 批次跨批次原子提交 + 第 2 批次 prepare 失败] → [全部 rollback，返回错误"2PC prepare failed, all batches rolled back"]
5. **原子性保证可配置**（EARS: Ubiquitous）
   系统应当支持原子性保证级别可配置（`AtomicityGuarantee` AllOrNothing/BestEffort/SagaCompensation），默认 `BestEffort`（兼容既有 `RollbackStrategy::None` 行为），不强制干扰开发者。
   a. 验收条件：[AtomicityGuarantee::BestEffort] → [行为与既有 RollbackStrategy::None 一致，允许部分成功]；[AtomicityGuarantee::AllOrNothing] → [全成功/全失败]
6. **原子提交日志可追溯**（EARS: Ubiquitous）
   系统应当记录原子提交日志（批次列表 + 原子性级别 + 提交/回滚结果 + Saga 补偿日志），供审计追溯。
   a. 验收条件：[AllOrNothing 3 批次提交] → [记录原子提交日志，含批次列表 + 级别 + 结果]
7. **禁止项**（EARS: Unwanted）
   如果批量事务原子性影响默认 feature 编译或破坏既有批量执行器，则系统应当通过 `batch-atomic` feature gate 隔离，默认不启用原子性保证，且既有 `BatchExecutor` 与 `RollbackStrategy` 保留不动。
   a. 验收条件：[`cargo build` 默认编译] → [无原子性保证，行为与 v4.5.0 一致，既有 BatchExecutor 仍可用]

### 5.3.2 交互流程

```plantuml
@startuml
title 批量事务原子性保证 交互流程
actor "应用开发者" as appdev
participant "BatchTransactionCoordinator\n(批量事务协调器)" as coord
participant "BatchExecutor\n(既有批量执行器)" as executor
participant "Saga\n(既有 sz-orm-dtx)" as saga
participant "DistributedTransaction\n(既有 2PC)" as dtx
database "DB" as db

appdev -> coord : execute_atomic(batches, AtomicityGuarantee)
alt AllOrNothing
  coord -> dtx : DistributedTransaction::new
  loop 每个批次
    coord -> executor : execute_batch(batch) [复用既有]
    executor -> db : 执行批次SQL
    coord -> dtx : add_participant(batch)
  end
  alt 全部 prepare 成功
    coord -> dtx : prepare() [复用既有]
    coord -> dtx : commit() [复用既有]
    coord --> appdev : BatchExecutionResult(全部成功)
  else 任一 prepare 失败
    coord -> dtx : rollback() [复用既有]
    coord --> appdev : Error("atomicity violated, all rolled back")
  end
else SagaCompensation
  coord -> saga : Saga::new [复用既有]
  loop 每个批次
    coord -> saga : add_step(SagaStep{action=批次, compensation=反向}) [复用既有]
  end
  coord -> saga : execute() [复用既有]
  alt 全部成功
    coord --> appdev : SagaResult(完成)
  else 某步失败
    coord -> saga : 补偿回滚已成功步骤
    coord --> appdev : SagaResult(补偿完成 + 补偿日志)
  end
else BestEffort
  coord -> executor : execute_batch(每批次) [复用既有 + RollbackStrategy::None]
  coord --> appdev : BatchExecutionResult(部分成功)
end
coord -> coord : 记录原子提交日志
@enduml
```

### 5.3.3 异常场景

1. **AllOrNothing 部分批次失败**
   a. 触发条件：AllOrNothing 模式下某批次失败
   b. 系统行为：全部回滚，返回错误"atomicity violated, all batches rolled back"
   c. 用户感知：错误"atomicity violated, all batches rolled back"
2. **Saga 补偿失败**
   a. 触发条件：Saga 补偿回滚时补偿操作失败
   b. 系统行为：记录补偿失败日志供人工干预，不静默忽略，返回 Saga 结果含补偿失败标记
   c. 用户感知：Saga 结果标注"compensation failed for step X, manual intervention required"
3. **2PC 协调器失败**
   a. 触发条件：2PC 协调器（DistributedTransaction）prepare/commit/rollback 失败
   b. 系统行为：按 2PC 协议处理（prepare 失败 rollback，commit 失败记录待提交事务），不产生不一致状态
   c. 用户感知：错误"2PC coordination failed: ..."
4. **批次执行超时**
   a. 触发条件：某批次执行超时
   b. 系统行为：按原子性级别处理（AllOrNothing 全回滚 / SagaCompensation 补偿 / BestEffort 计入失败）
   c. 用户感知：结果标注"batch X timed out"

## 5.4 异常检测（REQ-V46-004，P2）

### 5.4.1 业务规则

1. **异常检测器**（EARS: Ubiquitous）
   系统应当扩展既有 `sz-orm-observability` 可观测性，提供 `AnomalyDetector` 异常检测器，基于既有 `MetricsRegistry`（`packages/sz-orm-observability/src/lib.rs:259`）的指标历史数据检测异常，检测到异常后触发告警并在指标/日志上标注异常。
   a. 验收条件：[指标 "query_duration" 历史数据 + 异常检测] → [AnomalyDetector 检测异常 + 触发告警 + 标注异常]
2. **异常检测算法**（EARS: Ubiquitous）
   系统应当提供 `AnomalyAlgorithm` 异常检测算法枚举（Threshold 阈值检测 / Trend 趋势检测 / Statistical 统计检测 / ZScore Z-Score 检测 / IQR 四分位距检测），按算法检测指标异常，算法可配（默认 Threshold）。
   a. 验收条件：[AnomalyAlgorithm::Threshold + 阈值 1 秒 + 指标值 1.5 秒] → [检测到异常"query_duration 1.5s exceeds threshold 1s"]；[AnomalyAlgorithm::ZScore + Z-Score > 3] → [检测到异常"Z-Score 3.5 indicates anomaly"]
3. **复用既有指标与 SLO**（EARS: Ubiquitous）
   系统应当复用既有 `MetricsRegistry`（`packages/sz-orm-observability/src/lib.rs:259`）/ `SloMonitor`（`packages/sz-orm-observability/src/slo.rs:223`）/ `QueryLogger`（`packages/sz-orm-observability/src/query_logger.rs:73`），异常检测基于既有指标历史数据，不重复实现指标采集。
   a. 验收条件：[异常检测] → [复用既有 MetricsRegistry 指标历史数据，不新建指标采集]
4. **异常告警联动**（EARS: Event-driven）
   如果异常检测器检测到异常，则系统应当触发 `AnomalyAlert` 异常告警（含指标名 + 异常值 + 阈值 + 时间窗口 + 算法），告警通道可配（日志 / Webhook / 复用既有 SLO 告警通道）。
   a. 验收条件：[检测到异常] → [触发 AnomalyAlert，含指标名 + 异常值 + 阈值 + 时间窗口 + 算法，发送到配置的告警通道]
5. **异常标注**（EARS: Ubiquitous）
   系统应当在指标/日志上标注异常（复用既有 `QueryLogger` `packages/sz-orm-observability/src/query_logger.rs:73` 在查询日志上标注异常标记），便于排查。
   a. 验收条件：[检测到异常] → [在查询日志上标注"anomaly detected: ..."，便于排查]
6. **异常检测可配置**（EARS: Ubiquitous）
   系统应当支持异常检测算法、阈值/窗口、告警通道可配置，不强制干扰开发者，默认值保守（Threshold 阈值检测，窗口 5 分钟）。
   a. 验收条件：[配置算法 ZScore + 窗口 10 分钟] → [按 ZScore 算法 + 10 分钟窗口检测]
7. **禁止项**（EARS: Unwanted）
   如果异常检测影响默认 feature 编译或破坏既有可观测性，则系统应当通过 `anomaly-detection` feature gate 隔离，默认不启用异常检测，且既有 `MetricsRegistry`/`SloMonitor`/`QueryLogger` 保留不动。
   a. 验收条件：[`cargo build` 默认编译] → [无异常检测器，行为与 v4.5.0 一致]

### 5.4.2 交互流程

```plantuml
@startuml
title 异常检测 交互流程
actor "运维/SRE" as sre
participant "AnomalyDetector\n(异常检测器)" as detector
participant "MetricsRegistry\n(既有指标)" as metrics
participant "AnomalyAlgorithm\n(检测算法)" as algo
participant "QueryLogger\n(既有日志)" as logger

sre -> detector : 配置(算法+阈值+窗口+告警通道)
loop 周期性检测
  detector -> metrics : get_history(metric_name, window) [复用既有]
  metrics --> detector : 指标历史数据
  detector -> algo : detect(历史数据, 阈值)
  alt 检测到异常
    algo --> detector : Anomaly(指标名+异常值+阈值+管法)
    detector -> detector : 触发 AnomalyAlert
    detector -> logger : 标注异常"anomaly detected: ..." [复用既有]
    detector -> sre : 告警(指标名+异常值+阈值+窗口+管法)
  else 无异常
    algo --> detector : Normal
  end
end
@enduml
```

### 5.4.3 异常场景

1. **指标历史数据不足**
   a. 触发条件：指标历史数据不足（如刚启动无历史数据）
   b. 系统行为：跳过检测，记录日志"insufficient history data for metric X"
   c. 用户感知：日志标注"anomaly detection skipped, insufficient data"
2. **异常检测算法计算异常**
   a. 触发条件：算法计算异常（如除零、溢出）
   b. 系统行为：跳过该次检测，记录日志"anomaly algorithm calculation error: ..."
   c. 用户感知：日志标注"anomaly detection error, skipped"
3. **告警通道不可用**
   a. 触发条件：告警通道（Webhook）不可用
   b. 系统行为：记录告警到日志，标注"alert channel unavailable, logged only"
   c. 用户感知：日志标注"alert channel unavailable, anomaly logged only"

## 5.5 存储成本分析与优化建议（REQ-V46-005，P2）

### 5.5.1 业务规则

1. **成本分析器**（EARS: Ubiquitous）
   系统应当扩展既有 `sz-orm-storage` 存储，提供 `CostAnalyzer` 成本分析器，按 provider / bucket / tier 统计存储成本（容量成本 + 请求成本 + 流量成本），生成 `CostReport` 成本报表。
   a. 验收条件：[CostAnalyzer 分析 7 provider] → [生成 CostReport，含每 provider/bucket/tier 的容量+请求+流量成本]
2. **复用既有存储与生命周期**（EARS: Ubiquitous）
   系统应当复用既有 `Storage` trait（`packages/sz-orm-storage/src/storage.rs:14`）/ `StorageProvider`（`:287`）/ `BucketLifecycle`（`packages/sz-orm-storage/src/advanced.rs:438`）/ `LifecycleRule`（`:400`），成本分析基于既有存储与生命周期数据，不重复实现存储操作。
   a. 验收条件：[成本分析] → [复用既有 Storage 获取存储用量，不新建存储操作]
3. **成本优化建议**（EARS: Ubiquitous）
   系统应当基于成本分析结果生成 `CostOptimizationSuggestion` 成本优化建议枚举（TierDowngrade 存储层降级 / LifecycleOptimize 生命周期优化 / DeleteExpired 删除过期数据 / CompressCold 冷数据压缩），建议附预期节省成本。
   a. 验收条件：[成本分析 + 冷数据 100GB 在 Standard tier] → [生成 TierDowngrade 建议，降级到 Infrequent Access tier，预期节省 60%]
4. **成本报表周期性生成**（EARS: Ubiquitous）
   系统应当支持成本报表周期性生成（可配周期，默认每日一次），报表格式可配（JSON / CSV / 复用既有 `streaming-export` feature）。
   a. 验收条件：[配置周期每日 + 格式 JSON] → [每日生成 JSON 成本报表]
5. **成本数据准确**（EARS: Ubiquitous）
   系统应当使用 provider API 返回的计费数据（非估算），成本数据准确反映实际存储成本，不估算不编造。
   a. 验收条件：[成本分析] → [使用 provider API 计费数据，不估算]
6. **成本分析可配置**（EARS: Ubiquitous）
   系统应当支持成本分析周期、优化建议类型、报表格式可配置，不强制干扰开发者。
   a. 验收条件：[配置周期每周 + 建议 TierDowngrade] → [每周生成报表，仅含 TierDowngrade 建议]
7. **禁止项**（EARS: Unwanted）
   如果成本分析影响默认 feature 编译或破坏既有存储，则系统应当通过 `cost-analysis` feature gate 隔离，默认不启用成本分析，且既有 `Storage`/`BucketLifecycle` 保留不动。
   a. 验收条件：[`cargo build` 默认编译] → [无成本分析器，行为与 v4.5.0 一致]

### 5.5.2 交互流程

```plantuml
@startuml
title 存储成本分析与优化建议 交互流程
actor "FinOps/成本工程师" as finops
participant "CostAnalyzer\n(成本分析器)" as analyzer
participant "Storage\n(既有存储)" as storage
participant "BucketLifecycle\n(既有生命周期)" as lifecycle
participant "CostReport\n(成本报表)" as report

finops -> analyzer : 配置(周期+建议类型+报表格式)
loop 周期性分析
  analyzer -> storage : list_buckets() [复用既有]
  storage --> analyzer : buckets 列表
  loop 每个 bucket
    analyzer -> storage : get_usage(bucket) [复用既有]
    storage --> analyzer : 容量+请求+流量用量
    analyzer -> lifecycle : get_rules(bucket) [复用既有]
    lifecycle --> analyzer : 生命周期规则
  end
  analyzer -> analyzer : 计算成本(按 provider/bucket/tier)
  analyzer -> analyzer : 生成优化建议(TierDowngrade/LifecycleOptimize/...)
  analyzer -> report : 生成报表(JSON/CSV)
  analyzer -> finops : CostReport + 优化建议
end
@enduml
```

### 5.5.3 异常场景

1. **Provider API 不可用**
   a. 触发条件：某 provider API 不可用（如云服务故障）
   b. 系统行为：跳过该 provider 成本分析，记录日志"provider X API unavailable, skipped"
   c. 用户感知：报表标注"provider X cost analysis skipped, API unavailable"
2. **成本数据异常**
   a. 触发条件：provider API 返回的成本数据异常（如负数、超大值）
   b. 系统行为：标注异常数据，记录日志"abnormal cost data from provider X: ..."
   c. 用户感知：报表标注"abnormal cost data for provider X, please verify"
3. **优化建议不适用**
   a. 触发条件：优化建议不适用（如已是最低 tier 无法降级）
   b. 系统行为：跳过该建议，记录日志"suggestion X not applicable for bucket Y"
   c. 用户感知：报表不含不适用建议

## 5.6 连接级多租户隔离（REQ-V46-006，P1）

### 5.6.1 业务规则

1. **连接级多租户隔离**（EARS: Ubiquitous）
   系统应当扩展既有 `sz-orm-core` 连接池与多租户，提供 `ConnectionLevelIsolation` 连接级隔离枚举（SetTenantId 通过 `SET app.tenant_id = ?` / SchemaIsolation Schema 隔离 / ConnectionBinding 连接绑定），在同一连接池中连接绑定到特定租户，避免既有 `TenantPoolRegistry`（`packages/sz-orm-core/src/tenant_context.rs:224`）每租户独立池的资源开销，隔离机制可配（默认 SetTenantId，按方言能力适配）。
   a. 验收条件：[SetTenantId + PostgreSQL + tenant_id 1] → [连接执行 `SET app.tenant_id = 1`，后续查询自动按 tenant_id 隔离]；[SQLite 不支持 SET] → [降级为 SchemaIsolation，标注"SET app.tenant_id not supported, fallback to schema isolation"]
2. **复用既有连接池与多租户**（EARS: Ubiquitous）
   系统应当复用既有 `Pool`（`packages/sz-orm-core/src/pool.rs:743`）/ `Connection` trait（`:45`）/ `PooledConnection`（`:239`）/ `TenantContext`（`packages/sz-orm-core/src/tenant_context.rs:80`）/ `IsolationStrategy`（`:22`）/ `TenantContextGuard`（`:166`），连接级多租户隔离基于既有连接池与租户上下文扩展，不新建连接池。
   a. 验收条件：[连接级多租户隔离] → [复用既有 Pool 获取连接，在连接上设置租户上下文，不新建连接池]
3. **连接亲和性**（EARS: Ubiquitous）
   系统应当提供 `ConnectionAffinityPolicy` 连接亲和策略枚举（Strict 严格亲和 / Preferred 优先亲和 / None 无亲和），同一租户的请求优先复用绑定到该租户的连接，减少连接上的租户上下文切换开销，策略可配（默认 Preferred）。
   a. 验收条件：[ConnectionAffinityPolicy::Strict + tenant_id 1 请求] → [优先获取绑定到 tenant_id 1 的连接，无可用时等待]；[Preferred] → [优先获取绑定连接，无可用时获取任意连接并重新绑定]
4. **连接租户绑定防篡改**（EARS: Ubiquitous）
   系统应当确保连接绑定的 tenant_id 不可被客户端篡改（由可信路径/中间件设置，复用既有 `TenantContext` `packages/sz-orm-core/src/tenant_context.rs:80` 的可信路径设置语义），防止租户越权。
   a. 验收条件：[客户端尝试篡改 tenant_id] → [拒绝篡改，使用可信路径设置的 tenant_id]
5. **查询自动注入租户过滤**（EARS: Ubiquitous）
   系统应当对查询自动注入 tenant_id 过滤（复用既有 `RowLevelSecurityPolicy` `packages/sz-orm-core/src/tenant_security.rs:67` 行级安全策略，追加 `WHERE tenant_id = ?`），防止跨租户数据泄露。
   a. 验收条件：[tenant_id 1 查询 users 表] → [自动追加 `WHERE tenant_id = 1`，不返回其他租户数据]
6. **连接归还时清理租户上下文**（EARS: State-driven）
   当连接归还连接池时，系统应当清理连接上的租户上下文（重置 `SET app.tenant_id = NULL` 或等效操作），避免下一个请求复用连接时租户上下文残留。
   a. 验收条件：[连接归还池] → [清理租户上下文，下一个请求获取连接时无残留]
7. **禁止项**（EARS: Unwanted）
   如果连接级多租户隔离影响默认 feature 编译或破坏既有连接池/多租户，则系统应当通过 `connection-level-tenant` feature gate 隔离，默认不启用连接级隔离，且既有 `Pool`/`TenantContext`/`TenantPoolRegistry` 保留不动。
   a. 验收条件：[`cargo build` 默认编译] → [无连接级隔离，行为与 v4.5.0 一致，既有 TenantPoolRegistry 每租户独立池仍可用]

### 5.6.2 交互流程

```plantuml
@startuml
title 连接级多租户隔离 交互流程
actor "应用开发者" as appdev
participant "ConnectionTenantBinder\n(连接租户绑定器)" as binder
participant "Pool\n(既有连接池)" as pool
participant "TenantContext\n(既有租户上下文)" as ctx
database "DB" as db

appdev -> ctx : set_tenant_context(tenant_id=1) [复用既有, 可信路径]
appdev -> binder : acquire_with_tenant(tenant_id=1)
binder -> pool : acquire() [复用既有]
pool --> binder : PooledConnection
alt ConnectionAffinityPolicy::Strict/Preferred
  binder -> binder : 查找绑定到 tenant_id=1 的连接
  alt 找到绑定连接
    binder --> appdev : TenantConnectionGuard(连接)
  else 未找到 + SetTenantId
    binder -> db : SET app.tenant_id = 1 [参数化绑定]
    binder -> binder : 标记连接绑定到 tenant_id=1
    binder --> appdev : TenantConnectionGuard(连接)
  end
else SchemaIsolation
  binder -> binder : 路由到 tenant_1_table [复用既有 SchemaIsolationRouter]
  binder --> appdev : TenantConnectionGuard(连接)
end
appdev -> db : 查询(WHERE tenant_id=1 自动注入) [复用既有 RLS]
appdev -> binder : 归还连接(TenantConnectionGuard drop)
binder -> db : SET app.tenant_id = NULL [清理租户上下文]
binder -> pool : release(连接) [复用既有]
@enduml
```

### 5.6.3 异常场景

1. **方言不支持 SET app.tenant_id**
   a. 触发条件：方言不支持 `SET app.tenant_id`（如 SQLite）
   b. 系统行为：降级为 SchemaIsolation，标注"SET app.tenant_id not supported by dialect X, fallback to schema isolation"
   c. 用户感知：日志标注"fallback to schema isolation"
2. **连接亲和性无可用连接**
   a. 触发条件：Strict 亲和策略下无绑定到该租户的连接
   b. 系统行为：等待可用连接（可配超时），超时后返回错误
   c. 用户感知：错误"no connection bound to tenant X, timeout waiting"
3. **租户上下文篡改**
   a. 触发条件：客户端尝试篡改 tenant_id
   b. 系统行为：拒绝篡改，使用可信路径设置的 tenant_id
   c. 用户感知：错误"tenant_id tampering rejected"
4. **连接归还时清理失败**
   a. 触发条件：连接归还时 `SET app.tenant_id = NULL` 失败
   b. 系统行为：销毁该连接（不从池复用），避免租户上下文残留
   c. 用户感知：日志标注"connection destroyed, tenant context cleanup failed"

## 5.7 进程级 L1 缓存（REQ-V46-007，P1）

### 5.7.1 业务规则

1. **进程级 L1 缓存**（EARS: Ubiquitous）
   系统应当扩展既有 `sz-orm-core` L1 缓存，提供 `ProcessL1Cache` 进程级 L1 缓存（跨 Session 共享 Identity Map，线程安全 `Send + Sync`），与既有 `L2Cache`（`packages/sz-orm-core/src/l2_cache.rs:517`）协同，减少 L2 锁竞争，既有 `L1Cache`（`packages/sz-orm-core/src/l1_cache.rs:87`，Session 级）保留不动。
   a. 验收条件：[ProcessL1Cache 跨 Session 查询同主键] → [返回相同 Arc<T> 引用，Arc::ptr_eq 为 true，线程安全]
2. **跨 Session Identity Map**（EARS: Ubiquitous）
   系统应当提供跨 Session Identity Map 语义，相同主键的多次查询（跨 Session）返回相同 `Arc<T>` 引用（`Arc::ptr_eq` 为 true），线程安全，复用既有 `L1Cache` 的 Identity Map 语义（`packages/sz-orm-core/src/l1_cache.rs:87`）。
   a. 验收条件：[Session A 查询 pk=1 + Session B 查询 pk=1] → [返回相同 Arc<T> 引用，Arc::ptr_eq 为 true]
3. **复用既有 L1/L2 缓存**（EARS: Ubiquitous）
   系统应当复用既有 `L1Cache`（`packages/sz-orm-core/src/l1_cache.rs:87`）/ `L1CacheStats`（`:47`）/ `L2Cache`（`packages/sz-orm-core/src/l2_cache.rs:517`）/ `CacheKey`（`:143`）/ `CacheCoherenceProtocol`（`packages/sz-orm-core/src/cache_coherence.rs:103`），进程级 L1 缓存基于既有 L1/L2 缓存扩展，不重复实现缓存逻辑。
   a. 验收条件：[进程级 L1 缓存] → [复用既有 L1Cache Identity Map 语义 + L2Cache 协同，不新建缓存逻辑]
4. **L1→L2→DB 查询协作**（EARS: Ubiquitous）
   系统应当与既有 L2 缓存协同，按 L1→L2→DB 顺序查询：L1 命中直接返回；L1 未命中查 L2，L2 命中回填 L1 返回；L2 未命中查 DB，回填 L1+L2 返回，复用既有 L1→L2→DB 协作语义（`packages/sz-orm-core/src/l1_cache.rs:17`）。
   a. 验收条件：[L1 未命中 + L2 命中] → [回填 L1 返回]；[L1+L2 未命中] → [查 DB + 回填 L1+L2]
5. **缓存一致性**（EARS: Ubiquitous）
   系统应当与既有 L2 缓存保持一致性，L1 失效时 L2 同步失效（复用既有 `CacheCoherenceProtocol` `packages/sz-orm-core/src/cache_coherence.rs:103` + `MesiState` `:12`），不返回过期数据。
   a. 验收条件：[L1 失效 pk=1] → [L2 同步失效 pk=1，下次查询从 DB 加载]
6. **线程安全**（EARS: Ubiquitous）
   系统应当线程安全（`Send + Sync`），跨线程共享不数据竞争，使用 `RwLock` 或等效无锁结构保护内部数据，既有 `L1Cache`（`packages/sz-orm-core/src/l1_cache.rs:87`，非 Send + Sync）保留不动。
   a. 验收条件：[多线程并发查询 ProcessL1Cache] → [无数据竞争，线程安全]
7. **可配置容量与 TTL**（EARS: Ubiquitous）
   系统应当支持可配置容量上限（默认 10000）与 TTL（默认 5 分钟），超过容量时按 LRU 淘汰，超过 TTL 时过期失效，复用既有 `L1Cache` 的 LRU 淘汰语义（`packages/sz-orm-core/src/l1_cache.rs:91`）。
   a. 验收条件：[容量 10000 + 插入 10001 条] → [淘汰最久未使用条目]；[TTL 5 分钟 + 条目 6 分钟] → [过期失效]
8. **禁止项**（EARS: Unwanted）
   如果进程级 L1 缓存影响默认 feature 编译或破坏既有 L1/L2 缓存，则系统应当通过 `process-l1-cache` feature gate 隔离，默认不启用进程级 L1，且既有 `L1Cache`（Session 级）/ `L2Cache` 保留不动。
   a. 验收条件：[`cargo build` 默认编译] → [无进程级 L1，行为与 v4.5.0 一致，既有 L1Cache Session 级仍可用]

### 5.7.2 交互流程

```plantuml
@startuml
title 进程级 L1 缓存 交互流程
actor "应用开发者" as appdev
participant "ProcessL1Cache\n(进程级 L1)" as pl1
participant "L2Cache\n(既有 L2)" as l2
participant "CacheCoherenceProtocol\n(既有一致性)" as coherence
database "DB" as db

appdev -> pl1 : get(table, pk)
alt L1 命中
  pl1 --> appdev : Arc<T> [Identity Map]
else L1 未命中
  pl1 -> l2 : get(CacheKey) [复用既有]
  alt L2 命中
    l2 --> pl1 : Value
    pl1 -> pl1 : 回填 L1
    pl1 --> appdev : Arc<T>
  else L2 未命中
    l2 --> pl1 : None
    pl1 -> db : 查询
    db --> pl1 : T
    pl1 -> pl1 : 回填 L1
    pl1 -> l2 : put(CacheKey, Value) [复用既有]
    pl1 --> appdev : Arc<T>
  end
end
alt 缓存失效
  appdev -> pl1 : invalidate(table, pk)
  pl1 -> coherence : invalidate(CacheKey) [复用既有]
  coherence -> l2 : invalidate(CacheKey) [L1 失效同步 L2 失效]
end
@enduml
```

### 5.7.3 异常场景

1. **L1 与 L2 数据不一致**
   a. 触发条件：L1 与 L2 缓存数据不一致（如 L1 失效但 L2 未同步失效）
   b. 系统行为：通过 `CacheCoherenceProtocol` 同步失效，记录日志"cache inconsistency detected, synchronized"
   c. 用户感知：日志标注"cache coherence synchronized"
2. **LRU 淘汰热点数据**
   a. 触发条件：LRU 淘汰了热点数据（频繁访问的数据被误淘汰）
   b. 系统行为：按 LRU 语义淘汰（复用既有 `L1Cache` LRU），下次查询从 L2/DB 加载
   c. 用户感知：缓存命中率下降，可调大容量
3. **TTL 过期导致缓存穿透**
   a. 触发条件：TTL 过期后大量请求同时查询同一条目（缓存穿透）
   b. 系统行为：正常从 DB 加载回填，可配预加载（prewarm）
   c. 用户感知：短暂延迟，可配预加载避免
4. **多租户缓存隔离**
   a. 触发条件：多租户环境下不同租户查询同主键
   b. 系统行为：按租户隔离缓存项（CacheKey 含 tenant_id），不返回跨租户数据
   c. 用户感知：不同租户返回各自数据

---

# 6. 数据约束

## 6.1 DlxConfig（DLX 配置）

1. **enabled**：是否启用自动重投递，bool，必填，默认 false。
2. **backoff_policy**：退避策略，`BackoffPolicy` 枚举（Fixed/Exponential/Linear/RandomJitter），必填，默认 Exponential。
3. **initial_backoff_ms**：初始退避时间（毫秒），u64，必填，默认 1000（1 秒）。
4. **max_backoff_ms**：最大退避时间（毫秒），u64，必填，默认 60000（60 秒）。
5. **max_redelivery_count**：最大重投递次数，u32，必填，默认 10。
6. **routing_strategy**：DLX 路由策略，`DlxRoutingStrategy` 枚举（RequeueToOriginal/ForwardToDlxTopic/ForwardToDlxQueue/Drop），必填，默认 RequeueToOriginal。
7. **dlx_topic**：死信 topic（路由策略为 ForwardToDlxTopic 时必填），`Option<String>`，可选。
8. **dlx_queue**：死信 queue（路由策略为 ForwardToDlxQueue 时必填），`Option<String>`，可选。

## 6.2 BackoffPolicy（退避策略）

1. **Fixed**：固定间隔（initial_backoff_ms）。
2. **Exponential**：指数退避（initial_backoff_ms × 2^retry_count，不超过 max_backoff_ms）。
3. **Linear**：线性退避（initial_backoff_ms × retry_count，不超过 max_backoff_ms）。
4. **RandomJitter**：随机抖动（initial_backoff_ms × random(0.5~1.5)，避免重投递风暴）。

## 6.3 DlxRoutingStrategy（DLX 路由策略）

1. **RequeueToOriginal**：放回原队列（复用既有 `requeue_dead_letter` `packages/sz-orm-queue/src/queue.rs:484`）。
2. **ForwardToDlxTopic**：转发到死信 topic（通过 `publish` 发布到 dlx_topic）。
3. **ForwardToDlxQueue**：转发到死信 queue（通过 `publish` 发布到 dlx_queue）。
4. **Drop**：丢弃（从死信队列移除，记录日志）。

## 6.4 ZeroDowntimeRollbackConfig（零停机回滚配置）

1. **strategy**：零停机回滚策略，`ZeroDowntimeRollbackStrategy` 枚举（ShadowTable/ReverseMigration/BlueGreen），必填，默认 ShadowTable。
2. **rollback_window_ms**：回滚窗口（毫秒），u64，必填，默认 300000（5 分钟）。
3. **health_check_interval_ms**：健康检查间隔（毫秒），u64，必填，默认 10000（10 秒）。
4. **health_check_failure_threshold**：健康检查连续失败次数阈值，u32，必填，默认 3。
5. **error_rate_threshold**：错误率阈值（0.0~1.0），f64，必填，默认 0.05（5%）。
6. **response_time_threshold_ms**：响应时间阈值（毫秒），u64，必填，默认 5000（5 秒）。

## 6.5 ZeroDowntimeRollbackStrategy（零停机回滚策略）

1. **ShadowTable**：影子表（在 shadow table 执行 down SQL，校验数据一致性，切换流量）。
2. **ReverseMigration**：反向迁移（直接执行 down SQL，复用既有 `rollback` `packages/sz-orm-core/src/migration.rs:587`）。
3. **BlueGreen**：蓝绿回滚（切换到旧版本，需蓝绿部署支持）。

## 6.6 BatchAtomicConfig（批量事务原子性配置）

1. **atomicity_guarantee**：原子性保证级别，`AtomicityGuarantee` 枚举（AllOrNothing/BestEffort/SagaCompensation），必填，默认 BestEffort。
2. **chunk_size**：分片大小，usize，必填，默认 1000（复用既有 `DEFAULT_CHUNK_SIZE` `packages/sz-orm-batch/src/lib.rs:146`）。
3. **progress_callback**：进度回调，`Option<ProgressCallback>`（既有 `packages/sz-orm-batch/src/lib.rs:482`），可选。
4. **saga_log**：Saga 日志（SagaCompensation 模式用于故障恢复），`Option<Arc<dyn SagaLog>>`（既有 `packages/sz-orm-dtx/src/saga.rs:105`），可选。

## 6.7 AtomicityGuarantee（原子性保证级别）

1. **AllOrNothing**：全成功提交/全失败回滚（复用既有 `DistributedTransaction` 2PC `packages/sz-orm-dtx/src/lib.rs:270`）。
2. **BestEffort**：尽力而为/部分成功（兼容既有 `RollbackStrategy::None` `packages/sz-orm-batch/src/lib.rs:518`）。
3. **SagaCompensation**：Saga 补偿（复用既有 `Saga` `packages/sz-orm-dtx/src/saga.rs:377`，每批次作为 SagaStep，失败时补偿回滚）。

## 6.8 AnomalyConfig（异常检测配置）

1. **algorithm**：异常检测算法，`AnomalyAlgorithm` 枚举（Threshold/Trend/Statistical/ZScore/IQR），必填，默认 Threshold。
2. **window_ms**：检测窗口（毫秒），u64，必填，默认 300000（5 分钟）。
3. **threshold**：阈值（Threshold 算法用），f64，必填，默认 1.0。
4. **zscore_threshold**：Z-Score 阈值（ZScore 算法用），f64，必填，默认 3.0。
5. **alert_channel**：告警通道，`AlertChannel` 枚举（Log/Webhook/Slo），必填，默认 Log。
6. **webhook_url**：Webhook URL（告警通道为 Webhook 时必填），`Option<String>`，可选。

## 6.9 AnomalyAlgorithm（异常检测算法）

1. **Threshold**：阈值检测（指标值超过阈值即异常）。
2. **Trend**：趋势检测（指标持续上升/下降即异常）。
3. **Statistical**：统计检测（基于均值/方差检测异常）。
4. **ZScore**：Z-Score 检测（Z-Score 超过阈值即异常，Z-Score = (value - mean) / std_dev）。
5. **IQR**：四分位距检测（指标值超出 Q1-1.5×IQR 或 Q3+1.5×IQR 即异常）。

## 6.10 CostConfig（成本分析配置）

1. **analysis_interval_ms**：分析周期（毫秒），u64，必填，默认 86400000（每日）。
2. **suggestion_types**：优化建议类型，`Vec<CostOptimizationSuggestion>`，必填，默认全部类型。
3. **report_format**：报表格式，`ReportFormat` 枚举（Json/Csv），必填，默认 Json。
4. **providers**：分析的 provider 列表，`Vec<StorageProvider>`（既有 `packages/sz-orm-storage/src/storage.rs:287`），必填，默认全部 provider。

## 6.11 CostOptimizationSuggestion（成本优化建议）

1. **TierDowngrade**：存储层降级（冷数据从 Standard 降级到 Infrequent Access/Archive）。
2. **LifecycleOptimize**：生命周期优化（优化 `LifecycleRule` `packages/sz-orm-storage/src/advanced.rs:400` 规则）。
3. **DeleteExpired**：删除过期数据（超过保留期的数据删除）。
4. **CompressCold**：冷数据压缩（冷数据压缩存储减少容量成本）。

## 6.12 ConnectionLevelTenantConfig（连接级多租户配置）

1. **isolation**：连接级隔离机制，`ConnectionLevelIsolation` 枚举（SetTenantId/SchemaIsolation/ConnectionBinding），必填，默认 SetTenantId。
2. **affinity_policy**：连接亲和策略，`ConnectionAffinityPolicy` 枚举（Strict/Preferred/None），必填，默认 Preferred。
3. **affinity_timeout_ms**：亲和等待超时（毫秒，Strict 策略用），u64，必填，默认 5000（5 秒）。
4. **db_type**：数据库方言，`DbType`（既有 `packages/sz-orm-core/src/db_type.rs:11`），必填。

## 6.13 ConnectionLevelIsolation（连接级隔离机制）

1. **SetTenantId**：通过 `SET app.tenant_id = ?` 设置租户上下文（PostgreSQL/MySQL 支持，其他方言降级为 SchemaIsolation）。
2. **SchemaIsolation**：Schema 隔离（复用既有 `SchemaIsolationRouter` `packages/sz-orm-core/src/tenant_context.rs:194`，路由到 `tenant_{id}_{table}`）。
3. **ConnectionBinding**：连接绑定（连接绑定到特定租户，不设置上下文，依赖应用层隔离）。

## 6.14 ConnectionAffinityPolicy（连接亲和策略）

1. **Strict**：严格亲和（仅使用绑定到该租户的连接，无可用时等待）。
2. **Preferred**：优先亲和（优先使用绑定连接，无可用时获取任意连接并重新绑定）。
3. **None**：无亲和（任意连接，每次设置租户上下文）。

## 6.15 ProcessL1Config（进程级 L1 缓存配置）

1. **capacity**：容量上限，usize，必填，默认 10000。
2. **ttl_ms**：TTL（毫秒），u64，必填，默认 300000（5 分钟）。
3. **enable_coherence**：是否启用缓存一致性（与 L2 同步失效），bool，必填，默认 true。
4. **tenant_isolated**：是否按租户隔离（多租户环境下缓存项隔离），bool，必填，默认 true。

---

# 7. 需求追溯矩阵

| 需求 ID | 优先级 | 需求名称 | 验收条件数 | feature gate | 复用既有代码 |
|---------|--------|---------|-----------|-------------|-------------|
| REQ-V46-001 | P1 | 消息死信队列自动重投递 | 7 | `dlx-auto-redelivery` | `MessageQueue` `packages/sz-orm-queue/src/queue.rs:18` / `Message` `:57` / `InMemoryQueue` `:339` / `dead_letters` `:364` / `max_retries` `:366` / `nack` `:37` / `reject` `:44` / `requeue_dead_letter` `:484` / `DEFAULT_MAX_RETRIES` `:377` |
| REQ-V46-002 | P1 | 迁移回滚自动化 | 7 | `zero-downtime-rollback` | `Migration` `packages/sz-orm-core/src/migration.rs:10` / `sql_down` `:18` / `rollback` `:587` / `down` `:677` / `MigrationResolver` `:62` / `FileMigrationResolver` `:68` / `MigrationContext` `:193` |
| REQ-V46-003 | P1 | 批量事务原子性保证 | 7 | `batch-atomic` | `BatchExecutor` `packages/sz-orm-batch/src/executor.rs` / `BatchExecutorConfig` `:18` / `BatchExecutionResult` `:93` / `RollbackStrategy` `packages/sz-orm-batch/src/lib.rs:518` / `DEFAULT_CHUNK_SIZE` `:146` / `Saga` `packages/sz-orm-dtx/src/saga.rs:377` / `SagaStep` `:255` / `SagaLog` `:105` / `DistributedTransaction` `packages/sz-orm-dtx/src/lib.rs:270` |
| REQ-V46-004 | P2 | 异常检测 | 7 | `anomaly-detection` | `MetricsRegistry` `packages/sz-orm-observability/src/lib.rs:259` / `MetricKind` `:75` / `SloMonitor` `packages/sz-orm-observability/src/slo.rs:223` / `SloConfig` `:52` / `QueryLogger` `packages/sz-orm-observability/src/query_logger.rs:73` / `QueryLogEntry` `:46` |
| REQ-V46-005 | P2 | 存储成本分析与优化建议 | 7 | `cost-analysis` | `Storage` `packages/sz-orm-storage/src/storage.rs:14` / `StorageBuilder` `:22` / `StorageProvider` `:287` / `BucketLifecycle` `packages/sz-orm-storage/src/advanced.rs:438` / `LifecycleRule` `:400` / `LifecycleAction` `:378` |
| REQ-V46-006 | P1 | 连接级多租户隔离 | 7 | `connection-level-tenant` | `Pool` `packages/sz-orm-core/src/pool.rs:743` / `Connection` `:45` / `PooledConnection` `:239` / `acquire` `:1268` / `TenantContext` `packages/sz-orm-core/src/tenant_context.rs:80` / `IsolationStrategy` `:22` / `TenantContextGuard` `:166` / `SchemaIsolationRouter` `:194` / `TenantPoolRegistry` `:224` / `RowLevelSecurityPolicy` `packages/sz-orm-core/src/tenant_security.rs:67` / `DbType` `packages/sz-orm-core/src/db_type.rs:11` |
| REQ-V46-007 | P1 | 进程级 L1 缓存 | 8 | `process-l1-cache` | `L1Cache` `packages/sz-orm-core/src/l1_cache.rs:87` / `L1CacheStats` `:47` / `L2Cache` `packages/sz-orm-core/src/l2_cache.rs:517` / `CacheKey` `:143` / `invalidate_table` `:740` / `CacheCoherenceProtocol` `packages/sz-orm-core/src/cache_coherence.rs:103` / `MesiState` `:12` / `ConsistencyStrategy` `:25` |

---

# 8. 验收标准总览

## 8.1 REQ-V46-001 消息死信队列自动重投递（P1）

1. `RedeliveryScheduler` 自动重投递调度器，死信消息按退避策略自动调度重投递（无需手动调用 `requeue_dead_letter`）
2. `BackoffPolicy` 四种退避策略（Fixed/Exponential/Linear/RandomJitter），默认 Exponential
3. `DlxRoutingStrategy` 四种路由策略（RequeueToOriginal/ForwardToDlxTopic/ForwardToDlxQueue/Drop），默认 RequeueToOriginal
4. 重投递次数上限（默认 10 次），超过上限按路由策略处理
5. 复用既有 `InMemoryQueue.dead_letters` / `Message.retry_count` / `max_retries`，不重复实现死信存储
6. 重投递日志可追溯（消息 ID + 次数 + 退避时间 + 结果）
7. `dlx-auto-redelivery` feature gate 隔离，默认关闭，既有 `MessageQueue` trait 与 `requeue_dead_letter` 保留

## 8.2 REQ-V46-002 迁移回滚自动化（P1）

1. `ZeroDowntimeRollbackStrategy` 三种策略（ShadowTable/ReverseMigration/BlueGreen），默认 ShadowTable
2. 复用既有 `Migration.sql_down` / `rollback` / `down` / `MigrationResolver` / `FileMigrationResolver`，不重复实现回滚
3. 回滚窗口（默认 5 分钟），超时拒绝自动回滚
4. 健康检查触发自动回滚（连续失败 N 次，默认 3 次）
5. 回滚数据一致性校验（ShadowTable 模式校验数据一致）
6. 回滚日志可追溯（版本 + 策略 + 触发原因 + 耗时 + 结果）
7. `zero-downtime-rollback` feature gate 隔离，默认关闭，既有 `rollback`/`down` 保留

## 8.3 REQ-V46-003 批量事务原子性保证（P1）

1. `AtomicityGuarantee` 三种级别（AllOrNothing/BestEffort/SagaCompensation），默认 BestEffort
2. 复用既有 `BatchExecutor` + `sz-orm-dtx` Saga/2PC，不重复实现
3. Saga 补偿模式（每批次作为 SagaStep，失败时补偿回滚已成功批次）
4. 跨批次原子提交（复用 DistributedTransaction 2PC，prepare 全成功后 commit）
5. 原子性保证可配置（默认 BestEffort 兼容既有 RollbackStrategy::None）
6. 原子提交日志可追溯（批次列表 + 级别 + 结果 + 补偿日志）
7. `batch-atomic` feature gate 隔离，默认关闭，既有 `BatchExecutor` 与 `RollbackStrategy` 保留

## 8.4 REQ-V46-004 异常检测（P2）

1. `AnomalyDetector` 异常检测器，基于既有 `MetricsRegistry` 指标历史数据检测异常
2. `AnomalyAlgorithm` 五种算法（Threshold/Trend/Statistical/ZScore/IQR），默认 Threshold
3. 复用既有 `MetricsRegistry` / `SloMonitor` / `QueryLogger`，不重复实现指标采集
4. 异常告警联动（触发 AnomalyAlert，含指标名 + 异常值 + 阈值 + 窗口 + 管法）
5. 异常标注（在查询日志上标注异常标记）
6. 异常检测可配置（算法 + 阈值/窗口 + 告警通道）
7. `anomaly-detection` feature gate 隔离，默认关闭，既有 `MetricsRegistry`/`SloMonitor`/`QueryLogger` 保留

## 8.5 REQ-V46-005 存储成本分析与优化建议（P2）

1. `CostAnalyzer` 成本分析器，按 provider/bucket/tier 统计成本（容量+请求+流量）
2. 复用既有 `Storage` / `StorageProvider` / `BucketLifecycle` / `LifecycleRule`，不重复实现存储
3. `CostOptimizationSuggestion` 四种建议（TierDowngrade/LifecycleOptimize/DeleteExpired/CompressCold），附预期节省
4. 成本报表周期性生成（默认每日，JSON/CSV 格式）
5. 成本数据准确（使用 provider API 计费数据，不估算）
6. 成本分析可配置（周期 + 建议类型 + 报表格式）
7. `cost-analysis` feature gate 隔离，默认关闭，既有 `Storage`/`BucketLifecycle` 保留

## 8.6 REQ-V46-006 连接级多租户隔离（P1）

1. `ConnectionLevelIsolation` 三种隔离机制（SetTenantId/SchemaIsolation/ConnectionBinding），默认 SetTenantId
2. 复用既有 `Pool` / `Connection` / `TenantContext` / `IsolationStrategy`，不新建连接池
3. `ConnectionAffinityPolicy` 三种亲和策略（Strict/Preferred/None），默认 Preferred
4. 连接租户绑定防篡改（tenant_id 不可被客户端篡改）
5. 查询自动注入租户过滤（复用既有 `RowLevelSecurityPolicy`，追加 `WHERE tenant_id = ?`）
6. 连接归还时清理租户上下文（避免残留）
7. `connection-level-tenant` feature gate 隔离，默认关闭，既有 `Pool`/`TenantContext`/`TenantPoolRegistry` 保留

## 8.7 REQ-V46-007 进程级 L1 缓存（P1）

1. `ProcessL1Cache` 进程级 L1 缓存（跨 Session 共享 Identity Map，线程安全 Send + Sync）
2. 跨 Session Identity Map 语义（相同主键跨 Session 返回相同 Arc<T> 引用）
3. 复用既有 `L1Cache` / `L2Cache` / `CacheKey` / `CacheCoherenceProtocol`，不重复实现缓存
4. L1→L2→DB 查询协作（L1 命中直接返回，L1 未命中查 L2，L2 未命中查 DB）
5. 缓存一致性（L1 失效时 L2 同步失效，复用 `CacheCoherenceProtocol`）
6. 线程安全（Send + Sync，跨线程共享无数据竞争）
7. 可配置容量与 TTL（默认容量 10000，TTL 5 分钟，LRU 淘汰）
8. `process-l1-cache` feature gate 隔离，默认关闭，既有 `L1Cache`（Session 级）/ `L2Cache` 保留

---

# 9. feature gate 总览

| feature gate | 所属包 | 控制能力 | 默认 | 对应需求 |
|-------------|--------|---------|------|---------|
| `dlx-auto-redelivery` | sz-orm-queue（扩展） | 消息死信队列自动重投递（调度器 + 退避策略 + DLX 路由） | 关闭 | REQ-V46-001 |
| `zero-downtime-rollback` | sz-orm-core（扩展） | 迁移回滚自动化（零停机策略 + 健康检查 + 回滚窗口） | 关闭 | REQ-V46-002 |
| `batch-atomic` | sz-orm-batch（扩展）+ sz-orm-dtx（只读复用 Saga/2PC） | 批量事务原子性保证（all-or-nothing + Saga 补偿 + 跨批次原子提交） | 关闭 | REQ-V46-003 |
| `anomaly-detection` | sz-orm-observability（扩展） | 异常检测（算法 + 告警 + 标注） | 关闭 | REQ-V46-004 |
| `cost-analysis` | sz-orm-storage（扩展） | 存储成本分析与优化建议（成本分析 + 优化建议 + 报表） | 关闭 | REQ-V46-005 |
| `connection-level-tenant` | sz-orm-core（扩展） | 连接级多租户隔离（连接绑定 + 亲和性 + SET app.tenant_id） | 关闭 | REQ-V46-006 |
| `process-l1-cache` | sz-orm-core（扩展） | 进程级 L1 缓存（跨 Session Identity Map + 线程安全 + L1→L2→DB 协同） | 关闭 | REQ-V46-007 |

---

# 10. 与 v4.5.0 的关系

## 10.1 零重叠声明

v4.6.0 与 v4.5.0 零重叠：

| v4.5.0 能力（执行优化层） | v4.6.0 能力（可靠性 + 运维智能化层） | 关系 |
|-------------------------------|-------------------------|------|
| 并行查询执行器（`sz-orm-parallel`） | 连接级多租户隔离 / 进程级 L1 缓存 | v4.6.0 连接级多租户与进程级 L1 复用既有 `Pool`（`packages/sz-orm-core/src/pool.rs:743`），与并行查询复用同一连接池，不冲突 |
| 批量 INSERT/UPDATE/DELETE 优化（`sz-orm-batch` batch-v2） | 批量事务原子性保证（`sz-orm-batch` batch-atomic） | v4.6.0 批量事务原子性复用 v4.5.0 `BatchExecutor`（`packages/sz-orm-batch/src/executor.rs`），扩展原子性保证，不修改既有 batch-v2 逻辑 |
| 异步流式结果集（`sz-orm-stream`） | 进程级 L1 缓存 | v4.6.0 进程级 L1 缓存可与流式结果集协同（流式查询可接入进程级 L1），不冲突 |

## 10.2 依赖关系

```
v4.5.0 已验收基线（3 个 feature gate: parallel-query / batch-v2 / stream-resultset）
  │
  ├─ batch-v2 ───→ REQ-V46-003 批量事务原子性（复用 BatchExecutor + sz-orm-dtx Saga/2PC）
  │
  └─ (其他 v4.5.0 feature) ──→ 无 v4.6.0 强依赖（v4.6.0 七项需求主体独立）

v4.6.0 七项需求相互独立，可并行开发：
  ├─ REQ-V46-001 DLX 自动重投递（扩展 sz-orm-queue，复用既有 MessageQueue/InMemoryQueue/dead_letters）
  ├─ REQ-V46-002 零停机回滚（扩展 sz-orm-core migration，复用既有 Migration/rollback/down）
  ├─ REQ-V46-003 批量事务原子性（扩展 sz-orm-batch，复用既有 BatchExecutor + sz-orm-dtx Saga/2PC）
  ├─ REQ-V46-004 异常检测（扩展 sz-orm-observability，复用既有 MetricsRegistry/SloMonitor/QueryLogger）
  ├─ REQ-V46-005 成本分析（扩展 sz-orm-storage，复用既有 Storage/BucketLifecycle/LifecycleRule）
  ├─ REQ-V46-006 连接级多租户（扩展 sz-orm-core pool/tenant_context，复用既有 Pool/TenantContext）
  └─ REQ-V46-007 进程级 L1 缓存（扩展 sz-orm-core l1_cache/l2_cache，复用既有 L1Cache/L2Cache/CacheCoherenceProtocol）
```

## 10.3 扩展包

| 包名 | 对应需求 | 扩展内容 |
|------|---------|---------|
| `sz-orm-queue` | REQ-V46-001 | DLX 自动重投递调度器 + 退避策略 + DLX 路由策略（`dlx-auto-redelivery` feature） |
| `sz-orm-core` | REQ-V46-002 / REQ-V46-006 / REQ-V46-007 | 零停机回滚（`zero-downtime-rollback` feature）+ 连接级多租户隔离（`connection-level-tenant` feature）+ 进程级 L1 缓存（`process-l1-cache` feature） |
| `sz-orm-batch` | REQ-V46-003 | 批量事务原子性保证（all-or-nothing + Saga 补偿 + 跨批次原子提交，`batch-atomic` feature） |
| `sz-orm-observability` | REQ-V46-004 | 异常检测（算法 + 告警 + 标注，`anomaly-detection` feature） |
| `sz-orm-storage` | REQ-V46-005 | 存储成本分析与优化建议（成本分析 + 优化建议 + 报表，`cost-analysis` feature） |

## 10.4 新增包

本版本不新增包，所有能力通过既有包扩展实现（sz-orm-queue / sz-orm-core / sz-orm-batch / sz-orm-observability / sz-orm-storage），减少工作空间成员变更。

---

> 文档生成依据：`docs/spec/v4.5.0/` SDD 三阶段文档（spec 688 行 / design 98714 字节 / tasks 96260 字节，v4.5.0 已全部完成）+ 2026-08-12 逐项代码验证（所有 file:line 证据均已实测存在）+ 对比分析文档 5.2/5.4 节 16 项缺失能力筛选（按价值/可行性选择 7 项）
> 审计合规：本文档所有 file:line 证据均引用真实存在的代码，遵循 AGENTS.md 审计合规铁律
> 下一阶段：spec-design-agent 生成 `design.md`（技术设计），spec-task-agent 生成 `tasks.md`（编码任务规划）