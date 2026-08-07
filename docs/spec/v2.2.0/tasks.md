# sz-orm v2.2.0 编码任务规划

> **版本**：v2.2.0
> **基线**：v2.1.0（43 包 @ 2.1.0 已发布 crates.io，4,993 测试通过，sz-pay 5,139 试点验证无回归）
> **生成日期**：2026-08-06
> **文档目的**：将 v2.2.0 八项需求的技术设计（design.md）转化为可执行、可验收的编码任务清单
> **依据**：
> - `docs/spec/v2.2.0/spec.md`（需求规格，8 项需求 / 38 条 EARS）
> - `docs/spec/v2.2.0/design.md`（技术设计，6 章，含接口签名 / 状态机 / 类图 / 兼容性分析 / 测试策略 / 里程碑）
> - `docs/spec/v2.1.0/tasks.md`（v2.1.0 任务基线，格式参照）
> - `AGENTS.md`（11 道门禁 + 五维审查 + AI 辅助开发 10 条硬约束 + 审计合规铁律）

---

## 任务总览

| 里程碑 | 任务组 | 对应需求 | 优先级 | 工时 | 依赖 |
|--------|--------|---------|--------|------|------|
| M1 | 1. AnyPool 扩展支持 Oracle/MSSQL | A-1 | 🔴 高 | 3-4 天 | 无 |
| M1 | 2. Dialect 与 AnyPool 运行时切换集成验证 | A-2 | 🔴 高 | 2-3 天 | 任务组 1 |
| M2 | 3. AnyPool 与 Pool 统一抽象 UnifiedPool | A-3 | 🔴 高 | 3-4 天 | M1（5 后端 AnyPool 就绪） |
| M3 | 4. Eager Loading 多级关联增强 + 循环检测 | B-1 | 🟠 中 | 5-6 天 | 无（与 M1/M2 可并行） |
| M3 | 5. Schema Sync 破坏性变更安全策略 | B-2 | 🟠 中 | 5-6 天 | 无（与 M1/M2 可并行） |
| M4 | 6. Partial Models select_exclude() | B-3 | 🟡 低 | 1-2 天 | 无 |
| M4 | 7. Stream API 背压控制 | B-4 | 🟡 低 | 2-3 天 | 无 |
| M4 | 8. 嵌套持久化 cascade_delete 策略 | B-5 | 🟡 低 | 2-3 天 | 无 |
| M5 | 9. 集成验证与 v2.2.0 发布 | 全部 | — | 2-3 天 | M1-M4 全部完成 |

**关键路径**：M1 (5-7d) → M2 (3-4d) → M5 (2-3d) = **10-14 天**（A 系列多后端 ORM 增强，sz-rust 集成驱动）
**并行路径**：M3 (10-12d) ∥ M4 (5-8d)，与关键路径无依赖，可并行开发
**总工期**：约 **3-4 周**（关键路径 + 并行分支取较长者）

**实施顺序约束**（来自 design.md §5.2）：
```
关键路径：M1（任务组 1 → 任务组 2）→ M2（任务组 3）→ M5（任务组 9）
并行分支 1：M3（任务组 4 ∥ 任务组 5，任意时间并行）
并行分支 2：M4（任务组 6 ∥ 任务组 7 ∥ 任务组 8，任意时间并行）
发布门：M1-M4 全部完成 → M5
```

---

## 1. AnyPool 扩展支持 Oracle/MSSQL（A-1）

**目标**：`AnyBackend` 枚举从 3 变体扩展至 5 变体（新增 Oracle/Mssql），`from_dsn` 识别 `oracle://`/`mssql://`/`sqlserver://` scheme，`AnyPool::connect` 新增 Oracle/MSSQL 分派分支（feature gate），未启用 feature 时返回明确错误。
**对应需求**：spec.md §5.1，design.md §2.2.2 A-1 接口
**工时**：3-4 天
**依赖**：无（关键路径第 1 步）
**风险等级**：🟡 中（R-1 AnyBackend 新增变体破坏外部 match、R-2 feature 编译矩阵膨胀、R-9 集成测试环境依赖）

### 1.1 扩展 AnyBackend 枚举至 5 变体并标注 #[non_exhaustive]
- [ ] 在 `packages/sz-orm-sqlx/src/any_driver.rs` 的 `AnyBackend` 枚举新增 `Oracle` 和 `Mssql` 两个变体，每个变体添加 `/// Oracle（v2.2.0 新增，需启用 oracle feature）` 文档注释
- [ ] 为 `AnyBackend` 枚举添加 `#[non_exhaustive]` 标注，强制外部 crate match 时使用 wildcard 臂（缓解 R-1，design.md §3.4 结论）
- [ ] 保留现有 `#[derive(Debug, Clone, Copy, PartialEq, Eq)]` 派生
- [ ] 更新 `AnyBackend::name()` 方法，追加 `Oracle => "oracle"` 和 `Mssql => "mssql"` 两个 match 臂
- **输入**：存量 `AnyBackend` 枚举（`any_driver.rs:43`，3 变体）
- **输出**：5 变体 `AnyBackend` 枚举 + `#[non_exhaustive]` 标注 + `name()` 5 臂
- **验证**：`AnyBackend::Oracle.name() == "oracle"` 且 `AnyBackend::Mssql.name() == "mssql"`；外部 match 无 wildcard 时编译失败（`#[non_exhaustive]` 生效）

### 1.2 扩展 from_dsn 识别 oracle/mssql/sqlserver scheme
- [ ] 在 `AnyBackend::from_dsn` 方法中追加 scheme 识别分支：`oracle://` → `Ok(AnyBackend::Oracle)`，`mssql://` 和 `sqlserver://` → `Ok(AnyBackend::Mssql)`
- [ ] 未知 scheme 的错误信息列出全部 5 种支持 scheme：`mysql/postgres/sqlite/oracle/mssql`（spec §5.1.2 规则 6）
- [ ] scheme 解析使用白名单校验，禁止任意 scheme（DFX 4.3.2）
- [ ] 编写单元测试 `test_any_backend_from_dsn_oracle`、`test_any_backend_from_dsn_mssql`、`test_any_backend_from_dsn_sqlserver`、`test_any_backend_from_dsn_unknown_v22`（错误信息含 5 种 scheme）
- **输入**：存量 `from_dsn`（`any_driver.rs:60`，识别 5 种 scheme：mysql/mariadb/postgres/postgresql/sqlite）
- **输出**：`from_dsn` 识别 8 种 scheme（含 oracle/mssql/sqlserver），未知 scheme 错误含 5 种支持列表
- **验证**：`from_dsn("oracle://sys:test123@127.0.0.1:1521/freepdb1")` 返回 `Ok(Oracle)`；`from_dsn("redis://localhost")` 返回 Err 含 `mysql/postgres/sqlite/oracle/mssql`

### 1.3 为 Oracle/MSSQL 后端添加 cargo feature gate
- [ ] 在 `packages/sz-orm-sqlx/Cargo.toml` 的 `[features]` 段新增 `oracle = ["dep:sz-orm-oracle"]` 和 `mssql = ["dep:sz-orm-mssql"]` feature，并将 `sz-orm-oracle`/`sz-orm-mssql` 依赖改为 optional（`optional = true`）
- [ ] 在 `packages/sz-orm-sqlx/src/any_driver.rs` 顶部添加 `#[cfg(feature = "oracle")] use sz_orm_oracle::...` 和 `#[cfg(feature = "mssql")] use sz_orm_mssql::...` 条件导入
- [ ] 在 `AnyPool::connect` 的 Oracle/Mssql 分支用 `#[cfg(feature = "oracle")]` / `#[cfg(feature = "mssql")]` gate，未启用 feature 时 `from_dsn` 返回 `Err(DbError::ConnectionRefused)` 含 `请启用 oracle feature` 提示（spec §5.1.4 异常 1）
- [ ] 在工作空间根 `Cargo.toml` 的 sz-orm-sqlx 依赖中添加 `features = ["oracle", "mssql"]` 默认启用（缓解 R-2，CI 仅验证关键组合）
- **输入**：存量 sz-orm-sqlx 无 oracle/mssql feature
- **输出**：sz-orm-sqlx 新增 oracle/mssql feature gate，未启用时明确报错
- **验证**：`cargo check --no-default-features` 编译通过（无 oracle/mssql）；启用 oracle feature 后 `AnyPool::connect("oracle://...")` 可分派；未启用时返回 Err 含 feature 提示

### 1.4 扩展 AnyPool::connect 新增 Oracle/MSSQL 分派分支
- [ ] 在 `AnyPool::connect` 方法的 match backend 中追加 `AnyBackend::Oracle =>` 分支：构造 `OracleConnectionFactory`（复用 `packages/sz-orm-oracle/src/lib.rs:634` 已有实现），返回 `AnyPool { backend: Oracle, factory: Arc::new(oracle_factory) }`
- [ ] 追加 `AnyBackend::Mssql =>` 分支：构造 `MssqlConnectionFactory`（复用 `packages/sz-orm-mssql/src/lib.rs:445` 已有实现），返回 `AnyPool { backend: Mssql, factory: Arc::new(mssql_factory) }`
- [ ] Oracle 分支使用 `OraclePoolHandle::connect(dsn)` 派发到 blocking_pool（复用 v2.0.0 blocking 机制，`oracle lib.rs:684`），MSSQL 分支使用 tiberius 异步 API（复用 v2.0.0 MssqlConnection）
- [ ] 保留现有 MySql/Postgres/Sqlite 3 分支不变（零 Breaking Change）
- **输入**：存量 `AnyPool::connect`（`any_driver.rs:100`，3 分支）
- **输出**：`AnyPool::connect` 5 分支，Oracle/MSSQL 复用已有 ConnectionFactory
- **验证**：`AnyPool::connect("oracle://...").await` 返回 Ok 且 `pool.backend() == Oracle`；`AnyPool::connect("mssql://...").await` 返回 Ok 且 `pool.backend() == Mssql`

