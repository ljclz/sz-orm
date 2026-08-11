# sz-orm v4.3.0 编码任务规划

> 版本：v4.3.0（编译期 EXPLAIN 分析 + 查询性能火焰图 + N+1 静态检测 + 数据血缘可视化 + 编译期数据治理 + 自适应查询 + 多数据库融合（可选））
> 基线：v4.2.0（跨语言分布式事务 + Go/Java/C++ 绑定 + 可视化 Schema 设计器 + OpenAPI → ORM 反向生成 + WASM 真实数据库连接，40 任务 / 278 子任务 / 9.5 周，7 个 feature gate 隔离，已验收基线）
> 日期：2026-08-12
> 文档定位：编码任务规划（How to execute），对应需求规格 `spec.md`（What to build）+ 技术设计 `design.md`（How to build）+ 开发规划 `development-plan.md`（What/How/When）
> 任务约束：无 Breaking Change（7 个新 feature gate 隔离）+ 优先复用既有能力 + 五方言覆盖 + 每项任务附 file:line 代码证据 + unsafe 零容忍 + 禁止占位实现（todo!/unimplemented!/unreachable!）
> 审计合规铁律：每项任务结论须附真实存在的 file:line 证据，修复后必须运行 `cargo test` 并附输出，禁止未验证即标记 ✅
> 实施顺序：按 design.md §2.1.3 依赖关系，M0（P0 纯文档，立即并行）→ M1 P1（编译期 EXPLAIN + 火焰图，查询智能核心，与 v4.2.0 并行）→ M2 P1（N+1 静态检测，v4.2.0 交付后，cli 同文件规避）→ M3 P2（血缘可视化 + 编译期治理，与 v4.2.0 并行）→ M4 P2（自适应查询，v4.2.0 交付后）→ M5 P3（融合查询，可选/实验，v4.2.0 交付后）+ M6 最终验证
> 与 v4.2.0 零重叠：新增范围全部落在新包（sz-orm-explain/sz-orm-flamegraph/sz-orm-n1-lint/sz-orm-adaptive/sz-orm-fusion）或 v4.2.0 不触碰的既有包（sz-orm-macros db-verify 扩展 / sz-orm-audit lineage-viz 扩展 / sz-orm-core compile-governance 扩展），唯一同文件风险 `cli/src/main.rs` 通过排期规避（M2 在 v4.2.0 交付后启动）

---

# 一、任务总览

## 1.1 里程碑 × 任务数 × 预期工作量

| 里程碑 | 名称 | 对应需求 | 优先级 | 任务数 | 子任务数 | 预期工作量 | 启动时机 |
|--------|------|---------|--------|--------|----------|-----------|---------|
| M0 | 评估文档修正与基线合并 | — | P0 | 3 | 12 | 0.5 周 | 立即（与 v4.2.0 并行） |
| M1 | 查询智能：编译期 EXPLAIN 分析 + 火焰图 | REQ-V43-001 | P1 | 6 | 42 | 2.5 周 | 与 v4.2.0 并行 |
| M2 | N+1 静态检测器 | REQ-V43-002 | P1 | 3 | 18 | 2 周 | v4.2.0 交付后 |
| M3 | 数据治理：血缘可视化 + 编译期治理 | REQ-V43-003 | P2 | 5 | 30 | 2.5 周 | 与 v4.2.0 并行 |
| M4 | 自适应查询优化器 | REQ-V43-004 | P2 | 3 | 18 | 2 周 | v4.2.0 交付后 |
| M5 | 多数据库融合查询（可选/实验） | REQ-V43-005 | P3 | 3 | 16 | 3 周 | v4.2.0 交付后 |
| M6 | 最终验证与文档同步 | 全局 | — | 3 | 16 | 0.5 周 | 全部完成后 |
| **合计** | — | **5 项全覆盖** | — | **26** | **152** | **13 周**（不含 M5 为 10 周） | — |

## 1.2 任务编号约定

- 主任务：`M{里程碑号}-T{任务序号}`（如 M1-T1）
- 子任务：`M{里程碑号}-T{任务序号}.{子任务序号}`（如 M1-T2.1）
- 集成验证任务：每个里程碑末尾固定一个集成测试与门禁验证任务（如 M1-T6）
- 里程碑内需求按 REQ-V43-xxx 序号顺序编排任务

## 1.3 全局约束（适用于所有任务）

1. **feature gate 隔离**：7 个新 feature（`explain-analyzer` / `query-flamegraph` / `n1-lint` / `lineage-viz` / `compile-governance` / `adaptive-query` / `db-fusion`），默认全关闭，默认 feature 行为不变
2. **既有 API 不变**：既有公开 API 签名完全向后兼容，sz-pay 既有代码不受影响（sz-pay 从 crates.io 拉取 sz-orm-* 6 个包）
3. **禁止占位实现**：禁止 `todo!`/`unimplemented!`/`unreachable!`
4. **unsafe 零容忍**：所有新增代码无 `unsafe` 块，或必须有 `// SAFETY:` 注释
5. **五方言覆盖**：MySQL/PostgreSQL/SQLite/Oracle/MSSQL（EXPLAIN 解析/数据治理/融合查询按方言能力适配）
6. **审计证据**：每项任务结论附真实存在的 file:line 证据
7. **测试基线不回退**：v4.2.0 已验收测试基线不回退，v4.3.0 仅增不减
8. **复用优先**：优先复用既有能力，不重复实现（EXPLAIN 解析复用 `query!` 宏 db-verify 模式 `macros/lib.rs:548`；火焰图复用 `Tracer`/`SzTracer` `tracing/lib.rs:129/136`；N+1 检测复用 `N1QueryDetector` `entity_graph.rs:641` 运行时知识；血缘复用 `LineageGraph` `graph.rs:96`；治理复用 `access_control.rs`/`sz-orm-masking` `masking/lib.rs:21`；自适应复用 `plan_cache.rs`/`l1_cache.rs`/`l2_cache.rs`/`paginator.rs`/`cursor_stream.rs`；融合复用 `HybridSearcher` `searcher.rs:30`/`DialectCapturer` `capturer.rs:12`）
9. **Windows MSVC 编译环境**：RUST_MIN_STACK=134217728, CARGO_INCREMENTAL=0
10. **测试命令**：`cargo test --workspace -j 2 --no-fail-fast`；feature 包测试：`cargo test -p <package> --features <feature>`

## 1.4 里程碑依赖关系

```
M0（P0，纯文档，立即并行）
M1（P1，编译期 EXPLAIN + 火焰图，查询智能核心，与 v4.2.0 并行）
  - REQ-V43-001 复用既有 sz-orm-macros db-verify + sz-orm-tracing
  - sz-orm-explain 新包可与 v4.2.0 并行；sz-orm-flamegraph 建议 v4.2.0 M1 后启动避免 merge 冲突
M2（P1，N+1 静态检测，v4.2.0 交付后）
  - REQ-V43-002 复用既有 sz-orm-core/entity_graph + eager_loader
  - 唯一同文件风险 cli/src/main.rs，必须 v4.2.0 交付后启动
M3（P2，血缘可视化 + 编译期治理，与 v4.2.0 并行）
  - REQ-V43-003 复用既有 sz-orm-audit/lineage + sz-orm-core/access_control + sz-orm-masking
M4（P2，自适应查询，v4.2.0 交付后）
  - REQ-V43-004 复用既有 sz-orm-core cursor_stream/paginator/l1/l2_cache
  - 依赖 v4.2.0 后 sz-orm-core 最终形态
M5（P3，融合查询，可选/实验，v4.2.0 交付后）
  - REQ-V43-005 复用既有 sz-orm-vector + sz-orm-queue/cdc
M6（最终验证，全部完成后）
  - 依赖 M0~M5 全部完成
```

> **依赖关系说明**：M0/M1/M3 与 v4.2.0 并行（新包或 v4.2.0 不触碰的包）；M2/M4/M5 在 v4.2.0 交付后启动（M2 与 v4.2.0 同改 `cli/src/main.rs`；M4/M5 依赖 v4.2.0 后 sz-orm-core 最终形态）；M6 必须最后执行。五项需求主体相互独立，REQ-V43-001 与 REQ-V43-002 存在可选协同（查询智能互补，非强依赖），REQ-V43-003 与 REQ-V43-004 存在可选协同（治理标注指导自适应策略，PII 字段不缓存，非强依赖）。

## 1.5 feature gate 定义与测试命令

| feature gate | 所属包 | 依赖 | 测试命令 | 默认 |
|-------------|--------|------|---------|------|
| `explain-analyzer` | sz-orm-macros + sz-orm-explain（新包） | serde_json（可选）+ sz-orm-core | `cargo test -p sz-orm-explain --features explain-analyzer` | 关闭 |
| `query-flamegraph` | sz-orm-flamegraph（新包） | sz-orm-tracing | `cargo test -p sz-orm-flamegraph --features query-flamegraph` | 关闭 |
| `n1-lint` | sz-orm-n1-lint（新包） | syn 2.0（full）+ quote/proc-macro2 + sz-orm-core + serde_json | `cargo test -p sz-orm-n1-lint --features n1-lint` | 关闭 |
| `lineage-viz` | sz-orm-audit | 无新增（内部扩展） | `cargo test -p sz-orm-audit --features lineage-viz` | 关闭 |
| `compile-governance` | sz-orm-core + sz-orm-macros | sz-orm-masking | `cargo test -p sz-orm-core --features compile-governance` | 关闭 |
| `adaptive-query` | sz-orm-adaptive（新包） | sz-orm-core + sz-orm-observability | `cargo test -p sz-orm-adaptive --features adaptive-query` | 关闭 |
| `db-fusion` | sz-orm-fusion（新包，可选） | sz-orm-core + sz-orm-vector + sz-orm-queue | `cargo test -p sz-orm-fusion --features db-fusion` | 关闭 |

