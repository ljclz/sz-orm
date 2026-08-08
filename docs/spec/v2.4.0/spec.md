# sz-orm v2.4.0 需求规格说明书

> 版本：v2.4.0
> 基线：v2.3.0（已全部完成）
> 日期：2026-08-07
> 需求格式：EARS（Ubiquitous / Event-driven / State-driven / Optional / Unwanted）
> 文档定位：需求规格（What to build），不含技术设计（How to build）

---

# 1. 组件定位

## 1.1 核心职责

本组件负责交付 sz-orm v2.4.0 的三项任务：SmartEagerLoader 五方言集成测试、SmartEagerLoader 性能基准、crates.io v2.3.0 正式发布，实现智能 Eager Loading 的跨方言正确性验证、性能量化与生态可用性闭环。

## 1.2 核心输入

1. **SmartEagerLoader 现有实现**：v2.3.0 已交付的 `SmartEagerLoader` / `StrategyResolver` / `N1Eliminator`（位于 `packages/sz-orm-core/src/`），作为集成测试与性能基准的被测对象。
2. **手动 Eager Loading 现有实现**：`EagerLoader`（`eager_loader.rs`），作为等价性验证的对照基准。
3. **五方言真实数据库连接**：MySQL 9.6（127.0.0.1:3306）、PostgreSQL 18（127.0.0.1:5432）、SQLite（in-memory）、Oracle 23ai Free（127.0.0.1:1521）、MSSQL（远程 sh-mssql-adrul9nm.sql.tencentcdb.com:22527），作为集成测试的执行环境。
4. **43 个待发布包**：workspace 全部成员（41 lib + cli + examples），当前版本 2.3.0，作为 crates.io 发布的输入。
5. **sz-pay 项目依赖清单**：`E:\vue\test\sz-pay\server\sz-rust\Cargo.toml` 中 7 个 sz-orm-* 依赖及 `[patch.crates-io]` 本地覆盖段，作为发布后验证的下游消费方。
6. **crates.io 发布凭证**：API token，作为逐包 publish 的鉴权输入。

## 1.3 核心输出

1. **五方言集成测试套件**：新增的集成测试代码与测试报告，证明 SmartEagerLoader 在各方言下与手动 Eager Loading 结果集等价。
2. **性能基准报告**：决策延迟测量数据、智能 vs 手动对比数据、N+1 消除前后对比数据，覆盖 10/100/1000/10000 四种规模。
3. **crates.io 已发布制品**：43 个包在 crates.io 上可见且版本为 2.3.0。
4. **sz-pay 依赖更新**：移除 `[patch.crates-io]` 本地覆盖后的 `Cargo.toml`，且 sz-pay 可从 crates.io 拉取 v2.3.0 构建成功。
5. **需求追溯矩阵**：本文档第 7 章，建立需求 ↔ 任务 ↔ 验收条件映射。

## 1.4 职责边界

本组件**不负责**以下事项：

1. **不新增 SmartEagerLoader 业务功能**：v2.4.0 不扩展 SmartEagerLoader 的策略矩阵或关联类型，仅验证已有实现的正确性与性能。
2. **不修改 sz-orm 核心 API 签名**：v2.4.0 无 Breaking Change，所有现有 API 保持向后兼容。
3. **不负责 sz-pay 业务功能开发**：仅验证 sz-pay 能从 crates.io 拉取 v2.3.0 并构建，不修改 sz-pay 业务代码。
4. **不负责 sz-rust 框架发布**：sz-rust-core 等框架包的发布不在本组件范围。
5. **不负责新方言适配**：不新增第六种数据库方言支持。
6. **不负责性能优化实现**：性能基准仅测量与报告，不包含对 SmartEagerLoader 内部的优化改写（若基准暴露问题，由后续版本处理）。

---

# 2. 领域术语

**SmartEagerLoader**
: 基于 `RelationKind` 自动选择最优 Eager Loading 策略的加载器，v2.3.0 任务 C 交付，通过 `EagerLoader::smart()` 扩展方法构造。
: 备注：被测对象，本版本不扩展其功能。

**StrategyResolver**
: 纯内存枚举匹配的策略决策器，无 IO、无状态，相同输入始终返回相同输出，决策延迟目标 ≤ 100μs。
: 备注：被测对象，性能基准的核心测量目标。

**LoadStrategy**
: 加载策略枚举，包含 Join（单次 JOIN 查询）、DataLoader（批量 IN 查询，2 次）、IntermediateTableBatch（中间表批量查询，2 次）三个变体。

**N1Eliminator**
: N+1 自动消除器，检测连续相同模式的查询并在达到阈值时合并为 `WHERE id IN (?,...)` 批量查询。
: 备注：性能基准需测量消除前后的查询次数与耗时差异。

**Eager Loading（预加载）**
: 在一次查询中批量加载关联数据，避免逐条查询导致的 N+1 问题的加载模式。

**手动 Eager Loading**
: 通过 `EagerLoader::new().with()` 链式 API 显式指定关联的加载方式，作为智能 Eager Loading 的等价性对照基准。

**五方言**
: sz-orm 支持的五种数据库方言集合：MySQL、PostgreSQL、SQLite、Oracle、MSSQL（SQL Server）。
: 备注：本机可用 MySQL/PostgreSQL/Oracle/SQLite，MSSQL 使用远程服务器。

**关联类型（RelationKind）**
: 模型间关系的分类枚举，包含 HasOne、BelongsTo、HasMany、ManyToMany 四种，决定 StrategyResolver 的策略选择。

**等价性验证**
: 证明 SmartEagerLoader 与手动 EagerLoader 在相同输入下产生相同结果集（相同行数、相同字段、相同值、相同嵌套深度）的验证方法。

