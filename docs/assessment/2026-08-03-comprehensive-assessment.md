# SZ-ORM 真实能力评估报告（基于源码验证）

> **评估日期**：2026-08-03
> **评估方法**：直接读取源码，不依赖任何文档
> **验证范围**：43 个 workspace 成员全部扫描
> **重要声明**：本报告所有结论均基于 `packages/*/src/*.rs` 实际代码，非文档描述

---

## 一、实际拥有的能力（源码验证 ✅）

### 1.1 宏系统（`sz-orm-macros`，3671 行）

| 宏 | 类型 | 实际功能 | 源码位置 |
|----|------|---------|---------|
| `#[derive(Entity)]` | derive | **真实实现 `Model` trait**（table_name/pk_name/pk/set_pk），需 `#[column(primary_key)]` 标注主键 | `derive.rs:315-401` |
| `#[derive(Schema)]` | derive | 生成表结构元数据常量（列名/类型/主键/默认值） | `derive.rs:200-313` |
| `#[derive(Builder)]` | derive | 生成 builder 模式（`new()`/setters/`build()`） | `derive.rs:411+` |
| `query!` | proc_macro | SQL 语法校验 + 注入检测；`db-verify` feature + `SZ_ORM_QUERY_VERIFY=1` 时连真 DB 执行 `EXPLAIN` 验证 | `lib.rs:375-470` |
| `typed_query!` | proc_macro | Diesel 风格类型化 AST：解析 `table name { col: Type }` 声明，生成 `TypedColumn` 结构体 | `lib.rs:852-1207` |
| `schema!` | proc_macro | 解析 `CREATE TABLE` SQL，自动生成 `typed_query!` 等效的类型化列声明 | `lib.rs:1209-1398` |
| `sql_string!` | proc_macro | 编译期 SQL 语法校验（括号平衡、未闭合引号、注入模式检测） | `lib.rs:87-373` |

**结论**：`#[derive(Entity)]` **存在且可用**，能自动生成 `Model` trait 实现。之前"需手写 Model trait"的说法是错误的。

### 1.2 数据库驱动（实际实现验证）

| 驱动 | 代码行数 | 底层依赖 | 状态 |
|------|---------|---------|------|
| MySQL | — | sqlx `mysql` | ✅ 真实 |
| PostgreSQL | — | sqlx `postgres` | ✅ 真实 |
| SQLite | — | sqlx `sqlite` | ✅ 真实 |
| **Oracle** | **1208 行** | `oracle` crate（ODPI-C）+ 专用阻塞线程池 | ✅ **真实实现** |
| **MSSQL** | **1042 行** | `tiberius` crate（纯 Rust TDS 协议） | ✅ **真实实现** |

Oracle 和 MSSQL 均实现了 `Connection` trait，包含连接池管理、参数转换（`?` → `:N` / `@PN`）、行映射等完整逻辑。**不是 stub**。

### 1.3 核心 ORM 能力（`sz-orm-core`）

| 能力 | 源码验证 | 说明 |
|------|---------|------|
| `Model` trait | `model.rs:37` | `table_name/pk_name/pk/set_pk`，可由 `#[derive(Entity)]` 自动生成 |
| `Queryable` trait | `queryable.rs:137` | `from_values()` 从值列表构造实例 |
| `FromRow` trait | `queryable.rs:157` | `from_row(HashMap<String, Value>)` 按列名映射 |
| `Repository` trait | `repository.rs` | `find_by_id/find_all/find_by/save/save_many/delete/count/exists` + `batch_update` |
| `QueryBuilder<M>` | `query.rs` | 链式 SELECT/INSERT/UPDATE/DELETE，参数化 WHERE，JOIN，聚合，软删除，多租户 |
| `N1QueryDetector` | `entity_graph.rs:641` | N+1 检测，窗口式阈值告警 |
| `TenantBehavior` | `behaviors.rs` | INSERT 自动填充 tenant_id，UPDATE 策略（DenyMismatch/Strip/Allow） |
| 连接池 | `pool.rs` | 无锁队列（crossbeam-queue ArrayQueue）+ Notify |
| 事务 | `transaction.rs` | ACID，SAVEPOINT，隔离级别，retry_on_deadlock |
| 迁移 | `migration.rs` + `phinx_migration.rs` | 文件迁移 + Phinx 风格，含 rollback |
| 多 dialect | `dialect.rs` | MySQL（反引号）/ PG·SQLite（双引号）/ SQL Server（方括号） |
| 缓存 | `cache.rs` + `l2_cache.rs` | 多级缓存，TTL，Redis 后端（`redis` feature） |
| 钩子 | `hooks.rs` | before/after insert/update/delete/restore |
| 乐观锁 | `optimistic_lock.rs` | 存在 |
| 重试 | `retry.rs` | 存在 |

