# sz-orm v2.1.0 需求规格说明书

> **版本**：v2.1.0
> **基线**：v2.0.0（42 包 @ 2.0.0 已发布 crates.io，4,947 测试通过）
> **生成日期**：2026-08-06
> **文档目的**：定义 v2.1.0 七项功能任务的需求规格，聚焦缩小与 SeaORM 的功能差距
> **依据**：
> - `docs/assessment/2026-08-06-v2-progress-and-roadmap.md`（v2.0.0 进展 + v2.1.0 路线图）
> - `docs/assessment/2026-08-04-deep-comparison.md`（竞品深度对比，劣势 L-1/L-2/L-3/L-4/L-7）
> - `docs/assessment/2026-08-05-security-audit-report.md`（安全审计基线）

---

# **1. 组件定位**

## **1.1 核心职责**

本组件负责在 v2.0.0 基础上补齐 7 项功能能力，实现 sz-orm 与 SeaORM 功能覆盖度从 ~95% 提升至 ~99%，并建立与 Diesel/SeaORM/SQLx 的性能基准对比体系。

## **1.2 核心输入**

1. **应用层调用请求**：业务代码通过 sz-orm API 发起的关联查询、字段选择、Schema 同步、嵌套持久化、流式查询请求
2. **模型定义元数据**：`#[derive(Entity)]` / `#[derive(Relation)]` / `#[derive(ActiveModel)]` 宏标注的结构体定义
3. **数据库连接句柄**：来自连接池的异步连接（MySQL/PostgreSQL/SQLite/Oracle/MSSQL）
4. **目标数据库 Schema 状态**：执行 Schema Sync 时从 `INFORMATION_SCHEMA` 读取的现有表结构

## **1.3 核心输出**

1. **嵌套领域对象集合**：Eager loading 自动组装的 `Vec<(MainEntity, Vec<RelatedEntity>)>` 结构
2. **部分字段实体集合**：Partial Models 查询返回的只含指定字段的实体集合
3. **DDL 变更脚本**：Schema Sync 生成的 CREATE TABLE / ALTER TABLE 语句序列
4. **流式行迭代器**：异步流式查询返回的 `impl Stream<Item = Result<T>>`
5. **性能基准报告**：与 Diesel/SeaORM/SQLx 对比的 criterion 基准测试结果（吞吐量 / 延迟 / 内存）
6. **持久化确认**：ActiveModel 嵌套 save 返回的受影响行数与主键回填

## **1.4 职责边界**

本组件**不负责**以下事项：

1. **不负责**新增数据库方言支持（v2.1.0 维持 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 五方言）
2. **不负责**连接池重构（沿用 v2.0.0 的无锁 ArrayQueue 实现）
3. **不负责**安全审计跟进（unwrap SAFETY 注释补齐、eprintln 替换为 v2.0.x 维护任务）
4. **不负责**async-std 运行时支持（ADR-0011 设计决策，仅支持 Tokio）
5. **不负责**Python/JS FFI 绑定扩展（v2.0.0 已交付 0.1.0，v2.1.0 不扩展 API 面）
6. **不负责**生产案例验证（属 v2.2.0+ 长期目标，sz-pay 试点）
7. **不负责**图数据库 / WASM 完善 / maturin 发布产物（v2.2.0+ 长期目标）

---

# **2. 领域术语**

**Eager Loading（预加载）**
: 在查询主表记录时，一次性将关联表记录批量加载并组装为嵌套结构的能力，用于消除 N+1 查询问题。
: 备注：SeaORM 称 `find_with_related`，SQLx 无内建支持，Diesel 通过 `join` + 手动组装实现。

**RelationTrait（关联特征）**
: 描述实体间关联关系（HasOne / HasMany / BelongsTo / ManyToMany）的类型 trait，提供 `join()` 链式查询入口。
: 备注：sz-orm v2.0.0 的 `#[derive(Relation)]` 仅生成元数据常量，不生成 `RelationTrait` 实现。

**Partial Model（部分模型）**
: 查询时只选择实体部分字段（而非 `SELECT *`）的能力，用于大表宽表场景减少网络传输与内存占用。
: 备注：SeaORM 称 `select_only()`，SQLx 通过手写 SELECT 列表实现，Diesel 通过 `Select` trait 实现。

**Schema Sync（结构同步）**
: 比较实体定义与数据库现有表结构的差异，自动生成并执行 DDL（CREATE/ALTER TABLE）使数据库结构与代码定义一致的能力。
: 备注：SeaORM 2.0 称 `db.sync()`，Diesel 无此能力（依赖 `diesel migration`），sz-orm v2.0.0 有 Phinx 风格手动迁移但无自动 diff。

**ActiveModel 嵌套持久化（Nested Persistence）**
: 一次 `save()` 调用持久化整个对象图（父实体 + 子实体集合），自动处理外键关联与执行顺序的能力。
: 备注：SeaORM 支持，sz-orm v2.0.0 的 `ActiveModel` 仅支持单实体脏字段追踪，需逐个 model 调用 save。

**异步流式查询（Stream Query）**
: 以异步流（`impl Stream`）形式逐行产出查询结果，而非一次性收集为 `Vec` 的能力，用于大结果集内存控制。
: 备注：sz-orm v2.0.0 的 5 个 DB 适配器已实现 `query_stream` 真游标，v2.1.0 需提供更高层的 ORM Stream API。

**N+1 查询检测（N+1 Detection）**
: 运行时统计滑动窗口内单行加载次数，超过阈值触发告警的机制。
: 备注：sz-orm v2.0.0 已实现 `N1QueryDetector`，Eager Loading 自动执行后应消除 N+1 告警。

**性能基准对比（Benchmark Comparison）**
: 使用 criterion 框架对相同业务场景在 sz-orm / Diesel / SeaORM / SQLx 下的吞吐量、延迟、内存进行量化对比的测试体系。
: 备注：sz-orm v2.0.0 有 7 组内部 criterion 基准，但无跨框架对比。

---

# **3. 角色与边界**

## **3.1 核心角色**

- **应用开发者**：使用 sz-orm API 编写业务数据访问代码的 Rust 开发者，是 Eager Loading / RelationTrait / Partial Models / Stream API 的主要使用者
- **DBA / 运维工程师**：负责数据库结构管理，是 Schema Sync 的主要使用者，关注 DDL 变更安全性
- **性能工程师**：负责评估 ORM 选型，是性能基准对比报告的主要读者
- **架构决策者**：基于竞品对比报告做技术选型决策，关注 sz-orm 与 SeaORM/SQLx/Diesel 的功能对等性

## **3.2 外部系统**

- **MySQL 9.6**：`mysql://root:test123@127.0.0.1:3306/sz_orm_test`，集成测试与基准对比
- **PostgreSQL 18**：`postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`，集成测试与基准对比
- **SQLite**：内存模式，集成测试与基准对比
- **Oracle 23ai Free**：`127.0.0.1:1521/freepdb1`，集成测试（Schema Sync 需适配）
- **SQL Server**：集成测试（本机无实例，方言适配）
- **Diesel / SeaORM / SQLx**：性能基准对比的参照实现（作为 dev-dependency 引入基准测试 crate）

