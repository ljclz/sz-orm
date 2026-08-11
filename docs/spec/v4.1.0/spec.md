# sz-orm v4.1.0 需求规格说明书

> 版本：v4.1.0（数据 seeding/fixture 管理 + schema diff 可视化 + 缓存一致性协议 + 消息轨迹追踪 + 存储生命周期管理 + 数据质量自动检测 + 批量流式处理 + 迁移版本分支 + 备份验证自动化）
> 基线：v4.0.0（AI 自动调优闭环 + 多 LLM 模型 + 混合搜索 + 数据 lineage + 分片 rebalance + failover 自动化 + 服务网格 + GraphQL 深度集成 + CDC，9 项能力全部通过 feature gate 隔离）
> 日期：2026-08-11
> 需求格式：EARS（Ubiquitous / Event-driven / State-driven / Optional / Unwanted）
> 文档定位：需求规格（What to build），不含技术设计（How to build）
> 优先级声明：九项任务按"P0（数据 seeding/fixture + schema diff 可视化，测试效率与迁移可视化）→ P1（缓存一致性/消息轨迹/存储生命周期/数据质量，缓存/可观测/存储/数据治理）→ P2（批量流式/迁移分支/备份验证，性能/并行开发/灾备）"序推进
> 需求编号约定：REQ-V41-xxx（v4.1.0 需求项，REQ-V41-001 ~ REQ-V41-009）
> 缺陷来源：`docs/sz-orm与同类产品对比分析.md` 剩余弱点识别 + v4.0.0 规划遗留方向（测试数据管理/迁移可视化/缓存一致性/消息可观测/存储治理/数据质量/流式批量化/并行迁移/灾备验证）
> 兼容性铁律：所有新能力通过 feature gate 隔离，默认 feature 行为不变，既有公开 API 完全向后兼容，v4.0.0 已验收测试基线不回退；sz-pay 生产依赖（从 crates.io 拉取 sz-orm-* 6 个包）不得被破坏；五方言覆盖：MySQL/PostgreSQL/SQLite/Oracle/MSSQL
> 范围声明：本版本聚焦剩余弱点 9 项任务；长期（v4.x+ Go/Java/C++ 绑定/社区扩展/跨语言事务/Informix 真实驱动）在后续版本规划；crates.io 全 46 包发布与英文文档翻译沿用既有计划，本版本不涉及

---

# 1. 组件定位

## 1.1 核心职责

本组件负责交付 sz-orm v4.1.0 的九项剩余弱点补强能力：(1) 数据 seeding/fixture 管理（faker + fixture + 数据库种子数据生成与管理框架）；(2) schema diff 可视化（迁移变更可视化 diff 工具，CLI 支持）；(3) 缓存一致性协议（L1+L2 多级缓存 MESI 风格失效/更新协议）；(4) 消息轨迹追踪（消息队列分布式追踪与 OTLP 集成）；(5) 存储生命周期管理（对象存储自动分层/过期/清理）；(6) 数据质量自动检测（基于统计学的数据质量规则引擎）；(7) 批量流式处理（Stream + Batch 结合，大数据集批量流式写入）；(8) 迁移版本分支（多分支并行开发的迁移管理）；(9) 备份验证自动化（定期恢复验证 + 数据完整性检查）。所有能力通过 feature gate 隔离，不破坏现有 API 兼容性与 v4.0.0 已验收基线。

## 1.2 核心输入

1. **v4.0.0 已验收基线**：AI 自动调优闭环 + 多 LLM 模型 + 混合搜索 + 数据 lineage + 分片 rebalance + failover 自动化 + 服务网格 + GraphQL 深度集成 + CDC，9 项能力全部通过 feature gate 隔离，作为本版本基准。
2. **对比分析文档剩余弱点**：`docs/sz-orm与同类产品对比分析.md` 中识别的 v4.0.0 后剩余弱点 9 项（P0×2 + P1×4 + P2×3）。
3. **现有能力清单与缺口证据**：
   - **数据 seeding/fixture**：`cli/src/main.rs:770` `cmd_make_seeder`（CLI 已有 seeder 骨架命令）、`:808` `cmd_seed`（CLI 已有 seed 执行命令）、`packages/sz-orm-core/src/mock.rs:63` `MockConnection`（测试 Mock 连接）。缺口：无 faker 数据生成框架（随机/语义化假数据）、无 fixture 定义（YAML/JSON 模板）、无种子数据版本管理与依赖排序。
   - **schema diff 可视化**：`packages/sz-orm-core/src/schema_sync.rs:100` `SchemaDiff`（schema 差异结构）、`:200` `diff` 函数（差分计算）、`:612` `SchemaSync`（schema 同步）、`:361` `DdlGenerator` trait（5 方言 DDL 生成器：`MySqlDdlGenerator:369`/`PgDdlGenerator:439`/`SqliteDdlGenerator:479`/`OracleDdlGenerator:522`/`MssqlDdlGenerator:565`）、`cli/src/main.rs:1389` `cmd_generate_schema`（CLI schema 生成）。缺口：无可视化 diff 输出（CLI 友好 diff/HTML/Markdown 报告）、无变更影响摘要（破坏性变更标注）、无版本间 diff 对比。
   - **缓存一致性**：`packages/sz-orm-core/src/cache.rs:11` `Cache` trait（缓存抽象）、`:141` `MultiLevelCache`（多级缓存）、`packages/sz-orm-core/src/l1_cache.rs:87` `L1Cache`（L1 本地缓存）、`:216` `L1L2Coordinator`（L1+L2 协调器）、`packages/sz-orm-core/src/l2_cache.rs:517` `L2Cache`（L2 分布式缓存）、`:1176` `L2CacheBackend` trait（L2 后端抽象）。缺口：无 MESI 风格一致性协议（Modified/Exclusive/Shared/Invalid 状态机）、无跨实例缓存失效广播、无缓存与数据库一致性保证（write-through/write-behind）。
   - **消息轨迹追踪**：`packages/sz-orm-queue/src/queue.rs:18` `MessageQueue` trait（消息队列抽象）、`:57` `Message`（消息结构）、`:183` `MqProvider`（6 provider：Kafka/RabbitMQ/RocketMQ/ActiveMQ/NATS/Pulsar）、`packages/sz-orm-tracing/src/lib.rs:31` `Span`（追踪 span）、`:129` `Tracer` trait（追踪器抽象）、`:136` `SzTracer`（自研追踪器）、`:387` `OtelTracer`（OTLP 追踪器）、`:2049` `OtlpConfig`（OTLP 配置）。缺口：无消息队列分布式追踪集成（生产/消费 span 关联、trace context 注入/提取、消息轨迹端到端可视化）。
   - **存储生命周期**：`packages/sz-orm-storage/src/storage.rs:14` `Storage` trait（对象存储抽象）、`:22` `StorageBuilder`（存储构建器）、`:287` `StorageProvider`（7 provider：S3/AliyunOSS/TencentCOS/QiniuKodo/HuaweiOBS/UpYun/Local）、`packages/sz-orm-storage/src/lib.rs:83-98`（多 provider 导出：`AliyunOssStorage`/`HuaweiObsStorage`/`LocalStorage`/`QiniuKodoStorage`/`S3Storage`/`TencentCosStorage`/`UpYunStorage`/`OpendalStorage`）。缺口：无自动分层（hot/warm/cold 存储层自动迁移）、无过期清理（TTL 到期自动删除）、无生命周期策略（按年龄/访问频率/大小分层）。
   - **数据质量检测**：`packages/sz-orm-core/src/validation/mod.rs:16` `ValidationError`（验证错误）、`:64` `Validate` trait（验证抽象）、`:70` `aggregate`（验证聚合）。缺口：无基于统计学的数据质量规则引擎（缺失值检测/异常值检测/分布漂移检测/唯一性检测/完整性检测/一致性检测）、无数据质量评分与报告。
   - **批量流式处理**：`packages/sz-orm-batch/src/lib.rs:40` `BatchOperations` trait（批处理抽象）、`:16` `BatchResult`（批处理结果）、`:435` `BatchStage`（批处理阶段）、`:448` `BatchProgress`（批处理进度）、`packages/sz-orm-core/src/stream_api.rs:50` `StreamApiExt`（流式 API 扩展）、`packages/sz-orm-core/src/paginator.rs:273` `StreamQueryTrait`（流式查询 trait）、`packages/sz-orm-core/src/streaming_export/mod.rs:11` `ExportConfig`（流式导出配置）。缺口：无 Stream + Batch 结合（大数据集批量流式写入、背压控制、内存有界）。
   - **迁移版本分支**：`packages/sz-orm-core/src/migration.rs:10` `Migration`（迁移结构）、`:62` `MigrationResolver` trait（迁移解析器）、`:68` `FileMigrationResolver`（文件迁移解析器）、`:193` `MigrationContext`（迁移上下文）、`:276` `Migrator`（迁移执行器）、`:747` `MigrationProgress`（迁移进度）、`packages/sz-orm-core/src/migration_dry_run.rs:59` `MigrationImpact`（迁移影响分析）。缺口：无多分支并行开发迁移管理（分支合并/冲突解决/版本 DAG/三方合并）。
   - **备份验证**：`packages/sz-orm-back/src/backup.rs:15` `BackupManifest`（备份清单）、`:87` `BackupManager`（备份管理器）、`:324` `BackupConfig`（备份配置）、`:364` `BackupResult`（备份结果）、`:421` `BackupCatalog`（备份目录）、`packages/sz-orm-back/src/restore.rs:8` `RestoreManager`（恢复管理器）、`:195` `RestoreResult`（恢复结果）、`packages/sz-orm-back/src/lib.rs:75` `DisasterRecoveryDrill`（灾备演练）、`:52` `DrillReport`（演练报告）。缺口：无定期恢复验证自动化（定时恢复 + 完整性检查）、无数据完整性校验（行数比对/checksum/抽样比对/Schema 一致性）。
4. **本机数据库连接信息**：MySQL 9.6（`mysql://root:test123@127.0.0.1:3306/sz_orm_test`）、PostgreSQL 18（`postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`）、Oracle 23ai Free（`127.0.0.1:1521/freepdb1`）。
5. **sz-pay 生产依赖证据**：sz-pay 从 crates.io 拉取 sz-orm-* 6 个包，作为 API 兼容性验证的下游基准。
6. **五方言覆盖约束**：MySQL/PostgreSQL/SQLite/Oracle/MSSQL，schema diff/数据质量/备份验证须覆盖全部方言（按方言能力适配）。
7. **既有 feature gate 体系**：`packages/sz-orm-core/Cargo.toml` 已有 25+ feature（含 v3.8.0 prod-ready 14 子 feature + v3.9.0 benchmark-suite/data-validation/migration-dry-run/streaming-export + v4.0.0 九项 feature），作为新能力 feature gate 隔离的基础。

## 1.3 核心输出

