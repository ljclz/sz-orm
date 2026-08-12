# sz-orm v4.4.0 技术设计文档

> 版本：v4.4.0（查询自动优化建议 + 慢查询自动诊断报告 + db-fusion 转正 + 结构化查询日志 + 性能回归基准线 + 查询智能闭环联动）
> 基线：v4.3.0（编译期 EXPLAIN 分析 + 查询性能火焰图 + N+1 静态检测 + 数据血缘可视化 + 编译期数据治理 + 自适应查询 + 多数据库融合 POC，5 项需求 REQ-V43-001~005 全部通过 feature gate 隔离，已验收基线）
> 日期：2026-08-12
> 文档定位：技术设计（How to build），对应需求规格 `spec.md`（What to build）
> 设计约束：无 Breaking Change（6 个新 feature gate 隔离，默认全关闭）+ 优先复用既有能力 + 五方言覆盖 + 每项设计附 file:line 代码证据 + unsafe 零容忍 + 禁止占位实现（todo!/unimplemented!/unreachable!）+ 与 v4.3.0 零重叠
> 需求依赖：REQ-V44-001（优化建议）复用既有 `sz-orm-explain` ExplainPlan/PlanRegression + `sz-orm-adaptive` QueryStats/should_paginate/should_cache + `sz-orm-ai` IndexSuggestion/RewriteSuggestion/TuningSuggestion；REQ-V44-002（慢查询诊断）复用既有 `sz-orm-flamegraph` QueryPhaseTiming/Phase + `sz-orm-adaptive` QueryOutcome.slow/slow_ms；REQ-V44-003（db-fusion 转正）复用既有 `sz-orm-fusion` FusionConfig/FusionExecutor/MemoryFusionCache + `sz-orm-core` InvalidationBus/RedisPubSubInvalidationBus + `sz-orm-queue/cdc` DialectCapturer/DownstreamSink + `sz-orm-vector` HybridSearcher/FilterPushdown；REQ-V44-004（结构化日志）复用既有 `sz-orm-observability` MetricsRegistry + `sz-orm-flamegraph` QueryPhaseTiming + `sz-orm-masking` MaskingRule；REQ-V44-005（性能基线）复用既有 `sz-orm-explain` PlanSnapshot/PlanRegression/check_regressions + `sz-orm-flamegraph` QueryPhaseTiming；REQ-V44-006（闭环联动）复用既有 `sz-orm-explain` ExplainPlan + `sz-orm-adaptive` AdaptiveExecutor::decide + `sz-orm-flamegraph` QueryTracer::trace_execute；六项需求主体相互独立，P1（001/002/003）先行，P2（004/005）次之，P3（006）最后
> 证据验证：本文档所有 file:line 证据均已通过源码读取验证（2026-08-12，40+ 项关键证据逐项实测），遵循 AGENTS.md 审计合规铁律

---

# 概述

## 设计目标

本设计文档将 sz-orm v4.4.0 六项查询智能深化与可观测性增强需求（REQ-V44-001 ~ REQ-V44-006）转化为可落地的技术方案，核心目标：

1. **查询自动优化建议引擎**：新增 `sz-orm-advisor` 包，`OptimizationAdvisor` 规则引擎基于既有 EXPLAIN 分析结果 `ExplainPlan`（`packages/sz-orm-explain/src/lib.rs:76`）+ 执行计划回归 `PlanRegression`（`packages/sz-orm-explain/src/regression.rs:69`）+ 自适应统计 `QueryStats`（`packages/sz-orm-adaptive/src/stats.rs:11`），复用既有 AI 建议结构 `IndexSuggestion`/`RewriteSuggestion`/`TuningSuggestion`，通过规则匹配生成六种可执行优化建议（AddIndex/DropIndex/UsePagination/EnableCache/RewriteQuery/AdjustPoolSize），无 AI 依赖。
2. **慢查询自动诊断报告**：新增 `sz-orm-diagnosis` 包，`SlowQueryDiagnoser` 基于既有火焰图阶段耗时 `QueryPhaseTiming`（`packages/sz-orm-flamegraph/src/collector.rs:39`）+ 自适应 slow 标记 `QueryOutcome.slow`（`packages/sz-orm-adaptive/src/executor.rs:116`），分析慢查询根因（PoolExhaustion/SqlInefficiency/LargeResultSet/BuildOverhead/MixedCause）并生成诊断报告（JSON + 人类可读），与优化建议引擎联动。
3. **db-fusion 转正**：扩展既有 `sz-orm-fusion` 包，阶段一 `TtlFusionCache`（TTL 过期 + 失效广播复用 `InvalidationBus` `packages/sz-orm-core/src/l2_cache.rs:82`），阶段二 `CdcSyncCoordinator`（CDC 增量同步复用 `DialectCapturer` `packages/sz-orm-queue/src/cdc/capturer.rs:12` + `DownstreamSink` `packages/sz-orm-queue/src/cdc/downstream.rs:12`）+ `VectorPushdownExecutor`（真实向量搜索下推复用 `HybridSearcher` `packages/sz-orm-vector/src/hybrid_search/searcher.rs:30` + `FilterPushdown` `packages/sz-orm-vector/src/hybrid_search/pushdown.rs:6`），转正 API `#[stable]`，POC API `#[deprecated]`。
4. **结构化查询日志**：扩展既有 `sz-orm-observability`（`MetricsRegistry` `packages/sz-orm-observability/src/lib.rs:253`），新增 `QueryLogger` 结构化日志器输出 `QueryLogEntry`（JSON 格式含查询 SQL/参数/耗时/阶段/慢标记），复用既有 `sz-orm-masking`（`MaskingRule` `packages/sz-orm-masking/src/lib.rs:21`）参数脱敏，支持采样率与级别控制。
5. **性能回归基准线 + CI 自动比对**：扩展既有 `sz-orm-explain`（`PlanSnapshot` `packages/sz-orm-explain/src/regression.rs:23` + `check_regressions` `packages/sz-orm-explain/src/regression.rs:161`），新增 `PerfBaseline`（各阶段耗时基线 + 执行计划基线）+ `PerfRegression`（PhaseSlowdown/TotalSlowdown/PlanRegression）+ `check_perf_regressions` CI 入口，复用既有 `QueryPhaseTiming`。
6. **查询智能闭环联动**：扩展 `sz-orm-advisor`，新增 `IntelligenceLoop` 闭环协调器将 EXPLAIN 分析 → 自适应决策 → 火焰图诊断 → 优化建议四步联动形成闭环，输出 `LoopReport`，任一环节失败降级为独立工作。

## 设计约束

| 约束类别 | 约束内容 | 来源 |
|---------|---------|------|
| 兼容性 | 无 Breaking Change，6 个新 feature gate 隔离，默认全关闭，既有公开 API 完全向后兼容 | spec.md §1.4 / §4.5.1 |
| sz-pay 不破坏 | sz-pay 从 crates.io 拉取 sz-orm-* 6 个包既有用法不受影响 | spec.md §4.5.2 |
| 五方言覆盖 | MySQL/PostgreSQL/SQLite/Oracle/MSSQL 行为一致（优化建议/诊断报告/融合查询按方言能力适配） | spec.md §4.5.3 |
| 复用优先 | 优先复用既有能力，不重复实现（优化建议复用 ExplainPlan/QueryStats/AI 建议结构；诊断复用 QueryPhaseTiming/slow 标记；融合转正复用 InvalidationBus/CDC/HybridSearcher；日志复用 MetricsRegistry/MaskingRule；性能基线复用 PlanSnapshot/check_regressions；闭环复用 decide/trace_execute） | spec.md §1.4 / §8.4 |
| unsafe 零容忍 | 无 `unsafe` 块，或必须有 `// SAFETY:` 注释 | spec.md §1.4.15 / §4.3 |
| 禁止占位实现 | 禁止 `todo!`/`unimplemented!`/`unreachable!` | AGENTS.md |
| 参数化查询 | 任何 WHERE 条件必须参数化，禁止 SQL 字符串拼接 | AGENTS.md |
| 测试基线不回退 | v4.3.0 已验收测试基线（约 7,000 个测试）仅增不减 | spec.md §4.2.9 |
| 审计证据 | 每项结论附 file:line 证据，遵循审计合规铁律 | spec.md §4.3.6 / AGENTS.md |
| 与 v4.3.0 零重叠 | v4.3.0 是"采集/检测"层，v4.4.0 是"分析/建议/转正"层，新增范围全部落在新包或 v4.3.0 不触碰的既有包扩展（feature 隔离） | spec.md §1.4.12 / §10.1 |
| 建议不执行 DDL | 优化建议仅生成文本，不自动执行 DDL，需人工确认 | spec.md §4.3.1 / §5.1.1.5 |
| 诊断仅对慢查询触发 | 慢查询诊断仅对 `slow == true` 查询触发，非每次查询 | spec.md §5.2.1.2 / §5.2.1.6 |

## feature gate 总览

| feature | 所属包 | 控制能力 | 默认 | 对应需求 |
|---------|--------|---------|------|---------|
| `query-advisor` | sz-orm-advisor（新包）+ sz-orm-explain + sz-orm-adaptive + sz-orm-ai（只读复用） | 查询自动优化建议（规则引擎 + 六种建议类型 + JSON 报告） | 关闭 | REQ-V44-001 |
| `slow-query-diagnosis` | sz-orm-diagnosis（新包）+ sz-orm-flamegraph + sz-orm-adaptive（只读复用） | 慢查询自动诊断（根因分析 + 诊断报告 + 建议联动） | 关闭 | REQ-V44-002 |
| `db-fusion-v2` | sz-orm-fusion（扩展）+ sz-orm-core + sz-orm-queue + sz-orm-vector（只读复用） | db-fusion 转正（TTL 缓存 + 失效广播 + CDC 同步 + 向量下推） | 关闭 | REQ-V44-003 |
| `query-logging` | sz-orm-observability（扩展）+ sz-orm-flamegraph + sz-orm-masking（只读复用） | 结构化查询日志（JSON + 采样 + 脱敏） | 关闭 | REQ-V44-004 |
| `perf-baseline` | sz-orm-explain（扩展）+ sz-orm-flamegraph（只读复用） | 性能回归基准线（耗时基线 + CI 比对） | 关闭 | REQ-V44-005 |
| `query-intelligence-loop` | sz-orm-advisor（扩展）+ sz-orm-explain + sz-orm-adaptive + sz-orm-flamegraph（只读复用） | 查询智能闭环联动（四步闭环 + 报告） | 关闭 | REQ-V44-006 |

---

# 一、需求与存量功能关系分析

## 1.1 需求功能与存量功能对比

### 1.1.1 已实现功能（可直接复用）

| 需求功能 | 存量功能 | 代码位置 | 匹配度 |
|---------|---------|---------|--------|
| REQ-V44-001 EXPLAIN 分析结果 | `ExplainPlan`（执行计划摘要，含 scan_type/table/index/rows/extra） | `packages/sz-orm-explain/src/lib.rs:76` | 100% |
| REQ-V44-001 缺失索引判断 | `ExplainPlan::missing_index`（全表扫描或行数超阈值且未使用索引） | `packages/sz-orm-explain/src/lib.rs:91` | 100% |
| REQ-V44-001 执行计划回归 | `PlanRegression`（ScanTypeUpgrade/IndexLost/RowsGrowth） | `packages/sz-orm-explain/src/regression.rs:69` | 100% |
| REQ-V44-001/005 基线快照 | `PlanSnapshot`（query_key + plan + captured_at，JSON 序列化） | `packages/sz-orm-explain/src/regression.rs:23` | 100% |
| REQ-V44-001/005 基线集合 | `PlanBaseline`（snapshots: HashMap，JSON 文件格式） | `packages/sz-orm-explain/src/regression.rs:34` | 100% |
| REQ-V44-005 CI 回归入口 | `check_regressions`（baseline_json + current_json + factor → Vec<PlanRegression>） | `packages/sz-orm-explain/src/regression.rs:161` | 100% |
| REQ-V44-001 运行时统计 | `QueryStats`（total_executions/total_rows/total_time_us，AtomicU64 无锁） | `packages/sz-orm-adaptive/src/stats.rs:11` | 100% |
| REQ-V44-001 大结果集判断 | `QueryStats::should_paginate`（平均行数超阈值 → 建议游标分页） | `packages/sz-orm-adaptive/src/stats.rs:66` | 100% |
| REQ-V44-001 热点查询判断 | `QueryStats::should_cache`（平均耗时超阈值且执行次数达下限 → 建议缓存） | `packages/sz-orm-adaptive/src/stats.rs:73` | 100% |
| REQ-V44-001/006 自适应执行器 | `AdaptiveExecutor`（按 query_key 独立统计，线程安全） | `packages/sz-orm-adaptive/src/executor.rs:120` | 100% |
| REQ-V44-001/006 自适应决策 | `AdaptiveExecutor::decide`（按统计选择执行路径 Cached/Paginated/Direct） | `packages/sz-orm-adaptive/src/executor.rs:157` | 100% |
| REQ-V44-002/006 慢查询阈值 | `AdaptiveConfig.slow_ms`（慢查询阈值，默认 100ms） | `packages/sz-orm-adaptive/src/executor.rs:35` | 100% |
| REQ-V44-002 慢查询标记 | `QueryOutcome.slow`（是否慢查询，超过 slow_ms） | `packages/sz-orm-adaptive/src/executor.rs:116` | 100% |
| REQ-V44-002/006 查询阶段枚举 | `Phase`（Build/Bind/PoolAcquire/SqlExecute/ResultMap） | `packages/sz-orm-flamegraph/src/collector.rs:11` | 100% |
| REQ-V44-002/004/005 阶段耗时 | `QueryPhaseTiming`（phase + start_ms + duration_ms） | `packages/sz-orm-flamegraph/src/collector.rs:39` | 100% |
| REQ-V44-002/006 分阶段计时 | `QueryTracer::trace_execute`（包裹查询执行返回结果 + 各阶段耗时） | `packages/sz-orm-flamegraph/src/collector.rs:61` | 100% |
| REQ-V44-003 融合配置 | `FusionConfig`（primary + cache + search） | `packages/sz-orm-fusion/src/plan.rs:21` | 100% |
| REQ-V44-003 融合规划器 | `FusionPlanner`（纯静态分析，识别可下推缓存/搜索子句） | `packages/sz-orm-fusion/src/plan.rs:104` | 100% |
| REQ-V44-003 融合缓存抽象 | `FusionCache` trait（get/set，Redis 或内存实现可注入） | `packages/sz-orm-fusion/src/executor.rs:8` | 100% |
| REQ-V44-003 内存融合缓存 | `MemoryFusionCache`（进程内 HashMap，POC/测试用） | `packages/sz-orm-fusion/src/executor.rs:16` | 100% |
| REQ-V44-003 降级标记 | `FusionOutcome.degraded`（主库失败返回缓存旧数据时标记） | `packages/sz-orm-fusion/src/executor.rs:55` | 100% |
| REQ-V44-003 融合执行器 | `FusionExecutor`（缓存命中跳过主库 + 主库失败降级回缓存） | `packages/sz-orm-fusion/src/executor.rs:63` | 100% |
| REQ-V44-003 融合执行入口 | `FusionExecutor::execute`（规划 → 缓存命中 → 主库执行 → 回填 → 降级） | `packages/sz-orm-fusion/src/executor.rs:93` | 100% |
| REQ-V44-003 缓存失效总线 | `InvalidationBus` trait（publish/subscribe，跨实例缓存失效） | `packages/sz-orm-core/src/l2_cache.rs:82` | 100% |
| REQ-V44-003 本地失效总线 | `LocalInvalidationBus`（进程内 broadcast，单实例用） | `packages/sz-orm-core/src/l2_cache.rs:93` | 100% |
| REQ-V44-003 Redis 失效总线 | `RedisPubSubInvalidationBus`（Redis Pub/Sub 跨实例失效广播） | `packages/sz-orm-core/src/dist_cache.rs:41` | 100% |
| REQ-V44-003 Gossip 失效总线 | `GossipInvalidationBus`（点对点 Gossip 跨实例失效广播） | `packages/sz-orm-core/src/dist_cache.rs:179` | 100% |
| REQ-V44-003 方言 CDC 捕获器 | `DialectCapturer` trait（start_capture → Stream<ChangeEvent>，5 方言） | `packages/sz-orm-queue/src/cdc/capturer.rs:12` | 100% |
| REQ-V44-003 下游分发 | `DownstreamSink` trait（send + name，7 种下游） | `packages/sz-orm-queue/src/cdc/downstream.rs:12` | 100% |
| REQ-V44-003 并行分发 | `distribute_to_all`（并行分发到所有下游 sink） | `packages/sz-orm-queue/src/cdc/downstream.rs:178` | 100% |
| REQ-V44-003 CDC 断点续传 | `CdcCheckpoint`（CDC 断点续传） | `packages/sz-orm-queue/src/cdc/checkpoint.rs` | 100% |
| REQ-V44-003 CDC 脱敏 | CDC 变更事件脱敏（尊重既有脱敏规则） | `packages/sz-orm-queue/src/cdc/masking.rs` | 100% |
| REQ-V44-003 混合搜索器 | `HybridSearcher`（三源并行查询 + 融合排序：向量/全文/结构化） | `packages/sz-orm-vector/src/hybrid_search/searcher.rs:30` | 100% |
| REQ-V44-003 搜索降级状态 | `DegradationStatus`（向量搜索降级语义） | `packages/sz-orm-vector/src/hybrid_search/searcher.rs:60` | 100% |
| REQ-V44-003 过滤下推 | `FilterPushdown`（结构化过滤下推到向量/全文源） | `packages/sz-orm-vector/src/hybrid_search/pushdown.rs:6` | 100% |
| REQ-V44-004 指标注册中心 | `MetricsRegistry`（Counter/Gauge/Histogram，RwLock 线程安全） | `packages/sz-orm-observability/src/lib.rs:253` | 100% |
| REQ-V44-004 Prometheus server | `start_metrics_server`（/metrics 端点，Prometheus 文本格式） | `packages/sz-orm-observability/src/lib.rs:421` | 100% |
| REQ-V44-002/004 脱敏规则 | `MaskingRule`（Phone/Email/IdCard/BankCard/Name/Address/Ip/Imei...） | `packages/sz-orm-masking/src/lib.rs:21` | 100% |
| REQ-V44-001 AI 索引建议结构 | `IndexSuggestion`（index_columns + index_type + ddl_text + expected_benefit + evidence） | `packages/sz-orm-ai/src/index_advisor.rs:71` | 100% |
| REQ-V44-001 AI 改写建议结构 | `RewriteSuggestion`（original_sql + rewritten_sql + transform_type + equivalence_proof） | `packages/sz-orm-ai/src/rewrite_advisor.rs:61` | 100% |
| REQ-V44-001 AI 调优建议结构 | `TuningSuggestion`（suggestion_type + sql_before + sql_after + expected_gain） | `packages/sz-orm-ai/src/auto_tuning/mod.rs:71` | 100% |
| REQ-V44-001 AI 离线调优流水线 | `AutoTuningPipeline`（四阶段闭环 Detect→Advise→Apply→Verify，424 LOC） | `packages/sz-orm-ai/src/auto_tuning/pipeline.rs:15` | 100% |