### 1.5 验证 AnyPool 扩展 5 后端等价性与 feature gate 测试
- [ ] 编写单元测试 `test_any_backend_name_v22`（5 变体 name 正确）、`test_any_pool_feature_not_enabled`（未启用 oracle feature 时 connect 返回 Err 含提示）
- [ ] 编写集成测试 `test_any_pool_oracle_connect`（L5，`#[ignore]` 标注，连接 Oracle 23ai Free，CRUD 正常）、`test_any_pool_mssql_connect`（L6，`#[ignore]`，MSSQL 方言适配验证）
- [ ] 编写集成测试 `test_any_pool_5_backend_crud_equivalence`（L2-L6，同一表结构 5 后端 CRUD 结果按主键排序完全一致，spec §5.1.2 规则 5）
- [ ] 为 `AnyBackend` 新增变体和 `from_dsn` 扩展编写 rustdoc + doctest 示例
- [ ] 运行 `cargo check --workspace --all-targets --all-features` 验证 feature 全组合编译（门禁 10）
- **输入**：任务 1.1-1.4 实现完成
- **输出**：A-1 全部测试用例通过，5 后端 CRUD 等价性验证
- **验证**：`cargo test --workspace` 新增测试全通过；`cargo clippy --workspace --all-targets --all-features -- -D warnings` 零警告；Oracle/MSSQL 集成测试 `--ignored` 标注

---

## 2. Dialect 与 AnyPool 运行时切换集成验证（A-2）

**目标**：为 `AnyBackend` 和 `AnyPool` 新增 `dialect()` 方法，根据后端自动返回对应 Dialect（Oracle→OracleDialect，Mssql→SqlServerDialect），验证 OracleDialect/SqlServerDialect 与 AnyPool 端到端集成，补齐发现的缺口能力。
**对应需求**：spec.md §5.2，design.md §2.2.2 A-2 接口
**工时**：2-3 天
**依赖**：任务组 1（AnyBackend 5 变体就绪）
**风险等级**：🟡 中（R-9 Oracle/MSSQL 集成测试环境依赖）

### 2.1 实现 AnyBackend::dialect() 方法返回对应 Dialect
- [ ] 在 `packages/sz-orm-sqlx/src/any_driver.rs` 为 `AnyBackend` 新增 `pub fn dialect(&self) -> Box<dyn Dialect>` 方法，match backend 返回：MySql→MySqlDialect、Postgres→PostgreSqlDialect、Sqlite→SqliteDialect、Oracle→OracleDialect、Mssql→SqlServerDialect
- [ ] Dialect 实例从 `sz_orm_core::dialect` 模块导入（OracleDialect `dialect.rs:939`、SqlServerDialect `dialect.rs:1185` 已存在，复用）
- [ ] 为 `AnyPool` 新增 `pub fn dialect(&self) -> Box<dyn Dialect>` 方法，委托 `self.backend.dialect()`
- [ ] 编写单元测试 `test_any_backend_dialect_mapping`（5 后端 → 5 Dialect 映射正确）
- **输入**：存量 8 种 Dialect 实现（`dialect.rs`，OracleDialect/SqlServerDialect 已实现）
- **输出**：`AnyBackend::dialect()` 和 `AnyPool::dialect()` 方法，5 后端自动选择 Dialect
- **验证**：`AnyBackend::Oracle.dialect()` 返回 OracleDialect 实例；`AnyPool::connect("oracle://...").await?.dialect()` 返回 OracleDialect

### 2.2 验证 OracleDialect/SqlServerDialect 无占位实现
- [ ] 对 `OracleDialect` 和 `SqlServerDialect` 的全部 Dialect trait 方法（quote/escape_string/supports_returning/build_pagination/json_type/json_extract/full_text_search/auto_increment_keyword/last_insert_id_sql/build_create_table/build_alter_table/build_drop_table/build_upsert_conflict 等 20+ 方法）逐一调用，确认无 `todo!`/`unimplemented!`/`unreachable!`（DFX 4.3.5，门禁 8）
- [ ] 编写单元测试 `test_oracle_dialect_no_placeholder` 和 `test_mssql_dialect_no_placeholder`，对每个 trait 方法调用并断言不 panic
- [ ] 运行 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-core/src/dialect.rs` 确认 OracleDialect/SqlServerDialect 实现区域无占位
- **输入**：存量 OracleDialect（`dialect.rs:939`）、SqlServerDialect（`dialect.rs:1185`）
- **输出**：确认 2 个 Dialect 全方法实现无占位，测试覆盖
- **验证**：单元测试通过；门禁 8 占位扫描无新增 todo!/unimplemented!

### 2.3 验证 5 方言分页 SQL 生成正确性与等价性
- [ ] 编写单元测试 `test_oracle_dialect_pagination`：`OracleDialect.build_pagination(sql, 2, 10)` 含 `OFFSET ... FETCH NEXT`，不含 `LIMIT`（spec §5.2.2 规则 5）
- [ ] 编写单元测试 `test_mssql_dialect_pagination`：`SqlServerDialect.build_pagination(sql, 2, 10)` 含 `OFFSET ... FETCH NEXT`
- [ ] 编写集成测试 `test_any_pool_oracle_dialect_auto_select`（L5，`#[ignore]`）：通过 AnyPool 创建 Oracle 连接，执行分页查询，验证生成 SQL 使用 OracleDialect 语法（ROWNUM/FETCH FIRST）
- [ ] 编写集成测试 `test_5_dialect_pagination_equivalence`（L2-L6）：同一表结构与数据，5 种 Dialect 生成第 2 页（每页 10 行）分页 SQL 并执行，5 种数据库返回相同 10 行结果（spec §5.2.2 规则 4）
- **输入**：任务 2.1 dialect() 方法 + 存量 5 Dialect 实现
- **输出**：5 方言分页 SQL 正确性 + 等价性验证
- **验证**：Oracle 分页 SQL 不含 `LIMIT`；5 方言同数据分页返回相同结果集

### 2.4 验证 Dialect 与 AnyConnection 端到端协同
- [ ] 编写集成测试（L5，`#[ignore]`）：通过 `AnyPool::connect("oracle://...")` 创建连接，执行 `OracleDialect` 生成的 CREATE TABLE + INSERT + SELECT 分页 SQL，验证 Oracle 数据库执行成功返回正确结果集（spec §5.2.2 规则 3）
- [ ] 编写集成测试（L6，`#[ignore]`）：MSSQL 同理，验证 `SqlServerDialect` 生成的 SQL 在 SQL Server 执行成功
- [ ] 为 `AnyBackend::dialect()` 和 `AnyPool::dialect()` 编写 rustdoc + doctest
- [ ] 运行 `cargo test --workspace -- --ignored` 验证 Oracle/MSSQL 集成测试（本机有实例时）
- **输入**：任务 2.1-2.3 实现 + Oracle 23ai Free / SQL Server 本机实例
- **输出**：Dialect 与 AnyConnection 端到端协同验证
- **验证**：Oracle/MSSQL 集成测试通过（本机有实例时）；无实例时 `#[ignore]` 跳过，方言适配单元测试覆盖

---

## 3. AnyPool 与 Pool 统一抽象 UnifiedPool（A-3）

**目标**：新增 `UnifiedPool` 类型，包装 `sz_orm_core::Pool`（完整连接池）+ `AnyBackend`，提供 `connect`/`connect_with_config`/`from_pool`/`acquire`/`dialect`/`backend`/`resize`/`close_all`/`metrics` 方法，供 sz-rust AppState 持有单一类型 `Arc<UnifiedPool>`，零成本迁移。
**对应需求**：spec.md §5.3，design.md §2.2.2 A-3 接口
**工时**：3-4 天
**依赖**：M1（5 后端 AnyPool 就绪，任务组 1+2 完成）
**风险等级**：🟡 中（R-10 UnifiedPool 适配层性能开销、R-8 sz-pay 回归）

### 3.1 新建 unified_pool.rs 模块定义 UnifiedPool 结构
- [ ] 在 `packages/sz-orm-sqlx/src/unified_pool.rs` 新建模块，定义 `pub struct UnifiedPool { backend: AnyBackend, pool: sz_orm_core::Pool }`，字段为私有（pub struct 但字段无 pub）
- [ ] 派生 `Debug`（委托 Pool 的 Debug）；不派生 Clone（Pool 内部 Arc 可 clone，但语义上 UnifiedPool 为共享句柄，使用 `Arc<UnifiedPool>` 共享）
- [ ] 在 `packages/sz-orm-sqlx/src/lib.rs` 注册 `pub mod unified_pool;` 并 re-export `UnifiedPool`
- [ ] 在 `packages/sz-orm-sqlx/Cargo.toml` 确认 `sz-orm-core` 依赖已存在（复用 Pool）
- **输入**：存量 `Pool`（`pool.rs:712`，完整连接池）+ `AnyBackend`（任务组 1 扩展至 5 变体）
- **输出**：`UnifiedPool` 结构定义 + 模块注册
- **验证**：`cargo check` 编译通过；`UnifiedPool` 字段私有，外部仅通过方法访问