## **3.3 交互上下文**

```plantuml
@startuml
!theme plain
skinparam componentStyle rectangle

actor "应用开发者" as Dev
actor "DBA/运维" as DBA
actor "性能工程师" as Perf

component "sz-orm v2.1.0" as SzOrm {
  portin "模型定义 + 查询请求" as API
  portout "嵌套实体 / 流式行 / DDL" as Result
}

database "MySQL" as MySQL
database "PostgreSQL" as PG
database "SQLite" as SQLite
database "Oracle" as Oracle
database "SQL Server" as MSSQL

component "Diesel" as Diesel
component "SeaORM" as SeaORM
component "SQLx" as SQLx

Dev --> API : Eager Loading / join() / select_only() / Stream
DBA --> API : Schema Sync
Perf --> SzOrm : 基准对比

SzOrm --> MySQL : DDL + DML
SzOrm --> PG : DDL + DML
SzOrm --> SQLite : DDL + DML
SzOrm --> Oracle : DDL + DML
SzOrm --> MSSQL : DDL + DML

SzOrm ..> Diesel : 基准参照
SzOrm ..> SeaORM : 基准参照
SzOrm ..> SQLx : 基准参照

@enduml
```

---

# **4. DFX约束**

## **4.1 性能**

1. **Eager Loading 性能**：对于 1:N 关联查询（主表 N 行，关联表 M 行），自动执行 + 组装的耗时不得超过手动执行两条 SQL + 手动组装耗时的 1.1 倍（开销 ≤10%）
2. **Partial Models 性能**：选择 K 个字段查询的内存占用不得超过全字段查询的 `(K + 2) / 总字段数` 比例（2 为固定开销列：主键 + 行元数据）
3. **Stream 查询内存**：流式查询在结果集 100 万行时，峰值内存不得超过 50 MB（逐行产出，不缓存全量）
4. **基准对比场景**：CRUD 单行操作延迟不得超过 SQLx 的 1.5 倍，不得超过 SeaORM 的 1.2 倍
5. **Schema Sync 性能**：单表 diff + 生成 DDL 耗时不得超过 100 ms（10 万行表结构）

## **4.2 可靠性**

1. **API 向后兼容**：v2.1.0 不得引入 Breaking Change（v2.0.0 API 全部保留），新增能力以扩展方法提供
2. **Eager Loading 正确性**：自动组装结果与手动执行相同 SQL 并组装的结果必须按主键排序后完全一致
3. **Schema Sync 安全性**：自动生成的 DDL 不得导致数据丢失（禁止自动生成 `DROP COLUMN`，必须显式确认）
4. **嵌套持久化原子性**：ActiveModel 嵌套 save 在事务内执行，任一子实体失败则整体回滚
5. **Stream 错误传播**：流式查询中数据库连接断开时，Stream 必须 yield `Err(DbError)` 而非 panic

## **4.3 安全性**

1. **SQL 注入防护**：所有新增 API（join 链式 / select_only / Schema Sync DDL）必须使用参数化查询或标识符校验，禁止字符串拼接用户输入
2. **标识符校验**：Schema Sync 生成的表名 / 列名必须经过 `sql_safety::validate_identifier` 校验
3. **DDL 审计**：Schema Sync 执行的每条 DDL 必须经过 sz-orm-audit 记录审计日志
4. **unsafe 零容忍**：新增代码 0 处 `unsafe`，0 处 `todo!` / `unimplemented!` / `unreachable!`

## **4.4 可维护性**

1. **测试覆盖**：每个新增功能必须有单元测试 + 集成测试（至少 MySQL + SQLite 两方言），测试覆盖率不得低于 80%
2. **文档覆盖**：每个新增公开 API 必须有 rustdoc 文档注释 + 至少一个 doctest 示例
3. **门禁通过**：v2.1.0 必须通过 10 道门禁（fmt / check / clippy / test / doc / audit / integration / 占位扫描 / SQL 注入扫描 / feature 全组合）
4. **clippy 零警告**：`cargo clippy --workspace --all-targets -- -D warnings` 必须通过

## **4.5 兼容性**

1. **Rust 版本**：维持 `rust-version = "1.81"`，不提升 MSRV
2. **数据库方言**：新增功能必须覆盖 MySQL / PostgreSQL / SQLite / Oracle / MSSQL 五方言（MSSQL 无实例时方言适配 + ignored 测试）
3. **crates.io 发布**：v2.1.0 所有包发布到 crates.io，版本号从 2.0.0 升级至 2.1.0
4. **内部依赖**：所有 `version + path` 格式的内部依赖统一至 2.1.0

---

# **5. 核心能力**

## **5.1 Eager Loading 端到端自动执行与组装（P-F-1）**

### **5.1.1 用户故事**

**作为** 应用开发者，**我希望** 调用一行 API 即可完成主表 + 关联表的查询与嵌套组装，**以便** 消除手动执行两条 SQL 和手动组装结果的样板代码，追平 SeaORM `find_with_related().all()` 的开发体验。

### **5.1.2 业务规则**

1. **自动执行规则**：Eager Loading API 必须自动执行主表查询和关联表查询两条 SQL，无需调用方手动执行
   - 验收条件：[调用 `eager_load_all::<User, Order>(conn).await`] → [返回 `Vec<(User, Vec<Order>)>`，内部已执行 2 条 SQL]
2. **自动组装规则**：API 必须按外键关联自动将关联表记录组装到对应主表记录的嵌套字段中
   - 验收条件：[主表 User(id=1) 有 3 条 Order(user_id=1)] → [结果中 `(User{id:1}, orders)` 的 `orders.len() == 3`]
3. **N+1 消除规则**：Eager Loading 必须使用 `WHERE fk IN (...)` 批量查询关联表，不得逐行查询
   - 验收条件：[主表 100 行] → [关联表查询 SQL 恰好 1 条（非 100 条），`N1QueryDetector` 无告警]
4. **关联类型适配规则**：HasOne 关联使用 JOIN 策略（单条 SQL），HasMany 关联使用双查询策略（主表 + WHERE IN）
   - 验收条件：[查询 User HasOne Profile] → [执行 1 条 JOIN SQL]；[查询 User HasMany Order] → [执行 2 条 SQL]
5. **多级关联规则**：支持链式预加载多级关联（如 User → Order → OrderItem）
   - 验收条件：[`eager_load_all::<User, Order>().with(OrderItem)`] → [返回 `Vec<(User, Vec<(Order, Vec<OrderItem>)>)>`]
6. **禁止项**：禁止在 Eager Loading 中生成 `SELECT *`（违反 v2.0.0 默认禁止 SELECT * 约束）
   - 验收条件：[Eager Loading 生成的 SQL] → [不得包含 `SELECT *`，必须显式列出列名或使用 Partial Models]

### **5.1.3 交互流程**

```plantuml
@startuml
!theme plain
actor "应用开发者" as Dev
participant "sz-orm" as ORM
database "DB" as DB

Dev -> ORM : eager_load_all::<User, Order>(conn)
ORM -> DB : SELECT id, name FROM users WHERE ?
DB --> ORM : Vec<User>（100 行）
ORM -> ORM : 提取主键列表 [1, 2, ..., 100]
ORM -> DB : SELECT id, user_id, amount FROM orders WHERE user_id IN (?, ?, ..., ?)
DB --> ORM : Vec<Order>（300 行）
ORM -> ORM : 按 user_id 分组组装
ORM --> Dev : Vec<(User, Vec<Order>)>
@enduml
```

