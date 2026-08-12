# sz-orm v4.7.0 需求规格说明书

> 版本：v4.7.0（消息延迟队列与优先级调度 + 迁移前向兼容性检查与沙箱预演 + 批量 COPY 协议与并行分片执行 + 异常自愈与根因分析 + 多云成本对比与容量预测 + 租户资源配额与行级安全增强 + 缓存预热与穿透防护）
> 基线：v4.6.0（消息死信队列自动重投递 + 迁移回滚自动化 + 批量事务原子性保证 + 异常检测 + 存储成本分析 + 连接级多租户隔离 + 进程级 L1 缓存，7 项需求 REQ-V46-001~007 全部通过 feature gate 隔离，已验收基线，已发布到 crates.io）
> 日期：2026-08-13
> 需求格式：EARS（Ubiquitous / Event-driven / State-driven / Optional / Unwanted）
> 文档定位：需求规格（What to build），不含技术设计（How to build）
> 优先级声明：7 项需求中 5 项 P1（REQ-V47-001 延迟队列与优先级调度 / REQ-V47-002 前向兼容与沙箱预演 / REQ-V47-003 COPY 协议与并行分片 / REQ-V47-006 租户配额与 RLS 增强 / REQ-V47-007 缓存预热与穿透防护）+ 2 项 P2（REQ-V47-004 异常自愈与根因分析 / REQ-V47-005 多云成本对比与容量预测），按"REQ-V47-001 延迟优先级 → REQ-V47-002 前向兼容沙箱 → REQ-V47-003 COPY 并行分片 → REQ-V47-006 租户配额 RLS → REQ-V47-007 缓存预热防护 → REQ-V47-004 异常自愈 RCA → REQ-V47-005 多云成本预测"序推进，7 项无强依赖可并行开发
> 需求编号约定：REQ-V47-xxx（v4.7.0 需求项，REQ-V47-001 ~ REQ-V47-007）
> 规划依据：`docs/spec/v4.6.0/` SDD 三阶段文档（spec 1139 行 / design / tasks，v4.6.0 已全部完成并发布 crates.io）+ 2026-08-13 逐项代码验证（file:line 均已实测存在）+ v4.6.0 七项功能自然延伸方向分析
> 兼容性铁律：所有新能力通过 feature gate 隔离，默认 feature 行为不变，既有公开 API 完全向后兼容，v4.6.0 已验收测试基线不回退；sz-pay 生产依赖（从 crates.io 拉取 sz-orm-* 6 个包）不得被破坏；五方言覆盖：MySQL/PostgreSQL/SQLite/Oracle/MSSQL
> 范围声明：本版本聚焦智能化运维深化与性能深化（消息时间/优先级维度 + 迁移前向兼容/沙箱预演 + COPY 高速加载/并行分片 + 异常自愈/根因分析 + 多云成本对比/容量预测 + 租户配额/RLS 增强 + 缓存预热/穿透防护）；更长期（v4.x+ 跨语言分布式事务/低代码双向同步/OpenAPI 反向生成/WASM 真实连接）在后续版本规划；本版本不涉及 crates.io 发布流程变更
> 边界声明：与 v4.6.0 零重叠（见第 1.4 节），v4.6.0 是"可靠性 + 运维智能化"层（消息可靠/迁移安全/批量原子/异常自检/成本自优/租户隔离/缓存升级），v4.7.0 是"智能化运维深化 + 性能深化"层（消息时间优先级/迁移前向兼容沙箱/COPY 并行分片/异常自愈 RCA/多云成本预测/租户配额 RLS/缓存预热防护）

---

# 1. 组件定位

## 1.1 核心职责

本组件负责交付 sz-orm v4.7.0 的七项智能化运维深化与性能深化能力：(1) 消息延迟队列与优先级调度（扩展既有 `sz-orm-queue` 包 `packages/sz-orm-queue/src/queue.rs:18` `MessageQueue` trait + `:57` `Message` + `:339` `InMemoryQueue` + v4.6.0 `packages/sz-orm-queue/src/dlx.rs:216` `RedeliveryScheduler` 调度器基线，补齐延迟投递 + 优先级队列 + 定时调度，使消息队列支持时间维度和优先级维度的精确控制）；(2) 迁移前向兼容性检查与沙箱预演（扩展既有 `sz-orm-core` 迁移管理 `packages/sz-orm-core/src/migration.rs:10` `Migration` + v4.6.0 `packages/sz-orm-core/src/rollback_zero_downtime.rs:305` `RollbackExecutor` + 既有 `packages/sz-orm-core/src/migration_dry_run.rs:11` `DryRunMigration` + `:80` `ImpactReport`，补齐前向兼容性检查 + 沙箱预演 + 迁移依赖图分析）；(3) 批量 COPY 协议与并行分片执行（扩展既有 `sz-orm-batch` 包 `packages/sz-orm-batch/src/copy.rs:14` `CopyProtocolExecutor` + v4.6.0 `packages/sz-orm-batch/src/atomic.rs:216` `BatchTransactionCoordinator` + 既有 `packages/sz-orm-batch/src/executor.rs:18` `BatchExecutorConfig`，补齐 COPY 协议高速批量加载 + 并行分片执行 + 批量冲突解决策略）；(4) 异常自愈与根因分析（扩展既有 `sz-orm-observability` 包 v4.6.0 `packages/sz-orm-observability/src/anomaly.rs:254` `AnomalyDetector` + `:206` `AnomalyAlert` + `:154` `Anomaly`，补齐异常自愈 + 根因分析 + 异常关联分析）；(5) 多云成本对比与容量预测（扩展既有 `sz-orm-storage` 包 v4.6.0 `packages/sz-orm-storage/src/cost.rs:231` `CostAnalyzer` + `:213` `CostReport` + `:202` `ProviderCost`，补齐多云成本对比 + 容量预测 + 成本自动优化执行）；(6) 租户资源配额与行级安全增强（扩展既有 `sz-orm-core` 多租户 v4.6.0 `packages/sz-orm-core/src/connection_tenant.rs:133` `ConnectionTenantBinder` + 既有 `packages/sz-orm-core/src/tenant_security.rs:67` `RowLevelSecurityPolicy` + `:155` `ColumnMaskingRule` + `packages/sz-orm-core/src/tenant_context.rs:80` `TenantContext`，补齐租户资源配额 + 行级安全策略增强 + 租户级审计日志）；(7) 缓存预热与穿透防护（扩展既有 `sz-orm-core` 缓存 v4.6.0 `packages/sz-orm-core/src/process_l1_cache.rs:169` `ProcessL1Cache` + `:366` `CrossSessionIdentityMap` + 既有 `packages/sz-orm-core/src/l2_cache.rs:517` `L2Cache` + `packages/sz-orm-core/src/cache_coherence.rs:103` `CacheCoherenceProtocol`，补齐缓存预热 + 缓存穿透防护 + 缓存击穿防护）。

## 1.2 核心输入

1. **v4.6.0 已验收基线**：消息死信队列自动重投递 + 迁移回滚自动化 + 批量事务原子性保证 + 异常检测 + 存储成本分析 + 连接级多租户隔离 + 进程级 L1 缓存，7 项能力全部通过 feature gate 隔离，已发布到 crates.io，作为本版本基准。
2. **现有能力清单与缺口证据**：
   - **消息队列（v4.6.0 DLX 自动重投递）**：`packages/sz-orm-queue/src/dlx.rs:216` `pub struct RedeliveryScheduler`（v4.6.0 自动重投递调度器）、`:207` `RedeliveryLog`、`:197` `RedeliveryOutcome`、`:164` `DlxEntry`、`:91` `DlxConfig`、`:83` `DlxRoutingStrategy`、`:47` `BackoffPolicy`、`packages/sz-orm-queue/src/queue.rs:18` `pub trait MessageQueue`（publish/consume/ack/subscribe/nack/reject）、`:57` `pub struct Message`（topic/payload/key/timestamp/headers/id/retry_count）、`:339` `pub struct InMemoryQueue`。缺口：无延迟投递（delayed delivery，消息按指定时间投递，非立即投递），无优先级队列（priority queue，高优先级消息优先投递），无定时调度（scheduled message，周期性/定时消息）。
   - **迁移管理（v4.6.0 零停机回滚 + 既有 dry-run）**：`packages/sz-orm-core/src/rollback_zero_downtime.rs:305` `pub struct RollbackExecutor`（v4.6.0 回滚执行器）、`:84` `ZeroDowntimeRollbackConfig`、`:73` `ZeroDowntimeRollbackStrategy`、`:157` `RollbackPlan`、`packages/sz-orm-core/src/migration_dry_run.rs:11` `pub struct DryRunMigration`（既有 dry-run 迁移）、`:24` `DryRunReport`、`:59` `MigrationImpact`、`:80` `ImpactReport`、`:33` `DdlType`、`:48` `LockType`、`packages/sz-orm-core/src/migration.rs:10` `pub struct Migration`、`:62` `MigrationResolver`、`:193` `MigrationContext`。缺口：无前向兼容性检查（forward compatibility check，确保新 schema 不破坏旧应用），无沙箱预演（sandbox dry-run，在影子表上预执行迁移并验证），无迁移依赖图分析（dependency graph analysis，迁移间依赖关系分析）。
   - **批量执行（v4.6.0 批量事务原子性 + 既有 COPY 协议）**：`packages/sz-orm-batch/src/copy.rs:14` `pub struct CopyProtocolExecutor`（既有 PostgreSQL COPY 协议执行器，仅 PG/GaussDB/Kingbase/PolarDB 支持）、`packages/sz-orm-batch/src/atomic.rs:216` `pub struct BatchTransactionCoordinator`（v4.6.0 批量事务协调器）、`:20` `AtomicityGuarantee`、`:90` `BatchAtomicConfig`、`:436` `SagaCompensator`、`packages/sz-orm-batch/src/executor.rs:18` `pub struct BatchExecutorConfig`、`:93` `BatchExecutionResult`、`:141` `BatchExecutor`。缺口：COPY 协议仅 PostgreSQL 系，无 MySQL LOAD DATA INFILE / Oracle SQL*Loader / MSSQL BULK INSERT 方言适配，无并行分片执行（parallel shard execution，跨分片并行批量操作），无批量冲突解决策略（conflict resolution: upsert/ignore/merge）。
   - **可观测性（v4.6.0 异常检测）**：`packages/sz-orm-observability/src/anomaly.rs:254` `pub struct AnomalyDetector`（v4.6.0 异常检测器）、`:206` `AnomalyAlert`、`:154` `Anomaly`、`:74` `AnomalyConfig`、`:40` `AnomalyAlgorithm`、`:59` `AlertChannel`、`packages/sz-orm-observability/src/lib.rs:259` `MetricsRegistry`、`packages/sz-orm-observability/src/query_logger.rs:73` `QueryLogger`。缺口：无异常自愈（auto-remediation，检测到异常后自动执行预设修复动作），无根因分析（root cause analysis，异常归因到具体组件/SQL/连接），无异常关联分析（correlation analysis，跨指标关联异常事件）。
   - **存储成本（v4.6.0 成本分析）**：`packages/sz-orm-storage/src/cost.rs:231` `pub struct CostAnalyzer`（v4.6.0 成本分析器）、`:213` `CostReport`、`:202` `ProviderCost`、`:181` `BucketCost`、`:55` `CostOptimizationSuggestion`、`:96` `CostConfig`、`:42` `ReportFormat`、`packages/sz-orm-storage/src/storage.rs:14` `Storage` trait、`:287` `StorageProvider`（7 provider）。缺口：无多云成本对比（multi-cloud cost comparison，跨 provider 成本对比），无容量预测（capacity forecasting，基于历史趋势预测未来存储需求），无成本自动优化执行（auto-execute optimization suggestions，自动执行优化建议而非仅生成建议）。
   - **多租户（v4.6.0 连接级隔离 + 既有 RLS）**：`packages/sz-orm-core/src/connection_tenant.rs:133` `pub struct ConnectionTenantBinder`（v4.6.0 连接租户绑定器）、`:24` `ConnectionLevelIsolation`、`:35` `ConnectionAffinityPolicy`、`:249` `TenantConnectionGuard`、`packages/sz-orm-core/src/tenant_security.rs:67` `pub struct RowLevelSecurityPolicy`（既有行级安全策略）、`:155` `ColumnMaskingRule`（列级脱敏）、`:197` `TenantAuditOperation`、`:244` `TenantAuditContext`、`packages/sz-orm-core/src/tenant_context.rs:80` `TenantContext`、`:22` `IsolationStrategy`。缺口：无租户资源配额（tenant resource quota：max connections / max QPS / max storage），无行级安全策略增强（RLS policy enforcement，自动注入 WHERE 条件，复用既有 `RowLevelSecurityPolicy` 但增强策略表达能力），无租户级审计日志（tenant-level audit log，按租户独立审计日志）。
   - **缓存（v4.6.0 进程级 L1）**：`packages/sz-orm-core/src/process_l1_cache.rs:169` `pub struct ProcessL1Cache<T>`（v4.6.0 进程级 L1 缓存）、`:366` `CrossSessionIdentityMap<T>`、`:44` `ProcessL1Config`、`:103` `ProcessL1Stats`、`:421` `tenant_cache_key`、`packages/sz-orm-core/src/l2_cache.rs:517` `pub struct L2Cache`、`:143` `CacheKey`、`packages/sz-orm-core/src/cache_coherence.rs:103` `CacheCoherenceProtocol`、`:12` `MesiState`。缺口：无缓存预热（cache warmup/preload，启动时预加载热点数据），无缓存穿透防护（cache penetration protection，布隆过滤器拦截不存在的 key），无缓存击穿防护（cache stampede protection，singleflight 机制防止缓存重建风暴）。
3. **本机数据库连接信息**：MySQL 9.6（`mysql://root:test123@127.0.0.1:3306/sz_orm_test`）、PostgreSQL 18（`postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`）、Oracle 23ai Free（`127.0.0.1:1521/freepdb1`）。
4. **sz-pay 生产依赖证据**：sz-pay 从 crates.io 拉取 sz-orm-* 6 个包，作为 API 兼容性验证的下游基准。
5. **五方言覆盖约束**：MySQL/PostgreSQL/SQLite/Oracle/MSSQL，COPY 协议/并行分片/租户配额/RLS 增强须覆盖全部方言（按方言能力适配，如 COPY 协议仅 PostgreSQL 系原生支持，其他方言降级为 multi-value INSERT；RLS 自动注入 WHERE 须全部方言支持）。
6. **既有 feature gate 体系**：v4.6.0 已有 7 个 feature（`dlx-auto-redelivery` `packages/sz-orm-queue/Cargo.toml:38` / `zero-downtime-rollback` `packages/sz-orm-core/Cargo.toml:134` / `batch-atomic` `packages/sz-orm-batch/Cargo.toml:28` / `anomaly-detection` `packages/sz-orm-observability/Cargo.toml:28` / `cost-analysis` `packages/sz-orm-storage/Cargo.toml:19` / `connection-level-tenant` `packages/sz-orm-core/Cargo.toml:136` / `process-l1-cache` `packages/sz-orm-core/Cargo.toml:138`），作为新能力 feature gate 隔离的基础。

## 1.3 核心输出