### 3.2 实现 UnifiedPool::connect 和 connect_with_config 方法
- [ ] 实现 `pub async fn connect(dsn: &str) -> Result<Self, DbError>`：① `AnyBackend::from_dsn(dsn)` 识别后端 → ② match backend 选择对应 `ConnectionFactory`（复用任务组 1 的分派逻辑）→ ③ `Pool::new(default_config, Arc::new(factory))` 构造完整连接池 → ④ 返回 `UnifiedPool { backend, pool }`
- [ ] 实现 `pub async fn connect_with_config(dsn: &str, config: PoolConfig) -> Result<Self, DbError>`：同 connect 但使用自定义 PoolConfig（max_size/timeout/断路器/限流器配置）
- [ ] 默认 PoolConfig 复用 v2.1.0 默认值（max_size=10, timeout=30s 等）
- [ ] 编写单元测试 `test_unified_pool_connect_5_backends`（L2-L6，5 种 DSN 创建 UnifiedPool，backend() 返回 5 种，CRUD 正常）
- **输入**：任务组 1 AnyPool 5 后端分派 + 存量 Pool::new
- **输出**：`UnifiedPool::connect` / `connect_with_config` 方法
- **验证**：5 种 DSN 创建 UnifiedPool 成功，backend() 返回对应 AnyBackend

### 3.3 实现 UnifiedPool::from_pool 零成本迁移路径
- [ ] 实现 `pub fn from_pool(pool: sz_orm_core::Pool, backend: AnyBackend) -> Self`：直接包装已有 Pool + backend，无额外开销（design.md §2.2.2 迁移路径）
- [ ] 编写迁移文档示例：sz-rust 从 `Arc<Pool>` 迁移到 `Arc<UnifiedPool>` 的代码变更（旧代码 `AppState { pool: Arc<Pool> }` → 新代码 `AppState { pool: Arc<UnifiedPool> }`）
- [ ] 编写单元测试 `test_unified_pool_from_pool_migration`：`UnifiedPool::from_pool(pool, backend)` 行为与直接 connect 一致（acquire/release/metrics）
- **输入**：存量 sz-rust `Arc<Pool>` 用法
- **输出**：`from_pool` 零成本迁移方法 + 迁移文档
- **验证**：from_pool 包装后 acquire() 行为与原 Pool 一致；sz-rust 迁移编译通过

### 3.4 实现 UnifiedPool 委托 Pool 的全部方法
- [ ] 实现 `pub async fn acquire(&self) -> Result<PooledConnection, PoolError>`：委托 `self.pool.acquire()`，添加 `#[inline]` 缓解 R-10 性能开销
- [ ] 实现 `pub fn backend(&self) -> AnyBackend`：返回 `self.backend`（Copy 语义）
- [ ] 实现 `pub fn dialect(&self) -> Box<dyn Dialect>`：委托 `self.backend.dialect()`（任务组 2 实现）
- [ ] 实现 `pub fn resize(&self, new_max: u32)`：委托 `self.pool.resize(new_max)`，`#[inline]`
- [ ] 实现 `pub async fn close_all(&self)`：委托 `self.pool.close_all().await`，`#[inline]`
- [ ] 实现 `pub fn metrics(&self) -> PoolMetrics`：委托 `self.pool.metrics()`，`#[inline]`
- [ ] 确保不丢失 Pool 的任何现有能力（连接复用/超时/断路器/限流/监控/resize/close_all，spec §5.3.2 规则 7）
- **输入**：存量 Pool 的全部公开方法
- **输出**：UnifiedPool 委托方法，语义与 Pool 完全一致
- **验证**：UnifiedPool 的 acquire/resize/close_all/metrics 行为与 Pool 一致（测试覆盖）

### 3.5 验证 UnifiedPool 语义保持与性能开销
- [ ] 编写集成测试 `test_unified_pool_connection_reuse`（L2）：max_size=10，连续 acquire 20 次 release 10 次，实际创建连接数 ≤ 10（spec §5.3.2 规则 3）
- [ ] 编写集成测试 `test_unified_pool_circuit_breaker`（L2）：配置断路器阈值=5，连续 5 次连接失败，断路器跳闸拒绝 acquire（spec §5.3.2 规则 4）
- [ ] 编写集成测试 `test_unified_pool_metrics`（L2）：acquire/release 后 metrics() 计数正确
- [ ] 编写集成测试 `test_unified_pool_dialect_binding`（L2-L6）：`UnifiedPool::connect("oracle://...").dialect()` 返回 OracleDialect（spec §5.3.2 规则 6）
- [ ] 编写集成测试 `test_unified_pool_resize_close_all`（L2）：resize/close_all 行为与 Pool 一致
- [ ] 编写基准测试 `unified_pool_overhead`：UnifiedPool::acquire 耗时 ≤ 直接 Pool::acquire × 1.05（DFX 4.1.2，开销 ≤5%）
- [ ] 为 UnifiedPool 全部方法编写 rustdoc + doctest
- **输入**：任务 3.1-3.4 实现完成
- **输出**：UnifiedPool 语义保持 + 性能验证
- **验证**：连接复用/断路器/监控/resize 语义与 Pool 一致；适配开销 ≤ 5%

---

## 4. Eager Loading 多级关联增强 + 循环检测（B-1）

**目标**：`EagerLoader::with()` 从限 2 级扩展至无限级，`ChildLoadConfig` 改为递归结构，新增 `NestedEagerResult` 递归枚举类型和 `load_nested` 方法，新增 `CyclePolicy` 枚举和 `CycleDetector` 实现循环检测（Error/Truncate/AllowWithDepthLimit 三策略）。
**对应需求**：spec.md §5.4，design.md §2.2.2 B-1 接口
**工时**：5-6 天
**依赖**：无（与 M1/M2 可并行）
**风险等级**：🟠 高（R-3 无限级递归栈溢出、R-4 循环检测误判）

### 4.1 新建 cycle_detection.rs 模块定义 CyclePolicy 和 CycleDetector
- [ ] 在 `packages/sz-orm-core/src/cycle_detection.rs` 新建模块，定义 `pub enum CyclePolicy { Error, Truncate, AllowWithDepthLimit(usize) }`，派生 `Debug + Clone`
- [ ] 为 `CyclePolicy` 实现 `Default`，返回 `CyclePolicy::Truncate`（spec §6.4 默认值，安全截断）
- [ ] 定义 `pub struct CycleDetector { policy: CyclePolicy, visited: std::collections::HashSet<String>, current_depth: usize }`
- [ ] 实现 `CycleDetector::new(policy) -> Self`、`check(&mut self, entity_type: &str) -> Result<bool, DbError>`（true=继续递归，false=终止）、`enter(&mut self, entity_type: &str)`、`leave(&mut self)`
- [ ] `check` 方法逻辑：已访问该 entity 类型时按 policy 分支——Error→返回 Err 含循环路径，Truncate→返回 false（终止），AllowWithDepthLimit(n)→深度超限返回 false（缓解 R-3）
- [ ] 循环检测按 entity 类型 + 关联名联合去重（`User::manager` ≠ `User::orders`，缓解 R-4 误判）
- [ ] 在 `packages/sz-orm-core/src/lib.rs` 注册 `pub mod cycle_detection;` 并 re-export
- **输入**：无（全新模块）
- **输出**：`CyclePolicy` 枚举 + `CycleDetector` 类型
- **验证**：`CycleDetector::new(CyclePolicy::Error).check("User")` 首次返回 Ok(true)，再次返回 Err 含循环路径

### 4.2 改造 ChildLoadConfig 为递归结构支持无限级
- [ ] 在 `packages/sz-orm-core/src/eager_loader.rs` 将 `ChildLoadConfig` 改为递归结构：新增 `children: Vec<ChildLoadConfig>` 字段（design.md §1.1.2，存量仅含 relation 无 children）
- [ ] `ChildLoadConfig` 为私有 struct，外部不可见，不影响 API 兼容（design.md §3.2 兼容性分析）
- [ ] 修改 `EagerLoader::with` 方法，将 relation 追加到当前层级的 children，支持链式无限级调用（`EagerLoader::new(rel1).with(rel2).with(rel3).with(rel4)` 构建 4 级链）
- [ ] 新增 `EagerLoader::with_cycle_policy(mut self, policy: CyclePolicy) -> Self` 方法设置循环检测策略
- [ ] 保留存量 `with()` 2 级调用行为不变（零 Breaking Change，design.md §3.2）
- **输入**：存量 `ChildLoadConfig`（`eager_loader.rs:44`，仅 relation 字段，限 2 级）
- **输出**：递归 `ChildLoadConfig` + `with_cycle_policy` 方法
- **验证**：`EagerLoader::new(rel1).with(rel2).with(rel3).with(rel4)` 构建 4 级链；存量 2 级调用行为不变

