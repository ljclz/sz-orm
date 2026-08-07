# sz-orm v2.2.0 技术设计文档

> **版本**：v2.2.0
> **基线**：v2.1.0（43 包 @ 2.1.0 已发布 crates.io，4,993 测试通过，sz-pay 5,139 试点验证无回归）
> **生成日期**：2026-08-06
> **文档目的**：将 v2.2.0 八项需求（spec.md，38 条 EARS）转化为可落地的架构设计与接口契约
> **依据**：
> - `docs/spec/v2.2.0/spec.md`（需求规格，8 项需求 / 38 条 EARS）
> - `docs/spec/v2.1.0/design.md`（v2.1.0 技术设计，增量基线）
> - `docs/assessment/2026-08-06-v2-progress-and-roadmap.md`（v2.1.0 进展 + v2.2.0 路线图 §4.1）
> - `docs/assessment/2026-08-04-deep-comparison.md`（竞品深度对比）
> - 源码现状调研：`packages/sz-orm-sqlx/src/any_driver.rs`、`packages/sz-orm-core/src/{dialect,pool,eager_loader,schema_sync,stream_api,nested_active_model,query,partial_model,relation_trait}.rs`、`packages/sz-orm-{oracle,mssql}/src/lib.rs`

---

# 一、需求与存量功能关系分析

## 1.1 需求功能与存量功能对比

### 1.1.1 已实现功能

下表列出 v2.2.0 需求中与存量代码高度匹配（匹配度 ≥ 75%）的功能。这些功能可作为增量改造的基础，无需从零实现。

| 需求功能 | 存量功能 | 代码位置 | 匹配度 |
|---------|---------|---------|--------|
| A-1 AnyBackend 枚举（3→5 变体） | `AnyBackend` 枚举含 MySql/Postgres/Sqlite 3 变体 | `packages/sz-orm-sqlx/src/any_driver.rs:43` | 75% |
| A-1 DSN scheme 识别（3→5 scheme） | `AnyBackend::from_dsn` 识别 mysql/mariadb/postgres/postgresql/sqlite 5 种 scheme | `packages/sz-orm-sqlx/src/any_driver.rs:60` | 75% |
| A-1 Oracle ConnectionFactory | `OracleConnectionFactory` 实现 `ConnectionFactory` trait | `packages/sz-orm-oracle/src/lib.rs:634` | 100% |
| A-1 MSSQL ConnectionFactory | `MssqlConnectionFactory` 实现 `ConnectionFactory` trait | `packages/sz-orm-mssql/src/lib.rs:445` | 100% |
| A-1 AnyPool 后端无关抽象 | `AnyPool` 持有 `backend: AnyBackend` + `factory: Arc<dyn ConnectionFactory>` | `packages/sz-orm-sqlx/src/any_driver.rs:86` | 75% |
| A-1 AnyConnection 后端无关连接 | `AnyConnection` 持有 `backend` + `inner: Box<dyn Connection>`，实现 `Connection` trait | `packages/sz-orm-sqlx/src/any_driver.rs:138` | 100% |
| A-2 Dialect trait | `Dialect` trait 含 quote/build_pagination/build_create_table/build_alter_table 等 20+ 方法 | `packages/sz-orm-core/src/dialect.rs:23` | 100% |
| A-2 OracleDialect | `impl Dialect for OracleDialect` 完整实现 | `packages/sz-orm-core/src/dialect.rs:939` | 100% |
| A-2 SqlServerDialect | `impl Dialect for SqlServerDialect` 完整实现 | `packages/sz-orm-core/src/dialect.rs:1185` | 100% |
| A-3 Pool 完整连接池 | `Pool` 含 idle ArrayQueue + AtomicU32 + Notify + 断路器 + 限流器 + 6 监控计数器 | `packages/sz-orm-core/src/pool.rs:712` | 100% |
| A-3 ConnectionFactory trait | `ConnectionFactory: Send + Sync` 含 `async fn create()` | `packages/sz-orm-core/src/pool.rs:703` | 100% |
| B-1 EagerLoader 执行器 | `EagerLoader` 含 `relation` + `children: Vec<ChildLoadConfig>`，`load_many` 双查询策略 | `packages/sz-orm-core/src/eager_loader.rs:44` | 75% |
| B-1 EagerResult 类型 | `EagerResult = (HashMap<String, Value>, Vec<HashMap<String, Value>>)` | `packages/sz-orm-core/src/eager_loader.rs:34` | 75% |
| B-1 RelationDef 关联定义 | `RelationDef` 携带 name/from_entity/to_entity/from_key/to_key/kind | `packages/sz-orm-core/src/relation_trait.rs:84` | 100% |
| B-2 SchemaDiff 结构 | `SchemaDiff` 含 added/dropped_tables/columns、type_changed、renamed_columns 6 字段 | `packages/sz-orm-core/src/schema_sync.rs:100` | 75% |
| B-2 diff 纯函数 | `diff(entity, db) -> SchemaDiff` 比较实体与 DB 结构 | `packages/sz-orm-core/src/schema_sync.rs:155` | 75% |
| B-2 SchemaSync 协调器 | `SchemaSync` 含 `sync_dry_run`/`sync`/`diff_against` 方法 | `packages/sz-orm-core/src/schema_sync.rs:483` | 75% |
| B-2 DdlGenerator trait | `DdlGenerator` trait + 5 方言实现（MySql/PG/SQLite/Oracle/MSSQL） | `packages/sz-orm-core/src/schema_sync.rs:230` | 100% |
| B-3 QueryBuilder select 字段 | `QueryBuilder.select_columns: Vec<String>` + `select_mode: SelectMode` | `packages/sz-orm-core/src/query.rs:38` | 100% |
| B-3 select_only API | `select_only()`/`column()`/`columns()`/`column_as()` 已实现 | `packages/sz-orm-core/src/query.rs:1088` | 100% |
| B-4 StreamApiExt trait | `StreamApiExt<M>` 含 `stream_buffered`（无界兼容版） | `packages/sz-orm-core/src/stream_api.rs:50` | 75% |
| B-4 stream_cursor 真游标 | `stream_cursor` 委托 `conn.query_stream_cursor` | `packages/sz-orm-core/src/stream_api.rs:89` | 75% |
| B-4 stream_cursor_paged 分批 | `stream_cursor_paged` 基于 unfold 分批拉取（已有背压机制） | `packages/sz-orm-core/src/cursor_stream.rs:79` | 75% |
| B-5 NestedActiveModel 包装器 | `NestedActiveModel<M>` 含 parent + children + relation + cascade_delete: bool | `packages/sz-orm-core/src/nested_active_model.rs:134` | 75% |
| B-5 nested_delete 函数 | `nested_delete` 事务内删除子先父后，cascade 递归 | `packages/sz-orm-core/src/nested_active_model.rs:419` | 75% |
| B-5 ChildEntity 子实体 | `ChildEntity` 含 table + fields + children + relation，支持多级嵌套 | `packages/sz-orm-core/src/nested_active_model.rs:53` | 100% |

### 1.1.2 需要扩展的功能

下表列出需求与存量代码部分匹配，需在现有基础上改造的功能。

| 需求功能 | 存量功能 | 差异说明 | 扩展方向 |
|---------|---------|---------|---------|
| A-1 AnyBackend 枚举扩展 | `AnyBackend` 仅 3 变体（MySql/Postgres/Sqlite） | 存量：枚举无 Oracle/Mssql 变体。需求：扩展至 5 变体，`from_dsn` 识别 `oracle://`/`mssql://`/`sqlserver://`，`name()` 返回对应名称 | 新增 `Oracle`/`Mssql` 变体；`from_dsn` 追加 3 个 scheme 分支；`name()` 追加 2 个 match 臂。未知 scheme 错误信息列出 5 种支持 scheme |
| A-1 AnyPool::connect 分派扩展 | `connect` 仅 3 分支（MySql/Postgres/Sqlite） | 存量：match backend 仅 3 臂，调用 SqlxMySqlConnectionFactory 等。需求：追加 Oracle/Mssql 分支，调用 OracleConnectionFactory/MssqlConnectionFactory | `connect` 追加 2 个 match 臂；Oracle 分支 `OraclePoolHandle::connect(dsn)` + `OracleConnectionFactory::new`；MSSQL 分支同理。需引入 sz-orm-oracle/sz-orm-mssql 依赖（feature gate） |
| A-2 AnyPool 自动选择 Dialect | AnyPool 持有 backend 但未绑定 Dialect | 存量：AnyPool 无 `dialect()` 方法，SQL 生成需调用方手动选 Dialect。需求：AnyPool 根据 backend 自动返回对应 Dialect | AnyPool 新增 `dialect() -> Box<dyn Dialect>` 方法，match backend 返回 MySqlDialect/PostgreSqlDialect/SqliteDialect/OracleDialect/SqlServerDialect |
| A-2 Dialect 与 AnyConnection 集成验证 | OracleDialect/SqlServerDialect 已实现但未与 AnyPool 端到端验证 | 存量：Dialect 实现独立存在，未验证通过 AnyPool 执行 Oracle/MSSQL 时 SQL 生成正确。需求：集成测试覆盖 5 后端分页/DDL/upsert | 新增集成测试：通过 AnyPool 创建 Oracle/MSSQL 连接，执行 OracleDialect/SqlServerDialect 生成的分页 SQL，验证结果正确 |
| B-1 EagerLoader 多级嵌套 | `with()` 限 2 级（`children: Vec<ChildLoadConfig>` 仅 1 层） | 存量：`ChildLoadConfig` 仅含 `relation`，无子级 children 字段，`load_many` 递归限 2 级。需求：无限级 + 循环检测 | `ChildLoadConfig` 改为递归结构（含 `children: Vec<ChildLoadConfig>`）；`load_many` 递归执行无深度限制；新增 `CyclePolicy` 枚举 + 循环检测（访问集合 + 深度计数） |
| B-1 EagerResult 嵌套结构 | `EagerResult = (HashMap, Vec<HashMap>)` 仅 2 级 | 存量：结果类型为扁平 `(main, Vec<related>)`。需求：多级嵌套 `Vec<(main, Vec<(l2, Vec<l3>)>)>` | 新增 `NestedEagerResult` 递归枚举类型（`Leaf(row)` / `Node(row, Vec<NestedEagerResult>)`），`load_many` 返回 `Vec<NestedEagerResult>` |
| B-2 SchemaDiff 重命名检测 | `renamed_columns` 字段存在但 `diff_columns` 未填充 | 存量：`diff_columns` 仅检测 added/dropped/type_changed，`renamed_columns` 始终为空。需求：检测"一列删除 + 一列新增 + 类型兼容 + 名称相似"识别为重命名 | `diff_columns` 新增重命名检测逻辑：对 dropped_columns 与 added_columns 做笛卡尔积，Levenshtein 距离 ≤ 2 或编辑距离/长度 ≤ 0.3 且类型兼容 → 移入 renamed_columns，从 dropped/added 移除 |
| B-2 destructive_sync 方法 | `sync_dry_run` 检测破坏性变更返回 Err | 存量：`sync_dry_run` 遇 dropped_tables/dropped_columns 返回 `Err(DestructiveChangeDetected)`，无执行路径。需求：`destructive_sync(Confirm::Yes)` 显式执行破坏性 DDL | SchemaSync 新增 `destructive_sync(conn, confirm, hooks)` 方法：跳过破坏性检查，生成 DROP/RENAME DDL，执行前调用数据迁移钩子，执行后记录审计日志，事务内执行 |
| B-3 select_exclude API | 仅有 `select_only`（包含模式） | 存量：`select_only` 进入 Partial 模式后逐列添加。需求：`select_exclude(&["avatar"])` 排除指定列，互补 API | QueryBuilder 新增 `select_exclude(fields: &[&str])` 方法：读取实体全部列名（通过 `M::column_names()` 或元数据），减去排除列，设置 select_columns。校验：排除列存在、不排除全部列 |
| B-4 stream_with_backpressure | `stream_buffered` 无界、`stream_cursor` 无背压参数 | 存量：`stream_buffered` 全量收集后逐行 yield（内存无界）；`stream_cursor` 委托真游标但无有界缓冲。需求：有界缓冲区 + 背压阻塞 | StreamApiExt 新增 `stream_with_backpressure(buffer_size: usize)` 方法：使用 `tokio::sync::mpsc::channel(buffer_size)` 桥接生产者（游标 fetch）与消费者，缓冲区满时生产者 `send().await` 阻塞。buffer_size=0 返回 Err |
| B-5 CascadeStrategy 枚举 | `cascade_delete: bool` 仅支持 CASCADE 语义 | 存量：`cascade_delete` 为 bool，true=级联删除子实体，false=仅删父。需求：4 种策略（Restrict/Cascade/SetNull/SetDefault） | `cascade_delete: bool` 改为 `cascade_strategy: CascadeStrategy`（默认 Cascade 保持兼容）；`nested_delete` 新增 `strategy` 参数；`do_nested_delete` 按 strategy 分支：Restrict 先 COUNT 子实体→有则 Err；SetNull→UPDATE SET fk=NULL；SetDefault→UPDATE SET fk=default |
| B-5 nested_delete 策略参数 | `nested_delete(conn, nested)` 无策略参数 | 存量：策略由 `nested.cascade_delete` 字段决定。需求：`nested_delete(conn, nested, strategy)` 参数优先级高于字段 | 新增 `nested_delete_with_strategy(conn, nested, strategy)` 函数；保留 `nested_delete(conn, nested)` 向后兼容（使用 nested.cascade_strategy 字段） |

