# sz-orm v2.4.0 技术设计文档

> 版本：v2.4.0
> 基线：v2.3.0（已全部完成）
> 日期：2026-08-07
> 文档定位：技术设计（How to build），对应需求规格 `.codeartsdoer/specs/sz_orm_v2_4_0/spec.md`（33 条 EARS 需求）
> 设计约束：Rust 2021 Edition / rust-version 1.81 / API 向后兼容 / 禁止占位实现 / unsafe 零容忍

---

# 一、需求与存量功能关系分析

## 1.1 需求功能与存量功能对比

v2.4.0 的三项任务（五方言集成测试 / 性能基准 / crates.io 发布）与 v2.3.0 已交付代码的关系如下。被测对象 `SmartEagerLoader` / `StrategyResolver` / `N1Eliminator` 已在 v2.3.0 实现，本版本不扩展其业务功能，仅补充验证与发布闭环。

### 1.1.1 已实现功能

| 需求功能 | 存量功能 | 代码位置 | 匹配度 |
|---------|---------|---------|--------|
| SmartEagerLoader 策略选择（REQ-IT-001~006 被测对象） | `StrategyResolver::resolve()` 纯内存枚举匹配，决策矩阵 HasOne/BelongsTo→Join、HasMany→DataLoader、ManyToMany→IntermediateTableBatch/回退 | `packages/sz-orm-core/src/smart_eager_loader.rs:96-137` | 100% |
| 三种加载策略执行器（Join/DataLoader/IntermediateTable） | `JoinStrategy` / `DataLoaderStrategy` / `IntermediateTableStrategy` 三个执行器，含 SQL 生成 + 结果拆分 | `packages/sz-orm-core/src/smart_eager_loader.rs:167-284`（Join）、`:286+`（DataLoader）、中间表策略 | 100% |
| 手动 EagerLoader 对照基准（REQ-IT-001~003 等价性对照） | `EagerLoader` 链式 API + `eager_load_all` / `eager_load_one` 端到端执行 | `packages/sz-orm-core/src/eager_loader.rs:1-887` | 100% |
| `smart()` 扩展方法构造 SmartEagerLoader | `EagerLoader::smart(self) -> SmartEagerLoader`，向后兼容 | `packages/sz-orm-core/src/eager_loader.rs:202-203` | 100% |
| N1Eliminator N+1 自动消除（REQ-PB-003 被测对象） | `N1Eliminator` 检测连续同模式查询 + 阈值合并 + 等价性回退 | `packages/sz-orm-core/src/n1_eliminator.rs:1-354` | 100% |
| NestedEagerResult 多级嵌套结果树（REQ-IT-012 深度等价） | `NestedEagerResult::Leaf/Node` 递归类型 + `row()`/`children()`/`is_leaf()` 访问器 | `packages/sz-orm-core/src/eager_loader.rs:54-88` | 100% |
| RelationDef 关联定义 + ManyToMany 中间表元数据 | `RelationDef::new` / `new_many_to_many` 构造器，`join_table`/`join_from_key`/`join_to_key` 字段 | `packages/sz-orm-core/src/relation_trait.rs:86-178` | 100% |
| 五方言 Dialect 实现（MySQL/PG/SQLite/Oracle/MSSQL） | `MySqlDialect`/`SqliteDialect`/`OracleDialect`/`SqlServerDialect` + `get_dialect(DbType)` 工厂 | `packages/sz-orm-core/src/dialect.rs:213/662/937/2243-2254` | 100% |
| 五方言 CRUD 集成测试（非 SmartEagerLoader 专项） | `integration_mysql.rs`/`integration_pg.rs`/`integration_sqlite.rs`/`integration_oracle.rs`/`integration_mssql.rs` | `packages/sz-orm-core/tests/integration_*.rs` | 75% |
| BenchmarkReporter 报告框架（REQ-PB-008 报告生成） | `BenchmarkReporter` + `BenchmarkRecord` + `EnvironmentMetadata` + DSN 脱敏 | `bench-comparison/benches/benchmark_reporter.rs:1-332` | 100% |
| criterion 配置合规（REQ-PB-009） | `CriterionConfig` 默认 sample_size=100 / warm_up=3s / measurement=10s | `bench-comparison/benches/benchmark_reporter.rs:42-52` | 100% |
| 竞品适配器框架（4 竞品 9 维度） | `CompetitorAdapter` trait + Diesel/SeaORM/SQLx/rusqlite 适配器 | `bench-comparison/benches/competitor_adapter.rs` | 100% |
| workspace 版本集中管理（REQ-REL-003） | `workspace.package.version = "2.3.0"`，子包 `version.workspace = true` | `Cargo.toml:6` | 100% |
| sz-pay 7 个 sz-orm-* 依赖 + patch 段（REQ-REL-004~006 现状） | 依赖 `sz-orm-{core,sqlx,config,auth,macros,queue,scheduler} = "2.1.0"` + `[patch.crates-io]` 7 包本地路径 | `E:\vue\test\sz-pay\server\sz-rust\Cargo.toml:26-32,164-171` | 100% |

### 1.1.2 需要扩展的功能

| 需求功能 | 存量功能 | 差异说明 | 扩展方向 |
|---------|---------|---------|---------|
| 五方言 SmartEagerLoader 等价性集成测试（REQ-IT-001~013） | 现有 `integration_*.rs` 仅覆盖 CRUD/事务/迁移，不含 SmartEagerLoader 与手动 EagerLoader 的结果集等价性断言 | 输入输出差异：现有测试断言单查询结果，需新增"智能 vs 手动"双路径对比断言；业务逻辑差异：需构造 HasOne/HasMany/ManyToMany 三种关联的测试数据集并逐行逐字段比对 | 新增 `tests/smart_eager_integration_{mysql,pg,sqlite,oracle,mssql}.rs` 5 个集成测试文件，复用现有 `tests/common` 连接辅助 + 新增等价性断言工具模块 |
| SmartEagerLoader 专项性能基准（REQ-PB-001~010） | `bench-comparison` 有 CRUD/分页/事务/连接池基准，无决策延迟/智能 vs 手动/N+1 消除对比三类专项基准 | 输入差异：需 10/100/1000/10000 四规模数据集生成器；输出差异：需 P50/P95/P99/Max 统计 + 前后查询次数对比；现有 `BenchmarkReporter` 可复用 | 新增 `bench-comparison/benches/bench_smart_eager.rs` 基准文件，复用 `BenchmarkReporter` + 新增 `SmartEagerBenchHarness` 数据生成/清理工具 |
| sz-pay 依赖升级 + patch 段移除（REQ-REL-004~006） | sz-pay 当前依赖 `sz-orm-* = "2.1.0"` + 7 包 patch 本地路径 | 版本差异：2.1.0 → 2.3.0；配置差异：移除 `[patch.crates-io]` 段后依赖来源从本地路径变为 crates.io | 修改 sz-pay `Cargo.toml`：7 个依赖版本号 2.1.0→2.3.0，删除 `:164-171` patch 段；执行 `cargo build` + `cargo test` 回归验证 |

### 1.1.3 需要新增的功能或接口

按业务模块分组，以下功能在存量代码中完全没有对应实现，需新增。

**模块 A：SmartEagerLoader 五方言等价性验证套件（对应 REQ-IT-001~013）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| 等价性断言工具 `assert_eager_equivalent` | 智能结果集 A + 手动结果集 B | 断言通过或差异明细 | 逐行逐字段比对（行数、字段名、字段值、嵌套深度），HasMany/ManyToMany 子集合无序比对 | `NestedEagerResult`、`Value` |
| HasOne 等价性测试（各方言） | 方言连接 + HasOne 关联定义 | 测试通过/失败 | 建表→插入测试数据→smart() 加载→手动 with() 加载→断言等价 | `SmartEagerLoader`、`EagerLoader`、各方言 Dialect |
| HasMany 等价性测试（各方言） | 方言连接 + HasMany 关联定义 | 测试通过/失败 | 同上，子集合无序比对 | 同上 |
| ManyToMany 等价性测试（各方言） | 方言连接 + ManyToMany + 中间表定义 | 测试通过/失败 | 同上，经中间表关联记录无序比对 | 同上 |
| Join 策略独立覆盖（REQ-IT-004） | HasOne/BelongsTo 关联 | 策略结果 == 手动 JOIN | 断言 `StrategyResolver::resolve()` 选定 Join + 执行结果与手动单次 JOIN 一致 | `StrategyResolver`、`JoinStrategy` |
| DataLoader 策略独立覆盖（REQ-IT-005） | HasMany 关联 | 策略结果 == 手动合并 | 断言选定 DataLoader + 执行结果与手动逐条查询合并一致 | `StrategyResolver`、`DataLoaderStrategy` |
| IntermediateTable 策略独立覆盖（REQ-IT-006） | ManyToMany + 中间表 | 策略结果 == 手动中间表 | 断言选定 IntermediateTableBatch + 执行结果与手动经中间表查询一致 | `StrategyResolver`、`IntermediateTableStrategy` |
| 结果集深度等价（REQ-IT-012） | 多级关联链（User→Order→Item） | 嵌套树深度一致 | 递归比对 `NestedEagerResult` 树：逐层节点数 + 深度 | `NestedEagerResult` |
| 方言跳过标注（REQ-IT-013） | 方言环境不可用 | `#[ignore]` + 跳过原因 | 连接失败时标注 ignore 并在测试输出记录原因，禁止静默通过 | 各方言连接辅助 |

