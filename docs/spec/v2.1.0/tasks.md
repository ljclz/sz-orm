# sz-orm v2.1.0 编码任务规划

> **版本**：v2.1.0
> **基线**：v2.0.0（42 包 @ 2.0.0 已发布 crates.io，4,947 测试通过）
> **生成日期**：2026-08-06
> **文档目的**：将 v2.1.0 七项功能的技术设计（design.md）转化为可执行、可验收的编码任务清单
> **依据**：
> - `docs/spec/v2.1.0/spec.md`（需求规格，10 章 EARS 验收标准）
> - `docs/spec/v2.1.0/design.md`（技术设计，8 章，含接口签名/流程图/文件变更清单/ADR）
> - `AGENTS.md`（10 道门禁 + 五维审查 + AI 辅助开发 10 条硬约束）

---

## 任务总览

| 里程碑 | 任务组 | 对应功能 | 优先级 | 工时 | 依赖 |
|--------|--------|---------|--------|------|------|
| M1 | 实现 RelationTrait 关联关系基础设施 | P-F-2 | 🟠 中 | 5-7 天 | 无 |
| M2 | 实现 Partial Models 部分字段选择 | P-F-3 | 🟡 低 | 2-3 天 | 无 |
| M3 | 实现 Eager Loading 端到端自动执行与组装 | P-F-1 | 🟠 中 | 5-7 天 | M1, M2 |
| M4 | 实现 ActiveModel 嵌套持久化 | P-F-5 | 🟡 低 | 4-5 天 | M1 |
| M5 | 实现 Schema Sync 自动结构同步 | P-F-4 | 🟡 低 | 7-10 天 | 无（独立并行） |
| M6 | 实现异步流式查询 Stream API | P-F-6 | 🟡 低 | 3-4 天 | 无（独立并行） |
| M7 | 建立跨框架性能基准对比 | P-F-7 | 🟠 中 | 3-5 天 | M1-M6 |
| M8 | 集成验证与 v2.1.0 发布 | 全部 | — | 2-3 天 | M1-M7 |

**关键路径**：M1 (5-7d) → M2 (2-3d) → M3 (5-7d) → M4 (4-5d) → M7 (3-5d) → M8 (2-3d) = **21-30 天**
**并行优化**：M5、M6 可与关键路径并行，总工期约 **3-4 周**

**实施顺序约束**（来自 design.md 4.3 节）：
```
关键路径：M1 → M2 → M3 → M4 → M7 → M8
并行分支 1：M5（任意时间并行）
并行分支 2：M6（任意时间并行）
```

---

## 1. 实现 RelationTrait 关联关系基础设施（P-F-2）

**目标**：`#[derive(Relation)]` 自动生成 `RelationTrait` 实现，`QueryBuilder` 提供 `join()` / `left_join()` 链式 API，追平 SeaORM 关联查询体验。
**对应需求**：spec.md 5.2 节，design.md 2.2.2 P-F-2 接口
**工时**：5-7 天
**依赖**：无（关键路径第 1 步）

### 1.1 定义 RelationTrait / RelationDef / RelationKind 核心类型
- [ ] 在 `packages/sz-orm-core/src/relation_trait.rs` 新建模块，定义 `RelationKind` 枚举（HasOne/HasMany/BelongsTo/ManyToMany），实现 `default_join_type()` 方法（HasOne/BelongsTo → INNER，HasMany/ManyToMany → LEFT）
- [ ] 在同文件定义 `RelationDef` 结构体（name/from_entity/to_entity/from_key/to_key 均为 `&'static str`，kind 为 `RelationKind`），派生 `Debug + Clone`
- [ ] 在同文件定义 `RelationTrait` trait（`def(&self) -> &'static RelationDef` + `all_relations() -> &'static [RelationDef]`），bound 为 `Send + Sync`
- [ ] 在 `packages/sz-orm-core/src/lib.rs` 注册 `pub mod relation_trait;` 并 re-export 公开类型
- **验收**：Given 定义 RelationDef 字面量 → When 调用 `kind.default_join_type()` → Then HasMany 返回 LEFT、BelongsTo 返回 INNER；`cargo check` 通过

### 1.2 扩展 #[derive(Relation)] 宏生成 RelationTrait 实现
- [ ] 在 `packages/sz-orm-macros/src/derive.rs` 的 `derive_relation_impl` 函数中，复用 `parse_relation_attr`（存量 `derive.rs:1134`）解析结果，为每个 `RelationAttr` 构造 `RelationDef` 字面量 TokenStream
- [ ] 在生成的 TokenStream 中追加 `const RELATIONS: &[RelationDef] = &[ ... ]` 关联常量表
- [ ] 追加生成 `impl RelationTrait for Struct` 代码块，实现 `def()` 返回 `&RELATIONS[index]`、`all_relations()` 返回 `RELATIONS`
- [ ] 在 `packages/sz-orm-macros/src/lib.rs` 调整宏生成代码对 `sz_orm_core::relation_trait::{RelationTrait, RelationDef, RelationKind}` 的引用路径
- [ ] 保留存量 `impl ModelExt for Struct` 的 `relations()` 映射不变（向后兼容，C-9）
- **验收**：Given `#[derive(Relation)] struct User` with `#[relation(has_many = "Order", fk = "user_id", pk = "id")]` → When `cargo expand` → Then 展开代码含 `impl RelationTrait for User` 和 `const RELATIONS`；存量 `impl ModelExt` 仍存在

### 1.3 实现 Entity::relation(name) 关联查找方法
- [ ] 在 `packages/sz-orm-core/src/model.rs` 为 `Model` trait 或新增扩展 trait 添加 `relation(name: &str) -> Option<&'static RelationDef>` 方法，从 `RelationTrait::all_relations()` 中按 name 查找
- [ ] 方法返回 `Option`，未找到关联名时返回 `None`（运行时安全，不 panic）
- **验收**：Given User 定义了 "orders" 关联 → When 调用 `User::relation("orders")` → Then 返回 `Some(&RelationDef{ name: "orders", ... })`；调用 `User::relation("unknown")` 返回 `None`