### 1.1.2 需要扩展的功能

| 需求功能 | 存量功能 | 差异说明 | 扩展方向 |
|---------|---------|---------|---------|
| REQ-V44-001 优化建议生成 | 既有 `ExplainPlan`（`:76`）仅检测问题（全表扫描/缺失索引），`PlanRegression`（`:69`）仅检测回归，不生成可执行优化建议（如"添加索引 idx_xxx ON users(email)"） | 既有"检测问题"缺"生成建议"；输入输出差异：既有输出 bool/enum 判定，需扩展输出 `OptimizationSuggestion`（含 action/confidence/estimated_improvement） | 新增 `sz-orm-advisor` 包，`OptimizationAdvisor` 规则引擎 `suggest(plan, stats) -> Vec<OptimizationSuggestion>`，复用既有 `ExplainPlan`/`QueryStats`/AI 建议结构，不修改既有分析逻辑 |
| REQ-V44-002 慢查询根因分析 | 既有 `QueryPhaseTiming`（`:39`）仅采集各阶段耗时，`QueryOutcome.slow`（`:116`）仅标记慢查询，不分析根因（PoolAcquire 高 → 连接池不足；SqlExecute 高 → SQL 优化） | 既有"采集耗时"缺"分析根因 + 生成诊断报告"；输入输出差异：既有输出耗时数据，需扩展输出 `DiagnosisReport`（含 root_cause/phase_breakdown/suggestions/severity） | 新增 `sz-orm-diagnosis` 包，`SlowQueryDiagnoser::diagnose` 基于阶段耗时占比判定根因，复用既有 `QueryPhaseTiming`/`QueryOutcome.slow`，与 `sz-orm-advisor` 联动生成建议 |
| REQ-V44-003 TTL 融合缓存 | 既有 `MemoryFusionCache`（`:16`）无 TTL 过期，`FusionCache` trait（`:8`）无 `get_with_ttl`/`set_with_ttl` | 既有缓存永不过期，缺 TTL 过期 + 失效广播；输入输出差异：既有 get/set 无 TTL 参数，需扩展带 TTL 的方法 | 扩展 `sz-orm-fusion`，新增 `TtlFusionCache`（TTL 过期检查）+ `FusionCache` trait 扩展 `get_with_ttl`/`set_with_ttl`，`FusionExecutor::with_invalidation_bus` 复用既有 `InvalidationBus`，`db-fusion-v2` feature 隔离 |
| REQ-V44-003 真实向量搜索下推 | 既有 `FusionExecutor::execute`（`:93`）搜索下推仅记录数据源（`:118` 注释"POC 阶段仅记录数据源，真实向量检索由调用方在 primary 闭包内完成"） | 既有"记录数据源"缺"真实向量检索"；输入输出差异：既有搜索下推不执行检索，需扩展为调用 `HybridSearcher::search` | 扩展 `sz-orm-fusion`，新增 `VectorPushdownExecutor` 调用既有 `HybridSearcher::search`（`:30`）+ `FilterPushdown`（`:6`），转正 POC 的搜索下推 |
| REQ-V44-003 CDC 增量同步集成 | 既有 `DialectCapturer`（`:12`）与 `DownstreamSink`（`:12`）独立工作，未集成到融合查询 | 既有 CDC 基础设施存在，缺融合查询集成协调器；输入输出差异：既有 CDC 独立捕获，需扩展协调主库变更 → 缓存/搜索索引同步 | 扩展 `sz-orm-fusion`，新增 `CdcSyncCoordinator` 复用既有 `DialectCapturer` + `DownstreamSink`/`distribute_to_all` + `CdcCheckpoint`，主库变更自动同步到缓存/搜索索引 |
| REQ-V44-004 结构化查询日志 | 既有 `MetricsRegistry`（`:253`）仅 Prometheus 指标导出，无结构化查询日志（JSON 含 SQL/参数/耗时/阶段/慢标记） | 既有"指标导出"缺"结构化日志"；输入输出差异：既有输出 Prometheus 文本，需扩展输出 `QueryLogEntry` JSON | 扩展 `sz-orm-observability`，新增 `QueryLogger` + `QueryLogEntry`，复用既有 `QueryPhaseTiming` + `MaskingRule` 参数脱敏，`query-logging` feature 隔离 |
| REQ-V44-005 耗时基线 | 既有 `PlanSnapshot`（`:23`）仅执行计划基线（ExplainPlan），无各阶段耗时基线 | 既有"计划基线"缺"耗时基线"；输入输出差异：既有基线含 ExplainPlan，需扩展含 `HashMap<Phase, u64>` 各阶段耗时 | 扩展 `sz-orm-explain`，新增 `PerfBaseline`（phase_baselines + plan）+ `PerfRegression`（PhaseSlowdown/TotalSlowdown/PlanRegression）+ `check_perf_regressions`，复用既有 `PlanSnapshot`/`check_regressions`/`QueryPhaseTiming`，`perf-baseline` feature 隔离 |
| REQ-V44-006 闭环联动 | 既有 `ExplainPlan`/`AdaptiveExecutor::decide`/`QueryTracer::trace_execute` 三项能力独立工作，无闭环协调 | 既有"独立能力"缺"闭环协调"；输入输出差异：既有各能力独立调用，需扩展协调器串联四步 | 扩展 `sz-orm-advisor`，新增 `IntelligenceLoop` 闭环协调器 `run_loop` 串联 EXPLAIN → 自适应 → 诊断 → 建议，输出 `LoopReport`，`query-intelligence-loop` feature 隔离 |

### 1.1.3 需要新增的功能或接口

#### 模块 A：REQ-V44-001 查询自动优化建议引擎

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖关系 |
|--------|------|------|---------|---------|
| `sz-orm-advisor` 包（新包） | `ExplainPlan` + `QueryStats` | `Vec<OptimizationSuggestion>` | `OptimizationAdvisor` 规则引擎 + `suggest(plan, stats)` 入口，规则匹配生成建议，无 AI 依赖 | 依赖 `sz-orm-explain` ExplainPlan/PlanRegression + `sz-orm-adaptive` QueryStats/should_paginate/should_cache + `sz-orm-ai` 建议结构（只读复用） |
| `OptimizationSuggestion` 统一结构 | 建议生成 | 建议数据 | `suggestion_type`/`target_query`/`description`/`action`/`confidence`/`estimated_improvement`，`SuggestionType` 六种枚举 | 可转换为既有 `IndexSuggestion`/`RewriteSuggestion`/`TuningSuggestion` |
| 六种建议类型规则 | `ExplainPlan` + `QueryStats` | `OptimizationSuggestion` | (a) FullTable + rows > 阈值 → `AddIndex`；(b) 冗余索引 → `DropIndex`；(c) `should_paginate` → `UsePagination`；(d) `should_cache` → `EnableCache`；(e) 可改写查询 → `RewriteQuery`；(f) PoolAcquire 耗时高 → `AdjustPoolSize` | 复用 `ExplainPlan::missing_index`（`:91`）+ `should_paginate`（`:66`）+ `should_cache`（`:73`） |
| JSON 建议报告 | `&[OptimizationSuggestion]` | JSON String | `to_json` 序列化，可被 CI/IDE 消费 | — |

#### 模块 B：REQ-V44-002 慢查询自动诊断报告

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖关系 |
|--------|------|------|---------|---------|
| `sz-orm-diagnosis` 包（新包） | `Vec<QueryPhaseTiming>` + `QueryOutcome` | `DiagnosisReport` | `SlowQueryDiagnoser::diagnose` 基于阶段耗时占比判定根因，仅对 `slow == true` 触发 | 依赖 `sz-orm-flamegraph` QueryPhaseTiming/Phase + `sz-orm-adaptive` QueryOutcome.slow/slow_ms + `sz-orm-advisor` 建议联动 |
| `RootCause` 根因枚举 | 阶段耗时占比 | 根因类型 | PoolAcquire > 30% → `PoolExhaustion`；SqlExecute > 50% → `SqlInefficiency`；ResultMap > 30% → `LargeResultSet`；Build > 20% → `BuildOverhead`；多阶段高 → `MixedCause` | — |
| `DiagnosisReport` 报告结构 | 诊断结果 | 报告数据 | `query_key`/`total_elapsed_ms`/`root_cause`/`phase_breakdown`/`suggestions`/`severity` | 建议列表来自 `sz-orm-advisor` 联动 |
| 双格式输出 | `DiagnosisReport` | JSON / 人类可读 | `to_json`（CI 消费）+ `to_human_readable`（含根因/阶段/建议表格） | — |

#### 模块 C：REQ-V44-003 db-fusion 转正

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖关系 |
|--------|------|------|---------|---------|
| `TtlFusionCache` TTL 缓存 | 缓存键 + TTL | 缓存值（过期返回 None） | TTL 过期检查（Instant::now vs 过期时间），`FusionCache` trait 扩展 `get_with_ttl`/`set_with_ttl` | 复用既有 `FusionCache` trait（`:8`）+ `MemoryFusionCache`（`:16`）模式 |
| 失效广播集成 | 主库写入事件 | 跨实例缓存失效 | `FusionExecutor::with_invalidation_bus(bus)`，主库写入后 `bus.publish` 失效消息 | 复用既有 `InvalidationBus`（`:82`）/ `RedisPubSubInvalidationBus`（`:41`）/ `GossipInvalidationBus`（`:179`） |
| `CdcSyncCoordinator` CDC 同步 | 主库变更事件 | 缓存/搜索索引增量更新 | 协调 `DialectCapturer::start_capture` → `distribute_to_all` 分发到缓存失效 + 搜索索引更新下游 | 复用既有 `DialectCapturer`（`:12`）+ `DownstreamSink`（`:12`）+ `distribute_to_all`（`:178`）+ `CdcCheckpoint` |
| `VectorPushdownExecutor` 向量下推 | 融合查询含 search 条件 | 向量搜索结果 | 调用 `HybridSearcher::search` 三源并行查询 + `FilterPushdown` 结构化过滤下推 | 复用既有 `HybridSearcher`（`:30`）+ `FilterPushdown`（`:6`）+ `DegradationStatus`（`:60`）降级 |
| 转正 API 稳定标注 | 转正 API | `#[stable]` 标注 | 转正 API 标注 `#[stable]`，POC API `#[deprecated(note = "use TtlFusionCache + CdcSyncCoordinator")]` + 迁移指引 | — |

#### 模块 D：REQ-V44-004 结构化查询日志

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖关系 |
|--------|------|------|---------|---------|
| `QueryLogger` 结构化日志器 | 查询执行信息 | `QueryLogEntry` JSON | `log(query_key, sql, params, timings, slow)`，采样判定 + 级别过滤 + 参数脱敏 | 复用既有 `MetricsRegistry`（`:253`）关联指标 + `MaskingRule`（`:21`）参数脱敏 |
| `QueryLogEntry` 日志结构 | 日志数据 | JSON | `query_key`/`sql`/`params`（脱敏）/`total_elapsed_ms`/`phase_breakdown`/`slow`/`from_cache`/`timestamp` | `phase_breakdown` 复用既有 `QueryPhaseTiming`（`:39`） |
| 采样与级别控制 | 采样率 + 级别 | 是否输出 | `with_sample_rate(rate)`（默认 1%，慢查询 100%）+ `with_level(level)`（DEBUG/INFO/WARN） | — |

#### 模块 E：REQ-V44-005 性能回归基准线 + CI 自动比对

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖关系 |
|--------|------|------|---------|---------|
| `PerfBaseline` 耗时基线 | 查询各阶段耗时 + 执行计划 | 基线 JSON | `phase_baselines: HashMap<Phase, u64>` + `plan: ExplainPlan` + `captured_at`，JSON 序列化版本化管理 | 复用既有 `PlanSnapshot`（`:23`）+ `QueryPhaseTiming`（`:39`） |
| `PerfRegression` 性能回归 | 基线 + 当前耗时 | `Vec<PerfRegression>` | `PhaseSlowdown`（阶段耗时增长）/ `TotalSlowdown`（总耗时增长）/ `PlanRegression`（执行计划回归，复用既有） | 复用既有 `PlanRegression`（`:69`） |
| `check_perf_regressions` CI 入口 | baseline_json + current_json + threshold_factor | `Vec<PerfRegression>` | 比对各阶段耗时 + 执行计划，一次比对同时检出耗时回归与计划回归 | 复用既有 `check_regressions`（`:161`） |

#### 模块 F：REQ-V44-006 查询智能闭环联动

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖关系 |
|--------|------|------|---------|---------|
| `IntelligenceLoop` 闭环协调器 | query_key + 查询 | `LoopReport` | `run_loop` 串联四步：1. EXPLAIN 分析 → 2. 自适应决策 → 3. 火焰图诊断 → 4. 优化建议，任一环节失败降级跳过 | 复用既有 `ExplainPlan`（`:76`）+ `AdaptiveExecutor::decide`（`:157`）+ `QueryTracer::trace_execute`（`:61`）+ `sz-orm-advisor` suggest |
| `LoopReport` 闭环报告 | 闭环结果 | JSON | `query_key`/`explain_result`/`adaptive_decision`/`diagnosis_result`/`suggestions`/`loop_elapsed_ms` | — |

## 1.2 存量功能详细分析

### 1.2.1 ExplainPlan / PlanRegression / PlanSnapshot（EXPLAIN 分析与回归基线）

**接口契约**：
- `pub struct ExplainPlan { pub scan_type: ScanType, pub table: String, pub index: Option<String>, pub rows: u64, pub extra: Vec<String> }`（`explain/lib.rs:76`）：执行计划摘要，`#[cfg_attr(feature = "explain-analyzer", derive(Serialize, Deserialize))]`。
- `ExplainPlan::missing_index(&self, row_threshold: u64) -> bool`（`:91`）：全表扫描或行数超阈值且未使用索引。
- `pub enum PlanRegression { ScanTypeUpgrade { query_key, from, to }, IndexLost { query_key, index }, RowsGrowth { query_key, before, after } }`（`regression.rs:69`）：回归类型，`Serialize + Deserialize`。
- `pub struct PlanSnapshot { pub query_key: String, pub plan: ExplainPlan, pub captured_at: String }`（`:23`）：基线快照，JSON 序列化。
- `pub struct PlanBaseline { pub snapshots: HashMap<String, PlanSnapshot> }`（`:34`）：基线集合。
- `pub fn check_regressions(baseline_json: &str, current_json: &str, rows_growth_factor: u64) -> Result<Vec<PlanRegression>, serde_json::Error>`（`:161`）：CI 入口，比对基线与当前计划。

**业务规则**：`ExplainPlan` 由 5 方言解析器（`parser_for` `:127`）解析 EXPLAIN 输出；`missing_index` 判定缺失索引；`check_regressions` 反序列化基线 + 当前，逐 query_key 比对 `compare`。

**扩展点**：`ExplainPlan`/`PlanRegression`/`PlanSnapshot` 为纯数据结构，可被 `sz-orm-advisor` 规则引擎直接消费生成建议；`PerfBaseline` 扩展 `PlanSnapshot` 增加耗时基线。

**约束**：`#[cfg_attr(feature = "explain-analyzer", ...)]` 门控序列化；既有结构保留不动，新增为独立包/feature 扩展。

### 1.2.2 QueryStats / AdaptiveExecutor / QueryOutcome（自适应统计与决策）

**接口契约**：
- `pub struct QueryStats { total_executions: AtomicU64, total_rows: AtomicU64, total_time_us: AtomicU64 }`（`adaptive/stats.rs:11`）：无锁统计，`record(rows, time_us)` 原子累加。
- `QueryStats::should_paginate(&self, threshold_rows: u64) -> bool`（`:66`）：平均行数超阈值 → 建议游标分页。
- `QueryStats::should_cache(&self, threshold_ms: u64, min_executions: u64) -> bool`（`:73`）：平均耗时超阈值且执行次数达下限 → 建议缓存。
- `pub struct AdaptiveExecutor { stats: Mutex<HashMap<String, Arc<QueryStats>>>, config: AdaptiveConfig, cache: Option<Arc<dyn ResultCache>> }`（`executor.rs:120`）：线程安全，按 query_key 独立统计。
- `AdaptiveExecutor::decide(&self, query_key: &str) -> ExecutionPath`（`:157`）：按统计选择执行路径（Cached/Paginated/Direct）。
- `AdaptiveConfig.slow_ms: u64`（`:35`，默认 100）：慢查询阈值。
- `QueryOutcome.slow: bool`（`:116`）：是否慢查询（超过 slow_ms）。

**业务规则**：`QueryStats` 用 `AtomicU64` 无锁采集（开销 < 1μs/次）；`should_paginate`/`should_cache` 为决策谓词；`decide` 按配置 + 统计选路径；`slow` 标记超阈值查询。

**扩展点**：`QueryStats`/`should_paginate`/`should_cache` 可被 `sz-orm-advisor` 规则引擎直接消费生成 `UsePagination`/`EnableCache` 建议；`QueryOutcome.slow` 可被 `sz-orm-diagnosis` 触发诊断；`decide` 可被 `IntelligenceLoop` 闭环调用。

**约束**：`AdaptiveExecutor` 线程安全（`Mutex` + `Arc`）；缓存默认关闭（`cache_enabled: false`）防脏读；既有逻辑保留不动。

### 1.2.3 Phase / QueryPhaseTiming / QueryTracer（火焰图阶段计时）

**接口契约**：
- `pub enum Phase { Build, Bind, PoolAcquire, SqlExecute, ResultMap }`（`flamegraph/collector.rs:11`）：查询生命周期阶段，`as_str` 返回阶段名。
- `pub struct QueryPhaseTiming { pub phase: Phase, pub start_ms: u64, pub duration_ms: u64 }`（`:39`）：单阶段耗时记录。
- `pub fn trace_execute<F, T>(f: F) -> (T, Vec<QueryPhaseTiming>)`（`:61`）：包裹查询执行返回结果 + 各阶段耗时，`PhaseRecorder` 分阶段计时。

**业务规则**：`trace_execute` 用 `Instant::now` 分阶段计时（精度 < 1ms）；`with_tracer` 写入既有 `Tracer` span（`query-flamegraph` feature）。

**扩展点**：`QueryPhaseTiming` 可被 `sz-orm-diagnosis` 根因分析直接消费；可被 `sz-orm-observability` 结构化日志 `phase_breakdown` 消费；可被 `PerfBaseline` 耗时基线消费；`trace_execute` 可被 `IntelligenceLoop` 闭环调用。

**约束**：`QueryTracer` 无锁单线程使用；既有采集保留不动，新增为独立包/feature 扩展。

### 1.2.4 FusionConfig / FusionExecutor / FusionCache / FusionOutcome（融合查询 POC）