**模块 B：SmartEagerLoader 性能基准套件（对应 REQ-PB-001~010）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| 决策延迟基准（REQ-PB-001） | `StrategyResolver` + 关联定义 | P50/P95/P99/Max 统计 | criterion 采样 ≥100 次 `resolve()` 调用，统计墙钟耗时，断言 P99 ≤ 100μs | `StrategyResolver`、criterion |
| 智能 vs 手动对比基准（REQ-PB-002） | N 规模数据集 | 智能/手动耗时比 | 同数据集分别用 smart() 与手动 with() 加载，计时，断言比 ≤ 1.10 | `SmartEagerLoader`、`EagerLoader` |
| N+1 消除对比基准（REQ-PB-003） | N 规模数据集 | 前后查询次数/耗时 | 逐条查询（N+1 次）vs N1Eliminator 批量合并（1 次），对比查询次数与耗时 | `N1Eliminator` |
| 四规模数据集生成器（REQ-PB-004~007） | 规模 N ∈ {10,100,1000,10000} | 建表 + 插入 N 条 + 外键均匀分布 | 生成主表 N 条 + 关联表 ≈N 条，外键均匀分布，测试后清理 | 各方言连接 |
| 基准报告生成（REQ-PB-008） | 基准测量记录 | Markdown + CSV/JSON | 复用 `BenchmarkReporter`，新增 SmartEager 维度记录，DSN 脱敏 | `BenchmarkReporter` |
| 规模缺失标注（REQ-PB-010） | 某规模基准未执行 | 报告标注缺失 + 原因 | 基准执行后检查四规模数据完整性，缺失则在报告标注 | `BenchmarkReporter` |

**模块 C：crates.io v2.3.0 发布流程（对应 REQ-REL-001~010）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| 依赖拓扑排序（REQ-REL-002） | workspace 43 包依赖图 | 拓扑序包列表 | 解析各包 `Cargo.toml` 的 sz-orm-* 依赖，构建 DAG，拓扑排序 | `Cargo.toml` |
| 逐包发布脚本（REQ-REL-001/009） | 拓扑序包列表 + token | 发布结果 | 逐包 `cargo publish`，任一失败立即中止，输出失败包名 | cargo、crates.io token |
| 发布前门禁检查（REQ-REL-008） | workspace 代码 | 门禁结果 | 执行 AGENTS.md 10 道门禁（fmt/check/clippy/test/doc/audit/integration/占位/SQL 注入/Feature 全组合），任一 FAIL 阻断 | gate 脚本 |
| sz-pay 回归验证（REQ-REL-004~006） | sz-pay 项目 | 构建/测试结果 | 升级依赖 2.1.0→2.3.0 + 移除 patch 段 + `cargo build` + `cargo test` | sz-pay Cargo.toml |

## 1.2 存量功能详细分析

### 1.2.1 StrategyResolver（被测对象，REQ-IT-004~006 / REQ-PB-001）

- **接口契约**：`resolve(&self, relation: &RelationDef) -> StrategyDecision`，入参为关联定义（零分配 `&'static str`），出参为策略决策记录（含 `relation_name`/`relation_kind`/`strategy`/`reason`/`estimated_query_count`）。无异常、无副作用、无 IO。
- **业务规则**：纯枚举匹配。HasOne|BelongsTo → Join；HasMany → DataLoader；ManyToMany 且 `join_table`/`join_from_key`/`join_to_key` 全 Some → IntermediateTableBatch，否则回退 DataLoader + `tracing::warn!`。确定性保证：相同输入始终返回相同输出。
- **扩展点**：`resolve_chain(&[RelationDef]) -> Vec<StrategyDecision>` 支持多级关联逐级独立决策。
- **约束**：决策延迟目标 ≤ 100μs（P99），纯内存操作无 IO。无状态（`StrategyResolver` 为单元结构体），线程安全（`Send + Sync` 隐式满足）。

### 1.2.2 JoinStrategy / DataLoaderStrategy / IntermediateTableStrategy（策略执行器）

- **接口契约**：
  - `JoinStrategy::execute(conn, relation, main_cols, related_cols, where_clause, where_params) -> Vec<(main_row, Option<related_row>)>`，单次 JOIN 查询。
  - `DataLoaderStrategy::execute(conn, relation, main_cols, related_cols, where_clause, where_params) -> HashMap<fk, Vec<related_row>>`，主表查询 + 批量 IN 查询（2 次）。
  - `IntermediateTableStrategy::build_intermediate_sql(relation, related_cols, n) -> String`，经中间表 JOIN 关联表批量查询（2 次）。
- **业务规则**：所有 SQL 显式指定列（禁止 `SELECT *`），列名加 `main_`/`related_` 前缀避免歧义。WHERE 条件参数化（`?` 占位 + `where_params` 绑定）。LEFT JOIN 关联行全 Null 时返回 `None`。
- **约束**：Oracle IN 列表 >1000 时分批查询（`eager_loader.rs` 已实现）。参数化查询铁律（C-03）。

### 1.2.3 EagerLoader（手动对照基准，REQ-IT-001~003）

- **接口契约**：`EagerLoader::new(relation).with(child).load(conn, main_sql) -> NestedEagerResult`。`eager_load_all(conn, main_sql, relation) -> Vec<(main_row, Vec<related_row>)>` 端到端便捷函数。
- **业务规则**：HasMany/ManyToMany 双查询策略（主表 → 提取主键 → WHERE IN 批量 → 分组组装）；HasOne/BelongsTo JOIN 策略（单条 SQL 拆分组装）。多级关联 `with()` 支持无限级递归（`ChildLoadConfig` 递归结构）。
- **扩展点**：`smart(self) -> SmartEagerLoader` 扩展方法，将手动加载器转换为智能加载器，保留原 relation/children 配置。
- **约束**：循环检测（`CycleDetector` + `CyclePolicy`），多级关联限 2 级（ADR-v2.1.0-006，v2.2.0 已放宽至无限级）。

### 1.2.4 N1Eliminator（被测对象，REQ-PB-003）

- **接口契约**：`record_query(PendingQuery)` 记录待合并查询；`try_merge(&mut conn) -> N1EliminationReport` 执行合并。`N1EliminationReport` 含 `original_count`/`merged_count`/`saved_count`/`merged_sql`。
- **业务规则**：检测连续相同模式查询（同表、同 WHERE 列、同 SELECT 列），连续数 ≥ 阈值（默认 5）时合并为 `WHERE id IN (?,...)`。独立事务查询跳过合并。合并后等价性校验，不等价回退逐条执行。
- **约束**：合并 SQL 参数化，禁止 `SELECT *`。阈值可通过 `with_threshold` 配置。

### 1.2.5 现有五方言集成测试（integration_*.rs，需扩展非替换）

- **接口契约**：各方言独立测试文件，`#[tokio::test]` 异步测试，通过 `tests/common` 连接辅助获取真实 DB 连接。使用 `#[ignore]` 标注需真实服务的测试（`cargo test -- --ignored` 触发）。
- **业务规则**：覆盖 CRUD/事务/迁移/分页/连接池等，**不覆盖 SmartEagerLoader 等价性**。这是 v2.4.0 任务 1 的核心缺口。
- **约束**：各方言独立 schema/database，数据隔离。Oracle 需 Sysdba 权限，MSSQL 远程连接。

### 1.2.6 bench-comparison 框架（需扩展非替换）

- **接口契约**：`BenchmarkReporter` 聚合 `BenchmarkRecord`，输出 Markdown + CSV/JSON。`CompetitorAdapter` trait 抽象竞品（Diesel/SeaORM/SQLx/rusqlite）。criterion `[[bench]]` 配置。
- **业务规则**：现有基准覆盖 CRUD/分页/事务/连接池/关系/全量对比/跨 DB 对比/v2.1.0 特性，**不覆盖 SmartEagerLoader 决策延迟/智能 vs 手动/N+1 消除对比**。
- **约束**：`CriterionConfig` 默认 sample_size=100 / warm_up=3s / measurement=10s（已合规，REQ-PB-009 满足）。DSN 脱敏（`BenchmarkReporter` 已实现）。

### 1.2.7 sz-pay 依赖现状（REQ-REL-004~006 输入）

- **接口契约**：sz-pay `Cargo.toml` 声明 7 个 `sz-orm-* = "2.1.0"` 依赖 + `[patch.crates-io]` 段用本地路径覆盖 7 个包。
- **业务规则**：patch 段使 sz-pay 构建时使用本地 sz-orm 代码而非 crates.io 2.1.0。发布后需移除 patch 段并升级版本号至 2.3.0，使依赖来源变为 crates.io 正式制品。
- **约束**：ADR-0001 铁律——严禁下游 sz-pay 修改上游 sz-orm 仓库文件。本设计仅修改 sz-pay 自身 `Cargo.toml`，不触碰 sz-orm 业务代码。

---

# 二、增量设计方案

## 2.1 实现模型

### 2.1.1 上下文视图

v2.4.0 不新增独立服务，而是在现有 sz-orm-core / bench-comparison 两个 crate 内新增测试与基准代码，并新增发布脚本。上下文关系如下：