### 1.4 实现 QueryBuilder::join() / left_join() 链式方法
- [ ] 在 `packages/sz-orm-core/src/query.rs` 的 `QueryBuilder<M>` 新增 `select_mode: SelectMode` 字段（默认 `All`）和 `join_counter: HashMap<String, usize>` 字段（表别名计数）
- [ ] 实现 `pub fn join(mut self, relation: &dyn RelationTrait) -> Self`：调用 `relation.def()` 获取 `RelationDef`，按 `kind.default_join_type()` 决定 INNER/LEFT，构造 `JoinClause` 推入 `self.joins`，返回 `Self`
- [ ] 实现 `pub fn left_join(mut self, relation: &dyn RelationTrait) -> Self`：强制 LEFT JOIN
- [ ] 处理表别名冲突：多次 join 同一 `to_entity` 时，通过 `join_counter` 生成别名 `orders_1`、`orders_2`（spec 5.2.4 异常 3）
- [ ] `join()` 参数类型为 `&dyn RelationTrait`，编译期拒绝字符串参数（spec 5.2.2 规则 6，防 SQL 注入）
- [ ] build_select 时渲染 `JOIN {to_table} ON {from_table}.{from_key} = {to_table}.{to_key}`，标识符引用复用 `dialect.quote()`
- **验收**：Given `User::find().join(Order::relation())` → When `.build_select()` → Then 生成 `SELECT ... FROM users INNER JOIN orders ON users.id = orders.user_id`（BelongsTo）或 `LEFT JOIN`（HasMany）；`query.join("raw_sql")` 编译错误

### 1.5 验证 RelationTrait + join() 五方言与测试覆盖
- [ ] 编写单元测试 `derive_relation_generates_trait`（宏展开断言）、`join_generates_inner_join`、`join_generates_left_join`、`left_join_forced`、`multi_join`（三表 JOIN）、`join_table_alias_conflict`（自动别名）
- [ ] 编写编译时测试 `join_reject_string_param`（`query.join("raw")` 编译失败，使用 `trybuild` 或文档标注）
- [ ] 编写集成测试覆盖 MySQL + PostgreSQL + SQLite + Oracle + MSSQL 五方言 JOIN SQL 标识符引用正确性（MSSQL 无实例时标注 `#[ignore]`）
- [ ] 为 `RelationTrait`、`RelationDef`、`join()`、`left_join()` 编写 rustdoc 文档注释 + doctest 示例
- [ ] 运行 `cargo clippy --workspace --all-targets -- -D warnings` 确保 0 警告
- **验收**：Given 5 方言环境 → When 执行 `join_5_dialects` 集成测试 → Then 各方言 JOIN SQL 标识符引用正确（MySQL 反引号、PG/SQLite/Oracle 双引号、MSSQL 方括号）；clippy 零警告

---

## 2. 实现 Partial Models 部分字段选择（P-F-3）

**目标**：`QueryBuilder` 提供 `select_only()` 进入部分选择模式，支持 `.column(C)` / `.column_as(Expr, alias)` / `.group_by(C)`，追平 SeaORM `select_only()` 性能优化能力。
**对应需求**：spec.md 5.3 节，design.md 2.2.2 P-F-3 接口
**工时**：2-3 天
**依赖**：无（关键路径第 2 步，可与 M1 并行）

### 2.1 定义 ColumnTrait / Expr / SelectMode / AggFunc 类型
- [ ] 在 `packages/sz-orm-core/src/partial_model.rs` 新建模块，定义 `ColumnTrait` trait（`name() -> &'static str` + `table_name() -> &'static str`），bound `Send + Sync`
- [ ] 定义 `AggFunc` 枚举（Count/Sum/Avg/Max/Min）和 `Expr` 结构体（`func: AggFunc` + `column: Box<dyn ColumnTrait>`），实现 `Expr::count/sum/avg/max/min` 构造方法
- [ ] 定义 `SelectMode` 枚举（All/Partial），派生 `Debug + Clone + Copy + PartialEq + Eq`
- [ ] 在 `packages/sz-orm-core/src/lib.rs` 注册 `pub mod partial_model;` 并 re-export
- **验收**：Given `Expr::count(UserColumn::Id)` → When 渲染 → Then 生成 `COUNT(users.id)`；`ColumnTrait` 仅接受类型化列引用，拒绝字符串

### 2.2 实现 QueryBuilder select_only / column / columns / column_as 方法
- [ ] 在 `packages/sz-orm-core/src/query.rs` 的 `QueryBuilder<M>` 实现 `pub fn select_only(mut self) -> Self`：设置 `self.select_mode = SelectMode::Partial`，清空 `self.select_columns`（默认 `vec!["*"]`）
- [ ] 实现 `pub fn column(mut self, column: impl ColumnTrait) -> Self`：将 `column.name()` 推入 `self.select_columns`
- [ ] 实现 `pub fn columns(mut self, columns: Vec<impl ColumnTrait>) -> Self`：批量添加列
- [ ] 实现 `pub fn column_as(mut self, expr: Expr, alias: &str) -> Self`：将聚合表达式渲染为 `FUNC(col) AS alias` 推入 `self.select_columns`
- [ ] 确保所有方法返回 `Self` 支持链式调用；`.column()` 参数为 `impl ColumnTrait` 编译期拒绝字符串（spec 5.3.2 规则 3）
- **验收**：Given `User::find().select_only().column(UserColumn::Id).column(UserColumn::Name)` → When `.build_select()` → Then 生成 `SELECT id, name FROM users`；`select_only().column("id")` 编译错误

### 2.3 实现 build 时 Partial 模式校验与 GROUP BY
- [ ] 在 `QueryBuilder::build_select` 中，当 `select_mode == Partial` 且 `select_columns` 为空时，返回 `Err(DbError::InvalidInput("select_only requires at least one column"))`（spec 5.3.2 规则 6）
- [ ] 确保存量 `group_by` 字段（`query.rs:41`）在 Partial 模式下正确渲染 `GROUP BY` 子句
- [ ] Partial 模式下 build 生成 `SELECT col1, col2, FUNC(col) AS alias FROM ... GROUP BY ...`，不得包含 `SELECT *`
- **验收**：Given `select_only()` 后未调用 `.column()` → When `.build_select()` → Then 返回 `Err(DbError::InvalidInput)`；`select_only().column(Id).group_by(Id)` 生成 `SELECT id FROM users GROUP BY id`

### 2.4 验证 Partial Models 测试覆盖与内存占比
- [ ] 编写单元测试 `select_only_basic`、`select_only_no_column_error`、`column_as_aggregation`、`group_by_with_select_only`
- [ ] 编写编译时测试 `column_type_safety`（`select_only().column("id")` 编译失败）
- [ ] 编写基准测试 `partial_memory_ratio`：断言 K 字段查询内存 ≤ 全字段查询 × (K+2)/总字段数（spec 4.1 性能 2）
- [ ] 为 `select_only`、`column`、`column_as`、`Expr` 编写 rustdoc + doctest
- [ ] 运行 `cargo test --workspace --lib` 确保新增测试全通过
- **验收**：Given User 有 4 字段，select_only 选 2 字段 → When 查询 → Then 结果集仅含 2 字段，内存 ≤ 全字段 × (2+2)/4 = 100%（边界验证）；clippy 零警告