1. **数据 seeding/fixture 管理**：FakerGenerator（faker 数据生成）+ FixtureLoader（YAML/JSON fixture 加载）+ SeedManager（种子数据版本管理与依赖排序）+ CLI 集成（`sz-orm seed`/`sz-orm make:seeder` 增强）。
2. **schema diff 可视化**：SchemaDiffVisualizer（可视化 diff 输出）+ DiffReport（CLI/HTML/Markdown 报告）+ 变更影响摘要（破坏性变更标注）+ CLI 集成（`sz-orm migrate:diff`）。
3. **缓存一致性协议**：CacheCoherenceProtocol（MESI 风格状态机）+ InvalidationBroadcaster（跨实例失效广播）+ ConsistencyStrategy（write-through/write-behind）+ 一致性验证报告。
4. **消息轨迹追踪**：MessageTracingInterceptor（消息队列追踪拦截器）+ TraceContextPropagator（trace context 注入/提取）+ 消息轨迹端到端 span 关联 + OTLP 集成。
5. **存储生命周期管理**：StorageLifecycleManager（生命周期策略引擎）+ TieringPolicy（分层策略 hot/warm/cold）+ ExpirationCleaner（过期清理）+ 生命周期执行报告。
6. **数据质量自动检测**：DataQualityEngine（数据质量规则引擎）+ QualityRule（统计学规则：缺失值/异常值/分布漂移/唯一性/完整性）+ QualityReport（质量评分与报告）+ 质量告警。
7. **批量流式处理**：StreamBatchProcessor（Stream + Batch 结合处理器）+ BackpressureController（背压控制）+ 有界内存批量写入 + 流式批量进度报告。
8. **迁移版本分支**：MigrationBranchManager（分支管理）+ VersionDag（版本有向无环图）+ BranchMerger（三方合并）+ 冲突解决报告 + CLI 集成（`sz-orm migrate:branch`）。
9. **备份验证自动化**：BackupVerificationScheduler（定期验证调度）+ RestoreVerifier（恢复验证）+ IntegrityChecker（数据完整性校验：行数/checksum/抽样/Schema）+ 验证报告 + 告警通知。
10. **需求追溯矩阵**：本文档第 7 章，建立需求 ↔ 验收条件映射。
11. **验收标准总览**：本文档第 8 章，按需求项汇总验收条件。

## 1.4 职责边界

本组件**不负责**以下事项：

1. **不破坏既有公开 API**：所有新能力通过 feature gate 隔离，既有公开 API 签名保持完全向后兼容。
2. **不改变既有安全铁律**：任何 WHERE 条件必须参数化，默认禁止 `SELECT *`，N+1 检测自动拦截，沿用既有铁律。
3. **不替换既有 CLI seeder**：既有 `cli/src/main.rs:770` `cmd_make_seeder` 与 `:808` `cmd_seed` 保留，新增 `FakerGenerator`/`FixtureLoader`/`SeedManager` 增强既有命令，不修改既有命令行为。
4. **不替换既有 schema diff**：既有 `SchemaDiff`（`packages/sz-orm-core/src/schema_sync.rs:100`）与 `diff` 函数（`:200`）保留，新增 `SchemaDiffVisualizer` 输出可视化报告，不修改既有 diff 计算。
5. **不替换既有缓存**：既有 `MultiLevelCache`（`packages/sz-orm-core/src/cache.rs:141`）、`L1Cache`（`l1_cache.rs:87`）、`L2Cache`（`l2_cache.rs:517`）、`L1L2Coordinator`（`l1_cache.rs:216`）保留，新增 `CacheCoherenceProtocol` 编排既有缓存，不修改既有缓存逻辑。
6. **不替换既有消息队列**：既有 `MessageQueue` trait（`packages/sz-orm-queue/src/queue.rs:18`）与 6 provider 保留，新增 `MessageTracingInterceptor` 拦截既有消息队列调用，不修改既有消息队列实现。
7. **不替换既有追踪**：既有 `Tracer` trait（`packages/sz-orm-tracing/src/lib.rs:129`）、`SzTracer`（`:136`）、`OtelTracer`（`:387`）保留，新增消息轨迹集成复用既有追踪器，不重复实现追踪。
8. **不替换既有存储**：既有 `Storage` trait（`packages/sz-orm-storage/src/storage.rs:14`）与 7 provider 保留，新增 `StorageLifecycleManager` 编排既有存储，不修改既有存储操作。
9. **不替换既有验证**：既有 `Validate` trait（`packages/sz-orm-core/src/validation/mod.rs:64`）与 `ValidationError`（`:16`）保留，新增 `DataQualityEngine` 扩展既有验证为统计学规则引擎，不修改既有验证逻辑。
10. **不替换既有批处理**：既有 `BatchOperations` trait（`packages/sz-orm-batch/src/lib.rs:40`）保留，新增 `StreamBatchProcessor` 结合既有批处理与流式 API，不重复实现批处理。
11. **不替换既有流式 API**：既有 `StreamApiExt`（`packages/sz-orm-core/src/stream_api.rs:50`）与 `StreamQueryTrait`（`paginator.rs:273`）保留，新增 `StreamBatchProcessor` 复用既有流式 API。
12. **不替换既有迁移**：既有 `Migrator`（`packages/sz-orm-core/src/migration.rs:276`）、`MigrationResolver`（`:62`）、`MigrationContext`（`:193`）保留，新增 `MigrationBranchManager` 编排既有迁移，不修改既有迁移执行逻辑。
13. **不替换既有备份/恢复**：既有 `BackupManager`（`packages/sz-orm-back/src/backup.rs:87`）、`RestoreManager`（`restore.rs:8`）、`DisasterRecoveryDrill`（`lib.rs:75`）保留，新增 `BackupVerificationScheduler` 编排既有备份/恢复，不修改既有备份/恢复逻辑。
14. **不负责 sz-pay / sz-rust 下游代码修改**：ADR-0001 严禁修改下游/上游仓库，仅保证 API 兼容性。
15. **不降低既有测试覆盖**：v4.1.0 不得使 v4.0.0 已验收测试基线回退，仅增不减。
16. **不负责长期任务**：Go/Java/C++ 绑定/社区扩展/跨语言事务/Informix 真实驱动等在 v4.x+ 规划。
17. **不强制启用新能力**：所有新能力默认关闭或可选启用，避免无配置环境行为变化。
18. **不引入 unsafe**：所有新增代码无 `unsafe` 块，或必须有 `// SAFETY:` 注释，沿用既有 unsafe 零容忍铁律。

---

# 2. 领域术语

**数据 seeding/fixture 管理（Data Seeding & Fixture Management）**
: 测试数据生成与管理框架，结合 faker（随机/语义化假数据生成，如姓名/邮箱/地址/UUID）、fixture（YAML/JSON 模板定义静态测试数据）、数据库种子数据（基础数据/引用数据初始化），支持种子数据版本管理、依赖排序（外键依赖拓扑排序）、环境隔离（dev/test/staging 不同种子）。
: 备注：既有 `cli/src/main.rs:770` `cmd_make_seeder` 与 `:808` `cmd_seed` 仅有 CLI 骨架，本版本补 faker + fixture + 版本管理。

**schema diff 可视化（Schema Diff Visualization）**
: 迁移变更的可视化 diff 工具，将既有 `SchemaDiff`（`packages/sz-orm-core/src/schema_sync.rs:100`）结构化差分结果输出为可视化报告（CLI 彩色 diff/HTML/Markdown），标注破坏性变更（DROP TABLE/DROP COLUMN/ALTER COLUMN 类型变更），支持版本间 diff 对比与变更影响摘要。
: 备注：既有 `SchemaDiff` 与 `diff` 函数（`:200`）仅计算差分，本版本补可视化输出与破坏性变更标注。

**缓存一致性协议（Cache Coherence Protocol）**
: L1+L2 多级缓存的 MESI 风格一致性协议，为每个缓存行维护状态机（Modified/Exclusive/Shared/Invalid），跨实例缓存失效广播（通过消息队列/Pub-Sub），保证缓存与数据库最终一致性，支持 write-through（同步写穿）/write-behind（异步写回）策略。
: 备注：既有 `L1L2Coordinator`（`packages/sz-orm-core/src/l1_cache.rs:216`）仅协调 L1+L2 读写，本版本补 MESI 状态机与跨实例失效广播。

**消息轨迹追踪（Message Trace Tracking）**
: 消息队列分布式追踪集成，为消息生产/消费创建追踪 span，注入/提取 trace context（W3C Trace Context/B3 propagation），关联消息端到端轨迹（生产→队列→消费→下游），与 OTLP 集成导出到 Jaeger/Tempo/Zipkin。
: 备注：既有 `SzTracer`（`packages/sz-orm-tracing/src/lib.rs:136`）与 `OtelTracer`（`:387`）为通用追踪器，本版本补消息队列专用追踪集成。

**存储生命周期管理（Storage Lifecycle Management）**
: 对象存储自动生命周期管理，按策略自动分层（hot/warm/cold，基于访问频率/年龄/大小）、过期清理（TTL 到期自动删除）、存储策略配置（分层规则/迁移触发条件/清理周期），降低存储成本。
: 备注：既有 `Storage` trait（`packages/sz-orm-storage/src/storage.rs:14`）与 7 provider 仅提供存储操作，本版本补生命周期策略引擎。

**数据质量自动检测（Data Quality Auto-Detection）**
: 基于统计学的数据质量规则引擎，自动检测数据质量问题：缺失值检测（NULL 比例超阈值）、异常值检测（Z-Score/IQR/3σ）、分布漂移检测（KL 散度/PSI）、唯一性检测（主键/唯一约束违反）、完整性检测（外键引用完整性）、一致性检测（跨表/跨字段一致），输出质量评分与报告。
: 备注：既有 `Validate` trait（`packages/sz-orm-core/src/validation/mod.rs:64`）为字段级验证，本版本补表级/统计级数据质量规则引擎。

**批量流式处理（Batch Stream Processing）**
: Stream + Batch 结合的混合处理模式，大数据集批量流式写入（流式读取 + 批量写入 + 背压控制），内存有界（不一次性加载全量数据），支持断点续传与进度报告，适用于 ETL/数据迁移/批量导入场景。
: 备注：既有 `BatchOperations` trait（`packages/sz-orm-batch/src/lib.rs:40`）为批量操作，`StreamApiExt`（`packages/sz-orm-core/src/stream_api.rs:50`）为流式查询，本版本补两者结合。

**迁移版本分支（Migration Version Branching）**
: 多分支并行开发的迁移管理，每个分支独立迁移版本序列，合并时三方合并（base 分支迁移 + 两个分支迁移 → 合并后迁移序列），冲突检测与解决（同名版本号/表结构冲突），版本 DAG（有向无环图）管理分支间依赖关系。
: 备注：既有 `Migrator`（`packages/sz-orm-core/src/migration.rs:276`）为线性迁移执行，本版本补分支管理与三方合并。

**备份验证自动化（Backup Verification Automation）**
: 定期自动验证备份可用性的机制，定时执行恢复演练（从备份恢复到临时实例）+ 数据完整性检查（行数比对/checksum 校验/抽样比对/Schema 一致性），验证失败告警通知，确保备份真实可用（不只备份成功，还要恢复成功）。
: 备注：既有 `BackupManager`（`packages/sz-orm-back/src/backup.rs:87`）与 `RestoreManager`（`restore.rs:8`）为手动备份/恢复，`DisasterRecoveryDrill`（`lib.rs:75`）为手动灾备演练，本版本补定期自动验证与完整性检查。

**v4.1.0 feature gate**
: 控制本版本新能力的 feature gate 集合（`data-seeding` / `schema-diff-viz` / `cache-coherence` / `message-tracing` / `storage-lifecycle` / `data-quality` / `batch-stream` / `migration-branch` / `backup-verify`），默认关闭，避免无配置环境行为变化。

---

# 3. 角色与边界

## 3.1 核心角色

- **ORM 库维护者**：执行 v4.1.0 九项补强的开发、验证、测试操作者，是新增能力的主要使用者与验收人。
- **下游项目开发者（sz-pay）**：关注 API 兼容性、数据 seeding/缓存一致性/消息追踪新能力使用的下游使用者，v4.1.0 不得破坏其既有代码。
- **测试工程师**：使用数据 seeding/fixture 管理生成测试数据、备份验证自动化验证灾备、数据质量检测验证数据质量。
- **运维/SRE 工程师**：使用存储生命周期管理降低存储成本、备份验证自动化保障灾备、缓存一致性协议保障缓存正确、消息轨迹追踪排查消息问题。
- **数据工程师**：使用数据质量自动检测做数据治理、批量流式处理做 ETL、数据 seeding 做数据初始化。
- **应用开发者**：使用 schema diff 可视化了解迁移变更、迁移版本分支管理并行开发、批量流式处理做数据导入。