```plantuml
@startuml
!theme plain
title sz-orm v2.4.0 上下文视图

rectangle "sz-orm-core\n(被测对象 + 新增集成测试)" as Core {
  rectangle "SmartEagerLoader\n(存量, 不改)" as Smart
  rectangle "EagerLoader\n(存量, 对照基准)" as Manual
  rectangle "N1Eliminator\n(存量, 被测)" as N1
  rectangle "smart_eager_integration_*.rs\n(新增, 5 方言)" as IntegrationTests
  rectangle "assert_eager_equivalent\n(新增, 等价性断言工具)" as AssertUtil
}

rectangle "bench-comparison\n(新增专项基准)" as Bench {
  rectangle "bench_smart_eager.rs\n(新增)" as SmartBench
  rectangle "BenchmarkReporter\n(存量, 复用)" as Reporter
  rectangle "SmartEagerBenchHarness\n(新增, 数据生成/清理)" as Harness
}

rectangle "scripts/\n(新增发布脚本)" as Scripts {
  rectangle "publish_crates_io.ps1\n(拓扑序逐包发布)" as PublishScript
  rectangle "compute_topology.ps1\n(依赖拓扑排序)" as TopologyScript
}

database "MySQL 9.6" as MySQL
database "PostgreSQL 18" as PG
database "SQLite (内存)" as SQLite
database "Oracle 23ai" as Oracle
database "MSSQL (远程)" as MSSQL
cloud "crates.io" as CratesIo
rectangle "sz-pay\n(下游验证)" as SzPay

IntegrationTests --> Smart : smart() 加载
IntegrationTests --> Manual : 手动 with() 加载
IntegrationTests --> AssertUtil : 断言 A == B
IntegrationTests --> MySQL : 等价性验证
IntegrationTests --> PG : 等价性验证
IntegrationTests --> SQLite : 等价性验证
IntegrationTests --> Oracle : 等价性验证
IntegrationTests --> MSSQL : 等价性验证

SmartBench --> Smart : 决策延迟采样
SmartBench --> Manual : 智能 vs 手动对比
SmartBench --> N1 : N+1 消除对比
SmartBench --> Harness : 数据生成/清理
SmartBench --> Reporter : 报告输出
SmartBench --> SQLite : 基准执行

PublishScript --> TopologyScript : 获取拓扑序
PublishScript --> CratesIo : 逐包 cargo publish
CratesIo --> SzPay : 拉取 v2.3.0
SzPay --> CratesIo : cargo build 验证

@enduml
```

**通信协议与调用频率**：
- 集成测试 → 方言 DB：SQL over TCP（MySQL/PG/Oracle/MSSQL）/ in-memory（SQLite），每次测试建表+插入+查询+清理。
- 基准 → SQLite：criterion 采样 ≥100 次/基准，四规模独立测量。
- 发布脚本 → crates.io：HTTPS API，逐包 `cargo publish`，43 次调用。

### 2.1.2 服务/组件总体架构

v2.4.0 新增代码分布在三个位置，组件职责与依赖关系如下：

```plantuml
@startuml
!theme plain
title sz-orm v2.4.0 组件架构（增量）

package "sz-orm-core (增量)" {
  component "EagerEquivalenceAssert\n(等价性断言工具)" as AssertComp <<new>>
  component "SmartEagerIntegrationTestSuite\n(五方言集成测试套件)" as TestSuite <<new>>
  component "TestSchemaBuilder\n(测试数据构造器)" as SchemaBuilder <<new>>
}

package "bench-comparison (增量)" {
  component "SmartEagerBenchHarness\n(基准数据生成/清理)" as BenchHarness <<new>>
  component "DecisionLatencyBench\n(决策延迟基准)" as DecBench <<new>>
  component "SmartVsManualBench\n(智能vs手动对比)" as CompareBench <<new>>
  component "N1EliminationBench\n(N+1消除对比)" as N1Bench <<new>>
  component "SmartEagerBenchReport\n(专项报告生成)" as BenchReport <<new>>
}

package "scripts (增量)" {
  component "TopologySorter\n(依赖拓扑排序)" as Topo <<new>>
  component "CratesIoPublisher\n(逐包发布)" as Pub <<new>>
  component "GateChecker\n(门禁检查)" as Gate <<new>>
  component "SzPayVerifier\n(sz-pay回归验证)" as PayVerify <<new>>
}

' 存量依赖（虚线）
component "SmartEagerLoader\n(存量)" as SmartExist
component "EagerLoader\n(存量)" as ManualExist
component "N1Eliminator\n(存量)" as N1Exist
component "BenchmarkReporter\n(存量)" as ReporterExist
component "Dialect\n(存量)" as DialectExist

TestSuite ..> SmartExist : smart()
TestSuite ..> ManualExist : with()
TestSuite ..> AssertComp
TestSuite ..> SchemaBuilder
TestSuite ..> DialectExist

DecBench ..> SmartExist : resolve()
CompareBench ..> SmartExist
CompareBench ..> ManualExist
N1Bench ..> N1Exist
BenchReport ..> ReporterExist
DecBench ..> BenchHarness
CompareBench ..> BenchHarness
N1Bench ..> BenchHarness

Pub ..> Topo
Pub ..> Gate
PayVerify ..> Pub

@enduml
```

**模块划分与职责**：

| 模块 | 位置 | 职责 | 新增/复用 |
|------|------|------|----------|
| EagerEquivalenceAssert | `sz-orm-core/tests/common/equivalence.rs` | 智能与手动结果集逐行逐字段等价性断言，含无序集合比对与嵌套深度比对 | 新增 |
| TestSchemaBuilder | `sz-orm-core/tests/common/schema_builder.rs` | 按方言建表（users/orders/profiles/roles/user_roles）+ 插入测试数据（≥5 主 + ≥10 关联，覆盖空/单/多）+ 清理 | 新增 |
| SmartEagerIntegrationTestSuite | `sz-orm-core/tests/smart_eager_integration_*.rs`（5 文件） | 五方言 × 三关联类型 × 三策略的等价性验证测试 | 新增 |
| SmartEagerBenchHarness | `bench-comparison/benches/smart_eager_harness.rs` | 四规模（10/100/1000/10000）数据集生成 + 外键均匀分布 + 测后清理 | 新增 |
| DecisionLatencyBench | `bench-comparison/benches/bench_smart_eager.rs` | `StrategyResolver::resolve()` 决策延迟采样，P99 ≤ 100μs 断言 | 新增 |
| SmartVsManualBench | 同上 | 智能 smart() vs 手动 with() 耗时比，≤ 1.10 断言 | 新增 |
| N1EliminationBench | 同上 | N1Eliminator 消除前后查询次数/耗时对比 | 新增 |
| SmartEagerBenchReport | 同上 | 复用 BenchmarkReporter + 新增 SmartEager 维度记录 + 规模缺失标注 | 新增（复用存量） |
| TopologySorter | `scripts/compute_topology.ps1` | 解析 43 包 Cargo.toml 依赖，构建 DAG，拓扑排序输出 | 新增 |
| CratesIoPublisher | `scripts/publish_crates_io.ps1` | 按拓扑序逐包 `cargo publish`，失败即中止 | 新增 |
| GateChecker | 复用 `scripts/gate.ps1`（AGENTS.md 已定义） | 10 道门禁全通过检查 | 复用 |
| SzPayVerifier | `scripts/verify_sz_pay.ps1` | sz-pay 依赖升级 + 移除 patch + build + test 回归 | 新增 |

### 2.1.3 实现设计文档

#### 2.1.3.1 集成测试执行流程（状态机）

集成测试每个方言每个关联类型的执行状态流转：

```plantuml
@startuml
!theme plain
title 集成测试执行状态机

[*] --> Connecting : 启动测试
Connecting --> Connected : 连接成功
Connecting --> Ignored : 连接失败 (标注 #[ignore] + 记录原因)
Connected --> BuildingSchema : 建表 + 插入测试数据
BuildingSchema --> SchemaReady : 数据就绪
BuildingSchema --> Failed : 建表失败 (输出错误)
SchemaReady --> LoadingSmart : smart() 加载关联
LoadingSmart --> LoadingManual : 手动 with() 加载关联
LoadingManual --> Asserting : 断言 A == B
Asserting --> Passed : 等价性通过
Asserting --> Failed : 不等价 (输出差异明细)
Passed --> Cleaning : 清理测试数据
Cleaning --> [*] : 测试结束
Failed --> Cleaning : 清理后退出
Ignored --> [*] : 跳过

@enduml
```

**关键分支与触发条件**：
- 连接失败 → `Ignored`：满足 REQ-IT-013，标注 `#[ignore]` 并记录原因，禁止静默通过。
- 断言失败 → `Failed`：输出差异明细（差异行号、差异字段、期望值 vs 实际值），满足 spec §5.1.3 异常场景 2。
- 清理始终执行：即使断言失败也清理测试数据，避免残留影响后续测试（spec §6.1 数据清理）。

#### 2.1.3.2 性能基准执行流程（活动图）

```plantuml
@startuml
!theme plain
title 性能基准执行活动图

start
:初始化 BenchmarkReporter\n(环境元数据 + criterion 配置);

partition "决策延迟基准 (REQ-PB-001)" {
  :criterion 采样 ≥100 次\nStrategyResolver::resolve();
  :统计 P50/P95/P99/Max;
  if (P99 ≤ 100μs?) then (是)
    :记录 PASS;
  else (否)
    :标注决策延迟超标\n(REQ-PB-001 异常场景 1);
  endif
}

partition "智能 vs 手动对比 (REQ-PB-002)" {
  repeat :遍历四规模 N ∈ {10,100,1000,10000};
    :Harness 生成 N 条数据;
    :smart() 加载 (计时);
    :手动 with() 加载 (计时);
    if (智能/手动 ≤ 1.10?) then (是)
      :记录 PASS;
    else (否)
      :标注性能退化;
    endif
    :Harness 清理数据;
  repeat while (更多规模?)
}

partition "N+1 消除对比 (REQ-PB-003)" {
  repeat :遍历四规模;
    :Harness 生成 N 条数据;
    :逐条查询 (计数 N+1, 计时);
    :N1Eliminator 批量合并 (计数 1, 计时);
    if (消除后次数 < 消除前?) then (是)
      :记录 PASS;
    else (否)
      :标注消除无效;
    endif
    :Harness 清理数据;
  repeat while (更多规模?)
}

:检查四规模数据完整性;
if (有规模缺失?) then (是)
  :报告标注缺失 + 原因 (REQ-PB-010);
else (否)
  :全部完整;
endif
:生成 Markdown + CSV/JSON 报告\n(DSN 脱敏);
stop

@enduml
```