1. **消息延迟队列与优先级调度**：sz-orm-queue 扩展（`DelayedMessage` 延迟消息 + `PriorityQueue` 优先级队列 + `ScheduledMessage` 定时消息 + `DelayScheduler` 延迟调度器 + `PriorityPolicy` 优先级策略 + `ScheduleConfig` 调度配置，复用既有 `RedeliveryScheduler` 调度器基线）。
2. **迁移前向兼容性检查与沙箱预演**：sz-orm-core 扩展（`ForwardCompatChecker` 前向兼容性检查器 + `CompatCheckResult` 兼容性检查结果 + `SandboxDryRunner` 沙箱预演器 + `SandboxConfig` 沙箱配置 + `MigrationDependencyGraph` 迁移依赖图 + `DependencyAnalyzer` 依赖分析器，复用既有 `DryRunMigration`/`ImpactReport`/`RollbackExecutor`）。
3. **批量 COPY 协议与并行分片执行**：sz-orm-batch 扩展（`CopyProtocolAdapter` COPY 协议方言适配器 + `ParallelShardExecutor` 并行分片执行器 + `ShardConfig` 分片配置 + `ConflictResolution` 冲突解决策略枚举 + `CopyBatchResult` COPY 批量结果，复用既有 `CopyProtocolExecutor`/`BatchTransactionCoordinator`）。
4. **异常自愈与根因分析**：sz-orm-observability 扩展（`AutoRemediator` 异常自愈器 + `RemediationAction` 修复动作枚举 + `RootCauseAnalyzer` 根因分析器 + `RootCause` 根因结果 + `AnomalyCorrelator` 异常关联器 + `CorrelationResult` 关联结果，复用既有 `AnomalyDetector`/`AnomalyAlert`）。
5. **多云成本对比与容量预测**：sz-orm-storage 扩展（`MultiCloudCostComparator` 多云成本对比器 + `CostComparisonReport` 成本对比报表 + `CapacityForecaster` 容量预测器 + `CapacityForecast` 容量预测结果 + `AutoOptimizer` 自动优化执行器 + `OptimizationExecutionResult` 优化执行结果，复用既有 `CostAnalyzer`/`CostReport`/`ProviderCost`）。
6. **租户资源配额与行级安全增强**：sz-orm-core 扩展（`TenantResourceQuota` 租户资源配额 + `QuotaEnforcer` 配额执行器 + `RlsPolicyEnhancer` RLS 策略增强器 + `EnhancedRlsPolicy` 增强行级安全策略 + `TenantAuditLogger` 租户级审计日志器 + `TenantAuditEntry` 审计条目，复用既有 `ConnectionTenantBinder`/`RowLevelSecurityPolicy`/`TenantAuditContext`）。
7. **缓存预热与穿透防护**：sz-orm-core 扩展（`CacheWarmer` 缓存预热器 + `WarmupConfig` 预热配置 + `BloomFilter` 布隆过滤器 + `PenetrationGuard` 穿透防护器 + `SingleFlight` 击穿防护器 + `StampedeGuard` 击穿防护配置，复用既有 `ProcessL1Cache`/`CrossSessionIdentityMap`/`L2Cache`）。
8. **需求追溯矩阵**：本文档第 7 章，建立需求 ↔ 验收条件映射。
9. **验收标准总览**：本文档第 8 章，按需求项汇总验收条件。

## 1.4 职责边界

本组件**不负责**以下事项：

1. **不破坏既有公开 API**：所有新能力通过 feature gate 隔离，既有公开 API 签名保持完全向后兼容。既有 `MessageQueue` trait（`packages/sz-orm-queue/src/queue.rs:18`）与 v4.6.0 `RedeliveryScheduler`（`packages/sz-orm-queue/src/dlx.rs:216`）保留不动，新增延迟队列与优先级调度为扩展 API。
2. **不改变既有安全铁律**：任何 WHERE 条件必须参数化，默认禁止 `SELECT *`，N+1 检测自动拦截，多租户隔离须防止租户越权，RLS 自动注入须参数化绑定，沿用既有铁律。
3. **不重写消息队列核心**：既有 `MessageQueue` trait（`packages/sz-orm-queue/src/queue.rs:18`）/ `InMemoryQueue`（`:339`）/ `Message`（`:57`）/ `nack`（`:37`）/ `reject`（`:44`）/ `requeue_dead_letter`（`:484`）+ v4.6.0 `RedeliveryScheduler`（`packages/sz-orm-queue/src/dlx.rs:216`）/ `BackoffPolicy`（`:47`）/ `DlxRoutingStrategy`（`:83`）保留不动，延迟队列与优先级调度为扩展，不修改既有消息队列与 DLX 逻辑。
4. **不重写迁移核心**：既有 `Migration`（`packages/sz-orm-core/src/migration.rs:10`）/ `rollback`（`:587`）/ `down`（`:677`）/ `MigrationResolver`（`:62`）/ `FileMigrationResolver`（`:68`）/ `MigrationContext`（`:193`）+ 既有 `DryRunMigration`（`packages/sz-orm-core/src/migration_dry_run.rs:11`）/ `DryRunReport`（`:24`）/ `ImpactReport`（`:80`）+ v4.6.0 `RollbackExecutor`（`packages/sz-orm-core/src/rollback_zero_downtime.rs:305`）/ `ZeroDowntimeRollbackStrategy`（`:73`）保留不动，前向兼容性检查与沙箱预演为扩展，不修改既有迁移/dry-run/回滚逻辑。
5. **不重写批量执行器**：既有 `BatchExecutor`（`packages/sz-orm-batch/src/executor.rs:141`）/ `BatchExecutorConfig`（`:18`）/ `BatchExecutionResult`（`:93`）+ 既有 `CopyProtocolExecutor`（`packages/sz-orm-batch/src/copy.rs:14`）+ v4.6.0 `BatchTransactionCoordinator`（`packages/sz-orm-batch/src/atomic.rs:216`）/ `AtomicityGuarantee`（`:20`）/ `SagaCompensator`（`:436`）保留不动，COPY 协议方言适配与并行分片执行为扩展，不修改既有批量执行/COPY/原子性逻辑。
6. **不重写可观测性核心**：既有 `MetricsRegistry`（`packages/sz-orm-observability/src/lib.rs:259`）/ `SloMonitor`（`packages/sz-orm-observability/src/slo.rs:223`）/ `QueryLogger`（`packages/sz-orm-observability/src/query_logger.rs:73`）+ v4.6.0 `AnomalyDetector`（`packages/sz-orm-observability/src/anomaly.rs:254`）/ `AnomalyAlert`（`:206`）/ `AnomalyAlgorithm`（`:40`）保留不动，异常自愈与根因分析为扩展，不修改既有指标/日志/异常检测逻辑。
7. **不重写存储核心**：既有 `Storage` trait（`packages/sz-orm-storage/src/storage.rs:14`）/ `StorageBuilder`（`:22`）/ `StorageProvider`（`:287`）/ `BucketLifecycle`（`packages/sz-orm-storage/src/advanced.rs:438`）/ `LifecycleRule`（`:400`）+ v4.6.0 `CostAnalyzer`（`packages/sz-orm-storage/src/cost.rs:231`）/ `CostReport`（`:213`）/ `CostOptimizationSuggestion`（`:55`）保留不动，多云成本对比与容量预测为扩展，不修改既有存储/生命周期/成本分析逻辑。
8. **不重写连接池与多租户核心**：既有 `Pool`（`packages/sz-orm-core/src/pool.rs:743`）/ `Connection` trait（`:45`）/ `PooledConnection`（`:239`）/ `TenantContext`（`packages/sz-orm-core/src/tenant_context.rs:80`）/ `IsolationStrategy`（`:22`）/ `TenantPoolRegistry`（`:224`）+ 既有 `RowLevelSecurityPolicy`（`packages/sz-orm-core/src/tenant_security.rs:67`）/ `ColumnMaskingRule`（`:155`）/ `TenantAuditContext`（`:244`）+ v4.6.0 `ConnectionTenantBinder`（`packages/sz-orm-core/src/connection_tenant.rs:133`）/ `ConnectionLevelIsolation`（`:24`）保留不动，租户资源配额与 RLS 增强为扩展，不修改既有连接池/多租户/RLS 逻辑。
9. **不重写缓存核心**：既有 `L1Cache`（`packages/sz-orm-core/src/l1_cache.rs:87`）/ `L2Cache`（`packages/sz-orm-core/src/l2_cache.rs:517`）/ `CacheKey`（`:143`）/ `CacheCoherenceProtocol`（`packages/sz-orm-core/src/cache_coherence.rs:103`）/ `MesiState`（`:12`）+ v4.6.0 `ProcessL1Cache`（`packages/sz-orm-core/src/process_l1_cache.rs:169`）/ `CrossSessionIdentityMap`（`:366`）保留不动，缓存预热与穿透防护为扩展，不修改既有 L1/L2/进程级 L1 逻辑。
10. **不与 v4.6.0 任务重叠**：v4.6.0 已占用的包/模块（`sz-orm-queue` dlx / `sz-orm-core` rollback_zero_downtime / `sz-orm-batch` atomic / `sz-orm-observability` anomaly / `sz-orm-storage` cost / `sz-orm-core` connection_tenant / `sz-orm-core` process_l1_cache）本版本不触碰其新增逻辑，新增范围全部落在既有包扩展（sz-orm-queue / sz-orm-core / sz-orm-batch / sz-orm-observability / sz-orm-storage）。
11. **不负责 sz-pay / sz-rust 下游代码修改**：ADR-0001 严禁修改下游/上游仓库，仅保证 API 兼容性。
12. **不降低既有测试覆盖**：v4.7.0 不得使 v4.6.0 已验收测试基线回退，仅增不减。
13. **不引入 unsafe**：所有新增代码无 `unsafe` 块，或必须有 `// SAFETY:` 注释，沿用既有 unsafe 零容忍铁律。
14. **不引入 Breaking Change**：新能力通过 feature gate 隔离，默认全关闭，既有 feature 组合行为不变。
15. **不强制启用新能力**：所有新能力默认关闭或可选启用，避免无配置环境行为变化。
16. **不做跨语言分布式事务**：跨语言（Java/Go/C++）互操作的分布式事务需跨语言协议适配，排除出本版本范围。
17. **不做低代码双向同步**：低代码 ↔ 代码双向同步需可视化设计器与代码生成器深度集成，无既有代码基础，排除出本版本范围。
18. **不做 OpenAPI 反向生成**：OpenAPI → ORM 反向生成需 OpenAPI 规范解析与 ORM 模型生成，无既有代码基础，排除出本版本范围。
19. **不做 WASM 真实数据库连接**：WASM 真实数据库连接需 WASM 兼容的数据库驱动，无既有代码基础，排除出本版本范围。
20. **不做异常自愈的全自动执行**：异常自愈须人工确认（可配自动执行白名单），避免误修复导致更大故障，自愈动作须记录审计日志供追溯。

---

# 2. 领域术语

**消息延迟队列与优先级调度（Delayed Priority Queue）**

: 扩展既有 `MessageQueue` trait（`packages/sz-orm-queue/src/queue.rs:18`）与 v4.6.0 `RedeliveryScheduler`（`packages/sz-orm-queue/src/dlx.rs:216`），补齐延迟投递（delayed delivery，消息按指定时间投递，非立即投递）+ 优先级队列（priority queue，高优先级消息优先投递）+ 定时调度（scheduled message，周期性/定时消息），使消息队列支持时间维度和优先级维度的精确控制。
: 备注：v4.6.0 已有 DLX 自动重投递调度器，本版本补延迟/优先级/定时调度。

**延迟投递（Delayed Delivery）**

: 消息按指定时间（绝对时间或相对延迟）投递，非立即投递，`DelayedMessage` 结构含 `deliver_at` 投递时间，到期前不可消费。

**优先级队列（Priority Queue）**

: 消息按优先级排序投递，高优先级消息优先消费，`PriorityPolicy` 枚举（Strict 严格优先级 / Weighted 加权优先级 / FairShare 公平份额），可配（默认 Strict）。

**定时调度（Scheduled Message）**

: 周期性或定时消息，`ScheduledMessage` 结构含 `cron` Cron 表达式或 `interval` 间隔，按调度周期自动投递消息。

**前向兼容性检查（Forward Compatibility Check）**

: 扩展既有迁移 dry-run（`packages/sz-orm-core/src/migration_dry_run.rs:11` `DryRunMigration`），检查新 schema 不破坏旧应用的兼容性（如删除列/改列类型/改列约束可能破坏旧应用），生成 `CompatCheckResult` 兼容性检查结果。

**沙箱预演（Sandbox Dry-Run）**

: 扩展既有 dry-run 与 v4.6.0 零停机回滚（`packages/sz-orm-core/src/rollback_zero_downtime.rs:305` `RollbackExecutor`），在影子表上预执行迁移并验证（数据完整性 + 查询兼容性 + 性能影响），不修改真实数据。

**迁移依赖图分析（Migration Dependency Graph Analysis）**

: 分析迁移间的依赖关系（迁移 A 依赖迁移 B 的 schema 变更），生成 `MigrationDependencyGraph` 依赖图，用于迁移执行顺序规划与冲突检测。

**批量 COPY 协议（Batch COPY Protocol）**

: 扩展既有 `CopyProtocolExecutor`（`packages/sz-orm-batch/src/copy.rs:14`，仅 PostgreSQL 系），补齐 MySQL LOAD DATA INFILE / Oracle SQL*Loader / MSSQL BULK INSERT 方言适配，通过 COPY 协议高速批量加载（跳过 SQL 解析，性能高于 multi-value INSERT）。

**并行分片执行（Parallel Shard Execution）**

: 跨分片并行批量操作，`ParallelShardExecutor` 将批量操作按分片键拆分到多个分片并行执行，提升批量加载吞吐量。

**批量冲突解决策略（Conflict Resolution）**

: 批量操作遇到冲突（主键/唯一键冲突）时的解决策略，`ConflictResolution` 枚举（Upsert 插入或更新 / Ignore 忽略冲突 / Merge 合并 / Replace 替换），可配（默认 Upsert）。

**异常自愈（Auto Remediation）**

: 扩展 v4.6.0 异常检测（`packages/sz-orm-observability/src/anomaly.rs:254` `AnomalyDetector`），检测到异常后自动执行预设修复动作（`RemediationAction` 枚举：RestartConnection 重启连接 / ClearCache 清缓存 / ScaleOut 扩容 / CustomAction 自定义），须人工确认（可配自动执行白名单），记录审计日志。

**根因分析（Root Cause Analysis）**

: 异常归因到具体组件/SQL/连接，`RootCauseAnalyzer` 基于异常上下文（指标 + 日志 + 拓扑）分析根因，生成 `RootCause` 根因结果（含根因组件 + 根因 SQL + 置信度 + 证据链）。

**异常关联分析（Anomaly Correlation Analysis）**

: 跨指标关联异常事件，`AnomalyCorrelator` 分析多个异常事件的时间/空间关联性，识别同根因的异常集群，生成 `CorrelationResult` 关联结果。

**多云成本对比（Multi-Cloud Cost Comparison）**

: 扩展 v4.6.0 成本分析（`packages/sz-orm-storage/src/cost.rs:231` `CostAnalyzer`），跨 provider 成本对比（同容量在不同 provider 的成本差异），生成 `CostComparisonReport` 成本对比报表，支持 provider 迁移建议。

**容量预测（Capacity Forecasting）**

: 基于历史趋势预测未来存储需求，`CapacityForecaster` 基于历史容量数据（时间序列）预测未来容量（线性回归 / 指数平滑 / 可配算法），生成 `CapacityForecast` 容量预测结果（含预测容量 + 置信区间 + 预测算法）。

**成本自动优化执行（Auto Execute Optimization）**

: 扩展 v4.6.0 成本优化建议（`packages/sz-orm-storage/src/cost.rs:55` `CostOptimizationSuggestion`），自动执行优化建议（如自动降级冷数据到低成本 tier），而非仅生成建议，须人工确认（可配自动执行白名单）。

**租户资源配额（Tenant Resource Quota）**

: 扩展 v4.6.0 连接级多租户隔离（`packages/sz-orm-core/src/connection_tenant.rs:133` `ConnectionTenantBinder`），按租户设置资源配额（max connections 最大连接数 / max QPS 最大查询速率 / max storage 最大存储容量），`QuotaEnforcer` 执行配额检查与限流。

**行级安全策略增强（RLS Policy Enhancement）**

: 扩展既有 `RowLevelSecurityPolicy`（`packages/sz-orm-core/src/tenant_security.rs:67`），增强策略表达能力（多条件组合 / 复杂谓词 / 列级脱敏联动），`RlsPolicyEnhancer` 自动注入 WHERE 条件（参数化绑定），防止跨租户数据泄露。

**租户级审计日志（Tenant-Level Audit Log）**

: 按租户独立审计日志，`TenantAuditLogger` 记录租户操作（连接获取 / 查询执行 / 配额超限 / RLS 命中），复用既有 `TenantAuditContext`（`packages/sz-orm-core/src/tenant_security.rs:244`），供合规审计。

**缓存预热（Cache Warmup）**

: 扩展 v4.6.0 进程级 L1 缓存（`packages/sz-orm-core/src/process_l1_cache.rs:169` `ProcessL1Cache`），启动时预加载热点数据到 L1+L2，减少冷启动缓存未命中，`CacheWarmer` 按预热策略（热点表 / 热点主键 / 自定义查询）预加载。

**缓存穿透防护（Cache Penetration Protection）**

: 布隆过滤器拦截不存在的 key，`BloomFilter` 在查询前判断 key 是否可能存在，不存在则直接返回 None 不查 DB，避免大量不存在的 key 击穿到 DB。

**缓存击穿防护（Cache Stampede Protection）**

