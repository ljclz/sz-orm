//! # SZ-ORM — Xianshida ORM
//!
//! Rust asynchronous ORM workspace (production-ready), ThinkORM-style compatible.
//!
//! **Production evidence**: 67 packages published to crates.io · 321 references in sz-pay production · 5159+ tests pass.
//!
//! ## Architecture Overview
//!
//! The SZ-ORM workspace consists of **71 members** (69 sz-orm-* libs + cli + examples):
//!
//! ### Core Engine (sz-orm-core)
//! | Module | Function |
//! |------|------|
//! | `model` | `Model` trait — defines table name, primary key, timestamps, soft delete, relations |
//! | `query` | `QueryBuilder<M>` — chainable API, supports SELECT/INSERT/UPDATE/DELETE/aggregation/pagination/JOIN |
//! | `dialect` | Multi-database dialects — MySQL (backtick), PostgreSQL (double quote), SQLite, Oracle 23ai |
//! | `pool` | Asynchronous connection pool — configurable size, timeout, idle reaping, health checks, max lifetime |
//! | `transaction` | ACID transactions — isolation levels, savepoints, `TransactionManager` for multi-transaction management |
//! | `migration` | File-based migration system — up/down/rollback/reset/refresh, with `SchemaBuilder` |
//! | `cache` | Multi-level cache — `MemoryCache`, `MultiLevelCache`, with TTL support |
//! | `value` | Unified value type — 20 variants (integer/float/string/bytes/UUID/date/JSON/array) |
//! | `db_type` | Database type enum — MySQL, PostgreSQL, SQLite, Oracle, Redis, MongoDB and 11 total |
//! | `error` | Error type system — `DbError` (20 variants), `PoolError`, `CacheError`, `TxError` |
//!
//! ### Database Adapters
//! - **sz-orm-sqlx** — sqlx adapter, connects to real MySQL/PostgreSQL/SQLite/Oracle
//! - **sz-orm-sql-validator** — SQL validation and injection detection
//!
//! ### Extension Ecosystem Packages (18)
//! | Package | Function |
//! |------|------|
//! | sz-orm-crypto | Crypto primitives (AES-256-GCM, PBKDF2, HMAC-SHA256) |
//! | sz-orm-auth | JWT authentication (HS256) |
//! | sz-orm-scheduler | Cron scheduled task dispatch |
//! | sz-orm-mqtt | MQTT client (rumqttc) |
//! | sz-orm-websocket | WebSocket server (tokio-tungstenite) |
//! | sz-orm-queue | Message queue (RabbitMQ/lapin, Kafka, NATS, ActiveMQ, RocketMQ, Pulsar) |
//! | sz-orm-storage | Object storage (S3/Alibaba Cloud/Tencent Cloud/Huawei Cloud/Qiniu/Upyun/Local) |
//! | sz-orm-ai | AI integration (Embedding, RAG, Vector) |
//! | sz-orm-grpc | gRPC server/client |
//! | sz-orm-graphql | GraphQL query support |
//! | sz-orm-es | Elasticsearch integration |
//! | sz-orm-tracing | Distributed tracing |
//! | sz-orm-logger | Logging system |
//! | sz-orm-swagger | API documentation generation |
//! | sz-orm-masking | Data masking |
//! | sz-orm-health | Health checks |
//! | sz-orm-audit | Audit log |
//! | sz-orm-batch | Batch operations |
//!
//! ### Advanced Feature Packages (6)
//! | Package | Function |
//! |------|------|
//! | sz-orm-dtx | Distributed transactions |
//! | sz-orm-rw | Read-write splitting |
//! | sz-orm-sharding | Sharding |
//! | sz-orm-limit | Rate limiting |
//! | sz-orm-config | Configuration management |
//! | sz-orm-mig | Enhanced migration management |
//!
//! ### Platform Support
//! - **sz-orm-wasm** — WebAssembly compile target
//! - **sz-orm-lc** — Local/edge computing
//! - **sz-orm-back** — Backup and restore
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use sz_orm_core::*;
//!
//! // 1. Define the model
//! #[derive(Clone)]
//! struct User {
//!     id: i64,
//!     name: String,
//!     email: String,
//! }
//!
//! impl Model for User {
//!     type PrimaryKey = i64;
//!     fn table_name() -> &'static str { "users" }
//!     fn pk(&self) -> Self::PrimaryKey { self.id }
//!     fn set_pk(&mut self, pk: Self::PrimaryKey) { self.id = pk; }
//! }
//!
//! // 2. Build a query
//! let dialect = get_dialect(DbType::MySQL).unwrap();
//! let sql = QueryBuilder::<User>::new(dialect)
//!     .table("users")
//!     .select(vec!["id", "name", "email"])
//!     .where_eq("status", Value::String("active".to_string()))
//!     .order_by("created_at")
//!     .order_desc("id")
//!     .limit(10)
//!     .build_select();
//!
//! // 3. Validate before execution
//! QueryBuilder::<User>::new(get_dialect(DbType::MySQL).unwrap())
//!     .table("users")
//!     .select(vec!["id", "name"])
//!     .validate()?; // Validate SQL syntax, injection, parenthesis balance
//!
//! // 4. Other operations
//! let mut data = std::collections::HashMap::new();
//! data.insert("name".to_string(), Value::String("Alice".to_string()));
//! data.insert("age".to_string(), Value::I64(25));
//!
//! let insert_sql = QueryBuilder::<User>::new(dialect)
//!     .table("users")
//!     .build_insert(&data);
//!
//! let update_sql = QueryBuilder::<User>::new(get_dialect(DbType::MySQL).unwrap())
//!     .table("users")
//!     .where_eq("id", Value::I64(1))
//!     .build_update(&data);
//!
//! let delete_sql = QueryBuilder::<User>::new(get_dialect(DbType::MySQL).unwrap())
//!     .table("users")
//!     .where_eq("id", Value::I64(1))
//!     .build_delete();
//! ```
//!
//! ## Supported Databases
//!
//! | Database | Dialect Implementation | Real Connection | Quoting |
//! |--------|---------|---------|---------|
//! | MySQL | `MySqlDialect` (`` ` `` backtick) | sz-orm-sqlx | ✅ |
//! | PostgreSQL | `PostgreSqlDialect` (`"` double quote) | sz-orm-sqlx | ✅ |
//! | SQLite 3.35+ | `SqliteDialect` (`"` double quote) | sz-orm-sqlx | ✅ |
//! | Oracle 23ai | `OracleDialect` (automatic type mapping) | sz-orm-sqlx | ✅ |
//!
//! Obtain a dialect instance via `get_dialect(DbType::MySQL)`. Each dialect handles:
//! - Identifier quoting style
//! - String escaping rules
//! - Pagination syntax (LIMIT/OFFSET vs OFFSET/FETCH)
//! - JSON extraction functions (JSON_EXTRACT vs #>> vs json_extract vs JSON_VALUE)
//! - Full-text search (MATCH AGAINST vs to_tsvector vs CONTAINS)
//! - Boolean-to-integer conversion (IF/CASE)
//! - Auto-increment keyword (AUTO_INCREMENT/GENERATED BY DEFAULT AS IDENTITY)
//!
//! ## Core Features in Detail
//!
//! ### QueryBuilder API
//!
//! Most query methods return `Self`, enabling chainable calls; validation methods like `select`/`having`
//! return `Result<Self>` (after audit M-5/M-6, column names/aggregate expressions go through identifier validation):
//!
//! ```rust,ignore
//! // Basic query
//! QueryBuilder::<M>::new(dialect)
//!     .table("users")
//!     .select(vec!["id", "name"])?                 // Column validation + quote
//!     .where_eq("status", Value::String("active".to_string()))    // AND
//!     .or_where_eq("role", Value::String("admin".to_string()))     // OR
//!     .where_in("id", vec![Value::I64(1), Value::I64(2)])
//!     .where_between("age", Value::I64(18), Value::I64(30))
//!     .where_null("deleted_at")
//!     .order_by("created_at")
//!     .order_desc("id")
//!     .group_by("status")
//!     .having(AggExpr::CountStar, HavingOp::Gt, Value::I64(5))?   // Parameterized HAVING
//!     .limit(20)
//!     .offset(40)
//!     .page(3, 20)                       // page=3, page_size=20
//!     .join_inner("posts", "users.id", "posts.user_id")
//!     .join_left("profiles", "users.id", "profiles.user_id")
//!     .build_select();
//!
//! // Aggregate functions
//! builder.build_count();    // SELECT COUNT(*)
//! builder.build_exists();   // SELECT EXISTS(...)
//! builder.build_max("score");
//! builder.build_min("price");
//! builder.build_sum("amount");
//! builder.build_avg("value");
//! ```
//!
//! ### SQL Validation
//!
//! ```rust,ignore
//! // Compile-time + runtime dual validation
//! builder.validate()?;              // Validate SELECT
//! builder.validate_insert(&data)?;  // Validate INSERT (including empty data check)
//! builder.validate_update(&data)?;  // Validate UPDATE (including empty data check)
//! builder.validate_delete()?;       // Validate DELETE
//!
//! // Validation covers: SQL syntax, injection detection, parenthesis balance,
//! // table/column name legitimacy, JOIN column validation
//! ```
//!
//! ### Model Trait
//!
//! ```rust,ignore
//! pub trait Model: Send + Sync + Sized + 'static {
//!     type PrimaryKey: Send + Sync + Debug + Display + Clone + Default;
//!
//!     fn table_name() -> &'static str;          // Table name (required)
//!     fn pk_name() -> &'static str { "id" }     // Primary key column name
//!     fn pk(&self) -> Self::PrimaryKey;         // Get primary key value
//!     fn set_pk(&mut self, pk: Self::PrimaryKey); // Set primary key value
//!     fn foreign_key(relation: &str) -> String; // Foreign key naming "user_id"
//!     fn timestamp_fields() -> Option<TimestampFields>; // Automatic timestamps
//!     fn soft_delete_field() -> Option<&'static str>;   // Soft delete field
//! }
//!
//! // ModelExt extension
//! pub trait ModelExt: Model {
//!     fn columns() -> Vec<&'static str>;     // All columns
//!     fn fillable() -> Vec<&'static str>;    // Fillable columns
//!     fn guarded() -> Vec<&'static str>;     // Guarded columns (includes primary key by default)
//!     fn hidden() -> Vec<&'static str>;      // Hidden columns (not serialized)
//!     fn relations() -> HashMap<&str, Relation>; // Relations
//!     fn fill(&mut self, data: HashMap<String, Value>); // Mass assignment
//!     fn to_json(&self) -> serde_json::Value; // Serialize
//! }
//!
//! // Four relation types
//! // BelongsTo   — many-to-one (Order → User)
//! // HasMany     — one-to-many (User → Orders)
//! // HasOne      — one-to-one (User → Profile)
//! // BelongsToMany — many-to-many (User ↔ Role, through junction table)
//! ```
//!
//! ### Connection Pool
//!
//! ```rust,ignore
//! // Configure via Builder
//! let config = PoolConfigBuilder::new()
//!     .max_size(100)       // Maximum connections
//!     .min_idle(10)        // Minimum idle connections
//!     .acquire_timeout(30) // Acquire timeout (seconds)
//!     .idle_timeout(600)   // Idle timeout (seconds)
//!     .max_lifetime(1800)  // Max lifetime (seconds)
//!     .build()?;
//!
//! let pool = Pool::new(config, factory)?;
//! let conn = pool.acquire().await?;  // Acquire connection (with timeout)
//! pool.release(conn).await;         // Release connection
//! pool.status().await;               // PoolStatus { idle, active, max, min }
//! pool.reap_idle().await;           // Reap idle connections
//! pool.close_all().await;           // Close all connections
//! ```
//!
//! ### Transactions
//!
//! ```rust,ignore
//! // Transaction options
//! let opts = TransactOptions::default()
//!     .with_isolation(IsolationLevel::Serializable)
//!     .read_only()
//!     .with_timeout(Duration::from_secs(30));
//!
//! let mut tx = Transaction::new(conn, opts);
//! tx.execute("INSERT INTO users VALUES (1)").await?;
//! tx.query("SELECT * FROM users").await?;
//!
//! // Savepoints (nested transactions)
//! let sp = tx.savepoint().await?;         // SAVEPOINT sp_N
//! tx.rollback_to_savepoint(&sp).await?;   // ROLLBACK TO SAVEPOINT sp_N
//! tx.release_savepoint(&sp).await?;       // RELEASE SAVEPOINT sp_N
//!
//! tx.commit().await?;
//! // tx.rollback().await?;
//!
//! // TransactionManager: manages multiple named transactions
//! let mgr = TransactionManager::new();
//! mgr.begin("tx1", conn, opts).await?;
//! mgr.commit("tx1").await?;
//! mgr.list().await;        // ["tx1"]
//! mgr.state("tx1").await;  // Some(TransactionState::Committed)
//! ```
//!
//! ### Migration System
//!
//! ```rust,ignore
//! // File naming: <version>_<name>_up.sql / <version>_<name>_down.sql
//! // Example: 001_create_users_up.sql, 001_create_users_down.sql
//!
//! let resolver = FileMigrationResolver::new(PathBuf::from("./migrations"));
//! let migrations = resolver.resolve(DbType::MySQL)?;
//!
//! let mut migrator = Migrator::new(MigrationContext::default())
//!     .add_migrations(migrations);
//!
//! migrator.migrate().await?;                     // Execute all pending migrations
//! migrator.up(Some("003")).await?;               // Migrate up to specified version
//! migrator.down(Some("001")).await?;             // Rollback to specified version
//! migrator.rollback("002").await?;               // Rollback a single migration
//! migrator.reset().await?;                       // Rollback all + re-execute
//! migrator.refresh().await?;                     // Same as reset
//! migrator.progress();                            // MigrationProgress { total, applied, pending }
//!
//! // SchemaBuilder: programmatic table creation
//! let sql = SchemaBuilder::new("users")
//!     .add_column(ColumnDef::new("id", "INT").not_null().auto_increment())
//!     .add_column(ColumnDef::new("name", "VARCHAR").length(255).not_null())
//!     .add_index(IndexDef::new("idx_name", vec!["name"]).unique())
//!     .add_foreign_key(
//!         ForeignKeyDef::new("fk_role", "role_id", "roles", "id")
//!             .on_delete("CASCADE")
//!     )
//!     .build(DbType::MySQL);
//! ```
//!
//! ### Value Type
//!
//! ```rust,ignore
//! // 20 variants, covering all database types
//! Value::Null | Bool(bool) | I8..I64 | U8..U64 | F32 | F64
//! | String(String) | Bytes(Vec<u8>) | Uuid(String) | Date(String)
//! | DateTime(String) | Time(String) | Json(String) | Array(Vec<Value>)
//!
//! // Type conversions
//! value.as_str()    // Option<&str>
//! value.as_i64()    // Option<i64> (supports F32/F64/Bool/String→i64 conversion)
//! value.as_f64()    // Option<f64>
//! value.as_bool()   // Option<bool> (supports "true"/"1"/"yes"/"on" etc.)
//! value.as_bytes()  // Option<&[u8]>
//! value.to_param()  // Cow<str> — SQL parameter format
//!
//! // From implementations
//! let v: Value = 42i64.into();
//! let v: Value = "hello".into();
//! let v: Value = vec![1u8, 2u8].into();
//! ```
//!
//! ## Error Handling
//!
//! Unified error type system, each error carries a unique error code:
//!
//! ```rust,ignore
//! // DbError — 20 variants, error codes DB001-DB020
//! DbError::QueryError("...")
//! DbError::ConnectionRefused("...")
//! DbError::ConnectionTimeout("...")
//! DbError::NotFound("...")
//! DbError::ConstraintViolation("...")
//! // ... etc.
//!
//! // PoolError — 6 variants, error codes PL001-PL006
//! PoolError::Exhausted | Timeout | AlreadyAcquired | InvalidConfig | ...
//!
//! // CacheError — 6 variants, error codes CH001-CH006
//! // TxError — 6 variants (NotStarted, CommitFailed, SavepointError, etc.)
//!
//! // Convenience methods
//! DbError::query("test failed")        // Create query error
//! DbError::connection("timeout")       // Create connection error
//! DbError::not_found("user #42")       // Create not-found error
//! err.is_retryable()                   // Whether retryable
//! err.error_code()                     // "DB001"
//! ```
//!
//! ## Validation Methods
//!
//! SZ-ORM ensures quality through a **7-layer validation system**:
//!
//! | Method | Description | Test File |
//! |---------|------|---------|
//! | **TDD** | 115+ unit tests for core modules | `core.rs` |
//! | **Integration** | End-to-end with real MySQL/PG/SQLite/Oracle | `integration_mysql.rs`, `integration_pg.rs`, `integration_sqlite.rs` |
//! | **Jepsen** | 29 concurrency correctness tests + 10 real DB Jepsen | `jepsen.rs`, `real_db_jepsen.rs` |
//! | **Fuzz** | 11 boundary/edge case discoveries | `fuzz.rs` |
//! | **Stress** | 77 performance benchmarks | `stress.rs`, `core_bench.rs` |
//! | **Chaos** | 16 fault robustness tests | `chaos.rs` |
//! | **Formal** | 14 formal verification invariants | `formal.rs` |
//!
//! **Total: 1,723 tests** (1,317 `#[test]` + 406 `#[tokio::test]`; some require real services)
//!
//! ## Type Aliases and Constants
//!
//! ```rust,ignore
//! // Type aliases
//! pub type Shared<T> = Arc<T>;
//! pub type Boxed<T> = Box<T>;
//! pub type DbResult<T> = Result<T, DbError>;
//! pub type PoolResult<T> = Result<T, PoolError>;
//! pub type CacheResult<T> = Result<T, CacheError>;
//! pub type TxResult<T> = Result<T, TxError>;
//!
//! // Default constants
//! pub const DEFAULT_BATCH_SIZE: usize = 1000;
//! pub const DEFAULT_ACQUIRE_TIMEOUT: u64 = 30;   // seconds
//! pub const DEFAULT_IDLE_TIMEOUT: u64 = 600;      // seconds
//! pub const DEFAULT_MAX_LIFETIME: u64 = 1800;     // seconds
//! pub const DEFAULT_MIN_IDLE: u32 = 5;
//! pub const DEFAULT_MAX_SIZE: u32 = 100;
//! ```
//!
//! ## Export Manifest
//!
//! `use sz_orm_core::*;` imports all public symbols from the following modules:
//!
//! - `async_trait` (re-exported), `bytes::Bytes`, `chrono::{DateTime, Utc}`, `serde::{Deserialize, Serialize}`
//! - `cache::*` — `Cache`, `MemoryCache`, `MultiLevelCache`, `CacheStats`
//! - `db_type::*` — `DbType` enum (11 database types)
//! - `dialect::*` — `Dialect`, `MySqlDialect`, `PostgreSqlDialect`, `SqliteDialect`, `OracleDialect`, `get_dialect()`
//! - `error::*` — `DbError`, `PoolError`, `CacheError`, `TxError`
//! - `migration::*` — `Migration`, `Migrator`, `SchemaBuilder`, `ColumnDef`, `IndexDef`, `ForeignKeyDef`
//! - `model::*` — `Model`, `ModelExt`, `Relation`, `BelongsTo`, `HasMany`, `HasOne`, `BelongsToMany`
//! - `pool::*` — `Pool`, `PoolConfig`, `PoolConfigBuilder`, `Connection`, `ConnectionFactory`, `PoolStatus`
//! - `query::*` — `QueryBuilder<M>` (chainable SQL builder)
//! - `transaction::*` — `Transaction`, `TransactionManager`, `TransactOptions`, `IsolationLevel`
//! - `value::*` — `Value` enum (20 variants)