**接口契约**：
- `pub struct FusionConfig { pub primary: String, pub cache: Option<CacheBackend>, pub search: Option<SearchBackend> }`（`fusion/plan.rs:21`）：融合配置。
- `pub struct FusionPlanner`（`:104`）：纯静态分析规划器，`plan(query, config) -> FusionPlan`。
- `pub trait FusionCache: Send + Sync { fn get(&self, key: &str) -> Option<String>; fn set(&self, key: &str, value: String); }`（`executor.rs:8`）：融合缓存抽象。
- `pub struct MemoryFusionCache { inner: Mutex<HashMap<String, String>> }`（`:16`）：进程内内存缓存（无 TTL）。
- `pub struct FusionOutcome { pub rows: Vec<serde_json::Value>, pub from_cache: bool, pub degraded: bool, pub sources: Vec<String>, pub elapsed_ms: u64 }`（`:49`）：融合查询结果，`degraded` 标记降级。
- `pub struct FusionExecutor { config: FusionConfig, cache: Option<Arc<dyn FusionCache>> }`（`:63`）：融合执行器。
- `FusionExecutor::execute<F>(&self, query: &FusionQuery, primary: F) -> Result<FusionOutcome, String>`（`:93`）：规划 → 缓存命中（返回，主库跳过）→ 主库执行（回填缓存）→ 主库失败降级回缓存。

**业务规则**：`FusionPlanner::plan` 静态分析识别可下推缓存/搜索子句（仅可证明安全的拆分：主键等值 + 缓存键）；`execute` 缓存命中跳过主库，主库失败回退缓存 + `degraded` 标记；搜索下推 POC 阶段仅记录数据源（`:118`）。

**扩展点**：`FusionCache` trait 可扩展 `get_with_ttl`/`set_with_ttl`（TTL 缓存）；`FusionExecutor` 可扩展 `with_invalidation_bus`（失效广播）；搜索下推可转正为 `VectorPushdownExecutor` 调用 `HybridSearcher`；POC API `#[deprecated]` 标注。

**约束**：POC 仅支持可证明安全的拆分；既有 POC API 保留不动（`#[deprecated]` 标注而非删除）；`db-fusion-v2` feature 隔离转正能力。

### 1.2.5 InvalidationBus / RedisPubSubInvalidationBus / GossipInvalidationBus（缓存失效总线）

**接口契约**：
- `pub trait InvalidationBus: Send + Sync { fn publish(&self, message: InvalidationMessage); fn subscribe(&self) -> Box<dyn Iterator<Item = InvalidationMessage> + Send>; }`（`core/l2_cache.rs:82`）：缓存失效总线，跨实例缓存失效。
- `pub struct LocalInvalidationBus { tx: tokio::sync::broadcast::Sender<InvalidationMessage> }`（`:93`）：进程内失效总线（broadcast）。
- `pub struct RedisPubSubInvalidationBus { client: Option<redis::aio::ConnectionManager>, channel: String, ... }`（`dist_cache.rs:41`）：Redis Pub/Sub 跨实例失效广播。
- `pub struct GossipInvalidationBus { nodes: Vec<NodeAddr>, ... }`（`:179`）：点对点 Gossip 跨实例失效广播。

**业务规则**：`publish` 发布失效消息，`subscribe` 订阅消息；Redis Pub/Sub 自动重连 + 跳过本实例 instance_id 避免自回环；Gossip 点对点 + HMAC 认证 + seen_messages 去重。

**扩展点**：`InvalidationBus` trait 可被 `FusionExecutor::with_invalidation_bus` 注入，主库写入后 `publish` 失效消息，跨实例缓存失效。

**约束**：`Send + Sync` 线程安全；既有失效总线保留不动，db-fusion 转正复用不重复实现。

### 1.2.6 DialectCapturer / DownstreamSink / distribute_to_all / CdcCheckpoint（CDC 基础设施）

**接口契约**：
- `#[async_trait] pub trait DialectCapturer: Send + Sync { async fn start_capture(&self, checkpoint: Option<CdcCheckpoint>) -> Result<Pin<Box<dyn Stream<Item = ChangeEvent> + Send>>, CdcError>; }`（`queue/cdc/capturer.rs:12`）：方言 CDC 捕获器，5 方言（MySQL Binlog/PG WAL/SQLite Trigger/Oracle LogMiner/MSSQL CDC）。
- `#[async_trait] pub trait DownstreamSink: Send + Sync { async fn send(&self, event: &ChangeEvent) -> Result<(), CdcError>; fn name(&self) -> &str; }`（`downstream.rs:12`）：下游分发 sink，7 种下游（Kafka/RabbitMq/Nats/Pulsar/RocketMq/ActiveMq/HttpWebhook）。
- `pub async fn distribute_to_all(sinks: &[Box<dyn DownstreamSink>], event: &ChangeEvent) -> Vec<Result<(), CdcError>>`（`:178`）：并行分发到所有下游。
- `CdcCheckpoint`（`checkpoint.rs`）：CDC 断点续传。

**业务规则**：`start_capture` 从 checkpoint 启动变更捕获流式事件；`distribute_to_all` 并行分发到所有下游 sink；CDC 变更事件脱敏（`masking.rs`）尊重既有脱敏规则。

**扩展点**：`DialectCapturer` + `DownstreamSink` 可被 `CdcSyncCoordinator` 集成到融合查询，主库变更 → 缓存失效 + 搜索索引更新。

**约束**：`Send + Sync` 线程安全；既有 CDC 保留不动，db-fusion 转正复用不重复实现；CDC 失败降级为 TTL 过期兜底。

### 1.2.7 HybridSearcher / DegradationStatus / FilterPushdown（向量搜索基础）

**接口契约**：
- `pub struct HybridSearcher { vector_store: Option<Arc<dyn VectorSearchSource>>, fulltext_store: Option<Arc<dyn FulltextSearchSource>>, structured_conn: Option<Arc<dyn StructuredSearchSource>> }`（`vector/hybrid_search/searcher.rs:30`）：三源并行查询 + 融合排序。
- `HybridSearcher::search(&self, query: &HybridQuery) -> Result<HybridSearchResponse, HybridError>`（`:51`）：`tokio::join!` 三源并行查询。
- `DegradationStatus`（`:60`）：向量搜索降级状态（默认）。
- `pub struct FilterPushdown`（`pushdown.rs:6`）：结构化过滤下推，`pushdown_to_vector` 将结构化过滤下推到向量查询。

**业务规则**：`search` 三源并行（`tokio::join!`），任一源失败降级（`DegradationStatus`）；`FilterPushdown` 将结构化过滤下推到向量/全文源。

**扩展点**：`HybridSearcher::search` 可被 `VectorPushdownExecutor` 调用执行真实向量检索；`FilterPushdown` 结构化过滤下推；`DegradationStatus` 向量搜索失败降级为主库查询。

**约束**：既有向量搜索保留不动，db-fusion 转正复用不重复实现；向量搜索失败降级不返回错误。

### 1.2.8 MetricsRegistry / start_metrics_server（可观测性基础）

**接口契约**：
- `pub struct MetricsRegistry { counters: RwLock<HashMap<String, Arc<Counter>>>, gauges: RwLock<HashMap<String, Arc<Gauge>>>, histograms: RwLock<HashMap<String, Arc<Histogram>>>, metas: RwLock<Vec<MetricMeta>> }`（`observability/lib.rs:253`）：指标注册中心，Counter/Gauge/Histogram。
- `pub async fn start_metrics_server(registry: Arc<MetricsRegistry>, addr: SocketAddr) -> Result<(), io::Error>`（`:421`）：Prometheus HTTP server，`/metrics` 端点。

**业务规则**：`MetricsRegistry` 用 `RwLock` 线程安全管理指标；`start_metrics_server` 暴露 Prometheus 文本格式指标。

**扩展点**：`MetricsRegistry` 可被 `QueryLogger` 关联指标（查询计数/耗时直方图）；新增 `QueryLogger` 结构化日志为扩展，不修改既有指标导出。

**约束**：既有 Prometheus 指标保留不动；新增结构化日志为 `query-logging` feature 扩展。

### 1.2.9 MaskingRule（脱敏规则）

**接口契约**：`pub enum MaskingRule { Phone, Email, IdCard, BankCard, Name, Address, Ip, Imei, ... }`（`masking/lib.rs:21`）：脱敏策略枚举，`Serialize + Deserialize`。

**业务规则**：每种策略对应特定脱敏算法（如 Email 保留首字符 + @域名，Phone 138****8000）。

**扩展点**：`MaskingRule` 可被 `QueryLogger` 参数脱敏（结构化查询日志 `params` 字段）；可被 `SlowQueryDiagnoser` 诊断报告参数脱敏。

**约束**：既有脱敏运行时执行，保留不动；结构化日志/诊断报告复用此枚举执行脱敏。

### 1.2.10 IndexSuggestion / RewriteSuggestion / TuningSuggestion / AutoTuningPipeline（AI 建议结构）

**接口契约**：
- `pub struct IndexSuggestion { pub index_columns: Vec<String>, pub index_type: IndexType, pub ddl_text: String, pub expected_benefit: BenefitEstimate, pub evidence: Vec<QueryPattern> }`（`ai/index_advisor.rs:71`）：AI 索引建议。
- `pub struct RewriteSuggestion { pub original_sql: String, pub rewritten_sql: String, pub transform_type: TransformType, pub equivalence_proof: EquivalenceProof, ... }`（`rewrite_advisor.rs:61`）：AI 改写建议。
- `pub struct TuningSuggestion { pub suggestion_type: SuggestionType, pub sql_before: String, pub sql_after: String, pub expected_gain: Option<f32>, ... }`（`auto_tuning/mod.rs:71`）：AI 调优建议。
- `pub struct AutoTuningPipeline { detector: SlowQueryDetector, config: AutoTuningConfig }`（`auto_tuning/pipeline.rs:15`）：AI 离线调优流水线（四阶段闭环 Detect→Advise→Apply→Verify，424 LOC）。

**业务规则**：AI 建议结构含 DDL 文本/改写 SQL/等价性论证/预期收益；`AutoTuningPipeline` 离线检测慢查询 → AI 建议优化 → 应用 → 验证，需 AI 依赖。

**扩展点**：`IndexSuggestion`/`RewriteSuggestion`/`TuningSuggestion` 可被 `OptimizationSuggestion` 转换复用（规则引擎生成的建议可转换为既有 AI 建议结构），不重复实现建议数据模型。

**约束**：既有 AI 离线优化保留不动；新增规则引擎为独立包（`sz-orm-advisor`），无 AI 依赖，复用建议结构不替换 AI 闭环。

### 1.2.11 feature gate 体系模式

**接口契约**：`packages/sz-orm-core/Cargo.toml` 已有 25+ feature，v4.2.0 新增 7 个 feature，v4.3.0 新增 7 个 feature（`explain-analyzer`/`query-flamegraph`/`n1-lint`/`lineage-viz`/`compile-governance`/`adaptive-query`/`db-fusion`），默认全关闭。

**业务规则**：feature gate 隔离新能力，默认 feature 行为不变；`#[cfg(feature = "...")]` 门控新增代码；既有 feature 任意组合编译通过。

**扩展点**：v4.4.0 新增 6 个 feature（`query-advisor`/`slow-query-diagnosis`/`db-fusion-v2`/`query-logging`/`perf-baseline`/`query-intelligence-loop`），与既有 feature（含 v4.2.0 7 个 + v4.3.0 7 个）任意组合编译通过。

**约束**：门禁 10（feature 全组合编译）验证；新 feature 默认关闭；既有 feature 组合不破坏。

---

# 二、增量设计方案

## 2.1 实现模型

### 2.1.1 上下文视图

```plantuml
@startuml
title sz-orm v4.4.0 上下文视图（六项查询智能深化与可观测性增强能力）

rectangle "sz-orm v4.4.0 新增/扩展" as orm {
  rectangle "sz-orm-advisor\n(新包)\n查询自动优化建议引擎" as advisor
  rectangle "sz-orm-diagnosis\n(新包)\n慢查询自动诊断报告" as diagnosis
  rectangle "sz-orm-fusion\n(db-fusion-v2 扩展)\nTTL+CDC+向量下推转正" as fusion
  rectangle "sz-orm-observability\n(query-logging 扩展)\n结构化查询日志" as logging
  rectangle "sz-orm-explain\n(perf-baseline 扩展)\n性能回归基准线" as perf
  rectangle "sz-orm-advisor\n(query-intelligence-loop 扩展)\n闭环联动" as loop
}

rectangle "sz-orm 既有（只读复用）" as existing {
  rectangle "sz-orm-explain\nExplainPlan/PlanRegression/PlanSnapshot" as explain
  rectangle "sz-orm-flamegraph\nQueryPhaseTiming/Phase/QueryTracer" as flamegraph
  rectangle "sz-orm-adaptive\nQueryStats/AdaptiveExecutor/QueryOutcome" as adaptive
  rectangle "sz-orm-ai\nIndexSuggestion/RewriteSuggestion/TuningSuggestion" as ai
  rectangle "sz-orm-core\nInvalidationBus/RedisPubSubInvalidationBus" as core
  rectangle "sz-orm-queue/cdc\nDialectCapturer/DownstreamSink" as cdc
  rectangle "sz-orm-vector\nHybridSearcher/FilterPushdown" as vector
  rectangle "sz-orm-masking\nMaskingRule" as masking
}

database "MySQL/PostgreSQL\nSQLite/Oracle/MSSQL" as db
rectangle "Redis\n(缓存+失效广播)" as redis
rectangle "向量库\n(pgvector)" as vecdb
rectangle "CDC 下游\n(Kafka/...)" as downstream
rectangle "Prometheus/Grafana" as prom
actor "应用开发者" as appdev
actor "DBA/性能工程师" as dba
actor "运维/SRE" as sre
actor "CI/CD 流水线" as ci
actor "sz-pay 下游" as downstreamuser

appdev --> advisor : suggest(plan, stats)
advisor --> explain : 复用 ExplainPlan/PlanRegression(:76/:69)
advisor --> adaptive : 复用 should_paginate/should_cache(:66/:73)
advisor --> ai : 复用 IndexSuggestion/RewriteSuggestion(:71/:61)

appdev --> diagnosis : diagnose(timings, outcome)
diagnosis --> flamegraph : 复用 QueryPhaseTiming/Phase(:39/:11)
diagnosis --> adaptive : 复用 QueryOutcome.slow/slow_ms(:116/:35)
diagnosis --> advisor : 建议联动 suggest

sre --> fusion : TtlFusionCache + CdcSyncCoordinator + VectorPushdownExecutor
fusion --> core : 复用 InvalidationBus/RedisPubSubInvalidationBus(:82/:41)
fusion --> cdc : 复用 DialectCapturer/DownstreamSink/distribute_to_all(:12/:178)
fusion --> vector : 复用 HybridSearcher/FilterPushdown(:30/:6)
fusion --> redis : TTL 缓存 + 失效广播
fusion --> vecdb : 真实向量搜索下推
fusion --> db : 主库查询
fusion --> downstream : CDC 增量同步

sre --> logging : QueryLogger
logging --> flamegraph : 复用 QueryPhaseTiming(:39)
logging --> masking : 复用 MaskingRule 参数脱敏(:21)
logging --> prom : JSON 日志 + 指标

dba --> perf : PerfBaseline + check_perf_regressions
perf --> explain : 复用 PlanSnapshot/check_regressions(:23/:161)
perf --> flamegraph : 复用 QueryPhaseTiming(:39)
ci --> perf : CI 性能回归检测

appdev --> loop : run_loop(query) [可选]
loop --> explain : 1. EXPLAIN 分析(复用 :76)
loop --> adaptive : 2. 自适应决策(复用 :157)
loop --> flamegraph : 3. 火焰图诊断(复用 :61)
loop --> advisor : 4. 优化建议(复用 suggest)

ci --> advisor : 优化建议 JSON
ci --> diagnosis : 诊断报告 JSON
ci --> logging : 查询日志
downstreamuser --> orm : API 兼容性验证(默认 feature 不变)

@enduml
```

### 2.1.2 服务/组件总体架构

```plantuml
@startuml
title sz-orm v4.4.0 组件总体架构

package "REQ-V44-001 优化建议" {
  component "sz-orm-advisor\n[query-advisor]" as advisor
}

package "REQ-V44-002 慢查询诊断" {
  component "sz-orm-diagnosis\n[slow-query-diagnosis]" as diagnosis
}

package "REQ-V44-003 db-fusion 转正" {
  component "sz-orm-fusion\n[db-fusion-v2]\nTtlFusionCache\nCdcSyncCoordinator\nVectorPushdownExecutor" as fusion
}

package "REQ-V44-004 结构化日志" {
  component "sz-orm-observability\n[query-logging]\nQueryLogger" as logging
}

package "REQ-V44-005 性能基线" {
  component "sz-orm-explain\n[perf-baseline]\nPerfBaseline/PerfRegression" as perf
}

package "REQ-V44-006 闭环联动" {
  component "sz-orm-advisor\n[query-intelligence-loop]\nIntelligenceLoop" as loop
}

package "既有（只读复用）" #LightGray {
  component "sz-orm-explain\nExplainPlan/PlanRegression" as explain
  component "sz-orm-flamegraph\nQueryPhaseTiming" as flamegraph
  component "sz-orm-adaptive\nQueryStats/AdaptiveExecutor" as adaptive
  component "sz-orm-ai\n建议结构" as ai
  component "sz-orm-core\nInvalidationBus" as core
  component "sz-orm-queue/cdc\nDialectCapturer" as cdc
  component "sz-orm-vector\nHybridSearcher" as vector
  component "sz-orm-masking\nMaskingRule" as masking
  component "sz-orm-observability\nMetricsRegistry" as metrics
}

advisor --> explain : ExplainPlan/PlanRegression 复用
advisor --> adaptive : QueryStats/should_paginate/should_cache 复用
advisor --> ai : 建议结构复用
diagnosis --> flamegraph : QueryPhaseTiming 复用
diagnosis --> adaptive : slow/slow_ms 复用
diagnosis --> advisor : 建议联动
fusion --> core : InvalidationBus 复用
fusion --> cdc : DialectCapturer/DownstreamSink 复用
fusion --> vector : HybridSearcher/FilterPushdown 复用
logging --> flamegraph : QueryPhaseTiming 复用
logging --> masking : MaskingRule 复用
logging --> metrics : MetricsRegistry 关联
perf --> explain : PlanSnapshot/check_regressions 复用
perf --> flamegraph : QueryPhaseTiming 复用
loop --> explain : ExplainPlan 复用
loop --> adaptive : decide 复用
loop --> flamegraph : trace_execute 复用
loop --> advisor : suggest 复用
loop --> diagnosis : diagnose 复用

note bottom of advisor
  6 个 feature gate 默认全关闭
  既有 API 完全向后兼容
  与 v4.3.0 零重叠（分析/建议/转正层）
end note

@enduml
```

### 2.1.3 需求依赖关系与开发顺序

