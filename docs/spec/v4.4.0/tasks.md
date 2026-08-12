# sz-orm v4.4.0 编码任务规划

> 版本：v4.4.0（查询自动优化建议 + 慢查询自动诊断报告 + db-fusion 转正 + 结构化查询日志 + 性能回归基准线 + 查询智能闭环联动）
> 基线：v4.3.0（编译期 EXPLAIN 分析 + 查询性能火焰图 + N+1 静态检测 + 数据血缘可视化 + 编译期数据治理 + 自适应查询 + 多数据库融合 POC，5 项需求 REQ-V43-001~005 全部通过 feature gate 隔离，26 任务 / 156 子任务全部已完成并提交，约 7,000 个测试通过，14 道门禁全通过）
> 日期：2026-08-12
> 文档定位：编码任务规划（How to execute），对应需求规格 `spec.md`（What to build，870 行）+ 技术设计 `design.md`（How to build，1801 行）
> 任务约束：无 Breaking Change（6 个新 feature gate 隔离，默认全关闭）+ 优先复用既有能力 + 五方言覆盖 + 每项任务附 file:line 代码证据 + unsafe 零容忍 + 禁止占位实现（todo!/unimplemented!/unreachable!）
> 审计合规铁律：每项任务结论须附真实存在的 file:line 证据，修复后必须运行 `cargo test` 并附输出，禁止未验证即标记 ✅
> 实施顺序：按 design.md §2.1.3 依赖关系，M0（P0 文档基线，立即并行）→ M1 P1（优化建议引擎，查询智能核心）→ M2 P1（慢查询诊断，依赖 M1 建议联动）→ M3 P1（db-fusion 转正，独立）→ M4 P2（结构化日志，独立）→ M5 P2（性能基线，独立）→ M6 P3（闭环联动，依赖 M1+M2）→ M7 最终验证
> 与 v4.3.0 零重叠：v4.3.0 是"采集/检测"层（EXPLAIN 解析/火焰图采集/N+1 检测/血缘导出/治理标注/自适应决策/融合 POC），v4.4.0 是"分析/建议/转正"层（优化建议/诊断报告/融合转正/结构化日志/性能基线/闭环联动），新增范围全部落在新包（sz-orm-advisor/sz-orm-diagnosis）或 v4.3.0 不触碰的既有包扩展（sz-orm-fusion db-fusion-v2 扩展 / sz-orm-observability query-logging 扩展 / sz-orm-explain perf-baseline 扩展 / sz-orm-advisor query-intelligence-loop 扩展）

---

# 一、任务总览

## 1.1 里程碑 × 任务数 × 预期工作量

| 里程碑 | 名称 | 对应需求 | 优先级 | 任务数 | 子任务数 | 预期工作量 | 启动时机 |
|--------|------|---------|--------|--------|----------|-----------|---------|
| M0 | 文档基线与准备 | — | P0 | 3 | 12 | 0.5 周 | 立即（v4.3.0 已完成） |
| M1 | 查询自动优化建议引擎 | REQ-V44-001 | P1 | 6 | 32 | 2 周 | 立即（新包，独立） |
| M2 | 慢查询自动诊断报告 | REQ-V44-002 | P1 | 5 | 26 | 1.5 周 | M1 交付后（建议联动） |
| M3 | db-fusion 转正 | REQ-V44-003 | P1 | 7 | 38 | 3 周 | 立即（独立扩展） |
| M4 | 结构化查询日志 | REQ-V44-004 | P2 | 5 | 22 | 1.5 周 | 立即（独立扩展） |
| M5 | 性能回归基准线 + CI 自动比对 | REQ-V44-005 | P2 | 4 | 20 | 1.5 周 | 立即（独立扩展） |
| M6 | 查询智能闭环联动 | REQ-V44-006 | P3 | 4 | 18 | 1 周 | M1+M2 交付后 |
| M7 | 最终验证与文档同步 | 全局 | — | 3 | 16 | 0.5 周 | 全部完成后 |
| **合计** | — | **6 项全覆盖** | — | **37** | **184** | **11.5 周** | — |

## 1.2 任务编号约定

- 主任务：`M{里程碑号}-T{任务序号}`（如 M1-T1）
- 子任务：`M{里程碑号}-T{任务序号}.{子任务序号}`（如 M1-T2.1）
- 集成验证任务：每个里程碑末尾固定一个集成测试与门禁验证任务（如 M1-T6）
- 里程碑内需求按 REQ-V44-xxx 序号顺序编排任务

## 1.3 全局约束（适用于所有任务）

1. **feature gate 隔离**：6 个新 feature（`query-advisor` / `slow-query-diagnosis` / `db-fusion-v2` / `query-logging` / `perf-baseline` / `query-intelligence-loop`），默认全关闭，默认 feature 行为不变
2. **既有 API 不变**：既有公开 API 签名完全向后兼容，sz-pay 既有代码不受影响（sz-pay 从 crates.io 拉取 sz-orm-* 6 个包）
3. **禁止占位实现**：禁止 `todo!`/`unimplemented!`/`unreachable!`
4. **unsafe 零容忍**：所有新增代码无 `unsafe` 块，或必须有 `// SAFETY:` 注释
5. **五方言覆盖**：MySQL/PostgreSQL/SQLite/Oracle/MSSQL（优化建议/诊断报告/融合查询按方言能力适配）
6. **审计证据**：每项任务结论附真实存在的 file:line 证据
7. **测试基线不回退**：v4.3.0 已验收测试基线（约 7,000 个测试）不回退，v4.4.0 仅增不减
8. **复用优先**：优先复用既有能力，不重复实现（优化建议复用 `ExplainPlan` `explain/lib.rs:76` / `QueryStats` `adaptive/stats.rs:11` / AI 建议结构 `ai/index_advisor.rs:71`；诊断复用 `QueryPhaseTiming` `flamegraph/collector.rs:39` / `QueryOutcome.slow` `adaptive/executor.rs:116`；融合转正复用 `InvalidationBus` `core/l2_cache.rs:82` / `DialectCapturer` `queue/cdc/capturer.rs:12` / `HybridSearcher` `vector/searcher.rs:30`；日志复用 `MetricsRegistry` `observability/lib.rs:253` / `MaskingRule` `masking/lib.rs:21`；性能基线复用 `PlanSnapshot` `explain/regression.rs:23` / `check_regressions` `:161`；闭环复用 `decide` `adaptive/executor.rs:157` / `trace_execute` `flamegraph/collector.rs:61`）
9. **Windows MSVC 编译环境**：RUST_MIN_STACK=134217728, CARGO_INCREMENTAL=0
10. **测试命令**：`cargo test --workspace -j 2 --no-fail-fast`；feature 包测试：`cargo test -p <package> --features <feature>`

## 1.4 里程碑依赖关系

```
M0（P0，文档基线，立即并行）
M1（P1，优化建议引擎，查询智能核心，独立新包）
  - REQ-V44-001 复用既有 sz-orm-explain + sz-orm-adaptive + sz-orm-ai
M2（P1，慢查询诊断，M1 交付后）
  - REQ-V44-002 复用既有 sz-orm-flamegraph + sz-orm-adaptive
  - 依赖 M1 sz-orm-advisor 建议联动
M3（P1，db-fusion 转正，独立扩展）
  - REQ-V44-003 复用既有 sz-orm-fusion POC + sz-orm-core + sz-orm-queue + sz-orm-vector
M4（P2，结构化日志，独立扩展）
  - REQ-V44-004 复用既有 sz-orm-observability + sz-orm-flamegraph + sz-orm-masking
M5（P2，性能基线，独立扩展）
  - REQ-V44-005 复用既有 sz-orm-explain + sz-orm-flamegraph
M6（P3，闭环联动，M1+M2 交付后）
  - REQ-V44-006 复用既有 sz-orm-explain + sz-orm-adaptive + sz-orm-flamegraph
  - 依赖 M1 sz-orm-advisor suggest + M2 sz-orm-diagnosis diagnose
M7（最终验证，全部完成后）
  - 依赖 M0~M6 全部完成
```

> **依赖关系说明**：M0/M1/M3/M4/M5 可立即启动（新包或独立扩展）；M2 在 M1 交付后启动（建议联动依赖 sz-orm-advisor）；M6 在 M1+M2 交付后启动（闭环第四步建议 + 第三步诊断）；M7 必须最后执行。六项需求主体相互独立，REQ-V44-002 与 REQ-V44-001 存在强协同（诊断→建议联动），REQ-V44-006 与 REQ-V44-001/002 存在强协同（闭环串联四步）。

## 1.5 feature gate 定义与测试命令

| feature gate | 所属包 | 依赖 | 测试命令 | 默认 |
|-------------|--------|------|---------|------|
| `query-advisor` | sz-orm-advisor（新包） | serde_json（可选）+ sz-orm-explain + sz-orm-adaptive + sz-orm-ai | `cargo test -p sz-orm-advisor --features query-advisor` | 关闭 |
| `slow-query-diagnosis` | sz-orm-diagnosis（新包） | serde_json（可选）+ sz-orm-flamegraph + sz-orm-adaptive + sz-orm-advisor | `cargo test -p sz-orm-diagnosis --features slow-query-diagnosis` | 关闭 |
| `db-fusion-v2` | sz-orm-fusion（扩展） | db-fusion（既有 POC）+ sz-orm-core | `cargo test -p sz-orm-fusion --features db-fusion-v2` | 关闭 |
| `query-logging` | sz-orm-observability（扩展） | sz-orm-flamegraph + sz-orm-masking | `cargo test -p sz-orm-observability --features query-logging` | 关闭 |
| `perf-baseline` | sz-orm-explain（扩展） | explain-analyzer（既有）+ sz-orm-flamegraph | `cargo test -p sz-orm-explain --features perf-baseline` | 关闭 |
| `query-intelligence-loop` | sz-orm-advisor（扩展） | query-advisor（既有）+ sz-orm-flamegraph + sz-orm-diagnosis | `cargo test -p sz-orm-advisor --features query-intelligence-loop` | 关闭 |

---

# 二、M0：文档基线与准备（P0，0.5 周）

**目标**：锁定 v4.3.0 已验收基线，准备 v4.4.0 开发环境（新增 2 包 workspace 注册 + 6 feature gate 骨架）。
**对应需求**：—（文档基线与环境准备，非功能需求）
**预期工作量**：0.5 周
**依赖**：无（v4.3.0 已全部完成并提交）

## M0-T1：v4.3.0 完成总结与基线锁定

**任务描述**：总结 v4.3.0 交付成果（26 任务 / 156 子任务全部完成），锁定测试基线（约 7,000 个测试通过 + 14 道门禁全通过），作为 v4.4.0 开发的基准。

**涉及文件**：`docs/spec/v4.3.0/tasks.md`（既有，确认全部 `[x]`）、`docs/评估/2026-08-12_v4.2.0_基线评估.md`（既有，追加 v4.3.0 章节）

**子任务**：
- [ ] M0-T1.1 确认 `docs/spec/v4.3.0/tasks.md` 26 任务 / 156 子任务全部标记 `[x]`（v4.3.0 已完成）
- [ ] M0-T1.2 运行 `cargo test --workspace -j 2 --no-fail-fast` 记录 v4.3.0 测试基线（约 7,000 个测试通过）
- [ ] M0-T1.3 运行 14 道门禁全量验证，记录基线通过状态（fmt/check/clippy/test/doc/audit/integration/占位/SQL注入/feature组合/ADR-0001/文档一致性/审计证据/文档同步）
- [ ] M0-T1.4 确认 v4.3.0 7 个 feature gate（`explain-analyzer`/`query-flamegraph`/`n1-lint`/`lineage-viz`/`compile-governance`/`adaptive-query`/`db-fusion`）默认全关闭，行为不变

**验收标准**：v4.3.0 基线锁定（测试数 + 门禁通过状态 + feature gate 状态），每项附 file:line 或命令输出证据

**依赖**：无

## M0-T2：v4.4.0 开发环境准备

**任务描述**：在 workspace 注册新增 2 包（`sz-orm-advisor` / `sz-orm-diagnosis`）骨架，创建 6 个新 feature gate 占位（默认关闭），验证 workspace 编译通过。

**涉及文件**：
- `Cargo.toml`（workspace.members 新增 2 包）
- `packages/sz-orm-advisor/Cargo.toml`（新建骨架）
- `packages/sz-orm-advisor/src/lib.rs`（新建骨架）
- `packages/sz-orm-diagnosis/Cargo.toml`（新建骨架）
- `packages/sz-orm-diagnosis/src/lib.rs`（新建骨架）

**复用标注**：既有 workspace 结构 `Cargo.toml`（58 个成员）；既有 feature gate 模式 `packages/sz-orm-core/Cargo.toml`（25+ feature）

**子任务**：
- [ ] M0-T2.1 创建 `packages/sz-orm-advisor/` 包骨架（`Cargo.toml` + `src/lib.rs` 空 lib），workspace.members 注册
- [ ] M0-T2.2 创建 `packages/sz-orm-diagnosis/` 包骨架（`Cargo.toml` + `src/lib.rs` 空 lib），workspace.members 注册
- [ ] M0-T2.3 `sz-orm-advisor/Cargo.toml` 定义 `query-advisor` feature（默认关闭，依赖占位）
- [ ] M0-T2.4 `sz-orm-diagnosis/Cargo.toml` 定义 `slow-query-diagnosis` feature（默认关闭，依赖占位）
- [ ] M0-T2.5 验证 `cargo check --workspace` 编译通过（新增 2 包骨架不影响既有编译）
- [ ] M0-T2.6 验证默认 feature 行为与 v4.3.0 一致（`cargo build --workspace` 行为不变）

**验收标准**：2 新包骨架创建成功，workspace 编译通过，默认 feature 行为不变

**依赖**：M0-T1

## M0-T3：基线验证

**任务描述**：运行文档一致性、审计证据、文档同步三道门禁，验证 v4.3.0 基线可被工具消费，v4.4.0 骨架不破坏既有基线。

**涉及文件**：`scripts/check-doc-consistency.py`、`scripts/audit-verify.sh`、`scripts/check-doc-sync.py`

**子任务**：
- [ ] M0-T3.1 运行 `python scripts/check-doc-consistency.py`（门禁 12），验证文档与代码一致
- [ ] M0-T3.2 运行 `bash scripts/audit-verify.sh docs/spec/v4.3.0/tasks.md`（门禁 13），验证 v4.3.0 tasks.md 所有 file:line 引用真实存在
- [ ] M0-T3.3 运行 `python scripts/check-doc-sync.py --diff HEAD`（门禁 14），验证文档与 HEAD 同步

**验收标准**：三道门禁全部通过；v4.3.0 tasks.md 所有 file:line 引用经 audit-verify 验证真实存在

**依赖**：M0-T2

---

# 三、M1：查询自动优化建议引擎（REQ-V44-001，P1，2 周）

