# sz-orm v2.3.0 编码任务规划文档

> **版本**：v2.3.0
> **基线**：v2.2.0（代码已完成；43 包工作空间，sz-orm-core 1.0.0 已发布 crates.io）
> **生成日期**：2026-08-07
> **依据**：`docs/spec/v2.3.0/spec.md`（99 条 EARS 需求）+ `docs/spec/v2.3.0/design.md`（13 章节技术设计）
> **文档目的**：将 v2.3.0 三项中期目标（任务 A：sz-pay 生产案例深化、任务 B：性能基准完整报告、任务 C：Eager Loading 智能策略选择）从技术设计转化为可独立执行、可独立验证的编码任务清单，所有任务可追溯到 spec.md 需求 ID 与 design.md 设计章节。

---

## 一、任务总览

| 维度 | 数值 |
|------|------|
| 任务总数 | 32 个子任务 |
| 里程碑数量 | 5 个（M1~M5） |
| 预估总工作量 | 约 90~110 人时（按每任务 1~4 小时估算） |
| 高优先级任务 | 18 个 |
| 中优先级任务 | 14 个 |
| 低优先级任务 | 0 个 |
| 高风险任务 | 6 个（标注 ⚠） |
| 涉及仓库 | sz-orm（任务 B/C + 自身开发）、sz-pay（任务 A，下游修改） |
| 需求覆盖 | 99 条 EARS 需求 100% 覆盖 |

### 任务分类统计

| 任务类别 | 任务 ID 范围 | 任务数 | 所属里程碑 |
|---------|------------|--------|-----------|
| 任务 C：智能策略选择核心 | T-C-001 ~ T-C-006 | 6 | M1 |
| 任务 C：智能策略选择增强 | T-C-007 ~ T-C-011 | 5 | M2 |
| 任务 B：基准扩展 | T-B-001 ~ T-B-007 | 7 | M3 |
| 任务 B：报告生成 | T-B-008 ~ T-B-010 | 3 | M4 |
| 任务 A：依赖升级与验证 | T-A-001 ~ T-A-004 | 4 | M4 |
| 任务 A：性能采集与基线 | T-A-005 ~ T-A-007 | 3 | M5 |
| 集成验证与发布 | T-INT-001 ~ T-INT-004 | 4 | M5 |

---

## 二、里程碑规划

### M1：任务 C 核心实现（SmartEagerLoader + 策略决策器 + 三策略执行器）

- **目标**：实现智能 Eager Loading 的核心骨架——策略决策器与三种策略执行器（HasOne JOIN / HasMany data loader / ManyToMany 中间表），并提供 `SmartEagerLoader` 类型与 `EagerLoader::smart()` 扩展方法入口。
- **交付物**：`smart_eager_loader.rs`（新增）、`relation_trait.rs`（扩展中间表字段）、`eager_loader.rs`（新增 smart() 方法）。
- **验收门槛**：策略决策器对四种关联类型决策正确；三种策略执行器可独立执行；`smart()` 入口可用且不破坏 v2.2.0 API。
- **预估工作量**：18~22 人时。
- **后续依赖**：M2 依赖 M1 完成。

### M2：任务 C 增强（N+1 自动消除 + 向后兼容验证 + 单元/集成测试）

- **目标**：在 M1 核心骨架上集成 N+1 自动消除器、循环检测复用、决策日志，并完成单元测试、集成测试（智能 vs 手动等价性 + 五方言）与性能基准验证。
- **交付物**：`n1_eliminator.rs`（新增）、`smart_eager_loader.rs`（增强 load 方法）、测试套件。
- **验收门槛**：N+1 自动消除正确合并且结果等价；智能模式与手动模式结果 100% 等价；五方言智能模式结果一致；决策延迟 ≤ 100μs。
- **预估工作量**：16~20 人时。
- **后续依赖**：M4 的 sz-pay smart() 验证依赖 M2 完成。

### M3：任务 B 基准扩展（全维度 benchmark + criterion 配置 + 竞品适配层）

- **目标**：扩展基准测试至全维度（CRUD/关联/事务/连接池/分页）× 四档规模 × 四竞品（sz-orm/Diesel/SeaORM/SQLx），建立竞品适配层统一接口，配置 criterion 统计采样。
- **交付物**：`competitor_adapter.rs`（新增）、5 个维度基准模块（新增）、`full_comparison.rs`（新增主入口）。
- **验收门槛**：6 维度 × 4 规模 × 4 竞品基准可运行；竞品不支持维度标注 N/A；criterion 输出均值/中位数/StdDev/置信区间。
- **预估工作量**：18~22 人时。
- **后续依赖**：M4 的报告生成依赖 M3 完成。

### M4：任务 B 报告 + 任务 A 准备（报告生成器 + sz-pay 依赖升级 + 新功能验证用例）

- **目标**：生成公开基准报告（Markdown + CSV/JSON + 环境元数据 + DSN 脱敏）；将 sz-pay 依赖从 2.1.0 升级至 2.3.0，验证编译兼容与 5,139 基线测试零回归，新增 v2.2.0+ 新功能验证用例。
- **交付物**：`benchmark_reporter.rs`（新增）、基准报告文件、sz-pay `Cargo.toml`（升级）、sz-pay 验证用例。
- **验收门槛**：基准报告含全维度数据 + DSN 脱敏 + 复现指令；sz-pay 编译零错误 + 测试通过数 ≥ 5,139；新功能验证用例全部通过。
- **预估工作量**：16~20 人时。
- **后续依赖**：M5 的性能采集依赖 M4 完成。

### M5：任务 A 深化 + 集成验证（sz-pay 性能采集 + 回归测试 + 全 workspace 验证 + 版本发布）

- **目标**：在 sz-pay 中采集 v2.3.0 性能数据（QPS/P50/P95/P99/峰值内存）并与 v2.1.0 基线对比；运行全 workspace 10 道门禁；验证 ADR-0001 合规；bump 版本至 2.3.0 并发布 crates.io；收集审计合规证据。
- **交付物**：sz-pay 性能采集工具、性能对比报告、测试基线文档、crates.io 2.3.0 发布物、最终验证报告。
- **验收门槛**：性能数据完整且 DSN 脱敏；10 道门禁全过；ADR-0001 零违规；crates.io 存在 2.3.0；所有结论附 file:line 证据。
- **预估工作量**：14~18 人时。
- **后续依赖**：无（终点里程碑）。

---

## 三、任务清单

### 里程碑 M1：任务 C 核心实现

#### T-C-001：RelationDef 中间表字段扩展（向后兼容）

- **所属里程碑**：M1
- **优先级**：高
- **依赖任务**：无
- **涉及文件**：
  - `packages/sz-orm-core/src/relation_trait.rs`（修改：新增 3 个 Option 字段 + new_many_to_many 构造器）
- **任务描述**：
  - 在 `RelationDef` 结构体新增 3 个可选字段：`join_table: Option<&'static str>`、`join_from_key: Option<&'static str>`、`join_to_key: Option<&'static str>`，用于 ManyToMany 中间表元数据。
  - 保持 `RelationDef::new()` 签名不变（`const fn`），新增字段默认 `None`，确保 v2.2.0 代码零修改编译通过。
  - 新增 `RelationDef::new_many_to_many()` 构造器（`const fn`），接收中间表名与两侧外键，强制 `kind = ManyToMany`。
  - 为新增字段提供 rustdoc 文档注释（含用法示例 `ignore` 代码块）。
- **验收标准**：
  - [ ] `RelationDef::new()` 签名与 v2.2.0 完全一致，已有调用方零修改。
  - [ ] `RelationDef::new_many_to_many()` 可构造含中间表的关联定义，`join_table` 为 `Some`。
  - [ ] `cargo check -p sz-orm-core` 零错误；`cargo clippy -p sz-orm-core -- -D warnings` 零警告。
  - [ ] `cargo doc -p sz-orm-core` 新增 API 有文档与示例。
- **关联需求**：REQ-C-014, REQ-C-015, DFX-COMPAT-001, REQ-CON-COMPAT-001, REQ-NF-MAINT-004
- **关联设计章节**：design.md §3.4.2（RelationDef 中间表扩展）、§7.6（RelationDef 扩展）

---

#### T-C-002：LoadStrategy + StrategyDecision + StrategyResolver 策略决策器

- **所属里程碑**：M1
- **优先级**：高
- **依赖任务**：T-C-001
- **涉及文件**：
  - `packages/sz-orm-core/src/smart_eager_loader.rs`（新增：LoadStrategy 枚举 + StrategyDecision 结构 + StrategyResolver）
  - `packages/sz-orm-core/src/lib.rs`（修改：新增 `mod smart_eager_loader;` 声明）
- **任务描述**：
  - 新增 `LoadStrategy` 枚举：`Join` / `DataLoader` / `IntermediateTableBatch`，派生 `Debug/Clone/Copy/PartialEq/Eq`。
  - 新增 `StrategyDecision` 结构：`relation_name: &'static str`、`relation_kind: RelationKind`、`strategy: LoadStrategy`、`reason: String`、`estimated_query_count: usize`。
  - 新增 `StrategyResolver` 结构（无字段，纯规则匹配器），实现：
    - `resolve(relation: &RelationDef) -> StrategyDecision`：按决策规则矩阵（design.md §3.1.2）决策——HasOne/BelongsTo → Join（1 次）；HasMany → DataLoader（2 次）；ManyToMany 有中间表 → IntermediateTableBatch（2 次）；ManyToMany 无中间表 → 回退 DataLoader + `tracing::warn!` 告警。
    - `resolve_chain(relations: &[RelationDef]) -> Vec<StrategyDecision>`：逐级独立决策（REQ-C-027）。
  - 决策为纯内存枚举匹配 + Option 判空，无 IO，无内存分配（reason 使用 `&'static str` 或 String 字面量）。
  - 确定性保证：相同 `RelationDef` 输入始终返回相同 `StrategyDecision`。
- **验收标准**：
  - [ ] `StrategyResolver::resolve` 对四种关联类型决策正确（HasOne→Join, BelongsTo→Join, HasMany→DataLoader, ManyToMany 有中间表→IntermediateTableBatch）。
  - [ ] ManyToMany 缺中间表时回退 DataLoader 且输出 `tracing::warn!` 告警，不 panic。
  - [ ] `resolve_chain` 对多级关联逐级独立决策，每级策略独立。
  - [ ] 决策过程无 IO、无内存分配（除 reason String）。
  - [ ] `cargo test -p sz-orm-core --lib smart_eager_loader` 决策器单元测试通过。
  - [ ] 零 `todo!`/`unimplemented!`/`unreachable!`/`unsafe`。
- **关联需求**：REQ-C-002, REQ-C-003, REQ-C-004, REQ-C-005, REQ-C-027, DFX-PERF-001, REQ-NF-PERF-001, REQ-NF-TEST-001
- **关联设计章节**：design.md §3.1（策略决策器）、§3.1.2（决策规则矩阵）、§3.1.3（接口签名）、§7.2（策略决策结果）

---

#### T-C-003：JoinStrategy 执行器（HasOne/BelongsTo 自动 JOIN）

