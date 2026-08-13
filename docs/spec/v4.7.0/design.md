# sz-orm v4.7.0 技术设计文档

> 版本：v4.7.0（消息延迟队列与优先级调度 + 迁移前向兼容性检查与沙箱预演 + 批量 COPY 协议与并行分片执行 + 异常自愈与根因分析 + 多云成本对比与容量预测 + 租户资源配额与行级安全增强 + 缓存预热与穿透防护）
> 基线：v4.6.0（消息死信队列自动重投递 + 迁移回滚自动化 + 批量事务原子性保证 + 异常检测 + 存储成本分析 + 连接级多租户隔离 + 进程级 L1 缓存，7 项需求 REQ-V46-001~007 全部通过 feature gate 隔离，已验收基线，已发布到 crates.io）
> 日期：2026-08-13
> 文档定位：技术设计（How to build），对应需求规格 `spec.md`（What to build，1218 行，7 项 EARS 需求 REQ-V47-001~007）
> 设计约束：无 Breaking Change（7 个新 feature gate 隔离，默认全关闭）+ 优先复用既有能力 + 五方言覆盖（MySQL/PostgreSQL/SQLite/Oracle/MSSQL）+ 每项设计附 file:line 代码证据 + unsafe 零容忍 + 禁止占位实现 + 与 v4.6.0 零重叠 + 不新增包（全部通过既有包扩展实现）+ 参数化查询铁律 + 闭包式 API 风格
> 需求依赖：七项需求主体相互独立，可并行开发（详见 §七 依赖关系图）
> 证据验证：本文档所有 file:line 证据均已通过源码读取验证（2026-08-13，40+ 项关键证据逐项实测，行号均为实际存在行），遵循 AGENTS.md 审计合规铁律

---

# 概述

## 设计目标

本设计文档将 sz-orm v4.7.0 七项智能化运维深化与性能深化需求（REQ-V47-001 ~ REQ-V47-007）转化为可落地的技术方案，核心目标：

1. **消息延迟队列与优先级调度**：扩展既有 `sz-orm-queue` 包，`DelayScheduler` 延迟调度器 + `PriorityQueue` 优先级队列 + `ScheduledMessage` 定时消息，复用既有 `MessageQueue` trait（`packages/sz-orm-queue/src/queue.rs:18`）/ `Message`（`:57`）/ `InMemoryQueue`（`:339`）+ v4.6.0 `RedeliveryScheduler`（`packages/sz-orm-queue/src/dlx.rs:216`）调度器基线，补齐延迟投递（按 `deliver_at` 投递）+ 优先级排序（`PriorityPolicy` Strict/Weighted/FairShare）+ 定时调度（Cron 表达式），aging 机制避免低优先级饿死。
2. **迁移前向兼容性检查与沙箱预演**：扩展既有 `sz-orm-core` 迁移管理，`ForwardCompatChecker` 前向兼容性检查器复用既有 `DryRunMigration`（`packages/sz-orm-core/src/migration_dry_run.rs:11`）/ `ImpactReport`（`:80`），`SandboxDryRunner` 沙箱预演器复用 v4.6.0 `RollbackExecutor`（`packages/sz-orm-core/src/rollback_zero_downtime.rs:305`）影子表能力，在影子表上预执行迁移并校验数据完整性/查询兼容性/性能影响，`MigrationDependencyGraph` 迁移依赖图分析执行顺序与循环检测。
3. **批量 COPY 协议与并行分片执行**：扩展既有 `sz-orm-batch` 包，`CopyProtocolAdapter` COPY 协议方言适配器复用既有 `CopyProtocolExecutor`（`packages/sz-orm-batch/src/copy.rs:14`），补齐 MySQL LOAD DATA INFILE / Oracle SQL*Loader / MSSQL BULK INSERT 方言适配，`ParallelShardExecutor` 并行分片执行器复用 v4.6.0 `BatchTransactionCoordinator`（`packages/sz-orm-batch/src/atomic.rs:216`）原子性，`ConflictResolution` 冲突解决策略（Upsert/Ignore/Merge/Replace）。
4. **异常自愈与根因分析**：扩展既有 `sz-orm-observability` 包，`AutoRemediator` 异常自愈器 + `RootCauseAnalyzer` 根因分析器 + `AnomalyCorrelator` 异常关联器，复用 v4.6.0 `AnomalyDetector`（`packages/sz-orm-observability/src/anomaly.rs:254`）/ `AnomalyAlert`（`:206`）+ 既有 `MetricsRegistry`（`packages/sz-orm-observability/src/lib.rs:262`）/ `QueryLogger`（`packages/sz-orm-observability/src/query_logger.rs:73`），自愈动作须人工确认（可配白名单），根因分析附证据链与置信度。
5. **多云成本对比与容量预测**：扩展既有 `sz-orm-storage` 包，`MultiCloudCostComparator` 多云成本对比器 + `CapacityForecaster` 容量预测器 + `AutoOptimizer` 自动优化执行器，复用 v4.6.0 `CostAnalyzer`（`packages/sz-orm-storage/src/cost.rs:231`）/ `CostReport`（`:213`）/ `ProviderCost`（`:202`）+ 既有 `Storage` trait（`packages/sz-orm-storage/src/storage.rs:14`）/ `StorageProvider`（`:287`），容量预测附置信区间（线性回归/指数平滑/Holt-Winters）。
6. **租户资源配额与行级安全增强**：扩展既有 `sz-orm-core` 多租户，`TenantResourceQuota` 租户资源配额 + `QuotaEnforcer` 配额执行器 + `RlsPolicyEnhancer` RLS 策略增强器 + `TenantAuditLogger` 租户级审计日志器，复用 v4.6.0 `ConnectionTenantBinder`（`packages/sz-orm-core/src/connection_tenant.rs:133`）/ `TenantConnectionGuard`（`:249`）+ 既有 `RowLevelSecurityPolicy`（`packages/sz-orm-core/src/tenant_security.rs:67`）/ `ColumnMaskingRule`（`:155`）/ `TenantAuditContext`（`:244`）/ `Pool`（`packages/sz-orm-core/src/pool.rs:743`），配额检查在连接池/查询层强制执行，RLS 自动注入 WHERE 参数化绑定（⚠️ 2026-08-13 勘误：`QuotaEnforcer`/`RlsPolicyEnhancer` 组件已实现但未自动接线，pool.rs/query.rs 零引用，需手动调用——见 `docs/assessment/2026-08-13-production-zero-call-audit.md` §二-2/3）。
7. **缓存预热与穿透防护**：扩展既有 `sz-orm-core` 缓存，`CacheWarmer` 缓存预热器 + `BloomFilter` 布隆过滤器 + `PenetrationGuard` 穿透防护器 + `SingleFlight` 击穿防护器，复用 v4.6.0 `ProcessL1Cache`（`packages/sz-orm-core/src/process_l1_cache.rs:169`）/ `CrossSessionIdentityMap`（`:366`）+ 既有 `L2Cache`（`packages/sz-orm-core/src/l2_cache.rs:517`）/ `CacheCoherenceProtocol`（`packages/sz-orm-core/src/cache_coherence.rs:103`），预热异步不阻塞启动，布隆过滤器不漏判，singleflight 不死锁（⚠️ 2026-08-13 勘误：`CacheWarmer` 组件已实现但零调用，l2_cache/process_l1_cache 初始化未触发预热，需手动接入——见审计报告 §二-4）。

## 设计约束

| 约束类别 | 约束内容 | 来源 |
|---------|---------|------|
| 兼容性 | 无 Breaking Change，7 个新 feature gate 隔离，默认全关闭，既有公开 API 完全向后兼容 | spec.md §1.4 / §4.5.1 |
| sz-pay 不破坏 | sz-pay 从 crates.io 拉取 sz-orm-* 6 个包既有用法不受影响 | spec.md §4.5.2 |
| 五方言覆盖 | MySQL/PostgreSQL/SQLite/Oracle/MSSQL 行为一致（COPY 协议按方言能力适配，RLS 自动注入 WHERE 全方言支持（⚠️ 2026-08-13 勘误：RLS 增强组件未自动接线，见审计报告 §二-3）） | spec.md §4.4.9 / §1.2.5 |
| 复用优先 | 优先复用既有能力，不重复实现（7 项需求全部通过既有包扩展，不新增包） | spec.md §1.4 / §10.4 |
| unsafe 零容忍 | 无 `unsafe` 块，或必须有 `// SAFETY:` 注释 | spec.md §1.4.13 / §4.3 |
| 禁止占位实现 | 禁止 `todo!`/`unimplemented!`/`unreachable!` | AGENTS.md |
| 参数化查询 | 任何 WHERE 条件必须参数化，禁止 SQL 字符串拼接（复用既有 `Connection::execute_with_params` `packages/sz-orm-core/src/pool.rs:82`） | AGENTS.md / spec.md §4.3.2 |
| 测试基线不回退 | v4.6.0 已验收测试基线仅增不减 | spec.md §4.2.16 |
| 审计证据 | 每项结论附 file:line 证据，遵循审计合规铁律 | spec.md §4.3.12 / AGENTS.md |
| 与 v4.6.0 零重叠 | v4.6.0 是"可靠性+运维智能化"层，v4.7.0 是"智能化运维深化+性能深化"层，新增范围全部落在既有包扩展 | spec.md §1.4 / §10.1 |
| 不新增包 | 7 项需求全部通过既有包扩展实现（sz-orm-queue/sz-orm-core/sz-orm-batch/sz-orm-observability/sz-orm-storage） | spec.md §10.4 |
| 延迟队列不丢失消息 | 延迟消息到期前保留在延迟队列，到期后按优先级投递，投递失败按 v4.6.0 退避策略重试 | spec.md §4.2.1 |
| 优先级不饿死 | Strict 策略下 aging 机制提升等待过久的低优先级消息优先级（默认 5 分钟） | spec.md §4.2.2 |
| 沙箱不修改真实数据 | 沙箱预演在影子表执行，预演失败清理影子表不残留 | spec.md §4.2.4 |
| COPY 不丢数据 | COPY 协议加载成功数据全部落库，失败按冲突解决策略处理 | spec.md §4.2.5 |
| 异常自愈不误修复 | 自愈动作须人工确认（可配白名单），记录审计日志，不静默执行 | spec.md §1.4.20 / §4.2.7 |
| 根因分析附证据链 | 根因分析附证据链（指标+日志+拓扑），置信度可配阈值 | spec.md §4.2.8 |
| 容量预测附置信区间 | 容量预测附置信区间（默认 95%），不单点预测 | spec.md §4.2.10 |
| 配额不超限 | 租户配额严格执行，超限拒绝请求，配额检查在连接池/查询层强制不可绕过 | spec.md §4.2.11 / §4.3.7 |
| RLS 防越权 | RLS 自动注入 WHERE 参数化绑定，tenant_id 不可被客户端篡改（⚠️ 2026-08-13 勘误：组件已实现但未自动接线，见审计报告 §二-3） | spec.md §4.2.12 / §4.3.8 |
| 审计日志不可篡改 | 租户级审计日志追加写入不可修改/删除 | spec.md §4.3.9 |
| 预热不阻塞启动 | 缓存预热异步执行不阻塞服务启动，预热失败不影响启动（⚠️ 2026-08-13 勘误：`CacheWarmer` 已实现但零调用，无启动预热路径，见审计报告 §二-4） | spec.md §4.2.13 |
| 布隆不漏判 | 布隆过滤器不存在的 key 一定返回 None，误判回退 DB 查询 | spec.md §4.2.14 |
| singleflight 不死锁 | singleflight 重建超时释放锁，其他请求可重试 | spec.md §4.2.15 |

## feature gate 总览

| feature | 所属包 | 控制能力 | 默认 | 对应需求 | 依赖既有 feature |
|---------|--------|---------|------|---------|----------------|
| `delayed-priority-queue` | sz-orm-queue（扩展） | 消息延迟队列与优先级调度（延迟投递 + 优先级队列 + 定时调度） | 关闭 | REQ-V47-001 | `dlx-auto-redelivery`（复用 RedeliveryScheduler 调度器基线） |
| `forward-compat-sandbox` | sz-orm-core（扩展） | 迁移前向兼容性检查与沙箱预演（兼容检查 + 沙箱预演 + 依赖图） | 关闭 | REQ-V47-002 | `zero-downtime-rollback`（复用 RollbackExecutor 影子表能力） |
| `copy-parallel-shard` | sz-orm-batch（扩展） | 批量 COPY 协议与并行分片执行（COPY 方言适配 + 并行分片 + 冲突解决） | 关闭 | REQ-V47-003 | `batch-atomic`（复用 BatchTransactionCoordinator 原子性） |
| `anomaly-remediation-rca` | sz-orm-observability（扩展） | 异常自愈与根因分析（自愈 + RCA + 关联分析） | 关闭 | REQ-V47-004 | `anomaly-detection`（复用 AnomalyDetector 异常检测） |
| `multicloud-cost-forecast` | sz-orm-storage（扩展） | 多云成本对比与容量预测（成本对比 + 容量预测 + 自动优化） | 关闭 | REQ-V47-005 | `cost-analysis`（复用 CostAnalyzer 成本分析） |
| `tenant-quota-rls-enhanced` | sz-orm-core（扩展） | 租户资源配额与行级安全增强（配额 + RLS 增强 + 审计日志） | 关闭 | REQ-V47-006 | `connection-level-tenant`（复用 ConnectionTenantBinder 连接级隔离） |
| `cache-warmup-protection` | sz-orm-core（扩展） | 缓存预热与穿透防护（预热 + 布隆过滤器 + singleflight） | 关闭 | REQ-V47-007 | `process-l1-cache`（复用 ProcessL1Cache 进程级 L1） |

**既有 feature gate 位置（v4.6.0 基线，扩展基线）：**
- `dlx-auto-redelivery`：`packages/sz-orm-queue/Cargo.toml:38`
- `zero-downtime-rollback`：`packages/sz-orm-core/Cargo.toml:134`
- `batch-atomic`：`packages/sz-orm-batch/Cargo.toml:28`
- `anomaly-detection`：`packages/sz-orm-observability/Cargo.toml:28`
- `cost-analysis`：`packages/sz-orm-storage/Cargo.toml:19`
- `connection-level-tenant`：`packages/sz-orm-core/Cargo.toml:136`
- `process-l1-cache`：`packages/sz-orm-core/Cargo.toml:138`

## 架构总览

### 扩展包总览（不新增包）

| 包名 | 对应需求 | 依赖（只读复用） | 扩展内容 |
|------|---------|----------------|---------|------|
| `sz-orm-queue` | REQ-V47-001 | 既有 MessageQueue/Message/InMemoryQueue + v4.6.0 RedeliveryScheduler/BackoffPolicy + message_tracing | 延迟调度器 + 优先级队列 + 定时消息（`delayed-priority-queue` feature） |
| `sz-orm-core` | REQ-V47-002 / 006 / 007 | 既有 Migration/DryRunMigration + v4.6.0 RollbackExecutor + 既有 RowLevelSecurityPolicy/ColumnMaskingRule/TenantAuditContext + v4.6.0 ConnectionTenantBinder + 既有 Pool + v4.6.0 ProcessL1Cache + 既有 L2Cache/CacheCoherenceProtocol | 前向兼容与沙箱预演 + 租户配额与 RLS 增强 + 缓存预热与穿透防护（3 个 feature） |
| `sz-orm-batch` | REQ-V47-003 | 既有 CopyProtocolExecutor/BatchExecutor + v4.6.0 BatchTransactionCoordinator/AtomicityGuarantee/SagaCompensator | COPY 协议方言适配 + 并行分片执行 + 冲突解决（`copy-parallel-shard` feature） |
| `sz-orm-observability` | REQ-V47-004 | v4.6.0 AnomalyDetector/AnomalyAlert/Anomaly + 既有 MetricsRegistry/QueryLogger | 异常自愈 + 根因分析 + 异常关联（`anomaly-remediation-rca` feature） |
| `sz-orm-storage` | REQ-V47-005 | v4.6.0 CostAnalyzer/CostReport/ProviderCost/CostOptimizationSuggestion + 既有 Storage/StorageProvider | 多云成本对比 + 容量预测 + 自动优化（`multicloud-cost-forecast` feature） |

### 依赖关系图

```plantuml
@startuml
title sz-orm v4.7.0 七项需求 依赖关系

rectangle "sz-orm v4.7.0 扩展（不新增包）" as v47 {
  rectangle "sz-orm-queue\n(REQ-V47-001 延迟优先级)" as queue
  rectangle "sz-orm-core\n(REQ-V47-002 兼容沙箱\n/006 配额RLS/007 预热防护)" as core
  rectangle "sz-orm-batch\n(REQ-V47-003 COPY分片)" as batch
  rectangle "sz-orm-observability\n(REQ-V47-004 自愈RCA)" as obs
  rectangle "sz-orm-storage\n(REQ-V47-005 多云预测)" as storage
}

rectangle "v4.6.0 基线 feature" as v46 {
  rectangle "dlx-auto-redelivery\n(RedeliveryScheduler)" as v46dlx
  rectangle "zero-downtime-rollback\n(RollbackExecutor)" as v46rb
  rectangle "batch-atomic\n(BatchTransactionCoordinator)" as v46ba
  rectangle "anomaly-detection\n(AnomalyDetector)" as v46ad
  rectangle "cost-analysis\n(CostAnalyzer)" as v46ca
  rectangle "connection-level-tenant\n(ConnectionTenantBinder)" as v46ct
  rectangle "process-l1-cache\n(ProcessL1Cache)" as v46pl
}

database "MySQL/PG/SQLite\n/Oracle/MSSQL" as db
rectangle "消息队列 provider\n(既有 6 个)" as mq
rectangle "对象存储 provider\n(既有 7 个)" as obj

queue --> v46dlx : 复用 RedeliveryScheduler 调度器基线
queue --> mq : 复用既有队列
core --> v46rb : 复用 RollbackExecutor 影子表
core --> v46ct : 复用 ConnectionTenantBinder 连接级隔离
core --> v46pl : 复用 ProcessL1Cache 进程级 L1
core --> db : 影子表/RLS 注入 WHERE/预热加载
batch --> v46ba : 复用 BatchTransactionCoordinator 原子性
batch --> db : COPY/LOAD DATA/分片并行
obs --> v46ad : 复用 AnomalyDetector 异常检测
obs --> db : 自愈动作+根因定位
storage --> v46ca : 复用 CostAnalyzer 成本分析
storage --> obj : 复用既有 7 provider

note right of v47
  v4.7.0 七项需求主体独立
  可并行开发
  与 v4.6.0 零重叠
  不新增包（全部既有包扩展）
  每项复用对应 v4.6.0 feature 基线
end note

@enduml
```