**目标**：新增 `sz-orm-advisor` 包，`OptimizationAdvisor` 规则引擎基于既有 EXPLAIN 分析结果（`ExplainPlan` `packages/sz-orm-explain/src/lib.rs:76`）+ 自适应统计（`QueryStats` `packages/sz-orm-adaptive/src/stats.rs:11`），复用既有 AI 建议结构（`IndexSuggestion` `packages/sz-orm-ai/src/index_advisor.rs:71` / `RewriteSuggestion` `packages/sz-orm-ai/src/rewrite_advisor.rs:61` / `TuningSuggestion` `packages/sz-orm-ai/src/auto_tuning/mod.rs:71`），通过规则匹配生成六种可执行优化建议（AddIndex/DropIndex/UsePagination/EnableCache/RewriteQuery/AdjustPoolSize），无 AI 依赖。
**对应需求**：REQ-V44-001（spec.md §5.1，design.md §2.2.1）
**预期工作量**：2 周
**依赖**：无（M1 为 P1 独立需求，复用既有 sz-orm-explain + sz-orm-adaptive + sz-orm-ai，新包可与 v4.3.0 并行）

## M1-T1：sz-orm-advisor 包搭建 + query-advisor feature gate

**任务描述**：完善 `sz-orm-advisor` 包骨架（M0-T2 已创建），定义 `query-advisor` feature gate 隔离，配置依赖（sz-orm-explain + sz-orm-adaptive + sz-orm-ai + serde_json 可选），作为优化建议引擎的基础设施。

**涉及文件**：
- `packages/sz-orm-advisor/Cargo.toml`（完善：依赖 sz-orm-explain + sz-orm-adaptive + sz-orm-ai + serde_json optional）
- `packages/sz-orm-advisor/src/lib.rs`（完善：模块声明）
- `Cargo.toml`（workspace.members 已注册，M0-T2）

**复用标注**：既有 `ExplainPlan` `packages/sz-orm-explain/src/lib.rs:76`；既有 `QueryStats` `packages/sz-orm-adaptive/src/stats.rs:11`；既有 `IndexSuggestion` `packages/sz-orm-ai/src/index_advisor.rs:71`

**feature gate 隔离**：`query-advisor = ["dep:serde_json", "dep:sz-orm-explain", "dep:sz-orm-adaptive", "dep:sz-orm-ai"]`，默认关闭

**子任务**：
- [ ] M1-T1.1 `packages/sz-orm-advisor/Cargo.toml` 配置依赖：`sz-orm-explain`（optional）+ `sz-orm-adaptive`（optional）+ `sz-orm-ai`（optional）+ `serde_json`（optional）
- [ ] M1-T1.2 `[features] query-advisor = ["dep:serde_json", "dep:sz-orm-explain", "dep:sz-orm-adaptive", "dep:sz-orm-ai"]`，默认关闭
- [ ] M1-T1.3 `src/lib.rs` 声明模块结构（`mod suggestion; mod advisor; mod rules; mod report;`），`#[cfg(feature = "query-advisor")]` 门控
- [ ] M1-T1.4 验证 `cargo check -p sz-orm-advisor` 编译通过，默认 feature 行为不变（空 lib）
- [ ] M1-T1.5 验证 `cargo check -p sz-orm-advisor --features query-advisor` 编译通过（依赖链路打通）

**验收标准**：包搭建成功，feature gate 默认关闭，依赖链路打通，workspace 集成编译通过

**依赖**：M0-T2

## M1-T2：OptimizationSuggestion 统一建议结构 + 六种建议类型

**任务描述**：定义 `OptimizationSuggestion` 统一建议结构与 `SuggestionType` 六种建议类型枚举，作为规则引擎的输出数据模型。

**涉及文件**：
- `packages/sz-orm-advisor/src/suggestion.rs`（新建）

**复用标注**：既有 `IndexSuggestion` `packages/sz-orm-ai/src/index_advisor.rs:71`（index_columns + index_type + ddl_text + expected_benefit + evidence）；既有 `RewriteSuggestion` `packages/sz-orm-ai/src/rewrite_advisor.rs:61`（original_sql + rewritten_sql + transform_type + equivalence_proof）；既有 `TuningSuggestion` `packages/sz-orm-ai/src/auto_tuning/mod.rs:71`（suggestion_type + sql_before + sql_after + expected_gain）

**子任务**：
- [ ] M1-T2.1 定义 `pub enum SuggestionType { AddIndex, DropIndex, UsePagination, EnableCache, RewriteQuery, AdjustPoolSize }`（六种建议类型，`Serialize + Deserialize`）
- [ ] M1-T2.2 定义 `pub struct OptimizationSuggestion { pub suggestion_type: SuggestionType, pub target_query: String, pub description: String, pub action: String, pub confidence: f64, pub estimated_improvement: Option<String> }`（统一建议结构，`confidence` 范围 0.0~1.0）
- [ ] M1-T2.3 实现 `OptimizationSuggestion::needs_manual_confirmation(&self) -> bool`（`confidence < 0.5` 返回 true，标注"需人工确认"）
- [ ] M1-T2.4 单元测试：`SuggestionType` 六种变体序列化/反序列化往返一致
- [ ] M1-T2.5 单元测试：`confidence = 0.3` → `needs_manual_confirmation` 返回 true；`confidence = 0.9` → 返回 false
- [ ] M1-T2.6 边界测试：`confidence = 0.0` / `confidence = 1.0` 边界值正确处理

**验收标准**：统一建议结构 + 六种建议类型定义；序列化/反序列化测试通过；低置信度标注逻辑正确

**依赖**：M1-T1

## M1-T3：OptimizationAdvisor 规则引擎 + suggest 入口

**任务描述**：实现 `OptimizationAdvisor` 规则引擎，`suggest(plan, stats) -> Vec<OptimizationSuggestion>` 入口，六种建议类型的规则匹配，复用既有 `ExplainPlan`/`QueryStats` 决策谓词。

**涉及文件**：
- `packages/sz-orm-advisor/src/advisor.rs`（新建，规则引擎核心）
- `packages/sz-orm-advisor/src/rules.rs`（新建，六种建议类型规则）

**复用标注**：既有 `ExplainPlan::missing_index` `packages/sz-orm-explain/src/lib.rs:91`（AddIndex 规则复用）；既有 `QueryStats::should_paginate` `packages/sz-orm-adaptive/src/stats.rs:66`（UsePagination 规则复用）；既有 `QueryStats::should_cache` `packages/sz-orm-adaptive/src/stats.rs:73`（EnableCache 规则复用）；既有 `PlanRegression` `packages/sz-orm-explain/src/regression.rs:69`（回归 → 建议映射）

**子任务**：
- [ ] M1-T3.1 定义 `pub struct AdvisorConfig { pub row_threshold: u64, pub confidence_threshold: f64 }`（行数阈值默认 1000，置信度阈值默认 0.5）
- [ ] M1-T3.2 定义 `pub struct OptimizationAdvisor { config: AdvisorConfig }`，`impl OptimizationAdvisor { pub fn new(config: AdvisorConfig) -> Self }`
- [ ] M1-T3.3 实现 `pub fn suggest(&self, plan: Option<&ExplainPlan>, stats: Option<&QueryStats>) -> Vec<OptimizationSuggestion>`：规则匹配生成建议
- [ ] M1-T3.4 规则 (a)：`plan.scan_type == FullTable` 且 `plan.rows > row_threshold` → 生成 `AddIndex` 建议"添加索引 ON table(条件列)"，置信度 0.9（复用 `ExplainPlan::missing_index` `packages/sz-orm-explain/src/lib.rs:91`）
- [ ] M1-T3.5 规则 (b)：检测到冗余索引 → 生成 `DropIndex` 建议，置信度 0.7
- [ ] M1-T3.6 规则 (c)：`stats.should_paginate(row_threshold)` 为真 → 生成 `UsePagination` 建议"改用游标分页"，置信度 0.8（复用 `packages/sz-orm-adaptive/src/stats.rs:66`）
- [ ] M1-T3.7 规则 (d)：`stats.should_cache(threshold_ms, min_executions)` 为真 → 生成 `EnableCache` 建议"启用缓存"，置信度 0.8（复用 `packages/sz-orm-adaptive/src/stats.rs:73`）
- [ ] M1-T3.8 规则 (e)：可改写查询检测 → 生成 `RewriteQuery` 建议，置信度 0.6
- [ ] M1-T3.9 规则 (f)：PoolAcquire 耗时高（来自 `QueryPhaseTiming` 占比 > 30%）→ 生成 `AdjustPoolSize` 建议，置信度 0.7
- [ ] M1-T3.10 建议后处理：按置信度降序排序；冲突建议（AddIndex 与 DropIndex 针对同一索引）保留高置信度，低置信度标注"conflict, skipped"
- [ ] M1-T3.11 单元测试：`ExplainPlan { scan_type: FullTable, table: "users", index: None, rows: 10000 }` → 生成 `AddIndex` 建议，置信度 0.9，action 含"CREATE INDEX"
- [ ] M1-T3.12 单元测试：`QueryStats` 平均行数 2000 > 阈值 1000 → 生成 `UsePagination` 建议"改用游标分页"
- [ ] M1-T3.13 单元测试：`QueryStats` 平均耗时 50ms > 阈值 30ms 且执行 10 次 → 生成 `EnableCache` 建议"启用缓存"
- [ ] M1-T3.14 单元测试：建议冲突（AddIndex + DropIndex 同索引）→ 保留高置信度，低置信度标注"conflict, skipped"
- [ ] M1-T3.15 边界测试：`plan = None`（EXPLAIN 不可用）→ 降级为仅统计建议，不 panic
- [ ] M1-T3.16 边界测试：`stats = None`（统计不足）→ 降级为仅 EXPLAIN 建议，不 panic

**验收标准**：规则引擎 `suggest` 入口完整；六种建议类型规则匹配正确；复用既有 `missing_index`/`should_paginate`/`should_cache`；冲突处理 + 降级处理正确；`cargo test -p sz-orm-advisor --features query-advisor` 通过

**依赖**：M1-T1、M1-T2

## M1-T4：复用既有 AI 建议结构转换

**任务描述**：实现 `OptimizationSuggestion` 到既有 AI 建议结构（`IndexSuggestion`/`RewriteSuggestion`/`TuningSuggestion`）的转换方法，复用既有建议数据模型不重复实现。

**涉及文件**：
- `packages/sz-orm-advisor/src/suggestion.rs`（扩展：转换方法）

**复用标注**：既有 `IndexSuggestion` `packages/sz-orm-ai/src/index_advisor.rs:71`；既有 `RewriteSuggestion` `packages/sz-orm-ai/src/rewrite_advisor.rs:61`；既有 `TuningSuggestion` `packages/sz-orm-ai/src/auto_tuning/mod.rs:71`；既有 `AutoTuningPipeline` `packages/sz-orm-ai/src/auto_tuning/pipeline.rs:15`（互补关系，保留不动）

**子任务**：
- [ ] M1-T4.1 实现 `impl OptimizationSuggestion { pub fn to_index_suggestion(&self) -> Option<IndexSuggestion> }`：`AddIndex` 建议 → `IndexSuggestion`（index_columns + index_type + ddl_text + expected_benefit + evidence），其他类型返回 None
- [ ] M1-T4.2 实现 `impl OptimizationSuggestion { pub fn to_rewrite_suggestion(&self) -> Option<RewriteSuggestion> }`：`RewriteQuery` 建议 → `RewriteSuggestion`（original_sql + rewritten_sql + transform_type + equivalence_proof），其他类型返回 None
- [ ] M1-T4.3 实现 `impl OptimizationSuggestion { pub fn to_tuning_suggestion(&self) -> Option<TuningSuggestion> }`：所有建议 → `TuningSuggestion`（suggestion_type + sql_before + sql_after + expected_gain）
- [ ] M1-T4.4 单元测试：`AddIndex` 建议 → `to_index_suggestion` 返回 `Some(IndexSuggestion)`，`index_columns`/`ddl_text` 正确
- [ ] M1-T4.5 单元测试：`RewriteQuery` 建议 → `to_rewrite_suggestion` 返回 `Some(RewriteSuggestion)`，`original_sql`/`rewritten_sql` 正确
- [ ] M1-T4.6 单元测试：`UsePagination` 建议 → `to_index_suggestion` 返回 None（非索引建议）

**验收标准**：三种转换方法实现；复用既有 AI 建议结构不重复实现；转换测试通过

**依赖**：M1-T2、M1-T3

## M1-T5：JSON 报告输出 + 五方言 DDL 生成

**任务描述**：实现 JSON 格式优化建议报告输出（可被 CI/IDE 消费），AddIndex 建议的 DDL 文本按五方言生成（索引 DDL 语法差异）。

**涉及文件**：
- `packages/sz-orm-advisor/src/report.rs`（新建，JSON 报告）
- `packages/sz-orm-advisor/src/dialect.rs`（新建，五方言 DDL 生成）

**复用标注**：既有 28 方言枚举 `packages/sz-orm-core/src/db_type.rs:11`；既有 `ExplainPlan` 5 方言解析器 `parser_for` `packages/sz-orm-explain/src/lib.rs:127`

**子任务**：
- [ ] M1-T5.1 实现 `pub fn to_json(suggestions: &[OptimizationSuggestion]) -> String`：JSON 报告序列化（含建议列表 + 目标查询 + 置信度 + 预估改善）
- [ ] M1-T5.2 单元测试：生成 3 条建议 → `to_json` 输出 JSON 含 3 条建议，可被 `serde_json::from_str` 解析
- [ ] M1-T5.3 实现五方言 AddIndex DDL 生成：MySQL/PostgreSQL/SQLite/Oracle/MSSQL 索引 DDL（`CREATE INDEX idx_xxx ON table(col)` + 方言特定选项标注如 `USING GIST`（PG）/`CLUSTERED`（MSSQL））
- [ ] M1-T5.4 单元测试：五方言各生成 AddIndex 建议，action 字段 DDL 语法正确
- [ ] M1-T5.5 边界测试：空建议列表 → `to_json` 输出 `{"suggestions": []}`，不 panic

**验收标准**：JSON 报告输出可被 CI/IDE 消费；五方言 DDL 生成正确；`cargo test -p sz-orm-advisor --features query-advisor` 通过

**依赖**：M1-T3

## M1-T6：M1 集成测试与门禁验证

**任务描述**：M1 里程碑集成测试与门禁验证，确保 REQ-V44-001 全部验收条件满足，默认 feature 行为不变。

