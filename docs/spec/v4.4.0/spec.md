# sz-orm v4.4.0 需求规格说明书

> 版本：v4.4.0（查询自动优化建议 + 慢查询自动诊断报告 + db-fusion 转正 + 结构化查询日志 + 性能回归基准线 + 查询智能闭环联动）
> 基线：v4.3.0（编译期 EXPLAIN 分析 + 查询性能火焰图 + N+1 静态检测 + 数据血缘可视化 + 编译期数据治理 + 自适应查询 + 多数据库融合 POC，5 项需求 REQ-V43-001~005 全部通过 feature gate 隔离，已验收基线）
> 日期：2026-08-12
> 需求格式：EARS（Ubiquitous / Event-driven / State-driven / Optional / Unwanted）
> 文档定位：需求规格（What to build），不含技术设计（How to build）
> 优先级声明：六项需求按"P1（查询自动优化建议 + 慢查询自动诊断 + db-fusion 转正，查询智能从"检测"深化到"建议+诊断+转正"核心价值）→ P2（结构化查询日志 + 性能回归基准线，可观测性与 CI 集成增强）→ P3（查询智能闭环联动，跨能力协同可选验证）"序推进
> 需求编号约定：REQ-V44-xxx（v4.4.0 需求项，REQ-V44-001 ~ REQ-V44-006）
> 规划依据：`docs/spec/v4.3.0/` SDD 三阶段文档（development-plan 681 行 / spec 771 行 / design 1415 行 / tasks 596 行，26 任务 / 156 子任务全部已完成）+ `docs/评估/2026-08-12_db-fusion实验评估.md`（db-fusion POC 评估，推荐 v4.4.x 转正阶段二）+ 2026-08-12 逐项代码验证（file:line 均已实测存在）
> 兼容性铁律：所有新能力通过 feature gate 隔离，默认 feature 行为不变，既有公开 API 完全向后兼容，v4.3.0 已验收测试基线不回退；sz-pay 生产依赖（从 crates.io 拉取 sz-orm-* 6 个包）不得被破坏；五方言覆盖：MySQL/PostgreSQL/SQLite/Oracle/MSSQL
> 范围声明：本版本聚焦查询智能深化（从"采集/检测"到"建议/诊断/转正"）与可观测性增强；更长期（v4.x+ IDE 集成/LSP 协议/编译期数据库重写）在后续版本规划；本版本不涉及 crates.io 发布流程变更
> 边界声明：与 v4.3.0 零重叠（见第 1.4 节），v4.3.0 是"采集/检测"层（EXPLAIN 解析/火焰图采集/N+1 检测/血缘导出/治理标注/自适应决策/融合 POC），v4.4.0 是"分析/建议/转正"层（优化建议/诊断报告/融合转正/结构化日志/性能基线/闭环联动）

---

# 1. 组件定位

## 1.1 核心职责

本组件负责交付 sz-orm v4.4.0 的六项查询智能深化与可观测性增强能力：(1) 查询自动优化建议引擎（新增 `sz-orm-advisor` 包，基于既有 EXPLAIN 分析结果 `ExplainPlan` `packages/sz-orm-explain/src/lib.rs:76` + 执行计划回归 `PlanRegression` `packages/sz-orm-explain/src/regression.rs:69` + 自适应统计 `QueryStats` `packages/sz-orm-adaptive/src/stats.rs:11`，复用既有 AI 建议结构 `IndexSuggestion` `packages/sz-orm-ai/src/index_advisor.rs:71` / `RewriteSuggestion` `packages/sz-orm-ai/src/rewrite_advisor.rs:61` / `TuningSuggestion` `packages/sz-orm-ai/src/auto_tuning/mod.rs:71`，通过规则引擎生成可执行优化建议，无 AI 依赖）；(2) 慢查询自动诊断报告（新增 `sz-orm-diagnosis` 包，基于既有火焰图阶段耗时 `QueryPhaseTiming` `packages/sz-orm-flamegraph/src/collector.rs:39` + 自适应 slow 标记 `QueryOutcome.slow` `packages/sz-orm-adaptive/src/executor.rs:116`，分析慢查询根因并生成诊断报告）；(3) db-fusion 转正（扩展既有 `sz-orm-fusion` 包 `packages/sz-orm-fusion/src/plan.rs:21` + `executor.rs:63`，阶段一 TTL 缓存 + 失效广播复用既有 `InvalidationBus` `packages/sz-orm-core/src/l2_cache.rs:82` / `RedisPubSubInvalidationBus` `packages/sz-orm-core/src/dist_cache.rs:41`，阶段二 CDC 增量同步复用既有 `DialectCapturer` `packages/sz-orm-queue/src/cdc/capturer.rs:12` / `DownstreamSink` `packages/sz-orm-queue/src/cdc/downstream.rs:12`，真实向量搜索下推复用既有 `HybridSearcher` `packages/sz-orm-vector/src/hybrid_search/searcher.rs:30` / `FilterPushdown` `packages/sz-orm-vector/src/hybrid_search/pushdown.rs:6`）；(4) 结构化查询日志（扩展既有 `sz-orm-observability` `packages/sz-orm-observability/src/lib.rs:253` `MetricsRegistry` + 火焰图 `QueryPhaseTiming`，输出结构化查询日志）；(5) 性能回归基准线 + CI 自动比对（扩展既有 `PlanSnapshot` `packages/sz-orm-explain/src/regression.rs:23` + 火焰图 `QueryPhaseTiming`，建立耗时基线 + CI 自动性能比对）；(6) 查询智能闭环联动（可选，EXPLAIN 分析 → 自适应决策 → 火焰图诊断 → 优化建议闭环）。所有能力通过 feature gate 隔离，不破坏现有 API 兼容性与 v4.3.0 已验收基线。

## 1.2 核心输入

1. **v4.3.0 已验收基线**：编译期 EXPLAIN 分析 + 查询性能火焰图 + N+1 静态检测 + 数据血缘可视化 + 编译期数据治理 + 自适应查询优化器 + 多数据库融合 POC，7 项能力全部通过 feature gate 隔离，作为本版本基准。
2. **现有能力清单与缺口证据**：
   - **查询自动优化建议**：`packages/sz-orm-explain/src/lib.rs:76` `pub struct ExplainPlan`（执行计划摘要，含 `scan_type`/`table`/`index`/`rows`/`extra`）、`:91` `missing_index` 方法（缺失索引判断）、`packages/sz-orm-explain/src/regression.rs:69` `pub enum PlanRegression`（回归检测：`ScanTypeUpgrade`/`IndexLost`/`RowsGrowth`）、`packages/sz-orm-adaptive/src/stats.rs:11` `pub struct QueryStats`（运行时统计）、`:66` `should_paginate`（大结果集判断）、`:73` `should_cache`（热点查询判断）、`packages/sz-orm-ai/src/index_advisor.rs:71` `pub struct IndexSuggestion`（索引建议结构）、`packages/sz-orm-ai/src/rewrite_advisor.rs:61` `pub struct RewriteSuggestion`（查询改写建议）、`packages/sz-orm-ai/src/auto_tuning/mod.rs:71` `pub struct TuningSuggestion`（调优建议）。缺口：EXPLAIN 分析仅检测问题（全表扫描/缺失索引/回归），不生成可执行优化建议（如"添加索引 idx_xxx ON users(email)"、"改用游标分页"、"启用缓存"），既有 AI 建议结构存在但需 AI 离线闭环驱动，无轻量规则引擎。
   - **慢查询自动诊断**：`packages/sz-orm-flamegraph/src/collector.rs:11` `pub enum Phase`（查询阶段：Build/Bind/PoolAcquire/SqlExecute/ResultMap）、`:39` `pub struct QueryPhaseTiming`（阶段耗时）、`:61` `QueryTracer::trace_execute`（分阶段计时）、`packages/sz-orm-adaptive/src/executor.rs:116` `QueryOutcome.slow`（慢查询标记）、`:35` `AdaptiveConfig.slow_ms`（慢查询阈值）。缺口：火焰图仅采集各阶段耗时，不分析慢查询根因（PoolAcquire 耗时高 → 连接池不足；SqlExecute 耗时高 → SQL 优化；ResultMap 耗时高 → 结果集过大），无自动诊断报告。
   - **db-fusion 转正**：`packages/sz-orm-fusion/src/plan.rs:21` `pub struct FusionConfig`（融合配置）、`:104` `FusionPlanner`（规划器）、`packages/sz-orm-fusion/src/executor.rs:63` `pub struct FusionExecutor`（融合执行器）、`:93` `execute`（缓存命中跳过主库 + 降级回缓存）、`:16` `MemoryFusionCache`（进程内缓存，无 TTL）、`packages/sz-orm-core/src/l2_cache.rs:82` `pub trait InvalidationBus`（缓存失效总线）、`:93` `LocalInvalidationBus`（本地失效总线）、`packages/sz-orm-core/src/dist_cache.rs:41` `RedisPubSubInvalidationBus`（Redis Pub/Sub 失效总线）、`:179` `GossipInvalidationBus`（Gossip 失效总线）、`packages/sz-orm-queue/src/cdc/capturer.rs:12` `pub trait DialectCapturer`（方言 CDC 捕获器，5 方言）、`packages/sz-orm-queue/src/cdc/downstream.rs:12` `DownstreamSink` trait（下游分发）、`:178` `distribute_to_all`（并行分发）、`packages/sz-orm-vector/src/hybrid_search/searcher.rs:30` `pub struct HybridSearcher`（三源并行混合搜索）、`packages/sz-orm-vector/src/hybrid_search/pushdown.rs:6` `FilterPushdown`（结构化过滤下推）。缺口：POC 缓存无 TTL + 无失效广播，搜索下推仅记录数据源不真正执行向量检索，无 CDC 增量同步集成。
   - **结构化查询日志**：`packages/sz-orm-observability/src/lib.rs:253` `pub struct MetricsRegistry`（指标注册中心，Counter/Gauge/Histogram）、`:421` `start_metrics_server`（Prometheus HTTP server）、`packages/sz-orm-flamegraph/src/collector.rs:39` `QueryPhaseTiming`（阶段耗时）。缺口：仅有 Prometheus 指标导出，无结构化查询日志（JSON 格式含查询 SQL/参数/耗时/阶段/慢标记），无日志采样与级别控制。
   - **性能回归基准线**：`packages/sz-orm-explain/src/regression.rs:23` `pub struct PlanSnapshot`（执行计划基线快照）、`:34` `PlanBaseline`（基线集合）、`:161` `check_regressions`（CI 入口）、`packages/sz-orm-flamegraph/src/collector.rs:39` `QueryPhaseTiming`（阶段耗时）。缺口：仅有执行计划回归检测（EXPLAIN 基线），无耗时基线（如 Build 阶段基线 5ms，当前 50ms → 性能退化），无 CI 自动性能比对。
   - **查询智能闭环联动**：`packages/sz-orm-explain/src/lib.rs:76` `ExplainPlan`、`packages/sz-orm-adaptive/src/executor.rs:157` `decide`（自适应决策）、`packages/sz-orm-flamegraph/src/collector.rs:61` `trace_execute`（火焰图采集）。缺口：三项能力独立工作，无闭环联动（EXPLAIN 检测全表扫描 → 自适应决策建议分页 → 火焰图诊断验证 → 优化建议生成）。