### 1.1.3 需要新增的功能或接口

下表列出需求在存量代码中完全没有对应实现的部分，需新建模块/类型/trait。

#### 模块：`unified_pool`（A-3 新增）

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|--------|------|------|---------|------|
| `UnifiedPool` 类型 | DSN 或 (AnyBackend, PoolConfig) | `UnifiedPool` | 从 DSN 识别后端 → 选择 ConnectionFactory → 构造完整 Pool（复用 sz-orm-core::Pool） | `AnyBackend`、`Pool`、`ConnectionFactory`、`Dialect` |
| `UnifiedPool::connect` | DSN | `Result<UnifiedPool>` | `AnyBackend::from_dsn` → 选 factory → `Pool::new(config, factory)` | `AnyBackend`、`Pool` |
| `UnifiedPool::acquire` | `&self` | `Result<PooledConnection>` | 委托内部 `Pool::acquire()` | `Pool` |
| `UnifiedPool::dialect` | `&self` | `Box<dyn Dialect>` | 根据 `backend` 返回对应 Dialect | `Dialect`、`AnyBackend` |
| `UnifiedPool::backend` | `&self` | `AnyBackend` | 返回绑定的后端类型 | `AnyBackend` |
| `UnifiedPool::from_pool` | `Pool` + `AnyBackend` | `UnifiedPool` | 从已有 Pool 包装（迁移路径） | `Pool` |

#### 模块：`cycle_detection`（B-1 新增）

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|--------|------|------|---------|------|
| `CyclePolicy` 枚举 | Error/Truncate/AllowWithDepthLimit(n) | `CyclePolicy` | 循环检测策略配置 | 无 |
| `CycleDetector` 类型 | `CyclePolicy` | `CycleDetector` | 访问集合 + 深度计数，检测关联链循环 | `CyclePolicy` |
| `CycleDetector::check` | entity 类型名 + 当前深度 | `Result<bool, DbError>` | 检测是否已访问该 entity 类型；Error→返回 Err，Truncate→返回 false（终止），AllowWithDepthLimit→深度超限返回 false | 无 |

#### 模块：`destructive_sync`（B-2 新增）

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|--------|------|------|---------|------|
| `Confirm` 枚举 | Yes/No | `Confirm` | 破坏性操作显式确认 | 无 |
| `DataMigrationHook` trait | hook 上下文 | `Result<()>` | 数据迁移钩子接口（before_drop_column/before_rename_column/after_drop_column） | `Connection` |
| `DestructiveSyncResult` 结构 | 执行的 DDL + 钩子结果 | `DestructiveSyncResult` | 破坏性同步结果值对象 | `SyncResult` |
| `SchemaSync::destructive_sync` | conn + Confirm + hooks | `DestructiveSyncResult` | 跳过破坏性检查 → 生成 DROP/RENAME DDL → 执行前调钩子 → 执行 DDL → 审计日志 → 事务原子性 | `SchemaSync`、`DataMigrationHook`、`sz-orm-audit` |

#### 模块：`backpressure_stream`（B-4 新增）

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|--------|------|------|---------|------|
| `BackpressureStream` 类型 | `mpsc::Receiver` + 游标句柄 | `impl Stream` | 有界缓冲流式迭代器，drop 时关闭游标 | `tokio::sync::mpsc`、`Connection::query_stream_cursor` |
| `stream_with_backpressure` 方法 | buffer_size | `impl Stream<Item = Result<T>>` | 创建 mpsc::channel(buffer_size) → spawn 生产者任务（游标 fetch → send 阻塞）→ 返回 Receiver 转 Stream | `BackpressureStream` |

#### 模块：`cascade_strategy`（B-5 新增）

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|--------|------|------|---------|------|
| `CascadeStrategy` 枚举 | Restrict/Cascade/SetNull/SetDefault | `CascadeStrategy` | 级联删除策略 | 无 |
| `nested_delete_with_strategy` 函数 | conn + nested + strategy | `Result<u64>` | 按 strategy 分支执行：Restrict→COUNT 子实体→有则 Err；Cascade→递归删除；SetNull→UPDATE SET fk=NULL；SetDefault→UPDATE SET fk=default | `NestedActiveModel`、`Connection` 事务 API |

## 1.2 存量功能详细分析

### 1.2.1 `any_driver.rs` AnyPool/AnyBackend（A-1 改造基础）

**接口契约**：
- `AnyBackend::from_dsn(dsn: &str) -> Result<Self, DbError>`：DSN scheme 白名单校验，返回对应后端枚举
- `AnyPool::connect(dsn: &str) -> Result<Self, DbError>`：从 DSN 创建 AnyPool，内部 match backend 选择 ConnectionFactory
- `AnyPool::create() -> Result<AnyConnection, DbError>`：从工厂创建新连接（无连接复用）
- `AnyConnection` 实现 `Connection` trait（execute/query/begin_transaction/commit/rollback/ping/close）

**业务规则**：
- DSN scheme 白名单：仅 `mysql`/`mariadb`/`postgres`/`postgresql`/`sqlite` 5 种，未知 scheme 返回 `Err(ConnectionRefused)` 含支持列表
- `AnyPool` 无连接复用语义：每次 `create()` 调用 `factory.create()` 创建新连接，无 idle 队列
- `AnyConnection` 是 `Box<dyn Connection>` 的后端无关包装，所有方法委托 `inner`

**约束**：
- `AnyBackend` 为 `#[derive(Debug, Clone, Copy, PartialEq, Eq)]`，可 Copy（枚举变体无数据）
- `AnyPool` 持有 `Arc<dyn ConnectionFactory>`，可 clone（Arc 引用计数）
- `ConnectionFactory` trait 无 `#[async_trait]`（手动解糖 async，单一生命周期 `'a`），但 Oracle/MSSQL 的实现使用了 `#[async_trait]`（需确认兼容性）

### 1.2.2 `pool.rs` Pool 完整连接池（A-3 改造基础）

**接口契约**：
- `Pool::new(config: PoolConfig, factory: Arc<dyn ConnectionFactory>) -> Result<Self, PoolError>`：创建连接池
- `Pool::acquire() -> Result<PooledConnection, PoolError>`：获取连接（复用 idle 或新建，池满阻塞等待 notify）
- `Pool::release(conn)`：归还连接到 idle 队列
- `Pool::resize(new_max)` / `close_all()` / `metrics() -> PoolMetrics`：动态管理
- `PooledConnection` 实现 `Deref<Target = dyn Connection>` + `Drop` 自动归还

**业务规则**：
- 连接复用：idle 队列（`ArrayQueue<PooledConnection>`）非空时直接 pop，否则新建（`total_count < max_size`）
- 池满阻塞：`total_count >= max_size` 时 `notify.notified().await` 等待 release 唤醒
- 断路器（feature gate）：连续失败超阈值→跳闸拒绝 acquire，reset_timeout 后 HalfOpen
- 限流器（feature gate）：acquire 前 `try_acquire(key)`，被拒返回 `RateLimited`
- 监控：6 个 `AtomicU64` 计数器（acquire_count/failed/wait_time_ns/release/created/closed）

**约束**：
- 所有字段 `Arc` 或内部含 `Arc`，`Pool` 可低成本 clone（共享状态）
- 无锁：`ArrayQueue`（crossbeam MPMC）+ `AtomicU32`/`AtomicU64`，无 Mutex
- `PoolConfig` 需 `validate()`（max_size > 0、timeout > 0 等）

### 1.2.3 `dialect.rs` Dialect trait（A-2 集成基础）

**接口契约**：
- `Dialect: Send + Sync` trait，含 20+ 方法：`quote`/`escape_string`/`supports_returning`/`build_pagination`/`json_type`/`json_extract`/`full_text_search`/`auto_increment_keyword`/`last_insert_id_sql`/`build_create_table`/`build_alter_table`/`build_drop_table`/`build_upsert_conflict` 等
- `clone_box() -> Box<dyn Dialect>`：trait object 克隆
- `db_type() -> DbType`：返回数据库类型枚举

**业务规则**：
- 8 种方言实现：MySqlDialect/PostgreSqlDialect/SqliteDialect/OracleDialect/SqlServerDialect/ClickHouseDialect/DuckDBDialect/Db2Dialect
- 分页 SQL 差异：MySQL `LIMIT offset, count`、PG `LIMIT count OFFSET offset`、Oracle `OFFSET ... FETCH NEXT ... ROWS ONLY`（或 ROWNUM）、MSSQL `OFFSET ... FETCH NEXT ... ROWS ONLY`
- `supports_returning`：PG/Oracle true，MySQL/MSSQL false（MSSQL 用 OUTPUT），SQLite true（3.35+）
- `last_insert_id_sql`：MySQL `SELECT LAST_INSERT_ID()`、PG None（用 RETURNING）、Oracle None（用 RETURNING INTO）