## 3.2 外部系统

- **MySQL 9.6 / PostgreSQL 18 / SQLite / Oracle 23ai / MSSQL**：schema diff/数据质量/备份验证/数据 seeding 的五方言覆盖目标。
- **faker crate（或自研）**：数据 seeding 的假数据生成后端（姓名/邮箱/地址/UUID/随机数）。
- **YAML/JSON fixture 文件**：数据 seeding 的静态测试数据模板。
- **Jaeger / Tempo / Zipkin**：消息轨迹追踪的 OTLP 后端可视化系统。
- **W3C Trace Context / B3 propagation**：消息轨迹追踪的 context 传播协议。
- **对象存储（S3/AliyunOSS/TencentCOS/QiniuKodo/HuaweiOBS/UpYun/Local）**：存储生命周期管理的目标存储（复用既有 `sz-orm-storage` 7 provider）。
- **消息队列（Kafka/RabbitMQ/RocketMQ/ActiveMQ/NATS/Pulsar）**：缓存一致性失效广播与消息轨迹追踪的目标队列（复用既有 `sz-orm-queue` 6 provider）。
- **sz-pay 项目**：API 兼容性验证的下游基准。

## 3.3 交互上下文

```plantuml
@startuml
title sz-orm v4.1.0 九项剩余弱点补强 交互上下文

rectangle "sz-orm v4.1.0" as orm {
  rectangle "数据 seeding/fixture" as seeding
  rectangle "schema diff 可视化" as diffviz
  rectangle "缓存一致性协议" as coherence
  rectangle "消息轨迹追踪" as msgtrace
  rectangle "存储生命周期管理" as lifecycle
  rectangle "数据质量自动检测" as quality
  rectangle "批量流式处理" as batchstream
  rectangle "迁移版本分支" as migbranch
  rectangle "备份验证自动化" as backupverify
}

actor "ORM 库维护者" as dev
actor "测试工程师" as tester
actor "运维/SRE" as sre
actor "数据工程师" as data
actor "应用开发者" as app
actor "sz-pay 开发者" as downstream

database "MySQL/PG/SQLite\n/Oracle/MSSQL" as db
rectangle "faker + YAML/JSON\nfixture" as faker
cloud "Jaeger/Tempo/Zipkin" as tracing
rectangle "S3/AliyunOSS/TencentCOS\n/.../Local" as storage
rectangle "Kafka/RabbitMQ/\nNATS/Pulsar/..." as mq

dev --> orm : 开发/验证/测试
tester --> seeding : 生成测试数据
seeding --> faker : faker+fixture 生成
seeding --> db : 写入种子数据
app --> diffviz : 查看迁移变更
diffviz --> db : schema diff 计算
sre --> coherence : 缓存一致性配置
coherence --> mq : 失效广播
coherence --> db : write-through/behind
sre --> msgtrace : 消息轨迹排查
msgtrace --> mq : 拦截消息生产/消费
msgtrace --> tracing : OTLP 导出
sre --> lifecycle : 存储分层/过期配置
lifecycle --> storage : 分层迁移/清理
data --> quality : 数据质量检测
quality --> db : 统计学规则检测
data --> batchstream : ETL 批量流式
batchstream --> db : 流式读+批量写
app --> migbranch : 多分支并行迁移
migbranch --> db : 分支迁移管理
sre --> backupverify : 定期恢复验证
backupverify --> db : 恢复+完整性检查
downstream --> orm : API 兼容性验证

@enduml
```

---

# 4. DFX约束

## 4.1 性能

1. **faker 数据生成开销**：faker 单条数据生成开销不超过 10μs（含字段类型映射 + 随机生成），批量生成 10,000 条不超过 100ms。
2. **fixture 加载开销**：fixture 文件加载解析开销不超过 50ms/文件（YAML/JSON 解析 + 依赖排序），大型 fixture（10,000 条）不超过 500ms。
3. **schema diff 可视化开销**：schema diff 可视化报告生成开销不超过 200ms（含 diff 计算 + 报告渲染，表数量 ≤100）。
4. **缓存一致性开销**：缓存一致性协议单次失效广播开销不超过 5ms（本地状态更新 + 广播消息发送），跨实例失效传播不超过 50ms（含消息队列往返）。
5. **消息轨迹追踪开销**：消息轨迹 span 创建/注入/提取开销不超过 100μs/消息（不影响消息吞吐，采样率可配置降低开销）。
6. **存储生命周期执行开销**：存储生命周期策略评估开销不超过 100ms/1,000 对象（含分层判定 + 过期检查），分层迁移吞吐不低于 100 对象/秒。
7. **数据质量检测开销**：数据质量规则引擎单表检测开销不超过 1 秒/10,000 行（含缺失值/异常值/唯一性/完整性统计计算）。
8. **批量流式处理吞吐**：批量流式处理吞吐不低于 50,000 行/秒（批量大小 1,000，含流式读 + 批量写 + 背压控制，单机基准）。
9. **迁移分支合并开销**：迁移分支三方合并开销不超过 1 秒（含版本 DAG 构建 + 冲突检测，迁移文件数量 ≤100）。
10. **备份验证开销**：备份验证（恢复 + 完整性检查）开销不超过 5 分钟/1GB 备份（含恢复 + 行数比对 + checksum + 抽样）。

## 4.2 可靠性

1. **seeding 幂等性**：数据 seeding 须支持幂等执行（重复执行不产生重复数据，通过 upsert 或 truncate+insert），避免种子数据重复。
2. **seeding 依赖排序**：数据 seeding 须按外键依赖拓扑排序执行，避免外键约束违反。
3. **schema diff 准确性**：schema diff 可视化须与既有 `SchemaDiff`（`packages/sz-orm-core/src/schema_sync.rs:100`）计算结果一致，可视化不改变 diff 语义。
4. **缓存一致性最终一致**：缓存一致性协议须保证缓存与数据库最终一致（write-through 强一致 / write-behind 最终一致 + 失效广播），不出现陈旧缓存。
5. **消息轨迹不丢不重**：消息轨迹追踪须保证 span 不丢失（采样率配置为 100% 时），trace context 注入/提取不破坏消息内容。
6. **存储生命周期不误删**：存储生命周期过期清理须双重确认（TTL + 最后访问时间），不误删活跃对象，删除前可选备份。
7. **数据质量检测可重复**：数据质量检测结果须可重复（同一数据同一规则同一结果），不因检测时机不同产生不同结论。
8. **批量流式断点续传**：批量流式处理中断（网络/节点故障）须支持断点续传，已处理数据不丢失不重复。
9. **迁移分支合并正确性**：迁移分支三方合并须保证合并后迁移序列可正确执行（不跳过不重复），冲突须检测并阻止自动合并。
10. **备份验证完整性**：备份验证须校验恢复数据完整性（行数/checksum/Schema 一致性），验证失败须告警，不误报成功。
11. **v4.0.0 测试基线不回退**：v4.1.0 不得使 v4.0.0 已验收测试基线回退，仅增不减。

## 4.3 安全性

1. **seeding 数据脱敏**：数据 seeding 生成的测试数据须尊重既有脱敏规则（`sz-orm-masking`），敏感字段（手机号/身份证/密码）可选脱敏，禁止真实敏感数据入测试库。
2. **seeding 环境隔离**：数据 seeding 须环境隔离（dev/test/staging 不同种子配置），禁止 production 环境误执行 seeding（破坏生产数据）。
3. **schema diff 不泄露**：schema diff 可视化报告须尊重既有脱敏规则，敏感表/字段名可选脱敏展示。
4. **缓存失效广播鉴权**：缓存一致性失效广播须支持鉴权（消息队列 ACL），禁止未授权实例接收失效广播。
5. **消息轨迹脱敏**：消息轨迹 span 的属性/消息内容须尊重既有脱敏规则，敏感数据脱敏后再导出到 OTLP 后端。
6. **存储生命周期删除保护**：存储生命周期过期清理须支持删除保护（保留期/删除确认/软删除），不硬删受保护对象。
7. **数据质量检测只读**：数据质量检测须只读（不修改源数据），检测结果可选写入审计链。
8. **备份验证隔离**：备份验证恢复须在隔离实例执行（不污染生产实例），验证数据验证后清理。

## 4.4 可维护性

1. **seeding 可观测**：数据 seeding 须输出结构化执行报告（种子列表/执行顺序/行数/耗时/幂等标记），可被 CI/工具解析。
2. **schema diff 报告格式**：schema diff 可视化须支持多格式输出（CLI 彩色/HTML/Markdown/JSON），可被不同工具消费。
3. **缓存一致性可观测**：缓存一致性协议须输出状态机指标（各状态缓存行数/失效广播次数/一致性违反次数），接入既有 Prometheus。
4. **消息轨迹可观测**：消息轨迹追踪须输出消息轨迹指标（消息延迟/跨度/丢失率），接入既有 Prometheus + OTLP。
5. **存储生命周期可观测**：存储生命周期须输出执行报告（分层迁移数/过期清理数/节省存储量），可查询历史执行记录。
6. **数据质量报告**：数据质量检测须输出结构化质量报告（规则列表/通过率/评分/异常详情），可被 CI/工具解析。
7. **批量流式进度可观测**：批量流式处理进度须可查询（已处理/剩余/吞吐/预估完成时间），可中止/恢复。
8. **迁移分支可视化**：迁移分支须支持版本 DAG 可视化（DOT/JSON 导出），可被 Graphviz 渲染。
9. **备份验证报告**：备份验证须输出验证报告（备份 ID/恢复耗时/完整性检查结果/异常详情），验证失败告警通知。
10. **审计证据要求**：每项需求结论须附 file:line 证据，遵循 AGENTS.md 审计合规铁律。

## 4.5 兼容性

1. **API 向后兼容**：所有新能力通过 feature gate 隔离，默认 feature 行为不变，既有公开 API 完全向后兼容。
2. **sz-pay 不破坏**：sz-pay 从 crates.io 拉取的 sz-orm-* 6 个包既有用法不受影响。
3. **五方言一致**：新增能力在 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 五方言上行为一致（schema diff/数据质量/备份验证/seeding 按方言能力适配）。
4. **既有 CLI seeder 保留**：既有 `cli/src/main.rs:770` `cmd_make_seeder` 与 `:808` `cmd_seed` 保留不动，新增 faker/fixture 增强既有命令。
5. **既有 schema diff 保留**：既有 `SchemaDiff`（`packages/sz-orm-core/src/schema_sync.rs:100`）与 `diff` 函数（`:200`）保留不动，新增可视化输出层。
6. **既有缓存保留**：既有 `MultiLevelCache`（`packages/sz-orm-core/src/cache.rs:141`）、`L1Cache`（`l1_cache.rs:87`）、`L2Cache`（`l2_cache.rs:517`）、`L1L2Coordinator`（`l1_cache.rs:216`）保留不动，新增一致性协议编排。
7. **既有消息队列保留**：既有 `MessageQueue` trait（`packages/sz-orm-queue/src/queue.rs:18`）与 6 provider 保留不动，新增追踪拦截器。
8. **既有追踪保留**：既有 `Tracer` trait（`packages/sz-orm-tracing/src/lib.rs:129`）、`SzTracer`（`:136`）、`OtelTracer`（`:387`）保留不动，新增消息轨迹复用。
9. **既有存储保留**：既有 `Storage` trait（`packages/sz-orm-storage/src/storage.rs:14`）与 7 provider 保留不动，新增生命周期管理编排。
10. **既有验证保留**：既有 `Validate` trait（`packages/sz-orm-core/src/validation/mod.rs:64`）与 `ValidationError`（`:16`）保留不动，新增数据质量引擎扩展。
11. **既有批处理保留**：既有 `BatchOperations` trait（`packages/sz-orm-batch/src/lib.rs:40`）保留不动，新增流式批量处理器复用。
12. **既有流式 API 保留**：既有 `StreamApiExt`（`packages/sz-orm-core/src/stream_api.rs:50`）与 `StreamQueryTrait`（`paginator.rs:273`）保留不动。
13. **既有迁移保留**：既有 `Migrator`（`packages/sz-orm-core/src/migration.rs:276`）、`MigrationResolver`（`:62`）保留不动，新增分支管理编排。
14. **既有备份/恢复保留**：既有 `BackupManager`（`packages/sz-orm-back/src/backup.rs:87`）、`RestoreManager`（`restore.rs:8`）、`DisasterRecoveryDrill`（`lib.rs:75`）保留不动，新增验证调度编排。

