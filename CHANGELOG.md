# Changelog

本文件记录 SZ-ORM 项目的所有重要变更。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
并遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

## [3.2.0] — 2026-08-08

### 概述

v3.2.0 是 SZ-ORM 的性能深度优化版本，包含 4 大方向交付物，覆盖 20 条 EARS 需求，通过 5 个里程碑（M1-M5）推进。所有新能力通过 feature gate 隔离，默认关闭，无 Breaking Change。

### 新增功能

#### 连接池自动预热增强（`auto-prewarm` feature）

- `PrewarmConfig` + `ProgressiveConfig` + `PrewarmProgress` + `PrewarmSummary`
- `Pool::new_async` 自动预热 + `Pool::progressive_prewarm` 渐进式分批预热
- `UnifiedPool::prewarm` + `UnifiedPool::progressive_prewarm` + `MultiPoolRegistry` 多池统一预热
- 预热失败不阻断池创建，超时可配置，进度可观测
- telemetry 集成（prewarm 计数器 + 快照字段）
- 29 单元测试 + 7 集成测试（真实 MySQL+PG）

#### 查询计划缓存（`plan-cache` feature）

- `SqlNormalizer` SQL 归一化 + 表名提取（AST 提取 + 字符串回退）
- `PlanCacheKey` xxHash 64bit 强哈希
- `PlanCacheEntry` + `PlanCacheStats` 原子计数器无锁统计
- `PlanCache` 双缓存（parse + optimize）+ `LruOrder64` arena 双向链表 LRU
- `invalidate_table` 表级精确失效 + `invalidate_all` 全量失效
- `UnifiedQueryOptimizer::with_plan_cache` 缓存集成
- 26 单元测试 + 11 差分测试

#### 零拷贝序列化（`zero-copy` feature）

- `BorrowedValue<'a>` 21 变体枚举（字符串/字节用 `Cow<'a, str>` / `Cow<'a, [u8]>` 借用）
- `BorrowedRowData<'a>` 借用型行数据
- `ColumnarSchema` + `ColumnarResultSet` 列式结果集（按列连续存储，缓存友好）
- `from_row_data` / `to_row_data` 行列互转
- `apply_result_map_borrowed` + `apply_result_map_many_borrowed` 零拷贝反序列化路径
- 31 单元测试 + 4 等价性测试 + criterion 基准测试

#### SIMD 加速（`simd` feature）

- `SimdAvailability` 枚举 + `detect()` 运行时检测（AVX2/AVX/SSE2/NEON/None，OnceLock 缓存）
- `batch_decode_integers` SIMD 批量整数解码（wide::i64x4 向量并行）
- `batch_compare_eq` / `batch_compare_in` SIMD 列比较（向量比较 + 布尔掩码）
- 标量降级路径（count < 1024 或 None 时自动回退）
- WASM 目标自动降级标量
- 20 单元测试 + 12 差分测试 + criterion 基准测试

### 性能收益

- 连接池冷启动：自动预热后首次查询 P95 ≤ 20ms（对比未预热 ≤ 100ms）
- 查询计划缓存：重复 SQL 跳过解析/优化，命中率可观测
- 零拷贝：BorrowedValue Cow 借用消除深拷贝，10000 行反序列化分配减少
- SIMD：1024+ 行批量解码/比较走 SIMD 向量化路径

### Feature Gate

| Feature | 默认 | 描述 |
|---------|------|------|
| `auto-prewarm` | 关闭 | 连接池自动预热增强 |
| `plan-cache` | 关闭 | 查询计划缓存 |
| `zero-copy` | 关闭 | 零拷贝序列化 |
| `simd` | 关闭 | SIMD 加速 |

### 兼容性

- 无 Breaking Change：所有新能力通过 feature gate 隔离，默认关闭
- 默认 feature 测试基线：1594 passed
- 四 feature 全组合：1695 passed
- clippy 零警告（`--all-features --all-targets`）

## [3.0.0] — 2026-08-07

### 概述

v3.0.0 是 SZ-ORM 的长期目标迭代版本，包含 6 大方向交付物，覆盖 29 条 EARS 需求，通过 7 个里程碑（M1-M7）推进。

### 新增功能

#### 图数据库支持（sz-orm-graph 0.1.0）