**子任务**：
- [ ] M1-T6.1 集成测试：`OptimizationAdvisor::suggest` 完整流程（ExplainPlan + QueryStats → 六种建议 → 冲突处理 → JSON 报告）
- [ ] M1-T6.2 集成测试：复用既有 `ExplainPlan` `packages/sz-orm-explain/src/lib.rs:76` + `QueryStats` `packages/sz-orm-adaptive/src/stats.rs:11` 真实数据生成建议
- [ ] M1-T6.3 运行 `cargo test -p sz-orm-advisor --features query-advisor`（全部通过）
- [ ] M1-T6.4 `cargo clippy -p sz-orm-advisor --features query-advisor -- -D warnings`（clippy 静态分析）
- [ ] M1-T6.5 `cargo fmt -p sz-orm-advisor -- --check`（fmt 格式检查）
- [ ] M1-T6.6 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-advisor/` 无占位实现
- [ ] M1-T6.7 验证默认 feature 行为与 v4.3.0 一致（`cargo build -p sz-orm-advisor` 无建议生成，行为不变）
- [ ] M1-T6.8 验证 `query-advisor` 与既有 feature（`explain-analyzer`/`adaptive-query`）组合编译通过

**验收标准**：M1 集成测试通过；clippy/fmt/占位检查通过；默认 feature 行为不变；六种建议类型 + 复用既有 ExplainPlan/QueryStats/AI 建议结构 + JSON 报告 + 五方言 DDL 全部验证

**依赖**：M1-T1、M1-T2、M1-T3、M1-T4、M1-T5

---

# 四、M2：慢查询自动诊断报告（REQ-V44-002，P1，1.5 周）

**目标**：新增 `sz-orm-diagnosis` 包，`SlowQueryDiagnoser` 基于既有火焰图阶段耗时（`QueryPhaseTiming` `packages/sz-orm-flamegraph/src/collector.rs:39`）+ 自适应 slow 标记（`QueryOutcome.slow` `packages/sz-orm-adaptive/src/executor.rs:116`），分析慢查询根因（PoolExhaustion/SqlInefficiency/LargeResultSet/BuildOverhead/MixedCause）并生成诊断报告，与优化建议引擎（REQ-V44-001）联动。
**对应需求**：REQ-V44-002（spec.md §5.2，design.md §2.2.2）
**预期工作量**：1.5 周
**依赖**：M1 交付后（建议联动依赖 `sz-orm-advisor` `OptimizationAdvisor::suggest`）

## M2-T1：sz-orm-diagnosis 包搭建 + slow-query-diagnosis feature gate

**任务描述**：完善 `sz-orm-diagnosis` 包骨架（M0-T2 已创建），定义 `slow-query-diagnosis` feature gate 隔离，配置依赖（sz-orm-flamegraph + sz-orm-adaptive + sz-orm-advisor + serde_json 可选）。

**涉及文件**：
- `packages/sz-orm-diagnosis/Cargo.toml`（完善：依赖 sz-orm-flamegraph + sz-orm-adaptive + sz-orm-advisor + serde_json optional）
- `packages/sz-orm-diagnosis/src/lib.rs`（完善：模块声明）

**复用标注**：既有 `QueryPhaseTiming` `packages/sz-orm-flamegraph/src/collector.rs:39`；既有 `QueryOutcome.slow` `packages/sz-orm-adaptive/src/executor.rs:116`；REQ-V44-001 `OptimizationAdvisor` `packages/sz-orm-advisor/src/advisor.rs`（建议联动）

**feature gate 隔离**：`slow-query-diagnosis = ["dep:serde_json", "dep:sz-orm-flamegraph", "dep:sz-orm-adaptive", "dep:sz-orm-advisor"]`，默认关闭

**子任务**：
- [ ] M2-T1.1 `packages/sz-orm-diagnosis/Cargo.toml` 配置依赖：`sz-orm-flamegraph`（optional）+ `sz-orm-adaptive`（optional）+ `sz-orm-advisor`（optional）+ `serde_json`（optional）
- [ ] M2-T1.2 `[features] slow-query-diagnosis = ["dep:serde_json", "dep:sz-orm-flamegraph", "dep:sz-orm-adaptive", "dep:sz-orm-advisor"]`，默认关闭
- [ ] M2-T1.3 `src/lib.rs` 声明模块结构（`mod diagnoser; mod report; mod root_cause;`），`#[cfg(feature = "slow-query-diagnosis")]` 门控
- [ ] M2-T1.4 验证 `cargo check -p sz-orm-diagnosis --features slow-query-diagnosis` 编译通过（依赖链路打通，含 sz-orm-advisor）
- [ ] M2-T1.5 验证默认 `cargo check -p sz-orm-diagnosis` 编译通过（空 lib，行为不变）

**验收标准**：包搭建成功，feature gate 默认关闭，依赖链路打通（含 sz-orm-advisor 建议联动）

**依赖**：M0-T2、M1-T1（sz-orm-advisor 交付）

## M2-T2：RootCause 根因分析 + DiagnosisReport 报告结构

**任务描述**：定义 `RootCause` 根因枚举、`Severity` 严重度枚举、`PhaseBreakdown` 阶段分解、`DiagnosisReport` 诊断报告结构，实现根因分析逻辑（阶段耗时占比判定）。

**涉及文件**：
- `packages/sz-orm-diagnosis/src/root_cause.rs`（新建，根因分析）
- `packages/sz-orm-diagnosis/src/report.rs`（新建，报告结构）

**复用标注**：既有 `Phase` 枚举 `packages/sz-orm-flamegraph/src/collector.rs:11`（Build/Bind/PoolAcquire/SqlExecute/ResultMap）；既有 `QueryPhaseTiming` `packages/sz-orm-flamegraph/src/collector.rs:39`（phase + start_ms + duration_ms）

**子任务**：
- [ ] M2-T2.1 定义 `pub enum RootCause { PoolExhaustion, SqlInefficiency, LargeResultSet, BuildOverhead, MixedCause, Unknown }`（根因枚举，`Serialize + Deserialize`）
- [ ] M2-T2.2 定义 `pub enum Severity { Info, Warning, Critical }`（严重度枚举）
- [ ] M2-T2.3 定义 `pub struct PhaseBreakdown { pub phase: Phase, pub elapsed_ms: u64, pub percentage: f64, pub anomaly: bool }`（阶段分解，`phase` 复用既有 `Phase` `packages/sz-orm-flamegraph/src/collector.rs:11`）
- [ ] M2-T2.4 定义 `pub struct DiagnosisReport { pub query_key: String, pub total_elapsed_ms: u64, pub root_cause: RootCause, pub phase_breakdown: Vec<PhaseBreakdown>, pub suggestions: Vec<OptimizationSuggestion>, pub severity: Severity }`（诊断报告，`suggestions` 来自 REQ-V44-001 联动）
- [ ] M2-T2.5 实现 `pub fn analyze_root_cause(timings: &[QueryPhaseTiming], config: &DiagnosisConfig) -> RootCause`：阶段耗时占比判定（PoolAcquire > 30% → PoolExhaustion；SqlExecute > 50% → SqlInefficiency；ResultMap > 30% → LargeResultSet；Build > 20% → BuildOverhead；多阶段高 → MixedCause）
- [ ] M2-T2.6 定义 `pub struct DiagnosisConfig { pub pool_threshold_pct: f64, pub sql_threshold_pct: f64, pub result_threshold_pct: f64, pub build_threshold_pct: f64 }`（根因阈值配置，默认 30%/50%/30%/20%）
- [ ] M2-T2.7 实现 `pub fn build_phase_breakdown(timings: &[QueryPhaseTiming], config: &DiagnosisConfig) -> Vec<PhaseBreakdown>`：各阶段耗时占比 + 异常标记
- [ ] M2-T2.8 单元测试：PoolAcquire 耗时 60ms / 总 100ms → `PoolExhaustion`
- [ ] M2-T2.9 单元测试：SqlExecute 耗时 80ms / 总 100ms → `SqlInefficiency`
- [ ] M2-T2.10 单元测试：ResultMap 耗时 40ms / 总 100ms → `LargeResultSet`
- [ ] M2-T2.11 单元测试：Build 耗时 25ms / 总 100ms → `BuildOverhead`
- [ ] M2-T2.12 单元测试：多阶段超阈值（PoolAcquire 35% + SqlExecute 55%）→ `MixedCause`
- [ ] M2-T2.13 边界测试：阶段耗时缺失（空 timings）→ `RootCause::Unknown`，不 panic

**验收标准**：根因分析逻辑正确（阶段耗时占比判定）；五种根因 + MixedCause + Unknown 全覆盖；`cargo test -p sz-orm-diagnosis --features slow-query-diagnosis` 通过

**依赖**：M2-T1

## M2-T3：SlowQueryDiagnoser 诊断器 + diagnose 入口

**任务描述**：实现 `SlowQueryDiagnoser` 诊断器，`diagnose` 入口仅对 `slow == true` 查询触发诊断（非每次查询），复用既有 `QueryOutcome.slow` + `AdaptiveConfig.slow_ms`。

**涉及文件**：
- `packages/sz-orm-diagnosis/src/diagnoser.rs`（新建，诊断器核心）

**复用标注**：既有 `QueryOutcome.slow` `packages/sz-orm-adaptive/src/executor.rs:116`（慢查询标记）；既有 `AdaptiveConfig.slow_ms` `packages/sz-orm-adaptive/src/executor.rs:35`（慢查询阈值，默认 100ms）；既有 `QueryTracer::trace_execute` `packages/sz-orm-flamegraph/src/collector.rs:61`（分阶段计时）

**子任务**：
- [ ] M2-T3.1 定义 `pub struct SlowQueryDiagnoser { config: DiagnosisConfig, advisor: Arc<OptimizationAdvisor> }`（诊断器，含建议引擎联动）
- [ ] M2-T3.2 实现 `impl SlowQueryDiagnoser { pub fn new(config: DiagnosisConfig, advisor: Arc<OptimizationAdvisor>) -> Self }`
- [ ] M2-T3.3 实现 `pub fn diagnose(&self, query_key: &str, timings: &[QueryPhaseTiming], outcome: &QueryOutcome) -> Option<DiagnosisReport>`：仅 `outcome.slow == true` 返回 Some，否则 None
- [ ] M2-T3.4 诊断流程：根因分析 → 阶段分解 → 建议联动（根因 → 对应建议）→ severity 判定 → 生成 `DiagnosisReport`
- [ ] M2-T3.5 建议联动映射：`PoolExhaustion` → `AdjustPoolSize`；`SqlInefficiency` → `AddIndex`/`RewriteQuery`；`LargeResultSet` → `UsePagination`；`BuildOverhead` → `RewriteQuery`（调用 `OptimizationAdvisor::suggest`）
- [ ] M2-T3.6 severity 判定：`total_elapsed_ms > slow_ms * 3` → Critical；`> slow_ms * 2` → Warning；否则 Info
- [ ] M2-T3.7 单元测试：`outcome.slow == false` → 返回 None（不触发诊断）
- [ ] M2-T3.8 单元测试：`outcome.slow == true` → 返回 `Some(DiagnosisReport)`，含根因 + 阶段分解 + 建议
- [ ] M2-T3.9 单元测试：根因 `PoolExhaustion` → 建议列表含 `AdjustPoolSize`
- [ ] M2-T3.10 边界测试：阶段耗时总和与 `outcome.elapsed_ms` 误差超 10% → 标注"timing mismatch"，根因置信度降低

**验收标准**：诊断器 `diagnose` 入口完整；仅对慢查询触发；建议联动正确；severity 判定正确；timing mismatch 降级处理

**依赖**：M2-T2、M1-T3（OptimizationAdvisor::suggest）

## M2-T4：双格式输出（JSON + 人类可读）

**任务描述**：实现 `DiagnosisReport` 的 JSON 格式输出（CI 消费）与人类可读格式输出（含根因/阶段分解/建议表格）。

**涉及文件**：
- `packages/sz-orm-diagnosis/src/report.rs`（扩展：双格式输出）

**子任务**：
- [ ] M2-T4.1 实现 `impl DiagnosisReport { pub fn to_json(&self) -> String }`：JSON 序列化（含 query_key/total_elapsed_ms/root_cause/phase_breakdown/suggestions/severity）
- [ ] M2-T4.2 实现 `impl DiagnosisReport { pub fn to_human_readable(&self) -> String }`：人类可读格式（含根因/阶段分解表格/建议列表表格）
- [ ] M2-T4.3 单元测试：`to_json` 输出可被 `serde_json::from_str` 解析，字段完整
- [ ] M2-T4.4 单元测试：`to_human_readable` 含根因名/阶段表格/建议表格
- [ ] M2-T4.5 边界测试：空建议列表 → 人类可读格式标注"no suggestions"

**验收标准**：双格式输出正确；JSON 可被 CI 消费；人类可读格式含根因/阶段/建议表格

**依赖**：M2-T2、M2-T3

## M2-T5：M2 集成测试与门禁验证

**任务描述**：M2 里程碑集成测试与门禁验证，确保 REQ-V44-002 全部验收条件满足，默认 feature 行为不变。

**子任务**：
- [ ] M2-T5.1 集成测试：`SlowQueryDiagnoser::diagnose` 完整流程（QueryPhaseTiming + QueryOutcome.slow → 根因分析 → 建议联动 → DiagnosisReport → JSON/人类可读）
- [ ] M2-T5.2 集成测试：复用既有 `QueryTracer::trace_execute` `packages/sz-orm-flamegraph/src/collector.rs:61` 采集 timings + `QueryOutcome.slow` `packages/sz-orm-adaptive/src/executor.rs:116` 触发诊断
- [ ] M2-T5.3 运行 `cargo test -p sz-orm-diagnosis --features slow-query-diagnosis`（全部通过）
- [ ] M2-T5.4 `cargo clippy -p sz-orm-diagnosis --features slow-query-diagnosis -- -D warnings`
- [ ] M2-T5.5 `cargo fmt -p sz-orm-diagnosis -- --check`
- [ ] M2-T5.6 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-diagnosis/` 无占位实现
- [ ] M2-T5.7 验证默认 feature 行为与 v4.3.0 一致（`cargo build -p sz-orm-diagnosis` 无诊断，行为不变）
- [ ] M2-T5.8 验证 `slow-query-diagnosis` 与既有 feature（`query-flamegraph`/`adaptive-query`）+ `query-advisor` 组合编译通过

**验收标准**：M2 集成测试通过；门禁通过；默认行为不变；根因分析 + 仅慢查询触发 + 建议联动 + 双格式输出全部验证

**依赖**：M2-T1、M2-T2、M2-T3、M2-T4

---

# 五、M3：db-fusion 转正（REQ-V44-003，P1，3 周）

**目标**：扩展既有 `sz-orm-fusion` 包，将 v4.3.0 POC 转正为正式能力：阶段一 `TtlFusionCache`（TTL 过期 + 失效广播复用 `InvalidationBus` `packages/sz-orm-core/src/l2_cache.rs:82`），阶段二 `CdcSyncCoordinator`（CDC 增量同步复用 `DialectCapturer` `packages/sz-orm-queue/src/cdc/capturer.rs:12`）+ `VectorPushdownExecutor`（真实向量搜索下推复用 `HybridSearcher` `packages/sz-orm-vector/src/hybrid_search/searcher.rs:30`），转正 API `#[stable]`，POC API `#[deprecated]`。
**对应需求**：REQ-V44-003（spec.md §5.3，design.md §2.2.3）
**预期工作量**：3 周
**依赖**：无（M3 为 P1 独立需求，复用既有 sz-orm-fusion POC + sz-orm-core + sz-orm-queue + sz-orm-vector）

