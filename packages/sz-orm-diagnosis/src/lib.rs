//! # sz-orm-diagnosis — 慢查询自动诊断报告
//!
//! 基于 `slow-query-diagnosis` feature，`SlowQueryDiagnoser` 基于阶段耗时占比
//! 判定根因（PoolExhaustion/SqlInefficiency/LargeResultSet/BuildOverhead/MixedCause），
//! 仅对 slow==true 触发，与优化建议联动。
//!
//! ## 主要模块
//!
//! - [`root_cause`] — 根因分析（`RootCause` + `Severity` + `PhaseBreakdown` + `DiagnosisConfig`）
//! - [`report`] — 诊断报告（`DiagnosisReport` + `SuggestionHint` + JSON/人类可读双格式输出）
//! - [`diagnoser`] — 诊断器（`SlowQueryDiagnoser::diagnose` 仅对慢查询触发）
//! - [`pool_diagnoser`] — 连接池诊断器（`ConnectionPoolDiagnoser` + `PoolLeakDetector`）
//! - [`deadlock_detector`] — 死锁检测器与锁等待分析器
//! - [`bottleneck_locator`] — 性能瓶颈定位器
//! - [`advisor`] — 诊断建议引擎与慢查询报告生成器
//!
//! ## 复用
//!
//! - `Phase` 枚举 `packages/sz-orm-flamegraph/src/collector.rs:11`
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
