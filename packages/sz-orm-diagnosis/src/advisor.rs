//! 诊断建议引擎与慢查询报告生成器
//!
//! 基于 [`DiagnosisReport`] 生成结构化建议与汇总报告。
//! 依赖 `slow-query-diagnosis` feature。

use std::collections::HashMap;

use crate::report::DiagnosisReport;
use crate::root_cause::{RootCause, Severity};

/// 诊断建议类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdviceType {
    /// 添加索引
    AddIndex,
    /// 改写查询
    RewriteQuery,
    /// 调整连接池
    AdjustPool,
    /// 使用分页
    UsePagination,
    /// 预编译查询
    Precompile,
    /// 启用缓存
    EnableCache,
    /// 优化结果映射
    OptimizeResultMap,
}

impl AdviceType {
    /// 人类可读名称
    pub fn as_str(&self) -> &'static str {
        match self {
            AdviceType::AddIndex => "add-index",
            AdviceType::RewriteQuery => "rewrite-query",
            AdviceType::AdjustPool => "adjust-pool",
            AdviceType::UsePagination => "use-pagination",
            AdviceType::Precompile => "precompile",
            AdviceType::EnableCache => "enable-cache",
            AdviceType::OptimizeResultMap => "optimize-result-map",
        }
    }
}

/// 诊断建议
#[derive(Debug, Clone)]
pub struct DiagnosisAdvice {
    /// 建议类型
    pub advice_type: AdviceType,
    /// 优先级（0=最高）
    pub priority: u8,
    /// 建议描述
    pub description: String,
    /// 预期改善（如 "减少 60% 耗时"）
    pub expected_improvement: String,
    /// 适用根因
    pub root_cause: RootCause,
}

/// 诊断建议引擎
///
/// 根据 [`DiagnosisReport`] 的根因与严重度生成结构化建议。
pub struct DiagnosisAdvisor {
    /// 严重度权重：Critical=3, Warning=2, Info=1
    severity_weights: HashMap<Severity, u8>,
}

impl DiagnosisAdvisor {
    /// 创建建议引擎
    pub fn new() -> Self {
        let mut severity_weights = HashMap::new();
        severity_weights.insert(Severity::Critical, 3);
        severity_weights.insert(Severity::Warning, 2);
        severity_weights.insert(Severity::Info, 1);
        Self { severity_weights }
    }

    /// 为单个报告生成建议
    pub fn advise(&self, report: &DiagnosisReport) -> Vec<DiagnosisAdvice> {
        let base_priority = self
            .severity_weights
            .get(&report.severity)
            .copied()
            .unwrap_or(1);
        self.advise_for_root_cause(report.root_cause, base_priority)
    }

    /// 为多个报告批量生成建议（去重 + 按优先级排序）
    pub fn advise_batch(&self, reports: &[DiagnosisReport]) -> Vec<DiagnosisAdvice> {
        let mut all_advice: Vec<DiagnosisAdvice> = Vec::new();
        for report in reports {
            all_advice.extend(self.advise(report));
        }
        // 按优先级排序
        all_advice.sort_by_key(|a| a.priority);
        all_advice
    }