**约束**：
- `Dialect` trait object 需 `clone_box`（非 Clone，因 trait object 不自动 Clone）
- `quote_checked` 默认实现：先 `validate_identifier` 校验，再 `quote`
- 各方言 `build_create_table`/`build_alter_table` 需处理自增列、类型映射、约束语法差异

### 1.2.4 `eager_loader.rs` EagerLoader（B-1 改造基础）

**接口契约**：
- `EagerLoader::new(relation: RelationDef) -> Self`：创建加载器
- `EagerLoader::with(relation: RelationDef) -> Self`：添加子级关联（限 2 级）
- `EagerLoader::load_many(conn, main_sql) -> Result<Vec<EagerResult>, DbError>`：执行双查询策略
- `eager_load_all(conn, main_sql, relation) -> Result<Vec<EagerResult>>`：自由函数快捷入口

**业务规则**：
- 双查询策略（HasMany/ManyToMany）：① 执行主表 SQL → ② 提取主键列表 → ③ `WHERE fk IN (?, ...)` 批量查询 → ④ 按外键分组组装
- JOIN 策略（HasOne/BelongsTo）：单条 SQL，结果集拆分组装
- Oracle IN 列表 >1000 时分批查询（Oracle 限制）
- 多级递归：`load_children` 递归加载子级，但 `ChildLoadConfig` 无 children 字段，限 2 级

**约束**：
- `EagerResult = (HashMap<String, Value>, Vec<HashMap<String, Value>>)`：扁平 2 级，无法表达 3+ 级嵌套
- `ChildLoadConfig` 仅含 `relation`，无递归 children，无法配置 3+ 级
- 无循环检测：`User → Order → User` 会无限递归（栈溢出风险）

### 1.2.5 `schema_sync.rs` SchemaSync（B-2 改造基础）

**接口契约**：
- `SchemaSync::new(entity_tables) -> Self`：创建协调器（默认 MySqlDdlGenerator）
- `SchemaSync::with_generator(entity_tables, generator) -> Self`：指定 DDL 生成器
- `sync_dry_run(conn) -> Result<Vec<String>>`：introspect → diff → 检查破坏性 → 生成 DDL
- `sync(conn) -> Result<SyncResult>`：事务内执行 DDL
- `diff_against(db_tables) -> SchemaDiff`：纯函数 diff

**业务规则**：
- `sync_dry_run` 检测到 `dropped_tables`/`dropped_columns` 非空时返回 `Err(DestructiveChangeDetected)`，不生成破坏性 DDL
- `sync` 事务内逐条执行 DDL，任一失败 rollback
- `diff` 纯函数：比较 entity 与 db 的 TableDef 列表，输出 6 类变更
- `diff_columns` 仅检测 added/dropped/type_changed，`renamed_columns` 始终为空（未实现重命名启发式）

**约束**：
- `introspect` 为简化实现（返回空列表），实际应由各方言 SchemaIntrospector 实现
- `SchemaDiff.has_destructive_changes()` 仅检查 dropped_tables/dropped_columns，不含 renamed_columns（重命名视为非破坏性？需明确）
- DDL 生成器 5 方言已实现（MySql/PG/SQLite/Oracle/MSSQL DdlGenerator）

### 1.2.6 `nested_active_model.rs` NestedActiveModel（B-5 改造基础）

**接口契约**：
- `NestedActiveModel::from_model(parent, relation) -> Self`：从 ActiveModel 创建
- `NestedActiveModel::with_children(children) -> Self`：配置子实体
- `NestedActiveModel::cascade_delete(bool) -> Self`：配置级联删除（bool）
- `nested_save(conn, nested) -> Result<SaveResult>`：事务内递归保存父+子，外键回填
- `nested_delete(conn, nested) -> Result<u64>`：事务内删除子先父后，cascade 递归

**业务规则**：
- `nested_save`：① BEGIN → ② INSERT 父 → ③ last_insert_id 回填子外键 → ④ INSERT 子（递归）→ ⑤ COMMIT
- `nested_delete`：① BEGIN → ② DELETE children WHERE fk = parent_id → ③ DELETE parent → ④ COMMIT
- `cascade_delete: bool`：true 时递归删除子级的子级，false 时仅删直接子级
- `ChildEntity` 支持多级嵌套（`children: Vec<ChildEntity>`）

**约束**：
- `cascade_delete` 为 bool，仅支持 CASCADE 语义，无 RESTRICT/SET_NULL/SET_DEFAULT
- `nested_delete` 无策略参数，策略由 `nested.cascade_delete` 字段决定
- 事务原子性：任一操作失败 rollback
- DELETE SQL 使用参数化（`WHERE fk = ?`），无 SQL 注入风险

---

# 二、增量设计方案

## 2.1 实现模型

### 2.1.1 上下文视图

```plantuml
@startuml
!theme plain
skinparam componentStyle rectangle

actor "sz-rust 框架" as SzRust
actor "应用开发者" as AppDev
actor "DBA" as DBA

package "sz-orm v2.2.0" as SzOrm {
  component "UnifiedPool\n(A-3 新增)" as Unified
  component "AnyPool\n(A-1 扩展)" as AnyPool
  component "Dialect 自动选择\n(A-2 集成)" as DialectSel
  component "EagerLoader 多级\n(B-1 扩展)" as Eager
  component "SchemaSync 破坏性\n(B-2 扩展)" as SchemaSync
  component "select_exclude\n(B-3 新增)" as SelectExc
  component "BackpressureStream\n(B-4 新增)" as Backpressure
  component "CascadeStrategy\n(B-5 新增)" as Cascade
}

database "MySQL" as MySQL
database "PostgreSQL" as PG
database "SQLite" as SQLite
database "Oracle" as Oracle
database "SQL Server" as MSSQL

SzRust --> Unified : Arc<UnifiedPool> 持有
SzRust --> AnyPool : 5 后端运行时切换
AppDev --> Eager : 多级 with() + 循环检测
AppDev --> SelectExc : select_exclude(&[...])
AppDev --> Backpressure : stream_with_backpressure(n)
AppDev --> Cascade : nested_delete(strategy)
DBA --> SchemaSync : destructive_sync(Confirm::Yes)

Unified --> AnyPool : 委托后端识别
Unified --> DialectSel : 自动绑定 Dialect
AnyPool --> MySQL : DDL + DML
AnyPool --> PG : DDL + DML
AnyPool --> SQLite : DDL + DML
AnyPool --> Oracle : DDL + DML (A-1 新增)
AnyPool --> MSSQL : DDL + DML (A-1 新增)

Eager ..> AnyPool : 通过 Connection 执行
SchemaSync ..> AnyPool : 通过 Connection 执行
@enduml
```

**上下文关系说明**：
- **上游调用方**：sz-rust 框架（通过 `Arc<UnifiedPool>` 持有统一连接池）、应用开发者（通过 EagerLoader/select_exclude/BackpressureStream/CascadeStrategy API）、DBA（通过 destructive_sync）
- **下游依赖方**：5 种数据库后端（MySQL/PostgreSQL/SQLite/Oracle/MSSQL），通过 `ConnectionFactory` trait 抽象
- **中间件**：无新增中间件，复用 v2.1.0 的自研连接池（ArrayQueue + AtomicU32 + Notify）
- **跨进程通信**：无，所有操作在同一 Tokio 运行时内

### 2.1.2 服务/组件总体架构

```plantuml
@startuml
!theme plain
skinparam componentStyle rectangle

package "sz-orm-sqlx" {
  component "any_driver.rs\n(A-1 扩展)" as AnyDriver {
    portin "DSN" as DSN
    portout "AnyPool" as AnyPoolOut
  }
  component "unified_pool.rs\n(A-3 新增)" as UnifiedPoolMod {
    portin "DSN / Pool" as UnifiedIn
    portout "UnifiedPool" as UnifiedOut
  }
}

package "sz-orm-core" {
  component "dialect.rs\n(A-2 集成)" as DialectMod {
    portin "AnyBackend" as BackendIn
    portout "Box<dyn Dialect>" as DialectOut
  }
  component "pool.rs\n(复用)" as PoolMod
  component "eager_loader.rs\n(B-1 扩展)" as EagerMod {
    portin "RelationDef 链" as RelIn
    portout "NestedEagerResult" as EagerOut
  }
  component "cycle_detection.rs\n(B-1 新增)" as CycleMod
  component "schema_sync.rs\n(B-2 扩展)" as SchemaSyncMod {
    portin "Confirm + Hooks" as DestructiveIn
    portout "DestructiveSyncResult" as DestructiveOut
  }
  component "query.rs\n(B-3 扩展)" as QueryMod {
    portin "exclude fields" as ExcludeIn
  }
  component "stream_api.rs\n(B-4 扩展)" as StreamMod {
    portin "buffer_size" as BufIn
    portout "impl Stream" as StreamOut
  }
  component "nested_active_model.rs\n(B-5 扩展)" as NestedMod {
    portin "CascadeStrategy" as CascadeIn
  }
}

package "sz-orm-oracle" {
  component "OracleConnectionFactory\n(复用)" as OraFact
}
package "sz-orm-mssql" {
  component "MssqlConnectionFactory\n(复用)" as MssqlFact
}
package "sz-orm-audit" {
  component "审计日志\n(B-2 依赖)" as Audit
}

AnyDriver --> OraFact : A-1 新增分支
AnyDriver --> MssqlFact : A-1 新增分支
UnifiedPoolMod --> AnyDriver : 委托后端识别
UnifiedPoolMod --> PoolMod : 复用完整连接池
DialectMod --> BackendIn
EagerMod --> CycleMod : 循环检测
SchemaSyncMod --> Audit : 破坏性 DDL 审计
@enduml
```

**模块划分与职责**：
- **`any_driver.rs`（扩展）**：AnyBackend 枚举 3→5 变体，AnyPool::connect 分派 3→5 分支，新增 `dialect()` 方法
- **`unified_pool.rs`（新增）**：UnifiedPool 统一抽象，包装 `Pool` + `AnyBackend`，提供 `connect`/`acquire`/`dialect`/`backend`/`from_pool`
- **`dialect.rs`（集成）**：OracleDialect/SqlServerDialect 已实现，新增 `AnyBackend::dialect()` 映射
- **`eager_loader.rs`（扩展）**：ChildLoadConfig 改递归结构，load_many 无限级递归，新增 CyclePolicy
- **`cycle_detection.rs`（新增）**：CyclePolicy 枚举 + CycleDetector（访问集合 + 深度计数）
- **`schema_sync.rs`（扩展）**：diff_columns 新增重命名检测，新增 destructive_sync 方法
- **`query.rs`（扩展）**：新增 select_exclude 方法
- **`stream_api.rs`（扩展）**：新增 stream_with_backpressure 方法（mpsc channel 桥接）
- **`nested_active_model.rs`（扩展）**：cascade_delete: bool → CascadeStrategy 枚举，新增 nested_delete_with_strategy