---

## 3. 实现 Eager Loading 端到端自动执行与组装（P-F-1）

**目标**：一行 API `eager_load_all(conn, query, relation)` 自动执行主表 + 关联表查询并组装 `Vec<(M, Vec<R>)>`，消除 N+1，追平 SeaORM `find_with_related().all()`。
**对应需求**：spec.md 5.1 节，design.md 2.2.2 P-F-1 接口，ADR-v2.1.0-001（HasMany 双查询策略）
**工时**：5-7 天
**依赖**：M1（RelationDef）、M2（避免 SELECT *）

### 3.1 实现 EagerLoader 执行器结构与 load_many / load_one
- [ ] 在 `packages/sz-orm-core/src/eager_loader.rs` 新建模块，定义 `EagerLoader<'a, M, R>` 结构体（conn + main_query + relation + children + select_columns），bound `M: Model + FromQueryResult, R: Model + FromQueryResult`
- [ ] 实现 `pub fn new(conn, main_query, relation: RelationDef) -> Self`
- [ ] 实现 `async fn load_many(self) -> Result<Vec<(M, Vec<R>)>, DbError>`：执行主表查询 → 提取主键列表 → 生成 `WHERE fk IN (?, ...)` 批量查询 → 执行关联表查询 → 按外键分组组装（design.md P-F-1 流程）
- [ ] 实现 `async fn load_one(self) -> Result<Vec<(M, Option<R>)>, DbError>`：使用 JOIN 策略单条 SQL（HasOne/BelongsTo），拆分结果集组装
- [ ] 复用存量 `find_with_related_eager_sql`（`find_with_related.rs:263`）生成 SQL 模板，复用 `related_sql_with_ids`（`find_with_related.rs:621`）绑定主键参数
- [ ] 在 `packages/sz-orm-core/src/lib.rs` 注册 `pub mod eager_loader;`
- **验收**：Given 100 User 各 3 Order → When `eager_load_all(&mut conn, User::find(), Order::relation()).await` → Then 返回 100 元组，每 User.orders.len() == 3，执行 2 条 SQL（非 103 条）

### 3.2 实现 eager_load_all 便捷函数与异常处理
- [ ] 在 `eager_loader.rs` 实现 `pub async fn eager_load_all(conn, main_query, relation: &dyn RelationTrait) -> Result<Vec<(M, Vec<R>)>, DbError>`，内部构造 `EagerLoader::new` 并调用 `load_many`
- [ ] 主表查询失败时立即终止，不执行关联表查询，返回 `Err(DbError::SqlError)` 含主表 SQL（spec 5.1.4 异常 1）
- [ ] 关联表查询失败时丢弃主表结果，返回 `Err(DbError::SqlError)` 含关联表 SQL（spec 5.1.4 异常 2）
- [ ] 外键不匹配（孤立关联记录）时跳过并记录 warn 日志 `orphaned related record`（spec 5.1.4 异常 3）
- [ ] 主表结果为空时跳过关联表查询，直接返回 `Ok(Vec::new())`（spec 5.1.4 异常 4）
- **验收**：Given 主表 0 行 → When `eager_load_all` → Then 返回 `Ok(Vec::new())`，不执行关联表查询；主表查询失败 → 返回 Err 且不执行关联查询

### 3.3 实现多级关联 with() 链式（限 2 级）
- [ ] 在 `EagerLoader` 定义 `ChildLoadConfig` 结构体（relation + select_columns），`children: Vec<ChildLoadConfig>` 字段
- [ ] 实现 `pub fn with(mut self, relation: RelationDef) -> Self`：添加子级加载配置，返回 `Self`
- [ ] 在 `load_many` 中递归执行子级查询：User → Order → OrderItem，提取 Order 主键执行 OrderItem `WHERE order_id IN (...)`，按 order_id 分组组装 `(Order, Vec<OrderItem>)`，再按 user_id 分组组装最终 `Vec<(User, Vec<(Order, Vec<OrderItem>)>)>`（design.md 多级递归流程）
- [ ] 限制最大 2 级嵌套，超过时返回 `Err(DbError::InvalidInput("multi-level eager loading exceeds limit (2)"))`（ADR-v2.1.0-006，R-3 风险缓解）
- **验收**：Given `eager_load_all(&mut conn, User::find(), Order::relation()).with(OrderItem::relation())` → When `.await` → Then 返回 `Vec<(User, Vec<(Order, Vec<OrderItem>)>)>`，组装正确

### 3.4 消除 N+1 与避免 SELECT *
- [ ] EagerLoader 内部直接调用 `Connection::query_with_params`（批量查询路径），不经过 `N1QueryDetector` 逐行计数（design.md N+1 消除策略）
- [ ] EagerLoader 生成的 SQL 使用 Partial Models 显式列名（复用 M2 的 `select_only` 机制），不得包含 `SELECT *`（spec 5.1.2 规则 6，C-5 约束，R-6 风险缓解）
- [ ] 修改存量 `find_with_related.rs` 中生成 `SELECT *` 的位置（`find_with_related.rs:276`、`find_with_related.rs:533`），改用显式列名
- [ ] WHERE 条件使用参数化 `WhereCondition::In` 而非字符串拼接（C-4 约束）
- **验收**：Given 100 行主表 → When `eager_load_all` → Then 关联表查询恰好 1 条 SQL，`N1QueryDetector` 无告警；生成 SQL 不含 `SELECT *`

### 3.5 适配 Oracle IN 列表分批（>1000）
- [ ] 在 EagerLoader 关联表查询逻辑中，当方言为 Oracle 且主键数量 > 1000 时，分批查询（每批 1000 主键），合并结果后组装（design.md 3.1 节 Oracle 关键差异）
- [ ] 其他方言（MySQL/PG/SQLite/MSSQL）直接单次 `WHERE IN` 查询
- **验收**：Given Oracle 方言 + 主表 1500 行 → When `eager_load_all` → Then 关联表执行 2 批查询（1000 + 500），结果合并正确

