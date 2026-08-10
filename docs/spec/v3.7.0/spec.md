# sz-orm v3.7.0 需求规格说明书

> 版本：v3.7.0（真实数据库端到端测试体系 + 对比分析重评估与文档同步 + v3.6.0 探索能力成熟化 + 方言扩展延续 + 云数仓真实验证 + 工程规范化）
> 基线：v3.6.0（已完成 M1-M5：编译期类型安全深入优化 15 新表达式 + 313 pub API 文档补齐 195 missing_docs 全补齐 + QueryBuilder 渐进合并 lint/fix/差分测试 47 测试 + 方言扩展 Snowflake/Redshift 20 种方言 32 测试 + async trait 重评估保持方案 C）
> 日期：2026-08-10
> 需求格式：EARS（Ubiquitous / Event-driven / State-driven / Optional / Unwanted）
> 文档定位：需求规格（What to build），不含技术设计（How to build）
> 优先级声明：六项能力按"真实数据库端到端测试体系(1,最高,补 v3.6.0 最大缺口) → 对比分析重评估与文档同步(2,高,用户明确要求且文档滞后一版本) → v3.6.0 探索能力成熟化(3,高,探索性质需转正式) → 方言扩展延续(4,中,补企业数据库按需) → Snowflake/Redshift 真实云数据库验证(5,中,补 v3.6.0 方言测试缺口) → 工程规范化(6,低但必须,git 入库 + Prisma 评估落地)"的收益/风险序推进
> 需求编号约定：REQ-E2E-xxx（真实数据库端到端测试体系）/ REQ-REEVAL-xxx（对比分析重评估与文档同步）/ REQ-MAT-xxx（v3.6.0 探索能力成熟化）/ REQ-DIALECT-xxx（方言扩展延续）/ REQ-CLOUD-xxx（Snowflake/Redshift 真实云数据库验证）/ REQ-ENG-xxx（工程规范化）
> 缺陷来源：v3.6.0 端到端测试缺口（96 e2e 测试全用 InMemoryDb，63 真实 DB 测试全 #[ignore]）+ 对比分析文档滞后（停留 v3.5.0 基线）+ v3.6.0 探索性质能力（typed relation / 自定义诊断 / proc-macro SQL 验证）成熟度不足 + v3.5.0 方言扩展路线图 v3.7.0 候选（Informix/SAP HANA/Firebird）+ v3.6.0 Snowflake/Redshift 无真实云数据库验证 + 213 文件未提交 git
> 兼容性铁律：所有改进通过 feature gate 隔离，默认 feature 行为不变，既有公开 API 完全向后兼容，v3.6.0 已验收测试基线不回退；真实数据库端到端测试通过 `e2e-real-db` feature gate 隔离，默认关闭（避免无 DB 环境编译失败），CI 中启用；对比分析文档更新不改变任何代码行为；探索能力成熟化不破坏 v3.6.0 既有 API

---

# 1. 组件定位

## 1.1 核心职责

本组件负责交付 sz-orm v3.7.0 的六项优化任务：真实数据库端到端测试体系（建立连真实 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 的端到端测试，覆盖核心路径，通过 feature gate 隔离使 CI 可启用而本地无 DB 可跳过）、对比分析文档重评估与同步（将停留 v3.5.0 基线的对比分析文档更新到 v3.6.0 基线，重新评分各维度，更新已知不足与改进建议）、v3.6.0 探索能力成熟化（将 typed relation / 自定义编译期诊断 / proc-macro SQL 验证从探索性质转为正式 feature，补齐测试与文档）、方言扩展延续（Informix/SAP HANA/Firebird 按需实现）、Snowflake/Redshift 真实云数据库验证（补齐 v3.6.0 方言仅 SQL 生成测试的缺口）、工程规范化（v3.6.0 未提交工作入库 + Prisma 方言评估结论落地），实现 sz-orm 在"端到端测试可信度、文档与代码一致性、探索能力正式化、方言覆盖度、云数仓验证完整性、工程规范"六个维度的优化，且不破坏现有 API 兼容性与 v3.6.0 已验收基线。

## 1.2 核心输入

1. **v3.6.0 已验收基线**：M1-M5 五个里程碑已完成，测试基线不回退，作为本版本基准。
2. **v3.6.0 端到端测试现状**：5 个 `e2e_*.rs` 文件 96 个测试全用 `InMemoryDb`（`packages/sz-orm-core/tests/e2e_transaction.rs:27`），6 个 `integration_*.rs` 文件 63 个测试全 `#[ignore]`（`packages/sz-orm-core/tests/integration_mysql.rs:113`），无"默认运行 + 连真实 DB"的完整链路 e2e 测试，作为方向 1 需求来源。
3. **对比分析文档滞后证据**：`docs/sz-orm与同类产品对比分析.md:3` 标注 `版本：v3.5.0 | 日期：2026-08-09`，而 `Cargo.toml:6` 已是 `version = "3.6.0"`，文档滞后一个版本，作为方向 2 需求来源。
4. **v3.6.0 探索性质能力清单**：typed relation（编译期类型安全关联查询，v3.6.0 M1 探索实现）、自定义编译期诊断（`packages/sz-orm-macros/tests/typed_ast_diagnostic_test.rs`）、proc-macro SQL 验证（v3.6.0 M1 探索），均为探索性质未转正式 feature，作为方向 3 需求来源。
5. **v3.5.0 方言扩展路线图 v3.7.0 候选**：`docs/spec/v3.6.0/spec.md:943-945` 建议 v3.7.0 实现 Informix/SAP HANA/Firebird，作为方向 4 需求来源。
6. **v3.6.0 Snowflake/Redshift 测试缺口**：`packages/sz-orm-core/tests/dialect_snowflake_test.rs` + `dialect_redshift_test.rs` 共 32 测试均为 SQL 生成测试，未连真实云数据库验证行为一致性，作为方向 5 需求来源。
7. **v3.6.0 未提交工作**：`git status` 显示 213 个未提交改动文件，`git log` 只到 v3.3.0，v3.6.0 工作未入库，作为方向 6 需求来源。
8. **本机数据库连接信息**：MySQL 9.6（`mysql://root:test123@127.0.0.1:3306/sz_orm_test`）、PostgreSQL 18（`postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`）、Oracle 23ai Free（`127.0.0.1:1521/freepdb1`），作为真实 DB 端到端测试的连接输入。
9. **既有 integration_*.rs 测试基础设施**：`packages/sz-orm-core/tests/common/` 已有 sqlx_pg_adapter / sqlx_mysql_adapter / rusqlite_adapter / schema_builder / equivalence，作为真实 DB e2e 测试的复用基础。
10. **既有 feature gate 体系**：`packages/sz-orm-core/Cargo.toml` 已有 25+ feature，作为新能力 feature gate 隔离的基础。
11. **sz-pay 生产依赖证据**：sz-pay 从 crates.io 拉取 sz-orm-*，作为 API 兼容性验证的下游基准。
12. **六方言覆盖约束**：MySQL/PostgreSQL/SQLite/Oracle/MSSQL/Snowflake/Redshift，真实 DB e2e 测试须覆盖本机可用的方言。