: singleflight 机制防止缓存重建风暴，`SingleFlight` 对同一 key 的并发重建请求只执行一次，其他请求等待结果复用，避免热点 key 过期后大量请求同时重建缓存。

**v4.7.0 feature gate**

: 控制本版本新能力的 feature gate 集合（`delayed-priority-queue` / `forward-compat-sandbox` / `copy-parallel-shard` / `anomaly-remediation-rca` / `multicloud-cost-forecast` / `tenant-quota-rls-enhanced` / `cache-warmup-protection`），默认关闭，避免无配置环境行为变化。

---

# 3. 角色与边界

## 3.1 核心角色

- **ORM 库维护者**：执行 v4.7.0 七项智能化运维深化与性能深化能力的开发、验证、测试操作者，是新增能力的主要使用者与验收人。
- **下游项目开发者（sz-pay）**：关注 API 兼容性的下游使用者，v4.7.0 不得破坏其既有代码。
- **应用开发者**：使用延迟队列与优先级调度精确控制消息投递，使用前向兼容性检查与沙箱预演保证迁移安全，使用 COPY 协议与并行分片执行提升批量加载性能，使用异常自愈与根因分析快速定位修复异常，使用多云成本对比与容量预测优化存储成本，使用租户资源配额与 RLS 增强多租户安全，使用缓存预热与穿透防护提升缓存稳定性。
- **DBA / 性能工程师**：评估 COPY 协议与并行分片执行对数据库负载的影响，评估前向兼容性检查与沙箱预演对数据库的影响，评估租户资源配额对连接池与 QPS 的影响，评估缓存预热对内存与命中率的影响。
- **运维/SRE 工程师**：配置延迟队列与优先级调度策略，配置前向兼容性检查与沙箱预演，配置异常自愈白名单与根因分析，配置多云成本对比与容量预测周期，配置租户资源配额与 RLS 策略，配置缓存预热与穿透防护参数，监控智能化运维深化能力对系统的影响。
- **FinOps / 成本工程师**：使用多云成本对比评估跨 provider 成本，使用容量预测规划未来存储容量，使用成本自动优化执行降低存储成本。

## 3.2 外部系统

- **MySQL 9.6 / PostgreSQL 18 / SQLite / Oracle 23ai / MSSQL**：COPY 协议/并行分片/租户配额/RLS 增强的五方言覆盖目标，PostgreSQL 系原生支持 COPY 协议，其他方言降级为 multi-value INSERT 或方言特定批量加载（MySQL LOAD DATA INFILE / Oracle SQL*Loader / MSSQL BULK INSERT）。
- **DBRD**：批量 COPY 协议与并行分片执行通过既有连接池 `Pool`（`packages/sz-orm-core/src/pool.rs:743`）获取连接执行 SQL，租户资源配额在连接池层限流。
- **消息队列 provider**：延迟队列与优先级调度复用既有 6 provider（RabbitMQ/Kafka/NATS/ActiveMQ/Pulsar/RocketMQ）+ v4.6.0 `RedeliveryScheduler` 调度器。
- **对象存储 provider**：多云成本对比与容量预测复用既有 7 provider（Local/S3/Aliyun/Tencent/Huawei/Upyun/Qiniu）+ v4.6.0 `CostAnalyzer`。
- **sz-pay 项目**：API 兼容性验证的下游基准。

## 3.3 交互上下文

```plantuml
@startuml
title sz-orm v4.7.0 七项智能化运维深化与性能深化能力 交互上下文

rectangle "sz-orm v4.7.0" as orm {
  rectangle "延迟队列与优先级调度\n(延迟+优先级+定时)" as dpq
  rectangle "前向兼容与沙箱预演\n(兼容检查+沙箱+依赖图)" as compat
  rectangle "COPY 协议与并行分片\n(COPY+分片+冲突解决)" as copy
  rectangle "异常自愈与根因分析\n(自愈+RCA+关联)" as remediation
  rectangle "多云成本对比与容量预测\n(对比+预测+自动优化)" as multicloud
  rectangle "租户配额与 RLS 增强\n(配额+RLS+审计)" as quota
  rectangle "缓存预热与穿透防护\n(预热+布隆+singleflight)" as warmup
}

actor "ORM 库维护者" as dev
actor "应用开发者" as appdev
actor "DBA/性能工程师" as dba
actor "运维/SRE" as sre
actor "FinOps/成本工程师" as finops
actor "sz-pay 开发者" as downstream

database "MySQL/PG/SQLite\n/Oracle/MSSQL" as db
rectangle "连接池\n(既有 Pool)" as pool
rectangle "消息队列\n(既有 6 provider\n+v4.6.0 RedeliveryScheduler)" as mq
rectangle "对象存储\n(既有 7 provider\n+v4.6.0 CostAnalyzer)" as storage
rectangle "sz-orm-batch\n(既有 CopyProtocolExecutor\n+v4.6.0 BatchTransactionCoordinator)" as batch
rectangle "sz-orm-observability\n(v4.6.0 AnomalyDetector)" as obs

dev --> orm : 开发/验证/测试
appdev --> dpq : 消息时间优先级控制
dpq --> mq : 复用既有队列+调度器
appdev --> compat : 迁移前向兼容+沙箱预演
compat --> db : 影子表预演+依赖图分析
appdev --> copy : COPY 高速加载+并行分片
copy --> batch : 复用既有 COPY+原子性
copy --> db : COPY/LOAD DATA/分片并行
appdev --> remediation : 异常自愈+根因分析
remediation --> obs : 复用 v4.6.0 异常检测
remediation --> db : 自愈动作+根因定位
finops --> multicloud : 多云成本对比+容量预测
multicloud --> storage : 复用既有 7 provider+v4.6.0 成本分析
appdev --> quota : 租户配额+RLS 增强
quota --> pool : 复用既有连接池+连接级隔离
quota --> db : RLS 自动注入 WHERE
appdev --> warmup : 缓存预热+穿透防护
warmup --> db : 预加载热点+布隆过滤
dba --> copy : 评估 COPY 负载影响
dba --> compat : 评估沙箱预演影响
dba --> quota : 评估配额对连接池影响
dba --> warmup : 评估内存与命中率影响
sre --> dpq : 配置延迟+优先级策略
sre --> compat : 配置兼容检查+沙箱
sre --> remediation : 配置自愈白名单+RCA
sre --> multicloud : 配置对比+预测周期
sre --> quota : 配置配额+RLS 策略
sre --> warmup : 配置预热+防护参数
downstream --> orm : API 兼容性验证

@enduml
```

---

# 4. DFX约束

## 4.1 性能

1. **延迟队列调度开销**：延迟队列调度器检查开销不超过 1ms/次（含到期判定 + 优先级排序），不显著影响消息队列吞吐量。
2. **优先级队列排序开销**：优先级队列插入排序开销不超过 O(log n)（基于 BinaryHeap），高优先级消息投递延迟不超过 1ms（相比同优先级消息）。
3. **前向兼容性检查开销**：前向兼容性检查开销不超过 500ms/迁移（含 schema 解析 + 兼容性规则匹配），不显著影响迁移准备时间。
4. **沙箱预演开销**：沙箱预演开销不超过 10 秒/迁移（含影子表创建 + 迁移执行 + 数据校验 + 影子表清理），可在迁移前异步执行。
5. **COPY 协议加载性能**：COPY 协议批量加载性能应不低于 multi-value INSERT 的 3 倍（PostgreSQL COPY / MySQL LOAD DATA INFILE），加载 100 万行不超过 10 秒。
6. **并行分片执行吞吐量**：并行分片执行吞吐量应不低于单分片执行的 N 倍（N = 分片数，含分片拆分 + 并行执行 + 结果合并开销），分片间负载均衡。
7. **异常自愈响应时间**：异常自愈从检测到异常到执行修复动作不超过 5 秒（含根因分析 + 修复动作选择 + 人工确认等待，自动执行白名单内无需等待）。
8. **根因分析开销**：根因分析开销不超过 2 秒/异常（含上下文收集 + 根因推断 + 证据链构建），不显著影响异常检测性能。
9. **多云成本对比开销**：多云成本对比开销不超过 10 秒（含 7 provider 成本统计 + 对比分析 + 报表生成），可周期性执行。
10. **容量预测开销**：容量预测开销不超过 5 秒（含历史数据加载 + 预测算法计算 + 置信区间生成），可周期性执行。
11. **租户配额检查开销**：租户配额检查开销不超过 0.1ms/次（含连接数/QPS/存储配额检查），不显著影响连接获取与查询性能。
12. **RLS 自动注入开销**：RLS 自动注入 WHERE 条件开销不超过 0.2ms/查询（含策略匹配 + WHERE 条件生成 + 参数化绑定），不显著影响查询性能。
13. **缓存预热开销**：缓存预热开销不超过 30 秒（含热点数据识别 + 预加载到 L1+L2），可在启动时异步执行不阻塞服务启动。
14. **布隆过滤器查询开销**：布隆过滤器查询开销不超过 100ns/次（含哈希计算 + 位数组查询），不显著影响缓存查询性能。
15. **singleflight 开销**：singleflight 机制开销不超过 1ms/次（含 key 锁定 + 等待/复用结果），不显著影响缓存重建性能。

## 4.2 可靠性

1. **延迟队列不丢失消息**：延迟消息在到期前须保留在延迟队列中，到期后按优先级投递，投递失败时按 v4.6.0 退避策略重试，不丢失消息。
2. **优先级队列不饿死低优先级**：优先级队列须避免低优先级消息饿死（Strict 策略下高优先级持续投递时低优先级等待，可配 aging 机制提升等待过久的低优先级消息优先级）。
3. **前向兼容性检查不误报**：前向兼容性检查须准确识别破坏性变更（删除列/改列类型/改列约束），不误报非破坏性变更（加列/加索引/加表），检查结果附兼容性证据。
4. **沙箱预演不修改真实数据**：沙箱预演须在影子表上执行，不修改真实数据，预演失败时清理影子表不残留。
5. **COPY 协议不丢数据**：COPY 协议批量加载须保证数据完整性（加载成功的数据全部落库，加载失败时按冲突解决策略处理，不部分丢失）。
6. **并行分片执行原子性**：并行分片执行须保证分片间原子性（可配原子性级别，复用 v4.6.0 `AtomicityGuarantee` `packages/sz-orm-batch/src/atomic.rs:20`），分片失败时按原子性级别处理。
7. **异常自愈不误修复**：异常自愈须人工确认（可配自动执行白名单），自愈动作须记录审计日志供追溯，不静默执行。
8. **根因分析不误判**：根因分析须附证据链（指标 + 日志 + 拓扑），置信度可配阈值，低置信度时标注"根因不确定，需人工排查"。
9. **多云成本对比数据准确**：多云成本对比须使用 provider API 返回的计费数据（非估算），对比数据准确反映实际成本差异。
10. **容量预测附置信区间**：容量预测须附置信区间（如 95% 置信区间），不单点预测，预测算法可配（线性回归 / 指数平滑）。
11. **租户配额不超限**：租户配额须严格执行（连接数/QPS/存储超限时拒绝请求，不超配），配额超限记录审计日志。
12. **RLS 自动注入防越权**：RLS 自动注入 WHERE 条件须参数化绑定，tenant_id 不可被客户端篡改（由可信路径设置），防止跨租户数据泄露。
13. **缓存预热不阻塞启动**：缓存预热须异步执行不阻塞服务启动，预热失败时不影响服务启动（记录日志，启动后按需加载）。
14. **布隆过滤器可配误判率**：布隆过滤器误判率可配（默认 1%），误判时回退到 DB 查询（不返回错误），不漏判（不存在的 key 一定返回 None）。
15. **singleflight 不死锁**：singleflight 机制须避免死锁（重建超时后释放锁，其他请求可重试），不长期阻塞。
16. **v4.6.0 测试基线不回退**：v4.7.0 不得使 v4.6.0 已验收测试基线回退，仅增不减。

## 4.3 安全性

1. **延迟队列消息不泄露**：延迟消息须保留原始消息的脱敏状态（复用既有 `sz-orm-masking` 脱敏），不泄露敏感数据。
2. **沙箱预演 SQL 参数化**：沙箱预演 SQL 须参数化绑定（复用既有 `Connection::execute_with_params` `packages/sz-orm-core/src/pool.rs:82`），禁止 SQL 字符串拼接，防止 SQL 注入。
3. **COPY 协议数据不泄露**：COPY 协议加载数据须按租户隔离（多租户环境下数据按 tenant_id 隔离），不泄露跨租户数据。
4. **异常自愈动作审计**：异常自愈动作须记录审计日志（异常 ID + 修复动作 + 执行人 + 执行时间 + 结果），供合规审计。
5. **根因分析不泄露敏感信息**：根因分析证据链须脱敏敏感信息（如 SQL 参数值、连接凭据），不泄露敏感信息。
6. **多云成本对比不泄露凭据**：多云成本对比须使用既有存储凭据（`StorageBuilder` `packages/sz-orm-storage/src/storage.rs:22`），不暴露凭据。
7. **租户配额防绕过**：租户配额检查须在连接池/查询执行层强制执行，不可被应用层绕过，配额超限拒绝请求。
8. **RLS 自动注入防注入**：RLS 自动注入的 WHERE 条件须参数化绑定，tenant_id 不可被客户端篡改（由可信路径设置），防止 SQL 注入与越权。
9. **租户级审计日志不可篡改**：租户级审计日志须追加写入（不可修改/删除），供合规审计。
10. **缓存预热不泄露数据**：缓存预热须按租户隔离（多租户环境下预热数据按 tenant_id 隔离），不泄露跨租户数据。
11. **布隆过滤器不泄露 key**：布隆过滤器仅存储 key 的哈希（不存储原始 key），不泄露敏感 key。
12. **审计证据要求**：每项需求结论须附 file:line 证据，遵循 AGENTS.md 审计合规铁律。

## 4.4 可维护性

1. **延迟队列与优先级调度可配置**：延迟策略、优先级策略、定时调度 Cron 须可配置，不强制干扰开发者，默认值保守（Strict 优先级，延迟到期立即投递）。
2. **前向兼容性检查可配置**：兼容性规则（哪些变更视为破坏性）、检查严格度须可配置，不强制干扰开发者。
3. **沙箱预演可配置**：沙箱表命名、预演验证项（数据完整性/查询兼容性/性能影响）须可配置，不强制干扰开发者。
4. **COPY 协议与并行分片可配置**：COPY 协议方言适配、分片键、分片数、冲突解决策略须可配置，不强制干扰开发者。
5. **异常自愈可配置**：自愈动作白名单、自动执行阈值、根因分析置信度阈值须可配置，不强制干扰开发者。
6. **多云成本对比与容量预测可配置**：对比周期、预测算法、预测周期、自动优化白名单须可配置，不强制干扰开发者。
7. **租户配额与 RLS 可配置**：租户配额值、RLS 策略、审计日志级别须可配置，不强制干扰开发者。
8. **缓存预热与穿透防护可配置**：预热策略、预热数据量、布隆过滤器容量与误判率、singleflight 超时须可配置，不强制干扰开发者。
9. **五方言一致**：新增能力在 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 五方言上行为一致（COPY 协议/并行分片/租户配额/RLS 增强按方言能力适配）。
10. **智能化运维审计可追溯**：延迟队列与优先级调度须记录调度日志，前向兼容性检查须记录检查结果，沙箱预演须记录预演日志，异常自愈须记录自愈审计日志，多云成本对比须记录对比报表，容量预测须记录预测结果，租户配额须记录配额超限日志，RLS 增强须记录策略命中日志，缓存预热须记录预热日志，供审计追溯。

## 4.5 兼容性