3. **本机数据库连接信息**：MySQL 9.6（`mysql://root:test123@127.0.0.1:3306/sz_orm_test`）、PostgreSQL 18（`postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`）、Oracle 23ai Free（`127.0.0.1:1521/freepdb1`）。
4. **sz-pay 生产依赖证据**：sz-pay 从 crates.io 拉取 sz-orm-* 6 个包，作为 API 兼容性验证的下游基准。
5. **五方言覆盖约束**：MySQL/PostgreSQL/SQLite/Oracle/MSSQL，优化建议/诊断报告/融合查询须覆盖全部方言（按方言能力适配）。
6. **既有 feature gate 体系**：`packages/sz-orm-core/Cargo.toml` 已有 25+ feature，v4.2.0 新增 7 个 feature，v4.3.0 新增 7 个 feature（`explain-analyzer` / `query-flamegraph` / `n1-lint` / `lineage-viz` / `compile-governance` / `adaptive-query` / `db-fusion`），作为新能力 feature gate 隔离的基础。
7. **db-fusion POC 评估报告**：`docs/评估/2026-08-12_db-fusion实验评估.md`（90 行，12 测试通过，推荐转正两阶段：阶段一 v4.3.x TTL + 失效广播，阶段二 v4.4.x CDC + 真实向量下推）。

## 1.3 核心输出

1. **查询自动优化建议引擎**：sz-orm-advisor（新包，`OptimizationAdvisor` 规则引擎 + `OptimizationSuggestion` 建议结构 + `suggest` 建议生成入口）+ JSON/人类可读建议报告（CI 可消费）。
2. **慢查询自动诊断报告**：sz-orm-diagnosis（新包，`SlowQueryDiagnoser` 诊断器 + `DiagnosisReport` 报告结构 + `diagnose` 诊断入口）+ JSON/人类可读诊断报告（含根因分析 + 优化建议）。
3. **db-fusion 转正**：sz-orm-fusion 扩展（TTL 缓存 `TtlFusionCache` + 失效广播适配 `InvalidationBus` + CDC 增量同步集成 `CdcSyncCoordinator` + 真实向量搜索下推 `VectorPushdownExecutor`）+ 转正 API（正式非实验标注）。
4. **结构化查询日志**：sz-orm-observability 扩展（`QueryLogger` 结构化日志器 + `QueryLogEntry` 日志结构 + JSON 格式输出 + 采样与级别控制）。
5. **性能回归基准线**：sz-orm-explain 扩展（`PerfBaseline` 耗时基线 + `PerfRegression` 性能回归 + `check_perf_regressions` CI 入口）+ sz-orm-flamegraph 联动（`QueryPhaseTiming` 基线）。
6. **查询智能闭环联动**：sz-orm-advisor 扩展（`IntelligenceLoop` 闭环协调器 + `LoopReport` 闭环报告）+ 跨包联动（explain → adaptive → flamegraph → advisor）。
7. **需求追溯矩阵**：本文档第 7 章，建立需求 ↔ 验收条件映射。
8. **验收标准总览**：本文档第 8 章，按需求项汇总验收条件。

## 1.4 职责边界

本组件**不负责**以下事项：

1. **不破坏既有公开 API**：所有新能力通过 feature gate 隔离，既有公开 API 签名保持完全向后兼容。
2. **不改变既有安全铁律**：任何 WHERE 条件必须参数化，默认禁止 `SELECT *`，N+1 检测自动拦截，沿用既有铁律。
3. **不重写 ORM 核心**：`QueryBuilder`（`packages/sz-orm-core/src/query.rs:36`）运行时构造、`Model::table_name()` 运行时返回，不前移到编译期。
4. **不替换既有 EXPLAIN 分析**：既有 `ExplainPlan`（`packages/sz-orm-explain/src/lib.rs:76`）与 `PlanRegression`（`packages/sz-orm-explain/src/regression.rs:69`）保留，新增优化建议引擎为独立包 `sz-orm-advisor`，复用既有分析结果不重复实现。
5. **不替换既有火焰图采集**：既有 `QueryPhaseTiming`（`packages/sz-orm-flamegraph/src/collector.rs:39`）与 `QueryTracer::trace_execute`（`:61`）保留，新增慢查询诊断为独立包 `sz-orm-diagnosis`，复用既有阶段耗时不重复实现。
6. **不替换既有自适应决策**：既有 `AdaptiveExecutor`（`packages/sz-orm-adaptive/src/executor.rs:120`）与 `QueryStats`（`packages/sz-orm-adaptive/src/stats.rs:11`）保留，新增优化建议引擎复用既有统计与决策，不修改既有自适应逻辑。
7. **不替换既有 AI 离线优化**：既有 `AutoTuningPipeline`（`packages/sz-orm-ai/src/auto_tuning/pipeline.rs:15`）AI 离线闭环保留，新增优化建议引擎为规则引擎（无 AI 依赖），复用既有建议结构（`IndexSuggestion`/`RewriteSuggestion`/`TuningSuggestion`）不替换 AI 优化。
8. **不替换既有缓存失效总线**：既有 `InvalidationBus`（`packages/sz-orm-core/src/l2_cache.rs:82`）/ `RedisPubSubInvalidationBus`（`packages/sz-orm-core/src/dist_cache.rs:41`）/ `GossipInvalidationBus`（`:179`）保留，db-fusion 转正复用既有失效总线不重复实现。
9. **不替换既有 CDC 捕获器**：既有 `DialectCapturer`（`packages/sz-orm-queue/src/cdc/capturer.rs:12`）与 `DownstreamSink`（`packages/sz-orm-queue/src/cdc/downstream.rs:12`）保留，db-fusion 转正复用既有 CDC 基础设施不重复实现。
10. **不替换既有向量搜索**：既有 `HybridSearcher`（`packages/sz-orm-vector/src/hybrid_search/searcher.rs:30`）与 `FilterPushdown`（`packages/sz-orm-vector/src/hybrid_search/pushdown.rs:6`）保留，db-fusion 转正复用既有向量搜索不重复实现。
11. **不替换既有 Prometheus 指标**：既有 `MetricsRegistry`（`packages/sz-orm-observability/src/lib.rs:253`）与 `start_metrics_server`（`:421`）保留，新增结构化查询日志为扩展，不修改既有指标导出。
12. **不与 v4.3.0 任务重叠**：v4.3.0 已占用的包/模块（`sz-orm-explain` 解析器 / `sz-orm-flamegraph` 采集器 / `sz-orm-n1-lint` 检测器 / `sz-orm-audit` 血缘 / `sz-orm-core` 治理 / `sz-orm-adaptive` 决策器 / `sz-orm-fusion` POC）本版本仅做"分析/建议/转正"层扩展，不修改既有"采集/检测"层逻辑。
13. **不负责 sz-pay / sz-rust 下游代码修改**：ADR-0001 严禁修改下游/上游仓库，仅保证 API 兼容性。
14. **不降低既有测试覆盖**：v4.4.0 不得使 v4.3.0 已验收测试基线回退，仅增不减。
15. **不引入 unsafe**：所有新增代码无 `unsafe` 块，或必须有 `// SAFETY:` 注释，沿用既有 unsafe 零容忍铁律。
16. **不引入 Breaking Change**：新能力通过 feature gate 隔离，默认全关闭，既有 feature 组合行为不变。
17. **不强制启用新能力**：所有新能力默认关闭或可选启用，避免无配置环境行为变化。
18. **不做 IDE 集成 / LSP 协议**：IDE 集成需从零实现 LSP server，无既有代码基础，"自嗨"风险高，排除出本版本范围。

---

# 2. 领域术语

**查询自动优化建议（Query Optimization Advice）**
: 基于既有 EXPLAIN 分析结果（`ExplainPlan` `packages/sz-orm-explain/src/lib.rs:76`）与执行计划回归（`PlanRegression` `packages/sz-orm-explain/src/regression.rs:69`），通过规则引擎（无 AI 依赖）生成可执行的优化建议（添加索引/改用分页/启用缓存/查询改写），复用既有 AI 建议结构（`IndexSuggestion` `packages/sz-orm-ai/src/index_advisor.rs:71` / `RewriteSuggestion` `packages/sz-orm-ai/src/rewrite_advisor.rs:61` / `TuningSuggestion` `packages/sz-orm-ai/src/auto_tuning/mod.rs:71`）。
: 备注：既有 AI 离线优化（`AutoTuningPipeline` `packages/sz-orm-ai/src/auto_tuning/pipeline.rs:15`）需 AI 训练，本版本补轻量规则引擎（无 AI 依赖）。

**OptimizationSuggestion（优化建议统一结构）**
: 规则引擎生成的优化建议（`suggestion_type` / `target_query` / `description` / `action` / `confidence` / `estimated_improvement`），`SuggestionType` 枚举（AddIndex/DropIndex/UsePagination/EnableCache/RewriteQuery/AdjustPoolSize）抽象各类建议。

**慢查询自动诊断报告（Slow Query Diagnosis Report）**
: 基于既有火焰图阶段耗时（`QueryPhaseTiming` `packages/sz-orm-flamegraph/src/collector.rs:39`）与自适应 slow 标记（`QueryOutcome.slow` `packages/sz-orm-adaptive/src/executor.rs:116`），分析慢查询根因（PoolAcquire 耗时高 → 连接池不足；SqlExecute 耗时高 → SQL 优化；ResultMap 耗时高 → 结果集过大；Build 耗时高 → 查询构造优化），生成诊断报告（JSON + 人类可读格式）。
: 备注：既有火焰图（`QueryTracer::trace_execute` `packages/sz-orm-flamegraph/src/collector.rs:61`）仅采集耗时，本版本补根因分析与诊断报告。

**DiagnosisReport（诊断报告结构）**
: 慢查询诊断结果（`query_key` / `total_elapsed_ms` / `root_cause` / `phase_breakdown` / `suggestions` / `severity`），`RootCause` 枚举（PoolExhaustion/SqlInefficiency/LargeResultSet/BuildOverhead/MixedCause）抽象根因类型。

**db-fusion 转正（DB Fusion Promotion）**
: 将 v4.3.0 的 `sz-orm-fusion` POC（`packages/sz-orm-fusion/src/plan.rs:21` `FusionConfig` + `executor.rs:63` `FusionExecutor`）转正为正式能力：阶段一 TTL 缓存 + 失效广播（复用既有 `InvalidationBus` `packages/sz-orm-core/src/l2_cache.rs:82`），阶段二 CDC 增量同步集成（复用既有 `DialectCapturer` `packages/sz-orm-queue/src/cdc/capturer.rs:12`）+ 真实向量搜索下推（复用既有 `HybridSearcher` `packages/sz-orm-vector/src/hybrid_search/searcher.rs:30`）。
: 备注：v4.3.0 POC 评估报告（`docs/评估/2026-08-12_db-fusion实验评估.md`）推荐 v4.4.x 转正阶段二。