## 1.3 核心输出

1. **真实数据库端到端测试体系**：连真实 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 的 e2e 测试套件，覆盖 CRUD + 事务 + 关联预加载 + 分页 + 软删除 + 多租户 + 缓存 + 方言行为一致性，通过 `e2e-real-db` feature gate 隔离，CI 中启用，本地无 DB 可跳过。
2. **对比分析文档 v3.6.0 基线更新版**：`docs/sz-orm与同类产品对比分析.md` 更新到 v3.6.0 基线，重新评分 13 个维度，更新 §6 已知不足（标注 v3.6.0 已改进项），更新 §7 结论与建议。
3. **v3.6.0 探索能力正式化**：typed relation / 自定义编译期诊断 / proc-macro SQL 验证从探索转为正式 feature，补齐测试覆盖与文档，标注稳定可用。
4. **方言扩展能力**：Informix/SAP HANA/Firebird 方言（按需实现，通过 feature gate 隔离），方言数量从 20 种扩展。
5. **Snowflake/Redshift 真实云数据库验证报告**：连真实 Snowflake/Redshift（或评估无可用云实例时输出验证缺口报告 + 替代方案）。
6. **工程规范化交付**：v3.6.0 未提交工作入库 + Prisma 方言评估结论落地文档。
7. **需求追溯矩阵**：本文档第 7 章，建立需求 ↔ 验收条件映射。
8. **验收标准总览**：本文档第 9 章，按方向汇总验收条件。

## 1.4 职责边界

本组件**不负责**以下事项：

1. **不破坏既有公开 API**：所有新能力通过 feature gate 隔离，既有公开 API 签名保持完全向后兼容。
2. **不强制本地无 DB 环境运行真实 DB 测试**：真实 DB e2e 测试通过 `e2e-real-db` feature gate 隔离，默认关闭，仅在 CI 或本地有 DB 时启用，避免无 DB 环境编译失败。
3. **不替换既有 InMemoryDb e2e 测试**：既有 96 个 `e2e_*.rs`（InMemoryDb）测试保留，真实 DB e2e 测试为新增补充，两者共存。
4. **不替换既有 integration_*.rs 测试**：既有 63 个 `#[ignore]` 真实 DB 集成测试保留，真实 DB e2e 测试为新增的"默认运行（feature gate 启用时）"层次，覆盖更广业务路径。
5. **不实现所有候选方言**：方言扩展仅对用户需求驱动的方言实施，其他仅规划。
6. **不强制实现 Prisma 方言**：Prisma 方言仅落地 v3.6.0 评估结论，如评估为不可行则输出结论文档不实现。
7. **不负责社区规模扩展（§6.3）**：社区运营非代码改进范畴，v3.7.0 不包含。
8. **不负责扩展生产案例（§6.4）**：需外部采纳，v3.7.0 不包含。
9. **不修改 sz-pay / sz-rust 下游代码**：ADR-0001 严禁修改下游/上游仓库。
10. **不降低既有测试覆盖**：v3.7.0 不得使 v3.6.0 已验收测试基线回退，仅增不减。
11. **不改变既有安全铁律**：任何 WHERE 条件必须参数化，默认禁止 `SELECT *`，N+1 检测自动拦截，沿用既有铁律。
12. **不负责 LLM 模型训练与托管**：沿用既有边界。

---

# 2. 领域术语

**真实数据库端到端测试（Real-Database End-to-End Test）**
: 连接真实数据库实例（MySQL/PostgreSQL/SQLite/Oracle/MSSQL）执行完整业务路径（应用层 API → QueryBuilder → SQL 下推 → 真实 DB 执行 → 结果回填 → 断言），验证从用户视角到数据持久化的完整链路行为，区别于用 InMemoryDb 的逻辑端到端测试。
: 备注：v3.6.0 的 96 个 e2e 测试用 InMemoryDb（逻辑端到端），63 个 integration 测试连真实 DB 但全 `#[ignore]`，v3.7.0 补齐"feature gate 启用时默认运行"的真实 DB e2e 测试。

**逻辑端到端测试（Logical End-to-End Test）**
: 使用 InMemoryDb 或 mock 连接验证完整业务流程（SQL 下推 + 真实执行 + 行为验证），不连真实数据库，随 `cargo test` 默认运行。
: 备注：v3.6.0 的 `e2e_*.rs` 属于此类，v3.7.0 保留不替换。

**对比分析重评估（Comparative Analysis Re-evaluation）**
: 将基于 v3.5.0 基线的对比分析文档更新到 v3.6.0 基线，重新逐维度评分，更新已知不足章节（标注 v3.6.0 已改进项），更新结论与建议，确保文档与代码版本一致。
: 备注：对比分析文档 `docs/sz-orm与同类产品对比分析.md` 当前停留 v3.5.0，需更新到 v3.6.0。

**探索能力成熟化（Exploratory Capability Maturation）**
: 将 v3.6.0 以"探索"性质实现的能力（typed relation / 自定义编译期诊断 / proc-macro SQL 验证）转为正式 feature，补齐测试覆盖、文档、稳定性标注，使其可标注为生产可用。
: 备注：v3.6.0 M1 这些能力为探索实现，v3.7.0 成熟化。