---

# 一、需求与存量功能关系分析

## 1.1 需求功能与存量功能对比

### 1.1.1 已实现功能（可直接复用，匹配度 100%）

本节列出 v4.7.0 七项需求可直接复用的既有功能（v4.6.0 基线 + 既有代码），这些功能无需修改，作为扩展基线。

| 需求功能 | 存量功能 | 代码位置 | 匹配度 |
|---------|---------|---------|--------|
| REQ-V47-001 消息队列 trait | `MessageQueue`（publish/consume/ack/nack/reject/subscribe，异步 trait） | `packages/sz-orm-queue/src/queue.rs:18` | 100% |
| REQ-V47-001 消息结构 | `Message`（topic/payload/key/timestamp/headers/id/retry_count） | `packages/sz-orm-queue/src/queue.rs:57` | 100% |
| REQ-V47-001 内存队列实现 | `InMemoryQueue`（既有内存队列，含死信队列） | `packages/sz-orm-queue/src/queue.rs:339` | 100% |
| REQ-V47-001 重投递调度器基线 | `RedeliveryScheduler`（v4.6.0 自动重投递调度器） | `packages/sz-orm-queue/src/dlx.rs:216` | 100% |
| REQ-V47-001 退避策略 | `BackoffPolicy`（Fixed/Exponential/Linear/RandomJitter） | `packages/sz-orm-queue/src/dlx.rs:47` | 100% |
| REQ-V47-001 DLX 路由策略 | `DlxRoutingStrategy`（RequeueToOriginal/ForwardToDlxTopic/...） | `packages/sz-orm-queue/src/dlx.rs:83` | 100% |
| REQ-V47-002 迁移结构 | `Migration`（version/up_sql/down_sql/description） | `packages/sz-orm-core/src/migration.rs:11` | 100% |
| REQ-V47-002 迁移解析器 | `MigrationResolver`（trait）/ `FileMigrationResolver` | `packages/sz-orm-core/src/migration.rs:63` | 100% |
| REQ-V47-002 迁移上下文 | `MigrationContext`（迁移执行上下文） | `packages/sz-orm-core/src/migration.rs:194` | 100% |
| REQ-V47-002 dry-run 迁移 | `DryRunMigration`（既有 dry-run 迁移执行器） | `packages/sz-orm-core/src/migration_dry_run.rs:11` | 100% |
| REQ-V47-002 dry-run 报告 | `DryRunReport`（dry-run 结果报告） | `packages/sz-orm-core/src/migration_dry_run.rs:24` | 100% |
| REQ-V47-002 DDL 类型 | `DdlType`（Create/Drop/Alter/...） | `packages/sz-orm-core/src/migration_dry_run.rs:33` | 100% |
| REQ-V47-002 锁类型 | `LockType`（AccessExclusive/ShareRowExclusive/...） | `packages/sz-orm-core/src/migration_dry_run.rs:48` | 100% |
| REQ-V47-002 迁移影响 | `MigrationImpact`（迁移对 schema 的影响） | `packages/sz-orm-core/src/migration_dry_run.rs:59` | 100% |
| REQ-V47-002 影响报告 | `ImpactReport`（含 DDL 类型 + 锁类型 + 影响范围） | `packages/sz-orm-core/src/migration_dry_run.rs:80` | 100% |
| REQ-V47-002 回滚执行器基线 | `RollbackExecutor`（v4.6.0 零停机回滚执行器） | `packages/sz-orm-core/src/rollback_zero_downtime.rs:305` | 100% |
| REQ-V47-002 回滚计划 | `RollbackPlan`（回滚步骤计划） | `packages/sz-orm-core/src/rollback_zero_downtime.rs:157` | 100% |
| REQ-V47-003 COPY 协议执行器基线 | `CopyProtocolExecutor`（既有 PostgreSQL COPY 协议执行器） | `packages/sz-orm-batch/src/copy.rs:14` | 100% |
| REQ-V47-003 批量执行器 | `BatchExecutor`（既有批量执行器） | `packages/sz-orm-batch/src/executor.rs:141` | 100% |
| REQ-V47-003 批量执行配置 | `BatchExecutorConfig`（批量执行配置） | `packages/sz-orm-batch/src/executor.rs:18` | 100% |
| REQ-V47-003 批量执行结果 | `BatchExecutionResult`（批量执行结果） | `packages/sz-orm-batch/src/executor.rs:93` | 100% |
| REQ-V47-003 原子性保证级别 | `AtomicityGuarantee`（AllOrNothing/BestEffort/SagaCompensation） | `packages/sz-orm-batch/src/atomic.rs:20` | 100% |
| REQ-V47-003 批量事务协调器基线 | `BatchTransactionCoordinator`（v4.6.0 批量事务协调器） | `packages/sz-orm-batch/src/atomic.rs:216` | 100% |
| REQ-V47-003 Saga 补偿器 | `SagaCompensator`（Saga 补偿回滚） | `packages/sz-orm-batch/src/atomic.rs:436` | 100% |
| REQ-V47-004 异常检测器基线 | `AnomalyDetector`（v4.6.0 异常检测器） | `packages/sz-orm-observability/src/anomaly.rs:254` | 100% |
| REQ-V47-004 异常告警 | `AnomalyAlert`（异常告警结构） | `packages/sz-orm-observability/src/anomaly.rs:206` | 100% |
| REQ-V47-004 异常结构 | `Anomaly`（异常事件结构） | `packages/sz-orm-observability/src/anomaly.rs:154` | 100% |
| REQ-V47-004 异常配置 | `AnomalyConfig`（异常检测配置） | `packages/sz-orm-observability/src/anomaly.rs:74` | 100% |
| REQ-V47-004 异常算法 | `AnomalyAlgorithm`（Threshold/Trend/Statistical/ZScore/IQR） | `packages/sz-orm-observability/src/anomaly.rs:40` | 100% |
| REQ-V47-004 指标注册表 | `MetricsRegistry`（既有指标注册表） | `packages/sz-orm-observability/src/lib.rs:262` | 100% |
| REQ-V47-004 查询日志器 | `QueryLogger`（既有查询日志器） | `packages/sz-orm-observability/src/query_logger.rs:73` | 100% |
| REQ-V47-005 成本分析器基线 | `CostAnalyzer`（v4.6.0 成本分析器） | `packages/sz-orm-storage/src/cost.rs:231` | 100% |
| REQ-V47-005 成本报告 | `CostReport`（成本报表） | `packages/sz-orm-storage/src/cost.rs:213` | 100% |
| REQ-V47-005 provider 成本 | `ProviderCost`（单 provider 成本） | `packages/sz-orm-storage/src/cost.rs:202` | 100% |
| REQ-V47-005 bucket 成本 | `BucketCost`（单 bucket 成本） | `packages/sz-orm-storage/src/cost.rs:181` | 100% |
| REQ-V47-005 成本优化建议 | `CostOptimizationSuggestion`（TierDowngrade/LifecycleOptimize/DeleteExpired/CompressCold） | `packages/sz-orm-storage/src/cost.rs:55` | 100% |
| REQ-V47-005 成本配置 | `CostConfig`（成本分析配置） | `packages/sz-orm-storage/src/cost.rs:96` | 100% |
| REQ-V47-005 存储 trait | `Storage`（存储抽象 trait） | `packages/sz-orm-storage/src/storage.rs:14` | 100% |
| REQ-V47-005 存储构建器 | `StorageBuilder`（存储构建器） | `packages/sz-orm-storage/src/storage.rs:22` | 100% |
| REQ-V47-005 存储 provider | `StorageProvider`（7 provider: Local/S3/Aliyun/Tencent/Huawei/Upyun/Qiniu） | `packages/sz-orm-storage/src/storage.rs:287` | 100% |
| REQ-V47-006 连接租户绑定器基线 | `ConnectionTenantBinder`（v4.6.0 连接租户绑定器） | `packages/sz-orm-core/src/connection_tenant.rs:133` | 100% |
| REQ-V47-006 连接级隔离 | `ConnectionLevelIsolation`（SetTenantId/SchemaIsolation/ConnectionBinding） | `packages/sz-orm-core/src/connection_tenant.rs:24` | 100% |
| REQ-V47-006 连接亲和策略 | `ConnectionAffinityPolicy`（Strict/Preferred/None） | `packages/sz-orm-core/src/connection_tenant.rs:35` | 100% |
| REQ-V47-006 租户连接守卫 | `TenantConnectionGuard`（RAII 守卫） | `packages/sz-orm-core/src/connection_tenant.rs:249` | 100% |
| REQ-V47-006 RLS 策略基线 | `RowLevelSecurityPolicy`（既有行级安全策略） | `packages/sz-orm-core/src/tenant_security.rs:67` | 100% |
| REQ-V47-006 列级脱敏 | `ColumnMaskingRule`（列级脱敏规则） | `packages/sz-orm-core/src/tenant_security.rs:155` | 100% |
| REQ-V47-006 审计操作 | `TenantAuditOperation`（审计操作枚举） | `packages/sz-orm-core/src/tenant_security.rs:197` | 100% |
| REQ-V47-006 审计结果 | `AuditResult`（审计结果枚举） | `packages/sz-orm-core/src/tenant_security.rs:224` | 100% |
| REQ-V47-006 审计上下文 | `TenantAuditContext`（审计上下文） | `packages/sz-orm-core/src/tenant_security.rs:244` | 100% |
| REQ-V47-006 租户上下文 | `TenantContext`（租户上下文） | `packages/sz-orm-core/src/tenant_context.rs:80` | 100% |
| REQ-V47-006 隔离策略 | `IsolationStrategy`（隔离策略枚举） | `packages/sz-orm-core/src/tenant_context.rs:22` | 100% |
| REQ-V47-006 连接池 | `Pool`（自研连接池） | `packages/sz-orm-core/src/pool.rs:743` | 100% |
| REQ-V47-006 Connection trait | `Connection`（连接抽象 trait） | `packages/sz-orm-core/src/pool.rs:45` | 100% |
| REQ-V47-006 参数化执行 | `Connection::execute_with_params`（参数化查询执行） | `packages/sz-orm-core/src/pool.rs:82` | 100% |
| REQ-V47-006 池化连接 | `PooledConnection`（池化连接） | `packages/sz-orm-core/src/pool.rs:239` | 100% |
| REQ-V47-007 进程级 L1 缓存基线 | `ProcessL1Cache<T>`（v4.6.0 进程级 L1 缓存） | `packages/sz-orm-core/src/process_l1_cache.rs:169` | 100% |
| REQ-V47-007 跨会话 Identity Map | `CrossSessionIdentityMap<T>`（跨会话身份映射） | `packages/sz-orm-core/src/process_l1_cache.rs:366` | 100% |
| REQ-V47-007 进程级 L1 配置 | `ProcessL1Config`（进程级 L1 配置） | `packages/sz-orm-core/src/process_l1_cache.rs:44` | 100% |
| REQ-V47-007 进程级 L1 统计 | `ProcessL1Stats`（进程级 L1 统计） | `packages/sz-orm-core/src/process_l1_cache.rs:103` | 100% |
| REQ-V47-007 租户缓存键 | `tenant_cache_key`（租户缓存键生成函数） | `packages/sz-orm-core/src/process_l1_cache.rs:421` | 100% |
| REQ-V47-007 L2 缓存 | `L2Cache`（既有 L2 缓存） | `packages/sz-orm-core/src/l2_cache.rs:517` | 100% |
| REQ-V47-007 缓存键 | `CacheKey`（缓存键结构） | `packages/sz-orm-core/src/l2_cache.rs:143` | 100% |
| REQ-V47-007 缓存一致性协议 | `CacheCoherenceProtocol`（MESI 一致性协议） | `packages/sz-orm-core/src/cache_coherence.rs:103` | 100% |
| REQ-V47-007 MESI 状态 | `MesiState`（Modified/Exclusive/Shared/Invalid） | `packages/sz-orm-core/src/cache_coherence.rs:12` | 100% |
| REQ-V47-007 L1 缓存基线 | `L1Cache<T>`（既有 L1 缓存 Identity Map） | `packages/sz-orm-core/src/l1_cache.rs:87` | 100% |

### 1.1.2 需要扩展的功能（部分匹配，需在现有基础上改造）

本节列出 v4.7.0 七项需求与存量代码部分匹配、需要在现有基础上扩展的功能。