#### 2.1.3.3 crates.io 发布流程（活动图）

```plantuml
@startuml
!theme plain
title crates.io v2.3.0 发布活动图

start
:执行 10 道门禁 (GateChecker);
if (全 PASS?) then (是)
else (否)
  :阻断发布 + 输出失败门禁\n(REQ-REL-008 异常);
  stop
endif

:cargo login (token 从环境变量传入);
:TopologySorter 计算 43 包拓扑序;

repeat :按拓扑序遍历包;
  :cargo publish <pkg>;
  if (发布成功?) then (是)
  else (否)
    if (版本已存在?) then (是)
      :提示重复发布;
    else (否)
      :输出失败详情;
    endif
    :立即中止后续发布\n(REQ-REL-009 禁止部分发布);
    stop
  endif
repeat while (更多包?)

:43 包全部发布成功;

partition "sz-pay 验证 (REQ-REL-004~006)" {
  :升级 sz-pay 依赖 2.1.0 → 2.3.0;
  :移除 [patch.crates-io] 段;
  :cargo build;
  if (构建成功?) then (是)
    :cargo test (回归);
    if (回归零失败?) then (是)
      :验证通过;
    else (否)
      :保留 patch 段\n输出失败测试 (REQ-REL-006 异常);
      stop
    endif
  else (否)
    :保留 patch 段\n输出编译错误 (REQ-REL-005 异常);
    stop
  endif
}

:发布完成;
stop

@enduml
```

**事务设计**：发布流程无数据库事务。crates.io 发布是不可逆操作（已发布版本不可覆盖），因此采用"失败即中止"策略（REQ-REL-009），已成功发布的包保持已发布状态，失败包修复后重发（版本号已存在则需升版本号）。

## 2.2 接口设计

### 2.2.1 总体设计

v2.4.0 新增接口分为三类：测试辅助接口（sz-orm-core/tests）、基准接口（bench-comparison/benches）、发布脚本接口（scripts）。所有接口为内部接口（非 pub crate API），不影响 sz-orm-core 公开 API（满足 API 向后兼容约束）。

| 接口分类 | 接口名 | 位置 | 稳定性 | 对应需求 |
|---------|--------|------|--------|---------|
| 等价性断言 | `assert_eager_equivalent` | tests/common/equivalence.rs | 稳定（测试内部） | REQ-IT-001~003,012 |
| 策略断言 | `assert_strategy_selected` | tests/common/equivalence.rs | 稳定 | REQ-IT-004~006 |
| 深度断言 | `assert_nested_depth_equal` | tests/common/equivalence.rs | 稳定 | REQ-IT-012 |
| 测试数据构造 | `TestSchemaBuilder::build` | tests/common/schema_builder.rs | 稳定 | REQ-IT-001~013 |
| 基准数据生成 | `SmartEagerBenchHarness::setup` | benches/smart_eager_harness.rs | 稳定 | REQ-PB-004~007 |
| 基准数据清理 | `SmartEagerBenchHarness::teardown` | 同上 | 稳定 | REQ-PB-004~007 |
| 拓扑排序 | `compute_topology` | scripts/compute_topology.ps1 | 稳定 | REQ-REL-002 |
| 逐包发布 | `publish_package` | scripts/publish_crates_io.ps1 | 稳定 | REQ-REL-001,009 |
| sz-pay 验证 | `verify_sz_pay` | scripts/verify_sz_pay.ps1 | 稳定 | REQ-REL-004~006 |

**接口变更策略**：所有新增接口为测试/基准/脚本内部接口，不进入 sz-orm-core 公开 API，无 Breaking Change。存量公开 API（`SmartEagerLoader`/`EagerLoader`/`StrategyResolver`/`N1Eliminator`）签名保持不变。

### 2.2.2 接口清单

#### 2.2.2.1 等价性断言工具

```rust
/// 断言 SmartEagerLoader 与手动 EagerLoader 结果集完全等价
///
/// 逐行逐字段比对：行数、字段名、字段值。
/// HasMany/ManyToMany 子集合按外键无序比对（不要求顺序一致）。
pub fn assert_eager_equivalent(
    smart_results: &[EagerResult],
    manual_results: &[EagerResult],
    relation_kind: RelationKind,
)

/// 断言 StrategyResolver 选定了预期策略
///
/// 用于 REQ-IT-004~006 策略独立覆盖验证。
pub fn assert_strategy_selected(
    decision: &StrategyDecision,
    expected: LoadStrategy,
)

/// 断言两个 NestedEagerResult 树的嵌套深度与逐层节点数完全一致
///
/// 用于 REQ-IT-012 结果集深度等价验证。
pub fn assert_nested_depth_equal(
    smart_tree: &NestedEagerResult,
    manual_tree: &NestedEagerResult,
)
```

- **业务说明**：等价性断言是集成测试的核心，证明智能与手动加载结果一致。
- **前置条件**：`smart_results` 与 `manual_results` 来自相同输入数据集。
- **后置条件**：断言通过则继续，失败则 `panic!` 输出差异明细（差异行号、字段、期望值 vs 实际值）。
- **异常映射**：断言失败 → 测试 FAIL + 差异明细（spec §5.1.3 异常场景 2）。
- **调用示例**：
```rust
let smart = SmartEagerLoader::new(order_rel).load(&mut conn, "SELECT id, name FROM users").await?;
let manual = EagerLoader::new(order_rel).load(&mut conn, "SELECT id, name FROM users").await?;
assert_eager_equivalent(&smart, &manual, RelationKind::HasMany);
```

#### 2.2.2.2 测试数据构造器

```rust
/// 测试数据构造器：按方言建表 + 插入测试数据 + 清理
pub struct TestSchemaBuilder {
    dialect: DbType,
    conn: Box<dyn Connection>,
}

impl TestSchemaBuilder {
    pub fn new(dialect: DbType, conn: Box<dyn Connection>) -> Self;

    /// 建表（users/orders/profiles/roles/user_roles），方言感知 DDL
    pub async fn build(&mut self) -> Result<(), DbError>;

    /// 插入测试数据：≥5 主记录 + ≥10 关联记录，覆盖空/单/多关联
    pub async fn seed(&mut self) -> Result<(), DbError>;

    /// 清理所有测试数据（DROP TABLE IF EXISTS）
    pub async fn teardown(&mut self) -> Result<(), DbError>;
}
```

- **业务说明**：为五方言集成测试提供统一的数据构造与清理，满足 spec §6.1 数据约束（≥5 主 + ≥10 关联、数据隔离、数据清理、关联完整性）。
- **前置条件**：`conn` 已连接到目标方言数据库。
- **后置条件**：`build` + `seed` 后表与数据就绪；`teardown` 后表删除。
- **异常映射**：DDL/DML 失败 → `DbError`，测试标注失败原因。

#### 2.2.2.3 基准数据生成/清理工具

```rust
/// SmartEagerLoader 基准数据生成/清理工具
pub struct SmartEagerBenchHarness {
    conn: Box<dyn Connection>,
}

impl SmartEagerBenchHarness {
    pub fn new(conn: Box<dyn Connection>) -> Self;

    /// 按规模 N 生成测试数据（主表 N 条 + 关联表 ≈N 条，外键均匀分布）
    ///
    /// N ∈ {10, 100, 1000, 10000}，满足 spec §6.2 规模档位与数据分布约束。
    pub async fn setup(&mut self, scale: usize) -> Result<(), DbError>;

    /// 清理指定规模数据
    pub async fn teardown(&mut self, scale: usize) -> Result<(), DbError>;
}
```

- **业务说明**：为性能基准提供四规模数据集，外键均匀分布避免基准失真（spec §6.2.2）。
- **前置条件**：`conn` 已连接（基准使用 SQLite in-memory 避免外部依赖）。
- **后置条件**：`setup(N)` 后 N 条数据就绪；`teardown(N)` 后数据清理。
- **调用示例**：
```rust
let mut harness = SmartEagerBenchHarness::new(conn);
harness.setup(1000).await?;
// ... 执行基准测量 ...
harness.teardown(1000).await?;
```

#### 2.2.2.4 拓扑排序与发布脚本接口

拓扑排序与发布脚本为 PowerShell 脚本（非 Rust 接口），接口为命令行调用：

```powershell
# 计算依赖拓扑顺序，输出包名列表（被依赖的包在前）
# 输入：workspace 根目录
# 输出：拓扑序包名列表（stdout，每行一个包名）
scripts/compute_topology.ps1 -WorkspaceRoot <path>

# 按拓扑序逐包发布到 crates.io
# 输入：拓扑序包名列表（来自 compute_topology）、token（环境变量 CARGO_REGISTRY_TOKEN）
# 输出：逐包发布结果，任一失败立即中止
scripts/publish_crates_io.ps1 -WorkspaceRoot <path> [-DryRun]

# sz-pay 回归验证
# 输入：sz-pay 项目路径
# 输出：依赖升级 + 移除 patch + build + test 结果
scripts/verify_sz_pay.ps1 -SzPayRoot <path>
```

