# sz-orm v4.3.0 需求规格说明书

> 版本：v4.3.0（编译期 EXPLAIN 分析 + 查询性能火焰图 + N+1 静态检测 + 数据血缘可视化 + 编译期数据治理 + 自适应查询 + 多数据库融合（可选））
> 基线：v4.2.0（跨语言分布式事务 + Go/Java/C++ 绑定 + 可视化 Schema 设计器 + OpenAPI → ORM 反向生成 + WASM 真实数据库连接，5 项需求 REQ-V42-001~005 全部通过 feature gate 隔离）
> 日期：2026-08-12
> 需求格式：EARS（Ubiquitous / Event-driven / State-driven / Optional / Unwanted）
> 文档定位：需求规格（What to build），不含技术设计（How to build）
> 优先级声明：五项任务按"P1（编译期 EXPLAIN 分析 + 火焰图 + N+1 静态检测，查询智能与开发者体验核心）→ P2（数据血缘可视化 + 编译期治理 + 自适应查询，数据治理与运行时优化）→ P3（多数据库融合查询，可选实验验证价值）"序推进
> 需求编号约定：REQ-V43-xxx（v4.3.0 需求项，REQ-V43-001 ~ REQ-V43-005）
> 规划依据：`docs/spec/v4.3.0/development-plan.md`（681 行，26 任务 / 152 子任务 / 13 周，7 个新 feature gate 隔离）+ `docs/评估/` 8 份评估报告 + 2026-08-12 逐项代码验证（file:line 均已实测存在）
> 兼容性铁律：所有新能力通过 feature gate 隔离，默认 feature 行为不变，既有公开 API 完全向后兼容，v4.2.0 已验收测试基线不回退；sz-pay 生产依赖（从 crates.io 拉取 sz-orm-* 6 个包）不得被破坏；五方言覆盖：MySQL/PostgreSQL/SQLite/Oracle/MSSQL
> 范围声明：本版本聚焦查询智能与开发者体验（编译期查询分析前移 + 运行时自适应 + 数据治理）；更长期（v4.x+ Informix 真实驱动/编译期数据库重写/类型级查询验证）在后续版本规划；本版本不涉及 crates.io 发布流程变更
> 边界声明：与 v4.2.0 零重叠（见第 1.4 节），唯一同文件风险 `cli/src/main.rs`（v4.2.0 M3-T7/M3-T13 与 v4.3.0 M2-T2 均修改此文件）通过排期规避（M2 在 v4.2.0 交付后启动）

---

# 1. 组件定位

## 1.1 核心职责

本组件负责交付 sz-orm v4.3.0 的五项查询智能与开发者体验能力：(1) 编译期 EXPLAIN 分析 + 查询性能火焰图（扩展既有 `query!` 宏 db-verify 模式 `packages/sz-orm-macros/src/lib.rs:548`，新增 `sz-orm-explain` 包解析 5 方言 EXPLAIN 输出为统一 `ExplainPlan`，检测全表扫描/缺失索引并输出编译期警告，新增 `sz-orm-flamegraph` 包采集查询各阶段耗时生成 Brendan Gregg 格式火焰图 + SVG）；(2) N+1 静态检测器（新增 `sz-orm-n1-lint` 包，函数级标注宏 + syn AST 分析，检测循环内查询调用模式，复用既有运行时 `N1QueryDetector` `packages/sz-orm-core/src/entity_graph.rs:641` 检测知识）；(3) 数据血缘可视化 + 编译期数据治理（扩展既有 `LineageGraph` `packages/sz-orm-audit/src/lineage/graph.rs:96` 导出 Mermaid/Graphviz/HTML + 影响分析，新增 `compile-governance` feature 提供 `#[pii]`/`#[mask]` 编译期强制标注 + 合规报告）；(4) 自适应查询优化器（新增 `sz-orm-adaptive` 包，运行时统计采集 + 自动游标分页 + 热点缓存 + 慢查询降级，复用既有 `cursor_stream.rs`/`paginator.rs`/`l1_cache.rs`/`l2_cache.rs`）；(5) 多数据库融合查询（可选/实验，新增 `sz-orm-fusion` 包，主库 + 缓存 + 向量库透明拆分/聚合）。所有能力通过 feature gate 隔离，不破坏现有 API 兼容性与 v4.2.0 已验收基线。

## 1.2 核心输入

1. **v4.2.0 已验收基线**：跨语言分布式事务 + Go/Java/C++ 绑定 + 可视化 Schema 设计器 + OpenAPI → ORM 反向生成 + WASM 真实数据库连接，5 项能力全部通过 feature gate 隔离，作为本版本基准。
2. **v4.3.0 开发规划**：`docs/spec/v4.3.0/development-plan.md`（681 行，6 个里程碑 M0~M5 + M6 最终验证，26 任务 / 152 子任务 / 13 周）。
3. **现有能力清单与缺口证据**：
   - **编译期 EXPLAIN 分析**：`packages/sz-orm-macros/src/lib.rs:548` `SZ_ORM_QUERY_VERIFY` 环境变量开关（db-verify 模式连真 DB 执行 EXPLAIN），`:298` `validate_sql_content`（SQL 语法校验），`:642-645` 各方言 EXPLAIN 语句构造（MySQL/PostgreSQL `EXPLAIN`、SQLite `EXPLAIN QUERY PLAN`、Oracle `EXPLAIN PLAN FOR`）。缺口：EXPLAIN 输出未解析为结构化计划，无全表扫描/缺失索引编译期警告，无执行计划回归检测，无独立 EXPLAIN 解析包。
   - **查询性能火焰图**：`packages/sz-orm-tracing/src/lib.rs:129` `pub trait Tracer`（分布式追踪 trait）、`:136` `pub struct SzTracer`（span 关联实现），`packages/sz-orm-core/src/query.rs:36` `pub struct QueryBuilder`（查询构造）、`packages/sz-orm-core/src/pool.rs:45` `pub trait Connection`（连接执行）。缺口：无查询各阶段（构造/绑定/池获取/执行/映射）分阶段计时，无 Brendan Gregg 火焰图格式输出，无 SVG 渲染。
   - **N+1 静态检测**：`packages/sz-orm-core/src/entity_graph.rs:641` `pub struct N1QueryDetector`（运行时 N+1 检测器，`:751` impl），`packages/sz-orm-core/src/eager_loader.rs`（预加载器）、`packages/sz-orm-core/src/smart_eager_loader.rs`（智能预加载）。缺口：仅运行时检测（查询执行后才发现 N+1），无编译期/开发期静态检测（函数标注 + AST 分析），无 CLI 批量扫描，无循环内查询调用模式静态识别。
   - **数据血缘可视化**：`packages/sz-orm-audit/src/lineage/graph.rs:96` `pub struct LineageGraph`（血缘图，587 LOC，161 实测测试），`packages/sz-orm-audit/src/lineage/export.rs:16` `export` 函数（`:34` `export_dot`、`:68` `export_json`、`:105` `export_graphml`，已有 Dot/Json/GraphML 三种格式）。缺口：无 Mermaid 格式导出，无 HTML 报告导出，无血缘影响分析（下游影响/上游追溯），无与迁移 dry-run 联动。
   - **编译期数据治理**：`packages/sz-orm-core/src/access_control.rs:9` `pub struct AccessRule`（ABAC 权限规则）、`:22` `pub struct AccessContext`（访问上下文）、`:85` `pub struct RowLevelSecurity`（行级安全），`packages/sz-orm-masking/src/lib.rs:21` `pub enum MaskingRule`（脱敏规则）。缺口：无 `#[pii]`/`#[mask]` 编译期标注强制，无 PII 字段未脱敏编译报错，无合规报告生成（GDPR/等保清单）。
   - **自适应查询**：`packages/sz-orm-ai/src/auto_tuning/pipeline.rs:15` `pub struct AutoTuningPipeline`（AI 离线闭环 424 LOC），`packages/sz-orm-core/src/cursor_stream.rs`（游标分页）、`packages/sz-orm-core/src/paginator.rs`（分页器）、`packages/sz-orm-core/src/l1_cache.rs`（L1 缓存）、`packages/sz-orm-core/src/l2_cache.rs`（L2 缓存）、`packages/sz-orm-core/src/plan_cache.rs`（计划缓存）。缺口：仅 AI 离线优化，无轻量运行时自适应（统计采集 → 自动分页/缓存决策），无慢查询降级。
   - **多数据库融合**：`packages/sz-orm-core/src/db_type.rs:11` `pub enum DbType`（28 方言枚举），`packages/sz-orm-vector/src/hybrid_search/searcher.rs:30` `pub struct HybridSearcher`（三源并行混合搜索），`packages/sz-orm-queue/src/cdc/capturer.rs:12` `pub trait DialectCapturer`（CDC 捕获器，各方言 WAL/Binlog/Trigger/LogMiner）。缺口：无透明多数据库融合查询（主库 + 缓存 + 向量库自动拆分/聚合），无查询拆分计划器。
4. **本机数据库连接信息**：MySQL 9.6（`mysql://root:test123@127.0.0.1:3306/sz_orm_test`）、PostgreSQL 18（`postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`）、Oracle 23ai Free（`127.0.0.1:1521/freepdb1`）。
5. **sz-pay 生产依赖证据**：sz-pay 从 crates.io 拉取 sz-orm-* 6 个包，作为 API 兼容性验证的下游基准。
6. **五方言覆盖约束**：MySQL/PostgreSQL/SQLite/Oracle/MSSQL，EXPLAIN 解析/数据治理/融合查询须覆盖全部方言（按方言能力适配）。
7. **既有 feature gate 体系**：`packages/sz-orm-core/Cargo.toml` 已有 25+ feature，v4.2.0 新增 7 个 feature（`cross-lang-dtx` / `lang-binding-go` / `lang-binding-java` / `lang-binding-cpp` / `schema-designer` / `openapi-reverse` / `wasm-real-db`），作为新能力 feature gate 隔离的基础。
8. **既有 db-verify EXPLAIN 执行链路**：`packages/sz-orm-macros/src/lib.rs:548` `SZ_ORM_QUERY_VERIFY` 环境变量 + `:642-645` 各方言 EXPLAIN 语句构造，为编译期 EXPLAIN 分析扩展基础。