### 1.4 扩展包实际状态

| 包 | 代码行数 | 真实实现？ | 说明 |
|----|---------|-----------|------|
| `sz-orm-dtx` | 4040 行（saga 1835 + tcc 2205） | ✅ | 2PC/Saga/TCC 完整实现 |
| `sz-orm-graphql` | 399 行 `real_graphql.rs` | ✅ | async-graphql 7 + DbResolver 注入；无 resolver 时回退 mock（向后兼容，已文档化） |
| `sz-orm-grpc` | 264 行 `real_grpc.rs` | ✅ | tonic 0.14 实现 |
| `sz-orm-queue` | 14 文件，4 个 `real_*.rs` | ✅ | RabbitMQ/Kafka/NATS/Pulsar 真实客户端 |
| `sz-orm-storage` | 12 文件 | 🟡 | S3 真实（`s3-sdk` feature），其余 6 云为 in-memory |
| `sz-orm-vector` | 6 文件（1 real + 2 stub/memory） | 🟡 | pgvector 真实（`real-pg` feature） |
| `sz-orm-postgis` | 8 文件（1 real + 2 stub/memory） | 🟡 | PostGIS 真实（`real-postgis` feature） |
| `sz-orm-search` | 10 文件（0 real + 2 stub/memory） | 🟡 | ES/OpenSearch/Meilisearch 均为 stub+memory |
| `sz-orm-timeseries` | 9 文件（1 real + 2 stub/memory） | 🟡 | TimescaleDB 真实（`real-timescale` feature） |
| `sz-orm-tracing` | 2 文件 | 🟡 | OpenTelemetry OTLP（`otlp` feature） |
| `sz-orm-observability` | 3 文件 | 🟡 | Pushgateway HTTP PUT（`push-gateway` feature） |
| `sz-orm-rw` | 1239 行 | ✅ | 读写分离 |
| `sz-orm-sharding` | 885 行 | ✅ | 分片路由 |
| `sz-orm-lc` | 2140 行 | ✅ | 低代码 CRUD |
| `sz-orm-audit` | 1835 行 | ✅ | SQL 审计日志 |
| `sz-orm-swagger` | 2240 行 | ✅ | OpenAPI 生成 |
| `sz-orm-health` | 1956 行 | ✅ | 健康检查 + SLO |
| `sz-orm-batch` | 1850 行 | ✅ | 批量操作 |
| `sz-orm-limit` | 1342 行 | ✅ | 限流 |
| `sz-orm-config` | 1571 行 | ✅ | 配置管理 |
| `sz-orm-masking` | 528 行 | ✅ | 数据脱敏 |
| `sz-orm-sql-validator` | 1766 行 | ✅ | SQL 防火墙 |

### 1.5 测试数量（实际统计）

| 类型 | 数量 | 来源 |
|------|------|------|
| `#[test]` 单元测试 | 4661 | 全 workspace grep |
| `#[tokio::test]` 异步测试 | 887 | 全 workspace grep |
| **合计** | **5548** | — |

---

## 二、真正缺失的能力（源码验证 ❌）

### 2.1 P0 — 核心缺失

| # | 缺失项 | 源码验证结果 | 影响 |
|---|--------|------------|------|
| **M-1** | **`FromQueryResult` derive 宏** | 搜索 `FromQueryResult`/`derive.*QueryResult` 全 workspace = **0 结果**。`Queryable::from_values()` 需手动实现 | 🔴 查询结果无法自动映射到自定义结构体 |
| **M-2** | **`MockDatabase` 公开 API** | 搜索 `MockDatabase` 全 workspace = **0 结果**。仅有内部 `MockConnection`（`transaction.rs:588`、`pool.rs:1277`、`model.rs:1014`，均为 `pub(crate)` 或私有） | 🔴 无法在不连接 DB 的情况下测试业务查询 |
| **M-3** | **`ActiveModel` / `ActiveValue` 模式** | 搜索 `ActiveValue`/`ActiveModel` = **0 结果**。无 `Set/Unchanged/NotSet` 三态枚举 | 🔴 无法表达"仅更新部分字段"的类型安全语义 |