**typed relation（类型安全关联查询）**
: 在 typed_ast DSL 中类型安全地表达 Model 之间的关联查询，编译期校验关联的外键类型匹配与表归属。
: 备注：v3.6.0 探索实现，v3.7.0 转正式 feature。

**自定义编译期诊断（Custom Compile-Time Diagnostic）**
: 通过 proc-macro 的 Diagnostic API 或 compile_error! 生成的自定义编译期错误信息，指明错误位置、期望类型、实际类型、修复建议。
: 备注：v3.6.0 探索实现（`typed_ast_diagnostic_test.rs`），v3.7.0 成熟化。

**proc-macro SQL 验证（Proc-Macro Compile-Time SQL Verification）**
: 通过 proc-macro 在编译期解析 SQL 字符串，校验 SQL 语法 + 表/列存在性 + 类型匹配，类似 sqlx::query! 宏。
: 备注：v3.6.0 探索实现，v3.7.0 成熟化，连真 DB EXPLAIN 覆盖所有 QueryBuilder 路径。

**Informix 方言（Informix Dialect）**
: IBM Informix 数据库的 SQL 方言，支持 SERIAL/ROW 类型、PUT 语句等特性。
: 备注：v3.6.0 路线图建议 v3.7.0 实现，需 Rust 驱动成熟。

**SAP HANA 方言（SAP HANA Dialect）**
: SAP HANA 内存数据库的 SQL 方言，支持计算列、CE 函数等特性。
: 备注：v3.6.0 路线图建议 v3.7.0 实现，需 Rust 驱动成熟 + 企业需求。

**Firebird 方言（Firebird Dialect）**
: Firebird 数据库的 SQL 方言，支持 GENERATOR/SEQUENCE、EXECUTE BLOCK 等特性。
: 备注：v3.6.0 路线图建议 v3.7.0 实现，需用户需求出现。

**e2e-real-db feature gate**
: 控制真实数据库端到端测试编译与运行的 feature gate，默认关闭（避免无 DB 环境编译失败），CI 中启用，本地有 DB 时可手动启用。
: 备注：启用时要求 DATABASE_URL 环境变量指向真实 DB。

---

# 3. 角色与边界

## 3.1 核心角色

- **ORM 库维护者**：执行 v3.7.0 六项优化任务的开发、验证、测试操作者，是新增能力的主要使用者与验收人。
- **CI 环境**：在持续集成中启用 `e2e-real-db` feature gate 运行真实 DB e2e 测试的自动化环境，需预置 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 实例。
- **下游项目开发者（sz-pay）**：关注 API 兼容性与版本平滑升级的下游使用者，v3.7.0 不得破坏其既有代码。

## 3.2 外部系统

- **MySQL 9.6**：真实 DB e2e 测试的目标数据库之一（`mysql://root:test123@127.0.0.1:3306/sz_orm_test`）。
- **PostgreSQL 18**：真实 DB e2e 测试的目标数据库之一（`postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`）。
- **Oracle 23ai Free**：真实 DB e2e 测试的目标数据库之一（`127.0.0.1:1521/freepdb1`）。
- **SQLite**：真实 DB e2e 测试的目标数据库之一（文件型，无需独立服务）。
- **MSSQL**：真实 DB e2e 测试的目标数据库之一（如本机可用）。
- **Snowflake 云数仓**：方向 5 真实云数据库验证目标（如可用）。
- **Redshift 云数仓**：方向 5 真实云数据库验证目标（如可用）。
- **crates.io**：v3.7.0 发布目标（如发布）。
- **sz-pay 项目**：API 兼容性验证的下游基准。

## 3.3 交互上下文

```plantuml
@startuml
title sz-orm v3.7.0 交互上下文

rectangle "sz-orm v3.7.0" as orm {
  rectangle "真实 DB e2e 测试体系" as e2e
  rectangle "对比分析文档更新" as doc
  rectangle "探索能力成熟化" as mat
  rectangle "方言扩展" as dialect
  rectangle "云数仓验证" as cloud
  rectangle "工程规范化" as eng
}

actor "ORM 库维护者" as dev
actor "CI 环境" as ci
actor "sz-pay 开发者" as downstream

database "MySQL 9.6" as mysql
database "PostgreSQL 18" as pg
database "Oracle 23ai" as oracle
database "SQLite" as sqlite
database "MSSQL" as mssql
database "Snowflake" as snowflake
database "Redshift" as redshift

dev --> orm : 开发/验证/测试
ci --> e2e : 启用 e2e-real-db 运行
e2e --> mysql : 真实 DB 测试
e2e --> pg : 真实 DB 测试
e2e --> oracle : 真实 DB 测试
e2e --> sqlite : 真实 DB 测试
e2e --> mssql : 真实 DB 测试
cloud --> snowflake : 真实云验证
cloud --> redshift : 真实云验证
downstream --> orm : API 兼容性验证

@enduml
```

---

# 4. DFX约束

## 4.1 性能

1. **真实 DB e2e 测试执行时间**：单方言完整 e2e 套件执行时间不超过 60 秒（含建表/数据准备/测试/清理），全方言（5 种）不超过 300 秒。
2. **对比分析文档更新不引入性能回归**：文档更新为纯文档工作，不改变任何代码路径，性能基线不变。
3. **探索能力成熟化不引入运行时开销**：typed relation / 自定义诊断 / proc-macro SQL 验证均为编译期工作，运行时零开销。

## 4.2 可靠性

1. **真实 DB e2e 测试幂等性**：每次运行须清理上一次残留数据（DROP TABLE IF EXISTS + 重建），保证重复运行结果一致。
2. **真实 DB e2e 测试隔离性**：每个测试用独立表名或独立事务回滚，测试间互不干扰。
3. **v3.6.0 测试基线不回退**：v3.7.0 不得使 v3.6.0 已验收测试基线回退，仅增不减。

## 4.3 安全性

1. **真实 DB e2e 测试不使用生产数据库**：必须使用 `sz_orm_test` 测试库，禁止连生产库。
2. **测试数据清理**：测试完成后须清理所有测试数据与表，不残留。
3. **DATABASE_URL 不硬编码**：真实 DB 连接串通过环境变量传入，不硬编码到源码。

## 4.4 可维护性