### **5.1.4 异常场景**

1. **主表查询失败**
   - 触发条件：主表 SQL 执行返回错误（连接断开 / SQL 语法错误）
   - 系统行为：立即终止，不执行关联表查询
   - 用户感知：返回 `Err(DbError)`，错误信息包含主表 SQL 与底层 DB 错误
2. **关联表查询失败**
   - 触发条件：关联表 SQL 执行返回错误（外键列不存在 / 连接断开）
   - 系统行为：丢弃已查询的主表结果，返回错误
   - 用户感知：返回 `Err(DbError)`，错误信息包含关联表 SQL 与底层 DB 错误
3. **外键不匹配**
   - 触发条件：关联表记录的外键值在主表主键列表中不存在（数据不一致）
   - 系统行为：跳过孤立关联记录，记录 warn 日志
   - 用户感知：返回成功，但嵌套结果中不包含孤立记录；日志含 `orphaned related record` 警告
4. **空主表结果**
   - 触发条件：主表查询返回 0 行
   - 系统行为：跳过关联表查询，直接返回空 Vec
   - 用户感知：返回 `Ok(Vec::new())`

---

## **5.2 `#[derive(Relation)]` 生成 RelationTrait + join() 链式（P-F-2）**

### **5.2.1 用户故事**

**作为** 应用开发者，**我希望** `#[derive(Relation)]` 自动生成 `RelationTrait` 实现并提供 `join()` 链式 API，**以便** 以 `User::find().join(Posts)` 风格编写关联查询，追平 SeaORM 的关联查询开发体验。

### **5.2.2 业务规则**

1. **RelationTrait 自动生成规则**：`#[derive(Relation)]` 必须自动为目标实体生成 `RelationTrait` 实现，包含 `def(&self) -> RelationDef` 方法
   - 验收条件：[`#[derive(Relation)] struct User { #[relation(has_many = "Order")] ... }`] → [编译后 `User` 实现 `RelationTrait`，`User::relation("orders").def()` 返回 `RelationDef`]
2. **join() 链式规则**：`QueryBuilder` 必须提供 `join(relation: impl RelationTrait)` 方法，返回 `Self` 支持链式调用
   - 验收条件：[`User::find().join(Order::relation())`] → [生成 `SELECT ... FROM users JOIN orders ON users.id = orders.user_id`]
3. **多表 join 规则**：支持连续多次 `.join()` 调用，生成多表 JOIN SQL
   - 验收条件：[`User::find().join(Profile::relation()).join(Order::relation())`] → [生成三表 JOIN SQL]
4. **JOIN 类型选择规则**：`join()` 默认 INNER JOIN，提供 `left_join()` 方法生成 LEFT JOIN
   - 验收条件：[`User::find().left_join(Order::relation())`] → [生成 `LEFT JOIN orders ON ...`]
5. **关联类型映射规则**：HasOne / BelongsTo 生成 INNER JOIN，HasMany 生成 LEFT JOIN（避免丢失主表行）
   - 验收条件：[User HasMany Order 使用 `join()`] → [生成 LEFT JOIN]；[User BelongsTo Department 使用 `join()`] → [生成 INNER JOIN]
6. **禁止项**：禁止 `join()` 接受字符串参数（必须接受 `RelationTrait` 实现类型，防止 SQL 注入）
   - 验收条件：[尝试 `query.join("raw_sql")`] → [编译错误，类型不匹配]

### **5.2.3 交互流程**

```plantuml
@startuml
!theme plain
actor "应用开发者" as Dev
participant "sz-orm-macros" as Macro
participant "sz-orm-core" as Core
database "DB" as DB

Dev -> Macro : #[derive(Relation)] struct User { ... }
Macro --> Dev : 生成 RelationTrait impl for User

Dev -> Core : User::find().join(Order::relation()).build()
Core -> Core : 从 RelationTrait 获取 RelationDef
Core -> Core : 生成 JOIN SQL
Core --> Dev : QueryBuilder（含 JOIN SQL）

Dev -> DB : query.execute(conn)
DB --> Dev : 结果集
@enduml
```

### **5.2.4 异常场景**

1. **关联关系未定义**
   - 触发条件：`join()` 传入的 relation 名称在 `#[derive(Relation)]` 中未定义
   - 系统行为：编译期报错（宏展开时找不到 relation 定义）
   - 用户感知：编译错误 `relation "xxx" not defined in #[derive(Relation)]`
2. **外键列不存在**
   - 触发条件：`RelationDef` 中的外键列在目标表中不存在
   - 系统行为：SQL 执行时 DB 返回错误
   - 用户感知：返回 `Err(DbError::SqlError)`，含 "column xxx does not exist"
3. **JOIN 表名冲突**
   - 触发条件：多次 `join()` 同一关联表导致表别名冲突
   - 系统行为：自动生成表别名（如 `orders_1`, `orders_2`）
   - 用户感知：SQL 中使用自动别名，查询正常执行

---

## **5.3 Partial Models — select_only()（P-F-3）**

### **5.3.1 用户故事**

**作为** 应用开发者，**我希望** 查询时只选择实体部分字段而非全量 `SELECT *`，**以便** 在大表宽表场景减少网络传输与内存占用，追平 SeaORM `select_only()` 的性能优化能力。

### **5.3.2 业务规则**

1. **select_only 规则**：`QueryBuilder` 必须提供 `select_only()` 方法，进入"部分字段选择"模式
   - 验收条件：[`User::find().select_only()`] → [进入部分选择模式，后续 `.column()` 添加选中字段]
2. **字段添加规则**：部分选择模式下通过 `.column(C)` / `.columns(vec![C1, C2])` 添加选中字段
   - 验收条件：[`select_only().column(UserColumn::Id).column(UserColumn::Name)`] → [生成 `SELECT id, name FROM users`]
3. **列枚举类型安全规则**：`.column()` 接受 `impl ColumnTrait` 类型，不接受字符串（编译期防止列名拼写错误）
   - 验收条件：[`select_only().column(UserColumn::Id)`] → [编译通过]；[`select_only().column("id")`] → [编译错误，类型不匹配]
4. **聚合函数规则**：部分选择模式下支持 `.column_as(C, alias)` 添加聚合表达式（如 `COUNT(*)` / `SUM(amount)`）
   - 验收条件：[`select_only().column_as(Expr::count(OrderColumn::Id), "order_count")`] → [生成 `SELECT COUNT(orders.id) AS order_count`]
5. **GROUP BY 规则**：部分选择模式下支持 `.group_by(C)` 分组
   - 验收条件：[`select_only().column(UserColumn::Id).group_by(UserColumn::Id)`] → [生成 `SELECT id FROM users GROUP BY id`]
6. **禁止项**：禁止 `select_only()` 后不添加任何字段就 build（必须至少选择 1 个字段）
   - 验收条件：[`select_only().build()`] → [返回 `Err(DbError::InvalidInput)`，提示 "select_only requires at least one column"]

