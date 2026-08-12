//! 慢查询诊断器（`slow-query-diagnosis` feature）
//!
//! [`SlowQueryDiagnoser::diagnose`] 仅对 `slow == true` 查询触发诊断，
//! 复用既有 `QueryOutcome.slow` `packages/sz-orm-adaptive/src/executor.rs:116`
//! + `AdaptiveConfig.slow_ms` `packages/sz-orm-adaptive/src/executor.rs:35`。

use std::sync::Arc;

use sz_orm_adaptive::{AdaptiveConfig, QueryOutcome};
use sz_orm_flamegraph::QueryPhaseTiming;

use crate::report::{DiagnosisReport, SuggestionHint};
use crate::root_cause::{
    analyze_root_cause, build_phase_breakdown, DiagnosisConfig, RootCause, Severity,
};

/// 慢查询诊断器
///
/// 仅对 `outcome.slow == true` 的查询触发诊断，非每次查询都诊断。
/// 建议联动通过根因 → 建议类型映射生成 `SuggestionHint`，
/// M6 智能闭环联动时可由 sz-orm-advisor 转换为完整 `OptimizationSuggestion`。
pub struct SlowQueryDiagnoser {
    /// 根因阈值配置
    config: DiagnosisConfig,
    /// 自适应配置（提供 slow_ms 阈值）
    adaptive_config: Arc<AdaptiveConfig>,
}

impl SlowQueryDiagnoser {
    /// 创建诊断器
    pub fn new(config: DiagnosisConfig, adaptive_config: Arc<AdaptiveConfig>) -> Self {
        Self {
            config,
            adaptive_config,
        }
    }

    /// 使用默认配置创建诊断器
    pub fn with_defaults(adaptive_config: Arc<AdaptiveConfig>) -> Self {
        Self::new(DiagnosisConfig::default(), adaptive_config)
    }

    /// 诊断慢查询
    ///
    /// 仅当 `outcome.slow == true` 时返回 `Some(DiagnosisReport)`，否则返回 `None`。
    /// 诊断流程：根因分析 → 阶段分解 → 建议提示 → severity 判定 → 生成报告
    pub fn diagnose<T>(
        &self,
        query_key: &str,
        timings: &[QueryPhaseTiming],
        outcome: &QueryOutcome<T>,
    ) -> Option<DiagnosisReport> {
        if !outcome.slow {
            return None;
        }

        let root_cause = analyze_root_cause(timings, &self.config);
        let phase_breakdown = build_phase_breakdown(timings, &self.config);
        let suggestion_hints = root_cause_to_hints(root_cause);
        let severity = self.determine_severity(outcome.elapsed_ms);
        let timing_mismatch = self.check_timing_mismatch(timings, outcome.elapsed_ms);

        Some(DiagnosisReport {
            query_key: query_key.to_string(),
            total_elapsed_ms: outcome.elapsed_ms,
            root_cause,
            phase_breakdown,
            suggestion_hints,
            severity,
            timing_mismatch,
        })
    }

    /// severity 判定
    ///
    /// `total_elapsed_ms > slow_ms * 3` → Critical
    /// `> slow_ms * 2` → Warning
    /// 否则 Info
    fn determine_severity(&self, elapsed_ms: u64) -> Severity {
        let slow_ms = self.adaptive_config.slow_ms;
        if elapsed_ms > slow_ms * 3 {
            Severity::Critical
        } else if elapsed_ms > slow_ms * 2 {
            Severity::Warning
        } else {
            Severity::Info
        }
    }

    /// 检查 timing mismatch（阶段耗时总和与 elapsed_ms 误差超 10%）
    fn check_timing_mismatch(&self, timings: &[QueryPhaseTiming], elapsed_ms: u64) -> bool {
        if elapsed_ms == 0 {
            return false;
        }
        let sum: u64 = timings.iter().map(|t| t.duration_ms).sum();
        let diff = sum.abs_diff(elapsed_ms);
        (diff as f64 / elapsed_ms as f64) > 0.1
    }
}