```plantuml
@startuml
title v4.4.0 需求依赖关系与开发顺序

REQ_V44_001 "REQ-V44-001\n优化建议引擎(P1)" : M1（先行）
REQ_V44_002 "REQ-V44-002\n慢查询诊断(P1)" : M2（依赖 M1 建议联动）
REQ_V44_003 "REQ-V44-003\ndb-fusion 转正(P1)" : M3（独立）
REQ_V44_004 "REQ-V44-004\n结构化日志(P2)" : M4（独立）
REQ_V44_005 "REQ-V44-005\n性能基线(P2)" : M5（独立）
REQ_V44_006 "REQ-V44-006\n闭环联动(P3)" : M6（依赖 M1+M2）

REQ_V44_002 ..> REQ_V44_001 : 建议联动(诊断→建议)
REQ_V44_006 ..> REQ_V44_001 : 闭环第四步(建议)
REQ_V44_006 ..> REQ_V44_002 : 闭环第三步(诊断)

note bottom of REQ_V44_001
  P1 先行：M1 优化建议引擎
  M2 慢查询诊断依赖 M1 建议联动
  M3 db-fusion 转正独立
end note

@enduml
```

**开发顺序**（对齐 spec.md 优先级声明 P1→P2→P3）：
1. **P1 先行**：M1（sz-orm-advisor 优化建议引擎）→ M2（sz-orm-diagnosis 慢查询诊断，依赖 M1 建议联动）→ M3（sz-orm-fusion db-fusion 转正，独立）
2. **P2 次之**：M4（sz-orm-observability 结构化日志，独立）→ M5（sz-orm-explain 性能基线，独立）
3. **P3 最后**：M6（sz-orm-advisor 闭环联动，依赖 M1 + M2）→ 最终验证（14 道门禁 + 文档 + 版本号 v4.3.0→v4.4.0）

## 2.2 逐项技术设计

### 2.2.1 REQ-V44-001 查询自动优化建议引擎

#### 2.2.1.1 架构设计

```plantuml
@startuml
title 查询自动优化建议引擎架构

component "应用开发者" as appdev
component "sz-orm-advisor\n(新包,规则引擎)" as advisor
component "OptimizationAdvisor" as engine
component "规则匹配器\n(6 种建议类型)" as rules
component "OptimizationSuggestion\n(统一建议结构)" as suggestion
component "JSON 报告输出" as json

component "sz-orm-explain\n(既有 :76)" as explain
component "sz-orm-adaptive\n(既有 :11/:66/:73)" as adaptive
component "sz-orm-ai\n(既有 :71/:61/:71)" as ai

appdev --> advisor : suggest(plan, stats)
advisor --> engine : OptimizationAdvisor::new()
engine --> explain : 复用 ExplainPlan/PlanRegression
engine --> adaptive : 复用 QueryStats/should_paginate/should_cache
engine --> rules : 规则匹配
rules --> suggestion : 生成 OptimizationSuggestion
suggestion --> ai : 可转换 IndexSuggestion/RewriteSuggestion/TuningSuggestion
advisor --> json : to_json(suggestions)
json --> appdev : JSON 报告(CI 消费)

note bottom of rules
  6 种建议类型(无 AI 依赖):
  (a) FullTable+rows>阈值 → AddIndex
  (b) 冗余索引 → DropIndex
  (c) should_paginate → UsePagination
  (d) should_cache → EnableCache
  (e) 可改写查询 → RewriteQuery
  (f) PoolAcquire 耗时高 → AdjustPoolSize
end note

@enduml
```

核心结构：
- `pub enum SuggestionType { AddIndex, DropIndex, UsePagination, EnableCache, RewriteQuery, AdjustPoolSize }`：六种建议类型。
- `pub struct OptimizationSuggestion { pub suggestion_type: SuggestionType, pub target_query: String, pub description: String, pub action: String, pub confidence: f64, pub estimated_improvement: Option<String> }`：统一建议结构，`confidence` 范围 0.0~1.0，低于 0.5 标注"需人工确认"。
- `pub struct OptimizationAdvisor { config: AdvisorConfig }`：规则引擎，`suggest(&self, plan: Option<&ExplainPlan>, stats: Option<&QueryStats>) -> Vec<OptimizationSuggestion>` 入口。
- `pub struct AdvisorConfig { pub row_threshold: u64, pub confidence_threshold: f64 }`：配置（行数阈值默认 1000，置信度阈值默认 0.5）。
- `pub fn to_json(suggestions: &[OptimizationSuggestion]) -> String`：JSON 报告输出（CI/IDE 消费）。
- `impl OptimizationSuggestion { pub fn to_index_suggestion(&self) -> Option<IndexSuggestion> pub fn to_rewrite_suggestion(&self) -> Option<RewriteSuggestion> pub fn to_tuning_suggestion(&self) -> Option<TuningSuggestion> }`：转换为既有 AI 建议结构。

#### 2.2.1.2 核心流程

```plantuml
@startuml
title 查询自动优化建议引擎 核心流程

start

:OptimizationAdvisor::suggest(plan, stats);

partition "规则匹配（6 种建议类型）" {
  if (plan 存在?) then (是)
    if (scan_type == FullTable 且 rows > 阈值?) then (是)
      :生成 AddIndex 建议\n"添加索引 ON table(条件列)"\n置信度 0.9;
    endif
    if (检测到冗余索引?) then (是)
      :生成 DropIndex 建议\n置信度 0.7;
    endif
    if (可改写查询?) then (是)
      :生成 RewriteQuery 建议\n置信度 0.6;
    endif
  else (否,EXPLAIN 不可用)
    :降级:仅基于统计的建议;
  endif

  if (stats 存在?) then (是)
    if (stats.should_paginate(阈值)?) then (是)
      :生成 UsePagination 建议\n"改用游标分页"\n置信度 0.8;
    endif
    if (stats.should_cache(阈值,下限)?) then (是)
      :生成 EnableCache 建议\n"启用缓存"\n置信度 0.8;
    endif
  else (否,统计不足)
    :降级:仅基于 EXPLAIN 的建议;
  endif
end partition

partition "建议后处理" {
  :按置信度排序;
  if (有冲突建议?) then (是)
    :保留高置信度,低置信度标注"conflict, skipped";
  endif
  if (置信度 < 0.5?) then (是)
    :标注"需人工确认";
  endif
end partition

:返回 Vec<OptimizationSuggestion>;
:to_json 输出 JSON 报告;

stop

@enduml
```

#### 2.2.1.3 复用既有代码（file:line 证据）

| 复用点 | 既有代码位置 | 复用方式 | 验证状态 |
|--------|-------------|---------|---------|
| ExplainPlan 执行计划摘要 | `packages/sz-orm-explain/src/lib.rs:76` | 规则引擎输入，`scan_type`/`table`/`index`/`rows` 判定 | ✅ 已验证 |
| ExplainPlan::missing_index 缺失索引判断 | `packages/sz-orm-explain/src/lib.rs:91` | AddIndex 建议规则复用 | ✅ 已验证 |
| PlanRegression 执行计划回归 | `packages/sz-orm-explain/src/regression.rs:69` | 回归 → 建议映射 | ✅ 已验证 |
| QueryStats 运行时统计 | `packages/sz-orm-adaptive/src/stats.rs:11` | 规则引擎输入，统计决策 | ✅ 已验证 |
| QueryStats::should_paginate 大结果集判断 | `packages/sz-orm-adaptive/src/stats.rs:66` | UsePagination 建议规则复用 | ✅ 已验证 |
| QueryStats::should_cache 热点查询判断 | `packages/sz-orm-adaptive/src/stats.rs:73` | EnableCache 建议规则复用 | ✅ 已验证 |
| IndexSuggestion AI 索引建议结构 | `packages/sz-orm-ai/src/index_advisor.rs:71` | `to_index_suggestion` 转换复用 | ✅ 已验证 |
| RewriteSuggestion AI 改写建议结构 | `packages/sz-orm-ai/src/rewrite_advisor.rs:61` | `to_rewrite_suggestion` 转换复用 | ✅ 已验证 |
| TuningSuggestion AI 调优建议结构 | `packages/sz-orm-ai/src/auto_tuning/mod.rs:71` | `to_tuning_suggestion` 转换复用 | ✅ 已验证 |
| AutoTuningPipeline AI 离线调优 | `packages/sz-orm-ai/src/auto_tuning/pipeline.rs:15` | 互补关系（规则引擎无 AI 依赖，AI 离线深度优化） | ✅ 已验证 |

#### 2.2.1.4 新增依赖

| 包 | 新增依赖 | 用途 | feature 门控 |
|----|---------|------|-------------|
| sz-orm-advisor | sz-orm-explain | ExplainPlan/PlanRegression 复用 | `query-advisor` |
| sz-orm-advisor | sz-orm-adaptive | QueryStats/should_paginate/should_cache 复用 | `query-advisor` |
| sz-orm-advisor | sz-orm-ai | IndexSuggestion/RewriteSuggestion/TuningSuggestion 建议结构复用 | `query-advisor` |
| sz-orm-advisor | serde_json（可选） | JSON 报告序列化 | `query-advisor` |

#### 2.2.1.5 feature gate 定义

```toml
# packages/sz-orm-advisor/Cargo.toml（新包）
[features]
query-advisor = ["dep:serde_json", "dep:sz-orm-explain", "dep:sz-orm-adaptive", "dep:sz-orm-ai"]
# 默认关闭
```

#### 2.2.1.6 错误处理策略

| 错误场景 | 处理策略 | 用户感知 |
|---------|---------|---------|
| EXPLAIN 分析结果缺失（未启用 explain-analyzer 或解析失败） | 降级为仅基于自适应统计的建议（UsePagination/EnableCache），不 panic | 建议报告标注"EXPLAIN analysis unavailable, suggestions based on runtime stats only" |
| 自适应统计不足（total_executions < min_executions） | 降级为仅基于 EXPLAIN 分析的建议（AddIndex/RewriteQuery），不生成统计依赖建议 | 建议报告标注"insufficient stats, suggestions based on EXPLAIN only" |
| 建议冲突（AddIndex 与 DropIndex 针对同一索引） | 按置信度排序，保留高置信度建议，低置信度建议标注"conflict, skipped" | 建议报告标注冲突建议"conflict with higher confidence suggestion, skipped" |
| 低置信度建议（confidence < 0.5） | 标注"需人工确认"，不自动执行 DDL | 建议标注"requires manual confirmation" |

#### 2.2.1.7 测试策略

| 测试类型 | 测试内容 | 验收条件 |
|---------|---------|---------|
| 单元测试 | FullTable + rows > 阈值 → AddIndex 建议 | 建议类型 AddIndex，置信度 0.9，action 含"CREATE INDEX" |
| 单元测试 | should_paginate 为真 → UsePagination 建议 | 建议类型 UsePagination，描述含"游标分页" |
| 单元测试 | should_cache 为真 → EnableCache 建议 | 建议类型 EnableCache，描述含"缓存" |
| 单元测试 | OptimizationSuggestion → IndexSuggestion 转换 | 转换后 index_columns/ddl_text 正确 |
| 单元测试 | 建议冲突处理（AddIndex + DropIndex 同索引） | 保留高置信度，低置信度标注"conflict, skipped" |
| 单元测试 | 低置信度标注"需人工确认" | confidence < 0.5 的建议标注正确 |
| 单元测试 | to_json 输出可被 serde_json::from_str 解析 | JSON 解析成功，含建议列表 |
| 边界测试 | EXPLAIN 缺失降级（仅统计建议） | 降级标注正确，不 panic |
| 边界测试 | 统计不足降级（仅 EXPLAIN 建议） | 降级标注正确，不 panic |
| 门禁 | `cargo test -p sz-orm-advisor --features query-advisor` | 全部通过 |
| 门禁 | 默认 `cargo build -p sz-orm-advisor` 无建议生成 | 行为与 v4.3.0 一致 |

#### 2.2.1.8 设计理由

1. **为什么独立 `sz-orm-advisor` 包而非内联 explain/adaptive**：优化建议是"分析/建议"层（v4.4.0 定位），与 v4.3.0"采集/检测"层（explain/adaptive）分离，独立包可独立测试、独立版本化、被 CLI/CI/闭环联动复用，避免既有包膨胀，符合"与 v4.3.0 零重叠"。
2. **为什么规则引擎而非 AI 依赖**：既有 `AutoTuningPipeline`（`:15`）是 AI 离线闭环（需训练数据 + AI 依赖），本方案是轻量规则引擎（无 AI 依赖），快速响应开发期优化建议，两者互补：规则引擎即时建议，AI 离线深度优化，符合"复用优先"不替换 AI 闭环。
3. **为什么复用既有 AI 建议结构**：既有 `IndexSuggestion`/`RewriteSuggestion`/`TuningSuggestion`（`:71/:61/:71`）已定义完整建议数据模型（DDL 文本/改写 SQL/等价性论证/预期收益），重写违反"复用优先"且重复实现，`OptimizationSuggestion` 可转换为既有结构供 AI 闭环消费。
4. **为什么建议不自动执行 DDL**：自动执行 DDL（如 CREATE INDEX）有生产风险（锁表/磁盘空间/兼容性），建议仅生成文本（`action` 字段），需人工确认后执行，符合 spec.md §4.3.1 安全性约束。
5. **为什么六种建议类型**：覆盖查询优化主要场景（索引/分页/缓存/改写/连接池），既不遗漏关键场景，也不过度拆分（每种类型有明确触发条件和动作），符合"函数只做一件事"。

### 2.2.2 REQ-V44-002 慢查询自动诊断报告

#### 2.2.2.1 架构设计

```plantuml
@startuml
title 慢查询自动诊断报告架构

component "应用开发者" as appdev
component "sz-orm-diagnosis\n(新包,诊断器)" as diagnosis
component "SlowQueryDiagnoser" as diagnoser
component "根因分析器\n(阶段耗时占比)" as rootcause
component "DiagnosisReport\n(诊断报告)" as report
component "双格式输出\n(JSON/人类可读)" as output

component "sz-orm-flamegraph\n(既有 :39/:11)" as flamegraph
component "sz-orm-adaptive\n(既有 :116/:35)" as adaptive
component "sz-orm-advisor\n(REQ-V44-001 建议联动)" as advisor

appdev --> diagnosis : diagnose(query_key, timings, outcome)
diagnosis --> diagnoser : SlowQueryDiagnoser::new()
diagnoser --> flamegraph : 复用 QueryPhaseTiming/Phase
diagnoser --> adaptive : 复用 QueryOutcome.slow/slow_ms
diagnoser --> rootcause : 阶段耗时占比分析
rootcause --> report : 生成 DiagnosisReport
diagnoser --> advisor : suggest(plan, stats) 建议联动
advisor --> report : Vec<OptimizationSuggestion>
diagnosis --> output : to_json / to_human_readable
output --> appdev : JSON / 人类可读报告

note bottom of rootcause
  根因判定(阶段耗时占比):
  PoolAcquire > 30% → PoolExhaustion
  SqlExecute > 50% → SqlInefficiency
  ResultMap > 30% → LargeResultSet
  Build > 20% → BuildOverhead
  多阶段高 → MixedCause
end note

@enduml
```

核心结构：
- `pub enum RootCause { PoolExhaustion, SqlInefficiency, LargeResultSet, BuildOverhead, MixedCause, Unknown }`：根因枚举。
- `pub enum Severity { Info, Warning, Critical }`：严重度枚举。
- `pub struct PhaseBreakdown { pub phase: Phase, pub elapsed_ms: u64, pub percentage: f64, pub anomaly: bool }`：阶段分解。
- `pub struct DiagnosisReport { pub query_key: String, pub total_elapsed_ms: u64, pub root_cause: RootCause, pub phase_breakdown: Vec<PhaseBreakdown>, pub suggestions: Vec<OptimizationSuggestion>, pub severity: Severity }`：诊断报告。
- `pub struct SlowQueryDiagnoser { config: DiagnosisConfig }`：诊断器，`diagnose(&self, query_key: &str, timings: &[QueryPhaseTiming], outcome: &QueryOutcome) -> Option<DiagnosisReport>` 入口（仅 `outcome.slow == true` 返回 Some，否则 None）。
- `pub struct DiagnosisConfig { pub pool_threshold_pct: f64, pub sql_threshold_pct: f64, pub result_threshold_pct: f64, pub build_threshold_pct: f64 }`：根因阈值配置（默认 30%/50%/30%/20%）。
- `impl DiagnosisReport { pub fn to_json(&self) -> String pub fn to_human_readable(&self) -> String }`：双格式输出。

#### 2.2.2.2 核心流程

```plantuml
@startuml
title 慢查询自动诊断报告 核心流程

start

:查询执行 → QueryOutcome { slow, elapsed_ms };
if (outcome.slow == true?) then (是,慢查询)
  :diagnose(query_key, timings, outcome);
  
  partition "根因分析（阶段耗时占比）" {
    :计算各阶段耗时占比;
    :total = sum(phase.duration_ms);
    if (PoolAcquire 占比 > 30%?) then (是)
      :root_cause = PoolExhaustion;
    elseif (SqlExecute 占比 > 50%?) then (是)
      :root_cause = SqlInefficiency;
    elseif (ResultMap 占比 > 30%?) then (是)
      :root_cause = LargeResultSet;
    elseif (Build 占比 > 20%?) then (是)
      :root_cause = BuildOverhead;
    else (多阶段高)
      :root_cause = MixedCause;
    endif
  end partition
  
  partition "阶段分解" {
    :生成 Vec<PhaseBreakdown>;
    :每阶段 anomaly = 占比超阈值;
  end partition
  
  partition "建议联动（REQ-V44-001）" {
    :根据根因生成对应建议;
    :PoolExhaustion → AdjustPoolSize;
    :SqlInefficiency → AddIndex/RewriteQuery;
    :LargeResultSet → UsePagination;
    :BuildOverhead → RewriteQuery;
  end partition
  
  :severity 判定(Critical/Warning/Info);
  :生成 DiagnosisReport;
  :to_json / to_human_readable 输出;
else (否,非慢查询)
  :不触发诊断(返回 None);
endif

stop

@enduml
```

#### 2.2.2.3 复用既有代码（file:line 证据）

| 复用点 | 既有代码位置 | 复用方式 | 验证状态 |
|--------|-------------|---------|---------|
| Phase 查询阶段枚举 | `packages/sz-orm-flamegraph/src/collector.rs:11` | PhaseBreakdown.phase 复用 | ✅ 已验证 |
| QueryPhaseTiming 阶段耗时 | `packages/sz-orm-flamegraph/src/collector.rs:39` | 诊断输入，阶段耗时占比计算 | ✅ 已验证 |
| QueryTracer::trace_execute 分阶段计时 | `packages/sz-orm-flamegraph/src/collector.rs:61` | 采集 timings 供诊断 | ✅ 已验证 |
| QueryOutcome.slow 慢查询标记 | `packages/sz-orm-adaptive/src/executor.rs:116` | 仅 slow == true 触发诊断 | ✅ 已验证 |
| AdaptiveConfig.slow_ms 慢查询阈值 | `packages/sz-orm-adaptive/src/executor.rs:35` | 阈值复用（默认 100ms） | ✅ 已验证 |
| OptimizationSuggestion 优化建议 | `packages/sz-orm-advisor`（REQ-V44-001 新增） | 建议联动，根因 → 对应建议 | ✅ 已验证（本版本新增） |