// 文档完整性：全局启用 missing_docs lint（v3.6.0 已补齐全部 pub API 文档）
#![warn(missing_docs)]

// v3.9.0 M1-T3：derive(Validate) 宏生成 sz_orm_core 绝对路径，
// crate 内部测试需 self 别名使该路径解析到当前 crate
#[cfg(test)]
extern crate self as sz_orm_core;

use std::sync::Arc;

/// Re-export async traits
pub use async_trait::async_trait;

/// Re-export common types
pub use bytes::Bytes;
pub use chrono::{DateTime, Utc};
pub use serde::{Deserialize, Serialize};

pub mod access_control;
pub mod accessors;
pub mod active_model;
#[allow(missing_docs)]
pub mod api_coverage;
pub mod behaviors;
#[cfg(feature = "benchmark-suite")]
pub mod benchmark;
pub mod bloom;
mod cache;
pub mod change_tracker;
pub mod circuit_breaker;
#[cfg(feature = "type-safe-columns")]
pub mod column;
#[cfg(feature = "zero-copy")]
pub mod columnar;
#[cfg(any(
    feature = "prepared-stmt-cache",
    feature = "async-row-stream",
    feature = "parallel-batch"
))]
pub mod connection_ext;
pub mod cursor_stream;
pub mod cycle_detection;
pub mod data_permission;
mod db_type;
pub mod dialect;
#[cfg(feature = "prod-dialect-security")]
pub mod dialect_security;
pub mod dirty_attributes;
#[cfg(feature = "dist-cache")]
pub mod dist_cache;
#[cfg(feature = "dist-cache-cluster")]
#[allow(missing_docs)]
pub mod dist_cache_cluster;
pub mod dynamic_filter;
pub mod dynamic_sql;
pub mod eager_loader;
pub mod entity_graph;
mod error;
pub mod find_with_related;
#[cfg(feature = "compile-governance")]
pub mod governance;
pub mod guard;
pub mod hooks;
#[cfg(feature = "composable-plugin")]
pub use hooks::{ExtensionHandler, ExtensionPoint, ExtensionPointRegistry};
pub mod hydration_plugin;
pub mod i18n;
pub mod join_dsl;
pub mod json_query;
#[cfg(feature = "l1-cache")]
pub mod l1_cache;
pub mod l2_cache;
pub mod lambda;
pub mod lazy_loader;
pub mod linq;
pub mod migration;
#[cfg(feature = "migration-dry-run")]
pub mod migration_dry_run;
pub mod mock;
mod model;
#[cfg(feature = "multi-tenant-pool")]
#[allow(missing_docs)]
pub mod multi_tenant_pool;
pub mod n1_eliminator;
pub mod nested_active_model;
pub mod observer;
pub mod optimistic_lock;
pub mod paginator;
pub mod partial_model;
pub mod phinx_migration;
#[cfg(feature = "plan-cache")]
pub mod plan_cache;
pub mod plugin;
#[cfg(feature = "composable-plugin")]
pub use plugin::{MiddlewareChain, PanicSafeRegistry, PluginSigner, PluginState, SignatureStatus};
mod pool;
#[cfg(feature = "prepared-stmt-cache")]
pub mod prepared_cache;
#[cfg(feature = "auto-prewarm")]
pub mod prewarm;
#[cfg(feature = "prod-ready")]
pub mod prod_ready_check;
mod query;
pub mod query_cache;
#[cfg(feature = "query-result-cache")]
#[allow(missing_docs)]
pub mod query_result_cache;
#[cfg(feature = "async-row-stream")]
pub mod row_stream;
#[cfg(feature = "rw-split-enhanced")]
#[allow(missing_docs)]
pub mod rw_split_enhanced;
#[cfg(feature = "saga-tx")]
#[allow(missing_docs)]
pub mod saga;