---

# 5. 核心能力

## 5.1 数据 seeding/fixture 管理（REQ-V41-001）

### 5.1.1 业务规则

1. **faker 数据生成**（EARS: Ubiquitous）
   系统应当提供 `FakerGenerator`，支持按字段类型生成随机/语义化假数据（姓名/邮箱/地址/手机号/UUID/日期/数字/布尔/枚举/JSON），可按字段语义自定义生成器（如 `user.email` 用邮箱生成器）。
   a. 验收条件：[定义 User 模型含 name:String/email:String/age:u32，调用 FakerGenerator] → [生成随机但语义正确的数据，如 name="张三"、email="zhangsan@example.com"、age=随机 18-65]
2. **fixture 模板加载**（EARS: Ubiquitous）
   系统应当提供 `FixtureLoader`，支持从 YAML/JSON 文件加载静态测试数据模板，模板含表名/字段值/记录数/关联引用（如 `${user.0.id}` 引用 user 第一条记录 id），支持模板继承与覆盖。
   a. 验收条件：[fixture 文件定义 users 表 10 条 + orders 表引用 users.id] → [加载后 orders 每条记录的 user_id 正确引用 users 记录 id]
3. **种子数据版本管理**（EARS: Ubiquitous）
   系统应当提供 `SeedManager`，管理种子数据版本（类似迁移版本），每个种子文件含版本号/描述/依赖（前置种子），按依赖拓扑排序执行，记录已执行版本避免重复。
   a. 验收条件：[种子 A（users）← 种子 B（orders 依赖 users），执行 SeedManager] → [先执行 A 再执行 B，记录已执行版本，重复执行跳过]
4. **幂等执行**（EARS: Ubiquitous）
   数据 seeding 须支持幂等执行（重复执行不产生重复数据），通过 upsert（存在则更新不存在则插入）或 truncate+insert（先清空再插入）模式，可配置。
   a. 验收条件：[执行 seed 两次，mode=upsert] → [第二次不产生重复数据，仅更新；mode=truncate+insert] → [第二次先清空再插入，行数不变]
5. **环境隔离**（EARS: State-driven）
   在非 dev/test/staging 环境状态下，系统应当禁止执行 seeding（避免破坏生产数据），除非显式配置 `allow_production=true`（需双重确认）。
   a. 验收条件：[环境=production，执行 seed] → [拒绝执行，提示"production seeding forbidden"；配置 allow_production=true + 确认] → [执行]
6. **CLI 集成**（EARS: Ubiquitous）
   系统应当增强既有 CLI 命令（`cli/src/main.rs:770` `cmd_make_seeder` 与 `:808` `cmd_seed`），支持 `sz-orm make:seeder --faker`（生成 faker seeder 骨架）、`sz-orm seed --fixture=path --env=test`（加载 fixture 执行 seeding）。
   a. 验收条件：[执行 `sz-orm seed --fixture=fixtures/users.yml --env=test`] → [加载 fixture，在 test 环境执行 seeding，输出执行报告]
7. **复用既有 CLI seeder**（EARS: Ubiquitous）
   系统应当复用既有 `cmd_make_seeder`（`:770`）与 `cmd_seed`（`:808`），新增 faker/fixture 作为增强，不修改既有命令行为。
   a. 验收条件：[不启用 `data-seeding` feature，执行既有 `sz-orm seed`] → [行为与 v4.0.0 一致]
8. **禁止项**（EARS: Unwanted）
   如果数据 seeding 影响默认 feature 编译或运行时行为，则系统应当通过 `data-seeding` feature gate 隔离，默认不启用 faker/fixture。
   a. 验收条件：[`cargo build` 默认编译] → [无 faker/fixture，行为与 v4.0.0 一致]

### 5.1.2 交互流程

```plantuml
@startuml
title 数据 seeding/fixture 管理 交互流程
actor "测试工程师" as tester
participant "SeedManager" as manager
participant "FakerGenerator" as faker
participant "FixtureLoader" as loader
database "DB" as db

tester -> manager : seed(fixture=path, env=test)
manager -> manager : 检查环境隔离
manager -> loader : 加载 fixture 文件(YAML/JSON)
loader --> manager : fixture 模板(含依赖引用)
manager -> manager : 依赖拓扑排序
loop 按排序执行种子
  alt faker 生成
    manager -> faker : 按字段类型生成数据
    faker --> manager : 随机数据
  else fixture 静态数据
    manager -> loader : 解析模板引用(${user.0.id})
    loader --> manager : 解析后数据
  end
  manager -> db : upsert/truncate+insert
  manager -> manager : 记录已执行版本
end
manager --> tester : 执行报告(种子/顺序/行数/耗时)
@enduml
```

### 5.1.3 异常场景

1. **环境隔离拒绝**
   a. 触发条件：在 production 环境执行 seeding 且未配置 allow_production
   b. 系统行为：拒绝执行，提示环境隔离策略
   c. 用户感知：错误提示"production seeding forbidden, set allow_production=true with confirmation"
2. **依赖循环检测**
   a. 触发条件：种子依赖形成循环（A←B←A）
   b. 系统行为：检测循环，拒绝执行，提示循环依赖链
   c. 用户感知：错误提示"seed dependency cycle: A←B←A"
3. **fixture 文件格式错误**
   a. 触发条件：fixture YAML/JSON 格式错误
   b. 系统行为：解析失败，提示文件路径与错误位置
   c. 用户感知：错误提示"fixture parse failed at users.yml:10, invalid YAML"

## 5.2 schema diff 可视化（REQ-V41-002）

### 5.2.1 业务规则

1. **可视化 diff 输出**（EARS: Ubiquitous）
   系统应当提供 `SchemaDiffVisualizer`，将既有 `SchemaDiff`（`packages/sz-orm-core/src/schema_sync.rs:100`）差分结果输出为可视化报告，支持三种格式：(a) CLI 彩色 diff（类似 git diff，增删改颜色标注）；(b) HTML 报告（含表/字段变更详情 + 破坏性标注）；(c) Markdown 报告（可嵌入文档）。
   a. 验收条件：[schema 含新增表 users + 删除表 old_logs + 修改表 orders 加列 amount] → [CLI diff 绿色 +users、红色 -old_logs、黄色 ~orders；HTML/Markdown 含详情]
2. **破坏性变更标注**（EARS: Ubiquitous）
   系统应当标注破坏性变更（DROP TABLE/DROP COLUMN/ALTER COLUMN 类型变更/缩短长度/NOT NULL 加约束），破坏性变更用红色/⚠️ 标注，非破坏性变更（加表/加列/加索引）用绿色/✓ 标注。
   a. 验收条件：[diff 含 DROP COLUMN + ADD COLUMN] → [DROP COLUMN 红色 ⚠️ 破坏性，ADD COLUMN 绿色 ✓ 非破坏性]
3. **变更影响摘要**（EARS: Ubiquitous）
   系统应当输出变更影响摘要（新增表数/删除表数/修改表数/新增列数/删除列数/破坏性变更数/预估影响行数），便于快速评估变更规模。
   a. 验收条件：[diff 含 2 新增表 + 1 删除表 + 3 修改表] → [摘要"6 表变更，2 新增/1 删除/3 修改，破坏性 1"]
4. **版本间 diff 对比**（EARS: Optional）
   当指定两个迁移版本时，系统应当对比两版本 schema diff，输出版本间变更（从版本 A 到版本 B 的表/字段变更）。
   a. 验收条件：[`sz-orm migrate:diff --from=v001 --to=v003`] → [输出 v001→v003 的 schema 变更 diff]
5. **复用既有 SchemaDiff**（EARS: Ubiquitous）
   系统应当复用既有 `SchemaDiff`（`packages/sz-orm-core/src/schema_sync.rs:100`）与 `diff` 函数（`:200`），可视化不重新计算 diff，仅渲染既有结果。
   a. 验收条件：[SchemaDiffVisualizer] → [基于既有 SchemaDiff 渲染，不重复计算]
6. **CLI 集成**（EARS: Ubiquitous）
   系统应当提供 CLI 命令 `sz-orm migrate:diff --format=cli/html/markdown`，输出可视化 diff 报告。
   a. 验收条件：[执行 `sz-orm migrate:diff --format=html`] → [输出 HTML diff 报告文件]
7. **五方言 DDL 差异**（EARS: Ubiquitous）
   系统应当复用既有 `DdlGenerator` trait（`packages/sz-orm-core/src/schema_sync.rs:361`，5 方言：MySql/Pg/Sqlite/Oracle/Mssql），diff 可视化标注方言特定差异（如 MySQL AUTO_INCREMENT vs PostgreSQL SERIAL）。
   a. 验收条件：[MySQL schema 含 AUTO_INCREMENT，PostgreSQL 含 SERIAL] → [diff 标注方言特定差异]
8. **禁止项**（EARS: Unwanted）
   如果 schema diff 可视化影响默认 feature 编译或引入不必要的依赖，则系统应当通过 `schema-diff-viz` feature gate 隔离，默认不启用可视化。
   a. 验收条件：[`cargo build` 默认编译] → [无可视化，行为与 v4.0.0 一致]

### 5.2.2 交互流程

```plantuml
@startuml
title schema diff 可视化 交互流程
actor "应用开发者" as app
participant "SchemaDiffVisualizer" as viz
participant "SchemaSync" as sync
participant "DdlGenerator" as ddl
database "DB" as db

app -> viz : migrate:diff --format=html
viz -> sync : 获取 entity schema
viz -> db : 获取当前 db schema
sync -> sync : 调用既有 diff() 计算
sync --> viz : SchemaDiff(表/列变更)
viz -> viz : 标注破坏性变更
viz -> ddl : 方言特定差异(5 方言)
ddl --> viz : 方言差异标注
viz -> viz : 渲染报告(html/markdown/cli)
viz --> app : 可视化 diff 报告 + 影响摘要
@enduml
```

### 5.2.3 异常场景

1. **schema 获取失败**
   a. 触发条件：数据库连接失败或 schema 读取权限不足
   b. 系统行为：提示获取失败原因，不输出空报告
   c. 用户感知：错误提示"schema fetch failed: connection refused"
2. **diff 计算异常**
   a. 触发条件：既有 `diff` 函数异常（不支持的类型/方言）
   b. 系统行为：降级为部分 diff，标注未计算部分
   c. 用户感知：报告标注"partial diff, unsupported type for column X"

## 5.3 缓存一致性协议（REQ-V41-003）

### 5.3.1 业务规则

1. **MESI 状态机**（EARS: Ubiquitous）
   系统应当为每个缓存行维护 MESI 风格状态机：(a) Modified（已修改，仅本实例有，与数据库不一致需写回）；(b) Exclusive（独占，仅本实例有，与数据库一致）；(c) Shared（共享，多实例有，与数据库一致）；(d) Invalid（失效，需重新加载），状态转换由读写/失效广播触发。
   a. 验收条件：[实例 A 写缓存 key1，状态 M；实例 B 读 key1，A 广播 Shared，B 状态 S] → [A 与 B 均为 Shared]