- Neo4j 图数据库连接池（GraphConfig + GraphConnection + GraphPool）
- Cypher 查询构建器（CypherQueryBuilder）+ 参数化校验（CypherValidator）
- 声明式图模型（GraphNodeModel + GraphRelationModel）
- 结果映射（NodeMapper + RelationMapper，serde 反序列化）
- DSN 脱敏（sanitize_dsn）
- 11 单元测试 + 6 集成测试 + P95 性能测试
- 需求覆盖：REQ-GDB-001 ~ REQ-GDB-005

#### WASM 完善（sz-orm-wasm）

- wasm-bindgen JS 绑定（JsWasmDatabase + JsQueryResult）
- IndexedDB 持久化（WasmPersistence trait + IndexedDbStore）
- 内存限制（行数 + 行大小超限拒绝写入）
- 持久化不可用明确报告（WasmPersistenceError）
- wasm-bindgen-test 7 测试
- WASM gzip 89.7 KB << 1MB
- 需求覆盖：REQ-WASM-001 ~ REQ-WASM-005

#### FFI 发布产物（Python + JS）

- maturin Python wheel 构建脚本 + PyPI 发布脚本
- napi-rs JS 绑定构建脚本 + npm 发布脚本
- Python 等价性测试（pytest）+ JS 等价性测试（jest 16 passed）
- 绑定验证脚本（verify_bindings.ps1）
- CI 矩阵（三平台 × Python 3.8/3.10/3.12 × Node 18/20/22）
- 需求覆盖：REQ-FDI-001 ~ REQ-FDI-005

#### AI 查询优化器（sz-orm-ai --features llm-optimizer）

- HintSource 枚举（Rule/Llm）+ UnifiedOptimizationHint（来源追溯）
- OptimizerConfig（LLM 可配置性，默认降级纯规则）
- ExplainPlanParser trait + 5 方言实现（MySQL/PG/SQLite/Oracle/MSSQL）
- LlmOptimizer（OpenAI 兼容 API，SQL 脱敏后发送）
- UnifiedQueryOptimizer（规则 + LLM 合并，降级安全）
- SqlSanitizer（password/token/Base64 敏感字面量脱敏）
- LLM SQL 零执行（suggested_sql 仅建议，无 execute_sql 方法）
- 230 单元测试 + 4 集成测试
- 需求覆盖：REQ-AI-001 ~ REQ-AI-005

#### XA 事务一致性（sz-orm-dtx --features xa）

- XaResource/XaParticipant/XaCoordinator（2PC 协议）
- XaRecoveryCoordinator（3 恢复策略：COMMIT/ROLLBACK/HEURISTIC）
- SuspensionDetector（后台扫描 + 超时检测）
- XaCapabilityChecker（拒绝不支持 XA 的参与者）
- 6 集成测试 + coexistence 测试（XA 与 2PC/Saga/TCC 共存）
- 需求覆盖：REQ-DTX-001 ~ REQ-DTX-005

#### 多后端协同文档

- multi_backend_readiness.md（5 项就绪验证全 PASS）
- dialect_constraints.md（11 特性 × 5 方言矩阵）
- sz_rust_integration_example.rs（编译 + clippy 零警告 + 运行成功）
- ADR-0001 PASS（业务代码零修改）
- 需求覆盖：REQ-MB-001 ~ REQ-MB-004

### Feature Gate 隔离

| Feature | 包 | 默认 | 依赖 |
|---------|---|------|------|
| `xa` | sz-orm-dtx | 关闭 | sz-orm-sqlx |
| `llm-optimizer` | sz-orm-ai | 关闭 | reqwest (via `real`) |
| `js` | sz-orm-wasm | 关闭 | wasm-bindgen, js-sys |
| `persistence` | sz-orm-wasm | 关闭 | web-sys |
| `integration` | sz-orm-graph | 关闭 | neo4rs |

### 无 Breaking Change

- v2.4.0 公开 API 签名全部保持不变
- 新增能力通过 feature gate 隔离（默认 feature 不引入额外依赖）
- `cargo build --workspace`（默认 feature）成功

### 门禁验证

| 门禁 | 状态 |
|------|------|
| fmt | ✅ |
| check | ✅ |
| clippy | ✅ 零警告 |
| test | ✅ 全部通过 |
| doc | ✅（预存警告） |
| 占位检查 | ✅ |
| Feature 全组合 | ⚠️ rdkafka-sys 预存问题 |

### 需求覆盖