## 1.3 核心输出

1. **编译期 EXPLAIN 分析 + 火焰图**：sz-orm-explain（5 方言 EXPLAIN 解析器 + `ExplainPlan` 统一结构 + 执行计划回归检测）+ query! 宏编译期性能警告（全表扫描/缺失索引）+ sz-orm-flamegraph（查询各阶段计时 + Brendan Gregg 折叠格式 + SVG 火焰图）+ PlanSnapshot 基线快照 + CI 回归对比报告。
2. **N+1 静态检测器**：sz-orm-n1-lint（`#[detect_n_plus_one]` 函数标注宏 + syn AST 分析器 + 3 种检测模式）+ CLI 批量扫描命令（`sz-orm n1-lint`）+ JSON/table 输出（CI 可消费）+ 静态/运行时检测交叉验证报告。
3. **数据血缘可视化 + 编译期治理**：Mermaid/Graphviz/HTML 血缘导出 + 血缘影响分析（downstream_impact/upstream_trace）+ `#[derive(Governed)]` + `#[pii]`/`#[mask]` 编译期强制标注 + 合规报告（GDPR/等保清单，JSON 输出）+ 与迁移 dry-run 联动。
4. **自适应查询优化器**：sz-orm-adaptive（`QueryStats` 统计采集 + `AdaptiveExecutor` 决策执行器 + 自动游标分页 + 自动缓存 + 慢查询降级），复用既有 `cursor_stream.rs`/`paginator.rs`/`l1_cache.rs`/`l2_cache.rs`，不重写缓存/分页实现。
5. **多数据库融合查询（可选/实验）**：sz-orm-fusion（`FusionConfig` + `FusionPlanner` 查询拆分 + `FusionExecutor` 聚合执行 + CDC 同步）+《db-fusion 实验评估报告》（POC 结果 + 转正/废弃建议）。
6. **需求追溯矩阵**：本文档第 7 章，建立需求 ↔ 验收条件映射。
7. **验收标准总览**：本文档第 8 章，按需求项汇总验收条件。

## 1.4 职责边界

本组件**不负责**以下事项：

1. **不破坏既有公开 API**：所有新能力通过 feature gate 隔离，既有公开 API 签名保持完全向后兼容。
2. **不改变既有安全铁律**：任何 WHERE 条件必须参数化，默认禁止 `SELECT *`，N+1 检测自动拦截，沿用既有铁律。
3. **不重写 ORM 核心**：`QueryBuilder`（`packages/sz-orm-core/src/query.rs:36`）运行时构造、`Model::table_name()` 运行时返回，不前移到编译期（前移 = 重写 ORM，与"无 Breaking Change + sz-pay 生产依赖不破坏"铁律冲突）。
4. **不替换既有 db-verify EXPLAIN 执行**：既有 `SZ_ORM_QUERY_VERIFY` 环境变量（`packages/sz-orm-macros/src/lib.rs:548`）与各方言 EXPLAIN 语句构造（`:642-645`）保留，新增 EXPLAIN 解析为独立包 `sz-orm-explain`，扩展既有宏在 db-verify 模式下调用解析器，不修改既有 EXPLAIN 执行逻辑。
5. **不替换既有运行时 N+1 检测**：既有 `N1QueryDetector`（`packages/sz-orm-core/src/entity_graph.rs:641`）运行时检测保留，新增静态检测为独立包 `sz-orm-n1-lint`，不修改既有运行时检测逻辑。
6. **不替换既有血缘导出**：既有 `export_dot`/`export_json`/`export_graphml`（`packages/sz-orm-audit/src/lineage/export.rs:34/68/105`）保留，新增 Mermaid/HTML 导出与影响分析为扩展，不修改既有导出逻辑。
7. **不替换既有 ABAC/脱敏**：既有 `AccessRule`/`AccessContext`/`RowLevelSecurity`（`packages/sz-orm-core/src/access_control.rs:9/22/85`）与 `MaskingRule`（`packages/sz-orm-masking/src/lib.rs:21`）保留，新增编译期治理标注为扩展，运行时脱敏复用既有 `sz-orm-masking`。
8. **不替换既有 AI 离线优化**：既有 `AutoTuningPipeline`（`packages/sz-orm-ai/src/auto_tuning/pipeline.rs:15`）AI 离线闭环保留，新增运行时自适应为独立包 `sz-orm-adaptive`，无 AI 依赖，不修改既有 AI 优化。
9. **不重写既有缓存/分页**：既有 `cursor_stream.rs`/`paginator.rs`/`l1_cache.rs`/`l2_cache.rs`/`plan_cache.rs` 保留，自适应查询仅做决策层，复用既有实现。
10. **不与 v4.2.0 任务重叠**：v4.2.0 已占用的包/模块（`sz-orm-dtx`/`sz-orm-cabi`/`sz-orm-go`/`sz-orm-java`/`sz-orm-cpp`/`sz-orm-designer`/`sz-orm-swagger` reverse 模块/`sz-orm-wasm` real_db 模块）本版本不触碰；唯一同文件风险 `cli/src/main.rs`（v4.2.0 M3-T7/M3-T13 与 v4.3.0 M2-T2）通过排期规避（M2 在 v4.2.0 交付后启动）。
11. **不负责 sz-pay / sz-rust 下游代码修改**：ADR-0001 严禁修改下游/上游仓库，仅保证 API 兼容性。
12. **不降低既有测试覆盖**：v4.3.0 不得使 v4.2.0 已验收测试基线回退，仅增不减。
13. **不引入 unsafe**：所有新增代码无 `unsafe` 块，或必须有 `// SAFETY:` 注释，沿用既有 unsafe 零容忍铁律。
14. **不引入 Breaking Change**：新能力通过 feature gate 隔离，默认全关闭，既有 feature 组合行为不变。
15. **不强制启用新能力**：所有新能力默认关闭或可选启用，避免无配置环境行为变化。

---

# 2. 领域术语

**编译期 EXPLAIN 分析（Compile-time EXPLAIN Analysis）**
: 扩展既有 `query!` 宏 db-verify 模式（`packages/sz-orm-macros/src/lib.rs:548`），在编译期连真 DB 执行 EXPLAIN 后，解析输出为统一 `ExplainPlan`，检测全表扫描/缺失索引并输出编译期警告（非阻断），将查询优化前移到开发期。
: 备注：既有 db-verify 已支持 EXPLAIN 执行（`:642-645`），本版本补 EXPLAIN 输出解析 + 性能警告。

**ExplainPlan（执行计划统一结构）**
: 5 方言 EXPLAIN 输出解析后的统一结构（`scan_type: ScanType` / `table` / `index` / `rows` / `extra`），`ScanType` 枚举（FullTable/IndexRange/IndexScan/UniqueLookup/Other）抽象各方言扫描方式。

**执行计划回归检测（Plan Regression Detection）**
: 提供 `ExplainPlan` 基线快照（`PlanSnapshot`）与回归对比，CI 中检测查询性能退化（如 IndexRange 变为 FullTable、索引丢失、扫描行数增长），输出 `PlanRegression` 列表。

**查询性能火焰图（Query Performance Flamegraph）**
: 采集查询各阶段耗时（Build/Bind/PoolAcquire/SqlExecute/ResultMap），输出 Brendan Gregg 折叠格式（`flamegraph.pl` 兼容）与内联 SVG 火焰图，复用既有 `Tracer`（`packages/sz-orm-tracing/src/lib.rs:129`）span 关联。
: 备注：既有 `SzTracer`（`:136`）为 span 关联基础。

**N+1 静态检测（N+1 Static Detection）**
: 函数级静态检测，标注函数后用 `syn` 解析函数体 AST，检测循环内查询调用模式（QueryInLoop/ConditionalQueryInLoop/MissingEagerLoadHint），输出编译期警告/错误，将 N+1 检测前移到开发期。
: 备注：既有 `N1QueryDetector`（`packages/sz-orm-core/src/entity_graph.rs:641`）为运行时检测，本版本补静态检测。

**数据血缘可视化（Data Lineage Visualization）**
: 扩展既有 `LineageGraph`（`packages/sz-orm-audit/src/lineage/graph.rs:96`）导出 Mermaid/Graphviz/HTML 格式 + 影响分析（下游影响范围/上游数据追溯），与迁移 dry-run 联动（DROP 前输出受影响链路）。
: 备注：既有 `export_dot`/`export_json`/`export_graphml`（`export.rs:34/68/105`）为导出基础。

**编译期数据治理（Compile-time Data Governance）**
: 在 `sz-orm-core` 增加 `compile-governance` feature：`#[pii]` 字段标注 + `#[mask(strategy = "...")]` 脱敏策略标注 + 编译期强制（PII 字段必须声明脱敏策略，策略白名单 hash/partial/replace/encrypt）+ 合规报告生成（GDPR/等保清单）。
: 备注：复用既有 `access_control.rs`（ABAC 权限）与 `sz-orm-masking`（脱敏规则 `MaskingRule` `:21`）。

**自适应查询优化器（Adaptive Query Optimizer）**
: 运行时统计采集（`QueryStats`，AtomicU64 无锁）→ 自动策略切换（大结果集自动游标分页 / 热点查询自动缓存 / 慢查询降级），复用既有 `cursor_stream.rs`/`paginator.rs`/`l1_cache.rs`/`l2_cache.rs`，仅做决策层不重写实现。
: 备注：既有 `AutoTuningPipeline`（`packages/sz-orm-ai/src/auto_tuning/pipeline.rs:15`）为 AI 离线闭环，本版本补轻量运行时自适应（无 AI 依赖）。