## M3-T1：db-fusion-v2 feature gate + 转正 API 标注

**任务描述**：在 `sz-orm-fusion` 新增 `db-fusion-v2` feature gate（依赖既有 `db-fusion` POC feature），配置依赖（sz-orm-core InvalidationBus 复用），转正 API `#[stable]` 标注 + POC API `#[deprecated]` 标注。

**涉及文件**：
- `packages/sz-orm-fusion/Cargo.toml`（扩展：新增 `db-fusion-v2` feature）
- `packages/sz-orm-fusion/src/lib.rs`（扩展：`#[stable]`/`#[deprecated]` 标注）

**复用标注**：既有 `db-fusion` feature `packages/sz-orm-fusion/Cargo.toml`（POC feature）；既有 `FusionConfig` `packages/sz-orm-fusion/src/plan.rs:21`；既有 `FusionExecutor` `packages/sz-orm-fusion/src/executor.rs:63`；既有 `MemoryFusionCache` `packages/sz-orm-fusion/src/executor.rs:16`

**feature gate 隔离**：`db-fusion-v2 = ["db-fusion", "dep:sz-orm-core"]`，默认关闭，转正能力在 `db-fusion-v2` 内

**子任务**：
- [ ] M3-T1.1 `packages/sz-orm-fusion/Cargo.toml` 新增 `db-fusion-v2 = ["db-fusion", "dep:sz-orm-core"]` feature，默认关闭
- [ ] M3-T1.2 新增 `sz-orm-core` 依赖（optional，`InvalidationBus`/`RedisPubSubInvalidationBus` 复用）
- [ ] M3-T1.3 POC API `#[deprecated(note = "use TtlFusionCache + CdcSyncCoordinator")]` 标注：`MemoryFusionCache` `packages/sz-orm-fusion/src/executor.rs:16`
- [ ] M3-T1.4 POC API `#[deprecated]` 标注：`FusionConfig` `packages/sz-orm-fusion/src/plan.rs:21`（保留可用，标注迁移指引）
- [ ] M3-T1.5 转正 API `#[stable]` 标注：新增 `TtlFusionCache`/`CdcSyncCoordinator`/`VectorPushdownExecutor`（后续任务实现）
- [ ] M3-T1.6 验证 `cargo check -p sz-orm-fusion --features db-fusion-v2` 编译通过
- [ ] M3-T1.7 验证默认 `cargo check -p sz-orm-fusion` POC API 保留可用（`#[deprecated]` 标注而非删除）

**验收标准**：`db-fusion-v2` feature gate 定义；POC API `#[deprecated]` 标注 + 迁移指引；转正 API `#[stable]` 标注；默认 POC API 保留可用

**依赖**：无

## M3-T2：TtlFusionCache TTL 融合缓存

**任务描述**：实现 `TtlFusionCache` TTL 融合缓存，TTL 过期检查（Instant::now vs 过期时间），实现既有 `FusionCache` trait，`FusionCache` trait 扩展 `get_with_ttl`/`set_with_ttl` 方法。

**涉及文件**：
- `packages/sz-orm-fusion/src/ttl_cache.rs`（新建，TTL 缓存）
- `packages/sz-orm-fusion/src/executor.rs`（扩展：`FusionCache` trait 扩展 `get_with_ttl`/`set_with_ttl`）

**复用标注**：既有 `FusionCache` trait `packages/sz-orm-fusion/src/executor.rs:8`（get/set 抽象）；既有 `MemoryFusionCache` `packages/sz-orm-fusion/src/executor.rs:16`（模式参考）

**子任务**：
- [ ] M3-T2.1 `FusionCache` trait 扩展 `fn get_with_ttl(&self, key: &str) -> Option<String>` / `fn set_with_ttl(&self, key: &str, value: String, ttl: Duration)`（默认实现委托到 get/set 保持向后兼容）
- [ ] M3-T2.2 定义 `pub struct TtlFusionCache { inner: Mutex<HashMap<String, (String, Instant)>>, default_ttl: Duration }`（TTL 融合缓存，`#[stable]` 标注）
- [ ] M3-T2.3 实现 `impl TtlFusionCache { pub fn new(default_ttl: Duration) -> Self }`
- [ ] M3-T2.4 实现 `impl FusionCache for TtlFusionCache`：`get`/`set` 委托到 `get_with_ttl`/`set_with_ttl`
- [ ] M3-T2.5 实现 `get_with_ttl`：检查 `Instant::now` vs 过期时间，过期返回 None 并清理条目
- [ ] M3-T2.6 实现 `set_with_ttl`：存储 `(value, Instant::now + ttl)`
- [ ] M3-T2.7 单元测试：设置 TTL 60s 缓存 → 60s 内命中返回数据，60s 后返回 None
- [ ] M3-T2.8 单元测试：`TtlFusionCache` 实现 `FusionCache` trait（get/set 行为正确）
- [ ] M3-T2.9 边界测试：TTL = 0（立即过期）→ set 后 get 返回 None
- [ ] M3-T2.10 性能测试：TTL 缓存查找/写入开销 ≤ 1ms（含 TTL 过期检查 + 缓存键构造）

**验收标准**：TTL 融合缓存正确；实现 `FusionCache` trait；过期逻辑正确；性能 ≤ 1ms；`cargo test -p sz-orm-fusion --features db-fusion-v2` 通过

**依赖**：M3-T1

## M3-T3：InvalidationBus 失效广播集成

**任务描述**：`FusionExecutor` 扩展 `with_invalidation_bus(bus)` 方法，主库写入后发布失效消息，跨实例缓存失效，复用既有 `InvalidationBus`/`RedisPubSubInvalidationBus`/`GossipInvalidationBus`。

**涉及文件**：
- `packages/sz-orm-fusion/src/executor.rs`（扩展：`with_invalidation_bus` + 失效广播逻辑）

**复用标注**：既有 `InvalidationBus` trait `packages/sz-orm-core/src/l2_cache.rs:82`（publish/subscribe）；既有 `LocalInvalidationBus` `packages/sz-orm-core/src/l2_cache.rs:93`（进程内 broadcast）；既有 `RedisPubSubInvalidationBus` `packages/sz-orm-core/src/dist_cache.rs:41`（Redis Pub/Sub）；既有 `GossipInvalidationBus` `packages/sz-orm-core/src/dist_cache.rs:179`（Gossip）；既有 `FusionExecutor` `packages/sz-orm-fusion/src/executor.rs:63`；既有 `FusionExecutor::execute` `:93`

**子任务**：
- [ ] M3-T3.1 实现 `impl FusionExecutor { pub fn with_invalidation_bus(mut self, bus: Arc<dyn InvalidationBus>) -> Self }`（注入失效总线，`#[stable]` 标注）
- [ ] M3-T3.2 `FusionExecutor::execute` 扩展：主库写入成功后 `bus.publish(InvalidationMessage)` 失效消息
- [ ] M3-T3.3 跨实例缓存失效：实例 A 主库写入 → 发布失效消息 → 实例 B 缓存失效 → 下次查询回源主库
- [ ] M3-T3.4 失效总线不可用降级：`bus.publish` 失败（Redis Pub/Sub 断连）→ 降级为 TTL 过期兜底，告警"invalidation bus unavailable, relying on TTL"
- [ ] M3-T3.5 单元测试：`with_invalidation_bus` 主库写入后 publish 失效消息
- [ ] M3-T3.6 单元测试：跨实例缓存失效（实例 A 写入 → 实例 B 缓存失效 → 回源）
- [ ] M3-T3.7 边界测试：失效总线不可用 → TTL 兜底降级，告警正确
- [ ] M3-T3.8 性能测试：失效广播开销 ≤ 5ms（含消息序列化 + 总线发布）

**验收标准**：失效广播集成正确；跨实例缓存失效；降级处理正确；性能 ≤ 5ms；复用既有 `InvalidationBus` 不重复实现

**依赖**：M3-T1、M3-T2

## M3-T4：CdcSyncCoordinator CDC 增量同步集成

**任务描述**：实现 `CdcSyncCoordinator` CDC 同步协调器，复用既有 `DialectCapturer` + `DownstreamSink`/`distribute_to_all` + `CdcCheckpoint`，主库变更自动同步到缓存/搜索索引。

**涉及文件**：
- `packages/sz-orm-fusion/src/cdc_sync.rs`（新建，CDC 同步协调器）
- `packages/sz-orm-fusion/Cargo.toml`（扩展：`db-fusion-v2` 增加 `sz-orm-queue` 依赖）

**复用标注**：既有 `DialectCapturer` trait `packages/sz-orm-queue/src/cdc/capturer.rs:12`（5 方言 CDC 捕获器）；既有 `DownstreamSink` trait `packages/sz-orm-queue/src/cdc/downstream.rs:12`（7 种下游分发）；既有 `distribute_to_all` `packages/sz-orm-queue/src/cdc/downstream.rs:178`（并行分发）；既有 `CdcCheckpoint` `packages/sz-orm-queue/src/cdc/checkpoint.rs`（断点续传）；既有 CDC 脱敏 `packages/sz-orm-queue/src/cdc/masking.rs`

**子任务**：
- [ ] M3-T4.1 `packages/sz-orm-fusion/Cargo.toml` `db-fusion-v2` feature 增加 `dep:sz-orm-queue` 依赖
- [ ] M3-T4.2 定义 `pub struct CdcSyncCoordinator { capturer: Arc<dyn DialectCapturer>, sinks: Vec<Box<dyn DownstreamSink>>, checkpoint: Option<CdcCheckpoint> }`（`#[stable]` 标注）
- [ ] M3-T4.3 实现 `impl CdcSyncCoordinator { pub fn new(capturer: Arc<dyn DialectCapturer>, sinks: Vec<Box<dyn DownstreamSink>>) -> Self }`
- [ ] M3-T4.4 实现 `pub async fn start_sync(&self) -> Result<(), CdcError>`：`DialectCapturer::start_capture` → 流式接收 ChangeEvent → 变更事件脱敏（`masking.rs`）→ `distribute_to_all` 分发到缓存失效 + 搜索索引更新下游
- [ ] M3-T4.5 `CdcCheckpoint` 断点续传：中断后从断点恢复，不重复捕获
- [ ] M3-T4.6 CDC 捕获失败降级：`start_capture` 失败（WAL/binlog 未配置）→ 降级为 TTL 过期兜底，告警"CDC capture failed, relying on TTL"
- [ ] M3-T4.7 CDC 事件脱敏失败：敏感字段脱敏失败 → 跳过该事件，告警"CDC event masking failed, skipped"
- [ ] M3-T4.8 单元测试：主库 INSERT → CDC 捕获变更事件 → 分发到下游
- [ ] M3-T4.9 单元测试：`CdcCheckpoint` 断点续传（中断后从断点恢复）
- [ ] M3-T4.10 单元测试：CDC 事件脱敏（敏感字段变更事件脱敏后分发）
- [ ] M3-T4.11 边界测试：CDC 捕获失败 → TTL 兜底降级，告警正确
- [ ] M3-T4.12 性能测试：CDC 同步开销 ≤ 100ms（含变更事件捕获 + 下游分发，单事件）

**验收标准**：CDC 同步协调器正确；复用既有 `DialectCapturer`/`DownstreamSink`/`distribute_to_all`/`CdcCheckpoint` 不重复实现；断点续传 + 脱敏 + 降级正确；性能 ≤ 100ms

**依赖**：M3-T1

## M3-T5：VectorPushdownExecutor 真实向量搜索下推

**任务描述**：实现 `VectorPushdownExecutor` 向量下推执行器，调用既有 `HybridSearcher::search` 执行真实向量检索 + `FilterPushdown` 结构化过滤下推，转正 v4.3.0 POC 的"搜索下推仅记录数据源"。

**涉及文件**：
- `packages/sz-orm-fusion/src/vector_pushdown.rs`（新建，向量下推执行器）
- `packages/sz-orm-fusion/Cargo.toml`（扩展：`db-fusion-v2` 增加 `sz-orm-vector` 依赖）

**复用标注**：既有 `HybridSearcher` `packages/sz-orm-vector/src/hybrid_search/searcher.rs:30`（三源并行查询 + 融合排序）；既有 `HybridSearcher::search` `:51`；既有 `DegradationStatus` `packages/sz-orm-vector/src/hybrid_search/searcher.rs:60`（降级语义）；既有 `FilterPushdown` `packages/sz-orm-vector/src/hybrid_search/pushdown.rs:6`（结构化过滤下推）；既有 POC 搜索下推仅记录数据源 `packages/sz-orm-fusion/src/executor.rs:118`

**子任务**：
- [ ] M3-T5.1 `packages/sz-orm-fusion/Cargo.toml` `db-fusion-v2` feature 增加 `dep:sz-orm-vector` 依赖
- [ ] M3-T5.2 定义 `pub struct VectorPushdownExecutor { searcher: Arc<HybridSearcher> }`（`#[stable]` 标注）
- [ ] M3-T5.3 实现 `impl VectorPushdownExecutor { pub fn new(searcher: Arc<HybridSearcher>) -> Self }`
- [ ] M3-T5.4 实现 `pub async fn execute(&self, query: &FusionQuery) -> Result<HybridSearchResponse, HybridError>`：`FilterPushdown::pushdown_to_vector` 结构化过滤下推 → `HybridSearcher::search` 三源并行查询 → 返回融合排序结果
- [ ] M3-T5.5 向量搜索失败降级：`HybridSearcher::search` 失败（向量库不可用）→ 降级为主库查询（复用 `DegradationStatus` `packages/sz-orm-vector/src/hybrid_search/searcher.rs:60`），标记 `vector_degraded`
- [ ] M3-T5.6 单元测试：融合查询含 `search: 无线耳机` 条件 → 调用 `HybridSearcher::search` 执行真实向量检索，返回融合排序结果
- [ ] M3-T5.7 单元测试：向量搜索失败 → 降级为主库查询 + `vector_degraded` 标记
- [ ] M3-T5.8 边界测试：空 search 条件 → 不触发向量下推，回退主库查询
- [ ] M3-T5.9 性能测试：向量下推开销 ≤ 200ms（含 `HybridSearcher` 三源并行查询 + 结果融合，top_k ≤ 100）

**验收标准**：真实向量搜索下推正确；复用既有 `HybridSearcher`/`FilterPushdown`/`DegradationStatus` 不重复实现；降级处理正确；性能 ≤ 200ms

**依赖**：M3-T1

## M3-T6：降级语义保留 + POC API 废弃标注

**任务描述**：保留既有降级语义（主库失败回退缓存 + 向量失败降级主库 + CDC 失败 TTL 兜底），完善 POC API `#[deprecated]` 标注 + 迁移指引文档。