### 3.6 验证 Eager Loading 测试覆盖
- [ ] 编写集成测试 `tests/integration_eager_loading.rs`：`eager_load_has_many_basic`（100×3）、`eager_load_has_one_basic`（JOIN 策略）、`eager_load_empty_main`、`eager_load_orphaned_related`、`eager_load_n1_no_warning`、`eager_load_multi_level`、`eager_load_oracle_in_batch`
- [ ] 编写单元测试 `eager_load_no_select_star`、`eager_load_main_query_fail`、`eager_load_related_query_fail`
- [ ] 集成测试覆盖 MySQL + SQLite 两方言（V-2），Oracle 集成测试标注 `#[ignore]`（V-3）
- [ ] 为 `EagerLoader`、`eager_load_all`、`with` 编写 rustdoc + doctest
- [ ] 运行 `cargo test --workspace` 确保新增测试全通过
- **验收**：Given 100 User × 3 Order 数据集 → When 执行 `eager_load_has_many_basic` → Then 返回 100 元组每 User.orders.len() == 3，执行 2 条 SQL，N1QueryDetector 无告警（spec 10.1 EARS）

---

## 4. 实现 ActiveModel 嵌套持久化（P-F-5）

**目标**：一次 `nested_save()` 调用持久化整个对象图（父 + 子集合），事务内自动外键回填，追平 SeaORM 嵌套持久化。
**对应需求**：spec.md 5.5 节，design.md 2.2.2 P-F-5 接口，ADR-v2.1.0-002（独立包装器）
**工时**：4-5 天
**依赖**：M1（RelationDef 外键回填方向）

### 4.1 实现 NestedActiveModel 包装器与 NestedActiveModelTrait
- [ ] 在 `packages/sz-orm-core/src/nested_active_model.rs` 新建模块，定义 `NestedActiveModelTrait` trait（继承 `ActiveModelTrait`，新增 `children() -> &[Box<dyn NestedActiveModelTrait>]`、`relation() -> &RelationDef`、`depth() -> usize`）
- [ ] 定义 `NestedActiveModel<M: Model>` 结构体（parent: ActiveModel<M> + children: Vec<Box<dyn NestedActiveModelTrait>> + relation: RelationDef + cascade_delete: bool），**不修改存量 `ActiveModel<M>`**（ADR-v2.1.0-002，C-9 向后兼容）
- [ ] 实现 `pub fn from_model(model: ActiveModel<M>, relation: RelationDef) -> Self`
- [ ] 实现 `pub fn with_children(mut self, children: Vec<Box<dyn NestedActiveModelTrait>>) -> Self` 和 `pub fn cascade_delete(mut self, cascade: bool) -> Self`
- [ ] 在 `packages/sz-orm-core/src/lib.rs` 注册 `pub mod nested_active_model;`
- **验收**：Given `NestedActiveModel::from_model(user, Order::relation()).with_children(vec![o1, o2])` → When 访问 `.children()` → Then 返回 2 个子实体；存量 `ActiveModel` 结构不变

### 4.2 实现 nested_save 事务执行与外键回填
- [ ] 实现 `pub async fn nested_save(conn: &mut dyn Connection, nested: NestedActiveModel<impl Model>) -> Result<SaveResult, DbError>`
- [ ] 校验嵌套深度 ≤ 10，超过返回 `Err(DbError::InvalidInput("nested persistence depth exceeds limit (10)"))`（spec 5.5.4 异常 4）
- [ ] 调用 `conn.begin_transaction()` 开启事务；使用 RAII 事务 guard，drop 时自动 rollback（R-8 风险缓解）
- [ ] 执行父实体 INSERT（仅 `Set` 字段，复用存量 `save` `active_model.rs:293`）；失败则 rollback 返回 `Err(DbError::SqlError)`（spec 5.5.4 异常 1）
- [ ] 调用 `conn.last_insert_id()` 获取父主键；失败返回 `Err(DbError::UnsupportedFeature)`（spec 5.5.4 异常 3）
- [ ] 遍历子实体，`child.set(relation.to_key, parent_id)` 回填外键，执行 child INSERT；任一失败则 rollback（spec 5.5.4 异常 2）
- [ ] 若 child 有子级（多级嵌套），递归 `nested_save`（design.md 拓扑顺序：父先子后）
- [ ] 全部成功后 `conn.commit()`，返回 `SaveResult { affected_rows, parent_id }`
- **验收**：Given User + 2 Order → When `nested_save` → Then 执行 3 条 INSERT（1 user + 2 orders），Order.user_id 自动回填为 User.last_insert_id，事务提交（spec 10.5 EARS）

### 4.3 实现嵌套删除与脏字段追踪
- [ ] 实现 `nested_delete`：删除顺序子先父后（DELETE children → DELETE parent，spec 5.5.2 规则 3），事务内执行
- [ ] `cascade_delete` 为 true 时，删除父实体同时删除子实体
- [ ] 嵌套 save 仅持久化 `ActiveValue::Set` 字段，`Unchanged` / `NotSet` 跳过（复用存量 `changed_fields` `active_model.rs:221`，spec 5.5.2 规则 5）
- **验收**：Given User 仅 name 为 Set，Order 仅 amount 为 Set → When `nested_save` → Then 生成 SQL 仅含 name 和 amount 列；删除时 SQL 顺序 DELETE children → DELETE parent

### 4.4 验证嵌套持久化测试覆盖与五方言 last_insert_id
- [ ] 编写集成测试 `tests/integration_nested_save.rs`：`nested_save_basic`、`nested_save_fk_backfill`、`nested_save_transaction_rollback`（第 2 个 Order 失败 → 全部回滚）、`nested_save_multi_level`（User→Order→OrderItem 拓扑顺序）、`nested_save_delete_order`
- [ ] 编写单元测试 `nested_save_dirty_fields_only`、`nested_save_depth_limit`（>10 层 → Err）
- [ ] 集成测试覆盖 MySQL + SQLite，验证各方言 `last_insert_id` 回填正确（PG 用 RETURNING、SQLite 用 last_insert_rowid、Oracle 用 RETURNING INTO）
- [ ] 为 `NestedActiveModel`、`nested_save`、`with_children` 编写 rustdoc + doctest
- **验收**：Given 3 个子实体第 2 个 INSERT 失败 → When `nested_save` → Then 事务回滚，User 和第 1 个 Order 的 INSERT 也撤销，返回 `Err(DbError::SqlError)`（spec 10.5 EARS）

---

## 5. 实现 Schema Sync 自动结构同步（P-F-4）

**目标**：比较实体定义与 DB 现有表结构差异，自动生成并执行 DDL（CREATE/ALTER TABLE），禁止破坏性 DDL，追平 SeaORM 2.0 `db.sync()`。
**对应需求**：spec.md 5.4 节，design.md 2.2.2 P-F-4 接口，ADR-v2.1.0-004（破坏性 DDL 禁止）
**工时**：7-10 天
**依赖**：无（独立并行分支，可任意时间开发）