**多数据库融合查询（Multi-Database Fusion Query）**
: 透明多数据库操作（MySQL 主库 + Redis 缓存 + 向量库搜索自动拆分/聚合），`FusionPlanner` 静态分析 WHERE/排序识别可下推子句，`FusionExecutor` 主库 + 缓存/搜索并行执行 + 聚合，CDC 做主库→缓存/搜索索引同步。
: 备注：复用既有 `HybridSearcher`（`packages/sz-orm-vector/src/hybrid_search/searcher.rs:30`）与 `DialectCapturer`（`packages/sz-orm-queue/src/cdc/capturer.rs:12`）；标记为可选/实验，交付 POC 验证价值后决定转正/废弃。

**v4.3.0 feature gate**
: 控制本版本新能力的 feature gate 集合（`explain-analyzer` / `query-flamegraph` / `n1-lint` / `lineage-viz` / `compile-governance` / `adaptive-query` / `db-fusion`），默认关闭，避免无配置环境行为变化。

---

# 3. 角色与边界

## 3.1 核心角色

- **ORM 库维护者**：执行 v4.3.0 五项查询智能与开发者体验能力的开发、验证、测试操作者，是新增能力的主要使用者与验收人。
- **下游项目开发者（sz-pay）**：关注 API 兼容性的下游使用者，v4.3.0 不得破坏其既有代码。
- **应用开发者**：使用编译期 EXPLAIN 警告、N+1 静态检测、查询火焰图在开发期发现查询性能问题，是查询智能能力的主要使用者。
- **DBA / 性能工程师**：使用执行计划回归检测、查询火焰图、血缘影响分析监控查询性能退化与变更影响。
- **数据治理/合规工程师**：使用编译期数据治理标注（PII/脱敏）与合规报告，满足 GDPR/等保合规要求。
- **运维/SRE 工程师**：使用自适应查询优化器监控运行时查询统计，部署多数据库融合查询（实验）。
- **CI/CD 流水线**：消费执行计划回归检测、N+1 批量扫描 JSON 输出、合规报告，集成到 CI 门禁。

## 3.2 外部系统

- **MySQL 9.6 / PostgreSQL 18 / SQLite / Oracle 23ai / MSSQL**：EXPLAIN 解析/数据治理/融合查询的五方言覆盖目标（db-verify 编译期连真 DB 执行 EXPLAIN）。
- **syn 2.0（Rust AST 解析库）**：N+1 静态检测器的函数体 AST 解析依赖。
- `flamegraph.pl`（Brendan Gregg 火焰图工具）：查询火焰图折叠格式的兼容目标。
- **Mermaid / Graphviz（dot）**：数据血缘可视化的导出格式目标。
- **Redis（缓存后端）**：多数据库融合查询的缓存后端（可选）。
- **向量库（pgvector 等）**：多数据库融合查询的搜索后端（复用既有 `sz-orm-vector`）。
- **sz-pay 项目**：API 兼容性验证的下游基准。

## 3.3 交互上下文

```plantuml
@startuml
title sz-orm v4.3.0 五项查询智能与开发者体验能力 交互上下文

rectangle "sz-orm v4.3.0" as orm {
  rectangle "编译期 EXPLAIN 分析\n+ 查询火焰图" as explain
  rectangle "N+1 静态检测器" as n1lint
  rectangle "数据血缘可视化\n+ 编译期数据治理" as governance
  rectangle "自适应查询优化器" as adaptive
  rectangle "多数据库融合查询\n(可选/实验)" as fusion
}

actor "ORM 库维护者" as dev
actor "应用开发者" as appdev
actor "DBA/性能工程师" as dba
actor "数据治理工程师" as goveng
actor "运维/SRE" as sre
actor "sz-pay 开发者" as downstream
actor "CI/CD 流水线" as ci

database "MySQL/PG/SQLite\n/Oracle/MSSQL" as db
rectangle "syn 2.0\n(AST 解析)" as synlib
rectangle "flamegraph.pl" as flamegraph
rectangle "Mermaid/Graphviz" as viz
rectangle "Redis/向量库" as backends

dev --> orm : 开发/验证/测试
appdev --> explain : 编译期 EXPLAIN 警告
explain --> db : db-verify 连真 DB EXPLAIN
appdev --> n1lint : #[detect_n_plus_one] 标注
n1lint --> synlib : syn 解析函数体 AST
dba --> explain : 执行计划回归检测
explain --> flamegraph : Brendan Gregg 格式
dba --> explain : 查询火焰图(SVG)
goveng --> governance : #[pii]/#[mask] 标注
governance --> viz : Mermaid/Graphviz/HTML 血缘
governance --> db : 合规报告(PII 清单)
sre --> adaptive : 运行时统计/自动分页/缓存
adaptive --> db : 自适应查询执行
sre --> fusion : 多数据库融合(实验)
fusion --> backends : 主库+缓存+向量库
fusion --> db : CDC 同步
ci --> explain : 计划回归/火焰图
ci --> n1lint : N+1 批量扫描 JSON
ci --> governance : 合规报告
downstream --> orm : API 兼容性验证

@enduml
```

---

# 4. DFX约束

## 4.1 性能

1. **编译期 EXPLAIN 分析开销**：编译期 EXPLAIN 解析 + 警告生成开销不超过 500ms（含连真 DB EXPLAIN 执行 + 解析 + 警告判定，单查询），不阻断编译（仅 warning）。
2. **查询火焰图采集开销**：查询各阶段计时采集开销不超过 1μs/次（`Instant::now()` 系统调用），各阶段计时总和 = 总耗时（误差 < 1ms）。
3. **N+1 静态检测开销**：单函数 AST 解析 + 模式检测开销不超过 100ms（含 syn 解析 + 3 种模式匹配），批量扫描 1,000 个 .rs 文件不超过 30 秒。
4. **血缘影响分析开销**：`downstream_impact`/`upstream_trace` BFS 遍历开销不超过 50ms（深度 ≤10，节点数 ≤1,000），Mermaid/Graphviz/HTML 导出不超过 200ms。
5. **编译期治理开销**：`#[pii]`/`#[mask]` 标注编译期检查开销不超过 50ms/模型（含标注解析 + 策略白名单校验 + 治理代码生成），合规报告生成不超过 1 秒（模型数 ≤100）。
6. **自适应查询统计采集开销**：`QueryStats::record` 统计采集开销不超过 1μs/次（AtomicU64 原子操作，无锁），决策判定（`should_paginate`/`should_cache`）不超过 100ns。
7. **多数据库融合查询开销**：融合查询拆分 + 聚合开销不超过 100ms（含 `FusionPlanner::plan` 静态分析 + `FusionExecutor::execute` 并行执行 + 结果聚合，主库 + 缓存 + 向量库三源）。

## 4.2 可靠性

1. **EXPLAIN 解析降级**：EXPLAIN 输出格式随 DB 版本变化导致解析失败时，须降级为无警告（不阻断编译），Parser 按方言版本适配，不 panic。
2. **编译期警告不阻断**：编译期 EXPLAIN 警告（全表扫描/缺失索引）须为 warning 非阻断，提供 `SZ_ORM_EXPLAIN_ROW_THRESHOLD` 配置与 `allow` 属性抑制。
3. **N+1 静态检测保守**：N+1 静态检测须保守（仅检测循环体内直接查询调用，不误报复杂控制流），默认 warning 非阻断，`#![allow(n_plus_one)]` 可抑制，与运行时 `N1QueryDetector` 交叉验证一致。
4. **血缘影响分析深度受限**：`downstream_impact`/`upstream_trace` 须 BFS 深度受限（`depth` 参数），不无限遍历，环检测不死循环。
5. **编译期治理不阻断既有代码**：治理检查（PII 未脱敏/非法策略）仅在 `compile-governance` feature 启用时生效，默认关闭，不阻断既有代码编译。
6. **自适应查询不脏读**：自适应缓存须 TTL 可配 + 默认关闭自动缓存（需显式开启），统计决策仅建议不强制，慢查询降级返回明确超时错误不静默。
7. **融合查询拆分安全**：实验阶段仅支持可证明安全的拆分（主键等值 + 缓存键），主库失败回退缓存须返回降级标记，不静默返回脏数据。
8. **v4.2.0 测试基线不回退**：v4.3.0 不得使 v4.2.0 已验收测试基线回退，仅增不减。

## 4.3 安全性

1. **编译期治理强制脱敏**：`#[pii]` 字段必须声明 `#[mask]` 脱敏策略（未声明 → `compile_error!`），策略白名单强制（hash/partial/replace/encrypt，非法策略 → `compile_error!`），运行时输出自动脱敏。
2. **血缘导出不泄露**：血缘可视化导出须尊重既有脱敏规则（`sz-orm-masking`），敏感表/字段名可选脱敏展示，不暴露生产凭据。
3. **N+1 检测不执行查询**：N+1 静态检测仅做 AST 分析，不执行任何查询，不连接数据库，不泄露连接信息。
4. **EXPLAIN 不执行查询**：编译期 EXPLAIN 分析仅执行 EXPLAIN（不实际执行查询），用 NULL 代替参数，不泄露参数值。
5. **融合查询凭据隔离**：多数据库融合查询各后端凭据独立配置，不交叉泄露，CDC 同步不暴露主库凭据到缓存/搜索端。
6. **审计证据要求**：每项需求结论须附 file:line 证据，遵循 AGENTS.md 审计合规铁律。

## 4.4 可维护性

1. **编译期警告可配置**：EXPLAIN 警告须可配置（`SZ_ORM_EXPLAIN_ROW_THRESHOLD` 行数阈值，默认 1000），可抑制（`allow` 属性），不强制干扰开发者。
2. **执行计划基线版本化**：`PlanSnapshot` 基线快照须 JSON 序列化，可版本化管理，CI 中对比基线与当前计划检出回归。
3. **N+1 检测双入口**：N+1 检测须提供函数标注宏（单函数）+ CLI 批量扫描（全工程）双入口，CLI 输出 JSON/table 可被 CI 消费。
4. **血缘导出多格式**：血缘可视化须支持 Mermaid/Graphviz/HTML 多格式导出，可被文档系统/CI 消费。
5. **合规报告可消费**：合规报告须 JSON 输出（PII 字段清单 + 脱敏策略 + 保留策略），可被审计工具消费，报告哈希入审计链。
6. **自适应查询可观测**：自适应查询须输出统计指标（执行次数/平均行数/平均耗时/分页切换/缓存命中），接入既有 Prometheus。
7. **火焰图双格式**：查询火焰图须输出 Brendan Gregg 折叠格式（`flamegraph.pl` 兼容）+ SVG（内联，无外部依赖），可被性能分析工具消费。