### 4.3 定义 NestedEagerResult 递归枚举类型
- [ ] 在 `packages/sz-orm-core/src/eager_loader.rs` 新增 `pub enum NestedEagerResult { Leaf(HashMap<String, Value>), Node { row: HashMap<String, Value>, children: Vec<NestedEagerResult> } }`，派生 `Debug + Clone`
- [ ] 保留存量 `EagerResult = (HashMap<String, Value>, Vec<HashMap<String, Value>>)` 类型不变（向后兼容，design.md §3.2）
- [ ] 为 `NestedEagerResult` 实现 helper 方法：`row() -> &HashMap<String, Value>`、`children() -> &[NestedEagerResult]`、`is_leaf() -> bool`
- [ ] 在 lib.rs re-export `NestedEagerResult`
- **输入**：存量 `EagerResult`（扁平 2 级）
- **输出**：`NestedEagerResult` 递归枚举（无限级嵌套树）
- **验证**：`NestedEagerResult::Leaf(row).is_leaf() == true`；`Node { row, children }.children()` 返回子级

### 4.4 实现 EagerLoader::load_nested 多级加载方法
- [ ] 在 `EagerLoader` 新增 `pub async fn load_nested(&self, conn: &mut dyn Connection, main_sql: &str) -> Result<Vec<NestedEagerResult>, DbError>` 方法
- [ ] 执行流程（design.md §2.1.3 B-1 流程图）：① 初始化 CycleDetector(cycle_policy) → ② 执行主表 SQL → ③ 递归 `load_children`：提取父级主键列表 → `WHERE fk IN (?, ...)` 批量查询（参数化，禁止 SELECT *）→ 按外键分组组装 → 递归子级 → ④ 返回 NestedEagerResult 树
- [ ] 每级批量查询使用 `WHERE fk IN (?, ?, ...)` 参数化（DFX 4.3.1），显式列出列名禁止 `SELECT *`（spec §5.4.2 规则 6）
- [ ] Oracle IN 列表 >1000 时分批查询（复用存量 Oracle 限制处理，`eager_loader.rs` 已有逻辑）
- [ ] 循环检测：每级递归前 `cycle_detector.check(entity_type)`，终止时返回已加载部分
- [ ] 深度超内存限制时返回 `Err(DbError::MemoryLimitExceeded)` 含建议改用 Stream API（spec §5.4.4 异常 1）
- **输入**：任务 4.1 CycleDetector + 任务 4.2 递归 ChildLoadConfig + 任务 4.3 NestedEagerResult
- **输出**：`load_nested` 方法，无限级批量查询 + 循环检测
- **验证**：4 级关联 User→Order→OrderItem→Product 返回 4 级嵌套树，组装正确；N1QueryDetector 无告警

### 4.5 验证多级 Eager Loading 测试覆盖与循环安全
- [ ] 编写集成测试 `test_eager_load_4_level`（L2）：User→Order→OrderItem→Product 4 级，返回 NestedEagerResult 4 级嵌套树，组装正确（spec §5.4.2 规则 5）
- [ ] 编写集成测试 `test_eager_load_batch_query`（L2）：100 User→300 Order→900 OrderItem，执行 3 条 SQL（每级 1 条），N1QueryDetector 无告警（spec §5.4.2 规则 2）
- [ ] 编写集成测试 `test_eager_load_cycle_truncate`（L2）：User→Order→User 循环，CyclePolicy::Truncate，终止递归返回已加载部分（spec §5.4.2 规则 3）
- [ ] 编写集成测试 `test_eager_load_cycle_error`（L2）：CyclePolicy::Error，返回 Err 含循环路径 "User → Order → User"
- [ ] 编写集成测试 `test_eager_load_cycle_depth_limit`（L2）：CyclePolicy::AllowWithDepthLimit(3)，递归至深度 3 终止
- [ ] 编写集成测试 `test_eager_load_no_select_star`（L2）：多级 Eager Loading 生成的所有 SQL 不含 `SELECT *`
- [ ] 编写集成测试 `test_eager_load_memory_limit`（L2）：结果集超内存限制返回 Err 含建议
- [ ] 为 `CyclePolicy`、`NestedEagerResult`、`load_nested`、`with_cycle_policy` 编写 rustdoc + doctest
- **输入**：任务 4.1-4.4 实现完成
- **输出**：B-1 全部测试用例通过
- **验证**：4 级加载组装正确；循环 3 策略正确；无 SELECT *；无 N+1

---

## 5. Schema Sync 破坏性变更安全策略（B-2）

**目标**：新增 `destructive_sync(conn, Confirm, hooks)` 方法显式执行破坏性 DDL（DROP COLUMN/RENAME COLUMN），新增 `Confirm` 枚举和 `DataMigrationHook` trait，`diff_columns` 新增 Levenshtein 重命名检测，破坏性 DDL 经 sz-orm-audit 审计日志，事务内原子执行。
**对应需求**：spec.md §5.5，design.md §2.2.2 B-2 接口
**工时**：5-6 天
**依赖**：无（与 M1/M2 可并行）
**风险等级**：🟡 中（R-5 重命名检测误判、R-6 破坏性 DDL 事务不支持）

### 5.1 实现 diff_columns 列重命名检测（Levenshtein 启发式）
- [ ] 在 `packages/sz-orm-core/src/schema_sync.rs` 的 `diff_columns` 函数新增重命名检测逻辑：对 dropped_columns 与 added_columns 做笛卡尔积，计算 Levenshtein 距离
- [ ] 重命名判定条件：Levenshtein 距离 ≤ 2 或编辑距离/长度 ≤ 0.3 且类型兼容 → 识别为 RenameColumn，从 dropped_columns/added_columns 移除，填入 `renamed_columns` 字段（spec §6.5，design.md §1.1.2）
- [ ] 类型兼容性校验：类型不同不识别为重命名（缓解 R-5 误判，如 `VARCHAR→INT` 不识别）
- [ ] 阈值可配置：通过 `SchemaSync::with_rename_threshold(max_distance, max_ratio)` 方法配置
- [ ] 编写单元测试 `test_destructive_sync_rename_detection`：`user_name`→`username`（类型同），diff 返回 RenameColumn，非 DropColumn+AddColumn（spec §5.5.2 规则 3）
- **输入**：存量 `diff_columns`（`schema_sync.rs:155`，仅检测 added/dropped/type_changed，renamed_columns 始终为空）
- **输出**：`diff_columns` 填充 renamed_columns，Levenshtein 启发式
- **验证**：`user_name`→`username` 识别为 RenameColumn；`name`→`title`（距离 4）不识别

### 5.2 定义 Confirm 枚举和 DataMigrationHook trait
- [ ] 在 `packages/sz-orm-core/src/schema_sync.rs` 新增 `#[derive(Debug, Clone, Copy)] pub enum Confirm { Yes, No }`
- [ ] 新增 `pub trait DataMigrationHook: Send + Sync`，含 `before_drop_column<'a>(&'a self, conn: &'a mut dyn Connection, table: &'a str, column: &'a str) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>>` 和 `before_rename_column<'a>(...)` 方法（手动解糖 async，与 Connection trait 一致，design.md §2.2.2）
- [ ] 新增 `#[derive(Debug, Clone)] pub struct DestructiveSyncResult { pub executed_ddl: Vec<String>, pub hooks_called: usize, pub audit_entries: usize }`
- [ ] 在 lib.rs re-export `Confirm`、`DataMigrationHook`、`DestructiveSyncResult`
- **输入**：无（新增类型）
- **输出**：`Confirm` 枚举 + `DataMigrationHook` trait + `DestructiveSyncResult` 结构
- **验证**：`Confirm::Yes` 和 `Confirm::No` 可构造；DataMigrationHook trait 可实现

### 5.3 实现 SchemaSync::destructive_sync 方法
- [ ] 在 `SchemaSync` 新增 `pub async fn destructive_sync(&self, conn: &mut dyn Connection, confirm: Confirm, hooks: Option<&dyn DataMigrationHook>) -> Result<DestructiveSyncResult, DbError>` 方法
- [ ] 执行流程（design.md §2.1.3 B-2 流程图）：① 校验 `confirm == Confirm::Yes`，否则返回 Err 要求显式确认（spec §5.5.2 规则 2）→ ② introspect(conn) → db_tables → ③ diff(entity_tables, db_tables) 含重命名检测 → ④ 生成 DDL 序列（含 DROP/RENAME）→ ⑤ BEGIN TRANSACTION → ⑥ 逐条 DDL：破坏性 DDL 前调用对应钩子（before_drop_column/before_rename_column）→ 执行 DDL → 记录审计日志 → ⑦ COMMIT → ⑧ 返回 DestructiveSyncResult
- [ ] 钩子失败时 ROLLBACK 返回 Err 含钩子失败原因（spec §5.5.4 异常 2）
- [ ] DDL 执行失败时 ROLLBACK 返回 Err 含 DDL 与底层错误（spec §5.5.4 异常 3）
- [ ] 不支持事务的方言（SQLite 部分 DDL）返回 Err 含已执行 DDL 列表 + 未执行 DDL 列表（spec §5.5.4 异常 4，缓解 R-6）
- [ ] 保留存量 `sync()` 和 `sync_dry_run()` 非破坏性语义不变（零 Breaking Change）
- **输入**：存量 `SchemaSync`（`schema_sync.rs:483`）+ 任务 5.1 重命名检测 + 任务 5.2 新类型
- **输出**：`destructive_sync` 方法，事务内执行 + 钩子 + 审计
- **验证**：`destructive_sync(Confirm::No)` 返回 Err；`destructive_sync(Confirm::Yes)` 执行破坏性 DDL