**配置项及取值策略**：
- `oracle`/`mssql` cargo feature：gate Oracle/MSSQL 后端编译，未启用时 AnyPool::connect 返回 Err 提示启用 feature
- `CyclePolicy` 默认 `Truncate`（安全截断，不报错）
- `CascadeStrategy` 默认 `Cascade`（与 v2.1.0 bool=true 兼容）
- 重命名检测阈值：Levenshtein 距离 ≤ 2 或编辑距离/长度 ≤ 0.3（可配置）

### 2.1.3 实现设计文档

#### A-1 AnyBackend 扩展状态机

```plantuml
@startuml
!theme plain
title A-1 AnyBackend DSN 识别状态机

state "接收 DSN" as Receive
state "解析 scheme" as Parse
state "匹配白名单" as Match
state "MySql" as MySql
state "Postgres" as Postgres
state "Sqlite" as Sqlite
state "Oracle" as Oracle
state "Mssql" as Mssql
state "Err(未知 scheme)" as Err

Receive --> Parse
Parse --> Match

Match --> MySql : mysql:// / mariadb://
Match --> Postgres : postgres:// / postgresql://
Match --> Sqlite : sqlite:// / sqlite:
Match --> Oracle : oracle://
Match --> Mssql : mssql:// / sqlserver://
Match --> Err : 其他

note right of Oracle : A-1 新增分支\n需启用 oracle feature
note right of Mssql : A-1 新增分支\n需启用 mssql feature
note right of Err : 错误信息含\nmysql/postgres/sqlite/oracle/mssql
@enduml
```

#### B-1 多级 Eager Loading 递归流程

```plantuml
@startuml
!theme plain
title B-1 多级 Eager Loading 递归 + 循环检测

start
:接收 RelationDef 链\nUser → Order → OrderItem → Product;
:初始化 CycleDetector(CyclePolicy::Truncate);
:depth = 0;

while (还有未处理的关联级?) is (是)
  :check(entity_type, depth);
  if (已访问该 entity 类型?) then (是)
    if (CyclePolicy?) then (Error)
      :返回 Err(循环路径);
      stop
    elseif (Truncate) then (Truncate)
      :终止该分支递归;
    else (AllowWithDepthLimit(n))
      if (depth > n?) then (是)
        :终止递归;
      else (否)
        :继续递归;
      endif
    endif
  else (否)
    :标记已访问;
    :执行 WHERE fk IN (...) 批量查询;
    :按外键分组组装;
    :depth += 1;
  endif
endwhile (否)

:返回 NestedEagerResult 树;
stop
@enduml
```

#### B-2 破坏性 Schema Sync 流程

```plantuml
@startuml
!theme plain
title B-2 destructive_sync 流程

start
:接收 destructive_sync(conn, Confirm, hooks);
if (Confirm == Yes?) then (否)
  :返回 Err(需显式确认);
  stop
else (是)
endif

:introspect(conn) → db_tables;
:diff(entity_tables, db_tables) → SchemaDiff;
:检测重命名（Levenshtein）;
:生成 DDL 序列（含 DROP/RENAME）;

:BEGIN TRANSACTION;

while (DDL 序列未执行完?) is (有下一条 DDL)
  if (DDL 是破坏性?) then (是)
    :调用对应钩子\n(before_drop_column / before_rename_column);
    if (钩子执行成功?) then (否)
      :ROLLBACK;
      :返回 Err(钩子失败);
      stop
    else (是)
    endif
  else (否)
  endif

  :执行 DDL;
  if (执行成功?) then (否)
    :ROLLBACK;
    :返回 Err(DDL 失败);
    stop
  else (是)
    :记录审计日志\n(sz-orm-audit);
  endif
endwhile (无下一条)

:COMMIT;
:返回 DestructiveSyncResult;
stop
@enduml
```

#### B-5 cascade_delete 策略分支

```plantuml
@startuml
!theme plain
title B-5 nested_delete 策略分支

start
:接收 nested_delete(conn, nested, strategy);
:BEGIN TRANSACTION;

switch (strategy)
case (Restrict)
  :SELECT COUNT(*) FROM children WHERE fk = parent_id;
  if (count > 0?) then (是)
    :ROLLBACK;
    :返回 Err(存在 N 个子实体，禁止删除);
    stop
  else (否)
    :DELETE parent WHERE pk = parent_id;
  endif
case (Cascade)
  :递归 DELETE children 的 children;
  :DELETE children WHERE fk = parent_id;
  :DELETE parent WHERE pk = parent_id;
case (SetNull)
  if (fk 列允许 NULL?) then (否)
    :ROLLBACK;
    :返回 Err(外键列不允许 NULL);
    stop
  else (是)
    :UPDATE children SET fk = NULL WHERE fk = parent_id;
    :DELETE parent WHERE pk = parent_id;
  endif
case (SetDefault)
  if (fk 列有默认值?) then (否)
    :ROLLBACK;
    :返回 Err(外键列无默认值);
    stop
  else (是)
    :UPDATE children SET fk = DEFAULT WHERE fk = parent_id;
    :DELETE parent WHERE pk = parent_id;
  endif
endswitch

:COMMIT;
:返回受影响行数;
stop
@enduml
```

## 2.2 接口设计

### 2.2.1 总体设计

| 接口分类 | 接口名称 | 所在模块 | 稳定性 | 变更类型 |
|---------|---------|---------|--------|---------|
| 后端无关抽象 | `AnyBackend` | `sz-orm-sqlx::any_driver` | 稳定 | 扩展（3→5 变体） |
| 后端无关抽象 | `AnyPool` | `sz-orm-sqlx::any_driver` | 稳定 | 扩展（connect 3→5 分支 + dialect 方法） |
| 统一连接池 | `UnifiedPool` | `sz-orm-sqlx::unified_pool` | 稳定 | 新增 |
| Dialect 选择 | `AnyBackend::dialect` | `sz-orm-sqlx::any_driver` | 稳定 | 新增方法 |
| Eager Loading | `EagerLoader::with` | `sz-orm-core::eager_loader` | 稳定 | 扩展（限 2 级→无限级） |
| Eager Loading | `CyclePolicy` | `sz-orm-core::cycle_detection` | 实验 | 新增 |
| Eager Loading | `NestedEagerResult` | `sz-orm-core::eager_loader` | 稳定 | 新增 |
| Schema Sync | `SchemaSync::destructive_sync` | `sz-orm-core::schema_sync` | 稳定 | 新增方法 |
| Schema Sync | `Confirm` | `sz-orm-core::schema_sync` | 稳定 | 新增枚举 |
| Schema Sync | `DataMigrationHook` | `sz-orm-core::schema_sync` | 实验 | 新增 trait |
| Partial Models | `QueryBuilder::select_exclude` | `sz-orm-core::query` | 稳定 | 新增方法 |
| Stream API | `StreamApiExt::stream_with_backpressure` | `sz-orm-core::stream_api` | 稳定 | 新增方法 |
| 级联删除 | `CascadeStrategy` | `sz-orm-core::nested_active_model` | 稳定 | 新增枚举 |
| 级联删除 | `nested_delete_with_strategy` | `sz-orm-core::nested_active_model` | 稳定 | 新增函数 |

**接口变更策略**：
- **零 Breaking Change**：所有变更以扩展方法/新增类型提供，不修改现有 API 签名
- `AnyBackend` 枚举新增变体：不影响现有 match（现有代码 match 3 变体需追加 wildcard 或新变体，但 `#[non_exhaustive]` 标注可避免破坏——需评估是否加 `#[non_exhaustive]`）
- `NestedActiveModel.cascade_delete: bool` → `cascade_strategy: CascadeStrategy`：字段类型变更，但 `cascade_delete(bool)` 方法保留（内部映射到 CascadeStrategy），保持源码兼容

### 2.2.2 接口清单

#### A-1 AnyBackend 扩展

```rust
// packages/sz-orm-sqlx/src/any_driver.rs

/// 数据库后端类型（v2.2.0 扩展至 5 变体）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnyBackend {
    MySql,
    Postgres,
    Sqlite,
    /// Oracle（v2.2.0 新增，需启用 oracle feature）
    Oracle,
    /// SQL Server / MSSQL（v2.2.0 新增，需启用 mssql feature）
    Mssql,
}

impl AnyBackend {
    /// 从 DSN 自动识别后端类型（v2.2.0 扩展至 5 scheme）
    pub fn from_dsn(dsn: &str) -> Result<Self, DbError>;

    /// 后端名称
    pub fn name(&self) -> &'static str;

    /// 返回对应 Dialect（v2.2.0 新增，A-2 集成）
    pub fn dialect(&self) -> Box<dyn Dialect>;
}
```

**业务说明**：`from_dsn` 扩展识别 `oracle://`/`mssql://`/`sqlserver://`，未知 scheme 错误信息列出 5 种支持 scheme。`dialect()` 方法根据 backend 返回对应 Dialect 实例（Oracle→OracleDialect，Mssql→SqlServerDialect）。

**前置条件**：Oracle/MSSQL 后端需启用对应 cargo feature，否则 `from_dsn` 返回 Err 提示启用 feature。

**后置条件**：返回的 `AnyBackend` 可用于 `AnyPool::connect` 分派和 Dialect 自动选择。

**异常映射**：未知 scheme → `DbError::ConnectionRefused`（含 5 种支持 scheme 列表）；feature 未启用 → `DbError::ConnectionRefused`（含 feature 启用提示）。

#### A-1 AnyPool::connect 扩展

```rust
impl AnyPool {
    /// 连接数据库，根据 DSN 自动识别后端（v2.2.0 扩展至 5 后端）
    pub async fn connect(dsn: &str) -> Result<Self, DbError>;

    /// 获取对应 Dialect（v2.2.0 新增）
    pub fn dialect(&self) -> Box<dyn Dialect>;
}
```

**调用示例**：
```rust
// Oracle 后端
let pool = AnyPool::connect("oracle://sys:test123@127.0.0.1:1521/freepdb1").await?;
assert_eq!(pool.backend(), AnyBackend::Oracle);
let dialect = pool.dialect(); // OracleDialect
```

#### A-3 UnifiedPool 统一抽象

```rust
// packages/sz-orm-sqlx/src/unified_pool.rs

/// 统一连接池（v2.2.0 新增）
///
/// 结合 AnyPool 的后端切换能力与 Pool 的连接复用能力，
/// 供 sz-rust AppState 持有单一类型 Arc<UnifiedPool>。
pub struct UnifiedPool {
    backend: AnyBackend,
    pool: sz_orm_core::Pool,
}

impl UnifiedPool {
    /// 从 DSN 创建统一连接池
    pub async fn connect(dsn: &str) -> Result<Self, DbError>;

    /// 从 DSN + PoolConfig 创建（自定义连接池配置）
    pub async fn connect_with_config(
        dsn: &str,
        config: PoolConfig,
    ) -> Result<Self, DbError>;

    /// 从已有 Pool 包装（迁移路径）
    pub fn from_pool(pool: sz_orm_core::Pool, backend: AnyBackend) -> Self;

    /// 获取连接（委托 Pool::acquire）
    pub async fn acquire(&self) -> Result<PooledConnection, PoolError>;

    /// 获取后端类型
    pub fn backend(&self) -> AnyBackend;

    /// 获取对应 Dialect
    pub fn dialect(&self) -> Box<dyn Dialect>;

    /// 动态调整连接池大小（委托 Pool::resize）
    pub fn resize(&self, new_max: u32);

    /// 关闭所有连接（委托 Pool::close_all）
    pub async fn close_all(&self);

    /// 获取监控指标（委托 Pool::metrics）
    pub fn metrics(&self) -> PoolMetrics;
}
```