### 5.1 定义 SchemaDiff / TableDef / ColumnDef 类型与 diff 纯函数
- [ ] 在 `packages/sz-orm-core/src/schema_sync.rs` 新建模块，定义 `TableDef`（name + columns: Vec<ColumnDef>）、`ColumnDef`（name + sql_type + nullable + primary_key + default）、`SchemaDiff`（added_tables/dropped_tables/added_columns/dropped_columns/type_changed_columns/renamed_columns）、`SyncResult`（affected_tables + executed_ddl）
- [ ] 实现 `pub fn diff(entity: &[TableDef], db: &[TableDef]) -> SchemaDiff` 纯函数：比较两组表结构，输出 6 类变更（design.md 2.2.2 P-F-4）
- [ ] 在 `packages/sz-orm-core/src/error.rs` 新增 `DbError::DestructiveChangeDetected` 和 `DbError::UnsupportedFeature` 变体
- [ ] 在 `packages/sz-orm-core/src/lib.rs` 注册 `pub mod schema_sync;`
- **验收**：Given 实体新增 email 字段，DB 表无此列 → When `diff` → Then 输出 `added_columns` 含 `("users", ColumnDef{ name: "email", ... })`；实体删除 legacy_col → 输出 `dropped_columns` 含 `("users", "legacy_col")`

### 5.2 实现 SchemaIntrospector trait 与五方言实现
- [ ] 在 `schema_sync.rs` 定义 `#[async_trait] pub trait SchemaIntrospector: Send + Sync`（`async fn introspect(&self, conn: &mut dyn Connection) -> Result<Vec<TableDef>, DbError>`）
- [ ] 在 `packages/sz-orm-core/src/schema_introspector_mysql.rs` 实现 `MySqlIntrospector`：查询 `INFORMATION_SCHEMA.COLUMNS WHERE TABLE_SCHEMA = ?`（design.md 3.4.1）
- [ ] 在 `schema_introspector_pg.rs` 实现 `PgIntrospector`：查询 `INFORMATION_SCHEMA.COLUMNS WHERE TABLE_SCHEMA = 'public'`
- [ ] 在 `schema_introspector_sqlite.rs` 实现 `SqliteIntrospector`：逐表 `PRAGMA table_info(table)`（无 INFORMATION_SCHEMA）
- [ ] 在 `schema_introspector_oracle.rs` 实现 `OracleIntrospector`：查询 `ALL_TAB_COLUMNS WHERE OWNER = ?`
- [ ] 在 `schema_introspector_mssql.rs` 实现 `MssqlIntrospector`：查询 `sys.columns JOIN sys.tables JOIN sys.types`
- [ ] 在 `lib.rs` 注册 5 个 introspector 模块
- **验收**：Given MySQL DB 有 users 表 → When `MySqlIntrospector::introspect(conn)` → Then 返回 `Vec<TableDef>` 含 users 表所有列定义；SQLite 用 PRAGMA 正确读取

### 5.3 实现 DdlGenerator trait 与五方言 DDL 生成
- [ ] 在 `schema_sync.rs` 定义 `pub trait DdlGenerator: Send + Sync`（`fn generate(&self, diff: &SchemaDiff) -> Result<Vec<String>, DbError>`）
- [ ] 在 `packages/sz-orm-core/src/ddl_generator_mysql.rs` 实现 `MySqlDdlGenerator`：新增表 `CREATE TABLE`、新增列 `ALTER TABLE t ADD COLUMN col TYPE`、类型变更 `ALTER TABLE t MODIFY COLUMN col TYPE`、重命名 `ALTER TABLE t RENAME COLUMN old TO new`
- [ ] 在 `ddl_generator_pg.rs` 实现 `PgDdlGenerator`：类型变更用 `ALTER COLUMN col TYPE TYPE`
- [ ] 在 `ddl_generator_sqlite.rs` 实现 `SqliteDdlGenerator`：新增列自动补充 `DEFAULT NULL`（无 NOT NULL 无默认值限制）；类型变更返回 `Err(UnsupportedFeature)`（SQLite 不支持，需重建表）
- [ ] 在 `ddl_generator_oracle.rs` 实现 `OracleDdlGenerator`：新增列用 `ADD (col TYPE)`（括号包裹）；类型变更 `MODIFY (col TYPE)`
- [ ] 在 `ddl_generator_mssql.rs` 实现 `MssqlDdlGenerator`：重命名用 `EXEC sp_rename 't.old', 'new', 'COLUMN'`（存储过程）
- [ ] 所有方言的 `DdlGenerator` 对 `dropped_columns` / `dropped_tables` 不生成 DDL，仅记录（ADR-v2.1.0-004）
- [ ] 在 `lib.rs` 注册 5 个 ddl_generator 模块
- **验收**：Given 同一 AddColumn diff → When 各方言 `generate` → Then MySQL/PG 生成 `ALTER TABLE users ADD COLUMN email VARCHAR(255)`，SQLite 生成 `ADD COLUMN email TEXT`，Oracle 生成 `ADD (email VARCHAR2(255))`；SQLite 类型变更返回 `Err(UnsupportedFeature)`

### 5.4 实现 SchemaSync 协调器与 sync_dry_run / sync
- [ ] 在 `schema_sync.rs` 定义 `pub struct SchemaSync`（introspector: Box<dyn SchemaIntrospector> + ddl_generator: Box<dyn DdlGenerator> + entity_tables: Vec<TableDef>）
- [ ] 实现 `pub fn new(dialect: DbType, entity_tables: Vec<TableDef>) -> Self`：按方言选择对应 introspector + generator
- [ ] 实现 `async fn sync_dry_run(&self, conn) -> Result<Vec<String>, DbError>`：introspect → diff → 检查破坏性 → generate DDL，不执行（spec 5.4.2 规则 4）
- [ ] 实现 `async fn sync(&self, conn) -> Result<SyncResult, DbError>`：sync_dry_run → `conn.begin_transaction()` → 逐条执行 DDL（经 sz-orm-audit 记录审计日志）→ commit/rollback（spec 5.4.2 规则 5）
- [ ] diff 含 `DroppedColumn` / `DroppedTable` 时返回 `Err(DbError::DestructiveChangeDetected)` 含破坏性变更详情（spec 5.4.4 异常 3，R-7 风险缓解）
- [ ] DDL 执行失败时事务回滚，返回 `Err(DbError::SqlError)` 含失败 DDL（spec 5.4.4 异常 2）
- [ ] 所有表名/列名经 `sql_safety::validate_identifier` 校验（spec 4.3 安全性 2）
- **验收**：Given 实体有 email，DB 无 email → When `sync_dry_run` → Then 返回 `Vec<String>` 含 `ALTER TABLE users ADD COLUMN email VARCHAR(255)`，DB 结构不变；`sync` 执行后 DB 新增 email 列