---

# 二、M0：评估文档修正与基线合并（P0，0.5 周）

**目标**：修正 `docs/评估/` 8 份文档中的不实数字（2026-08-12 实测），合并为一份 v4.2.0 基线评估报告。纯文档任务，与 v4.2.0 完全并行。
**对应需求**：—（文档基线修正，非功能需求）
**预期工作量**：0.5 周
**依赖**：无（纯文档，立即并行）

## M0-T1：评估文档不实数字修正

**任务描述**：删除 `docs/评估/` 8 份文档中虚构的 per-feature 测试数、"88 个 ZST 结构"、"crates.io 仅发布 1/46 包"等不实声明，替换为 2026-08-12 实测值。

**涉及文件**：`docs/评估/*.md`（8 份）

**子任务**：
- [ ] M0-T1.1 删除各文档虚构的 per-feature 测试数（LlmRouter 213 → 实际 23 等），替换为实测值
- [ ] M0-T1.2 删除"88 个 ZST 结构"声明，替换为 typed_ast.rs 实测 24 个结构体
- [ ] M0-T1.3 修正"crates.io 仅发布 1/46 包"为实测 20+ 包已发布（含 1.x~3.x 版本线）
- [ ] M0-T1.4 修正 LOC 为实测 291,349（全部）/ 223,979（src/）
- [ ] M0-T1.5 统一测试数口径（`#[test]` 6,794 + `#[tokio::test]` 781 + 集成 1,320）
- [ ] M0-T1.6 修正成员数 46 → 50（含新增 cabi/go/java/cpp 4 包）
- [ ] M0-T1.7 标注"编译期执行计划分析 ❌ 无"为错误结论（db-verify 已支持 EXPLAIN，`packages/sz-orm-macros/src/lib.rs:548`）
- [ ] M0-T1.8 标注评估对象版本（v3.9.0/v4.0.0）与当前代码版本（v4.1.0）的差异

**验收标准**：8 份评估文档中虚构数字全部替换为实测值，每项修正附 file:line 证据

**依赖**：无

## M0-T2：v4.2.0 基线评估报告合并

**任务描述**：将 8 份评估报告的修正后结论合并为一份 v4.2.0 基线评估报告，作为 v4.3.0 开发的基准。

**涉及文件**：`docs/评估/2026-08-12_v4.2.0_基线评估.md`（新建）

**子任务**：
- [ ] M0-T2.1 汇总 8 份评估的修正后结论为一张事实表
- [ ] M0-T2.2 补充 v4.2.0 已验收的 5 项能力事实（引用 `docs/spec/v4.2.0/` 证据）
- [ ] M0-T2.3 明确"排除方向"及理由（编译期数据库/类型级验证：需重构核心架构）
- [ ] M0-T2.4 明确"本规划方向"与代码基础映射（见本规划各章）

**验收标准**：基线评估报告包含事实表 + v4.2.0 验收证据 + 排除方向 + 规划映射

**依赖**：M0-T1

## M0-T3：基线验证

**任务描述**：运行文档一致性、审计证据、文档同步三道门禁，验证基线评估报告可被工具消费。

**涉及文件**：`scripts/check-doc-consistency.py`、`scripts/audit-verify.sh`、`scripts/check-doc-sync.py`

**子任务**：
- [ ] M0-T3.1 运行 `python scripts/check-doc-consistency.py`（门禁 12），验证文档与代码一致
- [ ] M0-T3.2 运行 `bash scripts/audit-verify.sh docs/评估/2026-08-12_v4.2.0_基线评估.md`（门禁 13），验证报告中所有 file:line 引用真实存在
- [ ] M0-T3.3 运行 `python scripts/check-doc-sync.py --diff HEAD`（门禁 14），验证文档与 HEAD 同步

**验收标准**：三道门禁全部通过；基线评估报告所有 file:line 引用经 audit-verify 验证真实存在

**依赖**：M0-T2

---

# 三、M1：查询智能 — 编译期 EXPLAIN 分析 + 火焰图（REQ-V43-001，P1，2.5 周）

**目标**：把"查询优化前移到开发期"。既有 `query!` 宏已支持编译期连真 DB 执行 EXPLAIN（`packages/sz-orm-macros/src/lib.rs:546-588`），本里程碑在其上扩展：5 方言 EXPLAIN 结果解析 → 全表扫描/缺失索引编译警告 → CI 执行计划回归检测；并新增查询性能火焰图（复用 `Tracer`/`SzTracer`，`packages/sz-orm-tracing/src/lib.rs:129/136`）。
**对应需求**：REQ-V43-001（spec.md §5.1，design.md §2.2.1）
**预期工作量**：2.5 周
**依赖**：无（M1 为 P1 独立需求，复用既有 sz-orm-macros db-verify + sz-orm-tracing；sz-orm-explain 新包可与 v4.2.0 并行，sz-orm-flamegraph 建议 v4.2.0 M1 后启动避免 merge 冲突）

## M1-T1：sz-orm-explain 包搭建 + explain-analyzer feature gate

**任务描述**：新增 `sz-orm-explain` 包（EXPLAIN 结果解析器，5 方言），`explain-analyzer` feature gate 隔离，作为编译期 EXPLAIN 分析的基础设施。

**涉及文件**：
- `packages/sz-orm-explain/Cargo.toml`（新建，依赖 sz-orm-core + serde_json）
- `packages/sz-orm-explain/src/lib.rs`（新建）
- `packages/sz-orm-explain/src/dialect.rs`（新建，方言分派）
- `Cargo.toml`（workspace.members 新增）

**复用标注**：既有 28 方言枚举 `packages/sz-orm-core/src/db_type.rs:11`；`validate_sql_content`（`packages/sz-orm-macros/src/lib.rs:298`）模式参考

**feature gate 隔离**：`explain-analyzer = ["dep:serde_json"]`，默认关闭

**子任务**：
- [ ] M1-T1.1 创建 `sz-orm-explain` 包，workspace.members 注册
- [ ] M1-T1.2 `[features] explain-analyzer = ["dep:serde_json"]`，默认关闭
- [ ] M1-T1.3 定义 `pub struct ExplainPlan { pub scan_type: ScanType, pub table: String, pub index: Option<String>, pub rows: u64, pub extra: Vec<String> }`（`pub enum ScanType { FullTable, IndexRange, IndexScan, UniqueLookup, Other }`）
- [ ] M1-T1.4 验证 `cargo check -p sz-orm-explain` 编译通过，默认 feature 行为不变

**验收标准**：包创建成功，feature gate 默认关闭，workspace 集成编译通过

**依赖**：无（基础设施任务）

## M1-T2：5 方言 EXPLAIN 解析器

**任务描述**：实现 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 的 EXPLAIN 输出解析为统一 `ExplainPlan`，提供 `ExplainParser` trait 与 `parser_for(dialect)` 方言分派。

**涉及文件**：
- `packages/sz-orm-explain/src/parsers/mod.rs`（新建）
- `packages/sz-orm-explain/src/parsers/mysql.rs` / `postgres.rs` / `sqlite.rs` / `oracle.rs` / `mssql.rs`（新建）

**复用标注**：各方言 EXPLAIN 语句构造 `packages/sz-orm-macros/src/lib.rs:642-647`（MySQL/PostgreSQL `EXPLAIN`、SQLite `EXPLAIN QUERY PLAN`、Oracle `EXPLAIN PLAN FOR`、MSSQL `SET SHOWPLAN_TEXT`）

**子任务**：
- [ ] M1-T2.1 定义 `pub trait ExplainParser: Send + Sync { fn parse(&self, raw: &str) -> Result<ExplainPlan, ExplainError>; }`
- [ ] M1-T2.2 MySQL 解析（EXPLAIN 表格行：`type=ALL` → `FullTable`，`key` 为空 → 缺失索引）
- [ ] M1-T2.3 PostgreSQL 解析（EXPLAIN VERBOSE：`Seq Scan` → `FullTable`，`Index Scan` → `IndexRange`）
- [ ] M1-T2.4 SQLite 解析（EXPLAIN QUERY PLAN：`SCAN` → `FullTable`，`SEARCH` → `IndexRange`）
- [ ] M1-T2.5 Oracle 解析（EXPLAIN PLAN TABLE 行：`OPERATION TABLE ACCESS FULL` → `FullTable`）
- [ ] M1-T2.6 MSSQL 解析（SET SHOWPLAN_ALL 行：`Table Scan` → `FullTable`，`Index Seek` → `IndexRange`）
- [ ] M1-T2.7 定义 `pub fn parser_for(dialect: Dialect) -> Box<dyn ExplainParser>` 方言分派
- [ ] M1-T2.8 定义 `pub enum ExplainError { Unparseable { reason: String }, UnsupportedDialect }`
- [ ] M1-T2.9 单元测试：5 方言各提供真实 EXPLAIN 样例输出，解析为正确的 `ScanType`/`index`/`rows`
- [ ] M1-T2.10 边界测试：非 SQL 输入返回 `Unparseable`，不 panic

