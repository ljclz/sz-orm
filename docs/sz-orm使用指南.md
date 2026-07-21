# SZ-ORM 使用指南

> 项目名称：SZ-ORM（鲜视达 ORM）
> 文档版本：v5.0（v1.0.0 正式发布：补全 sz-orm-core 全部 21 个高级模块文档 + API 参考手册交叉引用）
> 适用版本：SZ-ORM **v1.0.0**（工作空间 39 个成员：37 个 sz-orm-* lib + cli + examples）
> 更新日期：2026-07-21
> 文档定位：面向使用者的完整上手指南，**所有 trait/结构体/函数签名详见 [API 参考手册](sz-ormAPI参考.md)**；本指南聚焦于"什么场景用什么包/模块、怎么用"，与《项目成熟度评估报告.md》《项目实施进度表.md》配套

> **导读**：本文 §3 按包/模块逐一展开使用示例，所有"详见 [API 参考手册]"链接均指向 `sz-ormAPI参考.md` 对应章节。若只需查阅类型签名与参数说明，直接打开 API 参考手册；若需端到端场景串联（CRUD/事务/连接池/迁移/分布式事务/向量搜索），按本指南章节顺序阅读。

---

## 一、项目概述

SZ-ORM 是一套**生产级、L4 金融级纯 Rust ORM 工作空间**，兼容 ThinkORM 风格的链式 API，由 39 个工作空间成员组成：1 个核心引擎（sz-orm-core）、2 个数据库适配/校验包（sz-orm-sqlx、sz-orm-sql-validator）、1 个编译时宏包（sz-orm-macros）、1 个查询构建器包（sz-orm-query-builder）、1 个可观测性包（sz-orm-observability）、1 个向量数据库包（sz-orm-vector）、3 个生态扩展包（sz-orm-postgis/sz-orm-timeseries/sz-orm-search）、27 个业务扩展生态包、1 个 CLI 工具（cli）、1 个示例集（examples）。

### 1.1 核心特性

| 特性 | 说明 |
|------|------|
| 多数据库方言 | MySQL / PostgreSQL / SQLite 3.35+ / Oracle 23ai，统一 `Dialect` 抽象 |
| 链式查询构建 | `QueryBuilder<M>` 支持 SELECT/INSERT/UPDATE/DELETE/聚合/分页/JOIN |
| 异步连接池 | 自研 `Pool`，可配置大小、超时、空闲回收、健康检查、最大生命周期 |
| ACID 事务 | 隔离级别、保存点（20 层嵌套验证）、`TransactionManager` 多事务管理 |
| 文件迁移系统 | up/down/rollback/reset/refresh，含 `SchemaBuilder` 程序化建表 |
| 编译时 SQL 检查 | `sql_string!` proc macro，编译期捕获语法错误与注入模式 |
| 运行时 SQL 校验 | `QueryBuilder::validate()` + sz-orm-sql-validator，12 种注入模式检测 |
| ActiveRecord 关系映射 | HasMany / HasOne / BelongsTo / BelongsToMany，支持 eager loading |
| 真实 DB 适配器 | sz-orm-sqlx 端到端连接 MySQL/PG/SQLite（sqlx 0.9.0） |
| L4 金融级能力 | 灾备演练、SLA 监控、Chaos 测试、形式化验证 |
| 真实云服务对接 | MQTT(rumqttc) / WebSocket(tokio-tungstenite) / RabbitMQ(lapin) / S3(rust-s3) |
| 安全审计基线 | cargo-audit + cargo-deny CI，RustCrypto 审计栈加密 |
| 钩子系统 | 16 种 HookEvent + HookDispatcher + 软删除 + 多租户 + 全局作用域 |
| 分布式事务 | 2PC + TCC（Try-Confirm-Cancel）+ Saga + 跨分片 ACID 协调器 |
| 高级查询 | JSON 字段查询 + 动态 SQL（XML 模板）+ 强类型 AST + find_with_related |
| AI 向量 + pgvector | sz-orm-vector：pgvector 向量数据库（cosine/euclidean/dot 三种度量）+ NL→SQL（Simple 规则引擎 + OpenAI API） |

### 1.2 质量基线（实测数据）

- 测试总量：**2950 passed, 0 failed**（112 个测试套件，需真实 DB/云服务的标记 ignored）
- 工作空间成员：**39（36 sz-orm-* lib + sz-orm-vector + cli + examples）**
- 代码规模：**85,834 LOC（src/ 18,430 + tests/ 67,404）**
- 七线验证：TDD + 集成 + Jepsen + Fuzz + Stress + Chaos + Formal
- 生产代码 **0 处 panic!**、0 处 `unimplemented!`/`todo!`
- `cargo clippy --workspace --all-targets -- -D warnings` 全通过（0 warnings）
- 批量插入吞吐：SQLite 72 万行/s、PG 26.8 万行/s、MySQL 14.5 万行/s（详见《性能基准.md》）

---

## 二、快速入门

### 2.1 环境要求

| 依赖 | 版本 |
|------|------|
| Rust toolchain | 1.94.0+（sqlx 0.9.0 要求） |
| 异步运行时 | tokio 1.40+ |
| 数据库（可选） | MySQL 8+/9.x、PostgreSQL 14+/18、SQLite 3.35+、Oracle 23ai |

### 2.2 安装

在 `Cargo.toml` 中按路径或版本引入所需包：

```toml
[dependencies]
sz-orm-core = { path = "packages/sz-orm-core" }
sz-orm-sqlx = { path = "packages/sz-orm-sqlx" }   # 需要连接真实数据库时
tokio = { version = "1.40", features = ["full"] }
```

### 2.3 最小示例：生成 SQL

```rust
use sz_orm_core::*;

// 1. 定义模型
#[derive(Clone)]
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
}

fn main() {
    let dialect = get_dialect(DbType::MySQL).unwrap();
    let sql = QueryBuilder::<User>::new(dialect)
        .table("users")
        .select(vec!["id", "name", "email"])
        .where_cond("status = 'active'")
        .order_desc("id")
        .limit(10)
        .build_select();
    // SELECT `id`, `name`, `email` FROM `users`
    // WHERE status = 'active' ORDER BY `id` DESC LIMIT 10
    println!("{sql}");
}
```

### 2.4 连接真实数据库（sz-orm-sqlx）

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

MySQL / PostgreSQL 仅需替换为 `MySqlPoolHandle` / `PgPoolHandle` 及对应 `SqlxMySqlConnectionFactory` / `SqlxPgConnectionFactory`。

### 2.5 编译时 SQL 检查（sql_string!）

```rust
use sz_orm_core::sql_string;

let sql = sql_string!("SELECT * FROM users WHERE id = 1");        // ✅ 编译通过
let sql = sql_string!("SELECT * FROM users WHERE id = ?"; params: 1); // ✅ 参数个数校验
// sql_string!("SELECT * FORM users");   // ❌ 编译错误：缺少 FROM
// sql_string!("SELECT * FROM users WHERE name = 'x' OR '1'='1'"); // ❌ 编译错误：注入模式
```

---

## 三、各包使用说明（39 个工作空间成员）

### 3.1 核心引擎

#### sz-orm-core — 核心引擎

SQL 生成器 + 抽象连接池框架，src 中 0 处依赖 sqlx，保持纯粹抽象层。模块一览：

| 模块 | 关键类型 | 功能 |
|------|---------|------|
| `model` | `Model`、`ModelExt`、`Relation` | 表名/主键/时间戳/软删除/四种关联关系 |
| `query` | `QueryBuilder<M>` | 链式 SQL 构造 + `validate()` 校验 |
| `dialect` | `Dialect`、`get_dialect()` | 四种数据库方言 |
| `pool` | `Pool`、`PoolConfigBuilder`、`Connection` | 异步连接池 |
| `transaction` | `Transaction`、`TransactionManager` | ACID 事务与保存点 |
| `migration` | `Migration`、`Migrator`、`SchemaBuilder` | 文件迁移与程序化建表 |
| `cache` | `Cache`、`MemoryCache`、`MultiLevelCache` | 多级缓存（TTL） |
| `value` | `Value`（20 变体） | 统一值类型 |
| `db_type` | `DbType`（11 种） | 数据库类型枚举 |
| `error` | `DbError`/`PoolError`/`CacheError`/`TxError` | 错误码体系 |
| `hooks` | `HookContext`、`Hookable`、`HookEvent`、`HookDispatcher`、`SoftDelete`、`TenantModel`、`ScopeRegistry` | 钩子系统（16 种事件 + 软删除 + 多租户 + 全局作用域） |
| `json_query` | `JsonQuery`、`JsonUpdate` | JSON 字段查询与更新（三方言映射） |
| `dynamic_sql` | `DynamicSqlParser`、`SqlParams` | XML 模板动态 SQL（rbatis 风格） |
| `typed_ast` | `TypedExpression`、`TypedSelectQuery`、`Eq`/`Lt`/`Gt`/`And`/`Or` 等 | 编译期类型安全的查询构建 |
| `find_with_related` | `FindWithRelated`、`find_with_related_join/subquery/eager_sql` | SeaORM 风格的关联加载 |

#### 3.1.1 hooks 钩子模块详解

钩子系统为 Model 提供 16 种生命周期事件回调，配合软删除与多租户全局作用域，可实现统一的字段填充、审计日志、行级过滤等横切逻辑。

**HookContext**（执行上下文，builder 模式）：

```rust
use sz_orm_core::hooks::HookContext;

let mut ctx = HookContext::new()
    .with_tenant(42)              // 设置租户 ID（多租户场景自动追加 `tenant_id = ?`）
    .with_operator(1)             // 设置操作人 ID（用于审计日志）
    .with_timestamp(1700000000);  // 设置时间戳（Unix 微秒）

ctx.set_meta("source", "api");    // 插入元数据
ctx.set_meta("ip", "127.0.0.1");
assert_eq!(ctx.get_meta("source"), Some(&"api".to_string()));
```

**Hookable trait（16 个钩子方法，按需 override）**：

| 类别 | 方法 | 触发时机 |
|------|------|----------|
| 写入通用 | `before_write` / `after_write` | 任何 insert/update 前后 |
| 保存通用 | `before_save` / `after_save` | 与 write 等价（命名风格不同） |
| 验证 | `before_validate` / `after_validate` | 写入前的业务规则校验前后 |
| 插入 | `before_insert` / `after_insert` | INSERT 前后 |
| 更新 | `before_update` / `after_update` | UPDATE 前后 |
| 删除 | `before_delete` / `after_delete` | DELETE 前后 |
| 恢复 | `before_restore` / `after_restore` | 软删除恢复前后 |
| 查询 | `before_find` / `after_find` | 单行 SELECT 前后 |

所有方法默认 `no-op`，Model 按需 override。返回 `Err(DbError)` 会短路后续操作。