## 4.5 兼容性

1. **API 向后兼容**：所有新能力通过 feature gate 隔离，默认 feature 行为不变，既有公开 API 完全向后兼容。
2. **sz-pay 不破坏**：sz-pay 从 crates.io 拉取的 sz-orm-* 6 个包既有用法不受影响。
3. **五方言一致**：新增能力在 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 五方言上行为一致（EXPLAIN 解析/数据治理/融合查询按方言能力适配）。
4. **既有 db-verify 保留**：既有 `SZ_ORM_QUERY_VERIFY` 环境变量（`packages/sz-orm-macros/src/lib.rs:548`）与 EXPLAIN 执行链路（`:642-645`）保留不动，新增 EXPLAIN 解析扩展既有宏。
5. **既有运行时 N+1 检测保留**：既有 `N1QueryDetector`（`packages/sz-orm-core/src/entity_graph.rs:641`）保留不动，新增静态检测为独立包。
6. **既有血缘导出保留**：既有 `export_dot`/`export_json`/`export_graphml`（`packages/sz-orm-audit/src/lineage/export.rs:34/68/105`）保留不动，新增 Mermaid/HTML 导出为扩展。
7. **既有 ABAC/脱敏保留**：既有 `AccessRule`/`AccessContext`/`RowLevelSecurity`（`packages/sz-orm-core/src/access_control.rs:9/22/85`）与 `MaskingRule`（`packages/sz-orm-masking/src/lib.rs:21`）保留不动，新增编译期治理标注为扩展。
8. **既有 AI 优化保留**：既有 `AutoTuningPipeline`（`packages/sz-orm-ai/src/auto_tuning/pipeline.rs:15`）保留不动，新增运行时自适应为独立包。
9. **既有缓存/分页保留**：既有 `cursor_stream.rs`/`paginator.rs`/`l1_cache.rs`/`l2_cache.rs`/`plan_cache.rs` 保留不动，自适应查询仅做决策层复用既有实现。
10. **既有 feature 组合不破坏**：v4.3.0 新增 feature（`explain-analyzer` / `query-flamegraph` / `n1-lint` / `lineage-viz` / `compile-governance` / `adaptive-query` / `db-fusion`）与既有 feature（含 v4.2.0 7 个 feature）任意组合编译通过。

---

# 5. 核心能力

## 5.1 编译期 EXPLAIN 分析 + 查询性能火焰图（REQ-V43-001）

### 5.1.1 业务规则

1. **5 方言 EXPLAIN 解析器**（EARS: Ubiquitous）
   系统应当提供 `sz-orm-explain` 包，将 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 的 EXPLAIN 输出解析为统一 `ExplainPlan`（`scan_type: ScanType` / `table` / `index` / `rows` / `extra`），`ScanType` 枚举（FullTable/IndexRange/IndexScan/UniqueLookup/Other）抽象各方言扫描方式，提供 `pub trait ExplainParser: Send + Sync` 与 `pub fn parser_for(dialect) -> Box<dyn ExplainParser>` 方言分派。
   a. 验收条件：[MySQL EXPLAIN 输出 type=ALL] → [解析为 ScanType::FullTable]；[PostgreSQL EXPLAIN VERBOSE Seq Scan] → [FullTable]；[SQLite EXPLAIN QUERY PLAN SCAN] → [FullTable]；[Oracle TABLE ACCESS FULL] → [FullTable]；[MSSQL Table Scan] → [FullTable]
2. **编译期性能警告**（EARS: Ubiquitous）
   系统应当扩展既有 `query!` 宏（`packages/sz-orm-macros/src/lib.rs:548` db-verify 模式），在编译期连真 DB 执行 EXPLAIN 后调用 `sz-orm-explain` 解析结果，`ScanType::FullTable` → `proc_macro::Span::warning("full table scan detected, consider adding index")`，`index == None` 且 rows 超过阈值（`SZ_ORM_EXPLAIN_ROW_THRESHOLD`，默认 1000）→ 缺失索引警告，警告不阻断编译。
   a. 验收条件：[`SZ_ORM_QUERY_VERIFY=1 DATABASE_URL=mysql://...` 编译含全表扫描查询] → [编译期输出 warning "full table scan detected"，编译成功不阻断]
3. **执行计划回归检测**（EARS: Ubiquitous）
   系统应当提供 `ExplainPlan` 基线快照（`PlanSnapshot`，JSON 序列化）与回归对比（`compare(baseline, current) -> Vec<PlanRegression>`），检出 `ScanTypeUpgrade`（IndexRange→FullTable）/`IndexLost`（索引丢失）/`RowsGrowth`（扫描行数增长），提供 `check_regressions(baseline_path, current) -> Result<Vec<PlanRegression>>` CI 入口。
   a. 验收条件：[基线快照为 IndexRange，当前计划为 FullTable] → [检出 `ScanTypeUpgrade` 回归]；[基线有索引，当前无索引] → [检出 `IndexLost`]
4. **查询性能火焰图采集**（EARS: Ubiquitous）
   系统应当提供 `sz-orm-flamegraph` 包，采集查询各阶段耗时（`QueryPhaseTiming`，`Phase` 枚举 Build/Bind/PoolAcquire/SqlExecute/ResultMap），`QueryTracer::trace_execute` 分阶段计时（`Instant::now()`），各阶段计时总和 = 总耗时（误差 < 1ms）。
   a. 验收条件：[执行查询并采集计时] → [返回各阶段耗时，总和 = 总耗时（误差 < 1ms）]
5. **火焰图双格式输出**（EARS: Ubiquitous）
   系统应当输出 Brendan Gregg 折叠格式（`render::to_brendan_gregg`，`phase;start;duration` 格式，兼容 `flamegraph.pl`）与内联 SVG（`render::to_svg`，无外部依赖），复用既有 `Tracer`（`packages/sz-orm-tracing/src/lib.rs:129`）span 关联（`QueryTracer::with_tracer`）。
   a. 验收条件：[采集计时后调用 `to_brendan_gregg`] → [输出 `flamegraph.pl` 兼容格式]；[调用 `to_svg`] → [输出内联 SVG 含 Build/PoolAcquire/SqlExecute 层]
6. **复用既有 db-verify 链路**（EARS: Ubiquitous）
   系统应当复用既有 `SZ_ORM_QUERY_VERIFY` 环境变量（`packages/sz-orm-macros/src/lib.rs:548`）与各方言 EXPLAIN 语句构造（`:642-645`），新增 EXPLAIN 解析扩展既有宏在 db-verify 模式下调用解析器，不修改既有 EXPLAIN 执行逻辑，默认模式（非 db-verify）零改动。
   a. 验收条件：[默认 `cargo build`（无 db-verify）] → [无 EXPLAIN 解析，行为与 v4.2.0 一致]
7. **复用既有 Tracer**（EARS: Ubiquitous）
   系统应当复用既有 `Tracer` trait（`packages/sz-orm-tracing/src/lib.rs:129`）与 `SzTracer`（`:136`），火焰图阶段耗时写入既有 Tracer span，不重复实现追踪基础设施。
   a. 验收条件：[`QueryTracer::with_tracer` 写入既有 Tracer] → [span 含各阶段耗时，复用既有 Tracer 不重复实现]
8. **解析失败降级**（EARS: Unwanted）
   如果 EXPLAIN 输出格式解析失败（DB 版本变化/非 SQL 输入），则系统应当降级为无警告（不阻断编译），返回 `ExplainError::Unparseable`，不 panic。
   a. 验收条件：[传入非 SQL 输入给解析器] → [返回 `Unparseable`，不 panic，编译期无警告不阻断]
9. **禁止项**（EARS: Unwanted）
   如果编译期 EXPLAIN 分析影响默认 feature 编译或运行时行为，则系统应当通过 `explain-analyzer` feature gate 隔离，默认不启用 EXPLAIN 解析与编译期警告；火焰图通过 `query-flamegraph` feature gate 隔离，默认不启用。
   a. 验收条件：[`cargo build` 默认编译] → [无 EXPLAIN 解析/警告/火焰图，行为与 v4.2.0 一致]

### 5.1.2 交互流程

```plantuml
@startuml
title 编译期 EXPLAIN 分析 + 查询性能火焰图 交互流程
actor "应用开发者" as appdev
participant "query! 宏\n(既有 db-verify)" as macro
participant "sz-orm-explain\n(解析器)" as explain
database "真 DB\n(MySQL/PG/...)" as db
participant "sz-orm-flamegraph\n(火焰图)" as flamegraph
participant "Tracer\n(既有)" as tracer

appdev -> macro : query!("SELECT ...")
macro -> db : SZ_ORM_QUERY_VERIFY=1 连真 DB EXPLAIN
db --> macro : EXPLAIN 原始输出
macro -> explain : parser_for(dialect).parse(raw)
alt 解析成功
  explain --> macro : ExplainPlan
  macro -> macro : 检测 FullTable/缺失索引
  macro --> appdev : 编译期 warning(非阻断)
else 解析失败
  explain --> macro : ExplainError::Unparseable
  macro --> appdev : 降级无警告(不阻断)
end

appdev -> flamegraph : QueryTracer::trace_execute(查询)
flamegraph -> flamegraph : 分阶段计时(Build/Bind/PoolAcquire/SqlExecute/ResultMap)
flamegraph -> tracer : with_tracer 写入 span(复用既有)
flamegraph --> appdev : QueryPhaseTiming 列表
appdev -> flamegraph : to_brendan_gregg / to_svg
flamegraph --> appdev : 折叠格式 / SVG 火焰图
@enduml
```

### 5.1.3 异常场景