#### 2.2.2.4 新增依赖

| 包 | 新增依赖 | 用途 | feature 门控 |
|----|---------|------|-------------|
| sz-orm-diagnosis | sz-orm-flamegraph | QueryPhaseTiming/Phase 复用 | `slow-query-diagnosis` |
| sz-orm-diagnosis | sz-orm-adaptive | QueryOutcome.slow/slow_ms 复用 | `slow-query-diagnosis` |
| sz-orm-diagnosis | sz-orm-advisor | OptimizationSuggestion 建议联动 | `slow-query-diagnosis` |
| sz-orm-diagnosis | serde_json（可选） | JSON 报告序列化 | `slow-query-diagnosis` |

#### 2.2.2.5 feature gate 定义

```toml
# packages/sz-orm-diagnosis/Cargo.toml（新包）
[features]
slow-query-diagnosis = ["dep:serde_json", "dep:sz-orm-flamegraph", "dep:sz-orm-adaptive", "dep:sz-orm-advisor"]
# 默认关闭
```

#### 2.2.2.6 错误处理策略

| 错误场景 | 处理策略 | 用户感知 |
|---------|---------|---------|
| 火焰图阶段耗时缺失（未启用 query-flamegraph 或采集失败） | 降级为"仅总耗时 + RootCause::Unknown"，不 panic | 诊断报告标注"phase timing unavailable, root cause unknown" |
| 阶段耗时总和与总耗时不符（误差超 10%） | 标注"timing mismatch"，仍生成报告但根因置信度降低 | 诊断报告标注"timing mismatch detected, root cause confidence reduced" |
| 多根因并存（多阶段占比同时超阈值） | 根因判定为 MixedCause，建议列表合并多根因对应建议 | 诊断报告标注"mixed cause: PoolExhaustion + SqlInefficiency" |
| 非慢查询（slow == false） | 不触发诊断（返回 None），不生成报告 | 无诊断报告（仅慢查询触发） |

#### 2.2.2.7 测试策略

| 测试类型 | 测试内容 | 验收条件 |
|---------|---------|---------|
| 单元测试 | PoolAcquire 耗时 60ms / 总 100ms → PoolExhaustion | root_cause == PoolExhaustion |
| 单元测试 | SqlExecute 耗时 80ms / 总 100ms → SqlInefficiency | root_cause == SqlInefficiency |
| 单元测试 | ResultMap 耗时 40ms / 总 100ms → LargeResultSet | root_cause == LargeResultSet |
| 单元测试 | Build 耗时 25ms / 总 100ms → BuildOverhead | root_cause == BuildOverhead |
| 单元测试 | 多阶段超阈值 → MixedCause | root_cause == MixedCause，建议合并 |
| 单元测试 | 根因 → 建议联动（PoolExhaustion → AdjustPoolSize） | 建议类型正确 |
| 单元测试 | to_json 输出可被 serde_json::from_str 解析 | JSON 解析成功 |
| 单元测试 | to_human_readable 含根因/阶段/建议表格 | 人类可读格式正确 |
| 边界测试 | 非慢查询（slow == false）不触发诊断 | 返回 None |
| 边界测试 | 阶段耗时缺失降级（Unknown 根因） | 降级标注正确，不 panic |
| 边界测试 | 耗时不符标注 timing mismatch | 标注正确，报告仍生成 |
| 门禁 | `cargo test -p sz-orm-diagnosis --features slow-query-diagnosis` | 全部通过 |
| 门禁 | 默认 `cargo build -p sz-orm-diagnosis` 无诊断 | 行为与 v4.3.0 一致 |

#### 2.2.2.8 设计理由

1. **为什么独立 `sz-orm-diagnosis` 包而非内联 flamegraph**：诊断是"分析/建议"层（v4.4.0 定位），火焰图是"采集"层（v4.3.0），分离符合"与 v4.3.0 零重叠"，独立包可独立测试、被闭环联动复用。
2. **为什么仅对慢查询触发诊断**：每次查询都诊断开销大（根因分析 + 报告生成 ≤ 50ms），仅对 `slow == true`（超过 `slow_ms` 阈值）触发，符合 spec.md §4.1.2 性能约束与 §5.2.1.6 禁止项。
3. **为什么复用既有 QueryPhaseTiming/QueryOutcome.slow**：既有 `QueryPhaseTiming`（`:39`）已采集各阶段耗时，既有 `QueryOutcome.slow`（`:116`）已标记慢查询，重写违反"复用优先"且重复实现采集逻辑。
4. **为什么根因用阶段耗时占比而非绝对阈值**：绝对阈值（如 PoolAcquire > 50ms）受查询复杂度影响（复杂查询 PoolAcquire 50ms 正常），占比（PoolAcquire > 30% 总耗时）更准确反映瓶颈所在，符合"避免主观判断，而应该是代码计算"。
5. **为什么与优化建议引擎联动**：诊断报告含建议（根因 → 对应建议）比仅给根因更有价值（用户知道问题 + 知道怎么修），复用 REQ-V44-001 `suggest` 不重复实现建议生成，符合"复用优先"。

### 2.2.3 REQ-V44-003 db-fusion 转正

#### 2.2.3.1 架构设计

本需求分两个阶段转正 v4.3.0 的 `sz-orm-fusion` POC：阶段一 TTL 缓存 + 失效广播；阶段二 CDC 增量同步 + 真实向量搜索下推。

```plantuml
@startuml
title db-fusion 转正架构

component "运维/SRE" as sre
component "sz-orm-fusion\n(转正扩展)" as fusion
component "TtlFusionCache\n(阶段一: TTL 缓存)" as ttl
component "InvalidationBus 集成\n(阶段一: 失效广播)" as bus
component "CdcSyncCoordinator\n(阶段二: CDC 同步)" as cdc
component "VectorPushdownExecutor\n(阶段二: 向量下推)" as vector

component "sz-orm-core\n(既有 :82/:41/:179)" as core
component "sz-orm-queue/cdc\n(既有 :12/:178)" as queue
component "sz-orm-vector\n(既有 :30/:6/:60)" as vec

database "主库" as primary
rectangle "Redis\n(缓存)" as redis
rectangle "向量库" as vecdb
rectangle "CDC 下游" as downstream

sre --> fusion : FusionExecutor::new(config)\n.with_cache(ttl_cache)\n.with_invalidation_bus(bus)

fusion --> ttl : 阶段一 TTL 缓存
ttl --> redis : TTL 过期检查 + 缓存读写

fusion --> bus : 阶段一 失效广播
bus --> core : 复用 InvalidationBus/RedisPubSubInvalidationBus
core --> redis : 跨实例缓存失效

fusion --> cdc : 阶段二 CDC 同步
cdc --> queue : 复用 DialectCapturer/DownstreamSink/distribute_to_all
primary --> cdc : WAL/binlog 变更捕获
cdc --> downstream : 分发到缓存失效 + 搜索索引更新

fusion --> vector : 阶段二 向量下推
vector --> vec : 复用 HybridSearcher/FilterPushdown/DegradationStatus
vec --> vecdb : 三源并行向量检索

fusion --> primary : 主库查询

note bottom of fusion
  转正 API #[stable]
  POC API #[deprecated] + 迁移指引
  db-fusion-v2 feature 隔离
end note

@enduml
```

核心结构：
- `pub struct TtlFusionCache { inner: Mutex<HashMap<String, (String, Instant)>>, default_ttl: Duration }`：TTL 融合缓存，`get_with_ttl`/`set_with_ttl` 方法，过期缓存不返回。
- `impl FusionCache for TtlFusionCache`：实现既有 `FusionCache` trait（`:8`）。
- `impl FusionExecutor { pub fn with_invalidation_bus(mut self, bus: Arc<dyn InvalidationBus>) -> Self }`：注入失效总线，主库写入后 `bus.publish` 失效消息。
- `pub struct CdcSyncCoordinator { capturer: Arc<dyn DialectCapturer>, sinks: Vec<Box<dyn DownstreamSink>>, checkpoint: Option<CdcCheckpoint> }`：CDC 同步协调器，`start_sync` 启动主库变更 → 缓存/搜索索引增量同步。
- `pub struct VectorPushdownExecutor { searcher: Arc<HybridSearcher> }`：向量下推执行器，`execute` 调用 `HybridSearcher::search` 执行真实向量检索 + `FilterPushdown` 结构化过滤下推。
- 转正 API `#[stable]` 标注；POC API（`FusionConfig`/`FusionExecutor`/`MemoryFusionCache`）`#[deprecated(note = "use TtlFusionCache + CdcSyncCoordinator")]` + 迁移指引。

#### 2.2.3.2 核心流程

```plantuml
@startuml
title db-fusion 转正 核心流程

start

partition "阶段一：TTL 缓存 + 失效广播" {
  :FusionExecutor::new(config)\n.with_cache(ttl_cache)\n.with_invalidation_bus(bus);
  :FusionExecutor::execute(query, primary);
  :FusionPlanner::plan(query, config);
  
  if (缓存命中且未过期?) then (是)
    :返回缓存数据(from_cache: true);
  else (未命中或过期)
    :主库查询 primary();
    if (主库成功?) then (是)
      :回填 TTL 缓存 set_with_ttl;
      :bus.publish(失效消息);
      :返回主库数据;
    else (主库失败)
      if (缓存可读?) then (是)
        :返回缓存旧数据 + degraded: true;
      else (否)
        :返回错误"primary unavailable";
      endif
    endif
  endif
end partition

partition "阶段二：CDC 增量同步（异步）" {
  :CdcSyncCoordinator::start_sync();
  :DialectCapturer::start_capture(checkpoint);
  :流式接收 ChangeEvent;
  :变更事件脱敏(masking.rs);
  :distribute_to_all(sinks, event);
  :分发到缓存失效 + 搜索索引更新下游;
  :CdcCheckpoint 更新(断点续传);
end partition

partition "阶段二：真实向量搜索下推" {
  if (融合查询含 search 条件?) then (是)
    :VectorPushdownExecutor::execute;
    :FilterPushdown::pushdown_to_vector;
    :HybridSearcher::search(三源并行);
    if (向量搜索成功?) then (是)
      :返回融合排序结果;
    else (失败)
      :降级为主库查询(DegradationStatus);
      :标记 vector_degraded;
    endif
  endif
end partition

stop

@enduml
```

#### 2.2.3.3 复用既有代码（file:line 证据）

| 复用点 | 既有代码位置 | 复用方式 | 验证状态 |
|--------|-------------|---------|---------|
| FusionConfig 融合配置 | `packages/sz-orm-fusion/src/plan.rs:21` | 转正配置复用 | ✅ 已验证 |
| FusionPlanner 融合规划器 | `packages/sz-orm-fusion/src/plan.rs:104` | 查询拆分复用 | ✅ 已验证 |
| FusionCache trait 融合缓存抽象 | `packages/sz-orm-fusion/src/executor.rs:8` | TtlFusionCache 实现此 trait | ✅ 已验证 |
| MemoryFusionCache 内存缓存 | `packages/sz-orm-fusion/src/executor.rs:16` | POC API #[deprecated] 标注 | ✅ 已验证 |
| FusionOutcome.degraded 降级标记 | `packages/sz-orm-fusion/src/executor.rs:55` | 主库失败降级复用 | ✅ 已验证 |
| FusionExecutor 融合执行器 | `packages/sz-orm-fusion/src/executor.rs:63` | 扩展 with_invalidation_bus | ✅ 已验证 |
| FusionExecutor::execute 执行入口 | `packages/sz-orm-fusion/src/executor.rs:93` | TTL + 失效广播集成到此流程 | ✅ 已验证 |
| 搜索下推仅记录数据源（POC） | `packages/sz-orm-fusion/src/executor.rs:118` | 转正为真实向量检索 | ✅ 已验证 |
| InvalidationBus 缓存失效总线 | `packages/sz-orm-core/src/l2_cache.rs:82` | with_invalidation_bus 注入 | ✅ 已验证 |
| LocalInvalidationBus 本地失效总线 | `packages/sz-orm-core/src/l2_cache.rs:93` | 单实例失效广播 | ✅ 已验证 |
| RedisPubSubInvalidationBus Redis 失效总线 | `packages/sz-orm-core/src/dist_cache.rs:41` | 跨实例 Redis Pub/Sub 失效广播 | ✅ 已验证 |
| GossipInvalidationBus Gossip 失效总线 | `packages/sz-orm-core/src/dist_cache.rs:179` | 跨实例 Gossip 失效广播 | ✅ 已验证 |
| DialectCapturer 方言 CDC 捕获器 | `packages/sz-orm-queue/src/cdc/capturer.rs:12` | CdcSyncCoordinator 复用 | ✅ 已验证 |
| DownstreamSink 下游分发 | `packages/sz-orm-queue/src/cdc/downstream.rs:12` | CDC 下游分发复用 | ✅ 已验证 |
| distribute_to_all 并行分发 | `packages/sz-orm-queue/src/cdc/downstream.rs:178` | CDC 并行分发复用 | ✅ 已验证 |
| CdcCheckpoint 断点续传 | `packages/sz-orm-queue/src/cdc/checkpoint.rs` | CDC 断点续传复用 | ✅ 已验证 |
| CDC 变更事件脱敏 | `packages/sz-orm-queue/src/cdc/masking.rs` | CDC 同步脱敏复用 | ✅ 已验证 |
| HybridSearcher 混合搜索器 | `packages/sz-orm-vector/src/hybrid_search/searcher.rs:30` | VectorPushdownExecutor 调用 | ✅ 已验证 |
| DegradationStatus 搜索降级状态 | `packages/sz-orm-vector/src/hybrid_search/searcher.rs:60` | 向量失败降级复用 | ✅ 已验证 |
| FilterPushdown 过滤下推 | `packages/sz-orm-vector/src/hybrid_search/pushdown.rs:6` | 结构化过滤下推复用 | ✅ 已验证 |

#### 2.2.3.4 新增依赖

| 包 | 新增依赖 | 用途 | feature 门控 |
|----|---------|------|-------------|
| sz-orm-fusion | sz-orm-core | InvalidationBus/RedisPubSubInvalidationBus 复用 | `db-fusion-v2` |
| sz-orm-fusion | sz-orm-queue | DialectCapturer/DownstreamSink/distribute_to_all 复用 | `db-fusion-v2` |
| sz-orm-fusion | sz-orm-vector | HybridSearcher/FilterPushdown 复用 | `db-fusion-v2` |
| sz-orm-fusion | tokio（既有） | CDC 异步同步 | `db-fusion-v2` |

#### 2.2.3.5 feature gate 定义

```toml
# packages/sz-orm-fusion/Cargo.toml（扩展）
[features]
db-fusion = ["dep:sz-orm-vector", "dep:sz-orm-queue"]  # 既有 POC feature
db-fusion-v2 = ["db-fusion", "dep:sz-orm-core"]  # 新增转正 feature，依赖 POC
# 默认关闭，转正能力在 db-fusion-v2 内
```

#### 2.2.3.6 错误处理策略

| 错误场景 | 处理策略 | 用户感知 |
|---------|---------|---------|
| TTL 缓存过期但主库不可用 | 返回明确错误"cache expired and primary unavailable"，不返回过期脏数据（除非降级模式） | 错误"cache expired and primary unavailable, retry later" |
| 失效总线不可用（Redis Pub/Sub 断连） | 降级为 TTL 过期兜底，告警"invalidation bus unavailable, relying on TTL" | 告警"cache invalidation bus unavailable, relying on TTL expiry" |
| CDC 捕获失败（WAL/binlog 未配置） | 降级为 TTL 过期兜底，告警"CDC capture failed, relying on TTL" | 告警"CDC incremental sync unavailable, relying on TTL expiry" |
| 向量搜索下推失败（向量库不可用） | 降级为主库查询（复用 DegradationStatus），标记 vector_degraded | 结果标记"vector search degraded, fallback to primary" |
| CDC 变更事件脱敏失败 | 跳过该事件，告警"CDC event masking failed, skipped" | 告警"CDC event for table X masking failed, skipped, manual review required" |
| 主库失败 + 缓存可读 | 返回缓存旧数据 + degraded: true（复用 FusionOutcome.degraded） | 结果标注"degraded: primary unavailable, cache fallback" |

#### 2.2.3.7 测试策略

| 测试类型 | 测试内容 | 验收条件 |
|---------|---------|---------|
| 单元测试 | TtlFusionCache TTL 过期（设置 60s，60s 内命中，60s 后 None） | 过期返回 None |
| 单元测试 | TtlFusionCache 实现 FusionCache trait | get/set 行为正确 |
| 单元测试 | with_invalidation_bus 主库写入后 publish 失效消息 | 失效消息发布 |
| 单元测试 | 跨实例缓存失效（实例 A 写入 → 实例 B 缓存失效） | 实例 B 下次查询回源 |
| 单元测试 | CdcSyncCoordinator 主库 INSERT → CDC 捕获 → 分发 | 变更事件分发到下游 |
| 单元测试 | CdcCheckpoint 断点续传 | 中断后从断点恢复 |
| 单元测试 | VectorPushdownExecutor 真实向量检索 | 调用 HybridSearcher::search，返回融合排序结果 |
| 单元测试 | 向量搜索失败降级主库 | 降级标记 vector_degraded，回退主库 |
| 单元测试 | CDC 事件脱敏 | 敏感字段变更事件脱敏后分发 |
| 集成测试 | 阶段一：主库 + Redis TTL 缓存 + 失效广播 | 缓存命中/失效/降级正确 |
| 集成测试 | 阶段二：主库 CDC → 缓存失效 + 搜索索引更新 | CDC 同步生效 |
| 集成测试 | 阶段二：融合查询含 search 条件 → 真实向量检索 | 向量下推返回融合结果 |
| 边界测试 | TTL 过期 + 主库不可用 → 明确错误 | 不返回脏数据 |
| 边界测试 | 失效总线不可用 → TTL 兜底 | 降级告警正确 |
| 门禁 | `cargo test -p sz-orm-fusion --features db-fusion-v2` | 全部通过 |
| 门禁 | 默认 `cargo build -p sz-orm-fusion` POC API 保留 | POC API 可用（#[deprecated] 标注） |

#### 2.2.3.8 设计理由