### **5.3.3 交互流程**

```plantuml
@startuml
!theme plain
actor "应用开发者" as Dev
participant "sz-orm" as ORM
database "DB" as DB

Dev -> ORM : User::find().select_only()
ORM -> ORM : 进入部分选择模式
Dev -> ORM : .column(UserColumn::Id).column(UserColumn::Name)
ORM -> ORM : 记录选中列
Dev -> ORM : .build()
ORM -> ORM : 生成 SELECT id, name FROM users
ORM --> Dev : QueryBuilder

Dev -> DB : query.execute(conn)
DB --> Dev : 部分字段结果集
@enduml
```

### **5.3.4 异常场景**

1. **列名不存在**
   - 触发条件：`ColumnTrait` 枚举变体对应的列名在 DB 表中不存在
   - 系统行为：SQL 执行时 DB 返回错误
   - 用户感知：返回 `Err(DbError::SqlError)`，含 "column xxx does not exist"
2. **未选择任何字段**
   - 触发条件：`select_only()` 后未调用 `.column()` 就 `.build()`
   - 系统行为：build 时校验选中字段数 ≥ 1
   - 用户感知：返回 `Err(DbError::InvalidInput)`，提示 "select_only requires at least one column"
3. **聚合别名冲突**
   - 触发条件：多个 `.column_as()` 使用相同别名
   - 系统行为：SQL 执行时 DB 返回错误（或方言不同行为不一）
   - 用户感知：返回 `Err(DbError::SqlError)`，含 "duplicate column alias"

---

## **5.4 Schema Sync — 自动建表/改表 diff（P-F-4）**

### **5.4.1 用户故事**

**作为** DBA / 应用开发者，**我希望** sz-orm 自动比较实体定义与数据库现有表结构的差异并生成 DDL 变更脚本，**以便** 追平 SeaORM 2.0 `db.sync()` 的开发体验，减少手动编写迁移脚本的工作量。

### **5.4.2 业务规则**

1. **diff 规则**：Schema Sync 必须比较实体定义（Rust 结构体字段）与 DB 现有表结构（`INFORMATION_SCHEMA`），输出差异列表
   - 验收条件：[实体新增字段 `email`，DB 表无此列] → [diff 输出 `AddColumn { table: "users", column: "email", type: "VARCHAR" }`]
2. **DDL 生成规则**：根据 diff 生成对应的 DDL 语句（CREATE TABLE / ADD COLUMN / ALTER COLUMN TYPE / RENAME COLUMN）
   - 验收条件：[diff 为 `AddColumn`] → [生成 `ALTER TABLE users ADD COLUMN email VARCHAR(255)`]
3. **安全禁止规则**：Schema Sync 禁止自动生成 `DROP COLUMN` 和 `DROP TABLE`（防止数据丢失）
   - 验收条件：[实体删除字段 `legacy_col`，DB 表有此列] → [diff 输出 `DroppedColumn` 警告，不生成 DDL，需显式确认]
4. **dry-run 规则**：提供 `sync_dry_run()` 方法，只生成 DDL 不执行，供人工审查
   - 验收条件：[`schema.sync_dry_run(conn).await`] → [返回 `Vec<String>` DDL 语句，DB 结构不变]
5. **执行规则**：提供 `sync(conn)` 方法，在事务内执行所有 DDL，任一失败回滚
   - 验收条件：[`schema.sync(conn).await`] → [执行所有 DDL，返回受影响表数；失败时事务回滚]
6. **多方言适配规则**：DDL 生成必须适配 MySQL / PostgreSQL / SQLite / Oracle / MSSQL 五方言的 DDL 语法差异
   - 验收条件：[同一 diff 在 MySQL 生成 `ALTER TABLE users ADD COLUMN email VARCHAR(255)`]；[在 PostgreSQL 生成 `ALTER TABLE users ADD COLUMN email VARCHAR(255)`]；[在 SQLite 生成 `ALTER TABLE users ADD COLUMN email TEXT`（SQLite 无 VARCHAR）]
7. **禁止项**：禁止 Schema Sync 自动执行破坏性 DDL（DROP / TRUNCATE / DELETE）
   - 验收条件：[diff 含 `DroppedColumn`] → [不生成 DDL，返回警告列表，需调用方显式确认]

### **5.4.3 交互流程**

```plantuml
@startuml
!theme plain
actor "DBA" as DBA
participant "sz-orm" as ORM
database "DB" as DB

DBA -> ORM : schema.sync_dry_run(conn)
ORM -> DB : SELECT * FROM INFORMATION_SCHEMA.COLUMNS
DB --> ORM : 现有表结构
ORM -> ORM : diff(实体定义, 现有结构)
ORM --> DBA : Vec<String> DDL 语句（待审查）

DBA -> DBA : 人工审查 DDL

DBA -> ORM : schema.sync(conn)
ORM -> DB : BEGIN TX
ORM -> DB : ALTER TABLE users ADD COLUMN email VARCHAR(255)
DB --> ORM : OK
ORM -> DB : COMMIT
ORM --> DBA : SyncResult { affected_tables: 1 }
@enduml
```

### **5.4.4 异常场景**

1. **DB 连接失败**
   - 触发条件：读取 INFORMATION_SCHEMA 时连接断开
   - 系统行为：返回错误，不生成任何 DDL
   - 用户感知：返回 `Err(DbError::ConnectionError)`
2. **DDL 执行失败**
   - 触发条件：生成的 DDL 执行失败（如列名冲突 / 类型不兼容 / 权限不足）
   - 系统行为：事务回滚，已执行的 DDL 全部撤销
   - 用户感知：返回 `Err(DbError::SqlError)`，含失败 DDL 与底层错误
3. **破坏性 diff 检测**
   - 触发条件：diff 包含 `DroppedColumn` / `DroppedTable`
   - 系统行为：不生成 DDL，返回警告列表
   - 用户感知：返回 `Err(DbError::DestructiveChangeDetected)`，含破坏性变更详情，提示需显式确认
4. **INFORMATION_SCHEMA 不支持**
   - 触发条件：目标 DB 不支持 INFORMATION_SCHEMA（如 SQLite）
   - 系统行为：回退到 `PRAGMA table_info()`（SQLite）等方言特定方案
   - 用户感知：正常返回 diff，无错误

---

## **5.5 ActiveModel 嵌套持久化（P-F-5）**

### **5.5.1 用户故事**

**作为** 应用开发者，**我希望** 一次 `save()` 调用即可持久化整个对象图（父实体 + 子实体集合），自动处理外键关联与执行顺序，**以便** 追平 SeaORM 的嵌套持久化体验，消除逐个 model save 的样板代码。

### **5.5.2 业务规则**

1. **嵌套 save 规则**：`ActiveModel` 必须支持嵌套子实体集合，`save()` 时自动持久化父实体 + 所有子实体
   - 验收条件：[`user.with_orders(vec![order1, order2]).save(conn).await`] → [执行 1 条 INSERT user + 2 条 INSERT order，共 3 条 SQL]