1. **EXPLAIN 输出格式不识别**
   a. 触发条件：DB 版本变化导致 EXPLAIN 输出格式与解析器不匹配
   b. 系统行为：降级为无警告（返回 `Unparseable`），不阻断编译，Parser 按方言版本适配
   c. 用户感知：编译期无警告（降级），日志可选记录"EXPLAIN parse failed, degraded to no warning"
2. **编译期警告过度干扰**
   a. 触发条件：全表扫描警告过多干扰开发者
   b. 系统行为：默认 warning 非阻断，提供 `SZ_ORM_EXPLAIN_ROW_THRESHOLD` 配置阈值与 `#![allow(full_table_scan)]` 抑制
   c. 用户感知：可配置阈值/抑制警告，编译不受阻断
3. **执行计划回归检出**
   a. 触发条件：CI 中当前查询计划与基线快照对比发现退化（IndexRange→FullTable）
   b. 系统行为：检出 `ScanTypeUpgrade`/`IndexLost`/`RowsGrowth`，CI 报告标注回归
   c. 用户感知：CI 报告"plan regression detected: ScanTypeUpgrade at query X, IndexRange→FullTable"
4. **火焰图阶段计时误差**
   a. 触发条件：各阶段计时总和与总耗时误差超 1ms
   b. 系统行为：计时基于 `Instant::now()`，误差 < 1ms，测试断言验证
   c. 用户感知：无（测试保证精度）

## 5.2 N+1 静态检测器（REQ-V43-002）

### 5.2.1 业务规则

1. **函数标注宏**（EARS: Ubiquitous）
   系统应当提供 `sz-orm-n1-lint` 包，`#[proc_macro_attribute] pub fn detect_n_plus_one` 函数标注宏，标注函数后用 `syn`（2.0，full feature）解析函数体 AST，检测循环内查询调用模式，生成警告 + 透传原函数。
   a. 验收条件：[`#[detect_n_plus_one] fn foo() { for u in users { orders.find_by_user(u.id) } }`] → [编译期输出 warning "QueryInLoop detected"]
2. **三种检测模式**（EARS: Ubiquitous）
   系统应当实现三种 N+1 检测模式：(a) `QueryInLoop`：`for`/`while` 循环体内出现 `QueryBuilder`/`find_by_*` 调用；(b) `ConditionalQueryInLoop`：循环体内 `if` 分支出现查询调用；(c) `MissingEagerLoadHint`：循环体内出现 `where_in` 可批量替代的单查询（引用既有 `eager_loader.rs`/`smart_eager_loader.rs` 预加载能力）。
   a. 验收条件：[循环内直接查询] → [`QueryInLoop`]；[循环内 if 分支查询] → [`ConditionalQueryInLoop`]；[循环内 where_in 可批量替代] → [`MissingEagerLoadHint`]
3. **CLI 批量扫描**（EARS: Ubiquitous）
   系统应当提供 CLI 命令 `sz-orm n1-lint --path=src --format=table|json`，`batch::scan_dir(path) -> Vec<(String, N1Finding)>` 递归扫描 .rs 文件，用 syn 解析全部函数（不依赖用户标注），JSON 输出可被 CI 消费。
   a. 验收条件：[执行 `sz-orm n1-lint --path=src --format=json`] → [递归扫描全部 .rs 文件，输出 JSON 格式 findings 列表]
4. **静态/运行时交叉验证**（EARS: Ubiquitous）
   系统应当与既有运行时 `N1QueryDetector`（`packages/sz-orm-core/src/entity_graph.rs:641`）交叉验证，同一样例代码静态检测与运行时检测发现一致（静态检测前移到开发期，运行时检测兜底）。
   a. 验收条件：[同一样例代码（循环内查询）] → [静态检测检出 `QueryInLoop`，运行时 `N1QueryDetector` 检出 N+1，结果一致]
5. **复用既有检测知识**（EARS: Ubiquitous）
   系统应当复用既有 `N1QueryDetector`（`packages/sz-orm-core/src/entity_graph.rs:641`）运行时检测知识（查询调用模式/预加载替代），静态检测模式参考运行时检测逻辑，不重复实现检测知识。
   a. 验收条件：[静态检测模式] → [参考既有 N1QueryDetector 检测逻辑，不重复实现]
6. **警告不阻断编译**（EARS: Ubiquitous）
   N+1 静态检测默认输出 `proc_macro::Span::warning`（非阻断），`#![allow(n_plus_one)]` 可抑制，可选 `compile_error!` 模式（严格模式）。
   a. 验收条件：[检测到 N+1 模式] → [默认输出 warning，编译成功不阻断；`#![allow(n_plus_one)]` 抑制后无警告]
7. **禁止项**（EARS: Unwanted）
   如果 N+1 静态检测影响默认 feature 编译或引入不必要的依赖，则系统应当通过 `n1-lint` feature gate 隔离，默认不启用静态检测。
   a. 验收条件：[`cargo build` 默认编译] → [无 N+1 静态检测，行为与 v4.2.0 一致]

### 5.2.2 交互流程

```plantuml
@startuml
title N+1 静态检测器 交互流程
actor "应用开发者" as appdev
participant "#[detect_n_plus_one]\n(标注宏)" as attr
participant "sz-orm-n1-lint\n(AST 分析器)" as lint
rectangle "syn 2.0\n(AST 解析)" as synlib
participant "N1QueryDetector\n(既有运行时)" as runtime
database "DB" as db

alt 函数标注模式
  appdev -> attr : #[detect_n_plus_one] fn foo() {...}
  attr -> synlib : syn::parse2 解析函数体 AST
  synlib --> attr : ItemFn AST
  attr -> lint : analyze_fn(&ast) -> Vec<N1Finding>
  lint -> lint : 检测 QueryInLoop/ConditionalQueryInLoop/MissingEagerLoadHint
  lint --> attr : N1Finding 列表
  attr --> appdev : warning(非阻断) + 透传原函数
else CLI 批量扫描模式
  appdev -> lint : sz-orm n1-lint --path=src --format=json
  lint -> synlib : 递归扫描 .rs 文件 syn 解析
  synlib --> lint : 全部函数 AST
  lint -> lint : analyze_fn 每个函数
  lint --> appdev : JSON findings 列表(CI 消费)
end

note over lint, runtime : 交叉验证：同一样例代码\n静态检测与运行时检测一致
runtime -> db : 运行时检测 N+1(兜底)
@enduml
```

### 5.2.3 异常场景

1. **AST 解析失败**
   a. 触发条件：函数体含 syn 无法解析的语法（如复杂宏展开后的代码）
   b. 系统行为：跳过该函数，告警"AST parse failed, skipped"，不阻断编译
   c. 用户感知：warning"n1-lint: AST parse failed at function X, skipped"
2. **误报（保守检测）**
   a. 触发条件：循环内查询调用实际不是 N+1（如循环内查询但结果不依赖循环变量）
   b. 系统行为：检测模式保守（仅循环体内直接查询调用），复杂控制流不检测，`#![allow(n_plus_one)]` 可抑制
   c. 用户感知：可抑制误报，编译不受阻断
3. **漏报（运行时兜底）**
   a. 触发条件：静态检测未检出的 N+1（如动态构造查询）
   b. 系统行为：运行时 `N1QueryDetector` 兜底检测，静态/运行时交叉验证
   c. 用户感知：运行时告警 N+1，静态检测标注"runtime fallback detected"

## 5.3 数据血缘可视化 + 编译期数据治理（REQ-V43-003）

### 5.3.1 业务规则

1. **血缘可视化导出**（EARS: Ubiquitous）
   系统应当扩展既有 `LineageGraph`（`packages/sz-orm-audit/src/lineage/graph.rs:96`）导出 Mermaid（`to_mermaid`，`graph LR` 格式）、Graphviz（`to_graphviz`，`digraph` 格式）、HTML 报告（`to_html_report`，内联样式无外部依赖）三种格式，复用既有 `export_dot`/`export_json`/`export_graphml`（`export.rs:34/68/105`）。
   a. 验收条件：[users→orders→report 血缘链路] → [Mermaid 含 3 节点 2 边；Graphviz `digraph` 格式；HTML 含内联样式]
2. **血缘影响分析**（EARS: Ubiquitous）
   系统应当提供 `downstream_impact(graph, node, depth) -> Vec<ImpactEdge>`（变更影响范围，BFS 深度受限）与 `upstream_trace(graph, node, depth) -> Vec<ImpactEdge>`（数据来源追溯），`ImpactEdge { from, to, via }`，与迁移 dry-run（`packages/sz-orm-core/src/migration_dry_run.rs:94` `migrate_dry_run`）联动，DROP 前输出受影响链路。
   a. 验收条件：[删除 users 表，`downstream_impact(users, depth=10)`] → [检出 orders/报表服务受影响]；[depth=1] → [只返回直接下游]
3. **PII 字段编译期强制**（EARS: Ubiquitous）
   系统应当在 `sz-orm-core` 增加 `compile-governance` feature，`sz-orm-macros` 新增 `#[proc_macro_derive(Governed, attributes(pii, mask))]`，`#[pii]` 字段必须标注 `#[mask(strategy = "...")]`（未标注 → `compile_error!("PII field must declare mask strategy")`），策略白名单强制（hash/partial/replace/encrypt，非法策略 → `compile_error!`）。
   a. 验收条件：[`#[pii] email` 未标注 `#[mask]`] → [`compile_error!`]；[`#[mask(strategy = "invalid")]`] → [`compile_error!`]
4. **运行时脱敏复用**（EARS: Ubiquitous）
   系统应当生成运行时治理代码（`fn pii_fields() -> Vec<&'static str>` + `fn mask_policy(field) -> Option<MaskPolicy>`），运行时脱敏复用既有 `sz-orm-masking`（`MaskingRule` `packages/sz-orm-masking/src/lib.rs:21`），`Governed` 模型通过既有 `access_control.rs`（`AccessRule` `:9`）检查后 PII 字段输出自动脱敏。
   a. 验收条件：[`Governed` 模型含 `#[pii] #[mask(strategy = "hash")] email`] → [通过 ABAC 检查后，email 输出自动哈希脱敏]
