# sz-orm v2.2.0 需求规格说明书

> **版本**：v2.2.0
> **基线**：v2.1.0（43 包 @ 2.1.0 已发布 crates.io，4,993 测试通过，sz-pay 5,139 试点验证无回归）
> **生成日期**：2026-08-06
> **文档目的**：定义 v2.2.0 多后端 ORM 增强（需求 A）与五项短期目标（需求 B）的需求规格，聚焦服务 sz-rust 框架多后端集成与补齐 ORM 高级能力
> **依据**：
> - `docs/assessment/2026-08-06-v2-progress-and-roadmap.md`（v2.1.0 进展 + v2.2.0+ 路线图 §4.1）
> - `docs/assessment/2026-08-04-deep-comparison.md`（竞品深度对比）
> - `docs/spec/v2.1.0/spec.md`（v2.1.0 需求规格基线）
> - 源码现状调研：`packages/sz-orm-sqlx/src/any_driver.rs`、`packages/sz-orm-core/src/dialect.rs`、`packages/sz-orm-core/src/pool.rs`、`packages/sz-orm-oracle/src/lib.rs`、`packages/sz-orm-mssql/src/lib.rs`

---

## 0. 现状基线与需求校准（基于源码调研）

> **重要**：本章节记录需求生成前的源码现状调研结果，用于校准需求范围，避免重复实现或基于错误前提生成需求。

### 0.1 AnyPool 后端覆盖现状