2. **外键自动回填规则**：父实体 INSERT 后，自动将生成的主键回填到子实体的外键字段
   - 验收条件：[User INSERT 后 id=42] → [Order 的 user_id 字段自动设为 42，无需手动设置]
3. **执行顺序规则**：父实体先 INSERT，子实体后 INSERT；删除时子实体先 DELETE，父实体后 DELETE
   - 验收条件：[save 父 + 子] → [SQL 顺序：INSERT parent → INSERT children]；[delete 父 + 子] → [SQL 顺序：DELETE children → DELETE parent]
4. **事务原子性规则**：嵌套 save 在单个事务内执行，任一子实体失败则整体回滚
   - 验收条件：[3 个子实体中第 2 个 INSERT 失败] → [父实体和第 1 个子实体的 INSERT 也回滚，DB 无变更]
5. **脏字段追踪规则**：嵌套 save 仅持久化标记为 `Set` 的字段，`Unchanged` 和 `NotSet` 字段跳过
   - 验收条件：[User 仅 name 字段为 Set，子实体 Order 仅 amount 为 Set] → [生成的 SQL 仅包含 name 和 amount 列]
6. **多级嵌套规则**：支持多级嵌套（User → Order → OrderItem），按拓扑顺序持久化
   - 验收条件：[`user.with_orders(vec![order.with_items(vec![item1, item2])]).save()`] → [SQL 顺序：INSERT user → INSERT order → INSERT item1 → INSERT item2]
7. **禁止项**：禁止嵌套 save 跳过事务（必须事务内执行，确保原子性）
   - 验收条件：[嵌套 save 内部] → [必须 BEGIN TX ... COMMIT/ROLLBACK，不得自动提交]

### **5.5.3 交互流程**

```plantuml
@startuml
!theme plain
actor "应用开发者" as Dev
participant "sz-orm" as ORM
database "DB" as DB

Dev -> ORM : user.with_orders(vec![o1, o2]).save(conn)
ORM -> DB : BEGIN TX
ORM -> DB : INSERT INTO users (name) VALUES (?)  -- name="Alice"
DB --> ORM : last_insert_id = 42
ORM -> ORM : 回填 o1.user_id = 42, o2.user_id = 42
ORM -> DB : INSERT INTO orders (user_id, amount) VALUES (42, 100)
DB --> ORM : OK
ORM -> DB : INSERT INTO orders (user_id, amount) VALUES (42, 200)
DB --> ORM : OK
ORM -> DB : COMMIT
ORM --> Dev : SaveResult { affected_rows: 3, user_id: 42 }
@enduml
```

### **5.5.4 异常场景**

1. **父实体 INSERT 失败**
   - 触发条件：父实体 INSERT 违反约束（如唯一键冲突）
   - 系统行为：事务回滚，不执行子实体 INSERT
   - 用户感知：返回 `Err(DbError::SqlError)`，含父实体 SQL 与错误
2. **子实体 INSERT 失败**
   - 触发条件：子实体 INSERT 违反约束（如外键约束 / 唯一键冲突）
   - 系统行为：事务回滚，父实体 INSERT 也撤销
   - 用户感知：返回 `Err(DbError::SqlError)`，含子实体 SQL 与错误
3. **主键回填失败**
   - 触发条件：父实体 INSERT 后无法获取自增主键（如 DB 不支持 `last_insert_id`）
   - 系统行为：返回错误，事务回滚
   - 用户感知：返回 `Err(DbError::UnsupportedFeature)`，提示 "cannot retrieve last insert id"
4. **嵌套层级过深**
   - 触发条件：嵌套层级超过 10 层（防止循环引用 / 栈溢出）
   - 系统行为：返回错误
   - 用户感知：返回 `Err(DbError::InvalidInput)`，提示 "nested persistence depth exceeds limit (10)"

---

## **5.6 异步流式查询 — Stream 接口（P-F-6）**

### **5.6.1 用户故事**

**作为** 应用开发者，**我希望** 以异步流（`impl Stream`）形式逐行消费大结果集，**以便** 控制内存峰值，避免一次性加载百万行结果导致 OOM。

### **5.6.2 业务规则**

1. **Stream API 规则**：`QueryBuilder` 必须提供 `stream(conn)` 方法，返回 `impl Stream<Item = Result<T, DbError>>`
   - 验收条件：[`query.stream(conn).await`] → [返回 `impl Stream`，逐行 yield `Result<T>`]
2. **逐行产出规则**：Stream 必须逐行从 DB 游标拉取并产出，不得一次性收集为 Vec
   - 验收条件：[查询 100 万行] → [Stream 每次 poll 产出 1 行，峰值内存 ≤ 50 MB]
3. **背压规则**：Stream 消费速度慢于 DB 产出速度时，必须背压（不无限缓存行）
   - 验收条件：[消费方每 100ms poll 一次] → [DB 游标暂停产出，不无限缓存]
4. **错误传播规则**：Stream 迭代中 DB 错误必须作为 `Item = Err(DbError)` yield，不 panic
   - 验收条件：[迭代到第 5000 行时连接断开] → [Stream yield `Err(DbError::ConnectionError)`，后续 poll 返回 None]
5. **资源释放规则**：Stream drop 时必须释放 DB 游标和连接（归还连接池）
   - 验收条件：[`drop(stream)`] → [DB 游标关闭，连接归还连接池，可被其他查询复用]
6. **方言覆盖规则**：Stream API 必须覆盖 MySQL / PostgreSQL / SQLite / Oracle / MSSQL 五方言
   - 验收条件：[5 方言均提供 `stream()` 实现] → [各方言集成测试通过，逐行产出正确]
7. **禁止项**：禁止 Stream 内部偷偷收集全量结果再逐行产出（必须是真游标）
   - 验收条件：[查询 100 万行 + Stream] → [内存峰值 ≤ 50 MB，不得等于全量收集的内存]

### **5.6.3 交互流程**

```plantuml
@startuml
!theme plain
actor "应用开发者" as Dev
participant "sz-orm" as ORM
database "DB" as DB

Dev -> ORM : query.stream(conn).await
ORM -> DB : 执行查询，打开游标
ORM --> Dev : impl Stream<Item = Result<T>>

loop 逐行消费
  Dev -> ORM : stream.next().await
  ORM -> DB : 游标 fetch next row
  DB --> ORM : row
  ORM --> Dev : Some(Ok(row))
end

Dev -> ORM : drop(stream)
ORM -> DB : 关闭游标
ORM -> ORM : 连接归还连接池
@enduml
```

### **5.6.4 异常场景**

1. **游标打开失败**
   - 触发条件：查询 SQL 错误导致游标无法打开
   - 系统行为：`stream()` 返回 `Err(DbError)`
   - 用户感知：`await` 返回 `Err`，含 SQL 错误信息
2. **迭代中连接断开**
   - 触发条件：Stream 迭代过程中 DB 连接断开
   - 系统行为：yield `Err(DbError::ConnectionError)`，Stream 终止
   - 用户感知：`next().await` 返回 `Some(Err(...))`，后续 `next().await` 返回 `None`
3. **未消费完就 drop**
   - 触发条件：消费部分行后 `drop(stream)`
   - 系统行为：关闭 DB 游标，连接归还连接池
   - 用户感知：无错误，连接池可用连接数恢复