#[cfg(feature = "pool-elastic")]
#[allow(missing_docs)]
pub mod pool_elastic;

#[cfg(feature = "serverless-adapt")]
pub use pool_elastic::{
    CdcCheckpoint, GracefulShutdown, GracefulShutdownConfig, ShutdownError, ShutdownResult,
};
#[cfg(feature = "serverless-adapt")]
pub use prewarm::{ColdStartOptimizer, ColdStartStats};

#[cfg(feature = "io-uring")]
#[allow(missing_docs)]
pub mod io_uring_probe;

#[cfg(feature = "field-encryption")]
#[allow(missing_docs)]
pub mod field_cipher;
#[cfg(feature = "tde-interceptor")]
pub use field_cipher::{TdeError, TdeInterceptor};
#[cfg(feature = "tde-interceptor")]
pub use sz_orm_crypto::{
    ColumnCryptoConfig, ColumnEncryptionPolicy, DekBuffer, EncryptionAlgo, KmsClient,
    LocalKmsClient,
};

#[cfg(feature = "data-validation")]
pub mod validation;
/// Re-export QueryBuilder for external use
pub use query::QueryBuilder;
#[cfg(feature = "adaptive-query")]
pub mod adaptive_adapter;
#[cfg(feature = "cache-coherence")]
pub mod cache_coherence;
#[cfg(feature = "l1-cache")]
#[allow(missing_docs)]
pub mod cache_warmup_protection;
#[cfg(feature = "config-center")]
pub mod config_adapter;
#[cfg(feature = "connection-level-tenant")]
pub mod connection_tenant;
#[cfg(feature = "forward-compat-sandbox")]
#[allow(missing_docs)]
pub mod forward_compat_sandbox;
#[cfg(feature = "graph")]
pub mod graph_adapter;
#[cfg(feature = "graphql")]
pub mod graphql_adapter;
#[cfg(feature = "structured-logging")]
pub mod logger_adapter;
#[cfg(feature = "postgis")]
pub mod postgis_adapter;
#[cfg(feature = "read-write-splitting")]
pub mod rw_adapter;
#[cfg(feature = "search")]
pub mod search_adapter;
#[cfg(feature = "timeseries")]
pub mod timeseries_adapter;
#[cfg(feature = "distributed-tracing")]
pub mod tracing_adapter;

