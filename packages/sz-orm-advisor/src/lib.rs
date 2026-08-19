//! # sz-orm-advisor — Query Auto-Optimization Advice Engine + Intelligent Closed-Loop Linkage
//!
//! Based on `query-advisor` feature, rule engine analyzes EXPLAIN plans + adaptive statistics,
//! generates six actionable optimization suggestions (AddIndex/DropIndex/UsePagination/EnableCache/RewriteQuery/AdjustPoolSize).
//!
//! Based on `query-intelligence-loop` feature, chains EXPLAIN → adaptive → diagnosis → advice four-step closed loop.

#[cfg(feature = "query-advisor")]
pub mod advisor;
#[cfg(feature = "query-advisor")]
pub mod dialect;
#[cfg(feature = "query-advisor")]
pub mod report;
#[cfg(feature = "query-advisor")]
pub mod rules;
#[cfg(feature = "query-advisor")]
pub mod suggestion;

#[cfg(feature = "query-advisor")]
pub mod index_usage_stats;
#[cfg(feature = "query-advisor")]
pub mod query_heat_tracker;
#[cfg(feature = "query-advisor")]
pub mod query_pattern_analyzer;
#[cfg(feature = "query-advisor")]
pub mod query_performance_ranker;
#[cfg(feature = "query-advisor")]
pub mod slow_query_analyzer;

#[cfg(feature = "query-advisor")]
pub use advisor::{AdvisorConfig, OptimizationAdvisor};
#[cfg(feature = "query-advisor")]
pub use suggestion::{
    OptimizationSuggestion, RiskLevel, SuggestionType, TuningSuggestion, TuningSuggestionType,
};

#[cfg(feature = "query-advisor")]
pub use index_usage_stats::{IndexStats, IndexUsageReport, IndexUsageStats, RedundantIndexPair};
#[cfg(feature = "query-advisor")]
pub use query_heat_tracker::{
    ErrorTrend, HeatQuerySummary, HeatReport, HeatSample, QueryHeatHistory, QueryHeatTracker,
    TimeWindow,
};
#[cfg(feature = "query-advisor")]
pub use query_pattern_analyzer::{
    QueryPatternAnalyzer, QueryPatternKind, QueryRecord, QueryTemplate,
};
#[cfg(feature = "query-advisor")]
pub use query_performance_ranker::{
    PerformanceReport, QueryMetrics, QueryPerformanceRanker, QuerySeverity, RankBy, RankEntry,
};
#[cfg(feature = "query-advisor")]
pub use slow_query_analyzer::{SlowQueryAnalyzer, SlowQueryEntry, SlowQueryStats};

#[cfg(feature = "query-intelligence-loop")]
pub mod intelligence_loop;
#[cfg(feature = "query-intelligence-loop")]
pub use intelligence_loop::{ExplainPlanSummary, IntelligenceLoop, LoopReport};