/// 根因 → 建议提示映射
///
/// - `PoolExhaustion` → `adjust-pool-size`
/// - `SqlInefficiency` → `add-index` + `rewrite-query`
/// - `LargeResultSet` → `use-pagination`
/// - `BuildOverhead` → `rewrite-query`
/// - `MixedCause` → 综合建议
/// - `Unknown` → 空列表
fn root_cause_to_hints(cause: RootCause) -> Vec<SuggestionHint> {
    match cause {
        RootCause::PoolExhaustion => vec![SuggestionHint {
            suggestion_type: "adjust-pool-size".into(),
            reason: "PoolAcquire 耗时占比过高，建议增大连接池或启用预热".into(),
        }],
        RootCause::SqlInefficiency => vec![
            SuggestionHint {
                suggestion_type: "add-index".into(),
                reason: "SqlExecute 耗时占比过高，建议添加索引减少扫描".into(),
            },
            SuggestionHint {
                suggestion_type: "rewrite-query".into(),
                reason: "SqlExecute 耗时占比过高，建议改写 SQL 消除 filesort/temporary".into(),
            },
        ],
        RootCause::LargeResultSet => vec![SuggestionHint {
            suggestion_type: "use-pagination".into(),
            reason: "ResultMap 耗时占比过高，建议改用游标分页减少结果集".into(),
        }],
        RootCause::BuildOverhead => vec![SuggestionHint {
            suggestion_type: "rewrite-query".into(),
            reason: "Build 耗时占比过高，建议简化查询构造或预编译".into(),
        }],
        RootCause::MixedCause => vec![
            SuggestionHint {
                suggestion_type: "adjust-pool-size".into(),
                reason: "多阶段瓶颈，建议优先排查连接池".into(),
            },
            SuggestionHint {
                suggestion_type: "add-index".into(),
                reason: "多阶段瓶颈，建议检查索引覆盖".into(),
            },
        ],
        RootCause::Unknown => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sz_orm_flamegraph::Phase;

    fn timing(phase: Phase, ms: u64) -> QueryPhaseTiming {
        QueryPhaseTiming {
            phase,
            start_ms: 0,
            duration_ms: ms,
        }
    }

    fn slow_outcome(ms: u64) -> QueryOutcome<i32> {
        QueryOutcome {
            value: 0,
            rows: 0,
            elapsed_ms: ms,
            from_cache: false,
            slow: true,
        }
    }

    fn fast_outcome(ms: u64) -> QueryOutcome<i32> {
        QueryOutcome {
            value: 0,
            rows: 0,
            elapsed_ms: ms,
            from_cache: false,
            slow: false,
        }
    }

    fn diagnoser() -> SlowQueryDiagnoser {
        SlowQueryDiagnoser::with_defaults(Arc::new(AdaptiveConfig::default()))
    }

    #[test]
    fn non_slow_query_returns_none() {
        let d = diagnoser();
        let timings = vec![timing(Phase::SqlExecute, 50)];
        let outcome = fast_outcome(50);
        assert!(d.diagnose("q1", &timings, &outcome).is_none());
    }

    #[test]
    fn slow_query_returns_some() {
        let d = diagnoser();
        let timings = vec![
            timing(Phase::PoolAcquire, 60),
            timing(Phase::SqlExecute, 30),
            timing(Phase::ResultMap, 10),
        ];
        let outcome = slow_outcome(100);
        let report = d
            .diagnose("q1", &timings, &outcome)
            .expect("slow query should diagnose");
        assert_eq!(report.query_key, "q1");
        assert_eq!(report.total_elapsed_ms, 100);
        assert_eq!(report.root_cause, RootCause::PoolExhaustion);
        assert_eq!(report.phase_breakdown.len(), 3);
    }

    #[test]
    fn pool_exhaustion_generates_adjust_pool_hint() {
        let d = diagnoser();
        let timings = vec![
            timing(Phase::PoolAcquire, 60),
            timing(Phase::SqlExecute, 30),
            timing(Phase::ResultMap, 10),
        ];
        let outcome = slow_outcome(100);
        let report = d.diagnose("q1", &timings, &outcome).unwrap();
        assert!(report
            .suggestion_hints
            .iter()
            .any(|h| h.suggestion_type == "adjust-pool-size"));
    }

    #[test]
    fn sql_inefficiency_generates_add_index_hint() {
        let d = diagnoser();
        let timings = vec![
            timing(Phase::SqlExecute, 80),
            timing(Phase::ResultMap, 15),
            timing(Phase::PoolAcquire, 5),
        ];
        let outcome = slow_outcome(100);
        let report = d.diagnose("q1", &timings, &outcome).unwrap();
        assert!(report
            .suggestion_hints
            .iter()
            .any(|h| h.suggestion_type == "add-index"));
        assert!(report
            .suggestion_hints
            .iter()
            .any(|h| h.suggestion_type == "rewrite-query"));
    }

    #[test]
    fn large_result_set_generates_pagination_hint() {
        let d = diagnoser();
        let timings = vec![
            timing(Phase::ResultMap, 40),
            timing(Phase::SqlExecute, 35),
            timing(Phase::PoolAcquire, 25),
        ];
        let outcome = slow_outcome(100);
        let report = d.diagnose("q1", &timings, &outcome).unwrap();
        assert!(report
            .suggestion_hints
            .iter()
            .any(|h| h.suggestion_type == "use-pagination"));
    }

    #[test]
    fn severity_critical_for_3x_slow() {
        let d = diagnoser();
        let timings = vec![timing(Phase::SqlExecute, 400)];
        let outcome = slow_outcome(400);
        let report = d.diagnose("q1", &timings, &outcome).unwrap();
        assert_eq!(report.severity, Severity::Critical);
    }

    #[test]
    fn severity_warning_for_2x_slow() {
        let d = diagnoser();
        let timings = vec![timing(Phase::SqlExecute, 250)];
        let outcome = slow_outcome(250);
        let report = d.diagnose("q1", &timings, &outcome).unwrap();
        assert_eq!(report.severity, Severity::Warning);
    }

    #[test]
    fn severity_info_for_just_over_slow() {
        let d = diagnoser();
        let timings = vec![timing(Phase::SqlExecute, 150)];
        let outcome = slow_outcome(150);
        let report = d.diagnose("q1", &timings, &outcome).unwrap();
        assert_eq!(report.severity, Severity::Info);
    }

    #[test]
    fn timing_mismatch_detected() {
        let d = diagnoser();
        let timings = vec![timing(Phase::SqlExecute, 50)];
        let outcome = slow_outcome(200);
        let report = d.diagnose("q1", &timings, &outcome).unwrap();
        assert!(report.timing_mismatch);
    }

    #[test]
    fn no_timing_mismatch_when_aligned() {
        let d = diagnoser();
        let timings = vec![
            timing(Phase::PoolAcquire, 30),
            timing(Phase::SqlExecute, 50),
            timing(Phase::ResultMap, 20),
        ];
        let outcome = slow_outcome(100);
        let report = d.diagnose("q1", &timings, &outcome).unwrap();
        assert!(!report.timing_mismatch);
    }

    #[test]
    fn unknown_root_cause_empty_hints() {
        let d = diagnoser();
        let timings: Vec<QueryPhaseTiming> = vec![];
        let outcome = slow_outcome(100);
        let report = d.diagnose("q1", &timings, &outcome).unwrap();
        assert_eq!(report.root_cause, RootCause::Unknown);
        assert!(report.suggestion_hints.is_empty());
    }
}