2. **跨实例失效广播**（EARS: Ubiquitous）
   系统应当通过消息队列（复用既有 `sz-orm-queue` 6 provider）广播缓存失效事件，某实例修改/删除缓存时广播 InvalidationEvent，其他实例收到后置对应缓存行为 Invalid。
   a. 验收条件：[实例 A 修改 key1，广播 Invalidation；实例 B 收到] → [B 的 key1 置 Invalid，下次读重新加载]
3. **write-through/write-behind**（EARS: Ubiquitous）
   系统应当支持两种写策略：(a) write-through（同步写穿，写缓存同时写数据库，强一致）；(b) write-behind（异步写回，先写缓存后异步写数据库，最终一致 + 失效广播），可配置。
   a. 验收条件：[配置 strategy=write-through，写 key1] → [缓存与数据库同步写入；strategy=write-behind] → [先写缓存，异步写数据库 + 广播失效]
4. **最终一致性保证**（EARS: Ubiquitous）
   缓存一致性协议须保证缓存与数据库最终一致，write-behind 模式下数据库写入失败须回滚缓存 + 告警，不出现陈旧缓存。
   a. 验收条件：[write-behind 模式，数据库写入失败] → [回滚缓存，告警"cache write-behind rollback, db write failed"]
5. **复用既有缓存**（EARS: Ubiquitous）
   系统应当复用既有 `MultiLevelCache`（`packages/sz-orm-core/src/cache.rs:141`）、`L1Cache`（`l1_cache.rs:87`）、`L2Cache`（`l2_cache.rs:517`）、`L1L2Coordinator`（`l1_cache.rs:216`），新增 `CacheCoherenceProtocol` 编排既有缓存，不修改既有缓存逻辑。
   a. 验收条件：[CacheCoherenceProtocol] → [调用既有 L1L2Coordinator 读写，不重复实现缓存]
6. **一致性指标可观测**（EARS: Ubiquitous）
   系统应当输出一致性指标（各状态缓存行数/失效广播次数/一致性违反次数/write-behind 回滚次数），接入既有 Prometheus（`sz-orm-observability`）。
   a. 验收条件：[启用缓存一致性] → [Prometheus 抓取 M/E/S/I 状态指标 + 失效广播计数]
7. **禁止项**（EARS: Unwanted）
   如果缓存一致性协议影响默认 feature 编译或运行时行为，则系统应当通过 `cache-coherence` feature gate 隔离，默认不启用一致性协议。
   a. 验收条件：[`cargo build` 默认编译] → [无一致性协议，行为与 v4.0.0 一致]

### 5.3.2 交互流程

```plantuml
@startuml
title 缓存一致性协议 交互流程
actor "运维/SRE" as sre
participant "CacheCoherenceProtocol" as protocol
participant "L1L2Coordinator" as cache
rectangle "sz-orm-queue\n(失效广播)" as mq
database "DB" as db

sre -> protocol : 配置 strategy=write-behind
loop 缓存读写
  alt 写缓存
    protocol -> cache : 写 L1+L2
    alt write-through
      protocol -> db : 同步写数据库
    else write-behind
      protocol -> db : 异步写数据库
      protocol -> mq : 广播 InvalidationEvent
      mq -> protocol : 其他实例收到失效
      protocol -> cache : 置 Invalid
    end
  else 读缓存
    protocol -> cache : 读 L1(状态检查)
    alt 状态 Valid
      cache --> protocol : 命中
    else 状态 Invalid
      protocol -> db : 加载
      db --> protocol : 数据
      protocol -> cache : 写入(状态 Exclusive)
    end
  end
end
protocol -> sre : 一致性指标(M/E/S/I + 广播计数)
@enduml
```

### 5.3.3 异常场景

1. **失效广播失败**
   a. 触发条件：消息队列不可用，失效广播发送失败
   b. 系统行为：本地置 Invalid，记录广播失败，重试或告警
   c. 用户感知：告警"cache invalidation broadcast failed, local invalidated, remote may stale"
2. **write-behind 数据库写入失败**
   a. 触发条件：write-behind 模式异步写数据库失败
   b. 系统行为：回滚缓存，告警，不保留陈旧缓存
   c. 用户感知：告警"cache write-behind rollback, db write failed for key X"
3. **脑裂检测**
   a. 触发条件：网络分区导致多实例同时 Modified 同一 key
   b. 系统行为：检测脑裂（多个 M 状态），最后写赢或人工解决
   c. 用户感知：告警"cache split-brain detected for key X, last-write-wins"

## 5.4 消息轨迹追踪（REQ-V41-004）

### 5.4.1 业务规则

1. **消息生产/消费 span**（EARS: Ubiquitous）
   系统应当为消息生产（publish）与消费（consume）创建追踪 span，span 含消息 ID/队列名/provider/消息大小/延迟，关联生产与消费 span（生产 span 为消费 span 的父 span）。
   a. 验收条件：[生产消息 msg1 到 Kafka topic1，消费 msg1] → [生产 span 与消费 span 关联，span 含 msg_id/topic/provider]
2. **trace context 注入/提取**（EARS: Ubiquitous）
   系统应当支持 trace context 注入到消息 header（生产时）与从消息 header 提取（消费时），兼容 W3C Trace Context（`traceparent`/`tracestate`）与 B3 propagation（`X-B3-TraceId`/`X-B3-SpanId`）。
   a. 验收条件：[生产消息，注入 W3C traceparent 到 header；消费消息] → [提取 traceparent，消费 span 关联到生产 trace]
3. **端到端轨迹关联**（EARS: Ubiquitous）
   系统应当关联消息端到端轨迹：生产→队列→消费→下游处理，形成完整 trace 链，可在 Jaeger/Tempo/Zipkin 可视化消息轨迹。
   a. 验收条件：[消息从生产经队列到消费再到下游 DB 写入] → [Jaeger 显示完整 trace：produce span → consume span → db span]
4. **采样率配置**（EARS: Optional）
   当配置采样率（如 10%）时，系统应当按采样率采样消息 span（降低追踪开销），采样率可动态调整，100% 时全量追踪。
   a. 验收条件：[配置采样率 10%，生产 1000 条消息] → [约 100 条有 span，900 条无 span]
5. **复用既有追踪器**（EARS: Ubiquitous）
   系统应当复用既有 `Tracer` trait（`packages/sz-orm-tracing/src/lib.rs:129`）、`SzTracer`（`:136`）、`OtelTracer`（`:387`）、`OtlpConfig`（`:2049`），不重复实现追踪，新增 `MessageTracingInterceptor` 拦截消息队列调用。
   a. 验收条件：[MessageTracingInterceptor] → [调用既有 OtelTracer 创建 span，不重复实现追踪]
6. **复用既有消息队列**（EARS: Ubiquitous）
   系统应当复用既有 `MessageQueue` trait（`packages/sz-orm-queue/src/queue.rs:18`）与 6 provider，拦截器包装既有 provider，不修改既有消息队列实现。
   a. 验收条件：[启用消息追踪，Kafka 生产/消费] → [拦截既有 real_kafka.rs，不修改既有实现]
7. **消息内容脱敏**（EARS: Optional）
   当启用脱敏（`sz-orm-masking`）时，span 属性中的消息内容须对敏感字段脱敏后再导出到 OTLP 后端。
   a. 验收条件：[消息含手机号，启用脱敏，导出到 Jaeger] → [Jaeger span 属性中手机号显示为 `138****8888`]
8. **禁止项**（EARS: Unwanted）
   如果消息轨迹追踪影响默认 feature 编译或运行时性能，则系统应当通过 `message-tracing` feature gate 隔离，默认不启用消息追踪。
   a. 验收条件：[`cargo build` 默认编译] → [无消息追踪，行为与 v4.0.0 一致]

### 5.4.2 交互流程

```plantuml
@startuml
title 消息轨迹追踪 交互流程
actor "运维/SRE" as sre
participant "MessageTracingInterceptor" as interceptor
participant "MessageQueue" as mq
participant "OtelTracer" as tracer
cloud "Jaeger/Tempo" as backend

sre -> interceptor : 配置采样率 + propagation=W3C
alt 生产消息
  interceptor -> tracer : 创建 produce span
  interceptor -> interceptor : 注入 traceparent 到 header
  interceptor -> mq : publish(msg, header)
end
alt 消费消息
  mq -> interceptor : consume(msg, header)
  interceptor -> interceptor : 提取 traceparent
  interceptor -> tracer : 创建 consume span(父=produce)
  interceptor -> tracer : span 含 msg_id/topic/延迟
end
tracer -> backend : OTLP 导出 span
backend --> sre : 消息轨迹可视化
@enduml
```

### 5.4.3 异常场景

1. **trace context 提取失败**
   a. 触发条件：消息 header 无 traceparent（生产端未注入或被剥离）
   b. 系统行为：创建新 trace（消费 span 无父 span），标记"no parent trace"
   c. 用户感知：Jaeger 显示消费 span 为根 span，标记"orphan consume"
2. **OTLP 导出失败**
   a. 触发条件：OTLP 后端不可用（Jaeger/Tempo 宕机）
   b. 系统行为：span 缓冲到本地（有界），后端恢复后重发，缓冲满则丢弃 + 告警
   c. 用户感知：告警"otlp export failed, span buffered/dropped"
3. **采样率过高影响性能**
   a. 触发条件：采样率 100% 且消息量大，span 创建开销影响吞吐
   b. 系统行为：告警建议降低采样率，不自动降级
   c. 用户感知：告警"message tracing sampling 100% may impact throughput, consider lowering"

## 5.5 存储生命周期管理（REQ-V41-005）

### 5.5.1 业务规则

1. **自动分层**（EARS: Ubiquitous）
   系统应当提供自动分层策略，按访问频率/年龄/大小将对象在 hot/warm/cold 存储层间迁移：hot（频繁访问，高性能存储）/warm（偶尔访问，标准存储）/cold（很少访问，归档存储），分层规则可配置。
   a. 验收条件：[对象 30 天未访问，配置 warm 阈值 30 天] → [自动迁移到 warm 层；90 天未访问，cold 阈值 90 天] → [迁移到 cold 层]
2. **过期清理**（EARS: Ubiquitous）
   系统应当提供过期清理（TTL 到期自动删除），TTL 按对象元数据/前缀/标签配置，删除前双重确认（TTL + 最后访问时间），不误删活跃对象。
   a. 验收条件：[对象 TTL=180 天，180 天后清理] → [删除对象；对象 180 天但最近访问过] → [不删除]
3. **生命周期策略配置**（EARS: Ubiquitous）
   系统应当提供 `LifecyclePolicy` 配置：分层规则（hot/warm/cold 阈值）、过期规则（TTL）、清理周期（每日/每周）、删除保护（保留期/软删除），策略按 bucket/prefix/tag 绑定。
   a. 验收条件：[配置 bucket=logs prefix=2024/ policy{TTL=365d, warm=30d, cold=90d}] → [logs/2024/ 下对象按策略分层与过期]
4. **分层迁移进度可观测**（EARS: Ubiquitous）
   系统应当输出分层迁移进度（已迁移对象数/剩余/预估完成时间/节省存储成本），可查询历史执行记录。
   a. 验收条件：[执行分层迁移，查询进度] → [返回已迁移 500/剩余 500/预估 10 分钟/节省 $100/月]
5. **复用既有存储**（EARS: Ubiquitous）
   系统应当复用既有 `Storage` trait（`packages/sz-orm-storage/src/storage.rs:14`）与 7 provider（S3/AliyunOSS/TencentCOS/QiniuKodo/HuaweiOBS/UpYun/Local），新增 `StorageLifecycleManager` 编排既有存储，不修改既有存储操作。
   a. 验收条件：[StorageLifecycleManager] → [调用既有 Storage trait 操作，不重复实现存储]