- **所属里程碑**：M1
- **优先级**：高
- **依赖任务**：T-C-001, T-C-002
- **涉及文件**：
  - `packages/sz-orm-core/src/smart_eager_loader.rs`（修改：新增 JoinStrategy 结构 + execute + split_join_row）
- **任务描述**：
  - 新增 `JoinStrategy` 结构，实现：
    - `async fn execute(conn, relation, main_columns, related_columns, where_clause, where_params) -> Result<Vec<(HashMap<String,Value>, Option<HashMap<String,Value>>)>, DbError>`：根据 `RelationDef` 复用 `QueryBuilder::join` 生成 JOIN SQL（主表列前缀 `main_`，关联表列前缀 `related_`），执行单次查询，拆分扁平行为 (主表行, 关联行)。
    - `fn split_join_row(flat_row, main_columns, related_columns) -> (HashMap, Option<HashMap>)`：按列名前缀拆分，关联表列全 Null 时返回 `None`。
  - SQL 生成约束：禁止 `SELECT *`，使用显式列名 + 前缀；WHERE 条件参数化（复用 `QueryBuilder::where_eq`）。
  - 方言回退（REQ-C-008）：若目标方言不支持 JOIN，回退 DataLoader 策略并 `tracing::warn!` 记录原因（本任务预留回退入口，实际回退逻辑在 T-C-008 集成）。
  - 查询次数固定 1 次（DFX-PERF-002）。
- **验收标准**：
  - [ ] HasOne 关联生成 1 条含 INNER JOIN 的 SQL，执行 1 次查询。
  - [ ] BelongsTo 关联生成 1 条含 INNER JOIN 的 SQL，执行 1 次查询。
  - [ ] `split_join_row` 正确拆分扁平行，无数据串行；关联表列全 Null 时返回 `None`。
  - [ ] 生成 SQL 无 `SELECT *`，WHERE 条件参数化（`?`/`$N` 占位符）。
  - [ ] `cargo test -p sz-orm-core --lib join_strategy` 单元测试通过（含 MockConnection）。
  - [ ] 零 `todo!`/`unimplemented!`/`unsafe`。
- **关联需求**：REQ-C-006, REQ-C-007, REQ-C-008, REQ-C-009, DFX-PERF-002, DFX-SEC-001, DFX-SEC-002, REQ-NF-SEC-001, REQ-NF-SEC-002
- **关联设计章节**：design.md §3.2（HasOne 自动 JOIN 策略）、§3.2.2（结果集拆分组装）、§3.2.3（拆分算法）

---

#### T-C-004：DataLoaderStrategy 执行器（HasMany 自动 data loader）

- **所属里程碑**：M1
- **优先级**：高
- **依赖任务**：T-C-001, T-C-002
- **涉及文件**：
  - `packages/sz-orm-core/src/smart_eager_loader.rs`（修改：新增 DataLoaderStrategy 结构 + execute + group_by_foreign_key）
  - `packages/sz-orm-core/src/eager_loader.rs`（复用：`batch_query_with_relation` / `group_rows_by_foreign_key` 内部逻辑）
- **任务描述**：
  - 新增 `DataLoaderStrategy` 结构，实现：
    - `async fn execute(conn, relation, main_sql, main_columns, related_columns) -> Result<Vec<EagerResult>, DbError>`：执行主表查询收集主键 → 空结果跳过（REQ-C-013）→ 生成 `WHERE fk IN (?,...)` 批量查询（参数化）→ Oracle 方言且主键数 >1000 时分批（复用 `eager_loader.rs:274` 的 `chunks(1000)` 逻辑）→ 按外键分组组装。
    - `fn group_by_foreign_key(related_rows, foreign_key) -> HashMap<String, Vec<HashMap>>`：按外键值分组，无遗漏无错配。
  - 核心复用：直接复用 `EagerLoader::load_many` 内部逻辑（`batch_query_with_relation` / `group_rows_by_foreign_key` 模块级自由函数），避免重复实现。
  - 显式列名投影（REQ-NF-SEC-002）：批量查询使用显式列名，禁止 `SELECT *`。
  - 查询次数固定 2 次（DFX-PERF-003）。
- **验收标准**：
  - [ ] HasMany 关联执行 2 次查询（主表 + 批量 IN），按外键分组组装。
  - [ ] 主表查询返回 0 行时跳过关联查询，返回空 Vec。
  - [ ] Oracle 方言且主键数 >1000 时自动分批，每批 ≤1000。
  - [ ] `group_by_foreign_key` 分组无遗漏无错配。
  - [ ] 生成 SQL 无 `SELECT *`，WHERE 条件参数化。
  - [ ] `cargo test -p sz-orm-core --lib data_loader_strategy` 单元测试通过。
  - [ ] 零 `todo!`/`unimplemented!`/`unsafe`。
- **关联需求**：REQ-C-010, REQ-C-011, REQ-C-012, REQ-C-013, DFX-PERF-003, DFX-REL-002, REQ-NF-SEC-001, REQ-NF-SEC-002
- **关联设计章节**：design.md §3.3（HasMany 自动 Data Loader 策略）、§3.3.2（接口签名）

---

#### T-C-005：IntermediateTableStrategy 执行器（ManyToMany 中间表批量）

- **所属里程碑**：M1
- **优先级**：高
- **依赖任务**：T-C-001, T-C-002
- **涉及文件**：
  - `packages/sz-orm-core/src/smart_eager_loader.rs`（修改：新增 IntermediateTableStrategy 结构 + execute）
- **任务描述**：
  - 新增 `IntermediateTableStrategy` 结构，实现：
    - `async fn execute(conn, relation, main_sql, main_columns, related_columns) -> Result<Vec<EagerResult>, DbError>`：
      1. 校验中间表配置（`join_table`/`join_from_key`/`join_to_key` 任一缺失返回 `DbError::InvalidInput("ManyToMany 关联 {name} 缺少中间表配置")`）。
      2. 查主表主键。
      3. 经中间表 JOIN 关联表批量查询：`SELECT related.* FROM {join_table} jt JOIN {to_entity} related ON jt.{join_to_key} = related.{from_key} WHERE jt.{join_from_key} IN (?, ...)`（参数化 IN）。
      4. 按主键分组组装为 (主实体, Vec<关联实体>)，处理同一关联被多个主表行引用的情况。
  - Oracle IN >1000 分批（复用 `chunks(1000)`）。
  - 显式列名投影，禁止 `SELECT *`。
  - 查询次数固定 2 次。
- **验收标准**：
  - [ ] ManyToMany 关联（有中间表）经中间表批量查询，按主键分组组装为 (主实体, Vec<关联实体>)。
  - [ ] 中间表元数据缺失时返回 `DbError::InvalidInput`，错误消息含关联名。
  - [ ] 同一关联被多个主表行引用时，每个主表行获得其全部关联实体，无重复无遗漏。
  - [ ] 生成 SQL 无 `SELECT *`，WHERE 条件参数化。
  - [ ] `cargo test -p sz-orm-core --lib intermediate_table_strategy` 单元测试通过。
  - [ ] 零 `todo!`/`unimplemented!`/`unsafe`。
- **关联需求**：REQ-C-014, REQ-C-015, REQ-C-016, DFX-SEC-001, DFX-SEC-002, REQ-NF-SEC-001, REQ-NF-SEC-002
- **关联设计章节**：design.md §3.4（ManyToMany 自动中间表策略）、§3.4.3（接口签名）

---

#### T-C-006：SmartEagerLoader 类型 + EagerLoader::smart() 扩展方法

- **所属里程碑**：M1
- **优先级**：高
- **依赖任务**：T-C-001, T-C-002, T-C-003, T-C-004, T-C-005
- **涉及文件**：
  - `packages/sz-orm-core/src/smart_eager_loader.rs`（修改：新增 SmartEagerLoader 结构 + with/with_cycle_policy/with_n1_threshold/decisions 方法）
  - `packages/sz-orm-core/src/eager_loader.rs`（修改：新增 `smart()` 扩展方法，返回 SmartEagerLoader）
- **任务描述**：
  - 新增 `SmartEagerLoader` 结构：`relation: RelationDef`、`children: Vec<ChildLoadConfig>`（复用 eager_loader 内部结构）、`cycle_policy: CyclePolicy`（默认 Truncate）、`n1_threshold: usize`（默认 5）、`decisions: Vec<StrategyDecision>`。
  - 实现 `SmartEagerLoader` 链式 API：
    - `with(relation) -> Self`：添加子级关联，读取关联元数据供策略决策器使用。
    - `with_cycle_policy(policy) -> Self`：设置循环检测策略（与 EagerLoader 一致语义）。
    - `with_n1_threshold(threshold) -> Self`：配置 N+1 消除阈值。
    - `decisions() -> &[StrategyDecision]`：返回策略决策记录（供调试与审计）。
  - 在 `EagerLoader` 上新增 `smart(self) -> SmartEagerLoader` 扩展方法：转移原 EagerLoader 的关联链配置到 SmartEagerLoader，不修改 `new()`/`with()`/`load_many()`/`load_nested()` 签名。
  - `load()` 方法本任务仅实现骨架（逐级决策 + 分发到三策略执行器），N+1 消除集成在 T-C-008 完成。
  - 为所有新增 API 提供 rustdoc 文档注释（含用法示例 `ignore` 代码块）。
- **验收标准**：
  - [ ] `EagerLoader::new(rel).smart()` 返回 `SmartEagerLoader`，原 EagerLoader API 不变。
  - [ ] `SmartEagerLoader::with().with()` 链式添加多级关联。
  - [ ] `decisions()` 返回策略决策记录列表。
  - [ ] v2.2.0 代码（未调用 smart()）行为与 v2.2.0 完全一致。
  - [ ] `cargo check -p sz-orm-core` 零错误；`cargo clippy -p sz-orm-core -- -D warnings` 零警告。
  - [ ] `cargo doc -p sz-orm-core` 新增 API 有文档与示例。
  - [ ] 零 `todo!`/`unimplemented!`/`unsafe`。
- **关联需求**：REQ-C-001, REQ-C-002, REQ-C-023, REQ-C-024, REQ-C-025, REQ-C-026, DFX-COMPAT-001, DFX-COMPAT-002, DFX-COMPAT-003, REQ-CON-COMPAT-001, REQ-NF-MAINT-004
- **关联设计章节**：design.md §3.6（向后兼容方案）、§3.6.1（smart() 扩展方法）、§3.7（SmartEagerLoader 数据结构）、§6.2.1（EagerLoader::smart 接口）

---

### 里程碑 M2：任务 C 增强

#### T-C-007：N1Eliminator N+1 自动消除器 ⚠ 高风险

- **所属里程碑**：M2
- **优先级**：高
- **依赖任务**：T-C-006
- **涉及文件**：
  - `packages/sz-orm-core/src/n1_eliminator.rs`（新增：N1Eliminator + PendingQuery + N1EliminationReport）
  - `packages/sz-orm-core/src/lib.rs`（修改：新增 `mod n1_eliminator;` 声明）