5. **合规报告生成**（EARS: Ubiquitous）
   系统应当生成合规报告（`compliance_report(models) -> ComplianceReport`，含 PII 字段清单 + 脱敏策略 + 保留策略 + 生成时间），JSON 输出可被审计工具消费，报告哈希入既有 `sz-orm-audit` 审计链。
   a. 验收条件：[含 2 个 PII 字段的模型] → [报告列出字段 + 策略，JSON 输出，哈希入审计链]
6. **复用既有血缘/ABAC/脱敏**（EARS: Ubiquitous）
   系统应当复用既有 `LineageGraph`（`graph.rs:96`）、`export_dot`/`export_json`/`export_graphml`（`export.rs:34/68/105`）、`AccessRule`/`AccessContext`/`RowLevelSecurity`（`access_control.rs:9/22/85`）、`MaskingRule`（`masking/lib.rs:21`），不重复实现血缘/权限/脱敏基础设施。
   a. 验收条件：[血缘可视化/编译期治理] → [复用既有 LineageGraph/ABAC/MaskingRule，不重复实现]
7. **治理不阻断既有代码**（EARS: Unwanted）
   如果编译期数据治理影响默认 feature 编译或阻断既有代码，则系统应当通过 `compile-governance` feature gate 隔离，治理检查仅在 feature 启用时生效，默认不阻断既有代码编译。
   a. 验收条件：[`cargo build` 默认编译（无 compile-governance）] → [无 PII/mask 检查，行为与 v4.2.0 一致]
8. **禁止项**（EARS: Unwanted）
   如果数据血缘可视化影响默认 feature 编译或引入不必要的依赖，则系统应当通过 `lineage-viz` feature gate 隔离，默认不启用 Mermaid/Graphviz/HTML 导出与影响分析。
   a. 验收条件：[`cargo build` 默认编译] → [无血缘可视化扩展，行为与 v4.2.0 一致]

### 5.3.2 交互流程

```plantuml
@startuml
title 数据血缘可视化 + 编译期数据治理 交互流程
actor "数据治理工程师" as goveng
actor "DBA" as dba
participant "LineageGraph\n(既有)" as lineage
participant "sz-orm-audit\n(血缘扩展)" as audit
participant "Governed 派生宏\n(新增)" as govmacro
participant "sz-orm-masking\n(既有脱敏)" as masking
participant "access_control\n(既有 ABAC)" as abac
database "DB" as db

alt 血缘可视化
  dba -> audit : to_mermaid/to_graphviz/to_html_report(graph)
  audit -> lineage : 复用既有 LineageGraph
  lineage --> audit : 图结构
  audit --> dba : Mermaid/Graphviz/HTML
  dba -> audit : downstream_impact(users, depth=10)
  audit -> audit : BFS 深度受限遍历
  audit --> dba : Vec<ImpactEdge>(受影响链路)
end

alt 编译期数据治理
  goveng -> govmacro : #[derive(Governed)] #[pii] #[mask(strategy="hash")]
  govmacro -> govmacro : 编译期检查(PII 必须有 mask/策略白名单)
  alt 检查通过
    govmacro -> masking : 生成运行时脱敏代码(复用 MaskingRule)
    govmacro --> goveng : Governed 实现(pii_fields/mask_policy)
    goveng -> abac : 通过 ABAC 检查
    abac -> masking : PII 字段自动脱敏
    masking --> goveng : 脱敏后输出
  else 检查失败
    govmacro --> goveng : compile_error!(PII 未脱敏/非法策略)
  end
end

goveng -> audit : compliance_report(models)
audit --> goveng : ComplianceReport(JSON + 哈希入审计链)
@enduml
```

### 5.3.3 异常场景

1. **PII 字段未声明脱敏策略**
   a. 触发条件：`#[pii]` 字段未标注 `#[mask]`
   b. 系统行为：`compile_error!("PII field must declare mask strategy")`，阻断编译（仅 `compile-governance` feature 启用时）
   c. 用户感知：编译错误"PII field 'email' must declare mask strategy"
2. **非法脱敏策略**
   a. 触发条件：`#[mask(strategy = "invalid")]` 策略不在白名单（hash/partial/replace/encrypt）
   b. 系统行为：`compile_error!`，阻断编译
   c. 用户感知：编译错误"invalid mask strategy 'invalid', allowed: hash/partial/replace/encrypt"
3. **血缘影响分析环检测**
   a. 触发条件：血缘图存在环（A→B→A）
   b. 系统行为：BFS 深度受限，环检测已访问节点不重复遍历，不死循环
   c. 用户感知：影响分析正常返回，不超时
4. **合规报告审计链写入失败**
   a. 触发条件：审计链不可用或哈希写入失败
   b. 系统行为：告警，报告仍生成但标注"audit chain write failed"
   c. 用户感知：告警"compliance report generated, audit chain write failed, manual review required"

## 5.4 自适应查询优化器（REQ-V43-004）

### 5.4.1 业务规则

1. **运行时统计采集**（EARS: Ubiquitous）
   系统应当提供 `sz-orm-adaptive` 包，`QueryStats`（`total_executions`/`total_rows`/`total_time_us`，AtomicU64 无锁）+ `record(rows, time_us)` 采集，`should_paginate(threshold_rows) -> bool`（avg_rows > threshold，默认 1000）+ `should_cache(threshold_ms, min_executions) -> bool`（avg_time > threshold 且执行次数达标）决策，统计采集开销 < 1μs/次。
   a. 验收条件：[查询执行 100 次平均行数 2000] → [`should_paginate(1000)` 返回 true]；[平均耗时 50ms 执行 10 次] → [`should_cache(30, 5)` 返回 true]
2. **自适应策略执行器**（EARS: Ubiquitous）
   系统应当提供 `AdaptiveExecutor`（`stats: HashMap<String, QueryStats>` + `config: AdaptiveConfig`），`execute(query_key, f) -> QueryOutcome` 记录统计 → 按决策选择执行路径，自动游标分页（`should_paginate` 为真时切换到既有 `cursor_stream.rs`，返回分页句柄）、自动缓存（`should_cache` 为真时结果写入既有 `l2_cache.rs`，TTL 可配）、慢查询降级（超时返回明确错误不静默）。
   a. 验收条件：[大结果集查询（avg_rows > 1000）] → [自动切换游标分页，返回分页句柄]；[热点查询（avg_time > 30ms）] → [自动缓存，命中返回缓存结果]
3. **复用既有缓存/分页**（EARS: Ubiquitous）
   系统应当复用既有 `cursor_stream.rs`（游标分页）、`paginator.rs`（分页器）、`l1_cache.rs`（L1 缓存）、`l2_cache.rs`（L2 缓存）、`plan_cache.rs`（计划缓存），自适应查询仅做决策层，不重写缓存/分页实现。
   a. 验收条件：[自适应分页/缓存] → [复用既有 cursor_stream/l2_cache，不重复实现]
4. **慢查询降级**（EARS: Event-driven）
   当单次查询执行超时（`timeout_ms` 可配）时，系统应当返回明确超时错误，不静默丢查询，不无限重试。
   a. 验收条件：[查询执行超时（> timeout_ms）] → [返回明确超时错误，不静默]
5. **缓存不脏读**（EARS: Unwanted）
   如果自适应缓存可能导致脏读，则系统应当 TTL 可配 + 默认关闭自动缓存（需显式开启），统计决策仅建议不强制。
   a. 验收条件：[默认配置] → [自动缓存关闭，无脏读风险；显式开启后 TTL 可配]
6. **统计不重复累加**（EARS: Ubiquitous）
   缓存命中返回缓存结果时，统计不重复累加执行时间（仅记录缓存命中次数），避免统计失真。
   a. 验收条件：[缓存命中返回缓存结果] → [执行时间不重复累加，仅记录命中次数]
7. **禁止项**（EARS: Unwanted）
   如果自适应查询影响默认 feature 编译或运行时行为，则系统应当通过 `adaptive-query` feature gate 隔离，默认不启用自适应查询。
   a. 验收条件：[`cargo build` 默认编译] → [无自适应查询，行为与 v4.2.0 一致]

### 5.4.2 交互流程

```plantuml
@startuml
title 自适应查询优化器 交互流程
actor "运维/SRE" as sre
participant "AdaptiveExecutor" as executor
participant "QueryStats\n(统计采集)" as stats
participant "cursor_stream\n(既有游标分页)" as cursor
participant "l2_cache\n(既有 L2 缓存)" as cache
database "DB" as db

sre -> executor : execute("query_key", || query.run())
executor -> stats : record(rows, time_us)(AtomicU64 无锁)
executor -> stats : should_paginate(1000)? / should_cache(30, 5)?

alt 大结果集(avg_rows > 1000)
  executor -> cursor : 切换游标分页(复用既有)
  cursor -> db : 游标分页查询
  db --> cursor : 分页结果
  cursor --> executor : 分页句柄
else 热点查询(avg_time > 30ms)
  executor -> cache : 查 L2 缓存(复用既有)
  alt 缓存命中
    cache --> executor : 缓存结果(统计不重复累加)
  else 缓存未命中
    executor -> db : 执行查询
    db --> executor : 结果
    executor -> cache : 写入 L2 缓存(TTL 可配)
  end
else 普通查询
  executor -> db : 直接执行
  db --> executor : 结果
end

alt 慢查询超时
  executor --> sre : 明确超时错误(不静默)
else 正常
  executor --> sre : QueryOutcome
end
@enduml
```

### 5.4.3 异常场景

1. **慢查询超时**
   a. 触发条件：单次查询执行超过 `timeout_ms`
   b. 系统行为：返回明确超时错误，不静默丢查询，不无限重试
   c. 用户感知：错误"adaptive query timeout, exceeded {timeout_ms}ms"