**TTL 融合缓存（TTL Fusion Cache）**
: 在既有 `MemoryFusionCache`（`packages/sz-orm-fusion/src/executor.rs:16`，无过期）基础上增加 TTL 过期 + 失效广播（适配既有 `InvalidationBus` `packages/sz-orm-core/src/l2_cache.rs:82`），保证缓存一致性。

**CDC 增量同步集成（CDC Incremental Sync Integration）**
: 将既有 `DialectCapturer`（`packages/sz-orm-queue/src/cdc/capturer.rs:12`，5 方言 CDC 捕获器）与 `DownstreamSink`（`packages/sz-orm-queue/src/cdc/downstream.rs:12`，7 种下游分发）集成到融合查询，主库变更自动同步到缓存/搜索索引。

**真实向量搜索下推（Real Vector Search Pushdown）**
: 将 v4.3.0 POC 的"搜索下推仅记录数据源"（`packages/sz-orm-fusion/src/executor.rs:118`）转正为真实向量检索，复用既有 `HybridSearcher`（`packages/sz-orm-vector/src/hybrid_search/searcher.rs:30`）三源并行查询 + `FilterPushdown`（`packages/sz-orm-vector/src/hybrid_search/pushdown.rs:6`）结构化过滤下推。

**结构化查询日志（Structured Query Log）**
: 基于 `MetricsRegistry`（`packages/sz-orm-observability/src/lib.rs:253`）+ `QueryPhaseTiming`（`packages/sz-orm-flamegraph/src/collector.rs:39`），输出结构化查询日志（JSON 格式，含查询 SQL/参数/耗时/阶段/慢标记），支持采样率与日志级别控制。

**性能回归基准线（Performance Regression Baseline）**
: 扩展既有执行计划基线（`PlanSnapshot` `packages/sz-orm-explain/src/regression.rs:23`）为耗时基线（`PerfBaseline`，含各阶段耗时基线），CI 中自动比对当前耗时与基线，检出性能退化（如 Build 阶段基线 5ms，当前 50ms → `PerfRegression`）。

**查询智能闭环联动（Query Intelligence Loop）**
: 可选能力，将 EXPLAIN 分析 → 自适应决策 → 火焰图诊断 → 优化建议四步联动形成闭环（EXPLAIN 检测全表扫描 → 自适应决策建议分页 → 火焰图诊断验证 → 优化建议生成），输出闭环报告。

**v4.4.0 feature gate**
: 控制本版本新能力的 feature gate 集合（`query-advisor` / `slow-query-diagnosis` / `db-fusion-v2` / `query-logging` / `perf-baseline` / `query-intelligence-loop`），默认关闭，避免无配置环境行为变化。

---

# 3. 角色与边界

## 3.1 核心角色

- **ORM 库维护者**：执行 v4.4.0 六项查询智能深化与可观测性增强能力的开发、验证、测试操作者，是新增能力的主要使用者与验收人。
- **下游项目开发者（sz-pay）**：关注 API 兼容性的下游使用者，v4.4.0 不得破坏其既有代码。
- **应用开发者**：使用查询自动优化建议、慢查询诊断报告在开发期发现并解决查询性能问题，是查询智能深化能力的主要使用者。
- **DBA / 性能工程师**：使用性能回归基准线、慢查询诊断报告、结构化查询日志监控查询性能退化与根因。
- **运维/SRE 工程师**：使用结构化查询日志、性能回归基准线部署可观测性增强能力，使用 db-fusion 转正能力部署多数据库融合查询。
- **CI/CD 流水线**：消费性能回归基准线、优化建议 JSON、诊断报告、结构化查询日志，集成到 CI 门禁。

## 3.2 外部系统

- **MySQL 9.6 / PostgreSQL 18 / SQLite / Oracle 23ai / MSSQL**：优化建议/诊断报告/融合查询的五方言覆盖目标。
- **Redis（缓存后端 + 失效广播）**：db-fusion 转正的缓存后端与失效广播通道（复用既有 `RedisPubSubInvalidationBus` `packages/sz-orm-core/src/dist_cache.rs:41`）。
- **向量库（pgvector 等）**：db-fusion 转正的搜索后端（复用既有 `HybridSearcher` `packages/sz-orm-vector/src/hybrid_search/searcher.rs:30`）。
- **CDC 下游（Kafka/RabbitMq/Nats/Pulsar/RocketMq/ActiveMq/HttpWebhook）**：db-fusion 转正的 CDC 增量同步下游（复用既有 `DownstreamSink` `packages/sz-orm-queue/src/cdc/downstream.rs:12`）。
- **Prometheus / Grafana**：结构化查询日志与性能指标的下游消费方。
- **sz-pay 项目**：API 兼容性验证的下游基准。

## 3.3 交互上下文

```plantuml
@startuml
title sz-orm v4.4.0 六项查询智能深化与可观测性增强能力 交互上下文

rectangle "sz-orm v4.4.0" as orm {
  rectangle "查询自动优化建议引擎" as advisor
  rectangle "慢查询自动诊断报告" as diagnosis
  rectangle "db-fusion 转正\n(TTL+CDC+向量下推)" as fusion
  rectangle "结构化查询日志" as logging
  rectangle "性能回归基准线" as perf
  rectangle "查询智能闭环联动\n(可选)" as loop
}

actor "ORM 库维护者" as dev
actor "应用开发者" as appdev
actor "DBA/性能工程师" as dba
actor "运维/SRE" as sre
actor "sz-pay 开发者" as downstream
actor "CI/CD 流水线" as ci

database "MySQL/PG/SQLite\n/Oracle/MSSQL" as db
rectangle "Redis\n(缓存+失效广播)" as redis
rectangle "向量库\n(pgvector)" as vector
rectangle "CDC 下游\n(Kafka/...)" as cdc
rectangle "Prometheus/Grafana" as prom

dev --> orm : 开发/验证/测试
appdev --> advisor : 优化建议(添加索引/分页/缓存)
advisor --> db : 复用 EXPLAIN 分析结果
appdev --> diagnosis : 慢查询诊断报告
diagnosis --> db : 复用火焰图阶段耗时
dba --> perf : 性能回归基准线
perf --> db : 耗时基线+CI 比对
sre --> logging : 结构化查询日志
logging --> prom : JSON 日志+指标
sre --> fusion : db-fusion 转正
fusion --> redis : TTL 缓存+失效广播
fusion --> vector : 真实向量搜索下推
fusion --> cdc : CDC 增量同步
fusion --> db : 主库查询
ci --> perf : 性能回归检测
ci --> advisor : 优化建议 JSON
ci --> diagnosis : 诊断报告 JSON
ci --> logging : 查询日志
downstream --> orm : API 兼容性验证
loop --> advisor : 闭环联动(可选)
loop --> diagnosis : 闭环联动(可选)

@enduml
```

---

# 4. DFX约束

## 4.1 性能

1. **优化建议生成开销**：规则引擎生成优化建议开销不超过 10ms（含 EXPLAIN 分析结果解析 + 规则匹配 + 建议生成，单查询），不阻断查询执行。
2. **慢查询诊断开销**：慢查询诊断报告生成开销不超过 50ms（含阶段耗时分析 + 根因判定 + 建议生成 + 报告序列化），仅对慢查询触发（非每次查询）。
3. **db-fusion TTL 缓存开销**：TTL 缓存查找/写入开销不超过 1ms（含 TTL 过期检查 + 缓存键构造），失效广播开销不超过 5ms（含消息序列化 + 总线发布）。
4. **db-fusion CDC 同步开销**：CDC 增量同步开销不超过 100ms（含变更事件捕获 + 下游分发，单事件），不影响主库查询性能。
5. **db-fusion 向量下推开销**：真实向量搜索下推开销不超过 200ms（含 `HybridSearcher` 三源并行查询 + 结果融合，top_k ≤ 100）。
6. **结构化查询日志开销**：结构化查询日志写入开销不超过 100μs/次（含日志结构构造 + JSON 序列化 + 采样判定），采样率可配（默认 1%，慢查询 100%）。
7. **性能回归基线比对开销**：CI 中性能基线比对开销不超过 5 秒（含基线加载 + 当前耗时对比 + 回归检测，基线查询数 ≤ 1,000）。
8. **闭环联动开销**：查询智能闭环联动开销不超过 200ms（含 EXPLAIN 分析 + 自适应决策 + 火焰图诊断 + 优化建议，单查询），仅可选 feature 启用时触发。

## 4.2 可靠性

1. **优化建议保守性**：优化建议须保守（仅基于已验证的 EXPLAIN 分析结果与统计，不猜测），建议附带 `confidence`（置信度）与 `estimated_improvement`（预估改善），低置信度建议标注"需人工确认"。
2. **诊断报告降级**：火焰图阶段耗时缺失时，诊断报告须降级为"仅总耗时 + 未知根因"，不 panic，不阻断查询。
3. **db-fusion TTL 缓存一致性**：TTL 缓存须保证最终一致性（TTL 过期 + 失效广播双保险），主库失败回退缓存须返回降级标记（复用既有 `FusionOutcome.degraded` `packages/sz-orm-fusion/src/executor.rs:55`），不静默返回脏数据。
4. **db-fusion CDC 容错**：CDC 增量同步失败须降级为 TTL 过期兜底（复用既有 `CdcCheckpoint` `packages/sz-orm-queue/src/cdc/checkpoint.rs` 断点续传），不阻断主库查询。
5. **db-fusion 向量下推降级**：向量搜索下推失败须降级为主库查询（复用既有 `HybridSearcher` 降级语义 `packages/sz-orm-vector/src/hybrid_search/searcher.rs:60` `DegradationStatus`），不返回错误。
6. **结构化查询日志不阻断**：日志写入失败须降级为静默丢弃（不阻断查询），日志级别低于阈值时不输出。
7. **性能基线版本化**：`PerfBaseline` 须 JSON 序列化，可版本化管理，CI 中对比基线与当前耗时检出回归。
8. **闭环联动非强依赖**：闭环联动须可选（`query-intelligence-loop` feature），任一环节失败须降级为独立工作，不阻断查询。
9. **v4.3.0 测试基线不回退**：v4.4.0 不得使 v4.3.0 已验收测试基线回退，仅增不减。

## 4.3 安全性