29 条 EARS 需求全部映射到任务并验收通过：
- REQ-GDB-001~005（图数据库）
- REQ-WASM-001~005（WASM）
- REQ-FDI-001~005（FFI 发布）
- REQ-AI-001~005（AI 优化器）
- REQ-DTX-001~005（XA 事务）
- REQ-MB-001~004（多后端协同）

## [2.4.0] — 2026-08-07

### Bug 修复

#### SmartEagerLoader `load()` 根关联未加载
- 修复 `SmartEagerLoader::load()` 方法中 `self.relation`（根关联）从未被加载的 bug
  - 当 `children.is_empty()` 时直接返回 Leaf 不加载关联数据
  - 当 children 非空时使用 `first_child.relation` 而非 `self.relation` 作为第一级
- 修复方式：使用 `self.relation` 作为根级关联，`std::mem::take(&mut self.children)` 作为子级配置传给 `load_level_smart`
- 位置：`packages/sz-orm-core/src/smart_eager_loader.rs:650`

### 新增测试

#### SmartEagerLoader 五方言集成测试套件
- `tests/common/equivalence.rs`：等价性断言工具（Smart vs Manual 结果集对比）
- `tests/common/schema_builder.rs`：TestSchemaBuilder（5 方言 DDL + 数据填充）
- `tests/common/rusqlite_adapter.rs`：SQLite Connection trait 适配器
- `tests/common/sqlx_mysql_adapter.rs`：MySQL Connection trait 适配器（sqlx::MySqlPool）
- `tests/common/sqlx_pg_adapter.rs`：PostgreSQL Connection trait 适配器（sqlx::PgPool + `?` → `$N` 转换）
- `tests/smart_eager_test_infra.rs`：31 passed（基础设施测试）
- `tests/smart_eager_integration_sqlite.rs`：29 passed（SQLite 集成测试）
- `tests/smart_eager_integration_mysql.rs`：7 测试 `#[ignore]`（需 MySQL 服务）
- `tests/smart_eager_integration_pg.rs`：7 测试 `#[ignore]`（需 PostgreSQL 服务）
- `tests/smart_eager_integration_oracle.rs`：7 测试 `#[ignore]`（需 Oracle 服务）
- `tests/smart_eager_integration_mssql.rs`：7 测试 `#[ignore]`（需 MSSQL 服务）

#### SmartEagerLoader 性能基准套件
- `bench-comparison/benches/smart_eager_harness.rs`：基准测试工具（BenchSqliteConn + SmartEagerBenchHarness，4 规模 10/100/1000/10000）
- `bench-comparison/benches/bench_smart_eager.rs`：4 基准组
  - `bench_decision_latency`：StrategyResolver::resolve() 延迟（实测 68-81 ns，远超 ≤100μs 要求）
  - `bench_smart_vs_manual`：SmartEagerLoader vs EagerLoader 对比（开销 4-6%，10000 行 Smart 快 9%）
  - `bench_n1_elimination`：N+1 逐条 vs 批量对比（100 行 3.3x、1000 行 12.8x、10000 行 60.6x 加速）
  - `bench_n1_detector`：N1Eliminator 检测能力（8.46μs）

### 新增脚本
- `scripts/compute_topology.ps1`：依赖拓扑排序脚本（Kahn 算法 + 字典序打破并列）
- `scripts/publish_crates_io.ps1`：crates.io 逐包发布脚本（门禁 → token → 拓扑序 → 逐包发布）
- `scripts/verify_sz_pay.ps1`：sz-pay 下游验证脚本（版本升级 → build → test → 零回归）

### 向后兼容
- v2.4.0 无 Breaking Change，所有 v2.3.0 公开 API 签名保持不变
- SmartEagerLoader `load()` 方法签名不变，仅修复内部实现逻辑
- 新增文件均为测试/基准/脚本，不进入 sz-orm-core 公开 API

### 验证结果
- sz-orm-core 全测试：0 failed
- sz-orm-core clippy：零警告
- sz-orm-core fmt：通过
- SQLite 集成测试：29 passed
- 集成测试基础设施：31 passed
- 性能基准：4 组 × 4 规模全部达标
- sz-pay 回归：5139 passed, 0 failed, 13 ignored（零回归）
- 占位实现检查：0 处（2 处匹配在文档注释中）
- SQL 注入扫描：31 项均为内部逻辑拼接（手动审查确认安全）