**验收标准**：5 方言解析器 + 方言分派；真实样例解析测试通过；`cargo test -p sz-orm-explain --features explain-analyzer` 全部通过

**依赖**：M1-T1

## M1-T3：query! 宏扩展 — 编译期性能警告

**任务描述**：扩展 `sz-orm-macros` 的 `query!` 宏（`packages/sz-orm-macros/src/lib.rs:435-588`），在 db-verify 模式下调用 sz-orm-explain 解析 EXPLAIN 结果，检测全表扫描/缺失索引并输出编译期警告（非阻断）。

**涉及文件**：
- `packages/sz-orm-macros/src/lib.rs`（修改：`verify_with_real_db` 函数扩展，`:546-588` 区域）
- `packages/sz-orm-macros/Cargo.toml`（`db-verify` feature 增加 `sz-orm-explain` 依赖）

**复用标注**：既有 db-verify EXPLAIN 执行链路 `packages/sz-orm-macros/src/lib.rs:546-588`；`SZ_ORM_QUERY_VERIFY` 环境变量开关 `:548`；各方言 EXPLAIN 语句构造 `:642-647`

**feature gate 隔离**：`db-verify` feature 增加可选依赖 `sz-orm-explain`，默认模式（非 db-verify）零改动

**子任务**：
- [ ] M1-T3.1 `db-verify` feature 增加可选依赖 `sz-orm-explain`（`packages/sz-orm-macros/Cargo.toml`）
- [ ] M1-T3.2 在 db-verify EXPLAIN 执行后（`packages/sz-orm-macros/src/lib.rs:546` 区域），调用 `sz-orm-explain` 解析结果
- [ ] M1-T3.3 `ScanType::FullTable` → `proc_macro::Span::warning("full table scan detected, consider adding index")`
- [ ] M1-T3.4 `index == None` 且 rows 超过阈值（可配 `SZ_ORM_EXPLAIN_ROW_THRESHOLD`，默认 1000）→ 缺失索引警告
- [ ] M1-T3.5 警告不阻断编译（仅 warning），解析失败降级为无警告（返回 `ExplainError::Unparseable` 不 panic）
- [ ] M1-T3.6 单元测试：mock EXPLAIN 输出（Seq Scan），编译期输出 warning 文案
- [ ] M1-T3.7 集成验证：`SZ_ORM_QUERY_VERIFY=1 DATABASE_URL=mysql://root:test123@127.0.0.1:3306/sz_orm_test` 连真库编译，验证全表扫描警告触发

**验收标准**：`query!` 宏在 db-verify 模式下输出编译期性能警告（全表扫描/缺失索引）；默认模式行为不变；警告不阻断编译；解析失败降级无警告

**依赖**：M1-T1、M1-T2

## M1-T4：执行计划回归检测（CI 集成）

**任务描述**：提供 `ExplainPlan` 基线快照与回归对比，CI 中检测查询性能退化（如 IndexRange 变为 FullTable、索引丢失、扫描行数增长）。

**涉及文件**：
- `packages/sz-orm-explain/src/regression.rs`（新建）
- `packages/sz-orm-explain/src/lib.rs`（导出 regression 模块）

**子任务**：
- [ ] M1-T4.1 定义 `pub struct PlanSnapshot { pub query_key: String, pub plan: ExplainPlan, pub captured_at: String }`
- [ ] M1-T4.2 实现 `PlanSnapshot::to_json()` / `from_json()`（基线文件格式，JSON 序列化可版本化管理）
- [ ] M1-T4.3 实现 `pub fn compare(baseline: &ExplainPlan, current: &ExplainPlan) -> Vec<PlanRegression>`（`pub enum PlanRegression { ScanTypeUpgrade { .. }, IndexLost { .. }, RowsGrowth { before: u64, after: u64 } }`）
- [ ] M1-T4.4 实现 `pub fn check_regressions(baseline_path: &str, current: &str) -> Result<Vec<PlanRegression>, ExplainError>`：CI 入口
- [ ] M1-T4.5 单元测试：IndexRange → FullTable 检出 `ScanTypeUpgrade`；索引丢失检出 `IndexLost`；扫描行数增长检出 `RowsGrowth`
- [ ] M1-T4.6 集成测试：生成基线快照 → 修改查询 → 对比检出回归

**验收标准**：基线快照 + 回归对比 + CI 入口；检出逻辑测试通过；JSON 序列化可版本化管理

**依赖**：M1-T2

## M1-T5：查询性能火焰图（sz-orm-flamegraph 包）

**任务描述**：新增 `sz-orm-flamegraph` 包，采集查询各阶段耗时（查询构造/参数绑定/连接池获取/SQL 执行/结果映射），生成 Brendan Gregg 格式火焰图 + SVG。

**涉及文件**：
- `packages/sz-orm-flamegraph/Cargo.toml`（新建）
- `packages/sz-orm-flamegraph/src/lib.rs`（新建）
- `packages/sz-orm-flamegraph/src/collector.rs`（新建，span 采集）
- `packages/sz-orm-flamegraph/src/render.rs`（新建，火焰图生成）
- `Cargo.toml`（workspace.members 新增）

**复用标注**：既有 `Tracer` trait `packages/sz-orm-tracing/src/lib.rs:129`、`SzTracer` `:136`（span 关联复用）；既有查询执行链路（`packages/sz-orm-core/src/query.rs:36` QueryBuilder、`packages/sz-orm-core/src/pool.rs:45` Connection）

**feature gate 隔离**：`query-flamegraph = []`，默认关闭

**子任务**：
- [ ] M1-T5.1 创建 `sz-orm-flamegraph` 包，`[features] query-flamegraph`，默认关闭，workspace.members 注册
- [ ] M1-T5.2 定义 `pub struct QueryPhaseTiming { pub phase: Phase, pub start_ms: u64, pub duration_ms: u64 }`（`pub enum Phase { Build, Bind, PoolAcquire, SqlExecute, ResultMap }`）
- [ ] M1-T5.3 实现 `QueryTracer`：`fn trace_execute<F>(&self, f: F) -> (F::Output, Vec<QueryPhaseTiming>)`，用 `Instant::now()` 分阶段计时
- [ ] M1-T5.4 实现 `QueryTracer::with_tracer(tracer: &dyn Tracer)`：将阶段耗时写入既有 Tracer span（复用 `packages/sz-orm-tracing/src/lib.rs:129`）
- [ ] M1-T5.5 实现 `render::to_brendan_gregg(timings: &[QueryPhaseTiming]) -> String`（`phase;start;duration` 格式，兼容 `flamegraph.pl`）
- [ ] M1-T5.6 实现 `render::to_svg(timings: &[QueryPhaseTiming]) -> String`（内联 SVG 火焰图，无外部依赖）
- [ ] M1-T5.7 单元测试：各阶段计时总和 = 总耗时（误差 < 1ms）
- [ ] M1-T5.8 单元测试：Brendan Gregg 输出格式与 `flamegraph.pl` 兼容（首行 header 正确）
- [ ] M1-T5.9 集成测试：真实查询执行，SVG 输出包含 Build/PoolAcquire/SqlExecute 层

**验收标准**：阶段计时 + 双格式输出；`flamegraph.pl` 兼容性验证；`cargo test -p sz-orm-flamegraph --features query-flamegraph` 通过

**依赖**：无（独立新包）

## M1-T6：M1 集成测试与门禁验证

**任务描述**：M1 里程碑集成测试与门禁验证，确保 REQ-V43-001 全部验收条件满足，默认 feature 行为不变。