### 2.2 P1 — 重要缺失

| # | 缺失项 | 源码验证结果 | 影响 |
|---|--------|------------|------|
| **M-4** | **离线编译验证缓存** | `db-verify` feature 需活 DB 执行 `EXPLAIN`。无 `.sqlx` 等效的离线缓存机制 | 🟠 CI 无 DB 时无法编译验证 SQL |
| **M-5** | **类型安全列引用（完整）** | `TypedColumn` trait 存在（`typed.rs`），`typed_query!`/`schema!` 可生成列结构体，但 `QueryBuilder` 主要仍用字符串列名 | 🟠 查询列名无编译期检查 |
| **M-6** | **CLI 实体生成工具** | `cli/` 存在但无 `generate entity` 子命令 | 🟠 无法从现有 DB 反向生成实体 |
| **M-7** | **内置分页 trait** | 无 `PaginatorTrait`。`PageResult<T>` 存在（`repository.rs:169`）但需手动构建 | 🟡 分页需手动实现 |
| **M-8** | **流式查询** | `futures::stream::empty()` 占位（`query.rs` 中） | 🟡 大结果集无法流式处理 |
| **M-9** | **arity-specific Join 类型** | 无 `SelectTwo`/`SelectThree` 等类型区分 | 🟡 Join 结果类型不够精确 |
| **M-10** | **`#[derive(Relation)]`** | 搜索 `derive.*Relation` = **0 结果**。关联关系需手动定义 `foreign_key()` | 🟡 关系定义有样板代码 |

### 2.3 P2 — 部分实现（已注释声明）

| # | 项目 | 状态 |
|---|------|------|
| **M-11** | `accessors.rs` `to_json`/`to_array` | 部分实现，代码中已注释声明局限性 |
| **M-12** | `sz-orm-search` real providers | ES/OpenSearch/Meilisearch 均为 stub+memory，无真实客户端 |

---

## 三、与竞品真实差距

### 3.1 vs SeaORM

| 能力 | SeaORM | sz-orm | 差距 |
|------|--------|--------|------|
| Entity 元数据 derive | ✅ `DeriveEntityModel`（Entity+Column+PrimaryKey 一键生成） | ✅ `#[derive(Entity)]`（实现 `Model` trait） | 🟢 **已覆盖**（实现方式不同，功能等效） |
| FromQueryResult derive | ✅ `DeriveModel` | ❌ **缺失** | 🔴 最大差距 |
| ActiveModel 模式 | ✅ `ActiveValue::Set/Unchanged/NotSet` | ❌ **缺失** | 🔴 最大差距 |
| MockDatabase | ✅ | ❌ **缺失** | 🔴 最大差距 |
| 关系 derive | ✅ `DeriveRelation` | ❌ **缺失** | 🟠 |
| 类型安全列引用 | ✅ 生成的 Column 枚举 | 🟡 `TypedColumn` trait 存在但 QB 主要用字符串 | 🟠 |
| PaginatorTrait | ✅ | ❌ | 🟡 |
| stream 查询 | ✅ | 🟡 占位 | 🟡 |
| CLI 实体生成 | ✅ | ❌ | 🟡 |
| Oracle 支持 | ❌ | ✅ 真实实现 | 🟢 sz-orm 优势 |
| 分布式事务 | ❌ | ✅ | 🟢 sz-orm 优势 |
| 多租户 | ❌ | ✅ | 🟢 sz-orm 优势 |
| 分片 | ❌ | ✅ | 🟢 sz-orm 优势 |

**修正结论**：之前报告称"Entity derive 缺失"是**错误的**。`#[derive(Entity)]` 存在且实现了 `Model` trait。真正的差距是 `FromQueryResult derive`、`MockDatabase` 和 `ActiveModel`。

### 3.2 vs SQLx