1. **API 向后兼容**：所有新能力通过 feature gate 隔离，默认 feature 行为不变，既有公开 API 完全向后兼容。
2. **sz-pay 不破坏**：sz-pay 从 crates.io 拉取的 sz-orm-* 6 个包既有用法不受影响。
3. **既有消息队列与 DLX 保留**：既有 `MessageQueue` trait（`packages/sz-orm-queue/src/queue.rs:18`）/ `InMemoryQueue`（`:339`）/ `Message`（`:57`）+ v4.6.0 `RedeliveryScheduler`（`packages/sz-orm-queue/src/dlx.rs:216`）/ `BackoffPolicy`（`:47`）/ `DlxRoutingStrategy`（`:83`）保留不动，延迟队列与优先级调度为扩展。
4. **既有迁移与 dry-run 与回滚保留**：既有 `Migration`（`packages/sz-orm-core/src/migration.rs:10`）/ `rollback`（`:587`）/ `down`（`:677`）/ `DryRunMigration`（`packages/sz-orm-core/src/migration_dry_run.rs:11`）/ `ImpactReport`（`:80`）+ v4.6.0 `RollbackExecutor`（`packages/sz-orm-core/src/rollback_zero_downtime.rs:305`）保留不动，前向兼容性检查与沙箱预演为扩展。
5. **既有批量执行与 COPY 与原子性保留**：既有 `BatchExecutor`（`packages/sz-orm-batch/src/executor.rs:141`）/ `CopyProtocolExecutor`（`packages/sz-orm-batch/src/copy.rs:14`）+ v4.6.0 `BatchTransactionCoordinator`（`packages/sz-orm-batch/src/atomic.rs:216`）保留不动，COPY 协议方言适配与并行分片执行为扩展。
6. **既有可观测性与异常检测保留**：既有 `MetricsRegistry`（`packages/sz-orm-observability/src/lib.rs:259`）/ `QueryLogger`（`packages/sz-orm-observability/src/query_logger.rs:73`）+ v4.6.0 `AnomalyDetector`（`packages/sz-orm-observability/src/anomaly.rs:254`）保留不动，异常自愈与根因分析为扩展。
7. **既有存储与成本分析保留**：既有 `Storage` trait（`packages/sz-orm-storage/src/storage.rs:14`）/ `BucketLifecycle`（`packages/sz-orm-storage/src/advanced.rs:438`）+ v4.6.0 `CostAnalyzer`（`packages/sz-orm-storage/src/cost.rs:231`）保留不动，多云成本对比与容量预测为扩展。
8. **既有连接池与多租户与 RLS 保留**：既有 `Pool`（`packages/sz-orm-core/src/pool.rs:743`）/ `TenantContext`（`packages/sz-orm-core/src/tenant_context.rs:80`）/ `RowLevelSecurityPolicy`（`packages/sz-orm-core/src/tenant_security.rs:67`）+ v4.6.0 `ConnectionTenantBinder`（`packages/sz-orm-core/src/connection_tenant.rs:133`）保留不动，租户资源配额与 RLS 增强为扩展。
9. **既有缓存与进程级 L1 保留**：既有 `L1Cache`（`packages/sz-orm-core/src/l1_cache.rs:87`）/ `L2Cache`（`packages/sz-orm-core/src/l2_cache.rs:517`）/ `CacheCoherenceProtocol`（`packages/sz-orm-core/src/cache_coherence.rs:103`）+ v4.6.0 `ProcessL1Cache`（`packages/sz-orm-core/src/process_l1_cache.rs:169`）保留不动，缓存预热与穿透防护为扩展。
10. **既有 feature 组合不破坏**：v4.7.0 新增 feature（`delayed-priority-queue` / `forward-compat-sandbox` / `copy-parallel-shard` / `anomaly-remediation-rca` / `multicloud-cost-forecast` / `tenant-quota-rls-enhanced` / `cache-warmup-protection`）与既有 feature（含 v4.3.0 7 个 + v4.4.0 6 个 + v4.5.0 3 个 + v4.6.0 7 个）任意组合编译通过。

---

# 5. 核心能力

## 5.1 消息延迟队列与优先级调度（REQ-V47-001，P1）

### 5.1.1 业务规则

1. **延迟投递**（EARS: Ubiquitous）
   系统应当扩展既有 `sz-orm-queue` 包，提供 `DelayedMessage` 延迟消息结构（含 `deliver_at` 投递时间，绝对时间或相对延迟），消息按指定时间投递，到期前不可消费，复用既有 `Message`（`packages/sz-orm-queue/src/queue.rs:57`）与 `MessageQueue` trait（`:18`）。
   a. 验收条件：[发布延迟消息 deliver_at=2026-08-13 10:00:00 + 当前 09:00:00] → [消息在 10:00:00 前不可消费，10:00:00 后可消费]
2. **优先级队列**（EARS: Ubiquitous）
   系统应当提供 `PriorityQueue` 优先级队列与 `PriorityPolicy` 优先级策略枚举（Strict 严格优先级 / Weighted 加权优先级 / FairShare 公平份额），消息按优先级排序投递，高优先级消息优先消费，策略可配（默认 Strict）。
   a. 验收条件：[Strict 策略 + 消息 A 优先级 10 + 消息 B 优先级 5] → [消息 A 先于消息 B 投递]；[Weighted 策略 + 权重配置] → [按权重比例投递]
3. **定时调度**（EARS: Ubiquitous）
   系统应当提供 `ScheduledMessage` 定时消息结构（含 `cron` Cron 表达式或 `interval` 间隔），按调度周期自动投递消息，复用 v4.6.0 `RedeliveryScheduler`（`packages/sz-orm-queue/src/dlx.rs:216`）调度器基线。
   a. 验收条件：[定时消息 cron="0 * * * *" + 启用调度] → [每分钟自动投递一条消息]
4. **延迟与优先级组合**（EARS: Ubiquitous）
   系统应当支持延迟与优先级组合（延迟到期后按优先级投递），延迟期间不参与优先级排序，到期后加入优先级队列。
   a. 验收条件：[消息 A deliver_at=10:00 优先级 5 + 消息 B deliver_at=09:00 优先级 10 + 当前 09:30] → [消息 B 已到期按优先级 10 投递，消息 A 未到期不投递；10:00 后消息 A 到期按优先级 5 投递]
5. **优先级队列不饿死低优先级**（EARS: State-driven）
   当 Strict 策略下高优先级消息持续投递导致低优先级消息长时间等待时，系统应当按 aging 机制（可配，默认启用，aging 时间 5 分钟）提升等待过久的低优先级消息优先级，避免饿死。
   a. 验收条件：[Strict + aging 5 分钟 + 低优先级消息等待 6 分钟] → [提升优先级，避免饿死]
6. **复用既有消息队列与 DLX 调度器**（EARS: Ubiquitous）
   系统应当复用既有 `MessageQueue` trait（`packages/sz-orm-queue/src/queue.rs:18`）/ `Message`（`:57`）/ `InMemoryQueue`（`:339`）+ v4.6.0 `RedeliveryScheduler`（`packages/sz-orm-queue/src/dlx.rs:216`）/ `BackoffPolicy`（`:47`），延迟队列与优先级调度基于既有消息队列与调度器扩展，不重复实现消息存储与调度。
   a. 验收条件：[延迟队列与优先级调度] → [复用既有 MessageQueue/Message/RedeliveryScheduler，不新建消息存储与调度逻辑]
7. **调度日志可追溯**（EARS: Ubiquitous）
   系统应当记录调度日志（消息 ID + 延迟时间 + 优先级 + 投递时间 + 结果），供审计追溯，复用既有 `message_tracing` 模块（`packages/sz-orm-queue/src/message_tracing.rs`）。
   a. 验收条件：[延迟消息投递] → [记录调度日志，含消息 ID + 延迟时间 + 优先级 + 投递时间 + 结果]
8. **禁止项**（EARS: Unwanted）
   如果延迟队列与优先级调度影响默认 feature 编译或破坏既有消息队列/DLX，则系统应当通过 `delayed-priority-queue` feature gate 隔离，默认不启用延迟队列与优先级调度，且既有 `MessageQueue` trait 与 v4.6.0 `RedeliveryScheduler` 保留不动。
   a. 验收条件：[`cargo build` 默认编译] → [无延迟队列与优先级调度，行为与 v4.6.0 一致]

### 5.1.2 交互流程

```plantuml
@startuml
title 消息延迟队列与优先级调度 交互流程
actor "应用开发者" as appdev
participant "DelayScheduler\n(延迟调度器)" as delay
participant "PriorityQueue\n(优先级队列)" as pq
participant "MessageQueue\n(既有队列)" as mq
participant "RedeliveryScheduler\n(v4.6.0 调度器)" as redel

appdev -> delay : publish_delayed(message, deliver_at, priority)
delay -> delay : 存储延迟消息(按 deliver_at 排序)
loop 调度周期
  delay -> delay : 检查到期消息
  alt 有到期消息
    delay -> pq : enqueue(到期消息, priority)
    pq -> pq : 按优先级排序插入
    pq -> mq : dequeue 最高优先级 [复用既有]
    mq -> appdev : 投递消息
    delay -> delay : 记录调度日志
  end
end
appdev -> delay : publish_scheduled(message, cron)
delay -> delay : 按 Cron 调度
loop Cron 周期
  delay -> pq : enqueue(message, priority)
  pq -> mq : 投递 [复用既有]
end
@enduml
```

### 5.1.3 异常场景

1. **延迟消息投递失败**
   a. 触发条件：延迟消息到期投递时失败（如目标队列不可用）
   b. 系统行为：按 v4.6.0 退避策略重试（复用 `BackoffPolicy` `packages/sz-orm-queue/src/dlx.rs:47`），重试超限后进死信队列
   c. 用户感知：日志标注"delayed message delivery failed, retried N times"
2. **Cron 表达式无效**
   a. 触发条件：定时消息的 Cron 表达式无效
   b. 系统行为：拒绝发布，返回错误"invalid cron expression"
   c. 用户感知：错误"invalid cron expression: ..."
3. **优先级队列容量超限**
   a. 触发条件：优先级队列容量超限（可配，默认 100000）
   b. 系统行为：拒绝入队，返回错误"priority queue capacity exceeded"
   c. 用户感知：错误"priority queue capacity exceeded, please increase capacity or drain queue"

## 5.2 迁移前向兼容性检查与沙箱预演（REQ-V47-002，P1）

### 5.2.1 业务规则

1. **前向兼容性检查**（EARS: Ubiquitous）
   系统应当扩展既有 `sz-orm-core` 迁移管理，提供 `ForwardCompatChecker` 前向兼容性检查器，检查新 schema 不破坏旧应用的兼容性（如删除列/改列类型/改列约束可能破坏旧应用），生成 `CompatCheckResult` 兼容性检查结果（含破坏性变更列表 + 影响的应用 + 建议的兼容策略），复用既有 `DryRunMigration`（`packages/sz-orm-core/src/migration_dry_run.rs:11`）/ `ImpactReport`（`:80`）。
   a. 验收条件：[迁移删除列 "users.email" + 前向兼容性检查] → [识别为破坏性变更，生成 CompatCheckResult 标注"删除列 email 可能破坏依赖该列的旧应用"]
2. **沙箱预演**（EARS: Ubiquitous）
   系统应当提供 `SandboxDryRunner` 沙箱预演器，在影子表上预执行迁移并验证（数据完整性 + 查询兼容性 + 性能影响），不修改真实数据，复用既有 `DryRunMigration`（`packages/sz-orm-core/src/migration_dry_run.rs:11`）+ v4.6.0 `RollbackExecutor`（`packages/sz-orm-core/src/rollback_zero_downtime.rs:305`）影子表能力。
   a. 验收条件：[沙箱预演迁移 V + 影子表 shadow_users] → [在 shadow_users 上执行迁移 V，校验数据完整性/查询兼容性/性能影响，不修改真实 users 表]
3. **迁移依赖图分析**（EARS: Ubiquitous）
   系统应当提供 `MigrationDependencyGraph` 迁移依赖图与 `DependencyAnalyzer` 依赖分析器，分析迁移间的依赖关系（迁移 A 依赖迁移 B 的 schema 变更），用于迁移执行顺序规划与冲突检测。
   a. 验收条件：[迁移 A 依赖迁移 B + 依赖图分析] → [生成依赖图，标注 A 依赖 B，执行顺序须 B 先于 A]
4. **前向兼容性规则可配置**（EARS: Ubiquitous）
   系统应当支持前向兼容性规则可配置（哪些变更视为破坏性：删除列/改列类型/改列约束/改表名，可配），检查严格度可配（Strict 严格 / Lenient 宽松），不强制干扰开发者。
   a. 验收条件：[配置删除列为非破坏性 + 检查] → [删除列不视为破坏性变更]
5. **沙箱预演验证项可配置**（EARS: Ubiquitous）
   系统应当支持沙箱预演验证项可配置（数据完整性/查询兼容性/性能影响，可配），沙箱表命名可配，不强制干扰开发者。
   a. 验收条件：[配置仅验证数据完整性 + 沙箱预演] → [仅校验数据完整性，跳过查询兼容性与性能影响]
6. **复用既有 dry-run 与回滚**（EARS: Ubiquitous）
   系统应当复用既有 `DryRunMigration`（`packages/sz-orm-core/src/migration_dry_run.rs:11`）/ `DryRunReport`（`:24`）/ `MigrationImpact`（`:59`）/ `ImpactReport`（`:80`）+ v4.6.0 `RollbackExecutor`（`packages/sz-orm-core/src/rollback_zero_downtime.rs:305`）/ `RollbackPlan`（`:157`）+ 既有 `Migration`（`packages/sz-orm-core/src/migration.rs:10`）/ `MigrationResolver`（`:62`），前向兼容性检查与沙箱预演基于既有 dry-run/回滚扩展，不重复实现。
   a. 验收条件：[前向兼容性检查与沙箱预演] → [复用既有 DryRunMigration/ImpactReport/RollbackExecutor，不新建 dry-run/回滚逻辑]
7. **检查与预演日志可追溯**（EARS: Ubiquitous）
   系统应当记录检查与预演日志（迁移版本 + 检查结果 + 预演结果 + 耗时），供审计追溯。
   a. 验收条件：[前向兼容性检查 + 沙箱预演] → [记录检查日志（含破坏性变更列表）+ 预演日志（含验证结果）]
8. **禁止项**（EARS: Unwanted）
   如果前向兼容性检查与沙箱预演影响默认 feature 编译或破坏既有迁移/dry-run/回滚，则系统应当通过 `forward-compat-sandbox` feature gate 隔离，默认不启用，且既有 `Migration`/`DryRunMigration`/`RollbackExecutor` 保留不动。
   a. 验收条件：[`cargo build` 默认编译] → [无前向兼容性检查与沙箱预演，行为与 v4.6.0 一致]

### 5.2.2 交互流程

```plantuml
@startuml
title 迁移前向兼容性检查与沙箱预演 交互流程
actor "运维/SRE" as sre
participant "ForwardCompatChecker\n(兼容性检查器)" as checker
participant "SandboxDryRunner\n(沙箱预演器)" as sandbox
participant "DependencyAnalyzer\n(依赖分析器)" as dep
participant "DryRunMigration\n(既有 dry-run)" as dryrun
participant "RollbackExecutor\n(v4.6.0 回滚)" as rollback
database "DB" as db

sre -> checker : check_compatibility(migration V)
checker -> dryrun : analyze_impact(migration V) [复用既有]
dryrun --> checker : ImpactReport
checker -> checker : 识别破坏性变更(删除列/改类型/改约束)
checker --> sre : CompatCheckResult(破坏性变更列表+影响应用+建议)

sre -> sandbox : dry_run_sandbox(migration V, shadow_table)
sandbox -> db : 创建影子表 shadow_users
sandbox -> db : 在 shadow_users 执行迁移 V SQL [参数化绑定]
sandbox -> db : 校验数据完整性(shadow_users vs users)
sandbox -> db : 校验查询兼容性(旧查询在 shadow_users 可执行)
sandbox -> db : 校验性能影响(shadow_users 查询性能)
alt 校验通过
  sandbox --> sre : SandboxResult(通过)
else 校验失败
  sandbox --> sre : SandboxResult(失败+原因)
end
sandbox -> db : 清理影子表 shadow_users

sre -> dep : analyze_dependencies(migrations)
dep -> dep : 构建依赖图
dep --> sre : MigrationDependencyGraph(依赖关系+执行顺序)
@enduml
```

### 5.2.3 异常场景

1. **沙箱预演影子表创建失败**
   a. 触发条件：影子表创建失败（如权限不足、表名冲突）
   b. 系统行为：中止预演，返回错误"sandbox table creation failed"，不修改真实数据
   c. 用户感知：错误"sandbox dry-run failed, table creation error: ..."
2. **沙箱预演迁移 SQL 执行失败**
   a. 触发条件：影子表上迁移 SQL 执行失败
   b. 系统行为：中止预演，清理影子表，返回错误含 SQL 执行错误
   c. 用户感知：错误"sandbox migration SQL failed: ..."
3. **依赖图存在循环依赖**
   a. 触发条件：迁移间存在循环依赖（A 依赖 B，B 依赖 A）
   b. 系统行为：标注循环依赖，返回错误"circular dependency detected"，建议人工拆解
   c. 用户感知：错误"circular dependency detected between migration A and B"