1. **真实 DB e2e 测试可复用基础设施**：复用 `tests/common/` 既有 adapter（sqlx_pg_adapter / sqlx_mysql_adapter / rusqlite_adapter / schema_builder / equivalence），不重复造轮子。
2. **对比分析文档可追溯**：每条评分变更附 v3.6.0 代码证据（file:line），可验证。
3. **探索能力成熟化附迁移指南**：从探索 API 到正式 API 的迁移路径文档化。

## 4.5 兼容性

1. **API 完全向后兼容**：既有公开 API 签名不变，新能力通过 feature gate 隔离。
2. **crates.io SemVer 兼容**：3.6.0 → 3.7.0 为 minor 版本升级，保证向后兼容，sz-pay 可平滑升级。
3. **feature gate 默认行为不变**：默认 feature 集合行为与 v3.6.0 一致。

---

# 5. 核心能力

## 5.1 真实数据库端到端测试体系

### 5.1.1 业务规则

1. **真实 DB e2e 测试须连真实数据库**：测试须通过 DATABASE_URL 连接真实 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 实例执行，禁止用 InMemoryDb 或 mock。
   a. 验收条件：[启用 e2e-real-db feature 运行测试] → [测试连接真实 DB 并执行 SQL，非内存模拟]
2. **真实 DB e2e 测试须覆盖核心业务路径**：覆盖 CRUD（增删改查）+ 事务（提交/回滚/保存点）+ 关联预加载（eager load）+ 分页（offset/limit + keyset）+ 软删除 + 多租户 + 缓存（L1/L2）+ 方言行为一致性（UPSERT/行锁/标识符引用）。
   a. 验收条件：[审查 e2e 测试覆盖矩阵] → [上述 8 类路径均有真实 DB e2e 测试用例]
3. **真实 DB e2e 测试须通过 feature gate 隔离**：通过 `e2e-real-db` feature gate 控制，默认关闭（无 DB 环境不编译失败），CI 中启用。
   a. 验收条件：[默认 cargo test 不启用 e2e-real-db] → [无 DB 环境编译运行通过]；[cargo test --features e2e-real-db] → [连真实 DB 运行]
4. **真实 DB e2e 测试须幂等且隔离**：每次运行清理残留，测试间互不干扰。
   a. 验收条件：[连续两次运行同一 e2e 套件] → [两次结果一致，无残留数据冲突]
5. **真实 DB e2e 测试须复用既有基础设施**：复用 `tests/common/` 既有 adapter，不重复实现连接管理。
   a. 验收条件：[审查 e2e 测试代码] → [使用 common::sqlx_pg_adapter / sqlx_mysql_adapter 等既有设施]
6. **禁止项**：禁止真实 DB e2e 测试连生产数据库。
   a. 验收条件：[审查 DATABASE_URL] → [指向 sz_orm_test 测试库，非生产库]

### 5.1.2 交互流程

```plantuml
@startuml
title 真实 DB e2e 测试执行流程

actor "CI / 开发者" as runner
participant "cargo test\n--features e2e-real-db" as cargo
database "真实 DB\n(MySQL/PG/SQLite/Oracle/MSSQL)" as db
participant "e2e 测试套件" as suite

runner -> cargo : 启用 e2e-real-db 运行
cargo -> db : 通过 DATABASE_URL 连接
db --> cargo : 连接成功
cargo -> suite : 编译并运行 e2e 测试
suite -> db : 建表 + 准备数据
suite -> db : 执行业务路径 (CRUD/事务/预加载/分页/...)
db --> suite : 返回结果
suite -> suite : 断言结果与预期一致
suite -> db : 清理测试数据与表
suite --> cargo : 测试通过/失败
cargo --> runner : 报告结果

@enduml
```

### 5.1.3 异常场景

1. **真实 DB 不可用**
   a. 触发条件：DATABASE_URL 未设置或指向不可达的 DB 实例
   b. 系统行为：e2e-real-db feature 启用时编译通过但运行时跳过并输出跳过原因（而非 panic）
   c. 用户感知：测试输出"skipped: DATABASE_URL not set or DB unreachable"
2. **真实 DB e2e 测试超时**
   a. 触发条件：单方言 e2e 套件执行超过 60 秒
   b. 系统行为：标记测试超时失败，输出耗时与卡点
   c. 用户感知：测试输出"timeout: >60s, stuck at [test_name]"
3. **测试数据清理失败**
   a. 触发条件：DROP TABLE 权限不足或连接断开
   b. 系统行为：输出清理失败警告，不影响测试结果判定
   c. 用户感知：测试输出"warning: cleanup failed for [table], manual cleanup required"

## 5.2 对比分析重评估与文档同步

### 5.2.1 业务规则

1. **对比分析文档须更新到 v3.6.0 基线**：文档头部版本号、日期、代码基线须更新为 v3.6.0，能力清单须反映 v3.6.0 实际能力。
   a. 验收条件：[审查文档头部] → [版本：v3.6.0，代码基线 Cargo.toml = "3.6.0"]
2. **逐维度评分须基于 v3.6.0 代码证据重新评分**：13 个维度（异步/类型安全/连接池/方言/查询API/事务/缓存/N+1/安全/性能/宏/文档生态/生产就绪）须基于 v3.6.0 实际能力重新评分，每条变更附 file:line 证据。
   a. 验收条件：[审查评分矩阵] → [每条评分变更附 v3.6.0 file:line 证据，可验证]
3. **已知不足章节须标注 v3.6.0 已改进项**：§6 各子节须标注 v3.6.0 是否已改进（✅ 已改进 / ⚠️ 部分改进 / ❌ 未改进），更新证据。
   a. 验收条件：[审查 §6 各子节] → [每节标注 v3.6.0 改进状态 + 证据]
4. **结论与建议须更新**：§7 综合结论、定位建议、改进建议须反映 v3.6.0 后的真实状态。
   a. 验收条件：[审查 §7] → [结论反映 v3.6.0 后优势与不足，建议反映 v3.7.0+ 方向]
5. **文档更新不改变任何代码行为**：纯文档工作，不引入代码变更。
   a. 验收条件：[git diff 对比分析文档] → [仅文档变更，无 .rs 代码变更]

### 5.2.2 交互流程