| 需求功能 | 存量功能 | 差异说明 | 扩展方向 |
|---------|---------|---------|---------|
| REQ-V47-001 延迟投递 | 既有 `Message.timestamp`（`queue.rs:57`）仅记录发布时间，无 `deliver_at` 投递时间 | 输入差异：需新增 `deliver_at: DateTime` 字段；业务逻辑差异：到期前不可消费，到期后投递；既有 Message 无延迟语义 | 新增 `DelayedMessage` 结构包装 `Message` + `deliver_at`，`DelayScheduler` 按 `deliver_at` 排序存储，到期后转入优先级队列，复用既有 `MessageQueue.publish` 投递 |
| REQ-V47-001 优先级排序 | 既有 `InMemoryQueue`（`queue.rs:339`）按 FIFO 投递，无优先级 | 业务逻辑差异：需按优先级排序投递而非 FIFO；边界差异：需 aging 机制避免饿死 | 新增 `PriorityQueue` 基于 `BinaryHeap` 按 `priority` 排序，`PriorityPolicy` 枚举控制排序策略，aging 机制提升等待过久的低优先级消息，复用既有 `MessageQueue.consume` |
| REQ-V47-001 定时调度 | v4.6.0 `RedeliveryScheduler`（`dlx.rs:216`）是重投递调度，非 Cron 定时调度 | 业务逻辑差异：重投递是失败后重试，定时调度是按 Cron 周期主动投递；输入差异：需 Cron 表达式 | 新增 `ScheduledMessage` 含 `cron` 或 `interval`，`DelayScheduler` 按 Cron 调度周期投递，复用 `RedeliveryScheduler` 的调度循环基线（不修改既有重投递逻辑） |
| REQ-V47-002 前向兼容性检查 | 既有 `DryRunMigration`（`migration_dry_run.rs:11`）分析迁移影响（DDL 类型/锁类型），但不检查前向兼容性（新 schema 是否破坏旧应用） | 业务逻辑差异：dry-run 分析"迁移做什么"，前向兼容性检查分析"迁移是否破坏旧应用"；输出差异：需 `CompatCheckResult` 而非 `ImpactReport` | 新增 `ForwardCompatChecker` 复用 `DryRunMigration.analyze_impact` 获取 `ImpactReport`，在此基础上识别破坏性变更（删除列/改列类型/改列约束/改表名），生成 `CompatCheckResult`，不修改既有 dry-run 逻辑 |
| REQ-V47-002 沙箱预演 | 既有 `DryRunMigration` 仅语法/影响分析不执行 SQL，v4.6.0 `RollbackExecutor`（`rollback_zero_downtime.rs:305`）有影子表能力但用于回滚非预演 | 业务逻辑差异：沙箱预演需在影子表上真实执行迁移 SQL 并校验，dry-run 不执行；边界差异：预演后须清理影子表 | 新增 `SandboxDryRunner` 创建影子表（`shadow_` 前缀），在影子表上执行迁移 SQL（参数化绑定，复用 `Connection::execute_with_params` `pool.rs:82`），校验数据完整性/查询兼容性/性能影响，预演后清理影子表，复用 `RollbackExecutor` 影子表管理能力 |
| REQ-V47-003 COPY 方言适配 | 既有 `CopyProtocolExecutor`（`copy.rs:14`）仅支持 PostgreSQL 系 COPY 协议 | 输入差异：需支持 MySQL LOAD DATA INFILE / Oracle SQL*Loader / MSSQL BULK INSERT；边界差异：SQLite 不支持 COPY 须降级 | 新增 `CopyProtocolAdapter` 按方言选择批量加载协议（PG COPY / MySQL LOAD DATA / Oracle SQL*Loader / MSSQL BULK INSERT / SQLite 降级 multi-value INSERT），复用既有 `CopyProtocolExecutor` 的 PG COPY 实现 |
| REQ-V47-003 并行分片 | 既有 `BatchExecutor`（`executor.rs:141`）单线程批量执行，v4.6.0 `BatchTransactionCoordinator`（`atomic.rs:216`）协调跨批次原子性但不分片 | 业务逻辑差异：并行分片需按分片键拆分数据并行执行；边界差异：并行度受连接池容量限制 | 新增 `ParallelShardExecutor` 按分片键拆分数据为 N 分片，`tokio::join!` 并行执行，复用 `BatchTransactionCoordinator` 保证分片间原子性，并行度限制到连接池容量 |
| REQ-V47-003 冲突解决 | 既有 `BatchExecutor` 遇主键冲突直接报错，无冲突解决策略 | 业务逻辑差异：需按策略处理冲突（Upsert/Ignore/Merge/Replace）而非直接报错 | 新增 `ConflictResolution` 枚举，`CopyProtocolAdapter` 按策略生成 `ON CONFLICT DO UPDATE` / `ON CONFLICT DO NOTHING` / `REPLACE INTO` / 自定义 Merge SQL |
| REQ-V47-004 异常自愈 | v4.6.0 `AnomalyDetector`（`anomaly.rs:254`）仅检测异常并告警，不执行修复动作 | 业务逻辑差异：检测后须执行修复动作；边界差异：须人工确认（白名单除外） | 新增 `AutoRemediator` + `RemediationAction` 枚举，检测到异常后选择修复动作，白名单内自动执行，非白名单请求人工确认，记录审计日志，复用 `AnomalyDetector.detect` |
| REQ-V47-004 根因分析 | v4.6.0 `AnomalyDetector` 检测异常但不归因到具体组件/SQL | 输出差异：需 `RootCause`（根因组件+SQL+置信度+证据链）而非仅 `AnomalyAlert` | 新增 `RootCauseAnalyzer` 收集异常上下文（复用 `MetricsRegistry` `lib.rs:262` 指标 + `QueryLogger` `query_logger.rs:73` 日志 + 拓扑），推断根因，生成 `RootCause` 附证据链与置信度 |
| REQ-V47-005 多云成本对比 | v4.6.0 `CostAnalyzer`（`cost.rs:231`）分析单 provider 成本，不跨 provider 对比 | 业务逻辑差异：需跨 provider 对比同容量成本差异；输出差异：需 `CostComparisonReport` 含迁移建议 | 新增 `MultiCloudCostComparator` 对每个 provider 调用 `CostAnalyzer.analyze_cost` 获取 `ProviderCost`，计算成本差异，生成 `CostComparisonReport` 含迁移建议，不修改既有成本分析逻辑 |
| REQ-V47-005 容量预测 | v4.6.0 `CostAnalyzer` 分析当前成本，不预测未来容量 | 业务逻辑差异：需基于历史时间序列预测未来；输出差异：需 `CapacityForecast` 含置信区间 | 新增 `CapacityForecaster` 基于历史容量数据（时间序列）按算法（LinearRegression/ExponentialSmoothing/HoltWinters）预测，计算置信区间，生成 `CapacityForecast` |
| REQ-V47-005 成本自动优化 | v4.6.0 `CostAnalyzer.suggest_optimization`（`cost.rs:293`）仅生成 `CostOptimizationSuggestion`，不自动执行 | 业务逻辑差异：须自动执行优化建议而非仅生成；边界差异：须人工确认（白名单除外） | 新增 `AutoOptimizer` 接收 `CostOptimizationSuggestion`，白名单内自动执行（如降级 tier），非白名单请求人工确认，生成 `OptimizationExecutionResult`，复用既有 `Storage` trait 执行优化 |
| REQ-V47-006 租户资源配额 | v4.6.0 `ConnectionTenantBinder`（`connection_tenant.rs:133`）绑定连接到租户，但不限制租户资源配额 | 业务逻辑差异：须按租户限制 max_connections/max_qps/max_storage；边界差异：超限拒绝请求 | 新增 `TenantResourceQuota` + `QuotaEnforcer`，在 `ConnectionTenantBinder.acquire_with_tenant` 路径上插入配额检查，超限拒绝，复用既有 `TenantConnectionGuard`（`:249`） |
| REQ-V47-006 RLS 策略增强 | 既有 `RowLevelSecurityPolicy`（`tenant_security.rs:67`）支持单条件 WHERE 注入，不支持多条件组合/复杂谓词 | 业务逻辑差异：需支持多条件组合（`tenant_id=? AND dept_id IN (?,?)`）；边界差异：须与列级脱敏联动 | 新增 `RlsPolicyEnhancer` + `EnhancedRlsPolicy` 支持多条件组合与复杂谓词，自动注入 WHERE 参数化绑定（复用 `Connection::execute_with_params` `pool.rs:82`），与 `ColumnMaskingRule`（`:155`）联动，不修改既有 `RowLevelSecurityPolicy`（⚠️ 2026-08-13 勘误：组件已实现但未自动接线，query.rs 零引用，需手动调用——见审计报告 §二-3） |
| REQ-V47-006 租户级审计日志 | 既有 `TenantAuditContext`（`tenant_security.rs:244`）/ `TenantAuditOperation`（`:197`）定义审计结构，但无独立租户级审计日志器 | 业务逻辑差异：须按租户独立记录审计日志；边界差异：追加写入不可篡改 | 新增 `TenantAuditLogger` + `TenantAuditEntry`，按租户独立记录审计日志（连接/查询/配额超限/RLS 命中），追加写入不可篡改，复用既有 `TenantAuditContext`/`TenantAuditOperation`/`AuditResult`（`:224`） |
| REQ-V47-007 缓存预热 | v4.6.0 `ProcessL1Cache`（`process_l1_cache.rs:169`）按需加载，无启动预热 | 业务逻辑差异：须启动时预加载热点数据；边界差异：异步不阻塞启动 | 新增 `CacheWarmer` + `WarmupConfig`，按预热策略（HotspotTable/HotspotKey/CustomQuery）异步预加载热点数据到 L1+L2，复用 `ProcessL1Cache.put` + `L2Cache.put`，预热失败不影响启动（⚠️ 2026-08-13 勘误：`CacheWarmer` 已实现但零调用，无启动预热路径——见审计报告 §二-4） |
| REQ-V47-007 缓存穿透防护 | 既有 `ProcessL1Cache`/`L2Cache` 查询未命中直接查 DB，无布隆过滤器拦截 | 业务逻辑差异：须查询前判断 key 是否可能存在；边界差异：不漏判（不存在一定返回 None） | 新增 `BloomFilter` + `PenetrationGuard`，查询前 `BloomFilter.might_contain` 判断，不存在直接返回 None 不查 DB，误判回退 DB，复用既有 L1/L2 查询路径 |
| REQ-V47-007 缓存击穿防护 | 既有 `ProcessL1Cache` 缓存未命中时各请求独立查 DB 重建，无 singleflight | 业务逻辑差异：须并发重建只执行一次其他等待复用；边界差异：超时释放锁不死锁 | 新增 `SingleFlight` + `StampedeGuard`，对同一 key 的并发重建请求只执行一次（`tokio::sync::OnceCell` 或 `Notify` 等待），其他等待结果复用，超时释放锁，复用 `ProcessL1Cache` 重建逻辑 |

### 1.1.3 需要新增的功能或接口（存量代码无对应实现）

本节列出 v4.7.0 七项需求在存量代码中完全没有对应实现、需新增的功能。所有新增功能通过 feature gate 隔离，默认关闭。

**REQ-V47-001 消息延迟队列与优先级调度（sz-orm-queue 扩展，`delayed-priority-queue` feature）：**
- `DelayedMessage`：延迟消息结构（包装 `Message` + `deliver_at: DateTime` + `priority: i32`），输入：消息+投递时间+优先级，输出：延迟消息，核心逻辑：按 `deliver_at` 排序存储。依赖既有 `Message`（`queue.rs:57`）。
- `PriorityQueue`：优先级队列（基于 `BinaryHeap` 按 `priority` 排序），输入：消息+优先级，输出：最高优先级消息，核心逻辑：O(log n) 插入排序 + aging 机制。依赖既有 `MessageQueue`（`queue.rs:18`）。
- `PriorityPolicy`：优先级策略枚举（Strict/Weighted/FairShare），控制 `PriorityQueue` 排序策略。
- `ScheduledMessage`：定时消息结构（包装 `Message` + `cron: String` 或 `interval: Duration`），输入：消息+Cron/间隔，输出：定时消息，核心逻辑：按 Cron 调度周期投递。依赖 v4.6.0 `RedeliveryScheduler`（`dlx.rs:216`）调度循环基线。
- `DelayScheduler`：延迟调度器（管理 `DelayedMessage` 到期检查 + `PriorityQueue` 投递 + `ScheduledMessage` Cron 调度），输入：延迟/定时消息，输出：到期后投递到既有队列，核心逻辑：调度周期检查到期消息→转入优先级队列→投递。依赖既有 `MessageQueue` + v4.6.0 `RedeliveryScheduler`。
- `ScheduleConfig`：调度配置（`DelayScheduler` 的配置结构）。

**REQ-V47-002 迁移前向兼容性检查与沙箱预演（sz-orm-core 扩展，`forward-compat-sandbox` feature）：**
- `ForwardCompatChecker`：前向兼容性检查器，输入：迁移，输出：`CompatCheckResult`，核心逻辑：复用 `DryRunMigration.analyze_impact` 获取 `ImpactReport`→识别破坏性变更→生成兼容性结果。依赖既有 `DryRunMigration`（`migration_dry_run.rs:11`）/ `ImpactReport`（`:80`）。
- `CompatCheckResult`：兼容性检查结果（破坏性变更列表 + 影响的应用 + 建议的兼容策略）。
- `BreakingChangeType`：破坏性变更类型枚举（DropColumn/AlterColumnType/AlterColumnConstraint/RenameTable）。
- `CompatStrictness`：检查严格度枚举（Strict/Lenient）。
- `SandboxDryRunner`：沙箱预演器，输入：迁移+影子表名，输出：`SandboxResult`，核心逻辑：创建影子表→执行迁移 SQL（参数化）→校验数据完整性/查询兼容性/性能影响→清理影子表。依赖 v4.6.0 `RollbackExecutor`（`rollback_zero_downtime.rs:305`）影子表能力 + 既有 `Connection::execute_with_params`（`pool.rs:82`）。
- `SandboxConfig`：沙箱配置（影子表前缀 + 验证项 + 表名策略）。
- `SandboxVerifyItem`：沙箱验证项枚举（DataIntegrity/QueryCompat/PerformanceImpact）。
- `SandboxResult`：沙箱预演结果（通过/失败 + 原因 + 校验详情）。
- `MigrationDependencyGraph`：迁移依赖图（节点=迁移，边=依赖关系），核心逻辑：拓扑排序 + 循环检测。
- `DependencyAnalyzer`：依赖分析器，输入：迁移列表，输出：`MigrationDependencyGraph`，核心逻辑：分析迁移间依赖关系构建图。

**REQ-V47-003 批量 COPY 协议与并行分片执行（sz-orm-batch 扩展，`copy-parallel-shard` feature）：**
- `CopyProtocolAdapter`：COPY 协议方言适配器，输入：数据+表名+方言，输出：批量加载结果，核心逻辑：按方言选择 COPY/LOAD DATA/SQL*Loader/BULK INSERT/multi-value INSERT。依赖既有 `CopyProtocolExecutor`（`copy.rs:14`）。
- `ParallelShardExecutor`：并行分片执行器，输入：数据+分片键+分片数，输出：`CopyBatchResult`，核心逻辑：按分片键拆分→`tokio::join!` 并行执行→结果合并。依赖 v4.6.0 `BatchTransactionCoordinator`（`atomic.rs:216`）。
- `ShardConfig`：分片配置（分片键 + 分片数 + 并行度 + 原子性级别）。
- `ConflictResolution`：冲突解决策略枚举（Upsert/Ignore/Merge/Replace）。
- `CopyBatchResult`：COPY 批量结果（加载行数 + 分片结果列表 + 冲突解决 + 耗时）。
- `CopyDialect`：COPY 方言枚举（PostgresCopy/MysqlLoadData/OracleSqlLoader/MssqlBulkInsert/MultiValueInsert）。

**REQ-V47-004 异常自愈与根因分析（sz-orm-observability 扩展，`anomaly-remediation-rca` feature）：**
- `AutoRemediator`：异常自愈器，输入：异常+根因，输出：自愈结果，核心逻辑：选择修复动作→白名单判断→自动执行/请求人工确认→记录审计日志。依赖 v4.6.0 `AnomalyDetector`（`anomaly.rs:254`）。
- `RemediationAction`：修复动作枚举（RestartConnection/ClearCache/ScaleOut/CustomAction）。
- `RootCauseAnalyzer`：根因分析器，输入：异常，输出：`RootCause`，核心逻辑：收集上下文（指标+日志+拓扑）→推断根因→构建证据链→计算置信度。依赖既有 `MetricsRegistry`（`lib.rs:262`）/ `QueryLogger`（`query_logger.rs:73`）。
- `RootCause`：根因结果（根因组件 + 根因 SQL + 置信度 + 证据链）。
- `AnomalyCorrelator`：异常关联器，输入：异常+历史，输出：`CorrelationResult`，核心逻辑：分析多个异常事件时间/空间关联性→识别同根因异常集群。依赖 v4.6.0 `AnomalyDetector`。
- `CorrelationResult`：关联结果（关联异常集群 + 关联性评分）。
- `RemediationConfig`：自愈配置（白名单 + 置信度阈值 + 关联窗口 + 超时）。

**REQ-V47-005 多云成本对比与容量预测（sz-orm-storage 扩展，`multicloud-cost-forecast` feature）：**
- `MultiCloudCostComparator`：多云成本对比器，输入：容量+provider 列表，输出：`CostComparisonReport`，核心逻辑：对每 provider 调用 `CostAnalyzer.analyze_cost`→计算差异→生成迁移建议。依赖 v4.6.0 `CostAnalyzer`（`cost.rs:231`）。
- `CostComparisonReport`：成本对比报表（每 provider 成本 + 差异 + 迁移建议）。
- `MigrationSuggestion`：provider 迁移建议（源 provider + 目标 provider + 迁移成本 + 迁移风险 + 预期节省）。
- `CapacityForecaster`：容量预测器，输入：历史数据+算法+预测周期，输出：`CapacityForecast`，核心逻辑：按算法拟合历史趋势→预测未来→计算置信区间。
- `CapacityForecast`：容量预测结果（预测容量 + 置信区间 + 预测算法）。
- `ForecastAlgorithm`：预测算法枚举（LinearRegression/ExponentialSmoothing/HoltWinters）。
- `AutoOptimizer`：自动优化执行器，输入：`CostOptimizationSuggestion`，输出：`OptimizationExecutionResult`，核心逻辑：白名单判断→自动执行/请求人工确认→执行优化。依赖既有 `Storage` trait（`storage.rs:14`）+ v4.6.0 `CostOptimizationSuggestion`（`cost.rs:55`）。
- `OptimizationExecutionResult`：优化执行结果（成功/失败 + 执行详情）。
- `MultiCloudForecastConfig`：多云预测配置（对比周期 + 预测算法 + 预测周期 + 置信水平 + 白名单）。

**REQ-V47-006 租户资源配额与行级安全增强（sz-orm-core 扩展，`tenant-quota-rls-enhanced` feature）：**
- `TenantResourceQuota`：租户资源配额（max_connections + max_qps + max_storage），输入：租户 ID，输出：配额值。
- `QuotaEnforcer`：配额执行器，输入：租户 ID+资源类型+当前值，输出：允许/拒绝，核心逻辑：检查当前值是否超限→超限拒绝+记录审计日志。依赖 v4.6.0 `ConnectionTenantBinder`（`connection_tenant.rs:133`）。
- `QuotaEnforceStrategy`：配额执行策略枚举（FailClose/FailOpen）。
- `RlsPolicyEnhancer`：RLS 策略增强器，输入：查询+租户 ID，输出：增强查询，核心逻辑：匹配增强 RLS 策略→注入多条件 WHERE（参数化）→与列级脱敏联动。依赖既有 `RowLevelSecurityPolicy`（`tenant_security.rs:67`）/ `ColumnMaskingRule`（`:155`）。
- `EnhancedRlsPolicy`：增强行级安全策略（多条件组合 + 复杂谓词 + 列级脱敏联动）。
- `TenantAuditLogger`：租户级审计日志器，输入：租户 ID+操作+表+结果，输出：审计日志条目，核心逻辑：按租户独立记录→追加写入不可篡改。依赖既有 `TenantAuditContext`（`tenant_security.rs:244`）/ `TenantAuditOperation`（`:197`）/ `AuditResult`（`:224`）。
- `TenantAuditEntry`：审计条目（租户 ID + 操作 + 表 + 时间 + 结果 + 详情）。
- `AuditLogLevel`：审计日志级别枚举（Full/Summary/Off）。
- `TenantQuotaRlsConfig`：配额与 RLS 配置（执行策略 + RLS 增强开关 + 审计级别 + 配额检查开关）。

**REQ-V47-007 缓存预热与穿透防护（sz-orm-core 扩展，`cache-warmup-protection` feature）：**
- `CacheWarmer`：缓存预热器，输入：`WarmupConfig`，输出：预热结果，核心逻辑：按策略识别热点数据→异步查询 DB→批量加载到 L1+L2→记录预热日志。依赖 v4.6.0 `ProcessL1Cache`（`process_l1_cache.rs:169`）/ 既有 `L2Cache`（`l2_cache.rs:517`）。
- `WarmupConfig`：预热配置（策略 + 批次大小 + 热点表/键列表）。
- `WarmupStrategy`：预热策略枚举（HotspotTable/HotspotKey/CustomQuery/Disabled）。
- `BloomFilter`：布隆过滤器，输入：key，输出：可能存在/不存在，核心逻辑：多哈希函数→位数组查询→不漏判（不存在一定返回 false）。无既有依赖（纯算法实现）。
- `PenetrationGuard`：穿透防护器，输入：key，输出：Value/None，核心逻辑：`BloomFilter.might_contain`→不存在返回 None→可能存在查 L1/L2/DB。依赖 `BloomFilter` + 既有 L1/L2。
- `SingleFlight`：击穿防护器，输入：key+重建函数，输出：Value，核心逻辑：同一 key 并发重建只执行一次→其他等待复用→超时释放锁。依赖 `tokio::sync` 同步原语。
- `StampedeGuard`：击穿防护配置（超时 + 并发限制）。
- `WarmupProtectionConfig`：预热与防护配置（预热策略 + 批次大小 + 布隆容量/误判率 + singleflight 超时 + 防护开关）。

## 1.2 存量功能详细分析