4. **前向兼容性检查规则匹配异常**
   a. 触发条件：兼容性规则匹配异常（如规则配置错误）
   b. 系统行为：跳过该规则，记录日志"compat rule matching error, skipped"
   c. 用户感知：日志标注"compat check skipped, rule error"

## 5.3 批量 COPY 协议与并行分片执行（REQ-V47-003，P1）

### 5.3.1 业务规则

1. **COPY 协议方言适配**（EARS: Ubiquitous）
   系统应当扩展既有 `sz-orm-batch` 包，提供 `CopyProtocolAdapter` COPY 协议方言适配器，补齐 MySQL LOAD DATA INFILE / Oracle SQL*Loader / MSSQL BULK INSERT 方言适配（既有 `CopyProtocolExecutor` `packages/sz-orm-batch/src/copy.rs:14` 仅 PostgreSQL 系），通过 COPY 协议高速批量加载（跳过 SQL 解析，性能高于 multi-value INSERT）。
   a. 验收条件：[PostgreSQL + COPY 100 万行] → [使用 `COPY table FROM STDIN`，性能不低于 multi-value INSERT 3 倍]；[MySQL + LOAD DATA 100 万行] → [使用 `LOAD DATA INFILE`，性能不低于 multi-value INSERT 3 倍]；[SQLite 不支持 COPY] → [降级为 multi-value INSERT，标注"COPY not supported, fallback to multi-value INSERT"]
2. **并行分片执行**（EARS: Ubiquitous）
   系统应当提供 `ParallelShardExecutor` 并行分片执行器与 `ShardConfig` 分片配置，将批量操作按分片键拆分到多个分片并行执行，提升批量加载吞吐量，复用既有 `BatchExecutor`（`packages/sz-orm-batch/src/executor.rs:141`）+ v4.6.0 `BatchTransactionCoordinator`（`packages/sz-orm-batch/src/atomic.rs:216`）。
   a. 验收条件：[批量加载 100 万行 + 4 分片] → [拆分为 4 分片并行执行，吞吐量不低于单分片 4 倍（含拆分/合并开销）]
3. **批量冲突解决策略**（EARS: Ubiquitous）
   系统应当提供 `ConflictResolution` 冲突解决策略枚举（Upsert 插入或更新 / Ignore 忽略冲突 / Merge 合并 / Replace 替换），批量操作遇到冲突（主键/唯一键冲突）时按策略处理，策略可配（默认 Upsert）。
   a. 验收条件：[Upsert + 主键冲突] → [更新已有行]；[Ignore + 主键冲突] → [跳过冲突行，继续加载其他行]；[Replace + 主键冲突] → [删除已有行，插入新行]
4. **COPY 协议与并行分片组合**（EARS: Ubiquitous）
   系统应当支持 COPY 协议与并行分片组合（每个分片使用 COPY 协议并行加载），最大化批量加载性能。
   a. 验收条件：[COPY + 4 分片 + 100 万行] → [4 分片各自使用 COPY 协议并行加载 25 万行，吞吐量最大化]
5. **并行分片原子性**（EARS: Ubiquitous）
   系统应当支持并行分片执行的原子性（可配原子性级别，复用 v4.6.0 `AtomicityGuarantee` `packages/sz-orm-batch/src/atomic.rs:20` AllOrNothing/BestEffort/SagaCompensation），分片失败时按原子性级别处理。
   a. 验收条件：[AllOrNothing + 4 分片 + 分片 2 失败] → [全部回滚，不产生部分加载]；[BestEffort + 分片 2 失败] → [分片 1/3/4 成功，分片 2 标记失败]
6. **复用既有 COPY 与批量执行与原子性**（EARS: Ubiquitous）
   系统应当复用既有 `CopyProtocolExecutor`（`packages/sz-orm-batch/src/copy.rs:14`）/ `BatchExecutor`（`packages/sz-orm-batch/src/executor.rs:141`）/ `BatchExecutorConfig`（`:18`）+ v4.6.0 `BatchTransactionCoordinator`（`packages/sz-orm-batch/src/atomic.rs:216`）/ `SagaCompensator`（`:436`），COPY 协议方言适配与并行分片执行基于既有 COPY/批量执行/原子性扩展，不重复实现。
   a. 验收条件：[COPY 协议与并行分片] → [复用既有 CopyProtocolExecutor/BatchExecutor/BatchTransactionCoordinator，不新建 COPY/批量执行逻辑]
7. **加载日志可追溯**（EARS: Ubiquitous）
   系统应当记录加载日志（加载行数 + 分片列表 + 冲突解决 + 加载耗时 + 结果），供审计追溯。
   a. 验收条件：[COPY 并行分片加载 100 万行] → [记录加载日志，含行数 + 分片列表 + 冲突解决策略 + 耗时 + 结果]
8. **禁止项**（EARS: Unwanted）
   如果 COPY 协议与并行分片执行影响默认 feature 编译或破坏既有批量执行/COPY/原子性，则系统应当通过 `copy-parallel-shard` feature gate 隔离，默认不启用，且既有 `CopyProtocolExecutor`/`BatchExecutor`/`BatchTransactionCoordinator` 保留不动。
   a. 验收条件：[`cargo build` 默认编译] → [无 COPY 方言适配与并行分片，行为与 v4.6.0 一致]

### 5.3.2 交互流程

```plantuml
@startuml
title 批量 COPY 协议与并行分片执行 交互流程
actor "应用开发者" as appdev
participant "ParallelShardExecutor\n(并行分片执行器)" as parallel
participant "CopyProtocolAdapter\n(COPY 方言适配器)" as copy
participant "BatchExecutor\n(既有批量执行器)" as executor
participant "BatchTransactionCoordinator\n(v4.6.0 原子性)" as coord
database "DB" as db

appdev -> parallel : execute_copy_shards(data, shard_key, ConflictResolution)
parallel -> parallel : 按分片键拆分数据为 N 分片
parallel -> coord : begin_atomic(AtomicityGuarantee) [复用 v4.6.0]
par 并行执行分片
  parallel -> copy : execute_copy(shard_1) [方言适配]
  copy -> db : COPY/LOAD DATA shard_1 [按方言]
  parallel -> copy : execute_copy(shard_2)
  copy -> db : COPY/LOAD DATA shard_2
  parallel -> copy : execute_copy(shard_N)
  copy -> db : COPY/LOAD DATA shard_N
end
alt 全部分片成功
  parallel -> coord : commit() [复用 v4.6.0]
  parallel --> appdev : CopyBatchResult(全部成功)
else 某分片失败
  alt AllOrNothing
    parallel -> coord : rollback() [复用 v4.6.0]
    parallel --> appdev : Error("atomicity violated, all rolled back")
  else BestEffort
    parallel --> appdev : CopyBatchResult(部分成功+失败分片)
  end
end
alt 冲突解决
  copy -> db : 按 ConflictResolution 处理冲突(Upsert/Ignore/Merge/Replace)
end
parallel -> parallel : 记录加载日志
@enduml
```

### 5.3.3 异常场景

1. **COPY 协议方言不支持**
   a. 触发条件：方言不支持 COPY 协议（如 SQLite）
   b. 系统行为：降级为 multi-value INSERT，标注"COPY not supported by dialect X, fallback to multi-value INSERT"
   c. 用户感知：日志标注"fallback to multi-value INSERT"
2. **分片键不均匀导致负载倾斜**
   a. 触发条件：分片键分布不均匀，某分片数据量远超其他
   b. 系统行为：按分片键拆分执行，记录分片数据量供优化，可配再平衡策略
   c. 用户感知：某分片执行时间远超其他，可调分片键
3. **COPY 协议加载失败**
   a. 触发条件：COPY 协议加载失败（如数据格式错误、约束冲突）
   b. 系统行为：按冲突解决策略处理（Upsert/Ignore/Merge/Replace），策略无法解决时按原子性级别处理
   c. 用户感知：结果标注"copy load failed for shard X, conflict resolution applied"
4. **并行分片数超限**
   a. 触发条件：并行分片数超过连接池容量
   b. 系统行为：限制并行度到连接池容量，多余分片排队等待
   c. 用户感知：日志标注"parallelism limited by pool capacity"

## 5.4 异常自愈与根因分析（REQ-V47-004，P2）

### 5.4.1 业务规则

1. **异常自愈**（EARS: Ubiquitous）
   系统应当扩展既有 `sz-orm-observability` 包，提供 `AutoRemediator` 异常自愈器与 `RemediationAction` 修复动作枚举（RestartConnection 重启连接 / ClearCache 清缓存 / ScaleOut 扩容 / CustomAction 自定义），检测到异常后自动执行预设修复动作，须人工确认（可配自动执行白名单），记录审计日志，复用 v4.6.0 `AnomalyDetector`（`packages/sz-orm-observability/src/anomaly.rs:254`）/ `AnomalyAlert`（`:206`）。
   a. 验收条件：[检测到异常"连接池耗尽" + 自愈动作 RestartConnection + 自动执行白名单] → [自动执行 RestartConnection，记录审计日志]；[非白名单动作] → [等待人工确认，不自动执行]
2. **根因分析**（EARS: Ubiquitous）
   系统应当提供 `RootCauseAnalyzer` 根因分析器与 `RootCause` 根因结果（含根因组件 + 根因 SQL + 置信度 + 证据链），基于异常上下文（指标 + 日志 + 拓扑）分析根因，置信度可配阈值，低置信度时标注"根因不确定，需人工排查"。
   a. 验收条件：[异常"查询超时" + 根因分析] → [生成 RootCause，根因组件"DB"、根因 SQL"SELECT * FROM large_table"、置信度 0.85、证据链[指标+日志+拓扑]]
3. **异常关联分析**（EARS: Ubiquitous）
   系统应当提供 `AnomalyCorrelator` 异常关联器与 `CorrelationResult` 关联结果，分析多个异常事件的时间/空间关联性，识别同根因的异常集群，复用 v4.6.0 `AnomalyDetector`（`packages/sz-orm-observability/src/anomaly.rs:254`）。
   a. 验收条件：[异常 A"查询超时" + 异常 B"连接池耗尽" + 时间关联] → [识别为同根因异常集群，生成 CorrelationResult 标注关联性]
4. **自愈动作白名单可配置**（EARS: Ubiquitous）
   系统应当支持自愈动作白名单可配置（白名单内动作自动执行，非白名单动作须人工确认），不强制干扰开发者，默认白名单为空（全部须人工确认）。
   a. 验收条件：[白名单 [ClearCache] + 异常"缓存不一致" + 动作 ClearCache] → [自动执行 ClearCache]；[白名单 [] + 任意动作] → [全部须人工确认]
5. **根因分析置信度阈值可配置**（EARS: Ubiquitous）
   系统应当支持根因分析置信度阈值可配置（低于阈值时标注"根因不确定"），不强制干扰开发者，默认阈值 0.7。
   a. 验收条件：[置信度阈值 0.7 + 根因置信度 0.5] → [标注"根因不确定，需人工排查"]
6. **复用既有异常检测**（EARS: Ubiquitous）
   系统应当复用 v4.6.0 `AnomalyDetector`（`packages/sz-orm-observability/src/anomaly.rs:254`）/ `AnomalyAlert`（`:206`）/ `Anomaly`（`:154`）/ `AnomalyConfig`（`:74`）/ `AnomalyAlgorithm`（`:40`）+ 既有 `MetricsRegistry`（`packages/sz-orm-observability/src/lib.rs:259`）/ `QueryLogger`（`packages/sz-orm-observability/src/query_logger.rs:73`），异常自愈与根因分析基于既有异常检测扩展，不重复实现异常检测。
   a. 验收条件：[异常自愈与根因分析] → [复用既有 AnomalyDetector/MetricsRegistry/QueryLogger，不新建异常检测逻辑]
7. **自愈审计日志可追溯**（EARS: Ubiquitous）
   系统应当记录自愈审计日志（异常 ID + 修复动作 + 执行人 + 执行时间 + 结果），供合规审计，自愈动作须追加写入不可篡改。
   a. 验收条件：[异常自愈执行] → [记录审计日志，含异常 ID + 动作 + 执行人 + 时间 + 结果]
8. **禁止项**（EARS: Unwanted）
   如果异常自愈与根因分析影响默认 feature 编译或破坏既有异常检测，则系统应当通过 `anomaly-remediation-rca` feature gate 隔离，默认不启用，且既有 `AnomalyDetector`/`MetricsRegistry` 保留不动。
   a. 验收条件：[`cargo build` 默认编译] → [无异常自愈与根因分析，行为与 v4.6.0 一致]

### 5.4.2 交互流程

```plantuml
@startuml
title 异常自愈与根因分析 交互流程
actor "运维/SRE" as sre
participant "AnomalyDetector\n(v4.6.0 异常检测)" as detector
participant "RootCauseAnalyzer\n(根因分析器)" as rca
participant "AnomalyCorrelator\n(异常关联器)" as correlator
participant "AutoRemediator\n(异常自愈器)" as remediator
participant "MetricsRegistry\n(既有指标)" as metrics
participant "QueryLogger\n(既有日志)" as logger

detector -> detector : detect() [复用 v4.6.0]
alt 检测到异常
  detector -> rca : analyze_root_cause(anomaly)
  rca -> metrics : get_context(anomaly) [复用既有]
  rca -> logger : get_logs(anomaly) [复用既有]
  rca -> rca : 推断根因(组件+SQL+置信度+证据链)
  rca --> detector : RootCause
  detector -> correlator : correlate(anomaly, history)
  correlator --> detector : CorrelationResult(关联异常集群)
  detector -> remediator : select_action(anomaly, root_cause)
  alt 动作在白名单
    remediator -> remediator : 自动执行修复动作
    remediator -> remediator : 记录审计日志
  else 动作不在白名单
    remediator -> sre : 请求人工确认(异常+根因+建议动作)
    sre -> remediator : 确认执行
    remediator -> remediator : 执行修复动作 + 记录审计日志
  end
  remediator --> sre : 自愈结果
end
@enduml
```

### 5.4.3 异常场景

1. **自愈动作执行失败**
   a. 触发条件：自愈动作执行失败（如重启连接失败）
   b. 系统行为：记录自愈失败日志，通知 SRE 人工干预，不静默忽略
   c. 用户感知：告警"remediation action failed, manual intervention required"
2. **根因分析证据不足**
   a. 触发条件：根因分析证据不足（如指标/日志缺失）
   b. 系统行为：标注"根因不确定，证据不足"，建议人工排查
   c. 用户感知：结果标注"root cause uncertain, insufficient evidence"
3. **异常关联误关联**
   a. 触发条件：异常关联分析误将无关异常关联为同根因
   b. 系统行为：按关联性评分标注，低评分关联标注"weak correlation"
   c. 用户感知：关联结果标注"weak correlation, please verify"
4. **自愈动作白名单配置错误**
   a. 触发条件：白名单配置了不存在的动作
   b. 系统行为：跳过无效动作，记录日志"invalid remediation action in whitelist: X"
   c. 用户感知：日志标注"invalid whitelist action skipped"

## 5.5 多云成本对比与容量预测（REQ-V47-005，P2）

### 5.5.1 业务规则

1. **多云成本对比**（EARS: Ubiquitous）
   系统应当扩展既有 `sz-orm-storage` 包，提供 `MultiCloudCostComparator` 多云成本对比器与 `CostComparisonReport` 成本对比报表，跨 provider 成本对比（同容量在不同 provider 的成本差异），支持 provider 迁移建议，复用 v4.6.0 `CostAnalyzer`（`packages/sz-orm-storage/src/cost.rs:231`）/ `CostReport`（`:213`）/ `ProviderCost`（`:202`）。
   a. 验收条件：[对比 100GB 在 S3/Aliyun/Tencent 的成本] → [生成 CostComparisonReport，含每 provider 成本 + 差异 + 迁移建议]
2. **容量预测**（EARS: Ubiquitous）
   系统应当提供 `CapacityForecaster` 容量预测器与 `CapacityForecast` 容量预测结果（含预测容量 + 置信区间 + 预测算法），基于历史容量数据（时间序列）预测未来容量，预测算法可配（LinearRegression 线性回归 / ExponentialSmoothing 指数平滑 / HoltWinters Holt-Winters），默认 LinearRegression。
   a. 验收条件：[历史容量数据 30 天 + 预测 7 天 + LinearRegression] → [生成 CapacityForecast，含未来 7 天预测容量 + 95% 置信区间]
