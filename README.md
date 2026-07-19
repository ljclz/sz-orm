# SZ-ORM — 鲜视达 ORM

> 生产级、L4 金融级纯 Rust 异步 ORM，兼容 ThinkORM 风格 API。

[![Rust](https://img.shields.io/badge/rust-1.75+-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Tests](https://img.shields.io/badge/tests-1330+-green.svg)](#测试)
[![Dialects](https://img.shields.io/badge/dialects-11-red.svg)](#支持的数据库)
[![Packages](https://img.shields.io/badge/packages-31-purple.svg)](#工作空间结构)

---

## 目录

- [概览](#概览)
- [核心特性](#核心特性)
- [工作空间结构](#工作空间结构)
- [快速入门](#快速入门)
- [支持的数据库](#支持的数据库)
- [核心 API](#核心-api)
- [钩子系统（软删除+多租户）](#钩子系统软删除多租户)
- [CLI 工具](#cli-工具)
- [示例集](#示例集)
- [测试](#测试)
- [构建与文档](#构建与文档)
- [许可证](#许可证)

---

## 概览

SZ-ORM 是一个纯 Rust 实现的异步 ORM 框架，目标是为 Rust 生态提供一个**生产级**、**金融级**的数据库访问层。

| 维度 | 数据 |
|------|------|
| 工作空间包数 | 31 |
| 支持数据库方言 | 11 |
| 测试用例 | 1330+ |
| 生产等级 | L4（金融级） |
| 异步运行时 | Tokio |
| Rust 最低版本 | 1.75 |

## 核心特性

- **异步**：基于 Tokio，全程 `async/await`
- **多数据库方言**：MySQL / PostgreSQL / SQLite / Oracle 23ai / OceanBase / SQL Server / ClickHouse / Redis / MongoDB / VectorDB / PureJsDb
- **链式 QueryBuilder**：仿 ThinkORM 风格的 fluent API
- **ACID 事务**：隔离级别、保存点（嵌套事务）、`TransactionManager` 多事务管理
- **连接池**：可配置大小、超时、空闲回收、健康检查、最大生命周期
- **迁移系统**：up/down/rollback/reset/refresh + `SchemaBuilder` 程序化建表
- **多级缓存**：`MemoryCache` / `MultiLevelCache`，支持 TTL
- **钩子系统**：`Hookable` trait + `HookRegistry` 运行时钩子
- **软删除**：`SoftDelete` trait + `SoftDeleteScope` 全局作用域
- **多租户**：`TenantModel` trait + `TenantScope` 自动 `tenant_id = ?` 过滤
- **SQL 校验**：编译期 + 运行时双重校验、注入检测
- **关联关系**：BelongsTo / HasMany / HasOne / BelongsToMany + Eager Loading
- **扩展生态**：加密、JWT、调度、MQTT、WebSocket、消息队列、对象存储、AI、gRPC、GraphQL、ES、追踪、日志、Swagger、脱敏、健康检查、审计、批量、WASM、备份、分布式事务、读写分离、分库分表、限流

## 工作空间结构

```
sz-orm/
├── packages/
│   ├── sz-orm-core/             # 核心引擎（Model/Query/Dialect/Pool/Tx/Migration/Cache/Hooks）
│   ├── sz-orm-sqlx/             # sqlx 真实数据库适配器
│   ├── sz-orm-sql-validator/    # SQL 校验与注入检测
│   ├── sz-orm-macros/           # 派生宏
│   │
│   ├── sz-orm-crypto/           # 加密（AES-256-GCM/PBKDF2/HMAC）
│   ├── sz-orm-auth/             # JWT 鉴权
│   ├── sz-orm-scheduler/        # Cron 定时任务
│   ├── sz-orm-mqtt/             # MQTT 客户端
│   ├── sz-orm-websocket/        # WebSocket 服务端
│   ├── sz-orm-queue/            # 消息队列（RabbitMQ/Kafka/NATS/...）
│   ├── sz-orm-storage/          # 对象存储（S3/阿里云/腾讯云/...）
│   ├── sz-orm-ai/               # AI 集成（Embedding/RAG/Vector）
│   ├── sz-orm-grpc/             # gRPC
│   ├── sz-orm-graphql/          # GraphQL
│   ├── sz-orm-es/               # Elasticsearch
│   ├── sz-orm-tracing/          # 分布式追踪
│   ├── sz-orm-logger/           # 日志
│   ├── sz-orm-swagger/          # API 文档
│   ├── sz-orm-masking/          # 数据脱敏
│   ├── sz-orm-health/           # 健康检查
│   ├── sz-orm-audit/            # 审计日志
│   ├── sz-orm-batch/            # 批量操作
│   │
│   ├── sz-orm-dtx/              # 分布式事务
│   ├── sz-orm-rw/               # 读写分离
│   ├── sz-orm-sharding/         # 分库分表
│   ├── sz-orm-limit/            # 限流控制
│   ├── sz-orm-config/           # 配置管理
│   ├── sz-orm-mig/              # 数据迁移
│   │
│   ├── sz-orm-wasm/             # WebAssembly 编译目标
│   ├── sz-orm-lc/               # 本地/边缘计算
│   └── sz-orm-back/             # 备份与恢复
│
├── cli/                         # 命令行工具（sz-orm）
├── examples/                    # 使用示例集
├── Cargo.toml                   # 工作空间清单
├── audit.toml                   # cargo-audit 配置
└── deny.toml                    # cargo-deny 配置
```

## 快速入门

### 1. 添加依赖

```toml
[dependencies]
sz-orm-core = "0.1"
tokio = { version = "1", features = ["full"] }
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
    .where_cond("status = 'active'")
    .order_by("created_at")
    .limit(10)
    .build_select();
```

### 4. INSERT / UPDATE / DELETE

```rust
use std::collections::HashMap;

let mut data = HashMap::new();
data.insert("name".to_string(), Value::String("Alice".into()));
data.insert("age".to_string(), Value::I64(25));

let insert_sql = QueryBuilder::<User>::new(get_dialect(DbType::MySQL)?)
    .table("users")
    .build_insert(&data);

let update_sql = QueryBuilder::<User>::new(get_dialect(DbType::MySQL)?)
    .table("users")
    .where_cond("id = 1")
    .build_update(&data);

let delete_sql = QueryBuilder::<User>::new(get_dialect(DbType::MySQL)?)
    .table("users")
    .where_cond("id = 1")
    .build_delete();
```

## 支持的数据库

| 数据库 | 方言 | 真实连接 | 默认端口 |
|--------|------|----------|----------|
| MySQL | `MySqlDialect`（反引号） | sz-orm-sqlx | 3306 |
| PostgreSQL | `PostgreSqlDialect`（双引号） | sz-orm-sqlx | 5432 |
| SQLite 3.35+ | `SqliteDialect` | sz-orm-sqlx | — |
| Oracle 23ai | `OracleDialect` | sz-orm-sqlx | 1521 |
| OceanBase | `MySqlDialect` 兼容 | — | 2881 |
| SQL Server | `MySqlDialect` 兼容 | — | 1433 |
| ClickHouse | `MySqlDialect` 兼容 | — | 8123 |
| Redis | NoSQL（不支持 SQL 方言） | — | 6379 |
| MongoDB | NoSQL | — | 27017 |
| VectorDB | 向量数据库 | — | 19530 |
| PureJsDb | JS 引擎数据库 | — | — |

通过 `get_dialect(DbType::MySQL)` 获取方言实例。

## 核心 API

### QueryBuilder 链式 API

```rust
QueryBuilder::<M>::new(dialect)
    .table("users")
    .select(vec!["id", "name"])
    .where_cond("status = 'active'")            // AND
    .or_where("role = 'admin'")                  // OR
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

migrator.migrate().await?;                // 执行所有待迁移
migrator.up(Some("003")).await?;           // 执行到 003
migrator.down(Some("001")).await?;         // 回滚到 001
migrator.rollback("002").await?;           // 回滚单个
migrator.reset().await?;                   // 全部回滚 + 重新执行
migrator.refresh().await?;                 // 同 reset
migrator.progress();                       // 迁移进度

// SchemaBuilder 程序化建表
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

// 类型转换
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

### 错误类型体系

```rust
use sz_orm_core::DbError;

// DbError — 20 种变体，错误码 DB001-DB020
DbError::QueryError("...")
DbError::ConnectionRefused("...")
DbError::ConnectionTimeout("...")
DbError::NotFound("...")
DbError::Hook("...")           // DB019 — 钩子执行失败
DbError::TenantError("...")    // DB020 — 多租户错误

err.is_retryable();             // 是否可重试
err.error_code();               // "DB001"
```

## 钩子系统（软删除+多租户）

### HookContext — 钩子执行上下文

```rust
use sz_orm_core::hooks::HookContext;

let mut ctx = HookContext::new()
    .with_tenant(42)
    .with_operator(1)
    .with_timestamp(1700000000);
ctx.set_meta("source", "api");
```

### Hookable trait — 6 个生命周期钩子

```rust
use sz_orm_core::hooks::{Hookable, HookContext, HookResult};

impl Hookable for User {
    fn before_insert(_ctx: &mut HookContext) -> HookResult<()> { Ok(()) }
    fn after_insert(_ctx: &HookContext, _id: &Self::PrimaryKey) -> HookResult<()> { Ok(()) }
    fn before_update(_ctx: &mut HookContext, _id: &Self::PrimaryKey) -> HookResult<()> { Ok(()) }
    fn after_update(_ctx: &HookContext, _id: &Self::PrimaryKey) -> HookResult<()> { Ok(()) }
    fn before_delete(_ctx: &mut HookContext, _id: &Self::PrimaryKey) -> HookResult<()> { Ok(()) }
    fn after_delete(_ctx: &HookContext, _id: &Self::PrimaryKey) -> HookResult<()> { Ok(()) }
}
```

### SoftDelete + SoftDeleteScope

```rust
use sz_orm_core::hooks::{SoftDelete, SoftDeleteScope, GlobalScope};

impl SoftDelete for Product {
    fn soft_delete_field() -> &'static str { "deleted_at" }
    fn is_deleted(&self) -> bool { self.deleted_at.is_some() }
}

// 查询时自动追加: AND deleted_at IS NULL
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

// ctx.tenant_id = Some(42) 时自动追加: AND tenant_id = ?
// ctx.tenant_id = None 时不追加（跨租户查询，调用方自行保证安全）
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
registry.enable("soft_delete");        // 启用
registry.is_enabled("soft_delete");    // true

// 临时禁用（闭包内）
let result = registry.without_scope("soft_delete", || {
    // 此处查询会包含已软删除的行
    42
});
```

## CLI 工具

SZ-ORM 提供命令行工具 `sz-orm`，用于迁移管理、代码生成、SQL 校验。

### 安装

```bash
cargo install --path cli
```

### 命令一览

```bash
sz-orm                              # 显示帮助
sz-orm info                         # 显示 ORM 概要信息
sz-orm --version                    # 显示版本号

sz-orm dialect list                 # 列出所有支持的方言
sz-orm dialect show mysql           # 显示 MySQL 方言详情

sz-orm make:migration create_users  # 生成迁移文件骨架
sz-orm make:model User              # 生成 Model 骨架代码

sz-orm migrate                      # 显示待执行迁移
sz-orm migrate:status               # 查看迁移进度

sz-orm sql:validate "SELECT * FROM users"  # SQL 校验
```

### 选项

- `--migrations <dir>` — 迁移文件目录（默认 `./migrations`）
- `--output <dir>` — 生成代码输出目录（默认 `./src/models` 或 `./migrations`）

## 示例集

`examples/` 目录提供 6 个可运行的示例：

| 示例 | 说明 | 运行命令 |
|------|------|----------|
| `quick_start` | QueryBuilder 基础用法 | `cargo run -p sz-orm-examples --bin quick_start` |
| `model_definition` | Model + ModelExt 完整实现 | `cargo run -p sz-orm-examples --bin model_definition` |
| `transaction` | 事务与保存点 | `cargo run -p sz-orm-examples --bin transaction` |
| `migration` | SchemaBuilder 建表 | `cargo run -p sz-orm-examples --bin migration` |
| `hooks_soft_delete` | 钩子+软删除 | `cargo run -p sz-orm-examples --bin hooks_soft_delete` |
| `multi_tenant` | 多租户隔离 | `cargo run -p sz-orm-examples --bin multi_tenant` |

## 测试

SZ-ORM 通过 **7 线验证体系** 保证质量：

| 验证方法 | 描述 | 测试文件 |
|---------|------|---------|
| **TDD** | 核心模块单元测试 | `core.rs` |
| **集成** | 真实 MySQL/PG/SQLite/Oracle 端到端 | `integration_*.rs` |
| **Jepsen** | 并发正确性测试 + 真实 DB Jepsen | `jepsen.rs`, `real_db_jepsen.rs` |
| **Fuzz** | 边界/边缘案例 | `fuzz.rs` |
| **Stress** | 性能基准测试 | `stress.rs`, `core_bench.rs` |
| **Chaos** | 故障鲁棒性 | `chaos.rs` |
| **Formal** | 形式化验证不变量 | `formal.rs` |

**总计：1330+ 测试，0 失败，57 忽略（需真实数据库服务）**

### 运行测试

```bash
# 全工作空间测试
cargo test --workspace

# 仅核心包测试
cargo test -p sz-orm-core

# 包含真实数据库测试（需启动 MySQL/PG/SQLite）
cargo test -p sz-orm-core --features testing

# 性能基准
cargo bench -p sz-orm-core
```

## 构建与文档

### 构建

```bash
# 全工作空间构建
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

# Lint 检查
cargo clippy --workspace -- -D warnings
```

### 安全审计

```bash
cargo audit
cargo deny check
```

## 项目文档

- [技术实现深度评估](../sz-orm技术实现深度评估.md) — 设计规范与架构决策
- [项目实施进度表](../sz-orm项目实施进度表.md) — 实施进度与里程碑
- [项目成熟度评估报告](../sz-orm项目成熟度评估报告.md) — 7 维度成熟度评分
- [架构设计](../docs/架构设计.md) — 31 包架构总览

## 许可证

MIT License © SZ-ORM Team