- **任务描述**：
  - 新增 `PendingQuery` 结构：`table`、`where_column`、`where_value: Value`、`select_columns`、`in_standalone_transaction: bool`、`trigger_location: String`（file:line）。
  - 新增 `N1EliminationReport` 结构：`original_count`、`merged_count`、`saved_count`、`trigger_location`、`merged_sql`（参数化 SQL）。
  - 新增 `N1Eliminator` 结构：`threshold: usize`（默认 5）、`pending_queries: Vec<PendingQuery>`，实现：
    - `new() -> Self`：默认阈值 5。
    - `with_threshold(threshold) -> Self`：配置阈值。
    - `record_query(query)`：记入一次查询，检测连续相同模式（同表、同 WHERE 列、同 SELECT 列）。
    - `async try_merge(conn) -> Result<Option<N1EliminationReport>, DbError>`：按 design.md §3.5.3 算法——未达阈值返回 `Ok(None)`；含独立事务返回 `Ok(None)` + 告警；生成 `WHERE id IN (?,...)` 批量查询；结果等价性校验（相同主键集合返回相同实体集合），不等价返回 `Err(DbError::Internal)` 回退逐条执行。
  - 合并 SQL 参数化（REQ-NF-SEC-003），禁止 `SELECT *`。
- **验收标准**：
  - [ ] 连续查询数 < 阈值时不合并（`Ok(None)`）。
  - [ ] 连续查询数 ≥ 阈值且无独立事务时合并为 1 次 `WHERE id IN` 批量查询。
  - [ ] 含独立事务时跳过合并 + `tracing::warn!` 告警"事务边界不兼容"。
  - [ ] 合并后结果与逐条查询结果等价；不等价时返回 `Err(DbError::Internal)` + `tracing::error!` 告警。
  - [ ] `N1EliminationReport` 含原次数/合并后次数/节省/触发位置/合并 SQL。
  - [ ] 合并 SQL 参数化，无 `SELECT *`。
  - [ ] `cargo test -p sz-orm-core --lib n1_eliminator` 单元测试通过。
  - [ ] 零 `todo!`/`unimplemented!`/`unsafe`。
- **关联需求**：REQ-C-017, REQ-C-018, REQ-C-019, REQ-C-020, REQ-C-021, REQ-C-022, DFX-PERF-004, DFX-REL-003, REQ-NF-PERF-003, REQ-NF-SEC-003, REQ-NF-TEST-002
- **关联设计章节**：design.md §3.5（N+1 自动消除）、§3.5.2（接口签名）、§3.5.3（合并算法）、§7.3（N+1 消除报告）
- **风险说明**：N+1 消除结果等价性校验是关键风险点，不等价回退逻辑必须严格验证，避免返回错误结果。

---

#### T-C-008：SmartEagerLoader::load 集成 N+1 消除 + 循环检测 + 决策日志

- **所属里程碑**：M2
- **优先级**：高
- **依赖任务**：T-C-006, T-C-007
- **涉及文件**：
  - `packages/sz-orm-core/src/smart_eager_loader.rs`（修改：完善 load 方法，集成 N1Eliminator + CycleDetector + tracing 日志）
- **任务描述**：
  - 完善 `SmartEagerLoader::load(conn, main_sql) -> Result<Vec<NestedEagerResult>, DbError>`，按 design.md §3.8 总流程：
    1. 初始化 `CycleDetector`（复用 v2.2.0，按 `cycle_policy`）。
    2. 执行主表查询；空结果返回空 Vec（REQ-C-013）；结果集 >1,000,000 行返回 `InvalidInput` 错误。
    3. 无子级关联返回叶子节点。
    4. 逐级智能加载（`load_level_smart` 递归）：循环检测 → `StrategyResolver::resolve` 决策 → `tracing::info!` 输出决策日志（关联名/类型/策略/原因四要素）→ 按策略分发到 JoinStrategy/DataLoaderStrategy/IntermediateTableStrategy → DataLoader 策略集成 `N1Eliminator::try_merge` → 递归子级。
    5. 次优策略警告（REQ-C-028）：智能策略查询次数 > 手动最优时 `tracing::warn!`。
  - 错误传播：策略执行 DbError 向上传播；N+1 不等价回退 `Err(Internal)` 向上传播；循环检测 `CyclePolicy::Error` 返回 `Err(InvalidInput)`。
  - 错误上下文链：复用 `DbError::Contextual` 附加调用方上下文。
- **验收标准**：
  - [ ] `smart().with().with().load()` 返回 `Vec<NestedEagerResult>` 树，结构与手动 `load_nested` 一致。
  - [ ] 每级关联独立决策，允许不同级使用不同策略（L1 HasMany→DataLoader, L2 HasOne→Join）。
  - [ ] 决策日志含关联名/类型/策略/原因四要素（`tracing::info!`）。
  - [ ] 循环检测在智能模式下生效（CyclePolicy::Error 检测到循环返回错误）。
  - [ ] N+1 消除集成到 DataLoader 策略，连续查询自动合并。
  - [ ] 次优策略查询次数多于手动最优时输出 `tracing::warn!` 警告。
  - [ ] `cargo test -p sz-orm-core --lib smart_eager_loader::load` 集成测试通过。
  - [ ] 零 `todo!`/`unimplemented!`/`unsafe`。
- **关联需求**：REQ-C-003, REQ-C-004, REQ-C-013, REQ-C-017, REQ-C-025, REQ-C-026, REQ-C-027, REQ-C-028, DFX-PERF-001, DFX-REL-002, REQ-NF-PERF-002, REQ-NF-MAINT-001
- **关联设计章节**：design.md §3.6.3（多级逐级独立决策）、§3.6.4（次优策略警告）、§3.8（智能加载总流程）、§8.2（错误传播策略）

---

#### T-C-009：任务 C 单元测试套件

- **所属里程碑**：M2
- **优先级**：高
- **依赖任务**：T-C-008
- **涉及文件**：
  - `packages/sz-orm-core/src/smart_eager_loader.rs`（修改：新增 `#[cfg(test)] mod tests` 单元测试）
  - `packages/sz-orm-core/src/n1_eliminator.rs`（修改：新增 `#[cfg(test)] mod tests` 单元测试）
  - `packages/sz-orm-core/src/relation_trait.rs`（修改：新增中间表字段单元测试）
- **任务描述**：
  - 为 `StrategyResolver::resolve` 编写单元测试：HasOne→Join、BelongsTo→Join、HasMany→DataLoader、ManyToMany（有中间表）→IntermediateTableBatch、ManyToMany（无中间表）→回退 DataLoader + 告警。
  - 为 `StrategyResolver::resolve_chain` 编写单元测试：多级关联逐级独立决策。
  - 为 `JoinStrategy::split_join_row` 编写单元测试：扁平行正确拆分，无数据串行，关联列全 Null 返回 None。
  - 为 `DataLoaderStrategy::group_by_foreign_key` 编写单元测试：分组无遗漏无错配。
  - 为 `DataLoaderStrategy::execute` 编写单元测试：空主表跳过、Oracle IN>1000 分批。
  - 为 `IntermediateTableStrategy::execute` 编写单元测试：中间表缺失返回 InvalidInput。
  - 为 `N1Eliminator::try_merge` 编写单元测试：未达阈值不合并、达阈值合并、事务边界跳过、结果不等价回退。
  - 为 `RelationDef::new_many_to_many` 编写单元测试：中间表字段正确初始化。
  - 为 `RelationDef::new` 编写单元测试：向后兼容，中间表字段为 None。
  - 使用 `MockConnection`（`mock.rs`）避免真实 DB 依赖。
- **验收标准**：
  - [ ] `cargo test -p sz-orm-core --lib` 全部单元测试通过，覆盖 design.md §9.1 所有场景。
  - [ ] 测试覆盖四种关联类型策略选择（REQ-NF-TEST-001）。
  - [ ] 测试覆盖 N+1 消除阈值触发、事务边界跳过、结果等价性（REQ-NF-TEST-002）。
  - [ ] 零 `todo!`/`unimplemented!`/`unsafe`。
- **关联需求**：REQ-NF-TEST-001, REQ-NF-TEST-002, REQ-NF-TEST-005, REQ-NF-TEST-006, REQ-C-003, REQ-C-005, REQ-C-009, REQ-C-012, REQ-C-013, REQ-C-015, REQ-C-020, REQ-C-021, REQ-C-027, DFX-COMPAT-001
- **关联设计章节**：design.md §9.1（单元测试）、§9.5（门禁与质量检查）

---

#### T-C-010：任务 C 集成测试套件（智能 vs 手动等价性 + 五方言）⚠ 高风险

- **所属里程碑**：M2
- **优先级**：高
- **依赖任务**：T-C-009
- **涉及文件**：
  - `packages/sz-orm-core/tests/smart_eager_loader_integration.rs`（新增：集成测试）
  - `packages/sz-orm-core/tests/n1_eliminator_integration.rs`（新增：N+1 消除集成测试）
- **任务描述**：
  - 编写智能模式 vs 手动模式结果等价性差分测试：相同关联配置下 `smart().load()` 与 `load_nested()` 结果一致（REQ-NF-TEST-003, DFX-REL-002）。
  - 编写 HasOne 智能 JOIN 集成测试：生成 1 条 JOIN SQL + 执行 1 次查询 + 结果正确。
  - 编写 BelongsTo 智能 JOIN 集成测试。
  - 编写 HasMany 智能 data loader 集成测试：执行 2 次查询 + 按外键分组组装。
  - 编写 ManyToMany 智能中间表集成测试：经中间表批量查询 + 双向组装。
  - 编写 N+1 自动消除集成测试：循环内 N 次 find_by_id → 合并为 1 次 WHERE id IN；结果等价性验证；不等价回退；事务边界跳过。
  - 编写循环检测智能模式兼容集成测试：smart + CyclePolicy::Error + 检测到循环 → 返回循环错误。
  - 编写多级智能加载集成测试：smart().with().with() 逐级独立策略，返回 NestedEagerResult 树。
  - 编写五方言智能模式集成测试：MySQL/PostgreSQL/SQLite/Oracle/MSSQL 下 smart 模式结果正确且一致（使用 `#[cfg(feature = "real-db")]` + 环境变量触发）。
  - 编写策略决策日志集成测试：启用日志后输出含四要素。
  - 编写次优策略警告集成测试：智能策略查询次数 > 手动最优 → 日志警告。
- **验收标准**：
  - [ ] `cargo test -p sz-orm-core --test smart_eager_loader_integration` 全部通过（Mock 模式）。
  - [ ] `cargo test -p sz-orm-core --test n1_eliminator_integration` 全部通过。
  - [ ] 智能模式与手动模式结果 100% 等价（差分测试无差异）。
  - [ ] 五方言（MySQL/PG/SQLite/Oracle/MSSQL）智能模式结果正确且一致（`--features real-db --ignored`）。
  - [ ] 覆盖 design.md §9.2 所有集成测试场景。
  - [ ] 零 `todo!`/`unimplemented!`/`unsafe`。