**涉及文件**：
- `packages/sz-orm-fusion/src/executor.rs`（扩展：降级语义保留）
- `packages/sz-orm-fusion/src/migration.rs`（新建，迁移指引）

**复用标注**：既有 `FusionOutcome.degraded` `packages/sz-orm-fusion/src/executor.rs:55`（降级标记）；既有 `DegradationStatus` `packages/sz-orm-vector/src/hybrid_search/searcher.rs:60`

**子任务**：
- [ ] M3-T6.1 主库失败 + 缓存可读 → 返回缓存旧数据 + `degraded: true`（复用 `FusionOutcome.degraded` `packages/sz-orm-fusion/src/executor.rs:55`）
- [ ] M3-T6.2 TTL 缓存过期 + 主库不可用 → 返回明确错误"cache expired and primary unavailable"，不返回过期脏数据（除非降级模式）
- [ ] M3-T6.3 向量搜索下推失败 → 降级为主库查询 + `vector_degraded` 标记（复用 `DegradationStatus`）
- [ ] M3-T6.4 CDC 同步失败 → 降级为 TTL 过期兜底
- [ ] M3-T6.5 编写迁移指引 `migration.rs`：POC API（`MemoryFusionCache`/`FusionConfig`/`FusionExecutor`）→ 转正 API（`TtlFusionCache`/`CdcSyncCoordinator`/`VectorPushdownExecutor`）迁移步骤
- [ ] M3-T6.6 单元测试：主库失败 + 缓存可读 → 返回缓存旧数据 + `degraded: true`
- [ ] M3-T6.7 单元测试：TTL 过期 + 主库不可用 → 明确错误，不返回脏数据
- [ ] M3-T6.8 边界测试：所有后端失败 → 返回明确错误，不静默返回脏数据

**验收标准**：降级语义保留正确；POC API `#[deprecated]` + 迁移指引完整；不静默返回脏数据

**依赖**：M3-T2、M3-T3、M3-T4、M3-T5

## M3-T7：M3 集成测试与门禁验证

**任务描述**：M3 里程碑集成测试与门禁验证，确保 REQ-V44-003 全部验收条件满足，默认 feature 行为不变。

**子任务**：
- [ ] M3-T7.1 集成测试阶段一：主库 + TTL 缓存 + 失效广播（缓存命中/过期/失效/降级正确）
- [ ] M3-T7.2 集成测试阶段二：主库 CDC → 缓存失效 + 搜索索引更新（CDC 同步生效）
- [ ] M3-T7.3 集成测试阶段二：融合查询含 search 条件 → 真实向量检索（向量下推返回融合结果）
- [ ] M3-T7.4 集成测试：转正 API `#[stable]` + POC API `#[deprecated]` 编译验证
- [ ] M3-T7.5 运行 `cargo test -p sz-orm-fusion --features db-fusion-v2`（全部通过）
- [ ] M3-T7.6 `cargo clippy -p sz-orm-fusion --features db-fusion-v2 -- -D warnings`
- [ ] M3-T7.7 `cargo fmt -p sz-orm-fusion -- --check`
- [ ] M3-T7.8 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-fusion/src/ttl_cache.rs packages/sz-orm-fusion/src/cdc_sync.rs packages/sz-orm-fusion/src/vector_pushdown.rs` 无占位实现
- [ ] M3-T7.9 验证默认 feature 行为与 v4.3.0 一致（`cargo build -p sz-orm-fusion` POC API 保留可用）
- [ ] M3-T7.10 验证 `db-fusion-v2` 与既有 `db-fusion` feature 组合编译通过

**验收标准**：M3 集成测试通过；门禁通过；默认 POC API 保留；TTL 缓存 + 失效广播 + CDC 同步 + 向量下推 + 转正 API + 降级语义全部验证

**依赖**：M3-T1、M3-T2、M3-T3、M3-T4、M3-T5、M3-T6

---

# 六、M4：结构化查询日志（REQ-V44-004，P2，1.5 周）

**目标**：扩展既有 `sz-orm-observability`（`MetricsRegistry` `packages/sz-orm-observability/src/lib.rs:253`），新增 `QueryLogger` 结构化日志器输出 `QueryLogEntry`（JSON 格式含查询 SQL/参数/耗时/阶段/慢标记），复用既有 `sz-orm-masking`（`MaskingRule` `packages/sz-orm-masking/src/lib.rs:21`）参数脱敏，支持采样率与级别控制。
**对应需求**：REQ-V44-004（spec.md §5.4，design.md §2.2.4）
**预期工作量**：1.5 周
**依赖**：无（M4 为 P2 独立需求，复用既有 sz-orm-observability + sz-orm-flamegraph + sz-orm-masking）

## M4-T1：query-logging feature gate + QueryLogger 结构化日志器

**任务描述**：在 `sz-orm-observability` 新增 `query-logging` feature gate，配置依赖（sz-orm-flamegraph + sz-orm-masking），实现 `QueryLogger` 结构化日志器骨架。

**涉及文件**：
- `packages/sz-orm-observability/Cargo.toml`（扩展：新增 `query-logging` feature）
- `packages/sz-orm-observability/src/query_logger.rs`（新建，结构化日志器）

**复用标注**：既有 `MetricsRegistry` `packages/sz-orm-observability/src/lib.rs:253`（Counter/Gauge/Histogram，RwLock 线程安全）；既有 `start_metrics_server` `packages/sz-orm-observability/src/lib.rs:421`（Prometheus HTTP server，保留不动）

**feature gate 隔离**：`query-logging = ["dep:sz-orm-flamegraph", "dep:sz-orm-masking"]`，默认关闭

**子任务**：
- [ ] M4-T1.1 `packages/sz-orm-observability/Cargo.toml` 新增 `query-logging = ["dep:sz-orm-flamegraph", "dep:sz-orm-masking"]` feature，默认关闭
- [ ] M4-T1.2 新增 `sz-orm-flamegraph`（optional）+ `sz-orm-masking`（optional）依赖
- [ ] M4-T1.3 定义 `pub enum LogLevel { Debug, Info, Warn }`（日志级别枚举）
- [ ] M4-T1.4 定义 `pub struct QueryLogger { sample_rate: f64, level: LogLevel, registry: Option<Arc<MetricsRegistry>> }`（结构化日志器）
- [ ] M4-T1.5 实现 `impl QueryLogger { pub fn new() -> Self }`（默认 sample_rate = 0.01, level = Info）
- [ ] M4-T1.6 实现 `impl QueryLogger { pub fn with_sample_rate(mut self, rate: f64) -> Self pub fn with_level(mut self, level: LogLevel) -> Self pub fn with_registry(mut self, registry: Arc<MetricsRegistry>) -> Self }`（配置方法）
- [ ] M4-T1.7 验证 `cargo check -p sz-orm-observability --features query-logging` 编译通过
- [ ] M4-T1.8 验证默认 `cargo check -p sz-orm-observability` 行为不变（既有 Prometheus 指标保留不动）

**验收标准**：`query-logging` feature gate 定义；`QueryLogger` 骨架搭建；既有 `MetricsRegistry`/`start_metrics_server` 保留不动

**依赖**：无

## M4-T2：QueryLogEntry 日志结构 + JSON 输出

**任务描述**：定义 `QueryLogEntry` 日志结构（JSON 格式含 query_key/sql/params/耗时/阶段/慢标记），实现 JSON 序列化输出，复用既有 `QueryPhaseTiming` 阶段耗时。

**涉及文件**：
- `packages/sz-orm-observability/src/query_logger.rs`（扩展：`QueryLogEntry` + JSON 输出）

**复用标注**：既有 `QueryPhaseTiming` `packages/sz-orm-flamegraph/src/collector.rs:39`（phase + start_ms + duration_ms，`phase_breakdown` 复用）

**子任务**：
- [ ] M4-T2.1 定义 `pub struct QueryLogEntry { pub query_key: String, pub sql: String, pub params: Vec<String>, pub total_elapsed_ms: u64, pub phase_breakdown: Vec<QueryPhaseTiming>, pub slow: bool, pub from_cache: bool, pub timestamp: String }`（日志结构，`params` 已脱敏，`phase_breakdown` 复用既有 `QueryPhaseTiming`）
- [ ] M4-T2.2 实现 `impl QueryLogEntry { pub fn to_json(&self) -> String }`：JSON 序列化
- [ ] M4-T2.3 实现 `impl QueryLogger { pub fn log(&self, entry: QueryLogEntry) }`：日志输出入口
- [ ] M4-T2.4 单元测试：`QueryLogEntry` JSON 格式含 query_key/sql/params/耗时/阶段/慢标记/from_cache/timestamp
- [ ] M4-T2.5 单元测试：`to_json` 输出可被 `serde_json::from_str` 解析，字段完整
- [ ] M4-T2.6 边界测试：未启用 `query-flamegraph` → `phase_breakdown` 为空，仍输出日志（仅总耗时）

**验收标准**：`QueryLogEntry` 结构完整；JSON 输出正确；复用既有 `QueryPhaseTiming`；未启用火焰图时 `phase_breakdown` 为空

**依赖**：M4-T1

## M4-T3：采样率与级别控制

**任务描述**：实现采样率配置（默认 1%，慢查询 100% 采样）与日志级别配置（DEBUG 含 SQL/参数，INFO 仅含统计，WARN 仅含慢查询）。

**涉及文件**：
- `packages/sz-orm-observability/src/query_logger.rs`（扩展：采样与级别控制）

**子任务**：
- [ ] M4-T3.1 实现采样判定：`slow == true` → 100% 采样（必输出）；`slow == false` → 按 `sample_rate` 概率采样
- [ ] M4-T3.2 实现级别过滤：`level == Warn` 且 `slow == false` → 不输出；`level == Info` → 仅含统计（不含 SQL/params）；`level == Debug` → 含 SQL/params（已脱敏）
- [ ] M4-T3.3 单元测试：采样率 1% + 100 次非慢查询 → 约 1 条日志（统计验证采样率生效）
- [ ] M4-T3.4 单元测试：慢查询 100% 采样（必输出日志）
- [ ] M4-T3.5 单元测试：级别控制（WARN 仅含慢查询，INFO 仅含统计，DEBUG 含 SQL/参数）
- [ ] M4-T3.6 边界测试：采样率 0.0 → 非慢查询不输出；采样率 1.0 → 全采样
- [ ] M4-T3.7 性能测试：日志写入开销 ≤ 100μs/次（含日志结构构造 + JSON 序列化 + 采样判定）

**验收标准**：采样率与级别控制正确；慢查询 100% 采样；性能 ≤ 100μs/次

**依赖**：M4-T1、M4-T2

## M4-T4：参数脱敏复用 MaskingRule

**任务描述**：复用既有 `sz-orm-masking`（`MaskingRule`）脱敏查询参数，日志 `params` 字段自动脱敏，不暴露生产参数值。

**涉及文件**：
- `packages/sz-orm-observability/src/query_logger.rs`（扩展：参数脱敏）

**复用标注**：既有 `MaskingRule` `packages/sz-orm-masking/src/lib.rs:21`（Phone/Email/IdCard/BankCard/Name/Address/Ip/Imei...）

**子任务**：
- [ ] M4-T4.1 实现 `pub fn mask_params(params: &[String], rules: &[MaskingRule]) -> Vec<String>`：参数脱敏（复用既有 `MaskingRule`）
- [ ] M4-T4.2 `QueryLogger::log` 流程：`mask_params` 脱敏 params → 采样判定 → 级别过滤 → JSON 输出
- [ ] M4-T4.3 脱敏失败处理：参数脱敏规则匹配失败 → 参数替换为 `[masking failed]`，不暴露原始值
- [ ] M4-T4.4 单元测试：查询参数含手机号 13800138000 → 日志 `params` 脱敏为 138****8000
- [ ] M4-T4.5 单元测试：查询参数含邮箱 → 日志 `params` 脱敏为 `a***@example.com`
- [ ] M4-T4.6 边界测试：脱敏失败 → `params` 为 `[masking failed]`，不暴露原始值

**验收标准**：参数脱敏正确；复用既有 `MaskingRule` 不重复实现；脱敏失败不暴露原始值

**依赖**：M4-T1、M4-T2

## M4-T5：M4 集成测试与门禁验证

**任务描述**：M4 里程碑集成测试与门禁验证，确保 REQ-V44-004 全部验收条件满足，默认 feature 行为不变。

**子任务**：
- [ ] M4-T5.1 集成测试：`QueryLogger::log` 完整流程（查询执行 → 参数脱敏 → 采样判定 → 级别过滤 → JSON 输出）
- [ ] M4-T5.2 集成测试：复用既有 `QueryPhaseTiming` `packages/sz-orm-flamegraph/src/collector.rs:39` 阶段耗时 + `MaskingRule` `packages/sz-orm-masking/src/lib.rs:21` 参数脱敏
- [ ] M4-T5.3 运行 `cargo test -p sz-orm-observability --features query-logging`（全部通过）
- [ ] M4-T5.4 `cargo clippy -p sz-orm-observability --features query-logging -- -D warnings`
- [ ] M4-T5.5 `cargo fmt -p sz-orm-observability -- --check`
- [ ] M4-T5.6 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-observability/src/query_logger.rs` 无占位实现
- [ ] M4-T5.7 验证默认 feature 行为与 v4.3.0 一致（`cargo build -p sz-orm-observability` 无结构化日志，既有 Prometheus 指标保留）
- [ ] M4-T5.8 验证 `query-logging` 与既有 feature（`query-flamegraph`）组合编译通过

**验收标准**：M4 集成测试通过；门禁通过；默认行为不变；`QueryLogEntry` JSON + 采样率/级别控制 + 参数脱敏 + 复用既有 QueryPhaseTiming/MaskingRule 全部验证

**依赖**：M4-T1、M4-T2、M4-T3、M4-T4

---

# 七、M5：性能回归基准线 + CI 自动比对（REQ-V44-005，P2，1.5 周）

**目标**：扩展既有 `sz-orm-explain`（`PlanSnapshot` `packages/sz-orm-explain/src/regression.rs:23` + `check_regressions` `packages/sz-orm-explain/src/regression.rs:161`），新增 `PerfBaseline`（各阶段耗时基线 + 执行计划基线）+ `PerfRegression`（PhaseSlowdown/TotalSlowdown/PlanRegression）+ `check_perf_regressions` CI 入口，复用既有 `QueryPhaseTiming`。
**对应需求**：REQ-V44-005（spec.md §5.5，design.md §2.2.5）
**预期工作量**：1.5 周
**依赖**：无（M5 为 P2 独立需求，复用既有 sz-orm-explain + sz-orm-flamegraph）

## M5-T1：perf-baseline feature gate + PerfBaseline 耗时基线

**任务描述**：在 `sz-orm-explain` 新增 `perf-baseline` feature gate（依赖既有 `explain-analyzer` + `sz-orm-flamegraph`），实现 `PerfBaseline` 耗时基线结构（各阶段耗时基线 + 执行计划基线），JSON 序列化版本化管理。