### 5.4 集成 sz-orm-audit 审计日志记录破坏性 DDL
- [ ] 在 `destructive_sync` 执行每条破坏性 DDL（DROP/RENAME）后，调用 `sz-orm-audit` 记录审计日志，含操作人、时间戳、DDL 内容、受影响行数（spec §5.5.2 规则 5，DFX 4.3.3）
- [ ] 复用 v2.0.0 已有的 sz-orm-audit 包（哈希链防篡改，`packages/sz-orm-audit`）
- [ ] 在 `packages/sz-orm-core/Cargo.toml` 确认 sz-orm-audit 依赖（若非已有则添加，optional feature gate）
- [ ] 编写集成测试 `test_destructive_sync_audit`：执行 destructive_sync 删除列，验证 sz-orm-audit 记录含 DDL/时间/受影响行数
- **输入**：存量 sz-orm-audit 包
- **输出**：破坏性 DDL 审计日志记录
- **验证**：destructive_sync 后审计日志含 DROP COLUMN DDL、操作时间、受影响行数

### 5.5 验证破坏性 Schema Sync 测试覆盖与事务原子性
- [ ] 编写集成测试 `test_sync_no_destructive`（L2）：`sync()` 不生成 DROP COLUMN，仅记录 diff（spec §5.5.2 规则 7）
- [ ] 编写集成测试 `test_destructive_sync_drop_column`（L2）：`destructive_sync(Yes)` 生成 `ALTER TABLE ... DROP COLUMN`（spec §5.5.2 规则 1）
- [ ] 编写集成测试 `test_destructive_sync_no_confirm`（L2）：`destructive_sync(No)` 返回 Err 要求显式确认
- [ ] 编写集成测试 `test_destructive_sync_hook`（L2）：注册 before_drop_column 钩子，执行 destructive_sync，钩子先执行备份 SQL（spec §5.5.2 规则 4）
- [ ] 编写集成测试 `test_destructive_sync_transaction_rollback`（L2）：3 条 DDL，第 2 条失败，第 1 条回滚（spec §5.5.2 规则 6）
- [ ] 编写集成测试 `test_destructive_sync_sqlite_no_transaction`（L2）：SQLite 部分 DDL 不支持事务，失败时返回 Err 含已执行 DDL 列表
- [ ] 为 `Confirm`、`DataMigrationHook`、`destructive_sync` 编写 rustdoc + doctest
- **输入**：任务 5.1-5.4 实现完成
- **输出**：B-2 全部测试用例通过
- **验证**：sync() 非破坏性；destructive_sync(Yes) 执行破坏性 DDL；事务回滚正确；审计日志记录

---

## 6. Partial Models select_exclude()（B-3）

**目标**：`QueryBuilder` 新增 `select_exclude(fields: &[&str])` 方法，排除指定字段查询（与 `select_only` 互补），校验排除字段存在、不排除全部字段，自动进入 Partial 模式。
**对应需求**：spec.md §5.6，design.md §2.2.2 B-3 接口
**工时**：1-2 天
**依赖**：无
**风险等级**：🟢 低

### 6.1 实现 QueryBuilder::select_exclude 方法
- [ ] 在 `packages/sz-orm-core/src/query.rs` 的 `QueryBuilder<M>` 新增 `pub fn select_exclude(mut self, fields: &[&str]) -> Result<Self, DbError>` 方法
- [ ] 实现逻辑：① 读取实体全部列名（通过 `M::column_names()` 或元数据）→ ② 减去排除列得到保留列 → ③ 校验排除列存在（不存在返回 `Err(DbError::InvalidInput)` 含字段名，spec §5.6.2 规则 3）→ ④ 校验不排除全部列（全排除返回 Err "不能排除所有字段"，spec §5.6.2 规则 4）→ ⑤ 设置 `self.select_mode = SelectMode::Partial` + `self.select_columns = 保留列` → ⑥ 返回 Self
- [ ] 排除主键字段时允许但返回 warning（日志含 `排除主键字段可能导致实体无法标识`，spec §5.6.4 异常 3）
- [ ] 生成的 SQL 显式列出保留列名，禁止 `SELECT *`（spec §5.6.2 规则 6）
- [ ] 保留存量 `select_only` 方法不变（零 Breaking Change）
- **输入**：存量 `select_only`（`query.rs:1088`）+ `SelectMode` 枚举（`partial_model.rs`）
- **输出**：`select_exclude` 方法，与 select_only 互补
- **验证**：`User::find().select_exclude(&["avatar", "blob_data"])?` 生成 SQL 不含这两列，含其余列

### 6.2 验证 select_exclude 测试覆盖与互补性
- [ ] 编写单元测试 `test_select_exclude_basic`（L1）：`select_exclude(&["avatar","blob_data"])` 生成 SQL 不含这两列
- [ ] 编写单元测试 `test_select_exclude_complement`（L1）：表 5 列，`select_exclude(&["d","e"])` 等价 `select_only(&["a","b","c"])`（spec §5.6.2 规则 2）
- [ ] 编写单元测试 `test_select_exclude_nonexistent`（L1）：排除不存在字段返回 Err 含字段名
- [ ] 编写单元测试 `test_select_exclude_all_fields`（L1）：排除所有字段返回 Err "不能排除所有字段"
- [ ] 编写单元测试 `test_select_exclude_no_select_star`（L1）：生成的 SQL 不含 `SELECT *`，显式列出保留列
- [ ] 编写集成测试 `test_select_exclude_with_eager`（L2）：`select_exclude(&["avatar"]).with(Order)`，User 不含 avatar，Order 全字段（spec §5.6.2 规则 5）
- [ ] 为 `select_exclude` 编写 rustdoc + doctest
- **输入**：任务 6.1 实现完成
- **输出**：B-3 全部测试用例通过
- **验证**：排除正确；互补性成立；不存在字段报错；全排除报错；无 SELECT *

---

## 7. Stream API 背压控制（B-4）

**目标**：`StreamApiExt` 新增 `stream_with_backpressure(buffer_size)` 方法，使用 `tokio::sync::mpsc::channel(buffer_size)` 桥接生产者（游标 fetch）与消费者，缓冲区满时生产者阻塞等待，控制内存占用，buffer_size=0 返回 Err。
**对应需求**：spec.md §5.7，design.md §2.2.2 B-4 接口
**工时**：2-3 天
**依赖**：无
**风险等级**：🟠 高（R-7 生产者任务泄漏）

### 7.1 实现 stream_with_backpressure 方法
- [ ] 在 `packages/sz-orm-core/src/stream_api.rs` 的 `StreamApiExt<M>` trait 新增 `fn stream_with_backpressure<'a, 'b: 'a, C: Connection + Send + 'b>(self, conn: &'b mut C, buffer_size: usize) -> Pin<Box<dyn Stream<Item = Result<RowResult, DbError>> + Send + 'a>>` 方法
- [ ] 实现逻辑：① 校验 buffer_size > 0，否则返回 Err（spec §5.7.2 规则 7，buffer_size=0 报错）→ ② 创建 `tokio::sync::mpsc::channel(buffer_size)` → ③ spawn 生产者任务：游标 fetch next row → `sender.send(row).await`（缓冲区满时阻塞，spec §5.7.2 规则 2）→ ④ 返回 Receiver 转 Stream
- [ ] 生产者任务持有 `Sender` clone，消费者 drop 时 `Receiver` 关闭，生产者检测到 `send` 返回 Err 终止游标（缓解 R-7，spec §5.7.4 异常 3）
- [ ] 使用 `tokio::select!` 监听 receiver 关闭信号，及时终止生产者并释放连接
- [ ] 数据库连接断开时 Stream yield `Err(DbError::ConnectionError)` 后终止（spec §5.7.2 规则 5）
- [ ] 保留存量 `stream_buffered`（无界）和 `stream_cursor` 方法不变（零 Breaking Change，spec §5.7.2 规则 6）
- **输入**：存量 `stream_buffered`（`stream_api.rs:50`）+ `stream_cursor`（`stream_api.rs:89`）+ `cursor_stream.rs`
- **输出**：`stream_with_backpressure` 方法，有界缓冲 + 背压阻塞
- **验证**：`stream_with_backpressure(1000)` 返回 Stream，缓冲区容量 1000；buffer_size=0 返回 Err