4. **Oracle 阻塞迭代桥接**
   - 触发条件：Oracle 同步 API 通过 mpsc 通道桥接，通道满时背压
   - 系统行为：阻塞线程暂停产出，等待异步端消费
   - 用户感知：消费速度慢时自动背压，无内存溢出

---

## **5.7 性能基准对比（P-F-7）**

### **5.7.1 用户故事**

**作为** 性能工程师 / 架构决策者，**我希望** 获得 sz-orm 与 Diesel / SeaORM / SQLx 在相同业务场景下的量化性能对比报告，**以便** 评估 sz-orm 的竞争力，为技术选型提供数据支撑。

### **5.7.2 业务规则**

1. **对比框架规则**：基准对比必须使用 criterion 框架，在相同硬件 / 相同数据 / 相同场景下对比
   - 验收条件：[`cargo bench --bench crud_comparison`] → [输出 sz-orm / Diesel / SeaORM / SQLx 四组吞吐量与延迟数据]
2. **场景覆盖规则**：基准对比必须覆盖至少 5 类核心场景：单行 INSERT / 单行 SELECT / 批量 INSERT（1000 行）/ 关联查询（JOIN）/ 分页查询
   - 验收条件：[基准报告] → [包含 5 类场景 × 4 框架 = 20 组数据]
3. **指标维度规则**：每组对比必须包含吞吐量（ops/s）、平均延迟（μs）、P99 延迟（μs）、内存峰值（MB）
   - 验收条件：[每组数据] → [包含 throughput / avg_latency / p99_latency / peak_memory 四个指标]
4. **公平性规则**：所有框架使用相同连接池配置（pool_size=10）、相同数据库（SQLite 内存，消除网络差异）、相同数据集
   - 验收条件：[基准配置] → [四框架连接池大小、DB、数据集完全一致]
5. **报告生成规则**：基准结果必须生成 Markdown 报告，含对比表格 + 结论
   - 验收条件：[`cargo bench` 完成] → [生成 `docs/benchmark/v2.1.0-comparison.md`，含对比表 + 结论]
6. **回归检测规则**：基准结果与上次结果对比，性能回退超过 10% 时告警
   - 验收条件：[本次吞吐量 < 上次 × 0.9] → [criterion 输出 `regression detected` 警告]
7. **禁止项**：禁止基准测试使用 mock DB（必须真 DB，确保数据真实）
   - 验收条件：[基准测试] → [使用 SQLite 内存真 DB，不得使用 MockConnection]

### **5.7.3 交互流程**

```plantuml
@startuml
!theme plain
actor "性能工程师" as Perf
participant "criterion" as Crit
database "SQLite (内存)" as DB
participant "sz-orm" as Sz
participant "Diesel" as Di
participant "SeaORM" as Sea
participant "SQLx" as SQLx

Perf -> Crit : cargo bench --bench crud_comparison
Crit -> DB : 初始化数据集

loop 5 类场景 × 4 框架
  Crit -> Sz : 执行场景
  Sz -> DB : SQL
  DB --> Sz : 结果
  Sz --> Crit : 耗时 / 内存
  Crit -> Di : 执行相同场景
  Di -> DB : SQL
  DB --> Di : 结果
  Di --> Crit : 耗时 / 内存
  Crit -> Sea : 执行相同场景
  Sea -> DB : SQL
  DB --> Sea : 结果
  Sea --> Crit : 耗时 / 内存
  Crit -> SQLx : 执行相同场景
  SQLx -> DB : SQL
  DB --> SQLx : 结果
  SQLx --> Crit : 耗时 / 内存
end

Crit -> Crit : 生成对比报告
Crit --> Perf : docs/benchmark/v2.1.0-comparison.md
@enduml
```

### **5.7.4 异常场景**

1. **参照框架编译失败**
   - 触发条件：Diesel / SeaORM / SQLx 版本不兼容导致基准 crate 编译失败
   - 系统行为：跳过该框架，报告中标注 "compilation failed"
   - 用户感知：报告含 N-1 框架数据，缺失框架标注原因
2. **数据集初始化失败**
   - 触发条件：基准数据集初始化 SQL 执行失败
   - 系统行为：终止基准测试，返回错误
   - 用户感知：`cargo bench` 返回错误，含初始化 SQL 与错误
3. **内存测量失败**
   - 触发条件：内存峰值统计工具不可用（如 Linux `/proc/self/status` 读取失败）
   - 系统行为：内存指标标注 "N/A"，其他指标正常输出
   - 用户感知：报告内存列标注 "N/A"，吞吐量与延迟正常

---

# **6. 数据约束**

## **6.1 EagerLoadingResult**

Eager Loading 自动组装的嵌套结果对象。

1. **main_records**：主表记录集合，类型 `Vec<M>`，必须非空（空时返回空 Vec）
2. **related_records**：关联表记录集合，类型 `Vec<R>`，可为空（主表记录无关联时为空）
3. **grouping_key**：分组键字段名，必须为关联表的外键列名，用于按主键分组组装
4. **relation_type**：关联类型，枚举值 `HasOne` / `HasMany` / `BelongsTo`，决定组装策略（单值 vs 集合）

## **6.2 RelationDef**

`#[derive(Relation)]` 生成的关联关系定义。

1. **relation_name**：关联名称，类型 `&'static str`，必须非空，用于 `Entity::relation(name)` 查找
2. **from_entity**：源实体表名，类型 `&'static str`，必须为合法 SQL 标识符
3. **to_entity**：目标实体表名，类型 `&'static str`，必须为合法 SQL 标识符
4. **from_key**：源实体键列名，类型 `&'static str`，必须为合法 SQL 标识符
5. **to_key**：目标实体键列名，类型 `&'static str`，必须为合法 SQL 标识符
6. **relation_kind**：关联类型，枚举值 `HasOne` / `HasMany` / `BelongsTo` / `ManyToMany`，决定 JOIN 策略

## **6.3 SchemaDiff**

Schema Sync 比较结果。

1. **added_tables**：新增表列表，类型 `Vec<TableDef>`，需生成 `CREATE TABLE`
2. **dropped_tables**：删除表列表，类型 `Vec<String>`，**禁止自动生成 DDL**，需显式确认
3. **added_columns**：新增列列表，类型 `Vec<(String, ColumnDef)>`（表名, 列定义），生成 `ALTER TABLE ADD COLUMN`
4. **dropped_columns**：删除列列表，类型 `Vec<(String, String)>`（表名, 列名），**禁止自动生成 DDL**，需显式确认
5. **type_changed_columns**：类型变更列列表，类型 `Vec<(String, String, String, String)>`（表名, 列名, 旧类型, 新类型），生成 `ALTER COLUMN TYPE`
6. **renamed_columns**：重命名列列表，类型 `Vec<(String, String, String)>`（表名, 旧名, 新名），生成 `RENAME COLUMN`

## **6.4 NestedActiveModel**

ActiveModel 嵌套持久化对象。