3. **成本自动优化执行**（EARS: Ubiquitous）
   系统应当提供 `AutoOptimizer` 自动优化执行器与 `OptimizationExecutionResult` 优化执行结果，自动执行优化建议（如自动降级冷数据到低成本 tier），而非仅生成建议，须人工确认（可配自动执行白名单），复用 v4.6.0 `CostOptimizationSuggestion`（`packages/sz-orm-storage/src/cost.rs:55`）。
   a. 验收条件：[优化建议 TierDowngrade + 白名单] → [自动执行降级，记录执行结果]；[非白名单建议] → [等待人工确认]
4. **provider 迁移建议**（EARS: Ubiquitous）
   系统应当基于成本对比生成 provider 迁移建议（如从 S3 迁移到 Aliyun 可节省 30%），建议附迁移成本 + 迁移风险 + 预期节省。
   a. 验收条件：[S3 成本高于 Aliyun 30%] → [生成迁移建议"从 S3 迁移到 Aliyun，预期节省 30%，附迁移成本与风险"]
5. **容量预测附置信区间**（EARS: Ubiquitous）
   系统应当对容量预测附置信区间（如 95% 置信区间），不单点预测，置信水平可配（默认 95%）。
   a. 验收条件：[预测 7 天容量 + 95% 置信区间] → [预测结果含上下界，不单点预测]
6. **复用既有成本分析**（EARS: Ubiquitous）
   系统应当复用 v4.6.0 `CostAnalyzer`（`packages/sz-orm-storage/src/cost.rs:231`）/ `CostReport`（`:213`）/ `ProviderCost`（`:202`）/ `BucketCost`（`:181`）/ `CostOptimizationSuggestion`（`:55`）/ `CostConfig`（`:96`）+ 既有 `Storage` trait（`packages/sz-orm-storage/src/storage.rs:14`）/ `StorageProvider`（`:287`），多云成本对比与容量预测基于既有成本分析扩展，不重复实现。
   a. 验收条件：[多云成本对比与容量预测] → [复用既有 CostAnalyzer/Storage/StorageProvider，不新建成本分析逻辑]
7. **对比与预测可配置**（EARS: Ubiquitous）
   系统应当支持对比周期、预测算法、预测周期、置信水平、自动优化白名单可配置，不强制干扰开发者。
   a. 验收条件：[配置对比每周 + 预测 ExponentialSmoothing + 置信 99%] → [每周对比，按 ExponentialSmoothing 预测，99% 置信区间]
8. **禁止项**（EARS: Unwanted）
   如果多云成本对比与容量预测影响默认 feature 编译或破坏既有成本分析，则系统应当通过 `multicloud-cost-forecast` feature gate 隔离，默认不启用，且既有 `CostAnalyzer`/`Storage` 保留不动。
   a. 验收条件：[`cargo build` 默认编译] → [无多云成本对比与容量预测，行为与 v4.6.0 一致]

### 5.5.2 交互流程

```plantuml
@startuml
title 多云成本对比与容量预测 交互流程
actor "FinOps/成本工程师" as finops
participant "MultiCloudCostComparator\n(多云成本对比器)" as comparator
participant "CapacityForecaster\n(容量预测器)" as forecaster
participant "AutoOptimizer\n(自动优化执行器)" as optimizer
participant "CostAnalyzer\n(v4.6.0 成本分析)" as analyzer
participant "Storage\n(既有存储)" as storage

finops -> comparator : compare_providers(capacity, providers)
loop 每个 provider
  comparator -> analyzer : analyze_cost(provider) [复用 v4.6.0]
  analyzer -> storage : get_usage(provider) [复用既有]
  analyzer --> comparator : ProviderCost
end
comparator -> comparator : 计算成本差异 + 生成迁移建议
comparator --> finops : CostComparisonReport

finops -> forecaster : forecast(history_data, algorithm, horizon)
forecaster -> forecaster : 按算法预测(LinearRegression/ExponentialSmoothing)
forecaster -> forecaster : 计算置信区间
forecaster --> finops : CapacityForecast(预测容量+置信区间)

finops -> optimizer : execute_suggestion(suggestion)
alt 建议在白名单
  optimizer -> storage : 执行优化(如降级 tier) [复用既有]
  optimizer --> finops : OptimizationExecutionResult(成功)
else 非白名单
  optimizer -> finops : 请求人工确认
  finops -> optimizer : 确认执行
  optimizer -> storage : 执行优化
  optimizer --> finops : OptimizationExecutionResult
end
@enduml
```

### 5.5.3 异常场景

1. **provider API 不可用**
   a. 触发条件：某 provider API 不可用（如云服务故障）
   b. 系统行为：跳过该 provider 对比，记录日志"provider X API unavailable, skipped"
   c. 用户感知：报表标注"provider X comparison skipped, API unavailable"
2. **历史数据不足无法预测**
   a. 触发条件：历史容量数据不足（如少于 7 天）
   b. 系统行为：拒绝预测，返回错误"insufficient history data for forecasting"
   c. 用户感知：错误"insufficient history data, at least 7 days required"
3. **自动优化执行失败**
   a. 触发条件：自动优化执行失败（如降级 tier 失败）
   b. 系统行为：记录执行失败日志，通知 FinOps 人工干预
   c. 用户感知：告警"optimization execution failed, manual intervention required"
4. **预测算法不适用**
   a. 触发条件：预测算法不适用（如 LinearRegression 对周期性数据预测偏差大）
   b. 系统行为：按配置算法预测，标注预测偏差，建议切换算法
   c. 用户感知：预测结果标注"high forecast error, consider switching algorithm"

## 5.6 租户资源配额与行级安全增强（REQ-V47-006，P1）

### 5.6.1 业务规则

1. **租户资源配额**（EARS: Ubiquitous）
   系统应当扩展既有 `sz-orm-core` 多租户，提供 `TenantResourceQuota` 租户资源配额（max_connections 最大连接数 / max_qps 最大查询速率 / max_storage 最大存储容量）与 `QuotaEnforcer` 配额执行器，按租户设置资源配额，配额超限时拒绝请求，复用 v4.6.0 `ConnectionTenantBinder`（`packages/sz-orm-core/src/connection_tenant.rs:133`）/ `TenantConnectionGuard`（`:249`）。
   a. 验收条件：[租户 1 配额 max_connections=10 + 当前 10 连接 + 新请求] → [拒绝新连接，返回错误"quota exceeded: max_connections"]；[max_qps=100 + 当前 100 QPS + 新查询] → [拒绝查询，返回错误"quota exceeded: max_qps"]
2. **行级安全策略增强**（EARS: Ubiquitous）
   系统应当提供 `RlsPolicyEnhancer` RLS 策略增强器与 `EnhancedRlsPolicy` �强行级安全策略，增强策略表达能力（多条件组合 / 复杂谓词 / 列级脱敏联动），自动注入 WHERE 条件（参数化绑定），复用既有 `RowLevelSecurityPolicy`（`packages/sz-orm-core/src/tenant_security.rs:67`）/ `ColumnMaskingRule`（`:155`）。
   a. 验收条件：[增强 RLS 策略"tenant_id=1 AND dept_id IN (1,2)" + 查询] → [自动注入 `WHERE tenant_id = ? AND dept_id IN (?,?)`，参数化绑定]
3. **租户级审计日志**（EARS: Ubiquitous）
   系统应当提供 `TenantAuditLogger` 租户级审计日志器与 `TenantAuditEntry` 审计条目，按租户独立审计日志（连接获取 / 查询执行 / 配额超限 / RLS 命中），追加写入不可篡改，复用既有 `TenantAuditContext`（`packages/sz-orm-core/src/tenant_security.rs:244`）/ `TenantAuditOperation`（`:197`）/ `AuditResult`（`:224`）。
   a. 验收条件：[租户 1 查询 users 表] → [记录审计日志，含租户 ID + 操作 + 表 + 时间 + 结果，追加写入]
4. **配额检查在连接池/查询层强制执行**（EARS: Ubiquitous）
   系统应当确保配额检查在连接池/查询执行层强制执行，不可被应用层绕过，复用既有 `Pool`（`packages/sz-orm-core/src/pool.rs:743`）/ `Connection` trait（`:45`）。
   a. 验收条件：[应用层尝试绕过配额] → [连接池/查询层强制检查，不可绕过]
5. **RLS 自动注入参数化绑定**（EARS: Ubiquitous）
   系统应当确保 RLS 自动注入的 WHERE 条件参数化绑定，tenant_id 不可被客户端篡改（由可信路径设置，复用既有 `TenantContext` `packages/sz-orm-core/src/tenant_context.rs:80`），防止 SQL 注入与越权。
   a. 验收条件：[客户端尝试篡改 tenant_id] → [拒绝篡改，使用可信路径设置的 tenant_id，WHERE 条件参数化绑定]
6. **配额与 RLS 可配置**（EARS: Ubiquitous）
   系统应当支持租户配额值、RLS 策略、审计日志级别可配置，不强制干扰开发者，默认配额无限制（向后兼容）。
   a. 验收条件：[配置租户 1 max_connections=10 + RLS 策略] → [按配置执行配额与 RLS]；[未配置配额] → [无配额限制，向后兼容]
7. **复用既有连接级多租户与 RLS**（EARS: Ubiquitous）
   系统应当复用 v4.6.0 `ConnectionTenantBinder`（`packages/sz-orm-core/src/connection_tenant.rs:133`）/ `ConnectionLevelIsolation`（`:24`）/ `ConnectionAffinityPolicy`（`:35`）+ 既有 `RowLevelSecurityPolicy`（`packages/sz-orm-core/src/tenant_security.rs:67`）/ `ColumnMaskingRule`（`:155`）/ `TenantAuditContext`（`:244`）/ `TenantContext`（`packages/sz-orm-core/src/tenant_context.rs:80`）/ `Pool`（`packages/sz-orm-core/src/pool.rs:743`），租户资源配额与 RLS 增强基于既有连接级多租户与 RLS 扩展，不重复实现。
   a. 验收条件：[租户资源配额与 RLS 增强] → [复用既有 ConnectionTenantBinder/RowLevelSecurityPolicy/Pool，不新建连接池/多租户逻辑]
8. **配额与审计日志可追溯**（EARS: Ubiquitous）
   系统应当记录配额超限日志与审计日志（租户 ID + 配额类型 + 当前值 + 限制值 + 操作 + 时间），供审计追溯。
   a. 验收条件：[租户 1 配额超限] → [记录配额超限日志 + 审计日志，含租户 ID + 配额类型 + 当前值 + 限制值]
9. **禁止项**（EARS: Unwanted）
   如果租户资源配额与 RLS 增强影响默认 feature 编译或破坏既有连接池/多租户/RLS，则系统应当通过 `tenant-quota-rls-enhanced` feature gate 隔离，默认不启用，且既有 `ConnectionTenantBinder`/`RowLevelSecurityPolicy`/`Pool` 保留不动。
   a. 验收条件：[`cargo build` 默认编译] → [无配额与 RLS 增强，行为与 v4.6.0 一致]

### 5.6.2 交互流程

```plantuml
@startuml
title 租户资源配额与行级安全增强 交互流程
actor "应用开发者" as appdev
participant "QuotaEnforcer\n(配额执行器)" as quota
participant "RlsPolicyEnhancer\n(RLS 增强器)" as rls
participant "TenantAuditLogger\n(租户审计日志器)" as audit
participant "ConnectionTenantBinder\n(v4.6.0 连接绑定)" as binder
participant "Pool\n(既有连接池)" as pool
database "DB" as db

appdev -> binder : acquire_with_tenant(tenant_id=1) [复用 v4.6.0]
binder -> quota : check_quota(tenant_id=1, resource=connection)
alt 配额未超限
  quota -> binder : OK
  binder -> pool : acquire() [复用既有]
  pool --> binder : PooledConnection
  binder --> appdev : TenantConnectionGuard
  appdev -> rls : enhance_query(query, tenant_id=1)
  rls -> rls : 匹配增强 RLS 策略
  rls -> rls : 注入 WHERE tenant_id=? AND dept_id IN (?,?) [参数化]
  rls -> db : 执行增强查询
  rls -> audit : log(tenant_id=1, operation=query, table=users, result=ok)
  audit -> audit : 追加写入审计日志
else 配额超限
  quota -> audit : log(tenant_id=1, quota_exceeded, current=10, limit=10)
  quota --> appdev : Error("quota exceeded: max_connections")
end
@enduml
```

### 5.6.3 异常场景

1. **配额检查失败**
   a. 触发条件：配额检查失败（如配额存储不可用）
   b. 系统行为：按 fail-open 或 fail-close 策略处理（可配，默认 fail-close 拒绝请求），记录日志
   c. 用户感知：fail-close 时错误"quota check failed, request rejected for safety"
2. **RLS 策略匹配冲突**
   a. 触发条件：多个 RLS 策略匹配同一查询，策略冲突
   b. 系统行为：按策略优先级选择最高优先级策略，记录冲突日志
   c. 用户感知：日志标注"RLS policy conflict, selected highest priority"
3. **审计日志写入失败**
   a. 触发条件：审计日志写入失败（如磁盘满）
   b. 系统行为：记录到备用日志（如 stderr），告警 SRE，不阻断主流程
   c. 用户感知：告警"audit log write failed, logged to fallback"
4. **配额值配置错误**
   a. 触发条件：配额值配置错误（如负数、超大值）
   b. 系统行为：拒绝配置，返回错误"invalid quota value"
   c. 用户感知：错误"invalid quota value: ..."

## 5.7 缓存预热与穿透防护（REQ-V47-007，P1）

### 5.7.1 业务规则

1. **缓存预热**（EARS: Ubiquitous）
   系统应当扩展既有 `sz-orm-core` 缓存，提供 `CacheWarmer` 缓存预热器与 `WarmupConfig` 预热配置，启动时预加载热点数据到 L1+L2，减少冷启动缓存未命中，预热策略可配（HotspotTable 热点表 / HotspotKey 热点主键 / CustomQuery 自定义查询），异步执行不阻塞服务启动，复用 v4.6.0 `ProcessL1Cache`（`packages/sz-orm-core/src/process_l1_cache.rs:169`）/ `CrossSessionIdentityMap`（`:366`）+ 既有 `L2Cache`（`packages/sz-orm-core/src/l2_cache.rs:517`）。
   a. 验收条件：[预热策略 HotspotTable "users" + 启动] → [异步预加载 users 表热点数据到 L1+L2，不阻塞启动，启动后查询 users 命中缓存]
2. **缓存穿透防护**（EARS: Ubiquitous）
   系统应当提供 `BloomFilter` 布隆过滤器与 `PenetrationGuard` 穿透防护器，在查询前判断 key 是否可能存在，不存在则直接返回 None 不查 DB，避免大量不存在的 key 击穿到 DB，误判率可配（默认 1%），误判时回退到 DB 查询，不漏判（不存在的 key 一定返回 None）。
   a. 验收条件：[查询不存在的 key "user:999" + 布隆过滤器判断不存在] → [直接返回 None 不查 DB]；[布隆过滤器误判存在 + DB 不存在] → [回退到 DB 查询返回 None，更新布隆过滤器]
3. **缓存击穿防护**（EARS: Ubiquitous）
   系统应当提供 `SingleFlight` 击穿防护器与 `StampedeGuard` 击穿防护配置，对同一 key 的并发重建请求只执行一次，其他请求等待结果复用，避免热点 key 过期后大量请求同时重建缓存，复用 v4.6.0 `ProcessL1Cache`（`packages/sz-orm-core/src/process_l1_cache.rs:169`）。
   a. 验收条件：[热点 key "user:1" 过期 + 100 并发查询] → [只执行 1 次 DB 查询重建缓存，其他 99 等待复用结果]
4. **预热与 L1/L2 协同**（EARS: Ubiquitous）
   系统应当与既有 L1/L2 缓存协同，预热数据同时加载到 L1+L2，复用 v4.6.0 `ProcessL1Cache`（`packages/sz-orm-core/src/process_l1_cache.rs:169`）/ 既有 `L2Cache`（`packages/sz-orm-core/src/l2_cache.rs:517`）/ `CacheCoherenceProtocol`（`packages/sz-orm-core/src/cache_coherence.rs:103`）。
   a. 验收条件：[预热 key "user:1"] → [同时加载到 L1+L2，查询命中 L1]
5. **布隆过滤器不漏判**（EARS: Ubiquitous）
   系统应当确保布隆过滤器不漏判（不存在的 key 一定返回 None，存在的 key 可能误判存在），误判时回退到 DB 查询，不返回错误。
   a. 验收条件：[不存在的 key] → [布隆过滤器一定返回"不存在"，直接返回 None]；[存在的 key + 误判] → [回退 DB 查询返回数据]