### 7.2 验证 Stream 背压测试覆盖与内存上限
- [ ] 编写单元测试 `test_stream_backpressure_zero_buffer`（L1）：`stream_with_backpressure(0)` 返回 Err
- [ ] 编写集成测试 `test_stream_backpressure_basic`（L2）：`stream_with_backpressure(1000)` 返回 Stream，缓冲区容量 1000
- [ ] 编写集成测试 `test_stream_backpressure_block`（L2）：buffer_size=10，生产 100 行消费 0 行，生产者第 11 行阻塞，缓冲区 ≤ 10（spec §5.7.2 规则 2）
- [ ] 编写集成测试 `test_stream_backpressure_unblock`（L2）：缓冲区满，消费 1 行，生产者恢复产出第 11 行（spec §5.7.2 规则 3）
- [ ] 编写集成测试 `test_stream_backpressure_no_drop`（L2）：buffer_size=10，生产 100 行消费 100 行，消费者收到 100 行无丢失（spec §5.7.2 规则 7）
- [ ] 编写集成测试 `test_stream_backpressure_error`（L2）：数据库断开，Stream yield Err 后终止
- [ ] 编写集成测试 `test_stream_buffered_backward_compat`（L2）：v2.1.0 `stream_buffered` 行为不变（spec §5.7.2 规则 6）
- [ ] 编写基准测试 `test_stream_backpressure_memory`（L2）：100 万行，buffer_size=1000，每行 1 KB，峰值内存 ≤ 1000*1 KB + 10 MB = ~11 MB（spec §5.7.2 规则 4，DFX 4.1.5）
- [ ] 为 `stream_with_backpressure` 编写 rustdoc + doctest
- **输入**：任务 7.1 实现完成
- **输出**：B-4 全部测试用例通过
- **验证**：背压阻塞正确；无数据丢失；内存上限 ≤ buffer_size*row_size + 10 MB；向后兼容

---

## 8. 嵌套持久化 cascade_delete 策略（B-5）

**目标**：新增 `CascadeStrategy` 枚举（Restrict/Cascade/SetNull/SetDefault），新增 `nested_delete_with_strategy(conn, nested, strategy)` 函数，`NestedActiveModel` 保留 `cascade_delete(bool)` 方法兼容 + 新增 `with_strategy` 方法，`#[relation(cascade = "restrict")]` 标注解析。
**对应需求**：spec.md §5.8，design.md §2.2.2 B-5 接口
**工时**：2-3 天
**依赖**：无
**风险等级**：🟡 中（R-8 NestedActiveModel 字段变更破坏 sz-pay）

### 8.1 定义 CascadeStrategy 枚举与兼容设计
- [ ] 在 `packages/sz-orm-core/src/nested_active_model.rs` 新增 `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum CascadeStrategy { Restrict, Cascade, SetNull, SetDefault }`
- [ ] 为 `CascadeStrategy` 实现 `Default`，返回 `CascadeStrategy::Cascade`（与 v2.1.0 bool=true 兼容，spec §6.2）
- [ ] `NestedActiveModel` 字段兼容设计（design.md §3.2 推荐）：保留 `cascade_delete: bool` 字段 + 新增 `cascade_strategy: Option<CascadeStrategy>` 字段，`cascade_delete(bool)` 方法设置 bool，`with_strategy(CascadeStrategy)` 方法设置 enum，strategy 优先级高于 bool
- [ ] 保留存量 `cascade_delete(bool)` 方法不变（零 Breaking Change，缓解 R-8）
- [ ] 在 lib.rs re-export `CascadeStrategy`
- **输入**：存量 `NestedActiveModel`（`nested_active_model.rs:134`，cascade_delete: bool）
- **输出**：`CascadeStrategy` 枚举 + 兼容字段设计
- **验证**：`CascadeStrategy::default() == Cascade`；`cascade_delete(true)` 方法保留

### 8.2 实现 nested_delete_with_strategy 函数
- [ ] 在 `packages/sz-orm-core/src/nested_active_model.rs` 新增 `pub async fn nested_delete_with_strategy<M: Model>(conn: &mut dyn Connection, nested: &NestedActiveModel<M>, strategy: CascadeStrategy) -> Result<u64, DbError>` 函数
- [ ] 执行流程（design.md §2.1.3 B-5 流程图）：① BEGIN TRANSACTION → ② match strategy 分支：
  - **Restrict**：`SELECT COUNT(*) FROM children WHERE fk = ?`（参数化）→ count > 0 返回 Err 含 `存在 N 个子实体，禁止删除`（spec §5.8.2 规则 2）→ count == 0 删除 parent
  - **Cascade**：递归删除子级的子级 → `DELETE FROM children WHERE fk = ?` → 删除 parent（spec §5.8.2 规则 3）
  - **SetNull**：校验 fk 列允许 NULL（不允许返回 Err，spec §5.8.4 异常 2）→ `UPDATE children SET fk = NULL WHERE fk = ?` → 删除 parent（spec §5.8.2 规则 4）
  - **SetDefault**：校验 fk 列有默认值（无默认值返回 Err，spec §5.8.4 异常 3）→ `UPDATE children SET fk = DEFAULT WHERE fk = ?` → 删除 parent（spec §5.8.2 规则 5）
  → ③ COMMIT → ④ 返回受影响行数
- [ ] 所有 DELETE/UPDATE 使用参数化（`WHERE fk = ?`，DFX 4.3.1），禁止 SQL 拼接
- [ ] 事务原子性：任一操作失败 ROLLBACK（spec §5.8.2 规则 7）
- [ ] 保留存量 `nested_delete(conn, nested)` 函数不变，内部使用 `nested.cascade_strategy` 或 `nested.cascade_delete` 字段决定策略（向后兼容）
- **输入**：存量 `nested_delete`（`nested_active_model.rs:419`）+ 任务 8.1 CascadeStrategy
- **输出**：`nested_delete_with_strategy` 函数，4 策略分支
- **验证**：RESTRICT + 有子实体返回 Err；CASCADE 递归删除；SET_NULL 置 NULL；SET_DEFAULT 置默认值

### 8.3 扩展 #[relation(cascade = "...")] 标注解析
- [ ] 在 `packages/sz-orm-macros/src/derive.rs` 的 `parse_relation_attr` 函数（存量 `derive.rs:1134`）新增 `cascade` 字段解析：`#[relation(cascade = "restrict")]` → `CascadeStrategy::Restrict`，支持 `restrict`/`cascade`/`set_null`/`set_default` 4 种值
- [ ] 在生成的 `NestedActiveModel` 代码中，将标注的 cascade 策略写入 `cascade_strategy` 字段
- [ ] 标注策略优先级低于 `nested_delete_with_strategy` 参数（参数 > 标注 > 默认 Cascade，spec §6.3）
- [ ] 编写单元测试验证 `#[relation(cascade = "restrict")]` 标注展开生成正确的 cascade_strategy 字段
- **输入**：存量 `parse_relation_attr`（`derive.rs:1134`）
- **输出**：cascade 标注解析，4 策略值支持
- **验证**：`#[relation(cascade = "restrict")]` 标注的实体 nested_delete 时按 RESTRICT 执行

### 8.4 验证 cascade_delete 测试覆盖与兼容性
- [ ] 编写集成测试 `test_cascade_restrict_with_children`（L2）：RESTRICT + 有子实体，返回 Err 含子实体数量，父未删（spec §5.8.2 规则 8）
- [ ] 编写集成测试 `test_cascade_restrict_no_children`（L2）：RESTRICT + 无子实体，删除成功
- [ ] 编写集成测试 `test_cascade_cascade_recursive`（L2）：CASCADE，User→3 Order→15 OrderItem，删除 1+3+15=19 行，事务内（spec §5.8.2 规则 3）
- [ ] 编写集成测试 `test_cascade_set_null`（L2）：SET_NULL，3 Order.user_id 置 NULL，User 删除
- [ ] 编写集成测试 `test_cascade_set_null_not_nullable`（L2）：SET_NULL + fk NOT NULL，返回 Err 提示
- [ ] 编写集成测试 `test_cascade_set_default`（L2）：SET_DEFAULT，3 Order.user_id 置默认值，User 删除
- [ ] 编写集成测试 `test_cascade_set_default_no_default`（L2）：SET_DEFAULT + fk 无默认值，返回 Err 提示
- [ ] 编写集成测试 `test_cascade_transaction_rollback`（L2）：CASCADE，删第 2 个 OrderItem 失败，事务回滚
- [ ] 编写集成测试 `test_cascade_default_compatibility`（L2）：`nested_delete(conn, nested)` 使用默认 Cascade，与 v2.1.0 行为一致（spec §5.8.2 规则 6 兼容）
- [ ] 为 `CascadeStrategy`、`nested_delete_with_strategy`、`with_strategy` 编写 rustdoc + doctest
- **输入**：任务 8.1-8.3 实现完成
- **输出**：B-5 全部测试用例通过
- **验证**：4 策略正确；事务回滚；默认 Cascade 兼容 v2.1.0

---

## 9. 集成验证与 v2.2.0 发布（M5）

**目标**：sz-pay 回归验证 5,139 测试无回归，11 道门禁全通过，43 包版本 2.1.0→2.2.0，crates.io 发布，CHANGELOG 更新。
**对应需求**：spec.md §4.4/§4.5，design.md §4.4 门禁清单
**工时**：2-3 天
**依赖**：M1-M4 全部完成（任务组 1-8）
**风险等级**：🟡 中（R-8 sz-pay 回归、R-1 AnyBackend 破坏 sz-pay match）