**业务说明**：`UnifiedPool` 内部持有 `sz_orm_core::Pool`（完整连接池），外加 `AnyBackend` 标识后端。所有连接池语义（复用/超时/断路器/限流/监控/resize/close_all）委托 `Pool`，无丢失。

**前置条件**：DSN 合法、对应 feature 已启用、PoolConfig 有效。

**后置条件**：返回的 `UnifiedPool` 绑定单一后端，创建后不可切换（需创建新实例）。

**迁移路径**：sz-rust 从 `Arc<Pool>` 迁移到 `Arc<UnifiedPool>`：
```rust
// 旧代码
let pool = Pool::new(config, factory)?;
let state = AppState { pool: Arc::new(pool) };

// 新代码（零成本迁移）
let unified = UnifiedPool::connect("mysql://...").await?;
let state = AppState { pool: Arc::new(unified) };
// 或从已有 Pool 包装
let unified = UnifiedPool::from_pool(pool, AnyBackend::MySql);
```

#### B-1 多级 Eager Loading + 循环检测

```rust
// packages/sz-orm-core/src/cycle_detection.rs

/// 循环检测策略（v2.2.0 新增）
#[derive(Debug, Clone)]
pub enum CyclePolicy {
    /// 遇循环报错
    Error,
    /// 截断循环，返回已加载部分（默认）
    Truncate,
    /// 允许至深度 n（0 = 仅主表）
    AllowWithDepthLimit(usize),
}

impl Default for CyclePolicy {
    fn default() -> Self { CyclePolicy::Truncate }
}

/// 循环检测器
pub struct CycleDetector {
    policy: CyclePolicy,
    visited: std::collections::HashSet<String>,
    current_depth: usize,
}

impl CycleDetector {
    pub fn new(policy: CyclePolicy) -> Self;
    /// 检查是否可继续递归（true=继续，false=终止）
    pub fn check(&mut self, entity_type: &str) -> Result<bool, DbError>;
    pub fn enter(&mut self, entity_type: &str);
    pub fn leave(&mut self);
}
```

```rust
// packages/sz-orm-core/src/eager_loader.rs（扩展）

/// 多级 Eager Loading 结果（递归类型，v2.2.0 新增）
#[derive(Debug, Clone)]
pub enum NestedEagerResult {
    /// 叶子节点（无子级关联）
    Leaf(HashMap<String, Value>),
    /// 分支节点（本级行 + 子级结果）
    Node {
        row: HashMap<String, Value>,
        children: Vec<NestedEagerResult>,
    },
}

/// 子级加载配置（v2.2.0 改为递归结构）
struct ChildLoadConfig {
    relation: RelationDef,
    /// 子级的子级（无限级嵌套）
    children: Vec<ChildLoadConfig>,
}

impl EagerLoader {
    /// 设置循环检测策略（v2.2.0 新增）
    pub fn with_cycle_policy(mut self, policy: CyclePolicy) -> Self;

    /// 添加子级关联（v2.2.0 扩展为无限级）
    pub fn with(mut self, relation: RelationDef) -> Self;

    /// 执行多级加载，返回嵌套结果树（v2.2.0 新增）
    pub async fn load_nested(
        &self,
        conn: &mut dyn Connection,
        main_sql: &str,
    ) -> Result<Vec<NestedEagerResult>, DbError>;
}
```

**调用示例**：
```rust
use sz_orm_core::eager_loader::EagerLoader;
use sz_orm_core::cycle_detection::CyclePolicy;

// 4 级关联：User → Order → OrderItem → Product
let loader = EagerLoader::new(order_relation)
    .with(order_item_relation)
    .with(product_relation)
    .with_cycle_policy(CyclePolicy::Truncate);
let results = loader.load_nested(&mut conn, "SELECT id, name FROM users").await?;
// results: Vec<NestedEagerResult>（4 级嵌套树）
```

#### B-2 破坏性 Schema Sync

```rust
// packages/sz-orm-core/src/schema_sync.rs（扩展）

/// 破坏性操作显式确认（v2.2.0 新增）
#[derive(Debug, Clone, Copy)]
pub enum Confirm {
    Yes,
    No,
}

/// 数据迁移钩子 trait（v2.2.0 新增）
pub trait DataMigrationHook: Send + Sync {
    /// 删除列前调用（备份数据）
    fn before_drop_column<'a>(
        &'a self,
        conn: &'a mut dyn Connection,
        table: &'a str,
        column: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>>;

    /// 重命名列前调用（备份旧列数据）
    fn before_rename_column<'a>(
        &'a self,
        conn: &'a mut dyn Connection,
        table: &'a str,
        from: &'a str,
        to: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>>;
}

/// 破坏性同步结果
#[derive(Debug, Clone)]
pub struct DestructiveSyncResult {
    pub executed_ddl: Vec<String>,
    pub hooks_called: usize,
    pub audit_entries: usize,
}

impl SchemaSync {
    /// 破坏性同步（v2.2.0 新增）
    ///
    /// 1. 校验 Confirm::Yes
    /// 2. introspect → diff（含重命名检测）
    /// 3. 生成 DDL（含 DROP/RENAME）
    /// 4. 事务内：钩子 → DDL → 审计
    pub async fn destructive_sync(
        &self,
        conn: &mut dyn Connection,
        confirm: Confirm,
        hooks: Option<&dyn DataMigrationHook>,
    ) -> Result<DestructiveSyncResult, DbError>;
}
```

**调用示例**：
```rust
use sz_orm_core::schema_sync::{SchemaSync, Confirm, DataMigrationHook};

struct AvatarBackupHook;
impl DataMigrationHook for AvatarBackupHook {
    fn before_drop_column<'a>(
        &'a self, conn: &'a mut dyn Connection, table: &'a str, column: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            let sql = format!("INSERT INTO {}_history SELECT id, {} FROM {}", column, column, table);
            conn.execute(&sql).await?;
            Ok(())
        })
    }
    // ...
}

let sync = SchemaSync::new(entity_tables);
let result = sync.destructive_sync(
    &mut conn,
    Confirm::Yes,
    Some(&AvatarBackupHook),
).await?;
```

#### B-3 select_exclude

```rust
// packages/sz-orm-core/src/query.rs（扩展）

impl<M: Model> QueryBuilder<M> {
    /// 排除指定字段查询（v2.2.0 新增，B-3）
    ///
    /// 与 `select_only` 互补：排除指定字段，查询其余所有字段。
    /// 自动进入 Partial 模式。
    ///
    /// # 错误
    ///
    /// - 排除字段不存在 → `DbError::InvalidInput`
    /// - 排除所有字段 → `DbError::InvalidInput`
    pub fn select_exclude(mut self, fields: &[&str]) -> Result<Self, DbError>;
}
```

**调用示例**：
```rust
// 排除 avatar 和 blob_data 字段
let sql = User::find()
    .select_exclude(&["avatar", "blob_data"])?
    .build_select();
// SELECT id, name, email, ... FROM users（不含 avatar, blob_data）
```

#### B-4 stream_with_backpressure

```rust
// packages/sz-orm-core/src/stream_api.rs（扩展）

impl<M: Model> StreamApiExt<M> for QueryBuilder<M> {
    /// 有界缓冲流式查询（v2.2.0 新增，B-4）
    ///
    /// 使用 mpsc::channel(buffer_size) 桥接生产者（游标 fetch）与消费者。
    /// 缓冲区满时生产者阻塞等待，控制内存占用。
    ///
    /// # 错误
    ///
    /// - buffer_size == 0 → `DbError::InvalidInput`
    fn stream_with_backpressure<'a, 'b: 'a, C: Connection + Send + 'b>(
        self,
        conn: &'b mut C,
        buffer_size: usize,
    ) -> Pin<Box<dyn Stream<Item = Result<RowResult, DbError>> + Send + 'a>>;
}
```

**调用示例**：
```rust
use futures::StreamExt;

let mut stream = User::find()
    .stream_with_backpressure(&mut conn, 1000)
    .await?;
while let Some(result) = stream.next().await {
    let row = result?;
    // 处理 row，缓冲区满时生产者自动阻塞
}
```

#### B-5 CascadeStrategy + nested_delete_with_strategy

```rust
// packages/sz-orm-core/src/nested_active_model.rs（扩展）

/// 级联删除策略（v2.2.0 新增）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CascadeStrategy {
    /// 有子实体则禁止删除
    Restrict,
    /// 级联删除子实体（默认，与 v2.1.0 兼容）
    Cascade,
    /// 子实体外键置 NULL
    SetNull,
    /// 子实体外键置默认值
    SetDefault,
}

impl Default for CascadeStrategy {
    fn default() -> Self { CascadeStrategy::Cascade }
}

/// 按策略执行嵌套删除（v2.2.0 新增）
pub async fn nested_delete_with_strategy<M: Model>(
    conn: &mut dyn Connection,
    nested: &NestedActiveModel<M>,
    strategy: CascadeStrategy,
) -> Result<u64, DbError>;
```

**调用示例**：
```rust
use sz_orm_core::nested_active_model::{nested_delete_with_strategy, CascadeStrategy};

// RESTRICT 策略：有子实体则拒绝
let result = nested_delete_with_strategy(&mut conn, &user_nested, CascadeStrategy::Restrict).await;
assert!(result.is_err()); // User 有 Order，拒绝删除

// SET_NULL 策略：子实体外键置 NULL
let rows = nested_delete_with_strategy(&mut conn, &user_nested, CascadeStrategy::SetNull).await?;
// 3 个 Order.user_id 置 NULL，User 删除
```

## 2.3 数据模型

### 2.3.1 设计目标

**需要支持的业务场景**：
1. sz-rust 框架通过 `Arc<UnifiedPool>` 持有 5 后端统一连接池，运行时透明切换
2. 应用开发者通过多级 Eager Loading 一次加载整个对象图（无限级 + 循环安全）
3. DBA 通过 `destructive_sync` 安全执行破坏性 Schema 变更（含数据迁移钩子 + 审计）
4. 应用开发者通过 `select_exclude` 排除大字段、通过 `stream_with_backpressure` 控制流式内存、通过 `CascadeStrategy` 选择级联策略

**性能、容量、扩展性目标**：
- AnyPool 后端切换开销 ≤ 5%（DFX 4.1.1）
- UnifiedPool 适配开销 ≤ 5%（DFX 4.1.2）
- 多级 Eager Loading 开销 ≤ 15%（DFX 4.1.3）
- 背压流式查询峰值内存 ≤ buffer_size * row_size + 10 MB（DFX 4.1.5）