**HookDispatcher 触发顺序**（推荐使用，避免手动逐个调用）：

```rust
use sz_orm_core::hooks::{HookContext, HookDispatcher, Hookable};

struct Order;
impl sz_orm_core::Model for Order { /* ... */ type PrimaryKey = i64; /* ... */ }
impl Hookable for Order {
    fn before_validate(ctx: &mut HookContext) -> Result<(), sz_orm_core::DbError> {
        // 业务规则校验：金额必须 > 0
        Ok(())
    }
    fn after_insert(ctx: &HookContext, _id: &i64) -> Result<(), sz_orm_core::DbError> {
        // 写入审计日志、推送消息
        Ok(())
    }
}

let mut ctx = HookContext::new().with_operator(1);
// INSERT 序列：before_write → before_save → before_validate → after_validate
//           → before_insert → (执行 INSERT) → after_insert → after_save → after_write
let id = HookDispatcher::insert::<Order, _>(&mut ctx, |_ctx| Ok(42_i64))?;

// UPDATE 序列与 INSERT 相同，最后执行 after_update
HookDispatcher::update::<Order, _>(&mut ctx, &42_i64, |_ctx| Ok(()))?;

// DELETE 序列：before_delete → (执行 DELETE) → after_delete
HookDispatcher::delete::<Order, _>(&mut ctx, &42_i64, |_ctx| Ok(()))?;

// RESTORE 序列（软删除恢复）：before_restore → (执行 UPDATE) → after_restore
HookDispatcher::restore::<Order, _>(&mut ctx, &42_i64, |_ctx| Ok(()))?;

// FIND 序列：before_find → (执行 SELECT) → after_find
HookDispatcher::find::<Order, _>(&mut ctx, &42_i64, |_ctx| Ok(()))?;
```

**SoftDelete trait（软删除）**：

```rust
use sz_orm_core::hooks::SoftDelete;

impl SoftDelete for User {
    fn soft_delete_field() -> &'static str { "deleted_at" }
    fn is_deleted(&self) -> bool { /* 判断 self.deleted_at */ unimplemented!() }
}
// 调用 delete 时自动执行 UPDATE SET deleted_at = NOW()，而非 DELETE
```

**TenantModel trait（多租户）**：

```rust
use sz_orm_core::hooks::TenantModel;

impl TenantModel for Order {
    fn tenant_field() -> &'static str { "tenant_id" }  // 默认值
    fn tenant_id(&self) -> i64 { self.tenant_id }
    fn set_tenant_id(&mut self, tid: i64) { self.tenant_id = tid; }
}
// 当 ctx.tenant_id = Some(42) 时，所有查询自动追加 `AND tenant_id = ?` 绑定 42
```

**ScopeRegistry（临时禁用作用域）**：

```rust
use sz_orm_core::hooks::ScopeRegistry;

let registry = ScopeRegistry::new();
// 查询时自动过滤 deleted_at IS NULL AND tenant_id = ?

registry.without_scope("soft_delete", || {
    // 此闭包内的查询会包含已软删除的行（仍受 tenant_id 限制）
});

registry.disable("tenant");   // 跨租户查询（需调用方自行保证安全）
registry.enable("tenant");     // 恢复
```

详细用法见「四、常见场景示例」。

### 3.2 数据库适配与校验

#### sz-orm-sqlx — sqlx 适配器

为 sz-orm-core 的 `Connection`/`ConnectionFactory` 提供真实实现，支持 MySQL/PostgreSQL/SQLite。公开类型：`MySqlPoolHandle`、`PgPoolHandle`、`SqlitePoolHandle`、`SqlxMySqlConnectionFactory`、`SqlxPgConnectionFactory`、`SqlxSqliteConnectionFactory`、`map_sqlx_error()`。

- 类型解码基于 `column.type_info().name()` 分发（规避 MySQL bool 陷阱）
- DECIMAL/NUMERIC 通过 rust_decimal feature 处理
- 错误映射 `sqlx::Error → DbError`（AlreadyExists/ConstraintViolation/InvalidInput 等）

#### sz-orm-sql-validator — SQL 校验器

运行时 SQL 语法/注入校验。公开函数：`validate()`、`validate_select/insert/update/delete()`、`validate_sql()`、`detect_statement_type()`、`validate_parameter_count()`、`validate_table_name()`、`validate_column_name()`。检测 12 种注入模式（`OR '1'='1'`、`UNION SELECT`、`'; DROP TABLE`、`--`、`/*` 等）。

#### sz-orm-macros — 编译时宏

`sql_string!` proc macro，编译期验证 SQL 字面量。零外部依赖，通过 `sz_orm_core::sql_string` 重导出使用。

### 3.3 扩展生态包

| 包 | 功能 | 关键类型 |
|----|------|---------|
| sz-orm-crypto | 加密原语（RustCrypto 审计栈） | `AesGcmCrypter`、`Pbkdf2Hasher`、`HmacSigner`、`sha256()`、`hmac_sha256()` |
| sz-orm-auth | JWT 鉴权（HS256） | `JwtAuthenticator`、`JwtEncoder`、`JwtClaims`、`RbacAuthorizer` |
| sz-orm-scheduler | Cron 定时任务（秒级） | `CronScheduler`、`ScheduledTask`、`CronExpr`、`JobHandler` |
| sz-orm-mqtt | MQTT 客户端 | `MqttPlugin`（内存）/ `RealMqttClient`（rumqttc 0.25，feature `real-broker`） |
| sz-orm-websocket | WebSocket 推送 | `RealtimePusher` / `WsServer`（tokio-tungstenite 0.30，feature `server`） |
| sz-orm-queue | 消息队列统一抽象 | `MessageQueue`、`QueueWrapper`、`LapinRabbitmqQueue`（feature `rabbitmq`） |
| sz-orm-storage | 对象存储（7 提供商） | `Storage`、`StorageBuilder`、`S3SdkStorage`（rust-s3 0.37，feature `s3-sdk`） |
| sz-orm-ai | AI 集成 | `EmbeddingModel`、`VectorStore`、`RagEngine` |
| sz-orm-grpc | gRPC 服务/客户端 | `GrpcServer`、`UserGrpcClient`、`GrpcChannel` |
| sz-orm-graphql | GraphQL 支持 | `GraphQLSchema`、`GraphQLSchemaGenerator`、`GraphQLServer` |
| sz-orm-es | Elasticsearch 集成 | `EsSyncManager`、`EsQuery`、`EsSearchRequest` |
| sz-orm-tracing | 分布式追踪 + SLA | `SzTracer`、`OtelTracer`、`SlaMonitor`、`LatencyHistogram` |
| sz-orm-logger | 结构化日志 + 指标 | `StructuredLogger`、`LoggerFactory`、`Metrics` |
| sz-orm-swagger | OpenAPI 文档生成 | `OpenAPIGenerator`、`OpenAPISpec`、`SwaggerUi` |
| sz-orm-masking | 数据脱敏 | `DataMasker`、`MaskingRule` |
| sz-orm-health | 健康检查 + 熔断 | `DefaultHealthChecker`、`CircuitBreaker`、`FailoverPolicy`、`AlertManager` |
| sz-orm-audit | SQL 审计 | `SqlAuditor`、`SqlAuditContext` |
| sz-orm-batch | 批量操作 | `BatchOperations`、`DefaultBatchOps`、`UpsertMode` |
| sz-orm-vector | pgvector 向量数据库 | `PgVectorStore`、`InMemoryVectorStore`、`RealPgVectorStore`（feature `real-pg`）、`StubVectorStore`、`VectorRecord`、`SearchResult`、`VectorMetric` |
| sz-orm-search | 全文搜索（多 provider） | `SearchExt`、`SearchQuery`、`SearchResult`、`SearchHit`、`SearchBuilder`、`ElasticsearchProvider`（feature `real-es`）、`OpensearchProvider`（feature `real-opensearch`）、`MeilisearchProvider`（feature `real-meilisearch`） |
| sz-orm-timeseries | TimescaleDB 时序扩展 | `TimeseriesExt`、`TimeseriesBuilder`、`Metric`、`Aggregation`、`TimeBucket`、`DownsampleConfig`、`RealTimescale`（feature `real-timescale`） |
| sz-orm-postgis | PostGIS 空间扩展 | `PostgisExt`、`PostgisBuilder`、`Geometry`、`Point`、`LineString`、`Polygon`、`RealPostgis`（feature `real-postgis`） |
| sz-orm-observability | 可观测性闭环 | `MetricsRegistry`、`Counter`、`Gauge`、`Histogram`、`SloMonitor`、`SloConfig` |

#### 3.3.1 sz-orm-search — 全文搜索

多 provider 全文搜索，支持 ES/OpenSearch/Meilisearch + 内存实现。

```rust
use sz_orm_search::{SearchBuilder, SearchExt, SearchProvider, SearchQuery};

// 内存实现（无需外部服务）
let wrapper = SearchBuilder::new(SearchProvider::Memory).build()?;
wrapper.create_index("docs", &serde_json::json!({})).await?;
wrapper.index_doc("docs", "1", &serde_json::json!({"title": "hello"})).await?;

let result = wrapper.search("docs", &SearchQuery::new("hello")).await?;
println!("hits: {}", result.hits.len());
```

**Feature 开关**：

| Feature | Provider | 依赖 |
|---------|----------|------|
| `real-es` | Elasticsearch | elasticsearch crate |
| `real-opensearch` | OpenSearch | opensearch crate |
| `real-meilisearch` | Meilisearch | meilisearch-sdk crate |
| 默认 | Memory + Stub | 无 |

**支持的操作**：create_index / delete_index / index_doc / bulk_index / get_doc / delete_doc / search / count / refresh。详见 [API 参考手册](sz-ormAPI参考.md) §2.14。

#### 3.3.2 sz-orm-timeseries — TimescaleDB 时序扩展

时序数据存储、查询和聚合。

```rust
use sz_orm_timeseries::{TimeseriesBuilder, TimeseriesExt, TimeseriesProvider, Metric, Aggregation};
use chrono::Utc;

let wrapper = TimeseriesBuilder::new(TimeseriesProvider::Memory).build()?;
wrapper.create_hypertable("cpu_usage", "ts").await?;

let now = Utc::now();
wrapper.insert_metric(&Metric::new("cpu_usage", now, 0.75)).await?;

let buckets = wrapper.time_bucket_aggregate(
    "cpu_usage", "1m", Aggregation::Avg,
    now - chrono::Duration::minutes(5), now
).await?;
```

**Feature**：`real-timescale`（tokio-postgres 连接 TimescaleDB）。详见 [API 参考手册](sz-ormAPI参考.md) §2.14。

#### 3.3.3 sz-orm-postgis — PostGIS 空间扩展

PostgreSQL PostGIS 空间几何查询。