- **关联需求**：REQ-C-006, REQ-C-007, REQ-C-010, REQ-C-014, REQ-C-017, REQ-C-018, REQ-C-021, REQ-C-022, REQ-C-025, REQ-C-026, REQ-C-027, REQ-C-028, REQ-NF-TEST-003, DFX-REL-002, DFX-REL-003, REQ-CON-DIALECT-001, REQ-NF-COMPAT-002
- **关联设计章节**：design.md §9.2（集成测试）、§9.4（MockConnection 与真实 DB 双模式）
- **风险说明**：五方言集成测试依赖真实数据库可用性，需确保本机 MySQL/PG/SQLite/Oracle/MSSQL 实例运行。

---

#### T-C-011：任务 C 性能基准验证（决策延迟 ≤100μs + 智能vs手动）

- **所属里程碑**：M2
- **优先级**：中
- **依赖任务**：T-C-010
- **涉及文件**：
  - `bench-comparison/benches/smart_strategy_perf.rs`（新增：策略决策延迟 + 智能vs手动性能基准）
- **任务描述**：
  - 编写策略决策延迟基准：`StrategyResolver::resolve` 10000 次平均延迟 ≤ 100μs（DFX-PERF-001, REQ-NF-PERF-001）。
  - 编写智能模式 vs 手动模式性能基准：相同查询下智能模式耗时 ≤ 手动模式 × 1.05（REQ-NF-PERF-002）。
  - 编写 N+1 消除性能基准：N=10 批量耗时 ≤ 单查总耗时 × 0.5（REQ-NF-PERF-003）。
  - 使用 criterion 框架，配置 `sample_size=100`、`warm_up_time=3s`、`measurement_time=10s`。
- **验收标准**：
  - [ ] `cargo bench --bench smart_strategy_perf` 运行完成，输出 criterion 报告。
  - [ ] 策略决策 10000 次平均延迟 ≤ 100μs。
  - [ ] 智能模式耗时 ≤ 手动模式 × 1.05。
  - [ ] N=10 批量耗时 ≤ 单查总耗时 × 0.5。
  - [ ] 覆盖 design.md §9.3 性能基准场景。
- **关联需求**：DFX-PERF-001, DFX-PERF-002, DFX-PERF-003, DFX-PERF-004, REQ-NF-PERF-001, REQ-NF-PERF-002, REQ-NF-PERF-003
- **关联设计章节**：design.md §9.3（基准测试）、§4.5（criterion 配置）

---

### 里程碑 M3：任务 B 基准扩展

#### T-B-001：CompetitorAdapter trait + 竞品适配实现

- **所属里程碑**：M3
- **优先级**：中
- **依赖任务**：无（可与 M1/M2 并行）
- **涉及文件**：
  - `bench-comparison/benches/competitor_adapter.rs`（新增：CompetitorAdapter trait + CompetitorCapability 枚举 + BenchRecord + 四竞品适配实现）
  - `bench-comparison/Cargo.toml`（修改：新增 diesel/sea-orm/sqlx dev-dependencies）
- **任务描述**：
  - 定义 `BenchRecord` 统一基准记录结构（id + 若干字段）。
  - 定义 `CompetitorCapability` 枚举：`Unsupported(String)`（竞品不支持某维度，REQ-B-011）、`Error(BoxError)`。
  - 定义 `CompetitorAdapter` async trait（`#[async_trait]`）：`name()`、`is_async()`、`setup(dataset_size)`、`teardown()` + CRUD 单条/批量 + 关联查询（HasOne/HasMany/M2M）+ 事务（含 savepoint）+ 连接池 + 分页（OFFSET/游标）方法签名（见 design.md §4.2.1）。
  - 实现 `SzOrmAdapter`：复用 sz-orm-core API，全维度支持。
  - 实现 `DieselAdapter`：复用 `orm_comparison.rs` 现有模式，同步 ORM，全维度支持。
  - 实现 `SeaOrmAdapter`：复用 `orm_comparison.rs` 现有模式，异步 ORM，全支持。
  - 实现 `SqlxAdapter`：底层驱动，关联查询返回 `Unsupported`（无 ORM 级关联，REQ-B-011），事务/连接池/分页支持。
- **验收标准**：
  - [ ] 四个适配器实现 `CompetitorAdapter` trait，`cargo check -p bench-comparison` 零错误。
  - [ ] SQLx 关联查询返回 `Unsupported`，非 panic。
  - [ ] `is_async()` 返回正确（sz-orm/SeaORM/SQLx=true, Diesel=false）。
  - [ ] `cargo test -p bench-comparison --lib competitor_adapter` 适配器单元测试通过。
  - [ ] 零 `todo!`/`unimplemented!`/`unsafe`。
- **关联需求**：REQ-B-008, REQ-B-009, REQ-B-010, REQ-B-011, REQ-B-012, REQ-NF-TEST-005, REQ-NF-TEST-006
- **关联设计章节**：design.md §4.2（竞品适配层设计）、§4.2.1（统一场景接口 trait）、§4.2.2（竞品适配实现）、§4.2.3（条件差异明示）

---

#### T-B-002：bench_crud.rs CRUD 维度基准（单条 + 批量）

- **所属里程碑**：M3
- **优先级**：中
- **依赖任务**：T-B-001
- **涉及文件**：
  - `bench-comparison/benches/bench_crud.rs`（新增：CRUD 单条 + 批量基准）
- **任务描述**：
  - 实现 CRUD 单条基准（REQ-B-001）：单条插入/查询/更新/删除，数据集规模 10/100/1000/10000 四档。
  - 实现 CRUD 批量基准（REQ-B-002）：批量插入/查询/更新/删除，四档规模。
  - 通过 `CompetitorAdapter` trait 调用四竞品，使用 criterion `bench_function` 测量。
  - 模块化组织（开闭原则，REQ-NF-MAINT-002）：本文件仅含 CRUD 维度，不依赖其他维度模块。
- **验收标准**：
  - [ ] `cargo bench --bench bench_crud` 运行完成，输出 CRUD 单条+批量在四档规模下的测量值。
  - [ ] 四竞品（sz-orm/Diesel/SeaORM/SQLx）均运行 CRUD 维度。
  - [ ] criterion 报告含均值/中位数/StdDev/置信区间。
  - [ ] 零 `todo!`/`unimplemented!`/`unsafe`。
- **关联需求**：REQ-B-001, REQ-B-002, REQ-B-007, DFX-PERF-005, REQ-NF-MAINT-002
- **关联设计章节**：design.md §4.1（bench 场景定义）、§4.5（criterion 配置）

---

#### T-B-003：bench_relation.rs 关联查询维度基准（1:1/1:N/N:M）

- **所属里程碑**：M3
- **优先级**：中
- **依赖任务**：T-B-001
- **涉及文件**：
  - `bench-comparison/benches/bench_relation.rs`（新增：关联查询基准）
- **任务描述**：
  - 实现 1:1 HasOne 关联查询基准（REQ-B-003）：四档规模。
  - 实现 1:N HasMany 关联查询基准：四档规模。
  - 实现 N:M ManyToMany 关联查询基准：四档规模。
  - 通过 `CompetitorAdapter` trait 调用四竞品，SQLx 返回 `Unsupported` 时标注 N/A。
  - 模块化组织。
- **验收标准**：
  - [ ] `cargo bench --bench bench_relation` 运行完成，输出三种关联类型在四档规模下的测量值。
  - [ ] SQLx 关联查询标注 N/A（Unsupported），非空缺。
  - [ ] criterion 报告完整。
  - [ ] 零 `todo!`/`unimplemented!`/`unsafe`。
- **关联需求**：REQ-B-003, REQ-B-007, REQ-B-011, DFX-PERF-005, REQ-NF-MAINT-002
- **关联设计章节**：design.md §4.1（bench 场景定义）

---

#### T-B-004：bench_transaction.rs 事务维度基准

- **所属里程碑**：M3
- **优先级**：中
- **依赖任务**：T-B-001
- **涉及文件**：
  - `bench-comparison/benches/bench_transaction.rs`（新增：事务基准）
- **任务描述**：
  - 实现单事务提交基准（REQ-B-004）。
  - 实现多语句事务基准。
  - 实现事务回滚基准。
  - 实现嵌套事务/savepoint 基准（竞品不支持时标注 N/A）。
  - 四档规模，通过 `CompetitorAdapter` trait 调用四竞品。
  - 模块化组织。
- **验收标准**：
  - [ ] `cargo bench --bench bench_transaction` 运行完成，输出四类事务操作测量值。
  - [ ] savepoint 不支持的竞品标注 N/A。
  - [ ] 零 `todo!`/`unimplemented!`/`unsafe`。
- **关联需求**：REQ-B-004, REQ-B-007, REQ-B-011, DFX-PERF-005, REQ-NF-MAINT-002
- **关联设计章节**：design.md §4.1（bench 场景定义）

---

#### T-B-005：bench_pool.rs 连接池维度基准

- **所属里程碑**：M3
- **优先级**：中
- **依赖任务**：T-B-001
- **涉及文件**：
  - `bench-comparison/benches/bench_pool.rs`（新增：连接池基准）
- **任务描述**：
  - 实现空闲池获取基准（REQ-B-005）。
  - 实现池满等待基准。
  - 实现并发竞争获取基准。
  - 四档规模，通过 `CompetitorAdapter` trait 调用四竞品。
  - 模块化组织。
- **验收标准**：
  - [ ] `cargo bench --bench bench_pool` 运行完成，输出三类连接池获取测量值。
  - [ ] 零 `todo!`/`unimplemented!`/`unsafe`。
- **关联需求**：REQ-B-005, REQ-B-007, DFX-PERF-005, REQ-NF-MAINT-002
- **关联设计章节**：design.md §4.1（bench 场景定义）

---

#### T-B-006：bench_pagination.rs 分页维度基准

- **所属里程碑**：M3
- **优先级**：中
- **依赖任务**：T-B-001
- **涉及文件**：
  - `bench-comparison/benches/bench_pagination.rs`（新增：分页基准）
- **任务描述**：
  - 实现 OFFSET/LIMIT 分页基准（REQ-B-006）。
  - 实现游标分页（Keyset）基准（竞品不支持时标注 N/A）。
  - 四档规模，通过 `CompetitorAdapter` trait 调用四竞品。
  - 模块化组织。
- **验收标准**：
  - [ ] `cargo bench --bench bench_pagination` 运行完成，输出两类分页查询测量值。
  - [ ] 游标分页不支持的竞品标注 N/A。
  - [ ] 零 `todo!`/`unimplemented!`/`unsafe`。
- **关联需求**：REQ-B-006, REQ-B-007, REQ-B-011, DFX-PERF-005, REQ-NF-MAINT-002
- **关联设计章节**：design.md §4.1（bench 场景定义）

---

#### T-B-007：criterion 配置 + full_comparison.rs 主入口

- **所属里程碑**：M3
- **优先级**：中
- **依赖任务**：T-B-002, T-B-003, T-B-004, T-B-005, T-B-006
- **涉及文件**：
  - `bench-comparison/benches/full_comparison.rs`（新增：主入口，聚合 5 维度模块）
  - `bench-comparison/Cargo.toml`（修改：注册 full_comparison bench）
