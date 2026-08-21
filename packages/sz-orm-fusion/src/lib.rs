//! # sz-orm-fusion — Multi-Database Fusion Query (experimental, not for production use)
//!
//! **Experimental POC** — this package is an optional experiment for v4.3.0 M5.
//! It is not intended for production use and may change or be removed in future versions.
//!
//! Transparent multi-database operations: query splitting, aggregation, and degradation for **primary + cache + search**.
//! Specific backends (Redis / vector store) are injected via traits, not bound to implementations.
//!
//! Core concepts:
//!
//! - [`FusionQuery`]: Structured query description (table + parameterized equality conditions + other conditions)
//! - [`FusionPlanner`]: Static analysis → [`FusionPlan`] (cache pushdown / search pushdown / primary steps)
//! - [`FusionExecutor`]: Execute by plan, cache hit skips primary, primary failure falls back to cache
//!
//! ```rust,ignore
//! let config = FusionConfig {
//!     primary: "mysql".into(),
//!     cache: Some(CacheBackend::Memory),
//!     search: None,
//! };
//! let ex = FusionExecutor::new(config).with_cache(Arc::new(MemoryFusionCache::new()));
//! let q = FusionQuery::new("users").eq("id", "42");
//! let out = ex.execute(&q, |q| primary_query(q)).unwrap();
//! assert!(!out.degraded);
//! ```

#[cfg(feature = "db-fusion-v2")]
pub mod cdc_sync;
#[cfg(feature = "db-fusion")]
pub mod conflict;
#[cfg(feature = "db-fusion")]
pub mod executor;
#[cfg(feature = "db-fusion")]
pub mod health_check;
#[cfg(feature = "db-fusion-v2")]
pub mod migration;
#[cfg(feature = "db-fusion")]
pub mod plan;
#[cfg(feature = "db-fusion")]
pub mod routing;
#[cfg(feature = "db-fusion")]
pub mod stats;
#[cfg(feature = "db-fusion")]
pub mod sync;
#[cfg(feature = "db-fusion-v2")]
pub mod ttl_cache;
#[cfg(feature = "db-fusion-v2")]
pub mod vector_pushdown;

#[cfg(feature = "db-fusion-v2")]
pub use cdc_sync::{CdcSyncCoordinator, SyncOutcome};
#[cfg(feature = "db-fusion")]
pub use conflict::{
    Conflict, ConflictLog, ConflictResolver, ConflictType, DataVersion, Resolution,
    ResolutionStrategy,
};
#[cfg(feature = "db-fusion")]
#[allow(deprecated)]
pub use executor::{FusionCache, FusionExecutor, FusionOutcome, MemoryFusionCache};
#[cfg(feature = "db-fusion")]
pub use health_check::{
    HealthCheckResult, HealthCheckScheduler, HealthChecker, HealthRecord, HealthStatus,
};
#[cfg(feature = "db-fusion-v2")]
pub use migration::{migration_guide, migration_steps, MigrationStep};
#[cfg(feature = "db-fusion")]
pub use plan::{FusionPlan, FusionPlanner, FusionQuery, PlanStep};
#[cfg(feature = "db-fusion")]
pub use routing::{
    AffinityRoutingStrategy, DataSource, QueryRouter, QueryType, RoutingDecision, RoutingStrategy,
    SourceRole, WeightedRoundRobinStrategy,
};
#[cfg(feature = "db-fusion")]
pub use stats::{FusionReport, FusionStats, FusionStatsCollector, SourceStats, TableStats};
#[cfg(feature = "db-fusion")]
pub use sync::{
    DataSynchronizer, SyncDirection, SyncResult, SyncScheduler, SyncState, SyncStats, SyncTask,
};
#[cfg(feature = "db-fusion-v2")]
pub use ttl_cache::TtlFusionCache;
#[cfg(feature = "db-fusion-v2")]
pub use vector_pushdown::{VectorPushdownExecutor, VectorPushdownOutcome};