### 5.5 验证 Schema Sync 测试覆盖与五方言
- [ ] 编写单元测试 `diff_add_column`、`diff_drop_column_no_ddl`、`sync_destructive_error`、`sync_sqlite_type_change_unsupported`
- [ ] 编写集成测试 `tests/integration_schema_sync.rs`：`sync_dry_run_no_change`、`sync_executes_ddl`、`sync_transaction_rollback`、`sync_mysql_introspect`、`sync_pg_introspect`、`sync_sqlite_pragma`、`sync_5_dialect_ddl`
- [ ] Oracle 集成测试 `sync_oracle_introspect` 标注 `#[ignore]`（V-3）
- [ ] 为 `SchemaSync`、`sync`、`sync_dry_run`、`diff` 编写 rustdoc + doctest
- **验收**：Given 实体删除 legacy_col，DB 有 legacy_col → When `sync` → Then 返回 `Err(DestructiveChangeDetected)`，不生成 DROP COLUMN DDL（spec 10.4 EARS）

---

## 6. 实现异步流式查询 Stream API（P-F-6）

**目标**：`QueryBuilder::stream()` 改造为真游标逐行产出，峰值内存 ≤ 50 MB（100 万行），追平 SQLx 流式查询。
**对应需求**：spec.md 5.6 节，design.md 2.2.2 P-F-6 接口，ADR-v2.1.0-003（impl 改造）
**工时**：3-4 天
**依赖**：无（独立并行分支，底层游标 v2.0.0 已实现）

### 6.1 改造 StreamQueryTrait::stream impl 为真游标
- [ ] 在 `packages/sz-orm-core/src/paginator.rs` 改造 `StreamQueryTrait<M> for QueryBuilder<M>` 的 `stream` impl：从 `stream::once(query).flat_map(iter)`（全量收集，`paginator.rs:281`）改为委托 `conn.query_stream_cursor(sql, params)`（真游标，`pool.rs:180`）
- [ ] **trait 签名不变**（向后兼容，C-9，ADR-v2.1.0-003），仅改 impl 实现
- [ ] 真游标逐行 fetch，不得一次性收集为 Vec（spec 5.6.2 规则 7）
- [ ] drop 时关闭 DB 游标，连接归还连接池（spec 5.6.2 规则 5）
- **验收**：Given 查询 100 万行 → When `query.stream(conn).await` 并逐行消费 → Then 每次 poll 产出 1 行，峰值内存 ≤ 50 MB（spec 10.6 EARS）

### 6.2 实现 stream_buffered 兼容版
- [ ] 在 `QueryBuilder<M>` 新增 `pub fn stream_buffered(self, conn) -> Pin<Box<dyn Stream<Item = Result<RowResult, DbError>> + Send>>`：保留旧实现（全量收集后逐行 yield），作为逃生舱（design.md 2.2.2 P-F-6，RC-1 兼容性风险缓解）
- [ ] 文档标注 `stream_buffered` 为兼容版，推荐使用 `stream`（真游标）
- **验收**：Given 旧代码使用全量收集行为 → When 迁移到 `stream_buffered` → Then 行为与 v2.0.0 `stream` 完全一致（向后兼容）

### 6.3 适配 Oracle mpsc 桥接背压与错误传播
- [ ] Oracle 方言委托存量 `stream_cursor_paged`（`cursor_stream.rs:79`），基于 `ROWNUM` 分页 + mpsc 通道桥接（容量 64），复用 v2.0.0 已验证方案（R-5 风险缓解）
- [ ] 其他方言（MySQL/PG/SQLite/MSSQL）委托 `conn.query_stream_cursor` 真游标
- [ ] 游标 fetch 失败时 yield `Some(Err(DbError::ConnectionError))`，标记 `is_exhausted = true`，后续 `next()` 返回 `None`（spec 5.6.4 异常 2）
- [ ] 游标打开失败时 `stream()` 返回 `Err(DbError)`（spec 5.6.4 异常 1）
- **验收**：Given Oracle + 消费速度慢 → When `stream` → Then mpsc 通道满时背压，无内存溢出；迭代中连接断开 → yield `Some(Err(ConnectionError))`，后续 `None`

### 6.4 验证 Stream API 测试覆盖与五方言
- [ ] 编写集成测试 `tests/integration_stream_api.rs`：`stream_yields_rows_one_by_one`、`stream_error_propagation`、`stream_drop_releases_connection`、`stream_5_dialects`
- [ ] 编写单元测试 `stream_buffered_compatible`（保留旧行为）
- [ ] 编写基准测试 `stream_1m_rows_memory`：100 万行峰值内存 ≤ 50 MB（spec 4.1 性能 3）
- [ ] Oracle 集成测试 `stream_oracle_backpressure` 标注 `#[ignore]`（V-3）
- [ ] 为 `stream`、`stream_buffered` 编写 rustdoc + doctest
- **验收**：Given 5 方言环境 → When 执行 `stream_5_dialects` → Then 各方言逐行产出正确；100 万行峰值内存 ≤ 50 MB

---

## 7. 建立跨框架性能基准对比（P-F-7）

**目标**：使用 criterion 对 sz-orm / Diesel / SeaORM / SQLx 在 5 场景下量化对比吞吐量/延迟/内存，生成 Markdown 报告。
**对应需求**：spec.md 5.7 节，design.md 2.2.2 P-F-7 接口，ADR-v2.1.0-005（独立 bench crate）
**工时**：3-5 天
**依赖**：M1-M6（覆盖所有新功能场景）

### 7.1 创建 bench-framework-comparison 独立 crate
- [ ] 新建 `bench-framework-comparison/` 目录，创建 `Cargo.toml`：dev-dependency 引入 `diesel`、`sea-orm`、`sqlx`、`sz-orm-core`（path 引用）、`criterion`
- [ ] 在 `bench-framework-comparison/src/lib.rs` 定义 `BenchmarkResult` 结构体（scenario_name + framework_name + throughput_ops + avg_latency_us + p99_latency_us + peak_memory_mb）
- [ ] 在根 `Cargo.toml` workspace.members 新增 `bench-framework-comparison`
- [ ] 使用 dev-dependency + 独立 crate 隔离三方框架依赖，避免污染核心包（ADR-v2.1.0-005，R-4 风险缓解）
- **验收**：Given `cargo check --benches` → When 编译 bench-framework-comparison → Then 编译通过，三方框架依赖不进入 sz-orm-core