6. **singleflight 不死锁**（EARS: Ubiquitous）
   系统应当确保 singleflight 机制不死锁（重建超时后释放锁，其他请求可重试），不长期阻塞，超时可配（默认 5 秒）。
   a. 验收条件：[重建超时 5 秒 + 等待请求] → [5 秒后释放锁，等待请求重试]
7. **预热与防护可配置**（EARS: Ubiquitous）
   系统应当支持预热策略、预热数据量、布隆过滤器容量与误判率、singleflight 超时可配置，不强制干扰开发者，默认值保守（布隆误判 1%，singleflight 超时 5 秒）。
   a. 验收条件：[配置布隆误判 0.1% + singleflight 超时 10 秒] → [按配置执行]
8. **复用既有进程级 L1 与 L2**（EARS: Ubiquitous）
   系统应当复用 v4.6.0 `ProcessL1Cache`（`packages/sz-orm-core/src/process_l1_cache.rs:169`）/ `CrossSessionIdentityMap`（`:366`）/ `ProcessL1Config`（`:44`）+ 既有 `L2Cache`（`packages/sz-orm-core/src/l2_cache.rs:517`）/ `CacheKey`（`:143`）/ `CacheCoherenceProtocol`（`packages/sz-orm-core/src/cache_coherence.rs:103`），缓存预热与穿透防护基于既有进程级 L1/L2 扩展，不重复实现缓存逻辑。
   a. 验收条件：[缓存预热与穿透防护] → [复用既有 ProcessL1Cache/L2Cache/CacheCoherenceProtocol，不新建缓存逻辑]
9. **预热与防护日志可追溯**（EARS: Ubiquitous）
   系统应当记录预热日志（预热策略 + 预热数据量 + 耗时 + 命中率提升）与防护日志（布隆过滤次数 + 误判次数 + singleflight 复用次数），供审计追溯。
   a. 验收条件：[缓存预热 + 穿透防护运行] → [记录预热日志 + 防护日志，含策略 + 数据量 + 命中率 + 过滤次数]
10. **禁止项**（EARS: Unwanted）
    如果缓存预热与穿透防护影响默认 feature 编译或破坏既有 L1/L2/进程级 L1，则系统应当通过 `cache-warmup-protection` feature gate 隔离，默认不启用，且既有 `ProcessL1Cache`/`L2Cache` 保留不动。
    a. 验收条件：[`cargo build` 默认编译] → [无预热与防护，行为与 v4.6.0 一致]

### 5.7.2 交互流程

```plantuml
@startuml
title 缓存预热与穿透防护 交互流程
actor "应用开发者" as appdev
participant "CacheWarmer\n(缓存预热器)" as warmer
participant "PenetrationGuard\n(穿透防护器)" as guard
participant "BloomFilter\n(布隆过滤器)" as bloom
participant "SingleFlight\n(击穿防护器)" as sf
participant "ProcessL1Cache\n(v4.6.0 进程级 L1)" as l1
participant "L2Cache\n(既有 L2)" as l2
database "DB" as db

== 启动时预热 ==
warmer -> warmer : 按 WarmupConfig 识别热点数据
warmer -> db : 查询热点数据 [异步]
db --> warmer : 热点数据
warmer -> l1 : put_batch(热点数据) [复用 v4.6.0]
warmer -> l2 : put_batch(热点数据) [复用既有]
warmer -> bloom : add_keys(热点 key)
warmer -> warmer : 记录预热日志

== 运行时查询 ==
appdev -> guard : get(key="user:1")
guard -> bloom : might_contain(key)
alt 布隆判断不存在
  bloom --> guard : false
  guard --> appdev : None [不查 DB]
else 布隆判断可能存在
  bloom --> guard : true
  guard -> l1 : get(key) [复用 v4.6.0]
  alt L1 命中
    l1 --> guard : Value
    guard --> appdev : Value
  else L1 未命中
    guard -> sf : get_or_rebuild(key)
    alt 首个请求
      sf -> db : 查询
      db --> sf : Value
      sf -> l1 : put(key, Value)
      sf -> l2 : put(key, Value)
      sf --> guard : Value
    else 并发请求
      sf --> guard : 等待首个请求结果
      sf --> guard : Value [复用]
    end
  end
end
@enduml
```

### 5.7.3 异常场景

1. **预热失败**
   a. 触发条件：缓存预热失败（如 DB 查询失败、热点数据识别失败）
   b. 系统行为：记录预热失败日志，不影响服务启动，启动后按需加载
   c. 用户感知：日志标注"warmup failed, cache will load on demand"
2. **布隆过滤器容量超限**
   a. 触发条件：布隆过滤器容量超限（key 数量超过配置容量）
   b. 系统行为：按配置策略处理（Rebuild 重建 / Evict 淘汰 / Degrade 降级为全查 DB），记录日志
   c. 用户感知：日志标注"bloom filter capacity exceeded, strategy applied"
3. **singleflight 重建超时**
   a. 触发条件：singleflight 重建超时（DB 查询超时）
   b. 系统行为：释放锁，等待请求重试，记录超时日志
   c. 用户感知：请求重试，日志标注"singleflight rebuild timeout, retried"
4. **预热数据与 DB 不一致**
   a. 触发条件：预热数据与 DB 不一致（预热期间 DB 数据变更）
   b. 系统行为：通过 `CacheCoherenceProtocol` 同步失效，记录日志
   c. 用户感知：日志标注"warmup data stale, coherence synchronized"

---

# 6. 数据约束

## 6.1 DelayScheduler（延迟调度配置）

1. **enabled**：是否启用延迟队列，bool，必填，默认 false。
2. **priority_policy**：优先级策略，`PriorityPolicy` 枚举（Strict/Weighted/FairShare），必填，默认 Strict。
3. **aging_enabled**：是否启用 aging 机制（避免低优先级饿死），bool，必填，默认 true。
4. **aging_threshold_ms**：aging 时间阈值（毫秒），u64，必填，默认 300000（5 分钟）。
5. **queue_capacity**：优先级队列容量上限，usize，必填，默认 100000。

## 6.2 PriorityPolicy（优先级策略）

1. **Strict**：严格优先级（高优先级先投递，低优先级可能饿死，aging 机制缓解）。
2. **Weighted**：加权优先级（按权重比例投递，避免饿死）。
3. **FairShare**：公平份额（按租户/类别公平分配投递份额）。

## 6.3 ForwardCompatConfig（前向兼容性检查配置）

1. **strictness**：检查严格度，`CompatStrictness` 枚举（Strict/Lenient），必填，默认 Strict。
2. **breaking_changes**：视为破坏性的变更类型，`Vec<BreakingChangeType>`（DropColumn/AlterColumnType/AlterColumnConstraint/RenameTable），必填，默认全部类型。
3. **sandbox_table_prefix**：沙箱表前缀，String，必填，默认 "shadow_"。
4. **sandbox_verify_items**：沙箱预演验证项，`Vec<SandboxVerifyItem>`（DataIntegrity/QueryCompat/PerformanceImpact），必填，默认全部验证项。

## 6.5 CopyShardConfig（COPY 并行分片配置）

1. **conflict_resolution**：冲突解决策略，`ConflictResolution` 枚举（Upsert/Ignore/Merge/Replace），必填，默认 Upsert。
2. **shard_key**：分片键，String，必填。
3. **shard_count**：分片数，usize，必填，默认 4。
4. **parallelism**：并行度，usize，必填，默认等于分片数（不超过连接池容量）。
5. **atomicity_guarantee**：原子性保证级别，`AtomicityGuarantee`（复用 v4.6.0 `packages/sz-orm-batch/src/atomic.rs:20`），必填，默认 BestEffort。

## 6.6 ConflictResolution（冲突解决策略）

1. **Upsert**：插入或更新（主键/唯一键冲突时更新已有行，`ON CONFLICT DO UPDATE`）。
2. **Ignore**：忽略冲突（主键/唯一键冲突时跳过，`ON CONFLICT DO NOTHING`）。
3. **Merge**：合并（按业务逻辑合并新旧数据，需自定义合并函数）。
4. **Replace**：替换（主键/唯一键冲突时删除已有行再插入，`REPLACE INTO`）。

## 6.7 RemediationConfig（异常自愈配置）

1. **auto_execute_whitelist**：自动执行白名单，`Vec<RemediationAction>`，必填，默认空（全部须人工确认）。
2. **rca_confidence_threshold**：根因分析置信度阈值，f64，必填，默认 0.7。
3. **correlation_window_ms**：异常关联时间窗口（毫秒），u64，必填，默认 300000（5 分钟）。
4. **remediation_timeout_ms**：自愈动作执行超时（毫秒），u64，必填，默认 30000（30 秒）。

## 6.8 RemediationAction（修复动作）

1. **RestartConnection**：重启连接（关闭并重新建立连接）。
2. **ClearCache**：清缓存（失效 L1+L2 缓存）。
3. **ScaleOut**：扩容（增加连接池容量或实例数，需外部编排支持）。
4. **CustomAction**：自定义动作（由应用层注册的自愈函数）。

## 6.9 MultiCloudForecastConfig（多云成本对比与容量预测配置）

1. **comparison_interval_ms**：对比周期（毫秒），u64，必填，默认 604800000（每周）。
2. **forecast_algorithm**：预测算法，`ForecastAlgorithm` 枚举（LinearRegression/ExponentialSmoothing/HoltWinters），必填，默认 LinearRegression。
3. **forecast_horizon_days**：预测周期（天），u32，必填，默认 7。
4. **confidence_level**：置信水平，f64，必填，默认 0.95（95%）。
5. **auto_optimize_whitelist**：自动优化白名单，`Vec<CostOptimizationSuggestion>`（复用 v4.6.0 `packages/sz-orm-storage/src/cost.rs:55`），必填，默认空。

## 6.10 ForecastAlgorithm（预测算法）

1. **LinearRegression**：线性回归（按最小二乘法拟合线性趋势）。
2. **ExponentialSmoothing**：指数平滑（按指数衰减权重平滑历史数据）。
3. **HoltWinters**：Holt-Winters（三指数平滑，支持趋势 + 季节性）。

## 6.11 TenantQuotaRlsConfig（租户配额与 RLS 增强配置）

1. **quota_enforce_strategy**：配额执行策略，`QuotaEnforceStrategy` 枚举（FailClose/FailOpen），必填，默认 FailClose。
2. **rls_enhancement_enabled**：是否启用 RLS 增强，bool，必填，默认 false。
3. **audit_log_level**：审计日志级别，`AuditLogLevel` 枚举（Full/Summary/Off），必填，默认 Summary。
4. **quota_check_enabled**：是否启用配额检查，bool，必填，默认 false（向后兼容，默认无配额限制）。

## 6.13 WarmupProtectionConfig（缓存预热与穿透防护配置）

1. **warmup_strategy**：预热策略，`WarmupStrategy` 枚举（HotspotTable/HotspotKey/CustomQuery/Disabled），必填，默认 Disabled。
2. **warmup_batch_size**：预热批次大小，usize，必填，默认 1000。
3. **bloom_filter_capacity**：布隆过滤器容量，usize，必填，默认 1000000。
4. **bloom_filter_fpp**：布隆过滤器误判率，f64，必填，默认 0.01（1%）。
5. **singleflight_timeout_ms**：singleflight 超时（毫秒），u64，必填，默认 5000（5 秒）。
6. **penetration_guard_enabled**：是否启用穿透防护，bool，必填，默认 false。
7. **stampede_guard_enabled**：是否启用击穿防护，bool，必填，默认 false。

## 6.14 WarmupStrategy（预热策略）

1. **HotspotTable**：热点表预热（按配置的热点表预加载全部/部分数据）。
2. **HotspotKey**：热点主键预热（按配置的热点主键列表预加载）。
3. **CustomQuery**：自定义查询预热（按自定义 SQL 查询预加载）。
4. **Disabled**：不预热（默认，向后兼容）。

---

# 7. 需求追溯矩阵

| 需求 ID | 优先级 | 需求名称 | 验收条件数 | feature gate | 复用既有代码 |
|---------|--------|---------|-----------|-------------|-------------|
| REQ-V47-001 | P1 | 消息延迟队列与优先级调度 | 8 | `delayed-priority-queue` | `MessageQueue` `packages/sz-orm-queue/src/queue.rs:18` / `Message` `:57` / `InMemoryQueue` `:339` + v4.6.0 `RedeliveryScheduler` `packages/sz-orm-queue/src/dlx.rs:216` / `BackoffPolicy` `:47` / `DlxRoutingStrategy` `:83` |
| REQ-V47-002 | P1 | 迁移前向兼容性检查与沙箱预演 | 8 | `forward-compat-sandbox` | `Migration` `packages/sz-orm-core/src/migration.rs:10` / `MigrationResolver` `:62` + 既有 `DryRunMigration` `packages/sz-orm-core/src/migration_dry_run.rs:11` / `DryRunReport` `:24` / `MigrationImpact` `:59` / `ImpactReport` `:80` + v4.6.0 `RollbackExecutor` `packages/sz-orm-core/src/rollback_zero_downtime.rs:305` / `RollbackPlan` `:157` |
| REQ-V47-003 | P1 | 批量 COPY 协议与并行分片执行 | 8 | `copy-parallel-shard` | 既有 `CopyProtocolExecutor` `packages/sz-orm-batch/src/copy.rs:14` / `BatchExecutor` `packages/sz-orm-batch/src/executor.rs:141` / `BatchExecutorConfig` `:18` + v4.6.0 `BatchTransactionCoordinator` `packages/sz-orm-batch/src/atomic.rs:216` / `AtomicityGuarantee` `:20` / `SagaCompensator` `:436` |
| REQ-V47-004 | P2 | 异常自愈与根因分析 | 8 | `anomaly-remediation-rca` | v4.6.0 `AnomalyDetector` `packages/sz-orm-observability/src/anomaly.rs:254` / `AnomalyAlert` `:206` / `Anomaly` `:154` / `AnomalyConfig` `:74` / `AnomalyAlgorithm` `:40` + 既有 `MetricsRegistry` `packages/sz-orm-observability/src/lib.rs:259` / `QueryLogger` `packages/sz-orm-observability/src/query_logger.rs:73` |
| REQ-V47-005 | P2 | 多云成本对比与容量预测 | 8 | `multicloud-cost-forecast` | v4.6.0 `CostAnalyzer` `packages/sz-orm-storage/src/cost.rs:231` / `CostReport` `:213` / `ProviderCost` `:202` / `BucketCost` `:181` / `CostOptimizationSuggestion` `:55` / `CostConfig` `:96` + 既有 `Storage` `packages/sz-orm-storage/src/storage.rs:14` / `StorageProvider` `:287` |
| REQ-V47-006 | P1 | 租户资源配额与行级安全增强 | 9 | `tenant-quota-rls-enhanced` | v4.6.0 `ConnectionTenantBinder` `packages/sz-orm-core/src/connection_tenant.rs:133` / `ConnectionLevelIsolation` `:24` / `ConnectionAffinityPolicy` `:35` / `TenantConnectionGuard` `:249` + 既有 `RowLevelSecurityPolicy` `packages/sz-orm-core/src/tenant_security.rs:67` / `ColumnMaskingRule` `:155` / `TenantAuditContext` `:244` / `TenantAuditOperation` `:197` / `AuditResult` `:224` / `TenantContext` `packages/sz-orm-core/src/tenant_context.rs:80` / `Pool` `packages/sz-orm-core/src/pool.rs:743` |
| REQ-V47-007 | P1 | 缓存预热与穿透防护 | 10 | `cache-warmup-protection` | v4.6.0 `ProcessL1Cache` `packages/sz-orm-core/src/process_l1_cache.rs:169` / `CrossSessionIdentityMap` `:366` / `ProcessL1Config` `:44` + 既有 `L2Cache` `packages/sz-orm-core/src/l2_cache.rs:517` / `CacheKey` `:143` / `CacheCoherenceProtocol` `packages/sz-orm-core/src/cache_coherence.rs:103` / `MesiState` `:12` |

---

# 8. 验收标准总览

## 8.1 REQ-V47-001 消息延迟队列与优先级调度（P1）

1. `DelayedMessage` 延迟投递（按 deliver_at 投递，到期前不可消费）
2. `PriorityQueue` + `PriorityPolicy` 优先级队列（Strict/Weighted/FairShare，默认 Strict）
3. `ScheduledMessage` 定时调度（Cron 表达式或间隔，复用 v4.6.0 `RedeliveryScheduler`）
4. 延迟与优先级组合（延迟到期后按优先级投递）
5. 优先级队列不饿死低优先级（aging 机制，默认 5 分钟）
6. 复用既有 `MessageQueue`/`Message`/`RedeliveryScheduler`，不重复实现
7. 调度日志可追溯（消息 ID + 延迟时间 + 优先级 + 投递时间 + 结果）
8. `delayed-priority-queue` feature gate 隔离，默认关闭，既有 `MessageQueue` 与 v4.6.0 `RedeliveryScheduler` 保留