2. **缓存脏读风险**
   a. 触发条件：自动缓存启用但 TTL 过长导致脏读
   b. 系统行为：默认关闭自动缓存（需显式开启），TTL 可配，统计决策仅建议不强制
   c. 用户感知：默认无脏读风险，显式开启后 TTL 可配置
3. **统计失真**
   a. 触发条件：缓存命中时执行时间重复累加导致平均耗时失真
   b. 系统行为：缓存命中不重复累加执行时间，仅记录命中次数
   c. 用户感知：统计准确，决策不受失真影响

## 5.5 多数据库融合查询（REQ-V43-005）

### 5.5.1 业务规则

1. **融合配置**（EARS: Optional）
   当启用 `db-fusion` feature 时，系统应当提供 `sz-orm-fusion` 包，`FusionConfig { primary: Dialect, cache: Option<CacheBackend>, search: Option<SearchBackend> }`（`CacheBackend::Redis` / `SearchBackend::Vector`，复用既有 `HybridSearcher` `packages/sz-orm-vector/src/hybrid_search/searcher.rs:30`）。
   a. 验收条件：[配置 `FusionConfig { primary: MySql, cache: Redis, search: Vector }`] → [融合查询器按配置连接主库 + Redis + 向量库]
2. **查询拆分与聚合**（EARS: Ubiquitous）
   系统应当提供 `FusionPlanner::plan(query) -> FusionPlan`（静态分析 WHERE/排序，识别可下推缓存/搜索子句）与 `FusionExecutor::execute(plan) -> Result<Vec<Row>>`（主库查询 + 缓存/搜索并行执行 + 聚合），实验阶段仅支持可证明安全的拆分（主键等值 + 缓存键）。
   a. 验收条件：[`where_eq` + 缓存键命中] → [主库跳过，返回缓存结果]；[主库失败回退缓存] → [返回缓存结果 + 降级标记]
3. **CDC 数据同步**（EARS: Ubiquitous）
   系统应当复用既有 CDC（`packages/sz-orm-queue/src/cdc/capturer.rs:12` `DialectCapturer`，各方言 WAL/Binlog/Trigger/LogMiner）做主库→缓存/搜索索引同步，不重复实现 CDC 捕获。
   a. 验收条件：[主库数据变更] → [CDC 捕获变更，同步到缓存/搜索索引]
4. **主库失败回退**（EARS: Event-driven）
   当主库查询失败时，系统应当回退缓存（缓存可读），返回缓存结果 + 降级标记，不静默返回脏数据。
   a. 验收条件：[主库不可用，缓存可读] → [返回缓存结果 + 降级标记]
5. **实验评估报告**（EARS: Ubiquitous）
   系统应当编写《db-fusion 实验评估报告》（POC 结果 + 价值判断），给出转正/废弃建议，评估后决定是否转正。
   a. 验收条件：[POC 运行完成] → [评估报告含 POC 结果 + 转正/废弃建议]
6. **禁止项**（EARS: Unwanted）
   如果多数据库融合查询影响默认 feature 编译或引入不必要的依赖，则系统应当通过 `db-fusion` feature gate 隔离，默认不启用融合查询（标记为可选/实验）。
   a. 验收条件：[`cargo build` 默认编译] → [无融合查询，行为与 v4.2.0 一致]

### 5.5.2 交互流程

```plantuml
@startuml
title 多数据库融合查询 交互流程
actor "运维/SRE" as sre
participant "FusionPlanner" as planner
participant "FusionExecutor" as executor
database "主库\n(MySQL)" as primary
rectangle "Redis\n(缓存)" as cache
rectangle "向量库\n(搜索)" as vector
participant "DialectCapturer\n(既有 CDC)" as cdc

sre -> planner : plan(query)
planner -> planner : 静态分析 WHERE/排序
planner --> executor : FusionPlan(可下推子句)

executor -> executor : 并行执行
par 主库查询
  executor -> primary : 主库查询
  primary --> executor : 主库结果
else 缓存查询
  executor -> cache : 缓存键查询
  cache --> executor : 缓存结果
else 向量搜索
  executor -> vector : 搜索查询(复用 HybridSearcher)
  vector --> executor : 搜索结果
end

alt 全部成功
  executor -> executor : 聚合结果
  executor --> sre : Vec<Row>
else 主库失败
  executor -> cache : 回退缓存
  cache --> executor : 缓存结果
  executor --> sre : 缓存结果 + 降级标记
end

note over cdc : CDC 同步：主库变更 → 缓存/搜索索引
primary -> cdc : WAL/Binlog 变更
cdc -> cache : 同步缓存索引
cdc -> vector : 同步搜索索引
@enduml
```

### 5.5.3 异常场景

1. **主库失败回退**
   a. 触发条件：主库不可用（网络/宕机），缓存可读
   b. 系统行为：回退缓存，返回缓存结果 + 降级标记，不静默返回脏数据
   c. 用户感知：结果标注"degraded: primary unavailable, cache fallback"
2. **拆分语义不透明**
   a. 触发条件：查询拆分无法证明安全（非主键等值/非缓存键）
   b. 系统行为：实验阶段仅支持可证明安全的拆分，不安全拆分回退全主库查询
   c. 用户感知：查询回退全主库，日志标注"fusion split unsafe, fallback to primary only"
3. **CDC 同步延迟**
   a. 触发条件：CDC 同步延迟导致缓存/搜索索引过期
   b. 系统行为：缓存 TTL 可配，延迟期间回退主库，评估报告标注同步延迟影响
   c. 用户感知：缓存 TTL 内可能读到旧数据，TTL 过期回退主库
4. **实验废弃**
   a. 触发条件：POC 评估价值不足，决定废弃
   b. 系统行为：评估报告给出废弃建议，feature gate 保留但标记 deprecated
   c. 用户感知：评估报告"db-fusion deprecated, value insufficient, recommend removal in v4.x+"

---

# 6. 数据约束

## 6.1 需求项

1. **需求 ID**：唯一标识，格式 `REQ-V43-xxx`（xxx = 001~005），必填。
2. **需求名称**：人类可读名称，必填。
3. **优先级**：P1 / P2 / P3，必填。
4. **分类**：查询智能 / N+1 检测 / 数据治理 / 自适应优化 / 融合查询，必填。
5. **EARS 分类**：Ubiquitous / Event-driven / State-driven / Optional / Unwanted，每条业务规则必填。
6. **验证方法**：可执行的验证命令或测试描述，必填。
7. **代码证据**：相关 file:line 引用，必填，遵循审计合规铁律。
8. **验收条件**：触发场景 → 预期行为，必填。
9. **状态**：PASS / FAIL / PENDING，必填。
10. **与 v4.2.0 兼容性**：feature gate 隔离 / 既有 API 保留 / 测试基线不回退，必填。

## 6.2 输出对象

1. **ExplainPlan**：执行计划统一结构（scan_type/table/index/rows/extra，5 方言解析结果）。
2. **PlanSnapshot**：执行计划基线快照（query_key/plan/captured_at，JSON 序列化）。
3. **PlanRegression**：执行计划回归（ScanTypeUpgrade/IndexLost/RowsGrowth）。
4. **QueryPhaseTiming**：查询阶段计时（phase/start_ms/duration_ms，Build/Bind/PoolAcquire/SqlExecute/ResultMap）。
5. **FlamegraphOutput**：火焰图输出（Brendan Gregg 折叠格式 / SVG）。
6. **N1Finding**：N+1 检测发现（span_line/pattern/message，QueryInLoop/ConditionalQueryInLoop/MissingEagerLoadHint）。
7. **ImpactEdge**：血缘影响边（from/to/via，downstream_impact/upstream_trace 结果）。
8. **LineageVizExport**：血缘可视化导出（Mermaid/Graphviz/HTML）。
9. **ComplianceReport**：合规报告（pii_fields/retention_days/generated_at，JSON 输出）。
10. **QueryStats**：查询统计（total_executions/total_rows/total_time_us，AtomicU64 无锁）。
11. **AdaptiveConfig**：自适应配置（row_threshold/cache_time_ms/min_executions）。
12. **QueryOutcome**：自适应查询结果（结果/分页句柄/缓存标记/降级标记）。
13. **FusionConfig**：融合配置（primary/cache/search）。
14. **FusionPlan**：融合查询计划（可下推子句/主库子句）。

---

# 7. 需求追溯矩阵