本节对 §1.1.1 已实现功能中作为扩展基线的关键存量功能进行深入分析，识别其接口契约、业务规则、扩展点与约束。

### 1.2.1 MessageQueue trait（REQ-V47-001 扩展基线）

- **接口契约**：`packages/sz-orm-queue/src/queue.rs:18` `pub trait MessageQueue: Send + Sync`，含 `publish`/`consume`/`ack`/`nack`/`reject`/`subscribe` 异步方法，入参为 `Message`（`:57`），出参为 `Result`，异常为队列错误，副作用为消息存储/投递。
- **业务规则**：`publish` 立即投递消息，`consume` 按 FIFO 消费，`nack` 消息重试计数 +1（`Message.retry_count` `:67`），达到 `max_retries` 进死信队列，`reject` 直接进死信队列。
- **扩展点**：trait 方法可被具体实现覆盖（`InMemoryQueue` `:339` 是内存实现），v4.6.0 `RedeliveryScheduler`（`dlx.rs:216`）在 trait 之上扩展自动重投递。
- **约束**：异步 trait（`async-trait`），实现须 `Send + Sync`（线程安全），消息不可丢失（持久化由实现保证）。
- **v4.7.0 扩展策略**：不修改 trait 本身，新增 `DelayScheduler` 在 `publish` 之前插入延迟/优先级逻辑，到期后调用既有 `publish` 投递。

### 1.2.2 RedeliveryScheduler（REQ-V47-001 扩展基线，v4.6.0）

- **接口契约**：`packages/sz-orm-queue/src/dlx.rs:216` `pub struct RedeliveryScheduler`，含调度循环（`tokio::spawn` 周期检查死信队列），按 `BackoffPolicy`（`:47`）退避策略计算重投递间隔，`DlxRoutingStrategy`（`:83`）控制路由。
- **业务规则**：死信消息按退避策略延迟后重投递到原队列/DLX topic/DLX queue，重投递次数上限保护，复用 `message_tracing` 模块记录日志。
- **扩展点**：调度循环可复用（周期检查 + 到期处理），退避策略计算可复用。
- **约束**：调度循环须优雅关闭（`tokio::CancellationToken`），退避间隔单调递增，线程安全。
- **v4.7.0 扩展策略**：复用调度循环基线实现 `DelayScheduler` 的到期检查，但不修改 `RedeliveryScheduler` 本身（新增独立调度器，共享调度循环模式）。

### 1.2.3 DryRunMigration + ImpactReport（REQ-V47-002 扩展基线）

- **接口契约**：`packages/sz-orm-core/src/migration_dry_run.rs:11` `pub struct DryRunMigration`，`analyze_impact` 方法输入迁移，输出 `ImpactReport`（`:80`，含 `MigrationImpact` `:59` 列表，每项含 `DdlType` `:33` + `LockType` `:48`）。
- **业务规则**：解析迁移 SQL 识别 DDL 类型（Create/Drop/Alter）与锁类型（AccessExclusive/ShareRowExclusive），评估影响范围，不执行 SQL。
- **扩展点**：`analyze_impact` 返回的 `ImpactReport` 可作为前向兼容性检查的输入，`DdlType` 可识别破坏性变更（DropColumn/AlterColumnType）。
- **约束**：不执行 SQL（仅语法/影响分析），须参数化解析（不拼接 SQL）。
- **v4.7.0 扩展策略**：`ForwardCompatChecker` 调用 `DryRunMigration.analyze_impact` 获取 `ImpactReport`，基于 `DdlType` 识别破坏性变更，不修改既有 dry-run 逻辑。

### 1.2.4 RollbackExecutor（REQ-V47-002 扩展基线，v4.6.0）

- **接口契约**：`packages/sz-orm-core/src/rollback_zero_downtime.rs:305` `pub struct RollbackExecutor`，含影子表管理能力（`ZeroDowntimeRollbackStrategy::ShadowTable` `:73`），`RollbackPlan`（`:157`）定义回滚步骤。
- **业务规则**：ShadowTable 策略在影子表上执行回滚验证，ReverseMigration 执行反向迁移，BlueGreen 蓝绿切换。
- **扩展点**：影子表创建/清理能力可复用于沙箱预演，`ZeroDowntimeRollbackConfig`（`:84`）配置可复用。
- **约束**：影子表须清理不残留，回滚窗口超时保护，参数化执行。
- **v4.7.0 扩展策略**：`SandboxDryRunner` 复用 `RollbackExecutor` 的影子表创建/清理能力，在影子表上预执行迁移 SQL，不修改既有回滚逻辑。

### 1.2.5 CopyProtocolExecutor + BatchTransactionCoordinator（REQ-V47-003 扩展基线）

- **接口契约**：`packages/sz-orm-batch/src/copy.rs:14` `pub struct CopyProtocolExecutor`（PostgreSQL COPY 协议执行器），`packages/sz-orm-batch/src/atomic.rs:216` `pub struct BatchTransactionCoordinator`（批量事务协调器），`AtomicityGuarantee`（`:20`）三级别。
- **业务规则**：`CopyProtocolExecutor` 通过 PostgreSQL COPY 协议（`COPY table FROM STDIN`）高速批量加载，跳过 SQL 解析；`BatchTransactionCoordinator` 按 `AtomicityGuarantee`（AllOrNothing 2PC / BestEffort / SagaCompensation）协调跨批次原子提交，`SagaCompensator`（`:436`）失败时补偿回滚。
- **扩展点**：`CopyProtocolExecutor` 的 COPY 实现可复用于 PG 方言，`BatchTransactionCoordinator` 的原子性协调可复用于并行分片，`AtomicityGuarantee` 可直接复用。
- **约束**：COPY 协议仅 PostgreSQL 系原生支持，原子性须保证不产生部分提交脏数据，`SagaCompensator` 须幂等。
- **v4.7.0 扩展策略**：`CopyProtocolAdapter` 复用 `CopyProtocolExecutor` 的 PG COPY 实现，新增其他方言适配；`ParallelShardExecutor` 复用 `BatchTransactionCoordinator` 保证分片间原子性，不修改既有逻辑。

### 1.2.6 AnomalyDetector + MetricsRegistry + QueryLogger（REQ-V47-004 扩展基线）

- **接口契约**：`packages/sz-orm-observability/src/anomaly.rs:254` `pub struct AnomalyDetector`，`detect` 方法输入指标历史，输出 `AnomalyAlert`（`:206`），`AnomalyAlgorithm`（`:40`）五种算法。`packages/sz-orm-observability/src/lib.rs:262` `pub struct MetricsRegistry`（指标注册表），`packages/sz-orm-observability/src/query_logger.rs:73` `pub struct QueryLogger`（查询日志器）。
- **业务规则**：`AnomalyDetector` 按 `AnomalyAlgorithm`（Threshold/Trend/Statistical/ZScore/IQR）分析指标历史，检测到异常触发 `AnomalyAlert`，`MetricsRegistry` 提供指标历史数据，`QueryLogger` 记录查询日志。
- **扩展点**：`AnomalyDetector.detect` 可作为异常自愈的触发源，`MetricsRegistry` 指标数据可作为根因分析上下文，`QueryLogger` 日志可作为根因分析证据链。
- **约束**：异常检测须低开销（不显著影响性能），`MetricsRegistry` 线程安全，`QueryLogger` 异步写入。
- **v4.7.0 扩展策略**：`AutoRemediator` 订阅 `AnomalyDetector` 的异常事件，`RootCauseAnalyzer` 收集 `MetricsRegistry` 指标 + `QueryLogger` 日志作为证据链，不修改既有异常检测逻辑。

### 1.2.7 CostAnalyzer + Storage + StorageProvider（REQ-V47-005 扩展基线）

- **接口契约**：`packages/sz-orm-storage/src/cost.rs:231` `pub struct CostAnalyzer`，`analyze_cost` 方法输入 provider，输出 `CostReport`（`:213`，含 `ProviderCost` `:202` 列表 + `BucketCost` `:181` 列表），`suggest_optimization`（`:293`）输出 `CostOptimizationSuggestion`（`:55`）。`packages/sz-orm-storage/src/storage.rs:14` `pub trait Storage`，`StorageProvider`（`:287`）7 provider 枚举。
- **业务规则**：`CostAnalyzer` 按 provider/bucket/tier 统计成本（容量+请求+流量），生成 `CostReport` + `CostOptimizationSuggestion`（TierDowngrade/LifecycleOptimize/DeleteExpired/CompressCold），`Storage` trait 抽象存储操作，`StorageProvider` 枚举 7 provider。
- **扩展点**：`CostAnalyzer.analyze_cost` 可复用于多云成本对比（对每 provider 调用），`CostOptimizationSuggestion` 可作为自动优化的输入，`Storage` trait 可执行优化动作（如降级 tier）。
- **约束**：成本数据须使用 provider API 返回的计费数据（非估算），`Storage` trait 须 `Send + Sync`。
- **v4.7.0 扩展策略**：`MultiCloudCostComparator` 对每 provider 调用 `CostAnalyzer.analyze_cost`，`AutoOptimizer` 接收 `CostOptimizationSuggestion` 通过 `Storage` trait 执行优化，不修改既有成本分析逻辑。

### 1.2.8 ConnectionTenantBinder + RowLevelSecurityPolicy + Pool（REQ-V47-006 扩展基线）

- **接口契约**：`packages/sz-orm-core/src/connection_tenant.rs:133` `pub struct ConnectionTenantBinder`，`acquire_with_tenant` 方法输入租户 ID，输出 `TenantConnectionGuard`（`:249`），`ConnectionLevelIsolation`（`:24`）三级别，`ConnectionAffinityPolicy`（`:35`）三策略。`packages/sz-orm-core/src/tenant_security.rs:67` `pub struct RowLevelSecurityPolicy`，`ColumnMaskingRule`（`:155`），`TenantAuditContext`（`:244`）/ `TenantAuditOperation`（`:197`）/ `AuditResult`（`:224`）。`packages/sz-orm-core/src/pool.rs:743` `pub struct Pool`，`Connection` trait（`:45`），`execute_with_params`（`:82`）。
- **业务规则**：`ConnectionTenantBinder` 在同一连接池中连接绑定到特定租户（`SET app.tenant_id = ?` 参数化），`TenantConnectionGuard` RAII 守卫归还时清理租户上下文，`RowLevelSecurityPolicy` 自动注入租户过滤 WHERE，`ColumnMaskingRule` 列级脱敏，`Pool` 自研连接池（AtomicU32 + crossbeam-queue ArrayQueue + Notify）。
- **扩展点**：`acquire_with_tenant` 路径可插入配额检查，`RowLevelSecurityPolicy` 的 WHERE 注入可增强为多条件组合，`TenantAuditContext` 可复用于租户级审计日志，`Pool` 的连接获取可限流。
- **约束**：连接绑定的 tenant_id 不可被篡改（由可信路径设置），WHERE 注入须参数化绑定，`Pool` 线程安全（无锁队列），配额检查须在连接池/查询层强制不可被应用层绕过。
- **v4.7.0 扩展策略**：`QuotaEnforcer` 在 `acquire_with_tenant` 路径上插入配额检查，`RlsPolicyEnhancer` 增强 `RowLevelSecurityPolicy` 的 WHERE 注入为多条件组合，`TenantAuditLogger` 复用 `TenantAuditContext` 记录审计日志，不修改既有连接级隔离/RLS 逻辑。

### 1.2.9 ProcessL1Cache + L2Cache + CacheCoherenceProtocol（REQ-V47-007 扩展基线）

- **接口契约**：`packages/sz-orm-core/src/process_l1_cache.rs:169` `pub struct ProcessL1Cache<T>`（跨 Session 共享 Identity Map，`RwLock` 保护），`CrossSessionIdentityMap<T>`（`:366`），`tenant_cache_key`（`:421`）租户缓存键生成。`packages/sz-orm-core/src/l2_cache.rs:517` `pub struct L2Cache`，`CacheKey`（`:143`）。`packages/sz-orm-core/src/cache_coherence.rs:103` `pub struct CacheCoherenceProtocol`，`MesiState`（`:12`）。
- **业务规则**：`ProcessL1Cache` 跨 Session 共享 Identity Map（线程安全 `Send + Sync`），L1→L2→DB 查询协作，LRU 淘汰 + TTL 过期，`tenant_cache_key` 按租户隔离缓存键，`L2Cache` 二级缓存，`CacheCoherenceProtocol` MESI 一致性协议（Modified/Exclusive/Shared/Invalid）。
- **扩展点**：`ProcessL1Cache.put`/`get` 可复用于缓存预热加载与查询，`L2Cache.put`/`get` 可协同预热，`CacheCoherenceProtocol` 可同步预热数据与 DB 不一致，`tenant_cache_key` 可复用于布隆过滤器按租户隔离。
- **约束**：`ProcessL1Cache` 须 `Send + Sync`（跨线程共享无数据竞争），`RwLock` 保护内部状态，`CacheCoherenceProtocol` 须保证一致性。
- **v4.7.0 扩展策略**：`CacheWarmer` 调用 `ProcessL1Cache.put` + `L2Cache.put` 预加载热点数据，`PenetrationGuard` 在 `ProcessL1Cache.get` 之前插入布隆过滤器判断，`SingleFlight` 包装 `ProcessL1Cache` 的重建逻辑，不修改既有 L1/L2 逻辑。

---

# 二、增量设计方案

## 2.1 实现模型

### 2.1.1 上下文视图

```plantuml
@startuml
title sz-orm v4.7.0 七项能力 上下文视图（部署图）

actor "应用开发者" as appdev
actor "运维/SRE" as sre
actor "FinOps" as finops

node "sz-orm v4.7.0 工作空间" {
  component "sz-orm-queue\n[delayed-priority-queue]" as queue {
    component "DelayScheduler" as ds
    component "PriorityQueue" as pq
    component "ScheduledMessage" as sm
  }
  component "sz-orm-core\n[forward-compat-sandbox]\n[tenant-quota-rls-enhanced]\n[cache-warmup-protection]" as core {
    component "ForwardCompatChecker" as fcc
    component "SandboxDryRunner" as sdr
    component "DependencyAnalyzer" as da
    component "QuotaEnforcer" as qe
    component "RlsPolicyEnhancer" as rls
    component "TenantAuditLogger" as tal
    component "CacheWarmer" as cw
    component "PenetrationGuard" as pg
    component "SingleFlight" as sf
  }
  component "sz-orm-batch\n[copy-parallel-shard]" as batch {
    component "CopyProtocolAdapter" as cpa
    component "ParallelShardExecutor" as pse
  }
  component "sz-orm-observability\n[anomaly-remediation-rca]" as obs {
    component "AutoRemediator" as ar
    component "RootCauseAnalyzer" as rca
    component "AnomalyCorrelator" as ac
  }
  component "sz-orm-storage\n[multicloud-cost-forecast]" as storage {
    component "MultiCloudCostComparator" as mcc
    component "CapacityForecaster" as cf
    component "AutoOptimizer" as ao
  }
}

database "MySQL/PG/SQLite\n/Oracle/MSSQL" as db
queue "消息队列 provider\n(6 个)" as mq
queue "对象存储 provider\n(7 个)" as obj

appdev --> ds : publish_delayed/publish_scheduled
appdev --> fcc : check_compatibility
appdev --> sdr : dry_run_sandbox
appdev --> cpa : execute_copy
appdev --> pse : execute_copy_shards
appdev --> rls : enhance_query
appdev --> pg : get
sre --> ar : 配置白名单
sre --> qe : 配置配额
sre --> cw : 配置预热
finops --> mcc : compare_providers
finops --> cf : forecast
finops --> ao : execute_suggestion

ds --> mq : 复用既有 MessageQueue
fcc --> db : 复用 DryRunMigration
sdr --> db : 影子表(参数化)
cpa --> db : COPY/LOAD DATA
pse --> db : 分片并行
qe --> db : 配额检查
rls --> db : 注入 WHERE(参数化)
cw --> db : 预加载热点
pg --> db : 布隆过滤后查 DB
ar --> db : 自愈动作
rca --> db : 根因定位
mcc --> obj : 复用 CostAnalyzer
cf --> obj : 历史数据
ao --> obj : 执行优化

@enduml
```

### 2.1.2 服务/组件总体架构

```plantuml
@startuml
title sz-orm v4.7.0 组件总体架构

package "sz-orm-queue (扩展)" as queue_pkg {
  [DelayScheduler] as ds
  [PriorityQueue] as pq
  [DelayedMessage] as dm
  [ScheduledMessage] as sm
  [PriorityPolicy] as pp
}

package "sz-orm-core (扩展)" as core_pkg {
  package "迁移兼容沙箱" {
    [ForwardCompatChecker] as fcc
    [SandboxDryRunner] as sdr
    [DependencyAnalyzer] as da
    [MigrationDependencyGraph] as mdg
  }
  package "租户配额RLS" {
    [QuotaEnforcer] as qe
    [RlsPolicyEnhancer] as rls
    [TenantAuditLogger] as tal
    [TenantResourceQuota] as trq
  }
  package "缓存预热防护" {
    [CacheWarmer] as cw
    [BloomFilter] as bf
    [PenetrationGuard] as pg
    [SingleFlight] as sf
  }
}

package "sz-orm-batch (扩展)" as batch_pkg {
  [CopyProtocolAdapter] as cpa
  [ParallelShardExecutor] as pse
  [ConflictResolution] as cr
}

package "sz-orm-observability (扩展)" as obs_pkg {
  [AutoRemediator] as ar
  [RootCauseAnalyzer] as rca
  [AnomalyCorrelator] as ac
}

package "sz-orm-storage (扩展)" as storage_pkg {
  [MultiCloudCostComparator] as mcc
  [CapacityForecaster] as cf
  [AutoOptimizer] as ao
}

package "既有基线 (只读复用)" as baseline {
  [MessageQueue] as mq_base
  [RedeliveryScheduler] as rs_base
  [DryRunMigration] as dr_base
  [RollbackExecutor] as rb_base
  [CopyProtocolExecutor] as cp_base
  [BatchTransactionCoordinator] as btc_base
  [AnomalyDetector] as ad_base
  [CostAnalyzer] as ca_base
  [ConnectionTenantBinder] as ctb_base
  [RowLevelSecurityPolicy] as rls_base
  [ProcessL1Cache] as pl1_base
  [L2Cache] as l2_base
  [Pool] as pool_base
}

ds --> mq_base : 到期后 publish
ds --> rs_base : 复用调度循环
pq --> mq_base : dequeue 投递
fcc --> dr_base : analyze_impact
sdr --> rb_base : 影子表管理
sdr --> pool_base : execute_with_params
cpa --> cp_base : PG COPY 实现
pse --> btc_base : 原子性协调
ar --> ad_base : 异常事件订阅
rca --> ad_base : 异常上下文
ac --> ad_base : 异常历史
mcc --> ca_base : analyze_cost
ao --> ca_base : suggest_optimization
qe --> ctb_base : acquire_with_tenant
rls --> rls_base : WHERE 注入增强
tal --> ctb_base : 审计上下文
cw --> pl1_base : put 预加载
cw --> l2_base : put 预加载
pg --> pl1_base : get 查询
pg --> l2_base : get 查询
sf --> pl1_base : 重建逻辑

note bottom of baseline
  既有基线全部保留不动
  v4.7.0 只读复用，不修改
  通过 feature gate 隔离新能力
end note

@enduml
```