**决策延迟**
: `StrategyResolver::resolve()` 方法从调用到返回的墙钟耗时，目标上限 100μs。
: 备注：纯内存操作，不含数据库 IO。

**crates.io 发布**
: 将 workspace 内 43 个包按依赖拓扑顺序逐个执行 `cargo publish` 上传到 crates.io 注册中心的过程。

**[patch.crates-io] 本地覆盖**
: Cargo 的依赖覆盖机制，用本地路径替换 crates.io 版本，用于未发布前的本地联调；发布后应移除以使用正式制品。
: 备注：sz-pay 当前有 7 个包的本地覆盖需在发布后移除。

**依赖拓扑顺序**
: 包之间的依赖关系构成的有向无环图（DAG）的拓扑排序，确保被依赖的包先发布，避免 `cargo publish` 因依赖未发布而失败。

---

# 3. 角色与边界

## 3.1 核心角色

- **ORM 库维护者**：执行集成测试编写、性能基准编写、crates.io 发布操作的本组件直接操作者。
- **sz-pay 业务开发者**：v2.3.0 发布后从 crates.io 拉取依赖的下游消费方，验证发布成功性的间接参与者。

## 3.2 外部系统

- **MySQL 9.6**：集成测试执行环境之一，DSN `mysql://root:test123@127.0.0.1:3306/sz_orm_test`。
- **PostgreSQL 18**：集成测试执行环境之二，DSN `postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`。
- **SQLite（in-memory）**：集成测试执行环境之三，无外部依赖。
- **Oracle 23ai Free**：集成测试执行环境之四，DSN `oracle://sys:test123@127.0.0.1:1521/freepdb1`（Sysdba 权限）。
- **MSSQL（SQL Server）**：集成测试执行环境之五，远程 `sh-mssql-adrul9nm.sql.tencentcdb.com:22527`。
- **crates.io 注册中心**：43 个包的发布目标，需 API token 鉴权。
- **sz-pay 项目**：发布后验证的下游消费方，路径 `E:\vue\test\sz-pay\server\sz-rust`。

## 3.3 交互上下文

```plantuml
@startuml
!theme plain
title sz-orm v2.4.0 系统上下文图

rectangle "sz-orm v2.4.0\n(集成测试 + 性能基准 + 发布)" as SzzOrm

actor "ORM 库维护者" as Maintainer
actor "sz-pay 业务开发者" as PayDev

database "MySQL 9.6" as MySQL
database "PostgreSQL 18" as PG
database "SQLite (内存)" as SQLite
database "Oracle 23ai" as Oracle
database "MSSQL (远程)" as MSSQL
cloud "crates.io" as CratesIo
rectangle "sz-pay 项目" as SzPay

Maintainer --> SzzOrm : 编写测试/基准/执行发布
SzzOrm --> MySQL : 集成测试 (等价性验证)
SzzOrm --> PG : 集成测试 (等价性验证)
SzzOrm --> SQLite : 集成测试 (等价性验证)
SzzOrm --> Oracle : 集成测试 (等价性验证)
SzzOrm --> MSSQL : 集成测试 (等价性验证)
SzzOrm --> CratesIo : 逐包 publish (43 包)
CratesIo --> SzPay : 拉取 v2.3.0 (发布后验证)
PayDev --> SzPay : cargo build 验证

@enduml
```

---

# 4. DFX 约束

## 4.1 性能

1. **决策延迟上限**：`StrategyResolver::resolve()` 单次调用的墙钟耗时必须 ≤ 100μs（P99）。
2. **基准采样配置**：criterion 基准必须使用 sample_size ≥ 100、warm_up ≥ 3s、measurement ≥ 10s，确保统计显著性。
3. **规模覆盖**：性能基准必须覆盖 10、100、1000、10000 四种数据集规模，每个规模独立测量。

## 4.2 可靠性

1. **五方言等价性**：SmartEagerLoader 在五种方言下与手动 EagerLoader 产生的结果集必须完全等价（行数、字段、值、嵌套深度）。
2. **测试零失败**：全 workspace 测试（`cargo test --workspace`）必须全部通过，零失败、零忽略（除明确标注 `#[ignore]` 的 soak/jepsen 长时测试）。
3. **发布完整性**：43 个包必须全部在 crates.io 上发布成功且版本号为 2.3.0，不允许部分发布。

## 4.3 安全性

1. **参数化查询**：所有 WHERE 条件必须使用参数化绑定（`where_eq` / `or_where_eq` 等），禁止 SQL 字符串拼接。
2. **禁止 SELECT \***：所有查询必须显式指定列，禁止 `SELECT *`。
3. **凭证安全**：crates.io token 不得硬编码进版本控制，通过环境变量 `CARGO_REGISTRY_TOKEN` 或 `cargo login` 传入。

## 4.4 可维护性

1. **禁止占位实现**：代码中禁止出现 `todo!` / `unimplemented!` / `unreachable!` 宏调用。
2. **unsafe 零容忍**：禁止 `unsafe` 代码块，除非附带 `// SAFETY:` 安全性论证注释。
3. **clippy 零警告**：`cargo clippy --workspace --all-targets -- -D warnings` 必须零警告。
4. **门禁全通过**：AGENTS.md 定义的 10 道门禁必须全部通过。

## 4.5 兼容性

1. **API 向后兼容**：v2.4.0 不得引入 Breaking Change，所有 v2.3.0 的公开 API 签名保持不变。
2. **Rust 版本兼容**：edition = "2021"，rust-version = "1.81"，不得提升。
3. **sz-pay 兼容**：发布后 sz-pay 移除 `[patch.crates-io]` 后必须能 `cargo build` 成功，业务行为不变。

---

# 5. 核心能力