- **业务说明**：拓扑排序确保被依赖的包先发布（REQ-REL-002）；逐包发布失败即中止（REQ-REL-009）；sz-pay 验证闭环（REQ-REL-004~006）。
- **前置条件**：`CARGO_REGISTRY_TOKEN` 环境变量已设置（REQ-REL-007 凭证安全）；10 道门禁全通过（REQ-REL-008）。
- **后置条件**：发布成功则 43 包在 crates.io 可见且版本 2.3.0；sz-pay 验证成功则依赖来源为 crates.io。
- **异常映射**：门禁失败 → 阻断发布；依赖未发布 → 该包 publish 失败 + 提示按拓扑序重试；版本已存在 → 提示冲突；token 无效 → 首包即失败 + 鉴权错误。

## 2.3 数据模型

### 2.3.1 设计目标

v2.4.0 数据模型需支持：

1. **集成测试数据集**：每方言每关联类型 ≥5 主记录 + ≥10 关联记录，覆盖空/单/多关联边界（spec §6.1）。
2. **性能基准数据集**：四规模（10/100/1000/10000）精确档位，外键均匀分布（spec §6.2）。
3. **发布元数据**：43 包版本号一致 2.3.0，workspace 集中管理（spec §6.3）。
4. **sz-pay 依赖数据**：7 个 sz-orm-* 依赖从 2.1.0 升级到 2.3.0，patch 段 7 包移除（spec §6.4）。
5. **与存量数据兼容**：不修改 sz-orm-core 公开数据结构（`EagerResult`/`NestedEagerResult`/`StrategyDecision` 等保持不变）。

### 2.3.2 模型实现

v2.4.0 不新增领域对象，复用存量数据结构。测试与基准使用的数据模型如下：

```plantuml
@startuml
!theme plain
title v2.4.0 数据模型（复用存量 + 测试数据集）

' 存量数据结构（不改）
class EagerResult <<struct>> {
  + row: HashMap<String, Value>
  + related: Vec<HashMap<String, Value>>
}
class NestedEagerResult <<enum>> {
  + Leaf(row)
  + Node { row, children }
}
class StrategyDecision <<struct>> {
  + relation_name: String
  + relation_kind: RelationKind
  + strategy: LoadStrategy
  + reason: String
  + estimated_query_count: usize
}
class N1EliminationReport <<struct>> {
  + original_count: usize
  + merged_count: usize
  + saved_count: usize
  + merged_sql: String
}
class BenchmarkRecord <<struct>> {
  + dimension: String
  + dialect: String
  + mean_ns: f64
  + p95_ns: f64
  + dataset_size: usize
}

' 测试数据集（新增，仅测试内部）
class TestDataset <<new>> {
  + users: Vec<TestUser>
  + orders: Vec<TestOrder>
  + profiles: Vec<TestProfile>
  + roles: Vec<TestRole>
  + user_roles: Vec<TestUserRole>
}
class TestUser {
  + id: i64
  + name: String
}
class TestOrder {
  + id: i64
  + user_id: i64
  + total: f64
}
class TestUserRole {
  + user_id: i64
  + role_id: i64
}

' 基准规模枚举（新增）
enum BenchScale <<new>> {
  S10
  S100
  S1000
  S10000
}

TestDataset "1" *-- "5..*" TestUser
TestDataset "1" *-- "10..*" TestOrder
TestDataset "1" *-- "7..*" TestUserRole

note right of BenchScale
  四规模档位（spec §6.2.1）
  精确值，禁止近似
end note

@enduml
```

**对象创建与销毁策略**：
- `TestDataset`：每条集成测试创建，测试结束（含失败）由 `TestSchemaBuilder::teardown` 销毁（DROP TABLE）。
- `BenchScale`：枚举常量，无创建销毁。基准数据由 `SmartEagerBenchHarness::setup/teardown` 管理。
- 存量对象（`EagerResult`/`StrategyDecision` 等）：生命周期不变，由各策略执行器按现有逻辑创建。

**持久化策略**：
- 集成测试数据：各方言独立 schema/database（spec §6.1.2 数据隔离），测试后 DROP。
- 基准数据：SQLite in-memory，进程结束自动释放。
- 发布元数据：crates.io 注册中心持久化（不可覆盖），workspace `Cargo.toml` 版本号集中管理。

---

## 2.4 算法设计

### 2.4.1 策略选择算法（存量，v2.4.0 验证非修改）

`StrategyResolver::resolve()` 的策略选择算法已在 v2.3.0 实现（`smart_eager_loader.rs:96-137`），v2.4.0 不修改，仅通过集成测试与基准验证其正确性与性能。

**算法描述（决策矩阵）**：

```
输入: relation: RelationDef
输出: StrategyDecision

1. 根据 relation.kind 匹配:
   - HasOne | BelongsTo  → strategy = Join
   - HasMany             → strategy = DataLoader
   - ManyToMany:
       若 relation.join_table ∧ join_from_key ∧ join_to_key 全 Some:
         → strategy = IntermediateTableBatch
       否则:
         → tracing::warn!(relation.name, "缺少中间表配置，回退 DataLoader")
         → strategy = DataLoader
2. 构造 reason 字符串（人类可读决策原因）
3. 返回 StrategyDecision { relation_name, relation_kind, strategy, reason, estimated_query_count }
```

**复杂度**：O(1) 纯枚举匹配，无循环无递归。决策延迟目标 ≤ 100μs（P99），由 REQ-PB-001 基准验证。

**确定性保证**：无状态、无 IO、无随机源，相同输入始终返回相同输出。`StrategyResolver` 为单元结构体（`#[derive(Default)]`），线程安全。

**v2.4.0 验证点**：
- REQ-IT-004：HasOne/BelongsTo 触发 Join 策略（断言 `decision.strategy == LoadStrategy::Join`）。
- REQ-IT-005：HasMany 触发 DataLoader 策略。
- REQ-IT-006：ManyToMany 有中间表触发 IntermediateTableBatch，无中间表回退 DataLoader + warn。
- REQ-PB-001：P99 决策延迟 ≤ 100μs。

### 2.4.2 等价性验证算法（新增，核心算法）

等价性验证是 v2.4.0 任务 1 的核心算法，证明 SmartEagerLoader 与手动 EagerLoader 结果集完全等价。

**算法 1：扁平结果集等价性比对（`assert_eager_equivalent`）**

```
输入: smart_results: &[EagerResult], manual_results: &[EagerResult], relation_kind: RelationKind
输出: 断言通过 或 panic!(差异明细)

1. 行数比对: 若 smart_results.len() != manual_results.len()
     → panic!("行数不等价: smart={} manual={}", smart.len(), manual.len())
2. 对每行 i:
   a. 主表行比对: smart_results[i].0 与 manual_results[i].0 逐字段比对
      - 字段名集合一致（忽略顺序）
      - 每个字段值 == 比对（Value 的 Eq）
      不一致 → panic!(行 i, 字段名, 期望值 vs 实际值)
   b. 关联行集合比对（按 relation_kind 分支）:
      - HasOne | BelongsTo: smart_results[i].1 与 manual_results[i].1
          长度一致（0 或 1）+ 逐字段比对
      - HasMany | ManyToMany: 无序集合比对
          按"关联外键值"排序后逐行逐字段比对（不要求原始顺序一致）
          不一致 → panic!(行 i, 关联行 j, 差异字段)
```

**复杂度**：O(R × F × C)，R = 行数，F = 字段数，C = 关联行数。无序比对需排序 O(C log C)。

**关键设计决策**：
- **无序比对**：HasMany/ManyToMany 的子记录集合顺序可能因方言 SQL 执行计划不同而不同（如 PG 与 MySQL 的默认排序差异），因此按外键值排序后比对，而非依赖顺序。这避免因方言排序差异导致的假阴性。
- **Value 的 Eq 比对**：`Value` 枚举已派生 `PartialEq`（`value.rs`），浮点数比对需注意精度。测试数据使用整型/字符串避免浮点精度问题（spec §6.1 关联完整性）。

**算法 2：嵌套树深度等价性比对（`assert_nested_depth_equal`，REQ-IT-012）**

```
输入: smart_tree: &NestedEagerResult, manual_tree: &NestedEagerResult
输出: 断言通过 或 panic!(深度差异)

1. 递归比对:
   a. 若 smart_tree.is_leaf() != manual_tree.is_leaf()
        → panic!("节点类型不一致: smart={} manual={}", ...)
   b. 若均为 Leaf: 比对 row 逐字段（同算法 1 主表行比对）
   c. 若均为 Node:
        - 比对本层 row 逐字段
        - 比对 children.len()（逐层节点数一致）
        - 对每对 children[i] 递归调用本算法
2. 深度计算: max_depth(tree) = Leaf → 1; Node → 1 + max(children.depth)
   断言 max_depth(smart) == max_depth(manual)
```

**复杂度**：O(N)，N = 树节点总数。递归深度受 `ChildLoadConfig::chain_depth` 限制（v2.2.0 已支持无限级，但测试用例限 3 级避免栈溢出）。

**算法 3：策略选择断言（`assert_strategy_selected`，REQ-IT-004~006）**

```
输入: decision: &StrategyDecision, expected: LoadStrategy
输出: 断言通过 或 panic!

若 decision.strategy != expected:
  → panic!("策略不符预期: relation={} actual={:?} expected={:?} reason={}",
           decision.relation_name, decision.strategy, expected, decision.reason)
```

**复杂度**：O(1) 枚举比对。

### 2.4.3 依赖拓扑排序算法（新增，发布流程核心）

拓扑排序算法用于 crates.io 发布前确定 43 包的发布顺序，确保被依赖的包先发布。