```rust
use sz_orm_postgis::{PostgisBuilder, PostgisExt, PostgisProvider, Geometry, Point};

let wrapper = PostgisBuilder::new(PostgisProvider::Memory).build()?;

let beijing = Geometry::Point(Point::new(116.404, 39.915));
let shanghai = Geometry::Point(Point::new(121.474, 31.230));

let distance = wrapper.st_distance(&beijing, &shanghai).await?;
println!("distance: {:.2} m", distance);
```

**几何类型**：Point / LineString / Polygon / MultiPoint / MultiLineString / MultiPolygon，携带 SRID（默认 WGS84=4326）。
**空间操作**：st_distance / st_contains / st_within / st_intersects / st_area / st_length / st_buffer / st_union。
**Feature**：`real-postgis`（tokio-postgres）。详见 [API 参考手册](sz-ormAPI参考.md) §2.14。

#### 3.3.4 sz-orm-observability — 可观测性闭环

MetricsRegistry + Prometheus exporter + SLO 燃烧率监控。

```rust
use sz_orm_observability::{MetricsRegistry, SloMonitor, SloConfig};

let registry = MetricsRegistry::new();

let counter = registry.register_counter("sz_orm_pool_acquires_total", "Total pool acquire calls");
let gauge = registry.register_gauge("sz_orm_pool_active_connections", "Current active connections");
let histogram = registry.register_histogram(
    "sz_orm_query_duration_seconds",
    "Query duration in seconds",
    vec![0.001, 0.01, 0.1, 1.0, 10.0],
);

counter.inc();
gauge.set(5.0);
histogram.observe(0.025);

// 输出 Prometheus 文本格式
let output = registry.render();
```

**Feature 开关**：

| Feature | 能力 |
|---------|------|
| `prometheus` | 在指定端口暴露 `/metrics` HTTP 端点 |
| `otlp` | 通过 OTLP 协议导出 traces 到 Collector |
| 默认 | MetricsRegistry + SLO 监控 |

**SLO 监控**：
```rust
use sz_orm_observability::{SloMonitor, SloConfig};

let config = SloConfig {
    slos: vec![/* SLO 定义 */],
    ..Default::default()
};
let monitor = SloMonitor::new(config);
// 计算 5m / 1h 燃烧率，多窗口告警
```

详见 [API 参考手册](sz-ormAPI参考.md) §2.14。

### 3.4 高级特性包

| 包 | 功能 | 关键类型 |
|----|------|---------|
| sz-orm-dtx | 分布式事务（2PC + TCC + Saga + 跨分片 ACID） | `DtxManager`、`DistributedTransaction`、`TransactionParticipant`、`TccCoordinator`、`Saga`、`CrossShardCoordinator` |
| sz-orm-rw | 读写分离 | `ReadWriteRouter`、`LoadBalanceStrategy` |
| sz-orm-sharding | 分库分表 | `ShardingRouter`、`ShardingStrategy`（路由返回 `Result`，生产代码 0 panic） |
| sz-orm-limit | 限流 | `TokenBucketRateLimiter`、`SlidingWindowRateLimiter` |
| sz-orm-config | 配置中心 | `ConfigCenter`、`ConsulConfigCenter`、`NacosConfigCenter` |
| sz-orm-mig | 数据迁移增强 | `DataMigrator`、`DataTransformer`、`TypeTransformer`、`ColumnMapper` |

#### 3.4.1 sz-orm-dtx 子模块使用示例

sz-orm-dtx 包含 4 个子模块：2PC（`DistributedTransaction`）、TCC（`tcc::TccCoordinator`）、Saga（`saga::Saga`）、跨分片 ACID（`cross_shard::CrossShardCoordinator`）。

**TCC 分布式事务（资金转账场景）**：

Try 阶段冻结金额 → Confirm 阶段实际扣款 → Cancel 阶段解冻。每个分支必须幂等。

```rust
use sz_orm_dtx::tcc::{TccCoordinator, TccParticipant, TccState};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

let mut coord = TccCoordinator::new("tx-transfer-001");

// 分支 1：付款账户（冻结 → 扣款 → 解冻）
let frozen = Arc::new(AtomicU32::new(0));
let confirmed = Arc::new(AtomicU32::new(0));
let cancelled = Arc::new(AtomicU32::new(0));

let f1 = frozen.clone();
let c1 = confirmed.clone();
let ca1 = cancelled.clone();
coord.add_participant(
    TccParticipant::new("account-deduct")
        .with_try(move || { f1.fetch_add(1, Ordering::SeqCst); Ok(()) })       // 冻结 100 元
        .with_confirm(move || { c1.fetch_add(1, Ordering::SeqCst); Ok(()) })   // 实际扣款
        .with_cancel(move || { ca1.fetch_add(1, Ordering::SeqCst); Ok(()) }),  // 解冻
);

// 分支 2：收款账户（同上三阶段）
let f2 = frozen.clone();
let c2 = confirmed.clone();
let ca2 = cancelled.clone();
coord.add_participant(
    TccParticipant::new("account-credit")
        .with_try(move || { f2.fetch_add(1, Ordering::SeqCst); Ok(()) })
        .with_confirm(move || { c2.fetch_add(1, Ordering::SeqCst); Ok(()) })
        .with_cancel(move || { ca2.fetch_add(1, Ordering::SeqCst); Ok(()) }),
);

// 全部 try 成功 → 自动 confirm；任一 try 失败 → 自动 cancel 已 try 成功的分支
coord.execute()?;
assert_eq!(coord.state(), TccState::Confirmed);

// 异常恢复：confirm/cancel 失败后可重试（必须幂等）
// coord.retry_confirm()?;
// coord.retry_cancel()?;
```

**Saga 长流程事务（订单创建 + 扣库存 + 发货）**：

将长事务拆分为多个本地事务步骤，每个步骤提供补偿操作。任一步骤失败时按反向顺序执行已成功步骤的补偿。

```rust
use sz_orm_dtx::saga::{Saga, SagaStep, SagaState};

let mut saga = Saga::new("order-create-001");

// 步骤 1：创建订单
saga.add_step(
    SagaStep::new("create_order")
        .with_action(|| Ok(()))           // INSERT INTO orders ...
        .with_compensation(|| Ok(())),    // DELETE FROM orders WHERE id = ?
)?;

// 步骤 2：扣减库存
saga.add_step(
    SagaStep::new("deduct_inventory")
        .with_action(|| Ok(()))           // UPDATE stock SET qty = qty - 1
        .with_compensation(|| Ok(())),    // UPDATE stock SET qty = qty + 1
)?;

// 步骤 3：发货
saga.add_step(
    SagaStep::new("ship_order")
        .with_action(|| Ok(()))           // INSERT INTO shipments ...
        .with_compensation(|| Ok(())),    // UPDATE shipments SET status = 'cancelled'
)?;

// 顺序执行所有 action；任一失败 → 反向执行已成功步骤的 compensation
saga.execute()?;
assert_eq!(saga.state(), SagaState::Completed);

// 用 SagaManager 全局管理多个 Saga 实例
use sz_orm_dtx::saga::SagaManager;
let manager = SagaManager::new();
manager.register("order-create-001", saga)?;
```

**跨分片 ACID 事务（跨分片订单创建）**：

基于 2PC 协调器，按 `shard_id` 自动分组操作并合并为分支事务。适合分片集群中一笔业务需同时写入多个分片的场景。

```rust
use sz_orm_dtx::cross_shard::CrossShardCoordinator;
use std::sync::{Arc, atomic::{AtomicU32, Ordering}};

let prepared = Arc::new(AtomicU32::new(0));
let committed = Arc::new(AtomicU32::new(0));

let mut coord = CrossShardCoordinator::new("tx-order-001");

// 分片 1：订单分片
let p1 = prepared.clone();
let c1 = committed.clone();
coord.add_operation(
    "shard-orders",
    move || { p1.fetch_add(1, Ordering::SeqCst); Ok(()) },  // prepare：写 undo log
    move || { c1.fetch_add(1, Ordering::SeqCst); Ok(()) },  // commit：实际写入
    || Ok(()),                                                // rollback：清理 undo log
)?;

// 分片 2：库存分片
let p2 = prepared.clone();
let c2 = committed.clone();
coord.add_operation(
    "shard-inventory",
    move || { p2.fetch_add(1, Ordering::SeqCst); Ok(()) },
    move || { c2.fetch_add(1, Ordering::SeqCst); Ok(()) },
    || Ok(()),
)?;

// 完整 2PC：prepare 全部分片 → 全部成功后 commit；任一失败则回滚已 prepare 的分片
coord.execute()?;
assert_eq!(prepared.load(Ordering::SeqCst), 2);
assert_eq!(committed.load(Ordering::SeqCst), 2);
```

### 3.5 平台支持包

| 包 | 功能 | 关键类型 |
|----|------|---------|
| sz-orm-back | 备份/恢复 + 灾备演练 | `BackupManager`、`RestoreManager`、`DisasterRecoveryDrill`、`DegradationPolicy` |
| sz-orm-wasm | WebAssembly 目标 | `WasmDatabase`、`WasmQuery` |
| sz-orm-lc | 低代码引擎 | `LowCodeEngine`、`ModelDefinition`、`FieldDef` |

#### 3.5.1 CLI 工具（cli 包）

sz-orm 提供 CLI 工具，支持迁移管理、代码生成等。

```bash
# 安装
cargo install --path packages/cli

# 查看帮助
sz-orm-cli --help

# 生成迁移
sz-orm-cli migrate generate create_users

# 执行迁移
sz-orm-cli migrate up

# 回滚迁移
sz-orm-cli migrate down

# 从数据库生成模型代码
sz-orm-cli generate model --table users --output src/models/
```

#### 3.5.2 示例集（examples 包）

examples 包含完整可运行示例。

```bash
# 运行 Hello World 示例
cargo run --package sz-orm-examples --bin hello_world

# 运行 CRUD 示例
cargo run --package sz-orm-examples --bin crud

# 运行事务示例
cargo run --package sz-orm-examples --bin transaction

# 运行连接池示例
cargo run --package sz-orm-examples --bin connection_pool
```

示例目录结构：`packages/examples/src/bin/`，每个 `.rs` 文件是一个独立示例。

### 3.6 高级查询与表达式

#### 3.6.1 JSON 字段查询（json_query 模块）

支持 MySQL `->` / PostgreSQL `->>` / SQLite `json_extract()` 三方言映射。