### 9.1 sz-pay 回归验证
- [ ] 在 sz-pay 项目（`E:\vue\test\sz-pay`）的 `Cargo.toml` 将 sz-orm 相关依赖版本从 `2.1.0` 升级至 `2.2.0`（sz-orm-core/sz-orm-sqlx/sz-orm-macros 等 6 个包）
- [ ] 检查 sz-pay 代码中所有 `match backend` 语句，确认有 wildcard 臂（无则追加，`#[non_exhaustive]` 强制，缓解 R-1，design.md §3.3）
- [ ] 在 sz-pay 项目运行 `cargo check` 验证编译通过（API 兼容）
- [ ] 在 sz-pay 项目运行 `cargo test --lib` 验证 5,139 测试全量通过，对比 v2.1.0 基线测试数量与通过率无下降（DFX 4.4.5，design.md §3.3）
- **输入**：任务组 1-8 全部完成 + sz-pay 项目
- **输出**：sz-pay 5,139 测试无回归
- **验证**：sz-pay `cargo test --lib` 通过，测试数 ≥ 5,139，通过率 100%

### 9.2 执行 11 道门禁全通过
- [ ] 门禁 1 fmt：`cargo fmt --all -- --check`（新增模块格式）
- [ ] 门禁 2 check：`cargo check --workspace --all-targets`（5 后端 feature 编译）
- [ ] 门禁 3 clippy：`cargo clippy --workspace --all-targets -- -D warnings`（零警告）
- [ ] 门禁 4 test：`cargo test --workspace`（新增测试全通过，4,993 + 新增 ≥ 60 用例）
- [ ] 门禁 5 doc：`cargo doc --workspace --no-deps --all-features`（新增 API doctest）
- [ ] 门禁 6 audit：`cargo audit` + `cargo deny check`（无新依赖漏洞）
- [ ] 门禁 7 integration：`cargo test --workspace -- --ignored`（Oracle/MSSQL 集成，本机有实例时）
- [ ] 门禁 8 占位扫描：`grep -rn 'todo!\|unimplemented!\|unreachable!' --include='*.rs' packages/`（新增代码 0 处占位，DFX 4.3.5）
- [ ] 门禁 9 SQL 注入扫描：`scripts/check-sql-injection.ps1`（新增 SQL 参数化）
- [ ] 门禁 10 feature 全组合：`cargo check --workspace --all-targets --all-features`（oracle/mssql feature）
- [ ] 门禁 11 ADR-0001：`git diff --name-only HEAD`（仅修改 sz-orm 仓库，未修改下游）
- **输入**：任务组 1-8 全部完成
- **输出**：11 道门禁全通过
- **验证**：每道门禁命令退出码 0；门禁 8 无新增占位；门禁 11 仅 sz-orm 仓库变更

### 9.3 升级 43 包版本与 crates.io 发布
- [ ] 在工作空间根 `Cargo.toml` 的 `[workspace.package]` 段将 `version = "2.1.0"` 改为 `version = "2.2.0"`（集中管理，AGENTS.md 版本 1.2.2 是当前代码版本，发布版本 2.2.0）
- [ ] 运行 `cargo check --workspace` 验证 43 包版本同步更新（内部依赖 `version + path` 格式自动跟随）
- [ ] 更新 `CHANGELOG.md`，新增 v2.2.0 版本段，列出 8 项需求变更（A-1/A-2/A-3/B-1/B-2/B-3/B-4/B-5）+ 新增 API 清单 + 兼容性说明
- [ ] 运行 `cargo publish --dry-run` 验证每个包可发布（无遗漏依赖、无未发布依赖）
- [ ] 按依赖顺序 `cargo publish` 发布 43 包到 crates.io（sz-orm-macros → sz-orm-core → sz-orm-sqlx → sz-orm-oracle → sz-orm-mssql → ... → sz-orm-cli）
- [ ] 验证 crates.io 上 sz-orm-core 2.2.0 可见（`curl https://crates.io/api/v1/crates/sz-orm-core/2.2.0`）
- **输入**：门禁全通过 + CHANGELOG 更新
- **输出**：43 包 @ 2.2.0 发布到 crates.io
- **验证**：crates.io 上 43 包 2.2.0 版本可见；sz-pay 可依赖 2.2.0 编译通过

### 9.4 五维审查与审计合规验证
- [ ] **正确性**：验证所有新增 API 行为符合 spec.md 38 条 EARS 验收标准，逐项附 `file:line` 证据
- [ ] **可读性**：审查新增代码命名规范、注释完整、无冗余（用户规则 1/2/6）
- [ ] **架构**：验证模块划分符合 design.md §2.1.2，无循环依赖，层次清晰
- [ ] **安全性**：验证所有新增 SQL 参数化（门禁 9），DSN scheme 白名单，破坏性 DDL 审计日志
- [ ] **性能**：验证 UnifiedPool 开销 ≤5%（任务 3.5 基准）、多级 Eager Loading 开销 ≤15%、背压内存上限
- [ ] **审计合规**：运行 `scripts/audit-verify.ps1` 验证本 tasks.md 及审查报告所有 `file:line` 引用真实存在（AGENTS.md 审计合规铁律）
- [ ] 运行 `cargo mutants --workspace` 变异测试验证测试套件质量（若配置了 .cargo-mutants.toml）
- **输入**：任务组 1-8 + 门禁全通过
- **输出**：五维审查报告 + 审计合规验证
- **验证**：五维审查全通过；审计验证脚本所有 file:line 引用真实存在；变异测试无存活变异体

---

## 文件变更清单

### 新建文件

| 文件路径 | 所属任务 | 变更描述 |
|---------|---------|---------|
| `packages/sz-orm-sqlx/src/unified_pool.rs` | 任务组 3（A-3） | UnifiedPool 统一抽象模块：结构定义 + connect/connect_with_config/from_pool/acquire/dialect/backend/resize/close_all/metrics 方法 |
| `packages/sz-orm-core/src/cycle_detection.rs` | 任务组 4（B-1） | 循环检测模块：CyclePolicy 枚举 + CycleDetector 类型（visited 集合 + 深度计数） |

### 修改文件

| 文件路径 | 所属任务 | 变更描述 |
|---------|---------|---------|
| `packages/sz-orm-sqlx/src/any_driver.rs` | 任务组 1（A-1）+ 任务组 2（A-2） | AnyBackend 枚举 3→5 变体 + `#[non_exhaustive]`；from_dsn 新增 3 scheme；AnyPool::connect 新增 2 分支（feature gate）；AnyBackend::dialect() + AnyPool::dialect() 新增方法 |
| `packages/sz-orm-sqlx/src/lib.rs` | 任务组 3（A-3） | 注册 `pub mod unified_pool;` 并 re-export UnifiedPool |
| `packages/sz-orm-sqlx/Cargo.toml` | 任务组 1（A-1） | 新增 `oracle`/`mssql` feature gate，sz-orm-oracle/sz-orm-mssql 改 optional 依赖 |
| `packages/sz-orm-core/src/eager_loader.rs` | 任务组 4（B-1） | ChildLoadConfig 改递归结构（+children 字段）；NestedEagerResult 新增枚举；load_nested 新增方法；with_cycle_policy 新增方法 |
| `packages/sz-orm-core/src/lib.rs` | 任务组 4（B-1）+ 任务组 5（B-2） | 注册 `pub mod cycle_detection;` 并 re-export CyclePolicy/CycleDetector/NestedEagerResult/Confirm/DataMigrationHook/DestructiveSyncResult/CascadeStrategy |
| `packages/sz-orm-core/src/schema_sync.rs` | 任务组 5（B-2） | diff_columns 新增 Levenshtein 重命名检测；Confirm 枚举 + DataMigrationHook trait + DestructiveSyncResult 结构；destructive_sync 方法 + sz-orm-audit 审计集成 |
| `packages/sz-orm-core/src/query.rs` | 任务组 6（B-3） | QueryBuilder::select_exclude 新增方法 |
| `packages/sz-orm-core/src/stream_api.rs` | 任务组 7（B-4） | StreamApiExt::stream_with_backpressure 新增方法（mpsc channel 桥接） |
| `packages/sz-orm-core/src/nested_active_model.rs` | 任务组 8（B-5） | CascadeStrategy 枚举；NestedActiveModel 新增 cascade_strategy 字段（兼容）；nested_delete_with_strategy 函数；with_strategy 方法 |
| `packages/sz-orm-macros/src/derive.rs` | 任务组 8（B-5） | parse_relation_attr 新增 cascade 字段解析（restrict/cascade/set_null/set_default） |
| `packages/sz-orm-core/Cargo.toml` | 任务组 5（B-2） | 确认/添加 sz-orm-audit 依赖（optional feature gate） |
| `Cargo.toml`（工作空间根） | 任务组 1（A-1）+ 任务组 9（M5） | sz-orm-sqlx 依赖添加 `features = ["oracle", "mssql"]`；[workspace.package] version 2.1.0→2.2.0 |
| `CHANGELOG.md` | 任务组 9（M5） | 新增 v2.2.0 版本段，8 项需求变更 + 新增 API 清单 + 兼容性说明 |

### 不变文件（复用存量实现）