    fn advise_for_root_cause(
        &self,
        root_cause: RootCause,
        base_priority: u8,
    ) -> Vec<DiagnosisAdvice> {
        match root_cause {
            RootCause::PoolExhaustion => vec![DiagnosisAdvice {
                advice_type: AdviceType::AdjustPool,
                priority: base_priority,
                description: "连接池耗尽：增大连接池大小或启用预热".to_string(),
                expected_improvement: "减少 PoolAcquire 耗时 50%+".to_string(),
                root_cause,
            }],
            RootCause::SqlInefficiency => vec![
                DiagnosisAdvice {
                    advice_type: AdviceType::AddIndex,
                    priority: base_priority,
                    description: "SQL 低效：添加索引减少全表扫描".to_string(),
                    expected_improvement: "减少 SqlExecute 耗时 60%+".to_string(),
                    root_cause,
                },
                DiagnosisAdvice {
                    advice_type: AdviceType::RewriteQuery,
                    priority: base_priority + 1,
                    description: "SQL 低效：改写消除 filesort/temporary".to_string(),
                    expected_improvement: "减少临时表使用".to_string(),
                    root_cause,
                },
            ],
            RootCause::LargeResultSet => vec![
                DiagnosisAdvice {
                    advice_type: AdviceType::UsePagination,
                    priority: base_priority,
                    description: "大结果集：改用游标分页".to_string(),
                    expected_improvement: "减少内存占用 80%+".to_string(),
                    root_cause,
                },
                DiagnosisAdvice {
                    advice_type: AdviceType::OptimizeResultMap,
                    priority: base_priority + 1,
                    description: "大结果集：优化结果映射逻辑".to_string(),
                    expected_improvement: "减少 ResultMap 耗时 30%+".to_string(),
                    root_cause,
                },
            ],
            RootCause::BuildOverhead => vec![
                DiagnosisAdvice {
                    advice_type: AdviceType::Precompile,
                    priority: base_priority,
                    description: "构造开销：预编译查询".to_string(),
                    expected_improvement: "减少 Build 耗时 70%+".to_string(),
                    root_cause,
                },
                DiagnosisAdvice {
                    advice_type: AdviceType::RewriteQuery,
                    priority: base_priority + 1,
                    description: "构造开销：简化查询构造".to_string(),
                    expected_improvement: "减少构造复杂度".to_string(),
                    root_cause,
                },
            ],
            RootCause::MixedCause => vec![
                DiagnosisAdvice {
                    advice_type: AdviceType::AdjustPool,
                    priority: base_priority,
                    description: "多阶段瓶颈：优先排查连接池".to_string(),
                    expected_improvement: "综合改善 30%+".to_string(),
                    root_cause,
                },
                DiagnosisAdvice {
                    advice_type: AdviceType::AddIndex,
                    priority: base_priority,
                    description: "多阶段瓶颈：检查索引覆盖".to_string(),
                    expected_improvement: "综合改善 20%+".to_string(),
                    root_cause,
                },
                DiagnosisAdvice {
                    advice_type: AdviceType::EnableCache,
                    priority: base_priority + 1,
                    description: "多阶段瓶颈：考虑启用缓存".to_string(),
                    expected_improvement: "减少重复查询".to_string(),
                    root_cause,
                },
            ],
            RootCause::Unknown => Vec::new(),
        }
    }

    /// 严重度权重
    pub fn severity_weight(&self, severity: Severity) -> u8 {
        self.severity_weights.get(&severity).copied().unwrap_or(1)
    }
}

impl Default for DiagnosisAdvisor {
    fn default() -> Self {
        Self::new()
    }
}

/// 慢查询报告汇总
#[derive(Debug, Clone)]
pub struct ReportSummary {
    /// 报告总数
    pub total_reports: usize,
    /// 按根因分布
    pub root_cause_distribution: Vec<(RootCause, usize)>,
    /// 按严重度分布
    pub severity_distribution: Vec<(Severity, usize)>,
    /// 平均耗时（毫秒）
    pub avg_elapsed_ms: f64,
    /// 最大耗时（毫秒）
    pub max_elapsed_ms: u64,
    /// 总耗时（毫秒）
    pub total_elapsed_ms: u64,
    /// timing mismatch 数
    pub timing_mismatch_count: usize,
}

/// 慢查询诊断报告生成器
///
/// 聚合多个 [`DiagnosisReport`] 生成汇总报告与统计。
pub struct SlowQueryReportGenerator {
    reports: Vec<DiagnosisReport>,
}

impl SlowQueryReportGenerator {
    /// 创建空生成器
    pub fn new() -> Self {
        Self {
            reports: Vec::new(),
        }
    }

    /// 添加报告
    pub fn add_report(&mut self, report: DiagnosisReport) {
        self.reports.push(report);
    }

    /// 批量添加报告
    pub fn add_reports(&mut self, reports: Vec<DiagnosisReport>) {
        self.reports.extend(reports);
    }

    /// 报告数
    pub fn count(&self) -> usize {
        self.reports.len()
    }

    /// 所有报告引用
    pub fn reports(&self) -> &[DiagnosisReport] {
        &self.reports
    }

