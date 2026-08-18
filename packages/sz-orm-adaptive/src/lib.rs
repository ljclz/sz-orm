//! # sz-orm-adaptive — 运行时自适应查询优化器
//!
//! 无 AI 依赖的轻量运行时自适应：采集查询统计（原子计数，无锁），
//! 按阈值自动切换执行路径：
//!
//! - **自动分页**：平均行数超阈值 → 建议切换到游标分页
//! - **热点缓存**：慢查询 + 高频执行 → 自动读缓存（需显式开启，防脏读）
//! - **慢查询标记**：单次执行超时阈值 → 标记 slow 并返回结构化信息
//!
//! 典型用法（配合既有 `sz-orm-core` 的 `cursor_stream` / `l2_cache`）：
//!
//! ```rust,ignore
//! let executor = AdaptiveExecutor::new(AdaptiveConfig::default());
//! match executor.decide("find_users") {
//!     ExecutionPath::Paginated => { /* 用 cursor_stream 游标分页 */ }
//!     _ => { /* 正常查询 */ }
//! }
//! executor.record("find_users", rows, elapsed_ms);
//! ```
//!
//! 本包零依赖 `sz-orm-core`：决策层只做统计与建议，具体执行路径
//! （游标分页/缓存写入）由调用方复用既有实现完成，避免依赖循环。

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