| 文件路径 | 复用内容 |
|---------|---------|
| `packages/sz-orm-core/src/dialect.rs` | OracleDialect（:939）/SqlServerDialect（:1185）已实现，任务组 2 仅集成验证 |
| `packages/sz-orm-oracle/src/lib.rs` | OracleConnectionFactory（:634）已实现，任务组 1 复用 |
| `packages/sz-orm-mssql/src/lib.rs` | MssqlConnectionFactory（:445）已实现，任务组 1 复用 |
| `packages/sz-orm-core/src/pool.rs` | Pool（:712）完整连接池，任务组 3 UnifiedPool 委托复用 |
| `packages/sz-orm-audit/` | 审计包已存在，任务组 5 破坏性 DDL 审计复用 |

---

## 门禁检查点

### M1 完成检查点（任务组 1 + 2）

```bash
# 1. 编译验证（5 后端 feature）
cargo check --workspace --all-targets --all-features

# 2. A-1/A-2 单元 + 集成测试
cargo test --workspace --lib -- any_backend any_pool dialect
cargo test --workspace -- --ignored --test oracle_mssql  # 本机有实例时

# 3. 占位扫描（dialect.rs 无新增占位）
grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-core/src/dialect.rs

# 4. clippy 零警告
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

### M2 完成检查点（任务组 3）

```bash
# 1. UnifiedPool 编译
cargo check --workspace --all-targets

# 2. UnifiedPool 测试
cargo test --workspace --lib -- unified_pool

# 3. 性能基准（适配开销 ≤5%）
cargo bench --bench unified_pool_overhead

# 4. sz-pay 迁移编译验证（在 sz-pay 项目）
cargo check  # sz-pay Cargo.toml 依赖 sz-orm-sqlx 2.2.0
```

### M3 完成检查点（任务组 4 + 5）

```bash
# 1. 多级 Eager + 破坏性 Sync 测试
cargo test --workspace --lib -- eager_load cycle destructive_sync rename

# 2. 循环安全验证（无栈溢出）
cargo test --workspace --lib -- cycle_truncate cycle_error cycle_depth_limit

# 3. 破坏性 DDL 审计验证
cargo test --workspace --lib -- destructive_sync_audit

# 4. 重命名检测验证
cargo test --workspace --lib -- rename_detection

# 5. 占位扫描
grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-core/src/eager_loader.rs packages/sz-orm-core/src/cycle_detection.rs packages/sz-orm-core/src/schema_sync.rs
```

### M4 完成检查点（任务组 6 + 7 + 8）

```bash
# 1. select_exclude + 背压 + cascade 测试
cargo test --workspace --lib -- select_exclude stream_backpressure cascade

# 2. 背压内存上限基准
cargo bench --bench stream_backpressure_memory

# 3. cascade 4 策略验证
cargo test --workspace --lib -- cascade_restrict cascade_cascade cascade_set_null cascade_set_default

# 4. 向后兼容验证
cargo test --workspace --lib -- stream_buffered_backward_compat cascade_default_compatibility
```

### M5 发布门禁（任务组 9，11 道门禁全通过）

```bash
# 门禁 1-11 逐项执行（见任务 9.2）
cargo fmt --all -- --check                                          # 1
cargo check --workspace --all-targets                               # 2
cargo clippy --workspace --all-targets -- -D warnings               # 3
cargo test --workspace                                              # 4
cargo doc --workspace --no-deps --all-features                      # 5
cargo audit && cargo deny check                                     # 6
cargo test --workspace -- --ignored                                 # 7
grep -rn 'todo!\|unimplemented!\|unreachable!' --include='*.rs' packages/  # 8
scripts/check-sql-injection.ps1                                     # 9
cargo check --workspace --all-targets --all-features                # 10
git diff --name-only HEAD                                           # 11

# sz-pay 回归（在 sz-pay 项目）
cargo test --lib  # 5,139 测试无回归

# 审计验证
scripts/audit-verify.ps1 docs/spec/v2.2.0/tasks.md

# crates.io 发布
cargo publish --dry-run  # 验证后逐包发布
```

---

## 风险任务标注

### 高风险任务

| 任务 | 风险 ID | 风险描述 | 缓解措施 |
|------|---------|---------|---------|
| 任务 4.4 load_nested 无限级递归 | R-3 🟠 高 | 深嵌套关联（如 100 级）可能栈溢出 | ① CyclePolicy::AllowWithDepthLimit 默认上限（如 10）；② 深度超限返回 Err；③ 文档建议深嵌套用 Stream API |
| 任务 7.1 stream_with_backpressure 生产者任务 | R-7 🟠 高 | mpsc 生产者 spawn 任务在消费者 drop 时未终止，连接不归还 | ① 生产者持有 Sender clone，drop 时关闭；② `tokio::select!` 监听 receiver 关闭；③ Drop impl 关闭游标 |

### 中风险任务

| 任务 | 风险 ID | 风险描述 | 缓解措施 |
|------|---------|---------|---------|
| 任务 1.1 AnyBackend 新增变体 | R-1 🟡 中 | sz-rust/sz-pay 现有 match backend 无 wildcard，编译失败 | ① `#[non_exhaustive]` 标注；② 提前检查 sz-pay match 语句；③ 迁移文档 |
| 任务 1.3 oracle/mssql feature gate | R-2 🟡 中 | 5 后端 feature 全组合编译（2^5=32），CI 耗时增长 | ① feature 默认全启用；② CI 仅验证关键组合；③ 文档说明 feature 矩阵 |
| 任务 4.1 CycleDetector 循环检测 | R-4 🟡 中 | 按 entity 类型名去重，同类型不同实例关联误判 | ① 按 entity 类型 + 关联名联合去重；② 文档说明检测粒度 |
| 任务 5.1 diff_columns 重命名检测 | R-5 🟡 中 | Levenshtein 距离 ≤2 误判（name→title 距离 4 不识别，user_name→user_title 距离 2 误识别） | ① 类型兼容性校验；② 阈值可配置；③ destructive_sync dry-run 供审查 |
| 任务 5.3 destructive_sync 事务 | R-6 🟡 中 | SQLite/Oracle 部分 DDL 不支持事务回滚 | ① 检测方言 DDL 事务支持；② 不支持时返回 Err 含已执行 DDL 列表；③ 文档说明限制 |
| 任务 8.1 NestedActiveModel 字段变更 | R-8 🟡 中 | cascade_delete: bool → cascade_strategy 若 sz-pay 直接访问字段 | ① 字段私有；② cascade_delete(bool) 方法保留；③ sz-pay 回归验证 |
| 任务 1.5/2.4 Oracle/MSSQL 集成测试 | R-9 🟡 中 | Oracle 23ai Free / SQL Server 本机实例可能不可用 | ① `#[ignore]` 标注；② 方言适配单元测试不依赖真实 DB；③ 文档说明本机 DB 配置 |
| 任务 3.4 UnifiedPool 委托方法 | R-10 🟡 中 | 委托 Pool 引入额外间接调用开销 | ① `#[inline]` 标注；② 基准测试验证开销 ≤5%；③ Pool 字段直接持有非 Arc |

### 低风险任务

| 任务 | 风险等级 | 说明 |
|------|---------|------|
| 任务组 6（B-3 select_exclude） | 🟢 低 | 纯 QueryBuilder 扩展，无递归/并发/事务复杂度 |
| 任务 2.2 Dialect 无占位验证 | 🟢 低 | OracleDialect/SqlServerDialect 已实现，仅验证 |

---

## 依赖关系图

```
任务组 1 (A-1 AnyPool 扩展) ──→ 任务组 2 (A-2 Dialect 集成) ──→ 任务组 3 (A-3 UnifiedPool)
                                                                    │
                                                                    ↓
任务组 4 (B-1 多级 Eager) ──────────────────────────────────────→ 任务组 9 (M5 发布)
任务组 5 (B-2 破坏性 Sync) ────────────────────────────────────→ 任务组 9 (M5 发布)
任务组 6 (B-3 select_exclude) ────────────────────────────────→ 任务组 9 (M5 发布)
任务组 7 (B-4 Stream 背压) ────────────────────────────────────→ 任务组 9 (M5 发布)
任务组 8 (B-5 cascade_delete) ────────────────────────────────→ 任务组 9 (M5 发布)

关键路径：1 → 2 → 3 → 9（A 系列，10-14 天）
并行路径：4 ∥ 5 ∥ 6 ∥ 7 ∥ 8（B 系列，与 A 系列无依赖）
发布门：1-8 全部完成 → 9
```

---

> **文档版本**：v1.0（v2.2.0 编码任务规划初稿）
> **生成日期**：2026-08-06
> **生成方法**：基于 spec.md（38 条 EARS）+ design.md（6 章技术设计）+ v2.1.0 tasks.md 格式参照 + 源码现状调研
> **审计合规**：所有文件路径引用基于源码实际结构验证（packages/sz-orm-core/src/ 56 文件、packages/sz-orm-sqlx/src/ 5 文件、packages/sz-orm-macros/src/ 2 文件）
> **设计约束**：零 Breaking Change、禁止占位实现、unsafe 零容忍、WHERE 参数化、禁止 SELECT *、Connection trait 手动解糖 async、最大 2 层任务嵌套
> **下一步**：用户确认任务规划 → 编码实现（由编码 agent 执行，本 agent 不参与编码）