### 2.1.3 实现设计文档

本节对七项需求的核心逻辑进行实现设计，包含状态机/流程分支/扩展点/事务设计。

#### 2.1.3.1 REQ-V47-001 延迟消息状态机与调度流程

**延迟消息状态机：**

```plantuml
@startuml
title 延迟消息状态机
state "Created\n(已创建)" as created
state "Waiting\n(等待到期)" as waiting
state "Ready\n(到期可投递)" as ready
state "Enqueued\n(入优先级队列)" as enqueued
state "Delivered\n(已投递)" as delivered
state "Failed\n(投递失败)" as failed
state "DeadLetter\n(死信)" as dead

[*] --> created : publish_delayed()
created --> waiting : 存储(按 deliver_at 排序)
waiting --> ready : now >= deliver_at
ready --> enqueued : 加入 PriorityQueue
enqueued --> delivered : 投递成功
enqueued --> failed : 投递失败
failed --> waiting : 按退避策略重试\n(复用 BackoffPolicy)
failed --> dead : 重试超限

note right of waiting
  到期前不可消费
  按 deliver_at 排序存储
end note

note right of enqueued
  按优先级排序
  aging 机制提升等待过久
  的低优先级消息
end note

@enduml
```

**调度流程分支设计：**
- **分支 1（到期检查）**：调度周期（默认 100ms）触发，检查 `Waiting` 状态消息是否 `now >= deliver_at`，到期则转为 `Ready`。
- **分支 2（优先级投递）**：`Ready` 消息加入 `PriorityQueue`，按 `PriorityPolicy` 排序，`Strict` 按 `priority` 严格排序（`BinaryHeap`），`Weighted` 按权重比例随机选择，`FairShare` 按租户/类别公平分配。
- **分支 3（aging 机制）**：`Strict` 策略下，检查 `Enqueued` 状态低优先级消息等待时间，超过 `aging_threshold_ms`（默认 5 分钟）则提升优先级。
- **分支 4（定时调度）**：`ScheduledMessage` 按 Cron 表达式计算下次投递时间，到点后创建消息加入 `PriorityQueue`。
- **分支 5（投递失败）**：投递失败按 v4.6.0 `BackoffPolicy`（`dlx.rs:47`）退避策略重试，重试超限进死信队列（复用既有 `requeue_dead_letter` `queue.rs:484`）。

**扩展点设计：** `PriorityPolicy` 为枚举，应用层可实现自定义策略（`FairShare` 按租户公平分配需应用层提供租户权重配置）。

**事务设计：** 延迟消息存储须持久化（由 `MessageQueue` 实现保证），投递成功后 ACK，失败按退避重试，不丢失消息。

#### 2.1.3.2 REQ-V47-002 沙箱预演流程与依赖图拓扑排序

**沙箱预演流程分支设计：**
- **分支 1（影子表创建）**：`SandboxDryRunner` 创建影子表（`shadow_` 前缀 + 原表名），复制原表 schema。失败则中止预演，返回错误"sandbox table creation failed"，不修改真实数据。
- **分支 2（迁移 SQL 执行）**：在影子表上执行迁移 SQL（参数化绑定，复用 `Connection::execute_with_params` `pool.rs:82`），将 SQL 中的原表名替换为影子表名。失败则中止预演，清理影子表，返回错误。
- **分支 3（数据完整性校验）**：校验影子表数据与原表数据一致性（行数 + 校验和 + 抽样比对），`SandboxVerifyItem::DataIntegrity`。
- **分支 4（查询兼容性校验）**：在影子表上执行旧应用查询，校验可执行性（不报错 + 结果结构兼容），`SandboxVerifyItem::QueryCompat`。
- **分支 5（性能影响校验）**：在影子表上执行代表性查询，测量执行时间，与原表对比，`SandboxVerifyItem::PerformanceImpact`。
- **分支 6（清理）**：预演完成后（无论成功失败）清理影子表，不残留。

**迁移依赖图拓扑排序（Kahn 算法）：**
- `DependencyAnalyzer` 构建有向图（节点=迁移，边=依赖关系），Kahn 算法拓扑排序确定执行顺序。
- **循环检测**：拓扑排序后若剩余节点非空，则存在循环依赖，标注循环并返回错误"circular dependency detected"。

**事务设计：** 沙箱预演在独立事务中执行，预演后回滚（影子表清理），不修改真实数据。

#### 2.1.3.3 REQ-V47-003 并行分片执行流程与原子性

**并行分片执行流程分支设计：**
- **分支 1（分片拆分）**：`ParallelShardExecutor` 按分片键（hash 或 range）将数据拆分为 N 分片，记录分片数据量供负载均衡分析。
- **分支 2（并行执行）**：`tokio::join!` 并行执行 N 分片，每分片调用 `CopyProtocolAdapter.execute_copy` 按方言加载。并行度限制到连接池容量（`Pool` `pool.rs:743`），多余分片排队等待。
- **分支 3（冲突解决）**：每分片按 `ConflictResolution` 处理冲突（Upsert: `ON CONFLICT DO UPDATE` / Ignore: `ON CONFLICT DO NOTHING` / Replace: `REPLACE INTO` / Merge: 自定义合并函数）。
- **分支 4（原子性处理）**：全部分片成功则提交（复用 `BatchTransactionCoordinator.commit` `atomic.rs:216`）；某分片失败则按 `AtomicityGuarantee`（`atomic.rs:20`）处理：`AllOrNothing` 全部回滚，`BestEffort` 标记失败分片，`SagaCompensation` 调用 `SagaCompensator`（`:436`）补偿回滚已成功分片。

**COPY 方言适配分支：**
- PostgreSQL/GaussDB/Kingbase/PolarDB：`COPY table FROM STDIN`（复用既有 `CopyProtocolExecutor` `copy.rs:14`）。
- MySQL：`LOAD DATA INFILE`。
- Oracle：`SQL*Loader`（生成控制文件 + 数据文件）。
- MSSQL：`BULK INSERT`。
- SQLite：不支持 COPY，降级为 multi-value INSERT，标注"fallback to multi-value INSERT"。

**事务设计：** 并行分片原子性由 `BatchTransactionCoordinator` 保证，`AllOrNothing` 模式 2PC，`SagaCompensation` 模式补偿回滚。

#### 2.1.3.4 REQ-V47-004 异常自愈流程与根因推断

**异常自愈流程分支设计：**
- **分支 1（异常检测）**：复用 v4.6.0 `AnomalyDetector.detect`（`anomaly.rs:254`）检测异常，触发 `AnomalyAlert`（`:206`）。
- **分支 2（根因分析）**：`RootCauseAnalyzer` 收集异常上下文（`MetricsRegistry` `lib.rs:262` 指标 + `QueryLogger` `query_logger.rs:73` 日志 + 拓扑），推断根因组件/SQL，计算置信度，构建证据链，生成 `RootCause`。置信度低于阈值（默认 0.7）标注"根因不确定，需人工排查"。
- **分支 3（异常关联）**：`AnomalyCorrelator` 分析当前异常与历史异常的时间/空间关联性（时间窗口默认 5 分钟），识别同根因异常集群，生成 `CorrelationResult`。关联性评分低标注"weak correlation"。
- **分支 4（修复动作选择）**：`AutoRemediator` 根据异常类型 + 根因选择 `RemediationAction`（RestartConnection/ClearCache/ScaleOut/CustomAction）。
- **分支 5（白名单判断）**：动作在 `auto_execute_whitelist` 内则自动执行；非白名单则请求人工确认（通知 SRE），不静默执行。
- **分支 6（执行 + 审计）**：执行修复动作，记录审计日志（异常 ID + 动作 + 执行人 + 时间 + 结果），追加写入不可篡改。执行失败通知 SRE 人工干预。

**扩展点设计：** `RemediationAction::CustomAction` 由应用层注册自定义自愈函数，`RootCauseAnalyzer` 的推断算法可扩展。

#### 2.1.3.5 REQ-V47-005 容量预测算法与成本对比流程

**容量预测算法分支设计：**
- **LinearRegression（线性回归）**：最小二乘法拟合 `y = ax + b`，预测未来容量。
- **ExponentialSmoothing（指数平滑）**：`S_t = α * y_t + (1-α) * S_{t-1}`，指数衰减权重平滑。
- **HoltWinters（Holt-Winters）**：三指数平滑（水平 + 趋势 + 季节性），支持周期性数据。
- **置信区间计算**：基于残差标准差 + 置信水平（默认 95%，正态分布 z=1.96）计算上下界。

**多云成本对比流程：**
- 对每 provider 调用 `CostAnalyzer.analyze_cost`（`cost.rs:231`）获取 `ProviderCost`（`:202`），provider API 不可用则跳过并记录日志。
- 计算同容量在不同 provider 的成本差异，生成 `CostComparisonReport`。
- 基于成本差异生成 `MigrationSuggestion`（源 provider + 目标 provider + 迁移成本 + 迁移风险 + 预期节省）。

**自动优化执行流程：**
- `AutoOptimizer` 接收 `CostOptimizationSuggestion`（`cost.rs:55`），白名单内自动执行（如 `TierDowngrade` 通过 `Storage` trait `storage.rs:14` 降级 tier），非白名单请求人工确认。
- 执行失败通知 FinOps 人工干预，生成 `OptimizationExecutionResult`。

#### 2.1.3.6 REQ-V47-006 配额检查与 RLS 注入流程

**配额检查流程分支设计：**
- **分支 1（连接获取配额检查）**：`QuotaEnforcer` 在 `ConnectionTenantBinder.acquire_with_tenant`（`connection_tenant.rs:133`）路径上检查 `max_connections`，当前连接数 >= 配额则拒绝，返回错误"quota exceeded: max_connections"。
- **分支 2（查询执行配额检查）**：查询执行前检查 `max_qps`，当前 QPS >= 配额则拒绝，返回错误"quota exceeded: max_qps"。
- **分支 3（存储配额检查）**：写入操作前检查 `max_storage`，当前存储 >= 配额则拒绝。
- **分支 4（配额检查失败处理）**：配额存储不可用时按 `QuotaEnforceStrategy` 处理（`FailClose` 拒绝请求 / `FailOpen` 放行），记录日志。
- **分支 5（审计日志）**：配额超限记录审计日志（租户 ID + 配额类型 + 当前值 + 限制值 + 操作 + 时间），`TenantAuditLogger` 追加写入不可篡改。

**RLS 增强注入流程：**
- `RlsPolicyEnhancer` 匹配 `EnhancedRlsPolicy`（多条件组合 + 复杂谓词），生成 WHERE 条件（`tenant_id = ? AND dept_id IN (?,?)`），参数化绑定（复用 `Connection::execute_with_params` `pool.rs:82`），tenant_id 由可信路径设置（`TenantContext` `tenant_context.rs:80`）不可被客户端篡改。
- 多个 RLS 策略匹配同一查询时按优先级选择最高优先级，记录冲突日志。
- 与 `ColumnMaskingRule`（`tenant_security.rs:155`）联动，列级脱敏在 RLS 注入后应用。

#### 2.1.3.7 REQ-V47-007 缓存预热与防护流程

**缓存预热流程分支设计：**
- **分支 1（热点识别）**：`CacheWarmer` 按 `WarmupStrategy` 识别热点数据（`HotspotTable` 按配置表预加载 / `HotspotKey` 按配置主键列表 / `CustomQuery` 按自定义 SQL）。
- **分支 2（异步预加载）**：`tokio::spawn` 异步查询 DB 热点数据，批量加载到 L1（`ProcessL1Cache.put` `process_l1_cache.rs:169`）+ L2（`L2Cache.put` `l2_cache.rs:517`），同时加入 `BloomFilter`。不阻塞服务启动。
- **分支 3（预热失败）**：预热失败（DB 查询失败/热点识别失败）记录日志，不影响服务启动，启动后按需加载。
- **分支 4（一致性同步）**：预热期间 DB 数据变更时通过 `CacheCoherenceProtocol`（`cache_coherence.rs:103`）同步失效。

**穿透防护流程：**
- `PenetrationGuard.get(key)` 先调用 `BloomFilter.might_contain(key)`，返回 false 则直接返回 None 不查 DB（不存在的 key 一定返回 None，不漏判）。
- 返回 true（可能存在）则查 L1（`ProcessL1Cache.get`）→ L2（`L2Cache.get`）→ DB，DB 不存在则更新 `BloomFilter`（标记 key 不存在），误判回退 DB 查询不返回错误。

**击穿防护流程：**
- `SingleFlight.get_or_rebuild(key, rebuild_fn)` 对同一 key 的并发请求，首个请求执行 `rebuild_fn`（查 DB 重建缓存），其他请求等待结果复用（`tokio::sync::Notify` 或 `OnceCell`）。
- 重建超时（默认 5 秒）释放锁，等待请求重试，不死锁不长期阻塞。

## 2.2 接口设计

### 2.2.1 总体设计

**接口分类依据：** 按需求项分为 7 组接口，每组对应一个 feature gate，接口稳定性等级均为"稳定"（v4.7.0 首次引入，通过 feature gate 隔离，后续版本保持向后兼容）。

**接口变更策略：** 新增接口通过 feature gate 隔离，默认关闭，既有接口完全不变。后续版本修改须保持向后兼容（新增方法而非修改签名）。

| 接口组 | 所属包 | feature gate | 核心接口 | 稳定性 |
|--------|--------|-------------|---------|--------|
| 延迟优先级队列 | sz-orm-queue | `delayed-priority-queue` | `DelayScheduler`/`PriorityQueue` | 稳定 |
| 前向兼容沙箱 | sz-orm-core | `forward-compat-sandbox` | `ForwardCompatChecker`/`SandboxDryRunner`/`DependencyAnalyzer` | 稳定 |
| COPY 并行分片 | sz-orm-batch | `copy-parallel-shard` | `CopyProtocolAdapter`/`ParallelShardExecutor` | 稳定 |
| 异常自愈 RCA | sz-orm-observability | `anomaly-remediation-rca` | `AutoRemediator`/`RootCauseAnalyzer`/`AnomalyCorrelator` | 稳定 |
| 多云成本预测 | sz-orm-storage | `multicloud-cost-forecast` | `MultiCloudCostComparator`/`CapacityForecaster`/`AutoOptimizer` | 稳定 |
| 租户配额 RLS | sz-orm-core | `tenant-quota-rls-enhanced` | `QuotaEnforcer`/`RlsPolicyEnhancer`/`TenantAuditLogger` | 稳定 |
| 缓存预热防护 | sz-orm-core | `cache-warmup-protection` | `CacheWarmer`/`PenetrationGuard`/`SingleFlight` | 稳定 |

### 2.2.2 接口清单

#### 2.2.2.1 REQ-V47-001 延迟优先级队列接口

```rust
// 延迟消息结构（包装既有 Message + deliver_at + priority）
pub struct DelayedMessage {
    pub message: Message,          // 复用既有 Message (queue.rs:57)
    pub deliver_at: DateTime<Utc>, // 投递时间（绝对时间）
    pub priority: i32,             // 优先级（数值越大优先级越高）
}

// 优先级策略枚举
pub enum PriorityPolicy {
    Strict,       // 严格优先级（BinaryHeap 按 priority 排序）
    Weighted,     // 加权优先级（按权重比例投递）
    FairShare,    // 公平份额（按租户/类别公平分配）
}

// 定时消息结构
pub struct ScheduledMessage {
    pub message: Message,
    pub cron: Option<String>,        // Cron 表达式
    pub interval: Option<Duration>,  // 或固定间隔
}

// 延迟调度器
pub struct DelayScheduler {
    // 复用既有 MessageQueue (queue.rs:18) + v4.6.0 RedeliveryScheduler (dlx.rs:216) 调度循环
}

impl DelayScheduler {
    pub async fn publish_delayed(&self, msg: DelayedMessage) -> Result<(), QueueError>;
    pub async fn publish_scheduled(&self, msg: ScheduledMessage) -> Result<(), QueueError>;
    pub fn with_priority_policy(&mut self, policy: PriorityPolicy) -> &mut Self;
    pub fn with_aging(&mut self, enabled: bool, threshold_ms: u64) -> &mut Self;
    pub async fn shutdown(&self) -> Result<(), QueueError>;
}
```