```plantuml
@startuml
title 对比分析文档重评估流程

actor "维护者" as dev
participant "对比分析文档" as doc
participant "v3.6.0 代码库" as code

dev -> code : 逐项提取 v3.6.0 真实能力清单
dev -> doc : 更新头部版本/日期/基线到 v3.6.0
dev -> doc : 逐维度重新评分 (附 file:line 证据)
dev -> doc : 更新 §6 已知不足 (标注 v3.6.0 改进状态)
dev -> doc : 更新 §7 结论与建议
dev -> doc : 更新 §5 独特优势 (v3.6.0 新增项)
doc --> dev : 文档更新完成

@enduml
```

### 5.2.3 异常场景

1. **v3.6.0 代码证据不存在**
   a. 触发条件：文档引用的 file:line 在 v3.6.0 代码库中不存在
   b. 系统行为：标注证据验证失败，须修正引用
   c. 用户感知：审计验证脚本输出"evidence not found: [file:line]"
2. **评分与证据矛盾**
   a. 触发条件：评分声称改进但证据显示未改进
   b. 系统行为：须修正评分或补充证据
   c. 用户感知：审查输出"score-evidence mismatch: [dimension]"

## 5.3 v3.6.0 探索能力成熟化

### 5.3.1 业务规则

1. **typed relation 须转正式 feature**：从探索性质转为正式 feature gate（`typed-relation`），补齐测试覆盖（编译期类型校验 + 运行时关联查询 + 外键类型匹配 + 表归属校验），补齐文档，标注稳定可用。
   a. 验收条件：[cargo test --features typed-relation] → [测试覆盖编译期校验 + 运行时查询，文档完整]
2. **自定义编译期诊断须转正式 feature**：从探索性质转为正式 feature gate（`custom-diagnostic`），补齐测试覆盖（错误位置 + 期望类型 + 实际类型 + 修复建议），补齐文档。
   a. 验收条件：[故意触发类型错误] → [输出自定义诊断信息，含错误位置/期望/实际/建议]
3. **proc-macro SQL 验证须转正式 feature**：从探索性质转为正式 feature gate（`sql-verify-proc`），连真 DB EXPLAIN 覆盖所有 QueryBuilder 路径（SELECT/INSERT/UPDATE/DELETE/JOIN/子查询/CTE/窗口），补齐文档。
   a. 验收条件：[cargo build --features sql-verify-proc + DATABASE_URL 设置] → [编译期连真 DB EXPLAIN 验证所有路径]
4. **成熟化不破坏 v3.6.0 既有 API**：探索期 API 如已发布须保持向后兼容，仅补齐测试与文档。
   a. 验收条件：[v3.6.0 既有 typed-relation/sql-verify 调用] → [v3.7.0 编译运行通过，无 breaking change]
5. **禁止项**：禁止成熟化引入运行时开销（均为编译期工作）。
   a. 验收条件：[性能基准对比] → [启用/禁用 feature 运行时性能无差异]

### 5.3.2 交互流程

```plantuml
@startuml
title 探索能力成熟化流程

actor "维护者" as dev
participant "typed relation" as trel
participant "自定义诊断" as diag
participant "proc-macro SQL 验证" as pmacro
database "真实 DB (EXPLAIN)" as db

dev -> trel : 补齐测试 + 文档 + 标注稳定
dev -> diag : 补齐测试 + 文档 + 标注稳定
dev -> pmacro : 连真 DB EXPLAIN 覆盖所有路径
pmacro -> db : EXPLAIN 验证 SQL
db --> pmacro : 验证结果
dev -> dev : 确认无运行时开销

@enduml
```

### 5.3.3 异常场景

1. **proc-macro SQL 验证连真 DB 失败**
   a. 触发条件：DATABASE_URL 未设置或 DB 不可达
   b. 系统行为：回退到仅语法校验（不连真 DB），输出降级警告
   c. 用户感知：编译输出"warning: sql-verify-proc degraded to syntax-only (DATABASE_URL not set)"
2. **typed relation 编译期校验误报**
   a. 触发条件：合法关联查询被误判为类型不匹配
   b. 系统行为：须修正校验逻辑，补充回归测试
   c. 用户感知：编译错误"false positive: [relation]"

## 5.4 方言扩展延续

### 5.4.1 业务规则

1. **Informix 方言按需实现**：当 Rust Informix 驱动成熟且用户需求出现时实现，通过 `dialect-informix` feature gate 隔离，支持 SERIAL/ROW 类型、PUT 语句等特性。
   a. 验收条件：[cargo test --features dialect-informix] → [Informix 方言 SQL 生成测试通过]
2. **SAP HANA 方言按需实现**：当 Rust SAP HANA 驱动成熟且企业需求出现时实现，通过 `dialect-saphana` feature gate 隔离。
   a. 验收条件：[cargo test --features dialect-saphana] → [SAP HANA 方言 SQL 生成测试通过]
3. **Firebird 方言按需实现**：当用户需求出现时实现，通过 `dialect-firebird` feature gate 隔离。
   a. 验收条件：[cargo test --features dialect-firebird] → [Firebird 方言 SQL 生成测试通过]
4. **新方言须保持既有方言行为不变**：新方言通过 feature gate 隔离，既有 20 种方言测试不回退。
   a. 验收条件：[既有 20 种方言测试] → [全部通过，无回退]
5. **新方言须更新 DbType 枚举与方言扩展路线图**：DbType 枚举新增变体，路线图标注 v3.7.0 已实现。
   a. 验收条件：[审查 db_type.rs] → [新增变体]；[审查路线图] → [v3.7.0 标注已实现]
6. **禁止项**：禁止新方言破坏既有方言的 SQL 生成。
   a. 验收条件：[既有方言 SQL 生成快照] → [与 v3.6.0 一致]

### 5.4.2 交互流程

```plantuml
@startuml
title 方言扩展流程

actor "维护者" as dev
participant "Dialect trait" as trait
participant "DbType 枚举" as enum
participant "路线图" as roadmap

dev -> trait : 实现 Informix/SAP HANA/Firebird Dialect
dev -> enum : 新增 DbType 变体
dev -> trait : 补齐 SQL 生成测试
dev -> roadmap : 标注 v3.7.0 已实现

@enduml
```

### 5.4.3 异常场景