- **任务描述**：
  - 创建 `full_comparison.rs` 主入口，聚合 5 个维度模块（bench_crud/relation/transaction/pool/pagination）。
  - 配置 criterion（design.md §4.5）：`sample_size=100`、`warm_up_time=3s`、`measurement_time=10s`、`confidence_level=0.95`、`noise_threshold=0.05`。
  - 性能约束优化（REQ-NF-PERF-004）：SQLite 始终运行；MySQL/PG 远程仅运行规模 100/1000/10000（跳过 10）；竞品基准并行进程。
  - 通过环境变量触发方言：`DATABASE_URL_MYSQL`/`DATABASE_URL_POSTGRES`/`DATABASE_URL_ORACLE`/`DATABASE_URL_MSSQL`，SQLite 始终运行。
  - 自定义 main（`harness = false`）聚合所有维度。
- **验收标准**：
  - [ ] `cargo bench --bench full_comparison` 运行完成，输出全维度 × 四档规模 × 四竞品测量值。
  - [ ] criterion 配置生效（sample_size=100 等）。
  - [ ] SQLite 始终运行，MySQL/PG 按环境变量触发。
  - [ ] 全套件运行时间 ≤ 30 分钟（REQ-NF-PERF-004）。
  - [ ] 零 `todo!`/`unimplemented!`/`unsafe`。
- **关联需求**：REQ-B-001, REQ-B-002, REQ-B-003, REQ-B-004, REQ-B-005, REQ-B-006, REQ-B-007, REQ-B-013, REQ-B-014, REQ-B-015, DFX-PERF-005, DFX-REL-004, REQ-NF-PERF-004, REQ-NF-COMPAT-003, REQ-CON-DIALECT-002
- **关联设计章节**：design.md §4.1（bench 场景定义）、§4.3（多方言基准运行策略）、§4.5（criterion 配置）

---

### 里程碑 M4：任务 B 报告 + 任务 A 准备

#### T-B-008：BenchmarkReporter 报告生成器

- **所属里程碑**：M4
- **优先级**：中
- **依赖任务**：T-B-007
- **涉及文件**：
  - `bench-comparison/benches/benchmark_reporter.rs`（新增：BenchmarkReporter + BenchmarkRecord + EnvironmentMetadata + AuditReport）
- **任务描述**：
  - 定义 `BenchmarkRecord` 结构（design.md §7.4）：dimension/dialect/competitor/mean_ns/median_ns/p95_ns/throughput_ops_per_sec/dataset_size，派生 `Serialize`。
  - 定义 `EnvironmentMetadata` 结构（REQ-B-020）：cpu/memory_gb/disk/rust_version/db_versions/criterion_config/dataset_sizes。
  - 定义 `AuditReport` 结构（REQ-B-023）：异常值检测报告。
  - 实现 `BenchmarkReporter`：
    - `generate_markdown() -> String`：生成 Markdown 报告，含全维度 × 多方言 × 竞品对比数据表（REQ-B-018）。
    - `generate_chart_data() -> (String, String)`：生成 CSV + JSON 图表数据（REQ-B-019）。
    - `mask_dsn(dsn) -> String`：DSN 密码替换为 `***`（REQ-B-021, DFX-SEC-003）。
    - `audit() -> AuditReport`：检测 0 延迟、负吞吐量等异常（REQ-B-023）。
    - `generate_repro_instructions() -> String`：生成复现指令（REQ-B-022）。
  - 报告含环境元数据章节（硬件/Rust 版本/DB 版本/criterion 配置/数据集规模）。
  - 报告含"差异说明"章节：列明 Diesel 同步 vs sz-orm 异步、SQLx 无 ORM 抽象、SeaORM SmartLoader 差异（REQ-B-012）。
  - 报告含"方言差异说明"章节：同维度跨方言差异 >2 倍时标注原因（REQ-B-017）。
- **验收标准**：
  - [ ] `generate_markdown()` 输出含全维度数据表的 Markdown。
  - [ ] `generate_chart_data()` 输出合法 CSV + JSON。
  - [ ] `mask_dsn()` 将密码替换为 `***`，不含明文凭据。
  - [ ] `audit()` 检测异常值（0 延迟、负吞吐量）。
  - [ ] 报告含环境元数据 + 差异说明 + 方言差异说明 + 复现指令章节。
  - [ ] `cargo test -p bench-comparison --lib benchmark_reporter` 单元测试通过。
  - [ ] 零 `todo!`/`unimplemented!`/`unsafe`。
- **关联需求**：REQ-B-012, REQ-B-017, REQ-B-018, REQ-B-019, REQ-B-020, REQ-B-021, REQ-B-022, REQ-B-023, DFX-SEC-003, DFX-MAINT-003, REQ-NF-SEC-004
- **关联设计章节**：design.md §4.4（报告生成器设计）、§7.4（基准测量记录）

---

#### T-B-009：多方言基准运行（MySQL/PG/SQLite + Oracle/MSSQL 尽力）⚠ 高风险

- **所属里程碑**：M4
- **优先级**：中
- **依赖任务**：T-B-008
- **涉及文件**：
  - `bench-comparison/benches/full_comparison.rs`（修改：集成 BenchmarkReporter，多方言运行）
- **任务描述**：
  - 在 `full_comparison.rs` 中集成 `BenchmarkReporter`：基准运行完成后聚合 criterion 输出，生成报告。
  - MySQL 方言运行（REQ-B-013）：`DATABASE_URL_MYSQL` 触发，输出 MySQL 全维度数据。
  - PostgreSQL 方言运行（REQ-B-014）：`DATABASE_URL_POSTGRES` 触发。
  - SQLite 方言运行（REQ-B-015）：始终运行（in-memory）。
  - Oracle/MSSQL 尽力覆盖（REQ-B-016）：`DATABASE_URL_ORACLE`/`DATABASE_URL_MSSQL` 触发，竞品不支持时标注"部分覆盖"。
  - 方言差异说明（REQ-B-017）：同维度跨方言差异 >2 倍时报告标注原因（SQLite 无网络开销、PG MVCC、Oracle IN 上限等）。
  - 生成报告文件：`benchmark-report.md` + `benchmark-data.csv` + `benchmark-data.json`。
- **验收标准**：
  - [ ] 设置 `DATABASE_URL_MYSQL`/`DATABASE_URL_POSTGRES` 后，`cargo bench --bench full_comparison` 输出三方言完整数据。
  - [ ] SQLite 始终运行，无需环境变量。
  - [ ] Oracle/MSSQL 按环境变量运行，未覆盖维度标注原因。
  - [ ] 生成 `benchmark-report.md` + `benchmark-data.csv` + `benchmark-data.json` 三个文件。
  - [ ] 报告中 DSN 密码显示 `***`。
  - [ ] 同维度跨方言差异 >2 倍时报告标注差异原因。
  - [ ] 零 `todo!`/`unimplemented!`/`unsafe`。
- **关联需求**：REQ-B-013, REQ-B-014, REQ-B-015, REQ-B-016, REQ-B-017, REQ-B-018, REQ-B-019, REQ-B-021, DFX-COMPAT-005, REQ-NF-COMPAT-003, REQ-CON-DIALECT-002
- **关联设计章节**：design.md §4.3（多方言基准运行策略）、§4.3.1（方言触发策略）、§4.3.2（方言差异说明）
- **风险说明**：多方言基准依赖真实数据库实例可用性，且全套件耗时需控制在 30 分钟内。

---

#### T-B-010：基准报告内部审查 + 复现指令验证

- **所属里程碑**：M4
- **优先级**：中
- **依赖任务**：T-B-009
- **涉及文件**：
  - `bench-comparison/benches/benchmark_reporter.rs`（修改：完善 audit + 复现指令）
  - `docs/spec/v2.3.0/benchmark-report.md`（生成物，审查对象）
- **任务描述**：
  - 运行 `BenchmarkReporter::audit()` 对生成的报告进行内部审查（REQ-B-023）：检测 0 延迟、负吞吐量、遗漏维度。
  - 验证复现指令（REQ-B-022）：按报告中的 `cargo bench --bench full_comparison` 命令独立复现测量结果。
  - 验证报告可复现性（DFX-REL-004）：相同硬件相同数据集波动 ≤15%。
  - 确认报告含完整环境元数据（硬件/Rust 版本/DB 版本/criterion 配置/数据集规模）。
  - 确认报告含"差异说明"章节（Diesel 同步 vs sz-orm 异步等非对等因素）。
  - 签字确认报告无异常值 + 全维度覆盖。
- **验收标准**：
  - [ ] `audit()` 输出无异常值（无 0 延迟、无负吞吐量）。
  - [ ] 报告含"复现步骤"章节，按指令可独立复现。
  - [ ] 相同硬件相同数据集重复运行波动 ≤15%。
  - [ ] 报告环境元数据章节完整。
  - [ ] 报告"差异说明"章节列明所有非对等因素。
  - [ ] 全维度（6 维度 × 4 规模 × 4 竞品 × 3 方言）覆盖，无遗漏。
- **关联需求**：REQ-B-012, REQ-B-020, REQ-B-022, REQ-B-023, DFX-REL-004, DFX-MAINT-003
- **关联设计章节**：design.md §4.4（报告生成器设计）、§9.3（基准测试）

---

#### T-A-001：sz-pay Cargo.toml 依赖升级 2.1.0→2.3.0（经 2.2.0）⚠ 高风险

- **所属里程碑**：M4
- **优先级**：高
- **依赖任务**：T-C-010（smart() 验证依赖任务 C 完成）
- **涉及文件**：
  - `E:\vue\test\sz-pay\server\sz-rust\Cargo.toml`（修改：7 个 sz-orm 包版本 2.1.0 → 2.3.0）
- **任务描述**：
  - 将 sz-pay 的 `Cargo.toml` 中 7 个 sz-orm 包版本声明从 `2.1.0` 改为 `2.3.0`：sz-orm-core、sz-orm-sqlx、sz-orm-config、sz-orm-auth、sz-orm-macros、sz-orm-queue、sz-orm-scheduler。
  - 升级路径（DFX-COMPAT-004, REQ-NF-COMPAT-004）：2.1.0 → 2.2.0 → 2.3.0，每步零编译错误。
  - 若 v2.3.0 尚未发布 crates.io，可临时使用 path 依赖或 git 依赖指向本地 sz-orm 仓库，待 T-INT-003 发布后切回 crates.io 版本声明。
  - ADR-0001 合规：仅修改 sz-pay 自身 `Cargo.toml`，严禁修改 sz-orm 仓库文件。
- **验收标准**：
  - [ ] sz-pay `Cargo.toml` 中 7 个 sz-orm 包版本为 `2.3.0`（或临时 path/git 依赖）。
  - [ ] `cargo build --workspace`（在 sz-pay 目录）零编译错误（仅允许 deprecation warning）。
  - [ ] 升级路径 2.1.0→2.2.0→2.3.0 每步可编译。
  - [ ] sz-orm 仓库 `git diff --name-only` 为空（ADR-0001 合规，sz-orm 自身开发除外）。
  - [ ] 若编译失败，生成阻断报告含失败用例与原因（REQ-A-003）。