### 7.2 实现 5 场景 × 4 框架基准
- [ ] 在 `bench-framework-comparison/benches/crud_comparison.rs` 实现 5 个基准函数：`bench_single_insert`、`bench_single_select`、`bench_batch_insert_1000`、`bench_join_query`、`bench_pagination_query`
- [ ] 每个场景在 sz-orm / Diesel / SeaORM / SQLx 四框架下执行相同业务逻辑
- [ ] 所有框架使用相同连接池配置（pool_size=10）、相同数据库（SQLite 内存，消除网络差异）、相同数据集（1000 行）（spec 5.7.2 规则 4 公平性）
- [ ] 禁止使用 MockConnection，必须真 DB（spec 5.7.2 规则 7）
- [ ] 每组对比收集 4 维指标：吞吐量（ops/s）、平均延迟（μs）、P99 延迟（μs）、内存峰值（MB）（spec 5.7.2 规则 3）
- **验收**：Given `cargo bench --bench crud_comparison` → When 执行 → Then 输出 5 场景 × 4 框架 = 20 组数据，每组含 4 维指标

### 7.3 实现报告生成器与回归检测
- [ ] 在 `bench-framework-comparison/src/report.rs` 实现 `pub fn generate_comparison_report(results: &[BenchmarkResult], output_path: &Path) -> Result<(), DbError>`：渲染 Markdown 对比表格 + 结论
- [ ] 基准完成后生成 `docs/benchmark/v2.1.0-comparison.md`，含 20 组数据 + 对比表 + 结论（spec 5.7.2 规则 5）
- [ ] 配置 criterion 回归检测：性能回退超过 10% 时输出 `regression detected` 警告（spec 5.7.2 规则 6）
- [ ] 参照框架编译失败时跳过该框架，报告中标注 "compilation failed"（spec 5.7.4 异常 1）
- [ ] 内存测量失败时标注 "N/A"，其他指标正常输出（spec 5.7.4 异常 3）
- **验收**：Given `cargo bench` 完成 → Then `docs/benchmark/v2.1.0-comparison.md` 存在，含 20 组数据 + 对比表 + 结论（spec 10.7 EARS）

### 7.4 验证基准对比性能指标
- [ ] 编写集成测试 `bench_report_generated`：断言报告文件存在且含 20 组数据
- [ ] 编写基准测试 `bench_regression_detection`：性能回退 > 10% 触发告警
- [ ] 验证 sz-orm 单行 SELECT 吞吐量 ≥ SQLx × 0.67（延迟不超过 SQLx 1.5 倍，spec 4.1 性能 4）
- [ ] 验证 sz-orm CRUD 单行延迟 ≤ SeaORM × 1.2（spec 4.1 性能 4）
- **验收**：Given sz-orm 单行 SELECT 吞吐量 X → When 对比 SQLx → Then sz-orm 吞吐量 ≥ SQLx × 0.67（spec 10.7 EARS）

---

## 8. 集成验证与 v2.1.0 发布

**目标**：版本 bump 至 2.1.0，10 道门禁全通过，crates.io 发布，sz-pay 试点验证。
**对应需求**：spec.md 4.5 兼容性，design.md 7.2 修改文件
**工时**：2-3 天
**依赖**：M1-M7（全部功能完成）

### 8.1 版本 bump 与内部依赖对齐
- [ ] 在根 `Cargo.toml`（workspace）将 `bench-framework-comparison` 加入 members，workspace.package.version 从 2.0.0 升级至 2.1.0
- [ ] 在 `packages/sz-orm-core/Cargo.toml` 版本 bump 至 2.1.0，内部依赖对齐
- [ ] 在 `packages/*/Cargo.toml`（40 个包）将内部依赖 `sz-orm-core` 等 `version + path` 格式统一对齐至 2.1.0（design.md 7.2）
- [ ] 在 `CHANGELOG.md` 新增 v2.1.0 版本条目，记录 7 项功能新增
- **验收**：Given `cargo check --workspace` → When 编译 → Then 所有包版本一致为 2.1.0，无版本冲突

### 8.2 通过 10 道门禁
- [ ] 门禁 1 fmt：`cargo fmt --all -- --check` 通过
- [ ] 门禁 2 check：`$env:CARGO_INCREMENTAL=0; cargo check --workspace --all-targets` 通过
- [ ] 门禁 3 clippy：`$env:CARGO_INCREMENTAL=0; cargo clippy --workspace --all-targets -- -D warnings` 零警告
- [ ] 门禁 4 test：`$env:CARGO_INCREMENTAL=0; cargo test --workspace --lib` 全通过（含新增测试）
- [ ] 门禁 5 doc：`cargo doc --workspace --no-deps --all-features` 通过（含新增 doctest）
- [ ] 门禁 6 audit：`cargo audit` + `cargo deny check` 通过（dev-dependency 安全）
- [ ] 门禁 7 integration：`cargo test --workspace -- --ignored` 通过（Oracle/MSSQL 集成）
- [ ] 门禁 8 占位扫描：`grep -rn 'todo!\|unimplemented!\|unreachable!' --include='*.rs'` 生产代码 0 处（C-3）
- [ ] 门禁 9 SQL 注入：`scripts/check-sql-injection.ps1` 通过（新增 DDL/join 参数化，C-4）
- [ ] 门禁 10 feature 全组合：`cargo check --workspace --all-targets --all-features` 通过
- **验收**：Given 10 道门禁命令 → When 逐一执行 → Then 全部通过，无 warning 无 error（C-7）

### 8.3 验证向后兼容与不修改文件保证
- [ ] 确认 `packages/sz-orm-core/src/active_model.rs` 未修改（`ActiveModel<M>` 结构不变，嵌套通过 `NestedActiveModel` 包装，design.md 7.3）
- [ ] 确认 `packages/sz-orm-core/src/pool.rs` 未修改（`Connection` trait 不变，复用 v2.0.0 `query_stream_cursor`）
- [ ] 确认 `packages/sz-orm-core/src/cursor_stream.rs` 未修改（`stream_cursor_paged` 不变，被 Stream API 复用）
- [ ] 确认 `packages/sz-orm-core/src/migration.rs` 未修改（`Migration` 结构不变，Schema Sync 独立模块）
- [ ] 确认 `StreamQueryTrait` trait 签名不变（仅改 impl，ADR-v2.1.0-003）
- [ ] 确认 `#[derive(Relation)]` 仅追加 `impl RelationTrait`，不修改既有 `impl ModelExt`（RC-3）
- **验收**：Given v2.0.0 既有 API → When v2.1.0 编译 → Then 所有 v2.0.0 调用代码无需改动即可编译（C-9 向后兼容）