**涉及文件**：
- `packages/sz-orm-explain/Cargo.toml`（扩展：新增 `perf-baseline` feature）
- `packages/sz-orm-explain/src/perf_baseline.rs`（新建，耗时基线）

**复用标注**：既有 `PlanSnapshot` `packages/sz-orm-explain/src/regression.rs:23`（query_key + plan + captured_at）；既有 `PlanBaseline` `packages/sz-orm-explain/src/regression.rs:34`（基线集合模式）；既有 `Phase` `packages/sz-orm-flamegraph/src/collector.rs:11`（Build/Bind/PoolAcquire/SqlExecute/ResultMap）；既有 `QueryPhaseTiming` `packages/sz-orm-flamegraph/src/collector.rs:39`

**feature gate 隔离**：`perf-baseline = ["explain-analyzer", "dep:sz-orm-flamegraph"]`，默认关闭，依赖既有 `explain-analyzer`

**子任务**：
- [ ] M5-T1.1 `packages/sz-orm-explain/Cargo.toml` 新增 `perf-baseline = ["explain-analyzer", "dep:sz-orm-flamegraph"]` feature，默认关闭
- [ ] M5-T1.2 新增 `sz-orm-flamegraph`（optional）依赖
- [ ] M5-T1.3 定义 `pub struct PerfBaseline { pub query_key: String, pub phase_baselines: HashMap<Phase, u64>, pub plan: ExplainPlan, pub captured_at: String }`（耗时基线，`phase_baselines` 各阶段耗时基线，`plan` 执行计划基线）
- [ ] M5-T1.4 实现 `impl PerfBaseline { pub fn new(query_key: impl Into<String>, plan: ExplainPlan, timings: &[QueryPhaseTiming]) -> Self }`：从 `QueryPhaseTiming` 采集各阶段耗时基线
- [ ] M5-T1.5 实现 `impl PerfBaseline { pub fn to_json(&self) -> String pub fn from_json(json: &str) -> Result<Self, serde_json::Error> }`：JSON 序列化/反序列化（版本化管理）
- [ ] M5-T1.6 单元测试：`PerfBaseline::new` 从 `QueryPhaseTiming` 采集各阶段耗时基线（Build/Bind/PoolAcquire/SqlExecute/ResultMap）
- [ ] M5-T1.7 单元测试：`to_json`/`from_json` 往返一致（JSON 序列化可版本化管理）
- [ ] M5-T1.8 边界测试：空 `timings` → `phase_baselines` 为空，仍生成基线（仅执行计划）

**验收标准**：`PerfBaseline` 结构完整；JSON 序列化/反序列化正确；复用既有 `PlanSnapshot`/`QueryPhaseTiming`

**依赖**：无

## M5-T2：PerfRegression 性能回归检测

**任务描述**：实现 `PerfRegression` 性能回归检测（PhaseSlowdown/TotalSlowdown/PlanRegression），`PlanRegression` 变体复用既有 `PlanRegression`。

**涉及文件**：
- `packages/sz-orm-explain/src/perf_baseline.rs`（扩展：性能回归检测）

**复用标注**：既有 `PlanRegression` `packages/sz-orm-explain/src/regression.rs:69`（ScanTypeUpgrade/IndexLost/RowsGrowth）；既有 `check_regressions` `packages/sz-orm-explain/src/regression.rs:161`（CI 回归入口）

**子任务**：
- [ ] M5-T2.1 定义 `pub enum PerfRegressionType { PhaseSlowdown, TotalSlowdown, PlanRegression }`（性能回归类型）
- [ ] M5-T2.2 定义 `pub enum PerfRegression { PhaseSlowdown { query_key: String, phase: Phase, before: u64, after: u64 }, TotalSlowdown { query_key: String, before: u64, after: u64 }, PlanRegression(PlanRegression) }`（`PlanRegression` 变体复用既有 `PlanRegression` `packages/sz-orm-explain/src/regression.rs:69`）
- [ ] M5-T2.3 实现耗时比对：`current.phase > baseline.phase * threshold_factor` → `PhaseSlowdown`；`current.total > baseline.total * threshold_factor` → `TotalSlowdown`
- [ ] M5-T2.4 实现执行计划比对：复用既有 `check_regressions` `packages/sz-orm-explain/src/regression.rs:161` → 检出 `PlanRegression`（ScanTypeUpgrade/IndexLost/RowsGrowth）→ 包装为 `PerfRegression::PlanRegression`
- [ ] M5-T2.5 单元测试：基线 Build 5ms，当前 Build 50ms，阈值 5 → 检出 `PhaseSlowdown { phase: Build, before: 5, after: 50 }`
- [ ] M5-T2.6 单元测试：基线总 10ms，当前总 100ms，阈值 5 → 检出 `TotalSlowdown`
- [ ] M5-T2.7 单元测试：基线 IndexRange，当前 FullTable → 检出 `PerfRegression::PlanRegression(ScanTypeUpgrade)`
- [ ] M5-T2.8 单元测试：一次比对同时检出耗时回归与计划回归 → `Vec<PerfRegression>` 含两种

**验收标准**：`PerfRegression` 三种类型检测正确；复用既有 `PlanRegression`/`check_regressions`；一次比对同时检出耗时与计划回归

**依赖**：M5-T1

## M5-T3：check_perf_regressions CI 入口

**任务描述**：实现 `check_perf_regressions` CI 入口，读取基线 JSON 与当前耗时 JSON，返回 `Vec<PerfRegression>`，CI 中检测性能退化并报告（非阻断）。

**涉及文件**：
- `packages/sz-orm-explain/src/perf_baseline.rs`（扩展：CI 入口）

**子任务**：
- [ ] M5-T3.1 实现 `pub fn check_perf_regressions(baseline_json: &str, current_json: &str, threshold_factor: u64) -> Result<Vec<PerfRegression>, serde_json::Error>`：CI 入口
- [ ] M5-T3.2 流程：反序列化 baseline + current → 耗时比对（各阶段 + 总耗时）→ 执行计划比对（复用 `check_regressions`）→ 合并 `Vec<PerfRegression>`
- [ ] M5-T3.3 未启用 `query-flamegraph` 降级：当前耗时缺失 → 仅比对执行计划回归（`PlanRegression`），不比对耗时
- [ ] M5-T3.4 基线文件缺失降级：基线 JSON 不存在 → 跳过比对，告警"baseline file not found, skip perf regression check"
- [ ] M5-T3.5 阈值因子配置不当处理：阈值因子过小（如 1.0）→ 按配置阈值比对，标注建议值（2 保守/3 常规/5 宽松）
- [ ] M5-T3.6 单元测试：CI 流程：基线采集 → 保存 → CI 比对 → 报告性能退化（非阻断）
- [ ] M5-T3.7 边界测试：基线文件缺失 → 跳过比对 + 告警，不 panic
- [ ] M5-T3.8 边界测试：未启用 `query-flamegraph` → 仅比对计划回归，降级标注正确
- [ ] M5-T3.9 性能测试：CI 基线比对开销 ≤ 5 秒（基线查询数 ≤ 1,000）

**验收标准**：CI 入口完整；非阻断；降级处理正确；性能 ≤ 5 秒；复用既有 `check_regressions`

**依赖**：M5-T1、M5-T2

## M5-T4：M5 集成测试与门禁验证

**任务描述**：M5 里程碑集成测试与门禁验证，确保 REQ-V44-005 全部验收条件满足，默认 feature 行为不变。

**子任务**：
- [ ] M5-T4.1 集成测试：`PerfBaseline` 采集 → 保存 JSON → `check_perf_regressions` CI 比对 → 报告 `Vec<PerfRegression>`
- [ ] M5-T4.2 集成测试：复用既有 `PlanSnapshot` `packages/sz-orm-explain/src/regression.rs:23` + `check_regressions` `packages/sz-orm-explain/src/regression.rs:161` + `QueryPhaseTiming` `packages/sz-orm-flamegraph/src/collector.rs:39`
- [ ] M5-T4.3 运行 `cargo test -p sz-orm-explain --features perf-baseline`（全部通过）
- [ ] M5-T4.4 `cargo clippy -p sz-orm-explain --features perf-baseline -- -D warnings`
- [ ] M5-T4.5 `cargo fmt -p sz-orm-explain -- --check`
- [ ] M5-T4.6 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-explain/src/perf_baseline.rs` 无占位实现
- [ ] M5-T4.7 验证默认 feature 行为与 v4.3.0 一致（`cargo build -p sz-orm-explain` 无性能基线，既有 `explain-analyzer` 保留）
- [ ] M5-T4.8 验证 `perf-baseline` 与既有 feature（`explain-analyzer`/`query-flamegraph`）组合编译通过

**验收标准**：M5 集成测试通过；门禁通过；默认行为不变；`PerfBaseline` + `PerfRegression` + `check_perf_regressions` CI 入口 + 复用既有 PlanSnapshot/check_regressions/QueryPhaseTiming 全部验证

**依赖**：M5-T1、M5-T2、M5-T3

---

# 八、M6：查询智能闭环联动（REQ-V44-006，P3，1 周）

**目标**：扩展 `sz-orm-advisor`，新增 `IntelligenceLoop` 闭环协调器将 EXPLAIN 分析（`ExplainPlan` `packages/sz-orm-explain/src/lib.rs:76`）→ 自适应决策（`AdaptiveExecutor::decide` `packages/sz-orm-adaptive/src/executor.rs:157`）→ 火焰图诊断（`QueryTracer::trace_execute` `packages/sz-orm-flamegraph/src/collector.rs:61`）→ 优化建议（REQ-V44-001 `suggest`）四步联动形成闭环，输出 `LoopReport`，任一环节失败降级为独立工作。
**对应需求**：REQ-V44-006（spec.md §5.6，design.md §2.2.6）
**预期工作量**：1 周
**依赖**：M1 + M2 交付后（闭环第四步建议 + 第三步诊断）

## M6-T1：query-intelligence-loop feature gate + IntelligenceLoop 闭环协调器

**任务描述**：在 `sz-orm-advisor` 新增 `query-intelligence-loop` feature gate（依赖既有 `query-advisor` + `sz-orm-flamegraph` + `sz-orm-diagnosis`），实现 `IntelligenceLoop` 闭环协调器骨架。

**涉及文件**：
- `packages/sz-orm-advisor/Cargo.toml`（扩展：新增 `query-intelligence-loop` feature）
- `packages/sz-orm-advisor/src/intelligence_loop.rs`（新建，闭环协调器）

**复用标注**：既有 `ExplainPlan` `packages/sz-orm-explain/src/lib.rs:76`（闭环第一步）；既有 `AdaptiveExecutor::decide` `packages/sz-orm-adaptive/src/executor.rs:157`（闭环第二步）；既有 `QueryTracer::trace_execute` `packages/sz-orm-flamegraph/src/collector.rs:61`（闭环第三步）；REQ-V44-001 `OptimizationAdvisor::suggest`（闭环第四步）；REQ-V44-002 `SlowQueryDiagnoser::diagnose`（闭环第三步诊断）

**feature gate 隔离**：`query-intelligence-loop = ["query-advisor", "dep:sz-orm-flamegraph", "dep:sz-orm-diagnosis"]`，默认关闭，依赖既有 `query-advisor`

**子任务**：
- [ ] M6-T1.1 `packages/sz-orm-advisor/Cargo.toml` 新增 `query-intelligence-loop = ["query-advisor", "dep:sz-orm-flamegraph", "dep:sz-orm-diagnosis"]` feature，默认关闭
- [ ] M6-T1.2 新增 `sz-orm-flamegraph`（optional）+ `sz-orm-diagnosis`（optional）依赖
- [ ] M6-T1.3 定义 `pub struct IntelligenceLoop { advisor: Arc<OptimizationAdvisor>, diagnoser: Option<Arc<SlowQueryDiagnoser>>, executor: Option<Arc<AdaptiveExecutor>> }`（闭环协调器）
- [ ] M6-T1.4 实现 `impl IntelligenceLoop { pub fn new(advisor: Arc<OptimizationAdvisor>) -> Self pub fn with_diagnoser(mut self, d: Arc<SlowQueryDiagnoser>) -> Self pub fn with_executor(mut self, e: Arc<AdaptiveExecutor>) -> Self }`
- [ ] M6-T1.5 验证 `cargo check -p sz-orm-advisor --features query-intelligence-loop` 编译通过（依赖链路打通，含 sz-orm-diagnosis）
- [ ] M6-T1.6 验证默认 `cargo check -p sz-orm-advisor` 行为不变（既有 `query-advisor` 保留）

**验收标准**：`query-intelligence-loop` feature gate 定义；`IntelligenceLoop` 骨架搭建；依赖链路打通

**依赖**：M1-T1、M2-T1（sz-orm-advisor + sz-orm-diagnosis 交付）

## M6-T2：LoopReport 闭环报告 + JSON 输出

**任务描述**：定义 `LoopReport` 闭环报告结构（含四步结果与最终建议），实现 JSON 输出可被 CI 消费。

**涉及文件**：
- `packages/sz-orm-advisor/src/intelligence_loop.rs`（扩展：`LoopReport` + JSON 输出）

**子任务**：
- [ ] M6-T2.1 定义 `pub struct LoopReport { pub query_key: String, pub explain_result: Option<ExplainPlan>, pub adaptive_decision: Option<ExecutionPath>, pub diagnosis_result: Option<DiagnosisReport>, pub suggestions: Vec<OptimizationSuggestion>, pub loop_elapsed_ms: u64 }`（闭环报告，各环节结果为 `Option`，跳过时 None）
- [ ] M6-T2.2 实现 `impl LoopReport { pub fn to_json(&self) -> String }`：JSON 输出（CI 消费）
- [ ] M6-T2.3 单元测试：`LoopReport` JSON 含 query_key/explain_result/adaptive_decision/diagnosis_result/suggestions/loop_elapsed_ms
- [ ] M6-T2.4 单元测试：`to_json` 输出可被 `serde_json::from_str` 解析，字段完整
- [ ] M6-T2.5 边界测试：所有环节跳过（全 None）→ JSON 仍输出，标注各环节 skipped

**验收标准**：`LoopReport` 结构完整；JSON 输出正确；各环节 `Option` 跳过处理

**依赖**：M6-T1

## M6-T3：闭环四步联动 + 降级处理

**任务描述**：实现 `IntelligenceLoop::run_loop` 闭环入口，串联四步（EXPLAIN → 自适应 → 诊断 → 建议），任一环节失败降级跳过不阻断查询。

**涉及文件**：
- `packages/sz-orm-advisor/src/intelligence_loop.rs`（扩展：`run_loop` 闭环入口）

**复用标注**：既有 `ExplainPlan` `packages/sz-orm-explain/src/lib.rs:76`；既有 `AdaptiveExecutor::decide` `packages/sz-orm-adaptive/src/executor.rs:157`；既有 `QueryTracer::trace_execute` `packages/sz-orm-flamegraph/src/collector.rs:61`；REQ-V44-001 `OptimizationAdvisor::suggest`；REQ-V44-002 `SlowQueryDiagnoser::diagnose`

**子任务**：
- [ ] M6-T3.1 实现 `impl IntelligenceLoop { pub async fn run_loop(&self, query_key: &str, query: &Query) -> LoopReport }`：闭环入口
- [ ] M6-T3.2 第一步 EXPLAIN 分析：`explain-analyzer` 启用 → 解析 EXPLAIN → `explain_result = Some(plan)`；失败/未启用 → `explain_result = None`，标注"EXPLAIN step skipped"
- [ ] M6-T3.3 第二步自适应决策：`adaptive-query` 启用且统计充足 → `AdaptiveExecutor::decide` → `adaptive_decision = Some(path)`；否则 `None`，标注"adaptive step skipped"
- [ ] M6-T3.4 第三步火焰图诊断：`query-flamegraph` + `slow-query-diagnosis` 启用且 slow → `QueryTracer::trace_execute` + `SlowQueryDiagnoser::diagnose` → `diagnosis_result = Some(report)`；否则 `None`，标注"diagnosis step skipped"
- [ ] M6-T3.5 第四步优化建议：`OptimizationAdvisor::suggest(explain_result, stats)` → `suggestions = Vec<OptimizationSuggestion>`（汇总前三步）
- [ ] M6-T3.6 闭环总耗时：`loop_elapsed_ms = elapsed`（`Instant::now` 计时）
- [ ] M6-T3.7 任一环节失败降级：环节结果 None，闭环继续，不 panic，不阻断查询
- [ ] M6-T3.8 单元测试：闭环四步全成功 → `LoopReport` 含四步结果（explain_result/adaptive_decision/diagnosis_result/suggestions 均有值）
- [ ] M6-T3.9 单元测试：EXPLAIN 失败 → 跳过第一步，其余继续（explain_result = None，标注"EXPLAIN step skipped"）
- [ ] M6-T3.10 单元测试：自适应统计不足 → 跳过第二步（adaptive_decision = None，标注"adaptive step skipped"）
- [ ] M6-T3.11 单元测试：火焰图未启用 → 跳过第三步（diagnosis_result = None，标注"diagnosis step skipped"）
- [ ] M6-T3.12 集成测试：闭环联动：全表扫描 → 自适应建议分页 → 火焰图诊断 → 优化建议（`LoopReport` 含完整闭环）
- [ ] M6-T3.13 性能测试：闭环总耗时 ≤ 200ms（单查询，含四步联动）

**验收标准**：闭环四步联动正确；任一环节失败降级跳过不阻断；复用既有 `decide`/`trace_execute`/`suggest`/`diagnose`；性能 ≤ 200ms

**依赖**：M6-T1、M6-T2、M1-T3、M2-T3

## M6-T4：M6 集成测试与门禁验证

**任务描述**：M6 里程碑集成测试与门禁验证，确保 REQ-V44-006 全部验收条件满足，默认 feature 行为不变。

**子任务**：
- [ ] M6-T4.1 集成测试：`IntelligenceLoop::run_loop` 完整闭环（EXPLAIN → 自适应 → 诊断 → 建议 → LoopReport → JSON）
- [ ] M6-T4.2 集成测试：复用既有 `ExplainPlan` `packages/sz-orm-explain/src/lib.rs:76` + `AdaptiveExecutor::decide` `packages/sz-orm-adaptive/src/executor.rs:157` + `QueryTracer::trace_execute` `packages/sz-orm-flamegraph/src/collector.rs:61` + REQ-V44-001 `suggest` + REQ-V44-002 `diagnose`
- [ ] M6-T4.3 运行 `cargo test -p sz-orm-advisor --features query-intelligence-loop`（全部通过）
- [ ] M6-T4.4 `cargo clippy -p sz-orm-advisor --features query-intelligence-loop -- -D warnings`
- [ ] M6-T4.5 `cargo fmt -p sz-orm-advisor -- --check`
- [ ] M6-T4.6 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-advisor/src/intelligence_loop.rs` 无占位实现
- [ ] M6-T4.7 验证默认 feature 行为与 v4.3.0 一致（`cargo build -p sz-orm-advisor` 无闭环联动，既有 `query-advisor` 保留）
- [ ] M6-T4.8 验证 `query-intelligence-loop` 与既有 feature（`query-advisor`/`slow-query-diagnosis`/`explain-analyzer`/`adaptive-query`/`query-flamegraph`）组合编译通过

