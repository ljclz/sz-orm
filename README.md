# SZ-ORM — Xianshida ORM

> **Rust asynchronous ORM workspace (production ready)**, ThinkORM-style API compatible
> v10.13.0 · 72 workspace members · 21274+ tests · 28 dialects (21 by default, 7 feature-gated) · sz-orm-core published on crates.io

[![Rust](https://img.shields.io/badge/rust-1.81.0+-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Tests](https://img.shields.io/badge/tests-21274+-green.svg)](#tests)
[![Dialects](https://img.shields.io/badge/dialects-28-red.svg)](#supported-databases)
[![Packages](https://img.shields.io/badge/packages-72-purple.svg)](#workspace-structure)
[![Version](https://img.shields.io/badge/version-10.13.0-blue.svg)](CHANGELOG.md)
[![Maturity](https://img.shields.io/badge/maturity-production--ready-brightgreen.svg)](#overview)
[![Security](https://img.shields.io/badge/security-audit%2Fdeny-brightgreen.svg)](#security-audit)
[![Coverage](https://img.shields.io/codecov/c/github/ljclz/sz-orm)](https://codecov.io/gh/ljclz/sz-orm)

[中文版](README.zh.md) · [Usage Guide](docs/sz-orm使用指南.md) · [API Reference](docs/sz-ormAPI参考.md)

---

## Table of Contents

- [Overview](#overview)
- [Core Features](#core-features)
- [Quality Baseline](#quality-baseline)
- [Workspace Structure](#workspace-structure)
- [Quick Start](#quick-start)
- [Supported Databases](#supported-databases)
- [Core API](#core-api)
- [Advanced Modules (21)](#advanced-modules-21)
- [Hook System (Soft Delete + Multi-Tenant)](#hook-system-soft-delete--multi-tenant)
- [CLI Tool](#cli-tool)
- [Examples](#examples)
- [Tests](#tests)
- [Build & Documentation](#build--documentation)
- [Security Audit](#security-audit)
- [Performance Benchmarks](#performance-benchmarks)
- [Documentation Index](#documentation-index)
- [License](#license)

---

## Overview

SZ-ORM is a pure Rust asynchronous ORM workspace, aiming to provide a full-featured database access layer for the Rust ecosystem. v10.13.0 includes 72 workspace members (70 sz-orm-* libs + cli + examples), covering ORM core engine, real database adapters, AI vector search, distributed transactions, observability, and other full-stack capabilities, with 509 feature gates for modular composition.

### v5.1.0 AI Application Optimization (34 tasks, 293 new tests)

| Category | Package | Description |
|----------|---------|------|
| Performance Prediction | sz-orm-ai | `PerformancePredictor`: query cost prediction based on table statistics + query characteristics |
| A/B Testing | sz-orm-ai | `QueryABTestFramework`: Welch t-test statistical comparison of query plan variants |
| Failure Prediction | sz-orm-diagnosis | `FailurePredictor`: anomaly detection with configurable thresholds + alert generation |
| Trend Prediction | sz-orm-adaptive | `TrendPredictor`: linear regression / exponential smoothing / moving average |
| Injection Pattern Learning | sz-orm-ai | `InjectionPatternStore::load_patterns/save_patterns`: persistent pattern file I/O |
| Permission Audit | sz-orm-ai | `PermissionAuditor`: DB account permission audit + least-privilege recommendations |
| Semantic Query Routing | sz-orm-ai | `SemanticQueryRouter`: intent analysis → route to SQL / graph / vector / hybrid |
| Graph Query Integration | sz-orm-ai | `GraphQueryExecutor` trait: integrate sz-orm-graph for relationship traversal |
| AI Agent | sz-orm-ai | `AiAgent` trait + `AnalysisAgent`: multi-step reasoning with tool use |
| Hybrid Query | sz-orm-ai | `HybridQueryExecutor`: combine SQL + vector + graph results with RRF fusion |
| Web GUI Studio | sz-orm-studio | `WebGuiServer`: axum-based data browser (table list / data edit / relations) |
| Entity Generator | sz-orm-cli | `EntityGenerator`: DB schema → Rust entity code (struct + relations + derive macros) |
| Doc Generator | sz-orm-cli | `DocGenerator`: DB schema → Markdown docs + PlantUML ER diagrams |
| LSP Server | sz-orm-lsp | `LspServer`: completion / hover / diagnostics for sz-orm query DSL |
| Plugin System | sz-orm-core | `SzOrmPlugin` trait + `PluginRegistry`: AI / dialect / middleware extensions |

### v4.1.0 New Capabilities (9 feature gates, off by default)

| Feature | Package | Description |
|---------|---------|------|
| `data-seeding` | sz-orm-core | Data seeding/fixture management: FakerGenerator + dependency topological sort + idempotent execution |
| `schema-diff-viz` | sz-orm-core | Schema diff visualization: text/json/html formats + breaking change annotations |
| `cache-coherence` | sz-orm-core | Cache coherence protocol: MESI state machine + invalidation broadcast + write-through/behind |
| `message-tracing` | sz-orm-queue | Message tracing: sampling rate control + desensitization + end-to-end correlation |
| `storage-lifecycle` | sz-orm-storage | Storage lifecycle management: tiering strategy + expiry cleanup + policy engine |
| `data-quality` | sz-orm-audit | Data quality auto-detection: six statistical rule types + quality report |
| `batch-stream` | sz-orm-batch | Batch streaming: backpressure control + window aggregation + parallelism control |
| `migration-branch` | sz-orm-core | Migration version branching: multi-branch parallel development + merge conflict detection |
| `backup-verify` | sz-orm-back | Backup verification automation: integrity check + recovery drill + verification report |

### v4.0.0 New Capabilities (9 feature gates, off by default)

| Feature | Package | Description |
|---------|---------|------|
| `multi-llm` | sz-orm-ai | Multi-LLM support (OpenAI/Claude/Gemini/Ollama), hot-swap + load balancing |
| `ai-auto-tuning` | sz-orm-ai | AI auto-tuning loop: detect→suggest→verify→apply→regress |
| `hybrid-search` | sz-orm-vector | Hybrid search: vector + full-text + structured, RRF fusion |
| `data-lineage` | sz-orm-audit | Data lineage tracking: SQL AST parsing + DAG graph + multi-format export |
| `shard-rebalance` | sz-orm-sharding | Shard auto-rebalance: load balancing + checkpoint + atomic migration |
| `auto-failover` | sz-orm-rw | Database auto failover: primary-standby switch + split-brain detection |
| `cdc` | sz-orm-queue | CDC (Change Data Capture): **polling capturer (real implementation, `PollingCapturer`) + exactly-once dedup + multi-sink**; protocol-level capture (PostgreSQL WAL / MySQL binlog / Oracle LogMiner / MSSQL CDC) explicitly not implemented (requires real DB replication protocol, returns explicit error, does not pretend success), see audit report §2-P4 |
| `async-graphql-integration` | sz-orm-graphql | GraphQL deep integration: DataLoader + Relay + Federation |
| `service-mesh` | sz-orm-observability | Service mesh integration: Istio/Linkerd config generation + observability |

> **⚠️ Honesty disclaimer**: This project is a single-author engineering practice project, **early production ready (internal project)**. The sz-pay project uses sz-orm-core/sqlx/config/auth/macros/queue/scheduler 7 packages in production (297 references, 5139 tests zero regression). For in-depth comparison with Diesel/SeaORM/SQLx, see [docs/sz-orm与同类产品对比分析.md](docs/sz-orm与同类产品对比分析.md).

> **📌 Production readiness positioning (2026-10-16 architecture review, gate count resynced 2026-10-28)**: This workspace is **early production ready for internal pilots** — the engineering quality system (34 gates, 94.4% line coverage, evidence-based auditing) is production-grade, but production adoption is currently limited to the **sz-pay internal pilot** (7 packages). Known limitations to review before external adoption: ① mutation-testing gate (G20) full 2352-mutant subset requires CI/Linux runtime and is not yet a daily gate; ② `--all-features` combination compile needs periodic verification (see `scripts/check-all-features.sh`); ③ feature-gated transitive dependency chain (e.g. rustls 0.21 via `real-es` feature) is tracked in `deny.toml` with ignore + upgrade plan; ④ cross-package mutation coverage (core logic tested from `sz-orm-sqlx`) requires `--test-package` wiring (see `scripts/collect-mutation-diff.sh`).

### Production Readiness Check (v3.8.0, config-driven since v10.13.0)

Enable the `prod-ready` feature to use `ProdReadyChecker` for 15 production readiness checks.
**The checker validates the production configuration you provide** — fields left as `None`
are reported as `Skipped` (manual integration required), not auto-passed:

```rust
use sz_orm_core::prod_ready_check::{ProdReadyChecker, ProdReadyCheckerConfig};

let config = ProdReadyCheckerConfig {
    config_masking_enabled: Some(true),
    redis_tls_enabled: Some(true),
    jwt_key_rotation_days: Some(30),       // 合法窗口 1..=90
    log_level: Some("warn".to_string()),   // debug/trace 生产不合格
    shutdown_timeout_secs: Some(30),       // 合法窗口 1..=3600
    pool_max_connections: Some(50),
    pool_min_idle: Some(5),
    sql_injection_scan_passed: Some(true), // 由门禁 9 扫描结果喂入
    ..Default::default()
};
let report = ProdReadyChecker::new(config).run();
println!("{}", report.to_json().unwrap());
// report.all_pass_strict(): 无 FAIL 且无 SKIPPED —— 全部检查真实执行且通过
```

| Feature | Description |
|---------|------|
| `prod-ready` | Aggregates all 14 sub-features |
| `prod-dialect-security` | Five-dialect TLS/auth/desensitization verification |
| `prod-n1-tuning` | N+1 detection window/interception tuning |
| `prod-leak-detection` | Connection leak detection config |
| `prod-pool-tuning` | Connection pool parameter validation |
| ... | 14 sub-features total |

| Dimension | Data |
|------|------|
| Workspace members | **72** (70 sz-orm-* libs + cli + examples) |
| Supported DB dialects | **28 dialects** — 21 compiled by default, 7 behind `dialect-*` features (at `--all-features`: 24 SQL + 4 NoSQL); dialect layer = SQL generation, not a connection driver (drivers: sqlx MySQL/PostgreSQL/SQLite + `sz-orm-oracle` + `sz-orm-mssql`) |
| Test cases | **21274 passed, 0 failed** |
| Code size | **~639,200 LOC** (src ~504,900 + tests ~131,400 + benches ~2,700 + examples ~200) |
| Project maturity | **Early production ready (internal project)** (sz-pay production pilot, crates.io has published sz-orm-core) |
| Async runtime | Tokio 1.40+ |
| Minimum Rust version | 1.81.0+ (Cargo.toml rust-version = "1.81") |
| sqlx version | 0.9.0 |
| Known bugs | **0** |
| `panic!`/`unimplemented!`/`todo!`/`unreachable!` | **0** (production code) |
| `cargo clippy -D warnings` | ✅ 0 warnings (`[workspace.lints]` enforced) |

## Historical: v3.4.0 New Features (2026-08-09)

> **Note**: This section describes the v3.4.0 release features. For the latest features (v10.13.0), see [CHANGELOG.md](CHANGELOG.md). Quality deepening release: test coverage backfill + architecture improvements + performance optimization + compile-time type safety + documentation ecosystem + sz-pay production case deepening. 10 feature gates all off by default, no breaking changes. 44 main tasks / 160 subtasks all completed, five-dialect integration tests 83 all passed.

### Test Coverage Backfill (`test-coverage`)

- 18 extension package tests from 0 → full coverage (≥ 5 tests per package)
- All 159 workspace test suites passed
- Five-dialect integration tests: MySQL 23 + PostgreSQL 18 + SQLite 25 + Oracle 10 + DuckDB 7 = 83 all passed

### Architecture Improvements (`arch-improvement`)

- `async_trait_style_evaluation.md`: async trait style evaluation (dyn Trait vs async-trait vs impl Trait)
- `query_builder_selection_guide.md`: QueryBuilder selection guide
- `result_map_macro_evaluation.md`: result_map macro generation evaluation

### Performance Optimization (`perf-smallstring` / `perf-enum-dispatch` / `perf-zero-copy-l2` / `perf-box-str`)

```toml
sz-orm-core = { version = "3.4", features = ["perf-smallstring", "perf-enum-dispatch", "perf-zero-copy-l2", "perf-box-str"] }
```

- `SqlBuffer`: CompactString/String dual backend, short strings ≤ 23 bytes inline storage
- `DialectKind` enum dispatch: replaces `Box<dyn Dialect>` vtable lookup
- `Value::BoxedStr(Box<str>)`: saves 8 bytes/value capacity field
- L2 cache zero-copy promotion: BorrowedValue + ColumnarResultSet extended to L2 cache path
- 4 benchmarks + 16 differential tests

### Compile-Time Type Safety (`type-safe-columns` / `typed-column` / `typed-dsl`)

```toml
sz-orm-core = { version = "3.4", features = ["type-safe-columns", "typed-column", "typed-dsl"] }
```

- `Column<T: Schema>`: type-safe column reference, compile-time column name typo detection
- `Schema` trait + `#[derive(Schema)]`: auto-generate column name constants
- `typed_ast` extension: `Like`/`In`/`Not` expressions + `BoolExpressionExt` trait
- `where_eq_col` / `where_expr`: type-safe WHERE condition building
- 30 tests + 1 benchmark

### Documentation Ecosystem (`migration-guide`)

- `docs/migration/diesel_to_sz_orm.md`: Diesel → SZ-ORM migration guide
- `docs/migration/seaorm_to_sz_orm.md`: SeaORM → SZ-ORM migration guide
- `docs/migration/sqlx_to_sz_orm.md`: SQLx → SZ-ORM migration guide

### sz-pay Production Case Deepening

- `examples/src/bin/sz_pay_pattern.rs`: desensitized production usage pattern example
- sz-pay project 6 test suites zero-regression verification passed

### Compatibility

- No breaking changes, default feature zero behavior change
- Five-dialect integration tests 83 all passed (MySQL/PostgreSQL/SQLite/Oracle/DuckDB)
- clippy zero warnings (including all v3.4.0 features)
- workspace version unified to 3.4.0

See [CHANGELOG.md](CHANGELOG.md).

## v3.3.0 New Features (2026-08-08)

> Enterprise data governance release: multi-tenant data isolation + distributed cache coherence + GraphQL query support + AI natural language query enhancement. 8 feature gates all off by default, no breaking changes. 22 EARS requirements all satisfied.

### Multi-Tenant and Data Isolation Enhancement (`multi-tenant-enhanced`)

```toml
sz-orm-core = { version = "2.3", features = ["multi-tenant-enhanced"] }
```

- `TenantContext` + RAII guard + `scope()` async scope
- `SchemaIsolationRouter` table name rewrite (`users` → `tenant_42_users`)
- `RowLevelSecurityPolicy` + `ColumnMaskingRule` row-level security + column masking
- `TenantAuditContext` multi-tenant audit log

```rust
use sz_orm_core::tenant_context::{TenantContext, IsolationStrategy};

TenantContext::scope(42, IsolationStrategy::Schema, async {
    // All queries auto-inject tenant_id = 42 + table name rewrite
    query.table("users").where_eq("status", "active").fetch_all().await
}).await?;
```

### Distributed Cache Coherence (`dist-cache`)

```toml
sz-orm-core = { version = "2.3", features = ["dist-cache"] }
```

- `ConsistencyLevel` (Strong / Eventual) optional consistency level
- `RedisPubSubInvalidationBus` Redis Pub/Sub cross-instance invalidation (HMAC auth)
- `GossipInvalidationBus` Gossip protocol invalidation (≤10 instances 1s convergence)
- `WriteBehindQueue` + `WalFile` async batch write + WAL persistence
- `BloomFilterGuard` + `CacheMutexGuard` + `RandomTtlJitter` penetration/stampede protection

### GraphQL Query Support (`graphql-n1` / `graphql-schema-gen` / `graphql-complexity`)

```toml
sz-orm-graphql = { version = "2.3", features = ["graphql-n1", "graphql-schema-gen", "graphql-complexity"] }
```

- `GraphQLIR` recursive descent parser
- `DataLoader<K, V>` N+1 auto-elimination (query count ≤ 2, reduction ≥ 90%)
- `SchemaGenerator` Rust model → GraphQL Schema auto-generation
- `#[derive(GraphQLModel)]` procedural macro
- `ComplexityCalculator` query complexity limit (depth/field count/cost)

### AI Natural Language Query Enhancement (`ai-nl2sql-enhanced` / `ai-index-advisor` / `ai-rewrite-advisor`)

```toml
sz-orm-ai = { version = "2.4", features = ["ai-nl2sql-enhanced", "ai-index-advisor", "ai-rewrite-advisor"] }
```

- `IntentAnalyzer` query intent analysis + risk flagging
- `IndexAdvisor` auto index advice + benefit estimation
- `RewriteAdvisor` query rewrite advice + equivalence argument
- `AiAdviceAuditRecord` AI advice audit record
- NL2SQL LLM prompt enhancement + `SqlSanitizer` desensitization
- **Zero database execution guarantee** (advisory only, no auto-execution)

### Compatibility

- No breaking changes, default feature zero behavior change
- Downstream sz-pay project 5139 tests zero-regression verification passed
- clippy zero warnings (including all v3.3.0 features)
- sz-pay production usage pattern example: `cargo run --bin sz_pay_pattern` (desensitized, demonstrates connection pool/SQL execution/error mapping/soft delete typical usage)

See [CHANGELOG.md](CHANGELOG.md) and [Upgrade Guide](docs/v3.3.0-upgrade-guide.md).

## v1.5.0 New Features (2026-08-05)

### Connection Pool Prometheus Metrics

- **`PoolMetrics`**: `Pool::pool_metrics()` returns pool lifetime cumulative metrics (acquire_count / acquire_failed_count / acquire_wait_time / release_count / connection_created_count / connection_closed_count)
- **`average_acquire_wait_time()`**: average acquire wait time (`acquire_wait_time / acquire_count`)
- Based on lock-free `AtomicU64` atomic counters, overhead on acquire/release hot path is negligible; 4 unit tests verify counter correctness

### ClickHouse Row Lock Support

- `ClickHouseDialect::supports_lock_for_update()` / `supports_lock_shared()` explicitly return `false` (columnar OLAP, no transactions, no row locks)
- `build_insert_or_ignore_prefix()` falls back to plain `INSERT INTO` (ClickHouse does not support `INSERT OR IGNORE`)

### SQL Server INSERT OR IGNORE Fallback

- `SqlServerDialect::build_insert_or_ignore_prefix()` falls back to plain `INSERT INTO` (SQL Server has no equivalent prefix syntax; MERGE / IF NOT EXISTS cannot be expressed as a "prefix")
- Application layer can use unique index + catch duplicate key conflict (SQLSTATE 2601/2627) or MERGE for idempotent insert

### DuckDB Real Integration Tests

- `packages/sz-orm-core/tests/integration_duckdb.rs`: 7 real DB tests (create table, parameterized insert/query, INSERT OR IGNORE, pagination, ALTER TABLE, escaping, DROP TABLE), using `duckdb` bundled feature (compilation requires MSVC + CMake)

### Vector / Time-Series Real Implementation Integration Tests

- `sz-orm-vector`: 3 `#[ignore]` real PostgreSQL + pgvector tests (create/insert/search workflow, upsert overwrite, Euclidean distance sorting)
- `sz-orm-timeseries`: 5 in-memory integration tests + 2 `#[ignore]` real TimescaleDB tests (hypertable creation + time bucket aggregation)

### crates.io Publication

- sz-orm-core **1.5.0** ✅ (2026-08-05)

## v1.4.0 New Features (2026-08-05)

### Lock Queries (TASK-024~027)

- **`lock_for_update()`**: pessimistic lock, `SELECT ... FOR UPDATE`, supports MySQL / PostgreSQL / SQLite (row lock)
- **`lock_shared()`**: shared lock, `SELECT ... FOR SHARE` (PostgreSQL) / `LOCK IN SHARE MODE` (MySQL)
- **`LockType` enum**: `Update` / `Shared`, generates dialect-specific SQL via `Dialect` trait's `lock_clause()` method
- 15 unit tests + 3 MySQL integration tests + 1 benchmark test

### INSERT OR IGNORE (TASK-028~029)

- **`insert_or_ignore()`**: insert ignoring unique key conflicts, supports MySQL (`INSERT IGNORE`) / PostgreSQL (`ON CONFLICT DO NOTHING`) / SQLite (`INSERT OR IGNORE`) / DuckDB (`INSERT OR IGNORE`)
- 2 MySQL integration tests + 1 benchmark test

### DuckDB Dialect Support (TASK-033~036)

- **`DuckDBDialect`**: full Dialect trait implementation, supports CREATE TABLE / ALTER TABLE / index / INSERT OR IGNORE
- **`DbType::DuckDB`**: new enum variant, `as_str()` / `from_str()` / `default_port()` all supported
- 10 unit tests covering table creation, table alteration, type mapping

### Redis Distributed Cache Backend Enabled by Default

- **`RedisBackend`** (Fix #39): based on redis 0.27 + `tokio-comp` + `connection-manager` (auto-reconnect connection pool)
- Supports `GET` / `SET EX` / `DEL` / `SCAN` + pipeline batch delete (avoids `KEYS` blocking main thread)
- **Enabled by default**: `redis` feature added to `default`, `RedisBackend::new("redis://127.0.0.1:6379/0")` works out of the box
- Added 1 unit test (invalid URL error path) + 4 real integration tests (`--ignored`, requires local Redis service)

### crates.io Publication

- sz-orm-sql-validator 1.4.0 ✅
- sz-orm-macros 1.4.0 ✅
- sz-orm-core 1.4.0 ✅
- sz-orm-core 1.5.0 ✅ (connection pool metrics + ClickHouse row lock + SQL Server INSERT OR IGNORE fallback)

## v1.3.0 New Features (2026-08-05)

### Performance Optimization

- **Connection pool prewarm** (TASK-021): `PoolConfig::prewarm` enabled, pool creates `min_idle` connections immediately at creation, first `acquire()` latency reduced from < 100ms to < 10ms
- **Query cache TTL** (TASK-022): `QueryBuilder::cache_ttl(Duration)` reserved query result cache TTL setting (⚠️ 2026-08-13 erratum: execution path does not yet consume this TTL, currently dead API, see [audit report §2-5](docs/assessment/2026-08-13-production-zero-call-audit.md))

### API Enhancement

- **deprecated method removal** (TASK-010~012): removed `QueryBuilder::where_cond` / `or_where` and other string concatenation methods, enforcing parameterized queries (`where_eq` / `where_ne` / `where_gt` etc.), eliminating SQL injection
- **PoolConfigBuilder::prewarm**: chained builder method, supports `PoolConfigBuilder::new().prewarm(true).min_idle(5).build()`

### Quality Improvements

- **Test coverage**: added 3 prewarm tests (`test_pool_prewarm` / `test_pool_prewarm_failure_non_blocking` / `test_pool_prewarm_disabled`), verify prewarm logic correctness
- **Documentation**: all public APIs supplemented with `///` doc comments and examples, `cargo doc` zero warnings

### crates.io Publication

- sz-orm-sql-validator 1.3.0 ✅
- sz-orm-macros 1.3.0 ✅
- sz-orm-core 1.3.0 ✅

## Core Features

- **Asynchronous**: based on Tokio, full `async/await`
- **Multi-database dialects**: 28 database dialects (24 SQL: MySQL/PostgreSQL/SQLite/Oracle/SQL Server/ClickHouse/DB2/DuckDB/MariaDB/TiDB/OceanBase/Dameng/Kingbase/PolarDB/GaussDB/GBase/Sybase/CockroachDB/YugabyteDB/Snowflake/Redshift/Informix/SAP HANA/Firebird + 4 NoSQL: Redis/MongoDB/VectorDB/PureJsDb)
- **Chained QueryBuilder**: ThinkORM-style fluent API
- **ACID transactions**: isolation levels, savepoints (default 8 levels nesting, `DEFAULT_MAX_NESTING_DEPTH = 8`, configurable), `TransactionManager` multi-transaction management
- **Connection pool**: configurable size, timeout, idle reaping, health check, max lifetime
- **Migration system**: up/down/rollback/reset/refresh + `SchemaBuilder` programmatic table creation
- **Multi-level cache**: `MemoryCache` / `MultiLevelCache` / `L2Cache`, supports TTL and table-level invalidation (⚠️ 2026-08-13 erratum: component available, query execution path does not auto-integrate cache, see [audit report §2-8](docs/assessment/2026-08-13-production-zero-call-audit.md))
- **Hook system**: 16 lifecycle events + `HookDispatcher` + `HookRegistry` runtime hooks (⚠️ 2026-08-13 erratum: requires manual `registry.dispatch`, CRUD execution does not auto-trigger, see [audit report §2-7](docs/assessment/2026-08-13-production-zero-call-audit.md))
- **Soft delete**: `SoftDelete` trait + `SoftDeleteScope` global scope
- **Multi-tenant**: `TenantModel` trait + `TenantScope` auto `tenant_id = ?` filtering
- **SQL validation**: compile-time (`sql_string!`) + runtime (`validate()`) dual validation, 10 injection pattern detection (5 regex patterns + 5 AST patterns)
- **Relations**: BelongsTo / HasMany / HasOne / BelongsToMany + Eager Loading + `find_with_related`
- **21 advanced modules**: accessors/behaviors/data_permission/dirty_attributes/dynamic_filter/entity_graph/guard/hydration_plugin/join_dsl/l2_cache/lambda/observer/optimistic_lock/phinx_migration/queryable/quick_query/repository/result_map/schema_gen/sql_safety/type_handler
- **Distributed transactions**: 2PC + TCC (Try-Confirm-Cancel) + Saga + cross-shard ACID coordinator
- **AI vector + pgvector**: sz-orm-vector (cosine/euclidean/dot three metrics) + sz-orm-ai (NL→SQL + RAG + Embedding)
- **Observability**: sz-orm-observability (Prometheus exporter + OTLP + SLO monitoring) + sz-orm-tracing (OpenTelemetry traceparent propagation)
- **Extension ecosystem**: encryption, JWT, scheduling, MQTT, WebSocket, message queue (7 types, RocketMQ is stub), object storage (7 types, 6 are in-memory mock), gRPC, GraphQL, ES, Swagger, desensitization, health check, audit, batch, WASM, backup, read-write splitting, sharding, rate limiting, migration, PostGIS, TimescaleDB, search (ES/Meilisearch/OpenSearch)

## Quality Baseline

- 7-line verification system: TDD + Integration + Jepsen + Fuzz + Stress + Chaos + Formal
- 0 `panic!` / 0 `unimplemented!` / 0 `todo!` / 0 `unreachable!` (production code)
- `[workspace.lints]` enforces `clippy::all` 0 warnings, compile-time quality gate
- `cargo clippy --workspace --all-targets -- -D warnings` passed, 0 warnings
- `cargo fmt --all --check` passed
- `cargo audit` — 0 unignored vulnerabilities (7 transitive dependency ignore items, all documented)
- `cargo deny check advisories bans licenses sources` — all OK
- 1-hour Soak Test: 1.38 billion operations, 1.16% throughput decay, P99 43μs→41μs, 0 errors, no connection pool leak (reproduce: `SOAK_DURATION=1h cargo test -p sz-orm-core --test soak -- --ignored --nocapture`)

## Workspace Structure

```
sz-orm/
├── packages/
│   ├── sz-orm-core/                 # Core engine (Model/Query/Dialect/Pool/Tx/Migration/Cache/Hooks + 21 advanced modules)
│   ├── sz-orm-sqlx/                 # sqlx real database adapter (MySQL/PG/SQLite)
│   ├── sz-orm-sql-validator/        # SQL syntax and injection validation
│   ├── sz-orm-macros/               # Derive macros + sql_string! compile-time validation
│   ├── sz-orm-query-builder/        # quote_ident + check_where_injection
│   ├── sz-orm-observability/        # MetricsRegistry + Counter/Gauge/Histogram + SloMonitor
│   ├── sz-orm-tracing/              # OpenTelemetry OTLP exporter
│   ├── sz-orm-vector/               # pgvector integration (cosine/euclidean/dot)
│   ├── sz-orm-ai/                   # NL→SQL + Embedding + RAG
│   │
│   ├── sz-orm-crypto/               # AES-256-GCM / PBKDF2 / HMAC
│   ├── sz-orm-auth/                 # JWT auth
│   ├── sz-orm-scheduler/            # Cron scheduled tasks
│   ├── sz-orm-mqtt/                 # MQTT client (rumqttc)
│   ├── sz-orm-websocket/            # WebSocket service
│   ├── sz-orm-queue/                # RabbitMQ/Kafka/NATS/ActiveMQ/RocketMQ/Pulsar
│   ├── sz-orm-storage/              # S3/Aliyun/Tencent/Huawei/Qiniu/Upyun/Local
│   ├── sz-orm-grpc/                 # gRPC (tonic)
│   ├── sz-orm-graphql/              # GraphQL (async-graphql + axum)
│   ├── sz-orm-postgis/              # PostGIS geometry
│   ├── sz-orm-timeseries/           # TimescaleDB
│   ├── sz-orm-search/               # Elasticsearch/OpenSearch/Meilisearch
│   ├── sz-orm-es/                   # Elasticsearch legacy
│   ├── sz-orm-logger/               # Structured logging
│   ├── sz-orm-swagger/              # OpenAPI doc generation
│   ├── sz-orm-masking/              # Data desensitization
│   ├── sz-orm-health/               # Health check
│   ├── sz-orm-audit/                # Audit log
│   ├── sz-orm-batch/                # Batch operations
│   ├── sz-orm-dtx/                  # Distributed transactions (2PC/TCC/Saga)
│   ├── sz-orm-rw/                   # Read-write splitting
│   ├── sz-orm-sharding/             # Sharding
│   ├── sz-orm-limit/                # Rate limiting
│   ├── sz-orm-config/               # Config management
│   ├── sz-orm-mig/                  # Data migration transformer
│   ├── sz-orm-wasm/                 # WebAssembly target
│   ├── sz-orm-lc/                   # Local/edge computing
│   └── sz-orm-back/                 # Backup and recovery
│
├── cli/                             # CLI tool (sz-orm)
├── examples/                        # 38 runnable examples
├── grafana/                         # Grafana dashboard JSON
├── docs/                            # Docs center (quickstart / usage guide / API reference / migration guides / assessments + adr/ subdir with 11 ADRs)
├── scripts/                         # gate.ps1/sh, install-hooks, audit-api-changes
├── Cargo.toml                       # Workspace manifest (version.workspace = true)
├── audit.toml                       # cargo-audit config (7 ignore items)
├── deny.toml                        # cargo-deny config (14 allowed licenses)
├── Dockerfile                       # Container image
└── docker-compose.yml               # Full-stack dev environment
```

## Quick Start

> ⚡ 5-minute tutorial with runnable examples: [docs/quickstart.md](docs/quickstart.md)
> （零依赖路径：`cargo run -p sz-orm-examples --bin real_db_crud`，SQLite 内存库即可跑通 CRUD）

### 1. Add Dependencies

```toml
[dependencies]
# 推荐：path 依赖（与生产试点 sz-pay 一致，始终使用仓库当前版本）
sz-orm-core = { path = "packages/sz-orm-core" }
sz-orm-sqlx = { path = "packages/sz-orm-sqlx" }

# 备选：crates.io（published version may trail the repo, see CHANGELOG.md; use path deps above for the latest）
# sz-orm-core = "10.7"
# sz-orm-sqlx = "10.7"

tokio = { version = "1.40", features = ["full"] }
```

### 2. Define Model

**方式 A（推荐）：`#[derive(Model)]` 自动生成 `impl Model` + `impl ModelExt`（R5，零样板）**

```rust
use sz_orm_core::Model;

#[derive(Debug, Clone, Default, Model)]
#[table(name = "users")]
struct User {
    #[column(primary_key)]
    id: i64,
    name: String,
    email: String,
}
```

**方式 B（手写）：完全掌控 trait 行为（进阶）**

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

### 3. Build Query

```rust
use sz_orm_core::dialect::get_dialect;
use sz_orm_core::{DbType, Model, QueryBuilder, TimestampFields, Value};

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
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

fn main() {
    let dialect = get_dialect(DbType::MySQL).expect("MySQL dialect available");
    let sql = QueryBuilder::<User>::new(dialect)
        .table("users")
        .select(vec!["id", "name", "email"])
        .expect("valid columns")                 // select() returns Result
        .where_eq("status", Value::String("active".to_string()))
        .order_by("created_at")
        .limit(10)
        .sql();                                  // returns the SQL string (`?` placeholders)
    println!("{sql}");
}
```

### 4. Connect to Real Database (sz-orm-sqlx)

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

For MySQL / PostgreSQL, replace with `MySqlPoolHandle` / `PgPoolHandle` and `SqlxMySqlConnectionFactory` / `SqlxPgConnectionFactory`.

### 5. Compile-Time SQL Validation (sql_string!)

```rust
use sz_orm_core::sql_string;

let sql = sql_string!("SELECT * FROM users WHERE id = 1");         // OK
let sql = sql_string!("SELECT * FROM users WHERE id = ?"; params: 1); // OK — param count validated
// sql_string!("SELECT * FORM users");                              // Compile error: missing FROM
// sql_string!("SELECT * FROM users WHERE name = 'x' OR '1'='1'"); // Compile error: injection pattern
```

## Supported Databases

| Database | Dialect | Real Connection | Default Port |
|--------|------|----------|----------|
| MySQL | `MySqlDialect` (backtick) | sz-orm-sqlx | 3306 |
| PostgreSQL | `PostgreSqlDialect` (double quote) | sz-orm-sqlx | 5432 |
| SQLite 3.35+ | `SqliteDialect` | sz-orm-sqlx | — |
| Oracle 23ai | `OracleDialect` (`:N` placeholder + OFFSET/FETCH) | sz-orm-oracle (based on `oracle` crate / ODPI-C binding) | 1521 |
| OceanBase | compatible with `MySqlDialect` | — | 2881 |
| SQL Server | `SqlServerDialect` (independent impl, TDS protocol) | sz-orm-mssql (based on `tiberius` crate) | 1433 |
| ClickHouse | `ClickHouseDialect` (independent impl, not MySQL compatible) | — | 8123 |
| DuckDB | `DuckDBDialect` (independent impl, supports INSERT OR IGNORE) | — | — |
| Redis | NoSQL (no SQL dialect, L2Cache distributed cache backend enabled by default) | — | 6379 |
| MongoDB | NoSQL | — | 27017 |
| VectorDB | Vector database | sz-orm-vector | 19530 |
| PureJsDb | JS engine DB | — | — |
| Informix | `InformixDialect` (SKIP FIRST pagination) | SQL generation only: SQL generation only, no real driver connection | 9088 |
| SAP HANA | `SapHanaDialect` (computed columns + CE functions) | sz-orm-sqlx (`dialect-saphana-driver` feature, based on `hdbconnect_async` v0.32.0) | 39015 |
| Firebird | `FirebirdDialect` (GENERATOR/SEQUENCE + EXECUTE BLOCK) | SQL generation only: SQL generation only, no real driver connection | 3050 |

Use `get_dialect(DbType::MySQL)` to get a dialect instance.

## Cross-Language Bindings

sz-orm exposes a unified C ABI (`packages/sz-orm-cabi`) consumed by language bindings. All
exported functions are `#[no_mangle] extern "C"`; cross-ABI structs are `#[repr(C)]`; the
ABI contract is the cbindgen-generated header `packages/sz-orm-cabi/include/sz_orm_cabi.h`
(single declaration source). Memory is allocated and freed on the Rust side with a
track/untrack registry providing double-free protection, and panics are caught at the
boundary (never propagated across FFI). Every string crossing the boundary is **UTF-8** and
NUL-terminated — callers must not decode it with the system ANSI code page — and
`sz_orm_pool_new` accepts **SQLite DSNs only** (other backends return a null handle instead
of a half-working pool). Both rules are stated in the ABI banner at the top of
`sz_orm_cabi.h`.

**Binding status matrix** (assessed 2026-10-27, see
`docs/assessment/2026-10-26-fullstack-review.md`):

| Binding | Status | Scope |
|---------|--------|-------|
| C# (`packages/sz-orm-cabi/csharp/`) | ⚠️ Real but narrow (22 of the 41 C ABI exports, typed exceptions) | P/Invoke pool/query/execute + transaction + batch + stats (`SzOrmTransaction`/`ExecuteBatch`/`GetStats`/`GetMetrics`), UTF-8 marshalling (`LPUTF8Str`/`PtrToStringUTF8`), typed `SzOrmException` hierarchy, atomic `IDisposable` handle wrapper, structs mirror `#[repr(C)]`; an E2E suite (incl. a non-ASCII round-trip and transaction/rollback/batch/stats cases) was **verified locally on 2026-10-29 (dotnet 9.0.100)** and wired into the `csharp-binding` CI job added 2026-10-28 — no NuGet package, no model/querybuilder surface yet |
| Python (`packages/sz-orm-python/`, PyO3) | ✅ Real (SQLite) | `Pool` / `QueryBuilder` / `Transaction` real; `Model`/`ActiveModel`/`Repository` are in-memory value objects |
| JavaScript (`packages/sz-orm-js/`, napi-rs) | ✅ Real (SQLite) | `Pool` (requires `connect(dsn)` first) / `QueryBuilder` / transactions real; `Model` is a value object; TypeScript definitions in `index.d.ts` |
| Java (`packages/sz-orm-java/`, JNI) | ⚠️ Partial | Pool-level CRUD only; transaction/model/QueryBuilder JNI exports exist on the Rust side but have no Java-side wrapper classes yet |
| Go (`packages/sz-orm-go/`) | ⚠️ Partial, **Windows only** | Loads `sz_orm_go.dll` via `syscall` (8 basic symbols); Unix `dlopen` requires a cgo variant that is **not provided yet** (`go/szorm/dll_unix.go`) |
| C++ (`packages/sz-orm-cpp/`) | ⚠️ Partial | RAII `szorm::Pool` wrapper (`cpp/szorm.h`); no Transaction/Model classes; no C++-side tests |

The FFI layer runs a shared tokio runtime; its worker count can be tuned before the first
pool creation via `sz_orm_set_runtime_workers(n)` (declared in `sz_orm_cabi.h`).

> **Publishing note**: crates.io releases lag the main branch. For the latest capabilities,
> prefer a git/path dependency — the Quick Start above already recommends the path-based
> setup used by the production pilot.

## Core API

### QueryBuilder Chained API

```rust
QueryBuilder::<M>::new(dialect)
    .table("users")
    .select(vec!["id", "name"])
    .expect("valid columns")                                   // select() returns Result
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
    .page(3, 20)                                 // Page 3, 20 per page
    .join_inner("posts", "users.id", "posts.user_id")
    .join_left("profiles", "users.id", "profiles.user_id")
    .build_select();

// Aggregation
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

### Connection Pool

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

// Savepoint (nested transaction)
let sp = tx.savepoint().await?;
tx.rollback_to_savepoint(&sp).await?;
tx.release_savepoint(&sp).await?;

tx.commit().await?;
// tx.rollback().await?;
```

### Migration System

```rust
use sz_orm_core::migration::{ColumnDef, FileMigrationResolver, ForeignKeyDef, IndexDef, MigrationContext, Migrator, SchemaBuilder};
use sz_orm_core::{MigrationResolver, DbType};

// File migrations: <version>_<name>_up.sql / <version>_<name>_down.sql
let resolver = FileMigrationResolver::new("./migrations".into());
let migrations = resolver.resolve(DbType::MySQL)?;

let mut migrator = Migrator::new(MigrationContext::default())
    .add_migrations(migrations);

migrator.migrate().await?;                // Apply all pending migrations
migrator.up(Some("003")).await?;           // Apply up to 003
migrator.down(Some("001")).await?;         // Rollback to 001
migrator.rollback("002").await?;           // Rollback single
migrator.reset().await?;                   // Rollback all + re-apply
migrator.refresh().await?;                 // reset alias
migrator.progress();                       // Migration progress

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

### Value Type (20 Variants)

```rust
use sz_orm_core::Value;

// Variants
Value::Null | Bool(bool) | I8 | I16 | I32 | I64 | U8 | U16 | U32 | U64
| F32 | F64 | String(String) | Bytes(Vec<u8>) | Uuid(String)
| Date(String) | DateTime(String) | Time(String) | Json(String)
| Array(Vec<Value>) | Object(HashMap<String, Value>)

// Conversion
value.as_str();    // Option<&str>
value.as_i64();    // Option<i64>
value.as_f64();    // Option<f64>
value.as_bool();   // Option<bool>
value.as_bytes();  // Option<&[u8]>

// From implementations
let v: Value = 42i64.into();
let v: Value = "hello".into();
let v: Value = vec![1u8, 2u8].into();
```

## Advanced Modules (21)

sz-orm-core provides 21 advanced modules beyond the base engine. See [Usage Guide §3.7](docs/sz-orm使用指南.md#37-sz-orm-core-高级特性模块21-个) and [API Reference §2.22](docs/sz-ormAPI参考.md#222-sz-orm-core-高级特性模块21-个).

| # | Module | Highlights |
|---|------|------|
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
| 19 | `schema_gen` | Diesel-style schema.rs auto-generation |
| 20 | `sql_safety` | SQL injection primitives (validate_identifier / validate_fk_action / validate_id_value) |
| 21 | `type_handler` | MyBatis-style TypeHandler SPI (DateTimeHandler / UuidHandler / ...) |

## Hook System (Soft Delete + Multi-Tenant)

### HookContext — Execution Context

```rust
use sz_orm_core::hooks::HookContext;

let mut ctx = HookContext::new()
    .with_tenant(42)
    .with_operator(1)
    .with_timestamp(1700000000);
ctx.set_meta("source", "api");
```

### Hookable trait — 16 Lifecycle Hooks

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

// Auto-append on query: AND deleted_at IS NULL
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

// When ctx.tenant_id = Some(42): auto-append AND tenant_id = ?
// When ctx.tenant_id = None: no append (cross-tenant query, caller must ensure safety)
let scope = <(TenantScope, Order) as GlobalScope>::apply_scope(&ctx);
```

### HookRegistry — Runtime Hook Registration

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

### ScopeRegistry — Scope Control

```rust
use sz_orm_core::hooks::ScopeRegistry;

let registry = ScopeRegistry::new();
registry.disable("soft_delete");       // Disable soft delete scope
registry.enable("soft_delete");        // Re-enable
registry.is_enabled("soft_delete");    // true

// Temporarily disable (within closure)
let result = registry.without_scope("soft_delete", || {
    // Queries here will include soft-deleted rows
    42
});
```

## CLI Tool

SZ-ORM provides a `sz-orm` CLI for migration management, code generation, and SQL validation.

### Installation

```bash
cargo install --path cli
```

### Commands

```bash
sz-orm                              # Show help
sz-orm info                         # Show ORM summary
sz-orm --version                    # Show version

sz-orm dialect list                 # List all dialects
sz-orm dialect show mysql           # Show MySQL dialect details

sz-orm make:migration create_users  # Generate migration skeleton
sz-orm make:model User              # Generate Model skeleton

sz-orm migrate                      # Show pending migrations
sz-orm migrate:status               # Show migration progress

sz-orm sql:validate "SELECT * FROM users"  # SQL validation
```

### Options

- `--migrations <dir>` — migration file directory (default `./migrations`)
- `--output <dir>` — generated code output directory (default `./src/models` or `./migrations`)

## Examples

The `examples/` directory provides 38 runnable examples:

| Example | Description | Run |
|------|------|------|
| `ai_integration_demo` | AI deep integration end-to-end demo | `cargo run -p sz-orm-examples --bin ai_integration_demo` |
| `backpressure_stream_demo` | Stream backpressure demo | `cargo run -p sz-orm-examples --bin backpressure_stream_demo` |
| `batch_parallel_demo` | Batch parallel execution demo | `cargo run -p sz-orm-examples --bin batch_parallel_demo` |
| `cdc_realtime_sync_demo` | CDC realtime sync demo | `cargo run -p sz-orm-examples --bin cdc_realtime_sync_demo` |
| `composable_plugin_demo` | Composable plugin system full usage | `cargo run -p sz-orm-examples --bin composable_plugin_demo` |
| `db_verify_test` | Compile-time DB verification (`query!` db-verify) | `cargo run -p sz-orm-examples --bin db_verify_test` |
| `dist_coordination_demo` | Distributed coordination demo | `cargo run -p sz-orm-examples --bin dist_coordination_demo` |
| `eco_extension_demo` | Eco extension end-to-end demo | `cargo run -p sz-orm-examples --bin eco_extension_demo` |
| `graph_traversal_demo` | Graph traversal API demo | `cargo run -p sz-orm-examples --bin graph_traversal_demo` |
| `ha_failover_demo` | High-availability failover demo | `cargo run -p sz-orm-examples --bin ha_failover_demo` |
| `hooks_soft_delete` | Hook system — soft delete + global scope | `cargo run -p sz-orm-examples --bin hooks_soft_delete` |
| `l2_cache_singleflight_demo` | L2Cache SingleFlight cache protection demo | `cargo run -p sz-orm-examples --bin l2_cache_singleflight_demo` |
| `migration` | Migration system — file migration + SchemaBuilder | `cargo run -p sz-orm-examples --bin migration` |
| `model_definition` | Model definition — Model + ModelExt traits | `cargo run -p sz-orm-examples --bin model_definition` |
| `multi_region_demo` | Multi-region active-active demo | `cargo run -p sz-orm-examples --bin multi_region_demo` |
| `multi_tenant` | Multi-tenant — TenantModel + TenantScope | `cargo run -p sz-orm-examples --bin multi_tenant` |
| `nl_query_demo` | AI natural language query demo | `cargo run -p sz-orm-examples --bin nl_query_demo` |
| `olap_vectorized_demo` | OLAP vectorized query demo | `cargo run -p sz-orm-examples --bin olap_vectorized_demo` |
| `parallel_join_demo` | `parallel_join!` macro demo | `cargo run -p sz-orm-examples --bin parallel_join_demo` |
| `parallel_queries_demo` | `parallel_queries` demo | `cargo run -p sz-orm-examples --bin parallel_queries_demo` |
| `perf_accel_demo` | Performance acceleration end-to-end demo | `cargo run -p sz-orm-examples --bin perf_accel_demo` |
| `prepared_cache_demo` | `prepared_cache` demo | `cargo run -p sz-orm-examples --bin prepared_cache_demo` |
| `production_app` | Production e-commerce order system (6 extension packages) | `cargo run -p sz-orm-examples --bin production_app` |
| `production_dtx` | Production cross-shard distributed order system | `cargo run -p sz-orm-examples --bin production_dtx` |
| `query_auto_tuning_demo` | Query auto-tuning demo | `cargo run -p sz-orm-examples --bin query_auto_tuning_demo` |
| `quick_start` | QueryBuilder basics | `cargo run -p sz-orm-examples --bin quick_start` |
| `rbac_abac_demo` | RBAC + ABAC permission model demo | `cargo run -p sz-orm-examples --bin rbac_abac_demo` |
| `real_db_crud` | Real DB end-to-end — pool + CRUD + transactions | `cargo run -p sz-orm-examples --bin real_db_crud` |
| `serverless_demo` | Serverless adaptation demo | `cargo run -p sz-orm-examples --bin serverless_demo` |
| `stream_processing_demo` | Realtime stream processing demo | `cargo run -p sz-orm-examples --bin stream_processing_demo` |
| `stream_unified_demo` | `stream_unified` demo | `cargo run -p sz-orm-examples --bin stream_unified_demo` |
| `sz_pay_pattern` | SZ-PAY production usage pattern (desensitized) | `cargo run -p sz-orm-examples --bin sz_pay_pattern` |
| `sz_rust_integration_example` | sz-rust co-integration — transparent multi-backend access | `cargo run -p sz-orm-examples --bin sz_rust_integration_example` |
| `tde_demo` | TDE transparent data encryption complete example | `cargo run -p sz-orm-examples --bin tde_demo` |
| `transaction` | Transactions — ACID transaction + savepoint | `cargo run -p sz-orm-examples --bin transaction` |
| `transaction_run_demo` | `run_transaction` / `run_savepoint` closure templates | `cargo run -p sz-orm-examples --bin transaction_run_demo` |
| `v900_perf_demo` | v9.0.0 performance optimization demo | `cargo run -p sz-orm-examples --bin v900_perf_demo` |
| `wiring_smoke` | Production entry wiring demo (Gate 15 evidence) | `cargo run -p sz-orm-examples --bin wiring_smoke` |

## Tests

SZ-ORM ensures quality through a **7-line verification system**:

| Method | Description | Test File |
|------|------|----------|
| **TDD** | Core unit tests | `core.rs` |
| **Integration** | Real MySQL/PG/SQLite/Oracle E2E | `integration_*.rs` |
| **Jepsen** | Concurrency correctness + real DB Jepsen | `jepsen.rs`, `real_db_jepsen.rs` |
| **Fuzz** | Boundary/extreme cases | `fuzz.rs` |
| **Stress** | Performance/stress | `stress.rs`, `core_bench.rs` |
| **Chaos** | Fault robustness | `chaos.rs` |
| **Formal** | Formal invariant verification | `formal.rs` |

### Running Tests

```bash
# Full workspace tests
cargo test --workspace

# Core package only
cargo test -p sz-orm-core

# Real DB tests (requires MySQL/PG/SQLite running)
cargo test -p sz-orm-core --features testing

# Performance benchmarks
cargo bench -p sz-orm-core

# 6h Soak test
SOAK_DURATION=6h cargo test -p sz-orm-core --test soak -- --ignored
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

### Documentation

```bash
# Generate docs
cargo doc --workspace --no-deps --open

# Lint
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

## Security Audit

SZ-ORM ensures security through CI-integrated `cargo-audit` and `cargo-deny`:

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

**Results (2026-07-21)**:
- ✅ `cargo audit`: 0 unignored vulnerabilities (7 transitive dependency ignore items, all documented)
- ✅ `cargo deny`: advisories ok / bans ok / licenses ok / sources ok
- License allowlist: 14 permissive licenses (MIT / Apache-2.0 / BSD / ISC / Zlib / CC0-1.0 / MPL-2.0 / ...)
- Sources: only crates.io official registry allowed; no git/path sources
- CI: `.github/workflows/security.yml` runs on every push/PR to main/master

### rsa Marvin Attack Eliminated

**The rsa Marvin Attack has been completely eliminated via sqlx 0.8.6 → 0.9.0 upgrade**: rsa has been completely removed from the dependency tree, this vulnerability no longer triggers. The current 7 ignore items are all unrelated to rsa.

## v2.0.0 Roadmap Delivery (2026-08-06)

### Delivery Checklist

| Task | Status | Deliverable |
|------|------|--------|
| Oracle integration tests | ✅ Completed | `tests/integration_oracle.rs` added 7 scenario types, 10 tests passed |
| SQL Server integration tests | ✅ Completed | `tests/integration_mssql.rs` created 8 scenario types, 5 dialect assertions passed |
| Python bindings (PyO3) | ✅ Real (SQLite) | `packages/sz-orm-python/`: PyPool/PyQueryBuilder/PyTransaction real (SQLite, transactions via `Pool.create_transaction()`); PyModel/ActiveModel/Repository are in-memory value objects |
| JavaScript bindings (napi-rs) | ✅ Real (SQLite) | `packages/sz-orm-js/`: Pool/QueryBuilder real (SQLite, transactions via `Pool.async_begin/commit/rollback`); Model is a value object |
| Security audit | ✅ Completed | `docs/assessment/2026-08-14-security-audit.md`, 7 dimensions covered |

### Gate Verification (historical record at v2.0.0 — 11 gates at that time, see current gate table above)

9 of 11 gates passed, 2 environment limitations (`cargo audit` network restricted, `--all-features` missing protoc).

### Audit Conclusion

🟡 Medium risk: 3 deprecated `where_cond`/`or_where` need removal in v2.0.0, unwrap/expect need SAFETY comments. See [Security Audit Report](docs/assessment/2026-08-14-security-audit.md).

---

## Performance Benchmarks

criterion benchmarks (sample_size=10, measurement_time=3s, warm_up=1s, Windows; reproduce: `cargo bench -p sz-orm-core`):

| Benchmark | Result |
|------|------|
| `value_to_param/null` | 3.2 ns (312 Melem/s) |
| `value_to_param/i64` | 53.4 ns (18.7 Melem/s) |
| `value_to_param/string_short` | 252 ns (3.97 Melem/s) |
| `dialect_escape_string/long_1024` | 954 ns (1.02 GiB/s) |
| `dialect_build_create_table/100 cols` | 31.7 µs (3.15 Melem/s) |
| `dialect_build_pagination/1M page` | 163 ns (page depth stable) |
| `pool_acquire_release` | 230 ns / round trip |
| `in_memory_scan/select_where_eq_1pct/100K` | 4.87 ms (20.5 Melem/s) |
| `json_parsing/3kb` | 85.0 µs (71 MiB/s) |

**Real DB batch INSERT throughput** (100K rows):

| Database | Throughput | Relative |
|--------|------|--------|
| SQLite (file) | 720K rows/s | 4.97× |
| PostgreSQL 18 | 268K rows/s | 1.85× |
| MySQL 9.6 | 145K rows/s | 1.0× (baseline) |
| Oracle 23ai Free | 19.1K rows/s | 0.13× |

**1-hour Soak Test**: 1.38 billion operations, 1.16% throughput decay, P99 43μs→41μs, 0 errors (reproduce as above).

**v6.5.0 Parallel Query / Cache / Stream**:

| Feature | Metric | Result |
|---------|--------|--------|
| parallel_queries (3 tables) | speedup | ≥ 2.1× (3 × 70%) |
| parallel_queries (10 tables) | speedup | ≥ 3.0× |
| PreparedStatementCache | p50 latency, cache off→on (Oracle 23ai point query, local, measured) | 25%–32% (see `docs/assessment/2026-10-28-db-review.md` §11.6) |
| AsyncRowStream (1M rows) | peak memory | ~1 row (streaming, no accumulation) |
| BackpressureRowStream | threshold trigger | pending >= threshold → pause |

## Documentation Index

| Document | Description |
|------|------|
| [Learning Tutorial](docs/sz-orm学习路线图.md) | Concrete learning tutorial for PHP/ThinkPHP engineers (includes Rust quick-start + AI collaboration patterns) |
| [Usage Guide](docs/sz-orm使用指南.md) | End-to-end usage guide (v5.0, covers all 21 advanced modules) |
| [API Reference](docs/sz-ormAPI参考.md) | Type signatures and parameter docs (v5.0) |
| [Architecture Design](docs/sz-orm架构设计.md) | Workspace architecture overview |
| [Engineering Practices](docs/sz-orm-engineering-practices.md) | Gate 1-10 + test pyramid T1-T6 |
| [API Contracts](docs/api-contracts.md) | Public API stability contracts |
| [ADR Index](docs/adr/README.md) | Architecture Decision Records (11 ADRs) |
| [ADR and Bug Localization Spec](docs/ADR与生产Bug定位规范.md) | Reusable spec: ADR writing + four-layer bug localization flow + validity verification (v1.0) |

## License

MIT License © SZ-ORM Team