| 能力 | SQLx | sz-orm | 差距 |
|------|------|--------|------|
| 编译期 SQL 验证 | ✅ `query!` 连真 DB 验证列名+类型 | 🟡 `query!` 连真 DB 执行 `EXPLAIN`（验证语法可行性，不验证列名/类型） | 🟠 |
| 离线验证 | ✅ `cargo sqlx prepare` + `.sqlx` | ❌ **缺失** | 🔴 |
| `FromRow` derive | ✅ 丰富属性（rename/flatten/skip/json） | ❌ **缺失**（`FromRow` trait 存在但无 derive） | 🔴 |
| `Type` derive | ✅ | ❌ | 🟠 |
| `Any` 驱动 | ✅ 运行时切换 DB | ❌ | 🟠 |
| async-std | ✅ | ❌ | 🟡 |
| MSSQL 成熟度 | ✅ | ✅ 真实实现（tiberius） | 🟢 **已覆盖** |
| Oracle 成熟度 | ✅ | ✅ 真实实现（oracle crate） | 🟢 **已覆盖** |
| ORM 层 | N/A | ✅ Entity/Model/Relation | 🟢 sz-orm 优势 |
| 多租户/分布式事务等 | ❌ | ✅ | 🟢 sz-orm 优势 |

**修正结论**：之前报告称"MSSQL/Oracle 仅声明"是**错误的**。两者均有真实实现。SQLx 的真正优势是离线验证和 `FromRow` derive。

---

## 四、修正后的优先级列表

### 🔴 P0 — 阻塞开发体验（3 项）

| # | 任务 | 预计工期 | 说明 |
|---|------|---------|------|
| **P0-1** | `FromQueryResult` derive 宏 | 1-2 周 | 按列名/列序自动映射查询结果到结构体，支持 `#[from_query(rename)]` 等属性 |
| **P0-2** | `MockDatabase` 公开 API | 1-2 周 | 设置预期查询结果 + 断言实际生成的 SQL，零 DB 依赖测试 |
| **P0-3** | `ActiveValue` + `ActiveModel` 模式 | 1-2 周 | `ActiveValue::Set/Unchanged/NotSet` 三态，支持部分更新 |

### 🟠 P1 — 重要竞争力（4 项）

| # | 任务 | 预计工期 |
|---|------|---------|
| **P1-1** | 离线编译验证（`.sz-orm` 缓存目录） | 1 周 |
| **P1-2** | `#[derive(Relation)]` 宏 | 1 周 |
| **P1-3** | `TypedColumn` 完善 + QueryBuilder 类型安全集成 | 2 周 |
| **P1-4** | CLI `generate entity` 子命令 | 1 周 |

### 🟡 P2 — 中等优先级（4 项）

| # | 任务 | 预计工期 |
|---|------|---------|
| **P2-1** | `PaginatorTrait` 内置分页 | 3 天 |
| **P2-2** | 流式查询 `.stream(db)` 真实实现 | 1 周 |
| **P2-3** | `sz-orm-search` 真实 ES/OpenSearch 客户端 | 1 周 |
| **P2-4** | arity-specific Join 类型 | 3 天 |

---

## 五、深度测试审计建议

### 5.1 六层深度测试体系

| 层级 | 类型 | 当前状态 | 目标 | 工期 |
|------|------|---------|------|------|
| **L1** | 变异测试（cargo-mutants） | ❌ 未实现 | 变异存活率 < 5% | 2 周 |
| **L2** | 属性测试（SQL 注入免疫） | 🟡 部分（proptest 存在） | 全覆盖参数化 API | 1 周 |
| **L3** | 混沌工程 | 🟡 部分（chaos_pool） | 网络延迟/DB 宕机/消息重复 | 2 周 |
| **L4** | 差分测试（跨 DB 结果一致性） | 🟡 部分（sz-orm-sqlx differential_fuzz） | MySQL/PG/SQLite/Oracle 四库一致 | 1 周 |
| **L5** | Fuzz 测试 | ✅ 已有 17 个 target | 新增 migration_sql/dynamic_sql_xml/identifier 3 个 | 1 周 |
| **L6** | Jepsen 分布式测试 | ❌ 未实现 | 2PC/Saga/TCC 正确性 | 2-3 周 |

**总计：6-8 周**

### 5.2 不易发现的 Bug 类型及检测方法

| Bug 类型 | 检测方法 | 风险等级 |
|---------|---------|---------|
| 并发竞态（连接池 acquire） | L3 混沌 + L5 fuzz | 🔴 |
| 事务隔离违反（幻读/不可重复读） | L6 Jepsen | 🔴 |
| 分布式事务悬挂（2PC prepare 后协调者宕机） | L6 Jepsen | 🔴 |
| Saga 补偿不完整（部分步骤成功后的回滚） | L3 混沌 + L6 Jepsen | 🔴 |
| SQL 注入（边缘 Unicode/特殊字符组合） | L2 属性测试 | 🔴 |
| 软删除与关联加载交互（软删除记录被 eager load 包含） | L1 变异测试 | 🟠 |
| 多租户隔离失效（tenant_id 被覆盖或绕过） | L1 变异 + L3 混沌 | 🔴 |
| 连接泄漏（长时间运行后 fd_count 增长） | Soak 测试（已有） | 🟠 |
| 跨数据库类型精度丢失（Decimal/DateTime） | L4 差分测试 | 🟠 |
| 分片路由错误（分片键边界条件） | L1 变异 + L5 fuzz | 🟠 |
| TCC Try 成功后 Confirm 失败的资源悬挂 | L3 混沌 | 🔴 |