    /// 生成汇总
    pub fn summary(&self) -> ReportSummary {
        let total_reports = self.reports.len();
        let total_elapsed_ms: u64 = self.reports.iter().map(|r| r.total_elapsed_ms).sum();
        let max_elapsed_ms = self
            .reports
            .iter()
            .map(|r| r.total_elapsed_ms)
            .max()
            .unwrap_or(0);
        let avg_elapsed_ms = if total_reports == 0 {
            0.0
        } else {
            total_elapsed_ms as f64 / total_reports as f64
        };
        let timing_mismatch_count = self.reports.iter().filter(|r| r.timing_mismatch).count();

        let root_cause_distribution = self.distribution_by_root_cause();
        let severity_distribution = self.distribution_by_severity();

        ReportSummary {
            total_reports,
            root_cause_distribution,
            severity_distribution,
            avg_elapsed_ms,
            max_elapsed_ms,
            total_elapsed_ms,
            timing_mismatch_count,
        }
    }

    fn distribution_by_root_cause(&self) -> Vec<(RootCause, usize)> {
        let mut counts: HashMap<RootCause, usize> = HashMap::new();
        for report in &self.reports {
            *counts.entry(report.root_cause).or_insert(0) += 1;
        }
        let mut dist: Vec<(RootCause, usize)> = counts.into_iter().collect();
        dist.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
        dist
    }

    fn distribution_by_severity(&self) -> Vec<(Severity, usize)> {
        let mut counts: HashMap<Severity, usize> = HashMap::new();
        for report in &self.reports {
            *counts.entry(report.severity).or_insert(0) += 1;
        }
        let mut dist: Vec<(Severity, usize)> = counts.into_iter().collect();
        dist.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
        dist
    }

    /// 生成人类可读汇总报告
    pub fn generate_summary_report(&self) -> String {
        let summary = self.summary();
        let mut out = String::new();
        out.push_str("=== 慢查询诊断汇总报告 ===\n\n");
        out.push_str(&format!("报告总数: {}\n", summary.total_reports));
        out.push_str(&format!("总耗时: {} ms\n", summary.total_elapsed_ms));
        out.push_str(&format!("平均耗时: {:.1} ms\n", summary.avg_elapsed_ms));
        out.push_str(&format!("最大耗时: {} ms\n", summary.max_elapsed_ms));
        out.push_str(&format!(
            "Timing mismatch: {} 处\n",
            summary.timing_mismatch_count
        ));
        out.push('\n');

        out.push_str("--- 根因分布 ---\n");
        for (cause, count) in &summary.root_cause_distribution {
            out.push_str(&format!("  {}: {}\n", cause.as_str(), count));
        }
        out.push('\n');

        out.push_str("--- 严重度分布 ---\n");
        for (severity, count) in &summary.severity_distribution {
            out.push_str(&format!("  {}: {}\n", severity.as_str(), count));
        }

        out
    }

    /// 生成详细报告（含每条诊断）
    pub fn generate_detail_report(&self) -> String {
        let mut out = self.generate_summary_report();
        out.push_str("\n--- 逐条诊断 ---\n");
        for (idx, report) in self.reports.iter().enumerate() {
            out.push_str(&format!("\n[{}] {}\n", idx + 1, report.query_key));
            out.push_str(&format!(
                "  耗时: {} ms | 根因: {} | 严重度: {}\n",
                report.total_elapsed_ms,
                report.root_cause.as_str(),
                report.severity.as_str()
            ));
            if !report.suggestion_hints.is_empty() {
                out.push_str("  建议:\n");
                for hint in &report.suggestion_hints {
                    out.push_str(&format!("    [{}] {}\n", hint.suggestion_type, hint.reason));
                }
            }
        }
        out
    }

    /// 按根因过滤报告
    pub fn filter_by_root_cause(&self, cause: RootCause) -> Vec<&DiagnosisReport> {
        self.reports
            .iter()
            .filter(|r| r.root_cause == cause)
            .collect()
    }

    /// 按严重度过滤报告
    pub fn filter_by_severity(&self, severity: Severity) -> Vec<&DiagnosisReport> {
        self.reports
            .iter()
            .filter(|r| r.severity == severity)
            .collect()
    }

    /// 按耗时降序排序的 Top-N 报告
    pub fn top_n_slowest(&self, n: usize) -> Vec<&DiagnosisReport> {
        let mut sorted: Vec<&DiagnosisReport> = self.reports.iter().collect();
        sorted.sort_by_key(|a| std::cmp::Reverse(a.total_elapsed_ms));
        sorted.truncate(n);
        sorted
    }