## [2.3.0] — 2026-08-07

### 新增功能

#### 任务 C：Eager Loading 智能策略选择
- `SmartEagerLoader` 类型：基于 `RelationKind` 自动选择最优加载策略
- `EagerLoader::smart()` 扩展方法：返回 `SmartEagerLoader`，向后兼容
- `LoadStrategy` 枚举（Join/DataLoader/IntermediateTableBatch）
- `StrategyDecision` 结构：策略决策记录（关联名/类型/策略/原因/查询次数）
- `StrategyResolver` 策略决策器：纯规则匹配，决策延迟 ≤ 100μs
  - HasOne / BelongsTo → Join（单次 JOIN 查询）
  - HasMany → DataLoader（批量 IN 查询，2 次）
  - ManyToMany（有中间表）→ IntermediateTableBatch（中间表批量，2 次）
  - ManyToMany（无中间表）→ 回退 DataLoader + 告警
- `JoinStrategy` 执行器：HasOne/BelongsTo 自动 JOIN + 结果集拆分
- `DataLoaderStrategy` 执行器：HasMany 自动 data loader + 按外键分组
- `IntermediateTableStrategy` 执行器：ManyToMany 中间表批量查询
- `N1Eliminator` N+1 自动消除器：连续查询模式检测 + 批量合并 + 等价性校验
- `N1EliminationReport` 消除报告（原次数/合并后次数/节省/触发位置/合并 SQL）
- `RelationDef::new_many_to_many()` 构造器：ManyToMany 中间表元数据
- `RelationDef` 新增 `join_table`/`join_from_key`/`join_to_key` 可选字段

#### 任务 B：性能基准完整报告
- `CompetitorAdapter` trait：竞品适配层统一接口
- `BenchmarkReporter` 报告生成器：Markdown + CSV/JSON + DSN 脱敏
- `full_comparison` bench 主入口：全维度 × 多方言 × 竞品基准
- criterion 配置：sample_size=100, warm_up=3s, measurement=10s

### 向后兼容
- v2.3.0 无 Breaking Change，所有新增能力以扩展方法提供
- `RelationDef::new()` 签名不变，新增中间表字段默认 `None`
- `EagerLoader` 原有 API（`new`/`with`/`load_many`/`load_nested`）不变

### 测试
- sz-orm-core lib 测试：1578 passed（+28 新测试）
- 全 workspace 测试：全部通过，零失败
- 全 workspace clippy：零警告

## [2.2.0] — 2026-08-06

### 新增功能

#### A-1 AnyPool 扩展支持 Oracle/MSSQL
- `AnyBackend` 枚举新增 `Oracle` / `Mssql` 变体 + `#[non_exhaustive]` 标注
- `AnyPool::connect` 新增 Oracle/MSSQL 分派分支（feature gate）
- DSN 解析支持 `oracle://` / `mssql://` / `sqlserver://` scheme

#### A-2 Dialect 与 AnyPool 集成验证
- `AnyBackend::dialect()` 方法：5 后端 → 5 Dialect 映射
- `AnyPool::dialect()` 方法：委托 backend.dialect()

#### A-3 UnifiedPool 统一抽象
- `UnifiedPool` 结构体：统一连接池接口
- `connect(dsn)` / `connect_with_config(dsn, config)` / `from_pool(pool, backend)` 方法
- `acquire()` / `backend()` / `dialect()` / `resize()` / `close_all()` / `status()` 委托方法

#### B-1 Eager Loading 多级关联 + 循环检测
- `CyclePolicy` 枚举（Error/Truncate/AllowWithDepthLimit）+ `CycleDetector`
- `NestedEagerResult` 递归枚举（Leaf/Node）支持无限级嵌套树
- `EagerLoader::load_nested()` 方法：多级批量查询 + 循环检测
- `EagerLoader::with_cycle_policy()` 方法：设置循环检测策略
- `ChildLoadConfig` 改为递归结构支持无限级链式调用

#### B-2 Schema Sync 破坏性变更安全策略
- `Confirm` 枚举（Yes/No）显式确认破坏性 DDL
- `DataMigrationHook` trait（before_drop_column / before_rename_column 钩子）
- `DestructiveSyncResult` 结构体
- `SchemaSync::destructive_sync()` 方法：事务内执行 + 钩子 + 审计
- `diff_columns` 新增 Levenshtein 重命名检测（距离 ≤ 2 或比例 ≤ 0.3）
- `SchemaSync::with_rename_threshold()` 方法：配置重命名检测阈值