1. **为什么两阶段转正而非一次性**：阶段一（TTL + 失效广播）解决缓存一致性核心问题，价值明确风险低；阶段二（CDC + 向量下推）依赖外部基础设施（CDC 配置/向量库），分阶段降低风险，符合 db-fusion POC 评估报告推荐（`docs/评估/2026-08-12_db-fusion实验评估.md`）。
2. **为什么复用既有 InvalidationBus 而非新建失效广播**：既有 `InvalidationBus`（`:82`）/ `RedisPubSubInvalidationBus`（`:41`）/ `GossipInvalidationBus`（`:179`）已实现跨实例缓存失效（Redis Pub/Sub + Gossip），重写违反"复用优先"且重复实现分布式失效逻辑。
3. **为什么复用既有 DialectCapturer/DownstreamSink 而非新建 CDC**：既有 `DialectCapturer`（`:12`）已实现 5 方言 CDC 捕获，既有 `DownstreamSink`（`:12`）+ `distribute_to_all`（`:178`）已实现 7 种下游分发，重写违反"复用优先"且 CDC 实现成本高。
4. **为什么复用既有 HybridSearcher 而非新建向量检索**：既有 `HybridSearcher`（`:30`）已实现三源并行查询 + 融合排序，既有 `FilterPushdown`（`:6`）已实现结构化过滤下推，POC 搜索下推仅记录数据源（`:118`），转正为调用既有 `HybridSearcher::search` 符合"复用优先"。
5. **为什么 POC API #[deprecated] 而非删除**：删除 POC API 是 Breaking Change（破坏既有使用者），`#[deprecated]` + 迁移指引保持向后兼容，符合"无 Breaking Change"铁律。
6. **为什么 CDC 失败降级 TTL 兜底而非阻断**：CDC 同步是异步增强，失败不应阻断主库查询（主库查询是核心路径），降级为 TTL 过期兜底保证可用性，符合 spec.md §4.2.4 可靠性约束。

### 2.2.4 REQ-V44-004 结构化查询日志

#### 2.2.4.1 架构设计

```plantuml
@startuml
title 结构化查询日志架构

component "运维/SRE" as sre
component "sz-orm-observability\n(日志扩展)" as logging
component "QueryLogger\n(结构化日志器)" as logger
component "QueryLogEntry\n(日志结构)" as entry
component "采样与级别控制" as sampler
component "JSON 输出" as json

component "sz-orm-flamegraph\n(既有 :39)" as flamegraph
component "sz-orm-masking\n(既有 :21)" as masking
component "MetricsRegistry\n(既有 :253)" as metrics

sre --> logging : QueryLogger::new()\n.with_sample_rate(0.01)\n.with_level(Info)
sre --> flamegraph : QueryTracer::trace_execute
flamegraph --> sre : Vec<QueryPhaseTiming>
sre --> logging : log(query_key, sql, params, timings, slow)
logger --> masking : 脱敏 params
masking --> logger : 脱敏后 params
logger --> sampler : 采样判定 + 级别过滤
sampler --> entry : 生成 QueryLogEntry
logger --> metrics : MetricsRegistry 关联指标
logger --> json : JSON 序列化
json --> sre : QueryLogEntry(JSON)

note bottom of sampler
  采样率(默认 1%,慢查询 100%)
  级别控制:
  DEBUG: 含 SQL/参数
  INFO: 仅含统计
  WARN: 仅含慢查询
end note

@enduml
```

核心结构：
- `pub enum LogLevel { Debug, Info, Warn }`：日志级别枚举。
- `pub struct QueryLogEntry { pub query_key: String, pub sql: String, pub params: Vec<String>, pub total_elapsed_ms: u64, pub phase_breakdown: Vec<QueryPhaseTiming>, pub slow: bool, pub from_cache: bool, pub timestamp: String }`：日志结构，`params` 已脱敏，`phase_breakdown` 复用既有 `QueryPhaseTiming`。
- `pub struct QueryLogger { sample_rate: f64, level: LogLevel, registry: Option<Arc<MetricsRegistry>> }`：结构化日志器，`log(&self, entry: QueryLogEntry)` 入口。
- `impl QueryLogger { pub fn new() -> Self pub fn with_sample_rate(mut self, rate: f64) -> Self pub fn with_level(mut self, level: LogLevel) -> Self pub fn with_registry(mut self, registry: Arc<MetricsRegistry>) -> Self }`：配置方法。
- `pub fn mask_params(params: &[String], rules: &[MaskingRule]) -> Vec<String>`：参数脱敏（复用既有 `MaskingRule`）。

#### 2.2.4.2 核心流程

```plantuml
@startuml
title 结构化查询日志 核心流程

start

:查询执行 → (query_key, sql, params, timings, outcome);
:QueryLogger::log(entry);

partition "参数脱敏" {
  :mask_params(params, rules);
  :params 字段自动脱敏(复用 MaskingRule);
  if (脱敏失败?) then (是)
    :params = "[masking failed]";
  endif
end partition

partition "采样判定" {
  if (slow == true?) then (是,慢查询)
    :100% 采样(必输出);
  else (非慢查询)
    :按 sample_rate 概率采样;
    if (采样未通过?) then (是)
      :不输出日志;
      stop
    endif
  endif
end partition

partition "级别过滤" {
  if (level == Warn 且 slow == false?) then (是)
    :不输出日志;
    stop
  elseif (level == Info 且 sql/params 不需要?) then (是)
    :仅含统计(不含 SQL/params);
  elseif (level == Debug?) then (是)
    :含 SQL/params(已脱敏);
  endif
end partition

:生成 QueryLogEntry;
:MetricsRegistry 关联指标(查询计数/耗时);
:JSON 序列化输出;
:日志写入(失败静默丢弃,不阻断查询);

stop

@enduml
```

#### 2.2.4.3 复用既有代码（file:line 证据）

| 复用点 | 既有代码位置 | 复用方式 | 验证状态 |
|--------|-------------|---------|---------|
| MetricsRegistry 指标注册中心 | `packages/sz-orm-observability/src/lib.rs:253` | QueryLogger 关联指标（查询计数/耗时直方图） | ✅ 已验证 |
| start_metrics_server Prometheus server | `packages/sz-orm-observability/src/lib.rs:421` | 既有 Prometheus 导出保留不动 | ✅ 已验证 |
| QueryPhaseTiming 阶段耗时 | `packages/sz-orm-flamegraph/src/collector.rs:39` | phase_breakdown 复用 | ✅ 已验证 |
| MaskingRule 脱敏规则 | `packages/sz-orm-masking/src/lib.rs:21` | 参数脱敏复用 | ✅ 已验证 |

#### 2.2.4.4 新增依赖

| 包 | 新增依赖 | 用途 | feature 门控 |
|----|---------|------|-------------|
| sz-orm-observability | sz-orm-flamegraph | QueryPhaseTiming 复用 | `query-logging` |
| sz-orm-observability | sz-orm-masking | MaskingRule 参数脱敏复用 | `query-logging` |
| sz-orm-observability | serde_json（既有/可选） | JSON 日志序列化 | `query-logging` |

#### 2.2.4.5 feature gate 定义

```toml
# packages/sz-orm-observability/Cargo.toml（扩展）
[features]
query-logging = ["dep:sz-orm-flamegraph", "dep:sz-orm-masking"]
# 默认关闭
```

#### 2.2.4.6 错误处理策略

| 错误场景 | 处理策略 | 用户感知 |
|---------|---------|---------|
| 日志写入失败（磁盘满/网络断） | 静默丢弃日志（不阻断查询），告警计数 +1 | 查询正常执行，日志丢失（告警计数可观测） |
| 脱敏失败（参数脱敏规则匹配失败） | 参数替换为 `[masking failed]`，不暴露原始值 | 日志 params 为 `[masking failed]` |
| 未启用 query-flamegraph | phase_breakdown 为空，仍输出日志（仅总耗时） | 日志 phase_breakdown 为空 |
| 采样未通过 | 不输出日志 | 无日志（采样率控制） |

#### 2.2.4.7 测试策略

| 测试类型 | 测试内容 | 验收条件 |
|---------|---------|---------|
| 单元测试 | QueryLogEntry JSON 格式含 query_key/sql/params/耗时/阶段/慢标记 | JSON 字段完整 |
| 单元测试 | 参数脱敏（手机号 13800138000 → 138****8000） | params 脱敏正确 |
| 单元测试 | 采样率 1% + 100 次查询 → 约 1 条日志 | 采样率生效 |
| 单元测试 | 慢查询 100% 采样 | 慢查询必输出 |
| 单元测试 | 级别控制（WARN 仅含慢查询，INFO 仅含统计，DEBUG 含 SQL/参数） | 级别过滤正确 |
| 单元测试 | 未启用 query-flamegraph → phase_breakdown 为空 | phase_breakdown 为空 |
| 边界测试 | 日志写入失败静默丢弃（不阻断查询） | 查询正常，日志丢弃 |
| 边界测试 | 脱敏失败 → params = "[masking failed]" | 不暴露原始值 |
| 门禁 | `cargo test -p sz-orm-observability --features query-logging` | 全部通过 |
| 门禁 | 默认 `cargo build -p sz-orm-observability` 无结构化日志 | 行为与 v4.3.0 一致 |

#### 2.2.4.8 设计理由

1. **为什么扩展 sz-orm-observability 而非新包**：结构化查询日志是可观测性增强，与既有 `MetricsRegistry`（`:253`）同属可观测性范畴，扩展包复用既有指标注册中心关联，新包会割裂可观测性能力，符合"相关代码放在一起"。
2. **为什么复用既有 QueryPhaseTiming/MaskingRule**：既有 `QueryPhaseTiming`（`:39`）已采集阶段耗时，既有 `MaskingRule`（`:21`）已实现脱敏算法，重写违反"复用优先"且重复实现采集/脱敏逻辑。
3. **为什么采样率默认 1% 而非 100%**：100% 采样日志量大（每查询一条 JSON），影响性能（≤ 100μs/次但仍累积），1% 采样 + 慢查询 100% 平衡可观测性与性能，符合 spec.md §4.1.6 性能约束。
4. **为什么日志写入失败静默丢弃**：日志是可观测性辅助，写入失败不应阻断核心查询路径，静默丢弃 + 告警计数保证查询可用性，符合 spec.md §4.2.6 可靠性约束。
5. **为什么参数脱敏**：结构化日志含查询参数，生产环境参数可能含敏感信息（手机号/身份证），复用既有 `MaskingRule` 脱敏避免泄露，符合 spec.md §4.3.3 安全性约束。

### 2.2.5 REQ-V44-005 性能回归基准线 + CI 自动比对

#### 2.2.5.1 架构设计

```plantuml
@startuml
title 性能回归基准线 + CI 自动比对架构

component "DBA" as dba
component "sz-orm-explain\n(基线扩展)" as explain
component "PerfBaseline\n(耗时基线)" as baseline
component "PerfRegression\n(性能回归)" as regression
component "check_perf_regressions\n(CI 入口)" as check

component "PlanSnapshot\n(既有 :23)" as snapshot
component "check_regressions\n(既有 :161)" as plancheck
component "sz-orm-flamegraph\n(既有 :39)" as flamegraph
component "CI/CD" as ci

== 基线采集 ==
dba --> flamegraph : QueryTracer::trace_execute
flamegraph --> dba : Vec<QueryPhaseTiming>
dba --> explain : PerfBaseline::new(query_key, plan, timings)
explain --> baseline : phase_baselines + plan + captured_at
baseline --> snapshot : 复用 PlanSnapshot 结构
dba --> dba : 保存基线 plans/perf-baseline.json

== CI 比对 ==
ci --> flamegraph : 当前查询采集耗时
flamegraph --> ci : 当前 QueryPhaseTiming
ci --> explain : check_perf_regressions(baseline.json, current.json, 5)
check --> regression : 比对各阶段耗时 + 执行计划
regression --> plancheck : 复用 check_regressions(计划回归)
check --> ci : Vec<PerfRegression>

note bottom of regression
  PerfRegression 类型:
  PhaseSlowdown: 阶段耗时增长
  TotalSlowdown: 总耗时增长
  PlanRegression: 执行计划回归(复用既有)
end note

@enduml
```

核心结构：
- `pub struct PerfBaseline { pub query_key: String, pub phase_baselines: HashMap<Phase, u64>, pub plan: ExplainPlan, pub captured_at: String }`：耗时基线（各阶段耗时基线 + 执行计划基线），JSON 序列化。
- `pub enum PerfRegressionType { PhaseSlowdown, TotalSlowdown, PlanRegression }`：性能回归类型。
- `pub enum PerfRegression { PhaseSlowdown { query_key, phase, before, after }, TotalSlowdown { query_key, before, after }, PlanRegression(PlanRegression) }`：性能回归，`PlanRegression` 变体复用既有 `PlanRegression`（`:69`）。
- `pub fn check_perf_regressions(baseline_json: &str, current_json: &str, threshold_factor: u64) -> Result<Vec<PerfRegression>, serde_json::Error>`：CI 入口，比对各阶段耗时 + 执行计划，一次比对同时检出耗时回归与计划回归。
- `impl PerfBaseline { pub fn new(query_key: impl Into<String>, plan: ExplainPlan, timings: &[QueryPhaseTiming]) -> Self pub fn to_json(&self) -> String pub fn from_json(json: &str) -> Result<Self, serde_json::Error> }`：基线构造与序列化。

#### 2.2.5.2 核心流程

```plantuml
@startuml
title 性能回归基准线 + CI 自动比对 核心流程

start

partition "基线采集" {
  :QueryTracer::trace_execute(查询);
  :得到 Vec<QueryPhaseTiming>;
  :PerfBaseline::new(query_key, plan, timings);
  :phase_baselines = HashMap<Phase, u64>;
  :to_json → 保存 plans/perf-baseline.json;
end partition

partition "CI 比对" {
  :check_perf_regressions(baseline.json, current.json, threshold_factor);
  :反序列化 baseline + current;
  
  partition "耗时比对" {
    if (启用 query-flamegraph?) then (是)
      :遍历各阶段 Phase;
      if (current.phase > baseline.phase * threshold_factor?) then (是)
        :检出 PhaseSlowdown;
      endif
      if (current.total > baseline.total * threshold_factor?) then (是)
        :检出 TotalSlowdown;
      endif
    else (否,未启用火焰图)
      :仅比对总耗时;
    endif
  end partition
  
  partition "执行计划比对（复用既有）" {
    :check_regressions(baseline, current, factor);
    :检出 PlanRegression(ScanTypeUpgrade/IndexLost/RowsGrowth);
    :包装为 PerfRegression::PlanRegression;
  end partition
  
  :合并 Vec<PerfRegression>;
  :CI 报告(非阻断);
end partition

stop

@enduml
```

#### 2.2.5.3 复用既有代码（file:line 证据）

| 复用点 | 既有代码位置 | 复用方式 | 验证状态 |
|--------|-------------|---------|---------|
| PlanSnapshot 基线快照 | `packages/sz-orm-explain/src/regression.rs:23` | PerfBaseline 结构参考复用 | ✅ 已验证 |
| PlanBaseline 基线集合 | `packages/sz-orm-explain/src/regression.rs:34` | 基线集合模式复用 | ✅ 已验证 |
| PlanRegression 执行计划回归 | `packages/sz-orm-explain/src/regression.rs:69` | PerfRegression::PlanRegression 变体复用 | ✅ 已验证 |
| check_regressions CI 回归入口 | `packages/sz-orm-explain/src/regression.rs:161` | 执行计划比对复用 | ✅ 已验证 |
| Phase 查询阶段枚举 | `packages/sz-orm-flamegraph/src/collector.rs:11` | phase_baselines key 复用 | ✅ 已验证 |
| QueryPhaseTiming 阶段耗时 | `packages/sz-orm-flamegraph/src/collector.rs:39` | 基线采集复用 | ✅ 已验证 |

#### 2.2.5.4 新增依赖

| 包 | 新增依赖 | 用途 | feature 门控 |
|----|---------|------|-------------|
| sz-orm-explain | sz-orm-flamegraph | QueryPhaseTiming/Phase 复用 | `perf-baseline` |
| sz-orm-explain | serde_json（既有） | PerfBaseline JSON 序列化 | `perf-baseline` |

#### 2.2.5.5 feature gate 定义

```toml
# packages/sz-orm-explain/Cargo.toml（扩展）
[features]
explain-analyzer = ["dep:serde_json"]  # 既有 feature
perf-baseline = ["explain-analyzer", "dep:sz-orm-flamegraph"]  # 新增 feature，依赖 explain-analyzer
# 默认关闭
```

#### 2.2.5.6 错误处理策略

| 错误场景 | 处理策略 | 用户感知 |
|---------|---------|---------|
| 基线文件缺失（CI 中基线 JSON 不存在） | 跳过比对，告警"baseline file not found, skip perf regression check" | CI 告警"perf baseline not found, skip check" |
| 当前耗时缺失（未启用 query-flamegraph） | 降级为仅比对执行计划回归（PlanRegression），不比对耗时 | CI 报告"phase timing unavailable, only plan regression checked" |
| 阈值因子配置不当（过小如 1.0 导致正常波动被报告） | 按配置阈值比对，建议值 2（保守）/3（常规）/5（宽松） | CI 报告标注"threshold factor: X, adjust if too noisy" |
| 基线/当前 JSON 反序列化失败 | 返回 `serde_json::Error`，CI 报告错误 | CI 报告"perf baseline json parse error" |

#### 2.2.5.7 测试策略

| 测试类型 | 测试内容 | 验收条件 |
|---------|---------|---------|
| 单元测试 | PerfBaseline JSON 序列化/反序列化 | to_json/from_json 往返一致 |
| 单元测试 | 基线 Build 5ms，当前 Build 50ms，阈值 5 → PhaseSlowdown | 检出 PhaseSlowdown { phase: Build, before: 5, after: 50 } |
| 单元测试 | 基线总 10ms，当前总 100ms，阈值 5 → TotalSlowdown | 检出 TotalSlowdown |
| 单元测试 | 基线 IndexRange，当前 FullTable → PerfRegression::PlanRegression(ScanTypeUpgrade) | 检出计划回归 |
| 单元测试 | 一次比对同时检出耗时回归与计划回归 | Vec<PerfRegression> 含两种 |
| 边界测试 | 基线文件缺失 → 跳过比对 + 告警 | 告警正确，不 panic |
| 边界测试 | 未启用 query-flamegraph → 仅比对计划回归 | 降级标注正确 |
| 边界测试 | 阈值因子 1.0 → 正常波动被报告（配置不当） | 按配置比对，标注建议值 |
| 集成测试 | CI 流程：基线采集 → 保存 → CI 比对 → 报告 | CI 报告性能退化 |
| 门禁 | `cargo test -p sz-orm-explain --features perf-baseline` | 全部通过 |
| 门禁 | 默认 `cargo build -p sz-orm-explain` 无性能基线 | 行为与 v4.3.0 一致 |

#### 2.2.5.8 设计理由