**与存量数据的兼容策略**：
- `AnyBackend` 新增变体不破坏现有 match（评估 `#[non_exhaustive]` 标注）
- `NestedActiveModel.cascade_delete: bool` 保留方法，内部映射到 `CascadeStrategy`
- `stream_buffered` 保留（向后兼容），`stream_with_backpressure` 为新增
- `sync`/`sync_dry_run` 保留（非破坏性），`destructive_sync` 为新增

### 2.3.2 模型实现

```plantuml
@startuml
!theme plain
title v2.2.0 核心数据模型类图

class AnyBackend <<enum>> {
  + MySql
  + Postgres
  + Sqlite
  + Oracle
  + Mssql
  + from_dsn(dsn: &str) -> Result<Self>
  + name() -> &'static str
  + dialect() -> Box<dyn Dialect>
}

class AnyPool {
  - backend: AnyBackend
  - factory: Arc<dyn ConnectionFactory>
  + connect(dsn: &str) -> Result<Self>
  + backend() -> AnyBackend
  + dialect() -> Box<dyn Dialect>
}

class UnifiedPool {
  - backend: AnyBackend
  - pool: Pool
  + connect(dsn: &str) -> Result<Self>
  + acquire() -> Result<PooledConnection>
  + backend() -> AnyBackend
  + dialect() -> Box<dyn Dialect>
  + from_pool(pool: Pool, backend: AnyBackend) -> Self
}

class Pool {
  - config: PoolConfig
  - factory: Arc<dyn ConnectionFactory>
  - idle: ArrayQueue
  - total_count: AtomicU32
  - notify: Notify
  - circuit_breaker
  - rate_limiter
  - metrics: AtomicU64
}

class EagerLoader {
  - relation: RelationDef
  - children: Vec<ChildLoadConfig>
  - cycle_policy: CyclePolicy
  + with(relation: RelationDef) -> Self
  + load_nested(conn) -> Vec<NestedEagerResult>
}

class ChildLoadConfig {
  - relation: RelationDef
  - children: Vec<ChildLoadConfig>
}

class NestedEagerResult <<enum>> {
  + Leaf(HashMap<String, Value>)
  + Node { row, children: Vec<NestedEagerResult> }
}

class CyclePolicy <<enum>> {
  + Error
  + Truncate
  + AllowWithDepthLimit(usize)
}

class SchemaSync {
  - entity_tables: Vec<TableDef>
  - ddl_generator: Box<dyn DdlGenerator>
  + sync(conn) -> SyncResult
  + destructive_sync(conn, Confirm, hooks) -> DestructiveSyncResult
}

class SchemaDiff {
  + added_tables: Vec<TableDef>
  + dropped_tables: Vec<String>
  + added_columns: Vec<(String, ColumnDef)>
  + dropped_columns: Vec<(String, String)>
  + type_changed_columns: Vec<...>
  + renamed_columns: Vec<(String, String, String)>
}

class CascadeStrategy <<enum>> {
  + Restrict
  + Cascade
  + SetNull
  + SetDefault
}

class NestedActiveModel {
  - parent: ActiveModel<M>
  - children: Vec<ChildEntity>
  - relation: RelationDef
  - cascade_strategy: CascadeStrategy
}

AnyPool --> AnyBackend : backend
UnifiedPool --> AnyBackend : backend
UnifiedPool --> Pool : pool
EagerLoader --> ChildLoadConfig : children
EagerLoader --> CyclePolicy : cycle_policy
ChildLoadConfig --> ChildLoadConfig : children (递归)
NestedEagerResult --> NestedEagerResult : children (递归)
SchemaSync --> SchemaDiff : diff
NestedActiveModel --> CascadeStrategy : cascade_strategy

note right of UnifiedPool
  v2.2.0 新增
  统一 AnyPool 后端切换
  + Pool 连接复用
end note

note right of CascadeStrategy
  v2.2.0 新增
  替代 cascade_delete: bool
  默认 Cascade（兼容）
end note
@enduml
```

**对象之间的关系**：
- `UnifiedPool` 组合 `AnyBackend`（值语义）+ `Pool`（共享语义，内部 Arc）
- `EagerLoader` 聚合 `ChildLoadConfig`（递归树结构）+ `CyclePolicy`（值语义）
- `NestedEagerResult` 是递归枚举（自引用树）
- `SchemaSync` 聚合 `TableDef` 列表 + `DdlGenerator` trait object
- `NestedActiveModel` 聚合 `ActiveModel<M>` + `ChildEntity` 列表 + `CascadeStrategy`

**对象创建和销毁策略**：
- `UnifiedPool::connect`：DSN → AnyBackend → ConnectionFactory → Pool::new，创建后绑定单一后端
- `UnifiedPool::from_pool`：从已有 Pool 包装，零成本迁移
- `NestedEagerResult` 树：load_nested 递归构建，叶子节点无 children
- `CycleDetector`：每次 load_nested 创建新实例，visited 集合随递归更新

**持久化策略**：
- `UnifiedPool` 不持久化（运行时对象，连接池状态在内存）
- `SchemaDiff` 不持久化（值对象，diff 计算结果）
- `CascadeStrategy` 不持久化（枚举，配置项）
- 审计日志通过 `sz-orm-audit` 持久化（破坏性 DDL 记录）

---

# 三、API 兼容性分析

## 3.1 兼容性原则

v2.2.0 严格遵守 **零 Breaking Change** 原则（DFX 4.2.1、4.5.5）。所有新增能力以扩展方法、新增类型、新增枚举变体提供，不修改现有 API 签名、不删除现有方法、不改变现有语义。

## 3.2 逐项兼容性分析

| 变更项 | 变更类型 | 兼容性影响 | 缓解措施 |
|--------|---------|-----------|---------|
| `AnyBackend` 新增 `Oracle`/`Mssql` 变体 | 枚举扩展 | ⚠️ 现有代码 `match backend { MySql=>..., Postgres=>..., Sqlite=>... }` 无 wildcard 时编译失败 | 方案1：加 `#[non_exhaustive]` 标注，强制外部 wildcard；方案2：文档提示新增 2 个 match 臂。**推荐方案1**（`#[non_exhaustive]` 已是 Rust 惯例，外部 crate 无法穷举 match，需 wildcard） |
| `AnyBackend::from_dsn` 新增 scheme 分支 | 方法扩展 | ✅ 无影响（新增分支不影响现有 scheme 识别） | 无需缓解 |
| `AnyPool::connect` 新增 Oracle/Mssql 分支 | 方法扩展 | ✅ 无影响（新增分支仅在 DSN 为 oracle/mssql 时触发） | 无需缓解 |
| `AnyPool::dialect()` 新增方法 | 新增方法 | ✅ 无影响（新增方法不破坏现有调用） | 无需缓解 |
| `UnifiedPool` 新增类型 | 新增类型 | ✅ 无影响（sz-rust 可选迁移，不强制） | 提供迁移文档 + `from_pool` 零成本路径 |
| `EagerLoader::with` 限 2 级→无限级 | 语义扩展 | ✅ 无影响（现有 2 级调用行为不变，新增 3+ 级为扩展） | 无需缓解 |
| `ChildLoadConfig` 新增 `children` 字段 | 内部结构扩展 | ✅ 无影响（`ChildLoadConfig` 为私有 struct，外部不可见） | 无需缓解 |
| `NestedEagerResult` 新增类型 | 新增类型 | ✅ 无影响（`EagerResult` 保留，`load_many` 保留，新增 `load_nested`） | 无需缓解 |
| `CyclePolicy` 新增枚举 | 新增枚举 | ✅ 无影响 | 无需缓解 |
| `SchemaSync::destructive_sync` 新增方法 | 新增方法 | ✅ 无影响（`sync`/`sync_dry_run` 保留，非破坏性语义不变） | 无需缓解 |
| `SchemaDiff.renamed_columns` 字段填充 | 语义扩展 | ✅ 无影响（字段已存在，v2.2.0 开始填充，现有代码读取该字段获得更多数据） | 无需缓解 |
| `Confirm`/`DataMigrationHook` 新增 | 新增类型 | ✅ 无影响 | 无需缓解 |
| `QueryBuilder::select_exclude` 新增方法 | 新增方法 | ✅ 无影响（`select_only` 保留） | 无需缓解 |
| `StreamApiExt::stream_with_backpressure` 新增方法 | 新增方法 | ✅ 无影响（`stream_buffered` 保留） | 无需缓解 |
| `CascadeStrategy` 新增枚举 | 新增枚举 | ✅ 无影响 | 无需缓解 |
| `NestedActiveModel.cascade_delete: bool` → `cascade_strategy: CascadeStrategy` | 字段类型变更 | ⚠️ 字段类型变更，但字段为私有（`cascade_delete(bool)` 方法保留） | `cascade_delete(bool)` 方法保留，内部映射：true→Cascade，false→Restrict（或保留 bool 字段 + 新增 strategy 字段，方法设置两者）。**推荐：保留 `cascade_delete: bool` 字段 + 新增 `cascade_strategy: Option<CascadeStrategy>` 字段，`cascade_delete()` 设置 bool，`with_strategy()` 设置 enum，strategy 优先** |
| `nested_delete` 保留 + `nested_delete_with_strategy` 新增 | 新增函数 | ✅ 无影响（`nested_delete` 保留，使用 `nested.cascade_delete` 字段） | 无需缓解 |

## 3.3 sz-pay 回归验证策略

v2.2.0 发布前必须在 sz-pay 项目运行 `cargo test --lib`，验证 5,139 测试无回归（DFX 4.4.5）。

**验证步骤**：
1. sz-pay `Cargo.toml` 依赖升级 `sz-orm-core = "2.2.0"` 等
2. `cargo check` 编译通过（验证 API 兼容）
3. `cargo test --lib` 全量测试通过（验证行为兼容）
4. 对比 v2.1.0 基线：测试数量、通过率无下降

**关键风险点**：
- `AnyBackend` 新增变体：sz-pay 若有 `match backend` 无 wildcard，编译失败 → 需提前检查 sz-pay 代码
- `NestedActiveModel` 字段变更：sz-pay 若直接访问 `cascade_delete` 字段（非方法），编译失败 → 字段为私有，风险低

## 3.4 `#[non_exhaustive]` 评估

**结论**：`AnyBackend` 添加 `#[non_exhaustive]` 标注。

**理由**：
- `AnyBackend` 为公开枚举，外部 crate（sz-rust/sz-pay）可能 match
- 新增变体（Oracle/Mssql）会导致无 wildcard 的 match 编译失败
- `#[non_exhaustive]` 强制外部 wildcard，未来再新增变体不破坏外部
- 这是 Rust 生态惯例（`sqlx::Any`、`sea_orm::DatabaseBackend` 等均使用）

**影响**：
- 外部 match 必须有 wildcard 臂：`match backend { MySql=>..., _=>... }`
- sz-pay 现有 match 需检查是否已有 wildcard，无则需追加（一次性迁移）

---

# 四、测试策略

## 4.1 测试分层