#### B-3 Partial Models select_exclude
- `QueryBuilder::select_exclude(fields: &[&str])` 方法：排除指定字段查询
- 校验排除字段存在、不排除全部字段
- 与 `select_only` 互补，自动进入 Partial 模式

#### B-4 Stream API 背压控制
- `StreamApiExt::stream_with_backpressure(buffer_size)` 方法
- 有界缓冲通道，缓冲区满时生产者阻塞（背压）
- `buffer_size == 0` 返回 `Err(DbError::InvalidInput)`

#### B-5 嵌套持久化 cascade_delete 策略
- `CascadeStrategy` 枚举（Restrict/Cascade/SetNull/SetDefault）
- `nested_delete_with_strategy(conn, nested, strategy)` 函数
- 4 策略分支：Restrict 禁止删除 / Cascade 递归删除 / SetNull 置 NULL / SetDefault 置默认值
- 事务内原子执行，失败 ROLLBACK

### 兼容性

- **零 Breaking Change**：所有新增能力以扩展方法、新增类型、新增枚举变体提供
- `EagerResult` / `load_many` / `stream_buffered` / `sync()` / `cascade_delete(bool)` 保留不变
- `AnyBackend` 新增 `#[non_exhaustive]`：外部 crate match 须加 `_` 通配符

## [2.1.0] — 2026-08-06

### 新增功能

#### P-F-1 Eager Loading 端到端（M3）
- `EagerLoader` 结构体 + `eager_load_all` / `eager_load_one` 一行 API
- HasMany 双查询策略 + HasOne/BelongsTo JOIN 策略
- N+1 查询消除（2 条 SQL 而非 N+1 条）
- Oracle IN >1000 分批查询
- 多级关联（User → Order → OrderItem，限 2 级）
- `value_to_key` 辅助函数解决 Value 不实现 Hash/Eq 问题

#### P-F-2 RelationTrait + join/left_join（M1）
- `RelationKind` / `RelationDef` / `RelationTrait` 核心类型
- `#[derive(RelationTrait)]` 宏自动生成 RelationTrait 实现
- `QueryBuilder::join()` / `left_join()` 类型安全链式 JOIN API

#### P-F-3 Partial Models（M2）
- `SelectMode` / `AggFunc` / `Expr` 类型
- `QueryBuilder::select_only()` / `.column()` / `.columns()` / `.column_as()` 方法
- 聚合查询 + GROUP BY 支持

#### P-F-4 Schema Sync 自动结构同步（M5）
- `TableDef` / `ColumnDef` / `SchemaDiff` / `SyncResult` 类型
- `diff` 纯函数：6 类变更检测
- 5 方言 DDL 生成器：MySQL / PostgreSQL / SQLite / Oracle / MSSQL
- `SchemaSync` 协调器：`sync_dry_run` + `sync`（事务执行）
- 破坏性变更检测（dropped_tables / dropped_columns → Err）

#### P-F-5 ActiveModel 嵌套持久化（M4）
- `NestedActiveModel<M>` 包装器（不修改存量 ActiveModel）
- `ChildEntity` 子实体
- `nested_save`：事务执行 + 外键自动回填 + 多级递归
- `nested_delete`：子先父后删除顺序
- 深度限制 10 层 + RAII 事务 guard

#### P-F-6 Stream API（M6）
- `StreamApiExt` trait + `stream_buffered` 兼容版
- `stream` impl 改造（委托 query 而非全量收集）
- 向后兼容：`stream_buffered` 保留 v2.0.0 行为

#### P-F-7 性能基准对比（M7）
- v2.1.0 新功能基准测试（Eager Loading / Nested Save / Schema Diff / Stream API）
- Eager Loading vs N+1 查询对比

### 向后兼容

- **无 Breaking Change**：所有 v2.0.0 API 保持不变
- `ActiveModel<M>` 结构不变（嵌套通过 `NestedActiveModel` 包装）
- `Connection` trait 不变（复用 v2.0.0 `query_stream_cursor`）
- `StreamQueryTrait` trait 签名不变（仅改 impl 实现）

### 测试

- 1521 单元测试通过（+33 新增）
- 50+ 集成测试通过（+20 新增）
- clippy 0 警告