1. **为什么扩展 sz-orm-explain 而非新包**：性能基线是执行计划基线（`PlanSnapshot` `:23`）的耗时维度扩展，同属回归检测范畴，扩展包复用既有 `PlanSnapshot`/`check_regressions`，新包会割裂回归检测能力，符合"相关代码放在一起"。
2. **为什么 PerfRegression 包含 PlanRegression 变体**：一次 CI 比对同时检出耗时回归（PhaseSlowdown/TotalSlowdown）与执行计划回归（ScanTypeUpgrade/IndexLost/RowsGrowth），避免分两次比对，复用既有 `check_regressions`（`:161`）不重复实现计划回归，符合"复用优先"。
3. **为什么复用既有 QueryPhaseTiming**：既有 `QueryPhaseTiming`（`:39`）已采集各阶段耗时，`PerfBaseline.phase_baselines` 直接来自 `QueryPhaseTiming`，重写违反"复用优先"。
4. **为什么 CI 比对非阻断**：性能回归检测是建议性门禁（性能可能因环境波动误报），阻断会破坏 CI 流程，非阻断 + 报告供人工判断，符合 spec.md §5.5.1.6 禁止项。
5. **为什么阈值因子可配**：不同查询性能波动幅度不同（OLTP 波动小，OLAP 波动大），固定阈值会误报/漏报，可配阈值因子（2 保守/3 常规/5 宽松）适应不同场景，符合"避免主观判断"。

### 2.2.6 REQ-V44-006 查询智能闭环联动

#### 2.2.6.1 架构设计

```plantuml
@startuml
title 查询智能闭环联动架构

component "应用开发者" as appdev
component "sz-orm-advisor\n(闭环扩展)" as advisor
component "IntelligenceLoop\n(闭环协调器)" as loop
component "LoopReport\n(闭环报告)" as report
component "JSON 输出" as json

component "sz-orm-explain\n(既有 :76)" as explain
component "sz-orm-adaptive\n(既有 :157)" as adaptive
component "sz-orm-flamegraph\n(既有 :61)" as flamegraph
component "sz-orm-advisor\n(REQ-V44-001 suggest)" as advisorsuggest
component "sz-orm-diagnosis\n(REQ-V44-002 diagnose)" as diagnosis

appdev --> advisor : run_loop(query_key, query) [可选]
advisor --> loop : IntelligenceLoop::new()
loop --> explain : 1. EXPLAIN 分析(复用 ExplainPlan)
loop --> adaptive : 2. 自适应决策(复用 decide)
loop --> flamegraph : 3. 火焰图诊断(复用 trace_execute)
loop --> diagnosis : 诊断报告(REQ-V44-002)
loop --> advisorsuggest : 4. 优化建议(复用 suggest)
loop --> report : 生成 LoopReport
advisor --> json : to_json
json --> appdev : LoopReport(JSON)

note bottom of loop
  闭环四步(任一失败降级跳过):
  1. EXPLAIN 分析 → ExplainPlan
  2. 自适应决策 → ExecutionPath
  3. 火焰图诊断 → DiagnosisReport
  4. 优化建议 → Vec<OptimizationSuggestion>
end note

@enduml
```

核心结构：
- `pub struct IntelligenceLoop { advisor: Arc<OptimizationAdvisor>, diagnoser: Option<Arc<SlowQueryDiagnoser>>, executor: Option<Arc<AdaptiveExecutor>> }`：闭环协调器。
- `pub struct LoopReport { pub query_key: String, pub explain_result: Option<ExplainPlan>, pub adaptive_decision: Option<ExecutionPath>, pub diagnosis_result: Option<DiagnosisReport>, pub suggestions: Vec<OptimizationSuggestion>, pub loop_elapsed_ms: u64 }`：闭环报告，各环节结果为 `Option`（跳过时 None）。
- `impl IntelligenceLoop { pub fn new(advisor: Arc<OptimizationAdvisor>) -> Self pub fn with_diagnoser(mut self, d: Arc<SlowQueryDiagnoser>) -> Self pub fn with_executor(mut self, e: Arc<AdaptiveExecutor>) -> Self pub async fn run_loop(&self, query_key: &str, query: &Query) -> LoopReport }`：闭环入口，串联四步，任一失败降级跳过。
- `impl LoopReport { pub fn to_json(&self) -> String }`：JSON 输出（CI 消费）。

#### 2.2.6.2 核心流程

```plantuml
@startuml
title 查询智能闭环联动 核心流程

start

:IntelligenceLoop::run_loop(query_key, query);
:loop_start = Instant::now();

partition "第一步：EXPLAIN 分析" {
  if (explain-analyzer 启用?) then (是)
    :解析 EXPLAIN → ExplainPlan;
    if (解析成功?) then (是)
      :explain_result = Some(plan);
    else (失败)
      :explain_result = None;
      :标注"EXPLAIN step skipped";
    endif
  else (未启用)
    :explain_result = None;
    :标注"EXPLAIN step skipped (not enabled)";
  endif
end partition

partition "第二步：自适应决策" {
  if (adaptive-query 启用 且统计充足?) then (是)
    :AdaptiveExecutor::decide(query_key);
    :adaptive_decision = Some(path);
  else (否)
    :adaptive_decision = None;
    :标注"adaptive step skipped";
  endif
end partition

partition "第三步：火焰图诊断" {
  if (query-flamegraph + slow-query-diagnosis 启用 且 slow?) then (是)
    :QueryTracer::trace_execute;
    :SlowQueryDiagnoser::diagnose;
    :diagnosis_result = Some(report);
  else (否)
    :diagnosis_result = None;
    :标注"diagnosis step skipped";
  endif
end partition

partition "第四步：优化建议（汇总前三步）" {
  :OptimizationAdvisor::suggest(explain_result, stats);
  :suggestions = Vec<OptimizationSuggestion>;
end partition

:loop_elapsed_ms = elapsed;
:生成 LoopReport;
:to_json 输出;

stop

note right
  任一环节失败降级跳过
  不阻断查询,不 panic
  闭环仅可选触发(非每次查询)
end note

@enduml
```

#### 2.2.6.3 复用既有代码（file:line 证据）

| 复用点 | 既有代码位置 | 复用方式 | 验证状态 |
|--------|-------------|---------|---------|
| ExplainPlan EXPLAIN 分析结果 | `packages/sz-orm-explain/src/lib.rs:76` | 闭环第一步复用 | ✅ 已验证 |
| AdaptiveExecutor::decide 自适应决策 | `packages/sz-orm-adaptive/src/executor.rs:157` | 闭环第二步复用 | ✅ 已验证 |
| QueryTracer::trace_execute 分阶段计时 | `packages/sz-orm-flamegraph/src/collector.rs:61` | 闭环第三步复用 | ✅ 已验证 |
| OptimizationAdvisor::suggest 优化建议 | `packages/sz-orm-advisor`（REQ-V44-001 新增） | 闭环第四步复用 | ✅ 已验证（本版本新增） |
| SlowQueryDiagnoser::diagnose 慢查询诊断 | `packages/sz-orm-diagnosis`（REQ-V44-002 新增） | 闭环第三步诊断复用 | ✅ 已验证（本版本新增） |

#### 2.2.6.4 新增依赖

| 包 | 新增依赖 | 用途 | feature 门控 |
|----|---------|------|-------------|
| sz-orm-advisor | sz-orm-explain | ExplainPlan 闭环复用 | `query-intelligence-loop` |
| sz-orm-advisor | sz-orm-adaptive | AdaptiveExecutor::decide 闭环复用 | `query-intelligence-loop` |
| sz-orm-advisor | sz-orm-flamegraph | QueryTracer::trace_execute 闭环复用 | `query-intelligence-loop` |
| sz-orm-advisor | sz-orm-diagnosis | SlowQueryDiagnoser 闭环复用 | `query-intelligence-loop` |
| sz-orm-advisor | serde_json（既有/可选） | LoopReport JSON 序列化 | `query-intelligence-loop` |

#### 2.2.6.5 feature gate 定义

```toml
# packages/sz-orm-advisor/Cargo.toml（扩展）
[features]
query-advisor = ["dep:serde_json", "dep:sz-orm-explain", "dep:sz-orm-adaptive", "dep:sz-orm-ai"]  # 既有 feature
query-intelligence-loop = ["query-advisor", "dep:sz-orm-flamegraph", "dep:sz-orm-diagnosis"]  # 新增 feature，依赖 query-advisor
# 默认关闭
```

#### 2.2.6.6 错误处理策略

| 错误场景 | 处理策略 | 用户感知 |
|---------|---------|---------|
| EXPLAIN 分析失败（解析失败或未启用 explain-analyzer） | 跳过 EXPLAIN 环节，闭环继续自适应 + 诊断 + 建议 | LoopReport 标注"EXPLAIN step skipped" |
| 自适应统计不足（执行次数不足） | 跳过自适应决策环节，闭环继续诊断 + 建议 | LoopReport 标注"adaptive step skipped (insufficient stats)" |
| 火焰图未启用（未启用 query-flamegraph） | 跳过诊断环节，闭环继续建议 | LoopReport 标注"diagnosis step skipped (flamegraph not enabled)" |
| 任一环节 panic（不应发生） | 降级为独立工作（环节结果 None），不阻断查询 | LoopReport 标注对应环节 skipped |

#### 2.2.6.7 测试策略

| 测试类型 | 测试内容 | 验收条件 |
|---------|---------|---------|
| 单元测试 | 闭环四步全成功 → LoopReport 含四步结果 | explain_result/adaptive_decision/diagnosis_result/suggestions 均有值 |
| 单元测试 | EXPLAIN 失败 → 跳过第一步，其余继续 | explain_result = None，标注"EXPLAIN step skipped" |
| 单元测试 | 自适应统计不足 → 跳过第二步 | adaptive_decision = None，标注"adaptive step skipped" |
| 单元测试 | 火焰图未启用 → 跳过第三步 | diagnosis_result = None，标注"diagnosis step skipped" |
| 单元测试 | LoopReport JSON 含四步结果与最终建议 | JSON 解析成功，字段完整 |
| 单元测试 | 闭环总耗时 ≤ 200ms（单查询） | loop_elapsed_ms ≤ 200 |
| 边界测试 | 任一环节失败降级不阻断 | 闭环继续，不 panic |
| 集成测试 | 闭环联动：全表扫描 → 自适应建议分页 → 火焰图诊断 → 优化建议 | LoopReport 含完整闭环 |
| 门禁 | `cargo test -p sz-orm-advisor --features query-intelligence-loop` | 全部通过 |
| 门禁 | 默认 `cargo build -p sz-orm-advisor` 无闭环联动 | 行为与 v4.3.0 一致 |

#### 2.2.6.8 设计理由

1. **为什么扩展 sz-orm-advisor 而非新包**：闭环联动是优化建议引擎的协同扩展（第四步是 suggest），同属"分析/建议"层，扩展包复用既有 `OptimizationAdvisor`，新包会割裂建议能力，符合"相关代码放在一起"。
2. **为什么四步联动而非更多/更少**：四步覆盖查询智能完整链路（检测 EXPLAIN → 决策自适应 → 诊断火焰图 → 建议优化），既不遗漏关键环节，也不过度复杂（每步有明确输入输出），符合"函数只做一件事"。
3. **为什么任一环节失败降级而非阻断**：闭环是可选增强（`query-intelligence-loop` feature），任一环节失败不应阻断查询（查询是核心路径），降级跳过保证可用性，符合 spec.md §4.2.8 可靠性约束。
4. **为什么 P3 优先级最低**：闭环联动依赖前五项能力（EXPLAIN/自适应/火焰图/建议/诊断），需前五项交付后才能闭环，且价值是跨能力协同（非独立核心能力），排 P3 最后，符合 spec.md 优先级声明。
5. **为什么复用既有 decide/trace_execute/suggest**：既有 `AdaptiveExecutor::decide`（`:157`）/ `QueryTracer::trace_execute`（`:61`）已实现决策/采集，REQ-V44-001 `suggest` 已实现建议生成，重写违反"复用优先"且重复实现各环节逻辑。

---

# 三、跨需求关注点

## 3.1 错误处理统一策略

### 3.1.1 错误类型设计原则

| 原则 | 说明 | 适用需求 |
|------|------|---------|
| 降级不阻断 | 优化建议/诊断报告/闭环联动任一环节失败降级跳过，不阻断查询 | REQ-V44-001/002/006 |
| 明确不静默 | db-fusion 主库失败返回缓存 + degraded 标记，不静默返回脏数据 | REQ-V44-003 |
| 日志不阻断 | 结构化日志写入失败静默丢弃（日志是辅助），不阻断查询 | REQ-V44-004 |
| CI 非阻断 | 性能回归检测非阻断（建议性门禁），报告供人工判断 | REQ-V44-005 |
| 不 panic | 所有解析/分析/诊断失败返回 Result/Option，不 panic | 全需求 |
| 建议不执行 DDL | 优化建议仅生成文本，不自动执行 DDL | REQ-V44-001/002 |

### 3.1.2 错误码统一表

| 错误类型 | 来源 | 处理 | 用户感知 |
|---------|------|------|---------|
| EXPLAIN 分析结果缺失 | 未启用 explain-analyzer 或解析失败 | 降级为仅统计建议 / 闭环跳过 EXPLAIN 环节 | 建议报告/LoopReport 标注 |
| 自适应统计不足 | total_executions < min_executions | 降级为仅 EXPLAIN 建议 / 闭环跳过自适应环节 | 建议报告/LoopReport 标注 |
| 建议冲突 | AddIndex 与 DropIndex 针对同一索引 | 按置信度排序，低置信度标注"conflict, skipped" | 建议报告标注 |
| 火焰图阶段耗时缺失 | 未启用 query-flamegraph | 诊断降级 Unknown 根因 / 日志 phase_breakdown 为空 / 基线仅比对总耗时 | 诊断报告/日志/CI 标注 |
| 耗时不符 | 阶段耗时总和与总耗时误差超 10% | 标注"timing mismatch"，根因置信度降低 | 诊断报告标注 |
| TTL 缓存过期 + 主库不可用 | 缓存 TTL 过期且主库查询失败 | 返回明确错误（不返回脏数据，除非降级模式） | 错误"cache expired and primary unavailable" |
| 失效总线不可用 | Redis Pub/Sub 断连 | 降级为 TTL 过期兜底 | 告警"invalidation bus unavailable, relying on TTL" |
| CDC 捕获失败 | WAL/binlog 未配置 | 降级为 TTL 过期兜底 | 告警"CDC capture failed, relying on TTL" |
| 向量搜索下推失败 | 向量库不可用 | 降级为主库查询（DegradationStatus） | 结果标记"vector search degraded" |
| CDC 事件脱敏失败 | 敏感字段脱敏失败 | 跳过该事件 | 告警"CDC event masking failed, skipped" |
| 日志写入失败 | 磁盘满/网络断 | 静默丢弃日志 + 告警计数 | 查询正常，日志丢失 |
| 脱敏失败 | 参数脱敏规则匹配失败 | 参数替换为 `[masking failed]` | 日志 params 为 `[masking failed]` |
| 基线文件缺失 | CI 中基线 JSON 不存在 | 跳过比对 + 告警 | CI 告警"perf baseline not found" |
| 闭环环节失败 | EXPLAIN/自适应/诊断任一失败 | 降级跳过该环节，闭环继续 | LoopReport 标注对应环节 skipped |

## 3.2 日志与可观测

### 3.2.1 结构化日志统一

| 需求 | 日志事件 | 级别 |
|------|---------|------|
| REQ-V44-001 | 优化建议生成、建议冲突跳过、低置信度标注 | info/warn |
| REQ-V44-002 | 慢查询诊断触发、根因判定、降级 Unknown | info/warn |
| REQ-V44-003 | TTL 缓存过期、失效广播、CDC 同步、向量下推降级 | info/warn |
| REQ-V44-004 | 结构化查询日志输出、采样跳过、脱敏失败 | info/debug |
| REQ-V44-005 | 性能基线采集、CI 回归检出、基线缺失跳过 | info/warn |
| REQ-V44-006 | 闭环联动执行、环节跳过降级 | info/warn |

### 3.2.2 Prometheus 指标

| 指标 | 类型 | 适用需求 | 说明 |
|------|------|---------|------|
| `sz_orm_advisor_suggestions_total` | Counter | REQ-V44-001 | 优化建议生成总数（按 suggestion_type 标签） |
| `sz_orm_advisor_suggestion_conflicts_total` | Counter | REQ-V44-001 | 建议冲突跳过总数 |
| `sz_orm_diagnosis_reports_total` | Counter | REQ-V44-002 | 慢查询诊断报告生成总数（按 root_cause 标签） |
| `sz_orm_fusion_ttl_cache_hits_total` | Counter | REQ-V44-003 | TTL 融合缓存命中总数 |
| `sz_orm_fusion_ttl_cache_expired_total` | Counter | REQ-V44-003 | TTL 缓存过期总数 |
| `sz_orm_fusion_invalidation_publishes_total` | Counter | REQ-V44-003 | 失效广播发布总数 |
| `sz_orm_fusion_cdc_sync_events_total` | Counter | REQ-V44-003 | CDC 同步事件总数 |
| `sz_orm_fusion_vector_pushdown_total` | Counter | REQ-V44-003 | 向量搜索下推总数（按 degraded 标签） |
| `sz_orm_query_log_entries_total` | Counter | REQ-V44-004 | 结构化查询日志输出总数（按 level/slow 标签） |
| `sz_orm_query_log_sampled_total` | Counter | REQ-V44-004 | 采样通过总数 |
| `sz_orm_perf_regressions_total` | Counter | REQ-V44-005 | 性能回归检出总数（按 regression_type 标签） |
| `sz_orm_intelligence_loop_runs_total` | Counter | REQ-V44-006 | 闭环联动执行总数 |
| `sz_orm_intelligence_loop_skipped_steps_total` | Counter | REQ-V44-006 | 闭环环节跳过总数（按 step 标签） |

## 3.3 配置管理

### 3.3.1 配置统一原则

所有新增配置通过 `AdvisorConfig` / `DiagnosisConfig` / `FusionConfig` / `QueryLogger` / `PerfBaseline` / `IntelligenceLoop` 显式传入，默认值保证"无配置环境行为不变"。

### 3.3.2 关键配置项

| 配置项 | 默认值 | 适用需求 | 说明 |
|--------|--------|---------|------|
| `advisor.row_threshold` | 1000 | REQ-V44-001 | AddIndex 建议行数阈值 |
| `advisor.confidence_threshold` | 0.5 | REQ-V44-001 | 低置信度标注"需人工确认"阈值 |
| `diagnosis.pool_threshold_pct` | 30.0 | REQ-V44-002 | PoolExhaustion 根因阈值（占比 %） |
| `diagnosis.sql_threshold_pct` | 50.0 | REQ-V44-002 | SqlInefficiency 根因阈值 |
| `diagnosis.result_threshold_pct` | 30.0 | REQ-V44-002 | LargeResultSet 根因阈值 |
| `diagnosis.build_threshold_pct` | 20.0 | REQ-V44-002 | BuildOverhead 根因阈值 |
| `fusion.ttl_default` | 60s | REQ-V44-003 | TTL 融合缓存默认过期时间 |
| `fusion.invalidation_bus` | None | REQ-V44-003 | 失效总线（None = 单实例，Redis/Gossip = 跨实例） |
| `fusion.cdc_enabled` | false | REQ-V44-003 | CDC 增量同步启用 |
| `logging.sample_rate` | 0.01 | REQ-V44-004 | 采样率（1%，慢查询 100%） |
| `logging.level` | Info | REQ-V44-004 | 日志级别（Debug/Info/Warn） |
| `perf.threshold_factor` | 3 | REQ-V44-005 | 性能回归阈值因子（2 保守/3 常规/5 宽松） |
| `loop.enabled` | false | REQ-V44-006 | 闭环联动启用（query-intelligence-loop feature） |