- **关联需求**：REQ-A-001, REQ-A-002, REQ-A-003, DFX-COMPAT-004, REQ-NF-COMPAT-004, REQ-CON-ADR0001-001
- **关联设计章节**：design.md §5.1（依赖升级方案）、§5.1.1（升级路径）、§5.1.3（ADR-0001 合规）
- **风险说明**：sz-pay 升级可能遇到 deprecation 或 API 不兼容，需逐步升级并适配。

---

#### T-A-002：sz-pay 编译兼容验证 + deprecation 适配

- **所属里程碑**：M4
- **优先级**：高
- **依赖任务**：T-A-001
- **涉及文件**：
  - `E:\vue\test\sz-pay\server\sz-rust\`（修改：适配 deprecation warning，如 `where_cond` → `where_eq`）
- **任务描述**：
  - 在 sz-pay 目录运行 `cargo build --workspace`，收集所有编译错误与 deprecation warning。
  - 逐项适配 deprecation warning：如 `where_cond` → `where_eq`、`or_where` → `or_where_eq`（AGENTS.md 约束）。
  - 若存在编译错误，逐项定位失败原因（REQ-A-018），区分"sz-orm 回归"与"sz-pay 自身问题"：
    - sz-orm 回归：在 sz-orm 仓库修复（sz-orm 自身开发）。
    - sz-pay 自身问题：修改 sz-pay 代码适配 v2.3.0 API。
  - 提供每项修复的 file:line 证据（REQ-CON-AUDIT-001）。
  - 修复后重新运行 `cargo build --workspace` 验证零错误。
- **验收标准**：
  - [ ] `cargo build --workspace`（sz-pay 目录）零编译错误。
  - [ ] deprecation warning 逐项适配（如 `where_cond` → `where_eq`）。
  - [ ] 编译失败时逐项定位 + 分类 + file:line 证据 + 修复建议。
  - [ ] 修复后 `cargo build --workspace` 零错误。
  - [ ] ADR-0001 合规：sz-orm 仓库无下游引入的修改。
- **关联需求**：REQ-A-002, REQ-A-003, REQ-A-018, DFX-COMPAT-004, REQ-CON-AUDIT-001, REQ-CON-AUDIT-002, REQ-CON-ADR0001-001
- **关联设计章节**：design.md §5.1.2（兼容性验证流程）、§5.4.2（失败处理流程）

---

#### T-A-003：sz-pay 全量测试回归验证（5,139 基线）⚠ 高风险

- **所属里程碑**：M4
- **优先级**：高
- **依赖任务**：T-A-002
- **涉及文件**：
  - `E:\vue\test\sz-pay\server\sz-rust\`（修改：修复回归测试失败，若存在）
- **任务描述**：
  - 在 sz-pay 目录运行 `cargo test --workspace`，验证 5,139 基线测试 100% 通过（DFX-REL-001, REQ-A-004）。
  - 若出现测试失败，按 design.md §5.4.2 流程：
    1. 逐项定位失败用例。
    2. 分类："sz-orm 回归"（sz-orm v2.3.0 bug → sz-orm 仓库修复）vs "sz-pay 自身问题"（sz-pay 代码与 v2.3.0 不兼容 → 修改 sz-pay）。
    3. 提供每项失败的 file:line 证据（REQ-CON-AUDIT-001）。
    4. 提供修复建议。
    5. 修复后重新运行 `cargo test --workspace` 验证（REQ-CON-AUDIT-002）。
  - 若为 sz-orm 回归且无法快速修复，回滚至 2.1.0 并生成阻断报告（REQ-A-003）。
- **验收标准**：
  - [ ] `cargo test --workspace`（sz-pay 目录）通过数 ≥ 5,139 且失败数 = 0。
  - [ ] 测试失败时逐项定位 + 分类 + file:line 证据 + 修复建议。
  - [ ] 修复后重新运行 `cargo test` 验证通过。
  - [ ] 若回滚，生成阻断报告含失败用例与原因。
  - [ ] ADR-0001 合规验证。
- **关联需求**：REQ-A-003, REQ-A-004, REQ-A-018, DFX-REL-001, REQ-CON-AUDIT-001, REQ-CON-AUDIT-002, REQ-CON-ADR0001-001
- **关联设计章节**：design.md §5.1.2（兼容性验证流程）、§5.4（回归测试策略）、§5.4.2（失败处理流程）
- **风险说明**：5,139 基线测试零回归是任务 A 的生死线，任何回归都需逐项定位并提供证据。

---

#### T-A-004：sz-pay 新功能验证用例

- **所属里程碑**：M4
- **优先级**：高
- **依赖任务**：T-A-003, T-C-010
- **涉及文件**：
  - `E:\vue\test\sz-pay\server\sz-rust\`（新增：验证用例测试文件）
- **任务描述**：
  - 在 sz-pay 内部新增 v2.2.0+ 新功能验证用例（不修改 sz-orm 仓库）：
    1. 多级 Eager Loading 验证（REQ-A-005）：商户 → 订单 → 订单明细 → 商品，`EagerLoader::with().with().load_nested()` 返回嵌套结构与逐级手动查询一致。
    2. Schema Sync 破坏性验证（REQ-A-006）：`destructive_sync()` 含列重命名检测 + 迁移钩子 + 数据不丢失。
    3. Stream API 背压验证（REQ-A-007）：`stream_with_backpressure(buffer_size)` 流式导出 10 万行订单，内存占用稳定在 buffer_size 量级。
    4. cascade_delete 验证（REQ-A-008）：SET_NULL 级联，订单明细外键置 NULL，明细行不删除。
    5. Partial Models 验证（REQ-A-009）：`select_exclude("敏感字段")` 返回实体不含敏感字段 + SQL 未查询该列。
    6. smart() 智能加载验证（REQ-A-010）：`EagerLoader::smart().with().load()` 结果与手动 Eager Loading 一致 + 策略日志可查。
  - 所有验证用例使用 sz-pay 真实业务场景与真实数据库。
- **验收标准**：
  - [ ] 6 个验证用例全部通过（`cargo test --workspace` 含新增用例）。
  - [ ] 多级 Eager Loading 嵌套结构与手动查询一致。
  - [ ] Schema Sync 列重命名检测 + 数据不丢失。
  - [ ] Stream API 内存占用稳定在 buffer_size 量级。
  - [ ] cascade_delete SET_NULL 行为正确。
  - [ ] Partial Models 不含敏感字段 + SQL 未查询该列。
  - [ ] smart() 结果与手动一致 + 策略日志可查。
  - [ ] ADR-0001 合规：仅新增 sz-pay 测试文件，未修改 sz-orm 仓库。
- **关联需求**：REQ-A-005, REQ-A-006, REQ-A-007, REQ-A-008, REQ-A-009, REQ-A-010, REQ-CON-ADR0001-001
- **关联设计章节**：design.md §5.2（新功能生产验证用例设计）

---

### 里程碑 M5：任务 A 深化 + 集成验证

#### T-A-005：sz-pay 性能采集工具（QPS/P50/P95/P99/峰值内存）

- **所属里程碑**：M5
- **优先级**：中
- **依赖任务**：T-A-004
- **涉及文件**：
  - `E:\vue\test\sz-pay\server\sz-rust\`（新增：性能采集工具/脚本）
- **任务描述**：
  - 在 sz-pay 内部新增性能采集工具，采集以下指标（DFX-PERF-006）：
    - QPS（REQ-A-011）：计数器/时间窗口，覆盖支付下单、订单查询、商户结算场景。
    - P50/P95/P99 延迟（REQ-A-012）：hdrhistogram 直方图，覆盖核心数据库操作。
    - 峰值内存（REQ-A-013）：进程内存采样（/proc 或 GetProcessMemoryInfo），含连接池 + 查询缓冲。
  - 定义 `SzPayPerformanceRecord` 结构（design.md §7.5）：scenario/qps/p50_ms/p95_ms/p99_ms/peak_memory_mb/orm_version/collected_at。
  - 采集流程（design.md §5.3.2）：在 sz-pay 测试环境部署 v2.3.0（REQ-A-015：不在生产环境采集）→ 运行真实业务负载 → 采集指标 → 清理临时文件与测试进程（REQ-NF-MAINT-005）。
  - 安全约束（DFX-SEC-004）：仅采集聚合指标，不记录敏感业务数据。
- **验收标准**：
  - [ ] 性能采集工具可运行，输出 QPS/P50/P95/P99/峰值内存五项指标。
  - [ ] 覆盖支付下单、订单查询、商户结算三个核心场景。
  - [ ] 采集在测试环境执行，不影响生产服务（REQ-A-015）。
  - [ ] 采集完成后清理临时文件与测试进程（REQ-NF-MAINT-005）。
  - [ ] 性能数据仅含聚合指标，无敏感业务数据（DFX-SEC-004）。
  - [ ] `SzPayPerformanceRecord` 序列化输出合法 JSON。
- **关联需求**：REQ-A-011, REQ-A-012, REQ-A-013, REQ-A-015, DFX-PERF-006, DFX-SEC-004, REQ-NF-MAINT-005
- **关联设计章节**：design.md §5.3（性能数据采集方案）、§5.3.1（采集指标）、§5.3.2（采集流程）、§7.5（性能数据记录）

---

#### T-A-006：sz-pay v2.1.0 vs v2.3.0 性能对比报告

- **所属里程碑**：M5
- **优先级**：中
- **依赖任务**：T-A-005
- **涉及文件**：
  - `E:\vue\test\sz-pay\server\sz-rust\`（新增：对比报告生成）
  - `docs/spec/v2.3.0/sz-pay-performance-comparison.md`（生成物）
- **任务描述**：
  - 在相同负载下分别采集 v2.1.0 与 v2.3.0 性能数据（REQ-A-014）。
  - 生成对比报告：标注各场景 QPS/延迟/内存的提升/持平/退化项。
  - 退化项定位并分析原因（DFX-PERF-006）。
  - 报告中 DSN 脱敏（REQ-NF-SEC-004）。
  - 清理采集过程中的临时文件与测试进程（REQ-NF-MAINT-005）。
- **验收标准**：
  - [ ] 生成 `sz-pay-performance-comparison.md` 对比报告。
  - [ ] 报告含 v2.1.0 与 v2.3.0 两组性能数据对比表。
  - [ ] 各场景标注提升/持平/退化项。
  - [ ] 退化项附原因分析。
  - [ ] 报告 DSN 密码显示 `***`。
  - [ ] 采集完成后临时文件删除 + 测试进程释放。
- **关联需求**：REQ-A-014, REQ-A-015, DFX-PERF-006, DFX-SEC-004, REQ-NF-SEC-004, REQ-NF-MAINT-005
- **关联设计章节**：design.md §5.3（性能数据采集方案）、§5.3.2（采集流程）

---

#### T-A-007：sz-pay 测试基线文档维护

- **所属里程碑**：M5
- **优先级**：中
- **依赖任务**：T-A-003, T-A-004
- **涉及文件**：
  - `docs/spec/v2.3.0/sz-pay-test-baseline.md`（新增：测试基线文档）
- **任务描述**：
  - 维护 sz-pay 测试基线文档（REQ-A-016）：记录 v2.1.0 基线（5,139 测试）与 v2.3.0 升级后测试通过数对比。
  - 将 v2.3.0 新增验证用例（T-A-004 的 6 个用例）纳入基线（REQ-A-017），更新为 5,139 + N。
  - 文档含：v2.1.0 通过数、v2.3.0 通过数、差值、回归项清单（若有）。
  - 若存在回归项，附 file:line 证据与修复记录（REQ-A-018）。
- **验收标准**：
  - [ ] 生成 `sz-pay-test-baseline.md` 基线文档。
  - [ ] 文档含 v2.1.0 通过数（5,139）、v2.3.0 通过数、差值、回归项清单。
  - [ ] 新增用例纳入基线，文档同步更新为 5,139 + N。
  - [ ] 回归项附 file:line 证据与修复记录。
- **关联需求**：REQ-A-016, REQ-A-017, REQ-A-018, DFX-REL-001, REQ-CON-AUDIT-001
- **关联设计章节**：design.md §5.4（回归测试策略）、§5.4.1（测试基线维护）

---

#### T-INT-001：全 workspace 10 道门禁验证 ⚠ 高风险

- **所属里程碑**：M5
- **优先级**：高
- **依赖任务**：T-C-010, T-C-011, T-B-010, T-A-004
- **涉及文件**：
  - `scripts/gate.ps1`（运行：10 道门禁脚本）
  - 全 sz-orm workspace
- **任务描述**：
  - 运行 sz-orm 仓库 10 道门禁（AGENTS.md + REQ-CON-GATE-001），使用 `CARGO_INCREMENTAL=0`（Windows MSVC rustc 栈溢出 bug）：
    1. `cargo fmt --all -- --check`（格式检查）
    2. `cargo check --workspace --all-targets`（编译检查）
    3. `cargo clippy --workspace --all-targets -- -D warnings`（静态分析）
    4. `cargo test --workspace`（单元/集成测试）
    5. `cargo doc --workspace --no-deps --all-features`（文档构建）
    6. `cargo audit` + `cargo deny check`（安全审计）
    7. `cargo test --workspace -- --ignored`（真实服务集成测试）
    8. `grep -rn 'todo!\|unimplemented!\|unreachable!' --include='*.rs'`（占位检查，须为空）
    9. `scripts/check-sql-injection.ps1`（SQL 注入扫描）
    10. `cargo check --workspace --all-targets --all-features`（Feature 全组合编译）
  - 任一门禁失败则修复并重新运行，直至全部通过。
  - 附每道门禁的运行输出作为证据（REQ-CON-AUDIT-002）。
- **验收标准**：
  - [ ] 10 道门禁全部通过，附每道门禁运行输出。
  - [ ] `cargo fmt --all -- --check` 零差异。
  - [ ] `cargo clippy --workspace --all-targets -- -D warnings` 零警告。
  - [ ] `cargo test --workspace` 全部通过。
  - [ ] 占位检查 grep 输出为空（零 `todo!`/`unimplemented!`/`unreachable!`）。
  - [ ] SQL 注入扫描通过（全部参数化）。
  - [ ] Feature 全组合编译零错误。
- **关联需求**：REQ-CON-GATE-001, REQ-NF-TEST-005, REQ-NF-TEST-006, REQ-NF-MAINT-003, REQ-NF-MAINT-004, REQ-CON-NOPLACEHOLDER-001, REQ-CON-UNSAFE-001, REQ-CON-PARAMQUERY-001, REQ-CON-PARAMQUERY-002, REQ-CON-AUDIT-002
- **关联设计章节**：design.md §9.5（门禁与质量检查）
- **风险说明**：10 道门禁是入库生死线，任一失败必须修复，clippy `-D warnings` 对新增代码要求严格。

---

#### T-INT-002：ADR-0001 合规验证

- **所属里程碑**：M5
- **优先级**：高
- **依赖任务**：T-A-004
- **涉及文件**：
  - sz-orm 仓库（验证：`git diff --name-only`）
  - sz-pay 仓库（验证：已修改文件清单）
- **任务描述**：
  - 验证 ADR-0001 合规（REQ-CON-ADR0001-001）：任务 A 仅修改 sz-pay 项目文件与依赖版本，不修改 sz-orm 仓库文件（sz-orm 自身开发除外）。
  - 在 sz-orm 仓库运行 `git diff --name-only HEAD`，确认所有修改均为 sz-orm 自身开发（任务 B/C），无下游引入的修改。
  - 在 sz-pay 仓库确认修改文件清单（Cargo.toml + 业务代码 + 测试用例 + 性能工具）。
  - 若检测到 sz-orm 仓库被下游误修改，回滚并记录违规（REQ-CON-ADR0001-002）。
- **验收标准**：
  - [ ] sz-orm 仓库 `git diff --name-only HEAD` 输出仅含 sz-orm 自身开发文件（任务 B/C）。
  - [ ] sz-pay 仓库修改文件清单符合预期（Cargo.toml + 业务代码 + 测试 + 性能工具）。
  - [ ] 无 ADR-0001 违规记录。
  - [ ] 若有违规，已回滚并记录。
- **关联需求**：REQ-CON-ADR0001-001, REQ-CON-ADR0001-002
- **关联设计章节**：design.md §5.1.3（ADR-0001 合规）、§10.5（ADR-0001 合规风险）

---

#### T-INT-003：v2.3.0 版本 bump + crates.io 发布 ⚠ 高风险

- **所属里程碑**：M5
- **优先级**：高
- **依赖任务**：T-INT-001, T-INT-002
- **涉及文件**：
  - `Cargo.toml`（workspace 根，修改：version = "2.3.0"）
  - 各包 `Cargo.toml`（修改：版本同步至 2.3.0）
  - `CHANGELOG.md`（修改：新增 v2.3.0 变更记录）
- **任务描述**：
  - 将 workspace 根 `Cargo.toml` 的 `workspace.package.version` 从当前版本 bump 至 `2.3.0`。
  - 同步所有 43 个包的版本至 `2.3.0`（继承 workspace 版本）。
  - 更新 `CHANGELOG.md`：新增 v2.3.0 变更记录，含任务 A/B/C 交付物、新增 API、deprecation 说明。
  - 若有 Breaking Change（经架构评审批准），在 CHANGELOG 显著标注并提供迁移指南（REQ-CON-COMPAT-002）。
  - 运行 `cargo publish` 发布 sz-orm-core（及其他需发布包）至 crates.io（REQ-A-001）。
  - 发布后验证 crates.io 存在 sz-orm-core 2.3.0。
  - 将 sz-pay 的临时 path/git 依赖（若 T-A-001 使用）切回 crates.io 版本声明 `2.3.0`，重新验证编译。
- **验收标准**：
  - [ ] workspace 根与所有包版本为 `2.3.0`。
  - [ ] `CHANGELOG.md` 含 v2.3.0 变更记录。
  - [ ] `cargo publish` 成功，crates.io 存在 sz-orm-core 2.3.0。
  - [ ] sz-pay 依赖切回 crates.io `2.3.0`，`cargo build --workspace` 零错误。
  - [ ] 若有 Breaking Change，CHANGELOG 显著标注 + 迁移指南。
- **关联需求**：REQ-A-001, REQ-A-002, REQ-CON-COMPAT-001, REQ-CON-COMPAT-002, DFX-COMPAT-004
- **关联设计章节**：design.md §5.1.1（升级路径）
- **风险说明**：crates.io 发布不可撤销，发布前必须确保 10 道门禁全过 + sz-pay 验证通过。

---

#### T-INT-004：审计合规证据收集 + 最终验证报告

- **所属里程碑**：M5
- **优先级**：高
- **依赖任务**：T-INT-003
- **涉及文件**：
  - `docs/spec/v2.3.0/v2.3.0-verification-report.md`（新增：最终验证报告）
  - `scripts/audit-verify.ps1`（运行：审计证据验证脚本）
- **任务描述**：
  - 收集 v2.3.0 所有交付物的审计合规证据（REQ-CON-AUDIT-001）：
    - 任务 C：smart_eager_loader.rs + n1_eliminator.rs + relation_trait.rs 扩展的 file:line 证据 + 单元/集成测试输出。
    - 任务 B：基准报告 + 报告生成器 file:line 证据 + 基准运行输出。
    - 任务 A：sz-pay 升级 + 回归测试 + 性能对比的 file:line 证据 + cargo test 输出。
    - 集成验证：10 道门禁输出 + ADR-0001 合规验证 + crates.io 发布证据。
  - 生成最终验证报告 `v2.3.0-verification-report.md`：含每项验收结论 + file:line 证据 + 测试输出。
  - 运行 `scripts/audit-verify.ps1` 验证报告中所有 file:line 引用真实存在（AGENTS.md 审计合规铁律）。
  - 确认所有结论附证据，无"已修复""应该没问题"等无证据结论。
- **验收标准**：
  - [ ] 生成 `v2.3.0-verification-report.md` 最终验证报告。
  - [ ] 报告含任务 A/B/C + 集成验证的所有验收结论。
  - [ ] 每条结论附 file:line 证据 + cargo test 输出。
  - [ ] `scripts/audit-verify.ps1` 验证所有 file:line 引用真实存在。
  - [ ] 无"已修复""应该没问题"等无证据结论。
  - [ ] 99 条 EARS 需求 100% 覆盖且验收通过。
- **关联需求**：REQ-CON-AUDIT-001, REQ-CON-AUDIT-002, REQ-CON-GATE-001
- **关联设计章节**：design.md §8（错误处理设计）、§9（测试设计）、§10（风险与缓解）

---

## 四、里程碑验收标准

### M1 验收标准

- [ ] `smart_eager_loader.rs` 新增文件存在，含 `SmartEagerLoader` / `StrategyResolver` / `LoadStrategy` / `StrategyDecision` / `JoinStrategy` / `DataLoaderStrategy` / `IntermediateTableStrategy` 类型。
- [ ] `relation_trait.rs` 的 `RelationDef` 含 3 个中间表字段 + `new_many_to_many()` 构造器，`new()` 签名不变。
- [ ] `eager_loader.rs` 含 `smart()` 扩展方法，已有 API 签名不变。
- [ ] `cargo check -p sz-orm-core` 零错误；`cargo clippy -p sz-orm-core -- -D warnings` 零警告。
- [ ] 策略决策器对四种关联类型决策正确（单元测试通过）。
- [ ] 三种策略执行器可独立执行（Mock 模式单元测试通过）。
- [ ] 零 `todo!`/`unimplemented!`/`unreachable!`/`unsafe`。

### M2 验收标准

- [ ] `n1_eliminator.rs` 新增文件存在，含 `N1Eliminator` / `N1EliminationReport` / `PendingQuery` 类型。
- [ ] `SmartEagerLoader::load()` 完整实现，集成 N+1 消除 + 循环检测 + 决策日志。
- [ ] 智能模式与手动模式结果 100% 等价（差分测试通过）。
- [ ] N+1 自动消除正确合并且结果等价；不等价回退逐条执行。
- [ ] 五方言（MySQL/PG/SQLite/Oracle/MSSQL）智能模式结果正确且一致（`--ignored` 测试通过）。
- [ ] 策略决策延迟 ≤ 100μs（基准验证通过）。
- [ ] `cargo test -p sz-orm-core` 全部通过（单元 + 集成）。
- [ ] 零 `todo!`/`unimplemented!`/`unreachable!`/`unsafe`。

### M3 验收标准

- [ ] `competitor_adapter.rs` 含 `CompetitorAdapter` trait + 四竞品适配实现。
- [ ] 5 个维度基准模块（bench_crud/relation/transaction/pool/pagination）存在且可独立运行。
- [ ] `full_comparison.rs` 主入口聚合 5 维度，criterion 配置生效。
- [ ] `cargo bench --bench full_comparison` 运行完成，输出全维度 × 四档规模 × 四竞品测量值。
- [ ] SQLx 关联查询标注 N/A（Unsupported），非空缺。
- [ ] criterion 报告含均值/中位数/StdDev/置信区间。
- [ ] 全套件运行时间 ≤ 30 分钟。
- [ ] 零 `todo!`/`unimplemented!`/`unreachable!`/`unsafe`。

### M4 验收标准

- [ ] `benchmark_reporter.rs` 含 `BenchmarkReporter` + `generate_markdown` + `generate_chart_data` + `mask_dsn` + `audit`。
- [ ] 生成 `benchmark-report.md` + `benchmark-data.csv` + `benchmark-data.json`，DSN 密码脱敏。
- [ ] 报告含环境元数据 + 差异说明 + 方言差异说明 + 复现指令章节。
- [ ] 三方言（MySQL/PG/SQLite）基准完整运行，Oracle/MSSQL 尽力覆盖并标注。
- [ ] sz-pay `Cargo.toml` 7 个 sz-orm 包版本为 `2.3.0`，`cargo build --workspace` 零错误。
- [ ] sz-pay `cargo test --workspace` 通过数 ≥ 5,139 且失败数 = 0。
- [ ] sz-pay 6 个新功能验证用例全部通过。
- [ ] ADR-0001 合规：sz-orm 仓库无下游引入的修改。
- [ ] 零 `todo!`/`unimplemented!`/`unreachable!`/`unsafe`。

### M5 验收标准

- [ ] sz-pay 性能采集工具可运行，输出 QPS/P50/P95/P99/峰值内存五项指标。
- [ ] 生成 `sz-pay-performance-comparison.md` v2.1.0 vs v2.3.0 对比报告。
- [ ] 生成 `sz-pay-test-baseline.md` 测试基线文档。
- [ ] sz-orm 10 道门禁全部通过，附每道门禁运行输出。
- [ ] ADR-0001 合规验证通过，无违规记录。
- [ ] workspace 版本 bump 至 `2.3.0`，crates.io 存在 sz-orm-core 2.3.0。
- [ ] sz-pay 依赖切回 crates.io `2.3.0`，编译 + 测试通过。
- [ ] 生成 `v2.3.0-verification-report.md` 最终验证报告，所有结论附 file:line 证据。
- [ ] `scripts/audit-verify.ps1` 验证报告所有 file:line 引用真实存在。
- [ ] 99 条 EARS 需求 100% 覆盖且验收通过。

---

## 五、风险任务标注

| 任务 ID | 任务标题 | 风险等级 | 风险说明 | 缓解措施 |
|---------|---------|---------|---------|---------|
| T-C-007 | N1Eliminator N+1 自动消除器 | ⚠ 高 | N+1 消除结果等价性校验是关键风险点，不等价回退逻辑必须严格验证，避免返回错误结果。 | 合并后结果等价性校验（REQ-C-018）；不等价立即回退逐条执行（REQ-C-022）；集成测试覆盖等价性 + 回退场景。 |
| T-C-010 | 任务 C 集成测试套件（五方言） | ⚠ 高 | 五方言集成测试依赖真实数据库可用性，需确保本机 MySQL/PG/SQLite/Oracle/MSSQL 实例运行。 | 使用 `#[cfg(feature = "real-db")]` + 环境变量触发；Mock 模式作为 CI 快速验证；真实 DB 模式用于最终测量。 |
| T-B-009 | 多方言基准运行 | ⚠ 高 | 多方言基准依赖真实数据库实例可用性，且全套件耗时需控制在 30 分钟内。 | SQLite 始终运行（快速）；远程 DB 仅运行规模 100/1000/10000；竞品基准并行进程；criterion 配置优化。 |
| T-A-001 | sz-pay 依赖升级 2.1.0→2.3.0 | ⚠ 高 | sz-pay 升级可能遇到 deprecation 或 API 不兼容，需逐步升级并适配。 | 逐步升级 2.1.0→2.2.0→2.3.0；每步零编译错误；deprecation warning 适配；回滚路径（REQ-A-003）。 |
| T-A-003 | sz-pay 全量测试回归验证 | ⚠ 高 | 5,139 基线测试零回归是任务 A 的生死线，任何回归都需逐项定位并提供证据。 | 全量测试验证（REQ-A-004）；逐项定位失败原因（REQ-A-018）；区分 sz-orm 回归与 sz-pay 自身问题；file:line 证据。 |
| T-INT-001 | 全 workspace 10 道门禁验证 | ⚠ 高 | 10 道门禁是入库生死线，任一失败必须修复，clippy `-D warnings` 对新增代码要求严格。 | 逐道门禁运行 + 修复；附每道门禁输出证据；使用 `CARGO_INCREMENTAL=0` 避免 Windows MSVC 栈溢出。 |
| T-INT-003 | v2.3.0 版本 bump + crates.io 发布 | ⚠ 高 | crates.io 发布不可撤销，发布前必须确保 10 道门禁全过 + sz-pay 验证通过。 | 发布前运行 10 道门禁 + sz-pay 全量测试；发布后验证 crates.io 存在 2.3.0；sz-pay 切回 crates.io 依赖重新验证。 |