```rust
use sz_orm_core::json_query::{JsonQuery, JsonUpdate};
use sz_orm_core::DbType;

// 查询：MySQL `prefs`->'$.theme' = 'dark'
let cond = JsonQuery::new(DbType::MySQL, "prefs")
    .path("theme")
    .eq_string("dark");

// 嵌套路径查询：MySQL `meta`->'$.user.level' > 5
let cond = JsonQuery::new(DbType::MySQL, "meta")
    .path("user.level")
    .gt_i64(5);

// PostgreSQL 方言自动切换为 "meta"->>'user'->>'level'
let cond_pg = JsonQuery::new(DbType::PostgreSQL, "meta")
    .path("user.level")
    .gt_i64(5);

// SQLite 方言自动切换为 json_extract(`meta`, '$.user.level')
let cond_sqlite = JsonQuery::new(DbType::Sqlite, "meta")
    .path("user.level")
    .gt_i64(5);

// 更新 JSON 字段（SET prefs = JSON_SET(prefs, '$.theme', 'light')）
let update = JsonUpdate::new(DbType::MySQL, "prefs")
    .set("theme", "light")
    .set("lang", "zh-CN")
    .build();
```

#### 3.6.2 动态 SQL（dynamic_sql 模块）

rbatis 风格的 XML 模板 + 命名参数绑定，支持 `<if>`、`<where>`、`<set>`、`<foreach>`、`<choose>`、`<trim>` 等标签。

```rust
use sz_orm_core::dynamic_sql::{DynamicSqlParser, SqlParams};

let xml = r#"
<select id="find_users">
    SELECT * FROM users
    <where>
        <if test="name != null">AND name = #{name}</if>
        <if test="age != null">AND age &gt; #{age}</if>
        <if test="status != null">AND status = #{status}</if>
    </where>
    ORDER BY id DESC
</select>
"#;

let parser = DynamicSqlParser::from_xml(xml);
let mut params = SqlParams::new();
params.set("name", "Alice");
params.set("age", 18);
// params.set("status", "active");  // 不设置则对应 <if> 不生效

let sql = parser.build("find_users", &params)?;
// SELECT * FROM users WHERE name = ? AND age > ?  ORDER BY id DESC
// 参数按出现顺序绑定：["Alice", 18]

// <foreach> 展开 IN 子句
let xml_foreach = r#"
<select id="find_by_ids">
    SELECT * FROM users
    WHERE id IN
    <foreach collection="ids" item="id" separator="," open="(" close=")">
        #{id}
    </foreach>
</select>
"#;
let parser = DynamicSqlParser::from_xml(xml_foreach);
let mut params = SqlParams::new();
params.set_list("ids", vec![1, 2, 3]);
let sql = parser.build("find_by_ids", &params)?;
// SELECT * FROM users WHERE id IN (?, ?, ?)
```

#### 3.6.3 强类型 AST（typed_ast 模块）

借鉴 Diesel 思路，提供编译期类型安全的查询构建。列类型不匹配、跨表列引用等错误在编译期被捕获。

```rust
use sz_orm_core::typed::{TypedTable, TypedColumn};
use sz_orm_core::typed_ast::*;

// 1. 声明表 schema（通常由 typed_query! 宏生成）
struct users;
impl TypedTable for users { const NAME: &'static str = "users"; }

mod users {
    use super::*;
    pub struct id;
    impl TypedColumn for id {
        const NAME: &'static str = "id";
        type Table = super::users;
        type RustType = i64;
    }
    pub struct name;
    impl TypedColumn for name {
        const NAME: &'static str = "name";
        type Table = super::users;
        type RustType = String;
    }
    pub struct age;
    impl TypedColumn for age {
        const NAME: &'static str = "age";
        type Table = super::users;
        type RustType = i64;
    }
}

// 2. 类型安全查询
let q = TypedSelectQuery::<users>::new()
    .filter(users::id.eq(42))          // ✅ i64 列与 i64 值比较
    .filter(users::name.eq("Alice"))   // ✅ String 列与 &str 值比较
    .filter(users::age.gt(18).and(users::age.lt(60)));

// 3. 编译期拒绝的错误
// q.filter(users::id.eq("Alice"));   // ❌ i64 列与 &str 值类型不匹配
// q.filter(users::name.eq(42));      // ❌ String 列与 i64 值类型不匹配
```

支持的 11 种表达式类型：`ColumnExpr`、`Literal`、`Eq`、`Ne`、`Lt`、`Gt`、`Le`、`Ge`、`And`、`Or`、`TypedSelectQuery`。

#### 3.6.4 find_with_related（关联加载）

对应 SeaORM 的 `find_with_related` API，提供三种关联查询模式：

```rust
use sz_orm_core::find_with_related::find_with_related_join;
use sz_orm_core::{get_dialect, DbType};

let dialect = get_dialect(DbType::MySQL).unwrap();

// 模式 1：JOIN（适合 1:1 / N:1 关联，如 BelongsTo / HasOne）
let sql = find_with_related_join(
    &*dialect,           // 方言（Box<dyn Dialect> 解引用为 &dyn Dialect）
    "users",             // 主表
    "profiles",          // 关联表
    "user_id",           // 外键（在 profiles 中）
    "id",                // 主表主键
    true,                // LEFT JOIN
)
.where_cond("users.id = 1")
.build();
// SELECT * FROM users LEFT JOIN profiles ON users.id = profiles.user_id WHERE users.id = 1

// 模式 2：Subquery（适合 1:N 关联，避免主表行膨胀）
use sz_orm_core::find_with_related::find_with_related_subquery;
let sql = find_with_related_subquery(
    &*dialect, "users", "orders", "user_id", "id",
).where_cond("users.id = 1").build();

// 模式 3：Eager Load（生成两条 SQL：先主表，后关联表 WHERE IN）
use sz_orm_core::find_with_related::find_with_related_eager_sql;
let (main_sql, related_sql) = find_with_related_eager_sql(
    &*dialect, "users", "orders", "user_id", "id",
);
// main_sql:    SELECT * FROM users WHERE ...
// related_sql: SELECT * FROM orders WHERE user_id IN (?, ?, ?)
```

#### 3.6.5 pgvector 向量数据库（sz-orm-vector 包）

pgvector 是 PostgreSQL 的向量扩展，提供近似最近邻（ANN）搜索。sz-orm-vector 提供 `PgVectorStore` trait + `InMemoryVectorStore`（内存）+ `RealPgVectorStore`（真实 PG，feature `real-pg`）。

```rust
use sz_orm_vector::{PgVectorStore, InMemoryVectorStore, VectorRecord, VectorMetric};

let store = InMemoryVectorStore::new();
store.create_collection("products", 128, None).await?;

store.insert("products", vec![
    VectorRecord::new("p1", vec![0.1; 128]),
    VectorRecord::new("p2", vec![0.9; 128]),
]).await?;

// 余弦相似度搜索
let results = store.search("products", &vec![0.1; 128], 5).await?;
// 返回按相似度排序的 SearchResult { id, score, vector, text, metadata }
```

RealPgVectorStore 使用 tokio-postgres + pgvector 扩展：
- 表结构：`collections`（name TEXT PRIMARY KEY, dimension INT, metric TEXT）+ `vectors_{name}`（id TEXT PRIMARY KEY, embedding vector(dim), metadata JSONB, text TEXT）
- 所有 SQL 使用参数化查询（$1, $2），禁止字符串拼接
- 集合名严格校验（仅允许 ASCII 字母数字+下划线，最大 63 字符，防止 SQL 注入）
- 支持 UPSERT（ON CONFLICT DO UPDATE）

#### 3.6.6 NL→SQL 自然语言转 SQL（sz-orm-ai 包）

NL→SQL 将自然语言查询转换为参数化 SQL，防止 SQL 注入。

**SimpleNl2SqlEngine**（内存规则引擎，面向英文关键词查询）：

```rust
use sz_orm_ai::nl2sql::{SimpleNl2SqlEngine, Nl2SqlEngine, SchemaContext, TableInfo, ColumnInfo};

let engine = SimpleNl2SqlEngine::new();
let schema = SchemaContext {
    tables: vec![TableInfo {
        name: "users".into(),
        columns: vec![
            ColumnInfo { name: "id".into(), data_type: "INTEGER".into(), nullable: false, is_primary_key: true },
            ColumnInfo { name: "age".into(), data_type: "INTEGER".into(), nullable: true, is_primary_key: false },
        ],
    }],
};
let result = engine.generate("find users where age > 25", &schema).await?;
// → SqlQuery { sql: "SELECT * FROM users WHERE age > $1", .. }
// 所有值使用 $1, $2 参数化占位符，禁止字符串拼接
```

**OpenAINl2SqlEngine**（真实 LLM 实现，feature `real`）：
- 调用 OpenAI 兼容 API（GPT-4o-mini 默认）
- system prompt 包含完整 schema 上下文
- 自动清理 markdown 代码块标记
- **双重安全验证**：只允许 SELECT + 注入检测（禁止 UNION/布尔注入/注释逃逸）
- 禁止 DROP/ALTER/TRUNCATE/INSERT/UPDATE/DELETE

**安全验证（safety 模块，返回 bool）**：

```rust
use sz_orm_ai::safety::{validate_select_only, validate_no_injection};

// 只允许 SELECT
assert!(validate_select_only("SELECT * FROM users"));       // ✅ 只读查询
assert!(!validate_select_only("DROP TABLE users"));         // ❌ 写入操作被拒绝

// SQL 注入检测
assert!(!validate_no_injection("SELECT * FROM users WHERE name = 'x' OR '1'='1'")); // ❌ 布尔注入
```

详细用法见「四、常见场景示例」。

#### 3.6.7 sz-orm-ai 向量与 RAG（sz-orm-ai 包）

