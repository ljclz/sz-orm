//! 诊断报告结构 + 双格式输出（`slow-query-diagnosis` feature）
//!
//! JSON 格式供 CI 消费，人类可读格式供开发者排查。

use serde::{Deserialize, Serialize};

use crate::root_cause::{PhaseBreakdown, RootCause, Severity};

/// 建议提示（不依赖 sz-orm-advisor，避免循环依赖）
///
/// 根因分析生成的建议提示，M6 智能闭环联动时可由 sz-orm-advisor
/// 转换为完整 `OptimizationSuggestion`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SuggestionHint {
    /// 建议类型名称（如 "add-index" / "adjust-pool-size" / "use-pagination" / "rewrite-query"）
    pub suggestion_type: String,
    /// 建议原因
    pub reason: String,
}

/// 慢查询诊断报告
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiagnosisReport {
    /// 查询标识
    pub query_key: String,
    /// 总耗时（毫秒）
    pub total_elapsed_ms: u64,
    /// 根因
    pub root_cause: RootCause,
    /// 阶段分解
    pub phase_breakdown: Vec<PhaseBreakdown>,
    /// 建议提示
    pub suggestion_hints: Vec<SuggestionHint>,
    /// 严重度
    pub severity: Severity,
    /// timing mismatch 标注（阶段耗时总和与 elapsed_ms 误差超 10%）
    pub timing_mismatch: bool,
}

impl DiagnosisReport {
    /// JSON 格式输出（CI 消费）
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"))
    }

    /// 人类可读格式输出（开发者排查）
    pub fn to_human_readable(&self) -> String {
        let mut out = String::new();
        out.push_str("=== 慢查询诊断报告 ===\n");
        out.push_str(&format!("查询标识: {}\n", self.query_key));
        out.push_str(&format!("总耗时: {} ms\n", self.total_elapsed_ms));
        out.push_str(&format!("根因: {}\n", self.root_cause.as_str()));
        out.push_str(&format!("严重度: {}\n", self.severity.as_str()));
        if self.timing_mismatch {
            out.push_str("⚠ timing mismatch: 阶段耗时总和与 elapsed_ms 误差超 10%\n");
        }
        out.push('\n');

        out.push_str("--- 阶段分解 ---\n");
        if self.phase_breakdown.is_empty() {
            out.push_str("(无阶段耗时数据)\n");
        } else {
            out.push_str(&format!(
                "{:<20} {:>10} {:>10} {:>8}\n",
                "阶段", "耗时(ms)", "占比(%)", "异常"
            ));
            for pb in &self.phase_breakdown {
                out.push_str(&format!(
                    "{:<20} {:>10} {:>10.1} {:>8}\n",
                    pb.phase.as_str(),
                    pb.elapsed_ms,
                    pb.percentage,
                    if pb.anomaly { "⚠" } else { "" }
                ));
            }
        }
        out.push('\n');

        out.push_str("--- 建议提示 ---\n");
        if self.suggestion_hints.is_empty() {
            out.push_str("no suggestions\n");
        } else {
            for hint in &self.suggestion_hints {
                out.push_str(&format!("  [{}] {}\n", hint.suggestion_type, hint.reason));
            }
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::root_cause::DiagnosisPhase;

    fn sample_report() -> DiagnosisReport {
        DiagnosisReport {
            query_key: "q1".into(),
            total_elapsed_ms: 200,
            root_cause: RootCause::PoolExhaustion,
            phase_breakdown: vec![PhaseBreakdown {
                phase: DiagnosisPhase::PoolAcquire,
                elapsed_ms: 120,
                percentage: 60.0,
                anomaly: true,
            }],
            suggestion_hints: vec![SuggestionHint {
                suggestion_type: "adjust-pool-size".into(),
                reason: "PoolAcquire 占 60%".into(),
            }],
            severity: Severity::Critical,
            timing_mismatch: false,
        }
    }

    #[test]
    fn json_roundtrip() {
        let report = sample_report();
        let json = report.to_json();
        let back: DiagnosisReport = serde_json::from_str(&json).unwrap();
        assert_eq!(report, back);
    }

    #[test]
    fn json_contains_all_fields() {
        let report = sample_report();
        let json = report.to_json();
        assert!(json.contains("query_key"));
        assert!(json.contains("total_elapsed_ms"));
        assert!(json.contains("root_cause"));
        assert!(json.contains("phase_breakdown"));
        assert!(json.contains("suggestion_hints"));
        assert!(json.contains("severity"));
    }

    #[test]
    fn human_readable_contains_root_cause() {
        let report = sample_report();
        let text = report.to_human_readable();
        assert!(text.contains("pool-exhaustion"));
    }

    #[test]
    fn human_readable_contains_phase_table() {
        let report = sample_report();
        let text = report.to_human_readable();
        assert!(text.contains("阶段分解"));
        assert!(text.contains("pool.acquire"));
        assert!(text.contains("120"));
    }

    #[test]
    fn human_readable_contains_suggestion_table() {
        let report = sample_report();
        let text = report.to_human_readable();
        assert!(text.contains("adjust-pool-size"));
        assert!(text.contains("PoolAcquire 占 60%"));
    }

    #[test]
    fn human_readable_empty_suggestions() {
        let mut report = sample_report();
        report.suggestion_hints.clear();
        let text = report.to_human_readable();
        assert!(text.contains("no suggestions"));
    }

    #[test]
    fn human_readable_timing_mismatch_warning() {
        let mut report = sample_report();
        report.timing_mismatch = true;
        let text = report.to_human_readable();
        assert!(text.contains("timing mismatch"));
    }

    #[test]
    fn human_readable_empty_phase_breakdown() {
        let mut report = sample_report();
        report.phase_breakdown.clear();
        let text = report.to_human_readable();
        assert!(text.contains("无阶段耗时数据"));
    }
}