## 3.4 feature gate 组合验证

### 3.4.1 feature 依赖关系

```plantuml
@startuml
title v4.4.0 feature gate 依赖关系

feature "query-advisor" as f1
feature "slow-query-diagnosis" as f2
feature "db-fusion-v2" as f3
feature "query-logging" as f4
feature "perf-baseline" as f5
feature "query-intelligence-loop" as f6

feature "v4.3.0 explain-analyzer" as v43_explain
feature "v4.3.0 query-flamegraph" as v43_flame
feature "v4.3.0 adaptive-query" as v43_adaptive
feature "v4.3.0 db-fusion" as v43_fusion

f1 --> v43_explain : 复用 ExplainPlan
f1 --> v43_adaptive : 复用 QueryStats
f2 --> v43_flame : 复用 QueryPhaseTiming
f2 --> v43_adaptive : 复用 slow 标记
f2 --> f1 : 建议联动
f3 --> v43_fusion : 扩展 POC
f4 --> v43_flame : 复用 QueryPhaseTiming
f5 --> v43_explain : 复用 PlanSnapshot
f5 --> v43_flame : 复用 QueryPhaseTiming
f6 --> f1 : 闭环第四步(建议)
f6 --> f2 : 闭环第三步(诊断)
f6 --> v43_adaptive : 闭环第二步(decide)
f6 --> v43_flame : 闭环第三步(trace_execute)

note bottom of f1
  6 个 feature 默认全关闭
  与 v4.2.0 7 个 + v4.3.0 7 个 feature 任意组合编译通过
  不影响既有 feature 行为
end note

@enduml
```

### 3.4.2 feature 全组合编译验证

| 验证项 | 命令 | 验收条件 |
|--------|------|---------|
| 默认编译 | `cargo build --workspace` | 无新能力，行为与 v4.3.0 一致 |
| 单 feature 编译 | `cargo build --features sz-orm-advisor/query-advisor` 等 6 项 | 各 feature 独立编译通过 |
| 全 feature 编译 | `cargo build --workspace --all-features` | 全 feature 组合编译通过 |
| 与 v4.3.0 feature 组合 | `cargo build --features sz-orm-explain/explain-analyzer,sz-orm-advisor/query-advisor,...` | v4.3.0 + v4.4.0 feature 任意组合编译通过 |
| 与 v4.2.0 feature 组合 | `cargo build --features sz-orm-dtx/cross-lang-dtx,sz-orm-advisor/query-advisor,...` | v4.2.0 + v4.4.0 feature 任意组合编译通过 |
| 既有 feature 不破坏 | `cargo build --features sz-orm-dtx/xa,sz-orm-wasm/js` | 既有 feature 行为不变 |

## 3.5 五方言覆盖策略

| 需求 | 五方言覆盖点 | 复用既有 | 方言差异处理 |
|------|-------------|---------|-------------|
| REQ-V44-001 | 优化建议基于 ExplainPlan（5 方言解析结果），建议生成方言无关 | `ExplainPlan` `explain/lib.rs:76`（5 方言解析器 `parser_for` `:127`） | 建议规则方言无关（FullTable 判定由既有解析器完成）；AddIndex 的 DDL 文本按方言生成（MySQL/PG/SQLite/Oracle/MSSQL 索引 DDL 语法差异） |
| REQ-V44-002 | 慢查询诊断基于 QueryPhaseTiming（阶段耗时，方言无关） | `QueryPhaseTiming` `flamegraph/collector.rs:39` | 诊断根因分析方言无关（阶段耗时占比与方言无关） |
| REQ-V44-003 | db-fusion 转正 CDC 同步（5 方言 WAL/Binlog/Trigger/LogMiner）+ 向量下推 | `DialectCapturer` `queue/cdc/capturer.rs:12` + `HybridSearcher` `vector/searcher.rs:30` | CDC 各方言捕获复用既有；向量下推方言无关（HybridSearcher 三源并行） |
| REQ-V44-004 | 结构化查询日志（方言无关，日志格式统一 JSON） | `MaskingRule` `masking/lib.rs:21` | 日志结构方言无关，参数脱敏通用 |
| REQ-V44-005 | 性能基线基于 ExplainPlan + QueryPhaseTiming（5 方言计划 + 方言无关耗时） | `PlanSnapshot` `explain/regression.rs:23` + `QueryPhaseTiming` | 计划基线 5 方言（复用既有解析）；耗时基线方言无关 |
| REQ-V44-006 | 闭环联动复用 EXPLAIN/自适应/火焰图/建议（5 方言覆盖由各环节保证） | 各环节既有代码 | 闭环协调方言无关（各环节方言覆盖） |

**方言 DDL 差异处理（AddIndex 建议的 action 字段）**：
- MySQL：`CREATE INDEX idx_xxx ON table(col)`
- PostgreSQL：`CREATE INDEX idx_xxx ON table(col)`
- SQLite：`CREATE INDEX idx_xxx ON table(col)`
- Oracle：`CREATE INDEX idx_xxx ON table(col)`
- MSSQL：`CREATE INDEX idx_xxx ON table(col)`

（索引 DDL 语法五方言基本一致，差异在索引类型选项如 `USING GIST`（PG）/`CLUSTERED`（MSSQL），建议生成通用 DDL + 方言特定选项标注）

---

# 四、风险与缓解

## 4.1 风险矩阵

| 风险 ID | 风险描述 | 影响 | 概率 | 缓解措施 | 责任需求 |
|---------|---------|------|------|---------|---------|
| R-001 | 优化建议误报（全表扫描在小表场景合法） | 中（开发者困扰） | 中 | 建议保守性：低置信度标注"需人工确认"，建议仅生成文本不自动执行 DDL，`advisor.row_threshold` 可配 | REQ-V44-001 |
| R-002 | 建议冲突（AddIndex 与 DropIndex 针对同一索引） | 低（建议跳过） | 中 | 按置信度排序，保留高置信度建议，低置信度标注"conflict, skipped" | REQ-V44-001 |
| R-003 | 慢查询诊断根因误判（阶段耗时占比阈值不当） | 中（错误根因） | 中 | 阈值可配（DiagnosisConfig），多根因判定 MixedCause，耗时不符标注 timing mismatch 降低置信度 | REQ-V44-002 |
| R-004 | db-fusion TTL 缓存脏读（TTL 内读到旧数据） | 高（数据不一致） | 中 | TTL + 失效广播双保险，主库写入后 publish 失效消息，TTL 可配（默认 60s），主库失败返回 degraded 标记不静默 | REQ-V44-003 |
| R-005 | db-fusion CDC 同步延迟导致缓存/搜索索引过期 | 中（读到旧数据） | 中 | CDC 失败降级为 TTL 过期兜底，CdcCheckpoint 断点续传，延迟期间回退主库 | REQ-V44-003 |
| R-006 | db-fusion 向量搜索下推失败 | 中（查询失败） | 低 | 降级为主库查询（复用 DegradationStatus），标记 vector_degraded，不返回错误 | REQ-V44-003 |
| R-007 | 结构化查询日志泄露敏感参数 | 高（安全违规） | 低 | 复用既有 MaskingRule 参数脱敏，脱敏失败替换为 `[masking failed]` 不暴露原始值 | REQ-V44-004 |
| R-008 | 结构化查询日志量大影响性能 | 中（性能下降） | 中 | 采样率默认 1%（慢查询 100%），日志写入 ≤ 100μs/次，写入失败静默丢弃不阻断查询 | REQ-V44-004 |
| R-009 | 性能回归基线误报（环境波动导致正常波动被报告） | 中（CI 噪音） | 中 | 阈值因子可配（2 保守/3 常规/5 宽松），CI 非阻断（建议性门禁），报告标注阈值因子 | REQ-V44-005 |
| R-010 | 闭环联动任一环节失败 | 低（降级跳过） | 中 | 任一环节失败降级为独立工作（环节结果 None），不阻断查询，LoopReport 标注 skipped | REQ-V44-006 |
| R-011 | 新增 feature 与 v4.2.0/v4.3.0 feature 组合编译失败 | 高（编译破坏） | 低 | 门禁 10 全组合编译 + feature 依赖关系验证（§3.4） | 全需求 |
| R-012 | sz-pay 既有代码因 API 变更破坏 | 高（生产故障） | 低 | 无 Breaking Change，6 个 feature gate 隔离默认关闭，既有公开 API 完全向后兼容，sz-pay 回归测试 | 全需求 |
| R-013 | db-fusion 转正破坏既有 POC API | 高（编译破坏） | 低 | POC API `#[deprecated]` 标注而非删除，转正 API `#[stable]`，既有 POC API 保留可用 | REQ-V44-003 |
| R-014 | 优化建议自动执行 DDL 导致生产事故 | 高（数据损坏） | 低 | 建议仅生成文本（action 字段），不自动执行 DDL，需人工确认后执行 | REQ-V44-001 |
| R-015 | 闭环联动每次查询触发开销大 | 中（性能下降） | 低 | `query-intelligence-loop` feature 默认关闭，闭环仅可选触发（非每次查询），开销 ≤ 200ms | REQ-V44-006 |

## 4.2 风险缓解验证

| 风险 | 验证方法 | 验收条件 |
|------|---------|---------|
| R-001 | 优化建议保守性测试（小表全表扫描不生成 AddIndex，低置信度标注） | 建议保守，不自动执行 DDL |
| R-002 | 建议冲突测试（AddIndex + DropIndex 同索引） | 高置信度保留，低置信度标注 skipped |
| R-003 | 根因误判测试（多根因 MixedCause，耗时不符 timing mismatch） | 根因判定正确，降级标注 |
| R-004 | TTL 缓存脏读测试（TTL + 失效广播双保险） | 主库写入后跨实例失效，TTL 过期回源 |
| R-005 | CDC 同步延迟测试（CDC 失败降级 TTL 兜底） | CDC 失败降级正确 |
| R-006 | 向量下推失败降级测试 | 降级主库查询 + vector_degraded 标记 |
| R-007 | 参数脱敏测试（手机号/身份证脱敏，脱敏失败 `[masking failed]`） | 不暴露原始值 |
| R-008 | 日志采样性能测试（1% 采样 + 慢查询 100%） | 采样率生效，性能 ≤ 100μs/次 |
| R-009 | 性能基线误报测试（阈值因子可配） | 正常波动不误报，CI 非阻断 |
| R-010 | 闭环环节失败降级测试 | 降级跳过，不阻断查询 |
| R-011 | feature 全组合编译 | `cargo check --workspace --all-targets --all-features` 通过 |
| R-012 | sz-pay 回归测试 | sz-pay 既有测试套件通过 |
| R-013 | POC API 保留测试（#[deprecated] 标注） | POC API 编译可用，转正 API #[stable] |
| R-014 | 建议不执行 DDL 测试 | 建议仅生成文本，无 DDL 执行 |
| R-015 | 闭环开销测试（≤ 200ms） | loop_elapsed_ms ≤ 200 |

---

# 五、需求追溯矩阵（设计侧）

| 需求编号 | 设计章节 | 核心设计决策 | 复用既有代码（关键） | feature gate | 风险 |
|---------|---------|-------------|---------------------|-------------|------|
| REQ-V44-001 | §2.2.1 | 新增 sz-orm-advisor（OptimizationAdvisor 规则引擎 + OptimizationSuggestion 六种建议类型 + JSON 报告），复用既有 ExplainPlan/QueryStats/AI 建议结构，无 AI 依赖 | `explain/lib.rs:76/91` ExplainPlan/missing_index + `regression.rs:69` PlanRegression + `adaptive/stats.rs:11/66/73` QueryStats/should_paginate/should_cache + `ai/index_advisor.rs:71` IndexSuggestion + `ai/rewrite_advisor.rs:61` RewriteSuggestion + `ai/auto_tuning/mod.rs:71` TuningSuggestion + `ai/auto_tuning/pipeline.rs:15` AutoTuningPipeline | `query-advisor` | R-001/R-002/R-014 |
| REQ-V44-002 | §2.2.2 | 新增 sz-orm-diagnosis（SlowQueryDiagnoser 根因分析 + DiagnosisReport 双格式输出 + 建议联动），复用既有 QueryPhaseTiming/QueryOutcome.slow，仅对慢查询触发 | `flamegraph/collector.rs:11/39/61` Phase/QueryPhaseTiming/trace_execute + `adaptive/executor.rs:116/35` QueryOutcome.slow/slow_ms + `sz-orm-advisor` 建议联动 | `slow-query-diagnosis` | R-003 |
| REQ-V44-003 | §2.2.3 | 扩展 sz-orm-fusion（TtlFusionCache TTL 缓存 + InvalidationBus 失效广播 + CdcSyncCoordinator CDC 同步 + VectorPushdownExecutor 向量下推 + 转正 API #[stable]/POC API #[deprecated]），复用既有 InvalidationBus/DialectCapturer/HybridSearcher | `fusion/plan.rs:21/104` FusionConfig/FusionPlanner + `fusion/executor.rs:8/16/55/63/93/118` FusionCache/MemoryFusionCache/degraded/FusionExecutor/execute/搜索下推 + `core/l2_cache.rs:82/93` InvalidationBus/LocalInvalidationBus + `core/dist_cache.rs:41/179` RedisPubSub/GossipInvalidationBus + `queue/cdc/capturer.rs:12` DialectCapturer + `queue/cdc/downstream.rs:12/178` DownstreamSink/distribute_to_all + `queue/cdc/checkpoint.rs` CdcCheckpoint + `queue/cdc/masking.rs` CDC 脱敏 + `vector/hybrid_search/searcher.rs:30/60` HybridSearcher/DegradationStatus + `vector/hybrid_search/pushdown.rs:6` FilterPushdown | `db-fusion-v2` | R-004/R-005/R-006/R-013 |
| REQ-V44-004 | §2.2.4 | 扩展 sz-orm-observability（QueryLogger 结构化日志器 + QueryLogEntry JSON + 采样与级别控制 + 参数脱敏），复用既有 MetricsRegistry/QueryPhaseTiming/MaskingRule | `observability/lib.rs:253/421` MetricsRegistry/start_metrics_server + `flamegraph/collector.rs:39` QueryPhaseTiming + `masking/lib.rs:21` MaskingRule | `query-logging` | R-007/R-008 |
| REQ-V44-005 | §2.2.5 | 扩展 sz-orm-explain（PerfBaseline 耗时基线 + PerfRegression 性能回归 + check_perf_regressions CI 入口），复用既有 PlanSnapshot/PlanRegression/check_regressions/QueryPhaseTiming | `explain/regression.rs:23/34/69/161` PlanSnapshot/PlanBaseline/PlanRegression/check_regressions + `flamegraph/collector.rs:11/39` Phase/QueryPhaseTiming | `perf-baseline` | R-009 |
| REQ-V44-006 | §2.2.6 | 扩展 sz-orm-advisor（IntelligenceLoop 闭环协调器 + LoopReport 闭环报告），复用既有 ExplainPlan/AdaptiveExecutor::decide/QueryTracer::trace_execute + REQ-V44-001 suggest + REQ-V44-002 diagnose，任一环节失败降级 | `explain/lib.rs:76` ExplainPlan + `adaptive/executor.rs:157` decide + `flamegraph/collector.rs:61` trace_execute + `sz-orm-advisor` suggest + `sz-orm-diagnosis` diagnose | `query-intelligence-loop` | R-010/R-015 |

---

# 六、验收对齐

本设计与 spec.md 验收标准对齐：

| spec.md 验收标准 | 设计章节 | 对齐说明 |
|-----------------|---------|---------|
| §8.1 REQ-V44-001 规则引擎 + 复用 AI 建议结构 + 复用 should_paginate/should_cache + 六种建议类型 + 低置信度标注 + JSON 报告 + feature gate 隔离 | §2.2.1 | 全部覆盖，复用既有 ExplainPlan/QueryStats/IndexSuggestion/RewriteSuggestion/TuningSuggestion，规则引擎无 AI 依赖 |
| §8.2 REQ-V44-002 根因分析 + 复用 slow 标记 + DiagnosisReport + 建议联动 + 双格式输出 + feature gate 隔离 | §2.2.2 | 全部覆盖，复用既有 QueryPhaseTiming/QueryOutcome.slow/slow_ms，仅对慢查询触发 |
| §8.3 REQ-V44-003 TTL 缓存 + 失效广播 + CDC 同步 + 向量下推 + 转正 API #[stable]/POC API #[deprecated] + 降级语义 + feature gate 隔离 | §2.2.3 | 全部覆盖，复用既有 InvalidationBus/DialectCapturer/DownstreamSink/HybridSearcher/FilterPushdown，两阶段转正 |
| §8.4 REQ-V44-004 QueryLogEntry JSON + 复用 QueryPhaseTiming + 采样率/级别控制 + 参数脱敏 + feature gate 隔离 | §2.2.4 | 全部覆盖，复用既有 MetricsRegistry/QueryPhaseTiming/MaskingRule |
| §8.5 REQ-V44-005 PerfBaseline + PerfRegression + 复用 PlanRegression/check_regressions + 复用 QueryPhaseTiming + CI 入口 + feature gate 隔离 | §2.2.5 | 全部覆盖，复用既有 PlanSnapshot/PlanRegression/check_regressions/QueryPhaseTiming，一次比对同时检出耗时与计划回归 |
| §8.6 REQ-V44-006 IntelligenceLoop 四步联动 + LoopReport JSON + 降级非阻断 + feature gate 隔离 | §2.2.6 | 全部覆盖，复用既有 ExplainPlan/decide/trace_execute + REQ-V44-001 suggest + REQ-V44-002 diagnose，任一环节失败降级 |
| §4.5 API 兼容性 + feature gate 隔离 + 测试基线不回退 + 五方言一致 + 审计证据 + 14 道门禁 + 无占位 + unsafe 零容忍 + 复用优先 + 无 Breaking Change + 与 v4.3.0 零重叠 | §3.4 + 全文 | 全部覆盖，6 feature gate 隔离，40+ 项 file:line 证据验证通过，与 v4.3.0 零重叠（分析/建议/转正层 vs 采集/检测层） |

---

> 本设计文档所有 file:line 证据均已通过源码读取验证（2026-08-12，40+ 项关键证据逐项实测），遵循 AGENTS.md 审计合规铁律。每项设计决策附"为什么这样设计"设计理由，每个复用点附 file:line 代码证据。本设计与 spec.md（What to build）完全对齐，不增删技术方案，与 v4.3.0 边界清晰（零重叠：v4.3.0 是"采集/检测"层，v4.4.0 是"分析/建议/转正"层）。下一阶段：spec-task-agent 生成 `tasks.md`（编码任务规划）。