**子任务**：
- [ ] M1-T6.1 集成测试：`query!` 宏 + db-verify + 真库（MySQL `mysql://root:test123@127.0.0.1:3306/sz_orm_test` / PostgreSQL `postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`），全表扫描警告触发
- [ ] M1-T6.2 集成测试：PlanSnapshot 基线 → 回归检出完整流程
- [ ] M1-T6.3 运行 `cargo test -p sz-orm-explain --features explain-analyzer` + `cargo test -p sz-orm-flamegraph --features query-flamegraph`
- [ ] M1-T6.4 `cargo clippy -p sz-orm-explain -p sz-orm-flamegraph --features explain-analyzer,query-flamegraph -- -D warnings`
- [ ] M1-T6.5 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-explain/ packages/sz-orm-flamegraph/` 无占位实现
- [ ] M1-T6.6 验证默认 feature 行为与 v4.2.0 一致（不启用新 feature，`cargo build --workspace` 行为不变）

**验收标准**：M1 集成测试通过；clippy/fmt/占位检查通过；默认 feature 行为不变；五方言 EXPLAIN 解析 + 编译期警告 + 计划回归 + 火焰图（Brendan Gregg 兼容）全部验证

**依赖**：M1-T1、M1-T2、M1-T3、M1-T4、M1-T5

---

# 四、M2：N+1 静态检测器（REQ-V43-002，P1，2 周）

**目标**：将 N+1 检测前移到开发期。既有运行时检测器 `N1QueryDetector`（`packages/sz-orm-core/src/entity_graph.rs:641`）已提供检测知识，本里程碑提供**函数级静态检测**：标注函数后，用 `syn` 解析函数体 AST，检测循环内查询调用模式，输出警告/错误。

> 注：评估文档建议 clippy lint 路线（可分析任意调用方代码），但 clippy lint 需发布为 rustc 插件 crate，分发与维护成本高。本规划采用**函数标注宏 + AST 静态分析器**（`syn` 解析），可在现有构建链内工作，无需用户安装额外工具链。

**对应需求**：REQ-V43-002（spec.md §5.2，design.md §2.2.2）
**预期工作量**：2 周
**依赖**：v4.2.0 交付后启动（唯一同文件风险 `cli/src/main.rs`，v4.2.0 M3-T7/M3-T13 与 v4.3.0 M2-T2 均修改此文件，排期规避 merge 冲突）

## M2-T1：sz-orm-n1-lint 包搭建 + n1-lint feature gate

**任务描述**：新增 `sz-orm-n1-lint` 包，提供 `#[detect_n_plus_one]` 函数标注宏 + syn 2.0 AST 分析器，实现三种 N+1 检测模式，复用既有运行时 `N1QueryDetector` 检测知识。

**涉及文件**：
- `packages/sz-orm-n1-lint/Cargo.toml`（新建，依赖 syn/quote/proc-macro2 + sz-orm-core + serde_json）
- `packages/sz-orm-n1-lint/src/lib.rs`（新建）
- `Cargo.toml`（workspace.members 新增）

**复用标注**：既有运行时 N+1 检测器 `packages/sz-orm-core/src/entity_graph.rs:641`（检测知识参考，交叉验证）；既有预加载器 `packages/sz-orm-core/src/eager_loader.rs`（MissingEagerLoadHint 模式替代建议）；既有智能预加载 `packages/sz-orm-core/src/smart_eager_loader.rs`（自动选择预加载策略参考）

**feature gate 隔离**：`n1-lint = ["dep:syn", "dep:serde_json"]`，默认关闭；`syn = { version = "2.0", features = ["full"], optional = true }`

**子任务**：
- [ ] M2-T1.1 创建 `sz-orm-n1-lint` 包（纯 lib + bin，无需 cdylib），workspace.members 注册
- [ ] M2-T1.2 `[features] n1-lint = ["dep:syn", "dep:serde_json"]`，默认关闭；依赖 `syn = { version = "2.0", features = ["full"], optional = true }`
- [ ] M2-T1.3 定义 `#[proc_macro_attribute] pub fn detect_n_plus_one(_attr: TokenStream, item: TokenStream) -> TokenStream`：解析函数体 → 分析 → 生成警告 + 透传原函数
- [ ] M2-T1.4 实现 `fn analyze_fn(ast: &syn::ItemFn) -> Vec<N1Finding>`（`pub struct N1Finding { pub span_line: usize, pub pattern: N1Pattern, pub message: String }`，`pub enum N1Pattern { QueryInLoop, ConditionalQueryInLoop, MissingEagerLoadHint }`）
- [ ] M2-T1.5 检测模式一：`for`/`while` 循环体内出现 `QueryBuilder`/`find_by_*` 调用 → `QueryInLoop`
- [ ] M2-T1.6 检测模式二：循环体内 `if` 分支出现查询调用 → `ConditionalQueryInLoop`
- [ ] M2-T1.7 检测模式三：循环体内出现 `where_in` 可批量替代的单查询（引用 `packages/sz-orm-core/src/eager_loader.rs`/`find_with_related.rs` 既有能力）→ `MissingEagerLoadHint`
- [ ] M2-T1.8 生成 `proc_macro::Span::warning` 或 `compile_error!`（默认 warning，`#![allow(n_plus_one)]` 可抑制）

**验收标准**：标注宏可解析函数体 AST；3 种检测模式单元测试通过；默认 warning 不阻断编译；复用既有 N1QueryDetector 检测知识不重复实现

**依赖**：无

## M2-T2：CLI 集成 + 批量检测

**任务描述**：在 CLI 中集成 `sz-orm n1-lint` 命令，支持批量扫描 .rs 文件（不依赖用户标注），输出 JSON/table 格式可被 CI 消费。

**涉及文件**：
- `cli/src/main.rs`（修改，**必须在 v4.2.0 交付后**，避免与 v4.2.0 M3-T7/M3-T13 同文件冲突）
- `packages/sz-orm-n1-lint/src/batch.rs`（新建）

**子任务**：
- [ ] M2-T2.1 实现 `batch::scan_dir(path: &str) -> Vec<(String, N1Finding)>`：递归扫描 .rs 文件，用 syn 解析全部函数（不依赖用户标注）
- [ ] M2-T2.2 `cli/src/main.rs` 新增 `cmd_n1_lint(path: &str, format: &str)` 命令
- [ ] M2-T2.3 CLI 命令分发新增 `sz-orm n1-lint --path=src --format=table|json`
- [ ] M2-T2.4 JSON 输出格式（`serde_json` 序列化 findings，可被 CI 消费）
- [ ] M2-T2.5 单元测试：扫描样例工程（含循环内查询），检出 `QueryInLoop`
- [ ] M2-T2.6 集成测试：`sz-orm n1-lint --path=packages/sz-orm-n1-lint/tests/fixtures` 输出预期 findings

**验收标准**：CLI 命令可用；批量扫描检出循环内查询；JSON 输出可被 CI 消费；`cli/src/main.rs` 修改在 v4.2.0 交付后进行无冲突

**依赖**：M2-T1

## M2-T3：M2 集成测试与门禁验证

**任务描述**：M2 里程碑集成测试与门禁验证，确保静态/运行时检测交叉一致，默认 feature 行为不变。

**子任务**：
- [ ] M2-T3.1 集成测试：既有 `N1QueryDetector`（`packages/sz-orm-core/src/entity_graph.rs:641`）与静态检测结果交叉验证（同一样例代码，运行时检测与静态检测发现一致）
- [ ] M2-T3.2 运行 `cargo test -p sz-orm-n1-lint --features n1-lint`
- [ ] M2-T3.3 `cargo clippy -p sz-orm-n1-lint --features n1-lint -- -D warnings`
- [ ] M2-T3.4 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-n1-lint/` 无占位实现
- [ ] M2-T3.5 验证默认 feature 行为不变（`cargo build --workspace` 行为与 v4.2.0 一致）

**验收标准**：静态/运行时检测交叉一致；门禁通过；默认行为不变；3 种 N+1 模式检测 + CLI 批量扫描 + JSON 输出全部验证

**依赖**：M2-T1、M2-T2

---

# 五、M3：数据治理 — 血缘可视化 + 编译期治理（REQ-V43-003，P2，2.5 周）

**目标**：基于既有 `LineageGraph`（`packages/sz-orm-audit/src/lineage/graph.rs:96`，587 LOC，161 实测测试）扩展可视化与影响分析；基于既有 `access_control.rs` + `sz-orm-masking` 增加编译期数据治理标注。
**对应需求**：REQ-V43-003（spec.md §5.3，design.md §2.2.3）
**预期工作量**：2.5 周
**依赖**：无（M3 为 P2 独立需求，复用既有 sz-orm-audit/lineage + sz-orm-core/access_control + sz-orm-masking，与 v4.2.0 并行）

## M3-T1：lineage-viz feature + 血缘可视化导出

**任务描述**：扩展既有 `sz-orm-audit` 的 `LineageGraph` 导出能力，新增 Mermaid/Graphviz/HTML 三种格式导出，复用既有图结构，既有三导出保留不动。

**涉及文件**：
- `packages/sz-orm-audit/Cargo.toml`（新增 `lineage-viz` feature）
- `packages/sz-orm-audit/src/lineage/export.rs`（扩展）

**复用标注**：既有 `LineageGraph` `packages/sz-orm-audit/src/lineage/graph.rs:96`（图结构复用，节点/边）；既有 `LineageExporter` `packages/sz-orm-audit/src/lineage/export.rs`（`:12` 区域）；既有 `export_dot` `:34`/`export_json` `:68`/`export_graphml` `:105` 保留不动

**feature gate 隔离**：`lineage-viz = []`，默认关闭，既有三导出保留不动

**子任务**：
- [ ] M3-T1.1 `[features] lineage-viz = []`，默认关闭
- [ ] M3-T1.2 实现 `to_mermaid(&self, graph: &LineageGraph) -> String`（`graph LR` 格式，表/列/服务节点 + 边）
- [ ] M3-T1.3 实现 `to_graphviz(&self, graph: &LineageGraph) -> String`（`digraph` 格式）
- [ ] M3-T1.4 实现 `to_html_report(&self, graph: &LineageGraph) -> String`（内联样式 HTML，无外部依赖）
- [ ] M3-T1.5 单元测试：users→orders→report 链路导出 Mermaid 含 3 节点 2 边
- [ ] M3-T1.6 单元测试：Graphviz 输出可被 `dot` 语法解析（结构断言）；HTML 含内联样式

**验收标准**：3 种格式导出；链路完整性测试通过；`cargo test -p sz-orm-audit --features lineage-viz` 通过；既有 export_dot/export_json/export_graphml 保留不动

**依赖**：无

## M3-T2：血缘影响分析工具

**任务描述**：提供 `downstream_impact`（变更影响范围）与 `upstream_trace`（数据来源追溯）BFS 遍历，深度受限 + 环检测，与迁移 dry-run 联动。

**涉及文件**：`packages/sz-orm-audit/src/lineage/impact.rs`（新建）

**复用标注**：既有 `migrate_dry_run` `packages/sz-orm-core/src/migration_dry_run.rs:94`（影响分析与 dry-run 联动，DROP 前输出受影响链路）

**子任务**：
- [ ] M3-T2.1 实现 `pub fn downstream_impact(graph: &LineageGraph, node: &str, depth: usize) -> Vec<ImpactEdge>`（变更影响范围，BFS 深度受限）
- [ ] M3-T2.2 实现 `pub fn upstream_trace(graph: &LineageGraph, node: &str, depth: usize) -> Vec<ImpactEdge>`（数据来源追溯）
- [ ] M3-T2.3 定义 `pub struct ImpactEdge { pub from: String, pub to: String, pub via: String }`
- [ ] M3-T2.4 单元测试：删除 users 表，`downstream_impact` 检出 orders/报表服务受影响
- [ ] M3-T2.5 单元测试：深度限制（depth=1 只返回直接下游）；环检测（已访问节点不重复遍历，不死循环）
- [ ] M3-T2.6 集成测试：迁移 dry-run（`packages/sz-orm-core/src/migration_dry_run.rs:94`）与血缘影响分析联动，DROP 前输出受影响链路

**验收标准**：影响分析正确（BFS 深度受限 + 环检测）；与迁移 dry-run 联动测试通过

**依赖**：M3-T1

## M3-T3：compile-governance feature + PII 标注强制

**任务描述**：在 `sz-orm-core` 增加 `compile-governance` feature：`#[pii]` 字段标注 + `#[mask(strategy = "...")]` 脱敏策略标注 + 编译期强制（PII 字段必须声明脱敏策略，策略白名单 hash/partial/replace/encrypt）+ 生成运行时治理代码。