---

## 六、功能覆盖度（修正版）

### 6.1 vs SQLx

| 类别 | SQLx | sz-orm | 覆盖度 |
|------|------|--------|--------|
| 数据库驱动 | 6 | 5（MySQL/PG/SQLite/Oracle/MSSQL） | 83% |
| 编译期 SQL 验证 | ✅ 真 DB 验证列名+类型 | 🟡 EXPLAIN 验证语法可行性 | 60% |
| 离线验证 | ✅ | ❌ | 0% |
| `FromRow` derive | ✅ | ❌ | 0% |
| 连接池 | ✅ | ✅（无锁优化） | 110% |
| 事务 | ✅ | ✅ | 100% |
| 迁移 | ✅（仅 up） | ✅（up/down/rollback） | 120% |
| ORM 层 | N/A | ✅ | — |

**修正后覆盖度：~75%**（之前 70%，Oracle/MSSQL 修正为真实实现后提升）

### 6.2 vs SeaORM

| 类别 | SeaORM | sz-orm | 覆盖度 |
|------|--------|--------|--------|
| Entity derive | ✅ DeriveEntityModel | ✅ Entity（功能等效，方式不同） | 100% |
| FromQueryResult derive | ✅ | ❌ | 0% |
| ActiveModel | ✅ | ❌ | 0% |
| MockDatabase | ✅ | ❌ | 0% |
| 关系 derive | ✅ | ❌ | 0% |
| 类型安全查询 | ✅ Column 枚举 | 🟡 TypedColumn 部分 | 40% |
| PaginatorTrait | ✅ | ❌ | 0% |
| 流式查询 | ✅ | 🟡 占位 | 20% |
| CLI 实体生成 | ✅ | ❌ | 0% |
| 数据库驱动 | 6 | 5 | 83% |
| 连接池 | ✅ | ✅（无锁优化） | 110% |
| 事务 | ✅ | ✅ | 100% |
| 迁移 | ✅ | ✅（含 rollback） | 120% |
| 多租户 | ❌ | ✅ | — |
| 分布式事务 | ❌ | ✅ | — |
| 分片 | ❌ | ✅ | — |
| Oracle | ❌ | ✅ | — |

**修正后覆盖度：~65%**（之前 60%，Entity derive 修正为存在后提升）

---

## 七、sz-orm 真实独有优势（源码验证）

以下功能经源码验证**真实存在且已实现**，SeaORM 和 SQLx 均无：

| 功能 | 源码位置 | 验证状态 |
|------|---------|---------|
| Oracle 适配器 | `sz-orm-oracle/src/lib.rs`（1208 行，`oracle` crate） | ✅ 真实 |
| MSSQL 适配器 | `sz-orm-mssql/src/lib.rs`（1042 行，`tiberius` crate） | ✅ 真实 |
| 分布式事务（2PC/Saga/TCC） | `sz-orm-dtx/src/{saga,tcc,cross_shard}.rs`（4040 行） | ✅ 真实 |
| 多租户自动填充+过滤 | `behaviors.rs` TenantBehavior + `hooks.rs` TenantScope | ✅ 真实 |
| N+1 运行时检测 | `entity_graph.rs` N1QueryDetector | ✅ 真实 |
| 分片路由 | `sz-orm-sharding/src/{routing,scatter,enhanced}.rs`（885 行） | ✅ 真实 |
| 读写分离 | `sz-orm-rw/src/lib.rs`（1239 行） | ✅ 真实 |
| 无锁连接池 | `pool.rs`（crossbeam-queue ArrayQueue + Notify） | ✅ 真实 |
| 编译期 SQL 注入检测 | `lib.rs` `sql_string!` 宏（10 种注入模式 + 5 AST 检测） | ✅ 真实 |
| Phinx 风格迁移 | `phinx_migration.rs`（`create_table`/`add_column` 等 DSL） | ✅ 真实 |
| 低代码 CRUD | `sz-orm-lc/src/lib.rs`（2140 行） | ✅ 真实 |
| SQL 审计日志 | `sz-orm-audit/src/lib.rs`（1835 行） | ✅ 真实 |
| 数据脱敏 | `sz-orm-masking/src/lib.rs`（528 行） | ✅ 真实 |
| 健康检查 + SLO | `sz-orm-health/src/lib.rs`（1956 行） | ✅ 真实 |
| 限流 | `sz-orm-limit/src/lib.rs`（1342 行） | ✅ 真实 |
| OpenAPI 生成 | `sz-orm-swagger/src/lib.rs`（2240 行） | ✅ 真实 |
| 批量操作 | `sz-orm-batch/src/lib.rs`（1850 行） | ✅ 真实 |
| SQL 防火墙 | `sz-orm-sql-validator/src/lib.rs`（1766 行） | ✅ 真实 |
| 配置管理 | `sz-orm-config/src/lib.rs`（1571 行） | ✅ 真实 |
| `typed_query!` 类型化 AST | `lib.rs:852-1207` | ✅ 真实 |
| `schema!` SQL→类型声明 | `lib.rs:1209-1398` | ✅ 真实 |
| `query!` 可选真 DB 验证 | `lib.rs:375-470` | ✅ 真实 |