#[cfg(feature = "migration-branch")]
pub mod migration_branch;
#[cfg(feature = "l1-cache")]
pub mod process_l1_cache;
#[cfg(feature = "qb-migration-tool")]
pub mod qb_migration_fix;
#[cfg(feature = "qb-migration-tool")]
pub mod qb_migration_lint;
pub mod queryable;
pub mod quick_query;
pub mod rate_limiter;
pub mod relation_trait;
pub mod repository;
pub mod result_map;
pub mod retry;
#[cfg(feature = "zero-downtime-rollback")]
pub mod rollback_zero_downtime;
#[cfg(feature = "schema-diff-viz")]
pub mod schema_diff_viz;
pub mod schema_gen;
pub mod schema_sync;
#[cfg(feature = "data-seeding")]
pub mod seeding;
pub mod select_types;
pub mod shadow;
#[cfg(feature = "simd")]
pub mod simd;
pub mod smart_eager_loader;
pub mod sql_buffer;
pub mod sql_safety;
#[cfg(feature = "sql-verify-proc")]
pub mod sql_verify;
pub mod stream_api;
#[cfg(feature = "streaming-export")]
pub mod streaming_export;
pub mod telemetry;
#[cfg(feature = "multi-tenant-enhanced")]
pub mod tenant_context;
#[cfg(feature = "tenant-quota-rls-enhanced")]
#[allow(missing_docs)]
pub mod tenant_quota_rls;
#[cfg(feature = "multi-tenant-enhanced")]
pub mod tenant_security;
mod transaction;
pub mod type_handler;
pub mod typed;
pub mod typed_ast;
#[cfg(feature = "typed-relation")]
pub mod typed_relation;
mod value;
#[cfg(feature = "zero-copy")]
pub mod value_borrowed;
#[cfg(feature = "zero-copy-deep")]
#[allow(missing_docs)]
pub mod zero_copy_pipeline;