## 8.2 REQ-V47-002 迁移前向兼容性检查与沙箱预演（P1）

1. `ForwardCompatChecker` 前向兼容性检查（识别删除列/改列类型/改列约束等破坏性变更）
2. `SandboxDryRunner` 沙箱预演（影子表预执行 + 数据完整性/查询兼容性/性能影响校验）
3. `MigrationDependencyGraph` 迁移依赖图分析（依赖关系 + 执行顺序 + 循环检测）
4. 前向兼容性规则可配置（破坏性变更类型 + 检查严格度）
5. 沙箱预演验证项可配置（数据完整性/查询兼容性/性能影响）
6. 复用既有 `DryRunMigration`/`ImpactReport`/`RollbackExecutor`，不重复实现
7. 检查与预演日志可追溯（迁移版本 + 检查结果 + 预演结果 + 耗时）
8. `forward-compat-sandbox` feature gate 隔离，默认关闭，既有 `Migration`/`DryRunMigration`/`RollbackExecutor` 保留

## 8.3 REQ-V47-003 批量 COPY 协议与并行分片执行（P1）

1. `CopyProtocolAdapter` COPY 协议方言适配（PostgreSQL COPY / MySQL LOAD DATA / Oracle SQL*Loader / MSSQL BULK INSERT，其他降级 multi-value INSERT）
2. `ParallelShardExecutor` 并行分片执行（按分片键拆分并行，吞吐量 N 倍提升）
3. `ConflictResolution` 冲突解决策略（Upsert/Ignore/Merge/Replace，默认 Upsert）
4. COPY 协议与并行分片组合（每分片 COPY 并行加载）
5. 并行分片原子性（复用 v4.6.0 `AtomicityGuarantee`，AllOrNothing/BestEffort/SagaCompensation）
6. 复用既有 `CopyProtocolExecutor`/`BatchExecutor`/`BatchTransactionCoordinator`，不重复实现
7. 加载日志可追溯（加载行数 + 分片列表 + 冲突解决 + 耗时 + 结果）
8. `copy-parallel-shard` feature gate 隔离，默认关闭，既有 `CopyProtocolExecutor`/`BatchExecutor` 保留

## 8.4 REQ-V47-004 异常自愈与根因分析（P2）

1. `AutoRemediator` 异常自愈（RemediationAction: RestartConnection/ClearCache/ScaleOut/CustomAction，须人工确认 + 白名单）
2. `RootCauseAnalyzer` 根因分析（根因组件 + SQL + 置信度 + 证据链）
3. `AnomalyCorrelator` 异常关联分析（跨指标关联，识别同根因异常集群）
4. 自愈动作白名单可配置（白名单内自动执行，非白名单须人工确认，默认空）
5. 根因分析置信度阈值可配置（默认 0.7，低于阈值标注"根因不确定"）
6. 复用 v4.6.0 `AnomalyDetector`/`MetricsRegistry`/`QueryLogger`，不重复实现
7. 自愈审计日志可追溯（异常 ID + 动作 + 执行人 + 时间 + 结果，追加写入不可篡改）
8. `anomaly-remediation-rca` feature gate 隔离，默认关闭，既有 `AnomalyDetector` 保留

## 8.5 REQ-V47-005 多云成本对比与容量预测（P2）

1. `MultiCloudCostComparator` 多云成本对比（跨 provider 成本差异 + 迁移建议）
2. `CapacityForecaster` 容量预测（LinearRegression/ExponentialSmoothing/HoltWinters，附置信区间）
3. `AutoOptimizer` 成本自动优化执行（自动执行优化建议，须人工确认 + 白名单）
4. provider 迁移建议（附迁移成本 + 迁移风险 + 预期节省）
5. 容量预测附置信区间（默认 95%，不单点预测）
6. 复用 v4.6.0 `CostAnalyzer`/`Storage`/`StorageProvider`，不重复实现
7. 对比与预测可配置（周期 + 算法 + 置信水平 + 白名单）
8. `multicloud-cost-forecast` feature gate 隔离，默认关闭，既有 `CostAnalyzer`/`Storage` 保留

## 8.6 REQ-V47-006 租户资源配额与行级安全增强（P1）

1. `TenantResourceQuota` 租户资源配额（max_connections/max_qps/max_storage，`QuotaEnforcer` 执行）
2. `RlsPolicyEnhancer` RLS 策略增强（多条件组合 + 复杂谓词 + 列级脱敏联动，自动注入 WHERE 参数化）
3. `TenantAuditLogger` 租户级审计日志（连接/查询/配额超限/RLS 命中，追加写入不可篡改）
4. 配额检查在连接池/查询层强制执行（不可被应用层绕过）
5. RLS 自动注入参数化绑定（tenant_id 不可篡改，由可信路径设置）
6. 配额与 RLS 可配置（默认无配额限制，向后兼容）
7. 复用 v4.6.0 `ConnectionTenantBinder` + 既有 `RowLevelSecurityPolicy`/`Pool`，不重复实现
8. 配额与审计日志可追溯（租户 ID + 配额类型 + 当前值 + 限制值 + 操作）
9. `tenant-quota-rls-enhanced` feature gate 隔离，默认关闭，既有 `ConnectionTenantBinder`/`RowLevelSecurityPolicy`/`Pool` 保留

## 8.7 REQ-V47-007 缓存预热与穿透防护（P1）

1. `CacheWarmer` 缓存预热（HotspotTable/HotspotKey/CustomQuery，异步不阻塞启动）
2. `BloomFilter` + `PenetrationGuard` 缓存穿透防护（不存在的 key 直接返回 None，误判率可配默认 1%）
3. `SingleFlight` 缓存击穿防护（并发重建只执行一次，其他等待复用）
4. 预热与 L1/L2 协同（预热数据同时加载到 L1+L2）
5. 布隆过滤器不漏判（不存在的 key 一定返回 None，误判回退 DB）
6. singleflight 不死锁（重建超时释放锁，默认 5 秒）
7. 预热与防护可配置（策略 + 数据量 + 布隆容量/误判率 + singleflight 超时）
8. 复用 v4.6.0 `ProcessL1Cache`/`L2Cache`/`CacheCoherenceProtocol`，不重复实现
9. 预热与防护日志可追溯（策略 + 数据量 + 命中率 + 过滤次数）
10. `cache-warmup-protection` feature gate 隔离，默认关闭，既有 `ProcessL1Cache`/`L2Cache` 保留

---

# 9. feature gate 总览

| feature gate | 所属包 | 控制能力 | 默认 | 对应需求 |
|-------------|--------|---------|------|---------|
| `delayed-priority-queue` | sz-orm-queue（扩展） | 消息延迟队列与优先级调度（延迟投递 + 优先级队列 + 定时调度） | 关闭 | REQ-V47-001 |
| `forward-compat-sandbox` | sz-orm-core（扩展） | 迁移前向兼容性检查与沙箱预演（兼容检查 + 沙箱预演 + 依赖图） | 关闭 | REQ-V47-002 |
| `copy-parallel-shard` | sz-orm-batch（扩展） | 批量 COPY 协议与并行分片执行（COPY 方言适配 + 并行分片 + 冲突解决） | 关闭 | REQ-V47-003 |
| `anomaly-remediation-rca` | sz-orm-observability（扩展） | 异常自愈与根因分析（自愈 + RCA + 关联分析） | 关闭 | REQ-V47-004 |
| `multicloud-cost-forecast` | sz-orm-storage（扩展） | 多云成本对比与容量预测（成本对比 + 容量预测 + 自动优化） | 关闭 | REQ-V47-005 |
| `tenant-quota-rls-enhanced` | sz-orm-core（扩展） | 租户资源配额与行级安全增强（配额 + RLS 增强 + 审计日志） | 关闭 | REQ-V47-006 |
| `cache-warmup-protection` | sz-orm-core（扩展） | 缓存预热与穿透防护（预热 + 布隆过滤器 + singleflight） | 关闭 | REQ-V47-007 |

---

# 10. 与 v4.6.0 的关系

## 10.1 零重叠声明

v4.7.0 与 v4.6.0 零重叠：

| v4.6.0 能力（可靠性 + 运维智能化层） | v4.7.0 能力（智能化运维深化 + 性能深化层） | 关系 |
|-------------------------------|-------------------------|------|
| 消息死信队列自动重投递（`sz-orm-queue` dlx-auto-redelivery） | 消息延迟队列与优先级调度（`sz-orm-queue` delayed-priority-queue） | v4.7.0 延迟队列与优先级调度复用 v4.6.0 `RedeliveryScheduler`（`packages/sz-orm-queue/src/dlx.rs:216`）调度器基线，扩展延迟/优先级/定时维度，不修改既有 DLX 自动重投递逻辑 |
| 迁移回滚自动化（`sz-orm-core` zero-downtime-rollback） | 迁移前向兼容性检查与沙箱预演（`sz-orm-core` forward-compat-sandbox） | v4.7.0 沙箱预演复用 v4.6.0 `RollbackExecutor`（`packages/sz-orm-core/src/rollback_zero_downtime.rs:305`）影子表能力 + 既有 `DryRunMigration`（`packages/sz-orm-core/src/migration_dry_run.rs:11`），扩展前向兼容性检查与依赖图，不修改既有回滚/dry-run 逻辑 |
| 批量事务原子性保证（`sz-orm-batch` batch-atomic） | 批量 COPY 协议与并行分片执行（`sz-orm-batch` copy-parallel-shard） | v4.7.0 并行分片执行复用 v4.6.0 `BatchTransactionCoordinator`（`packages/sz-orm-batch/src/atomic.rs:216`）原子性 + 既有 `CopyProtocolExecutor`（`packages/sz-orm-batch/src/copy.rs:14`），扩展 COPY 方言适配与并行分片，不修改既有原子性/COPY 逻辑 |
| 异常检测（`sz-orm-observability` anomaly-detection） | 异常自愈与根因分析（`sz-orm-observability` anomaly-remediation-rca） | v4.7.0 异常自愈与根因分析复用 v4.6.0 `AnomalyDetector`（`packages/sz-orm-observability/src/anomaly.rs:254`）异常检测，扩展自愈/RCA/关联，不修改既有异常检测逻辑 |
| 存储成本分析与优化建议（`sz-orm-storage` cost-analysis） | 多云成本对比与容量预测（`sz-orm-storage` multicloud-cost-forecast） | v4.7.0 多云成本对比与容量预测复用 v4.6.0 `CostAnalyzer`（`packages/sz-orm-storage/src/cost.rs:231`）成本分析，扩展跨 provider 对比/容量预测/自动优化，不修改既有成本分析逻辑 |
| 连接级多租户隔离（`sz-orm-core` connection-level-tenant） | 租户资源配额与行级安全增强（`sz-orm-core` tenant-quota-rls-enhanced） | v4.7.0 租户配额与 RLS 增强复用 v4.6.0 `ConnectionTenantBinder`（`packages/sz-orm-core/src/connection_tenant.rs:133`）连接级隔离 + 既有 `RowLevelSecurityPolicy`（`packages/sz-orm-core/src/tenant_security.rs:67`），扩展配额/RLS 增强/审计，不修改既有连接级隔离/RLS 逻辑 |
| 进程级 L1 缓存（`sz-orm-core` process-l1-cache） | 缓存预热与穿透防护（`sz-orm-core` cache-warmup-protection） | v4.7.0 缓存预热与穿透防护复用 v4.6.0 `ProcessL1Cache`（`packages/sz-orm-core/src/process_l1_cache.rs:169`）进程级 L1 + 既有 `L2Cache`（`packages/sz-orm-core/src/l2_cache.rs:517`），扩展预热/布隆/singleflight，不修改既有进程级 L1/L2 逻辑 |

## 10.2 依赖关系

```
v4.6.0 已验收基线（7 个 feature gate: dlx-auto-redelivery / zero-downtime-rollback / batch-atomic / anomaly-detection / cost-analysis / connection-level-tenant / process-l1-cache）
  │
  ├─ dlx-auto-redelivery ───→ REQ-V47-001 延迟队列与优先级调度（复用 RedeliveryScheduler 调度器基线）
  ├─ zero-downtime-rollback ─→ REQ-V47-002 前向兼容与沙箱预演（复用 RollbackExecutor 影子表能力）
  ├─ batch-atomic ──────────→ REQ-V47-003 COPY 协议与并行分片（复用 BatchTransactionCoordinator 原子性）
  ├─ anomaly-detection ─────→ REQ-V47-004 异常自愈与根因分析（复用 AnomalyDetector 异常检测）
  ├─ cost-analysis ─────────→ REQ-V47-005 多云成本对比与容量预测（复用 CostAnalyzer 成本分析）
  ├─ connection-level-tenant→ REQ-V47-006 租户配额与 RLS 增强（复用 ConnectionTenantBinder 连接级隔离）
  └─ process-l1-cache ─────→ REQ-V47-007 缓存预热与穿透防护（复用 ProcessL1Cache 进程级 L1）

v4.7.0 七项需求相互独立，可并行开发：
  ├─ REQ-V47-001 延迟队列与优先级调度（扩展 sz-orm-queue，复用既有 MessageQueue + v4.6.0 RedeliveryScheduler）
  ├─ REQ-V47-002 前向兼容与沙箱预演（扩展 sz-orm-core migration，复用既有 DryRunMigration + v4.6.0 RollbackExecutor）
  ├─ REQ-V47-003 COPY 协议与并行分片（扩展 sz-orm-batch，复用既有 CopyProtocolExecutor + v4.6.0 BatchTransactionCoordinator）
  ├─ REQ-V47-004 异常自愈与根因分析（扩展 sz-orm-observability，复用 v4.6.0 AnomalyDetector）
  ├─ REQ-V47-005 多云成本对比与容量预测（扩展 sz-orm-storage，复用 v4.6.0 CostAnalyzer）
  ├─ REQ-V47-006 租户配额与 RLS 增强（扩展 sz-orm-core tenant，复用 v4.6.0 ConnectionTenantBinder + 既有 RowLevelSecurityPolicy）
  └─ REQ-V47-007 缓存预热与穿透防护（扩展 sz-orm-core cache，复用 v4.6.0 ProcessL1Cache + 既有 L2Cache）
```

## 10.3 扩展包

| 包名 | 对应需求 | 扩展内容 |
|------|---------|---------|
| `sz-orm-queue` | REQ-V47-001 | 消息延迟队列与优先级调度（延迟投递 + 优先级队列 + 定时调度，`delayed-priority-queue` feature） |
| `sz-orm-core` | REQ-V47-002 / REQ-V47-006 / REQ-V47-007 | 前向兼容与沙箱预演（`forward-compat-sandbox` feature）+ 租户配额与 RLS 增强（`tenant-quota-rls-enhanced` feature）+ 缓存预热与穿透防护（`cache-warmup-protection` feature） |
| `sz-orm-batch` | REQ-V47-003 | 批量 COPY 协议与并行分片执行（COPY 方言适配 + 并行分片 + 冲突解决，`copy-parallel-shard` feature） |
| `sz-orm-observability` | REQ-V47-004 | 异常自愈与根因分析（自愈 + RCA + 关联，`anomaly-remediation-rca` feature） |
| `sz-orm-storage` | REQ-V47-005 | 多云成本对比与容量预测（成本对比 + 容量预测 + 自动优化，`multicloud-cost-forecast` feature） |

## 10.4 新增包

本版本不新增包，所有能力通过既有包扩展实现（sz-orm-queue / sz-orm-core / sz-orm-batch / sz-orm-observability / sz-orm-storage），减少工作空间成员变更。

---

> 文档生成依据：`docs/spec/v4.6.0/` SDD 三阶段文档（spec 1139 行 / design / tasks，v4.6.0 已全部完成并发布 crates.io）+ 2026-08-13 逐项代码验证（所有 file:line 证据均已实测存在）+ v4.6.0 七项功能自然延伸方向分析（每项 v4.6.0 功能对应一项 v4.7.0 深化延伸）
> 审计合规：本文档所有 file:line 证据均引用真实存在的代码，遵循 AGENTS.md 审计合规铁律
> 下一阶段：spec-design-agent 生成 `design.md`（技术设计），spec-task-agent 生成 `tasks.md`（编码任务规划）