6. **删除保护**（EARS: Optional）
   当配置删除保护（保留期/软删除）时，系统应当不硬删受保护对象，软删除（标记删除但保留）或等待保留期过后再硬删。
   a. 验收条件：[对象配置保留期 90 天，TTL 到期] → [软删除标记，90 天后硬删]
7. **禁止项**（EARS: Unwanted）
   如果存储生命周期管理影响默认 feature 编译或运行时行为，则系统应当通过 `storage-lifecycle` feature gate 隔离，默认不启用生命周期管理。
   a. 验收条件：[`cargo build` 默认编译] → [无生命周期管理，行为与 v4.0.0 一致]

### 5.5.2 交互流程

```plantuml
@startuml
title 存储生命周期管理 交互流程
actor "运维/SRE" as sre
participant "StorageLifecycleManager" as manager
participant "Storage" as storage
rectangle "S3/AliyunOSS/\nTencentCOS/..." as providers

sre -> manager : 配置 LifecyclePolicy(hot/warm/cold/TTL)
loop 定期执行
  manager -> storage : 列举对象(含元数据)
  storage --> manager : 对象列表(访问时间/大小)
  manager -> manager : 评估分层/过期
  loop 每个对象
    alt 需分层迁移
      manager -> storage : 迁移到目标层
    else 需过期清理
      alt 删除保护
        manager -> storage : 软删除(标记)
      else 可硬删
        manager -> storage : 删除对象
      end
    end
  end
  manager -> sre : 执行报告(迁移数/清理数/节省成本)
end
@enduml
```

### 5.5.3 异常场景

1. **存储 provider 不可用**
   a. 触发条件：S3/OSS 等存储 provider 不可用
   b. 系统行为：暂停生命周期执行，告警，等待恢复后继续
   c. 用户感知：告警"storage provider unavailable, lifecycle paused"
2. **误删保护**
   a. 触发条件：对象被误判为过期（TTL 计算错误或元数据丢失）
   b. 系统行为：双重确认失败（最后访问时间近期），不删除，告警
   c. 用户感知：告警"object X TTL expired but recently accessed, skipped deletion"
3. **分层迁移失败**
   a. 触发条件：跨层迁移失败（目标层不可用/对象大）
   b. 系统行为：记录失败，重试或跳过，不影响其他对象
   c. 用户感知：执行报告标记"object X tier migration failed, reason Y"

## 5.6 数据质量自动检测（REQ-V41-006）

### 5.6.1 业务规则

1. **统计学规则引擎**（EARS: Ubiquitous）
   系统应当提供 `DataQualityEngine`，支持基于统计学的数据质量规则：(a) 缺失值检测（NULL 比例超阈值）；(b) 异常值检测（Z-Score/IQR/3σ 异常）；(c) 分布漂移检测（KL 散度/PSI，当前分布与基准分布差异）；(d) 唯一性检测（主键/唯一约束违反）；(e) 完整性检测（外键引用完整性）；(f) 一致性检测（跨表/跨字段逻辑一致）。
   a. 验收条件：[表 users 含 20% NULL email，规则"缺失值 email 阈值 10%"] → [检测失败，报告"email 缺失率 20% > 阈值 10%"]
2. **质量评分与报告**（EARS: Ubiquitous）
   系统应当输出数据质量评分（0-100 分，按规则通过率加权）与结构化报告（规则列表/通过率/评分/异常详情/趋势），可被 CI/工具解析。
   a. 验收条件：[10 条规则 8 通过 2 失败] → [评分 80 分，报告含失败规则详情]
3. **规则配置**（EARS: Ubiquitous）
   系统应当支持规则配置：每规则含名称/类型/表/字段/参数（阈值/基准分布）/严重级别（error/warning/info），规则按表/字段绑定，可从 YAML/JSON 加载。
   a. 验收条件：[配置规则 YAML 含缺失值+异常值+唯一性规则] → [加载后按规则检测，输出各规则结果]
4. **分布漂移检测**（EARS: Optional）
   当配置基准分布时，系统应当检测当前数据分布与基准分布的漂移（KL 散度/PSI），漂移超阈值告警，用于数据质量监控。
   a. 验收条件：[基准分布 age 均值 30，当前均值 45，PSI=0.3 > 阈值 0.1] → [告警"age distribution drift, PSI=0.3"]
5. **复用既有验证**（EARS: Ubiquitous）
   系统应当复用既有 `Validate` trait（`packages/sz-orm-core/src/validation/mod.rs:64`）与 `ValidationError`（`:16`），新增 `DataQualityEngine` 扩展为统计学规则引擎，不修改既有字段级验证。
   a. 验收条件：[DataQualityEngine] → [既有 Validate trait 保留，新增统计学规则扩展]
6. **只读检测**（EARS: Ubiquitous）
   数据质量检测须只读（不修改源数据），检测结果可选写入审计链（`sz-orm-audit`），保证检测不破坏数据。
   a. 验收条件：[执行数据质量检测] → [源数据不变，检测结果可选写入审计]
7. **五方言覆盖**（EARS: Ubiquitous）
   数据质量检测须覆盖五方言（MySQL/PostgreSQL/SQLite/Oracle/MSSQL），统计学计算通过 SQL 聚合（COUNT/AVG/STDDEV/PERCENTILE）按方言适配。
   a. 验收条件：[MySQL 与 PostgreSQL 同一规则检测] → [结果一致，SQL 按方言适配]
8. **禁止项**（EARS: Unwanted）
   如果数据质量检测影响默认 feature 编译或运行时行为，则系统应当通过 `data-quality` feature gate 隔离，默认不启用质量检测。
   a. 验收条件：[`cargo build` 默认编译] → [无数据质量检测，行为与 v4.0.0 一致]

### 5.6.2 交互流程

```plantuml
@startuml
title 数据质量自动检测 交互流程
actor "数据工程师" as data
participant "DataQualityEngine" as engine
database "DB" as db
participant "QualityReport" as report

data -> engine : detect(rules.yml, table=users)
engine -> engine : 加载规则(缺失值/异常值/唯一性/...)
loop 每条规则
  engine -> db : SQL 聚合统计(COUNT/AVG/STDDEV/...)
  db --> engine : 统计结果
  engine -> engine : 规则判定(阈值/分布)
end
engine -> report : 汇总结果(评分/通过率/异常)
engine -> data : 质量报告
note right of engine : 只读检测,不修改源数据
@enduml
```

### 5.6.3 异常场景

1. **规则配置错误**
   a. 触发条件：规则 YAML 格式错误或参数无效（阈值 < 0）
   b. 系统行为：跳过该规则，记录配置错误
   c. 用户感知：报告标记"rule X config invalid, skipped"
2. **统计计算超时**
   a. 触发条件：大表统计计算超时（> 配置阈值）
   b. 系统行为：该规则标记 TIMEOUT，其他规则正常
   c. 用户感知：报告标记"rule X timeout, statistics too slow"
3. **基准分布缺失**
   a. 触发条件：分布漂移检测但未配置基准分布
   b. 系统行为：跳过漂移检测，提示配置基准
   c. 用户感知：报告标记"drift detection skipped, no baseline distribution"

## 5.7 批量流式处理（REQ-V41-007）

### 5.7.1 业务规则

1. **Stream + Batch 结合**（EARS: Ubiquitous）
   系统应当提供 `StreamBatchProcessor`，结合流式读取（复用既有 `StreamApiExt`，`packages/sz-orm-core/src/stream_api.rs:50`）与批量写入（复用既有 `BatchOperations`，`packages/sz-orm-batch/src/lib.rs:40`），流式读取源数据 + 批量写入目标，内存有界（不一次性加载全量）。
   a. 验收条件：[源表 100 万行，批量大小 1000，执行流式批量处理] → [内存占用恒定（约 1000 行），写入 100 万行，吞吐 ≥50,000 行/秒]
2. **背压控制**（EARS: Ubiquitous）
   系统应当提供背压控制（`BackpressureController`），当写入速度慢于读取速度时，暂停/减慢读取，避免内存溢出，背压策略可配置（有界队列/丢弃/阻塞）。
   a. 验收条件：[写入慢于读取，背压策略=阻塞] → [读取暂停等待写入，内存不溢出]
3. **断点续传**（EARS: Ubiquitous）
   批量流式处理中断（网络/节点故障）须支持断点续传，记录已处理位点（主键/offset），恢复后从断点继续，不丢不重。
   a. 验收条件：[处理 50 万行时中断，恢复] → [从 50 万行断点继续，不重处理已处理行]
4. **进度可观测**（EARS: Ubiquitous）
   批量流式处理进度须可查询（已处理/剩余/吞吐/预估完成时间），可中止/恢复，进度接入既有 Prometheus。
   a. 验收条件：[处理中查询进度] → [返回已处理 50 万/剩余 50 万/吞吐 50,000 行/秒/预估 10 秒]
5. **复用既有批处理与流式 API**（EARS: Ubiquitous）
   系统应当复用既有 `BatchOperations` trait（`packages/sz-orm-batch/src/lib.rs:40`）与 `StreamApiExt`（`packages/sz-orm-core/src/stream_api.rs:50`）/`StreamQueryTrait`（`paginator.rs:273`），不重复实现批处理与流式查询。
   a. 验收条件：[StreamBatchProcessor] → [调用既有 StreamApiExt 流式读 + BatchOperations 批量写，不重复实现]
6. **多种源/目标**（EARS: Optional）
   当配置不同源/目标时，系统应当支持数据库→数据库、数据库→文件（CSV/JSON）、文件→数据库的流式批量转换。
   a. 验收条件：[源=MySQL users 表，目标=CSV 文件] → [流式读取 users + 批量写入 CSV]
7. **禁止项**（EARS: Unwanted）
   如果批量流式处理影响默认 feature 编译或运行时行为，则系统应当通过 `batch-stream` feature gate 隔离，默认不启用流式批量处理。
   a. 验收条件：[`cargo build` 默认编译] → [无流式批量处理，行为与 v4.0.0 一致]

### 5.7.2 交互流程

```plantuml
@startuml
title 批量流式处理 交互流程
actor "数据工程师" as data
participant "StreamBatchProcessor" as processor
participant "StreamApiExt" as stream
participant "BatchOperations" as batch
participant "BackpressureController" as backpressure
database "源 DB" as src
database "目标 DB" as dst

data -> processor : process(src=users, dst=users_copy, batch_size=1000)
loop 流式批量
  processor -> backpressure : 检查背压
  alt 无背压
    processor -> stream : 流式读取一批(1000 行)
    stream -> src : SELECT ... LIMIT 1000 OFFSET n
    src --> stream : 1000 行
    stream --> processor : 一批数据
    processor -> batch : 批量写入(1000 行)
    batch -> dst : INSERT ... VALUES (...),(...)
    batch --> processor : 写入结果
    processor -> processor : 更新位点 + 进度
  else 背压触发
    backpressure -> processor : 暂停读取
  end
end
processor --> data : 处理报告(总行数/吞吐/耗时)
@enduml
```

### 5.7.3 异常场景

1. **源/目标不可用**
   a. 触发条件：源数据库或目标数据库不可用
   b. 系统行为：暂停处理，告警，等待恢复后断点续传
   c. 用户感知：告警"batch stream paused, source/target unavailable"
2. **背压溢出**
   a. 触发条件：背压策略=有界队列，队列满
   b. 系统行为：按策略处理（阻塞/丢弃最旧/告警）
   c. 用户感知：告警"backpressure overflow, strategy=drop_oldest"
3. **位点持久化失败**
   a. 触发条件：位点持久化存储不可用
   b. 系统行为：暂停处理，告警，避免重启后丢数据
   c. 用户感知：告警"checkpoint store unavailable, processing paused"