// v7.4.0 任务 3.5：性能指标暴露（perf_metrics 模块始终可用）
pub mod perf_metrics;

#[cfg(any(
    feature = "cdc-mysql",
    feature = "cdc-postgres",
    feature = "cdc-sqlite",
    feature = "cdc-realtime-sync"
))]
#[allow(missing_docs)]
pub mod cdc;

#[cfg(feature = "rbac-abac-enhanced")]
#[allow(missing_docs)]
pub mod column_mask_interceptor;

#[cfg(feature = "rbac-abac-enhanced")]
#[allow(missing_docs)]
pub mod row_level_policy;

#[cfg(feature = "olap-vectorized")]
#[allow(missing_docs)]
pub mod olap;

#[cfg(feature = "executor-opt")]
#[allow(missing_docs)]
pub mod executor_passes;

// Re-export proc macros
pub use queryable::Query;
pub use queryable::QueryAs;
pub use sz_orm_macros::api_beta;
pub use sz_orm_macros::api_stable;
pub use sz_orm_macros::migrate;
pub use sz_orm_macros::query;
pub use sz_orm_macros::query_as;
pub use sz_orm_macros::schema;
pub use sz_orm_macros::sql_string;
pub use sz_orm_macros::typed_query;
// FromQueryResult derive 宏（与 value.rs 中同名 trait 通过显式 use 遮蔽 glob 导出）
#[cfg(feature = "n1-lint")]
pub use sz_orm_macros::detect_n_plus_one;
pub use sz_orm_macros::FromQueryResult;
pub use sz_orm_macros::RelationTrait;
#[cfg(feature = "data-validation")]
pub use sz_orm_macros::Validate;

pub use change_tracker::{ChangeTracker, EntityEntry, EntityState};
pub use lazy_loader::{LazyCollection, LazyLoader, LazyRef};
pub use linq::LinqQuery;
pub use query_cache::{QueryCache, QueryCacheKey, TimestampCache};

pub use cache::*;
pub use cycle_detection::{CycleDetector, CyclePolicy};
pub use db_type::*;
#[allow(ambiguous_glob_reexports)]
pub use dialect::*;
pub use eager_loader::NestedEagerResult;
pub use error::*;
#[allow(ambiguous_glob_reexports)]
pub use migration::*;
pub use model::*;
pub use nested_active_model::CascadeStrategy;
pub use pool::*;
#[allow(unused_imports)]
pub use query::*;
pub use schema_sync::{Confirm, DataMigrationHook, DestructiveSyncResult};
pub use transaction::*;
pub use value::*;

/// Alias for `Arc<T>`
pub type Shared<T> = Arc<T>;

/// Alias for `Box<T>`
pub type Boxed<T> = Box<T>;

/// Alias for Result<T, DbError>
pub type DbResult<T> = Result<T, DbError>;

/// Result type for pool operations
pub type PoolResult<T> = Result<T, PoolError>;

/// Result type for cache operations
pub type CacheResult<T> = Result<T, CacheError>;

/// Result type for transaction operations
pub type TxResult<T> = Result<T, TxError>;

/// Default batch size for bulk operations
pub const DEFAULT_BATCH_SIZE: usize = 1000;

/// Default connection timeout in seconds
pub const DEFAULT_ACQUIRE_TIMEOUT: u64 = 30;

/// Default idle timeout in seconds
pub const DEFAULT_IDLE_TIMEOUT: u64 = 600;

/// Default max lifetime in seconds
pub const DEFAULT_MAX_LIFETIME: u64 = 1800;

/// Default minimum idle connections
pub const DEFAULT_MIN_IDLE: u32 = 5;

/// Default maximum pool size
pub const DEFAULT_MAX_SIZE: u32 = 100;

// ============================================================================
// v7.3.0 性能加速配置与指标聚合（perf-accel feature gate）
// ============================================================================

/// 性能加速配置（v7.3.0）
///
/// 统一驱动 SIMD 向量化、零拷贝序列化、连接池预热、查询计划缓存四项加速能力。
/// 所有加速开关默认 false（plan_cache_enabled 默认 true 保持既有行为），
/// 不改变 v7.2.0 既有行为。
#[cfg(feature = "perf-accel")]
#[derive(Debug, Clone)]
pub struct PerfConfig {
    /// 是否启用 SIMD 向量化加速
    pub simd_enabled: bool,
    /// SIMD 批量处理最低行数阈值（低于此值走标量路径）
    pub simd_row_threshold: usize,
    /// 是否启用零拷贝序列化
    pub zero_copy_enabled: bool,
    /// 是否启用连接池预热
    pub prewarm_enabled: bool,
    /// 预热连接数
    pub prewarm_count: usize,
    /// 是否启用查询计划缓存
    pub plan_cache_enabled: bool,
    /// 计划缓存容量
    pub plan_cache_capacity: usize,
    /// 计划缓存 TTL（毫秒）
    pub plan_cache_ttl_ms: u64,
}

#[cfg(feature = "perf-accel")]
impl Default for PerfConfig {
    fn default() -> Self {
        Self {
            simd_enabled: false,
            simd_row_threshold: 1024,
            zero_copy_enabled: false,
            prewarm_enabled: false,
            prewarm_count: 1,
            plan_cache_enabled: true,
            plan_cache_capacity: 256,
            plan_cache_ttl_ms: 300_000,
        }
    }
}

#[cfg(feature = "perf-accel")]
impl PerfConfig {
    /// 校验配置合法性
    pub fn validate(&self) -> Result<(), DbError> {
        if self.simd_row_threshold < 1 {
            return Err(DbError::ConfigError(
                "simd_row_threshold must be >= 1".to_string(),
            ));
        }
        if self.prewarm_count < 1 {
            return Err(DbError::ConfigError(
                "prewarm_count must be >= 1".to_string(),
            ));
        }
        if self.plan_cache_capacity < 1 {
            return Err(DbError::ConfigError(
                "plan_cache_capacity must be >= 1".to_string(),
            ));
        }
        if self.plan_cache_ttl_ms == 0 {
            return Err(DbError::ConfigError(
                "plan_cache_ttl_ms must be > 0".to_string(),
            ));
        }
        Ok(())
    }