1. **parent**：父实体 ActiveModel，类型 `ActiveModel<M>`，必须实现 `ActiveModelTrait`
2. **children**：子实体集合，类型 `Vec<Box<dyn NestedActiveModelTrait>>`，可为空（无嵌套）
3. **relation**：父子关联关系，类型 `RelationDef`，决定外键回填方向
4. **cascade_delete**：级联删除标志，类型 `bool`，为 `true` 时删除父实体同时删除子实体

## **6.5 QueryStream**

异步流式查询句柄。

1. **underlying_cursor**：底层 DB 游标，类型为方言特定的游标类型，必须支持逐行 fetch
2. **connection**：持有的 DB 连接，Stream drop 时必须归还连接池
3. **buffer_size**：内部缓冲区大小，类型 `usize`，Oracle mpsc 桥接时为 64 行，其他方言为 0（真游标无缓冲）
4. **is_exhausted**：游标是否耗尽，类型 `bool`，为 `true` 后 `next()` 永远返回 `None`

## **6.6 BenchmarkResult**

性能基准对比结果。

1. **scenario_name**：场景名称，枚举值 `SingleInsert` / `SingleSelect` / `BatchInsert1000` / `JoinQuery` / `PaginationQuery`
2. **framework_name**：框架名称，枚举值 `SzOrm` / `Diesel` / `SeaORM` / `SQLx`
3. **throughput_ops**：吞吐量，类型 `f64`（ops/s），必须 > 0
4. **avg_latency_us**：平均延迟，类型 `f64`（微秒），必须 > 0
5. **p99_latency_us**：P99 延迟，类型 `f64`（微秒），必须 > 0 且 ≥ avg_latency
6. **peak_memory_mb**：内存峰值，类型 `f64`（MB），必须 > 0

---

# **7. 功能依赖关系分析**

## **7.1 依赖关系图**

```plantuml
@startuml
!theme plain
skinparam componentStyle rectangle

component "P-F-2\nRelationTrait + join()" as PF2
component "P-F-1\nEager Loading 端到端" as PF1
component "P-F-3\nPartial Models" as PF3
component "P-F-5\nActiveModel 嵌套持久化" as PF5
component "P-F-4\nSchema Sync" as PF4
component "P-F-6\nStream API" as PF6
component "P-F-7\n性能基准对比" as PF7

PF2 --> PF1 : P-F-1 依赖 P-F-2 的\nRelationTrait 获取关联定义
PF3 --> PF1 : P-F-1 使用 Partial Models\n避免 SELECT *
PF2 --> PF5 : P-F-5 依赖 P-F-2 的\nRelationDef 回填外键
PF1 --> PF7 : P-F-7 基准测试\n覆盖 Eager Loading 场景
PF3 --> PF7 : P-F-7 基准测试\n覆盖 Partial Models 场景
PF6 --> PF7 : P-F-7 基准测试\n覆盖 Stream 场景

note right of PF4 : P-F-4 Schema Sync 独立，\n不依赖其他功能
note right of PF6 : P-F-6 Stream API 独立，\n底层游标 v2.0.0 已实现

@enduml
```

## **7.2 依赖关系说明**

| 依赖 | 说明 | 影响 |
|------|------|------|
| P-F-1 → P-F-2 | Eager Loading 需要 `RelationTrait` 获取关联定义（外键、关联表） | P-F-2 必须先于 P-F-1 完成或并行开发 |
| P-F-1 → P-F-3 | Eager Loading 生成的 SQL 应使用 Partial Models 避免 SELECT * | P-F-3 可先完成或 P-F-1 临时使用全字段 |
| P-F-5 → P-F-2 | 嵌套持久化需要 `RelationDef` 确定外键回填方向 | P-F-2 必须先于 P-F-5 完成 |
| P-F-7 → P-F-1/3/6 | 基准测试覆盖新功能场景 | P-F-7 应最后完成，覆盖所有新功能 |
| P-F-4 独立 | Schema Sync 不依赖其他功能 | 可独立并行开发 |
| P-F-6 独立 | Stream API 底层游标 v2.0.0 已实现，仅需高层封装 | 可独立并行开发 |

---

# **8. 优先级排序建议**

## **8.1 建议实施顺序**

| 顺序 | 任务 | 优先级 | 理由 | 预估工时 |
|------|------|--------|------|----------|
| 1 | **P-F-2** RelationTrait + join() | 🟠 中 | 基础设施，P-F-1 和 P-F-5 的依赖项；追平 SeaORM 关联查询 API | 5-7 天 |
| 2 | **P-F-3** Partial Models | 🟡 低 | P-F-1 的依赖项；实现简单（QueryBuilder 扩展），收益明确 | 2-3 天 |
| 3 | **P-F-1** Eager Loading 端到端 | 🟠 中 | 依赖 P-F-2 + P-F-3；追平 SeaORM `find_with_related().all()`，消除 N+1 | 5-7 天 |
| 4 | **P-F-5** ActiveModel 嵌套持久化 | 🟡 低 | 依赖 P-F-2；追平 SeaORM 一次 save 整个对象图 | 4-5 天 |
| 5 | **P-F-6** Stream API | 🟡 低 | 独立，底层已实现；高层封装 + 五方言适配 | 3-4 天 |
| 6 | **P-F-4** Schema Sync | 🟡 低 | 独立，复杂度高（diff 算法 + 五方言 DDL 适配）；可延后 | 7-10 天 |
| 7 | **P-F-7** 性能基准对比 | 🟠 中 | 最后完成，覆盖所有新功能场景；引入 Diesel/SeaORM/SQLx dev-dependency | 3-5 天 |

## **8.2 关键路径**

```
P-F-2 (5-7d) → P-F-3 (2-3d) → P-F-1 (5-7d) → P-F-5 (4-5d) → P-F-7 (3-5d)
                                                    ↑
P-F-6 (3-4d) ──────────────────────────────────────┘
P-F-4 (7-10d) ─────────────────────────────────────┘
```

**关键路径总工时**：P-F-2 + P-F-3 + P-F-1 + P-F-5 + P-F-7 = **19-27 天**

**并行优化**：P-F-4 和 P-F-6 可与关键路径并行，总工期约 **3-4 周**。

---

# **9. 风险和约束**

## **9.1 技术风险**

| # | 风险 | 等级 | 影响 | 缓解措施 |
|---|------|------|------|----------|
| R-1 | `#[derive(Relation)]` 宏改复杂度高 | 🟠 中 | P-F-2 延期影响 P-F-1/P-F-5 | 先实现最小可用 RelationTrait（仅 HasMany/HasOne），迭代扩展 |
| R-2 | 五方言 DDL 语法差异大（Schema Sync） | 🟠 中 | P-F-4 工时超预期 | 优先 MySQL+PostgreSQL+SQLite，Oracle/MSSQL 标注 experimental |
| R-3 | Eager Loading 多级嵌套组装复杂 | 🟡 低 | P-F-1 多级关联场景受限 | v2.1.0 仅支持 2 级嵌套，3+ 级标注 TODO v2.2.0 |
| R-4 | 基准测试引入 Diesel/SeaORM/SQLx 依赖冲突 | 🟡 低 | P-F-7 编译失败 | 使用 dev-dependency + 独立 bench crate 隔离 |
| R-5 | Stream API Oracle mpsc 桥接背压 | 🟡 低 | P-F-6 Oracle 场景内存不可控 | 复用 v2.0.0 已验证的 mpsc 通道方案（容量 64） |