1. **Rust 驱动不成熟**
   a. 触发条件：目标方言无可用 Rust 驱动或驱动不稳定
   b. 系统行为：仅实现 SQL 生成方言（不连真实 DB），标注"SQL 生成 only，无真实 DB 验证"
   c. 用户感知：文档标注"dialect-informix: SQL generation only, no real DB driver"

## 5.5 Snowflake/Redshift 真实云数据库验证

### 5.5.1 业务规则

1. **Snowflake 真实云数据库验证**：连真实 Snowflake 实例验证 SQL 行为一致性（UPSERT/TIME TRAVEL/VARIANT 类型等），或评估无可用云实例时输出验证缺口报告 + 替代方案（如本地 Snowflake 模拟或仅 SQL 生成 + 人工审核）。
   a. 验收条件：[有 Snowflake 实例] → [连真实云验证行为一致性]；[无实例] → [输出缺口报告 + 替代方案]
2. **Redshift 真实云数据库验证**：连真实 Redshift 实例验证 SQL 行为一致性（COPY/UNLOAD/PG 兼容性等），或评估无可用云实例时输出验证缺口报告。
   a. 验收条件：[有 Redshift 实例] → [连真实云验证]；[无实例] → [输出缺口报告]
3. **验证结果须文档化**：验证报告附测试用例、结果、与 SQL 生成测试的差异（如有）。
   a. 验收条件：[审查验证报告] → [含测试用例/结果/差异分析]

### 5.5.2 交互流程

```plantuml
@startuml
title 云数仓验证流程

actor "维护者" as dev
database "Snowflake" as sf
database "Redshift" as rs
participant "验证报告" as report

dev -> sf : 连真实 Snowflake 验证 (或评估不可用)
dev -> rs : 连真实 Redshift 验证 (或评估不可用)
dev -> report : 输出验证报告 (用例/结果/差异)

@enduml
```

### 5.5.3 异常场景

1. **云数仓实例不可用**
   a. 触发条件：无 Snowflake/Redshift 云账号或实例不可达
   b. 系统行为：输出验证缺口报告，标注"SQL 生成测试通过，真实云验证待补"，提供替代方案
   c. 用户感知：验证报告标注"cloud verification pending: no accessible instance"

## 5.6 工程规范化

### 5.6.1 业务规则

1. **v3.6.0 未提交工作须入库**：213 个未提交文件须按规范提交 git，提交信息遵循既有风格（feat/docs/refactor 前缀）。
   a. 验收条件：[git status] → [无未提交 v3.6.0 工作残留]；[git log] → [v3.6.0 提交可见]
2. **Prisma 方言评估结论须落地**：v3.6.0 Prisma 方言兼容评估结论须输出正式文档（可行性/推荐方案/如可行则实现计划）。
   a. 验收条件：[审查 Prisma 评估文档] → [含可行性结论/推荐方案/实现计划或不可行理由]
3. **门禁须通过**：v3.7.0 交付前 14 道门禁须全部通过（fmt/check/clippy/test/doc/audit/integration/占位检查/SQL注入/feature全组合/上游未改/文档一致性/审计证据/文档同步）。
   a. 验收条件：[运行 14 道门禁] → [全部通过]
4. **禁止项**：禁止提交含 todo!/unimplemented!/unreachable! 的占位实现。
   a. 验收条件：[grep todo!/unimplemented!/unreachable!] → [无匹配]

### 5.6.2 交互流程

```plantuml
@startuml
title 工程规范化流程

actor "维护者" as dev
participant "git" as git
participant "门禁脚本" as gate

dev -> git : 提交 v3.6.0 未入库工作
dev -> git : 输出 Prisma 评估结论文档
dev -> gate : 运行 14 道门禁
gate --> dev : 全部通过 / 失败列表

@enduml
```

### 5.6.3 异常场景

1. **门禁失败**
   a. 触发条件：14 道门禁任一未通过
   b. 系统行为：输出失败门禁列表与修复建议，阻断交付
   c. 用户感知：门禁报告"FAILED: [gate_name], fix: [suggestion]"

---

# 6. 数据约束

## 6.1 真实 DB e2e 测试配置

1. **DATABASE_URL**：真实 DB 连接串，通过环境变量传入，格式 `mysql://user:pass@host:port/db` / `postgres://...` / `sqlite://path` 等，禁止硬编码。
2. **e2e-real-db feature**：布尔 feature gate，默认 false，CI 中 true，控制真实 DB e2e 测试编译与运行。
3. **测试库命名**：必须为 `sz_orm_test`，禁止连生产库。
4. **测试超时**：单方言 60 秒，全方言 300 秒，超时标记失败。

## 6.2 对比分析文档元数据

1. **版本号**：文档头部 `版本：vX.Y.Z`，须与 Cargo.toml workspace.package.version 一致。
2. **日期**：文档头部 `日期：YYYY-MM-DD`，须为文档更新实际日期。
3. **代码基线**：文档头部 `代码基线：Cargo.toml:6 workspace.package.version = "X.Y.Z"`，须与实际一致。
4. **证据格式**：每条能力结论附 `[file.rs:line](file:///path#Lline)` 格式证据，可点击验证。

## 6.3 探索能力成熟化标注

1. **feature gate 命名**：`typed-relation` / `custom-diagnostic` / `sql-verify-proc`，语义清晰。
2. **稳定性标注**：Cargo.toml feature 注释标注"stable"（成熟化后）或"experimental"（探索期）。
3. **测试覆盖下限**：每个正式 feature 至少 10 个测试用例覆盖核心路径。

## 6.4 方言扩展

1. **DbType 枚举变体**：新增方言须在 `db_type.rs` DbType 枚举新增变体，保持 `#[non_exhaustive]`。
2. **feature gate 命名**：`dialect-informix` / `dialect-saphana` / `dialect-firebird`，与既有 `dialect-snowflake` 风格一致。
3. **方言数量**：v3.7.0 后方言数量 = 20 + 新增数，须在对比分析文档更新。

---

# 7. 需求追溯矩阵