**涉及文件**：
- `packages/sz-orm-core/Cargo.toml`（新增 `compile-governance` feature）
- `packages/sz-orm-core/src/governance/mod.rs`（新建）
- `packages/sz-orm-macros/src/lib.rs`（新增 `#[proc_macro_derive(Governed, attributes(pii, mask))]`）

**复用标注**：既有 `access_control.rs`（`packages/sz-orm-core/src/access_control.rs`，ABAC 权限 `AccessRule` `:9`/`AccessContext` `:22`/`RowLevelSecurity` `:85`）；既有 `sz-orm-masking`（脱敏规则 `MaskingRule` `packages/sz-orm-masking/src/lib.rs:21`）；既有 derive 宏模式 `packages/sz-orm-macros/src/lib.rs:2853`

**feature gate 隔离**：`compile-governance = ["dep:sz-orm-masking"]`，默认关闭，治理检查仅在 feature 启用时生效

**子任务**：
- [ ] M3-T3.1 `sz-orm-core` 新增 `compile-governance` feature（依赖 `sz-orm-masking`），默认关闭
- [ ] M3-T3.2 `sz-orm-macros` 新增 `#[proc_macro_derive(Governed, attributes(pii, mask))]`：解析 `#[pii]`/`#[mask(strategy = "...")]` 标注
- [ ] M3-T3.3 编译期检查：`#[pii]` 字段未标注 `#[mask]` → `compile_error!("PII field must declare mask strategy")`
- [ ] M3-T3.4 编译期检查：`#[mask(strategy = "invalid")]` → `compile_error!`（策略白名单：hash/partial/replace/encrypt）
- [ ] M3-T3.5 生成运行时治理代码：`fn pii_fields() -> Vec<&'static str>` + `fn mask_policy(field) -> Option<MaskPolicy>`（复用 `sz-orm-masking` 执行脱敏）
- [ ] M3-T3.6 单元测试：合法标注编译通过；缺 mask 的 PII 字段编译失败
- [ ] M3-T3.7 单元测试：非法 mask 策略编译失败
- [ ] M3-T3.8 集成测试：`Governed` 模型通过既有 `access_control.rs` ABAC 检查后，PII 字段输出自动脱敏

**验收标准**：编译期强制（缺 mask/非法策略 → compile_error）；运行时脱敏复用 sz-orm-masking；默认 feature 行为不变；既有 ABAC/脱敏保留不动

**依赖**：无

## M3-T4：合规报告生成（GDPR/等保清单）

**任务描述**：生成合规报告（PII 字段清单 + 脱敏策略 + 保留策略），JSON 输出可被审计工具消费，报告哈希入既有 `sz-orm-audit` 审计链。

**涉及文件**：`packages/sz-orm-core/src/governance/report.rs`（新建）

**复用标注**：既有 `sz-orm-audit` 审计链（报告哈希入链）

**子任务**：
- [ ] M3-T4.1 实现 `pub fn compliance_report(models: &[&dyn GovernedModel]) -> ComplianceReport`（PII 字段清单 + 脱敏策略 + 保留策略）
- [ ] M3-T4.2 定义 `pub struct ComplianceReport { pub pii_fields: Vec<PiiFieldEntry>, pub retention_days: Option<u32>, pub generated_at: String }`
- [ ] M3-T4.3 实现 JSON 输出（可被审计工具消费）
- [ ] M3-T4.4 单元测试：含 2 个 PII 字段的模型，报告列出字段 + 策略
- [ ] M3-T4.5 集成测试：与既有 `sz-orm-audit` 审计链联动（报告哈希入链）；审计链写入失败时告警 + 报告标注"audit chain write failed"

**验收标准**：合规报告生成正确；JSON 输出；审计联动测试通过；审计链写入失败降级处理

**依赖**：M3-T3

## M3-T5：M3 集成测试与门禁验证

**任务描述**：M3 里程碑集成测试与门禁验证，确保 REQ-V43-003 全部验收条件满足，默认 feature 行为不变。