| 需求编号 | 需求项 | 优先级 | 分类 | EARS 分类 | 验收条件（节选） | 现有代码证据 | 与 v4.2.0 兼容性 |
|---------|--------|--------|------|----------|----------------|-------------|----------------|
| REQ-V43-001 | 编译期 EXPLAIN 分析 + 查询性能火焰图 | P1 | 查询智能 | Ubiquitous/Unwanted | 5 方言 EXPLAIN 解析 + 编译期警告 + 计划回归 + 火焰图（Brendan Gregg + SVG） | `packages/sz-orm-macros/src/lib.rs:548` SZ_ORM_QUERY_VERIFY、`:298` validate_sql_content、`:642` 各方言 EXPLAIN 构造、`packages/sz-orm-tracing/src/lib.rs:129` Tracer、`:136` SzTracer、`packages/sz-orm-core/src/query.rs:36` QueryBuilder、`packages/sz-orm-core/src/pool.rs:45` Connection | `explain-analyzer`/`query-flamegraph` feature gate，既有 db-verify EXPLAIN 执行保留 |
| REQ-V43-002 | N+1 静态检测器 | P1 | N+1 检测 | Ubiquitous/Unwanted | 函数标注宏 + 3 种检测模式 + CLI 批量扫描 + 静态/运行时交叉验证 | `packages/sz-orm-core/src/entity_graph.rs:641` N1QueryDetector、`packages/sz-orm-core/src/eager_loader.rs` 预加载、`packages/sz-orm-core/src/smart_eager_loader.rs` 智能预加载 | `n1-lint` feature gate，既有运行时 N1QueryDetector 保留 |
| REQ-V43-003 | 数据血缘可视化 + 编译期数据治理 | P2 | 数据治理 | Ubiquitous/Unwanted | Mermaid/Graphviz/HTML 血缘 + 影响分析 + PII 编译期强制 + 合规报告 | `packages/sz-orm-audit/src/lineage/graph.rs:96` LineageGraph、`packages/sz-orm-audit/src/lineage/export.rs:34` export_dot、`:68` export_json、`:105` export_graphml、`packages/sz-orm-core/src/access_control.rs:9` AccessRule、`:22` AccessContext、`:85` RowLevelSecurity、`packages/sz-orm-masking/src/lib.rs:21` MaskingRule、`packages/sz-orm-core/src/migration_dry_run.rs:94` migrate_dry_run | `lineage-viz`/`compile-governance` feature gate，既有血缘导出/ABAC/脱敏保留 |
| REQ-V43-004 | 自适应查询优化器 | P2 | 自适应优化 | Ubiquitous/Event-driven/Unwanted | 统计采集 <1μs + 自动分页/缓存决策 + 慢查询降级 + 复用既有实现 | `packages/sz-orm-ai/src/auto_tuning/pipeline.rs:15` AutoTuningPipeline、`packages/sz-orm-core/src/cursor_stream.rs` 游标分页、`packages/sz-orm-core/src/paginator.rs` 分页器、`packages/sz-orm-core/src/l1_cache.rs` L1 缓存、`packages/sz-orm-core/src/l2_cache.rs` L2 缓存、`packages/sz-orm-core/src/plan_cache.rs` 计划缓存 | `adaptive-query` feature gate，既有 AI 优化/缓存/分页保留 |
| REQ-V43-005 | 多数据库融合查询（可选/实验） | P3 | 融合查询 | Ubiquitous/Optional/Unwanted | 融合配置 + 拆分/聚合 + CDC 同步 + 主库失败回退 + 实验评估报告 | `packages/sz-orm-core/src/db_type.rs:11` DbType（28 方言）、`packages/sz-orm-vector/src/hybrid_search/searcher.rs:30` HybridSearcher、`packages/sz-orm-queue/src/cdc/capturer.rs:12` DialectCapturer | `db-fusion` feature gate，可选/实验，既有方言/向量搜索/CDC 保留 |

---

# 8. 验收标准总览

## 8.1 P1 类（最高优先级）

| 编号 | 验收标准 | 验证方法 |
|------|---------|---------|
| REQ-V43-001 | 5 方言 EXPLAIN 解析（FullTable/IndexRange/IndexScan/UniqueLookup）+ 编译期警告（全表扫描/缺失索引，非阻断）+ 计划回归检测（ScanTypeUpgrade/IndexLost/RowsGrowth）+ 火焰图（Brendan Gregg + SVG，各阶段计时误差 < 1ms）+ 复用既有 db-verify/Tracer | 5 方言真实 EXPLAIN 样例解析验证 ScanType；`SZ_ORM_QUERY_VERIFY=1` 连真库编译验证全表扫描 warning；基线快照→回归检出验证；火焰图各阶段计时总和验证；`flamegraph.pl` 兼容性验证 |
| REQ-V43-002 | 函数标注宏 + 3 种检测模式（QueryInLoop/ConditionalQueryInLoop/MissingEagerLoadHint）+ CLI 批量扫描（JSON/table）+ 静态/运行时交叉验证一致 + 默认 warning 非阻断 | 标注函数循环内查询验证检出；CLI 批量扫描样例工程验证；静态/运行时同一样例交叉验证；`#![allow(n_plus_one)]` 抑制验证 |

## 8.2 P2 类（高优先级）

| 编号 | 验收标准 | 验证方法 |
|------|---------|---------|
| REQ-V43-003 | Mermaid/Graphviz/HTML 血缘导出 + 影响分析（downstream/upstream，BFS 深度受限）+ PII 编译期强制（缺 mask/非法策略 → compile_error）+ 运行时脱敏复用 sz-orm-masking + 合规报告（JSON + 审计链）+ 复用既有 LineageGraph/ABAC/脱敏 | 血缘链路导出验证 3 格式；影响分析验证深度受限与环检测；PII 未脱敏验证 compile_error；Governed 模型验证自动脱敏；合规报告验证 JSON + 审计链 |
| REQ-V43-004 | 统计采集 < 1μs/次（AtomicU64 无锁）+ 自动游标分页/缓存决策 + 慢查询降级（明确错误不静默）+ 缓存不脏读（TTL 可配 + 默认关闭）+ 统计不重复累加 + 复用既有 cursor_stream/l2_cache | 统计采集性能测试验证 < 1μs；大结果集验证自动分页；热点查询验证自动缓存；慢查询验证超时错误；缓存命中验证统计不重复累加 |

## 8.3 P3 类（中优先级）

| 编号 | 验收标准 | 验证方法 |
|------|---------|---------|
| REQ-V43-005 | 融合配置 + 查询拆分/聚合（仅安全拆分）+ CDC 同步 + 主库失败回退（降级标记）+ 实验评估报告（转正/废弃建议）+ 复用既有 HybridSearcher/CDC | POC 运行验证拆分/聚合；主库失败验证缓存回退；CDC 同步验证；评估报告验证转正/废弃建议 |

## 8.4 全局验收条件

1. **API 兼容性**：v4.2.0 既有公开 API 完全向后兼容，sz-pay 既有代码不受影响。
2. **feature gate 隔离**：所有新能力通过 feature gate 隔离（`explain-analyzer` / `query-flamegraph` / `n1-lint` / `lineage-viz` / `compile-governance` / `adaptive-query` / `db-fusion`），默认 feature 行为不变。
3. **测试基线不回退**：v4.2.0 已验收测试基线不回退，v4.3.0 仅增不减。
4. **五方言一致**：新增能力在 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 五方言上行为一致（EXPLAIN 解析/数据治理/融合查询按方言能力适配）。
5. **审计证据**：每项需求结论附 file:line 证据，遵循 AGENTS.md 审计合规铁律。
6. **14 道门禁通过**：v4.3.0 须通过 AGENTS.md 定义的 14 道门禁（fmt/check/clippy/test/doc/audit/integration/占位检查/SQL 注入/feature 全组合/上游未改/文档一致/审计证据/文档同步）。
7. **无占位实现**：禁止 `todo!` / `unimplemented!` / `unreachable!`，所有新增代码须完整实现。
8. **unsafe 零容忍**：所有新增代码无 `unsafe` 块，或必须有 `// SAFETY:` 注释。
9. **复用优先**：优先复用既有能力，不重复实现（如 EXPLAIN 解析复用既有 db-verify EXPLAIN 执行链路；火焰图复用既有 Tracer/SzTracer；N+1 检测复用既有 N1QueryDetector 检测知识；血缘复用既有 LineageGraph/export_dot/export_json/export_graphml；治理复用既有 access_control.rs/sz-orm-masking；自适应复用既有 cursor_stream.rs/paginator.rs/l1_cache.rs/l2_cache.rs/plan_cache.rs；融合复用既有 HybridSearcher/DialectCapturer）。
10. **无 Breaking Change**：新能力通过 feature gate 隔离，默认全关闭，既有 feature 组合行为不变。
11. **与 v4.2.0 零重叠**：v4.3.0 新增范围全部落在新包或 v4.2.0 不触碰的既有包（sz-orm-explain/sz-orm-flamegraph/sz-orm-n1-lint/sz-orm-adaptive/sz-orm-fusion 新包 + sz-orm-macros db-verify 扩展 + sz-orm-audit lineage-viz 扩展 + sz-orm-core compile-governance 扩展），唯一同文件风险 `cli/src/main.rs` 通过排期规避（M2 在 v4.2.0 交付后启动）。
12. **依赖关系**：REQ-V43-001（EXPLAIN + 火焰图）复用既有 `sz-orm-macros` db-verify + `sz-orm-tracing`；REQ-V43-002（N+1 静态检测）复用既有 `sz-orm-core/entity_graph` + `eager_loader`；REQ-V43-003（血缘 + 治理）复用既有 `sz-orm-audit/lineage` + `sz-orm-core/access_control` + `sz-orm-masking`；REQ-V43-004（自适应查询）复用既有 `sz-orm-core` cursor_stream/paginator/l1_cache/l2_cache；REQ-V43-005（融合查询）复用既有 `sz-orm-vector` + `sz-orm-queue/cdc`；五项需求主体相互独立，M1/M3 可与 v4.2.0 并行，M2/M4/M5 在 v4.2.0 交付后启动。

## 8.5 需求依赖关系

```plantuml
@startuml
title v4.3.0 需求依赖关系图
REQ-V43-001 "编译期 EXPLAIN 分析 + 火焰图" : 复用 sz-orm-macros db-verify + sz-orm-tracing
REQ-V43-002 "N+1 静态检测器" : 复用 sz-orm-core/entity_graph + eager_loader
REQ-V43-003 "血缘可视化 + 编译期治理" : 复用 sz-orm-audit/lineage + access_control + sz-orm-masking
REQ-V43-004 "自适应查询优化器" : 复用 sz-orm-core cursor_stream/paginator/l1/l2_cache
REQ-V43-005 "多数据库融合查询" : 复用 sz-orm-vector + sz-orm-queue/cdc

REQ-V43-001 ..> REQ-V43-002 : EXPLAIN 警告与 N+1 检测协同(查询智能互补,可选)
REQ-V43-003 ..> REQ-V43-004 : 治理标注指导自适应策略(PII 字段不缓存,可选协同)
@enduml
```

> 说明：五项需求主体相互独立，可并行开发。REQ-V43-001 与 REQ-V43-002 存在可选协同（EXPLAIN 警告与 N+1 检测同为查询智能互补，但非强依赖）；REQ-V43-003 与 REQ-V43-004 存在可选协同（治理标注可指导自适应策略，如 PII 字段不缓存，但非强依赖）。M1/M3 可与 v4.2.0 并行（新包或 v4.2.0 不触碰的包），M2/M4/M5 在 v4.2.0 交付后启动（M2 与 v4.2.0 同改 `cli/src/main.rs`；M4/M5 依赖 v4.2.0 后 sz-orm-core 最终形态）。