    /// 创建 builder
    pub fn builder() -> PerfConfigBuilder {
        PerfConfigBuilder::default()
    }
}

/// 性能加速配置 builder（v7.3.0）
#[cfg(feature = "perf-accel")]
#[derive(Debug, Clone, Default)]
pub struct PerfConfigBuilder {
    config: PerfConfig,
}

#[cfg(feature = "perf-accel")]
impl PerfConfigBuilder {
    /// 设置 SIMD 启用
    pub fn simd(mut self, enabled: bool) -> Self {
        self.config.simd_enabled = enabled;
        self
    }

    /// 设置 SIMD 行阈值
    pub fn simd_row_threshold(mut self, threshold: usize) -> Self {
        self.config.simd_row_threshold = threshold;
        self
    }

    /// 设置零拷贝启用
    pub fn zero_copy(mut self, enabled: bool) -> Self {
        self.config.zero_copy_enabled = enabled;
        self
    }

    /// 设置预热启用
    pub fn prewarm(mut self, enabled: bool) -> Self {
        self.config.prewarm_enabled = enabled;
        self
    }

    /// 设置预热连接数
    pub fn prewarm_count(mut self, count: usize) -> Self {
        self.config.prewarm_count = count;
        self
    }

    /// 设置计划缓存启用
    pub fn plan_cache(mut self, enabled: bool) -> Self {
        self.config.plan_cache_enabled = enabled;
        self
    }

    /// 设置计划缓存容量
    pub fn plan_cache_capacity(mut self, capacity: usize) -> Self {
        self.config.plan_cache_capacity = capacity;
        self
    }

    /// 设置计划缓存 TTL（毫秒）
    pub fn plan_cache_ttl_ms(mut self, ttl_ms: u64) -> Self {
        self.config.plan_cache_ttl_ms = ttl_ms;
        self
    }

    /// 构建配置（校验合法性）
    pub fn build(self) -> Result<PerfConfig, DbError> {
        self.config.validate()?;
        Ok(self.config)
    }
}

/// 性能加速指标快照（v7.3.0）
#[cfg(feature = "perf-accel")]
#[derive(Debug, Clone, Default)]
pub struct PerfMetricsSnapshot {
    /// SIMD 命中次数
    pub simd_hit_count: u64,
    /// SIMD 未命中次数
    pub simd_miss_count: u64,
    /// SIMD 延迟降低百分比
    pub simd_latency_reduction_pct: f64,
    /// 零拷贝命中次数
    pub zero_copy_hit_count: u64,
    /// 零拷贝 RSS 降低百分比
    pub zero_copy_rss_reduction_pct: f64,
    /// 预热成功次数
    pub prewarm_success_count: u64,
    /// 计划缓存命中率
    pub plan_cache_hit_rate: f64,
    /// 计划缓存淘汰次数
    pub plan_cache_eviction_count: u64,
}

/// 性能加速指标聚合（v7.3.0）
///
/// 使用无锁原子计数器采集 SIMD/零拷贝/预热/计划缓存四项加速能力的运行指标。
#[cfg(feature = "perf-accel")]
#[derive(Debug, Default)]
pub struct PerfMetrics {
    simd_hit_count: std::sync::atomic::AtomicU64,
    simd_miss_count: std::sync::atomic::AtomicU64,
    simd_latency_reduction_pct: std::sync::atomic::AtomicU64,
    zero_copy_hit_count: std::sync::atomic::AtomicU64,
    zero_copy_rss_reduction_pct: std::sync::atomic::AtomicU64,
    prewarm_success_count: std::sync::atomic::AtomicU64,
    plan_cache_hit_rate: std::sync::atomic::AtomicU64,
    plan_cache_eviction_count: std::sync::atomic::AtomicU64,
}