| 需求编号 | 需求描述 | 验收条件章节 | 优先级 |
|---------|---------|------------|--------|
| REQ-E2E-001 | 真实 DB e2e 测试连真实数据库 | §5.1.1 规则 1 | high |
| REQ-E2E-002 | 覆盖 8 类核心业务路径 | §5.1.1 规则 2 | high |
| REQ-E2E-003 | feature gate 隔离默认关闭 | §5.1.1 规则 3 | high |
| REQ-E2E-004 | 幂等且隔离 | §5.1.1 规则 4 | high |
| REQ-E2E-005 | 复用既有基础设施 | §5.1.1 规则 5 | medium |
| REQ-E2E-006 | 禁止连生产库 | §5.1.1 规则 6 | high |
| REQ-REEVAL-001 | 文档更新到 v3.6.0 基线 | §5.2.1 规则 1 | high |
| REQ-REEVAL-002 | 逐维度重新评分附证据 | §5.2.1 规则 2 | high |
| REQ-REEVAL-003 | 已知不足标注改进状态 | §5.2.1 规则 3 | high |
| REQ-REEVAL-004 | 结论与建议更新 | §5.2.1 规则 4 | medium |
| REQ-REEVAL-005 | 纯文档不改变代码 | §5.2.1 规则 5 | high |
| REQ-MAT-001 | typed relation 转正式 feature | §5.3.1 规则 1 | high |
| REQ-MAT-002 | 自定义诊断转正式 feature | §5.3.1 规则 2 | high |
| REQ-MAT-003 | proc-macro SQL 验证转正式 feature | §5.3.1 规则 3 | high |
| REQ-MAT-004 | 不破坏 v3.6.0 既有 API | §5.3.1 规则 4 | high |
| REQ-MAT-005 | 无运行时开销 | §5.3.1 规则 5 | medium |
| REQ-DIALECT-001 | Informix 方言按需实现 | §5.4.1 规则 1 | medium |
| REQ-DIALECT-002 | SAP HANA 方言按需实现 | §5.4.1 规则 2 | medium |
| REQ-DIALECT-003 | Firebird 方言按需实现 | §5.4.1 规则 3 | medium |
| REQ-DIALECT-004 | 既有方言不回退 | §5.4.1 规则 4 | high |
| REQ-DIALECT-005 | 更新 DbType 与路线图 | §5.4.1 规则 5 | medium |
| REQ-CLOUD-001 | Snowflake 真实云验证 | §5.5.1 规则 1 | medium |
| REQ-CLOUD-002 | Redshift 真实云验证 | §5.5.1 规则 2 | medium |
| REQ-CLOUD-003 | 验证结果文档化 | §5.5.1 规则 3 | medium |
| REQ-ENG-001 | v3.6.0 未提交工作入库 | §5.6.1 规则 1 | high |
| REQ-ENG-002 | Prisma 评估结论落地 | §5.6.1 规则 2 | medium |
| REQ-ENG-003 | 14 道门禁全通过 | §5.6.1 规则 3 | high |
| REQ-ENG-004 | 禁止占位实现 | §5.6.1 规则 4 | high |

---

# 8. 优先级与风险

## 8.1 优先级排序

| 方向 | 优先级 | 收益 | 风险 | 理由 |
|------|--------|------|------|------|
| 1 真实 DB e2e 测试体系 | 最高 | 高 | 中 | 补 v3.6.0 最大缺口，用户最关心，提升测试可信度 |
| 2 对比分析重评估 | 高 | 高 | 低 | 用户明确要求，文档滞后一版本，纯文档工作 |
| 3 探索能力成熟化 | 高 | 高 | 低 | v3.6.0 探索性质需转正式，补齐测试文档 |
| 4 方言扩展延续 | 中 | 中 | 中 | 补企业数据库按需，需 Rust 驱动成熟 |
| 5 云数仓真实验证 | 中 | 中 | 中 | 补 v3.6.0 方言测试缺口，需云实例可用 |
| 6 工程规范化 | 低但必须 | 中 | 低 | git 入库 + Prisma 评估落地，必须做 |

## 8.2 风险缓解

1. **真实 DB e2e 测试环境依赖**：通过 feature gate 默认关闭缓解，CI 预置 DB 实例，本地无 DB 可跳过。
2. **云数仓实例不可用**：输出验证缺口报告 + 替代方案，不阻断交付。
3. **Rust 方言驱动不成熟**：仅实现 SQL 生成方言，标注"SQL generation only"。
4. **探索能力成熟化 breaking change**：保持 v3.6.0 既有 API 向后兼容，仅补齐测试与文档。

---

# 9. 验收标准总览

## 9.1 方向 1：真实数据库端到端测试体系

- [ ] 连真实 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 的 e2e 测试套件实现
- [ ] 覆盖 8 类核心业务路径（CRUD/事务/预加载/分页/软删除/多租户/缓存/方言行为）
- [ ] `e2e-real-db` feature gate 隔离，默认关闭，CI 启用
- [ ] 无 DB 环境默认 cargo test 编译运行通过
- [ ] `cargo test --features e2e-real-db` 连真实 DB 运行通过
- [ ] 测试幂等且隔离（连续两次运行结果一致）
- [ ] 复用 tests/common/ 既有 adapter
- [ ] 单方言执行 ≤60s，全方言 ≤300s
- [ ] DATABASE_URL 不硬编码，测试库为 sz_orm_test

## 9.2 方向 2：对比分析重评估与文档同步

- [ ] 文档头部更新到 v3.6.0 基线（版本/日期/代码基线）
- [ ] 13 个维度重新评分，每条变更附 file:line 证据
- [ ] §6 已知不足各子节标注 v3.6.0 改进状态（✅/⚠️/❌）
- [ ] §7 结论与建议更新
- [ ] §5 独特优势更新（v3.6.0 新增项）
- [ ] 审计验证脚本通过（所有 file:line 证据真实存在）
- [ ] 纯文档变更，无 .rs 代码变更

## 9.3 方向 3：v3.6.0 探索能力成熟化

- [ ] typed relation 转正式 feature（`typed-relation`），测试覆盖 + 文档完整
- [ ] 自定义编译期诊断转正式 feature（`custom-diagnostic`），测试覆盖 + 文档完整
- [ ] proc-macro SQL 验证转正式 feature（`sql-verify-proc`），连真 DB EXPLAIN 覆盖所有路径
- [ ] v3.6.0 既有 API 向后兼容（无 breaking change）
- [ ] 无运行时开销（编译期工作）
- [ ] 每个 feature 至少 10 个测试用例

