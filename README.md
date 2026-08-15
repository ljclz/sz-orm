# SZ-ORM — 鲜视达 ORM

> **Rust 异步 ORM 工作空间（生产就绪）**，兼容 ThinkORM 风格 API
> v4.3.0 · 60 工作空间成员 · 9205+ 测试 · 27 SQL 方言 · 已发布 crates.io

[![Rust](https://img.shields.io/badge/rust-1.81.0+-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Tests](https://img.shields.io/badge/tests-9205+-green.svg)](#测试)
[![Dialects](https://img.shields.io/badge/dialects-17-red.svg)](#支持的数据库)
[![Packages](https://img.shields.io/badge/packages-60-purple.svg)](#工作空间结构)
[![Version](https://img.shields.io/badge/version-4.7.0-blue.svg)](CHANGELOG.md)
[![Maturity](https://img.shields.io/badge/maturity-production--ready-brightgreen.svg)](#概览)
[![Security](https://img.shields.io/badge/security-audit%2Fdeny-brightgreen.svg)](#安全审计)
[![Coverage](https://img.shields.io/codecov/c/github/ljclz/sz-orm)](https://codecov.io/gh/ljclz/sz-orm)

[English Documentation](README.en.md) · [使用指南](docs/sz-orm使用指南.md) · [API 参考手册](docs/sz-ormAPI参考.md)

---

## 目录

- [概览](#概览)
- [核心特性](#核心特性)
- [质量基线](#质量基线)
- [工作空间结构](#工作空间结构)
- [快速开始](#快速开始)
- [支持的数据库](#支持的数据库)
- [核心 API](#核心-api)
- [高级模块（21 个）](#高级模块21-个)
- [钩子系统（软删除 + 多租户）](#钩子系统软删除--多租户)
- [CLI 工具](#cli-工具)
- [示例](#示例)
- [测试](#测试)
- [构建与文档](#构建与文档)
- [安全审计](#安全审计)
- [性能基准](#性能基准)
- [文档索引](#文档索引)
- [许可证](#许可证)

---

## 概览

SZ-ORM 是一个纯 Rust 实现的异步 ORM 工作空间，目标是为 Rust 生态提供一个功能完整的数据库访问层。v4.3.0 版本包含 56 个工作空间成员（v4.3.0 新增 sz-orm-explain/sz-orm-flamegraph/sz-orm-adaptive/sz-orm-fusion/sz-orm-n1-lint），覆盖 ORM 核心引擎、真实数据库适配、AI 向量搜索、分布式事务、可观测性等全栈能力，新增 9 项数据治理与运维增强能力。

### v4.1.0 新增能力（9 个 feature gate，默认关闭）

| Feature | 包 | 说明 |
|---------|-----|------|
| `data-seeding` | sz-orm-core | 数据 seeding/fixture 管理：FakerGenerator + 依赖拓扑排序 + 幂等执行 |
| `schema-diff-viz` | sz-orm-core | schema diff 可视化：text/json/html 三格式 + 破坏性变更标注 |
| `cache-coherence` | sz-orm-core | 缓存一致性协议：MESI 状态机 + 失效广播 + Write-through/behind |
| `message-tracing` | sz-orm-queue | 消息轨迹追踪：采样率控制 + 脱敏 + 端到端关联 |
| `storage-lifecycle` | sz-orm-storage | 存储生命周期管理：分层策略 + 过期清理 + 策略引擎 |
| `data-quality` | sz-orm-audit | 数据质量自动检测：六类统计学规则 + 质量报告 |
| `batch-stream` | sz-orm-batch | 批量流式处理：背压控制 + 窗口聚合 + 并行度控制 |
| `migration-branch` | sz-orm-core | 迁移版本分支：多分支并行开发 + 合并冲突检测 |
| `backup-verify` | sz-orm-back | 备份验证自动化：完整性校验 + 恢复演练 + 校验报告 |

### v4.0.0 新增能力（9 个 feature gate，默认关闭）

| Feature | 包 | 说明 |
|---------|-----|------|
| `multi-llm` | sz-orm-ai | 多 LLM 模型支持（OpenAI/Claude/Gemini/Ollama），热切换 + 负载均衡 |
| `ai-auto-tuning` | sz-orm-ai | AI 自动调优闭环：检测→建议→验证→应用→回归 |
| `hybrid-search` | sz-orm-vector | 混合搜索：向量 + 全文 + 结构化，RRF 融合 |
| `data-lineage` | sz-orm-audit | 数据 lineage 追踪：SQL AST 解析 + DAG 图 + 多格式导出 |
| `shard-rebalance` | sz-orm-sharding | 分片自动 rebalance：负载均衡 + 检查点 + 原子迁移 |
| `auto-failover` | sz-orm-rw | 数据库 failover 自动化：主从切换 + 脑裂检测 |
| `cdc` | sz-orm-queue | CDC 变更数据捕获：**轮询式捕获器（真实实现，`PollingCapturer`）+ 精确一次去重 + 多下游**；协议级捕获（PostgreSQL WAL / MySQL binlog / Oracle LogMiner / MSSQL CDC）明确未实现（需真实 DB 复制协议，返回明确错误不假装成功），见审计报告 §二-P4 |
| `async-graphql-integration` | sz-orm-graphql | GraphQL 深度集成：DataLoader + Relay + Federation |
| `service-mesh` | sz-orm-observability | 服务网格集成：Istio/Linkerd 配置生成 + 可观测性 |

> **⚠️ 诚实声明**：本项目为单作者工程实践项目，**早期生产可用（内部项目）**。sz-pay 项目已在生产环境使用 sz-orm-core/sqlx/config/auth/macros/queue/scheduler 7 个包（297 处引用、5139 测试零回归）。与 Diesel/SeaORM/SQLx 的深度对比详见 [docs/sz-orm与同类产品对比分析.md](docs/sz-orm与同类产品对比分析.md)。

### 生产就绪检查（v3.8.0 新增）

启用 `prod-ready` feature 后，可使用 `ProdReadyChecker` 执行 15 项生产就绪检查：

```rust
use sz_orm_core::prod_ready_check::{ProdReadyChecker, ProdReadyCheckerConfig};

let checker = ProdReadyChecker::new(ProdReadyCheckerConfig::default());
let report = checker.run();
println!("{}", report.to_json().unwrap());
// 15 项检查（REQ-PROD-001~015），每项附 file:line 证据
```

| Feature | 说明 |
|---------|------|
| `prod-ready` | 聚合全部 14 个子 feature |
| `prod-dialect-security` | 五方言 TLS/认证/脱敏验证 |
| `prod-n1-tuning` | N+1 检测窗口/拦截调优 |
| `prod-leak-detection` | 连接泄漏检测配置 |
| `prod-pool-tuning` | 连接池参数验证 |
| ... | 共 14 个子 feature |

| 维度 | 数据 |
|------|------|
| 工作空间成员 | **60**（58 个 sz-orm-* lib + cli + examples） |
| 支持数据库方言 | **17 种 SQL 方言**（8 原生 + 9 委派，含国产信创 6 种） |
| 测试用例 | **9205 passed, 0 failed** |
| 代码规模 | **~139,000 LOC**（深度优化后，src ~115,000 + tests ~20,000 + cli/examples/benches ~4,000） |
| 项目成熟度 | **早期生产可用（内部项目）**（sz-pay 生产试点，crates.io 已发布 sz-orm-core） |
| 异步运行时 | Tokio 1.40+ |
| Rust 最低版本 | 1.94.0+（sqlx 0.9.0 要求） |
| sqlx 版本 | 0.9.0 |
| 已知 Bug | **0** |
| `panic!`/`unimplemented!`/`todo!`/`unreachable!` | **0**（生产代码） |
| `cargo clippy -D warnings` | ✅ 0 warnings（`[workspace.lints]` 强制） |

## v3.4.0 新特性（2026-08-09）

> 质量深耕版本：测试覆盖补齐 + 架构改进 + 性能优化 + 编译期类型安全 + 文档生态 + sz-pay 生产案例深化。10 个 feature gate 全部默认关闭，无 Breaking Change。44 主任务 / 160 子任务全部完成，五方言集成测试 83 项全部通过。

### 测试覆盖补齐（`test-coverage`）

- 18 个扩展包测试从 0 → 全覆盖（每个包 ≥ 5 测试）
- 全 workspace 159 个测试套件全部通过
- 五方言集成测试：MySQL 23 + PostgreSQL 18 + SQLite 25 + Oracle 10 + DuckDB 7 = 83 项全通过

### 架构改进（`arch-improvement`）

- `async_trait_style_evaluation.md`：async trait 风格评估（dyn Trait vs async-trait vs impl Trait）
- `query_builder_selection_guide.md`：QueryBuilder 选择指南
- `result_map_macro_evaluation.md`：result_map 宏生成评估

### 性能优化（`perf-smallstring` / `perf-enum-dispatch` / `perf-zero-copy-l2` / `perf-box-str`）

```toml
sz-orm-core = { version = "3.4", features = ["perf-smallstring", "perf-enum-dispatch", "perf-zero-copy-l2", "perf-box-str"] }
```

- `SqlBuffer`：CompactString/String 双后端，短字符串 ≤ 23 字节内联存储
- `DialectKind` enum dispatch：替代 `Box<dyn Dialect>` vtable 查找
- `Value::BoxedStr(Box<str>)`：节省 8 字节/值 capacity 字段
- L2 缓存零拷贝推广：BorrowedValue + ColumnarResultSet 推广到 L2 缓存路径
- 4 个基准 + 16 个差分测试

### 编译期类型安全（`type-safe-columns` / `typed-column` / `typed-dsl`）

```toml
sz-orm-core = { version = "3.4", features = ["type-safe-columns", "typed-column", "typed-dsl"] }
```

- `Column<T: Schema>`：类型安全列引用，编译期检测列名拼写错误
- `Schema` trait + `#[derive(Schema)]`：自动生成列名常量
- `typed_ast` 扩展：`Like`/`In`/`Not` 表达式 + `BoolExpressionExt` trait
- `where_eq_col` / `where_expr`：类型安全 WHERE 条件构建
- 30 个测试 + 1 个基准

### 文档生态（`migration-guide`）

- `docs/migration/diesel_to_sz_orm.md`：Diesel → SZ-ORM 迁移指南
- `docs/migration/seaorm_to_sz_orm.md`：SeaORM → SZ-ORM 迁移指南
- `docs/migration/sqlx_to_sz_orm.md`：SQLx → SZ-ORM 迁移指南

### sz-pay 生产案例深化

- `examples/src/bin/sz_pay_pattern.rs`：脱敏版生产使用模式示例
- sz-pay 项目 6 个测试套件零回归验证通过

### 兼容性

- 无 Breaking Change，默认 feature 零行为变更
- 五方言集成测试 83 项全部通过（MySQL/PostgreSQL/SQLite/Oracle/DuckDB）
- clippy 零警告（含全部 v3.4.0 feature）
- workspace 版本统一为 3.4.0

详见 [CHANGELOG.md](CHANGELOG.md)。

## v3.3.0 新特性（2026-08-08）

> 企业级数据治理版本：多租户数据隔离 + 分布式缓存一致性 + GraphQL 查询支持 + AI 自然语言查询增强。8 个 feature gate 全部默认关闭，无 Breaking Change。22 条 EARS 需求全部满足。

### 多租户与数据隔离增强（`multi-tenant-enhanced`）

```toml
sz-orm-core = { version = "2.3", features = ["multi-tenant-enhanced"] }
```

- `TenantContext` + RAII 守卫 + `scope()` 异步作用域
- `SchemaIsolationRouter` 表名重写（`users` → `tenant_42_users`）
- `RowLevelSecurityPolicy` + `ColumnMaskingRule` 行级安全 + 列级脱敏
- `TenantAuditContext` 多租户审计日志

```rust
use sz_orm_core::tenant_context::{TenantContext, IsolationStrategy};

TenantContext::scope(42, IsolationStrategy::Schema, async {
    // 所有查询自动注入 tenant_id = 42 + 表名重写
    query.table("users").where_eq("status", "active").fetch_all().await
}).await?;
```

### 分布式缓存一致性（`dist-cache`）

```toml
sz-orm-core = { version = "2.3", features = ["dist-cache"] }
```

- `ConsistencyLevel`（Strong / Eventual）可选一致性级别
- `RedisPubSubInvalidationBus` Redis Pub/Sub 跨实例失效（HMAC 认证）
- `GossipInvalidationBus` Gossip 协议失效（≤10 实例 1s 收敛）
- `WriteBehindQueue` + `WalFile` 异步批量写入 + WAL 持久化
- `BloomFilterGuard` + `CacheMutexGuard` + `RandomTtlJitter` 击穿/雪崩防护

### GraphQL 查询支持（`graphql-n1` / `graphql-schema-gen` / `graphql-complexity`）

```toml
sz-orm-graphql = { version = "2.3", features = ["graphql-n1", "graphql-schema-gen", "graphql-complexity"] }
```

- `GraphQLIR` 递归下降解析器
- `DataLoader<K, V>` N+1 自动消除（查询次数 ≤ 2，减少 ≥ 90%）
- `SchemaGenerator` Rust 模型 → GraphQL Schema 自动生成
- `#[derive(GraphQLModel)]` 过程宏
- `ComplexityCalculator` 查询复杂度限制（深度/字段数/成本）

### AI 自然语言查询增强（`ai-nl2sql-enhanced` / `ai-index-advisor` / `ai-rewrite-advisor`）

```toml
sz-orm-ai = { version = "2.4", features = ["ai-nl2sql-enhanced", "ai-index-advisor", "ai-rewrite-advisor"] }
```

- `IntentAnalyzer` 查询意图分析 + 风险标记
- `IndexAdvisor` 自动索引建议 + 收益评估
- `RewriteAdvisor` 查询重写建议 + 等价论证
- `AiAdviceAuditRecord` AI 建议审计记录
- NL2SQL LLM prompt 增强 + `SqlSanitizer` 脱敏
- **零数据库执行保证**（仅建议展示，不自动执行）

### 兼容性

- 无 Breaking Change，默认 feature 零行为变更
- 下游 sz-pay 项目 5139 测试零回归验证通过
- clippy 零警告（含全部 v3.3.0 feature）
- sz-pay 生产使用模式示例：`cargo run --bin sz_pay_pattern`（脱敏版，展示连接池/SQL 执行/错误映射/软删除典型用法）

详见 [CHANGELOG.md](CHANGELOG.md) 和 [升级指南](docs/v3.3.0-upgrade-guide.md)。

## v1.5.0 新特性（2026-08-05）

### 连接池 Prometheus 统计指标

- **`PoolMetrics`**：`Pool::pool_metrics()` 返回池生命周期累计指标（acquire_count / acquire_failed_count / acquire_wait_time / release_count / connection_created_count / connection_closed_count）
- **`average_acquire_wait_time()`**：平均获取等待时长（`acquire_wait_time / acquire_count`）
- 基于无锁 `AtomicU64` 原子计数，对 acquire/release 热路径开销可忽略；4 个单元测试验证计数正确性

### ClickHouse 行锁支持

- `ClickHouseDialect::supports_lock_for_update()` / `supports_lock_shared()` 显式返回 `false`（列式 OLAP 无事务无行锁）
- `build_insert_or_ignore_prefix()` 回退为普通 `INSERT INTO`（ClickHouse 不支持 `INSERT OR IGNORE`）

### SQL Server INSERT OR IGNORE 回退

- `SqlServerDialect::build_insert_or_ignore_prefix()` 回退为普通 `INSERT INTO`（SQL Server 无等价前缀语法；MERGE / IF NOT EXISTS 均无法以"前缀"形式表达）
- 应用层可通过唯一索引 + 捕获重复键冲突（SQLSTATE 2601/2627）或 MERGE 实现幂等插入

### DuckDB 真实集成测试

- `packages/sz-orm-core/tests/integration_duckdb.rs`：7 个真实 DB 测试（建表、参数化插入/查询、INSERT OR IGNORE、分页、ALTER TABLE、转义、DROP TABLE），使用 `duckdb` bundled 特性（编译需 MSVC + CMake）

### 向量 / 时序真实实现集成测试

- `sz-orm-vector`：3 个 `#[ignore]` 真实 PostgreSQL + pgvector 测试（create/insert/search 工作流、upsert 覆盖、欧氏距离排序）
- `sz-orm-timeseries`：5 个内存版集成测试 + 2 个 `#[ignore]` 真实 TimescaleDB 测试（hypertable 创建 + 时间桶聚合）

### crates.io 发布

- sz-orm-core **1.5.0** ✅（2026-08-05）

## v1.4.0 新特性（2026-08-05）

### 锁查询（TASK-024~027）

- **`lock_for_update()`**：悲观锁，`SELECT ... FOR UPDATE`，支持 MySQL / PostgreSQL / SQLite（行锁）
- **`lock_shared()`**：共享锁，`SELECT ... FOR SHARE`（PostgreSQL）/ `LOCK IN SHARE MODE`（MySQL）
- **`LockType` 枚举**：`Update` / `Shared`，通过 `Dialect` trait 的 `lock_clause()` 方法生成方言特定 SQL
- 15 个单元测试 + 3 个 MySQL 集成测试 + 1 个基准测试

### INSERT OR IGNORE（TASK-028~029）

- **`insert_or_ignore()`**：插入时忽略唯一键冲突，支持 MySQL（`INSERT IGNORE`）/ PostgreSQL（`ON CONFLICT DO NOTHING`）/ SQLite（`INSERT OR IGNORE`）/ DuckDB（`INSERT OR IGNORE`）
- 2 个 MySQL 集成测试 + 1 个基准测试

### DuckDB 方言支持（TASK-033~036）

- **`DuckDBDialect`**：完整实现 Dialect trait，支持 CREATE TABLE / ALTER TABLE / 索引 / INSERT OR IGNORE
- **`DbType::DuckDB`**：新增枚举变体，`as_str()` / `from_str()` / `default_port()` 均已支持
- 10 个单元测试覆盖建表、改表、类型映射

### Redis 分布式缓存后端默认启用

- **`RedisBackend`**（Fix #39）：基于 redis 0.27 + `tokio-comp` + `connection-manager`（自动重连连接池）
- 支持 `GET` / `SET EX` / `DEL` / `SCAN` + pipeline 批量删除（避免 `KEYS` 阻塞主线程）
- **默认启用**：`redis` feature 已加入 `default`，`RedisBackend::new("redis://127.0.0.1:6379/0")` 开箱即用
- 新增 1 个单元测试（无效 URL 错误路径）+ 4 个真实集成测试（`--ignored`，需本地 Redis 服务）

### crates.io 发布

- sz-orm-sql-validator 1.4.0 ✅
- sz-orm-macros 1.4.0 ✅
- sz-orm-core 1.4.0 ✅
- sz-orm-core 1.5.0 ✅（连接池统计指标 + ClickHouse 行锁 + SQL Server INSERT OR IGNORE 回退）

## v1.3.0 新特性（2026-08-05）

### 性能优化

- **连接池预热**（TASK-021）：`PoolConfig::prewarm` 启用后，池创建时立即建立 `min_idle` 个连接，首次 `acquire()` 延迟从 < 100ms 降至 < 10ms
- **查询缓存 TTL**（TASK-022）：`QueryBuilder::cache_ttl(Duration)` 预留查询结果缓存 TTL 设置项（⚠️ 2026-08-13 勘误：执行路径尚未消费该 TTL，暂为死 API，见 [审计报告 §二-5](docs/assessment/2026-08-13-production-zero-call-audit.md)）

### API 增强

- **deprecated 方法移除**（TASK-010~012）：移除 `QueryBuilder::where_cond` / `or_where` 等字符串拼接方法，强制使用参数化查询（`where_eq` / `where_ne` / `where_gt` 等），杜绝 SQL 注入
- **PoolConfigBuilder::prewarm**：链式构建方法，支持 `PoolConfigBuilder::new().prewarm(true).min_idle(5).build()`

### 质量改进

- **测试覆盖**：新增 3 个预热测试（`test_pool_prewarm` / `test_pool_prewarm_failure_non_blocking` / `test_pool_prewarm_disabled`），验证预热逻辑正确性
- **文档完善**：所有公开 API 补充 `///` 文档注释和示例，`cargo doc` 零警告

### crates.io 发布

- sz-orm-sql-validator 1.3.0 ✅
- sz-orm-macros 1.3.0 ✅
- sz-orm-core 1.3.0 ✅

## 核心特性

- **异步**：基于 Tokio，全程 `async/await`
- **多数据库方言**：17 种 SQL 方言（8 原生：MySQL/PostgreSQL/SQLite/Oracle/SQL Server/ClickHouse/DB2/DuckDB + 9 委派：MariaDB/TiDB/OceanBase/达梦/金仓/PolarDB/GaussDB/GBase/Sybase）
- **链式 QueryBuilder**：仿 ThinkORM 风格的 fluent API
- **ACID 事务**：隔离级别、保存点（默认 8 层嵌套，`DEFAULT_MAX_NESTING_DEPTH = 8`，可配置）、`TransactionManager` 多事务管理
- **连接池**：可配置大小、超时、空闲回收、健康检查、最大生命周期
- **迁移系统**：up/down/rollback/reset/refresh + `SchemaBuilder` 程序化建表
- **多级缓存**：`MemoryCache` / `MultiLevelCache` / `L2Cache`，支持 TTL 与表级失效（⚠️ 2026-08-13 勘误：组件可用，查询执行路径未自动接入缓存，见 [审计报告 §二-8](docs/assessment/2026-08-13-production-zero-call-audit.md)）
- **钩子系统**：16 种生命周期事件 + `HookDispatcher` + `HookRegistry` 运行时钩子（⚠️ 2026-08-13 勘误：需手动 `registry.dispatch`，CRUD 执行不自动触发，见 [审计报告 §二-7](docs/assessment/2026-08-13-production-zero-call-audit.md)）
- **软删除**：`SoftDelete` trait + `SoftDeleteScope` 全局作用域
- **多租户**：`TenantModel` trait + `TenantScope` 自动 `tenant_id = ?` 过滤
- **SQL 校验**：编译期（`sql_string!`）+ 运行时（`validate()`）双重校验、10 种注入模式检测（5 种正则模式 + 5 种 AST 模式）
- **关联关系**：BelongsTo / HasMany / HasOne / BelongsToMany + Eager Loading + `find_with_related`
- **21 个高级模块**：accessors/behaviors/data_permission/dirty_attributes/dynamic_filter/entity_graph/guard/hydration_plugin/join_dsl/l2_cache/lambda/observer/optimistic_lock/phinx_migration/queryable/quick_query/repository/result_map/schema_gen/sql_safety/type_handler
- **分布式事务**：2PC + TCC（Try-Confirm-Cancel）+ Saga + 跨分片 ACID 协调器
- **AI 向量 + pgvector**：sz-orm-vector（cosine/euclidean/dot 三种度量）+ sz-orm-ai（NL→SQL + RAG + Embedding）
- **可观测性**：sz-orm-observability（Prometheus exporter + OTLP + SLO 监控） + sz-orm-tracing（OpenTelemetry traceparent 传播）
- **扩展生态**：加密、JWT、调度、MQTT、WebSocket、消息队列（7 种，其中 RocketMQ 为 stub）、对象存储（7 种，其中 6 种为 in-memory mock）、gRPC、GraphQL、ES、Swagger、脱敏、健康检查、审计、批量、WASM、备份、读写分离、分库分表、限流、迁移、PostGIS、TimescaleDB、搜索（ES/Meilisearch/OpenSearch）

## 质量基线

- 7 线验证体系：TDD + Integration + Jepsen + Fuzz + Stress + Chaos + Formal
- 0 `panic!` / 0 `unimplemented!` / 0 `todo!` / 0 `unreachable!`（生产代码）
- `[workspace.lints]` 强制 `clippy::all` 0 警告，编译期质量门禁
- `cargo clippy --workspace --all-targets -- -D warnings` 通过，0 warnings
- `cargo fmt --all --check` 通过
- `cargo audit` — 0 未忽略漏洞（7 个传递依赖忽略项，均有文档说明）
- `cargo deny check advisories bans licenses sources` — 全部 OK
- 1 小时 Soak Test：13.8 亿次操作，1.16% 吞吐衰减，P99 43μs→41μs，0 错误，无连接池泄漏（复现：`SOAK_DURATION=1h cargo test -p sz-orm-core --test soak -- --ignored --nocapture`）

## 工作空间结构

```
sz-orm/
├── packages/
│   ├── sz-orm-core/                 # 核心引擎（Model/Query/Dialect/Pool/Tx/Migration/Cache/Hooks + 21 高级模块）
│   ├── sz-orm-sqlx/                 # sqlx 真实数据库适配器（MySQL/PG/SQLite）
│   ├── sz-orm-sql-validator/        # SQL 语法与注入校验
│   ├── sz-orm-macros/               # 派生宏 + sql_string! 编译期校验
│   ├── sz-orm-query-builder/        # quote_ident + check_where_injection
│   ├── sz-orm-observability/        # MetricsRegistry + Counter/Gauge/Histogram + SloMonitor
│   ├── sz-orm-tracing/              # OpenTelemetry OTLP exporter
│   ├── sz-orm-vector/               # pgvector 集成（cosine/euclidean/dot）
│   ├── sz-orm-ai/                   # NL→SQL + Embedding + RAG
│   │
│   ├── sz-orm-crypto/               # AES-256-GCM / PBKDF2 / HMAC
│   ├── sz-orm-auth/                 # JWT 认证
│   ├── sz-orm-scheduler/            # Cron 定时任务
│   ├── sz-orm-mqtt/                 # MQTT 客户端（rumqttc）
│   ├── sz-orm-websocket/            # WebSocket 服务
│   ├── sz-orm-queue/                # RabbitMQ/Kafka/NATS/ActiveMQ/RocketMQ/Pulsar
│   ├── sz-orm-storage/              # S3/Aliyun/Tencent/Huawei/Qiniu/Upyun/Local
│   ├── sz-orm-grpc/                 # gRPC（tonic）
│   ├── sz-orm-graphql/              # GraphQL（async-graphql + axum）
│   ├── sz-orm-postgis/              # PostGIS 几何
│   ├── sz-orm-timeseries/           # TimescaleDB
│   ├── sz-orm-search/               # Elasticsearch/OpenSearch/Meilisearch
│   ├── sz-orm-es/                   # Elasticsearch legacy
│   ├── sz-orm-logger/               # 结构化日志
│   ├── sz-orm-swagger/              # OpenAPI 文档生成
│   ├── sz-orm-masking/              # 数据脱敏
│   ├── sz-orm-health/               # 健康检查
│   ├── sz-orm-audit/                # 审计日志
│   ├── sz-orm-batch/                # 批量操作
│   ├── sz-orm-dtx/                  # 分布式事务（2PC/TCC/Saga）
│   ├── sz-orm-rw/                   # 读写分离
│   ├── sz-orm-sharding/             # 分库分表
│   ├── sz-orm-limit/                # 限流
│   ├── sz-orm-config/               # 配置管理
│   ├── sz-orm-mig/                  # 数据迁移转换器
│   ├── sz-orm-wasm/                 # WebAssembly 目标
│   ├── sz-orm-lc/                   # 本地/边缘计算
│   └── sz-orm-back/                 # 备份与恢复
│
├── cli/                             # CLI 工具（sz-orm）
├── examples/                        # 9 个可运行示例
├── grafana/                         # Grafana 仪表盘 JSON
├── docs/                            # 22 份文档（含 adr/ 子目录 11 份 ADR）
├── scripts/                         # gate.ps1/sh, install-hooks, audit-api-changes
├── Cargo.toml                       # 工作空间清单（version.workspace = true）
├── audit.toml                       # cargo-audit 配置（7 个忽略项）
├── deny.toml                        # cargo-deny 配置（14 个允许许可证）
├── Dockerfile                       # 容器镜像
└── docker-compose.yml               # 全栈开发环境
```

## 快速开始

### 1. 添加依赖

```toml
[dependencies]
# 从 crates.io 安装（推荐）
sz-orm-core = "1.4"
sz-orm-sqlx = "1.4"

# 本地开发（path 依赖）
# sz-orm-core = { version = "1.5", path = "packages/sz-orm-core" }
# sz-orm-sqlx = { version = "1.5", path = "packages/sz-orm-sqlx" }

tokio = { version = "1.40", features = ["full"] }
```

### 2. 定义 Model

```rust
use sz_orm_core::{Model, TimestampFields};

#[derive(Debug, Clone, Default)]
struct User {
    id: i64,
    name: String,
    email: String,
}

impl Model for User {
    type PrimaryKey = i64;
    fn table_name() -> &'static str { "users" }
    fn pk(&self) -> Self::PrimaryKey { self.id }
    fn set_pk(&mut self, pk: Self::PrimaryKey) { self.id = pk; }
    fn timestamp_fields() -> Option<TimestampFields> {
        Some(TimestampFields::with_both("created_at", "updated_at"))
    }
}
```

### 3. 构建查询

```rust
use sz_orm_core::dialect::get_dialect;
use sz_orm_core::{DbType, QueryBuilder, Value};

let dialect = get_dialect(DbType::MySQL)?;
let sql = QueryBuilder::<User>::new(dialect)
    .table("users")
    .select(vec!["id", "name", "email"])
    .where_eq("status", Value::String("active".to_string()))
    .order_by("created_at")
    .limit(10)
    .build_select();
```

### 4. 连接真实数据库（sz-orm-sqlx）

```rust,no_run
use sz_orm_core::{Pool, PoolConfigBuilder};
use sz_orm_sqlx::{SqlitePoolHandle, SqlxSqliteConnectionFactory};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let handle = SqlitePoolHandle::connect("sqlite::memory:").await?;
    let factory = Arc::new(SqlxSqliteConnectionFactory::new(Arc::new(handle)));
    let config = PoolConfigBuilder::new().max_size(10).build()?;
    let pool = Pool::new(config, factory)?;

    let mut conn = pool.acquire().await?;
    let rows = conn.query("SELECT 1 AS one").await?;
    println!("rows = {}", rows.len());
    Ok(())
}
```

MySQL / PostgreSQL 请替换为 `MySqlPoolHandle` / `PgPoolHandle` 与 `SqlxMySqlConnectionFactory` / `SqlxPgConnectionFactory`。

### 5. 编译期 SQL 校验（sql_string!）

```rust
use sz_orm_core::sql_string;

let sql = sql_string!("SELECT * FROM users WHERE id = 1");         // OK
let sql = sql_string!("SELECT * FROM users WHERE id = ?"; params: 1); // OK — 参数数量已校验
// sql_string!("SELECT * FORM users");                              // 编译错误：缺少 FROM
// sql_string!("SELECT * FROM users WHERE name = 'x' OR '1'='1'"); // 编译错误：注入模式
```

## 支持的数据库

| 数据库 | 方言 | 真实连接 | 默认端口 |
|--------|------|----------|----------|
| MySQL | `MySqlDialect`（反引号） | sz-orm-sqlx | 3306 |
| PostgreSQL | `PostgreSqlDialect`（双引号） | sz-orm-sqlx | 5432 |
| SQLite 3.35+ | `SqliteDialect` | sz-orm-sqlx | — |
| Oracle 23ai | `OracleDialect`（`:N` 占位符 + OFFSET/FETCH） | sz-orm-oracle（基于 `oracle` crate / ODPI-C 绑定） | 1521 |
| OceanBase | 兼容 `MySqlDialect` | — | 2881 |
| SQL Server | `SqlServerDialect`（独立实现，TDS 协议） | sz-orm-mssql（基于 `tiberius` crate） | 1433 |
| ClickHouse | `ClickHouseDialect`（独立实现，非 MySQL 兼容） | — | 8123 |
| DuckDB | `DuckDBDialect`（独立实现，支持 INSERT OR IGNORE） | — | — |
| Redis | NoSQL（无 SQL 方言，L2Cache 分布式缓存后端默认启用） | — | 6379 |
| MongoDB | NoSQL | — | 27017 |
| VectorDB | 向量数据库 | sz-orm-vector | 19530 |
| PureJsDb | JS 引擎 DB | — | — |

使用 `get_dialect(DbType::MySQL)` 获取方言实例。

## 核心 API

### QueryBuilder 链式 API

```rust
QueryBuilder::<M>::new(dialect)
    .table("users")
    .select(vec!["id", "name"])
    .where_eq("status", Value::String("active".to_string()))  // AND
    .or_where_eq("role", Value::String("admin".to_string()))   // OR
    .where_in("id", vec![Value::I64(1), Value::I64(2)])
    .where_between("age", Value::I64(18), Value::I64(30))
    .where_null("deleted_at")
    .order_by("created_at")
    .order_desc("id")
    .group_by("status")
    .having("COUNT(*) > 5")
    .limit(20)
    .offset(40)
    .page(3, 20)                                 // 第 3 页，每页 20 条
    .join_inner("posts", "users.id", "posts.user_id")
    .join_left("profiles", "users.id", "profiles.user_id")
    .build_select();

// 聚合
builder.build_count();
builder.build_exists();
builder.build_max("score");
builder.build_min("price");
builder.build_sum("amount");
builder.build_avg("value");

// 校验
builder.validate()?;              // SELECT 校验
builder.validate_insert(&data)?;  // INSERT 校验
builder.validate_update(&data)?;  // UPDATE 校验
builder.validate_delete()?;       // DELETE 校验
```

### 连接池

```rust
use sz_orm_core::{Pool, PoolConfigBuilder};

let config = PoolConfigBuilder::new()
    .max_size(100)
    .min_idle(10)
    .acquire_timeout(30)
    .idle_timeout(600)
    .max_lifetime(1800)
    .build()?;

let pool = Pool::new(config, factory)?;
let conn = pool.acquire().await?;
pool.release(conn).await;
pool.status().await;     // PoolStatus { idle, active, max, min }
pool.reap_idle().await;
pool.close_all().await;
```

### 事务

```rust
use sz_orm_core::{Transaction, TransactOptions, IsolationLevel};

let opts = TransactOptions::default()
    .with_isolation(IsolationLevel::Serializable)
    .read_only()
    .with_timeout(Duration::from_secs(30));

let mut tx = Transaction::new(conn, opts);
tx.execute("INSERT INTO users VALUES (1)").await?;

// 保存点（嵌套事务）
let sp = tx.savepoint().await?;
tx.rollback_to_savepoint(&sp).await?;
tx.release_savepoint(&sp).await?;

tx.commit().await?;
// tx.rollback().await?;
```

### 迁移系统

```rust
use sz_orm_core::migration::{FileMigrationResolver, MigrationContext, Migrator, SchemaBuilder};
use sz_orm_core::{MigrationResolver, DbType};

// 文件迁移：<version>_<name>_up.sql / <version>_<name>_down.sql
let resolver = FileMigrationResolver::new("./migrations".into());
let migrations = resolver.resolve(DbType::MySQL)?;

let mut migrator = Migrator::new(MigrationContext::default())
    .add_migrations(migrations);

migrator.migrate().await?;                // 应用所有待迁移
migrator.up(Some("003")).await?;           // 应用至 003
migrator.down(Some("001")).await?;         // 回滚至 001
migrator.rollback("002").await?;           // 回滚单个
migrator.reset().await?;                   // 回滚所有 + 重新应用
migrator.refresh().await?;                 // reset 别名
migrator.progress();                       // 迁移进度

// SchemaBuilder 程序化 DDL
let sql = SchemaBuilder::new("users")
    .add_column(ColumnDef::new("id", "BIGINT").not_null().auto_increment())
    .add_column(ColumnDef::new("name", "VARCHAR").length(255).not_null())
    .add_index(IndexDef::new("idx_email", vec!["email"]).unique())
    .add_foreign_key(
        ForeignKeyDef::new("fk_role", "role_id", "roles", "id").on_delete("CASCADE")
    )
    .build(DbType::MySQL);
```

### Value 类型（20 种变体）

```rust
use sz_orm_core::Value;

// 变体
Value::Null | Bool(bool) | I8 | I16 | I32 | I64 | U8 | U16 | U32 | U64
| F32 | F64 | String(String) | Bytes(Vec<u8>) | Uuid(String)
| Date(String) | DateTime(String) | Time(String) | Json(String)
| Array(Vec<Value>) | Object(HashMap<String, Value>)

// 转换
value.as_str();    // Option<&str>
value.as_i64();    // Option<i64>
value.as_f64();    // Option<f64>
value.as_bool();   // Option<bool>
value.as_bytes();  // Option<&[u8]>

// From 实现
let v: Value = 42i64.into();
let v: Value = "hello".into();
let v: Value = vec![1u8, 2u8].into();
```

## 高级模块（21 个）

sz-orm-core 在基础引擎之外提供 21 个高级模块。详见 [使用指南 §3.7](docs/sz-orm使用指南.md#37-sz-orm-core-高级特性模块21-个) 与 [API 参考 §2.22](docs/sz-ormAPI参考.md#222-sz-orm-core-高级特性模块21-个)。

| # | 模块 | 亮点 |
|---|------|------|
| 1 | `accessors` | 字段访问器/修改器 + 类型转换 |
| 2 | `behaviors` | 可插拔行为（TimestampBehavior / BlameableBehavior） |
| 3 | `data_permission` | 数据权限拦截器（TenantIsolation / OwnerOnly / DepartmentScope） |
| 4 | `dirty_attributes` | 脏字段追踪（DirtyTracker + build_dynamic_update） |
| 5 | `dynamic_filter` | 运行时动态 Filter（FilterRegistry） |
| 6 | `entity_graph` | 实体图 + 批量加载器（解决 N+1） |
| 7 | `guard` | SQL 安全卫士（SafeSqlGuard + GuardPolicy::Strict） |
| 8 | `hydration_plugin` | Hydration + 插件链（SqlLogPlugin / SlowQueryPlugin / AuditPlugin） |
| 9 | `join_dsl` | 类型安全 JOIN DSL（JoinBuilder + 5 JoinKind） |
| 10 | `l2_cache` | L2 缓存（LRU + TTL + 表级失效） |
| 11 | `lambda` | Lambda 类型安全查询（LambdaWrapper + define_columns! 宏） |
| 12 | `observer` | Model 生命周期观察者（9 事件 + EventDispatcher） |
| 13 | `optimistic_lock` | 乐观锁（OptimisticLock trait + retry fn） |
| 14 | `phinx_migration` | Phinx 风格 Schema 构建器（14 ColumnType + index + FK） |
| 15 | `queryable` | Diesel 风格 Queryable trait（from_row） |
| 16 | `quick_query` | 通过 Db::name() 快速查询（无需 Model） |
| 17 | `repository` | DDD 仓储模式（Repository trait + InMemoryRepository + PageResult） |
| 18 | `result_map` | MyBatis ResultMap + Hibernate Native Query |
| 19 | `schema_gen` | Diesel 风格 schema.rs 自动生成 |
| 20 | `sql_safety` | SQL 注入原语（validate_identifier / validate_fk_action / validate_id_value） |
| 21 | `type_handler` | MyBatis 风格 TypeHandler SPI（DateTimeHandler / UuidHandler / ...） |

## 钩子系统（软删除 + 多租户）

### HookContext — 执行上下文

```rust
use sz_orm_core::hooks::HookContext;

let mut ctx = HookContext::new()
    .with_tenant(42)
    .with_operator(1)
    .with_timestamp(1700000000);
ctx.set_meta("source", "api");
```

### Hookable trait — 16 个生命周期钩子

```rust
use sz_orm_core::hooks::{Hookable, HookContext, HookResult};

impl Hookable for User {
    fn before_insert(_ctx: &mut HookContext) -> HookResult<()> { Ok(()) }
    fn after_insert(_ctx: &HookContext, _id: &Self::PrimaryKey) -> HookResult<()> { Ok(()) }
    fn before_update(_ctx: &mut HookContext, _id: &Self::PrimaryKey) -> HookResult<()> { Ok(()) }
    fn after_update(_ctx: &HookContext, _id: &Self::PrimaryKey) -> HookResult<()> { Ok(()) }
    fn before_delete(_ctx: &mut HookContext, _id: &Self::PrimaryKey) -> HookResult<()> { Ok(()) }
    fn after_delete(_ctx: &HookContext, _id: &Self::PrimaryKey) -> HookResult<()> { Ok(()) }
    // ... 另外 10 个（before_save / after_save / before_write / after_write / before_validate / after_validate / before_restore / after_restore / before_find / after_find）
}
```

### SoftDelete + SoftDeleteScope

```rust
use sz_orm_core::hooks::{SoftDelete, SoftDeleteScope, GlobalScope};

impl SoftDelete for Product {
    fn soft_delete_field() -> &'static str { "deleted_at" }
    fn is_deleted(&self) -> bool { self.deleted_at.is_some() }
}

// 查询时自动追加：AND deleted_at IS NULL
let scope = <(SoftDeleteScope, Product) as GlobalScope>::apply_scope(&ctx);
```

### TenantModel + TenantScope

```rust
use sz_orm_core::hooks::{TenantModel, TenantScope, GlobalScope};

impl TenantModel for Order {
    fn tenant_field() -> &'static str { "tenant_id" }
    fn tenant_id(&self) -> i64 { self.tenant_id }
    fn set_tenant_id(&mut self, tenant_id: i64) { self.tenant_id = tenant_id; }
}

// 当 ctx.tenant_id = Some(42)：自动追加 AND tenant_id = ?
// 当 ctx.tenant_id = None：不追加（跨租户查询，调用方需自行确保安全）
let scope = <(TenantScope, Order) as GlobalScope>::apply_scope(&ctx);
```

### HookRegistry — 运行时钩子注册

```rust
use sz_orm_core::hooks::{HookRegistry, HookEvent};
use std::sync::Arc;

let registry = HookRegistry::new();
registry.register(
    HookEvent::BeforeInsert,
    Arc::new(|_ctx| { println!("before insert"); Ok(()) }),
);

registry.dispatch(HookEvent::BeforeInsert, &ctx)?;
registry.clear(HookEvent::BeforeInsert);
registry.clear_all();
```

### ScopeRegistry — 作用域控制

```rust
use sz_orm_core::hooks::ScopeRegistry;

let registry = ScopeRegistry::new();
registry.disable("soft_delete");       // 禁用软删除作用域
registry.enable("soft_delete");        // 重新启用
registry.is_enabled("soft_delete");    // true

// 临时禁用（闭包内）
let result = registry.without_scope("soft_delete", || {
    // 此处查询将包含软删除行
    42
});
```

## CLI 工具

SZ-ORM 提供 `sz-orm` CLI，用于迁移管理、代码生成与 SQL 校验。

### 安装

```bash
cargo install --path cli
```

### 命令

```bash
sz-orm                              # 显示帮助
sz-orm info                         # 显示 ORM 摘要
sz-orm --version                    # 显示版本

sz-orm dialect list                 # 列出所有方言
sz-orm dialect show mysql           # 显示 MySQL 方言详情

sz-orm make:migration create_users  # 生成迁移骨架
sz-orm make:model User              # 生成 Model 骨架

sz-orm migrate                      # 显示待迁移
sz-orm migrate:status               # 显示迁移进度

sz-orm sql:validate "SELECT * FROM users"  # SQL 校验
```

### 选项

- `--migrations <dir>` — 迁移文件目录（默认 `./migrations`）
- `--output <dir>` — 生成代码输出目录（默认 `./src/models` 或 `./migrations`）

## 示例

`examples/` 目录提供 8 个可运行示例：

| 示例 | 描述 | 运行 |
|------|------|------|
| `quick_start` | QueryBuilder 基础 | `cargo run -p sz-orm-examples --bin quick_start` |
| `model_definition` | Model + ModelExt 完整实现 | `cargo run -p sz-orm-examples --bin model_definition` |
| `transaction` | 事务 + 保存点 | `cargo run -p sz-orm-examples --bin transaction` |
| `migration` | SchemaBuilder DDL | `cargo run -p sz-orm-examples --bin migration` |
| `hooks_soft_delete` | 钩子 + 软删除 | `cargo run -p sz-orm-examples --bin hooks_soft_delete` |
| `multi_tenant` | 多租户隔离 | `cargo run -p sz-orm-examples --bin multi_tenant` |
| `production_app` | 生产应用模式 | `cargo run -p sz-orm-examples --bin production_app` |
| `production_dtx` | 分布式事务模式 | `cargo run -p sz-orm-examples --bin production_dtx` |

## 测试

SZ-ORM 通过 **7 线验证体系**保障质量：

| 方法 | 描述 | 测试文件 |
|------|------|----------|
| **TDD** | 核心单元测试 | `core.rs` |
| **Integration** | 真实 MySQL/PG/SQLite/Oracle E2E | `integration_*.rs` |
| **Jepsen** | 并发正确性 + 真实 DB Jepsen | `jepsen.rs`, `real_db_jepsen.rs` |
| **Fuzz** | 边界/极端用例 | `fuzz.rs` |
| **Stress** | 性能/压力 | `stress.rs`, `core_bench.rs` |
| **Chaos** | 故障鲁棒性 | `chaos.rs` |
| **Formal** | 形式化不变量验证 | `formal.rs` |

**总计：9205 tests, 0 failed**（部分测试需真实 DB/云凭证）

### 运行测试

```bash
# 全工作空间测试
cargo test --workspace

# 仅核心包
cargo test -p sz-orm-core

# 真实 DB 测试（需 MySQL/PG/SQLite 运行）
cargo test -p sz-orm-core --features testing

# 性能基准
cargo bench -p sz-orm-core

# 6h Soak 测试
SOAK_DURATION=6h cargo test -p sz-orm-core --test soak -- --ignored
```

## 构建与文档

### 构建

```bash
# 全工作空间
cargo build --workspace

# 仅核心包
cargo build -p sz-orm-core

# Release 构建
cargo build --workspace --release
```

### 文档

```bash
# 生成文档
cargo doc --workspace --no-deps --open

# Lint
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

## 安全审计

SZ-ORM 通过 CI 集成的 `cargo-audit` 与 `cargo-deny` 保障安全：

```bash
# 漏洞扫描
cargo audit \
            --ignore RUSTSEC-2026-0049 \
            --ignore RUSTSEC-2026-0098 \
            --ignore RUSTSEC-2026-0099 \
            --ignore RUSTSEC-2026-0104 \
            --ignore RUSTSEC-2026-0194 \
            --ignore RUSTSEC-2026-0195 \
            --ignore RUSTSEC-2025-0134

# 综合检查（advisories + bans + licenses + sources）
cargo deny check advisories bans licenses sources
```

**结果（2026-07-21）**：
- ✅ `cargo audit`：0 未忽略漏洞（7 个传递依赖忽略项，均有文档说明）
- ✅ `cargo deny`：advisories ok / bans ok / licenses ok / sources ok
- 许可证白名单：14 个宽松许可证（MIT / Apache-2.0 / BSD / ISC / Zlib / CC0-1.0 / MPL-2.0 / ...）
- 来源：仅允许 crates.io 官方 registry；无 git/path 来源
- CI：`.github/workflows/security.yml` 在每次 push/PR 到 main/master 时运行

### rsa Marvin Attack 已消除

**rsa Marvin Attack 已通过 sqlx 0.8.6 → 0.9.0 升级彻底消除**：rsa 已从依赖树中完全移除，该漏洞不再触发。当前 7 个忽略项均与 rsa 无关。

## v2.0.0 路线图交付（2026-08-06）

### 交付清单

| 任务 | 状态 | 交付物 |
|------|------|--------|
| Oracle 集成测试 | ✅ 完成 | `tests/integration_oracle.rs` 追加 7 类场景，10 测试通过 |
| SQL Server 集成测试 | ✅ 完成 | `tests/integration_mssql.rs` 新建 8 类场景，5 方言断言通过 |
| Python 绑定（PyO3） | ✅ 完成 | `packages/sz-orm-python/`，暴露 PyModel/PyQueryBuilder/PyPool/PyTransaction |
| JavaScript 绑定（napi-rs） | ✅ 完成 | `packages/sz-orm-js/`，暴露 Model/QueryBuilder/Pool/Transaction |
| 安全专项审计 | ✅ 完成 | `docs/assessment/2026-08-05-security-audit-report.md`，7 维度覆盖 |

### 门禁验证

11 道门禁中 9 项通过，2 项环境限制（`cargo audit` 网络受限、`--all-features` 缺 protoc）。

### 审计结论

🟡 中等风险：3 处已废弃 `where_cond`/`or_where` 需在 v2.0.0 移除，unwrap/expect 需补齐 SAFETY 注释。详见 [安全审计报告](docs/assessment/2026-08-05-security-audit-report.md)。

---

## 性能基准

criterion 基准（sample_size=10, measurement_time=3s, warm_up=1s, Windows；复现：`cargo bench -p sz-orm-core`）：

| 基准 | 结果 |
|------|------|
| `value_to_param/null` | 3.2 ns（312 Melem/s） |
| `value_to_param/i64` | 53.4 ns（18.7 Melem/s） |
| `value_to_param/string_short` | 252 ns（3.97 Melem/s） |
| `dialect_escape_string/long_1024` | 954 ns（1.02 GiB/s） |
| `dialect_build_create_table/100 cols` | 31.7 µs（3.15 Melem/s） |
| `dialect_build_pagination/1M page` | 163 ns（页深度稳定） |
| `pool_acquire_release` | 230 ns / 往返 |
| `in_memory_scan/select_where_eq_1pct/100K` | 4.87 ms（20.5 Melem/s） |
| `json_parsing/3kb` | 85.0 µs（71 MiB/s） |

**真实 DB 批量 INSERT 吞吐**（10 万行）：

| 数据库 | 吞吐 | 相对值 |
|--------|------|--------|
| SQLite（文件） | 720K rows/s | 4.97× |
| PostgreSQL 18 | 268K rows/s | 1.85× |
| MySQL 9.6 | 145K rows/s | 1.0×（基线） |
| Oracle 23ai Free | 19.1K rows/s | 0.13× |

**1 小时 Soak 测试**：13.8 亿次操作，1.16% 吞吐衰减，P99 43μs→41μs，0 错误（复现方式同上）。

## 文档索引

| 文档 | 描述 |
|------|------|
| [学习教程](docs/sz-orm学习路线图.md) | 面向 PHP/ThinkPHP 工程师的具体学习教程（含 Rust 速通 + AI 协作姿势） |
| [使用指南](docs/sz-orm使用指南.md) | 端到端使用指南（v5.0，覆盖全部 21 个高级模块） |
| [API 参考](docs/sz-ormAPI参考.md) | 类型签名与参数文档（v5.0） |
| [架构设计](docs/sz-orm架构设计.md) | 39 包架构概览 |
| [工程实践](docs/sz-orm-engineering-practices.md) | Gate 1-10 + 测试金字塔 T1-T6 |
| [API 契约](docs/api-contracts.md) | 公共 API 稳定性契约 |
| [ADR 索引](docs/adr/README.md) | 架构决策记录（9 条 ADR） |
| [ADR 与 Bug 定位规范](docs/ADR与生产Bug定位规范.md) | 可复用规范：ADR 写作 + 四层 bug 定位流程 + 有效性验证（v1.0） |

## 许可证

MIT License © SZ-ORM Team
