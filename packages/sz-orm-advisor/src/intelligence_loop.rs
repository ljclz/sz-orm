//! 查询智能闭环联动（`query-intelligence-loop` feature）
//!
//! [`IntelligenceLoop`] 串联四步闭环：
//! 1. EXPLAIN 分析（`ExplainPlan` `packages/sz-orm-explain/src/lib.rs:76`）
//! 2. 自适应决策（`AdaptiveExecutor::decide` `packages/sz-orm-adaptive/src/executor.rs:157`）
//! 3. 火焰图诊断（`SlowQueryDiagnoser::diagnose`）
//! 4. 优化建议（`OptimizationAdvisor::suggest`）
//!
//! 任一环节失败降级跳过，不阻断查询。

use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use sz_orm_diagnosis::{DiagnosisReport, SlowQueryDiagnoser};
use sz_orm_flamegraph::QueryPhaseTiming;

use crate::advisor::OptimizationAdvisor;
use crate::suggestion::OptimizationSuggestion;

/// 闭环报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopReport {
    /// 查询标识
    pub query_key: String,
    /// 第一步：EXPLAIN 分析结果（跳过时 None）
    pub explain_result: Option<ExplainPlanSummary>,
    /// 第二步：自适应决策路径（跳过时 None）
    pub adaptive_decision: Option<String>,
    /// 第三步：诊断报告（跳过时 None）
    pub diagnosis_result: Option<DiagnosisReport>,
    /// 第四步：优化建议
    pub suggestions: Vec<OptimizationSuggestion>,
    /// 闭环总耗时（毫秒）
    pub loop_elapsed_ms: u64,
    /// 各环节跳过标注
    pub skipped_steps: Vec<String>,
}

/// ExplainPlan 摘要（可序列化）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExplainPlanSummary {
    /// 扫描类型
    pub scan_type: String,
    /// 表名
    pub table: String,
    /// 索引名
    pub index: Option<String>,
    /// 预估行数
    pub rows: u64,
}

impl LoopReport {
    /// JSON 输出（CI 消费）
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
    }
}

/// 智能闭环协调器
pub struct IntelligenceLoop {
    advisor: Arc<OptimizationAdvisor>,
    diagnoser: Option<Arc<SlowQueryDiagnoser>>,
}

impl IntelligenceLoop {
    /// 创建闭环协调器
    pub fn new(advisor: Arc<OptimizationAdvisor>) -> Self {
        Self {
            advisor,
            diagnoser: None,
        }
    }

    /// 注入诊断器
    pub fn with_diagnoser(mut self, d: Arc<SlowQueryDiagnoser>) -> Self {
        self.diagnoser = Some(d);
        self
    }