**算法描述（Kahn 算法变体）**：

```
输入: workspace 43 包及其 Cargo.toml 中的 sz-orm-* 依赖
输出: 拓扑序包名列表（被依赖的包在前）

1. 构建有向图 G:
   - 节点: 43 个包名
   - 边: 若包 A 的 Cargo.toml 依赖 sz-orm-B（path 或 version 依赖），则 B → A（B 先发布）
2. 计算各节点入度 indegree
3. 初始化队列 Q = {节点 | indegree == 0}（无内部依赖的包，如 sz-orm-core）
4. result = []
5. while Q 非空:
     a. 取 Q 中节点 n（按包名字典序保证确定性）
     b. result.push(n)
     c. 对 n 的每个后继 m: indegree[m] -= 1; 若 indegree[m] == 0: Q.push(m)
6. 若 result.len() != 43:
     → 存在循环依赖，报错（sz-orm 内部不应有循环依赖）
7. 返回 result
```

**复杂度**：O(V + E)，V = 43 包，E = 内部依赖边数。确定性输出（同字典序打破并列）。

**关键设计决策**：
- **仅 sz-orm-* 内部依赖**：外部依赖（tokio/serde 等）已在 crates.io，不影响拓扑序。
- **path 与 version 依赖均计入**：workspace 内 path 依赖（开发期）和 version 依赖（发布期）都构成拓扑约束。
- **字典序打破并列**：入度相同的包按名字典序排序，确保拓扑序唯一可复现（满足 REQ-REL-002 "拓扑顺序可追溯"）。

**验证**：拓扑序正确性验证——对结果中任意包 P，P 的所有 sz-orm-* 依赖在 P 之前出现。

### 2.4.4 N+1 消除对比算法（新增，基准测量）

**算法描述**：

```
输入: scale N, conn
输出: N1EliminationComparison { before_count, after_count, before_time, after_time }

1. Harness.setup(N): 生成 N 条主记录 + N 条关联记录
2. 消除前（逐条查询）:
   before_count = 0
   start = now()
   for i in 1..=N:
     conn.query("SELECT ... WHERE id = ?", [i])  // 逐条
     before_count += 1
   before_time = now() - start
3. 消除后（N1Eliminator 批量合并）:
   eliminator = N1Eliminator::new()
   for i in 1..=N:
     eliminator.record_query(PendingQuery { table: "...", where_value: i, ... })
   start = now()
   report = eliminator.try_merge(&mut conn).await
   after_time = now() - start
   after_count = report.merged_count
4. Harness.teardown(N)
5. 断言 after_count < before_count（消除生效）
6. 返回对比数据
```

**复杂度**：消除前 O(N) 次查询；消除后 O(1) 次批量查询（N 合并为 1）。耗时对比量化消除收益。

---

## 2.5 测试设计

### 2.5.1 集成测试策略（对应 REQ-IT-001~013）

#### 2.5.1.1 测试矩阵

五方言 × 三关联类型 × 三策略 = 45 个等价性验证点，组织为 5 个集成测试文件：

| 测试文件 | 方言 | 连接 | 测试函数 | 对应需求 |
|---------|------|------|---------|---------|
| `smart_eager_integration_mysql.rs` | MySQL 9.6 | `mysql://root:test123@127.0.0.1:3306/sz_orm_test` | `test_hasone_equivalent_mysql` / `test_hasmany_equivalent_mysql` / `test_many_to_many_equivalent_mysql` / `test_join_strategy_mysql` / `test_dataloader_strategy_mysql` / `test_intermediate_strategy_mysql` / `test_nested_depth_mysql` | REQ-IT-001~006,012,007 |
| `smart_eager_integration_pg.rs` | PostgreSQL 18 | `postgres://postgres:test123@127.0.0.1:5432/sz_orm_test` | 同上 `_pg` 后缀 | REQ-IT-001~006,012,008 |
| `smart_eager_integration_sqlite.rs` | SQLite in-memory | `sqlite::memory:` | 同上 `_sqlite` 后缀 | REQ-IT-001~006,012,009 |
| `smart_eager_integration_oracle.rs` | Oracle 23ai | `oracle://sys:test123@127.0.0.1:1521/freepdb1` | 同上 `_oracle` 后缀 | REQ-IT-001~006,012,010 |
| `smart_eager_integration_mssql.rs` | MSSQL | 远程 `sh-mssql-adrul9nm.sql.tencentcdb.com:22527` | 同上 `_mssql` 后缀 | REQ-IT-001~006,012,011 |

**测试标注策略**：
- 需真实 DB 服务的测试标注 `#[ignore]`，通过 `cargo test --workspace -- --ignored` 触发（与现有 `integration_*.rs` 一致）。
- SQLite in-memory 无外部依赖，不标注 `#[ignore]`，默认执行。
- 方言环境不可用时（REQ-IT-013）：连接失败 → 测试标注 `#[ignore]` + `tracing::warn!` 记录跳过原因，禁止静默通过。

#### 2.5.1.2 测试数据集设计（spec §6.1）

每方言每关联类型的测试数据集：

```
users 表: 5 条记录（id 1-5）
  - user 1: 有 3 个 orders, 1 个 profile, 2 个 roles（经 user_roles）
  - user 2: 有 2 个 orders, 1 个 profile, 1 个 role
  - user 3: 有 0 个 orders（空关联边界）, 0 个 profile, 0 个 roles
  - user 4: 有 1 个 order（单条关联边界）, 1 个 profile, 0 个 roles
  - user 5: 有 4 个 orders, 0 个 profile, 3 个 roles

orders 表: 10 条记录（id 101-110, user_id 均匀分布 1-5）
profiles 表: 3 条记录（user_id 1,2,4）
roles 表: 3 条记录（id 1-3）
user_roles 表: 6 条记录（(1,1),(1,2),(2,1),(5,1),(5,2),(5,3)）
```

**边界覆盖**：空关联（user 3）、单条关联（user 4）、多条关联（user 1/5），满足 spec §6.1.1 ≥5 主 + ≥10 关联 + 边界情况。

#### 2.5.1.3 等价性验证流程（每测试函数）

```rust
#[tokio::test]
#[ignore] // 需真实 MySQL
async fn test_hasmany_equivalent_mysql() {
    let mut conn = common::connect_mysql().await
        .unwrap_or_else(|e| { /* REQ-IT-013: 标注跳过 */ ignore!("MySQL 不可用: {e}") });

    let mut schema = TestSchemaBuilder::new(DbType::MySQL, conn);
    schema.build().await.unwrap();
    schema.seed().await.unwrap();

    let order_rel = RelationDef::new("orders", "users", "orders", "id", "user_id", RelationKind::HasMany);

    // 智能加载
    let smart = EagerLoader::new(order_rel).smart()
        .load(&mut schema.conn, "SELECT id, name FROM users").await.unwrap();

    // 手动加载（对照基准）
    let manual = EagerLoader::new(order_rel)
        .load(&mut schema.conn, "SELECT id, name FROM users").await.unwrap();

    // 等价性断言
    assert_eager_equivalent(&smart, &manual, RelationKind::HasMany);

    schema.teardown().await.unwrap();
}
```

#### 2.5.1.4 策略独立覆盖测试（REQ-IT-004~006）

```rust
#[test]
fn test_join_strategy_selection() {
    let resolver = StrategyResolver::new();
    let rel = RelationDef::new("profile", "users", "profiles", "id", "user_id", RelationKind::HasOne);
    let decision = resolver.resolve(&rel);
    assert_strategy_selected(&decision, LoadStrategy::Join);
    assert_eq!(decision.estimated_query_count, 1);
}
```

策略选择为纯内存操作，无需真实 DB，不标注 `#[ignore]`，默认执行。

### 2.5.2 性能基准方案（对应 REQ-PB-001~010）

#### 2.5.2.1 基准文件组织

新增 `bench-comparison/benches/bench_smart_eager.rs`，含三个基准组：

| 基准组 | criterion bench 函数 | 测量目标 | 对应需求 |
|-------|---------------------|---------|---------|
| 决策延迟 | `bench_decision_latency` | `StrategyResolver::resolve()` P99 ≤ 100μs | REQ-PB-001 |
| 智能 vs 手动 | `bench_smart_vs_manual/{s10,s100,s1000,s10000}` | 耗时比 ≤ 1.10 | REQ-PB-002,004~007 |
| N+1 消除对比 | `bench_n1_elimination/{s10,s100,s1000,s10000}` | 消除后查询次数 < 消除前 | REQ-PB-003,004~007 |

#### 2.5.2.2 criterion 配置（REQ-PB-009）

```rust
use criterion::{Criterion, BenchmarkId};

fn bench_smart_eager(c: &mut Criterion) {
    // 决策延迟基准：sample_size=100, warm_up=3s, measurement=10s（已合规）
    let mut group = c.benchmark_group("smart_eager_decision_latency");
    group.sample_size(100);           // ≥ 100 (REQ-PB-009)
    group.warm_up_time(Duration::from_secs(3));    // ≥ 3s
    group.measurement_time(Duration::from_secs(10)); // ≥ 10s

    let resolver = StrategyResolver::new();
    let rel = RelationDef::new("orders", "users", "orders", "id", "user_id", RelationKind::HasMany);

    group.bench_function("resolve", |b| {
        b.iter(|| {
            let decision = resolver.resolve(black_box(&rel));
            black_box(decision)
        })
    });
    group.finish();

    // 智能 vs 手动 + N+1 消除：四规模
    for scale in [10, 100, 1000, 10000] {
        // bench_smart_vs_manual(group, scale)
        // bench_n1_elimination(group, scale)
    }
}

criterion_group!(benches, bench_smart_eager);
criterion_main!(benches);
```