**子任务**：
- [ ] M3-T5.1 运行 `cargo test -p sz-orm-audit --features lineage-viz` + `cargo test -p sz-orm-core --features compile-governance`
- [ ] M3-T5.2 `cargo clippy` 相关包 feature 组合 `-D warnings`
- [ ] M3-T5.3 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-audit/src/lineage/ packages/sz-orm-core/src/governance/` 无占位实现
- [ ] M3-T5.4 验证默认 feature 行为与 v4.2.0 一致（不启用新 feature，`cargo build --workspace` 行为不变）
- [ ] M3-T5.5 验证 `compile-governance` 与既有 feature 组合编译（`cargo check --workspace --all-targets --all-features`）

**验收标准**：门禁通过；feature 组合编译通过；默认行为不变；Mermaid/Graphviz/HTML 血缘导出 + 影响分析 + PII 编译期强制 + 合规报告全部验证

**依赖**：M3-T1、M3-T2、M3-T3、M3-T4

---

# 六、M4：自适应查询优化器（REQ-V43-004，P2，2 周）

**目标**：运行时统计 + 自动策略切换。既有 `AutoTuningPipeline`（`packages/sz-orm-ai/src/auto_tuning/pipeline.rs:15`，424 LOC）是 AI 离线闭环，本里程碑提供**轻量运行时自适应**（无 AI 依赖）：统计采集（AtomicU64）→ 大结果集自动游标分页 → 热点查询自动缓存 → 慢查询降级。

> 注意：v4.0.0 已存在 `plan_cache.rs` / `l1_cache.rs` / `l2_cache.rs` / `paginator.rs` / `cursor_stream.rs`，本里程碑**只做决策层**，不重写缓存/分页实现。

**对应需求**：REQ-V43-004（spec.md §5.4，design.md §2.2.4）
**预期工作量**：2 周
**依赖**：v4.2.0 交付后启动（依赖 v4.2.0 后 sz-orm-core 最终形态）

## M4-T1：sz-orm-adaptive 包搭建 + adaptive-query feature gate

**任务描述**：新增 `sz-orm-adaptive` 包，提供 `QueryStats`（AtomicU64 无锁统计采集）+ 决策阈值（`should_paginate`/`should_cache`），作为运行时自适应查询的基础设施。

**涉及文件**：
- `packages/sz-orm-adaptive/Cargo.toml`（新建，依赖 sz-orm-core + sz-orm-observability）
- `packages/sz-orm-adaptive/src/lib.rs`（新建）
- `Cargo.toml`（workspace.members 新增）

**feature gate 隔离**：`adaptive-query = ["dep:sz-orm-observability"]`，默认关闭

**子任务**：
- [ ] M4-T1.1 创建 `sz-orm-adaptive` 包，workspace.members 注册
- [ ] M4-T1.2 `[features] adaptive-query = ["dep:sz-orm-observability"]`，默认关闭
- [ ] M4-T1.3 定义 `pub struct QueryStats { total_executions: AtomicU64, total_rows: AtomicU64, total_time_us: AtomicU64 }` + `record(&self, rows: u64, time_us: u64)`（原子操作无锁采集）
- [ ] M4-T1.4 实现 `pub fn should_paginate(&self, threshold_rows: u64) -> bool`（avg_rows > threshold，默认 1000）
- [ ] M4-T1.5 实现 `pub fn should_cache(&self, threshold_ms: u64, min_executions: u64) -> bool`（avg_time > threshold 且执行次数达标）

**验收标准**：统计采集 + 决策阈值；单元测试通过；默认关闭；统计采集开销 < 1μs/次

**依赖**：无

## M4-T2：自适应策略执行器

**任务描述**：实现 `AdaptiveExecutor` 决策执行器，按统计决策选择执行路径（自动游标分页/自动缓存/慢查询降级），复用既有缓存/分页实现，仅做决策层不重写。

**涉及文件**：`packages/sz-orm-adaptive/src/executor.rs`（新建）

**复用标注**：既有游标分页 `packages/sz-orm-core/src/cursor_stream.rs`、分页器 `packages/sz-orm-core/src/paginator.rs`、L1 缓存 `packages/sz-orm-core/src/l1_cache.rs`、L2 缓存 `packages/sz-orm-core/src/l2_cache.rs`、计划缓存 `packages/sz-orm-core/src/plan_cache.rs`；既有 AI 离线调优 `packages/sz-orm-ai/src/auto_tuning/pipeline.rs:15`（互补关系，保留不动）

**子任务**：
- [ ] M4-T2.1 定义 `pub struct AdaptiveExecutor { stats: HashMap<String, QueryStats>, config: AdaptiveConfig }`（`pub struct AdaptiveConfig { pub row_threshold: u64, pub cache_time_ms: u64, pub min_executions: u64 }`）
- [ ] M4-T2.2 实现 `AdaptiveExecutor::execute(&self, query_key: &str, f: impl FnOnce() -> QueryOutcome) -> QueryOutcome`：记录统计 → 按决策选择执行路径
- [ ] M4-T2.3 自动游标分页：`should_paginate` 为真时切换到既有游标分页（复用 `packages/sz-orm-core/src/cursor_stream.rs`），返回分页句柄而非全量
- [ ] M4-T2.4 自动缓存：`should_cache` 为真时结果写入既有 L2 缓存（复用 `packages/sz-orm-core/src/l2_cache.rs`），TTL 按 `cache_time_ms`，默认关闭自动缓存（需显式开启避免脏读）
- [ ] M4-T2.5 慢查询降级：单次执行超时（`timeout_ms` 可配，默认 5000）返回明确超时错误，不静默丢查询，不无限重试
- [ ] M4-T2.6 单元测试：模拟统计增长，验证 `should_paginate` 状态翻转
- [ ] M4-T2.7 单元测试：缓存命中返回缓存结果，统计不重复累加执行时间（仅记录命中次数，避免统计失真）
- [ ] M4-T2.8 性能测试：统计采集开销 < 1μs/次（AtomicU64 原子操作，无锁）

**验收标准**：决策执行器完整；复用既有缓存/分页实现（不重写）；性能测试通过；默认关闭自动缓存避免脏读

**依赖**：M4-T1

## M4-T3：M4 集成测试与门禁验证

**任务描述**：M4 里程碑集成测试与门禁验证，确保 REQ-V43-004 全部验收条件满足，默认 feature 行为不变。

**子任务**：
- [ ] M4-T3.1 集成测试：真实查询（SQLite）执行 N 次后自动切游标分页
- [ ] M4-T3.2 集成测试：热点查询自动缓存，命中返回缓存结果；慢查询超时返回明确错误
- [ ] M4-T3.3 运行 `cargo test -p sz-orm-adaptive --features adaptive-query` + clippy/fmt/占位检查
- [ ] M4-T3.4 验证默认 feature 行为不变（`cargo build --workspace` 行为与 v4.2.0 一致）

**验收标准**：集成测试通过；门禁通过；默认行为不变；统计采集 <1μs + 自动分页/缓存决策 + 慢查询降级全部验证

**依赖**：M4-T1、M4-T2

---

# 七、M5：多数据库融合查询（REQ-V43-005，P3，可选/实验，3 周）

**目标**：透明多数据库操作（MySQL 主库 + Redis 缓存 + 向量库搜索自动拆分/聚合）。评估为"架构可行"（28 方言 + CDC 基础），但价值需验证，故标记为**可选实验**：交付 POC 验证价值后决定是否转正。
**对应需求**：REQ-V43-005（spec.md §5.5，design.md §2.2.5）
**预期工作量**：3 周
**依赖**：v4.2.0 交付后启动（依赖 v4.2.0 后 sz-orm-core 最终形态）

## M5-T1：sz-orm-fusion 包搭建 + db-fusion feature gate

**任务描述**：新增 `sz-orm-fusion` 包，提供 `FusionConfig` 融合配置（主库 + 缓存 + 搜索后端），复用既有 `HybridSearcher` 三源并行模式。

**涉及文件**：
- `packages/sz-orm-fusion/Cargo.toml`（新建，依赖 sz-orm-core + sz-orm-vector + sz-orm-queue）
- `packages/sz-orm-fusion/src/lib.rs`（新建）
- `Cargo.toml`（workspace.members 新增）

**复用标注**：既有 28 方言枚举 `packages/sz-orm-core/src/db_type.rs:11`（FusionConfig primary 方言标识）；既有混合搜索器 `packages/sz-orm-vector/src/hybrid_search/searcher.rs:30`（三源并行查询 + 融合排序模式复用）

**feature gate 隔离**：`db-fusion = ["dep:sz-orm-vector", "dep:sz-orm-queue"]`，默认关闭，标记为可选/实验

**子任务**：
- [ ] M5-T1.1 创建 `sz-orm-fusion` 包，`[features] db-fusion = ["dep:sz-orm-vector", "dep:sz-orm-queue"]`，默认关闭，workspace.members 注册
- [ ] M5-T1.2 定义 `pub struct FusionConfig { pub primary: Dialect, pub cache: Option<CacheBackend>, pub search: Option<SearchBackend> }`
- [ ] M5-T1.3 定义 `pub enum CacheBackend { Redis }` / `pub enum SearchBackend { Vector }`（复用既有 `sz-orm-vector` 混合搜索 `packages/sz-orm-vector/src/hybrid_search/searcher.rs:30`）

**验收标准**：包创建成功；feature gate 默认关闭；FusionConfig 支持主库 + 缓存 + 搜索后端配置

**依赖**：无

## M5-T2：查询拆分与聚合

**任务描述**：实现 `FusionPlanner` 查询拆分器（静态分析 WHERE/排序，识别可下推缓存/搜索子句，仅支持可证明安全的拆分）与 `FusionExecutor` 聚合执行器（主库 + 缓存/搜索并行执行 + 聚合，主库失败回退缓存 + 降级标记）。

**涉及文件**：
- `packages/sz-orm-fusion/src/planner.rs`（新建）
- `packages/sz-orm-fusion/src/executor.rs`（新建）

**复用标注**：既有 `HybridSearcher` `packages/sz-orm-vector/src/hybrid_search/searcher.rs:30`（三源并行模式复用）

**子任务**：
- [ ] M5-T2.1 实现 `FusionPlanner::plan(query: &QueryBuilder<M>) -> FusionPlan`：静态分析 WHERE/排序，识别可下推缓存/搜索子句；实验阶段仅支持安全拆分（主键等值 + 缓存键），不安全拆分回退全主库
- [ ] M5-T2.2 实现 `FusionExecutor::execute(plan: FusionPlan) -> Result<Vec<Row>>`：主库查询 + 缓存/搜索并行执行 + 聚合
- [ ] M5-T2.3 单元测试：`where_eq` + 缓存键命中 → 主库跳过，返回缓存结果
- [ ] M5-T2.4 单元测试：主库失败回退（缓存可读）→ 返回缓存结果 + 降级标记，不静默返回脏数据


**验收标准**：查询拆分/聚合正确；仅安全拆分；主库失败回退 + 降级标记；不静默返回脏数据

**依赖**：M5-T1

## M5-T3：数据同步 + 实验评估

**任务描述**：复用既有 CDC 做主库→缓存/搜索索引同步，编写《db-fusion 实验评估报告》给出转正/废弃建议。

**涉及文件**：
- `packages/sz-orm-fusion/src/sync.rs`（新建）
- `docs/评估/2026-08-12_db-fusion_实验评估报告.md`（新建）

**复用标注**：既有方言 CDC 捕获器 `packages/sz-orm-queue/src/cdc/capturer.rs:12`（`DialectCapturer` trait，各方言 WAL/Binlog/Trigger/LogMiner 捕获，不重复实现 CDC）

**子任务**：
- [ ] M5-T3.1 复用既有 CDC（`packages/sz-orm-queue/src/cdc/capturer.rs:12`）做主库→缓存/搜索索引同步
- [ ] M5-T3.2 编写《db-fusion 实验评估报告》：POC 结果 + 价值判断（转正/废弃建议）+ CDC 同步延迟影响分析
- [ ] M5-T3.3 门禁验证（`cargo test -p sz-orm-fusion --features db-fusion` + clippy/fmt/占位检查）+ 默认 feature 行为不变

**验收标准**：POC 可运行；CDC 同步生效；评估报告给出转正/废弃建议；默认行为不变

**依赖**：M5-T1、M5-T2

---

# 八、M6：最终验证与文档同步（全局，0.5 周）

**目标**：14 道门禁全量验证 + 文档同步 + 版本号更新 + sz-pay 兼容性验证 + feature gate 逐步启用计划。
**对应需求**：全局（覆盖 REQ-V43-001~005 全部验收条件）
**预期工作量**：0.5 周
**依赖**：M0~M5 全部完成

## M6-T1：14 道门禁全量验证

**任务描述**：运行 AGENTS.md 定义的 14 道门禁全量验证，确保 v4.3.0 全部门禁通过，v4.2.0 已验收测试基线不回退。

**子任务**：
- [ ] M6-T1.1 `cargo fmt --all -- --check`（门禁 1，fmt 格式检查）
- [ ] M6-T1.2 `cargo check --workspace --all-targets`（门禁 2，编译检查）
- [ ] M6-T1.3 `cargo clippy --workspace --all-targets -- -D warnings`（门禁 3，clippy 静态分析）
- [ ] M6-T1.4 `cargo test --workspace -j 2 --no-fail-fast`（门禁 4，单元/集成测试，v4.2.0 基线不回退）
- [ ] M6-T1.5 `cargo doc --workspace --no-deps --all-features`（门禁 5，文档构建）
- [ ] M6-T1.6 `cargo audit` + `cargo deny check`（门禁 6，安全审计）
- [ ] M6-T1.7 `cargo test --workspace -- --ignored`（门禁 7，真实服务集成测试）
- [ ] M6-T1.8 扫描占位实现 `grep -rn 'todo!\|unimplemented!\|unreachable!' --include='*.rs'`（门禁 8）
- [ ] M6-T1.9 `scripts/check-sql-injection.ps1`（门禁 9，SQL 注入扫描）
- [ ] M6-T1.10 `cargo check --workspace --all-targets --all-features`（门禁 10，feature 全组合编译）
- [ ] M6-T1.11 `git diff --name-only HEAD`（门禁 11，ADR-0001 上游仓库未修改检查）
- [ ] M6-T1.12 `python scripts/check-doc-consistency.py`（门禁 12，文档与代码一致性检查）
- [ ] M6-T1.13 `bash scripts/audit-verify.sh <审计报告.md>`（门禁 13，审计证据验证）
- [ ] M6-T1.14 `python scripts/check-doc-sync.py --diff HEAD`（门禁 14，文档同步更新检查）

**验收标准**：14 道门禁全部通过；v4.2.0 已验收测试基线不回退

**依赖**：M0~M5 全部完成

## M6-T2：文档同步 + 版本号更新 + sz-pay 兼容性验证

**任务描述**：更新版本号 v4.2.0 → v4.3.0，同步更新所有相关文档，验证 sz-pay 兼容性。

**子任务**：
- [ ] M6-T2.1 版本号 v4.2.0 → v4.3.0（`Cargo.toml` workspace.package.version）
- [ ] M6-T2.2 更新 `docs/API-STABILITY.md`（7 个新 feature 接口为 Experimental 等级）
- [ ] M6-T2.3 更新 `CHANGELOG.md` / `README.md` / `AGENTS.md`（新包 sz-orm-explain/sz-orm-flamegraph/sz-orm-n1-lint/sz-orm-adaptive/sz-orm-fusion + 7 个 feature 列表）
- [ ] M6-T2.4 验证 sz-pay 兼容性（不启用新 feature 行为不变，sz-pay 既有测试套件通过）
- [ ] M6-T2.5 更新 `docs/评估/2026-08-12_v4.2.0_基线评估.md` → 追加 v4.3.0 章节

**验收标准**：版本号更新；文档同步；sz-pay 兼容性验证通过

**依赖**：M6-T1

## M6-T3：feature gate 逐步启用计划

**任务描述**：验证 7 个新 feature 与既有 feature（含 v4.2.0 7 个 feature）任意组合编译通过，制定逐步启用计划。

**子任务**：
- [ ] M6-T3.1 每个 feature 独立编译验证（`cargo build --features sz-orm-macros/explain-analyzer` 等 7 项）
- [ ] M6-T3.2 feature 组合编译验证（含与 v4.2.0 7 个 feature 的组合，`cargo build --features sz-orm-dtx/cross-lang-dtx,sz-orm-macros/explain-analyzer,...`）
- [ ] M6-T3.3 全 feature 编译验证（`cargo build --workspace --all-features`）
- [ ] M6-T3.4 制定逐步启用计划（按 P1→P2→P3 优先级，标注每个 feature 的启用条件与风险）

**验收标准**：feature 组合无冲突；全 feature 编译通过；逐步启用计划文档化

**依赖**：M6-T1

---

# 九、任务依赖关系

```
M0（P0，纯文档，并行）  M0-T1 → M0-T2 → M0-T3