sz-orm-ai 提供完整的 AI 基础设施：Embedding 模型 + VectorStore + RAG 引擎。NL→SQL 见 [3.6.6](#366-nlsql-自然语言转-sqlsz-orm-ai-包)。

**Embedding 模型**：
```rust
use sz_orm_ai::{EmbeddingModel, SimpleEmbeddingModel};

let model = SimpleEmbeddingModel::new();
let embedding = model.embed("Hello, world!").await?;
// Vec<f32>，维度 64
```

**OpenAI Embedding**（feature `real`）：
```rust
use sz_orm_ai::OpenAIEmbeddingClient;

let client = OpenAIEmbeddingClient::new("sk-xxx", "text-embedding-3-small")?;
let embedding = client.embed("Hello, world!").await?;
// Vec<f32>，维度 1536
```

**VectorStore**（内存）：
```rust
use sz_orm_ai::{VectorStore, InMemoryVectorStore, VectorRecord, VectorMetric};

let store = InMemoryVectorStore::new();
store.create_collection("docs", 1536, None).await?;
store.insert("docs", vec![
    VectorRecord::new("d1", vec![0.1; 1536]),
    VectorRecord::new("d2", vec![0.9; 1536]),
]).await?;

let results = store.search("docs", &vec![0.1; 1536], 5, None).await?;
// 返回按相似度排序的 SearchResult
```

**RAG 引擎**：
```rust
use sz_orm_ai::{RagEngine, SimpleEmbeddingModel, InMemoryVectorStore};

let engine = RagEngine::new(
    Box::new(SimpleEmbeddingModel::new()),
    Box::new(InMemoryVectorStore::new()),
);

// 索引文档
engine.index_document("d1", "SZ-ORM 是生产级 Rust ORM").await?;

// 语义搜索
let results = engine.query("什么是 SZ-ORM", 3).await?;
for r in results {
    println!("score={:.3} text={}", r.score, r.text);
}
```

#### 3.6.8 独立查询构建器（sz-orm-query-builder 包）

sz-orm-query-builder 是不绑定 Model 的纯 SQL 构造器，可独立编译、独立发布到 crates.io。设计灵感来自 sea-query。

**与 sz-orm-core::QueryBuilder 的区别**：

| 特性 | `sz-orm-core::QueryBuilder<M>` | `sz-orm-query-builder::Query` |
|------|------------------------------|----------------------------|
| 绑定 Model | 是（`<M: Model>`） | 否 |
| 类型安全 | 编译期表/列校验 | 运行时字符串 |
| 适用场景 | ORM 完整流程 | 纯 SQL 构造、动态查询 |
| 依赖 | sz-orm-core 全部 | 仅 dialect 模块 |
| 独立发布 | 否 | 是 |

**快速入门**：
```rust
use sz_orm_core::DbType;
use sz_orm_query_builder::Query;

// SELECT
let sql = Query::select()
    .column("id")
    .column("name")
    .from("users")
    .where_clause("age > 18")
    .order_by("id", true)
    .limit(10)
    .build(DbType::MySQL);
// → SELECT `id`, `name` FROM `users` WHERE age > 18 ORDER BY `id` ASC LIMIT 10

// INSERT
let sql = Query::insert()
    .into_table("users")
    .value("name", "'Alice'")
    .value("age", "30")
    .build();
// → INSERT INTO `users` (`name`, `age`) VALUES ('Alice', 30)

// UPDATE
let sql = Query::update()
    .table("users")
    .set("name", "'Bob'")
    .where_clause("id = 1")
    .build();
// → UPDATE `users` SET `name` = 'Bob' WHERE id = 1

// DELETE
let sql = Query::delete()
    .from_table("users")
    .where_clause("id = 1")
    .build();
// → DELETE FROM `users` WHERE id = 1
```

**安全性**（门禁 9 修复）：
- 表名、列名通过 `quote_ident()` 用反引号包裹，内部反引号加倍（MySQL 标准）
- WHERE 条件通过 `check_where_injection()` 检测高危模式（`; DROP`、`--`、`/* */`）
- JOIN 表名支持别名（`orders o` → `` `orders` o ``）
- GROUP BY / ORDER BY 列名也转义

**支持的 SQL 语句**：SELECT（含 JOIN/GROUP BY/HAVING/ORDER BY/LIMIT）、INSERT、UPDATE、DELETE。详见 [API 参考手册](sz-ormAPI参考.md) §2.14。

### 3.7 sz-orm-core 高级特性模块（21 个）

sz-orm-core 除 §3.1 列出的基础模块外，还提供 21 个高级特性模块，覆盖访问器、行为、权限、脏字段、动态过滤、实体图、SQL 守卫、Hydration、JOIN DSL、二级缓存、Lambda 查询、观察者、乐观锁、Phinx 迁移、Queryable、快速查询、仓储、ResultMap、Schema 生成、SQL 安全、TypeHandler。以下逐个介绍使用方式；类型签名详见 [API 参考手册](sz-ormAPI参考.md)。

#### 3.7.1 accessors — 字段访问器/修改器 + 类型转换

字段读取/写入的统一拦截点，支持自定义访问器、修改器和类型转换。对应 PHP ThinkORM 的 `getAttr/setAttr/getData/__isset`。

```rust
use sz_orm_core::accessors::{AccessorRegistry, CastType};
use sz_orm_core::Value;

let mut reg = AccessorRegistry::new();
// 注册类型转换：DB 字段 status 是字符串 "1"，读取时自动转 I64
reg.register_cast("status", CastType::Integer);

let v = reg.cast_read("status", Value::String("1".into()));
assert_eq!(v, Value::I64(1));

// 也可注册闭包风格的 accessor / mutator
use sz_orm_core::accessors::{ClosureAccessor, ClosureMutator};
reg.register_accessor(Box::new(ClosureAccessor::new("email", |v| {
    // 读取时统一小写
    if let Value::String(s) = v { Value::String(s.to_lowercase()) } else { v }
})));
reg.register_mutator(Box::new(ClosureMutator::new("email", |v| {
    // 写入时去空格
    if let Value::String(s) = v { Value::String(s.trim().to_string()) } else { v }
})));
```

#### 3.7.2 behaviors — 可插拔行为系统

Model 行为插件，类似 Yii Framework 的 Behavior。内置 `TimestampBehavior`（自动时间戳）和 `BlameableBehavior`（自动操作人）。

```rust
use sz_orm_core::behaviors::{BehaviorRegistry, TimestampBehavior, BlameableBehavior};

let mut reg = BehaviorRegistry::new();
// 自动填充 created_at / updated_at
reg.register(Box::new(TimestampBehavior::default_fields()));
// 自动填充 created_by / updated_by（需配合 PermissionContext）
reg.register(Box::new(BlameableBehavior::default_fields()));

// 在 INSERT 前自动设置时间字段
reg.before_insert(&mut entity);
// 在 UPDATE 前自动更新时间字段
reg.before_update(&mut entity);
// 在 FIND 后自动 hydration
reg.after_find(&mut entity);
```

#### 3.7.3 data_permission — 数据权限拦截器

行级数据权限控制，支持租户隔离、所有者隔离、部门范围、自定义条件。可拦截 SELECT/UPDATE/DELETE。

```rust
use sz_orm_core::data_permission::{
    DataPermissionInterceptor, PermissionContext, TenantIsolation, OwnerOnly, DepartmentScope,
};

let mut interceptor = DataPermissionInterceptor::new();
interceptor.register(Box::new(TenantIsolation::default_field()));   // tenant_id = ?
interceptor.register(Box::new(OwnerOnly::default_field()));          // user_id = ?
interceptor.register(Box::new(DepartmentScope::default_field()));    // dept_id IN (...)

let ctx = PermissionContext::new()
    .with_user_id(42)
    .with_tenant_id(1)
    .with_dept_id(10);

// 自动追加 WHERE 子句
let sql = interceptor.apply_to_select("SELECT * FROM orders", &ctx)?;
// → SELECT * FROM orders WHERE tenant_id = 1 AND user_id = 42 AND dept_id IN (10, ...)
```

#### 3.7.4 dirty_attributes — 脏字段追踪

追踪实体字段变更，仅 UPDATE 变更字段，避免整行写入。对应 PHP ThinkORM 的 `getDirty`。

```rust
use sz_orm_core::dirty_attributes::{DirtyTracker, build_dynamic_update};
use sz_orm_core::{Value, DbType, get_dialect};

let initial = std::collections::HashMap::from([
    ("name".to_string(), Value::String("Alice".into())),
    ("age".to_string(), Value::I64(25)),
]);
let mut tracker = DirtyTracker::new(initial);
tracker.set("name", Value::String("Bob".into()));

assert!(tracker.is_field_dirty("name"));
assert!(!tracker.is_field_dirty("age"));

// 仅 UPDATE 脏字段
let dialect = get_dialect(DbType::PostgreSQL).unwrap();
let sql = build_dynamic_update(&**dialect, "users", "id", Value::I64(1), &tracker).unwrap();
// → UPDATE "users" SET "name" = 'Bob' WHERE "id" = 1
```

#### 3.7.5 dynamic_filter — 运行时动态 Filter

运行时注册/启用/禁用全局 Filter（类似 MyBatis-Plus 的 TableLogic 全局过滤）。

```rust
use sz_orm_core::dynamic_filter::{FilterDef, FilterParam, FilterRegistry};
use std::collections::HashMap;

let mut reg = FilterRegistry::new();
reg.register(
    FilterDef::new("active")
        .with_condition("status = :status")
        .with_param(FilterParam::new("status"))
);
reg.register(
    FilterDef::new("soft_delete")
        .with_condition("deleted_at IS NULL")
);

// 启用 Filter 并传参
let mut params = HashMap::new();
params.insert("status".to_string(), Value::String("active".into()));
reg.enable("active", params)?;
reg.enable("soft_delete", HashMap::new())?;

// 自动追加到 SELECT
let sql = reg.apply("SELECT * FROM users")?;
// → SELECT * FROM users WHERE status = 'active' AND deleted_at IS NULL

// 临时禁用某个 Filter
reg.disable("active")?;
```

#### 3.7.6 entity_graph — 实体图与批量加载（解决 N+1）

定义实体关联图，通过 BatchLoader 批量加载关联数据，避免 N+1 查询。

```rust
use sz_orm_core::entity_graph::{EntityGraph, BatchLoaderFn};

// 闭包风格的批量加载器：根据 user_id 列表一次性加载部门信息
let dept_loader = BatchLoaderFn::new(|keys: &[i64]| -> std::collections::HashMap<i64, String> {
    // SELECT dept_id, dept_name FROM depts WHERE dept_id IN (?, ?, ?)
    keys.iter().map(|&k| (k, format!("dept_{}", k))).collect()
});

let graph = EntityGraph::new()
    .edge("user", "dept", dept_loader);

// 一次性加载 100 个用户的部门，避免 100 次 SQL
let user_ids: Vec<i64> = vec![1, 2, 3, 4, 5];
let depts = graph.load_batch("user", "dept", &user_ids);
```

#### 3.7.7 guard — SQL 安全守卫

拦截危险 SQL（无 WHERE 的全表 UPDATE/DELETE），避免生产事故。

```rust
use sz_orm_core::guard::{SafeSqlGuard, GuardPolicy};

let guard = SafeSqlGuard::new(GuardPolicy::Strict);

// ✅ 安全：有 WHERE
guard.check("DELETE FROM users WHERE id = 1")?;
guard.check("UPDATE users SET name = 'Bob' WHERE id = 1")?;

// ❌ 危险：无 WHERE 的全表操作被拦截
guard.check("DELETE FROM users").unwrap_err();
guard.check("UPDATE users SET name = 'Bob'").unwrap_err();

// Permissive 策略下仅记录警告不阻断
let permissive = SafeSqlGuard::new(GuardPolicy::Permissive);
```

#### 3.7.8 hydration_plugin — Hydration 模式 + Plugin 拦截链

MyBatis 风格的插件链，可在 SQL 执行前后插入日志、慢查询、审计、重写、阻断等逻辑。

```rust
use sz_orm_core::hydration_plugin::{PluginChain, SqlLogPlugin, SlowQueryPlugin, AuditPlugin};
use std::time::Duration;

let mut chain = PluginChain::new();
chain.add(Box::new(SqlLogPlugin::default()));                       // SQL 日志
chain.add(Box::new(SlowQueryPlugin::new(Duration::from_millis(500)))); // 慢查询检测
chain.add(Box::new(AuditPlugin::default()));                         // 审计日志

// 查询前拦截
let mut sql = "SELECT * FROM users".to_string();
let mut params = vec![];
chain.before_query(&mut sql, &mut params)?;
// 查询后拦截
chain.after_query(&sql, &params, &rows)?;
```

#### 3.7.9 join_dsl — 类型安全 JOIN 语法

链式构造 JOIN 表达式，支持 INNER/LEFT/RIGHT/FULL/CROSS 五种 JOIN。

```rust
use sz_orm_core::join_dsl::{JoinBuilder, JoinKind};

let join = JoinBuilder::new(JoinKind::Left, "orders")
    .on("users.id", "=", "orders.user_id")
    .on("users.tenant_id", "=", "orders.tenant_id")  // 多个 ON 条件
    .build();

let sql = format!("SELECT * FROM users {}", join.to_sql());
// → SELECT * FROM users LEFT JOIN orders ON users.id = orders.user_id AND users.tenant_id = orders.tenant_id
```

#### 3.7.10 l2_cache — 二级缓存

跨 Session 共享的二级缓存，LRU 淘汰 + TTL 过期 + 表级失效。

```rust
use sz_orm_core::l2_cache::{L2Cache, CacheKey};
use sz_orm_core::Value;
use std::time::Duration;

let cache = L2Cache::new(1000, Duration::from_secs(300)); // 最多 1000 条目，TTL 5 分钟

let key = CacheKey::by_pk("users", Value::I64(42));
cache.put(&key, Value::String("Alice".into()));

if let Some(v) = cache.get(&key) {
    println!("cache hit: {:?}", v);
}

// 写入 users 表后失效该表所有缓存
cache.invalidate_table("users");
```

#### 3.7.11 lambda — Lambda 类型安全查询构造器

MyBatis-Plus 风格 Lambda 查询包装器，通过 `User::age` 形式引用列名，编译期检查列名错误。

```rust
use sz_orm_core::lambda::{LambdaWrapper, define_columns};

define_columns! { User { id, name, age } }

let wrapper = LambdaWrapper::<User>::new()
    .eq(User::age, 18)
    .like(User::name, "Ali%")
    .order_desc(User::id)
    .page(1, 20);

let where_sql = wrapper.build_where();
// → WHERE age = ? AND name LIKE ? ORDER BY id DESC LIMIT 20 OFFSET 0
```

#### 3.7.12 observer — 模型生命周期观察者

观察者模式，监听 Model 9 种生命周期事件（BeforeInsert / AfterInsert / BeforeUpdate / AfterUpdate / BeforeDelete / AfterDelete / AfterFind / BeforeSave / AfterSave）。

```rust
use sz_orm_core::observer::{EventDispatcher, Event, AuditLogSubscriber, Observer};

struct CacheInvalidationObserver;
impl Observer for CacheInvalidationObserver {
    fn name(&self) -> &'static str { "cache_invalidation" }
    fn handle(&self, event: &Event, entity: &dyn std::any::Any) -> Result<(), Box<dyn std::error::Error>> {
        match event {
            Event::AfterUpdate | Event::AfterDelete => {
                // 清除该实体的缓存
            }
            _ => {}
        }
        Ok(())
    }
}

let mut dispatcher = EventDispatcher::new();
dispatcher.subscribe(Box::new(AuditLogSubscriber::new()));
dispatcher.subscribe(Box::new(CacheInvalidationObserver));

// Model 操作时自动分发事件
dispatcher.dispatch(&Event::AfterInsert, &user)?;
```

#### 3.7.13 optimistic_lock — 乐观锁

基于版本号的乐观锁，自动冲突重试。

```rust
use sz_orm_core::optimistic_lock::{OptimisticLock, retry};

impl OptimisticLock for User {
    fn version_field() -> &'static str { "version" }
    fn current_version(&self) -> i64 { self.version }
    fn bump_version(&mut self) { self.version += 1; }
}

// 自动重试 3 次：find → modify → save（含 version 检查）
let result = retry(|| {
    let mut user = repo.find_by_id(42)?;
    user.name = "new_name".into();
    repo.save(user)  // UPDATE users SET name = ?, version = version + 1 WHERE id = ? AND version = ?
}, 3)?;
```

#### 3.7.14 phinx_migration — Phinx 风格 migration API

Phinx 风格的链式建表 API，14 种列类型 + 索引 + 外键。

```rust
use sz_orm_core::phinx_migration::{PhinxTable, ColumnType, ColumnOptions, IndexOptions, ForeignKeyOptions};

let table = PhinxTable::new("users")
    .add_column("id", ColumnType::Bigint, ColumnOptions::new().primary().auto_increment())
    .add_column("name", ColumnType::String, ColumnOptions::new().length(255).not_null())
    .add_column("email", ColumnType::String, ColumnOptions::new().length(255).unique())
    .add_column("dept_id", ColumnType::Bigint, ColumnOptions::new().not_null())
    .add_column("created_at", ColumnType::DateTime, ColumnOptions::new().default_current_timestamp())
    .add_index(IndexOptions::new().columns(&["email"]).unique())
    .add_foreign_key(
        ForeignKeyOptions::new("dept_id")
            .references("depts", "id")
            .on_delete_cascade()
    );

let sql = table.create_sql(&*dialect);
// → CREATE TABLE users (...) + CREATE INDEX + ALTER TABLE ADD CONSTRAINT
```

#### 3.7.15 queryable — Diesel 风格 Queryable trait

Diesel 风格的从数据库行构造实体的 trait，配合 `#[derive(Queryable)]` 使用。

```rust
use sz_orm_core::queryable::{Queryable, RowDesc, Row};

#[derive(Queryable, Debug)]
struct User {
    id: i64,
    name: String,
    email: String,
}

// 从查询结果行自动构造 User
let row = Row::new(vec![
    ("id".to_string(), Value::I64(42)),
    ("name".to_string(), Value::String("Alice".into())),
    ("email".to_string(), Value::String("a@b.com".into())),
]);
let user: User = User::from_row(&row)?;
```

#### 3.7.16 quick_query — 快捷查询 Db::name()

无需定义 Model 即可构造查询，类似 ThinkPHP 的 `Db::name('users')->where(...)->select()`。

```rust
use sz_orm_core::quick_query::Db;
use sz_orm_core::DbType;

let (sql, params) = Db::new(DbType::PostgreSQL)
    .name("users")
    .select(&["id", "name", "email"])
    .where_cond("age", ">=", 18)
    .where_in("status", &["active", "verified"])
    .order_desc("created_at")
    .page(1, 20)
    .build_select();
// → SELECT id, name, email FROM users WHERE age >= 18 AND status IN (?, ?) ORDER BY created_at DESC LIMIT 20 OFFSET 0
// params = [18, "active", "verified"]
```

#### 3.7.17 repository — DDD 仓储模式

领域驱动设计（DDD）仓储模式抽象，支持任意 Key 类型、分页、批量操作。

```rust
use sz_orm_core::repository::{Repository, InMemoryRepository, WhereCondition, WhereOp};

let repo = InMemoryRepository::<User>::new();
repo.save(User { id: 1, name: "Alice".into(), age: 25 })?;
repo.save(User { id: 2, name: "Bob".into(), age: 30 })?;

// 条件查询 + 分页
let page = repo.paginate_by(
    &[WhereCondition::new("age", WhereOp::Ge, 18)],
    1, 20,
)?;
// PageResult { items: [User { id: 1, ... }, User { id: 2, ... }], total: 2, page: 1, page_size: 20 }

// 单个查询
let user = repo.find_one_by(&[WhereCondition::new("name", WhereOp::Eq, "Alice")])?;
```

#### 3.7.18 result_map — MyBatis ResultMap + Hibernate Native Query

MyBatis 风格的 ResultMap，支持嵌套关联、嵌套集合、多态鉴别器；以及 Hibernate `@SqlResultSetMapping` 风格的原生 SQL 映射。

```rust
use sz_orm_core::result_map::{
    ResultMapRegistry, ResultMap, Mapping, NestedAssociation, apply_result_map,
};

let mut registry = ResultMapRegistry::new();

let mut user_map = ResultMap::new("userMap", "User");
user_map.add_id_mapping(Mapping::new("id", "user_id"));
user_map.add_result_mapping(Mapping::new("name", "user_name"));
user_map.add_association(
    NestedAssociation::new("dept", "deptMap")
        .with_prefix("dept_")
);
registry.register(user_map);

let mut dept_map = ResultMap::new("deptMap", "Dept");
dept_map.add_id_mapping(Mapping::new("id", "dept_id"));
dept_map.add_result_mapping(Mapping::new("name", "dept_name"));
registry.register(dept_map);

// 应用 ResultMap 到查询结果行
let result = apply_result_map(&registry, "userMap", &row)?;
// → 自动构造 User { id, name, dept: Dept { id, name } }
```

#### 3.7.19 schema_gen — Diesel 风格 schema.rs 自动生成

从数据库 schema 生成 Diesel 风格的 `schema.rs`（`typed_query!` 宏声明）。

```rust
use sz_orm_core::schema_gen::{SchemaGenerator, TableSchema, ColumnSchema};

let gen = SchemaGenerator::new().emit_use(true);
let tables = vec![
    TableSchema {
        name: "users".into(),
        columns: vec![
            ColumnSchema { name: "id".into(), rust_type: "i64".into() },
            ColumnSchema { name: "name".into(), rust_type: "String".into() },
            ColumnSchema { name: "age".into(), rust_type: "i32".into() },
        ],
    },
];

let schema_rs = gen.generate(&tables);
// 输出：
// use sz_orm_core::typed::{TypedTable, TypedColumn};
// typed_query! {
//     pub struct users { id: i64, name: String, age: i32 }
// }
```

#### 3.7.20 sql_safety — SQL 注入防护原语

提供标识符、外键动作、IN 子句 id 值的校验原语，被多个模块复用。

```rust
use sz_orm_core::sql_safety::{validate_identifier, validate_fk_action, validate_id_value};

// ✅ 合法标识符
validate_identifier("users", "table")?;
validate_identifier("user_name", "column")?;

// ❌ 拒绝包含 SQL 注入的标识符
validate_identifier("users; DROP TABLE users", "table").unwrap_err();
validate_identifier("1abc", "column").unwrap_err();  // 数字开头非法

// ✅ 合法外键动作
validate_fk_action("CASCADE")?;
validate_fk_action("SET NULL")?;

// ❌ IN 子句注入防护
validate_id_value("1").unwrap_err();      // 不允许 -- 注释
validate_id_value("1; DROP").unwrap_err();
```

#### 3.7.21 type_handler — MyBatis 风格 TypeHandler SPI

MyBatis 风格的 TypeHandler SPI，Rust 类型与 ORM `Value` 之间的双向转换注册中心。

```rust
use sz_orm_core::type_handler::{TypeHandlerRegistry, DateTimeHandler, UuidHandler};
use sz_orm_core::Value;

let mut registry = TypeHandlerRegistry::new();
registry.register("datetime", Box::new(DateTimeHandler));
registry.register("uuid", Box::new(UuidHandler));

// 将字段绑定到指定 TypeHandler
registry.bind("created_at", "datetime");
registry.bind("user_uuid", "uuid");

// 读取：Value → Rust 类型
let parsed: String = registry.handle("created_at", &Value::DateTime("2026-07-19T10:00:00Z".into()))?;
// 写入：Rust 类型 → Value
let value = registry.to_value("user_uuid", &String::from("550e8400-e29b-41d4-a716-446655440000"))?;
// → Value::Uuid("550e8400-e29b-41d4-a716-446655440000")
```

#### 3.7.22 模块间协作矩阵

各模块可组合使用，典型协作关系：

| 场景 | 涉及模块 | 说明 |
|------|---------|------|
| 写入路径 | hooks + behaviors + dirty_attributes + accessors | 钩子→行为→脏字段→访问器/修改器→SQL |
| 读取路径 | queryable + result_map + type_handler + l2_cache | 行构造→ResultMap→TypeHandler→缓存 |
| 权限控制 | data_permission + guard + sql_safety | 拦截器→守卫→原语校验 |
| 关联加载 | entity_graph + find_with_related + join_dsl | 批量加载→JOIN→eager load |
| 缓存层 | l2_cache + cache + dirty_attributes | L2→L1→脏字段失效 |
| 迁移建表 | phinx_migration + migration + schema_gen | Phinx API→文件迁移→schema 生成 |
| 观察者 | observer + hooks + behaviors | 多套生命周期钩子协同 |
| 乐观锁 | optimistic_lock + dirty_attributes | 版本检查→仅更新脏字段 |
| 动态过滤 | dynamic_filter + data_permission | 全局 Filter+权限规则 |
| Plugin 链 | hydration_plugin + audit + observer | MyBatis 风格拦截器链 |

---

## 四、常见场景示例

### 4.1 CRUD

```rust
use sz_orm_core::*;
use std::collections::HashMap;

let dialect = get_dialect(DbType::PostgreSQL).unwrap();

// Create
let mut data = HashMap::new();
data.insert("name".to_string(), Value::String("Alice".into()));
data.insert("age".to_string(), Value::I64(25));
let insert = QueryBuilder::<User>::new(get_dialect(DbType::PostgreSQL).unwrap())
    .table("users").build_insert(&data);

// Read
let select = QueryBuilder::<User>::new(get_dialect(DbType::PostgreSQL).unwrap())
    .table("users").select(vec!["id", "name"])
    .where_between("age", Value::I64(18), Value::I64(30))
    .where_in("status", vec![Value::String("active".into())])
    .page(2, 20)               // 第 2 页，每页 20 条
    .build_select();

// Update
let update = QueryBuilder::<User>::new(get_dialect(DbType::PostgreSQL).unwrap())
    .table("users").where_cond("id = 1").build_update(&data);

// Delete
let delete = QueryBuilder::<User>::new(dialect)
    .table("users").where_cond("id = 1").build_delete();

// 聚合
let count = QueryBuilder::<User>::new(get_dialect(DbType::PostgreSQL).unwrap())
    .table("users").where_cond("status = 'active'").build_count();
```

### 4.2 事务

```rust,no_run
use sz_orm_core::*;
use std::time::Duration;

let opts = TransactOptions::default()
    .with_isolation(IsolationLevel::Serializable)
    .with_timeout(Duration::from_secs(30));

let mut tx = Transaction::new(conn, opts);
tx.execute("UPDATE account SET balance = balance - 100 WHERE id = 1").await?;
let sp = tx.savepoint().await?;                    // SAVEPOINT sp_N
match tx.execute("UPDATE account SET balance = balance + 100 WHERE id = 2").await {
    Ok(_) => tx.release_savepoint(&sp).await?,
    Err(e) => { tx.rollback_to_savepoint(&sp).await?; return Err(e); }
}
tx.commit().await?;

// 多事务管理
let mgr = TransactionManager::new();
mgr.begin("tx1", conn2, TransactOptions::default()).await?;
mgr.commit("tx1").await?;
```

### 4.3 连接池

```rust,no_run
use sz_orm_core::*;

let config = PoolConfigBuilder::new()
    .max_size(100)          // 最大连接数（默认 DEFAULT_MAX_SIZE = 100）
    .min_idle(10)           // 最小空闲连接（默认 5）
    .acquire_timeout(30)    // 获取超时秒（默认 30）
    .idle_timeout(600)      // 空闲超时秒（默认 600）
    .max_lifetime(1800)     // 最大生命周期秒（默认 1800）
    .build()?;              // 配置非法时返回 PoolError::InvalidConfig

let pool = Pool::new(config, factory)?;
let conn = pool.acquire().await?;    // 带超时获取
pool.release(conn).await;            // 归还
let status = pool.status().await;    // PoolStatus { idle, active, max, min }
pool.reap_idle().await;              // 主动回收空闲连接
pool.close_all().await;              // 关闭池，拒绝新 release
```

### 4.4 迁移

```rust,no_run
use sz_orm_core::*;
use std::path::PathBuf;

// 文件命名约定：<version>_<name>_up.sql / <version>_<name>_down.sql
let resolver = FileMigrationResolver::new(PathBuf::from("./migrations"));
let migrations = resolver.resolve(DbType::MySQL)?;

let mut migrator = Migrator::new(MigrationContext::default())
    .add_migrations(migrations);
migrator.migrate().await?;          // 执行所有待迁移
migrator.up(Some("003")).await?;    // 迁移到指定版本
migrator.down(Some("001")).await?;  // 回滚到指定版本
migrator.rollback("002").await?;    // 回滚单个迁移
migrator.reset().await?;            // 全部回滚并重新执行
let p = migrator.progress();        // MigrationProgress { total, applied, pending }

// SchemaBuilder 程序化建表
let ddl = SchemaBuilder::new("users")
    .add_column(ColumnDef::new("id", "INT").not_null().auto_increment())
    .add_column(ColumnDef::new("name", "VARCHAR").length(255).not_null())
    .add_index(IndexDef::new("idx_name", vec!["name"]).unique())
    .add_foreign_key(ForeignKeyDef::new("fk_role", "role_id", "roles", "id").on_delete("CASCADE"))
    .build(DbType::MySQL);
```

### 4.5 查询构建 + 校验

```rust
use sz_orm_core::*;

let builder = QueryBuilder::<User>::new(get_dialect(DbType::MySQL).unwrap())
    .table("users")
    .select(vec!["id", "name"])
    .join_left("profiles", "users.id", "profiles.user_id")
    .group_by("status")
    .having("COUNT(*) > 5");

builder.validate()?;                 // 校验 SELECT：语法/注入/括号平衡/标识符
let sql = builder.build_select();    // 校验通过后再生成
```

### 4.6 真实数据库端到端（MySQL 示例）

```rust,no_run
use sz_orm_core::{Pool, PoolConfigBuilder};
use sz_orm_sqlx::{MySqlPoolHandle, SqlxMySqlConnectionFactory};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let handle = MySqlPoolHandle::connect(
        "mysql://root:<your-password>@127.0.0.1:3306/sz_orm_test"
    ).await?;
    let factory = Arc::new(SqlxMySqlConnectionFactory::new(Arc::new(handle)));
    let pool = Pool::new(PoolConfigBuilder::new().max_size(20).build()?, factory)?;

    let mut conn = pool.acquire().await?;
    conn.execute("CREATE TABLE IF NOT EXISTS t (id INT PRIMARY KEY)").await?;
    conn.execute("INSERT INTO t VALUES (1)").await?;
    let rows = conn.query("SELECT id FROM t").await?;
    println!("{} rows", rows.len());
    Ok(())
}
```

---

## 五、数据库支持说明

| 数据库 | Dialect 实现 | 占位符 | 分页语法 | 标识符引用 | 真实连接 |
|--------|-------------|--------|----------|-----------|---------|
| MySQL 8+/9.x | `MySqlDialect` | `?` | `LIMIT n OFFSET m` | 反引号 `` ` `` | ✅ sz-orm-sqlx |
| PostgreSQL 14+/18 | `PostgresDialect` | `$1, $2, ...` | `LIMIT n OFFSET m` | 双引号 `"` | ✅ sz-orm-sqlx |
| SQLite 3.35+ | `SqliteDialect` | `?` | `LIMIT n OFFSET m` | 双引号 `"` | ✅ sz-orm-sqlx |
| Oracle 23ai | `OracleDialect` | `:1, :2, ...` | `OFFSET n ROWS FETCH NEXT m ROWS ONLY` | 双引号 `"` | ✅ dialect 级 |
| pgvector 扩展 | - | `vector(dim)` 类型 | `<->` / `<=>` / `<#>` 距离操作符 | - | ✅ sz-orm-vector（feature `real-pg`） |

每个方言负责：标识符引用风格、字符串转义、分页语法、JSON 提取（`JSON_EXTRACT` / `#>>` / `json_extract` / `JSON_VALUE`）、全文搜索（`MATCH AGAINST` / `to_tsvector` / `CONTAINS`）、布尔转整数（`IF`/`CASE`）、自增关键字（`AUTO_INCREMENT` / `GENERATED BY DEFAULT AS IDENTITY`）。

**pgvector 支持**：PostgreSQL 14+ 可通过 `CREATE EXTENSION vector` 启用 pgvector 扩展。sz-orm-vector 的 `RealPgVectorStore` 自动处理 `vector(dim)` 类型列的创建和查询，支持 cosine、euclidean（L2）、dot-product（IP）三种距离度量。

通过 `get_dialect(DbType::MySQL)` 获取方言实例；`DbType` 共 11 种枚举（含 Redis、MongoDB 等预留类型），并提供 `as_str()`、`from_str()`、`supports_schema()`、`supports_transaction()`、`supports_foreign_key()`、`default_port()` 等能力查询方法。

---

## 六、性能优化建议

1. **连接池调参**：高并发场景将 `max_size` 设为 CPU 核数 × 2～4；`min_idle` 保持日常均值，避免冷启动建连开销；`max_lifetime` 小于数据库 `wait_timeout`，防止拿到已被服务端断开的连接。
2. **批量写入**：使用 `DEFAULT_BATCH_SIZE`（1000）分批 INSERT；10 万行批量插入实测 SQLite 72 万行/s、PG 26.8 万行/s、MySQL 14.5 万行/s。
3. **只选必要列**：`select(vec!["id", "name"])` 而非 `SELECT *`，减少网络与解码开销。
4. **分页深翻页**：大 offset 场景改用主键游标（`where_cond("id > ?")` + `limit(n)`），避免 `OFFSET` 扫描放大。
5. **校验前置**：开发/测试环境开启 `validate()` 与 `sql_string!`；生产热路径可在构建期完成校验后直接使用生成的 SQL。
6. **事务最小化**：事务内只放必须原子执行的语句；长事务会占用池连接并阻塞空闲回收。
7. **缓存复用**：热点读使用 `MultiLevelCache`（内存 L1 + 外部 L2），设置合理 TTL。
8. **空闲回收**：低流量时段调用 `reap_idle()` 主动释放空闲连接，降低数据库句柄压力。
9. **JSON 字段查询**：使用 `JsonQuery` 链式构建跨方言 JSON 查询，避免手写方言特化 SQL；高频 JSON 字段考虑生成列 + 索引。
10. **关联加载**：1:1/N:1 用 `find_with_related_join`；1:N 用 `find_with_related_eager_sql` 避免行膨胀；大数据量用 `find_with_related_subquery`。
11. **分布式事务选型**：同构数据库用 2PC（`DtxManager`）；异构系统用 TCC（`TccCoordinator`）；长流程业务用 Saga（`Saga`）；跨分片原子写入用 `CrossShardCoordinator`。
12. **pgvector 向量搜索**：对 embedding 列创建 ivfflat 索引（`CREATE INDEX ON vectors_{name} USING ivfflat (embedding vector_cosine_ops)`），大规模向量搜索性能提升 10-100 倍。
13. **NL→SQL 使用建议**：SimpleNl2SqlEngine 适合固定模板场景（零延迟、零成本）；OpenAINl2SqlEngine 适合灵活查询，但需缓存常见查询模式以降低成本。
14. **AI 安全**：使用 NL→SQL 时必须启用 `validate()` 安全检查，禁止直接将 LLM 输出作为 SQL 执行。

---

## 七、故障排除

| 症状 | 可能原因 | 处理 |
|------|---------|------|
| `PoolError::Exhausted` (PL001) | 池满且均有活跃连接 | 增大 `max_size`；检查连接是否泄漏（未 `release`） |
| `PoolError::Timeout` (PL002) | `acquire_timeout` 过短或慢查询堆积 | 增大超时；排查慢 SQL；确认事务及时提交 |
| `PoolError::InvalidConfig` (PL005) | `min_idle > max_size` 等非法配置 | `PoolConfigBuilder::build()` 会校验，按错误信息调整 |
| `DbError::ConnectionRefused` (DB003) | 数据库未启动/端口错误 | 核对 URL 与端口（MySQL 3306 / PG 5432 / Oracle 1521） |
| `DbError::ConstraintViolation` (DB014) | 违反唯一/外键约束 | 检查插入数据；该错误 `is_retryable() = false`，不要重试 |
| `sql_string!` 编译失败 | SQL 语法错误、注入模式、参数个数不符 | 按编译错误提示修正 SQL 字面量或 `params:` 数量 |
| `validate()` 返回 `InjectionDetected` | 命中 12 种注入模式 | 改用参数绑定，不要拼接用户输入 |
| 真实 DB 测试被跳过 | 测试标记 `#[ignore]`，默认不运行 | `cargo test -- --ignored`，并确认本机 DB 已启动 |
| 云服务测试失败 | 未启用 feature 或未启动服务 | 例：`cargo test -p sz-orm-mqtt --features real-broker -- --ignored` |
| MySQL bool 解码异常 | 直接按列 Rust 类型解码踩坑 | sz-orm-sqlx 已按 `type_info().name()` 分发，升级到最新版本 |
| TCC `confirm` 失败 | 网络/数据库抖动导致 confirm 中断 | 调用 `retry_confirm()` 重试（confirm 必须幂等） |
| Saga 补偿失败 | 补偿操作自身失败 | 状态进入 `CompensationFailed`，需人工介入或修复补偿逻辑后重试 |
| 跨分片协调 prepare 失败 | 某分片不可达或 prepare 阶段写 undo log 失败 | 自动回滚已 prepare 的分片；排查分片连通性后重试整个事务 |

---

## 八、工程化门禁与质量保障

SZ-ORM 实施严格的工程化门禁体系，确保代码质量。

### 8.1 标准门禁（门禁 1-7）

| 门禁 | 检查项 | 说明 |
|------|--------|------|
| 1 | `cargo fmt --all --check` | 代码格式 |
| 2 | `cargo check --workspace` | 编译检查 |
| 3 | `cargo clippy --workspace -- -D warnings` | lint 0 警告 |
| 4 | `cargo test --workspace` | 全部测试通过 |
| 5 | `cargo doc --workspace` | 文档生成 |
| 6 | API 扫描 | 公开 API 变更检测 |
| 7 | 契约测试 | 向后兼容性验证 |

### 8.2 强化门禁（门禁 8-10）

| 门禁 | 检查项 | 结果 | 说明 |
|------|--------|------|------|
| 8 | 占位实现扫描 | ✅ 0 处 | 禁止 `todo!()`/`unimplemented!()`/`unreachable!()` |
| 9 | SQL 注入扫描 | ✅ 8 处已修复 | 检测 `format!` 拼接、字符串插值、`to_string()+SQL` |
| 10 | `--all-features` 编译 | ✅ 零错误 | 全 feature 组合编译 |

门禁 9 修复的 8 处 SQL 注入：query-builder 5 处（Insert/Update/Delete/JOIN/GROUP BY/ORDER BY 表名列名未转义）+ sz-orm-back 1 处（backup.rs）+ sz-orm-lc 2 处（generate_crud）。全部修复为 `quote_ident()` 标识符转义或参数化查询。

### 8.3 测试金字塔

| 层级 | 类型 | 说明 |
|------|------|------|
| T1 | 单元测试 | 每个模块独立测试 |
| T2 | 契约测试 | API 向后兼容 |
| T3 | 集成测试 | 真实 DB 端到端 |
| T4 | 属性测试 | Property-Based + Fuzz |
| T5 | 压力测试 | 高并发场景 |
| T6 | Soak 测试 | 长时间稳定性 |

### 8.4 Soak Test 体系

Soak 测试验证长时间运行稳定性。

**运行方式**：
```bash
# 10 秒冒烟测试
SOAK_DURATION=10s cargo test -p sz-orm-core --test soak -- --ignored

# 1 小时测试
SOAK_DURATION=1h cargo test -p sz-orm-core --test soak -- --ignored

# 24 小时测试（CI 自动触发）
SOAK_DURATION=24h cargo test -p sz-orm-core --test soak -- --ignored
```

**1h Soak 实测结果**：
- 总操作数：13.8 亿次
- 吞吐衰减：1.16%（382 万 → 378 万 ops/s）
- P99 延迟：43μs → 41μs（稳定无漂移）
- 错误数：0
- 连接池：终态 idle=active=max=8（无泄漏）

**退化检测（6 类）**：
- 吞吐衰减 >10%
- P99 延迟增长 >2x
- RSS 内存增长 >50MB
- fd_count 增长 >10
- 连接池泄漏（active != 0 且 idle != max）
- 错误率 >0.1%

**CI 自动触发**：每周日 UTC 00:00 自动运行 24h Soak，支持 `workflow_dispatch` 手动触发。

### 8.5 工程化规范文档

详细的工程化规范见 [sz-orm-engineering-practices.md](../../sz-orm/docs/sz-orm-engineering-practices.md)。

---

## 九、相关文档

| 文档 | 说明 | 何时查阅 |
|------|------|---------|
| **[API 参考手册](sz-ormAPI参考.md)** | **核心 trait/结构体与各包公开 API 手册**（v5.0，覆盖 §2.1-§2.22 共 22 个章节） | 需查阅类型签名、参数说明、错误码时 |
| 《架构设计.md》 | 整体架构、依赖关系、设计决策、扩展开发指南 | 需理解整体架构与设计决策时 |
| 《性能基准.md》 | 性能数据与基准测试运行方式 | 需了解吞吐/延迟/对比数据时 |
| 《项目成熟度评估报告.md》 | 成熟度评分与测试规模实测 | 需评估生产就绪度时 |
| 《项目实施进度表.md》 | 分阶段实施进度 | 需了解功能完成进度时 |
| 《sz-orm生产就绪报告.md》 | L4 金融级生产就绪度评估与签发 | 生产上线前评审 |
| 《sz-orm-engineering-practices.md》 | 工程化规范（门禁 1-10 + 测试金字塔 + Soak Test） | 贡献代码前需了解工程规范 |
| 《SZ-ORM 与主流 ORM 对比.md》 | 与 Diesel/SeaORM/SQLx 的深度对比 | 选型决策时 |
| 《sz-orm技术实现深度评估.md》 | 技术实现深度评估 | 深度技术评估 |
| 《sz-orm全面审查报告v1.md》 | 全面代码审查报告 | 代码质量审查 |
| 《Security.md》 | 安全设计文档 | 安全评估 |
| 《api-contracts.md》 | API 契约文档 | 契约测试 |
| 《sz-orm改造实施文档.md》 | 改造实施文档 | 历史决策追溯 |

### 9.1 文档分工

| 文档类型 | 本指南（使用指南） | API 参考手册 |
|---------|------------------|-------------|
| 定位 | 场景驱动：什么场景用什么模块、怎么用 | 类型驱动：每个 trait/结构体/函数的签名 |
| 内容 | 端到端示例 + 模块间协作矩阵 | 速查表 + 错误码 + 类型签名 |
| 适用 | 初学者入门、快速上手、最佳实践 | API 速查、IDE 配合查阅、深度集成 |
| 阅读方式 | 顺序阅读 | 按需查阅 |

### 9.2 文档与代码的对应关系

| sz-orm-core 模块 | 本指南章节 | API 手册章节 |
|----------------|----------|------------|
| model / query / dialect / pool / transaction / migration / cache / value / db_type / error / hooks / json_query / dynamic_sql / typed_ast / find_with_related | §3.1 / §3.6 | §1 / §2.16-§2.19 |
| accessors / behaviors / data_permission / dirty_attributes / dynamic_filter / entity_graph / guard / hydration_plugin / join_dsl / l2_cache / lambda / observer / optimistic_lock / phinx_migration / queryable / quick_query / repository / result_map / schema_gen / sql_safety / type_handler | §3.7 | §2.22 |
| sqlx / sql-validator / macros | §3.2 | §2.1-§2.3 |
| 27 个扩展包 | §3.3-§3.5 | §2.4-§2.15 / §2.20-§2.21 |
| 错误码体系 | §7 故障排除 | §3 错误处理指南 |
| 钩子系统 | §3.1.1 | §4 钩子系统 |
| 工程化门禁 | §8 | 《sz-orm-engineering-practices.md》 |