### 8.4 crates.io 发布与 sz-pay 试点验证
- [ ] 执行 `cargo publish` 将 42 包 @ 2.1.0 + bench-framework-comparison 发布到 crates.io（spec 4.5 兼容性 3）
- [ ] 在 sz-pay 项目（`E:\vue\test\sz-pay\server\sz-rust`）将 sz-orm-core 依赖从 2.0.0 升级至 2.1.0，验证编译通过
- [ ] 运行 sz-pay 测试套件，确认无回归
- [ ] 记录 sz-pay 试点验证结果附 `file:line` 证据
- **验收**：Given `cargo publish` → When 发布 42 包 → Then crates.io 上版本为 2.1.0；sz-pay 升级后编译 + 测试通过

---

## 任务依赖关系图

```
M1 (P-F-2 RelationTrait) ──┬──→ M3 (P-F-1 Eager Loading) ──→ M7 (P-F-7 基准) ──→ M8 (发布)
                            │        ↑
M2 (P-F-3 Partial Models) ──┘        │
                                     │
M1 ──→ M4 (P-F-5 嵌套持久化) ────────┘
                                     │
M5 (P-F-4 Schema Sync) ──────────────┤（独立并行，汇入 M7）
                                     │
M6 (P-F-6 Stream API) ───────────────┘（独立并行，汇入 M7）
```

**关键路径**：M1 → M2 → M3 → M4 → M7 → M8（21-30 天）
**并行分支**：M5（7-10 天）、M6（3-4 天）可与关键路径任意阶段并行
**总工期**：约 3-4 周（并行优化后）

---

## 风险与注意事项

### 技术风险（来自 design.md 6.1 节）

| # | 风险 | 等级 | 影响任务 | 缓解措施 |
|---|------|------|---------|----------|
| R-1 | `#[derive(Relation)]` 宏改复杂度高 | 🟠 中 | M1 延期影响 M3/M4 | 先实现最小可用 RelationTrait（仅 HasMany/HasOne），迭代扩展 BelongsTo/ManyToMany |
| R-2 | 五方言 DDL 语法差异大 | 🟠 中 | M5 工时超预期 | 优先 MySQL+PG+SQLite，Oracle/MSSQL 标注 experimental |
| R-3 | Eager Loading 多级嵌套组装复杂 | 🟡 低 | M3 多级场景受限 | v2.1.0 仅支持 2 级嵌套，3+ 级标注 v2.2.0（ADR-v2.1.0-006） |
| R-4 | 基准测试引入三方依赖冲突 | 🟡 低 | M7 编译失败 | dev-dependency + 独立 bench crate 隔离（ADR-v2.1.0-005） |
| R-6 | Eager Loading 生成 SQL 含 SELECT * | 🟠 中 | M3 不合规 | 使用 Partial Models 显式列名（依赖 M2） |
| R-7 | Schema Sync 破坏性 DDL 误执行 | 🔴 高 | M5 数据丢失 | 禁止自动生成 DROP，返回 DestructiveChangeDetected（ADR-v2.1.0-004） |
| R-8 | 嵌套持久化事务泄漏 | 🟠 中 | M4 数据不一致 | RAII 事务 guard，drop 时自动 rollback |

### 工程约束（来自 AGENTS.md，必须严格遵守）

1. **ADR-0001 铁律**：仅修改 sz-orm 仓库内文件，不修改上游 sz-rust 仓库
2. **unsafe 零容忍**：新增代码 0 处 `unsafe`，0 处 `todo!` / `unimplemented!` / `unreachable!`（C-2, C-3）
3. **参数化查询**：所有新增 API（join / select_only / Schema Sync DDL）必须参数化或标识符校验，禁止字符串拼接用户输入（C-4）
4. **禁止 SELECT ***：Eager Loading / join 生成 SQL 必须显式列名（C-5）
5. **API 向后兼容**：v2.1.0 无 Breaking Change，新增能力以扩展方法提供（C-9）
6. **五方言覆盖**：MySQL/PG/SQLite/Oracle/MSSQL 适配，MSSQL 无实例时方言适配 + `#[ignore]` 测试（C-10）
7. **审计合规**：每条验收结论附 `file:line` 证据 + `cargo test` 输出，禁止未验证即标记完成

### 注意事项

1. **M1 是关键基础设施**：M3（Eager Loading）和 M4（嵌套持久化）都依赖 M1 的 `RelationDef`，M1 延期会阻塞关键路径，建议优先投入
2. **M2 应先于 M3**：M3 的 Eager Loading 生成 SQL 需使用 M2 的 Partial Models 避免 SELECT *（R-6），若 M2 未完成 M3 可临时用 `select_columns` 字段但不合规
3. **M5/M6 可并行**：这两个任务独立于关键路径，可分配给不同开发者并行推进以缩短总工期
4. **M7 必须最后**：基准测试需覆盖所有新功能场景，必须在 M1-M6 完成后执行
5. **Oracle IN 列表限制**：M3 需处理 Oracle `IN` 列表上限 1000 的分批查询（design.md 3.1 节）
6. **SQLite 类型变更不支持**：M5 的 SQLite DdlGenerator 对类型变更返回 `Err(UnsupportedFeature)`，需提示用户手动重建表
7. **Stream 行为变更**：M6 改造 `stream` impl 从全量收集为真游标，虽 trait 签名不变但行为变更，需保留 `stream_buffered` 兼容版并文档标注（RC-1）

---

> **文档版本**：v1.0（v2.1.0 编码任务规划初版）
> **生成日期**：2026-08-06
> **生成方法**：基于 spec.md 需求规格 + design.md 技术设计 + AGENTS.md 工程规范
> **任务统计**：8 个里程碑（任务组），39 个子任务，覆盖 7 项功能（P-F-1 到 P-F-7）+ 发布验证
> **约束遵循**：tasks_example.md 格式 + 垂直切割 + 可验收 + 原子性 + 有序性 + 最大 2 层
> **下一步**：用户审查 → 确认/修改 → 移交编码实现