**验收标准**：M6 集成测试通过；门禁通过；默认行为不变；闭环四步联动 + 降级处理 + LoopReport JSON + 复用既有 decide/trace_execute/suggest/diagnose 全部验证

**依赖**：M6-T1、M6-T2、M6-T3

---

# 九、M7：最终验证与文档同步（全局，0.5 周）

**目标**：14 道门禁全量验证 + 文档同步 + 版本号更新 v4.3.0→v4.4.0 + sz-pay 兼容性验证 + feature gate 逐步启用计划。
**对应需求**：全局（覆盖 REQ-V44-001~006 全部验收条件）
**预期工作量**：0.5 周
**依赖**：M0~M6 全部完成

## M7-T1：14 道门禁全量验证

**任务描述**：运行 AGENTS.md 定义的 14 道门禁全量验证，确保 v4.4.0 全部门禁通过，v4.3.0 已验收测试基线不回退。

**子任务**：
- [ ] M7-T1.1 `cargo fmt --all -- --check`（门禁 1，fmt 格式检查）
- [ ] M7-T1.2 `cargo check --workspace --all-targets`（门禁 2，编译检查）
- [ ] M7-T1.3 `cargo clippy --workspace --all-targets -- -D warnings`（门禁 3，clippy 静态分析）
- [ ] M7-T1.4 `cargo test --workspace -j 2 --no-fail-fast`（门禁 4，单元/集成测试，v4.3.0 基线不回退）
- [ ] M7-T1.5 `cargo doc --workspace --no-deps --all-features`（门禁 5，文档构建）
- [ ] M7-T1.6 `cargo audit` + `cargo deny check`（门禁 6，安全审计）
- [ ] M7-T1.7 `cargo test --workspace -- --ignored`（门禁 7，真实服务集成测试）
- [ ] M7-T1.8 扫描占位实现 `grep -rn 'todo!\|unimplemented!\|unreachable!' --include='*.rs'`（门禁 8）
- [ ] M7-T1.9 `scripts/check-sql-injection.ps1`（门禁 9，SQL 注入扫描）
- [ ] M7-T1.10 `cargo check --workspace --all-targets --all-features`（门禁 10，feature 全组合编译）
- [ ] M7-T1.11 `git diff --name-only HEAD`（门禁 11，ADR-0001 上游仓库未修改检查）
- [ ] M7-T1.12 `python scripts/check-doc-consistency.py`（门禁 12，文档与代码一致性检查）
- [ ] M7-T1.13 `bash scripts/audit-verify.sh <审计报告.md>`（门禁 13，审计证据验证）
- [ ] M7-T1.14 `python scripts/check-doc-sync.py --diff HEAD`（门禁 14，文档同步更新检查）

**验收标准**：14 道门禁全部通过；v4.3.0 已验收测试基线（约 7,000 个测试）不回退

**依赖**：M0~M6 全部完成

## M7-T2：文档同步 + 版本号更新 + sz-pay 兼容性验证

**任务描述**：更新版本号 v4.3.0 → v4.4.0，同步更新所有相关文档，验证 sz-pay 兼容性。

**子任务**：
- [ ] M7-T2.1 版本号 v4.3.0 → v4.4.0（`Cargo.toml` workspace.package.version）
- [ ] M7-T2.2 更新 `docs/API-STABILITY.md`（6 个新 feature 接口为 Experimental 等级，db-fusion-v2 转正 API 为 Stable 等级）
- [ ] M7-T2.3 更新 `CHANGELOG.md`（新增 2 包 sz-orm-advisor/sz-orm-diagnosis + 扩展 3 包 sz-orm-fusion/sz-orm-observability/sz-orm-explain + 6 个 feature 列表 + 六项需求验收记录）
- [ ] M7-T2.4 更新 `README.md`（v4.4.0 能力概览：查询自动优化建议 + 慢查询自动诊断 + db-fusion 转正 + 结构化查询日志 + 性能回归基准线 + 查询智能闭环联动）
- [ ] M7-T2.5 更新 `AGENTS.md`（工作空间成员 56 → 58，新增 sz-orm-advisor/sz-orm-diagnosis；版本 4.3.0 → 4.4.0；6 个新 feature gate 列表）
- [ ] M7-T2.6 验证 sz-pay 兼容性（不启用新 feature 行为不变，sz-pay 既有测试套件通过，sz-pay 从 crates.io 拉取 sz-orm-* 6 个包不受影响）
- [ ] M7-T2.7 更新 `docs/评估/2026-08-12_v4.2.0_基线评估.md` → 追加 v4.4.0 章节
- [ ] M7-T2.8 更新 `docs/sz-orm-engineering-practices.md`（如有工程实践变更）

**验收标准**：版本号更新；文档同步；sz-pay 兼容性验证通过

**依赖**：M7-T1

## M7-T3：feature gate 逐步启用计划

**任务描述**：验证 6 个新 feature 与既有 feature（含 v4.2.0 7 个 + v4.3.0 7 个）任意组合编译通过，制定逐步启用计划。

**子任务**：
- [ ] M7-T3.1 每个 feature 独立编译验证（`cargo build --features sz-orm-advisor/query-advisor` 等 6 项）
- [ ] M7-T3.2 feature 组合编译验证（含与 v4.3.0 7 个 feature 的组合，`cargo build --features sz-orm-explain/explain-analyzer,sz-orm-advisor/query-advisor,...`）
- [ ] M7-T3.3 feature 组合编译验证（含与 v4.2.0 7 个 feature 的组合，`cargo build --features sz-orm-dtx/cross-lang-dtx,sz-orm-advisor/query-advisor,...`）
- [ ] M7-T3.4 全 feature 编译验证（`cargo build --workspace --all-features`）
- [ ] M7-T3.5 feature 依赖关系验证：`slow-query-diagnosis` 依赖 `query-advisor`（建议联动）；`query-intelligence-loop` 依赖 `query-advisor` + `slow-query-diagnosis`（闭环联动）；`db-fusion-v2` 依赖 `db-fusion`（POC 扩展）；`perf-baseline` 依赖 `explain-analyzer`（基线扩展）
- [ ] M7-T3.6 制定逐步启用计划（按 P1→P2→P3 优先级，标注每个 feature 的启用条件与风险）

**验收标准**：feature 组合无冲突；全 feature 编译通过；feature 依赖关系正确；逐步启用计划文档化

**依赖**：M7-T1

---

# 十、任务依赖关系

```
M0（P0，文档基线，立即并行）  M0-T1 → M0-T2 → M0-T3

M1（P1，优化建议引擎，独立新包）
                        M1-T1 → M1-T2 → M1-T3 → M1-T4 → M1-T6
                        M1-T5 ──────────────────────────↗

M2（P1，慢查询诊断，M1 交付后）
                        M2-T1 → M2-T2 → M2-T3 → M2-T4 → M2-T5

M3（P1，db-fusion 转正，独立扩展）
                        M3-T1 → M3-T2 → M3-T3 → M3-T6 → M3-T7
                        M3-T4 ──────────────────↗          │
                        M3-T5 ────────────────────────────↗

M4（P2，结构化日志，独立扩展）
                        M4-T1 → M4-T2 → M4-T3 → M4-T5
                        M4-T4 ──────────────────↗

M5（P2，性能基线，独立扩展）
                        M5-T1 → M5-T2 → M5-T3 → M5-T4

M6（P3，闭环联动，M1+M2 交付后）
                        M6-T1 → M6-T2 → M6-T3 → M6-T4

M7（最终验证，全部完成后）
                        M7-T1 ← M1-T6 / M2-T5 / M3-T7 / M4-T5 / M5-T4 / M6-T4 / M0-T3
                        M7-T2 ← M7-T1
                        M7-T3 ← M7-T1
```

**并行说明**：
1. M0/M1/M3/M4/M5 可立即启动（新包或独立扩展，与 v4.3.0 零重叠）
2. M2 在 M1 交付后启动（建议联动依赖 sz-orm-advisor）
3. M6 在 M1+M2 交付后启动（闭环第四步建议 + 第三步诊断）
4. M7 必须最后执行（依赖 M0~M6 全部完成）

---

# 十一、风险与缓解措施

| 风险 ID | 风险描述 | 影响 | 概率 | 缓解措施 | 对应任务 |
|---------|---------|------|------|---------|---------|
| R-001 | 优化建议误报（全表扫描在小表场景合法） | 中（开发者困扰） | 中 | 建议保守性：低置信度标注"需人工确认"，建议仅生成文本不自动执行 DDL，`advisor.row_threshold` 可配 | M1-T3 |
| R-002 | 建议冲突（AddIndex 与 DropIndex 针对同一索引） | 低（建议跳过） | 中 | 按置信度排序，保留高置信度建议，低置信度标注"conflict, skipped" | M1-T3 |
| R-003 | 慢查询诊断根因误判（阶段耗时占比阈值不当） | 中（错误根因） | 中 | 阈值可配（DiagnosisConfig），多根因判定 MixedCause，耗时不符标注 timing mismatch 降低置信度 | M2-T2、M2-T3 |
| R-004 | db-fusion TTL 缓存脏读（TTL 内读到旧数据） | 高（数据不一致） | 中 | TTL + 失效广播双保险，主库写入后 publish 失效消息，TTL 可配（默认 60s），主库失败返回 degraded 标记不静默 | M3-T2、M3-T3、M3-T6 |
| R-005 | db-fusion CDC 同步延迟导致缓存/搜索索引过期 | 中（读到旧数据） | 中 | CDC 失败降级为 TTL 过期兜底，CdcCheckpoint 断点续传，延迟期间回退主库 | M3-T4 |
| R-006 | db-fusion 向量搜索下推失败 | 中（查询失败） | 低 | 降级为主库查询（复用 DegradationStatus），标记 vector_degraded，不返回错误 | M3-T5 |
| R-007 | 结构化查询日志泄露敏感参数 | 高（安全违规） | 低 | 复用既有 MaskingRule 参数脱敏，脱敏失败替换为 `[masking failed]` 不暴露原始值 | M4-T4 |
| R-008 | 结构化查询日志量大影响性能 | 中（性能下降） | 中 | 采样率默认 1%（慢查询 100%），日志写入 ≤ 100μs/次，写入失败静默丢弃不阻断查询 | M4-T3 |
| R-009 | 性能回归基线误报（环境波动导致正常波动被报告） | 中（CI 噪音） | 中 | 阈值因子可配（2 保守/3 常规/5 宽松），CI 非阻断（建议性门禁），报告标注阈值因子 | M5-T3 |
| R-010 | 闭环联动任一环节失败 | 低（降级跳过） | 中 | 任一环节失败降级为独立工作（环节结果 None），不阻断查询，LoopReport 标注 skipped | M6-T3 |
| R-011 | 新增 feature 与 v4.2.0/v4.3.0 feature 组合编译失败 | 高（编译破坏） | 低 | 门禁 10 全组合编译 + feature 依赖关系验证（M7-T3） | M7-T1、M7-T3 |
| R-012 | sz-pay 既有代码因 API 变更破坏 | 高（生产故障） | 低 | 无 Breaking Change，6 个 feature gate 隔离默认关闭，既有公开 API 完全向后兼容，sz-pay 回归测试 | M7-T2 |
| R-013 | db-fusion 转正破坏既有 POC API | 高（编译破坏） | 低 | POC API `#[deprecated]` 标注而非删除，转正 API `#[stable]`，既有 POC API 保留可用 | M3-T1、M3-T6 |
| R-014 | 优化建议自动执行 DDL 导致生产事故 | 高（数据损坏） | 低 | 建议仅生成文本（action 字段），不自动执行 DDL，需人工确认后执行 | M1-T3 |
| R-015 | 闭环联动每次查询触发开销大 | 中（性能下降） | 低 | `query-intelligence-loop` feature 默认关闭，闭环仅可选触发（非每次查询），开销 ≤ 200ms | M6-T3 |

