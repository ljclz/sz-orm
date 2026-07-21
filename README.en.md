# SZ-ORM — Xian Shi Da ORM

> Production-grade, L4 financial-grade pure Rust async ORM with ThinkORM-style fluent API.

[![Rust](https://img.shields.io/badge/rust-1.94.0+-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Tests](https://img.shields.io/badge/tests-2950-green.svg)](#testing)
[![Dialects](https://img.shields.io/badge/dialects-11-red.svg)](#supported-databases)
[![Packages](https://img.shields.io/badge/packages-39-purple.svg)](#workspace-structure)
[![Version](https://img.shields.io/badge/version-1.0.0-blue.svg)](CHANGELOG.md)
[![Maturity](https://img.shields.io/badge/maturity-L4%20financial-brightgreen.svg)](docs/sz-orm项目成熟度评估报告.md)

---

## Table of Contents

- [Overview](#overview)
- [Core Features](#core-features)
- [Quality Baseline](#quality-baseline)
- [Workspace Structure](#workspace-structure)
- [Quick Start](#quick-start)
- [Supported Databases](#supported-databases)
- [Core API](#core-api)
- [Advanced Modules (21 in sz-orm-core)](#advanced-modules-21-in-sz-orm-core)
- [Hook System (Soft Delete + Multi-Tenancy)](#hook-system-soft-delete--multi-tenancy)
- [CLI Tool](#cli-tool)
- [Examples](#examples)
- [Testing](#testing)
- [Build & Documentation](#build--documentation)
- [Security Audit](#security-audit)
- [Performance Benchmarks](#performance-benchmarks)
- [Project Documentation](#project-documentation)
- [License](#license)

---

## Overview

SZ-ORM is a pure Rust async ORM framework aiming to provide a **production-grade**, **financial-grade** data access layer for the Rust ecosystem. It is compatible with ThinkORM-style fluent chainable API, supports 11 database dialects, and ships as a 39-member Cargo workspace.

| Dimension | Value |
|-----------|-------|
| Workspace packages | 39 (37 libs + CLI + examples) |
| Supported dialects | 11 (MySQL / PostgreSQL / SQLite / Oracle 23ai / OceanBase / SQL Server / ClickHouse / Redis / MongoDB / VectorDB / PureJsDb) |
| Test cases | 2950 passed, 0 failed (79 ignored requiring real DB/cloud credentials) |
| Code size | 85,834 LOC (src 18,430 + tests 67,404) |
| Production level | L4 (Financial-grade) |
| Maturity rating | 4.98 / 5.00 |
| Async runtime | Tokio 1.40+ |
| Minimum Rust version | 1.94.0+ (sqlx 0.9.0 requires) |
| Known bugs | 0 |

## Core Features

- **Async**: Built on Tokio, fully `async/await` end-to-end
- **Multi-dialect**: MySQL / PostgreSQL / SQLite / Oracle 23ai / OceanBase / SQL Server / ClickHouse / Redis / MongoDB / VectorDB / PureJsDb
- **Chainable QueryBuilder**: ThinkORM-style fluent API
- **ACID transactions**: Isolation levels, savepoints (nested transactions), `TransactionManager` for multi-tx management
- **Connection pool**: Configurable size, timeout, idle reaping, health check, max lifetime
- **Migration system**: up/down/rollback/reset/refresh + `SchemaBuilder` for programmatic DDL
- **Multi-level cache**: `MemoryCache` / `MultiLevelCache` with TTL
- **Hook system**: `Hookable` trait + `HookRegistry` runtime hooks (16 lifecycle events)
- **Soft delete**: `SoftDelete` trait + `SoftDeleteScope` global scope
- **Multi-tenancy**: `TenantModel` trait + `TenantScope` auto `tenant_id = ?` filtering
- **SQL validation**: Compile-time (`sql_string!` macro) + runtime (12 injection patterns)
- **Relations**: BelongsTo / HasMany / HasOne / BelongsToMany + Eager Loading
- **Distributed transactions**: 2PC + TCC (Try-Confirm-Cancel) + Saga + cross-shard ACID coordinator
- **AI extensions**: pgvector integration + NL→SQL (Simple rule engine + OpenAI API)
- **Real cloud services**: MQTT (rumqttc) / WebSocket (tokio-tungstenite) / RabbitMQ (lapin) / S3 (rust-s3)
- **Observability**: Prometheus exporter + OpenTelemetry traceparent propagation + Grafana dashboard
- **Soak test**: 24h CI soak (Sunday 00:00 UTC) with 10-field snapshots and 6 degradation detectors
- **Extension ecosystem**: 27 business extension packages (crypto/JWT/scheduler/storage/AI/gRPC/GraphQL/ES/tracing/audit/batch/WASM/backup/rw-split/sharding/rate-limit/...)

## Quality Baseline

- 7-line verification: TDD + Integration + Jepsen + Fuzz + Stress + Chaos + Formal
- 0 `panic!` / 0 `unimplemented!` / 0 `todo!` in production code
- `cargo clippy --workspace --all-targets -- -D warnings` passes with 0 warnings
- `cargo fmt --all --check` passes
- `cargo audit` — 0 unignored vulnerabilities (7 transitive dependencies ignored with documented reasons)
- `cargo deny check advisories bans licenses sources` — all OK
- 1h Soak Test: 1.38 billion operations, 1.16% throughput decay, P99 43μs→41μs, 0 errors, no pool leak

## Workspace Structure

```
sz-orm/
├── packages/
│   ├── sz-orm-core/                 # Core engine (Model/Query/Dialect/Pool/Tx/Migration/Cache/Hooks + 21 advanced modules)
│   ├── sz-orm-sqlx/                 # sqlx real-DB adapter (MySQL/PG/SQLite)
│   ├── sz-orm-sql-validator/        # SQL syntax + injection validation
│   ├── sz-orm-macros/               # Derive macros + sql_string! compile-time check
│   ├── sz-orm-query-builder/        # quote_ident + check_where_injection
│   ├── sz-orm-observability/        # MetricsRegistry + Counter/Gauge/Histogram + SloMonitor
│   ├── sz-orm-tracing/              # OpenTelemetry OTLP exporter
│   ├── sz-orm-vector/               # pgvector integration (cosine/euclidean/dot)
│   ├── sz-orm-ai/                   # NL→SQL + Embedding + RAG
│   │
│   ├── sz-orm-crypto/               # AES-256-GCM / PBKDF2 / HMAC
│   ├── sz-orm-auth/                 # JWT authentication
│   ├── sz-orm-scheduler/            # Cron tasks
│   ├── sz-orm-mqtt/                 # MQTT client (rumqttc)
│   ├── sz-orm-websocket/            # WebSocket server
│   ├── sz-orm-queue/                # RabbitMQ/Kafka/NATS/ActiveMQ/RocketMQ/Pulsar
│   ├── sz-orm-storage/              # S3/Aliyun/Tencent/Huawei/Qiniu/Upyun/Local
│   ├── sz-orm-grpc/                 # gRPC (tonic)
│   ├── sz-orm-graphql/              # GraphQL (async-graphql + axum)
│   ├── sz-orm-postgis/              # PostGIS geometry
│   ├── sz-orm-timeseries/           # TimescaleDB
│   ├── sz-orm-search/               # Elasticsearch/OpenSearch/Meilisearch
│   ├── sz-orm-es/                   # Elasticsearch legacy
│   ├── sz-orm-logger/               # Structured logging
│   ├── sz-orm-swagger/              # OpenAPI doc gen
│   ├── sz-orm-masking/              # Data masking
│   ├── sz-orm-health/               # Health check
│   ├── sz-orm-audit/                # Audit log
│   ├── sz-orm-batch/                # Batch operations
│   ├── sz-orm-dtx/                  # Distributed transactions (2PC/TCC/Saga)
│   ├── sz-orm-rw/                   # Read-write split
│   ├── sz-orm-sharding/             # Sharding
│   ├── sz-orm-limit/                # Rate limiting
│   ├── sz-orm-config/               # Config management
│   ├── sz-orm-mig/                  # Data migration transformer
│   ├── sz-orm-wasm/                 # WebAssembly target
│   ├── sz-orm-lc/                   # Local/edge compute
│   └── sz-orm-back/                 # Backup & restore
│
├── cli/                             # CLI tool (sz-orm)
├── examples/                        # 8 runnable examples
├── grafana/                         # Grafana dashboard JSON
├── docs/                            # 14 documentation files
├── scripts/                         # gate.ps1/sh, install-hooks, audit-api-changes
├── Cargo.toml                       # Workspace manifest (version.workspace = true)
├── audit.toml                       # cargo-audit config (7 ignored advisories)
├── deny.toml                        # cargo-deny config (14 allow licenses)
├── Dockerfile                       # Container image
└── docker-compose.yml               # Full-stack dev environment
```

## Quick Start

### 1. Add dependencies

```toml
[dependencies]
# From crates.io (recommended, after publish)
sz-orm-core = "1.0"
sz-orm-sqlx = "1.0"

# Local development (path)
# sz-orm-core = { version = "1.0", path = "packages/sz-orm-core" }
# sz-orm-sqlx = { version = "1.0", path = "packages/sz-orm-sqlx" }

tokio = { version = "1.40", features = ["full"] }
```

### 2. Define a Model

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

### 3. Build a query

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

### 4. Connect to a real database (sz-orm-sqlx)

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

For MySQL / PostgreSQL, swap in `MySqlPoolHandle` / `PgPoolHandle` and `SqlxMySqlConnectionFactory` / `SqlxPgConnectionFactory`.

### 5. Compile-time SQL check (sql_string!)

```rust
use sz_orm_core::sql_string;

let sql = sql_string!("SELECT * FROM users WHERE id = 1");         // OK
let sql = sql_string!("SELECT * FROM users WHERE id = ?"; params: 1); // OK — param count checked
// sql_string!("SELECT * FORM users");                              // Compile error: missing FROM
// sql_string!("SELECT * FROM users WHERE name = 'x' OR '1'='1'"); // Compile error: injection pattern
```

## Supported Databases

| Database | Dialect | Real connection | Default port |
|----------|---------|-----------------|--------------|
| MySQL | `MySqlDialect` (backtick) | sz-orm-sqlx | 3306 |
| PostgreSQL | `PostgreSqlDialect` (double quote) | sz-orm-sqlx | 5432 |
| SQLite 3.35+ | `SqliteDialect` | sz-orm-sqlx | — |
| Oracle 23ai | `OracleDialect` (`:N` placeholder + OFFSET/FETCH) | sz-orm-sqlx | 1521 |
| OceanBase | `MySqlDialect` compatible | — | 2881 |
| SQL Server | `MySqlDialect` compatible | — | 1433 |
| ClickHouse | `MySqlDialect` compatible | — | 8123 |
| Redis | NoSQL (no SQL dialect) | — | 6379 |
| MongoDB | NoSQL | — | 27017 |
| VectorDB | Vector database | sz-orm-vector | 19530 |
| PureJsDb | JS-engine DB | — | — |

Use `get_dialect(DbType::MySQL)` to get a dialect instance.

## Core API

### QueryBuilder chainable API

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
    .page(3, 20)                                 // page 3, 20 per page
    .join_inner("posts", "users.id", "posts.user_id")
    .join_left("profiles", "users.id", "profiles.user_id")
    .build_select();

// Aggregates
builder.build_count();
builder.build_exists();
builder.build_max("score");
builder.build_min("price");
builder.build_sum("amount");
builder.build_avg("value");

// Validation
builder.validate()?;              // SELECT validation
builder.validate_insert(&data)?;  // INSERT validation
builder.validate_update(&data)?;  // UPDATE validation
builder.validate_delete()?;       // DELETE validation
```

### Connection pool

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

### Transactions

```rust
use sz_orm_core::{Transaction, TransactOptions, IsolationLevel};

let opts = TransactOptions::default()
    .with_isolation(IsolationLevel::Serializable)
    .read_only()
    .with_timeout(Duration::from_secs(30));

let mut tx = Transaction::new(conn, opts);
tx.execute("INSERT INTO users VALUES (1)").await?;

// Savepoints (nested transactions)
let sp = tx.savepoint().await?;
tx.rollback_to_savepoint(&sp).await?;
tx.release_savepoint(&sp).await?;

tx.commit().await?;
// tx.rollback().await?;
```

### Migration system

```rust
use sz_orm_core::migration::{FileMigrationResolver, MigrationContext, Migrator, SchemaBuilder};
use sz_orm_core::{MigrationResolver, DbType};

// File migrations: <version>_<name>_up.sql / <version>_<name>_down.sql
let resolver = FileMigrationResolver::new("./migrations".into());
let migrations = resolver.resolve(DbType::MySQL)?;

let mut migrator = Migrator::new(MigrationContext::default())
    .add_migrations(migrations);

migrator.migrate().await?;                // apply all pending
migrator.up(Some("003")).await?;           // apply up to 003
migrator.down(Some("001")).await?;         // rollback to 001
migrator.rollback("002").await?;           // rollback single
migrator.reset().await?;                   // rollback all + re-apply
migrator.refresh().await?;                 // alias for reset
migrator.progress();                       // migration progress

// SchemaBuilder programmatic DDL
let sql = SchemaBuilder::new("users")
    .add_column(ColumnDef::new("id", "BIGINT").not_null().auto_increment())
    .add_column(ColumnDef::new("name", "VARCHAR").length(255).not_null())
    .add_index(IndexDef::new("idx_email", vec!["email"]).unique())
    .add_foreign_key(
        ForeignKeyDef::new("fk_role", "role_id", "roles", "id").on_delete("CASCADE")
    )
    .build(DbType::MySQL);
```

### Value type (20 variants)

```rust
use sz_orm_core::Value;

// Variants
Value::Null | Bool(bool) | I8 | I16 | I32 | I64 | U8 | U16 | U32 | U64
| F32 | F64 | String(String) | Bytes(Vec<u8>) | Uuid(String)
| Date(String) | DateTime(String) | Time(String) | Json(String)
| Array(Vec<Value>) | Object(HashMap<String, Value>)

// Conversions
value.as_str();    // Option<&str>
value.as_i64();    // Option<i64>
value.as_f64();    // Option<f64>
value.as_bool();   // Option<bool>
value.as_bytes();  // Option<&[u8]>

// From impls
let v: Value = 42i64.into();
let v: Value = "hello".into();
let v: Value = vec![1u8, 2u8].into();
```

## Advanced Modules (21 in sz-orm-core)

sz-orm-core ships 21 advanced modules beyond the base engine. See [Usage Guide §3.7](docs/sz-orm使用指南.md#37-sz-orm-core-高级特性模块) and [API Reference §2.22](docs/sz-ormAPI参考.md#22-高级特性模块-api-速查) for details.

| # | Module | Highlights |
|---|--------|-----------|
| 1 | `accessors` | Field accessors/mutators + type conversion |
| 2 | `behaviors` | Pluggable behaviors (TimestampBehavior / BlameableBehavior) |
| 3 | `data_permission` | Data permission interceptor (TenantIsolation / OwnerOnly / DepartmentScope) |
| 4 | `dirty_attributes` | Dirty field tracking (DirtyTracker + build_dynamic_update) |
| 5 | `dynamic_filter` | Runtime dynamic Filter (FilterRegistry) |
| 6 | `entity_graph` | Entity graph + batch loader (solves N+1) |
| 7 | `guard` | SQL safety guard (SafeSqlGuard + GuardPolicy::Strict) |
| 8 | `hydration_plugin` | Hydration + plugin chain (SqlLogPlugin / SlowQueryPlugin / AuditPlugin) |
| 9 | `join_dsl` | Type-safe JOIN DSL (JoinBuilder + 5 JoinKind) |
| 10 | `l2_cache` | L2 cache (LRU + TTL + table-level invalidation) |
| 11 | `lambda` | Lambda type-safe query (LambdaWrapper + define_columns! macro) |
| 12 | `observer` | Model lifecycle observer (9 events + EventDispatcher) |
| 13 | `optimistic_lock` | Optimistic lock (OptimisticLock trait + retry fn) |
| 14 | `phinx_migration` | Phinx-style schema builder (14 ColumnType + index + FK) |
| 15 | `queryable` | Diesel-style Queryable trait (from_row) |
| 16 | `quick_query` | Quick query via Db::name() (no Model needed) |
| 17 | `repository` | DDD repository pattern (Repository trait + InMemoryRepository + PageResult) |
| 18 | `result_map` | MyBatis ResultMap + Hibernate Native Query |
| 19 | `schema_gen` | Diesel-style schema.rs auto generation |
| 20 | `sql_safety` | SQL injection primitives (validate_identifier / validate_fk_action / validate_id_value) |
| 21 | `type_handler` | MyBatis-style TypeHandler SPI (DateTimeHandler / UuidHandler / ...) |

## Hook System (Soft Delete + Multi-Tenancy)

### HookContext — execution context

```rust
use sz_orm_core::hooks::HookContext;

let mut ctx = HookContext::new()
    .with_tenant(42)
    .with_operator(1)
    .with_timestamp(1700000000);
ctx.set_meta("source", "api");
```

### Hookable trait — 16 lifecycle hooks

```rust
use sz_orm_core::hooks::{Hookable, HookContext, HookResult};

impl Hookable for User {
    fn before_insert(_ctx: &mut HookContext) -> HookResult<()> { Ok(()) }
    fn after_insert(_ctx: &HookContext, _id: &Self::PrimaryKey) -> HookResult<()> { Ok(()) }
    fn before_update(_ctx: &mut HookContext, _id: &Self::PrimaryKey) -> HookResult<()> { Ok(()) }
    fn after_update(_ctx: &HookContext, _id: &Self::PrimaryKey) -> HookResult<()> { Ok(()) }
    fn before_delete(_ctx: &mut HookContext, _id: &Self::PrimaryKey) -> HookResult<()> { Ok(()) }
    fn after_delete(_ctx: &HookContext, _id: &Self::PrimaryKey) -> HookResult<()> { Ok(()) }
    // ... 10 more (before_save / after_save / before_write / after_write / before_validate / after_validate / before_restore / after_restore / before_find / after_find)
}
```

### SoftDelete + SoftDeleteScope

```rust
use sz_orm_core::hooks::{SoftDelete, SoftDeleteScope, GlobalScope};

impl SoftDelete for Product {
    fn soft_delete_field() -> &'static str { "deleted_at" }
    fn is_deleted(&self) -> bool { self.deleted_at.is_some() }
}

// Auto-appended on query: AND deleted_at IS NULL
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

// When ctx.tenant_id = Some(42): auto-appended AND tenant_id = ?
// When ctx.tenant_id = None: not appended (cross-tenant query, caller must ensure safety)
let scope = <(TenantScope, Order) as GlobalScope>::apply_scope(&ctx);
```

### HookRegistry — runtime hook registration

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

### ScopeRegistry — scope control

```rust
use sz_orm_core::hooks::ScopeRegistry;

let registry = ScopeRegistry::new();
registry.disable("soft_delete");       // disable soft-delete scope
registry.enable("soft_delete");        // re-enable
registry.is_enabled("soft_delete");    // true

// Temporarily disable (within closure)
let result = registry.without_scope("soft_delete", || {
    // queries here will include soft-deleted rows
    42
});
```

## CLI Tool

SZ-ORM ships a `sz-orm` CLI for migration management, code generation, and SQL validation.

### Install

```bash
cargo install --path cli
```

### Commands

```bash
sz-orm                              # show help
sz-orm info                         # show ORM summary
sz-orm --version                    # show version

sz-orm dialect list                 # list all dialects
sz-orm dialect show mysql           # show MySQL dialect details

sz-orm make:migration create_users  # generate migration skeleton
sz-orm make:model User              # generate Model skeleton

sz-orm migrate                      # show pending migrations
sz-orm migrate:status               # show migration progress

sz-orm sql:validate "SELECT * FROM users"  # SQL validation
```

### Options

- `--migrations <dir>` — migration file directory (default `./migrations`)
- `--output <dir>` — generated code output dir (default `./src/models` or `./migrations`)

## Examples

The `examples/` directory provides 8 runnable examples:

| Example | Description | Run |
|---------|-------------|-----|
| `quick_start` | QueryBuilder basics | `cargo run -p sz-orm-examples --bin quick_start` |
| `model_definition` | Model + ModelExt full impl | `cargo run -p sz-orm-examples --bin model_definition` |
| `transaction` | Transactions + savepoints | `cargo run -p sz-orm-examples --bin transaction` |
| `migration` | SchemaBuilder DDL | `cargo run -p sz-orm-examples --bin migration` |
| `hooks_soft_delete` | Hooks + soft delete | `cargo run -p sz-orm-examples --bin hooks_soft_delete` |
| `multi_tenant` | Multi-tenant isolation | `cargo run -p sz-orm-examples --bin multi_tenant` |
| `production_app` | Production app pattern | `cargo run -p sz-orm-examples --bin production_app` |
| `production_dtx` | Distributed tx pattern | `cargo run -p sz-orm-examples --bin production_dtx` |

## Testing

SZ-ORM enforces quality via a **7-line verification system**:

| Method | Description | Test file |
|--------|-------------|-----------|
| **TDD** | Core unit tests | `core.rs` |
| **Integration** | Real MySQL/PG/SQLite/Oracle E2E | `integration_*.rs` |
| **Jepsen** | Concurrent correctness + real-DB Jepsen | `jepsen.rs`, `real_db_jepsen.rs` |
| **Fuzz** | Boundary/edge cases | `fuzz.rs` |
| **Stress** | Performance/stress | `stress.rs`, `core_bench.rs` |
| **Chaos** | Fault robustness | `chaos.rs` |
| **Formal** | Formal invariant verification | `formal.rs` |

**Total: 2950 tests, 0 failed, 79 ignored (require real DB/cloud credentials)**

### Run tests

```bash
# Full workspace tests
cargo test --workspace

# Core package only
cargo test -p sz-orm-core

# Real-DB tests (requires MySQL/PG/SQLite running)
cargo test -p sz-orm-core --features testing

# Performance benchmarks
cargo bench -p sz-orm-core

# 24h soak test
SOAK_DURATION=24h cargo test -p sz-orm-core --test soak -- --ignored
```

## Build & Documentation

### Build

```bash
# Full workspace
cargo build --workspace

# Core package only
cargo build -p sz-orm-core

# Release build
cargo build --workspace --release
```

### Docs

```bash
# Generate docs
cargo doc --workspace --no-deps --open

# Lint
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

## Security Audit

SZ-ORM enforces security via CI-integrated `cargo-audit` and `cargo-deny`:

```bash
# Vulnerability scan
cargo audit \
            --ignore RUSTSEC-2026-0049 \
            --ignore RUSTSEC-2026-0098 \
            --ignore RUSTSEC-2026-0099 \
            --ignore RUSTSEC-2026-0104 \
            --ignore RUSTSEC-2026-0194 \
            --ignore RUSTSEC-2026-0195 \
            --ignore RUSTSEC-2025-0134

# Comprehensive check (advisories + bans + licenses + sources)
cargo deny check advisories bans licenses sources
```

**Result (2026-07-21)**:
- ✅ `cargo audit`: 0 unignored vulnerabilities (7 transitive deps ignored with documented reasons)
- ✅ `cargo deny`: advisories ok / bans ok / licenses ok / sources ok
- License whitelist: 14 permissive licenses (MIT / Apache-2.0 / BSD / ISC / Zlib / CC0-1.0 / MPL-2.0 / ...)
- Sources: only crates.io official registry allowed; no git/path sources
- CI: `.github/workflows/security.yml` runs on every push/PR to main/master

## Performance Benchmarks

 criterion benchmarks (sample_size=10, measurement_time=3s, warm_up=1s, Windows):

| Benchmark | Result |
|-----------|--------|
| `value_to_param/null` | 3.2 ns (312 Melem/s) |
| `value_to_param/i64` | 53.4 ns (18.7 Melem/s) |
| `value_to_param/string_short` | 252 ns (3.97 Melem/s) |
| `dialect_escape_string/long_1024` | 954 ns (1.02 GiB/s) |
| `dialect_build_create_table/100 cols` | 31.7 µs (3.15 Melem/s) |
| `dialect_build_pagination/1M page` | 163 ns (stable across page depth) |
| `pool_acquire_release` | 230 ns per round-trip |
| `in_memory_scan/select_where_eq_1pct/100K` | 4.87 ms (20.5 Melem/s) |
| `json_parsing/3kb` | 85.0 µs (71 MiB/s) |

**Real-DB batch INSERT throughput** (100K rows):

| Database | Throughput | Relative |
|----------|-----------|----------|
| SQLite (file) | 720K rows/s | 4.97× |
| PostgreSQL 18 | 268K rows/s | 1.85× |
| MySQL 9.6 | 145K rows/s | 1.0× (baseline) |
| Oracle 23ai Free | 19.1K rows/s | 0.13× |

**1h Soak Test**: 1.38B operations, 1.16% throughput decay, P99 43μs→41μs, 0 errors.

See [Performance Benchmark Report](docs/sz-orm性能基准.md) for full details.

## Project Documentation

| Document | Description |
|----------|-------------|
| [Usage Guide](docs/sz-orm使用指南.md) | End-to-end usage guide (v5.0, covers all 21 advanced modules) |
| [API Reference](docs/sz-ormAPI参考.md) | Type signatures and parameter docs (v5.0) |
| [Performance Benchmark](docs/sz-orm性能基准.md) | criterion + real-DB + soak test (v4.0) |
| [Maturity Assessment](docs/sz-orm项目成熟度评估报告.md) | 7-dimension maturity scoring (4.98/5) |
| [Production Readiness](docs/sz-orm生产就绪报告.md) | Production deployment checklist |
| [Architecture Design](docs/sz-orm架构设计.md) | 39-package architecture overview |
| [Implementation Progress](docs/sz-orm项目实施进度表.md) | Milestones and stage tracking |
| [Engineering Practices](docs/sz-orm-engineering-practices.md) | Gate 1-10 + test pyramid T1-T6 |
| [Comparison with Mainstream ORMs](docs/SZ-ORM%20与主流%20ORM%20对比.md) | Diesel / SeaORM / SQLx comparison |
| [Technical Deep Dive](docs/sz-orm技术实现深度评估.md) | Design specs and architecture decisions |
| [Security](docs/Security.md) | Security baseline and threat model |
| [API Contracts](docs/api-contracts.md) | Public API stability contracts |
| [Comprehensive Review](docs/sz-orm全面审查报告v1.md) | Multi-dimensional code review |

## License

MIT License © SZ-ORM Team