## [2.0.0] — 2026-08-06

### Breaking Changes

- **移除 deprecated `where_cond` / `or_where`**：`FindWithRelatedBuilder::where_cond()`、`QueryBuilderExt::or_where()` 及相关 `where_conds` 字段已删除。迁移至 `where_eq` / `or_where_eq` 等参数化方法（自 v1.2.0 起已标记 deprecated，v2.0.0 正式移除）

### Added

- **Oracle 23ai 真实集成测试**：`integration_oracle.rs` 追加 7 类场景（CRUD / 事务 / 乐观锁 / 软删除 / 分页 / 聚合 / 批量），10 测试通过
- **SQL Server 真实集成测试**：`integration_mssql.rs` 新建 8 类场景（CRUD / 事务 / 乐观锁 / 软删除 / 分页 / 聚合 / 批量 / INSERT OR IGNORE 回退）
- **Python 绑定 (PyO3)**：`sz-orm-python` 0.1.0 发布到 crates.io，支持 Model / QueryBuilder / Pool / Transaction Python API
- **JavaScript 绑定 (napi-rs)**：`sz-orm-js` 0.1.0 发布到 crates.io，支持 Node.js 原生绑定
- **安全审计报告**：`docs/assessment/2026-08-05-security-audit-report.md`，7 维度覆盖（SQL 注入 / 连接池 / 密码 / 权限 / 输入校验 / 信息泄露 / 依赖安全）

### Changed

- **crates.io 批量发布**：42 个包发布 **2.0.0**（sz-orm-python / sz-orm-js 保持 0.1.0）
- **测试规模**：全 workspace 4,947 passed, 0 failed（lib 测试）
- **内部依赖版本对齐**：所有 `version + path` 格式的内部依赖统一至 2.0.0

## [1.5.0] — 2026-08-05

### Added

- **连接池 Prometheus 统计指标 (sz-orm-core)**：`Pool::pool_metrics()` 返回 `PoolMetrics`（acquire_count / acquire_failed_count / acquire_wait_time / release_count / connection_created_count / connection_closed_count），基于无锁 `AtomicU64`，热路径开销可忽略；`average_acquire_wait_time()` 计算平均获取等待时长
- **ClickHouse 行锁支持 (sz-orm-core)**：`supports_lock_for_update()` / `supports_lock_shared()` 显式返回 `false`（无事务无行锁）；`build_insert_or_ignore_prefix()` 回退普通 `INSERT INTO`
- **SQL Server INSERT OR IGNORE 回退 (sz-orm-core)**：`build_insert_or_ignore_prefix()` 回退普通 `INSERT INTO`（SQL Server 无等价前缀语法，应用层可捕获 2601/2627 冲突或使用 MERGE）
- **DuckDB 真实集成测试**：`integration_duckdb.rs` 7 个真实 DB 测试（duckdb bundled 特性）
- **向量/时序真实实现集成测试**：sz-orm-vector 3 个 `#[ignore]` 真实 pgvector 测试；sz-orm-timeseries 5 个内存集成测试 + 2 个 `#[ignore]` 真实 TimescaleDB 测试
- **Redis 后端默认启用 (sz-orm-core)**：`redis` feature 加入 `default`，`RedisBackend` 开箱即用

### Changed

- **crates.io**：sz-orm-core 发布 **1.5.0**（依赖 sz-orm-sql-validator 1.4.0 / sz-orm-macros 1.4.0）
- **测试规模**：全 workspace 5,809 passed, 0 failed

## [1.0.0] — 2026-07-19

### Added