## **9.2 项目约束**

| # | 约束 | 来源 | 影响 |
|---|------|------|------|
| C-1 | Rust 2021 Edition，rust-version = "1.81" | AGENTS.md | 不得使用 1.82+ 特性 |
| C-2 | unsafe 零容忍 | AGENTS.md | 新增代码 0 处 unsafe |
| C-3 | 禁止占位实现 | AGENTS.md | 0 处 todo!/unimplemented!/unreachable! |
| C-4 | WHERE 条件必须参数化 | AGENTS.md | 新增 API 禁止字符串拼接 WHERE |
| C-5 | 默认禁止 SELECT * | AGENTS.md | Eager Loading / join 生成 SQL 必须显式列名 |
| C-6 | N+1 检测自动拦截 | AGENTS.md | Eager Loading 完成后 N1QueryDetector 无告警 |
| C-7 | 10 道门禁必过 | AGENTS.md | fmt/check/clippy/test/doc/audit/integration/占位/SQL注入/feature |
| C-8 | ADR-0001 合规 | AGENTS.md | 仅修改 sz-orm 仓库内文件 |
| C-9 | API 向后兼容 | v2.0.0 承诺 | v2.1.0 无 Breaking Change |
| C-10 | 五方言覆盖 | 项目约束 | MySQL/PG/SQLite/Oracle/MSSQL 适配 |

## **9.3 验证约束**

| # | 约束 | 验证方法 |
|---|------|----------|
| V-1 | 每个功能有单元测试 + 集成测试 | `cargo test --workspace` 全通过 |
| V-2 | 集成测试覆盖 MySQL + SQLite（本机可用） | `cargo test -- --ignored` |
| V-3 | Oracle 集成测试通过 | `cargo test --test integration_oracle -- --ignored` |
| V-4 | clippy 零警告 | `cargo clippy --workspace --all-targets -- -D warnings` |
| V-5 | 无占位实现 | `grep -rn 'todo!\|unimplemented!\|unreachable!' --include='*.rs'`（生产代码 0 处） |
| V-6 | 无 SQL 注入 | `scripts/check-sql-injection.ps1` 通过 |
| V-7 | 基准报告生成 | `docs/benchmark/v2.1.0-comparison.md` 存在且含 20 组数据 |
| V-8 | 审计证据 | 每条验收结论附 `file:line` 证据 + `cargo test` 输出 |

---

# **10. 验收标准汇总（EARS 格式）**

## **10.1 P-F-1 Eager Loading 端到端**

- **Given** 一个 User HasMany Order 的关联定义和 100 个 User 各 3 个 Order 的数据集
- **When** 调用 `eager_load_all::<User, Order>(conn).await`
- **Then** 返回 `Vec<(User, Vec<Order>)>`，长度 100，每个 User 的 orders 长度 3，且总共执行 2 条 SQL（非 103 条）

- **Given** 主表查询返回 0 行
- **When** 调用 `eager_load_all::<User, Order>(conn).await`
- **Then** 返回 `Ok(Vec::new())`，不执行关联表查询

## **10.2 P-F-2 RelationTrait + join() 链式**

- **Given** `#[derive(Relation)] struct User` with `#[relation(has_many = "Order")]`
- **When** 编译并调用 `User::find().join(Order::relation()).build()`
- **Then** 生成 `SELECT ... FROM users JOIN orders ON users.id = orders.user_id`，且 `User` 实现 `RelationTrait`

- **Given** 尝试 `query.join("raw_sql")`（字符串参数）
- **When** 编译
- **Then** 编译错误（类型不匹配，`join()` 仅接受 `impl RelationTrait`）

## **10.3 P-F-3 Partial Models**

- **Given** User 有 id/name/email/created_at 四个字段
- **When** 调用 `User::find().select_only().column(UserColumn::Id).column(UserColumn::Name).build()`
- **Then** 生成 `SELECT id, name FROM users`，结果集仅含 id 和 name

- **Given** `select_only()` 后未添加任何字段
- **When** 调用 `.build()`
- **Then** 返回 `Err(DbError::InvalidInput)`，提示 "select_only requires at least one column"

## **10.4 P-F-4 Schema Sync**

- **Given** 实体 User 有字段 id/name/email，DB 表 users 有 id/name（缺 email）
- **When** 调用 `schema.sync_dry_run(conn).await`
- **Then** 返回 `Vec<String>` 含 `ALTER TABLE users ADD COLUMN email VARCHAR(255)`，DB 结构不变

- **Given** 实体 User 删除字段 legacy_col，DB 表有 legacy_col
- **When** 调用 `schema.sync(conn).await`
- **Then** 返回 `Err(DbError::DestructiveChangeDetected)`，不生成 DROP COLUMN DDL

## **10.5 P-F-5 ActiveModel 嵌套持久化**

- **Given** User ActiveModel with 2 Order ActiveModel 子实体
- **When** 调用 `user.with_orders(vec![o1, o2]).save(conn).await`
- **Then** 执行 3 条 INSERT（1 user + 2 orders），Order 的 user_id 自动回填为 User 的 last_insert_id，事务提交

- **Given** 3 个子实体中第 2 个 INSERT 失败（唯一键冲突）
- **When** 调用 `user.with_orders(vec![o1, o2, o3]).save(conn).await`
- **Then** 事务回滚，User 和 o1 的 INSERT 也撤销，返回 `Err(DbError::SqlError)`

## **10.6 P-F-6 Stream API**

- **Given** 查询返回 100 万行结果集
- **When** 调用 `query.stream(conn).await` 并逐行消费
- **Then** 返回 `impl Stream`，逐行 yield `Result<T>`，峰值内存 ≤ 50 MB

- **Given** Stream 消费到第 5000 行时 DB 连接断开
- **When** 继续 `stream.next().await`
- **Then** yield `Some(Err(DbError::ConnectionError))`，后续 `next().await` 返回 `None`

## **10.7 P-F-7 性能基准对比**

- **Given** SQLite 内存 DB + 1000 行测试数据
- **When** 执行 `cargo bench --bench crud_comparison`
- **Then** 生成 `docs/benchmark/v2.1.0-comparison.md`，含 5 场景 × 4 框架 = 20 组数据，每组含 throughput/avg_latency/p99_latency/peak_memory

- **Given** sz-orm 单行 SELECT 吞吐量为 X ops/s
- **When** 对比 SQLx 单行 SELECT 吞吐量
- **Then** sz-orm 吞吐量 ≥ SQLx 吞吐量 × 0.67（即延迟不超过 SQLx 1.5 倍）

---

> **文档版本**：v1.0（v2.1.0 需求规格初版）
> **生成日期**：2026-08-06
> **生成方法**：基于 v2.0.0 进展总结 + 竞品深度对比 + 安全审计基线 + 源码验证
> **约束遵循**：spec_template.md 格式 + EARS 验收标准 + AGENTS.md 工程规范
> **下一步**：用户审查 → 确认/修改 → 移交 spec-design-agent 生成 design.md