## 5.1 SmartEagerLoader 五方言集成测试

### 5.1.1 业务规则

1. **等价性验证 — HasOne 关联**（EARS: Event-driven）
   当对 HasOne 关联执行 Eager Loading 时，系统应当保证 SmartEagerLoader 产生的结果集与手动 EagerLoader 完全等价（相同行数、相同字段值、相同嵌套结构）。
   a. 验收条件：[五种方言各执行 HasOne 关联查询] → [SmartEagerLoader 结果集 == 手动 EagerLoader 结果集，逐行逐字段比对通过]

2. **等价性验证 — HasMany 关联**（EARS: Event-driven）
   当对 HasMany 关联执行 Eager Loading 时，系统应当保证 SmartEagerLoader 产生的结果集与手动 EagerLoader 完全等价。
   a. 验收条件：[五种方言各执行 HasMany 关联查询] → [SmartEagerLoader 结果集 == 手动 EagerLoader 结果集，子记录集合无序比对通过]

3. **等价性验证 — ManyToMany 关联**（EARS: Event-driven）
   当对 ManyToMany 关联（含中间表）执行 Eager Loading 时，系统应当保证 SmartEagerLoader 产生的结果集与手动 EagerLoader 完全等价。
   a. 验收条件：[五种方言各执行 ManyToMany 关联查询] → [SmartEagerLoader 结果集 == 手动 EagerLoader 结果集，经中间表关联的记录集合无序比对通过]

4. **Join 策略覆盖**（EARS: State-driven）
   在 StrategyResolver 选定 Join 策略的状态下，系统应当对该策略执行独立的等价性验证，证明 Join 策略产出的结果集与手动单次 JOIN 查询一致。
   a. 验收条件：[HasOne/BelongsTo 关联触发 Join 策略] → [策略执行结果 == 手动 JOIN 查询结果]

5. **DataLoader 策略覆盖**（EARS: State-driven）
   在 StrategyResolver 选定 DataLoader 策略的状态下，系统应当对该策略执行独立的等价性验证，证明批量 IN 查询产出的结果集与手动逐条查询合并后一致。
   a. 验收条件：[HasMany 关联触发 DataLoader 策略] → [策略执行结果 == 手动逐条查询合并结果]

6. **IntermediateTable 策略覆盖**（EARS: State-driven）
   在 StrategyResolver 选定 IntermediateTableBatch 策略的状态下，系统应当对该策略执行独立的等价性验证，证明中间表批量查询产出的结果集与手动经中间表查询一致。
   a. 验收条件：[ManyToMany 有中间表触发 IntermediateTableBatch 策略] → [策略执行结果 == 手动中间表查询结果]

7. **MySQL 方言覆盖**（EARS: Ubiquitous）
   系统应当提供针对 MySQL 9.6 的 SmartEagerLoader 集成测试，连接 `mysql://root:test123@127.0.0.1:3306/sz_orm_test`。
   a. 验收条件：[执行 MySQL 集成测试] → [三种关联类型 × 三种策略全部等价性验证通过]

8. **PostgreSQL 方言覆盖**（EARS: Ubiquitous）
   系统应当提供针对 PostgreSQL 18 的 SmartEagerLoader 集成测试，连接 `postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`。
   a. 验收条件：[执行 PostgreSQL 集成测试] → [三种关联类型 × 三种策略全部等价性验证通过]

9. **SQLite 方言覆盖**（EARS: Ubiquitous）
   系统应当提供针对 SQLite（in-memory）的 SmartEagerLoader 集成测试，无外部依赖。
   a. 验收条件：[执行 SQLite 集成测试] → [三种关联类型 × 三种策略全部等价性验证通过]

10. **Oracle 方言覆盖**（EARS: Ubiquitous）
    系统应当提供针对 Oracle 23ai Free 的 SmartEagerLoader 集成测试，连接 `127.0.0.1:1521/freepdb1`（Sysdba 权限）。
    a. 验收条件：[执行 Oracle 集成测试] → [三种关联类型 × 三种策略全部等价性验证通过]

11. **MSSQL 方言覆盖**（EARS: Ubiquitous）
    系统应当提供针对 MSSQL（SQL Server）的 SmartEagerLoader 集成测试，连接远程 `sh-mssql-adrul9nm.sql.tencentcdb.com:22527`。
    a. 验收条件：[执行 MSSQL 集成测试] → [三种关联类型 × 三种策略全部等价性验证通过]

12. **结果集深度等价**（EARS: Ubiquitous）
    系统应当验证 SmartEagerLoader 与手动 EagerLoader 产生结果集的嵌套深度完全一致，包括多级关联（NestedEagerResult 树结构）。
    a. 验收条件：[多级关联查询] → [SmartEagerLoader 嵌套树深度 == 手动 EagerLoader 嵌套树深度，逐层节点数一致]