1. **优化建议不执行 DDL**：优化建议仅生成建议文本（如"添加索引 idx_xxx ON users(email)"），不自动执行 DDL，需人工确认后执行。
2. **诊断报告不泄露参数**：慢查询诊断报告须脱敏查询参数（复用既有 `sz-orm-masking` `MaskingRule` `packages/sz-orm-masking/src/lib.rs:21`），不暴露生产参数值。
3. **结构化查询日志脱敏**：结构化查询日志须脱敏查询参数（复用既有 `sz-orm-masking`），日志级别可配（DEBUG 含 SQL/参数，INFO 仅含统计，WARN 仅含慢查询）。
4. **db-fusion 凭据隔离**：db-fusion 转正各后端凭据独立配置，不交叉泄露，CDC 同步不暴露主库凭据到缓存/搜索端。
5. **db-fusion CDC 不泄露变更**：CDC 增量同步须尊重既有脱敏规则（`packages/sz-orm-queue/src/cdc/masking.rs`），敏感字段变更事件脱敏后分发。
6. **审计证据要求**：每项需求结论须附 file:line 证据，遵循 AGENTS.md 审计合规铁律。

## 4.4 可维护性

1. **优化建议可消费**：优化建议须 JSON 输出（含建议类型/目标查询/描述/动作/置信度/预估改善），可被 CI/IDE 消费。
2. **诊断报告可消费**：诊断报告须 JSON + 人类可读双格式输出（含根因/阶段分解/建议/严重度），可被 CI/运维工具消费。
3. **db-fusion 转正 API 稳定**：db-fusion 转正后 API 须标注 `#[stable]`（非 `#[experimental]`），向后兼容，废弃 POC API 须 `#[deprecated]` 标注并提供迁移指引。
4. **结构化查询日志可配置**：日志采样率、日志级别、输出格式（JSON/文本）须可配置，不强制干扰开发者。
5. **性能基线版本化管理**：`PerfBaseline` 须 JSON 序列化，可版本化管理，CI 中对比基线与当前耗时检出回归。
6. **闭环联动可观测**：闭环联动须输出闭环报告（含各环节耗时/结果/建议），可被 CI/运维工具消费。
7. **五方言一致**：新增能力在 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 五方言上行为一致（优化建议/诊断报告/融合查询按方言能力适配）。

## 4.5 兼容性

1. **API 向后兼容**：所有新能力通过 feature gate 隔离，默认 feature 行为不变，既有公开 API 完全向后兼容。
2. **sz-pay 不破坏**：sz-pay 从 crates.io 拉取的 sz-orm-* 6 个包既有用法不受影响。
3. **既有 EXPLAIN 分析保留**：既有 `ExplainPlan`（`packages/sz-orm-explain/src/lib.rs:76`）与 `PlanRegression`（`packages/sz-orm-explain/src/regression.rs:69`）保留不动，新增优化建议为独立包。
4. **既有火焰图保留**：既有 `QueryPhaseTiming`（`packages/sz-orm-flamegraph/src/collector.rs:39`）与 `QueryTracer::trace_execute`（`:61`）保留不动，新增诊断报告为独立包。
5. **既有自适应决策保留**：既有 `AdaptiveExecutor`（`packages/sz-orm-adaptive/src/executor.rs:120`）与 `QueryStats`（`packages/sz-orm-adaptive/src/stats.rs:11`）保留不动，新增优化建议复用既有统计。
6. **既有 AI 优化保留**：既有 `AutoTuningPipeline`（`packages/sz-orm-ai/src/auto_tuning/pipeline.rs:15`）保留不动，新增规则引擎为独立包，无 AI 依赖。
7. **既有缓存失效总线保留**：既有 `InvalidationBus`（`packages/sz-orm-core/src/l2_cache.rs:82`）/ `RedisPubSubInvalidationBus`（`packages/sz-orm-core/src/dist_cache.rs:41`）/ `GossipInvalidationBus`（`:179`）保留不动，db-fusion 转正复用既有失效总线。
8. **既有 CDC 保留**：既有 `DialectCapturer`（`packages/sz-orm-queue/src/cdc/capturer.rs:12`）与 `DownstreamSink`（`packages/sz-orm-queue/src/cdc/downstream.rs:12`）保留不动，db-fusion 转正复用既有 CDC。
9. **既有向量搜索保留**：既有 `HybridSearcher`（`packages/sz-orm-vector/src/hybrid_search/searcher.rs:30`）与 `FilterPushdown`（`packages/sz-orm-vector/src/hybrid_search/pushdown.rs:6`）保留不动，db-fusion 转正复用既有向量搜索。
10. **既有 Prometheus 指标保留**：既有 `MetricsRegistry`（`packages/sz-orm-observability/src/lib.rs:253`）与 `start_metrics_server`（`:421`）保留不动，新增结构化查询日志为扩展。
11. **既有 db-fusion POC 保留**：既有 `FusionConfig`（`packages/sz-orm-fusion/src/plan.rs:21`）/ `FusionExecutor`（`executor.rs:63`）/ `MemoryFusionCache`（`:16`）保留不动，db-fusion 转正为扩展（TTL + CDC + 向量下推），POC API `#[deprecated]` 标注。
12. **既有 feature 组合不破坏**：v4.4.0 新增 feature（`query-advisor` / `slow-query-diagnosis` / `db-fusion-v2` / `query-logging` / `perf-baseline` / `query-intelligence-loop`）与既有 feature（含 v4.2.0 7 个 + v4.3.0 7 个）任意组合编译通过。

---

# 5. 核心能力

## 5.1 查询自动优化建议引擎（REQ-V44-001，P1）

### 5.1.1 业务规则

1. **规则引擎建议生成**（EARS: Ubiquitous）
   系统应当提供 `sz-orm-advisor` 包，`OptimizationAdvisor` 规则引擎基于既有 EXPLAIN 分析结果（`ExplainPlan` `packages/sz-orm-explain/src/lib.rs:76`）与执行计划回归（`PlanRegression` `packages/sz-orm-explain/src/regression.rs:69`），通过规则匹配生成 `OptimizationSuggestion` 列表，`suggest(plan, stats) -> Vec<OptimizationSuggestion>` 入口，无 AI 依赖。
   a. 验收条件：[传入 `ExplainPlan { scan_type: FullTable, table: "users", index: None, rows: 10000 }`] → [生成 `AddIndex` 建议"添加索引 ON users(条件列)"，置信度 0.9]
2. **复用既有 AI 建议结构**（EARS: Ubiquitous）
   系统应当复用既有 AI 建议结构 `IndexSuggestion`（`packages/sz-orm-ai/src/index_advisor.rs:71`）/ `RewriteSuggestion`（`packages/sz-orm-ai/src/rewrite_advisor.rs:61`）/ `TuningSuggestion`（`packages/sz-orm-ai/src/auto_tuning/mod.rs:71`），`OptimizationSuggestion` 可转换为既有建议结构，不重复实现建议数据模型。
   a. 验收条件：[`OptimizationSuggestion` 生成] → [可转换为 `IndexSuggestion`/`RewriteSuggestion`/`TuningSuggestion`，复用既有结构]