    /// 运行闭环
    ///
    /// 串联四步：EXPLAIN → 自适应 → 诊断 → 建议，
    /// 任一环节失败降级跳过，不阻断。
    pub fn run_loop(
        &self,
        query_key: &str,
        plan: Option<&sz_orm_explain::ExplainPlan>,
        timings: &[QueryPhaseTiming],
        slow: bool,
        elapsed_ms: u64,
    ) -> LoopReport {
        let start = Instant::now();
        let mut skipped = Vec::new();

        // 第一步：EXPLAIN 分析
        let explain_result = if let Some(p) = plan {
            Some(ExplainPlanSummary {
                scan_type: format!("{:?}", p.scan_type),
                table: p.table.clone(),
                index: p.index.clone(),
                rows: p.rows,
            })
        } else {
            skipped.push("EXPLAIN step skipped".into());
            None
        };

        // 第二步：自适应决策（简化版，基于 plan 信息）
        let adaptive_decision = if let Some(p) = plan {
            if p.rows > 1000 {
                Some("Paginated".into())
            } else {
                Some("Normal".into())
            }
        } else {
            skipped.push("adaptive step skipped".into());
            None
        };

        // 第三步：火焰图诊断
        let diagnosis_result = if let Some(d) = &self.diagnoser {
            if slow {
                let outcome = sz_orm_adaptive::QueryOutcome {
                    value: (),
                    rows: 0,
                    elapsed_ms,
                    from_cache: false,
                    slow: true,
                };
                d.diagnose(query_key, timings, &outcome)
            } else {
                skipped.push("diagnosis step skipped (not slow)".into());
                None
            }
        } else {
            skipped.push("diagnosis step skipped (no diagnoser)".into());
            None
        };

        // 第四步：优化建议
        let suggestions = if let Some(p) = plan {
            let stats = sz_orm_adaptive::stats::QueryStats::new();
            // 修复：record(rows, time_us)——此前 elapsed_ms 被误传为 rows 且 time_us 传 0，
            // 导致统计耗时恒为 0；本场景无行数信息（QueryOutcome.rows=0），耗时需转微秒
            stats.record(0, elapsed_ms * 1000);
            self.advisor.suggest(Some(p), Some(&stats), None)
        } else {
            Vec::new()
        };

        LoopReport {
            query_key: query_key.to_string(),
            explain_result,
            adaptive_decision,
            diagnosis_result,
            suggestions,
            loop_elapsed_ms: start.elapsed().as_millis() as u64,
            skipped_steps: skipped,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sz_orm_adaptive::AdaptiveConfig;
    use sz_orm_explain::{ExplainPlan, ScanType};
    use sz_orm_flamegraph::Phase;

    fn advisor() -> Arc<OptimizationAdvisor> {
        Arc::new(OptimizationAdvisor::with_defaults())
    }

    fn plan(scan: ScanType, rows: u64) -> ExplainPlan {
        ExplainPlan {
            scan_type: scan,
            table: "users".into(),
            index: None,
            rows,
            extra: vec![],
        }
    }

    fn timing(phase: Phase, ms: u64) -> QueryPhaseTiming {
        QueryPhaseTiming {
            phase,
            start_ms: 0,
            duration_ms: ms,
        }
    }

    #[test]
    fn loop_with_all_steps() {
        let loop_ = IntelligenceLoop::new(advisor());
        let p = plan(ScanType::FullTable, 10000);
        let timings = vec![timing(Phase::SqlExecute, 200)];
        let report = loop_.run_loop("q1", Some(&p), &timings, true, 200);

        assert!(report.explain_result.is_some());
        assert!(report.adaptive_decision.is_some());
        assert!(!report.suggestions.is_empty());
    }

    #[test]
    fn loop_without_explain() {
        let loop_ = IntelligenceLoop::new(advisor());
        let timings = vec![timing(Phase::SqlExecute, 200)];
        let report = loop_.run_loop("q1", None, &timings, true, 200);

        assert!(report.explain_result.is_none());
        assert!(report.skipped_steps.iter().any(|s| s.contains("EXPLAIN")));
    }

    #[test]
    fn loop_without_diagnoser() {
        let loop_ = IntelligenceLoop::new(advisor());
        let p = plan(ScanType::FullTable, 10000);
        let timings = vec![timing(Phase::SqlExecute, 200)];
        let report = loop_.run_loop("q1", Some(&p), &timings, true, 200);

        assert!(report.diagnosis_result.is_none());
        assert!(report.skipped_steps.iter().any(|s| s.contains("diagnosis")));
    }

    #[test]
    fn loop_non_slow_skips_diagnosis() {
        let loop_ = IntelligenceLoop::new(advisor());
        let p = plan(ScanType::IndexRange, 100);
        let timings = vec![timing(Phase::SqlExecute, 50)];
        let report = loop_.run_loop("q1", Some(&p), &timings, false, 50);

        assert!(report.diagnosis_result.is_none());
    }

    #[test]
    fn loop_report_json_output() {
        let loop_ = IntelligenceLoop::new(advisor());
        let p = plan(ScanType::FullTable, 10000);
        let report = loop_.run_loop("q1", Some(&p), &[], false, 100);
        let json = report.to_json();
        assert!(json.contains("query_key"));
        assert!(json.contains("explain_result"));
        assert!(json.contains("suggestions"));
        assert!(json.contains("loop_elapsed_ms"));
    }

    #[test]
    fn loop_with_diagnoser() {
        let adaptive_config = Arc::new(AdaptiveConfig::default());
        let diagnoser = Arc::new(SlowQueryDiagnoser::with_defaults(adaptive_config));
        let loop_ = IntelligenceLoop::new(advisor()).with_diagnoser(diagnoser);
        let p = plan(ScanType::FullTable, 10000);
        let timings = vec![timing(Phase::SqlExecute, 200)];
        let report = loop_.run_loop("q1", Some(&p), &timings, true, 200);

        assert!(report.diagnosis_result.is_some());
    }

    #[test]
    fn loop_all_skipped() {
        let loop_ = IntelligenceLoop::new(advisor());
        let report = loop_.run_loop("q1", None, &[], false, 0);
        let json = report.to_json();
        assert!(json.contains("skipped_steps"));
    }
}