---

## 八、行动路线图（修正版）

### 短期（1-2 个月）— 补齐 P0/P1

| 周次 | 任务 | 验收标准（源码验证） |
|------|------|-------------------|
| W1-2 | `FromQueryResult` derive | `#[derive(FromQueryResult)]` 可映射 `SELECT col1, col2` 到结构体 |
| W3-4 | `MockDatabase` | `MockDatabase::from_query_result()` + SQL 断言 |
| W5-6 | `ActiveValue` + `ActiveModel` | `ActiveValue::Set/Unchanged/NotSet` + 部分更新 SQL |
| W7 | `#[derive(Relation)]` | `#[relation(has_many = "Posts")]` 自动生成 `RelationTrait` 实现 |
| W8 | 离线验证缓存 | `cargo sz-orm prepare` 生成 `.sz-orm` 目录，CI 无 DB 可编译 |

### 中期（3-6 个月）

| 月份 | 任务 | 验收标准 |
|------|------|---------|
| M3 | `TypedColumn` 完善 | `QueryBuilder::select([UserColumn::Name, UserColumn::Email])` 类型安全 |
| M4 | `PaginatorTrait` + stream | `.paginate(db, 10).fetch_page(1)` + `.stream(db)` 真实实现 |
| M5 | CLI `generate entity` | `sz-orm-cli generate entity -u <url> -t users` 生成带 `#[derive(Entity)]` 的结构体 |
| M6 | 深度测试 L1-L4 | 变异存活率 < 5%，差分测试四库一致 |

---

## 九、本次评估与之前报告的差异对照

| 之前报告声称 | 实际源码验证结果 | 差异原因 |
|------------|----------------|---------|
| `#[derive(Entity)]` 缺失，需手写 Model | ✅ **存在**，`derive.rs:315-401` 真实实现 `Model` trait | 基于过时/错误文档，未读源码 |
| Oracle 仅声明未实现 | ✅ **真实实现**，1208 行，`oracle` crate + 专用阻塞池 | 基于文档，未读 `sz-orm-oracle/src/lib.rs` |
| MSSQL 仅声明未实现 | ✅ **真实实现**，1042 行，`tiberius` crate | 基于文档，未读 `sz-orm-mssql/src/lib.rs` |
| 测试数量 5442 | 实际 **5548**（4661 + 887） | 文档数据过时 |
| 覆盖度 vs SeaORM 60% | 修正后 **~65%** | Entity derive 修正后提升 |
| 覆盖度 vs SQLx 70% | 修正后 **~75%** | Oracle/MSSQL 修正后提升 |

**教训**：本次评估首次完全基于源码验证，但之前的评估报告（包括另一个 AI 声称生成的版本）均基于文档而非代码，导致多项结论错误。**后续所有评估必须以 `packages/*/src/*.rs` 实际代码为准。**

---

> **文档版本**：v2.0（基于源码验证）
> **生成日期**：2026-08-03
> **验证方法**：直接读取 43 个 workspace 成员的全部 `.rs` 源文件
> **下次更新**：P0 三项补齐后重新评估