M1（P1，查询智能，与 v4.2.0 并行）
                        M1-T1 → M1-T2 → M1-T3 → M1-T6
                        M1-T4 ──────────↗          │
                        M1-T5 ────────────────────↗

M2（P1，N+1 静态检测，v4.2.0 交付后）
                        M2-T1 → M2-T2 → M2-T3

M3（P2，数据治理，与 v4.2.0 并行）
                        M3-T1 → M3-T2 → M3-T5
                        M3-T3 → M3-T4 →↗

M4（P2，自适应查询，v4.2.0 交付后）
                        M4-T1 → M4-T2 → M4-T3

M5（P3，融合查询，可选实验，v4.2.0 交付后）
                        M5-T1 → M5-T2 → M5-T3

M6（最终验证，全部完成后）
                        M6-T1 ← M1-T6 / M2-T3 / M3-T5 / M4-T3 / M5-T3 / M0-T3
                        M6-T2 ← M6-T1
                        M6-T3 ← M6-T1
```

**并行说明**：
1. M0/M1/M3 与 v4.2.0 并行（新包或 v4.2.0 不触碰的包）
2. M2/M4/M5 在 v4.2.0 交付后启动（M2 与 v4.2.0 同改 `cli/src/main.rs`；M4/M5 依赖 v4.2.0 后 sz-orm-core 最终形态）
3. M6 必须最后执行

---

# 十、风险与缓解措施

| 风险 ID | 风险描述 | 影响 | 概率 | 缓解措施 | 对应任务 |
|---------|---------|------|------|---------|---------|
| R-001 | EXPLAIN 输出格式随 DB 版本变化解析失败 | 中（降级无警告） | 中 | 解析失败降级为无警告（不阻断编译），Parser 按方言版本适配 | M1-T2、M1-T3 |
| R-002 | 编译期警告过度干扰开发者 | 低（可抑制） | 中 | 默认 warning 非阻断，`SZ_ORM_EXPLAIN_ROW_THRESHOLD` 配置阈值 + `allow` 属性抑制 | M1-T3 |
| R-003 | N+1 静态检测误报/漏报 | 中（开发者困扰） | 中 | 检测模式保守（仅循环体内直接查询调用），标注宏 + 批量扫描双入口，运行时 `N1QueryDetector` 交叉验证 | M2-T1、M2-T3 |
| R-004 | `cli/src/main.rs` 与 v4.2.0 修改冲突 | 高（merge 冲突） | 低 | M2 排期在 v4.2.0 交付后，merge 时 `git diff` 先行核对 | M2-T2 |
| R-005 | `sz-orm-macros` 扩展影响既有 `query!` 行为 | 高（编译破坏） | 低 | 全部新增逻辑在 `db-verify` feature 内，默认模式零改动；既有测试基线验证 | M1-T3 |
| R-006 | 自适应查询自动切缓存导致脏读 | 高（数据不一致） | 中 | 缓存 TTL 可配 + 默认关闭自动缓存（需显式开启），统计决策仅建议不强制 | M4-T2 |
| R-007 | 编译期治理 compile_error 阻断既有代码编译 | 高（编译破坏） | 低 | 治理检查仅在 `compile-governance` feature 启用时生效，默认关闭 | M3-T3 |
| R-008 | db-fusion 拆分语义不透明导致数据不一致 | 高（数据错误） | 中 | 实验阶段仅支持可证明安全的拆分（主键等值 + 缓存键），不安全拆分回退全主库，评估报告决定转正/废弃 | M5-T2 |
| R-009 | 新增 feature 与 v4.2.0 7 个 feature 组合编译失败 | 高（编译破坏） | 低 | 门禁 10 全组合编译 + M6-T3 组合验证 | M6-T1、M6-T3 |
| R-010 | sz-pay 既有代码因 API 变更破坏 | 高（生产故障） | 低 | 无 Breaking Change，7 个 feature gate 隔离默认关闭，既有公开 API 完全向后兼容，sz-pay 回归测试 | M6-T2 |
| R-011 | 火焰图阶段计时误差超 1ms | 低（精度不足） | 低 | `Instant::now()` 精度保证，测试断言误差 < 1ms | M1-T5 |
| R-012 | 血缘影响分析环导致死循环 | 中（超时） | 低 | BFS 深度受限 + 已访问节点不重复遍历，环检测 | M3-T2 |
| R-013 | 合规报告审计链写入失败 | 低（报告仍生成） | 低 | 告警，报告标注"audit chain write failed"，人工复核 | M3-T4 |
| R-014 | CDC 同步延迟导致融合查询缓存/搜索索引过期 | 中（读到旧数据） | 中 | 缓存 TTL 可配，延迟期间回退主库，评估报告标注同步延迟影响 | M5-T3 |

---

# 十一、验收标准汇总

## 11.1 全局验收条件

1. **API 兼容性**：v4.2.0 既有公开 API 完全向后兼容，sz-pay 既有代码不受影响
2. **feature gate 隔离**：7 个新 feature 默认全关闭，默认 feature 行为不变
3. **测试基线不回退**：v4.2.0 已验收测试基线不回退，v4.3.0 仅增不减
4. **五方言一致**：EXPLAIN 解析/治理/融合按方言能力适配
5. **审计证据**：每项结论附 file:line 证据，`bash scripts/audit-verify.sh` 验证通过
6. **14 道门禁通过**
7. **unsafe 零容忍** / **禁止占位实现** / **复用优先**
8. **与 v4.2.0 零重叠**：唯一同文件风险 `cli/src/main.rs` 通过排期规避

## 11.2 里程碑验收

| 里程碑 | 核心验收点 | 对应需求 |
|--------|-----------|---------|
| M0 | 虚构数字全部修正（file:line 证据）；基线报告通过 audit-verify | — |
| M1 | 5 方言 EXPLAIN 解析 + 编译期警告 + 计划回归 + 火焰图（Brendan Gregg 兼容 + SVG） | REQ-V43-001 |
| M2 | 3 种 N+1 模式检测；CLI 批量扫描；与运行时 N1QueryDetector 交叉一致 | REQ-V43-002 |
| M3 | Mermaid/Graphviz/HTML 血缘导出 + 影响分析；PII 编译期强制；合规报告 | REQ-V43-003 |
| M4 | 统计采集 <1μs；自动分页/缓存决策；慢查询降级；复用既有实现 | REQ-V43-004 |
| M5 | db-fusion POC + 转正/废弃评估报告 | REQ-V43-005 |
| M6 | 14 道门禁 + 文档同步 + sz-pay 兼容 + feature 组合验证 | 全局 |

## 11.3 需求验收对齐

| 需求编号 | 验收标准（spec.md §8） | 对应任务 | 设计章节 |
|---------|----------------------|---------|---------|
| REQ-V43-001 | 5 方言 EXPLAIN 解析 + 编译期警告 + 计划回归 + 火焰图（Brendan Gregg + SVG）+ 复用既有 db-verify/Tracer | M1-T1~M1-T6 | design.md §2.2.1 |
| REQ-V43-002 | 函数标注宏 + 3 种检测模式 + CLI 批量扫描 + 静态/运行时交叉验证 + 默认 warning 非阻断 | M2-T1~M2-T3 | design.md §2.2.2 |
| REQ-V43-003 | Mermaid/Graphviz/HTML 血缘 + 影响分析 + PII 编译期强制 + 合规报告 + 复用既有 LineageGraph/ABAC/脱敏 | M3-T1~M3-T5 | design.md §2.2.3 |
| REQ-V43-004 | 统计采集 <1μs + 自动分页/缓存决策 + 慢查询降级 + 缓存不脏读 + 复用既有 cursor_stream/l2_cache | M4-T1~M4-T3 | design.md §2.2.4 |
| REQ-V43-005 | 融合配置 + 拆分/聚合（仅安全拆分）+ CDC 同步 + 主库失败回退 + 实验评估报告 + 复用既有 HybridSearcher/CDC | M5-T1~M5-T3 | design.md §2.2.5 |

---

# 十二、实施建议

## 12.1 开发顺序

1. **立即启动（与 v4.2.0 并行）**：
   - M0（0.5 周，纯文档，立即并行）
   - M1（sz-orm-explain 新包先做，sz-orm-flamegraph 建议 v4.2.0 M1 后启动避免 merge 冲突）
   - M3（sz-orm-audit lineage-viz 扩展 + sz-orm-core compile-governance 扩展）
2. **v4.2.0 交付后启动**：
   - M2（cli 同文件，必须 v4.2.0 交付后）
   - M4（依赖 core 最终形态）
   - M5（可选实验）
3. **最后**：
   - M6 最终验证（14 道门禁 + 文档 + 版本号 v4.2.0→v4.3.0 + sz-pay 兼容性）

## 12.2 验证节奏

- 每个任务完成后运行 `cargo test -p <package> --features <feature>`
- 每个里程碑末尾运行集成测试与门禁验证（M1-T6/M2-T3/M3-T5/M4-T3/M5-T3）
- M6 运行 14 道门禁全量验证

## 12.3 文档同步

- 每个里程碑完成后更新 `CHANGELOG.md`
- M6-T2 统一更新 `docs/API-STABILITY.md` / `README.md` / `AGENTS.md` / `docs/sz-orm-engineering-practices.md` + 评估文档
- 版本号 v4.2.0 → v4.3.0 在 M6-T2 更新（`Cargo.toml` workspace.package.version）

## 12.4 复用优先清单（file:line 证据）

| 复用点 | 既有代码位置 | 复用任务 | 复用方式 |
|--------|-------------|---------|---------|
| db-verify 环境变量开关 | `packages/sz-orm-macros/src/lib.rs:548` | M1-T3 | 扩展在此 feature 内调用解析器 |
| SQL 语法校验 | `packages/sz-orm-macros/src/lib.rs:298` | M1-T1 | 模式参考 |
| 各方言 EXPLAIN 语句构造 | `packages/sz-orm-macros/src/lib.rs:642-647` | M1-T2 | 解析器对应方言输出格式 |
| Tracer trait | `packages/sz-orm-tracing/src/lib.rs:129` | M1-T5 | `QueryTracer::with_tracer` 写入 span |
| SzTracer | `packages/sz-orm-tracing/src/lib.rs:136` | M1-T5 | span 关联复用 |
| QueryBuilder | `packages/sz-orm-core/src/query.rs:36` | M1-T5 | 火焰图包装查询执行 |
| Connection trait | `packages/sz-orm-core/src/pool.rs:45` | M1-T5 | 查询执行链路 |
| N1QueryDetector | `packages/sz-orm-core/src/entity_graph.rs:641` | M2-T1、M2-T3 | 检测知识参考 + 交叉验证 |
| eager_loader | `packages/sz-orm-core/src/eager_loader.rs` | M2-T1 | MissingEagerLoadHint 模式替代建议 |
| smart_eager_loader | `packages/sz-orm-core/src/smart_eager_loader.rs` | M2-T1 | 自动选择预加载策略参考 |
| LineageGraph | `packages/sz-orm-audit/src/lineage/graph.rs:96` | M3-T1、M3-T2 | 图结构复用（节点/边） |
| export_dot / export_json / export_graphml | `packages/sz-orm-audit/src/lineage/export.rs:34/68/105` | M3-T1 | 导出模式参考，保留不动 |
| AccessRule / AccessContext / RowLevelSecurity | `packages/sz-orm-core/src/access_control.rs:9/22/85` | M3-T3 | ABAC 权限复用 |
| MaskingRule | `packages/sz-orm-masking/src/lib.rs:21` | M3-T3、M3-T4 | 脱敏规则执行复用 |
| migrate_dry_run | `packages/sz-orm-core/src/migration_dry_run.rs:94` | M3-T2 | 影响分析与 dry-run 联动 |
| cursor_stream | `packages/sz-orm-core/src/cursor_stream.rs` | M4-T2 | `should_paginate` 为真时切换游标分页 |
| paginator | `packages/sz-orm-core/src/paginator.rs` | M4-T2 | 分页句柄复用 |
| l1_cache | `packages/sz-orm-core/src/l1_cache.rs` | M4-T2 | 本地缓存复用 |
| l2_cache | `packages/sz-orm-core/src/l2_cache.rs` | M4-T2 | `should_cache` 为真时写入 L2 |
| plan_cache | `packages/sz-orm-core/src/plan_cache.rs` | M4-T2 | 查询计划缓存复用 |
| AutoTuningPipeline | `packages/sz-orm-ai/src/auto_tuning/pipeline.rs:15` | M4-T1 | 互补关系（本方案无 AI 依赖），保留不动 |
| DbType（28 方言枚举） | `packages/sz-orm-core/src/db_type.rs:11` | M5-T1 | FusionConfig primary 方言标识 |
| HybridSearcher | `packages/sz-orm-vector/src/hybrid_search/searcher.rs:30` | M5-T1、M5-T2 | 三源并行查询 + 融合排序模式复用 |
| DialectCapturer | `packages/sz-orm-queue/src/cdc/capturer.rs:12` | M5-T3 | 主库→缓存/搜索索引同步，不重复实现 CDC |

---

> 文档结束。本任务规划所有 file:line 证据均来自 2026-08-12 实测（非文档声明），与 v4.2.0 边界清晰（唯一同文件风险 `cli/src/main.rs` 已通过排期规避），与 `spec.md`（What to build）+ `design.md`（How to build）+ `development-plan.md`（What/How/When）完全对齐，不增删任务，遵循 AGENTS.md 审计合规铁律。