## 5.8 迁移版本分支（REQ-V41-008）

### 5.8.1 业务规则

1. **分支管理**（EARS: Ubiquitous）
   系统应当提供 `MigrationBranchManager`，支持多分支并行开发迁移：每个分支独立迁移版本序列（如 `feature-a/001_create_users`、`feature-b/001_add_orders`），分支创建/切换/合并，分支间迁移隔离。
   a. 验收条件：[创建分支 feature-a，添加迁移 001_create_users；切换到 feature-b，添加迁移 001_add_orders] → [两分支迁移独立，互不影响]
2. **版本 DAG**（EARS: Ubiquitous）
   系统应当维护迁移版本 DAG（有向无环图），记录分支间迁移依赖关系，DAG 支持拓扑排序确定执行顺序，检测循环依赖。
   a. 验收条件：[分支 A 与 B 从 main 分出，各自迁移] → [DAG 记录 main→A、main→B 依赖，合并时拓扑排序]
3. **三方合并**（EARS: Ubiquitous）
   系统应当支持迁移三方合并（base 分支迁移 + 两个分支迁移 → 合并后迁移序列），合并时检测冲突（同名版本号/表结构冲突），冲突须人工解决，不自动合并。
   a. 验收条件：[分支 A 与 B 各加 001 迁移，合并] → [检测版本号冲突，提示人工解决；A 加 001 + B 加 002] → [自动合并，顺序 001→002]
4. **冲突检测与解决**（EARS: Ubiquitous）
   系统应当检测合并冲突：(a) 版本号冲突（两分支同版本号）；(b) 表结构冲突（两分支修改同一表不兼容）；(c) 依赖冲突（迁移依赖顺序矛盾），冲突须人工解决（选择/重命名/修改），不自动合并。
   a. 验收条件：[A 的 001 创建 users，B 的 001 也创建 users] → [冲突"both branches create users in 001"，提示人工解决]
5. **复用既有迁移**（EARS: Ubiquitous）
   系统应当复用既有 `Migrator`（`packages/sz-orm-core/src/migration.rs:276`）、`MigrationResolver`（`:62`）、`MigrationContext`（`:193`），新增 `MigrationBranchManager` 编排既有迁移，不修改既有迁移执行逻辑。
   a. 验收条件：[MigrationBranchManager] → [调用既有 Migrator 执行，不重复实现迁移]
6. **CLI 集成**（EARS: Ubiquitous）
   系统应当提供 CLI 命令 `sz-orm migrate:branch`（分支管理）、`sz-orm migrate:merge`（三方合并），输出合并报告/冲突详情。
   a. 验收条件：[执行 `sz-orm migrate:merge feature-a feature-b`] → [输出合并结果或冲突详情]
7. **DAG 可视化**（EARS: Ubiquitous）
   系统应当支持版本 DAG 导出为 DOT/JSON，可被 Graphviz 渲染可视化分支依赖关系。
   a. 验收条件：[导出 DAG 为 DOT] → [Graphviz 渲染为分支依赖图]
8. **禁止项**（EARS: Unwanted）
   如果迁移版本分支影响默认 feature 编译或运行时行为，则系统应当通过 `migration-branch` feature gate 隔离，默认不启用分支管理。
   a. 验收条件：[`cargo build` 默认编译] → [无分支管理，行为与 v4.0.0 一致]

### 5.8.2 交互流程

```plantuml
@startuml
title 迁移版本分支 交互流程
actor "应用开发者" as app
participant "MigrationBranchManager" as manager
participant "Migrator" as migrator
database "DB" as db

app -> manager : create_branch(feature-a)
app -> manager : add_migration(feature-a, 001_create_users)
app -> manager : create_branch(feature-b)
app -> manager : add_migration(feature-b, 001_add_orders)
app -> manager : merge(feature-a, feature-b)
manager -> manager : 三方合并(base + A + B)
manager -> manager : 冲突检测
alt 无冲突
  manager -> migrator : 执行合并后迁移序列
  migrator -> db : 按拓扑顺序执行
  migrator --> manager : 执行结果
  manager --> app : 合并成功报告
else 有冲突
  manager --> app : 冲突详情(版本号/表结构/依赖)
  app -> manager : 人工解决冲突
  manager -> migrator : 执行解决后迁移
end
@enduml
```

### 5.8.3 异常场景

1. **合并冲突**
   a. 触发条件：两分支迁移版本号冲突或表结构冲突
   b. 系统行为：阻止自动合并，输出冲突详情，等待人工解决
   c. 用户感知：提示"merge conflict: both branches have 001_create_users, manual resolution required"
2. **循环依赖检测**
   a. 触发条件：分支间迁移依赖形成循环
   b. 系统行为：检测循环，拒绝合并，提示循环依赖链
   c. 用户感知：错误提示"migration dependency cycle: A.001←B.001←A.001"
3. **分支不存在**
   a. 触发条件：合并/切换到不存在的分支
   b. 系统行为：提示分支不存在，列出可用分支
   c. 用户感知：错误提示"branch feature-x not found, available: feature-a, feature-b"

## 5.9 备份验证自动化（REQ-V41-009）

### 5.9.1 业务规则

1. **定期恢复验证**（EARS: Ubiquitous）
   系统应当提供 `BackupVerificationScheduler`，定期（每日/每周/可配置）自动执行恢复验证：从备份恢复到隔离临时实例 + 数据完整性检查 + 验证后清理，确保备份真实可用。
   a. 验收条件：[配置每周验证，到时间] → [自动从最新备份恢复到临时实例，执行完整性检查，输出验证报告，清理临时实例]
2. **数据完整性检查**（EARS: Ubiquitous）
   系统应当提供 `IntegrityChecker`，校验恢复数据完整性：(a) 行数比对（备份与恢复行数一致）；(b) checksum 校验（表 checksum 一致）；(c) 抽样比对（随机抽样数据比对）；(d) Schema 一致性（表结构/索引/约束一致），任一不一致标记验证失败。
   a. 验收条件：[备份 users 表 1000 行，恢复后 999 行] → [行数比对失败，标记验证失败]
3. **验证失败告警**（EARS: Ubiquitous）
   备份验证失败须告警通知（邮件/Slack/ webhook，复用既有告警），告警含备份 ID/失败原因/完整性检查详情，不误报成功。
   a. 验收条件：[验证失败，checksum 不一致] → [告警"backup X verification failed, checksum mismatch on table Y"]
4. **隔离恢复**（EARS: Ubiquitous）
   备份验证恢复须在隔离临时实例执行（不污染生产实例），验证数据验证后清理，避免残留。
   a. 验收条件：[执行备份验证] → [恢复到临时实例（隔离），验证后清理，生产实例不受影响]
5. **复用既有备份/恢复**（EARS: Ubiquitous）
   系统应当复用既有 `BackupManager`（`packages/sz-orm-back/src/backup.rs:87`）、`RestoreManager`（`restore.rs:8`）、`DisasterRecoveryDrill`（`lib.rs:75`），新增 `BackupVerificationScheduler` 编排既有备份/恢复，不修改既有备份/恢复逻辑。
   a. 验收条件：[BackupVerificationScheduler] → [调用既有 RestoreManager 恢复，不重复实现]
6. **验证报告**（EARS: Ubiquitous）
   系统应当输出验证报告（备份 ID/恢复耗时/完整性检查结果/异常详情/验证时间），可查询历史验证记录，接入既有审计。
   a. 验收条件：[验证完成] → [输出报告含备份 ID/耗时/行数比对/checksum/Schema 检查结果]
7. **五方言覆盖**（EARS: Ubiquitous）
   备份验证须覆盖五方言（MySQL/PostgreSQL/SQLite/Oracle/MSSQL），完整性检查按方言适配（如 MySQL checksum table vs PostgreSQL pg_checksums）。
   a. 验收条件：[MySQL 与 PostgreSQL 备份验证] → [均执行完整性检查，SQL 按方言适配]
8. **禁止项**（EARS: Unwanted）
   如果备份验证自动化影响默认 feature 编译或运行时行为，则系统应当通过 `backup-verify` feature gate 隔离，默认不启用自动验证。
   a. 验收条件：[`cargo build` 默认编译] → [无自动验证，行为与 v4.0.0 一致]

### 5.9.2 交互流程

```plantuml
@startuml
title 备份验证自动化 交互流程
actor "运维/SRE" as sre
participant "BackupVerificationScheduler" as scheduler
participant "RestoreManager" as restore
participant "IntegrityChecker" as checker
database "备份存储" as backup
database "临时实例\n(隔离)" as temp
database "源 DB" as src

sre -> scheduler : 配置验证周期(每周)
loop 定期验证
  scheduler -> backup : 获取最新备份
  backup --> scheduler : 备份文件
  scheduler -> restore : 恢复到临时实例(隔离)
  restore -> temp : 恢复数据
  temp --> restore : 恢复完成
  scheduler -> checker : 完整性检查
  checker -> src : 获取源数据(行数/checksum/Schema)
  checker -> temp : 获取恢复数据
  checker -> checker : 比对(行数/checksum/抽样/Schema)
  alt 完整性通过
    scheduler -> scheduler : 记录验证成功
  else 完整性失败
    scheduler -> sre : 告警(备份 ID/失败原因)
  end
  scheduler -> temp : 清理临时实例
  scheduler -> sre : 验证报告
end
@enduml
```

### 5.9.3 异常场景

1. **备份文件不可用**
   a. 触发条件：备份文件损坏或丢失
   b. 系统行为：验证失败，告警，标记备份不可用
   c. 用户感知：告警"backup X unavailable, file corrupted/missing"
2. **恢复失败**
   a. 触发条件：恢复到临时实例失败（磁盘满/权限不足/格式错误）
   b. 系统行为：验证失败，告警，清理临时资源
   c. 用户感知：告警"backup X restore failed, reason Y"
3. **临时实例资源不足**
   a. 触发条件：临时实例磁盘/内存不足
   b. 系统行为：验证失败，告警建议扩容临时实例
   c. 用户感知：告警"verification temp instance resource insufficient, scale up required"

---

# 6. 数据约束

## 6.1 需求项

1. **需求 ID**：唯一标识，格式 `REQ-V41-xxx`（xxx = 001~009），必填。
2. **需求名称**：人类可读名称，必填。
3. **优先级**：P0 / P1 / P2，必填。
4. **分类**：测试数据 / 迁移可视化 / 缓存治理 / 可观测 / 存储治理 / 数据治理 / 性能 / 并行开发 / 灾备，必填。
5. **EARS 分类**：Ubiquitous / Event-driven / State-driven / Optional / Unwanted，每条业务规则必填。
6. **验证方法**：可执行的验证命令或测试描述，必填。
7. **代码证据**：相关 file:line 引用，必填，遵循审计合规铁律。
8. **验收条件**：触发场景 → 预期行为，必填。
9. **状态**：PASS / FAIL / PENDING，必填。
10. **与 v4.0.0 兼容性**：feature gate 隔离 / 既有 API 保留 / 测试基线不回退，必填。

## 6.2 输出对象