---

# 十二、验收标准汇总

## 12.1 全局验收条件

1. **API 兼容性**：v4.3.0 既有公开 API 完全向后兼容，sz-pay 既有代码不受影响
2. **feature gate 隔离**：6 个新 feature 默认全关闭，默认 feature 行为不变
3. **测试基线不回退**：v4.3.0 已验收测试基线（约 7,000 个测试）不回退，v4.4.0 仅增不减
4. **五方言一致**：优化建议/诊断报告/融合查询按方言能力适配
5. **审计证据**：每项结论附 file:line 证据，`bash scripts/audit-verify.sh` 验证通过
6. **14 道门禁通过**
7. **unsafe 零容忍** / **禁止占位实现** / **复用优先**
8. **与 v4.3.0 零重叠**：v4.3.0 是"采集/检测"层，v4.4.0 是"分析/建议/转正"层

## 12.2 里程碑验收

| 里程碑 | 核心验收点 | 对应需求 |
|--------|-----------|---------|
| M0 | v4.3.0 基线锁定（测试数 + 门禁通过状态 + feature gate 状态）；2 新包骨架创建 | — |
| M1 | 六种建议类型 + 规则引擎 suggest + 复用 ExplainPlan/QueryStats/AI 建议结构 + JSON 报告 + 五方言 DDL | REQ-V44-001 |
| M2 | 五种根因 + 仅慢查询触发 + 建议联动 + 双格式输出 + 复用 QueryPhaseTiming/QueryOutcome.slow | REQ-V44-002 |
| M3 | TTL 缓存 + 失效广播 + CDC 同步 + 向量下推 + 转正 API #[stable]/POC API #[deprecated] + 降级语义 | REQ-V44-003 |
| M4 | QueryLogEntry JSON + 采样率/级别控制 + 参数脱敏 + 复用 QueryPhaseTiming/MaskingRule | REQ-V44-004 |
| M5 | PerfBaseline + PerfRegression + check_perf_regressions CI 入口 + 复用 PlanSnapshot/check_regressions/QueryPhaseTiming | REQ-V44-005 |
| M6 | 闭环四步联动 + 降级处理 + LoopReport JSON + 复用 decide/trace_execute/suggest/diagnose | REQ-V44-006 |
| M7 | 14 道门禁 + 文档同步 + sz-pay 兼容 + feature 组合验证 + 版本号 v4.3.0→v4.4.0 | 全局 |

## 12.3 需求验收对齐

| 需求编号 | 验收标准（spec.md §8） | 对应任务 | 设计章节 |
|---------|----------------------|---------|---------|
| REQ-V44-001 | 规则引擎 + 复用 AI 建议结构 + 复用 should_paginate/should_cache + 六种建议类型 + 低置信度标注 + JSON 报告 + feature gate 隔离 | M1-T1~M1-T6 | design.md §2.2.1 |
| REQ-V44-002 | 根因分析 + 复用 slow 标记 + DiagnosisReport + 建议联动 + 双格式输出 + feature gate 隔离 | M2-T1~M2-T5 | design.md §2.2.2 |
| REQ-V44-003 | TTL 缓存 + 失效广播 + CDC 同步 + 向量下推 + 转正 API #[stable]/POC API #[deprecated] + 降级语义 + feature gate 隔离 | M3-T1~M3-T7 | design.md §2.2.3 |
| REQ-V44-004 | QueryLogEntry JSON + 复用 QueryPhaseTiming + 采样率/级别控制 + 参数脱敏 + feature gate 隔离 | M4-T1~M4-T5 | design.md §2.2.4 |
| REQ-V44-005 | PerfBaseline + PerfRegression + 复用 PlanRegression/check_regressions + 复用 QueryPhaseTiming + CI 入口 + feature gate 隔离 | M5-T1~M5-T4 | design.md §2.2.5 |
| REQ-V44-006 | IntelligenceLoop 四步联动 + LoopReport JSON + 降级非阻断 + feature gate 隔离 | M6-T1~M6-T4 | design.md §2.2.6 |

---

# 十三、实施建议

## 13.1 开发顺序

1. **立即启动（与 v4.3.0 零重叠，可并行）**：
   - M0（0.5 周，文档基线 + 2 新包骨架）
   - M1（2 周，sz-orm-advisor 新包，P1 查询智能核心）
   - M3（3 周，sz-orm-fusion db-fusion-v2 扩展，P1 独立）
   - M4（1.5 周，sz-orm-observability query-logging 扩展，P2 独立）
   - M5（1.5 周，sz-orm-explain perf-baseline 扩展，P2 独立）
2. **M1 交付后启动**：
   - M2（1.5 周，sz-orm-diagnosis 新包，P1 建议联动依赖 sz-orm-advisor）
3. **M1+M2 交付后启动**：
   - M6（1 周，sz-orm-advisor query-intelligence-loop 扩展，P3 闭环联动依赖 suggest + diagnose）
4. **最后**：
   - M7 最终验证（0.5 周，14 道门禁 + 文档 + 版本号 v4.3.0→v4.4.0 + sz-pay 兼容性）

## 13.2 验证节奏

- 每个任务完成后运行 `cargo test -p <package> --features <feature>`
- 每个里程碑末尾运行集成测试与门禁验证（M1-T6/M2-T5/M3-T7/M4-T5/M5-T4/M6-T4）
- M7 运行 14 道门禁全量验证

## 13.3 文档同步

- 每个里程碑完成后更新 `CHANGELOG.md`
- M7-T2 统一更新 `docs/API-STABILITY.md` / `README.md` / `AGENTS.md` / `docs/sz-orm-engineering-practices.md` + 评估文档
- 版本号 v4.3.0 → v4.4.0 在 M7-T2 更新（`Cargo.toml` workspace.package.version）

## 13.4 复用优先清单（file:line 证据）

| 复用点 | 既有代码位置 | 复用任务 | 复用方式 |
|--------|-------------|---------|---------|
| ExplainPlan 执行计划摘要 | `packages/sz-orm-explain/src/lib.rs:76` | M1-T3、M6-T3 | 规则引擎输入，scan_type/table/index/rows 判定 |
| ExplainPlan::missing_index 缺失索引判断 | `packages/sz-orm-explain/src/lib.rs:91` | M1-T3 | AddIndex 建议规则复用 |
| PlanRegression 执行计划回归 | `packages/sz-orm-explain/src/regression.rs:69` | M1-T3、M5-T2 | 回归 → 建议映射 + PerfRegression::PlanRegression 变体 |
| PlanSnapshot 基线快照 | `packages/sz-orm-explain/src/regression.rs:23` | M5-T1 | PerfBaseline 结构参考复用 |
| check_regressions CI 回归入口 | `packages/sz-orm-explain/src/regression.rs:161` | M5-T2、M5-T3 | 执行计划比对复用 |
| QueryStats 运行时统计 | `packages/sz-orm-adaptive/src/stats.rs:11` | M1-T3 | 规则引擎输入，统计决策 |
| QueryStats::should_paginate 大结果集判断 | `packages/sz-orm-adaptive/src/stats.rs:66` | M1-T3 | UsePagination 建议规则复用 |
| QueryStats::should_cache 热点查询判断 | `packages/sz-orm-adaptive/src/stats.rs:73` | M1-T3 | EnableCache 建议规则复用 |
| AdaptiveExecutor::decide 自适应决策 | `packages/sz-orm-adaptive/src/executor.rs:157` | M6-T3 | 闭环第二步复用 |
| QueryOutcome.slow 慢查询标记 | `packages/sz-orm-adaptive/src/executor.rs:116` | M2-T3 | 仅 slow == true 触发诊断 |
| AdaptiveConfig.slow_ms 慢查询阈值 | `packages/sz-orm-adaptive/src/executor.rs:35` | M2-T3 | 阈值复用（默认 100ms） |
| Phase 查询阶段枚举 | `packages/sz-orm-flamegraph/src/collector.rs:11` | M2-T2、M5-T1 | PhaseBreakdown.phase / phase_baselines key 复用 |
| QueryPhaseTiming 阶段耗时 | `packages/sz-orm-flamegraph/src/collector.rs:39` | M2-T2、M4-T2、M5-T1 | 诊断输入 / 日志 phase_breakdown / 基线采集复用 |
| QueryTracer::trace_execute 分阶段计时 | `packages/sz-orm-flamegraph/src/collector.rs:61` | M2-T3、M6-T3 | 采集 timings 供诊断 / 闭环第三步复用 |
| IndexSuggestion AI 索引建议结构 | `packages/sz-orm-ai/src/index_advisor.rs:71` | M1-T4 | to_index_suggestion 转换复用 |
| RewriteSuggestion AI 改写建议结构 | `packages/sz-orm-ai/src/rewrite_advisor.rs:61` | M1-T4 | to_rewrite_suggestion 转换复用 |
| TuningSuggestion AI 调优建议结构 | `packages/sz-orm-ai/src/auto_tuning/mod.rs:71` | M1-T4 | to_tuning_suggestion 转换复用 |
| AutoTuningPipeline AI 离线调优 | `packages/sz-orm-ai/src/auto_tuning/pipeline.rs:15` | M1-T1 | 互补关系（规则引擎无 AI 依赖），保留不动 |
| FusionConfig 融合配置 | `packages/sz-orm-fusion/src/plan.rs:21` | M3-T1 | 转正配置复用 + #[deprecated] 标注 |
| FusionPlanner 融合规划器 | `packages/sz-orm-fusion/src/plan.rs:104` | M3-T2 | 查询拆分复用 |
| FusionCache trait 融合缓存抽象 | `packages/sz-orm-fusion/src/executor.rs:8` | M3-T2 | TtlFusionCache 实现此 trait |
| MemoryFusionCache 内存缓存 | `packages/sz-orm-fusion/src/executor.rs:16` | M3-T1 | POC API #[deprecated] 标注 |
| FusionOutcome.degraded 降级标记 | `packages/sz-orm-fusion/src/executor.rs:55` | M3-T6 | 主库失败降级复用 |
| FusionExecutor 融合执行器 | `packages/sz-orm-fusion/src/executor.rs:63` | M3-T3 | 扩展 with_invalidation_bus |
| FusionExecutor::execute 执行入口 | `packages/sz-orm-fusion/src/executor.rs:93` | M3-T3 | TTL + 失效广播集成到此流程 |
| 搜索下推仅记录数据源（POC） | `packages/sz-orm-fusion/src/executor.rs:118` | M3-T5 | 转正为真实向量检索 |
| InvalidationBus 缓存失效总线 | `packages/sz-orm-core/src/l2_cache.rs:82` | M3-T3 | with_invalidation_bus 注入 |
| LocalInvalidationBus 本地失效总线 | `packages/sz-orm-core/src/l2_cache.rs:93` | M3-T3 | 单实例失效广播 |
| RedisPubSubInvalidationBus Redis 失效总线 | `packages/sz-orm-core/src/dist_cache.rs:41` | M3-T3 | 跨实例 Redis Pub/Sub 失效广播 |
| GossipInvalidationBus Gossip 失效总线 | `packages/sz-orm-core/src/dist_cache.rs:179` | M3-T3 | 跨实例 Gossip 失效广播 |
| DialectCapturer 方言 CDC 捕获器 | `packages/sz-orm-queue/src/cdc/capturer.rs:12` | M3-T4 | CdcSyncCoordinator 复用 |
| DownstreamSink 下游分发 | `packages/sz-orm-queue/src/cdc/downstream.rs:12` | M3-T4 | CDC 下游分发复用 |
| distribute_to_all 并行分发 | `packages/sz-orm-queue/src/cdc/downstream.rs:178` | M3-T4 | CDC 并行分发复用 |
| CdcCheckpoint 断点续传 | `packages/sz-orm-queue/src/cdc/checkpoint.rs` | M3-T4 | CDC 断点续传复用 |
| CDC 变更事件脱敏 | `packages/sz-orm-queue/src/cdc/masking.rs` | M3-T4 | CDC 同步脱敏复用 |
| HybridSearcher 混合搜索器 | `packages/sz-orm-vector/src/hybrid_search/searcher.rs:30` | M3-T5 | VectorPushdownExecutor 调用 |
| DegradationStatus 搜索降级状态 | `packages/sz-orm-vector/src/hybrid_search/searcher.rs:60` | M3-T5、M3-T6 | 向量失败降级复用 |
| FilterPushdown 过滤下推 | `packages/sz-orm-vector/src/hybrid_search/pushdown.rs:6` | M3-T5 | 结构化过滤下推复用 |
| MetricsRegistry 指标注册中心 | `packages/sz-orm-observability/src/lib.rs:253` | M4-T1 | QueryLogger 关联指标 |
| start_metrics_server Prometheus server | `packages/sz-orm-observability/src/lib.rs:421` | M4-T1 | 既有 Prometheus 导出保留不动 |
| MaskingRule 脱敏规则 | `packages/sz-orm-masking/src/lib.rs:21` | M4-T4 | 参数脱敏复用 |
| DbType（28 方言枚举） | `packages/sz-orm-core/src/db_type.rs:11` | M1-T5 | 五方言 DDL 生成 |

---

> 文档结束。本任务规划所有 file:line 证据均来自 2026-08-12 实测（非文档声明），与 v4.3.0 边界清晰（零重叠：v4.3.0 是"采集/检测"层，v4.4.0 是"分析/建议/转正"层），与 `spec.md`（What to build，870 行）+ `design.md`（How to build，1801 行）完全对齐，不增删任务，遵循 AGENTS.md 审计合规铁律。