| 后端 | AnyBackend 枚举 | AnyPool::connect 分支 | ConnectionFactory 实现 | 集成状态 |
|------|-----------------|----------------------|----------------------|----------|
| MySQL | ✅ `AnyBackend::MySql` | ✅ [`any_driver.rs:101`](../../../packages/sz-orm-sqlx/src/any_driver.rs#L101) | ✅ `SqlxMySqlConnectionFactory` | ✅ 已集成 |
| PostgreSQL | ✅ `AnyBackend::Postgres` | ✅ [`any_driver.rs:105`](../../../packages/sz-orm-sqlx/src/any_driver.rs#L105) | ✅ `SqlxPgConnectionFactory` | ✅ 已集成 |
| SQLite | ✅ `AnyBackend::Sqlite` | ✅ [`any_driver.rs:109`](../../../packages/sz-orm-sqlx/src/any_driver.rs#L109) | ✅ `SqlxSqliteConnectionFactory` | ✅ 已集成 |
| **Oracle** | ❌ 缺失 | ❌ 缺失 | ✅ `OracleConnectionFactory` [`lib.rs:645`](../../../packages/sz-orm-oracle/src/lib.rs#L645) | ❌ **未集成到 AnyPool** |
| **MSSQL** | ❌ 缺失 | ❌ 缺失 | ✅ `MssqlConnectionFactory` [`lib.rs:456`](../../../packages/sz-orm-mssql/src/lib.rs#L456) | ❌ **未集成到 AnyPool** |

**结论**：Oracle/MSSQL 已有独立 `ConnectionFactory` 实现，但未集成到 `AnyPool` 运行时切换机制。需求 A-1（AnyPool 扩展）为真实缺口。

### 0.2 Dialect 实现现状

| 方言 | Dialect 实现 | 行号 | 状态 |
|------|-------------|------|------|
| MySqlDialect | ✅ `impl Dialect for MySqlDialect` | [`dialect.rs:215`](../../../packages/sz-orm-core/src/dialect.rs#L215) | ✅ 已实现 |
| PostgreSqlDialect | ✅ `impl Dialect for PostgreSqlDialect` | [`dialect.rs:438`](../../../packages/sz-orm-core/src/dialect.rs#L438) | ✅ 已实现 |
| SqliteDialect | ✅ `impl Dialect for SqliteDialect` | [`dialect.rs:664`](../../../packages/sz-orm-core/src/dialect.rs#L664) | ✅ 已实现 |
| **OracleDialect** | ✅ `impl Dialect for OracleDialect` | [`dialect.rs:939`](../../../packages/sz-orm-core/src/dialect.rs#L939) | ✅ **已实现** |
| **SqlServerDialect** | ✅ `impl Dialect for SqlServerDialect` | [`dialect.rs:1185`](../../../packages/sz-orm-core/src/dialect.rs#L1185) | ✅ **已实现** |
| ClickHouseDialect | ✅ `impl Dialect for ClickHouseDialect` | [`dialect.rs:1520`](../../../packages/sz-orm-core/src/dialect.rs#L1520) | ✅ 已实现 |
| DuckDBDialect | ✅ `impl Dialect for DuckDBDialect` | [`dialect.rs:1773`](../../../packages/sz-orm-core/src/dialect.rs#L1773) | ✅ 已实现 |
| Db2Dialect | ✅ `impl Dialect for Db2Dialect` | [`dialect.rs:1976`](../../../packages/sz-orm-core/src/dialect.rs#L1976) | ✅ 已实现 |

**结论**：**Oracle/MSSQL 的 Dialect 实现已存在**（共 8 种方言实现）。原始需求 A-2 描述"Dialect 仅 3 种方言、Oracle/MSSQL Dialect 缺失"与代码现状不符。需求 A-2 范围调整为"Dialect 与 AnyPool 运行时切换的集成验证 + 缺失能力补齐"（详见 §5.2）。

### 0.3 AnyPool 与 Pool 类型关系现状

| 类型 | 定义位置 | 内部结构 | 语义 |
|------|---------|---------|------|
| `AnyPool` | [`any_driver.rs:86`](../../../packages/sz-orm-sqlx/src/any_driver.rs#L86) | `backend: AnyBackend` + `factory: Arc<dyn ConnectionFactory>` | 工厂持有，无连接池状态 |
| `Pool` | [`pool.rs:712`](../../../packages/sz-orm-core/src/pool.rs#L712) | `config` + `factory` + `idle: ArrayQueue` + `total_count: AtomicU32` + `Notify` + 监控计数器 | 完整连接池（无锁队列 + 原子计数 + 等待通知） |

**结论**：`AnyPool` 仅持有 `ConnectionFactory`，无连接复用语义；`Pool` 是完整连接池实现。两者类型不兼容，sz-rust 持有 `Arc<sz_orm_core::Pool>` 无法直接使用 `AnyPool`。需求 A-3（统一抽象）为真实缺口。

---

# **1. 组件定位**

## **1.1 核心职责**

本组件负责在 v2.1.0 基础上补齐多后端 ORM 运行时切换能力（AnyPool 扩展 Oracle/MSSQL + AnyPool/Pool 统一抽象）与五项 ORM 高级能力（Eager Loading 多级、Schema Sync 破坏性安全、Partial Models 互补、Stream 背压、cascade_delete），实现 sz-rust 框架多后端透明切换并补齐与 SeaORM 的剩余功能差距。

## **1.2 核心输入**

1. **框架层多后端切换请求**：sz-rust 框架通过统一 AppState 发起的 5 后端（MySQL/PostgreSQL/SQLite/Oracle/MSSQL）运行时切换请求
2. **DSN（数据源名称）**：`oracle://` / `mssql://` 等 scheme 的连接字符串，用于 AnyPool 自动识别后端
3. **应用层 ORM 高级操作请求**：多级 Eager Loading、破坏性 Schema Sync、字段排除查询、背压流式查询、级联删除配置
4. **模型定义元数据**：`#[derive(Entity)]` / `#[derive(Relation)]` / `#[derive(ActiveModel)]` 宏标注的结构体定义（含级联策略标注）
5. **数据库连接句柄**：来自连接池的异步连接（5 种后端）
6. **目标数据库 Schema 状态**：执行 Schema Sync 时从 `INFORMATION_SCHEMA` 读取的现有表结构

## **1.3 核心输出**

1. **统一连接池句柄**：5 后端透明的 `UnifiedPool`（或适配层），供 sz-rust AppState 持有，业务代码无需感知后端类型
2. **多级嵌套领域对象集合**：Eager Loading 多级关联自动组装的 `Vec<(MainEntity, Vec<(L2Entity, Vec<L3Entity>)>)>` 结构（无限级 + 循环检测）
3. **破坏性 DDL 变更脚本**：Schema Sync `destructive_sync()` 生成的 DROP COLUMN / RENAME COLUMN / 数据迁移 DDL 序列
4. **部分字段实体集合**：`select_exclude()` 排除特定字段后查询返回的实体集合
5. **背压流式行迭代器**：`stream_with_backpressure(buffer_size)` 返回的有界流式迭代器
6. **级联删除确认**：cascade_delete 按 RESTRICT/CASCADE/SET_NULL/SET_DEFAULT 策略执行的受影响行数

## **1.4 职责边界**

本组件**不负责**以下事项：

1. **不负责**新增第 6/7 种数据库方言支持（v2.2.0 维持 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 五方言，ClickHouse/DuckDB/Db2 的 Dialect 已存在但不纳入 AnyPool 运行时切换）
2. **不负责**连接池底层重构（沿用 v2.1.0 的无锁 ArrayQueue + AtomicU32 + Notify 实现，仅在统一抽象层适配）
3. **不负责**async-std 运行时支持（ADR-0011 设计决策，仅支持 Tokio）
4. **不负责**Python/JS FFI 绑定扩展（v2.0.0 已交付 0.1.0，v2.2.0 不扩展 API 面）
5. **不负责**Eager Loading 智能策略选择（HasOne 自动 JOIN / HasMany 自动 data loader，属 v2.3.0 中期目标）
6. **不负责**sz-pay 生产案例深化（属 v2.3.0 中期目标）
7. **不负责**图数据库 / WASM / AI 查询优化器（属 v3.0.0+ 长期目标）
8. **不负责**重新实现 Oracle/MSSQL Dialect（已存在于 `dialect.rs:939` / `:1185`，v2.2.0 仅做集成验证与缺口补齐）

---

# **2. 领域术语**

**AnyPool（后端无关连接池）**
: 持有 `Arc<dyn ConnectionFactory>` 的后端无关连接工厂抽象，通过 DSN 自动识别后端类型，运行时透明切换数据库后端。
: 备注：v2.1.0 仅支持 MySQL/PostgreSQL/SQLite 三后端，v2.2.0 扩展至 Oracle/MSSQL。

**UnifiedPool（统一连接池）**
: v2.2.0 新增的统一抽象，使 `AnyPool`（工厂持有）与 `sz_orm_core::Pool`（完整连接池）对业务代码透明，消除 sz-rust AppState 持有 `Arc<Pool>` 时的类型不兼容问题。
: 备注：可能实现为 trait 抽象、newtype 包装或适配层，具体设计由 design.md 决定。

**AnyBackend（后端类型枚举）**
: 标识当前数据库后端类型的枚举，v2.1.0 含 `MySql`/`Postgres`/`Sqlite` 三种，v2.2.0 扩展至含 `Oracle`/`Mssql` 共五种。
: 备注：通过 DSN scheme 自动识别（`oracle://` → Oracle，`mssql://` / `sqlserver://` → Mssql）。

**Dialect（方言）**
: 处理各数据库特有 SQL 语法差异的 trait，v2.1.0 已实现 8 种（MySQL/PG/SQLite/Oracle/MSSQL/ClickHouse/DuckDB/Db2）。
: 备注：v2.2.0 不新增 Dialect 实现，仅验证 Oracle/MSSQL Dialect 与 AnyPool 运行时切换的集成，并补齐发现的缺口能力。

**多级 Eager Loading（Multi-level Eager Loading）**
: 链式预加载超过 2 级的关联关系（如 User → Order → OrderItem → Product），自动执行多表批量查询并组装为深度嵌套结构的能力。
: 备注：v2.1.0 的 `with()` 限 2 级，v2.2.0 扩展至无限级 + 循环检测（防止 User → Order → User 无限递归）。

**破坏性 Schema Sync（Destructive Schema Sync）**
: 生成并执行可能导致数据丢失的 DDL（DROP COLUMN / DROP TABLE / RENAME COLUMN）的 Schema 同步模式，需显式确认 + 数据迁移钩子。
: 备注：v2.1.0 的 `sync()` 禁止自动生成 DROP COLUMN，v2.2.0 新增 `destructive_sync()` 显式启用破坏性变更。

**数据迁移钩子（Data Migration Hook）**
: 在执行破坏性 DDL 前后调用的用户自定义回调，用于在列重命名/删除前备份数据、删除后迁移数据。
: 备注：典型场景 — RENAME COLUMN 前将旧列数据复制到新列；DROP COLUMN 前归档到历史表。

**select_exclude（字段排除查询）**
: Partial Models 的互补 API，查询时排除指定字段（而非 `select_only` 指定包含字段），用于大表排除 BLOB/TEXT 等大字段。
: 备注：`select_only(["id","name"])` 与 `select_exclude(["avatar","blob_data"])` 互补，后者在大表场景更简洁。

**背压流式查询（Backpressure Stream Query）**
: 带有界缓冲区的流式查询，当消费者速度慢于生产者时，生产者阻塞等待而非无限堆积行，控制内存占用。
: 备注：v2.1.0 的 `stream_buffered()` 为无界缓冲，v2.2.0 新增 `stream_with_backpressure(buffer_size)` 有界缓冲。

**cascade_delete 策略（级联删除策略）**
: 删除父实体时子实体的处理策略：RESTRICT（有子实体则禁止删除）/ CASCADE（级联删除子实体）/ SET_NULL（子实体外键置 NULL）/ SET_DEFAULT（子实体外键置默认值）。
: 备注：v2.1.0 的 `nested_delete()` 仅支持 CASCADE 语义，v2.2.0 扩展至 4 种策略可选。

---

# **3. 角色与边界**

## **3.1 核心角色**

- **框架开发者（sz-rust）**：集成 sz-orm 的 Web 框架开发者，是多后端 ORM 增强（需求 A）的主要受益者，关注 AppState 持有统一连接池、5 后端运行时透明切换
- **应用开发者**：使用 sz-orm API 编写业务数据访问代码的 Rust 开发者，是多级 Eager Loading / select_exclude / Stream 背压 / cascade_delete 的主要使用者
- **DBA / 运维工程师**：负责数据库结构管理，是破坏性 Schema Sync 的主要使用者，关注 DDL 变更安全性与数据迁移钩子
- **架构决策者**：基于竞品对比报告做技术选型决策，关注 sz-orm 与 SeaORM 的剩余功能差距是否补齐

## **3.2 外部系统**

- **sz-rust 框架**：sz-orm 的下游集成方，AppState 持有 `Arc<sz_orm_core::Pool>`，需要 5 后端运行时透明切换能力（需求 A 的驱动方）
- **MySQL 9.6**：`mysql://root:test123@127.0.0.1:3306/sz_orm_test`，集成测试
- **PostgreSQL 18**：`postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`，集成测试
- **SQLite**：内存模式，集成测试
- **Oracle 23ai Free**：`127.0.0.1:1521/freepdb1`（用户 sys，密码 test123，Sysdba），集成测试（AnyPool 扩展验证）
- **SQL Server**：集成测试（本机无实例时方言适配 + ignored 测试）
- **SeaORM**：功能对等性参照（多级 Eager Loading / cascade_delete 策略对标）

## **3.3 交互上下文**

```plantuml
@startuml
!theme plain
skinparam componentStyle rectangle

actor "框架开发者(sz-rust)" as FrameDev
actor "应用开发者" as AppDev
actor "DBA/运维" as DBA

component "sz-orm v2.2.0" as SzOrm {
  portin "DSN + 模型定义 + 查询请求" as API
  portout "统一连接池 + 嵌套实体 + DDL + 流式行" as Result
}

database "MySQL" as MySQL
database "PostgreSQL" as PG
database "SQLite" as SQLite
database "Oracle" as Oracle
database "SQL Server" as MSSQL

component "sz-rust 框架" as SzRust
component "SeaORM" as SeaORM

FrameDev --> SzRust : AppState 持有统一连接池
SzRust --> API : 5 后端运行时切换
AppDev --> API : 多级 Eager / select_exclude / Stream 背压 / cascade_delete
DBA --> API : 破坏性 Schema Sync

SzOrm --> MySQL : DDL + DML
SzOrm --> PG : DDL + DML
SzOrm --> SQLite : DDL + DML
SzOrm --> Oracle : DDL + DML
SzOrm --> MSSQL : DDL + DML

SzOrm ..> SeaORM : 功能对等参照

@enduml
```

---

# **4. DFX约束**

## **4.1 性能**

1. **AnyPool 后端切换开销**：AnyPool 从 DSN 识别后端 + 创建连接的耗时不得超过各后端独立 `Pool::connect()` 耗时的 1.05 倍（开销 ≤5%）
2. **UnifiedPool 适配开销**：UnifiedPool 适配层的 `acquire()` / `release()` 耗时不得超过直接使用 `Pool` 的 1.05 倍（开销 ≤5%）
3. **多级 Eager Loading 性能**：N 级关联查询（主表 N0 行，每级平均 M 行）的自动执行 + 组装耗时不得超过手动执行 N 条 SQL + 手动组装耗时的 1.15 倍（开销 ≤15%，级数越多开销比例越高）
4. **select_exclude 性能**：排除 K 个字段的查询内存占用不得超过全字段查询的 `(总字段数 - K + 2) / 总字段数` 比例
5. **背压流式查询内存**：`stream_with_backpressure(buffer_size)` 在结果集 100 万行、buffer_size=1000 时，峰值内存不得超过 buffer_size 行的内存占用 + 10 MB 固定开销
6. **cascade_delete 性能**：CASCADE 策略删除父实体 + N 个子实体的耗时不得超过事务内逐个删除耗时的 1.1 倍

## **4.2 可靠性**

1. **API 向后兼容**：v2.2.0 不得引入 Breaking Change（v2.1.0 API 全部保留），新增能力以扩展方法提供
2. **AnyPool 5 后端等价性**：通过 AnyPool 执行的 CRUD 操作结果必须与通过各后端独立 `Pool` 执行的结果完全一致（按主键排序后逐字段对比）
3. **UnifiedPool 语义保持**：UnifiedPool 的连接复用、超时、断路器、限流、监控等语义必须与 `Pool` 完全一致
4. **多级 Eager Loading 循环安全**：检测到关联循环（如 User → Order → User）时必须终止递归并返回已加载部分，不得栈溢出
5. **破坏性 Schema Sync 原子性**：`destructive_sync()` 在事务内执行，任一 DDL 失败则整体回滚（不支持事务的 DDL 方言除外，需显式标注）
6. **背压流式查询背压正确性**：当消费者速度慢于生产者且缓冲区满时，生产者必须阻塞等待而非丢弃行或无限堆积
7. **cascade_delete RESTRICT 语义**：RESTRICT 策略在存在子实体时必须拒绝删除并返回错误，不得静默删除

## **4.3 安全性**

1. **SQL 注入防护**：所有新增 API（AnyPool DSN 解析 / 多级 Eager Loading / 破坏性 Schema Sync / select_exclude / cascade_delete）必须使用参数化查询或标识符校验，禁止字符串拼接用户输入
2. **DSN scheme 白名单**：AnyPool DSN 解析的 scheme 必须为白名单校验（`mysql`/`postgres`/`sqlite`/`oracle`/`mssql`/`sqlserver`），禁止任意 scheme
3. **破坏性 DDL 审计**：`destructive_sync()` 执行的每条破坏性 DDL（DROP/RENAME）必须经过 sz-orm-audit 记录审计日志，含操作人、时间、DDL 内容、受影响行数
4. **破坏性 DDL 确认**：`destructive_sync()` 必须要求显式确认参数（如 `destructive_sync(Confirm::Yes)`），禁止默认执行破坏性变更
5. **unsafe 零容忍**：新增代码 0 处 `unsafe`，0 处 `todo!` / `unimplemented!` / `unreachable!`

## **4.4 可维护性**

1. **测试覆盖**：每个新增功能必须有单元测试 + 集成测试（至少 MySQL + SQLite 两方言，AnyPool 扩展需覆盖 Oracle），测试覆盖率不得低于 80%
2. **文档覆盖**：每个新增公开 API 必须有 rustdoc 文档注释 + 至少一个 doctest 示例
3. **门禁通过**：v2.2.0 必须通过 11 道门禁（fmt / check / clippy / test / doc / audit / integration / 占位扫描 / SQL 注入扫描 / feature 全组合 / ADR-0001）
4. **clippy 零警告**：`cargo clippy --workspace --all-targets -- -D warnings` 必须通过
5. **sz-pay 回归验证**：v2.2.0 发布前必须在 sz-pay 项目运行 `cargo test --lib`，5,139 测试无回归

## **4.5 兼容性**

1. **Rust 版本**：维持 `rust-version = "1.81"`，不提升 MSRV
2. **数据库方言**：新增功能必须覆盖 MySQL / PostgreSQL / SQLite / Oracle / MSSQL 五方言（MSSQL 无实例时方言适配 + ignored 测试）
3. **crates.io 发布**：v2.2.0 所有包发布到 crates.io，版本号从 2.1.0 升级至 2.2.0
4. **内部依赖**：所有 `version + path` 格式的内部依赖统一至 2.2.0
5. **sz-rust 集成兼容**：UnifiedPool 必须兼容 sz-rust 当前 AppState 持有 `Arc<sz_orm_core::Pool>` 的用法，提供零成本或低成本迁移路径

---

# **5. 核心能力**

## **5.1 AnyPool 扩展支持 Oracle/MSSQL（需求 A-1，高优先级）**

### **5.1.1 用户故事**

**作为** sz-rust 框架开发者，**我希望** 通过 `AnyPool::connect("oracle://...")` 或 `AnyPool::connect("mssql://...")` 一行代码即可创建 Oracle/MSSQL 连接池并纳入运行时切换，**以便** 框架层统一支持 5 种数据库后端，无需为 Oracle/MSSQL 编写独立的连接管理代码。

### **5.1.2 业务规则（EARS 格式）**

1. **AnyBackend 枚举扩展（Ubiquitous）**：The system shall include `Oracle` and `Mssql` variants in the `AnyBackend` enum.
   - 验收条件：[编译后 `AnyBackend` 枚举] → [含 `MySql`/`Postgres`/`Sqlite`/`Oracle`/`Mssql` 共 5 种变体]
2. **DSN scheme 识别扩展（Event-driven）**：When the system receives a DSN starting with `oracle://` or `mssql://` or `sqlserver://`, the system shall identify the backend as `Oracle` or `Mssql` respectively.
   - 验收条件：[`AnyBackend::from_dsn("oracle://sys:test123@127.0.0.1:1521/freepdb1")`] → [返回 `Ok(AnyBackend::Oracle)`]
   - 验收条件：[`AnyBackend::from_dsn("mssql://user:pass@host:1433/db")`] → [返回 `Ok(AnyBackend::Mssql)`]
   - 验收条件：[`AnyBackend::from_dsn("sqlserver://user:pass@host:1433/db")`] → [返回 `Ok(AnyBackend::Mssql)`]
3. **AnyPool 连接 Oracle（Event-driven）**：When `AnyPool::connect(dsn)` is called with an Oracle DSN, the system shall create an `OracleConnectionFactory` and return an `AnyPool` with `backend == Oracle`.
   - 验收条件：[`AnyPool::connect("oracle://sys:test123@127.0.0.1:1521/freepdb1").await`] → [返回 `Ok(AnyPool)`，`pool.backend() == AnyBackend::Oracle`，`pool.create().await` 返回可执行 SQL 的连接]
4. **AnyPool 连接 MSSQL（Event-driven）**：When `AnyPool::connect(dsn)` is called with an MSSQL DSN, the system shall create an `MssqlConnectionFactory` and return an `AnyPool` with `backend == Mssql`.
   - 验收条件：[`AnyPool::connect("mssql://user:pass@host:1433/db").await`] → [返回 `Ok(AnyPool)`，`pool.backend() == AnyBackend::Mssql`]
5. **5 后端 CRUD 等价性（State-driven）**：When the same CRUD operation is executed via `AnyPool` and via the backend-specific `Pool`, the results shall be identical.
   - 验收条件：[对同一表结构执行 INSERT/SELECT/UPDATE/DELETE，分别通过 `AnyPool`（5 后端）和各后端独立 `Pool` 执行] → [结果按主键排序后完全一致]
6. **未知 scheme 拒绝（Event-driven）**：When `AnyBackend::from_dsn(dsn)` receives an unknown scheme, the system shall return `Err(DbError::ConnectionRefused)` with a message listing all 5 supported schemes.
   - 验收条件：[`AnyBackend::from_dsn("redis://localhost")`] → [返回 `Err`，错误信息含 `mysql/postgres/sqlite/oracle/mssql`]
7. **禁止项**：禁止 AnyPool 在运行时动态加载未编译的后端 feature（如未启用 `oracle` feature 时调用 `oracle://` DSN）
   - 验收条件：[未启用 `oracle` feature 时调用 `AnyPool::connect("oracle://...")`] → [返回 `Err`，错误信息提示需启用 `oracle` feature]

### **5.1.3 交互流程**

```plantuml
@startuml
!theme plain
actor "框架开发者" as Dev
participant "AnyPool" as AnyPool
participant "OracleConnectionFactory" as OraFact
participant "MssqlConnectionFactory" as MssqlFact
database "Oracle" as Oracle
database "SQL Server" as MSSQL

Dev -> AnyPool : connect("oracle://sys:pwd@host:1521/svc")
AnyPool -> AnyPool : from_dsn() → AnyBackend::Oracle
AnyPool -> OraFact : OracleConnectionFactory::new(handle)
OraFact -> Oracle : 建立连接池
Oracle --> OraFact : 连接池就绪
AnyPool --> Dev : Ok(AnyPool{backend: Oracle})

Dev -> AnyPool : connect("mssql://user:pwd@host:1433/db")
AnyPool -> AnyPool : from_dsn() → AnyBackend::Mssql
AnyPool -> MssqlFact : MssqlConnectionFactory::new(handle)
MssqlFact -> MSSQL : 建立连接池
MSSQL --> MssqlFact : 连接池就绪
AnyPool --> Dev : Ok(AnyPool{backend: Mssql})
@enduml
```

### **5.1.4 异常场景**

1. **Oracle/MSSQL feature 未启用**
   - 触发条件：调用 `AnyPool::connect("oracle://...")` 但未启用 `oracle` cargo feature
   - 系统行为：返回 `Err(DbError::ConnectionRefused)`，错误信息提示需启用对应 feature
   - 用户感知：编译期或运行时明确错误，错误信息含 `请启用 oracle feature` 提示
2. **Oracle/MSSQL 连接失败**
   - 触发条件：DSN 正确但数据库不可达（网络问题 / 认证失败 / 服务未启动）
   - 系统行为：返回 `Err(DbError::ConnectionError)`，含底层驱动错误
   - 用户感知：返回错误，错误信息含底层 Oracle/MSSQL 驱动的原始错误
3. **DSN 格式错误**
   - 触发条件：DSN 缺少必要部分（如 `oracle://` 无主机无服务名）
   - 系统行为：返回 `Err(DbError::ConnectionRefused)`，含 DSN 解析错误
   - 用户感知：返回错误，错误信息含 DSN 格式问题定位

---

## **5.2 Dialect 与 AnyPool 运行时切换集成验证与缺口补齐（需求 A-2，高优先级）**

> **现状说明**：源码调研确认 `OracleDialect`（[`dialect.rs:939`](../../../packages/sz-orm-core/src/dialect.rs#L939)）和 `SqlServerDialect`（[`dialect.rs:1185`](../../../packages/sz-orm-core/src/dialect.rs#L1185)）已实现。本模块不重新实现这两个 Dialect，而是：(1) 验证它们与 AnyPool 运行时切换的集成；(2) 补齐调研中发现的缺口能力（如 AnyPool 切换后端时自动选择对应 Dialect 的机制）。

### **5.2.1 用户故事**

**作为** sz-rust 框架开发者，**我希望** AnyPool 切换后端时自动选择对应的 Dialect（Oracle 后端 → OracleDialect，MSSQL 后端 → SqlServerDialect），SQL 生成（DDL/分页/upsert/行锁等）自动适配，**以便** 业务代码无需手动指定 Dialect，5 后端 SQL 生成透明切换。

### **5.2.2 业务规则（EARS 格式）**

1. **AnyPool 自动选择 Dialect（State-driven）**：When an `AnyPool` has `backend == Oracle`, the system shall use `OracleDialect` for all SQL generation; when `backend == Mssql`, the system shall use `SqlServerDialect`.
   - 验收条件：[`AnyPool::connect("oracle://...").await` 后执行分页查询] → [生成的分页 SQL 使用 OracleDialect 的 `ROWNUM` / `FETCH FIRST N ROWS ONLY` 语法，而非 MySQL 的 `LIMIT`]
   - 验收条件：[`AnyPool::connect("mssql://...").await` 后执行分页查询] → [生成的分页 SQL 使用 SqlServerDialect 的 `OFFSET ... FETCH NEXT ... ROWS ONLY` 语法]
2. **Dialect 能力完整性验证（Ubiquitous）**：The system shall ensure `OracleDialect` and `SqlServerDialect` implement all required `Dialect` trait methods with non-placeholder behavior.
   - 验收条件：[对 `OracleDialect` 和 `SqlServerDialect` 调用 `quote` / `build_pagination` / `build_create_table` / `build_alter_table` / `auto_increment_keyword` / `supports_returning` / `last_insert_id_sql` 等全部 trait 方法] → [返回有效 SQL，无 `todo!` / `unimplemented!`]
3. **Dialect 与 AnyConnection 协同（State-driven）**：When an `AnyConnection` with `backend == Oracle` executes SQL generated by `OracleDialect`, the database shall accept and execute the SQL without syntax errors.
   - 验收条件：[通过 `AnyPool::connect("oracle://...")` 创建连接，执行 `OracleDialect` 生成的 CREATE TABLE + INSERT + SELECT 分页 SQL] → [Oracle 数据库执行成功，返回正确结果集]
4. **5 方言 SQL 生成等价性（Optional feature）**：Where the same logical operation (e.g., pagination with page=2, page_size=10) is expressed via different dialects, the generated SQL shall be semantically equivalent (produce the same result set on the same data).
   - 验收条件：[同一表结构与数据，分别用 5 种 Dialect 生成第 2 页（每页 10 行）的分页 SQL 并执行] → [5 种数据库返回相同的 10 行结果]
5. **禁止项**：禁止 AnyPool 运行时切换后端后使用错误的 Dialect（如 Oracle 后端使用 MySqlDialect 生成 `LIMIT` 语法）
   - 验收条件：[`AnyPool::connect("oracle://...")` 后执行分页查询] → [生成的 SQL 不得包含 `LIMIT`，必须使用 Oracle 方言语法]

### **5.2.3 交互流程**

```plantuml
@startuml
!theme plain
actor "框架开发者" as Dev
participant "AnyPool" as AnyPool
participant "OracleDialect" as OraDialect
participant "SqlServerDialect" as MssqlDialect
database "Oracle" as Oracle
database "SQL Server" as MSSQL

Dev -> AnyPool : connect("oracle://...") → backend=Oracle
Dev -> AnyPool : 分页查询 page=2, size=10
AnyPool -> AnyPool : backend=Oracle → 选择 OracleDialect
AnyPool -> OraDialect : build_pagination(sql, 2, 10)
OraDialect --> AnyPool : SQL with OFFSET/FETCH NEXT
AnyPool -> Oracle : 执行分页 SQL
Oracle --> AnyPool : 第 2 页结果集
AnyPool --> Dev : Vec<T>

Dev -> AnyPool : connect("mssql://...") → backend=Mssql
Dev -> AnyPool : 分页查询 page=2, size=10
AnyPool -> AnyPool : backend=Mssql → 选择 SqlServerDialect
AnyPool -> MssqlDialect : build_pagination(sql, 2, 10)
MssqlDialect --> AnyPool : SQL with OFFSET/FETCH NEXT
AnyPool -> MSSQL : 执行分页 SQL
MSSQL --> AnyPool : 第 2 页结果集
AnyPool --> Dev : Vec<T>
@enduml
```

### **5.2.4 异常场景**

1. **Dialect 方法未实现**
   - 触发条件：调用 `OracleDialect` 或 `SqlServerDialect` 的某个 trait 方法发现 `todo!` / `unimplemented!`
   - 系统行为：编译期 clippy 扫描拦截（门禁 8），运行时若触发则 panic
   - 用户感知：编译期门禁失败，禁止入库
2. **Dialect 生成 SQL 语法错误**
   - 触发条件：`OracleDialect` 生成的 SQL 在 Oracle 数据库执行时报语法错误
   - 系统行为：返回 `Err(DbError::SqlError)`，含 Oracle 驱动的语法错误信息
   - 用户感知：返回错误，错误信息含生成的 SQL 与 Oracle 的语法错误定位
3. **AnyPool 与 Dialect 后端不匹配**
   - 触发条件：AnyPool 后端为 Oracle 但使用了 MySqlDialect 生成 SQL
   - 系统行为：运行时 SQL 执行失败（Oracle 不识别 `LIMIT` 语法）
   - 用户感知：返回 `Err(DbError::SqlError)`，需通过 AnyPool 自动选择 Dialect 机制避免

---

## **5.3 AnyPool 与 Pool 统一抽象（需求 A-3，高优先级）**

### **5.3.1 用户故事**

**作为** sz-rust 框架开发者，**我希望** 持有一个统一类型的连接池（既能享受 `Pool` 的连接复用、断路器、限流、监控等完整能力，又能通过 `AnyPool` 实现 5 后端运行时切换），**以便** AppState 持有单一类型 `Arc<UnifiedPool>`，业务代码无需感知后端类型与池实现差异。

### **5.3.2 业务规则（EARS 格式）**

1. **统一抽象定义（Ubiquitous）**：The system shall provide a `UnifiedPool` (or equivalent abstraction) that combines the backend-switching capability of `AnyPool` with the connection-pooling capability of `Pool`.
   - 验收条件：[存在 `UnifiedPool` 类型（或 trait / newtype）] → [可从 DSN 创建，持有 5 后端之一的完整连接池状态]
2. **从 Pool 迁移路径（Optional feature）**：Where an existing codebase holds `Arc<sz_orm_core::Pool>`, the system shall provide a zero-cost or low-cost migration path to `Arc<UnifiedPool>`.
   - 验收条件：[sz-rust 当前 `AppState { pool: Arc<Pool> }`] → [可改为 `AppState { pool: Arc<UnifiedPool> }`，编译通过且行为不变]
3. **连接复用语义保持（State-driven）**：When `UnifiedPool::acquire()` is called, the system shall reuse idle connections from the pool (if any) rather than creating a new connection.
   - 验收条件：[创建 `UnifiedPool`（max_size=10），连续 acquire 20 次 release 10 次] → [实际创建连接数 ≤ 10，连接被复用]
4. **断路器/限流/监控语义保持（State-driven）**：When `UnifiedPool` is configured with circuit-breaker / rate-limit / metrics, the system shall enforce these policies identically to `Pool`.
   - 验收条件：[配置断路器阈值=5，连续 5 次连接失败] → [断路器跳闸，拒绝新 acquire，与 `Pool` 行为一致]
5. **5 后端运行时切换（Event-driven）**：When `UnifiedPool::connect(dsn)` is called with different DSNs (mysql/postgres/sqlite/oracle/mssql), the system shall create a pool targeting the corresponding backend.
   - 验收条件：[分别用 5 种 DSN 创建 `UnifiedPool`] → [5 个池的 `backend()` 返回 5 种不同 `AnyBackend`，各自 CRUD 正常]
6. **Dialect 自动绑定（State-driven）**：When a `UnifiedPool` has a specific backend, the system shall automatically bind the corresponding `Dialect` for SQL generation.
   - 验收条件：[`UnifiedPool::connect("oracle://...")` 后执行 ORM 操作] → [SQL 生成使用 `OracleDialect`]
7. **禁止项**：禁止 `UnifiedPool` 丢失 `Pool` 的任何现有能力（连接复用 / 超时 / 断路器 / 限流 / 监控 / 动态 resize / close_all）
   - 验收条件：[对 `UnifiedPool` 调用 `resize()` / `close_all()` / `metrics()` 等方法] → [行为与 `Pool` 完全一致]

### **5.3.3 交互流程**

```plantuml
@startuml
!theme plain
actor "sz-rust 框架" as SzRust
participant "UnifiedPool" as Unified
participant "Pool(内核)" as Pool
participant "AnyBackend" as Backend
database "5 后端之一" as DB

SzRust -> Unified : connect("oracle://...") 
Unified -> Backend : from_dsn() → Oracle
Unified -> Pool : new(config, OracleConnectionFactory)
Pool -> DB : 建立连接池
DB --> Pool : 就绪
Unified --> SzRust : Arc<UnifiedPool>

SzRust -> Unified : acquire()
Unified -> Pool : acquire()
Pool --> Unified : PooledConnection
Unified --> SzRust : 连接句柄

SzRust -> Unified : metrics()
Unified -> Pool : metrics()
Pool --> Unified : PoolMetrics
Unified --> SzRust : 监控数据
@enduml
```

### **5.3.4 异常场景**

1. **迁移后行为不一致**
   - 触发条件：sz-rust 从 `Arc<Pool>` 迁移到 `Arc<UnifiedPool>` 后，相同业务行为发生变化
   - 系统行为：sz-pay 回归测试拦截（门禁要求 5,139 测试无回归）
   - 用户感知：测试失败，禁止入库
2. **UnifiedPool 丢失 Pool 能力**
   - 触发条件：`UnifiedPool` 缺少 `Pool` 的某项能力（如断路器、动态 resize）
   - 系统行为：编译期或测试期发现
   - 用户感知：编译失败或测试失败，需补齐能力
3. **5 后端切换后状态泄漏**
   - 触发条件：切换后端时前一个后端的连接池状态（idle 连接、计数器）泄漏到新后端
   - 系统行为：每个 `UnifiedPool` 实例绑定单一后端，切换需创建新实例
   - 用户感知：文档明确说明 `UnifiedPool` 实例不可切换后端，需创建新实例

---

## **5.4 Eager Loading 多级关联增强（需求 B-1，中优先级）**

### **5.4.1 用户故事**

**作为** 应用开发者，**我希望** `with()` 链式预加载支持无限级关联（如 User → Order → OrderItem → Product）并自动检测循环，**以便** 一次调用加载整个对象图，追平 SeaORM 的多级 eager loading 能力。

### **5.4.2 业务规则（EARS 格式）**

1. **无限级链式预加载（Ubiquitous）**：The system shall support chaining `with()` calls to an arbitrary depth (not limited to 2 levels).
   - 验收条件：[`eager_load_all::<User>().with(Order).with(OrderItem).with(Product)`] → [返回 `Vec<(User, Vec<(Order, Vec<(OrderItem, Vec<Product>)>)>)>`]
2. **多级批量查询（State-driven）**：When multi-level eager loading is executed, the system shall use batch queries (WHERE fk IN (...)) at each level, not N+1 per-row queries.
   - 验收条件：[User 100 行 → Order 300 行 → OrderItem 900 行 → Product 1800 行] → [执行 4 条 SQL（每级 1 条），`N1QueryDetector` 无告警]
3. **循环检测（State-driven）**：When the eager loading chain contains a cycle (e.g., User → Order → User), the system shall detect the cycle and terminate recursion, returning the already-loaded portion.
   - 验收条件：[`eager_load_all::<User>().with(Order).with(User)`] → [检测到 User 重复，终止递归，返回 `Vec<(User, Vec<(Order, Vec<User>)>)>`，User 第二层为已加载实例，不无限递归]
4. **循环检测可配置（Optional feature）**：Where a cycle detection policy is specified (e.g., `CyclePolicy::Error` / `CyclePolicy::Truncate` / `CyclePolicy::AllowWithDepthLimit(n)`), the system shall apply the specified policy.
   - 验收条件：[配置 `CyclePolicy::Error` 遇到循环] → [返回 `Err`，错误信息含循环路径 `User → Order → User`]
   - 验收条件：[配置 `CyclePolicy::AllowWithDepthLimit(3)`] → [递归至深度 3 后终止]
5. **多级组装正确性（State-driven）**：When multi-level eager loading assembles results, the nesting structure shall correctly reflect foreign key relationships at each level.
   - 验收条件：[User(id=1) → Order(id=10, user_id=1) → OrderItem(id=100, order_id=10)] → [结果中 `User{id:1}.orders[0].id == 10` 且 `User{id:1}.orders[0].items[0].id == 100`]
6. **禁止项**：禁止多级 Eager Loading 生成 `SELECT *`（违反 v2.0.0 默认禁止 SELECT * 约束）
   - 验收条件：[多级 Eager Loading 生成的所有 SQL] → [不得包含 `SELECT *`，必须显式列出列名]

### **5.4.3 交互流程**

```plantuml
@startuml
!theme plain
actor "应用开发者" as Dev
participant "sz-orm" as ORM
database "DB" as DB

Dev -> ORM : eager_load_all::<User>().with(Order).with(OrderItem).with(Product)
ORM -> DB : SELECT id, name FROM users
DB --> ORM : Vec<User>(100)
ORM -> DB : SELECT id, user_id, ... FROM orders WHERE user_id IN (1..100)
DB --> ORM : Vec<Order>(300)
ORM -> DB : SELECT id, order_id, ... FROM order_items WHERE order_id IN (10..30)
DB --> ORM : Vec<OrderItem>(900)
ORM -> DB : SELECT id, ... FROM products WHERE id IN (item.product_id 集合)
DB --> ORM : Vec<Product>(1800)
ORM -> ORM : 4 级分组组装 + 循环检测
ORM --> Dev : Vec<(User, Vec<(Order, Vec<(OrderItem, Vec<Product>)>)>)>
@enduml
```

### **5.4.4 异常场景**

1. **关联深度超过内存限制**
   - 触发条件：多级关联每级行数爆炸（如 100 → 1000 → 10000 → 100000），总结果集超过内存
   - 系统行为：返回 `Err(DbError::MemoryLimitExceeded)` 或建议改用 Stream API
   - 用户感知：返回错误，错误信息含当前结果集大小与建议（改用 Stream API）
2. **循环未检测导致栈溢出**
   - 触发条件：关联链含循环但循环检测未生效
   - 系统行为：门禁测试覆盖循环场景，禁止入库
   - 用户感知：测试失败，需修复循环检测逻辑
3. **中间级查询失败**
   - 触发条件：第 K 级关联表查询失败（表不存在 / 连接断开）
   - 系统行为：立即终止，丢弃已查询的 K-1 级结果，返回错误
   - 用户感知：返回 `Err(DbError)`，错误信息含第 K 级 SQL 与底层错误

---

## **5.5 Schema Sync 破坏性变更安全策略（需求 B-2，中优先级）**

### **5.5.1 用户故事**

**作为** DBA / 运维工程师，**我希望** Schema Sync 提供 `destructive_sync()` 显式启用破坏性变更（DROP COLUMN / RENAME COLUMN），并在执行前调用数据迁移钩子备份数据，**以便** 在生产环境安全地执行破坏性 Schema 变更，避免数据丢失。

### **5.5.2 业务规则（EARS 格式）**

1. **破坏性 Sync 显式启用（Ubiquitous）**：The system shall provide a `destructive_sync()` method that explicitly enables destructive DDL generation (DROP COLUMN / RENAME COLUMN / DROP TABLE), separate from the non-destructive `sync()`.
   - 验收条件：[实体定义删除字段 `avatar`，调用 `sync()`] → [不生成 `DROP COLUMN avatar`，仅记录 diff]
   - 验收条件：[实体定义删除字段 `avatar`，调用 `destructive_sync(Confirm::Yes)`] → [生成 `ALTER TABLE ... DROP COLUMN avatar`]
2. **显式确认要求（Event-driven）**：When `destructive_sync()` is called without explicit confirmation, the system shall refuse to execute and return an error.
   - 验收条件：[`destructive_sync()` 无确认参数] → [返回 `Err`，错误信息要求显式确认]
   - 验收条件：[`destructive_sync(Confirm::Yes)`] → [执行破坏性 DDL]
3. **列重命名检测（State-driven）**：When Schema Sync detects a column removal + a column addition with compatible type and similar name, the system shall suggest a rename operation instead of drop + add.
   - 验收条件：[实体定义将 `user_name` 改为 `username`（类型相同）] → [`diff()` 返回 `SchemaDiff::RenameColumn { from: "user_name", to: "username" }`，而非 `DropColumn + AddColumn`]
4. **数据迁移钩子（Event-driven）**：When a destructive DDL is about to execute, the system shall invoke the registered data migration hook (if any) before execution, allowing backup/migration of data.
   - 验收条件：[注册 `before_drop_column` 钩子将旧列数据归档到历史表，执行 `destructive_sync` 删除 `avatar` 列] → [钩子先执行 `INSERT INTO avatar_history SELECT id, avatar FROM users`，再执行 `ALTER TABLE users DROP COLUMN avatar`]
5. **破坏性 DDL 审计（Event-driven）**：When a destructive DDL is executed, the system shall record an audit log entry via sz-orm-audit containing the operator, timestamp, DDL content, and affected row count.
   - 验收条件：[执行 `destructive_sync` 删除列] → [sz-orm-audit 记录含 `DROP COLUMN` DDL、操作时间、受影响行数]
6. **事务原子性（State-driven）**：When `destructive_sync()` executes multiple DDLs, the system shall execute them in a transaction (where supported), rolling back all on any failure.
   - 验收条件：[破坏性 sync 含 3 条 DDL，第 2 条失败] → [第 1 条回滚，数据库状态不变]
7. **禁止项**：禁止 `sync()`（非破坏性）自动生成任何 DROP / RENAME DDL
   - 验收条件：[调用 `sync()`] → [生成的 DDL 仅含 CREATE / ADD COLUMN / 修改类型等非破坏性操作]

### **5.5.3 交互流程**

```plantuml
@startuml
!theme plain
actor "DBA" as DBA
participant "SchemaSync" as Sync
participant "数据迁移钩子" as Hook
participant "sz-orm-audit" as Audit
database "DB" as DB

DBA -> Sync : destructive_sync(Confirm::Yes)
Sync -> Sync : diff() → [RenameColumn(user_name→username), DropColumn(avatar)]
Sync -> Hook : before_rename_column("user_name", "username")
Hook -> DB : 备份 user_name 数据
DB --> Hook : 备份完成
Sync -> Audit : 记录 RENAME COLUMN 审计日志
Sync -> DB : ALTER TABLE users RENAME COLUMN user_name TO username
DB --> Sync : 成功
Sync -> Hook : before_drop_column("avatar")
Hook -> DB : INSERT INTO avatar_history SELECT id, avatar FROM users
DB --> Hook : 归档完成
Sync -> Audit : 记录 DROP COLUMN 审计日志
Sync -> DB : ALTER TABLE users DROP COLUMN avatar
DB --> Sync : 成功
Sync --> DBA : 破坏性 sync 完成（2 条 DDL）
@enduml
```

### **5.5.4 异常场景**

1. **未显式确认**
   - 触发条件：调用 `destructive_sync()` 未传 `Confirm::Yes`
   - 系统行为：返回 `Err`，拒绝执行
   - 用户感知：错误信息要求显式确认
2. **数据迁移钩子失败**
   - 触发条件：`before_drop_column` 钩子执行失败（如历史表不存在）
   - 系统行为：终止破坏性 sync，不执行后续 DDL，已执行的操作回滚
   - 用户感知：返回 `Err`，错误信息含钩子失败原因
3. **DDL 执行失败**
   - 触发条件：破坏性 DDL 执行失败（如列不存在 / 权限不足）
   - 系统行为：事务回滚（如支持），已执行 DDL 回滚
   - 用户感知：返回 `Err`，错误信息含 DDL 与底层错误
4. **不支持事务的方言**
   - 触发条件：在 SQLite（部分 DDL 不支持事务）执行破坏性 sync，中途失败
   - 系统行为：已执行 DDL 无法回滚，返回错误并标注 `部分 DDL 已执行，需手动修复`
   - 用户感知：返回错误，错误信息含已执行 DDL 列表与未执行 DDL 列表

---

## **5.6 Partial Models select_exclude()（需求 B-3，低优先级）**

### **5.6.1 用户故事**

**作为** 应用开发者，**我希望** 通过 `select_exclude(["avatar", "blob_data"])` 排除特定字段查询，**以便** 在大表（含 BLOB/TEXT 大字段）场景下减少网络传输与内存占用，且无需逐个列出保留字段。

### **5.6.2 业务规则（EARS 格式）**

1. **字段排除查询（Ubiquitous）**：The system shall provide a `select_exclude(fields: &[&str])` method that queries all columns except the specified ones.
   - 验收条件：[`User::find().select_exclude(&["avatar", "blob_data"]).build()`] → [生成 `SELECT id, name, email, ... FROM users`，不含 `avatar` / `blob_data` 列]
2. **与 select_only 互补（State-driven）**：When a table has columns {a, b, c, d, e}, `select_exclude(&["d", "e"])` shall produce the same SQL as `select_only(&["a", "b", "c"])`.
   - 验收条件：[表含 5 列 {a,b,c,d,e}] → [`select_exclude(&["d","e"])` 生成的 SQL 等于 `select_only(&["a","b","c"])` 生成的 SQL]
3. **排除不存在字段报错（Event-driven）**：When `select_exclude` is called with a field name that does not exist in the entity, the system shall return an error.
   - 验收条件：[`User::find().select_exclude(&["nonexistent"])`] → [返回 `Err`，错误信息含 `字段 nonexistent 不存在`]
4. **排除全部字段报错（Event-driven）**：When `select_exclude` is called with all fields of the entity, the system shall return an error (cannot select zero columns).
   - 验收条件：[实体含 {a,b,c}，`select_exclude(&["a","b","c"])`] → [返回 `Err`，错误信息含 `不能排除所有字段`]
5. **与 Eager Loading 兼容（State-driven）**：When `select_exclude` is combined with eager loading, the main entity query shall exclude the specified fields while related entity queries remain unaffected.
   - 验收条件：[`User::find().select_exclude(&["avatar"]).with(Order)`] → [User 查询不含 `avatar`，Order 查询含全部字段]
6. **禁止项**：禁止 `select_exclude` 生成 `SELECT *`（必须显式列出保留列名）
   - 验收条件：[`select_exclude` 生成的 SQL] → [不得包含 `SELECT *`，必须显式列出保留列]

### **5.6.3 交互流程**

```plantuml
@startuml
!theme plain
actor "应用开发者" as Dev
participant "sz-orm" as ORM
database "DB" as DB

Dev -> ORM : User::find().select_exclude(&["avatar", "blob_data"]).build()
ORM -> ORM : 读取 User 全部列 → 排除 avatar, blob_data → 保留 [id, name, email, ...]
ORM --> Dev : QueryBuilder(SELECT id, name, email, ... FROM users)
Dev -> DB : execute(conn)
DB --> Dev : Vec<User>（avatar/blob_data 字段为默认值）
@enduml
```

### **5.6.4 异常场景**

1. **排除字段不存在**
   - 触发条件：`select_exclude` 传入实体中不存在的字段名
   - 系统行为：返回 `Err(DbError::InvalidInput)`
   - 用户感知：错误信息含不存在的字段名
2. **排除所有字段**
   - 触发条件：`select_exclude` 传入实体的全部字段
   - 系统行为：返回 `Err`，禁止生成零列 SELECT
   - 用户感知：错误信息含 `不能排除所有字段`
3. **排除字段含主键**
   - 触发条件：`select_exclude` 传入主键字段
   - 系统行为：允许（主键排除后实体无法标识，但 SQL 合法），返回 warning
   - 用户感知：返回成功，日志含 `排除主键字段可能导致实体无法标识` 警告

---

## **5.7 Stream API 背压控制（需求 B-4，低优先级）**

### **5.7.1 用户故事**

**作为** 应用开发者，**我希望** 通过 `stream_with_backpressure(buffer_size)` 创建有界缓冲区的流式查询，**以便** 当消费者速度慢于生产者时，生产者阻塞等待而非无限堆积行，控制大结果集处理的内存占用。

### **5.7.2 业务规则（EARS 格式）**

1. **有界缓冲流式查询（Ubiquitous）**：The system shall provide a `stream_with_backpressure(buffer_size: usize)` method that returns a stream with a bounded buffer of the specified size.
   - 验收条件：[`query.stream_with_backpressure(1000).await`] → [返回 `impl Stream<Item = Result<T>>`，内部缓冲区容量为 1000 行]
2. **背压阻塞（State-driven）**：When the buffer is full and the consumer has not consumed any item, the producer shall block (await) instead of dropping items or growing the buffer unboundedly.
   - 验收条件：[buffer_size=10，生产者产出 100 行，消费者未消费] → [生产者在产出第 11 行时阻塞，缓冲区始终保持 ≤ 10 行]
3. **背压释放（Event-driven）**：When the consumer consumes an item from a full buffer, the producer shall be unblocked and resume producing.
   - 验收条件：[buffer_size=10，缓冲区满，消费者消费 1 行] → [生产者恢复，产出第 11 行进入缓冲区]
4. **内存上限（State-driven）**：When `stream_with_backpressure(buffer_size)` processes a result set of N rows (N >> buffer_size), the peak memory shall not exceed `buffer_size * row_size + fixed_overhead`.
   - 验收条件：[结果集 100 万行，buffer_size=1000，每行 1 KB] → [峰值内存 ≤ 1000 * 1 KB + 10 MB = ~11 MB]
5. **错误传播（Event-driven）**：When the database connection drops during streaming, the stream shall yield `Err(DbError)` and terminate, not panic.
   - 验收条件：[流式查询中途数据库断开] → [Stream yield `Err(DbError::ConnectionError)` 后终止]
6. **与 stream_buffered 兼容（Optional feature）**：Where `stream_buffered()` (v2.1.0 unbounded) is used, the behavior shall remain unchanged (backward compatible).
   - 验收条件：[v2.1.0 代码使用 `stream_buffered()`] → [v2.2.0 编译通过，行为不变]
7. **禁止项**：禁止 `stream_with_backpressure` 静默丢弃行（背压必须通过阻塞实现，不得丢数据）
   - 验收条件：[buffer_size=10，生产 100 行，消费 100 行] → [消费者收到的行数 == 100，无丢失]

### **5.7.3 交互流程**

```plantuml
@startuml
!theme plain
actor "应用开发者" as Dev
participant "sz-orm" as ORM
database "DB" as DB

Dev -> ORM : query.stream_with_backpressure(1000).await
ORM -> DB : 执行查询，获取游标
ORM --> Dev : impl Stream (缓冲区=1000)

loop 生产者-消费者并行
  alt 缓冲区未满
    ORM -> DB : fetch next row
    DB --> ORM : row
    ORM -> ORM : 放入缓冲区
  else 缓冲区已满
    ORM -> ORM : 阻塞等待消费者
  end
  Dev -> ORM : stream.next().await
  ORM --> Dev : Some(Ok(row))
end
@enduml
```

### **5.7.4 异常场景**

1. **buffer_size 为 0**
   - 触发条件：`stream_with_backpressure(0)`
   - 系统行为：返回 `Err(DbError::InvalidInput)`，禁止零容量缓冲
   - 用户感知：错误信息含 `buffer_size 必须大于 0`
2. **数据库连接断开**
   - 触发条件：流式查询中途数据库连接断开
   - 系统行为：Stream yield `Err(DbError::ConnectionError)` 后终止
   - 用户感知：`stream.next().await` 返回 `Some(Err(...))`，后续 `next()` 返回 `None`
3. **消费者提前 drop**
   - 触发条件：消费者在 Stream 未耗尽前 drop
   - 系统行为：生产者检测到接收端关闭，终止游标查询，释放连接
   - 用户感知：连接归还连接池，无泄漏

---

## **5.8 嵌套持久化 cascade_delete 策略（需求 B-5，低优先级）**

### **5.8.1 用户故事**

**作为** 应用开发者，**我希望** `nested_delete()` 支持 4 种级联策略（RESTRICT / CASCADE / SET_NULL / SET_DEFAULT），**以便** 根据业务语义选择删除父实体时子实体的处理方式，追平 SeaORM 的级联删除能力。

### **5.8.2 业务规则（EARS 格式）**

1. **4 种策略定义（Ubiquitous）**：The system shall support 4 cascade delete strategies: `CascadeStrategy::Restrict` / `Cascade` / `SetNull` / `SetDefault`.
   - 验收条件：[存在 `CascadeStrategy` 枚举含 4 种变体] → [可配置于 `#[relation(cascade = "...")]` 或 `nested_delete(strategy)` 参数]
2. **RESTRICT 策略（State-driven）**：When `nested_delete(CascadeStrategy::Restrict)` is called and child entities exist, the system shall refuse to delete the parent and return an error.
   - 验收条件：[User(id=1) 有 3 个 Order，`user.nested_delete(Restrict)`] → [返回 `Err`，错误信息含 `存在 3 个子实体，禁止删除`，User 未删除]
3. **CASCADE 策略（State-driven）**：When `nested_delete(CascadeStrategy::Cascade)` is called, the system shall delete the parent and all child entities (recursively) in a transaction.
   - 验收条件：[User(id=1) 有 3 个 Order，每个 Order 有 5 个 OrderItem，`user.nested_delete(Cascade)`] → [删除 1 User + 3 Order + 15 OrderItem，全部在事务内]
4. **SET_NULL 策略（State-driven）**：When `nested_delete(CascadeStrategy::SetNull)` is called, the system shall set child entities' foreign key to NULL and delete the parent.
   - 验收条件：[User(id=1) 有 3 个 Order，`user.nested_delete(SetNull)`] → [3 个 Order 的 `user_id` 置 NULL，User(id=1) 删除]
5. **SET_DEFAULT 策略（State-driven）**：When `nested_delete(CascadeStrategy::SetDefault)` is called, the system shall set child entities' foreign key to its default value and delete the parent.
   - 验收条件：[Order.user_id 默认值为 0，`user.nested_delete(SetDefault)`] → [3 个 Order 的 `user_id` 置 0，User(id=1) 删除]
6. **策略可标注（Optional feature）**：Where a cascade strategy is annotated via `#[relation(cascade = "restrict")]`, the system shall apply the annotated strategy during `nested_delete()` without requiring explicit parameter.
   - 验收条件：[`#[relation(has_many = "Order", cascade = "restrict")]`，`user.nested_delete()`] → [按 RESTRICT 策略执行]
7. **事务原子性（State-driven）**：When `nested_delete()` executes, all operations (delete parent + update/delete children) shall be in a transaction, rolling back on any failure.
   - 验收条件：[CASCADE 策略，删除第 2 个 OrderItem 失败] → [事务回滚，User + Order + 已删 OrderItem 全部恢复]
8. **禁止项**：禁止 RESTRICT 策略静默降级为 CASCADE（存在子实体时必须拒绝）
   - 验收条件：[RESTRICT 策略 + 存在子实体] → [必须返回 `Err`，不得删除任何数据]

### **5.8.3 交互流程**

```plantuml
@startuml
!theme plain
actor "应用开发者" as Dev
participant "sz-orm" as ORM
database "DB" as DB

Dev -> ORM : user.nested_delete(CascadeStrategy::Cascade)
ORM -> DB : BEGIN TRANSACTION

alt RESTRICT 策略
  ORM -> DB : SELECT COUNT(*) FROM orders WHERE user_id = ?
  DB --> ORM : 3
  ORM --> Dev : Err(存在 3 个子实体，禁止删除)
  ORM -> DB : ROLLBACK
else CASCADE 策略
  ORM -> DB : DELETE FROM order_items WHERE order_id IN (子查询: orders.user_id=?)
  DB --> ORM : 15 行删除
  ORM -> DB : DELETE FROM orders WHERE user_id = ?
  DB --> ORM : 3 行删除
  ORM -> DB : DELETE FROM users WHERE id = ?
  DB --> ORM : 1 行删除
  ORM -> DB : COMMIT
  ORM --> Dev : Ok(19 行删除)
else SET_NULL 策略
  ORM -> DB : UPDATE orders SET user_id = NULL WHERE user_id = ?
  DB --> ORM : 3 行更新
  ORM -> DB : DELETE FROM users WHERE id = ?
  DB --> ORM : 1 行删除
  ORM -> DB : COMMIT
  ORM --> Dev : Ok(1 行删除 + 3 行更新)
end
@enduml
```

### **5.8.4 异常场景**

1. **RESTRICT 策略存在子实体**
   - 触发条件：RESTRICT 策略 + 子实体存在
   - 系统行为：返回 `Err`，不删除任何数据
   - 用户感知：错误信息含子实体数量
2. **SET_NULL 策略外键列非 nullable**
   - 触发条件：SET_NULL 策略 + 外键列定义为 NOT NULL
   - 系统行为：返回 `Err`，错误信息含 `外键列 user_id 不允许 NULL，无法执行 SET_NULL`
   - 用户感知：错误信息提示改用 CASCADE 或 SET_DEFAULT
3. **SET_DEFAULT 策略外键列无默认值**
   - 触发条件：SET_DEFAULT 策略 + 外键列无默认值定义
   - 系统行为：返回 `Err`，错误信息含 `外键列 user_id 无默认值，无法执行 SET_DEFAULT`
   - 用户感知：错误信息提示定义默认值或改用其他策略
4. **事务回滚**
   - 触发条件：级联删除中途某操作失败
   - 系统行为：事务回滚，所有已执行操作撤销
   - 用户感知：返回 `Err`，数据状态不变

---

# **6. 数据约束**

## **6.1 AnyBackend**

1. **变体集合**：含 `MySql` / `Postgres` / `Sqlite` / `Oracle` / `Mssql` 共 5 种变体（v2.2.0 从 3 种扩展至 5 种）
2. **DSN scheme 映射**：`mysql://` / `mariadb://` → MySql；`postgres://` / `postgresql://` → Postgres；`sqlite://` / `sqlite:` → Sqlite；`oracle://` → Oracle；`mssql://` / `sqlserver://` → Mssql
3. **唯一性**：每个 DSN scheme 映射到唯一的 `AnyBackend` 变体

## **6.2 UnifiedPool**

1. **后端绑定**：每个 `UnifiedPool` 实例绑定唯一的 `AnyBackend`，创建后不可切换
2. **连接池状态**：持有 `idle: ArrayQueue<PooledConnection>` + `total_count: AtomicU32` + `Notify` + 监控计数器，与 `Pool` 结构等价
3. **Dialect 绑定**：根据 `AnyBackend` 自动绑定对应 `Dialect`（Oracle → OracleDialect，Mssql → SqlServerDialect）

## **6.3 CascadeStrategy**

1. **变体集合**：`Restrict` / `Cascade` / `SetNull` / `SetDefault` 共 4 种
2. **默认值**：未显式指定时默认为 `Cascade`（与 v2.1.0 `nested_delete()` 行为兼容）
3. **标注来源**：可通过 `#[relation(cascade = "...")]` 标注或 `nested_delete(strategy)` 参数指定，参数优先级高于标注

## **6.4 CyclePolicy（多级 Eager Loading 循环策略）**

1. **变体集合**：`Error`（遇循环报错）/ `Truncate`（截断循环，返回已加载部分）/ `AllowWithDepthLimit(n)`（允许至深度 n）
2. **默认值**：未显式指定时默认为 `Truncate`（安全截断，不报错）
3. **深度计数**：`AllowWithDepthLimit(n)` 的 n 从 0 开始，0 表示仅加载主表

## **6.5 SchemaDiff（破坏性变更）**

1. **新增变体**：`RenameColumn { from: String, to: String }`（v2.2.0 新增，区别于 `DropColumn + AddColumn`）
2. **检测规则**：同表内一列删除 + 一列新增且类型兼容 + 名称相似度 ≥ 阈值 → 识别为 `RenameColumn`
3. **名称相似度阈值**：Levenshtein 距离 ≤ 2 或编辑距离 / 长度 ≤ 0.3（可配置）

---

# **7. 优先级排序与里程碑**

## **7.1 需求优先级矩阵**

| 需求 ID | 需求名称 | 优先级 | 来源 | 预期里程碑 |
|---------|---------|--------|------|-----------|
| A-1 | AnyPool 扩展支持 Oracle/MSSQL | 🔴 高 | sz-rust 框架集成 | M1 |
| A-2 | Dialect 与 AnyPool 运行时切换集成验证 | 🔴 高 | sz-rust 框架集成 | M1（与 A-1 同期） |
| A-3 | AnyPool 与 Pool 统一抽象 | 🔴 高 | sz-rust 框架集成 | M2 |
| B-1 | Eager Loading 多级关联增强 | 🟠 中 | v2.1.0 路线图 §4.1 | M3 |
| B-2 | Schema Sync 破坏性变更安全策略 | 🟠 中 | v2.1.0 路线图 §4.1 | M3 |
| B-3 | Partial Models select_exclude() | 🟡 低 | v2.1.0 路线图 §4.1 | M4 |
| B-4 | Stream API 背压控制 | 🟡 低 | v2.1.0 路线图 §4.1 | M4 |
| B-5 | 嵌套持久化 cascade_delete 策略 | 🟡 低 | v2.1.0 路线图 §4.1 | M4 |

## **7.2 里程碑规划**

| 里程碑 | 内容 | 优先级 | 依赖 |
|--------|------|--------|------|
| M1 | A-1 + A-2：AnyPool 扩展 Oracle/MSSQL + Dialect 集成验证 | 🔴 高 | 无 |
| M2 | A-3：AnyPool 与 Pool 统一抽象 | 🔴 高 | M1（需 5 后端 AnyPool 就绪） |
| M3 | B-1 + B-2：多级 Eager Loading + 破坏性 Schema Sync | 🟠 中 | 无（与 M1/M2 可并行） |
| M4 | B-3 + B-4 + B-5：select_exclude + Stream 背压 + cascade_delete | 🟡 低 | 无 |
| M5 | 集成验证与版本发布：sz-pay 回归 + 11 道门禁 + crates.io 发布 | — | M1-M4 全部完成 |

## **7.3 EARS 格式汇总**

本规格使用的 EARS 需求格式统计：

| EARS 类型 | 数量 | 说明 |
|-----------|------|------|
| Ubiquitous（始终为真） | 8 | 系统始终满足的约束（如 AnyBackend 含 5 变体、4 种 cascade 策略） |
| State-driven（状态驱动） | 14 | 当系统处于某状态时触发（如缓冲区满 → 阻塞） |
| Event-driven（事件驱动） | 11 | 当事件发生时触发（如收到 DSN → 识别后端） |
| Optional feature（可选特性） | 5 | 可选能力（如循环策略可配置、cascade 标注） |
| **合计** | **38** | — |

---

# **8. 需求来源追溯**

| 需求 ID | 来源 | 源码现状证据 | 是否真实缺口 |
|---------|------|-------------|-------------|
| A-1 | sz-rust 多后端集成需求 | `any_driver.rs:43-50` AnyBackend 仅 3 变体；`any_driver.rs:100-113` connect 仅 3 分支 | ✅ 真实缺口 |
| A-2 | sz-rust 多后端集成需求 | `dialect.rs:939` OracleDialect 已实现；`dialect.rs:1185` SqlServerDialect 已实现 | ⚠️ Dialect 已实现，缺口在"与 AnyPool 集成验证 + 运行时自动选择" |
| A-3 | sz-rust 多后端集成需求 | `any_driver.rs:86` AnyPool 仅持工厂；`pool.rs:712` Pool 是完整连接池 | ✅ 真实缺口 |
| B-1 | v2.1.0 路线图 §4.1 第 1 项 | v2.1.0 `with()` 限 2 级 | ✅ 真实缺口 |
| B-2 | v2.1.0 路线图 §4.1 第 2 项 | v2.1.0 `sync()` 禁止 DROP COLUMN | ✅ 真实缺口 |
| B-3 | v2.1.0 路线图 §4.1 第 3 项 | v2.1.0 仅有 `select_only` | ✅ 真实缺口 |
| B-4 | v2.1.0 路线图 §4.1 第 4 项 | v2.1.0 `stream_buffered` 无界 | ✅ 真实缺口 |
| B-5 | v2.1.0 路线图 §4.1 第 5 项 | v2.1.0 `nested_delete` 仅 CASCADE | ✅ 真实缺口 |

---

> **文档版本**：v1.0（v2.2.0 需求规格初稿）
> **生成日期**：2026-08-06
> **生成方法**：基于源码现状调研 + v2.1.0 路线图 + sz-rust 框架集成需求，EARS 格式编写
> **审计合规**：所有现状结论附 `file:line` 证据（见 §0 与 §8）
> **下一步**：用户确认需求规格 → spec-design-agent 生成 design.md → spec-task-agent 生成 tasks.md