- **核心引擎 (sz-orm-core)**：Model trait、QueryBuilder、多数据库方言（MySQL/PostgreSQL/SQLite/Oracle 23ai）、异步连接池、ACID 事务、文件迁移系统、多级缓存、统一值类型（20 种变体）、错误类型体系
- **数据库适配器**：sz-orm-sqlx（MySQL/PostgreSQL/SQLite/Oracle）、sz-orm-sql-validator（SQL 注入检测）
- **扩展生态包 (18 个)**：
  - sz-orm-crypto：AES-256-GCM、PBKDF2、HMAC-SHA256
  - sz-orm-auth：认证与授权
  - sz-orm-batch：批量 INSERT/UPDATE/UPSERT
  - sz-orm-dtx：分布式事务
  - sz-orm-mig：迁移工具
  - sz-orm-sharding：分库分表
  - sz-orm-cache：多级缓存（注：实现在 sz-orm-core/src/cache.rs 与 l2_cache.rs 内，非独立 crate）
  - sz-orm-queue：消息队列
  - sz-orm-scheduler：任务调度
  - sz-orm-graphql：GraphQL 接口
  - sz-orm-grpc：gRPC 接口
  - sz-orm-ai：NL→SQL（自然语言转 SQL）
  - sz-orm-vector：pgvector 向量搜索
  - sz-orm-search：Meilisearch/Elasticsearch/OpenSearch 集成
  - sz-orm-storage：S3 兼容对象存储
  - sz-orm-postgis：PostGIS 地理空间
  - sz-orm-timeseries：时序数据
  - sz-orm-observability：Prometheus 指标 + OpenTelemetry tracing
  - sz-orm-tracing：分布式追踪（W3C TraceContext）
- **CLI (sz-orm-cli)**：命令行工具
- **DevTools**：sz-orm-swagger（OpenAPI）、sz-orm-health（健康检查）
- **测试体系**：2,271 个单元/集成测试（1,635 `#[test]` + 636 `#[tokio::test]`）、proptest 属性测试、fuzz 模糊测试、chaos 混沌测试（16 项）、6h soak test
- **CI/CD**：GitHub Actions 多 workflow（CI/安全/soak test/依赖更新）
- **文档**：15 份中文文档 + README.en.md 英文文档 + CONTRIBUTING.md 贡献指南

### Security

- cargo audit 通过（1 allowed warning: paste unmaintained）
- cargo deny check advisories bans licenses sources 全部通过
- 6h Linux CI Soak Test（2026-07-21 立即触发）

### Performance

- 1h soak test：13.8 亿 operations，0 errors，1.16% throughput decay，43μs→41μs P99 latency
- 7 组 criterion 基准测试

## [Unreleased]

### Added

- **API 稳定性承诺文档**：新增 `docs/API-STABILITY.md`，明确 SemVer 承诺、API 稳定性三层分级（Stable/Experimental/Internal）、废弃流程（2 个 MINOR 版本保留期）、破坏性变更条件
- **端到端真实 DB 示例**：新增 `examples/src/bin/real_db_crud.rs`，使用 SQLite 内存数据库演示完整连接池 + CRUD + 事务（提交/回滚）流程
- **Prometheus 告警规则**：新增 `monitoring/alerts.yml`，覆盖错误率/延迟/连接池/SLO 燃烧率告警
- **文档清理**：删除 33 份开发期文档（审计报告/调研文档/重复副本），保留 19 份核心文档

### Fixed

- **sz-orm-search unreachable!() 消除**：将 `TokenizerType::Keyword` 的 `unreachable!()` 替换为正确的 `vec![text]`（整个文本作为单个 token）
- **README 测试数字不一致**：统一 README/README.en.md 中测试数从 2,271/4,959 → 5,404，版本号从 1.0.0 → 1.2.0
- **CI minio:latest 可变标签**：固定为 `minio/minio:RELEASE.2024-10-13T13-34-11Z`

## [1.2.0] — 2026-07-26

### Added

- **Oracle 独立适配器包 (sz-orm-oracle)**：基于 `oracle` crate (ODPI-C 绑定) 实现 `Connection` trait，支持 Oracle 12c+；阻塞池隔离、占位符自动转换、完整类型映射
- **SQL Server 独立适配器包 (sz-orm-mssql)**：基于 `tiberius` crate (纯 Rust TDS 协议) 实现 `Connection` trait，支持 SQL Server 2008+；占位符自动转换为 `@PN` 格式
- **axum Web 框架集成 (sz-orm-axum)**：提供 `PoolState`、`JsonRows`、`JsonResp<T>`、`transaction_layer` 组件
- **actix-web Web 框架集成 (sz-orm-actix)**：提供 `PoolState`、`JsonRows`、`JsonResp<T>`、`TransactionMiddleware` 组件
- **独立查询构建器 (sz-orm-query-builder)**：提供与 core `QueryBuilder` 不同的 fluent API，支持 SELECT/INSERT/UPDATE/DELETE 及 UNION/INTERSECT/EXCEPT 集合操作
- **DI 容器 (container.rs)**：依赖注入容器，支持构造函数注入和单例注册
- **ORM 迁移集成 (migrate.rs)**：sz-orm-mig 与 sz-orm-core 的集成层
- **Whoops 调试页面 (debug_page.rs)**：开发环境调试信息展示
- **API 版本管理 (api_version.rs)**：API 版本协商与路由
- **缓存预热 (cache_warmer.rs)**：启动时预加载热点数据到缓存
- **迁移历史表 (migration_history.rs)**：迁移执行记录持久化