3. **复用自适应统计决策**（EARS: Ubiquitous）
   系统应当复用既有自适应统计 `QueryStats`（`packages/sz-orm-adaptive/src/stats.rs:11`）与决策 `should_paginate`（`:66`）/ `should_cache`（`:73`），当 `should_paginate` 为真时生成 `UsePagination` 建议，当 `should_cache` 为真时生成 `EnableCache` 建议。
   a. 验收条件：[`QueryStats` 平均行数 2000 > 阈值 1000] → [生成 `UsePagination` 建议"改用游标分页"）；[平均耗时 50ms > 阈值 30ms 且执行 10 次] → [生成 `EnableCache` 建议"启用缓存"]
4. **六种建议类型**（EARS: Ubiquitous）
   系统应当实现六种优化建议类型：`AddIndex`（添加索引）/ `DropIndex`（删除冗余索引）/ `UsePagination`（改用游标分页）/ `EnableCache`（启用缓存）/ `RewriteQuery`（查询改写）/ `AdjustPoolSize`（调整连接池大小），每种建议附带 `description`（描述）/ `action`（可执行动作）/ `confidence`（置信度 0.0~1.0）/ `estimated_improvement`（预估改善）。
   a. 验收条件：[全表扫描] → [`AddIndex`]；[冗余索引] → [`DropIndex`]；[大结果集] → [`UsePagination`]；[热点查询] → [`EnableCache`]；[可改写查询] → [`RewriteQuery`]；[PoolAcquire 耗时高] → [`AdjustPoolSize`]
5. **建议保守性**（EARS: State-driven）
   当优化建议置信度低于阈值（默认 0.5）时，系统应当标注"需人工确认"，不自动执行建议，建议仅生成文本不执行 DDL。
   a. 验收条件：[置信度 0.3 的建议] → [标注"需人工确认"，不自动执行]
6. **JSON 报告输出**（EARS: Ubiquitous）
   系统应当输出 JSON 格式优化建议报告（含建议列表 + 目标查询 + 置信度 + 预估改善），可被 CI/IDE 消费，`to_json(&[OptimizationSuggestion]) -> String`。
   a. 验收条件：[生成 3 条建议] → [JSON 报告含 3 条建议，可被 `serde_json::from_str` 解析]
7. **禁止项**（EARS: Unwanted）
   如果查询自动优化建议影响默认 feature 编译或自动执行 DDL，则系统应当通过 `query-advisor` feature gate 隔离，默认不启用建议生成，且建议仅生成文本不自动执行 DDL。
   a. 验收条件：[`cargo build` 默认编译] → [无优化建议生成，行为与 v4.3.0 一致]

### 5.1.2 交互流程

```plantuml
@startuml
title 查询自动优化建议引擎 交互流程
actor "应用开发者" as appdev
participant "sz-orm-advisor\n(规则引擎)" as advisor
participant "sz-orm-explain\n(既有 EXPLAIN 分析)" as explain
participant "sz-orm-adaptive\n(既有统计决策)" as adaptive
participant "sz-orm-ai\n(既有建议结构)" as ai
database "DB" as db

appdev -> explain : query! 宏 db-verify EXPLAIN
explain --> appdev : ExplainPlan
appdev -> adaptive : 查询执行统计
adaptive --> appdev : QueryStats
appdev -> advisor : suggest(plan, stats)
advisor -> explain : 复用 ExplainPlan/PlanRegression
advisor -> adaptive : 复用 should_paginate/should_cache
advisor -> ai : 复用 IndexSuggestion/RewriteSuggestion
advisor -> advisor : 规则匹配生成建议
advisor --> appdev : Vec<OptimizationSuggestion>
appdev -> advisor : to_json(suggestions)
advisor --> appdev : JSON 报告(CI 消费)
@enduml
```

### 5.1.3 异常场景

1. **EXPLAIN 分析结果缺失**
   a. 触发条件：未启用 `explain-analyzer` feature 或 EXPLAIN 解析失败
   b. 系统行为：降级为仅基于自适应统计的建议（`UsePagination`/`EnableCache`），不 panic
   c. 用户感知：建议报告标注"EXPLAIN analysis unavailable, suggestions based on runtime stats only"
2. **自适应统计不足**
   a. 触发条件：查询执行次数不足（`total_executions < min_executions`）
   b. 系统行为：降级为仅基于 EXPLAIN 分析的建议（`AddIndex`/`RewriteQuery`），不生成统计依赖建议
   c. 用户感知：建议报告标注"insufficient stats, suggestions based on EXPLAIN only"
3. **建议冲突**
   a. 触发条件：多条建议冲突（如 `AddIndex` 与 `DropIndex` 针对同一索引）
   b. 系统行为：按置信度排序，保留高置信度建议，低置信度建议标注"conflict, skipped"
   c. 用户感知：建议报告标注冲突建议"conflict with higher confidence suggestion, skipped"

## 5.2 慢查询自动诊断报告（REQ-V44-002，P1）

### 5.2.1 业务规则

1. **根因分析**（EARS: Ubiquitous）
   系统应当提供 `sz-orm-diagnosis` 包，`SlowQueryDiagnoser` 基于既有火焰图阶段耗时（`QueryPhaseTiming` `packages/sz-orm-flamegraph/src/collector.rs:39`，`Phase` 枚举 Build/Bind/PoolAcquire/SqlExecute/ResultMap `:11`），分析慢查询根因：PoolAcquire 耗时占比 > 30% → `PoolExhaustion`；SqlExecute 耗时占比 > 50% → `SqlInefficiency`；ResultMap 耗时占比 > 30% → `LargeResultSet`；Build 耗时占比 > 20% → `BuildOverhead`；多阶段高 → `MixedCause`。
   a. 验收条件：[PoolAcquire 耗时 60ms / 总耗时 100ms] → [根因 `PoolExhaustion`"；SqlExecute 耗时 80ms / 总耗时 100ms] → [根因 `SqlInefficiency`"]
2. **复用自适应 slow 标记**（EARS: Ubiquitous）
   系统应当复用既有自适应 slow 标记 `QueryOutcome.slow`（`packages/sz-orm-adaptive/src/executor.rs:116`）与阈值 `AdaptiveConfig.slow_ms`（`:35`），仅对 `slow == true` 的查询触发诊断（非每次查询）。
   a. 验收条件：[查询耗时 50ms < `slow_ms` 100ms] → [不触发诊断]；[查询耗时 150ms > `slow_ms` 100ms] → [触发诊断]
3. **诊断报告结构**（EARS: Ubiquitous）
   系统应当生成 `DiagnosisReport`（`query_key` / `total_elapsed_ms` / `root_cause: RootCause` / `phase_breakdown: Vec<PhaseBreakdown>` / `suggestions: Vec<OptimizationSuggestion>` / `severity: Severity`），`Severity` 枚举（Info/Warning/Critical），`PhaseBreakdown` 含阶段名/耗时/占比/异常标记。
   a. 验收条件：[慢查询诊断] → [报告含根因 + 阶段分解 + 建议 + 严重度]
4. **诊断建议联动**（EARS: Ubiquitous）
   系统应当与查询自动优化建议引擎（REQ-V44-001）联动，根据根因生成对应建议：`PoolExhaustion` → `AdjustPoolSize`；`SqlInefficiency` → `AddIndex`/`RewriteQuery`；`LargeResultSet` → `UsePagination`；`BuildOverhead` → `RewriteQuery`。
   a. 验收条件：[根因 `PoolExhaustion`] → [建议 `AdjustPoolSize`"；根因 `LargeResultSet`] → [建议 `UsePagination`"]
5. **双格式输出**（EARS: Ubiquitous）
   系统应当输出 JSON 格式（`to_json`，CI 消费）与人类可读格式（`to_human_readable`，含根因/阶段分解/建议表格），无外部依赖。
   a. 验收条件：[诊断报告] → [JSON 可被 `serde_json::from_str` 解析；人类可读格式含根因/阶段/建议表格]
6. **禁止项**（EARS: Unwanted）
   如果慢查询自动诊断影响默认 feature 编译或每次查询都触发诊断，则系统应当通过 `slow-query-diagnosis` feature gate 隔离，默认不启用诊断，且仅对慢查询触发（非每次查询）。
   a. 验收条件：[`cargo build` 默认编译] → [无慢查询诊断，行为与 v4.3.0 一致]

### 5.2.2 交互流程

```plantuml
@startuml
title 慢查询自动诊断报告 交互流程
actor "应用开发者" as appdev
participant "sz-orm-diagnosis\n(诊断器)" as diagnosis
participant "sz-orm-flamegraph\n(既有火焰图)" as flamegraph
participant "sz-orm-adaptive\n(既有 slow 标记)" as adaptive
participant "sz-orm-advisor\n(优化建议)" as advisor

appdev -> flamegraph : QueryTracer::trace_execute(查询)
flamegraph --> appdev : Vec<QueryPhaseTiming>
appdev -> adaptive : execute_cached(查询)
adaptive --> appdev : QueryOutcome { slow: true, elapsed_ms: 150 }
alt slow == true
  appdev -> diagnosis : diagnose(query_key, timings, outcome)
  diagnosis -> flamegraph : 复用 QueryPhaseTiming
  diagnosis -> adaptive : 复用 slow_ms 阈值
  diagnosis -> diagnosis : 根因分析(阶段耗时占比)
  diagnosis -> advisor : suggest(plan, stats)
  advisor --> diagnosis : Vec<OptimizationSuggestion>
  diagnosis --> appdev : DiagnosisReport(根因+建议)
  appdev -> diagnosis : to_json / to_human_readable
  diagnosis --> appdev : JSON / 人类可读报告
else slow == false
  adaptive --> appdev : 不触发诊断
end
@enduml
```

### 5.2.3 异常场景

1. **火焰图阶段耗时缺失**
   a. 触发条件：未启用 `query-flamegraph` feature 或阶段耗时采集失败
   b. 系统行为：降级为"仅总耗时 + `RootCause::Unknown`"，不 panic
   c. 用户感知：诊断报告标注"phase timing unavailable, root cause unknown"
2. **阶段耗时总和与总耗时不符**
   a. 触发条件：各阶段耗时总和与 `QueryOutcome.elapsed_ms` 误差超 10%
   b. 系统行为：标注"timing mismatch"，仍生成报告但根因置信度降低
   c. 用户感知：诊断报告标注"timing mismatch detected, root cause confidence reduced"
3. **多根因并存**
   a. 触发条件：多个阶段耗时占比同时超阈值（如 PoolAcquire 35% + SqlExecute 55%）
   b. 系统行为：根因判定为 `MixedCause`，建议列表合并多根因对应建议
   c. 用户感知：诊断报告标注"mixed cause: PoolExhaustion + SqlInefficiency"

## 5.3 db-fusion 转正（REQ-V44-003，P1）

### 5.3.1 业务规则

1. **阶段一：TTL 融合缓存**（EARS: Ubiquitous）
   系统应当在既有 `MemoryFusionCache`（`packages/sz-orm-fusion/src/executor.rs:16`，无过期）基础上增加 `TtlFusionCache`（TTL 过期 + 过期检查），`FusionCache` trait（`:8`）扩展 `get_with_ttl`/`set_with_ttl` 方法，TTL 可配（默认 60 秒），过期缓存不返回。
   a. 验收条件：[设置 TTL 60 秒的缓存] → [60 秒内命中返回数据，60 秒后返回 None]
2. **阶段一：失效广播**（EARS: Ubiquitous）
   系统应当复用既有 `InvalidationBus`（`packages/sz-orm-core/src/l2_cache.rs:82`）/ `RedisPubSubInvalidationBus`（`packages/sz-orm-core/src/dist_cache.rs:41`）/ `GossipInvalidationBus`（`:179`），主库写入后发布失效消息，跨实例缓存失效，`FusionExecutor` 增加 `with_invalidation_bus(bus)` 方法。
   a. 验收条件：[实例 A 主库写入] → [发布失效消息，实例 B 缓存失效，下次查询回源主库]
3. **阶段二：CDC 增量同步集成**（EARS: Ubiquitous）
   系统应当复用既有 `DialectCapturer`（`packages/sz-orm-queue/src/cdc/capturer.rs:12`，5 方言 CDC 捕获器）与 `DownstreamSink`（`packages/sz-orm-queue/src/cdc/downstream.rs:12`，7 种下游分发）/ `distribute_to_all`（`:178`），提供 `CdcSyncCoordinator` 协调主库变更 → 缓存/搜索索引增量同步，复用既有 `CdcCheckpoint`（`packages/sz-orm-queue/src/cdc/checkpoint.rs`）断点续传。
   a. 验收条件：[主库 users 表 INSERT] → [CDC 捕获变更事件，分发到缓存失效 + 搜索索引更新下游]
4. **阶段二：真实向量搜索下推**（EARS: Ubiquitous）
   系统应当将 v4.3.0 POC 的"搜索下推仅记录数据源"（`packages/sz-orm-fusion/src/executor.rs:118`）转正为真实向量检索，复用既有 `HybridSearcher`（`packages/sz-orm-vector/src/hybrid_search/searcher.rs:30`）三源并行查询 + `FilterPushdown`（`packages/sz-orm-vector/src/hybrid_search/pushdown.rs:6`）结构化过滤下推，提供 `VectorPushdownExecutor` 执行真实向量搜索。
   a. 验收条件：[融合查询含 `search: 无线耳机` 条件] → [调用 `HybridSearcher::search` 执行真实向量检索，返回融合排序结果]
5. **转正 API 稳定标注**（EARS: Ubiquitous）
   系统应当将 db-fusion 转正后 API 标注 `#[stable]`（非 `#[experimental]`），向后兼容，既有 POC API（`FusionConfig`/`FusionExecutor`/`MemoryFusionCache`）标注 `#[deprecated(note = "use TtlFusionCache + CdcSyncCoordinator")]` 并提供迁移指引。
   a. 验收条件：[转正 API] → [标注 `#[stable]`]；[POC API] → [标注 `#[deprecated]` + 迁移指引]
6. **降级语义保留**（EARS: Ubiquitous）
   系统应当保留既有降级语义：主库失败回退缓存须返回 `degraded` 标记（`FusionOutcome.degraded` `packages/sz-orm-fusion/src/executor.rs:55`），向量搜索下推失败须降级为主库查询（复用 `DegradationStatus` `packages/sz-orm-vector/src/hybrid_search/searcher.rs:60`），CDC 同步失败须降级为 TTL 过期兜底。
   a. 验收条件：[主库失败 + 缓存可读] → [返回缓存旧数据 + `degraded: true`]；[向量搜索失败] → [降级为主库查询]
7. **禁止项**（EARS: Unwanted）
   如果 db-fusion 转正影响默认 feature 编译或破坏既有 POC API，则系统应当通过 `db-fusion-v2` feature gate 隔离，默认不启用转正能力，且既有 POC API 保留（`#[deprecated]` 标注而非删除）。
   a. 验收条件：[`cargo build` 默认编译] → [无 db-fusion 转正，既有 POC API 保留可用]

### 5.3.2 交互流程

```plantuml
@startuml
title db-fusion 转正 交互流程
actor "运维/SRE" as sre
participant "sz-orm-fusion\n(转正)" as fusion
participant "InvalidationBus\n(既有)" as bus
participant "DialectCapturer\n(既有 CDC)" as cdc
participant "HybridSearcher\n(既有向量)" as vector
database "主库" as primary
rectangle "Redis\n(缓存)" as redis
rectangle "向量库" as vecdb
rectangle "CDC 下游" as downstream

alt 阶段一：TTL 缓存 + 失效广播
  sre -> fusion : FusionExecutor::new(config).with_cache(ttl_cache).with_invalidation_bus(bus)
  fusion -> redis : TTL 缓存查找
  alt 缓存命中
    redis --> fusion : 返回缓存数据
  else 缓存未命中
    fusion -> primary : 主库查询
    primary --> fusion : 结果
    fusion -> redis : 回填 TTL 缓存
    fusion -> bus : 发布失效消息
    bus --> redis : 跨实例缓存失效
  end
end

alt 阶段二：CDC 增量同步
  primary -> cdc : 主库变更(WAL/binlog)
  cdc -> fusion : ChangeEvent
  fusion -> downstream : distribute_to_all(缓存失效+搜索索引更新)
end

alt 阶段二：真实向量搜索下推
  sre -> fusion : FusionQuery 含 search: 条件
  fusion -> vector : VectorPushdownExecutor::execute
  vector -> vecdb : HybridSearcher::search(三源并行)
  vecdb --> vector : 融合排序结果
  vector --> fusion : 向量搜索结果
end
@enduml
```

### 5.3.3 异常场景

1. **TTL 缓存过期但主库不可用**
   a. 触发条件：缓存 TTL 过期且主库查询失败
   b. 系统行为：返回明确错误"cache expired and primary unavailable"，不返回过期脏数据（除非降级模式）
   c. 用户感知：错误"cache expired and primary unavailable, retry later"
2. **失效总线不可用**
   a. 触发条件：`InvalidationBus` 发布失败（Redis Pub/Sub 断连）
   b. 系统行为：降级为 TTL 过期兜底，告警"invalidation bus unavailable, relying on TTL"
   c. 用户感知：告警"cache invalidation bus unavailable, relying on TTL expiry"
3. **CDC 捕获失败**
   a. 触发条件：`DialectCapturer::start_capture` 失败（WAL/binlog 未配置）
   b. 系统行为：降级为 TTL 过期兜底，告警"CDC capture failed, relying on TTL"
   c. 用户感知：告警"CDC incremental sync unavailable, relying on TTL expiry"
4. **向量搜索下推失败**
   a. 触发条件：`HybridSearcher::search` 失败（向量库不可用）
   b. 系统行为：降级为主库查询（复用 `DegradationStatus`），标记 `vector_degraded`
   c. 用户感知：结果标记"vector search degraded, fallback to primary"
5. **CDC 变更事件脱敏失败**
   a. 触发条件：CDC 变更事件含敏感字段但脱敏失败
   b. 系统行为：跳过该事件，告警"CDC event masking failed, skipped"
   c. 用户感知：告警"CDC event for table X masking failed, skipped, manual review required"

## 5.4 结构化查询日志（REQ-V44-004，P2）

### 5.4.1 业务规则

1. **结构化日志结构**（EARS: Ubiquitous）
   系统应当扩展既有 `sz-orm-observability`（`packages/sz-orm-observability/src/lib.rs:253` `MetricsRegistry`），提供 `QueryLogger` 结构化日志器，输出 `QueryLogEntry`（`query_key` / `sql` / `params`（脱敏）/ `total_elapsed_ms` / `phase_breakdown` / `slow` / `from_cache` / `timestamp`），JSON 格式。
   a. 验收条件：[查询执行] → [输出 JSON 日志含 query_key/sql/params(脱敏)/耗时/阶段/慢标记]
2. **复用火焰图阶段耗时**（EARS: Ubiquitous）
   系统应当复用既有 `QueryPhaseTiming`（`packages/sz-orm-flamegraph/src/collector.rs:39`），日志 `phase_breakdown` 含各阶段耗时，未启用 `query-flamegraph` feature 时 `phase_breakdown` 为空。
   a. 验收条件：[启用 `query-flamegraph`] → [日志含 5 阶段耗时]；[未启用] → [日志 `phase_breakdown` 为空]
3. **采样率与级别控制**（EARS: Ubiquitous）
   系统应当支持采样率配置（默认 1%，慢查询 100% 采样）与日志级别配置（DEBUG 含 SQL/参数，INFO 仅含统计，WARN 仅含慢查询），`QueryLogger::with_sample_rate(rate)` / `with_level(level)`。
   a. 验收条件：[采样率 1% + 100 次查询] → [约 1 条日志]；[慢查询] → [100% 采样，必输出日志]
4. **参数脱敏**（EARS: Ubiquitous）
   系统应当复用既有 `sz-orm-masking`（`MaskingRule` `packages/sz-orm-masking/src/lib.rs:21`）脱敏查询参数，日志 `params` 字段自动脱敏，不暴露生产参数值。
   a. 验收条件：[查询参数含手机号 13800138000] → [日志 `params` 脱敏为 138****8000]
5. **禁止项**（EARS: Unwanted）
   如果结构化查询日志影响默认 feature 编译或每次查询都输出日志，则系统应当通过 `query-logging` feature gate 隔离，默认不启用日志，且采样率默认 1%（非 100%）。
   a. 验收条件：[`cargo build` 默认编译] → [无结构化查询日志，行为与 v4.3.0 一致]

### 5.4.2 交互流程

```plantuml
@startuml
title 结构化查询日志 交互流程
actor "运维/SRE" as sre
participant "sz-orm-observability\n(日志扩展)" as logging
participant "sz-orm-flamegraph\n(既有火焰图)" as flamegraph
participant "sz-orm-masking\n(既有脱敏)" as masking
participant "MetricsRegistry\n(既有)" as metrics

sre -> logging : QueryLogger::new().with_sample_rate(0.01).with_level(Info)
sre -> flamegraph : QueryTracer::trace_execute(查询)
flamegraph --> sre : Vec<QueryPhaseTiming>
sre -> logging : log(query_key, sql, params, timings, slow)
logging -> masking : 脱敏 params
masking --> logging : 脱敏后 params
logging -> logging : 采样判定 + 级别过滤
alt 采样通过
  logging -> metrics : MetricsRegistry 关联指标
  logging --> sre : QueryLogEntry(JSON)
else 采样未通过
  logging --> sre : 不输出日志
end
@enduml
```

### 5.4.3 异常场景

1. **日志写入失败**
   a. 触发条件：日志输出 IO 失败（磁盘满/网络断）
   b. 系统行为：静默丢弃日志（不阻断查询），告警计数 +1
   c. 用户感知：查询正常执行，日志丢失（告警计数可观测）
2. **脱敏失败**
   a. 触发条件：参数脱敏规则匹配失败
   b. 系统行为：参数替换为 `[masking failed]`，不暴露原始值
   c. 用户感知：日志 `params` 为 `[masking failed]`

## 5.5 性能回归基准线 + CI 自动比对（REQ-V44-005，P2）

### 5.5.1 业务规则

1. **耗时基线快照**（EARS: Ubiquitous）
   系统应当扩展既有 `PlanSnapshot`（`packages/sz-orm-explain/src/regression.rs:23`）为 `PerfBaseline`（含各阶段耗时基线 `phase_baselines: HashMap<Phase, u64>` + 执行计划基线 `ExplainPlan` + 采集时间），JSON 序列化，版本化管理。
   a. 验收条件：[采集查询各阶段耗时基线] → [`PerfBaseline` JSON 含 Build/Bind/PoolAcquire/SqlExecute/ResultMap 各阶段基线]
2. **性能回归检测**（EARS: Ubiquitous）
   系统应当提供 `PerfRegression`（`PhaseSlowdown` 阶段耗时增长 / `TotalSlowdown` 总耗时增长 / `PlanRegression` 执行计划回归），`check_perf_regressions(baseline_json, current_json, threshold_factor) -> Vec<PerfRegression>` CI 入口，检出性能退化（如 Build 阶段基线 5ms，当前 50ms → `PhaseSlowdown`）。
   a. 验收条件：[基线 Build 5ms，当前 Build 50ms，阈值因子 5] → [检出 `PhaseSlowdown { phase: Build, before: 5, after: 50 }`]
3. **复用执行计划回归**（EARS: Ubiquitous）
   系统应当复用既有 `PlanRegression`（`packages/sz-orm-explain/src/regression.rs:69`）与 `check_regressions`（`:161`），`PerfRegression` 包含 `PlanRegression` 变体，一次比对同时检出耗时回归与执行计划回归。
   a. 验收条件：[基线 IndexRange，当前 FullTable] → [检出 `PerfRegression::PlanRegression(ScanTypeUpgrade)`]
4. **复用火焰图阶段耗时**（EARS: Ubiquitous）
   系统应当复用既有 `QueryPhaseTiming`（`packages/sz-orm-flamegraph/src/collector.rs:39`），`PerfBaseline` 各阶段耗时基线来自 `QueryPhaseTiming`，未启用 `query-flamegraph` feature 时仅比对总耗时。
   a. 验收条件：[启用 `query-flamegraph`] → [比对 5 阶段耗时]；[未启用] → [仅比对总耗时]
5. **CI 自动比对**（EARS: Ubiquitous）
   系统应当提供 CI 入口 `check_perf_regressions`，读取基线 JSON 与当前耗时 JSON，返回 `Vec<PerfRegression>`，CI 中检测性能退化并报告，基线文件版本化管理。
   a. 验收条件：[CI 中基线 Build 5ms，当前 Build 50ms] → [CI 报告"perf regression: Build phase slowed from 5ms to 50ms"]
6. **禁止项**（EARS: Unwanted）
   如果性能回归基准线影响默认 feature 编译或阻断 CI，则系统应当通过 `perf-baseline` feature gate 隔离，默认不启用性能基线，且 CI 比对结果为建议非阻断。
   a. 验收条件：[`cargo build` 默认编译] → [无性能基线，行为与 v4.3.0 一致]

### 5.5.2 交互流程

```plantuml
@startuml
title 性能回归基准线 + CI 自动比对 交互流程
actor "DBA" as dba
participant "sz-orm-explain\n(基线扩展)" as explain
participant "sz-orm-flamegraph\n(既有火焰图)" as flamegraph
participant "CI/CD" as ci

== 基线采集 ==
dba -> flamegraph : QueryTracer::trace_execute(查询)
flamegraph --> dba : Vec<QueryPhaseTiming>
dba -> explain : PerfBaseline::new(query_key, plan, timings)
explain --> dba : PerfBaseline(JSON)
dba -> dba : 保存基线 plans/baseline.json

== CI 比对 ==
ci -> flamegraph : 当前查询采集耗时
flamegraph --> ci : 当前 QueryPhaseTiming
ci -> explain : check_perf_regressions(baseline.json, current.json, 5)
explain -> explain : 比对各阶段耗时 + 执行计划
explain --> ci : Vec<PerfRegression>
alt 有回归
  ci --> ci : 报告性能退化(非阻断)
else 无回归
  ci --> ci : 性能基线通过
end
@enduml
```

### 5.5.3 异常场景

1. **基线文件缺失**
   a. 触发条件：CI 中基线 JSON 文件不存在
   b. 系统行为：跳过比对，告警"baseline file not found, skip perf regression check"
   c. 用户感知：CI 告警"perf baseline not found, skip check"
2. **当前耗时缺失**
   a. 触发条件：当前查询耗时未采集（未启用 `query-flamegraph`）
   b. 系统行为：降级为仅比对执行计划回归（`PlanRegression`），不比对耗时
   c. 用户感知：CI 报告"phase timing unavailable, only plan regression checked"
3. **阈值因子配置不当**
   a. 触发条件：阈值因子过小（如 1.0）导致正常波动被报告
   b. 系统行为：按配置阈值比对，建议值 2（保守）/3（常规）/5（宽松）
   c. 用户感知：CI 报告标注"threshold factor: X, adjust if too noisy"

## 5.6 查询智能闭环联动（REQ-V44-006，P3）

### 5.6.1 业务规则

1. **闭环四步联动**（EARS: Optional）
   当 `query-intelligence-loop` feature 启用时，系统应当提供 `IntelligenceLoop` 闭环协调器，将 EXPLAIN 分析（`packages/sz-orm-explain/src/lib.rs:76` `ExplainPlan`）→ 自适应决策（`packages/sz-orm-adaptive/src/executor.rs:157` `decide`）→ 火焰图诊断（`packages/sz-orm-flamegraph/src/collector.rs:61` `trace_execute`）→ 优化建议（REQ-V44-001 `suggest`）四步联动形成闭环，输出 `LoopReport`。
   a. 验收条件：[启用 `query-intelligence-loop` + 执行查询] → [输出 `LoopReport` 含四步结果与最终建议]
2. **闭环报告结构**（EARS: Ubiquitous）
   系统应当生成 `LoopReport`（`query_key` / `explain_result` / `adaptive_decision` / `diagnosis_result` / `suggestions` / `loop_elapsed_ms`），JSON 输出可被 CI 消费。
   a. 验收条件：[闭环执行] → [`LoopReport` JSON 含 EXPLAIN 结果/自适应决策/诊断结果/建议列表]
3. **非强依赖降级**（EARS: Unwanted）
   如果闭环联动任一环节失败（EXPLAIN 解析失败/自适应统计不足/火焰图未启用），则系统应当降级为独立工作（跳过失败环节，剩余环节继续），不阻断查询，不 panic。
   a. 验收条件：[火焰图未启用] → [闭环跳过诊断环节，仍输出 EXPLAIN + 自适应 + 建议]
4. **禁止项**（EARS: Unwanted）
   如果查询智能闭环联动影响默认 feature 编译或每次查询都触发闭环，则系统应当通过 `query-intelligence-loop` feature gate 隔离，默认不启用闭环，且闭环仅可选触发（非每次查询）。
   a. 验收条件：[`cargo build` 默认编译] → [无闭环联动，行为与 v4.3.0 一致]

### 5.6.2 交互流程

```plantuml
@startuml
title 查询智能闭环联动 交互流程
actor "应用开发者" as appdev
participant "IntelligenceLoop\n(闭环协调器)" as loop
participant "sz-orm-explain\n(EXPLAIN 分析)" as explain
participant "sz-orm-adaptive\n(自适应决策)" as adaptive
participant "sz-orm-flamegraph\n(火焰图诊断)" as flamegraph
participant "sz-orm-advisor\n(优化建议)" as advisor

appdev -> loop : run_loop(query_key, query)
loop -> explain : 1. EXPLAIN 分析
explain --> loop : ExplainPlan
loop -> adaptive : 2. 自适应决策
adaptive --> loop : ExecutionPath
loop -> flamegraph : 3. 火焰图诊断
flamegraph --> loop : DiagnosisReport
loop -> advisor : 4. 优化建议(汇总前三步)
advisor --> loop : Vec<OptimizationSuggestion>
loop --> appdev : LoopReport(四步结果+最终建议)
@enduml
```

### 5.6.3 异常场景

1. **EXPLAIN 分析失败**
   a. 触发条件：EXPLAIN 解析失败或未启用 `explain-analyzer`
   b. 系统行为：跳过 EXPLAIN 环节，闭环继续自适应 + 诊断 + 建议
   c. 用户感知：`LoopReport` 标注"EXPLAIN step skipped"
2. **自适应统计不足**
   a. 触发条件：查询执行次数不足
   b. 系统行为：跳过自适应决策环节，闭环继续诊断 + 建议
   c. 用户感知：`LoopReport` 标注"adaptive step skipped (insufficient stats)"
3. **火焰图未启用**
   a. 触发条件：未启用 `query-flamegraph` feature
   b. 系统行为：跳过诊断环节，闭环继续建议
   c. 用户感知：`LoopReport` 标注"diagnosis step skipped (flamegraph not enabled)"

---

# 6. 数据约束

## 6.1 OptimizationSuggestion（优化建议）

1. **suggestion_type**：`SuggestionType` 枚举（AddIndex/DropIndex/UsePagination/EnableCache/RewriteQuery/AdjustPoolSize），必填。
2. **target_query**：目标查询标识（query_key 或 SQL 摘要），String，必填。
3. **description**：建议描述（人类可读），String，必填。
4. **action**：可执行动作（如"CREATE INDEX idx_xxx ON users(email)"），String，必填。
5. **confidence**：置信度，f64，范围 0.0~1.0，必填，低于 0.5 标注"需人工确认"。
6. **estimated_improvement**：预估改善（如"减少 90% 扫描行数"），String，可选。

## 6.2 DiagnosisReport（诊断报告）

1. **query_key**：查询标识，String，必填。
2. **total_elapsed_ms**：总耗时（毫秒），u64，必填。
3. **root_cause**：根因，`RootCause` 枚举（PoolExhaustion/SqlInefficiency/LargeResultSet/BuildOverhead/MixedCause/Unknown），必填。
4. **phase_breakdown**：阶段分解，`Vec<PhaseBreakdown>`，每项含 `phase: Phase` / `elapsed_ms: u64` / `percentage: f64` / `anomaly: bool`，必填。
5. **suggestions**：建议列表，`Vec<OptimizationSuggestion>`，必填（可为空）。
6. **severity**：严重度，`Severity` 枚举（Info/Warning/Critical），必填。

## 6.3 PerfBaseline（性能基线）

1. **query_key**：查询标识，String，必填。
2. **phase_baselines**：各阶段耗时基线，`HashMap<Phase, u64>`（毫秒），必填。
3. **plan**：执行计划基线，`ExplainPlan`，必填。
4. **captured_at**：采集时间（ISO 8601），String，必填。

## 6.4 PerfRegression（性能回归）

1. **query_key**：查询标识，String，必填。
2. **regression_type**：回归类型，`PerfRegressionType` 枚举（PhaseSlowdown/TotalSlowdown/PlanRegression），必填。
3. **before**：基线值，u64（耗时）或 `ExplainPlan`（计划），必填。
4. **after**：当前值，u64（耗时）或 `ExplainPlan`（计划），必填。

## 6.5 QueryLogEntry（查询日志）

1. **query_key**：查询标识，String，必填。
2. **sql**：SQL 文本（脱敏），String，必填。
3. **params**：查询参数（脱敏），`Vec<String>`，必填。
4. **total_elapsed_ms**：总耗时（毫秒），u64，必填。
5. **phase_breakdown**：阶段分解，`Vec<QueryPhaseTiming>`，可选（未启用 `query-flamegraph` 时为空）。
6. **slow**：慢查询标记，bool，必填。
7. **from_cache**：缓存命中标记，bool，必填。
8. **timestamp**：时间戳（ISO 8601），String，必填。

## 6.6 LoopReport（闭环报告）

1. **query_key**：查询标识，String，必填。
2. **explain_result**：EXPLAIN 分析结果，`Option<ExplainPlan>`，可选（环节跳过时 None）。
3. **adaptive_decision**：自适应决策，`Option<ExecutionPath>`，可选。
4. **diagnosis_result**：诊断结果，`Option<DiagnosisReport>`，可选。
5. **suggestions**：建议列表，`Vec<OptimizationSuggestion>`，必填（可为空）。
6. **loop_elapsed_ms**：闭环总耗时（毫秒），u64，必填。

---

# 7. 需求追溯矩阵

| 需求 ID | 优先级 | 需求名称 | 验收条件数 | feature gate | 复用既有代码 |
|---------|--------|---------|-----------|-------------|-------------|
| REQ-V44-001 | P1 | 查询自动优化建议引擎 | 7 | `query-advisor` | `ExplainPlan` `packages/sz-orm-explain/src/lib.rs:76` / `PlanRegression` `packages/sz-orm-explain/src/regression.rs:69` / `QueryStats` `packages/sz-orm-adaptive/src/stats.rs:11` / `IndexSuggestion` `packages/sz-orm-ai/src/index_advisor.rs:71` / `RewriteSuggestion` `packages/sz-orm-ai/src/rewrite_advisor.rs:61` / `TuningSuggestion` `packages/sz-orm-ai/src/auto_tuning/mod.rs:71` |
| REQ-V44-002 | P1 | 慢查询自动诊断报告 | 6 | `slow-query-diagnosis` | `QueryPhaseTiming` `packages/sz-orm-flamegraph/src/collector.rs:39` / `Phase` `packages/sz-orm-flamegraph/src/collector.rs:11` / `QueryOutcome.slow` `packages/sz-orm-adaptive/src/executor.rs:116` / `AdaptiveConfig.slow_ms` `packages/sz-orm-adaptive/src/executor.rs:35` |
| REQ-V44-003 | P1 | db-fusion 转正 | 7 | `db-fusion-v2` | `FusionConfig` `packages/sz-orm-fusion/src/plan.rs:21` / `FusionExecutor` `packages/sz-orm-fusion/src/executor.rs:63` / `MemoryFusionCache` `packages/sz-orm-fusion/src/executor.rs:16` / `InvalidationBus` `packages/sz-orm-core/src/l2_cache.rs:82` / `RedisPubSubInvalidationBus` `packages/sz-orm-core/src/dist_cache.rs:41` / `DialectCapturer` `packages/sz-orm-queue/src/cdc/capturer.rs:12` / `DownstreamSink` `packages/sz-orm-queue/src/cdc/downstream.rs:12` / `HybridSearcher` `packages/sz-orm-vector/src/hybrid_search/searcher.rs:30` / `FilterPushdown` `packages/sz-orm-vector/src/hybrid_search/pushdown.rs:6` |
| REQ-V44-004 | P2 | 结构化查询日志 | 5 | `query-logging` | `MetricsRegistry` `packages/sz-orm-observability/src/lib.rs:253` / `QueryPhaseTiming` `packages/sz-orm-flamegraph/src/collector.rs:39` / `MaskingRule` `packages/sz-orm-masking/src/lib.rs:21` |
| REQ-V44-005 | P2 | 性能回归基准线 + CI 自动比对 | 6 | `perf-baseline` | `PlanSnapshot` `packages/sz-orm-explain/src/regression.rs:23` / `PlanRegression` `packages/sz-orm-explain/src/regression.rs:69` / `check_regressions` `packages/sz-orm-explain/src/regression.rs:161` / `QueryPhaseTiming` `packages/sz-orm-flamegraph/src/collector.rs:39` |
| REQ-V44-006 | P3 | 查询智能闭环联动 | 4 | `query-intelligence-loop` | `ExplainPlan` `packages/sz-orm-explain/src/lib.rs:76` / `AdaptiveExecutor::decide` `packages/sz-orm-adaptive/src/executor.rs:157` / `QueryTracer::trace_execute` `packages/sz-orm-flamegraph/src/collector.rs:61` |

---

# 8. 验收标准总览

## 8.1 REQ-V44-001 查询自动优化建议引擎（P1）

1. 规则引擎基于 EXPLAIN 分析 + 自适应统计生成 `OptimizationSuggestion`，无 AI 依赖
2. 复用既有 `IndexSuggestion`/`RewriteSuggestion`/`TuningSuggestion` 建议结构
3. 复用 `should_paginate`/`should_cache` 生成 `UsePagination`/`EnableCache` 建议
4. 六种建议类型（AddIndex/DropIndex/UsePagination/EnableCache/RewriteQuery/AdjustPoolSize）
5. 低置信度建议标注"需人工确认"，不自动执行 DDL
6. JSON 报告输出可被 CI/IDE 消费
7. `query-advisor` feature gate 隔离，默认关闭

## 8.2 REQ-V44-002 慢查询自动诊断报告（P1）

1. 基于火焰图阶段耗时分析根因（PoolExhaustion/SqlInefficiency/LargeResultSet/BuildOverhead/MixedCause）
2. 复用 `QueryOutcome.slow` + `AdaptiveConfig.slow_ms`，仅对慢查询触发
3. `DiagnosisReport` 含根因 + 阶段分解 + 建议 + 严重度
4. 与 REQ-V44-001 联动，根因 → 对应建议
5. JSON + 人类可读双格式输出
6. `slow-query-diagnosis` feature gate 隔离，默认关闭

## 8.3 REQ-V44-003 db-fusion 转正（P1）

1. 阶段一：`TtlFusionCache` TTL 过期 + `InvalidationBus` 失效广播
2. 阶段二：`CdcSyncCoordinator` CDC 增量同步（复用 `DialectCapturer` + `DownstreamSink`）
3. 阶段二：`VectorPushdownExecutor` 真实向量搜索下推（复用 `HybridSearcher` + `FilterPushdown`）
4. 转正 API `#[stable]`，POC API `#[deprecated]` + 迁移指引
5. 降级语义保留（主库失败回退缓存 + 向量失败降级主库 + CDC 失败 TTL 兜底）
6. `db-fusion-v2` feature gate 隔离，默认关闭，POC API 保留

## 8.4 REQ-V44-004 结构化查询日志（P2）

1. `QueryLogEntry` JSON 格式含 query_key/sql/params(脱敏)/耗时/阶段/慢标记
2. 复用 `QueryPhaseTiming`，未启用火焰图时 `phase_breakdown` 为空
3. 采样率（默认 1%，慢查询 100%）+ 级别控制（DEBUG/INFO/WARN）
4. 复用 `sz-orm-masking` 参数脱敏
5. `query-logging` feature gate 隔离，默认关闭

## 8.5 REQ-V44-005 性能回归基准线 + CI 自动比对（P2）

1. `PerfBaseline` 含各阶段耗时基线 + 执行计划基线，JSON 序列化
2. `PerfRegression` 检出 `PhaseSlowdown`/`TotalSlowdown`/`PlanRegression`
3. 复用 `PlanRegression` + `check_regressions`，一次比对同时检出耗时与计划回归
4. 复用 `QueryPhaseTiming`，未启用火焰图时仅比对总耗时
5. CI 入口 `check_perf_regressions`，非阻断
6. `perf-baseline` feature gate 隔离，默认关闭

## 8.6 REQ-V44-006 查询智能闭环联动（P3）

1. `IntelligenceLoop` 四步联动（EXPLAIN → 自适应 → 诊断 → 建议）
2. `LoopReport` JSON 含四步结果与最终建议
3. 任一环节失败降级为独立工作，不阻断查询
4. `query-intelligence-loop` feature gate 隔离，默认关闭

---

# 9. feature gate 总览

| feature gate | 所属包 | 控制能力 | 默认 | 对应需求 |
|-------------|--------|---------|------|---------|
| `query-advisor` | sz-orm-advisor（新包）+ sz-orm-explain + sz-orm-adaptive + sz-orm-ai（只读复用） | 查询自动优化建议（规则引擎 + 六种建议类型 + JSON 报告） | 关闭 | REQ-V44-001 |
| `slow-query-diagnosis` | sz-orm-diagnosis（新包）+ sz-orm-flamegraph + sz-orm-adaptive（只读复用） | 慢查询自动诊断（根因分析 + 诊断报告 + 建议联动） | 关闭 | REQ-V44-002 |
| `db-fusion-v2` | sz-orm-fusion（扩展）+ sz-orm-core + sz-orm-queue + sz-orm-vector（只读复用） | db-fusion 转正（TTL 缓存 + 失效广播 + CDC 同步 + 向量下推） | 关闭 | REQ-V44-003 |
| `query-logging` | sz-orm-observability（扩展）+ sz-orm-flamegraph + sz-orm-masking（只读复用） | 结构化查询日志（JSON + 采样 + 脱敏） | 关闭 | REQ-V44-004 |
| `perf-baseline` | sz-orm-explain（扩展）+ sz-orm-flamegraph（只读复用） | 性能回归基准线（耗时基线 + CI 比对） | 关闭 | REQ-V44-005 |
| `query-intelligence-loop` | sz-orm-advisor（扩展）+ sz-orm-explain + sz-orm-adaptive + sz-orm-flamegraph（只读复用） | 查询智能闭环联动（四步闭环 + 报告） | 关闭 | REQ-V44-006 |

---

# 10. 与 v4.3.0 的关系

## 10.1 零重叠声明

v4.4.0 与 v4.3.0 零重叠：

| v4.3.0 能力（采集/检测层） | v4.4.0 能力（分析/建议/转正层） | 关系 |
|---------------------------|-------------------------------|------|
| EXPLAIN 解析（`ExplainPlan`） | 优化建议引擎（基于 `ExplainPlan` 生成建议） | v4.4.0 复用 v4.3.0 结果，不重复解析 |
| 执行计划回归检测（`PlanRegression`） | 性能回归基准线（`PerfBaseline` + `PerfRegression`） | v4.4.0 扩展 v4.3.0 基线为耗时基线 |
| 火焰图采集（`QueryPhaseTiming`） | 慢查询诊断报告（基于 `QueryPhaseTiming` 分析根因） | v4.4.0 复用 v4.3.0 采集，不重复实现 |
| 火焰图采集（`QueryPhaseTiming`） | 结构化查询日志（含 `QueryPhaseTiming` 阶段分解） | v4.4.0 复用 v4.3.0 采集 |
| 自适应决策（`AdaptiveExecutor`） | 优化建议引擎（复用 `should_paginate`/`should_cache`） | v4.4.0 复用 v4.3.0 决策 |
| 自适应决策（`QueryOutcome.slow`） | 慢查询诊断报告（复用 slow 标记触发诊断） | v4.4.0 复用 v4.3.0 标记 |
| db-fusion POC（`FusionExecutor`） | db-fusion 转正（TTL + CDC + 向量下推） | v4.4.0 扩展 v4.3.0 POC 为正式能力 |
| AI 离线优化（`AutoTuningPipeline`） | 优化建议引擎（规则引擎，无 AI 依赖） | v4.4.0 复用建议结构，不替换 AI 闭环 |

## 10.2 依赖关系

```
v4.3.0 已验收基线（7 个 feature gate）
  │
  ├─ explain-analyzer ──→ REQ-V44-001 优化建议（复用 ExplainPlan/PlanRegression）
  │                  ──→ REQ-V44-005 性能基线（复用 PlanSnapshot/check_regressions）
  │                  ──→ REQ-V44-006 闭环联动（复用 ExplainPlan）
  │
  ├─ query-flamegraph ──→ REQ-V44-002 慢查询诊断（复用 QueryPhaseTiming）
  │                  ──→ REQ-V44-004 结构化日志（复用 QueryPhaseTiming）
  │                  ──→ REQ-V44-005 性能基线（复用 QueryPhaseTiming）
  │                  ──→ REQ-V44-006 闭环联动（复用 trace_execute）
  │
  ├─ adaptive-query ───→ REQ-V44-001 优化建议（复用 QueryStats/should_paginate/should_cache）
  │                  ──→ REQ-V44-002 慢查询诊断（复用 QueryOutcome.slow/slow_ms）
  │                  ──→ REQ-V44-006 闭环联动（复用 decide）
  │
  ├─ db-fusion ────────→ REQ-V44-003 db-fusion 转正（扩展 POC 为正式能力）
  │
  └─ (其他 v4.3.0 feature) ──→ 无 v4.4.0 依赖
```

## 10.3 新增包

| 包名 | 对应需求 | 依赖 | 说明 |
|------|---------|------|------|
| `sz-orm-advisor` | REQ-V44-001 / REQ-V44-006 | sz-orm-explain + sz-orm-adaptive + sz-orm-ai（只读复用建议结构） | 查询自动优化建议引擎（规则引擎，无 AI 依赖） |
| `sz-orm-diagnosis` | REQ-V44-002 | sz-orm-flamegraph + sz-orm-adaptive + sz-orm-advisor（建议联动） | 慢查询自动诊断报告 |

## 10.4 扩展包

| 包名 | 对应需求 | 扩展内容 |
|------|---------|---------|
| `sz-orm-fusion` | REQ-V44-003 | TTL 缓存 + 失效广播 + CDC 同步 + 向量下推（`db-fusion-v2` feature） |
| `sz-orm-observability` | REQ-V44-004 | 结构化查询日志（`query-logging` feature） |
| `sz-orm-explain` | REQ-V44-005 | 性能基线（`perf-baseline` feature） |

---

> 文档生成依据：`docs/spec/v4.3.0/` SDD 三阶段文档 + `docs/评估/2026-08-12_db-fusion实验评估.md` + 2026-08-12 逐项代码验证（所有 file:line 证据均已实测存在）
> 审计合规：本文档所有 file:line 证据均引用真实存在的代码，遵循 AGENTS.md 审计合规铁律
> 下一阶段：spec-design-agent 生成 `design.md`（技术设计），spec-task-agent 生成 `tasks.md`（编码任务规划）