- **业务说明**：`publish_delayed` 发布延迟消息（按 `deliver_at` 投递），`publish_scheduled` 发布定时消息（按 Cron/间隔周期投递）。
- **前置条件**：`delayed-priority-queue` feature 启用，`MessageQueue` 实现已初始化。
- **后置条件**：延迟消息存储在延迟队列，到期后投递到既有队列；定时消息按周期投递。
- **异常映射**：`QueueError::InvalidCron`（Cron 表达式无效）/ `QueueError::CapacityExceeded`（优先级队列容量超限）/ `QueueError::DeliveryFailed`（投递失败，按退避重试）。

#### 2.2.2.2 REQ-V47-002 前向兼容沙箱接口

```rust
// 前向兼容性检查器
pub struct ForwardCompatChecker {
    // 复用既有 DryRunMigration (migration_dry_run.rs:11) + ImpactReport (:80)
}

impl ForwardCompatChecker {
    pub async fn check_compatibility(&self, migration: &Migration) -> Result<CompatCheckResult, MigrationError>;
    pub fn with_strictness(&mut self, strictness: CompatStrictness) -> &mut Self;
    pub fn with_breaking_changes(&mut self, changes: Vec<BreakingChangeType>) -> &mut Self;
}

// 沙箱预演器
pub struct SandboxDryRunner {
    // 复用 v4.6.0 RollbackExecutor (rollback_zero_downtime.rs:305) 影子表能力
}

impl SandboxDryRunner {
    pub async fn dry_run_sandbox(&self, migration: &Migration, shadow_prefix: &str) -> Result<SandboxResult, MigrationError>;
    pub fn with_verify_items(&mut self, items: Vec<SandboxVerifyItem>) -> &mut Self;
}

// 依赖分析器
pub struct DependencyAnalyzer;

impl DependencyAnalyzer {
    pub fn analyze_dependencies(&self, migrations: &[Migration]) -> Result<MigrationDependencyGraph, MigrationError>;
}
```

- **业务说明**：`check_compatibility` 检查迁移前向兼容性，`dry_run_sandbox` 在影子表预演迁移，`analyze_dependencies` 分析迁移依赖图。
- **前置条件**：`forward-compat-sandbox` feature 启用，`Migration` 已加载。
- **后置条件**：兼容性检查生成 `CompatCheckResult`，沙箱预演不修改真实数据，依赖图标注执行顺序与循环。
- **异常映射**：`MigrationError::SandboxTableCreationFailed` / `MigrationError::SandboxSqlFailed` / `MigrationError::CircularDependency` / `MigrationError::CompatRuleError`。

#### 2.2.2.3 REQ-V47-003 COPY 并行分片接口

```rust
// COPY 协议方言适配器
pub struct CopyProtocolAdapter {
    // 复用既有 CopyProtocolExecutor (copy.rs:14) PG COPY 实现
}

impl CopyProtocolAdapter {
    pub async fn execute_copy(&self, table: &str, data: &[Row], dialect: CopyDialect, conflict: ConflictResolution) -> Result<u64, BatchError>;
}

// 并行分片执行器
pub struct ParallelShardExecutor {
    // 复用 v4.6.0 BatchTransactionCoordinator (atomic.rs:216) 原子性
}

impl ParallelShardExecutor {
    pub async fn execute_copy_shards(&self, data: &[Row], config: &ShardConfig) -> Result<CopyBatchResult, BatchError>;
}

pub enum ConflictResolution { Upsert, Ignore, Merge, Replace }
pub enum CopyDialect { PostgresCopy, MysqlLoadData, OracleSqlLoader, MssqlBulkInsert, MultiValueInsert }
```

- **业务说明**：`execute_copy` 按方言 COPY 加载，`execute_copy_shards` 并行分片加载。
- **前置条件**：`copy-parallel-shard` feature 启用，连接池容量 >= 并行度。
- **后置条件**：数据加载到目标表，冲突按策略解决，原子性按级别保证。
- **异常映射**：`BatchError::CopyNotSupported`（方言不支持 COPY，降级）/ `BatchError::ShardImbalanced`（分片倾斜）/ `BatchError::AtomicityViolated`（原子性违反）/ `BatchError::PoolCapacityExceeded`（并行度超限）。

#### 2.2.2.4 REQ-V47-004 异常自愈 RCA 接口

```rust
pub struct AutoRemediator {
    // 复用 v4.6.0 AnomalyDetector (anomaly.rs:254) 异常事件订阅
}

impl AutoRemediator {
    pub async fn select_action(&self, anomaly: &Anomaly, root_cause: &RootCause) -> RemediationAction;
    pub async fn execute_action(&self, action: RemediationAction) -> Result<RemediationResult, RemediationError>;
    pub fn with_whitelist(&mut self, whitelist: Vec<RemediationAction>) -> &mut Self;
}

pub struct RootCauseAnalyzer {
    // 复用既有 MetricsRegistry (lib.rs:262) + QueryLogger (query_logger.rs:73)
}

impl RootCauseAnalyzer {
    pub async fn analyze_root_cause(&self, anomaly: &Anomaly) -> Result<RootCause, RemediationError>;
}

pub struct AnomalyCorrelator;
impl AnomalyCorrelator {
    pub async fn correlate(&self, anomaly: &Anomaly, history: &[Anomaly]) -> Result<CorrelationResult, RemediationError>;
}

pub enum RemediationAction { RestartConnection, ClearCache, ScaleOut, CustomAction(Box<dyn Fn() -> BoxFuture> + Send + Sync>) }
```

- **业务说明**：`select_action` 选择修复动作，`execute_action` 执行（白名单自动/非白名单人工确认），`analyze_root_cause` 根因分析，`correlate` 异常关联。
- **前置条件**：`anomaly-remediation-rca` feature 启用，`AnomalyDetector` 已运行。
- **后置条件**：自愈动作记录审计日志，根因附证据链与置信度。
- **异常映射**：`RemediationError::ActionFailed` / `RemediationError::InsufficientEvidence` / `RemediationError::InvalidWhitelistAction`。

#### 2.2.2.5 REQ-V47-005 多云成本预测接口

```rust
pub struct MultiCloudCostComparator {
    // 复用 v4.6.0 CostAnalyzer (cost.rs:231)
}

impl MultiCloudCostComparator {
    pub async fn compare_providers(&self, capacity: u64, providers: &[StorageProvider]) -> Result<CostComparisonReport, StorageError>;
}

pub struct CapacityForecaster;
impl CapacityForecaster {
    pub async fn forecast(&self, history: &[CapacityPoint], algorithm: ForecastAlgorithm, horizon_days: u32, confidence: f64) -> Result<CapacityForecast, StorageError>;
}

pub struct AutoOptimizer {
    // 复用 v4.6.0 CostOptimizationSuggestion (cost.rs:55) + 既有 Storage (storage.rs:14)
}

impl AutoOptimizer {
    pub async fn execute_suggestion(&self, suggestion: &CostOptimizationSuggestion) -> Result<OptimizationExecutionResult, StorageError>;
    pub fn with_whitelist(&mut self, whitelist: Vec<CostOptimizationSuggestion>) -> &mut Self;
}

pub enum ForecastAlgorithm { LinearRegression, ExponentialSmoothing, HoltWinters }
```

- **业务说明**：`compare_providers` 多云成本对比，`forecast` 容量预测，`execute_suggestion` 自动执行优化。
- **前置条件**：`multicloud-cost-forecast` feature 启用，provider API 可用，历史数据 >= 7 天。
- **后置条件**：对比报表含迁移建议，预测附置信区间，优化执行记录结果。
- **异常映射**：`StorageError::ProviderApiUnavailable` / `StorageError::InsufficientHistoryData` / `StorageError::OptimizationFailed` / `StorageError::HighForecastError`。

#### 2.2.2.6 REQ-V47-006 租户配额 RLS 接口

```rust
pub struct TenantResourceQuota {
    pub max_connections: Option<u32>,
    pub max_qps: Option<u32>,
    pub max_storage: Option<u64>,
}

pub struct QuotaEnforcer {
    // 复用 v4.6.0 ConnectionTenantBinder (connection_tenant.rs:133)
}

impl QuotaEnforcer {
    pub async fn check_quota(&self, tenant_id: &str, resource: QuotaResource, current: u64) -> Result<(), QuotaError>;
    pub fn with_strategy(&mut self, strategy: QuotaEnforceStrategy) -> &mut Self;
}

pub struct RlsPolicyEnhancer {
    // 复用既有 RowLevelSecurityPolicy (tenant_security.rs:67) + ColumnMaskingRule (:155)
}

impl RlsPolicyEnhancer {
    pub fn enhance_query(&self, query: &mut Query, tenant_id: &str) -> Result<(), QuotaError>;
    pub fn with_policy(&mut self, policy: EnhancedRlsPolicy) -> &mut Self;
}

pub struct TenantAuditLogger {
    // 复用既有 TenantAuditContext (tenant_security.rs:244) + TenantAuditOperation (:197) + AuditResult (:224)
}

impl TenantAuditLogger {
    pub async fn log(&self, entry: TenantAuditEntry) -> Result<(), QuotaError>;
}

pub enum QuotaEnforceStrategy { FailClose, FailOpen }
pub enum QuotaResource { Connection, Qps, Storage }
```

- **业务说明**：`check_quota` 配额检查，`enhance_query` RLS 增强（注入 WHERE 参数化），`log` 审计日志。
- **前置条件**：`tenant-quota-rls-enhanced` feature 启用，`ConnectionTenantBinder` 已初始化，tenant_id 由可信路径设置。
- **后置条件**：配额超限拒绝请求，RLS 注入 WHERE 参数化绑定，审计日志追加写入不可篡改。
- **异常映射**：`QuotaError::QuotaExceeded` / `QuotaError::QuotaCheckFailed` / `QuotaError::RlsPolicyConflict` / `QuotaError::AuditLogWriteFailed` / `QuotaError::InvalidQuotaValue`。

#### 2.2.2.7 REQ-V47-007 缓存预热防护接口

```rust
pub struct CacheWarmer {
    // 复用 v4.6.0 ProcessL1Cache (process_l1_cache.rs:169) + 既有 L2Cache (l2_cache.rs:517)
}

impl CacheWarmer {
    pub async fn warmup(&self, config: &WarmupConfig) -> Result<WarmupResult, CacheError>;
}

pub struct BloomFilter;
impl BloomFilter {
    pub fn new(capacity: usize, fpp: f64) -> Self;
    pub fn add(&mut self, key: &str);
    pub fn might_contain(&self, key: &str) -> bool;  // 不漏判：不存在一定返回 false
}

pub struct PenetrationGuard {
    // 包装 BloomFilter + 既有 ProcessL1Cache + L2Cache
}

impl PenetrationGuard {
    pub async fn get(&self, key: &str) -> Option<Value>;  // 不存在返回 None 不查 DB
}

pub struct SingleFlight;
impl SingleFlight {
    pub async fn get_or_rebuild<F, Fut>(&self, key: &str, rebuild: F) -> Result<Value, CacheError>
    where F: FnOnce() -> Fut, Fut: Future<Output = Result<Value, CacheError>>;
}

pub enum WarmupStrategy { HotspotTable(String), HotspotKey(Vec<String>), CustomQuery(String), Disabled }
```

- **业务说明**：`warmup` 异步预热，`PenetrationGuard.get` 穿透防护，`SingleFlight.get_or_rebuild` 击穿防护。
- **前置条件**：`cache-warmup-protection` feature 启用，`ProcessL1Cache`/`L2Cache` 已初始化。
- **后置条件**：预热数据加载到 L1+L2，布隆过滤器不漏判，singleflight 不死锁。
- **异常映射**：`CacheError::WarmupFailed` / `CacheError::BloomFilterCapacityExceeded` / `CacheError::SingleFlightTimeout` / `CacheError::WarmupDataStale`。

## 2.3 数据模型

### 2.3.1 设计目标

- **支持的业务场景**：7 项需求的配置管理 + 运行时状态管理 + 审计日志记录。
- **性能目标**：延迟队列调度开销 <= 1ms/次，优先级排序 O(log n)，配额检查 <= 0.1ms/次，RLS 注入 <= 0.2ms/查询，布隆查询 <= 100ns/次，singleflight <= 1ms/次。
- **容量目标**：优先级队列容量默认 100000，布隆过滤器容量默认 1000000（误判率 1%）。
- **扩展性目标**：`PriorityPolicy`/`ConflictResolution`/`RemediationAction`/`ForecastAlgorithm`/`WarmupStrategy` 均为枚举，支持扩展新策略。
- **与存量数据兼容**：新数据结构通过 feature gate 隔离，默认不启用，既有数据结构不变。

### 2.3.2 模型实现

```plantuml
@startuml
title sz-orm v4.7.0 核心数据模型类图

class DelayedMessage {
  +message : Message
  +deliver_at : DateTime
  +priority : i32
}

class ScheduledMessage {
  +message : Message
  +cron : Option<String>
  +interval : Option<Duration>
}

class PriorityPolicy <<enum>> {
  Strict
  Weighted
  FairShare
}

class DelayScheduler {
  -delayed_messages : BTreeMap<DateTime, Vec<DelayedMessage>>
  -priority_queue : PriorityQueue
  -scheduled : Vec<ScheduledMessage>
  -policy : PriorityPolicy
  -aging_enabled : bool
  -aging_threshold_ms : u64
}

class CompatCheckResult {
  +breaking_changes : Vec<BreakingChangeType>
  +affected_apps : Vec<String>
  +suggested_strategy : String
}

class SandboxResult {
  +passed : bool
  +reason : String
  +verify_details : Vec<VerifyDetail>
}

class MigrationDependencyGraph {
  -nodes : Vec<Migration>
  -edges : HashMap<MigrationId, Vec<MigrationId>>
}

class CopyBatchResult {
  +loaded_rows : u64
  +shard_results : Vec<ShardResult>
  +conflict_resolution : ConflictResolution
  +elapsed : Duration
}

class ConflictResolution <<enum>> {
  Upsert
  Ignore
  Merge
  Replace
}

class RootCause {
  +component : String
  +sql : Option<String>
  +confidence : f64
  +evidence_chain : Vec<Evidence>
}

class RemediationAction <<enum>> {
  RestartConnection
  ClearCache
  ScaleOut
  CustomAction
}

class CostComparisonReport {
  +provider_costs : Vec<ProviderCost>
  +differences : Vec<CostDifference>
  +migration_suggestions : Vec<MigrationSuggestion>
}

class CapacityForecast {
  +predicted : Vec<CapacityPoint>
  +confidence_interval : (f64, f64)
  +algorithm : ForecastAlgorithm
}

class TenantResourceQuota {
  +max_connections : Option<u32>
  +max_qps : Option<u32>
  +max_storage : Option<u64>
}

class EnhancedRlsPolicy {
  +conditions : Vec<RlsCondition>
  +masking_rules : Vec<ColumnMaskingRule>
}

class TenantAuditEntry {
  +tenant_id : String
  +operation : TenantAuditOperation
  +table : String
  +timestamp : DateTime
  +result : AuditResult
  +details : String
}

class WarmupConfig {
  +strategy : WarmupStrategy
  +batch_size : usize
}

class BloomFilter {
  -bit_array : Vec<u64>
  -hash_functions : Vec<HashFn>
  -capacity : usize
  -fpp : f64
}

class WarmupStrategy <<enum>> {
  HotspotTable
  HotspotKey
  CustomQuery
  Disabled
}

DelayedMessage --> Message : 包装
ScheduledMessage --> Message : 包装
DelayScheduler --> DelayedMessage : 管理
DelayScheduler --> ScheduledMessage : 调度
DelayScheduler --> PriorityPolicy : 使用
EnhancedRlsPolicy --> ColumnMaskingRule : 联动
TenantAuditEntry --> TenantAuditOperation : 引用
TenantAuditEntry --> AuditResult : 引用

note bottom of DelayedMessage
  复用既有 Message (queue.rs:57)
  新增 deliver_at + priority
end note

note bottom of EnhancedRlsPolicy
  复用既有 RowLevelSecurityPolicy (tenant_security.rs:67)
  增强多条件组合 + 列级脱敏联动
end note

@enduml
```

**对象创建与销毁策略：**
- `DelayScheduler`：启动时创建，`shutdown` 时优雅关闭（等待调度周期完成），延迟消息持久化由 `MessageQueue` 实现保证。
- `BloomFilter`：按容量与误判率初始化（计算最优哈希函数数与位数组大小），容量超限时按策略处理（Rebuild/Evict/Degrade）。
- `SingleFlight`：`tokio::sync` 同步原语管理 key 锁，重建完成或超时后释放锁。
- `TenantAuditLogger`：追加写入审计日志文件，不可修改/删除。

**持久化策略（不包含表结构）：**
- 延迟消息：由 `MessageQueue` 实现持久化（既有能力）。
- 审计日志：追加写入文件/DB（`TenantAuditLogger`），不可篡改。
- 布隆过滤器：内存数据结构，重启后重建（从 DB 重新加载 key）。
- 配置：`DelayScheduler`/`ForwardCompatChecker`/`CopyShardConfig`/`RemediationConfig`/`MultiCloudForecastConfig`/`TenantQuotaRlsConfig`/`WarmupProtectionConfig` 通过 `serde` 序列化，配置文件加载。

---

# 三、跨需求共享设计

## 3.1 共享类型

七项需求间存在可共享的类型与工具，避免重复实现：