1. **SeedReport**：种子执行报告（种子列表/执行顺序/行数/耗时/幂等标记/环境）。
2. **FakerGenerator**：faker 数据生成器（字段类型→生成器映射，语义化假数据）。
3. **FixtureTemplate**：fixture 模板（YAML/JSON，表名/字段值/记录数/关联引用）。
4. **DiffReport**：schema diff 可视化报告（格式 CLI/HTML/Markdown，变更详情，破坏性标注，影响摘要）。
5. **CacheCoherenceState**：缓存一致性状态（M/E/S/I 状态机，缓存行状态，失效广播计数）。
6. **InvalidationEvent**：缓存失效事件（key/实例 ID/时间戳/操作类型）。
7. **MessageTraceSpan**：消息轨迹 span（msg_id/topic/provider/延迟/trace_context）。
8. **LifecyclePolicy**：存储生命周期策略（分层规则 hot/warm/cold 阈值，TTL，清理周期，删除保护）。
9. **LifecycleExecutionReport**：生命周期执行报告（分层迁移数/过期清理数/节省存储成本）。
10. **QualityReport**：数据质量报告（规则列表/通过率/评分 0-100/异常详情/趋势）。
11. **QualityRule**：数据质量规则（名称/类型/表/字段/参数/严重级别）。
12. **StreamBatchProgress**：流式批量进度（已处理/剩余/吞吐/预估完成时间/位点）。
13. **MigrationBranch**：迁移分支（分支名/迁移版本序列/依赖 DAG）。
14. **MergeResult**：合并结果（合并后迁移序列/冲突详情/解决状态）。
15. **VerificationReport**：备份验证报告（备份 ID/恢复耗时/完整性检查结果/异常详情/验证时间）。
16. **IntegrityCheckResult**：完整性检查结果（行数比对/checksum/抽样比对/Schema 一致性，通过/失败）。

---

# 7. 需求追溯矩阵

| 需求编号 | 需求项 | 优先级 | 分类 | EARS 分类 | 验收条件（节选） | 现有代码证据 | 与 v4.0.0 兼容性 |
|---------|--------|--------|------|----------|----------------|-------------|----------------|
| REQ-V41-001 | 数据 seeding/fixture 管理 | P0 | 测试数据 | Ubiquitous/State-driven/Unwanted | faker 生成 + fixture 加载 + 版本管理 + 幂等 + 环境隔离 + CLI | `cli/src/main.rs:770` cmd_make_seeder、`:808` cmd_seed、`packages/sz-orm-core/src/mock.rs:63` MockConnection | `data-seeding` feature gate，既有 CLI seeder 保留 |
| REQ-V41-002 | schema diff 可视化 | P0 | 迁移可视化 | Ubiquitous/Optional/Unwanted | 可视化 diff + 破坏性标注 + 影响摘要 + 版本间对比 + 五方言 | `packages/sz-orm-core/src/schema_sync.rs:100` SchemaDiff、`:200` diff、`:612` SchemaSync、`:361` DdlGenerator（5 方言）、`cli/src/main.rs:1389` cmd_generate_schema | `schema-diff-viz` feature gate，既有 SchemaDiff/diff 保留 |
| REQ-V41-003 | 缓存一致性协议 | P1 | 缓存治理 | Ubiquitous/Unwanted | MESI 状态机 + 失效广播 + write-through/behind + 最终一致 | `packages/sz-orm-core/src/cache.rs:11` Cache、`:141` MultiLevelCache、`l1_cache.rs:87` L1Cache、`:216` L1L2Coordinator、`l2_cache.rs:517` L2Cache、`:1176` L2CacheBackend | `cache-coherence` feature gate，既有 L1/L2/Coordinator 保留 |
| REQ-V41-004 | 消息轨迹追踪 | P1 | 可观测 | Ubiquitous/Optional/Unwanted | 生产/消费 span + context 注入/提取 + 端到端关联 + 采样率 | `packages/sz-orm-queue/src/queue.rs:18` MessageQueue、`:57` Message、`:183` MqProvider、`packages/sz-orm-tracing/src/lib.rs:129` Tracer、`:136` SzTracer、`:387` OtelTracer、`:2049` OtlpConfig | `message-tracing` feature gate，既有 MessageQueue/Tracer 保留 |
| REQ-V41-005 | 存储生命周期管理 | P1 | 存储治理 | Ubiquitous/Optional/Unwanted | 自动分层 + 过期清理 + 生命周期策略 + 删除保护 | `packages/sz-orm-storage/src/storage.rs:14` Storage、`:22` StorageBuilder、`:287` StorageProvider、`lib.rs:83-98` 7 provider 导出 | `storage-lifecycle` feature gate，既有 Storage/7 provider 保留 |
| REQ-V41-006 | 数据质量自动检测 | P1 | 数据治理 | Ubiquitous/Optional/Unwanted | 统计学规则引擎 + 质量评分 + 分布漂移 + 只读 + 五方言 | `packages/sz-orm-core/src/validation/mod.rs:16` ValidationError、`:64` Validate、`:70` aggregate | `data-quality` feature gate，既有 Validate/ValidationError 保留 |
| REQ-V41-007 | 批量流式处理 | P2 | 性能 | Ubiquitous/Optional/Unwanted | Stream+Batch + 背压 + 断点续传 + 进度可观测 | `packages/sz-orm-batch/src/lib.rs:40` BatchOperations、`:16` BatchResult、`packages/sz-orm-core/src/stream_api.rs:50` StreamApiExt、`paginator.rs:273` StreamQueryTrait、`streaming_export/mod.rs:11` ExportConfig | `batch-stream` feature gate，既有 BatchOperations/StreamApiExt 保留 |
| REQ-V41-008 | 迁移版本分支 | P2 | 并行开发 | Ubiquitous/Unwanted | 分支管理 + 版本 DAG + 三方合并 + 冲突检测 + CLI | `packages/sz-orm-core/src/migration.rs:10` Migration、`:62` MigrationResolver、`:193` MigrationContext、`:276` Migrator、`:747` MigrationProgress、`migration_dry_run.rs:59` MigrationImpact | `migration-branch` feature gate，既有 Migrator/MigrationResolver 保留 |
| REQ-V41-009 | 备份验证自动化 | P2 | 灾备 | Ubiquitous/Unwanted | 定期恢复验证 + 完整性检查 + 失败告警 + 隔离恢复 + 五方言 | `packages/sz-orm-back/src/backup.rs:87` BackupManager、`:15` BackupManifest、`restore.rs:8` RestoreManager、`lib.rs:75` DisasterRecoveryDrill、`:52` DrillReport | `backup-verify` feature gate，既有 BackupManager/RestoreManager/Drill 保留 |

---

# 8. 验收标准总览

## 8.1 P0 类（最高优先级）

| 编号 | 验收标准 | 验证方法 |
|------|---------|---------|
| REQ-V41-001 | faker 生成 + fixture 加载 + 版本管理 + 幂等 + 环境隔离 + CLI 集成 + 复用既有 seeder | 定义模型生成 faker 数据验证语义正确；加载 fixture 验证依赖引用；重复执行验证幂等；production 环境验证拒绝 |
| REQ-V41-002 | 可视化 diff（CLI/HTML/Markdown）+ 破坏性标注 + 影响摘要 + 版本间对比 + 五方言 + 复用既有 SchemaDiff | 执行 schema diff 验证三种格式输出；DROP COLUMN 验证破坏性标注；版本间 diff 验证对比 |

## 8.2 P1 类（高优先级）

| 编号 | 验收标准 | 验证方法 |
|------|---------|---------|
| REQ-V41-003 | MESI 状态机 + 跨实例失效广播 + write-through/behind + 最终一致 + 复用既有缓存 | 多实例写缓存验证状态转换；失效广播验证其他实例 Invalid；write-behind 失败验证回滚 |
| REQ-V41-004 | 生产/消费 span + context 注入/提取 + 端到端关联 + 采样率 + 脱敏 + 复用既有追踪/队列 | 生产消费消息验证 span 关联；W3C/B3 验证 context 传播；Jaeger 验证端到端轨迹 |
| REQ-V41-005 | 自动分层 + 过期清理 + 生命周期策略 + 删除保护 + 进度可观测 + 复用既有存储 | 配置分层策略验证对象迁移；TTL 到期验证清理；最近访问对象验证不误删 |
| REQ-V41-006 | 统计学规则引擎 + 质量评分 + 分布漂移 + 只读 + 五方言 + 复用既有验证 | 配置规则验证检测；NULL 比例超阈值验证缺失值检测；分布漂移验证 PSI 告警 |

## 8.3 P2 类（中优先级）

| 编号 | 验收标准 | 验证方法 |
|------|---------|---------|
| REQ-V41-007 | Stream+Batch + 背压 + 断点续传 + 进度可观测 + 多源/目标 + 复用既有批处理/流式 | 100 万行验证流式批量处理内存有界；写入慢验证背压；中断验证断点续传 |
| REQ-V41-008 | 分支管理 + 版本 DAG + 三方合并 + 冲突检测 + CLI + DAG 可视化 + 复用既有迁移 | 多分支添加迁移验证隔离；合并验证三方合并；同版本号验证冲突检测 |
| REQ-V41-009 | 定期恢复验证 + 完整性检查（行数/checksum/抽样/Schema）+ 失败告警 + 隔离恢复 + 五方言 + 复用既有备份/恢复 | 定期验证自动执行；行数不一致验证完整性失败；验证失败验证告警；临时实例验证隔离 |

## 8.4 全局验收条件

1. **API 兼容性**：v4.1.0 既有公开 API 完全向后兼容，sz-pay 既有代码不受影响。
2. **feature gate 隔离**：所有新能力通过 feature gate 隔离（`data-seeding` / `schema-diff-viz` / `cache-coherence` / `message-tracing` / `storage-lifecycle` / `data-quality` / `batch-stream` / `migration-branch` / `backup-verify`），默认 feature 行为不变。
3. **测试基线不回退**：v4.0.0 已验收测试基线不回退，v4.1.0 仅增不减。
4. **五方言一致**：新增能力在 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 五方言上行为一致（schema diff/数据质量/备份验证/seeding 按方言能力适配）。
5. **审计证据**：每项需求结论附 file:line 证据，遵循 AGENTS.md 审计合规铁律。
6. **14 道门禁通过**：v4.1.0 须通过 AGENTS.md 定义的 14 道门禁（fmt/check/clippy/test/doc/audit/integration/占位检查/SQL 注入/feature 全组合/上游未改/文档一致/审计证据/文档同步）。
7. **无占位实现**：禁止 `todo!` / `unimplemented!` / `unreachable!`，所有新增代码须完整实现。
8. **unsafe 零容忍**：所有新增代码无 `unsafe` 块，或必须有 `// SAFETY:` 注释。
9. **复用优先**：优先复用既有能力，不重复实现（如 seeding 复用既有 CLI seeder，schema diff 复用既有 SchemaDiff/diff，缓存一致性复用既有 L1/L2/Coordinator，消息追踪复用既有 Tracer/MessageQueue，存储生命周期复用既有 Storage/7 provider，数据质量复用既有 Validate，批量流式复用既有 BatchOperations/StreamApiExt，迁移分支复用既有 Migrator，备份验证复用既有 BackupManager/RestoreManager）。
10. **依赖关系**：REQ-V41-004（消息轨迹）复用既有 `sz-orm-tracing` 与 `sz-orm-queue`；REQ-V41-003（缓存一致性）复用既有 `sz-orm-queue` 做失效广播；REQ-V41-009（备份验证）复用既有 `sz-orm-back`；其余需求相互独立，可并行开发。

## 8.5 需求依赖关系

```plantuml
@startuml
title v4.1.0 需求依赖关系图
REQ-V41-001 "数据 seeding" : 独立
REQ-V41-002 "schema diff 可视化" : 独立
REQ-V41-003 "缓存一致性" : 复用 sz-orm-queue(失效广播)
REQ-V41-004 "消息轨迹追踪" : 复用 sz-orm-tracing + sz-orm-queue
REQ-V41-005 "存储生命周期" : 独立
REQ-V41-006 "数据质量检测" : 独立
REQ-V41-007 "批量流式处理" : 独立
REQ-V41-008 "迁移版本分支" : 独立
REQ-V41-009 "备份验证自动化" : 独立
note right of REQ-V41-003 : 复用既有 sz-orm-queue\n做失效广播通道
note right of REQ-V41-004 : 复用既有 sz-orm-tracing\n+ sz-orm-queue
@enduml
```