//! # sz-orm-diagnosis — Slow Query Automatic Diagnosis Report
//!
//! Based on `slow-query-diagnosis` feature, `SlowQueryDiagnoser` determines root cause based on phase timing ratios
//! (PoolExhaustion/SqlInefficiency/LargeResultSet/BuildOverhead/MixedCause),
//! only triggered when slow==true, linked with optimization suggestions.
//!
//! ## Main Modules
//!
//! - [`root_cause`] — Root cause analysis (`RootCause` + `Severity` + `PhaseBreakdown` + `DiagnosisConfig`)
//! - [`report`] — Diagnosis report (`DiagnosisReport` + `SuggestionHint` + JSON/human-readable dual format output)
//! - [`diagnoser`] — Diagnoser (`SlowQueryDiagnoser::diagnose` only triggered for slow queries)
//! - [`pool_diagnoser`] — Connection pool diagnoser (`ConnectionPoolDiagnoser` + `PoolLeakDetector`)
//! - [`deadlock_detector`] — Deadlock detector and lock wait analyzer
//! - [`bottleneck_locator`] — Performance bottleneck locator
//! - [`advisor`] — Diagnosis advice engine and slow query report generator
//!
//! ## Reuse
//!
//! - `Phase` enum `packages/sz-orm-flamegraph/src/collector.rs:11`
//! - `QueryPhaseTiming` `packages/sz-orm-flamegraph/src/collector.rs:39`
//! - `QueryOutcome.slow` `packages/sz-orm-adaptive/src/executor.rs:116`
//! - `AdaptiveConfig.slow_ms` `packages/sz-orm-adaptive/src/executor.rs:35`

pub mod bottleneck_locator;
pub mod deadlock_detector;
pub mod pool_diagnoser;

#[cfg(feature = "slow-query-diagnosis")]
pub mod advisor;
#[cfg(feature = "slow-query-diagnosis")]
pub mod diagnoser;
pub mod metrics;
#[cfg(feature = "slow-query-diagnosis")]
pub mod report;
#[cfg(feature = "slow-query-diagnosis")]
pub mod root_cause;

#[cfg(feature = "slow-query-diagnosis")]
pub use advisor::{
    AdviceType, DiagnosisAdvice, DiagnosisAdvisor, ReportSummary, SlowQueryReportGenerator,
};
pub use bottleneck_locator::{
    Bottleneck, BottleneckLocator, BottleneckLocatorConfig, BottleneckSample, BottleneckSeverity,
    BottleneckTrendAnalyzer,
};
pub use deadlock_detector::{
    DeadlockDetector, DeadlockPreventionStrategy, DeadlockResolution, LockWaitAnalysis,
    LockWaitAnalyzer, LockWaitEvent,
};
#[cfg(feature = "slow-query-diagnosis")]
pub use diagnoser::SlowQueryDiagnoser;
pub use metrics::{BottleneckRanker, DiagnosisMetrics, DiagnosisSummary, FixAction};
pub use pool_diagnoser::{
    ConnectionPoolDiagnoser, ConnectionPoolMetrics, PoolDiagnoserConfig, PoolDiagnosisResult,
    PoolHealthStatus, PoolLeakDetector, PoolMetricsSampler, PoolSuggestion,
};
#[cfg(feature = "slow-query-diagnosis")]
pub use report::{DiagnosisReport, SuggestionHint};
#[cfg(feature = "slow-query-diagnosis")]
pub use root_cause::{
    analyze_root_cause, build_phase_breakdown, DiagnosisConfig, DiagnosisPhase, PhaseBreakdown,
    RootCause, Severity,
};

// v5.1.0：LLM 驱动故障诊断（llm-diagnosis feature gate 隔离）
// 启用 llm-diagnosis feature 后规则引擎 MixedCause 时调用 LLM
#[cfg(feature = "llm-diagnosis")]
pub mod llm_diagnoser;
#[cfg(feature = "llm-diagnosis")]
pub use llm_diagnoser::{
    DiagnosisResult, DiagnosisSource, FixSuggestion, LlmDiagnoser, LlmDiagnosisError,
};

// v5.1.0 P2：故障预测器（failure-prediction feature gate 隔离）
// 基于性能指标时序数据预测未来故障
#[cfg(feature = "failure-prediction")]
pub mod failure_predictor;
#[cfg(feature = "failure-prediction")]
pub use failure_predictor::{
    AlertSeverity, FailureAlert, FailurePrediction, FailurePredictor, FailurePredictorConfig,
    MetricSample,
};