| 共享类型 | 用途 | 使用需求 | 复用既有代码 |
|---------|------|---------|-------------|
| `DateTime<Utc>` | 时间表示（延迟投递时间/审计时间戳/预测时间点） | REQ-V47-001 / 004 / 005 / 006 | `chrono` crate（既有依赖） |
| `Duration` | 时间间隔（退避间隔/singleflight 超时/调度周期） | REQ-V47-001 / 007 | `std::time::Duration`（既有） |
| `BackoffPolicy` | 退避策略（延迟消息投递失败重试） | REQ-V47-001 | `packages/sz-orm-queue/src/dlx.rs:47`（v4.6.0 既有） |
| `AtomicityGuarantee` | 原子性保证级别（并行分片原子性） | REQ-V47-003 | `packages/sz-orm-batch/src/atomic.rs:20`（v4.6.0 既有） |
| `TenantContext` | 租户上下文（配额检查/RLS 注入/审计日志/缓存隔离） | REQ-V47-006 / 007 | `packages/sz-orm-core/src/tenant_context.rs:80`（既有） |
| `Pool` / `Connection` | 连接池与连接（沙箱预演/COPY 加载/配额检查/缓存预热） | REQ-V47-002 / 003 / 006 / 007 | `packages/sz-orm-core/src/pool.rs:743` / `:45`（既有） |
| `Connection::execute_with_params` | 参数化执行（沙箱 SQL/RLS WHERE 注入） | REQ-V47-002 / 006 | `packages/sz-orm-core/src/pool.rs:82`（既有） |
| `TenantAuditContext` / `TenantAuditOperation` / `AuditResult` | 审计上下文与操作（租户审计日志/自愈审计日志） | REQ-V47-004 / 006 | `packages/sz-orm-core/src/tenant_security.rs:244` / `:197` / `:224`（既有） |
| `message_tracing` | 消息追踪日志（延迟调度日志） | REQ-V47-001 | `packages/sz-orm-queue/src/message_tracing.rs`（既有） |
| `serde` / `serde_json` | 序列化（所有配置结构） | 全部 7 项 | 既有依赖 |

## 3.2 共享工具函数

| 共享工具 | 功能 | 使用需求 | 实现策略 |
|---------|------|---------|---------|
| 审计日志写入 | 追加写入不可篡改审计日志 | REQ-V47-004 / 006 | 共享 `TenantAuditLogger` 的追加写入逻辑，自愈审计日志与租户审计日志复用同一写入路径 |
| 白名单判断 | 判断动作/建议是否在自动执行白名单内 | REQ-V47-004 / 005 | 共享白名单判断逻辑（`Vec` contains），自愈动作白名单与成本优化白名单复用同一判断模式 |
| 人工确认请求 | 非白名单动作/建议请求人工确认 | REQ-V47-004 / 005 | 共享人工确认通知逻辑（通知 SRE/FinOps + 等待确认），自愈与成本优化复用同一确认流程 |
| 参数化 WHERE 注入 | 自动注入参数化 WHERE 条件 | REQ-V47-006 | 复用既有 `Connection::execute_with_params`（`pool.rs:82`），RLS 增强注入复用既有参数化绑定 |
| 异步调度循环 | `tokio::spawn` 周期检查循环 | REQ-V47-001 / 005 | 延迟调度器与成本对比周期执行复用同一调度循环模式（`tokio::time::interval` + `CancellationToken`） |
| 置信区间计算 | 基于标准差与置信水平计算上下界 | REQ-V47-005 | 容量预测置信区间计算（正态分布 z 值），独立实现（无既有依赖） |

## 3.3 feature 交互

七项需求的 feature gate 相互独立，但存在以下交互场景：

| 交互场景 | 涉及 feature | 交互行为 | 设计策略 |
|---------|-------------|---------|---------|
| 延迟队列 + 租户配额 | `delayed-priority-queue` + `tenant-quota-rls-enhanced` | 延迟消息投递时检查租户配额（max_qps） | `DelayScheduler` 投递前调用 `QuotaEnforcer.check_quota`，配额超限则延迟投递 |
| 沙箱预演 + RLS 增强 | `forward-compat-sandbox` + `tenant-quota-rls-enhanced` | 沙箱预演时 RLS 策略须在影子表上生效 | `SandboxDryRunner` 创建影子表后应用 `RlsPolicyEnhancer` 策略 |
| COPY 分片 + 租户配额 | `copy-parallel-shard` + `tenant-quota-rls-enhanced` | 并行分片加载时检查租户存储配额（max_storage） | `ParallelShardExecutor` 加载前调用 `QuotaEnforcer.check_quota` |
| 异常自愈 + 缓存防护 | `anomaly-remediation-rca` + `cache-warmup-protection` | 异常自愈 `ClearCache` 动作清缓存须通过 `PenetrationGuard` | `AutoRemediator` 执行 `ClearCache` 时调用 `ProcessL1Cache` + `L2Cache` 失效 + `BloomFilter` 重建 |
| 缓存预热 + 租户隔离 | `cache-warmup-protection` + `tenant-quota-rls-enhanced` | 缓存预热须按租户隔离 | `CacheWarmer` 预热时使用 `tenant_cache_key`（`process_l1_cache.rs:421`）按租户隔离缓存键 |
| 异常自愈 + 延迟队列 | `anomaly-remediation-rca` + `delayed-priority-queue` | 异常自愈 `RestartConnection` 须暂停延迟调度 | `AutoRemediator` 执行 `RestartConnection` 时通知 `DelayScheduler` 暂停调度 |
| 多云成本 + 异常自愈 | `multicloud-cost-forecast` + `anomaly-remediation-rca` | 成本优化执行失败可触发异常自愈 | `AutoOptimizer` 执行失败生成 `Anomaly`，`AutoRemediator` 检测后自愈 |

**feature 组合编译保证：** 7 个新 feature 与既有 feature（v4.3.0 7 个 + v4.4.0 6 个 + v4.5.0 3 个 + v4.6.0 7 个）任意组合编译通过（门禁第 10 道：`cargo check --workspace --all-targets --all-features`）。

---

# 四、性能设计

## 4.1 内存布局

| 组件 | 内存布局 | 容量控制 | 依据 |
|------|---------|---------|------|
| `DelayScheduler.delayed_messages` | `BTreeMap<DateTime, Vec<DelayedMessage>>`（按 `deliver_at` 排序，O(log n) 插入/到期检查） | 无上限（由 `MessageQueue` 持久化保证） | spec.md §4.1.1（调度开销 <= 1ms/次） |
| `PriorityQueue` | `BinaryHeap<PriorityMessage>`（按 `priority` 大顶堆，O(log n) 插入/弹出） | `queue_capacity` 默认 100000 | spec.md §4.1.2（排序开销 O(log n)） |
| `BloomFilter.bit_array` | `Vec<u64>` 位数组（按容量与误判率计算最优大小，`m = -n*ln(p)/(ln2)^2`） | `bloom_filter_capacity` 默认 1000000，误判率默认 1% | spec.md §4.1.14（查询开销 <= 100ns/次） |
| `BloomFilter.hash_functions` | `Vec<HashFn>`（最优哈希函数数 `k = m/n * ln2`） | 由容量与误判率计算 | spec.md §4.1.14 |
| `SingleFlight` | `HashMap<String, Shared<OnceCell<Value>>>`（key 锁映射） | 并发请求复用，超时释放 | spec.md §4.1.15（开销 <= 1ms/次） |
| `MigrationDependencyGraph` | `HashMap<MigrationId, Vec<MigrationId>>`（邻接表） | 迁移数量 | 拓扑排序 O(V+E) |
| `TenantResourceQuota` | `HashMap<TenantId, TenantResourceQuota>`（租户配额映射） | 租户数量 | spec.md §4.1.11（检查开销 <= 0.1ms/次） |
| `EnhancedRlsPolicy.conditions` | `Vec<RlsCondition>`（多条件组合） | 策略数量 | spec.md §4.1.12（注入开销 <= 0.2ms/查询） |

**内存优化策略：**
- 延迟消息按 `deliver_at` 排序存储在 `BTreeMap`，到期检查只需检查头部（O(1)），避免全量扫描。
- 布隆过滤器位数组按 `u64` 紧凑存储（每元素 64 位），减少内存碎片。
- `SingleFlight` 的 key 锁映射在重建完成后移除（避免内存泄漏）。
- 配额检查使用 `AtomicU32`/`AtomicU64` 原子计数器（无锁，O(1) 检查），复用既有连接池 `AtomicU32` 模式（`pool.rs`）。

## 4.2 并发策略

| 组件 | 并发策略 | 线程安全保证 | 依据 |
|------|---------|-------------|------|
| `DelayScheduler` | `tokio::spawn` 调度循环 + `RwLock` 保护 `delayed_messages` | `Send + Sync`（`RwLock` 保护） | spec.md §4.2.1（不丢失消息） |
| `PriorityQueue` | `Mutex<BinaryHeap>` 保护（写入互斥，读取互斥） | `Send + Sync`（`Mutex` 保护） | spec.md §4.2.2（不饿死） |
| `ParallelShardExecutor` | `tokio::join!` 并行分片 + 并行度限制到连接池容量 | 分片独立连接，无共享状态 | spec.md §4.1.6（吞吐量 N 倍） |
| `BloomFilter` | `RwLock` 保护位数组（多读单写） | `Send + Sync`（`RwLock` 保护） | spec.md §4.2.14（不漏判） |
| `SingleFlight` | `tokio::sync::Notify` / `OnceCell` 协调并发重建 | `Send + Sync`（`tokio::sync` 原语） | spec.md §4.2.15（不死锁） |
| `QuotaEnforcer` | `AtomicU32`/`AtomicU64` 原子计数器（无锁） | `Send + Sync`（原子操作） | spec.md §4.1.11（检查 <= 0.1ms） |
| `TenantAuditLogger` | `tokio::sync::Mutex` 保护追加写入（串行化写入） | `Send + Sync`（`Mutex` 保护） | spec.md §4.3.9（不可篡改） |
| `CacheWarmer` | `tokio::spawn` 异步预热（不阻塞启动） | `Send + Sync`（异步任务） | spec.md §4.2.13（不阻塞启动） |
| `AutoRemediator` | 异步事件订阅 + `Mutex` 保护白名单 | `Send + Sync` | spec.md §4.2.7（不误修复） |

**并发优化策略：**
- 优先使用无锁数据结构（`AtomicU32`/`AtomicU64` 原子计数器，复用既有连接池 `AtomicU32` 模式 `pool.rs`）。
- 读多写少场景使用 `RwLock`（布隆过滤器位数组、延迟消息队列）。
- 并行分片使用 `tokio::join!` 并行执行，并行度限制到连接池容量避免连接耗尽。
- singleflight 使用 `tokio::sync` 协调原语，重建超时释放锁避免死锁。

## 4.3 异步调度

| 组件 | 调度方式 | 调度周期 | 优雅关闭 | 依据 |
|------|---------|---------|---------|------|
| `DelayScheduler` | `tokio::time::interval` 周期检查到期消息 | 100ms（可配） | `CancellationToken` 停止调度循环 | spec.md §4.1.1 |
| `ScheduledMessage` | Cron 表达式计算下次投递时间 | 按 Cron 周期 | `CancellationToken` 停止 | spec.md §5.1.1.3 |
| `CacheWarmer` | `tokio::spawn` 一次性异步预热 | 启动时一次 | 预热完成或失败自动结束 | spec.md §4.2.13 |
| `MultiCloudCostComparator` | `tokio::time::interval` 周期对比 | `comparison_interval_ms` 默认每周 | `CancellationToken` 停止 | spec.md §4.1.9 |
| `CapacityForecaster` | `tokio::time::interval` 周期预测 | 按 `forecast_horizon_days` | `CancellationToken` 停止 | spec.md §4.1.10 |
| `AnomalyCorrelator` | 事件驱动（异常检测触发） | 按异常事件 | `CancellationToken` 停止 | spec.md §4.1.8 |
| `AutoRemediator` | 事件驱动（异常检测触发） | 按异常事件 | `CancellationToken` 停止 | spec.md §4.1.7 |

**异步调度优化策略：**
- 调度循环使用 `tokio::time::interval`（而非 `sleep` 循环），避免累积漂移。
- 优雅关闭使用 `CancellationToken`（而非 `bool` flag），避免竞态条件。
- 异步预热使用 `tokio::spawn`（不 `await`），不阻塞服务启动。
- 事件驱动组件（异常自愈/关联）使用 `tokio::sync::broadcast` / `mpsc` 订阅异常事件，避免轮询。

---

# 五、安全设计

## 5.1 参数化查询

**铁律：所有 SQL 执行必须参数化绑定，禁止 SQL 字符串拼接。**

| 组件 | 参数化场景 | 复用既有代码 | 依据 |
|------|-----------|-------------|------|
| `SandboxDryRunner` | 影子表迁移 SQL 执行 | `Connection::execute_with_params` `packages/sz-orm-core/src/pool.rs:82` | spec.md §4.3.2 |
| `RlsPolicyEnhancer` | RLS WHERE 条件注入（`tenant_id = ? AND dept_id IN (?,?)`） | `Connection::execute_with_params` `packages/sz-orm-core/src/pool.rs:82` | spec.md §4.3.8 |
| `CopyProtocolAdapter` | COPY 协议数据加载（参数化绑定或 COPY 协议原生参数化） | 既有 `CopyProtocolExecutor` `packages/sz-orm-batch/src/copy.rs:14` | spec.md §4.3.3 |
| `CacheWarmer` | 预热查询（自定义 SQL 参数化） | `Connection::execute_with_params` `packages/sz-orm-core/src/pool.rs:82` | spec.md §4.3.10 |

**设计策略：** 所有新增组件的 SQL 执行路径统一调用既有 `Connection::execute_with_params`（`pool.rs:82`），该方法是既有参数化执行入口，保证 SQL 注入防护。RLS 自动注入的 `tenant_id` 不可被客户端篡改（由可信路径 `TenantContext` `tenant_context.rs:80` 设置）。

## 5.2 多租户隔离

| 组件 | 隔离场景 | 隔离策略 | 复用既有代码 | 依据 |
|------|---------|---------|-------------|------|
| `DelayScheduler` | 延迟消息按租户隔离 | 消息 `headers` 含 `tenant_id`，投递时按租户隔离 | 既有 `Message.headers` `queue.rs:57` | spec.md §4.3.1 |
| `CopyProtocolAdapter` | COPY 加载数据按租户隔离 | 加载数据含 `tenant_id` 列，按租户隔离 | spec.md §4.3.3 | spec.md §4.3.3 |
| `QuotaEnforcer` | 配额按租户独立 | `TenantResourceQuota` 按租户 ID 独立配额 | v4.6.0 `ConnectionTenantBinder` `connection_tenant.rs:133` | spec.md §4.3.7 |
| `RlsPolicyEnhancer` | RLS 按租户隔离 | 自动注入 `tenant_id` WHERE 条件，参数化绑定 | 既有 `RowLevelSecurityPolicy` `tenant_security.rs:67` | spec.md §4.3.8 |
| `TenantAuditLogger` | 审计日志按租户独立 | 按租户 ID 独立审计日志文件/记录 | 既有 `TenantAuditContext` `tenant_security.rs:244` | spec.md §4.3.9 |
| `CacheWarmer` | 预热数据按租户隔离 | 使用 `tenant_cache_key` 按租户隔离缓存键 | `tenant_cache_key` `process_l1_cache.rs:421` | spec.md §4.3.10 |
| `BloomFilter` | 布隆过滤器按租户隔离 | key 含租户前缀（`tenant_id:key`），按租户隔离 | spec.md §4.3.11 | spec.md §4.3.11 |

**设计策略：** 所有新增组件的多租户隔离复用既有 `TenantContext`（`tenant_context.rs:80`）/ `tenant_cache_key`（`process_l1_cache.rs:421`）/ `ConnectionTenantBinder`（`connection_tenant.rs:133`），不新建隔离逻辑。`tenant_id` 由可信路径设置，不可被客户端篡改。

## 5.3 资源限制

| 组件 | 资源限制 | 限制策略 | 依据 |
|------|---------|---------|------|
| `PriorityQueue` | 容量上限 `queue_capacity` 默认 100000 | 超限拒绝入队，返回错误"priority queue capacity exceeded" | spec.md §5.1.3.3 |
| `ParallelShardExecutor` | 并行度限制到连接池容量 | 超限排队等待，日志标注"parallelism limited by pool capacity" | spec.md §5.3.3.4 |
| `BloomFilter` | 容量上限 `bloom_filter_capacity` 默认 1000000 | 超限按策略处理（Rebuild/Evict/Degrade） | spec.md §5.7.3.2 |
| `SingleFlight` | 重建超时 `singleflight_timeout_ms` 默认 5000 | 超时释放锁，等待请求重试 | spec.md §5.7.3.3 |
| `QuotaEnforcer` | 租户配额 `max_connections`/`max_qps`/`max_storage` | 超限拒绝请求，记录审计日志 | spec.md §4.2.11 |
| `AutoRemediator` | 自愈动作执行超时 `remediation_timeout_ms` 默认 30000 | 超时中止，通知 SRE 人工干预 | spec.md §5.4.3.1 |
| `RootCauseAnalyzer` | 根因分析开销 <= 2 秒 | 超时标注"root cause analysis timeout" | spec.md §4.1.8 |
| `CapacityForecaster` | 预测开销 <= 5 秒 | 超时标注"forecast timeout" | spec.md §4.1.10 |

**设计策略：** 所有资源限制可配置（不强制干扰开发者），默认值保守。超限场景明确返回错误或降级，不静默忽略。

## 5.4 敏感信息脱敏

| 组件 | 脱敏场景 | 脱敏策略 | 依据 |
|------|---------|---------|------|
| `DelayScheduler` | 延迟消息保留原始消息脱敏状态 | 复用既有 `sz-orm-masking` 脱敏 | spec.md §4.3.1 |
| `RootCauseAnalyzer` | 根因证据链脱敏敏感信息 | SQL 参数值、连接凭据脱敏 | spec.md §4.3.5 |
| `MultiCloudCostComparator` | 不暴露存储凭据 | 复用既有 `StorageBuilder` `storage.rs:22` 凭据管理 | spec.md §4.3.6 |
| `BloomFilter` | 不存储原始 key | 仅存储 key 的哈希（位数组），不泄露敏感 key | spec.md §4.3.11 |
| `TenantAuditLogger` | 审计日志脱敏 | 审计详情脱敏敏感字段 | spec.md §4.3.9 |