---

## 六、任务依赖关系图

```
M1（任务 C 核心实现）
  T-C-001 (RelationDef 扩展)
    └─ T-C-002 (策略决策器)
         ├─ T-C-003 (JoinStrategy)
         ├─ T-C-004 (DataLoaderStrategy)
         ├─ T-C-005 (IntermediateTableStrategy)
         └─ T-C-006 (SmartEagerLoader + smart())
              └─ T-C-007 (N1Eliminator) ⚠
                   └─ T-C-008 (load 集成)
                        └─ T-C-009 (单元测试)
                             └─ T-C-010 (集成测试) ⚠
                                  └─ T-C-011 (性能基准)

M3（任务 B 基准扩展，可与 M1/M2 并行）
  T-B-001 (CompetitorAdapter)
       ├─ T-B-002 (bench_crud)
       ├─ T-B-003 (bench_relation)
       ├─ T-B-004 (bench_transaction)
       ├─ T-B-005 (bench_pool)
       └─ T-B-006 (bench_pagination)
            └─ T-B-007 (full_comparison 主入口)
                 └─ T-B-008 (BenchmarkReporter)
                      └─ T-B-009 (多方言运行) ⚠
                           └─ T-B-010 (报告审查)

M4（任务 A 准备，依赖 M2）
  T-C-010 ── T-A-001 (sz-pay 升级) ⚠
                └─ T-A-002 (编译兼容)
                     └─ T-A-003 (回归测试) ⚠
                          └─ T-A-004 (新功能验证)

M5（任务 A 深化 + 集成验证）
  T-A-004 ── T-A-005 (性能采集)
                └─ T-A-006 (对比报告)
  T-A-003 + T-A-004 ── T-A-007 (基线文档)
  T-C-010 + T-C-011 + T-B-010 + T-A-004 ── T-INT-001 (10 道门禁) ⚠
  T-A-004 ── T-INT-002 (ADR-0001 合规)
  T-INT-001 + T-INT-002 ── T-INT-003 (版本发布) ⚠
  T-INT-003 ── T-INT-004 (审计证据)
```