**配置合规验证**：`CriterionConfig` 默认值（sample_size=100, warm_up=3s, measurement=10s）已满足 REQ-PB-009，基准代码显式设置确保不被覆盖。

#### 2.5.2.3 基准报告生成（REQ-PB-008）

复用存量 `BenchmarkReporter`，新增 SmartEager 维度记录：

```rust
let mut reporter = BenchmarkReporter::new(environment);
// 决策延迟记录
reporter.add_record(BenchmarkRecord {
    dimension: "smart_eager_decision_latency".into(),
    dialect: "sqlite".into(),
    competitor: "sz-orm-smart".into(),
    mean_ns, median_ns, p95_ns, throughput_ops_per_sec,
    dataset_size: 0,  // 决策延迟无数据集规模
});
// 智能 vs 手动记录（四规模）
for scale in [10, 100, 1000, 10000] {
    reporter.add_record(BenchmarkRecord { dimension: "smart_vs_manual".into(), dataset_size: scale, ... });
}
// N+1 消除记录（四规模）
// ...
let report = reporter.generate_markdown();  // 含 CSV/JSON + DSN 脱敏
```

**规模缺失标注（REQ-PB-010）**：基准执行后检查四规模数据完整性，若某规模缺失则在报告 `AuditReport.missing_dimensions` 标注 + 原因，禁止静默缺失。

#### 2.5.2.4 基准执行环境

- **决策延迟基准**：纯内存，无 DB，任意平台可运行。
- **智能 vs 手动 / N+1 消除基准**：使用 SQLite in-memory（无外部依赖，避免基准受网络/DB 负载干扰），四规模数据由 `SmartEagerBenchHarness` 生成。
- **环境记录**：报告记录 CPU / Rust 版本 / DB 版本 / 时间戳（spec §6.2.4 可复现）。

### 2.5.3 发布前门禁测试（对应 REQ-REL-008）

发布前执行 AGENTS.md 定义的 10 道门禁（复用 `scripts/gate.ps1`）：

| # | 门禁 | 命令 | 阻断条件 |
|---|------|------|---------|
| 1 | fmt | `cargo fmt --all -- --check` | 格式不符 |
| 2 | check | `cargo check --workspace --all-targets` | 编译错误 |
| 3 | clippy | `cargo clippy --workspace --all-targets -- -D warnings` | 任一警告 |
| 4 | test | `cargo test --workspace` | 任一测试失败 |
| 5 | doc | `cargo doc --workspace --no-deps --all-features` | 文档构建失败 |
| 6 | audit | `cargo audit` + `cargo deny check` | 安全漏洞 |
| 7 | integration | `cargo test --workspace -- --ignored` | 集成测试失败 |
| 8 | 占位检查 | `grep -rn 'todo!\|unimplemented!\|unreachable!' --include='*.rs'` | 存在占位 |
| 9 | SQL 注入 | `scripts/check-sql-injection.ps1` | SQL 拼接 |
| 10 | Feature 全组合 | `cargo check --workspace --all-targets --all-features` | 任一组合失败 |

任一门禁 FAIL → 阻断发布（REQ-REL-008），输出失败门禁名称与详情。

---

## 2.6 发布流程设计（对应 REQ-REL-001~010）

### 2.6.1 crates.io 发布拓扑顺序

基于 §2.4.3 拓扑排序算法，43 包的发布拓扑顺序（被依赖的包先发布）。拓扑层级如下（同层包可并行发布，实际按字典序串行）：

```
Layer 0（无 sz-orm 内部依赖）:
  sz-orm-core          ← 核心库，所有包依赖它
  sz-orm-macros        ← 过程宏，独立（可能被 core 依赖，需确认）

Layer 1（依赖 Layer 0）:
  sz-orm-sqlx          ← 依赖 core
  sz-orm-config        ← 依赖 core
  sz-orm-logger        ← 依赖 core
  sz-orm-tracing       ← 依赖 core
  sz-orm-crypto        ← 依赖 core
  sz-orm-sql-validator ← 依赖 core
  sz-orm-query-builder ← 依赖 core
  sz-orm-masking       ← 依赖 core
  sz-orm-observability ← 依赖 core
  sz-orm-oracle        ← 依赖 core
  sz-orm-mssql         ← 依赖 core
  sz-orm-storage       ← 依赖 core
  sz-orm-limit         ← 依赖 core
  sz-orm-health        ← 依赖 core
  sz-orm-audit         ← 依赖 core
  sz-orm-batch         ← 依赖 core
  sz-orm-rw            ← 依赖 core
  sz-orm-dtx           ← 依赖 core
  sz-orm-sharding      ← 依赖 core
  sz-orm-timeseries    ← 依赖 core
  sz-orm-search        ← 依赖 core
  sz-orm-vector        ← 依赖 core
  sz-orm-lc            ← 依赖 core
  sz-orm-wasm          ← 依赖 core
  sz-orm-ai            ← 依赖 core
  sz-orm-mig           ← 依赖 core
  sz-orm-back          ← 依赖 core
  sz-orm-websocket     ← 依赖 core
  sz-orm-mqtt          ← 依赖 core
  sz-orm-queue         ← 依赖 core
  sz-orm-es            ← 依赖 core
  sz-orm-postgis       ← 依赖 core
  sz-orm-python        ← 依赖 core
  sz-orm-js            ← 依赖 core

Layer 2（依赖 Layer 0-1）:
  sz-orm-auth          ← 依赖 core + config + macros
  sz-orm-scheduler     ← 依赖 core + queue
  sz-orm-swagger       ← 依赖 core
  sz-orm-graphql       ← 依赖 core
  sz-orm-grpc          ← 依赖 core
  sz-orm-actix         ← 依赖 core + actix
  sz-orm-axum          ← 依赖 core + axum

Layer 3（应用层，不发布到 crates.io）:
  cli                  ← publish = false
  examples             ← publish = false
```

**注意**：实际拓扑序由 `scripts/compute_topology.ps1` 解析各包 `Cargo.toml` 自动计算，上图为基于依赖关系的预估。`cli` 和 `examples` 的 `publish = false`，不参与发布（43 包 = 41 lib + cli + examples，但 cli/examples 不发布，实际发布 41 个 lib 包；若 spec 要求 43 包全发布，需确认 cli/examples 的 publish 字段——根据 spec §1.2 "43 个待发布包"，此处以 spec 为准，发布脚本按拓扑序处理所有非 `publish=false` 的包）。

### 2.6.2 发布脚本设计

```powershell
# scripts/publish_crates_io.ps1
param(
    [Parameter(Mandatory)] [string] $WorkspaceRoot,
    [switch] $DryRun
)

# 1. 门禁检查（REQ-REL-008）
& "$WorkspaceRoot\scripts\gate.ps1"
if ($LASTEXITCODE -ne 0) { Write-Error "门禁未通过，阻断发布"; exit 1 }

# 2. 检查 token（REQ-REL-007，不从 git 读取）
if (-not $env:CARGO_REGISTRY_TOKEN) { Write-Error "CARGO_REGISTRY_TOKEN 未设置"; exit 1 }

# 3. 计算拓扑序（REQ-REL-002）
$topology = & "$WorkspaceRoot\scripts\compute_topology.ps1" -WorkspaceRoot $WorkspaceRoot

# 4. 逐包发布（REQ-REL-001, REQ-REL-009 禁止部分发布）
$published = @()
foreach ($pkg in $topology) {
    Write-Host "发布 $pkg ..."
    if ($DryRun) { Write-Host "  [DryRun] cargo publish -p $pkg"; continue }
    cargo publish -p $pkg 2>&1 | Tee-Object -Variable publishLog
    if ($LASTEXITCODE -ne 0) {
        Write-Error "发布失败: $pkg（已发布: $($published -join ', ')）"
        Write-Error "中止后续发布（REQ-REL-009）"
        exit 1
    }
    $published += $pkg
}

Write-Host "全部 $($published.Count) 包发布成功，版本 2.3.0"

# 5. sz-pay 验证（REQ-REL-004~006）
& "$WorkspaceRoot\scripts\verify_sz_pay.ps1" -SzPayRoot "E:\vue\test\sz-pay\server\sz-rust"
```

### 2.6.3 sz-pay 验证脚本设计

```powershell
# scripts/verify_sz_pay.ps1
param([Parameter(Mandatory)] [string] $SzPayRoot)

$cargoToml = Join-Path $SzPayRoot "Cargo.toml"

# 1. 升级依赖版本 2.1.0 → 2.3.0（7 个包）
$packages = @("sz-orm-core","sz-orm-sqlx","sz-orm-config","sz-orm-auth","sz-orm-macros","sz-orm-queue","sz-orm-scheduler")
foreach ($pkg in $packages) {
    # 替换 'sz-orm-xxx = "2.1.0"' → 'sz-orm-xxx = "2.3.0"'
    # 使用文本替换（非 PowerShell 重定向，用脚本库）
}

# 2. 移除 [patch.crates-io] 段（7 行 path 覆盖）
# 删除从 "[patch.crates-io]" 到文件末尾或下一个非 patch 段的 7 行

# 3. cargo build
cargo build --manifest-path $cargoToml
if ($LASTEXITCODE -ne 0) {
    Write-Error "sz-pay 构建失败，恢复 patch 段（REQ-REL-005 异常）"
    # 恢复 patch 段（git checkout）
    exit 1
}

# 4. cargo test 回归（REQ-REL-006）
cargo test --manifest-path $cargoToml
if ($LASTEXITCODE -ne 0) {
    Write-Error "sz-pay 回归失败（REQ-REL-006 异常），保留 patch 段"
    exit 1
}

Write-Host "sz-pay 验证通过：依赖来源为 crates.io v2.3.0，回归零失败"
```