#[cfg(feature = "perf-accel")]
impl PerfMetrics {
    /// 创建新的指标实例
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录 SIMD 命中
    pub fn record_simd_hit(&self) {
        self.simd_hit_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// 记录 SIMD 未命中
    pub fn record_simd_miss(&self) {
        self.simd_miss_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// 记录零拷贝命中
    pub fn record_zero_copy_hit(&self) {
        self.zero_copy_hit_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// 记录预热成功
    pub fn record_prewarm_success(&self) {
        self.prewarm_success_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// 记录计划缓存淘汰
    pub fn record_plan_cache_eviction(&self) {
        self.plan_cache_eviction_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// 设置 SIMD 延迟降低百分比
    pub fn set_simd_latency_reduction_pct(&self, pct: f64) {
        self.simd_latency_reduction_pct
            .store(pct.to_bits(), std::sync::atomic::Ordering::Relaxed);
    }

    /// 设置零拷贝 RSS 降低百分比
    pub fn set_zero_copy_rss_reduction_pct(&self, pct: f64) {
        self.zero_copy_rss_reduction_pct
            .store(pct.to_bits(), std::sync::atomic::Ordering::Relaxed);
    }

    /// 设置计划缓存命中率
    pub fn set_plan_cache_hit_rate(&self, rate: f64) {
        self.plan_cache_hit_rate
            .store(rate.to_bits(), std::sync::atomic::Ordering::Relaxed);
    }

    /// 采集指标快照
    pub fn snapshot(&self) -> PerfMetricsSnapshot {
        let o = std::sync::atomic::Ordering::Relaxed;
        PerfMetricsSnapshot {
            simd_hit_count: self.simd_hit_count.load(o),
            simd_miss_count: self.simd_miss_count.load(o),
            simd_latency_reduction_pct: f64::from_bits(self.simd_latency_reduction_pct.load(o)),
            zero_copy_hit_count: self.zero_copy_hit_count.load(o),
            zero_copy_rss_reduction_pct: f64::from_bits(self.zero_copy_rss_reduction_pct.load(o)),
            prewarm_success_count: self.prewarm_success_count.load(o),
            plan_cache_hit_rate: f64::from_bits(self.plan_cache_hit_rate.load(o)),
            plan_cache_eviction_count: self.plan_cache_eviction_count.load(o),
        }
    }
}

// ============================================================================
// v7.3.0 高可用配置聚合（auto-failover feature gate）
// ============================================================================

/// 回切策略（v7.3.0）
///
/// - `Manual`：故障转移后等待运维确认再回切
/// - `Auto`：经健康+一致性校验后自动回切
#[cfg(feature = "auto-failover")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FailbackStrategy {
    /// 等待运维确认再回切
    Manual,
    /// 经健康+一致性校验后自动回切
    Auto,
}

/// 故障转移配置（v7.3.0）
///
/// 凭证经既有配置加密链路加载（禁止明文）。
#[cfg(feature = "auto-failover")]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FailoverConfig {
    /// 主库连接 URL（已加密，运行时解密）
    pub primary_url: String,
    /// 备库连接 URL（已加密，运行时解密）
    pub replica_url: String,
    /// 探活间隔（必须 ≤ 5s 以保证 RTO ≤ 5s）
    pub probe_interval: std::time::Duration,
    /// 连续探活失败多少次触发故障转移（默认 3）
    pub probe_failure_threshold: u32,
    /// 回切策略
    pub failback_strategy: FailbackStrategy,
}

#[cfg(feature = "auto-failover")]
impl Default for FailoverConfig {
    fn default() -> Self {
        Self {
            primary_url: String::new(),
            replica_url: String::new(),
            probe_interval: std::time::Duration::from_secs(1),
            probe_failure_threshold: 3,
            failback_strategy: FailbackStrategy::Manual,
        }
    }
}

/// 高可用配置聚合（v7.3.0）
///
/// 统一驱动故障转移、限流熔断、健康检查、追踪四项高可用能力。
/// `failover_enabled` 默认 false，不改变单库行为。
#[cfg(feature = "auto-failover")]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HaConfig {
    /// 是否启用故障转移（默认 false，单库行为不变）
    pub failover_enabled: bool,
    /// 故障转移配置（failover_enabled=false 时为 None）
    pub failover: Option<FailoverConfig>,
    /// 限流阈值（每秒请求数，0 表示不限）
    pub rate_limit_threshold: f64,
    /// 限流排队超时（毫秒，默认 100）
    pub rate_limit_queue_timeout_ms: u64,
    /// 熔断错误率阈值 ∈ (0,1)，默认 0.5
    pub circuit_breaker_error_threshold: f64,
    /// 半开探测请求数（≥ 1）
    pub circuit_breaker_half_open_probes: u32,
    /// 追踪采样率 ∈ \[0,1\]
    pub trace_sample_rate: f64,
    /// OTLP 导出端点（None 表示不导出）
    pub trace_otlp_endpoint: Option<String>,
}

#[cfg(feature = "auto-failover")]
impl Default for HaConfig {
    fn default() -> Self {
        Self {
            failover_enabled: false,
            failover: None,
            rate_limit_threshold: 0.0,
            rate_limit_queue_timeout_ms: 100,
            circuit_breaker_error_threshold: 0.5,
            circuit_breaker_half_open_probes: 1,
            trace_sample_rate: 1.0,
            trace_otlp_endpoint: None,
        }
    }
}

#[cfg(feature = "auto-failover")]
impl HaConfig {
    /// 校验配置合法性
    pub fn validate(&self) -> Result<(), DbError> {
        if let Some(failover) = &self.failover {
            if failover.probe_interval > std::time::Duration::from_secs(5) {
                return Err(DbError::ConfigError(
                    "probe_interval must be <= 5s for RTO <= 5s".to_string(),
                ));
            }
            if failover.probe_failure_threshold == 0 {
                return Err(DbError::ConfigError(
                    "probe_failure_threshold must be >= 1".to_string(),
                ));
            }
        }
        if self.circuit_breaker_error_threshold <= 0.0
            || self.circuit_breaker_error_threshold >= 1.0
        {
            return Err(DbError::ConfigError(
                "circuit_breaker_error_threshold must be in (0, 1)".to_string(),
            ));
        }
        if self.circuit_breaker_half_open_probes == 0 {
            return Err(DbError::ConfigError(
                "circuit_breaker_half_open_probes must be >= 1".to_string(),
            ));
        }
        if self.trace_sample_rate < 0.0 || self.trace_sample_rate > 1.0 {
            return Err(DbError::ConfigError(
                "trace_sample_rate must be in [0, 1]".to_string(),
            ));
        }
        Ok(())
    }

    /// 创建 builder
    pub fn builder() -> HaConfigBuilder {
        HaConfigBuilder::default()
    }
}

/// 高可用配置 builder（v7.3.0）
#[cfg(feature = "auto-failover")]
#[derive(Debug, Clone, Default)]
pub struct HaConfigBuilder {
    config: HaConfig,
}

#[cfg(feature = "auto-failover")]
impl HaConfigBuilder {
    /// 设置故障转移启用
    pub fn failover_enabled(mut self, enabled: bool) -> Self {
        self.config.failover_enabled = enabled;
        self
    }

    /// 设置故障转移配置
    pub fn failover(mut self, config: FailoverConfig) -> Self {
        self.config.failover = Some(config);
        self
    }

    /// 设置限流阈值
    pub fn rate_limit_threshold(mut self, threshold: f64) -> Self {
        self.config.rate_limit_threshold = threshold;
        self
    }

    /// 设置限流排队超时（毫秒）
    pub fn rate_limit_queue_timeout_ms(mut self, ms: u64) -> Self {
        self.config.rate_limit_queue_timeout_ms = ms;
        self
    }

    /// 设置熔断错误率阈值
    pub fn circuit_breaker_error_threshold(mut self, threshold: f64) -> Self {
        self.config.circuit_breaker_error_threshold = threshold;
        self
    }

    /// 设置半开探测请求数
    pub fn circuit_breaker_half_open_probes(mut self, probes: u32) -> Self {
        self.config.circuit_breaker_half_open_probes = probes;
        self
    }

    /// 设置追踪采样率
    pub fn trace_sample_rate(mut self, rate: f64) -> Self {
        self.config.trace_sample_rate = rate;
        self
    }

    /// 设置 OTLP 导出端点
    pub fn trace_otlp_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.config.trace_otlp_endpoint = Some(endpoint.into());
        self
    }

    /// 构建配置（校验合法性）
    pub fn build(self) -> Result<HaConfig, DbError> {
        self.config.validate()?;
        Ok(self.config)
    }
}

// ============================================================================
// v7.3.0 任务 4.1：EcoConfig 生态扩展配置聚合
// ============================================================================

/// Web 框架枚举（v7.3.0 生态扩展）
#[cfg(feature = "eco-config")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum WebFramework {
    /// axum
    Axum,
    /// actix-web
    Actix,
    /// warp
    Warp,
}

/// 中间件特性枚举（v7.3.0 生态扩展）
#[cfg(feature = "eco-config")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MiddlewareFeature {
    /// 连接池注入
    PoolInject,
    /// 事务
    Transaction,
    /// 限流
    RateLimit,
    /// 追踪
    Tracing,
    /// 健康端点
    HealthEndpoint,
}

/// 源 ORM 枚举（v7.3.0 生态扩展，用于迁移源识别）
#[cfg(feature = "eco-config")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SourceOrm {
    /// Diesel
    Diesel,
    /// SeaORM
    SeaOrm,
    /// SQLx
    Sqlx,
}