| 层级 | 测试类型 | 覆盖目标 | 工具 |
|------|---------|---------|------|
| L1 | 单元测试 | 每个新增类型/方法的纯逻辑（枚举构造、DSN 解析、循环检测、重命名启发式、CascadeStrategy 分支） | `#[test]` + `#[tokio::test]` |
| L2 | 集成测试（SQLite） | 端到端行为（AnyPool 5 后端 CRUD、Eager Loading 多级组装、destructive_sync DDL 执行、select_exclude SQL 生成、stream 背压、cascade 4 策略） | SQLite 内存模式（快速、无外部依赖） |
| L3 | 集成测试（MySQL） | 真实数据库验证（MySQL 9.6 本机） | `mysql://root:test123@127.0.0.1:3306/sz_orm_test` |
| L4 | 集成测试（PostgreSQL） | 真实数据库验证（PG 18 本机） | `postgres://postgres:test123@127.0.0.1:5432/sz_orm_test` |
| L5 | 集成测试（Oracle） | AnyPool Oracle 后端验证（Oracle 23ai Free 本机） | `oracle://sys:test123@127.0.0.1:1521/freepdb1`，`#[ignore]` 标注 |
| L6 | 集成测试（MSSQL） | AnyPool MSSQL 后端验证（本机无实例时方言适配 + ignored） | `mssql://...`，`#[ignore]` 标注 |
| L7 | 回归测试 | sz-pay 5,139 测试无回归 | sz-pay 项目 `cargo test --lib` |
| L8 | 门禁测试 | 11 道门禁全通过 | `scripts/gate.ps1` |

## 4.2 逐需求测试方案

### 4.2.1 A-1 AnyPool 扩展测试

| 测试用例 | 层级 | 验收点 |
|---------|------|--------|
| `test_any_backend_from_dsn_oracle` | L1 | `from_dsn("oracle://...")` 返回 `Ok(Oracle)` |
| `test_any_backend_from_dsn_mssql` | L1 | `from_dsn("mssql://...")` 和 `from_dsn("sqlserver://...")` 返回 `Ok(Mssql)` |
| `test_any_backend_from_dsn_unknown_v22` | L1 | 未知 scheme 返回 Err，错误信息含 5 种支持 scheme |
| `test_any_backend_name_v22` | L1 | `Oracle.name()` == "oracle"，`Mssql.name()` == "mssql" |
| `test_any_pool_oracle_connect` | L5 | `AnyPool::connect("oracle://...")` 返回 Ok，backend==Oracle，CRUD 正常 |
| `test_any_pool_mssql_connect` | L6 | `AnyPool::connect("mssql://...")` 返回 Ok，backend==Mssql，CRUD 正常 |
| `test_any_pool_5_backend_crud_equivalence` | L2-L6 | 同一表结构，5 后端 CRUD 结果按主键排序完全一致 |
| `test_any_pool_feature_not_enabled` | L1 | 未启用 oracle feature 时 `AnyPool::connect("oracle://...")` 返回 Err 含 feature 提示 |

### 4.2.2 A-2 Dialect 集成测试

| 测试用例 | 层级 | 验收点 |
|---------|------|--------|
| `test_any_backend_dialect_mapping` | L1 | `MySql.dialect()` 返回 MySqlDialect，`Oracle.dialect()` 返回 OracleDialect，等 5 映射 |
| `test_oracle_dialect_pagination` | L1 | `OracleDialect.build_pagination(sql, 2, 10)` 含 `OFFSET ... FETCH NEXT`，不含 `LIMIT` |
| `test_mssql_dialect_pagination` | L1 | `SqlServerDialect.build_pagination(sql, 2, 10)` 含 `OFFSET ... FETCH NEXT` |
| `test_any_pool_oracle_dialect_auto_select` | L5 | 通过 AnyPool 创建 Oracle 连接，执行分页查询，生成 SQL 使用 OracleDialect 语法 |
| `test_5_dialect_pagination_equivalence` | L2-L6 | 同一数据，5 方言分页 SQL 执行返回相同 10 行结果 |
| `test_oracle_dialect_no_placeholder` | L1 | 对 OracleDialect/SqlServerDialect 所有 trait 方法调用，无 panic（无 todo!/unimplemented!） |

### 4.2.3 A-3 UnifiedPool 测试

| 测试用例 | 层级 | 验收点 |
|---------|------|--------|
| `test_unified_pool_connect_5_backends` | L2-L6 | 5 种 DSN 创建 UnifiedPool，backend() 返回 5 种，CRUD 正常 |
| `test_unified_pool_connection_reuse` | L2 | max_size=10，连续 acquire 20 次 release 10 次，实际创建连接数 ≤ 10 |
| `test_unified_pool_circuit_breaker` | L2 | 配置断路器阈值=5，连续 5 次连接失败，断路器跳闸 |
| `test_unified_pool_metrics` | L2 | acquire/release 后 metrics() 计数正确 |
| `test_unified_pool_from_pool_migration` | L2 | `UnifiedPool::from_pool(pool, backend)` 行为与直接 connect 一致 |
| `test_unified_pool_dialect_binding` | L2-L6 | `UnifiedPool::connect("oracle://...").dialect()` 返回 OracleDialect |
| `test_unified_pool_resize_close_all` | L2 | resize/close_all 行为与 Pool 一致 |

### 4.2.4 B-1 多级 Eager Loading 测试

| 测试用例 | 层级 | 验收点 |
|---------|------|--------|
| `test_eager_load_4_level` | L2 | User→Order→OrderItem→Product 4 级，返回 NestedEagerResult 4 级嵌套树，组装正确 |
| `test_eager_load_batch_query` | L2 | 100 User→300 Order→900 OrderItem，执行 3 条 SQL（每级 1 条），N1QueryDetector 无告警 |
| `test_eager_load_cycle_truncate` | L2 | User→Order→User 循环，CyclePolicy::Truncate，终止递归，返回已加载部分 |
| `test_eager_load_cycle_error` | L2 | CyclePolicy::Error，返回 Err 含循环路径 "User → Order → User" |
| `test_eager_load_cycle_depth_limit` | L2 | CyclePolicy::AllowWithDepthLimit(3)，递归至深度 3 终止 |
| `test_eager_load_no_select_star` | L2 | 多级 Eager Loading 生成的所有 SQL 不含 `SELECT *` |
| `test_eager_load_memory_limit` | L2 | 结果集超内存限制时返回 Err 含建议（改用 Stream API） |

### 4.2.5 B-2 破坏性 Schema Sync 测试

| 测试用例 | 层级 | 验收点 |
|---------|------|--------|
| `test_sync_no_destructive` | L2 | `sync()` 不生成 DROP COLUMN，仅记录 diff |
| `test_destructive_sync_drop_column` | L2 | `destructive_sync(Yes)` 生成 `ALTER TABLE ... DROP COLUMN` |
| `test_destructive_sync_no_confirm` | L2 | `destructive_sync(No)` 返回 Err 要求显式确认 |
| `test_destructive_sync_rename_detection` | L2 | `user_name`→`username`（类型同），diff 返回 RenameColumn，非 DropColumn+AddColumn |
| `test_destructive_sync_hook` | L2 | 注册 before_drop_column 钩子，执行 destructive_sync，钩子先执行备份 SQL |
| `test_destructive_sync_audit` | L2 | 执行 destructive_sync，sz-orm-audit 记录含 DDL/时间/受影响行数 |
| `test_destructive_sync_transaction_rollback` | L2 | 3 条 DDL，第 2 条失败，第 1 条回滚 |
| `test_destructive_sync_sqlite_no_transaction` | L2 | SQLite 部分 DDL 不支持事务，失败时返回 Err 含已执行 DDL 列表 |

### 4.2.6 B-3 select_exclude 测试

| 测试用例 | 层级 | 验收点 |
|---------|------|--------|
| `test_select_exclude_basic` | L1 | `select_exclude(&["avatar","blob_data"])` 生成 SQL 不含这两列 |
| `test_select_exclude_complement` | L1 | 表 5 列，`select_exclude(&["d","e"])` 等价 `select_only(&["a","b","c"])` |
| `test_select_exclude_nonexistent` | L1 | 排除不存在字段返回 Err 含字段名 |
| `test_select_exclude_all_fields` | L1 | 排除所有字段返回 Err "不能排除所有字段" |
| `test_select_exclude_with_eager` | L2 | `select_exclude(&["avatar"]).with(Order)`，User 不含 avatar，Order 全字段 |
| `test_select_exclude_no_select_star` | L1 | 生成的 SQL 不含 `SELECT *`，显式列出保留列 |

### 4.2.7 B-4 Stream 背压测试

| 测试用例 | 层级 | 验收点 |
|---------|------|--------|
| `test_stream_backpressure_basic` | L2 | `stream_with_backpressure(1000)` 返回 Stream，缓冲区容量 1000 |
| `test_stream_backpressure_block` | L2 | buffer_size=10，生产 100 行消费 0 行，生产者第 11 行阻塞，缓冲区 ≤ 10 |
| `test_stream_backpressure_unblock` | L2 | 缓冲区满，消费 1 行，生产者恢复产出第 11 行 |
| `test_stream_backpressure_memory` | L2 | 100 万行，buffer_size=1000，峰值内存 ≤ 1000*row_size + 10 MB |
| `test_stream_backpressure_no_drop` | L2 | buffer_size=10，生产 100 行消费 100 行，消费者收到 100 行无丢失 |
| `test_stream_backpressure_error` | L2 | 数据库断开，Stream yield Err 后终止 |
| `test_stream_backpressure_zero_buffer` | L1 | `stream_with_backpressure(0)` 返回 Err |
| `test_stream_buffered_backward_compat` | L2 | v2.1.0 `stream_buffered` 行为不变 |

### 4.2.8 B-5 cascade_delete 测试

| 测试用例 | 层级 | 验收点 |
|---------|------|--------|
| `test_cascade_restrict_with_children` | L2 | RESTRICT + 有子实体，返回 Err 含子实体数量，父未删 |
| `test_cascade_restrict_no_children` | L2 | RESTRICT + 无子实体，删除成功 |
| `test_cascade_cascade_recursive` | L2 | CASCADE，User→3 Order→15 OrderItem，删除 1+3+15=19 行，事务内 |
| `test_cascade_set_null` | L2 | SET_NULL，3 Order.user_id 置 NULL，User 删除 |
| `test_cascade_set_null_not_nullable` | L2 | SET_NULL + fk NOT NULL，返回 Err 提示 |
| `test_cascade_set_default` | L2 | SET_DEFAULT，3 Order.user_id 置默认值，User 删除 |
| `test_cascade_set_default_no_default` | L2 | SET_DEFAULT + fk 无默认值，返回 Err 提示 |
| `test_cascade_transaction_rollback` | L2 | CASCADE，删第 2 个 OrderItem 失败，事务回滚 |
| `test_cascade_default_compatibility` | L2 | `nested_delete(conn, nested)` 使用默认 Cascade，与 v2.1.0 行为一致 |

## 4.3 测试覆盖率目标

- 每个新增功能单元测试 + 集成测试覆盖率 ≥ 80%（DFX 4.4.1）
- 每个新增公开 API 有 rustdoc + ≥ 1 doctest（DFX 4.4.2）
- 5 方言验证：A 系列（AnyPool/Dialect）必须覆盖 Oracle；B 系列至少 MySQL + SQLite