---

## 七、任务统计表

### 按任务类别统计

| 任务类别 | 任务数 | 占比 |
|---------|--------|------|
| 任务 C：智能策略选择（T-C-*） | 11 | 34.4% |
| 任务 B：性能基准报告（T-B-*） | 10 | 31.3% |
| 任务 A：sz-pay 深化（T-A-*） | 7 | 21.9% |
| 集成验证与发布（T-INT-*） | 4 | 12.5% |
| **合计** | **32** | **100%** |

### 按里程碑统计

| 里程碑 | 任务数 | 预估工作量 | 目标 |
|--------|--------|-----------|------|
| M1：任务 C 核心实现 | 6 | 18~22 人时 | SmartEagerLoader + 策略决策器 + 三策略执行器 |
| M2：任务 C 增强 | 5 | 16~20 人时 | N+1 自动消除 + 向后兼容 + 测试 |
| M3：任务 B 基准扩展 | 7 | 18~22 人时 | 全维度 benchmark + 竞品适配层 |
| M4：任务 B 报告 + 任务 A 准备 | 7 | 16~20 人时 | 报告生成器 + sz-pay 升级 + 验证用例 |
| M5：任务 A 深化 + 集成验证 | 7 | 14~18 人时 | 性能采集 + 门禁 + 发布 |
| **合计** | **32** | **82~102 人时** | — |

### 按优先级统计

| 优先级 | 任务数 | 任务 ID |
|--------|--------|---------|
| 高 | 18 | T-C-001~010, T-A-001~004, T-INT-001~004 |
| 中 | 14 | T-C-011, T-B-001~010, T-A-005~007 |
| 低 | 0 | — |
| **合计** | **32** | — |

### 按风险等级统计

| 风险等级 | 任务数 | 任务 ID |
|---------|--------|---------|
| ⚠ 高风险 | 7 | T-C-007, T-C-010, T-B-009, T-A-001, T-A-003, T-INT-001, T-INT-003 |
| 普通风险 | 25 | 其余任务 |
| **合计** | **32** | — |

### 需求覆盖统计

| 需求类别 | 需求数 | 覆盖任务 | 覆盖率 |
|---------|--------|---------|--------|
| 任务 A（REQ-A-001~018） | 18 | T-A-001~007, T-INT-002~004 | 100% |
| 任务 B（REQ-B-001~023） | 23 | T-B-001~010, T-INT-001 | 100% |
| 任务 C（REQ-C-001~028） | 28 | T-C-001~011, T-INT-001 | 100% |
| 非功能性（REQ-NF-*） | 20 | T-C-009~011, T-B-001~010, T-A-005~006, T-INT-001 | 100% |
| 约束条件（REQ-CON-*） | 10 | T-C-001/009, T-A-001/003, T-INT-001~004 | 100% |
| **合计** | **99** | **32 个任务** | **100%** |

---

> **文档结束**
> 本编码任务规划文档将 sz-orm v2.3.0 的 99 条 EARS 需求分解为 32 个可独立执行、可独立验证的编码任务，按 5 个里程碑（M1~M5）组织，所有任务可追溯到 spec.md 需求 ID 与 design.md 设计章节。任务执行按里程碑依赖顺序 M1→M2→M3→M4→M5 推进，M3 可与 M1/M2 并行。