### Changed

- **MSRV 升级**：1.80 → 1.81（trait_variant dyn compatibility 要求）
- **workspace lints 强制**：新增 `[workspace.lints]` 配置，全 workspace clippy 零警告强制执行
- **测试数增长**：4,959 → 5,404（+445），新增 soak test/Jepsen/kill-9 崩溃恢复测试

### Fixed

- **clippy writeln_empty_string**：修复 `sz-orm-audit` 中的 `writeln!(file, "")` 警告
- **clippy unnecessary_cast**：修复 `sz-orm-sqlx` 中的 `as i64` 不必要转换
- **hydration_plugin unwrap**：修复 `chars().next().unwrap()` 为安全错误处理
- **postgis partial_cmp unwrap**：替换为 `total_cmp` 实现 NaN 安全比较

## [1.1.0] — 2026-07-22

### Added

- **位置式查询优化 (query_values / query_values_with_params)**：为 `Connection` trait 新增两个高性能查询方法，绕过 HashMap 行映射开销。SQLite 提升 34.4%，Oracle 提升 57.4%
- **真实 MQ 客户端 (sz-orm-queue)**：新增 5 种真实消息队列客户端 — RabbitMQ/NATS/Kafka/ActiveMQ Artemis/Pulsar
- **全部 37 扩展包深度优化**：测试数从 2,271 增至 4,959（+2,688），每个包补充 200-500 行高级特性代码与 15-30 个单元测试
- **Connection trait 参数绑定**：新增 `execute_with_params`/`query_with_params`，MySQL/PostgreSQL/SQLite 实现真实 prepared statement 绑定
- **编译时类型推断完善**：`SqlType` 扩展至 13 种变体，`InferSqlType` trait 覆盖 14 种 Rust 类型
- **编译时 SQL schema 生成（`schema!` 宏）**：接受 SQL CREATE TABLE 语句，编译期自动生成类型安全查询代码
- **英文文档**：新增 `README.en.md` + `CONTRIBUTING.md`
- **ADR 体系**：9 个 ADR + `ADR与生产Bug定位规范.md`
- **SeaORM 迁移指南**：547 行，10 章 + 检查清单
- **Fuzz Testing**：3 个 fuzz target（query_builder/value_escape/pool_config）
- **PooledConnection Drop 修复**：连接在 drop 时自动归还池中
- **core 包 tracing 可观测性**：关键路径添加 `#[tracing::instrument]` 注解
- **学习路线图**：面向 PHP/ThinkPHP 工程师的 17 章学习教程

### Changed

- **Rust 工具链升级**：升级至 Rust 1.97.1
- **sqlx 升级**：0.8.6 → 0.9.0，消除 rsa Marvin Attack 漏洞

### Security

- **Critical 修复 (C-2/C-3)**：修复 2 个 Critical 安全漏洞
- **反向审计全量修复**：H-1 至 H-9（9 项 High）、M-1 至 M-17（17 项 Medium）、L-1 至 L-5（5 项 Low）全部修复
- **cargo audit / cargo deny 全通过**

### Fixed

- **hook 测试锁毒化**：替换为 `AtomicU32` 无锁计数器
- **SQLite 集成测试磁盘 I/O 错误**：改用 `open_in_memory()`
- **unreachable!() 消除**：简化 `sz-orm-postgis` `st_union` 的冗余嵌套 match

### CI

- **CI 基础设施非阻塞**：4 类外部依赖 job 设为 `continue-on-error: true`
- **integration.yml 独立工作流**：手动触发 + 每日定时
- **test job 解耦**：test 不再依赖 build

[1.2.0]: https://github.com/ljclz/sz-orm/releases/tag/v1.2.0
[1.1.0]: https://github.com/ljclz/sz-orm/releases/tag/v1.1.0
[1.0.0]: https://github.com/ljclz/sz-orm/releases/tag/v1.0.0