## 4.4 门禁清单

| # | 门禁 | 命令 | v2.2.0 重点 |
|---|------|------|------------|
| 1 | fmt | `cargo fmt --all -- --check` | 新增模块格式 |
| 2 | check | `cargo check --workspace --all-targets` | 5 后端 feature 编译 |
| 3 | clippy | `cargo clippy --workspace --all-targets -- -D warnings` | 零警告 |
| 4 | test | `cargo test --workspace` | 新增测试全通过 |
| 5 | doc | `cargo doc --workspace --no-deps --all-features` | 新增 API doctest |
| 6 | audit | `cargo audit` + `cargo deny check` | 无新依赖漏洞 |
| 7 | integration | `cargo test --workspace -- --ignored` | Oracle/MSSQL 集成 |
| 8 | 占位扫描 | `grep -rn 'todo!\|unimplemented!\|unreachable!'` | 新增代码 0 处占位 |
| 9 | SQL 注入扫描 | `scripts/check-sql-injection.ps1` | 新增 SQL 参数化 |
| 10 | feature 全组合 | `cargo check --workspace --all-targets --all-features` | oracle/mssql feature |
| 11 | ADR-0001 | `git diff --name-only HEAD` | 仅修改 sz-orm 仓库 |

---

# 五、里程碑划分

## 5.1 里程碑技术设计

| 里程碑 | 内容 | 技术重点 | 依赖 | 预估工作量 |
|--------|------|---------|------|-----------|
| **M1** | A-1 + A-2：AnyPool 扩展 Oracle/MSSQL + Dialect 集成验证 | `AnyBackend` 枚举 3→5 变体 + `#[non_exhaustive]`；`from_dsn` 新增 3 scheme；`AnyPool::connect` 新增 2 分支（feature gate）；`AnyBackend::dialect()` 新增方法；Oracle/MSSQL 集成测试（`#[ignore]`） | 无 | 中（枚举扩展 + feature gate + 集成测试） |
| **M2** | A-3：AnyPool 与 Pool 统一抽象 | `unified_pool.rs` 新增模块；`UnifiedPool` 结构（backend + Pool）；`connect`/`connect_with_config`/`from_pool`/`acquire`/`dialect`/`backend`/`resize`/`close_all`/`metrics` 方法；sz-rust 迁移文档 | M1（需 5 后端 AnyPool 就绪） | 中（新模块 + 委托 Pool + 迁移路径） |
| **M3** | B-1 + B-2：多级 Eager Loading + 破坏性 Schema Sync | `cycle_detection.rs` 新增（CyclePolicy + CycleDetector）；`ChildLoadConfig` 改递归；`NestedEagerResult` 新增；`load_nested` 方法；`diff_columns` 重命名检测（Levenshtein）；`destructive_sync` 方法 + `Confirm` + `DataMigrationHook` + 审计集成 | 无（与 M1/M2 可并行） | 大（递归数据结构 + 循环检测 + 重命名启发式 + 钩子 + 审计） |
| **M4** | B-3 + B-4 + B-5：select_exclude + Stream 背压 + cascade_delete | `QueryBuilder::select_exclude` 方法；`stream_with_backpressure` 方法（mpsc channel）；`CascadeStrategy` 枚举；`nested_delete_with_strategy` 函数；`NestedActiveModel` 字段兼容设计 | 无 | 中（3 个独立功能，各中等复杂度） |
| **M5** | 集成验证与版本发布 | sz-pay 回归（5,139 测试）；11 道门禁；43 包版本 2.1.0→2.2.0；crates.io 发布；CHANGELOG 更新 | M1-M4 全部完成 | 小（验证 + 发布流程） |

## 5.2 里程碑依赖关系

```plantuml
@startuml
!theme plain
skinparam componentStyle rectangle

rectangle "M1: A-1+A-2\nAnyPool 扩展 + Dialect 集成" as M1
rectangle "M2: A-3\nUnifiedPool 统一抽象" as M2
rectangle "M3: B-1+B-2\n多级 Eager + 破坏性 Sync" as M3
rectangle "M4: B-3+B-4+B-5\nselect_exclude + 背压 + cascade" as M4
rectangle "M5: 集成验证 + 发布" as M5

M1 --> M2 : 5 后端 AnyPool 就绪
M1 --> M5
M2 --> M5
M3 --> M5
M4 --> M5

note right of M3
  M3 与 M1/M2 可并行
  （无依赖关系）
end note
@enduml
```

## 5.3 关键路径

**关键路径**：M1 → M2 → M5（A 系列多后端 ORM 增强，sz-rust 集成驱动，高优先级）

**并行路径**：M3 ∥ M4（B 系列短期目标，与 A 系列无依赖，可并行开发）

**发布条件**：M1-M4 全部完成 + M5 集成验证通过（11 道门禁 + sz-pay 回归 + crates.io 发布）

---

# 六、风险评估

## 6.1 技术难点与缓解措施

| # | 风险 | 等级 | 技术难点 | 缓解措施 |
|---|------|------|---------|---------|
| R-1 | `AnyBackend` 新增变体破坏外部 match | 🟡 中 | sz-rust/sz-pay 现有 `match backend` 无 wildcard，新增 Oracle/Mssql 变体导致编译失败 | ① 添加 `#[non_exhaustive]` 标注；② 提前检查 sz-pay 代码 match 语句；③ 提供迁移文档 |
| R-2 | Oracle/MSSQL feature gate 编译矩阵膨胀 | 🟡 中 | 5 后端 feature 全组合编译（2^5=32 组合），CI 耗时增长 | ① feature 默认全启用；② CI 仅验证关键组合（全启用 + 各单独启用）；③ 文档说明 feature 矩阵 |
| R-3 | 多级 Eager Loading 无限级递归栈溢出 | 🟠 高 | `load_nested` 递归深度无上限，深嵌套关联（如 100 级）可能栈溢出 | ① CyclePolicy::AllowWithDepthLimit 默认上限（如 10）；② 深度超限返回 Err；③ 文档建议深嵌套用 Stream API |
| R-4 | 循环检测误判 | 🟡 中 | `CycleDetector` 按 entity 类型名去重，同类型不同实例关联（如 User→Manager(User)）可能误判为循环 | ① 按 entity 类型 + 关联名联合去重（User::manager ≠ User::orders）；② 文档说明检测粒度 |
| R-5 | 重命名检测误判 | 🟡 中 | Levenshtein 距离 ≤ 2 可能误判（如 `name`→`title` 距离 4 不识别，`user_name`→`user_title` 距离 2 识别为重命名但实为不同字段） | ① 类型兼容性校验（类型不同不识别）；② 阈值可配置；③ `destructive_sync` dry-run 模式供审查 |
| R-6 | 破坏性 DDL 事务不支持 | 🟡 中 | SQLite/Oracle 部分 DDL 不支持事务回滚，中途失败无法恢复 | ① 检测方言 DDL 事务支持；② 不支持时返回 Err 含已执行 DDL 列表；③ 文档说明各方言 DDL 事务限制 |
| R-7 | stream_with_backpressure 生产者任务泄漏 | 🟠 高 | mpsc 生产者 spawn 任务在消费者 drop 时未终止，连接不归还 | ① 生产者持有 `Receiver` 的 `Sender` clone，drop 时关闭；② 使用 `tokio::select!` 监听 receiver 关闭；③ Drop impl 关闭游标 |
| R-8 | `NestedActiveModel` 字段类型变更破坏 sz-pay | 🟡 中 | `cascade_delete: bool` → `cascade_strategy: CascadeStrategy` 若 sz-pay 直接访问字段 | ① 字段为私有（pub struct 但字段无 pub）；② `cascade_delete(bool)` 方法保留；③ sz-pay 回归验证 |
| R-9 | Oracle/MSSQL 集成测试环境依赖 | 🟡 中 | Oracle 23ai Free / SQL Server 本机实例可能不可用 | ① `#[ignore]` 标注，CI 有实例时运行；② 方言适配单元测试不依赖真实 DB；③ 文档说明本机 DB 配置 |
| R-10 | UnifiedPool 适配层性能开销 | 🟡 中 | UnifiedPool 委托 Pool 可能引入额外间接调用开销 | ① 委托方法内联（`#[inline]`）；② 基准测试验证开销 ≤ 5%（DFX 4.1.2）；③ Pool 字段直接持有非 Arc |

## 6.2 依赖风险

| 依赖 | 风险 | 缓解措施 |
|------|------|---------|
| `sz-orm-oracle` | Oracle 驱动（oracle-rs）阻塞 API，需 spawn_blocking | 复用 v2.0.0 已有的 blocking_pool 派发机制（`oracle lib.rs:684`） |
| `sz-orm-mssql` | MSSQL 驱动（tiberius）异步 API | 复用 v2.0.0 已有的 MssqlConnection 实现 |
| `sz-orm-audit` | 破坏性 DDL 审计依赖审计模块 | 复用 v2.0.0 已有的 sz-orm-audit 包（哈希链防篡改） |
| `tokio::sync::mpsc` | 背压流依赖 mpsc channel | tokio 已是核心依赖，无新增 |
| `sz-orm-macros` | `#[relation(cascade = "restrict")]` 标注需宏扩展 | 复用 v2.1.0 `parse_relation_attr`（`derive.rs:1134`），新增 cascade 字段解析 |

## 6.3 总体风险等级

| 维度 | 等级 | 说明 |
|------|------|------|
| 技术可行性 | 🟢 低 | 所有需求基于存量代码扩展，无全新技术栈；Oracle/MSSQL ConnectionFactory/Dialect 已实现 |
| API 兼容性 | 🟡 中 | `AnyBackend` 新增变体 + `NestedActiveModel` 字段变更有风险，但有缓解措施 |
| 性能风险 | 🟢 低 | UnifiedPool 委托可内联；多级 Eager Loading 批量查询；背压 mpsc 高效 |
| 测试风险 | 🟡 中 | Oracle/MSSQL 集成测试依赖本机 DB 实例，可能不可用（#[ignore] 缓解） |
| 工期风险 | 🟡 中 | M3（多级 Eager + 破坏性 Sync）工作量较大，可与 M1/M2 并行缓解 |

---

> **文档版本**：v1.0（v2.2.0 技术设计初稿）
> **生成日期**：2026-08-06
> **生成方法**：基于 spec.md（38 条 EARS）+ 源码现状调研（any_driver.rs/dialect.rs/pool.rs/eager_loader.rs/schema_sync.rs/stream_api.rs/nested_active_model.rs/query.rs/oracle lib.rs/mssql lib.rs）+ v2.1.0 设计基线
> **审计合规**：所有存量代码引用附 `file:line` 证据（见 §1.1.1 / §1.2）
> **设计约束**：零 Breaking Change、禁止占位实现、unsafe 零容忍、WHERE 参数化、禁止 SELECT *、Connection trait 手动解糖 async
> **下一步**：用户确认技术设计 → spec-task-agent 生成 tasks.md → 编码实现