    /// 清空报告
    pub fn clear(&mut self) {
        self.reports.clear();
    }
}

impl Default for SlowQueryReportGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::SuggestionHint;
    use crate::root_cause::PhaseBreakdown;

    fn make_report(
        key: &str,
        elapsed: u64,
        cause: RootCause,
        severity: Severity,
    ) -> DiagnosisReport {
        DiagnosisReport {
            query_key: key.to_string(),
            total_elapsed_ms: elapsed,
            root_cause: cause,
            phase_breakdown: vec![PhaseBreakdown {
                phase: crate::root_cause::DiagnosisPhase::SqlExecute,
                elapsed_ms: elapsed,
                percentage: 100.0,
                anomaly: true,
            }],
            suggestion_hints: vec![SuggestionHint {
                suggestion_type: "test".to_string(),
                reason: "test".to_string(),
            }],
            severity,
            timing_mismatch: false,
        }
    }

    // --- AdviceType tests ---

    #[test]
    fn advice_type_as_str() {
        assert_eq!(AdviceType::AddIndex.as_str(), "add-index");
        assert_eq!(AdviceType::RewriteQuery.as_str(), "rewrite-query");
        assert_eq!(AdviceType::AdjustPool.as_str(), "adjust-pool");
        assert_eq!(AdviceType::UsePagination.as_str(), "use-pagination");
        assert_eq!(AdviceType::Precompile.as_str(), "precompile");
        assert_eq!(AdviceType::EnableCache.as_str(), "enable-cache");
        assert_eq!(
            AdviceType::OptimizeResultMap.as_str(),
            "optimize-result-map"
        );
    }

    #[test]
    fn advice_type_distinct() {
        assert_ne!(AdviceType::AddIndex, AdviceType::RewriteQuery);
        assert_ne!(AdviceType::AdjustPool, AdviceType::UsePagination);
    }

    // --- DiagnosisAdvisor tests ---

    #[test]
    fn advisor_pool_exhaustion() {
        let advisor = DiagnosisAdvisor::new();
        let report = make_report("q1", 200, RootCause::PoolExhaustion, Severity::Critical);
        let advice = advisor.advise(&report);
        assert_eq!(advice.len(), 1);
        assert_eq!(advice[0].advice_type, AdviceType::AdjustPool);
    }

    #[test]
    fn advisor_sql_inefficiency() {
        let advisor = DiagnosisAdvisor::new();
        let report = make_report("q1", 200, RootCause::SqlInefficiency, Severity::Warning);
        let advice = advisor.advise(&report);
        assert_eq!(advice.len(), 2);
        assert_eq!(advice[0].advice_type, AdviceType::AddIndex);
        assert_eq!(advice[1].advice_type, AdviceType::RewriteQuery);
    }

    #[test]
    fn advisor_large_result_set() {
        let advisor = DiagnosisAdvisor::new();
        let report = make_report("q1", 200, RootCause::LargeResultSet, Severity::Warning);
        let advice = advisor.advise(&report);
        assert_eq!(advice.len(), 2);
        assert_eq!(advice[0].advice_type, AdviceType::UsePagination);
    }

    #[test]
    fn advisor_build_overhead() {
        let advisor = DiagnosisAdvisor::new();
        let report = make_report("q1", 200, RootCause::BuildOverhead, Severity::Info);
        let advice = advisor.advise(&report);
        assert_eq!(advice.len(), 2);
        assert_eq!(advice[0].advice_type, AdviceType::Precompile);
    }

    #[test]
    fn advisor_mixed_cause() {
        let advisor = DiagnosisAdvisor::new();
        let report = make_report("q1", 200, RootCause::MixedCause, Severity::Critical);
        let advice = advisor.advise(&report);
        assert_eq!(advice.len(), 3);
    }

    #[test]
    fn advisor_unknown_empty() {
        let advisor = DiagnosisAdvisor::new();
        let report = make_report("q1", 200, RootCause::Unknown, Severity::Info);
        let advice = advisor.advise(&report);
        assert!(advice.is_empty());
    }

    #[test]
    fn advisor_priority_based_on_severity() {
        let advisor = DiagnosisAdvisor::new();
        let critical = make_report("q1", 200, RootCause::PoolExhaustion, Severity::Critical);
        let info = make_report("q2", 200, RootCause::PoolExhaustion, Severity::Info);
        let critical_advice = advisor.advise(&critical);
        let info_advice = advisor.advise(&info);
        assert!(critical_advice[0].priority > info_advice[0].priority);
    }

    #[test]
    fn advisor_advise_batch() {
        let advisor = DiagnosisAdvisor::new();
        let reports = vec![
            make_report("q1", 200, RootCause::PoolExhaustion, Severity::Critical),
            make_report("q2", 300, RootCause::SqlInefficiency, Severity::Warning),
        ];
        let advice = advisor.advise_batch(&reports);
        assert!(advice.len() >= 3);
    }

    #[test]
    fn advisor_severity_weight() {
        let advisor = DiagnosisAdvisor::new();
        assert_eq!(advisor.severity_weight(Severity::Critical), 3);
        assert_eq!(advisor.severity_weight(Severity::Warning), 2);
        assert_eq!(advisor.severity_weight(Severity::Info), 1);
    }

    #[test]
    fn advisor_default() {
        let advisor = DiagnosisAdvisor::default();
        let report = make_report("q1", 200, RootCause::PoolExhaustion, Severity::Critical);
        assert!(!advisor.advise(&report).is_empty());
    }

    #[test]
    fn advice_descriptions_nonempty() {
        let advisor = DiagnosisAdvisor::new();
        for cause in [
            RootCause::PoolExhaustion,
            RootCause::SqlInefficiency,
            RootCause::LargeResultSet,
            RootCause::BuildOverhead,
            RootCause::MixedCause,
        ] {
            let report = make_report("q", 100, cause, Severity::Warning);
            let advice = advisor.advise(&report);
            for a in &advice {
                assert!(!a.description.is_empty());
                assert!(!a.expected_improvement.is_empty());
            }
        }
    }

    // --- SlowQueryReportGenerator tests ---

    #[test]
    fn generator_empty() {
        let g = SlowQueryReportGenerator::new();
        assert_eq!(g.count(), 0);
        let summary = g.summary();
        assert_eq!(summary.total_reports, 0);
        assert_eq!(summary.avg_elapsed_ms, 0.0);
    }

    #[test]
    fn generator_add_report() {
        let mut g = SlowQueryReportGenerator::new();
        g.add_report(make_report(
            "q1",
            100,
            RootCause::PoolExhaustion,
            Severity::Warning,
        ));
        assert_eq!(g.count(), 1);
    }

    #[test]
    fn generator_add_reports_batch() {
        let mut g = SlowQueryReportGenerator::new();
        let reports = vec![
            make_report("q1", 100, RootCause::PoolExhaustion, Severity::Warning),
            make_report("q2", 200, RootCause::SqlInefficiency, Severity::Critical),
        ];
        g.add_reports(reports);
        assert_eq!(g.count(), 2);
    }

    #[test]
    fn generator_summary_stats() {
        let mut g = SlowQueryReportGenerator::new();
        g.add_report(make_report(
            "q1",
            100,
            RootCause::PoolExhaustion,
            Severity::Warning,
        ));
        g.add_report(make_report(
            "q2",
            300,
            RootCause::SqlInefficiency,
            Severity::Critical,
        ));
        let summary = g.summary();
        assert_eq!(summary.total_reports, 2);
        assert_eq!(summary.total_elapsed_ms, 400);
        assert!((summary.avg_elapsed_ms - 200.0).abs() < 1e-9);
        assert_eq!(summary.max_elapsed_ms, 300);
    }

    #[test]
    fn generator_root_cause_distribution() {
        let mut g = SlowQueryReportGenerator::new();
        g.add_report(make_report(
            "q1",
            100,
            RootCause::PoolExhaustion,
            Severity::Warning,
        ));
        g.add_report(make_report(
            "q2",
            200,
            RootCause::PoolExhaustion,
            Severity::Critical,
        ));
        g.add_report(make_report(
            "q3",
            300,
            RootCause::SqlInefficiency,
            Severity::Critical,
        ));
        let summary = g.summary();
        assert_eq!(summary.root_cause_distribution.len(), 2);
        assert_eq!(summary.root_cause_distribution[0].1, 2);
    }

    #[test]
    fn generator_severity_distribution() {
        let mut g = SlowQueryReportGenerator::new();
        g.add_report(make_report(
            "q1",
            100,
            RootCause::PoolExhaustion,
            Severity::Warning,
        ));
        g.add_report(make_report(
            "q2",
            200,
            RootCause::SqlInefficiency,
            Severity::Critical,
        ));
        g.add_report(make_report(
            "q3",
            300,
            RootCause::SqlInefficiency,
            Severity::Critical,
        ));
        let summary = g.summary();
        assert_eq!(summary.severity_distribution[0].0, Severity::Critical);
        assert_eq!(summary.severity_distribution[0].1, 2);
    }

    #[test]
    fn generator_timing_mismatch_count() {
        let mut g = SlowQueryReportGenerator::new();
        let mut r1 = make_report("q1", 100, RootCause::PoolExhaustion, Severity::Warning);
        r1.timing_mismatch = true;
        g.add_report(r1);
        g.add_report(make_report(
            "q2",
            200,
            RootCause::SqlInefficiency,
            Severity::Critical,
        ));
        let summary = g.summary();
        assert_eq!(summary.timing_mismatch_count, 1);
    }

    #[test]
    fn generator_summary_report_string() {
        let mut g = SlowQueryReportGenerator::new();
        g.add_report(make_report(
            "q1",
            100,
            RootCause::PoolExhaustion,
            Severity::Warning,
        ));
        let report = g.generate_summary_report();
        assert!(report.contains("慢查询诊断汇总报告"));
        assert!(report.contains("报告总数: 1"));
    }

    #[test]
    fn generator_detail_report_string() {
        let mut g = SlowQueryReportGenerator::new();
        g.add_report(make_report(
            "q1",
            100,
            RootCause::PoolExhaustion,
            Severity::Warning,
        ));
        let report = g.generate_detail_report();
        assert!(report.contains("逐条诊断"));
        assert!(report.contains("q1"));
    }

    #[test]
    fn generator_filter_by_root_cause() {
        let mut g = SlowQueryReportGenerator::new();
        g.add_report(make_report(
            "q1",
            100,
            RootCause::PoolExhaustion,
            Severity::Warning,
        ));
        g.add_report(make_report(
            "q2",
            200,
            RootCause::SqlInefficiency,
            Severity::Critical,
        ));
        let filtered = g.filter_by_root_cause(RootCause::PoolExhaustion);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn generator_filter_by_severity() {
        let mut g = SlowQueryReportGenerator::new();
        g.add_report(make_report(
            "q1",
            100,
            RootCause::PoolExhaustion,
            Severity::Warning,
        ));
        g.add_report(make_report(
            "q2",
            200,
            RootCause::SqlInefficiency,
            Severity::Critical,
        ));
        g.add_report(make_report(
            "q3",
            300,
            RootCause::SqlInefficiency,
            Severity::Critical,
        ));
        let filtered = g.filter_by_severity(Severity::Critical);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn generator_top_n_slowest() {
        let mut g = SlowQueryReportGenerator::new();
        g.add_report(make_report(
            "q1",
            100,
            RootCause::PoolExhaustion,
            Severity::Warning,
        ));
        g.add_report(make_report(
            "q2",
            300,
            RootCause::SqlInefficiency,
            Severity::Critical,
        ));
        g.add_report(make_report(
            "q3",
            200,
            RootCause::SqlInefficiency,
            Severity::Critical,
        ));
        let top = g.top_n_slowest(2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].total_elapsed_ms, 300);
        assert_eq!(top[1].total_elapsed_ms, 200);
    }

    #[test]
    fn generator_clear() {
        let mut g = SlowQueryReportGenerator::new();
        g.add_report(make_report(
            "q1",
            100,
            RootCause::PoolExhaustion,
            Severity::Warning,
        ));
        g.clear();
        assert_eq!(g.count(), 0);
    }

    #[test]
    fn generator_default() {
        let g = SlowQueryReportGenerator::default();
        assert_eq!(g.count(), 0);
    }

    #[test]
    fn generator_reports_ref() {
        let mut g = SlowQueryReportGenerator::new();
        g.add_report(make_report(
            "q1",
            100,
            RootCause::PoolExhaustion,
            Severity::Warning,
        ));
        assert_eq!(g.reports().len(), 1);
    }
}