13. **禁止项 — 跳过方言**（EARS: Unwanted）
    如果任一方言因环境不可用而跳过测试，则系统应当在该测试标注 `#[ignore]` 并在测试报告中明确记录跳过原因，禁止静默跳过。
    a. 验收条件：[方言环境不可用] → [测试标注 #[ignore] + 报告记录跳过原因，非静默通过]

### 5.1.2 交互流程

```plantuml
@startuml
!theme plain
title SmartEagerLoader 五方言集成测试主流程

actor "ORM 库维护者" as Maintainer
participant "集成测试套件" as Suite
database "方言 DB" as DB
participant "SmartEagerLoader" as Smart
participant "手动 EagerLoader" as Manual

Maintainer -> Suite : cargo test --workspace\n-- --ignored (五方言)
loop 五种方言
    Suite -> DB : 建表 + 插入测试数据
    loop 三种关联类型 (HasOne/HasMany/ManyToMany)
        Suite -> Smart : 加载关联 (smart())
        Smart -> DB : 按策略执行查询
        DB --> Smart : 结果集 A
        Suite -> Manual : 加载关联 (手动 with())
        Manual -> DB : 手动查询
        DB --> Manual : 结果集 B
        Suite -> Suite : 断言 A == B\n(行数/字段/值/深度)
    end
    Suite -> DB : 清理测试数据
end
Suite --> Maintainer : 测试报告 (全部通过/失败明细)

@enduml
```

### 5.1.3 异常场景

1. **方言连接失败**
   a. 触发条件：某方言数据库不可达（服务未启动、端口被占、凭证错误）
   b. 系统行为：该方言测试返回连接错误，不影响其他方言测试执行
   c. 用户感知：测试报告标注该方言 FAIL + 连接错误详情，其余方言结果独立呈现

2. **结果集不等价**
   a. 触发条件：SmartEagerLoader 与手动 EagerLoader 结果集存在差异（行数不同、字段值不同、嵌套深度不同）
   b. 系统行为：断言失败，输出差异明细（差异行号、差异字段、期望值 vs 实际值）
   c. 用户感知：测试 FAIL + 差异明细，定位到具体关联类型与方言

3. **策略选择与预期不符**
   a. 触发条件：StrategyResolver 对某关联选定的策略与设计矩阵不符（如 HasOne 未选 Join）
   b. 系统行为：策略断言失败，输出实际选定策略与预期策略
   c. 用户感知：测试 FAIL + 策略决策日志（StrategyDecision 的 reason 字段）

4. **中间表缺失**
   a. 触发条件：ManyToMany 关联未配置中间表元数据（join_table 为 None）
   b. 系统行为：StrategyResolver 回退 DataLoader 策略并发出 tracing::warn! 告警
   c. 用户感知：测试验证回退行为正确 + 告警日志存在

---

## 5.2 SmartEagerLoader 性能基准

### 5.2.1 业务规则

1. **决策延迟 ≤ 100μs**（EARS: Ubiquitous）
   系统应当通过性能基准证明 `StrategyResolver::resolve()` 单次调用的 P99 墙钟耗时 ≤ 100μs。
   a. 验收条件：[运行决策延迟基准] → [P99 耗时 ≤ 100μs，基准报告含 P50/P95/P99/Max 统计]

2. **智能 vs 手动性能对比**（EARS: Event-driven）
   当执行智能 Eager Loading 与手动 Eager Loading 性能对比基准时，系统应当输出两者的耗时对比数据，证明智能策略不引入显著性能退化（智能耗时 ≤ 手动耗时 × 1.10，即退化容忍上限 10%）。
   a. 验收条件：[运行对比基准] → [智能耗时 / 手动耗时 ≤ 1.10，报告含两者均值/中位数/P99]

3. **N+1 消除前后对比**（EARS: Event-driven）
   当执行 N+1 自动消除前后性能对比基准时，系统应当输出消除前（逐条查询）与消除后（N1Eliminator 批量合并）的查询次数与耗时对比，证明消除后查询次数显著减少。
   a. 验收条件：[运行 N+1 消除对比基准] → [消除后查询次数 < 消除前查询次数，耗时降幅可量化，报告含前后查询次数/耗时]

4. **数据集规模 10 覆盖**（EARS: State-driven）
   在数据集规模为 10 条记录的状态下，系统应当执行完整的性能基准（决策延迟 + 智能 vs 手动 + N+1 消除对比）。
   a. 验收条件：[规模 = 10] → [三类基准全部执行并产出数据]

5. **数据集规模 100 覆盖**（EARS: State-driven）
   在数据集规模为 100 条记录的状态下，系统应当执行完整的性能基准。
   a. 验收条件：[规模 = 100] → [三类基准全部执行并产出数据]

6. **数据集规模 1000 覆盖**（EARS: State-driven）
   在数据集规模为 1000 条记录的状态下，系统应当执行完整的性能基准。
   a. 验收条件：[规模 = 1000] → [三类基准全部执行并产出数据]

7. **数据集规模 10000 覆盖**（EARS: State-driven）
   在数据集规模为 10000 条记录的状态下，系统应当执行完整的性能基准。
   a. 验收条件：[规模 = 10000] → [三类基准全部执行并产出数据]

8. **基准报告生成**（EARS: Ubiquitous）
   系统应当生成结构化的性能基准报告，包含 Markdown + CSV/JSON 格式，覆盖所有维度（决策延迟 / 智能 vs 手动 / N+1 消除 × 4 规模），并对 DSN 脱敏。
   a. 验收条件：[基准执行完成] → [产出 Markdown 报告 + CSV/JSON 数据，DSN 已脱敏，含时间戳/环境信息]

9. **criterion 配置合规**（EARS: Ubiquitous）
   系统应当使用 criterion 配置 sample_size ≥ 100、warm_up ≥ 3s、measurement ≥ 10s，确保基准结果具备统计显著性。
   a. 验收条件：[检查 criterion 配置] → [sample_size ≥ 100 ∧ warm_up ≥ 3s ∧ measurement ≥ 10s]

10. **禁止项 — 基准跳过规模**（EARS: Unwanted）
    如果任一数据集规模的基准未执行，则系统应当在该基准标注并报告缺失原因，禁止静默缺失。
    a. 验收条件：[某规模基准未执行] → [报告标注缺失 + 原因，非静默通过]

### 5.2.2 交互流程

```plantuml
@startuml
!theme plain
title SmartEagerLoader 性能基准主流程

actor "ORM 库维护者" as Maintainer
participant "基准套件" as Bench
participant "StrategyResolver" as Resolver
participant "SmartEagerLoader" as Smart
participant "手动 EagerLoader" as Manual
participant "N1Eliminator" as N1
database "测试 DB" as DB

Maintainer -> Bench : cargo bench (性能基准)

== 决策延迟基准 ==
loop 10000 次采样
    Bench -> Resolver : resolve(relation)
    Resolver --> Bench : StrategyDecision (计时)
end
Bench -> Bench : 统计 P50/P95/P99/Max

== 智能 vs 手动对比 ==
loop 4 种规模 (10/100/1000/10000)
    Bench -> DB : 插入 N 条测试数据
    Bench -> Smart : 加载关联 (计时)
    Smart -> DB : 查询
    Bench -> Manual : 加载关联 (计时)
    Manual -> DB : 查询
    Bench -> Bench : 计算 智能耗时/手动耗时
    Bench -> DB : 清理数据
end

== N+1 消除对比 ==
loop 4 种规模
    Bench -> DB : 插入 N 条数据
    Bench -> N1 : 逐条查询 (计时/计数)
    Bench -> N1 : 批量合并 (计时/计数)
    Bench -> Bench : 对比前后查询次数/耗时
    Bench -> DB : 清理数据
end

Bench --> Maintainer : 基准报告 (Markdown + CSV/JSON)

@enduml
```

### 5.2.3 异常场景

1. **决策延迟超限**
   a. 触发条件：`StrategyResolver::resolve()` P99 耗时 > 100μs
   b. 系统行为：基准报告标注决策延迟超标，输出实际 P99 值与超标幅度
   c. 用户感知：基准报告 WARN + 实际 P99 值 + 超标幅度

2. **智能性能显著退化**
   a. 触发条件：智能 Eager Loading 耗时 > 手动耗时 × 1.10（退化超 10%）
   b. 系统行为：基准报告标注性能退化，输出智能/手动耗时比
   c. 用户感知：基准报告 WARN + 退化比例 + 规模维度

3. **N+1 消除未生效**
   a. 触发条件：N1Eliminator 合并后查询次数 ≥ 合并前查询次数（未减少）
   b. 系统行为：基准报告标注消除无效，输出前后查询次数
   c. 用户感知：基准报告 WARN + 前后查询次数对比

4. **大规模数据插入超时**
   a. 触发条件：10000 规模数据插入耗时超过基准超时阈值
   b. 系统行为：该规模基准超时失败，不影响其他规模基准
   c. 用户感知：基准报告标注该规模 TIMEOUT + 超时详情

---

## 5.3 crates.io v2.3.0 发布

### 5.3.1 业务规则

1. **43 包全部发布**（EARS: Ubiquitous）
   系统应当将 workspace 全部 43 个成员包发布到 crates.io，发布后版本号均为 2.3.0。
   a. 验收条件：[发布完成] → [43 个包在 crates.io 上可见 ∧ 版本号 == 2.3.0]

2. **依赖拓扑顺序发布**（EARS: State-driven）
   在执行逐包发布的状态下，系统应当按依赖拓扑顺序（被依赖的包先发布）执行 `cargo publish`，确保任一包发布时其所有 sz-orm 内部依赖已在 crates.io 上可见。
   a. 验收条件：[逐包 publish] → [每包 publish 成功，无"依赖未找到"错误，拓扑顺序可追溯]

3. **版本号一致性**（EARS: Ubiquitous）
   系统应当保证所有 43 个包的版本号一致为 2.3.0（workspace.package.version 集中管理），不允许个别包版本偏差。
   a. 验收条件：[检查所有包 Cargo.toml] → [version.workspace = true ∧ workspace.package.version == "2.3.0"]

4. **sz-pay 从 crates.io 拉取 v2.3.0**（EARS: Event-driven）
   当 v2.3.0 发布完成后，系统应当验证 sz-pay 项目将 sz-orm-* 依赖版本从 2.1.0 升级到 2.3.0 后可从 crates.io 成功拉取并 `cargo build` 通过。
   a. 验收条件：[sz-pay 依赖升级到 2.3.0 + cargo build] → [构建成功，无编译错误]

5. **移除 [patch.crates-io] 本地覆盖**（EARS: Event-driven）
   当 v2.3.0 发布验证通过后，系统应当移除 sz-pay 的 `Cargo.toml` 中 `[patch.crates-io]` 段（7 个 sz-orm-* 本地路径覆盖），使 sz-pay 完全使用 crates.io 正式制品。
   a. 验收条件：[移除 patch 段 + cargo build] → [构建成功，依赖来源为 crates.io 而非本地路径]

6. **发布后 sz-pay 回归验证**（EARS: Event-driven）
   当移除 `[patch.crates-io]` 后，系统应当执行 sz-pay 的回归测试，证明业务行为与本地覆盖期间一致（零回归）。
   a. 验收条件：[sz-pay 回归测试] → [全部通过，业务行为不变]

7. **凭证安全**（EARS: Ubiquitous）
   系统应当通过环境变量或 `cargo login` 传入 crates.io token，禁止将 token 硬编码进版本控制文件。
   a. 验收条件：[检查发布脚本/配置] → [token 不出现在 git 跟踪文件中，通过环境变量传入]

8. **发布前门禁全通过**（EARS: State-driven）
   在执行 `cargo publish` 之前，系统应当确保 AGENTS.md 定义的 10 道门禁全部通过（fmt/check/clippy/test/doc/audit/integration/占位检查/SQL 注入扫描/Feature 全组合）。
   a. 验收条件：[发布前门禁检查] → [10 道门禁全部 PASS，任一 FAIL 则阻断发布]

9. **禁止项 — 部分发布**（EARS: Unwanted）
   如果任一包发布失败，则系统应当立即中止后续包发布，输出失败包名与错误详情，禁止部分发布后声称"发布完成"。
   a. 验收条件：[某包 publish 失败] → [中止后续发布 + 输出失败包名/错误，不标记发布完成]

10. **禁止项 — 覆盖上游仓库**（EARS: Unwanted）
    如果发布过程需要修改 sz-orm 仓库文件，则系统应当仅修改版本号相关配置，禁止借发布之名修改业务代码（ADR-0001）。
    a. 验收条件：[发布相关 git diff] → [仅版本号/发布元数据变更，无业务代码变更]

### 5.3.2 交互流程

```plantuml
@startuml
!theme plain
title crates.io v2.3.0 发布主流程

actor "ORM 库维护者" as Maintainer
participant "门禁检查" as Gate
participant "发布脚本" as Publish
cloud "crates.io" as CratesIo
rectangle "sz-pay" as SzPay

Maintainer -> Gate : 执行 10 道门禁
Gate --> Maintainer : 全部 PASS

Maintainer -> Publish : cargo login (token)
Publish -> Publish : 计算依赖拓扑顺序

loop 43 个包 (拓扑序)
    Publish -> CratesIo : cargo publish <pkg>
    alt 发布成功
        CratesIo --> Publish : OK
    else 发布失败
        CratesIo --> Publish : ERROR
        Publish --> Maintainer : 中止 + 失败详情
    end
end
Publish --> Maintainer : 43 包全部发布成功

== sz-pay 验证 ==
Maintainer -> SzPay : 依赖升级 2.1.0 -> 2.3.0
Maintainer -> SzPay : 移除 [patch.crates-io]
Maintainer -> SzPay : cargo build
SzPay -> CratesIo : 拉取 sz-orm-* 2.3.0
CratesIo --> SzPay : 包数据
SzPay --> Maintainer : 构建成功
Maintainer -> SzPay : cargo test (回归)
SzPay --> Maintainer : 全部通过 (零回归)

@enduml
```

### 5.3.3 异常场景

1. **门禁未通过**
   a. 触发条件：发布前 10 道门禁任一失败（如 clippy 有警告、测试有失败）
   b. 系统行为：阻断发布流程，输出失败门禁名称与详情
   c. 用户感知：发布中止 + 失败门禁名称 + 修复建议

2. **依赖未发布**
   a. 触发条件：某包 publish 时其内部依赖（如 sz-orm-macros）尚未在 crates.io 上可见
   b. 系统行为：该包 publish 失败，提示依赖版本未找到
   c. 用户感知：发布中止 + 未发布依赖名 + 建议按拓扑顺序重试

3. **版本号已存在**
   a. 触发条件：crates.io 上某包 2.3.0 版本已存在（重复发布）
   b. 系统行为：该包 publish 失败，提示版本已存在
   c. 用户感知：发布中止 + 冲突包名 + 建议升级版本号或确认是否需发布

4. **sz-pay 构建失败**
   a. 触发条件：移除 `[patch.crates-io]` 后 sz-pay `cargo build` 失败
   b. 系统行为：输出编译错误详情，保留 patch 段不删除
   c. 用户感知：构建失败 + 错误详情 + patch 段保留提示

5. **sz-pay 回归失败**
   a. 触发条件：sz-pay 回归测试出现失败（业务行为变化）
   b. 系统行为：输出失败测试详情，标记发布验证未完成
   c. 用户感知：回归失败 + 失败测试列表 + 建议排查兼容性

6. **token 鉴权失败**
   a. 触发条件：crates.io token 无效或过期
   b. 系统行为：首个包 publish 即失败，提示鉴权错误
   c. 用户感知：发布中止 + 鉴权错误 + 建议重新 cargo login

---

# 6. 数据约束

## 6.1 集成测试数据

1. **测试数据集**：每方言每关联类型的测试数据集必须包含 ≥ 5 条主记录 + ≥ 10 条关联记录，覆盖边界情况（空关联、单条关联、多条关联）。
2. **数据隔离**：各方言测试使用独立 schema/database，禁止跨方言共享数据。
3. **数据清理**：每条集成测试结束后必须清理所创建的测试数据，禁止残留数据影响后续测试。
4. **关联完整性**：测试数据的外键关系必须完整有效，禁止存在孤儿记录（除非测试场景明确需要）。

## 6.2 性能基准数据

1. **规模档位**：数据集规模必须精确为 10、100、1000、10000 四档，禁止使用近似值。
2. **数据分布**：基准数据的外键分布必须均匀（每主记录关联子记录数 ≈ N/主记录数），避免极端分布导致基准失真。
3. **测量次数**：每基准每规模必须执行 ≥ 100 次采样（criterion sample_size），取统计值。
4. **环境记录**：基准报告必须记录执行环境（CPU/Rust 版本/DB 版本/时间戳），保证可复现。

## 6.3 发布元数据

1. **版本号**：所有 43 包版本号必须为 2.3.0，通过 `workspace.package.version` 集中管理。
2. **包名**：43 个包名必须以 `sz-orm-` 前缀（cli/examples 除外），符合 crates.io 命名规范。
3. **license**：所有包 license 必须为 MIT，与 workspace 一致。
4. **repository**：所有包 repository 必须为 `https://github.com/ljclz/sz-orm`，与 workspace 一致。

## 6.4 sz-pay 依赖数据

1. **依赖包数量**：sz-pay 依赖的 sz-orm-* 包为 7 个（core/sqlx/config/auth/macros/queue/scheduler）。
2. **patch 段数量**：`[patch.crates-io]` 本地覆盖段必须恰好覆盖上述 7 个包，发布后全部移除。
3. **版本升级**：sz-pay 的 sz-orm-* 依赖版本必须从 2.1.0 升级到 2.3.0（与发布版本一致）。

---

# 7. 需求追溯矩阵

| 需求编号 | 需求描述 | EARS 类型 | 所属任务 | 验收条件 | 关联章节 |
|---------|---------|----------|---------|---------|---------|
| REQ-IT-001 | HasOne 关联等价性验证 | Event-driven | 任务1 集成测试 | 五方言 HasOne 结果集等价 | 5.1.1 规则1 |
| REQ-IT-002 | HasMany 关联等价性验证 | Event-driven | 任务1 集成测试 | 五方言 HasMany 结果集等价 | 5.1.1 规则2 |
| REQ-IT-003 | ManyToMany 关联等价性验证 | Event-driven | 任务1 集成测试 | 五方言 ManyToMany 结果集等价 | 5.1.1 规则3 |
| REQ-IT-004 | Join 策略覆盖 | State-driven | 任务1 集成测试 | Join 策略结果 == 手动 JOIN | 5.1.1 规则4 |
| REQ-IT-005 | DataLoader 策略覆盖 | State-driven | 任务1 集成测试 | DataLoader 结果 == 手动合并 | 5.1.1 规则5 |
| REQ-IT-006 | IntermediateTable 策略覆盖 | State-driven | 任务1 集成测试 | IntermediateTable 结果 == 手动中间表 | 5.1.1 规则6 |
| REQ-IT-007 | MySQL 方言覆盖 | Ubiquitous | 任务1 集成测试 | MySQL 三关联×三策略通过 | 5.1.1 规则7 |
| REQ-IT-008 | PostgreSQL 方言覆盖 | Ubiquitous | 任务1 集成测试 | PG 三关联×三策略通过 | 5.1.1 规则8 |
| REQ-IT-009 | SQLite 方言覆盖 | Ubiquitous | 任务1 集成测试 | SQLite 三关联×三策略通过 | 5.1.1 规则9 |
| REQ-IT-010 | Oracle 方言覆盖 | Ubiquitous | 任务1 集成测试 | Oracle 三关联×三策略通过 | 5.1.1 规则10 |
| REQ-IT-011 | MSSQL 方言覆盖 | Ubiquitous | 任务1 集成测试 | MSSQL 三关联×三策略通过 | 5.1.1 规则11 |
| REQ-IT-012 | 结果集深度等价 | Ubiquitous | 任务1 集成测试 | 嵌套树深度一致 | 5.1.1 规则12 |
| REQ-IT-013 | 禁止静默跳过方言 | Unwanted | 任务1 集成测试 | 跳过需标注+记录 | 5.1.1 规则13 |
| REQ-PB-001 | 决策延迟 ≤ 100μs | Ubiquitous | 任务2 性能基准 | P99 ≤ 100μs | 5.2.1 规则1 |
| REQ-PB-002 | 智能 vs 手动性能对比 | Event-driven | 任务2 性能基准 | 智能耗时/手动耗时 ≤ 1.10 | 5.2.1 规则2 |
| REQ-PB-003 | N+1 消除前后对比 | Event-driven | 任务2 性能基准 | 消除后查询次数 < 消除前 | 5.2.1 规则3 |
| REQ-PB-004 | 规模 10 覆盖 | State-driven | 任务2 性能基准 | 规模10 三类基准执行 | 5.2.1 规则4 |
| REQ-PB-005 | 规模 100 覆盖 | State-driven | 任务2 性能基准 | 规模100 三类基准执行 | 5.2.1 规则5 |
| REQ-PB-006 | 规模 1000 覆盖 | State-driven | 任务2 性能基准 | 规模1000 三类基准执行 | 5.2.1 规则6 |
| REQ-PB-007 | 规模 10000 覆盖 | State-driven | 任务2 性能基准 | 规模10000 三类基准执行 | 5.2.1 规则7 |
| REQ-PB-008 | 基准报告生成 | Ubiquitous | 任务2 性能基准 | Markdown+CSV/JSON+DSN脱敏 | 5.2.1 规则8 |
| REQ-PB-009 | criterion 配置合规 | Ubiquitous | 任务2 性能基准 | sample≥100/warm≥3s/measure≥10s | 5.2.1 规则9 |
| REQ-PB-010 | 禁止静默缺失规模 | Unwanted | 任务2 性能基准 | 缺失需标注+原因 | 5.2.1 规则10 |
| REQ-REL-001 | 43 包全部发布 | Ubiquitous | 任务3 发布 | 43包可见∧版本2.3.0 | 5.3.1 规则1 |
| REQ-REL-002 | 依赖拓扑顺序发布 | State-driven | 任务3 发布 | 拓扑序publish无依赖错误 | 5.3.1 规则2 |
| REQ-REL-003 | 版本号一致性 | Ubiquitous | 任务3 发布 | 全包version=2.3.0 | 5.3.1 规则3 |
| REQ-REL-004 | sz-pay 拉取 v2.3.0 | Event-driven | 任务3 发布 | 升级后cargo build成功 | 5.3.1 规则4 |
| REQ-REL-005 | 移除 [patch.crates-io] | Event-driven | 任务3 发布 | 移除后构建成功 | 5.3.1 规则5 |
| REQ-REL-006 | sz-pay 回归验证 | Event-driven | 任务3 发布 | 回归测试零失败 | 5.3.1 规则6 |
| REQ-REL-007 | 凭证安全 | Ubiquitous | 任务3 发布 | token不入git | 5.3.1 规则7 |
| REQ-REL-008 | 发布前门禁全通过 | State-driven | 任务3 发布 | 10道门禁全PASS | 5.3.1 规则8 |
| REQ-REL-009 | 禁止部分发布 | Unwanted | 任务3 发布 | 失败即中止 | 5.3.1 规则9 |
| REQ-REL-010 | 禁止覆盖上游业务代码 | Unwanted | 任务3 发布 | 仅版本号变更 | 5.3.1 规则10 |

---

# 8. 约束条件汇总

## 8.1 语言与工具链

| 约束项 | 约束值 | 来源 |
|-------|-------|------|
| Rust edition | 2021 | workspace.package.edition |
| rust-version | 1.81 | workspace.package.rust-version |
| 异步运行时 | tokio 1.40 (full) | workspace.dependencies |
| 基准框架 | criterion 0.5 (html_reports + async_tokio) | bench-comparison |

## 8.2 五方言环境

| 方言 | 版本 | 连接信息 | 可用性 |
|------|------|---------|--------|
| MySQL | 9.6 | mysql://root:test123@127.0.0.1:3306/sz_orm_test | 本机可用 |
| PostgreSQL | 18 | postgres://postgres:test123@127.0.0.1:5432/sz_orm_test | 本机可用 |
| SQLite | bundled | in-memory | 本机可用 |
| Oracle | 23ai Free | 127.0.0.1:1521/freepdb1 (Sysdba) | 本机可用 |
| MSSQL | SQL Server | sh-mssql-adrul9nm.sql.tencentcdb.com:22527 | 远程可用 |

## 8.3 工程化铁律

| 编号 | 铁律 | 验证方式 |
|------|------|---------|
| C-01 | 禁止占位实现 | grep todo!/unimplemented!/unreachable! |
| C-02 | unsafe 零容忍 | grep unsafe（须有 // SAFETY: 注释） |
| C-03 | 参数化查询 | where_eq/or_where_eq，禁止 where_cond/or_where |
| C-04 | 禁止 SELECT * | SQL 注入扫描脚本 |
| C-05 | API 向后兼容 | 无 Breaking Change |
| C-06 | clippy 零警告 | cargo clippy -- -D warnings |
| C-07 | 10 道门禁全通过 | gate.ps1 |
| C-08 | ADR-0001 不改上游 | git diff 仅版本号 |

## 8.4 发布约束

| 约束项 | 约束值 |
|-------|-------|
| 发布目标 | crates.io |
| 发布版本 | 2.3.0 |
| 包数量 | 43（41 lib + cli + examples） |
| 下游验证 | sz-pay（7 个 sz-orm-* 依赖） |
| token 传入 | 环境变量 / cargo login，禁止入 git |

---

# 9. 验收标准总览

## 9.1 任务 1 验收标准（SmartEagerLoader 五方言集成测试）

- [ ] AC-IT-1：五种方言（MySQL/PG/SQLite/Oracle/MSSQL）的集成测试全部执行且通过
- [ ] AC-IT-2：三种关联类型（HasOne/HasMany/ManyToMany）的等价性验证全部通过
- [ ] AC-IT-3：三种策略（Join/DataLoader/IntermediateTable）的独立覆盖验证全部通过
- [ ] AC-IT-4：结果集深度等价验证（多级关联 NestedEagerResult）通过
- [ ] AC-IT-5：任一方言不可用时标注 `#[ignore]` 并记录原因，非静默跳过
- [ ] AC-IT-6：集成测试代码无占位实现、无 unsafe、clippy 零警告

## 9.2 任务 2 验收标准（SmartEagerLoader 性能基准）

- [ ] AC-PB-1：`StrategyResolver::resolve()` P99 决策延迟 ≤ 100μs
- [ ] AC-PB-2：智能 vs 手动 Eager Loading 耗时比 ≤ 1.10（退化容忍 10%）
- [ ] AC-PB-3：N+1 消除后查询次数 < 消除前，耗时降幅可量化
- [ ] AC-PB-4：四种规模（10/100/1000/10000）基准全部执行并产出数据
- [ ] AC-PB-5：基准报告生成（Markdown + CSV/JSON），DSN 已脱敏
- [ ] AC-PB-6：criterion 配置合规（sample_size ≥ 100, warm_up ≥ 3s, measurement ≥ 10s）

## 9.3 任务 3 验收标准（crates.io v2.3.0 发布）

- [ ] AC-REL-1：43 个包全部在 crates.io 上发布成功，版本号 2.3.0
- [ ] AC-REL-2：按依赖拓扑顺序发布，无"依赖未找到"错误
- [ ] AC-REL-3：发布前 10 道门禁全部通过
- [ ] AC-REL-4：sz-pay 依赖升级到 2.3.0 后 `cargo build` 成功
- [ ] AC-REL-5：移除 `[patch.crates-io]` 后 sz-pay `cargo build` 成功（依赖来源为 crates.io）
- [ ] AC-REL-6：sz-pay 回归测试全部通过（零回归）
- [ ] AC-REL-7：crates.io token 未入版本控制
- [ ] AC-REL-8：发布过程未修改 sz-orm 业务代码（仅版本号/发布元数据变更）

## 9.4 总体验收标准

- [ ] AC-ALL-1：v2.4.0 无 Breaking Change，v2.3.0 公开 API 全部保持不变
- [ ] AC-ALL-2：全 workspace `cargo test --workspace` 全部通过
- [ ] AC-ALL-3：全 workspace `cargo clippy --workspace --all-targets -- -D warnings` 零警告
- [ ] AC-ALL-4：CHANGELOG.md 更新 v2.4.0 变更记录
- [ ] AC-ALL-5：本需求规格文档所有 REQ 编号需求全部满足

---

> **文档结束**
> 本文档为需求规格（What to build），不含技术设计（How to build）。
> 技术设计文档（design.md）与任务分解（tasks.md）由后续 spec-design-agent 与 spec-task-agent 分别处理。