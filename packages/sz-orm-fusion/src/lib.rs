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
    Conflict, ConflictLog, ConflictResolver, ConflictType, CustomResolveFn, DataVersion,
    Resolution, ResolutionStrategy, VectorClock,
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
// v7.0.0 multi-region 模块
#[cfg(feature = "chaos")]
pub mod chaos_injector;
#[cfg(feature = "multi-region")]
pub mod edge_node;
#[cfg(feature = "multi-region")]
pub mod global_router;
#[cfg(feature = "multi-region")]
pub mod region_failover;
#[cfg(feature = "multi-region")]
pub mod region_topology;
#[cfg(feature = "multi-region")]
pub mod replication_lag;

#[cfg(feature = "chaos")]
pub use chaos_injector::{
    ChaosConfig, ChaosInjector, CircuitState as ChaosCircuitState, CircuitTransition, FaultType,
    RecoveryTimeMeasurer, StabilityReport, WorkloadType as ChaosWorkloadType,
};
#[cfg(feature = "multi-region")]
pub use edge_node::{EdgeCacheStatus, EdgeNode, EdgeNodeRouter, EdgeRoutingPolicy, GeoLocation};
#[cfg(feature = "multi-region")]
pub use global_router::{
    ConsistencyLevel, GlobalRouter, LatencyStats, RouteDecision, RouteError, RouteRequest,
};
#[cfg(feature = "multi-region")]
pub use region_failover::{
    FailoverAuditLog, FailoverDecision, FailoverError, RegionFailoverCoordinator, RTO_TARGET,
};
#[cfg(feature = "multi-region")]
pub use region_topology::{
    DataAffinityPolicy, MultiRegionHealthView, RegionHealth, RegionNode, RegionRole,
    RegionTopology, ReplicationMode, TopologyError,
};
#[cfg(feature = "multi-region")]
pub use replication_lag::{
    LinkType, ReplicationLagTracker, CROSS_CONTINENT_THRESHOLD, SAME_CITY_THRESHOLD,
};
// v7.7.0 任务 3.2：ConflictAutoResolver + GeoRouter 导出
#[cfg(feature = "multi-region-enhanced")]
pub use conflict::{AutoResolutionResult, AutoResolutionStrategy, ConflictAutoResolver};
#[cfg(feature = "multi-region-enhanced")]
pub use global_router::GeoRouter;

// v7.7.0 任务 3.3：CDC 增强导出
#[cfg(feature = "cdc-enhanced")]
pub use cdc_sync::{
    CdcIncrementalSyncer, CdcResumeCoordinator, CdcSchemaSyncer, IncrementalSyncResult,
    ResumeResult, SchemaSyncResult,
};

// v7.7.0 任务 3.4：FailoverEnhancer 导出
#[cfg(feature = "failover-enhanced")]
pub use region_failover::{
    AutoRecoverCoordinator, DetectionDimension, FailoverEnhancedResult, FailoverEnhancer,
    RecoverResult,
};
