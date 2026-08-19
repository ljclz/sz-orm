//! # sz-orm-adaptive — Runtime Adaptive Query Optimizer
//!
//! Lightweight runtime adaptation without AI dependencies: collects query statistics (atomic counters, lock-free),
//! automatically switches execution paths based on thresholds:
//!
//! - **Auto pagination**: Average row count exceeds threshold → suggest switching to cursor pagination
//! - **Hot cache**: Slow query + high frequency → auto read cache (requires explicit enable, prevents dirty read)
//! - **Slow query marker**: Single execution timeout threshold → mark slow and return structured info
//!
//! Typical usage (with existing `sz-orm-core` `cursor_stream` / `l2_cache`):
//!
//! ```rust,ignore
//! let executor = AdaptiveExecutor::new(AdaptiveConfig::default());
//! match executor.decide("find_users") {
//!     ExecutionPath::Paginated => { /* use cursor_stream for cursor pagination */ }
//!     _ => { /* normal query */ }
//! }
//! executor.record("find_users", rows, elapsed_ms);
//! ```
//!
//! This package has zero dependency on `sz-orm-core`: the decision layer only does statistics and suggestions,
//! the actual execution paths (cursor pagination / cache write) are completed by the caller reusing existing implementations, avoiding circular dependencies.

pub mod complexity;
pub mod executor;
pub mod param_tuner;
pub mod planner;
pub mod stats;

pub use complexity::{
    AdaptiveIndexSelector, ComplexityLevel, IndexInfo, QueryComplexityEvaluator, QueryFeatures,
};
pub use executor::{
    AdaptiveConfig, AdaptiveExecutor, BatchSizeTuner, ExecutionPath, IndexSelectionStrategy,
    JoinOrderStrategy, MemoryTtlCache, QueryOutcome, ResultCache,
};
pub use param_tuner::{
    AdaptiveParameterTuner, PerformanceMetrics, SuggestionSeverity, TunableParam, TuningAdvisor,
    TuningEvent, TuningImpactEvaluator, TuningPlan, TuningPlanStep, TuningSignal, TuningStats,
    TuningStrategy, TuningSuggestion,
};
pub use planner::{
    AdaptiveQueryPlanner, CachedPlan, ExecutionPlanCache, PlannerConfig, QueryPlan, TableMetadata,
};
pub use stats::{QueryStats, SlidingWindowStats};