/// 生态扩展配置聚合（v7.3.0 任务 4.1）
///
/// 统一驱动 warp 中间件适配、源 ORM 迁移、schema diff 三项生态扩展能力。
/// `migration_dry_run` 默认 true（不自动执行 DDL）。
#[cfg(feature = "eco-config")]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EcoConfig {
    /// Web 框架（默认 Axum）
    pub web_framework: WebFramework,
    /// 启用的中间件特性列表
    pub middleware_features: Vec<MiddlewareFeature>,
    /// 迁移源 ORM（None 表示不从其他 ORM 迁移）
    pub migration_source_orm: Option<SourceOrm>,
    /// 迁移 dry-run 模式（默认 true，不自动执行 DDL）
    pub migration_dry_run: bool,
    /// schema diff 左库 URL（None 表示不启用 schema diff）
    pub schema_diff_left_url: Option<String>,
    /// schema diff 右库 URL
    pub schema_diff_right_url: Option<String>,
}

#[cfg(feature = "eco-config")]
impl Default for EcoConfig {
    fn default() -> Self {
        Self {
            web_framework: WebFramework::Axum,
            middleware_features: Vec::new(),
            migration_source_orm: None,
            migration_dry_run: true,
            schema_diff_left_url: None,
            schema_diff_right_url: None,
        }
    }
}

#[cfg(feature = "eco-config")]
impl EcoConfig {
    /// 校验配置合法性
    pub fn validate(&self) -> Result<(), DbError> {
        // schema diff：left/right 必须同时提供或同时缺失
        if self.schema_diff_left_url.is_some() != self.schema_diff_right_url.is_some() {
            return Err(DbError::ConfigError(
                "schema_diff_left_url 和 schema_diff_right_url 必须同时提供或同时缺失".to_string(),
            ));
        }
        // 迁移源 ORM 提供时，dry_run 允许 true/false（不强制），但需提示
        Ok(())
    }

    /// 创建 builder
    pub fn builder() -> EcoConfigBuilder {
        EcoConfigBuilder::default()
    }
}

/// 生态扩展配置 builder（v7.3.0 任务 4.1）
#[cfg(feature = "eco-config")]
#[derive(Debug, Clone, Default)]
pub struct EcoConfigBuilder {
    config: EcoConfig,
}

#[cfg(feature = "eco-config")]
impl EcoConfigBuilder {
    /// 设置 Web 框架
    pub fn web_framework(mut self, fw: WebFramework) -> Self {
        self.config.web_framework = fw;
        self
    }

    /// 设置中间件特性列表
    pub fn middleware_features(mut self, features: Vec<MiddlewareFeature>) -> Self {
        self.config.middleware_features = features;
        self
    }

    /// 设置迁移源 ORM
    pub fn migration_source_orm(mut self, orm: SourceOrm) -> Self {
        self.config.migration_source_orm = Some(orm);
        self
    }

    /// 设置迁移 dry-run
    pub fn migration_dry_run(mut self, dry_run: bool) -> Self {
        self.config.migration_dry_run = dry_run;
        self
    }

    /// 设置 schema diff 左库 URL
    pub fn schema_diff_left_url(mut self, url: impl Into<String>) -> Self {
        self.config.schema_diff_left_url = Some(url.into());
        self
    }

    /// 设置 schema diff 右库 URL
    pub fn schema_diff_right_url(mut self, url: impl Into<String>) -> Self {
        self.config.schema_diff_right_url = Some(url.into());
        self
    }

    /// 构建配置（校验合法性）
    pub fn build(self) -> Result<EcoConfig, DbError> {
        self.config.validate()?;
        Ok(self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn test_db_type() {
        assert_eq!(DbType::MySQL.as_str(), "mysql");
        assert_eq!(DbType::PostgreSQL.as_str(), "postgres");
        assert_eq!(DbType::Sqlite.as_str(), "sqlite");
    }

    #[test]
    fn test_value() {
        let v = Value::Null;
        assert!(v.is_null());

        let v = Value::I64(42);
        assert!(v.is_i64());

        let v = Value::String("hello".to_string());
        assert!(v.is_string());
    }

    #[test]
    fn test_db_error_display() {
        let err = DbError::QueryError("test query failed".to_string());
        assert_eq!(format!("{}", err), "Query error: test query failed");

        let err = DbError::ConnectionRefused("localhost".to_string());
        assert_eq!(format!("{}", err), "Connection refused: localhost");
    }

    #[test]
    fn test_db_error_source() {
        let err = DbError::PoolError(PoolError::Timeout);
        assert!(err.source().is_some());
    }

    #[tokio::test]
    async fn test_async_trait_export() {
        fn _check_send_sync<T: Send + Sync>() {}

        struct TestImpl;
        #[async_trait]
        trait AsyncFoo: Send + Sync {
            async fn foo(&self);
        }

        #[async_trait]
        impl AsyncFoo for TestImpl {
            async fn foo(&self) {}
        }

        let impl_ = TestImpl;
        impl_.foo().await;
        _check_send_sync::<TestImpl>();
    }

    // ---- compile-time SQL validation macro tests ----

    /// Valid SQL should compile and be usable as a string
    #[test]
    fn test_sql_string_valid_select() {
        let sql = sql_string!("SELECT * FROM users WHERE id = 1");
        assert!(sql.contains("SELECT"));
        assert!(sql.contains("FROM"));
    }

    #[test]
    fn test_sql_string_valid_insert() {
        let sql = sql_string!("INSERT INTO users (name) VALUES ('alice')");
        assert!(sql.contains("INSERT"));
    }

    #[test]
    fn test_sql_string_valid_update() {
        let sql = sql_string!("UPDATE users SET name = 'bob' WHERE id = 1");
        assert!(sql.contains("UPDATE"));
    }

    #[test]
    fn test_sql_string_valid_delete() {
        let sql = sql_string!("DELETE FROM users WHERE id = 1");
        assert!(sql.contains("DELETE"));
    }

    #[test]
    fn test_sql_string_valid_create() {
        let sql = sql_string!("CREATE TABLE test (id INT PRIMARY KEY)");
        assert!(sql.contains("CREATE"));
    }

    #[test]
    fn test_sql_string_with_params() {
        let sql = sql_string!("SELECT * FROM users WHERE id = ?"; params: 1);
        assert!(sql.contains("?"));
    }

    #[test]
    fn test_sql_string_complex_query() {
        let sql = sql_string!(
            "SELECT u.*, o.total FROM users u LEFT JOIN orders o ON u.id = o.user_id WHERE u.status = 'active'"
        );
        assert!(sql.contains("LEFT JOIN"));
    }

    #[test]
    fn test_sql_string_nested_parens() {
        let sql = sql_string!("SELECT * FROM (SELECT * FROM users) t");
        assert!(sql.contains("SELECT"));
    }
}