## 9.4 方向 4：方言扩展延续

- [ ] Informix 方言实现（或标注 Rust 驱动不成熟暂缓）
- [ ] SAP HANA 方言实现（或标注暂缓）
- [ ] Firebird 方言实现（或标注暂缓）
- [ ] 既有 20 种方言测试不回退
- [ ] DbType 枚举新增变体 + 路线图更新
- [ ] 对比分析文档方言数量更新

## 9.5 方向 5：Snowflake/Redshift 真实云数据库验证

- [ ] Snowflake 真实云验证（或输出缺口报告 + 替代方案）
- [ ] Redshift 真实云验证（或输出缺口报告）
- [ ] 验证报告文档化（用例/结果/差异分析）

## 9.6 方向 6：工程规范化

- [ ] v3.6.0 未提交工作（213 文件）入库
- [ ] Prisma 方言评估结论落地文档
- [ ] 14 道门禁全部通过
- [ ] 无 todo!/unimplemented!/unreachable! 占位实现

---

# 10. 版本号与里程碑

## 10.1 版本号

- **v3.7.0**：本文档六项优化，minor 版本升级（向后兼容），通过 feature gate 隔离新能力。

## 10.2 里程碑规划

| 里程碑 | 内容 | 优先级 | 风险 | 依赖 |
|--------|------|--------|------|------|
| M1 | 方向 1 真实 DB e2e 测试体系 | 最高 | 中 | e2e-real-db feature gate + 本机 DB 实例 |
| M2 | 方向 2 对比分析重评估与文档同步 | 高 | 低 | 纯文档工作 |
| M3 | 方向 3 v3.6.0 探索能力成熟化 | 高 | 低 | typed-relation/custom-diagnostic/sql-verify-proc feature gate |
| M4 | 方向 4 方言扩展延续 | 中 | 中 | Rust 驱动评估 |
| M5 | 方向 5 Snowflake/Redshift 真实云验证 | 中 | 中 | 云数仓实例可用性 |
| M6 | 方向 6 工程规范化 | 低但必须 | 低 | git + 门禁脚本 |

## 10.3 Breaking Change 风险评估

| 方向 | Breaking Change 风险 | 缓解措施 |
|------|---------------------|---------|
| 1 真实 DB e2e 测试 | 无（feature gate 隔离，默认关闭） | e2e-real-db feature gate |
| 2 对比分析重评估 | 无（纯文档） | — |
| 3 探索能力成熟化 | 无（保持 v3.6.0 API 向后兼容） | feature gate + 仅补齐测试文档 |
| 4 方言扩展 | 无（feature gate 隔离，既有 20 种不变） | dialect-informix/saphana/firebird feature gate |
| 5 云数仓验证 | 无（仅验证不改变代码） | — |
| 6 工程规范化 | 无（git 提交 + 门禁） | — |

## 10.4 与 v3.6.0 的关系

v3.7.0 是 v3.6.0 的增量改进，非 Breaking Change：

1. **增量改进**：所有新能力通过 feature gate 隔离，默认 feature 行为与 v3.6.0 一致。
2. **测试基线不回退**：v3.6.0 已验收测试基线仅增不减。
3. **API 完全向后兼容**：既有公开 API 签名不变。
4. **crates.io SemVer 兼容**：3.6.0 → 3.7.0 为 minor 版本升级，保证向后兼容，sz-pay 可平滑升级。
5. **对比分析不足改进**：§6.3 社区规模 / §6.4 生产案例非代码工作不在本版本范畴；v3.6.0 端到端测试缺口（方向 1）/ 文档滞后（方向 2）/ 探索能力成熟度（方向 3）/ 方言数量（方向 4）/ 云数仓验证（方向 5）在 v3.7.0 改进。

## 10.5 v3.6.0 方言扩展路线图延续

v3.6.0 `spec.md` §10.5 制定的方言扩展路线图在 v3.7.0 延续：

| 版本 | 新增方言 | 触发条件 | feature gate | v3.7.0 状态 |
|------|---------|---------|-------------|------------|
| v3.5.0 | CockroachDB + YugabyteDB | PG 兼容分布式数据库需求 | dialect-cockroachdb / dialect-yugabytedb | ✅ 已完成 |
| v3.6.0 | Snowflake + Redshift | 云数仓需求 | dialect-snowflake / dialect-redshift | ✅ 已完成 |
| v3.7.0 | Informix | Rust 驱动成熟 + 用户需求 | dialect-informix | 🔄 本版本按需实施 |
| v3.7.0 | SAP HANA | Rust 驱动成熟 + 企业需求 | dialect-saphana | 🔄 本版本按需实施 |
| v3.7.0 | Firebird | 用户需求出现 | dialect-firebird | 🔄 本版本按需实施 |
| v3.8.0（建议） | Prisma 方言 | v3.6.0 评估可行 + 用户需求 | dialect-prisma | 待定 |

---

> 本文档为 sz-orm v3.7.0 需求规格说明书，基于 v3.6.0 已验收基线 + 对比分析文档滞后（停留 v3.5.0）+ v3.6.0 端到端测试缺口（96 e2e 用 InMemoryDb，63 真实 DB 全 ignore）+ v3.6.0 探索能力成熟度不足 + v3.6.0 方言扩展路线图 v3.7.0 候选 + v3.6.0 Snowflake/Redshift 无真实云验证 + 213 文件未提交 git 生成。所有改进通过 feature gate 隔离，保证既有 API 完全向后兼容与测试基线不回退。
> 生成日期：2026-08-10
> 基线版本：v3.6.0
> 目标版本：v3.7.0
> 需求总数：28 条（REQ-E2E-001~006 + REQ-REEVAL-001~005 + REQ-MAT-001~005 + REQ-DIALECT-001~005 + REQ-CLOUD-001~003 + REQ-ENG-001~004）
> 优先级分布：high 15 条 / medium 10 条 / low 3 条
> 来源分布：v3.6.0 端到端测试缺口 6 条 / 对比分析文档滞后 5 条 / v3.6.0 探索能力成熟度 5 条 / 方言扩展路线图 5 条 / 云数仓验证缺口 3 条 / 工程规范化 4 条