**ADR-0001 合规（REQ-REL-010）**：发布脚本仅修改 sz-pay 自身 `Cargo.toml`（版本号 + patch 段），不修改 sz-orm 仓库任何业务代码。sz-orm 仓库的 `git diff` 应仅含版本号/发布元数据变更（若有），无业务代码变更。

### 2.6.4 凭证安全设计（REQ-REL-007）

- crates.io token 通过环境变量 `CARGO_REGISTRY_TOKEN` 传入，或 `cargo login`（token 存储在 `~/.cargo/credentials`，不入 git）。
- 发布脚本不硬编码 token，不将 token 写入任何 git 跟踪文件。
- 验证：`git log --all -p | grep <token>` 应无结果（token 不出现在 git 历史中）。

---

## 2.7 风险分析

### 2.7.1 风险登记表

| 风险 ID | 风险描述 | 影响 | 概率 | 严重度 | 缓解措施 | 对应需求 |
|---------|---------|------|------|--------|---------|---------|
| R-01 | MSSQL 远程连接不稳定（`sh-mssql-adrul9nm.sql.tencentcdb.com:22527`） | MSSQL 集成测试无法执行 | 中 | 中 | 标注 `#[ignore]` + 记录跳过原因（REQ-IT-013），本机 MySQL/PG/Oracle/SQLite 四方言优先保障 | REQ-IT-011,013 |
| R-02 | Oracle 23ai Sysdba 权限测试数据清理不彻底 | 残表失败影响后续测试 | 低 | 中 | `teardown` 使用 `DROP TABLE IF EXISTS`（Oracle 23ai 支持），测试独立 schema 隔离 | REQ-IT-010 |
| R-03 | 五方言 SQL 方言差异导致等价性假阴性（如 PG 与 MySQL 默认排序不同） | 等价性断言误报失败 | 中 | 高 | 无序集合比对算法（§2.4.2 算法 1 按外键排序后比对），不依赖原始顺序 | REQ-IT-001~003 |
| R-04 | 决策延迟基准受系统噪声影响，P99 偶发超 100μs | REQ-PB-001 基准偶发失败 | 低 | 中 | criterion 配置 sample_size=100 + warm_up=3s + measurement=10s 统计平滑；基准在低负载环境运行 | REQ-PB-001 |
| R-05 | 10000 规模数据插入超时 | 大规模基准超时失败 | 低 | 低 | 该规模基准独立超时阈值，不影响其他规模（spec §5.2.3 异常场景 4）；SQLite in-memory 插入快 | REQ-PB-007 |
| R-06 | crates.io 发布中途某包失败（如版本已存在、依赖未发布） | 部分包已发布，后续包未发布 | 中 | 高 | 失败即中止（REQ-REL-009）；已发布包保持已发布（不可覆盖）；失败包修复后重发或升版本号 | REQ-REL-001,009 |
| R-07 | sz-orm-core 1.0.0 已发布到 crates.io，2.3.0 版本可能已存在 | 重复发布失败 | 中 | 中 | 发布前检查 `cargo search` 或 crates.io API 确认版本未存在；若已存在则跳过该包 | REQ-REL-001 |
| R-08 | sz-pay 移除 patch 段后构建失败（API 不兼容） | sz-pay 无法使用 crates.io 正式制品 | 低 | 高 | patch 段保留不删除（REQ-REL-005 异常处理）；排查 sz-orm 2.1.0→2.3.0 API 兼容性；v2.4.0 无 Breaking Change 应保证兼容 | REQ-REL-005 |
| R-09 | sz-pay 回归测试出现业务行为变化 | 发布验证未完成 | 低 | 高 | 输出失败测试详情（REQ-REL-006 异常）；排查 sz-orm 行为差异；v2.4.0 不改 SmartEagerLoader 业务功能应保证零回归 | REQ-REL-006 |
| R-10 | 浮点数等价性比对精度问题 | 等价性断言误报 | 低 | 低 | 测试数据使用整型/字符串避免浮点（§2.4.2 算法 1 设计决策）；必要时用 epsilon 比对 | REQ-IT-001~003 |
| R-11 | 拓扑排序算法遇到循环依赖 | 发布顺序无法确定 | 极低 | 高 | sz-orm 内部不应有循环依赖（编译期会报错）；算法检测循环并报错（§2.4.3 步骤 6） | REQ-REL-002 |
| R-12 | token 泄露到 git 历史 | 安全风险 | 低 | 高 | token 仅通过环境变量传入（REQ-REL-007）；发布脚本不含 token 字面量；`git log` 审计 | REQ-REL-007 |

### 2.7.2 风险应对优先级

**高严重度风险（需优先缓解）**：
- R-03（等价性假阴性）：无序比对算法已内置缓解，集成测试阶段验证。
- R-06（发布中途失败）：失败即中止策略已内置，发布脚本阶段验证。
- R-08/R-09（sz-pay 兼容性）：v2.4.0 无 Breaking Change + 不改 SmartEagerLoader 业务功能，理论上零回归；验证脚本保留 patch 段回退机制。

**中严重度风险（需监控）**：
- R-01（MSSQL 远程）：标注 ignore + 四方言优先，不阻断整体交付。
- R-07（版本已存在）：发布前检查，已存在则跳过。

### 2.7.3 回退策略

| 场景 | 回退策略 |
|------|---------|
| 集成测试等价性失败 | 输出差异明细，定位到关联类型 + 方言，修复 SmartEagerLoader（若为 bug）或测试数据（若为数据问题） |
| 性能基准超标 | 标注超标（决策延迟/N+1 消除），不阻断集成测试；若为 SmartEagerLoader 性能 bug，由后续版本优化（spec §1.4 职责边界 6） |
| crates.io 发布失败 | 失败即中止；已发布包保持；失败包修复后重发（版本未存在）或升版本号（版本已存在） |
| sz-pay 验证失败 | 保留 patch 段不删除；sz-pay 继续使用本地覆盖；排查兼容性后重试 |

---

## 2.8 需求覆盖追溯

本设计对 spec.md 全部 33 条需求的覆盖映射：

| 需求编号 | 设计章节 | 实现位置 |
|---------|---------|---------|
| REQ-IT-001~003 | §2.4.2 算法 1 + §2.5.1 集成测试矩阵 | `tests/smart_eager_integration_*.rs` + `tests/common/equivalence.rs` |
| REQ-IT-004~006 | §2.4.1 策略选择算法 + §2.4.2 算法 3 + §2.5.1.4 策略独立覆盖 | `tests/common/equivalence.rs::assert_strategy_selected` |
| REQ-IT-007~011 | §2.5.1.1 测试矩阵（五方言） | `tests/smart_eager_integration_{mysql,pg,sqlite,oracle,mssql}.rs` |
| REQ-IT-012 | §2.4.2 算法 2 + §2.5.1.3 深度等价 | `tests/common/equivalence.rs::assert_nested_depth_equal` |
| REQ-IT-013 | §2.5.1.1 测试标注策略 + §2.1.3.1 状态机 Ignored 分支 | 各集成测试 `unwrap_or_else(ignore!)` |
| REQ-PB-001 | §2.4.1 + §2.5.2.1 决策延迟基准 | `benches/bench_smart_eager.rs::bench_decision_latency` |
| REQ-PB-002 | §2.5.2.1 智能 vs 手动基准 | `benches/bench_smart_eager.rs::bench_smart_vs_manual` |
| REQ-PB-003 | §2.4.4 N+1 消除对比算法 + §2.5.2.1 | `benches/bench_smart_eager.rs::bench_n1_elimination` |
| REQ-PB-004~007 | §2.5.2.1 四规模 + §2.3.2 BenchScale | `benches/smart_eager_harness.rs::setup(scale)` |
| REQ-PB-008 | §2.5.2.3 基准报告生成 | `benches/benchmark_reporter.rs`（复用） |
| REQ-PB-009 | §2.5.2.2 criterion 配置 | `CriterionConfig` 默认值已合规 |
| REQ-PB-010 | §2.5.2.3 规模缺失标注 | `AuditReport.missing_dimensions` |
| REQ-REL-001 | §2.6.2 发布脚本 | `scripts/publish_crates_io.ps1` |
| REQ-REL-002 | §2.4.3 拓扑排序算法 + §2.6.1 拓扑顺序 | `scripts/compute_topology.ps1` |
| REQ-REL-003 | §1.1.1 workspace 版本集中管理 | `Cargo.toml:6`（存量，已满足） |
| REQ-REL-004~006 | §2.6.3 sz-pay 验证脚本 | `scripts/verify_sz_pay.ps1` |
| REQ-REL-007 | §2.6.4 凭证安全设计 | 环境变量 `CARGO_REGISTRY_TOKEN` |
| REQ-REL-008 | §2.5.3 发布前门禁测试 | `scripts/gate.ps1`（复用） |
| REQ-REL-009 | §2.6.2 失败即中止 | `publish_crates_io.ps1` 中 `exit 1` |
| REQ-REL-010 | §2.6.3 ADR-0001 合规 | 仅修改 sz-pay `Cargo.toml` |

---

> **文档结束**
> 本文档为技术设计（How to build），对应需求规格 `.codeartsdoer/specs/sz_orm_v2_4_0/spec.md`（33 条 EARS 需求）。
> 任务分解（tasks.md）由 spec-task-agent 处理。