---

# 六、兼容性设计

## 6.1 API 兼容性

**铁律：所有新能力通过 feature gate 隔离，默认 feature 行为不变，既有公开 API 完全向后兼容。**

| 既有 API | 兼容性保证 | 证据 |
|---------|-----------|------|
| `MessageQueue` trait（`queue.rs:18`） | 保留不动，延迟队列与优先级调度为扩展 API（`DelayScheduler`/`PriorityQueue`） | spec.md §4.5.3 |
| `Message`（`queue.rs:57`）/ `InMemoryQueue`（`:339`） | 保留不动，`DelayedMessage` 包装 `Message` | spec.md §4.5.3 |
| `RedeliveryScheduler`（`dlx.rs:216`）/ `BackoffPolicy`（`:47`） | 保留不动，`DelayScheduler` 复用调度循环基线 | spec.md §4.5.3 |
| `Migration`（`migration.rs:11`）/ `MigrationResolver`（`:63`） | 保留不动，`ForwardCompatChecker`/`SandboxDryRunner` 为扩展 API | spec.md §4.5.4 |
| `DryRunMigration`（`migration_dry_run.rs:11`）/ `ImpactReport`（`:80`） | 保留不动，`ForwardCompatChecker` 复用 `analyze_impact` | spec.md §4.5.4 |
| `RollbackExecutor`（`rollback_zero_downtime.rs:305`） | 保留不动，`SandboxDryRunner` 复用影子表能力 | spec.md §4.5.4 |
| `CopyProtocolExecutor`（`copy.rs:14`）/ `BatchExecutor`（`executor.rs:141`） | 保留不动，`CopyProtocolAdapter`/`ParallelShardExecutor` 为扩展 API | spec.md §4.5.5 |
| `BatchTransactionCoordinator`（`atomic.rs:216`）/ `AtomicityGuarantee`（`:20`） | 保留不动，`ParallelShardExecutor` 复用原子性 | spec.md §4.5.5 |
| `AnomalyDetector`（`anomaly.rs:254`）/ `MetricsRegistry`（`lib.rs:262`） | 保留不动，`AutoRemediator`/`RootCauseAnalyzer` 为扩展 API | spec.md §4.5.6 |
| `CostAnalyzer`（`cost.rs:231`）/ `Storage`（`storage.rs:14`） | 保留不动，`MultiCloudCostComparator`/`CapacityForecaster` 为扩展 API | spec.md §4.5.7 |
| `ConnectionTenantBinder`（`connection_tenant.rs:133`）/ `RowLevelSecurityPolicy`（`tenant_security.rs:67`） | 保留不动，`QuotaEnforcer`/`RlsPolicyEnhancer` 为扩展 API | spec.md §4.5.8 |
| `Pool`（`pool.rs:743`）/ `Connection`（`:45`） | 保留不动，配额检查在 `acquire_with_tenant` 路径插入 | spec.md §4.5.8 |
| `ProcessL1Cache`（`process_l1_cache.rs:169`）/ `L2Cache`（`l2_cache.rs:517`） | 保留不动，`CacheWarmer`/`PenetrationGuard`/`SingleFlight` 为扩展 API | spec.md §4.5.9 |
| `CacheCoherenceProtocol`（`cache_coherence.rs:103`） | 保留不动，预热一致性复用 | spec.md §4.5.9 |

**sz-pay 兼容性：** sz-pay 从 crates.io 拉取 sz-orm-* 6 个包（`sz-orm-core`/`sqlx`/`config`/`auth`/`macros`/`queue`），v4.7.0 新能力默认关闭，sz-pay 既有用法不受影响（spec.md §4.5.2）。

## 6.2 feature gate 隔离

**新增 feature gate 定义位置（Cargo.toml `[features]` 段追加）：**

| feature gate | 定义位置 | 依赖既有 feature | 默认 |
|-------------|---------|-----------------|------|
| `delayed-priority-queue` | `packages/sz-orm-queue/Cargo.toml`（`[features]` 段，既有 `dlx-auto-redelivery` `:38` 之后） | `["dlx-auto-redelivery"]` | 关闭 |
| `forward-compat-sandbox` | `packages/sz-orm-core/Cargo.toml`（`[features]` 段，既有 `zero-downtime-rollback` `:134` 之后） | `["zero-downtime-rollback"]` | 关闭 |
| `copy-parallel-shard` | `packages/sz-orm-batch/Cargo.toml`（`[features]` 段，既有 `batch-atomic` `:28` 之后） | `["batch-atomic"]` | 关闭 |
| `anomaly-remediation-rca` | `packages/sz-orm-observability/Cargo.toml`（`[features]` 段，既有 `anomaly-detection` `:28` 之后） | `["anomaly-detection"]` | 关闭 |
| `multicloud-cost-forecast` | `packages/sz-orm-storage/Cargo.toml`（`[features]` 段，既有 `cost-analysis` `:19` 之后） | `["cost-analysis"]` | 关闭 |
| `tenant-quota-rls-enhanced` | `packages/sz-orm-core/Cargo.toml`（`[features]` 段，既有 `connection-level-tenant` `:136` 之后） | `["connection-level-tenant"]` | 关闭 |
| `cache-warmup-protection` | `packages/sz-orm-core/Cargo.toml`（`[features]` 段，既有 `process-l1-cache` `:138` 之后） | `["process-l1-cache"]` | 关闭 |

**feature gate 隔离策略：**
- 每个新 feature 依赖对应的 v4.6.0 feature（复用基线），启用新能力时自动启用基线 feature。
- 新增代码使用 `#[cfg(feature = "xxx")]` 属性门控，默认不编译。
- `lib.rs` 中新增模块使用 `#[cfg(feature = "xxx")] pub mod xxx;` 条件导出。
- 既有模块不添加 `#[cfg]` 属性（保持默认编译）。

## 6.3 向后兼容

| 兼容场景 | 兼容策略 | 依据 |
|---------|---------|------|
| 默认 feature 行为不变 | 7 个新 feature 默认关闭，`cargo build` 行为与 v4.6.0 一致 | spec.md §4.5.1 |
| 既有 feature 组合不破坏 | 新 feature 与既有 feature 任意组合编译通过（门禁第 10 道） | spec.md §4.5.10 |
| 既有测试基线不回退 | v4.7.0 仅增不减，v4.6.0 测试全部保留通过 | spec.md §4.2.16 |
| 配额默认无限制 | `quota_check_enabled` 默认 false，未配置配额时无限制（向后兼容） | spec.md §6.11.4 |
| RLS 增强默认关闭 | `rls_enhancement_enabled` 默认 false，使用既有 `RowLevelSecurityPolicy` | spec.md §6.11.2 |
| 预热默认关闭 | `warmup_strategy` 默认 Disabled，不预热（向后兼容） | spec.md §6.14.1 |
| 穿透防护默认关闭 | `penetration_guard_enabled` 默认 false | spec.md §6.13.6 |
| 击穿防护默认关闭 | `stampede_guard_enabled` 默认 false | spec.md §6.13.7 |
| 自愈白名单默认空 | `auto_execute_whitelist` 默认空，全部须人工确认 | spec.md §6.7.1 |
| 优化白名单默认空 | `auto_optimize_whitelist` 默认空，全部须人工确认 | spec.md §6.9.5 |

---

# 七、部署设计

## 7.1 feature 组合

**推荐 feature 组合：**

| 场景 | feature 组合 | 说明 |
|------|-------------|------|
| 最小化部署（默认） | 无新 feature | 行为与 v4.6.0 一致，sz-pay 兼容 |
| 消息队列增强 | `delayed-priority-queue` | 延迟投递 + 优先级 + 定时调度 |
| 迁移安全增强 | `forward-compat-sandbox` | 前向兼容检查 + 沙箱预演 + 依赖图 |
| 批量加载增强 | `copy-parallel-shard` | COPY 方言适配 + 并行分片 + 冲突解决 |
| 运维智能化 | `anomaly-remediation-rca` | 异常自愈 + 根因分析 + 关联 |
| 成本优化 | `multicloud-cost-forecast` | 多云对比 + 容量预测 + 自动优化 |
| 多租户增强 | `tenant-quota-rls-enhanced` | 配额 + RLS 增强 + 审计日志 |
| 缓存增强 | `cache-warmup-protection` | 预热 + 穿透防护 + 击穿防护 |
| 全能力 | 全部 7 个新 feature | 全部智能化运维深化与性能深化 |

## 7.2 编译矩阵

**门禁第 10 道：Feature 全组合编译**

```bash
# 全 feature 编译验证
cargo check --workspace --all-targets --all-features

# 单 feature 编译验证（每个新 feature 独立编译）
cargo check -p sz-orm-queue --features delayed-priority-queue
cargo check -p sz-orm-core --features forward-compat-sandbox
cargo check -p sz-orm-batch --features copy-parallel-shard
cargo check -p sz-orm-observability --features anomaly-remediation-rca
cargo check -p sz-orm-storage --features multicloud-cost-forecast
cargo check -p sz-orm-core --features tenant-quota-rls-enhanced
cargo check -p sz-orm-core --features cache-warmup-protection

# 默认 feature 编译验证（无新 feature，行为与 v4.6.0 一致）
cargo check --workspace --all-targets
```

**五方言覆盖验证：**
- COPY 协议：PostgreSQL（`COPY FROM STDIN`）/ MySQL（`LOAD DATA INFILE`）/ Oracle（`SQL*Loader`）/ MSSQL（`BULK INSERT`）/ SQLite（降级 multi-value INSERT）。
- RLS 自动注入 WHERE：全方言支持（参数化绑定）。
- 沙箱预演影子表：全方言支持（`CREATE TABLE shadow_xxx LIKE xxx` 或 `CREATE TABLE shadow_xxx AS SELECT * FROM xxx WHERE 1=0`）。

## 7.3 发布策略

| 阶段 | 操作 | 验证 |
|------|------|------|
| 1. 编译验证 | 14 道门禁全过（`scripts/gate.ps1`） | 门禁第 1-14 道全绿 |
| 2. 单元测试 | `cargo test --workspace`（含新能力测试） | v4.6.0 基线不回退 + 新能力测试通过 |
| 3. 集成测试 | `cargo test --workspace -- --ignored`（真实 DB） | 五方言集成测试通过 |
| 4. sz-pay 兼容验证 | sz-pay 拉取 v4.7.0 编译运行 | sz-pay 既有功能不受影响 |
| 5. crates.io 发布 | 版本号 4.7.0，发布 5 个扩展包 | `cargo publish` 成功 |
| 6. 文档同步 | `CHANGELOG.md` + `AGENTS.md` 版本号更新 | 门禁第 12/14 道 |

**版本号策略：** workspace.package.version 从 4.6.0 升至 4.7.0（`Cargo.toml` 集中管理），5 个扩展包版本同步升级。

**不新增包声明：** v4.7.0 不新增 workspace 成员（`Cargo.toml` `[workspace] members` 不变），所有能力通过既有 5 个包扩展实现（spec.md §10.4）。

---

# 八、测试策略

## 8.1 单元测试

每项需求的单元测试覆盖正常/边界/异常场景：

| 需求 | 单元测试覆盖 | 测试文件 |
|------|-------------|---------|
| REQ-V47-001 | 延迟投递（到期前不可消费/到期后可消费）+ 优先级排序（Strict/Weighted/FairShare）+ aging 机制 + Cron 调度 + 容量超限 | `packages/sz-orm-queue/src/delayed_priority.rs`（新增） |
| REQ-V47-002 | 前向兼容性检查（破坏性变更识别）+ 沙箱预演（影子表创建/执行/校验/清理）+ 依赖图（拓扑排序/循环检测） | `packages/sz-orm-core/src/forward_compat_sandbox.rs`（新增） |
| REQ-V47-003 | COPY 方言适配（5 方言）+ 并行分片（拆分/并行/合并）+ 冲突解决（4 策略）+ 原子性（3 级别） | `packages/sz-orm-batch/src/copy_parallel_shard.rs`（新增） |
| REQ-V47-004 | 异常自愈（白名单/人工确认/审计日志）+ 根因分析（证据链/置信度）+ 异常关联（时间/空间关联） | `packages/sz-orm-observability/src/remediation_rca.rs`（新增） |
| REQ-V47-005 | 多云对比（7 provider）+ 容量预测（3 算法/置信区间）+ 自动优化（白名单/人工确认） | `packages/sz-orm-storage/src/multicloud_forecast.rs`（新增） |
| REQ-V47-006 | 配额检查（3 资源/超限拒绝）+ RLS 增强（多条件/参数化/脱敏联动）+ 审计日志（追加写入/不可篡改） | `packages/sz-orm-core/src/tenant_quota_rls.rs`（新增） |
| REQ-V47-007 | 预热（3 策略/异步/不阻塞）+ 布隆过滤器（不漏判/误判回退）+ singleflight（不死锁/超时释放） | `packages/sz-orm-core/src/cache_warmup_protection.rs`（新增） |

## 8.2 集成测试

集成测试使用本机数据库（真实 DB）：
- MySQL 9.6：`mysql://root:test123@127.0.0.1:3306/sz_orm_test`
- PostgreSQL 18：`postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`
- Oracle 23ai Free：`127.0.0.1:1521/freepdb1`

**集成测试覆盖：**
- COPY 协议五方言真实加载（PostgreSQL COPY / MySQL LOAD DATA / Oracle SQL*Loader / MSSQL BULK INSERT / SQLite 降级）。
- 沙箱预演真实影子表创建/迁移/校验/清理。
- RLS 自动注入 WHERE 真实查询（参数化绑定验证）。
- 缓存预热真实 DB 预加载（L1+L2 命中验证）。

## 8.3 测试基线不回退

v4.7.0 不得使 v4.6.0 已验收测试基线回退（spec.md §4.2.16）：
- v4.6.0 全部测试保留通过（`cargo test --workspace`）。
- 新增测试独立于既有测试（新测试文件，不修改既有测试）。
- 既有 feature 组合测试保留通过（门禁第 10 道全 feature 编译）。

---

# 九、需求追溯矩阵

| 需求 ID | 设计章节 | 接口章节 | 数据模型 | feature gate | 复用既有代码证据 |
|---------|---------|---------|---------|-------------|-----------------|
| REQ-V47-001 | §2.1.3.1 | §2.2.2.1 | §2.3.2 | `delayed-priority-queue` | `MessageQueue` `queue.rs:18` / `Message` `:57` / `InMemoryQueue` `:339` + v4.6.0 `RedeliveryScheduler` `dlx.rs:216` / `BackoffPolicy` `:47` |
| REQ-V47-002 | §2.1.3.2 | §2.2.2.2 | §2.3.2 | `forward-compat-sandbox` | `Migration` `migration.rs:11` / `MigrationResolver` `:63` + `DryRunMigration` `migration_dry_run.rs:11` / `ImpactReport` `:80` + v4.6.0 `RollbackExecutor` `rollback_zero_downtime.rs:305` / `RollbackPlan` `:157` |
| REQ-V47-003 | §2.1.3.3 | §2.2.2.3 | §2.3.2 | `copy-parallel-shard` | `CopyProtocolExecutor` `copy.rs:14` / `BatchExecutor` `executor.rs:141` + v4.6.0 `BatchTransactionCoordinator` `atomic.rs:216` / `AtomicityGuarantee` `:20` / `SagaCompensator` `:436` |
| REQ-V47-004 | §2.1.3.4 | §2.2.2.4 | §2.3.2 | `anomaly-remediation-rca` | v4.6.0 `AnomalyDetector` `anomaly.rs:254` / `AnomalyAlert` `:206` / `Anomaly` `:154` + `MetricsRegistry` `lib.rs:262` / `QueryLogger` `query_logger.rs:73` |
| REQ-V47-005 | §2.1.3.5 | §2.2.2.5 | §2.3.2 | `multicloud-cost-forecast` | v4.6.0 `CostAnalyzer` `cost.rs:231` / `CostReport` `:213` / `ProviderCost` `:202` / `CostOptimizationSuggestion` `:55` + `Storage` `storage.rs:14` / `StorageProvider` `:287` |
| REQ-V47-006 | §2.1.3.6 | §2.2.2.6 | §2.3.2 | `tenant-quota-rls-enhanced` | v4.6.0 `ConnectionTenantBinder` `connection_tenant.rs:133` / `TenantConnectionGuard` `:249` + `RowLevelSecurityPolicy` `tenant_security.rs:67` / `ColumnMaskingRule` `:155` / `TenantAuditContext` `:244` / `Pool` `pool.rs:743` / `Connection::execute_with_params` `:82` |
| REQ-V47-007 | §2.1.3.7 | §2.2.2.7 | §2.3.2 | `cache-warmup-protection` | v4.6.0 `ProcessL1Cache` `process_l1_cache.rs:169` / `CrossSessionIdentityMap` `:366` / `tenant_cache_key` `:421` + `L2Cache` `l2_cache.rs:517` / `CacheCoherenceProtocol` `cache_coherence.rs:103` |

---

> 文档生成依据：`docs/spec/v4.7.0/spec.md`（1218 行，7 项 EARS 需求）+ 2026-08-13 逐项代码验证（40+ 项关键证据逐项实测，所有 file:line 行号均为实际存在行）+ v4.6.0 design.md 基线结构参考（2242 行）
> 审计合规：本文档所有 file:line 证据均引用真实存在的代码（2026-08-13 源码读取验证），遵循 AGENTS.md 审计合规铁律
> 设计约束：无 Breaking Change（7 个新 feature gate 隔离，默认全关闭）+ 优先复用既有能力 + 五方言覆盖 + unsafe 零容忍 + 禁止占位实现 + 与 v4.6.0 零重叠 + 不新增包 + 参数化查询铁律
> 下一阶段：spec-task-agent 生成 `tasks.md`（编码任务规划）