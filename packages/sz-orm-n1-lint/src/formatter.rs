//! 检测报告格式化器与误报过滤器
//!
//! 提供多种格式输出（text/json/markdown）与误报过滤。

use std::collections::HashSet;

use crate::{N1Finding, N1Pattern, N1Report, N1Severity};

/// 报告输出格式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportFormat {
    /// 纯文本
    Text,
    /// JSON
    Json,
    /// Markdown
    Markdown,
    /// CSV
    Csv,
}

impl ReportFormat {
    /// 人类可读名称
    pub fn as_str(&self) -> &'static str {
        match self {
            ReportFormat::Text => "text",
            ReportFormat::Json => "json",
            ReportFormat::Markdown => "markdown",
            ReportFormat::Csv => "csv",
        }
    }

    /// 文件扩展名
    pub fn file_extension(&self) -> &'static str {
        match self {
            ReportFormat::Text => "txt",
            ReportFormat::Json => "json",
            ReportFormat::Markdown => "md",
            ReportFormat::Csv => "csv",
        }
    }
}

/// N+1 检测报告格式化器
pub struct N1ReportFormatter {
    /// 是否包含严重度
    include_severity: bool,
    /// 是否包含修复建议
    include_suggestions: bool,
}

impl N1ReportFormatter {
    /// 创建格式化器
    pub fn new() -> Self {
        Self {
            include_severity: true,
            include_suggestions: true,
        }
    }

    /// 不包含严重度
    pub fn without_severity(mut self) -> Self {
        self.include_severity = false;
        self
    }

    /// 不包含修复建议
    pub fn without_suggestions(mut self) -> Self {
        self.include_suggestions = false;
        self
    }

    /// 按指定格式输出
    pub fn format(&self, report: &N1Report, format: ReportFormat) -> String {
        match format {
            ReportFormat::Text => self.format_text(report),
            ReportFormat::Json => self.format_json(report),
            ReportFormat::Markdown => self.format_markdown(report),
            ReportFormat::Csv => self.format_csv(report),
        }
    }

    /// 纯文本格式
    pub fn format_text(&self, report: &N1Report) -> String {
        let mut out = String::new();
        out.push_str("=== N+1 Query Detection Report ===\n");
        out.push_str(&format!("Total findings: {}\n\n", report.count()));
        for f in report.findings() {
            out.push_str(&self.format_finding_text(f));
        }
        out
    }

    fn format_finding_text(&self, f: &N1Finding) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "[{}] {}:{}\n  {}\n",
            f.pattern.as_str(),
            f.file,
            f.line,
            f.message
        ));
        if self.include_severity {
            let severity = N1Severity::from_pattern(f.pattern);
            out.push_str(&format!("  Severity: {}\n", severity.as_str()));
        }
        if self.include_suggestions {
            let suggestion = crate::N1Suggestion::new(f.pattern);
            out.push_str(&format!("  Suggestion: {}\n", suggestion.description()));
        }
        out
    }

    /// JSON 格式
    pub fn format_json(&self, report: &N1Report) -> String {
        let mut out = String::from("[");
        let findings = report.findings();
        for (i, f) in findings.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&self.format_finding_json(f));
        }
        out.push(']');
        out
    }

    fn format_finding_json(&self, f: &N1Finding) -> String {
        let severity = N1Severity::from_pattern(f.pattern);
        let mut out = format!(
            r#"{{"pattern":"{}","file":"{}","line":{},"message":"{}""#,
            f.pattern.as_str(),
            f.file.replace('"', "\\\""),
            f.line,
            f.message.replace('"', "\\\"")
        );
        if self.include_severity {
            out.push_str(&format!(r#","severity":"{}""#, severity.as_str()));
        }
        if self.include_suggestions {
            let suggestion = crate::N1Suggestion::new(f.pattern);
            out.push_str(&format!(
                r#","suggestion":"{}""#,
                suggestion.description().replace('"', "\\\"")
            ));
        }
        out.push('}');
        out
    }

    /// Markdown 格式
    pub fn format_markdown(&self, report: &N1Report) -> String {
        let mut out = String::new();
        out.push_str("# N+1 Query Detection Report\n\n");
        out.push_str(&format!("**Total findings:** {}\n\n", report.count()));
        if report.is_clean() {
            out.push_str("✅ No N+1 query issues found.\n");
            return out;
        }
        out.push_str("| Pattern | File | Line | Message");
        if self.include_severity {
            out.push_str(" | Severity");
        }
        if self.include_suggestions {
            out.push_str(" | Suggestion");
        }
        out.push_str(" |\n|---|---|---|---");
        if self.include_severity {
            out.push_str("|---");
        }
        if self.include_suggestions {
            out.push_str("|---");
        }
        out.push_str("|\n");
        for f in report.findings() {
            out.push_str(&format!(
                "| {} | {} | {} | {}",
                f.pattern.as_str(),
                f.file,
                f.line,
                f.message
            ));
            if self.include_severity {
                out.push_str(&format!(
                    " | {}",
                    N1Severity::from_pattern(f.pattern).as_str()
                ));
            }
            if self.include_suggestions {
                out.push_str(&format!(
                    " | {}",
                    crate::N1Suggestion::new(f.pattern).description()
                ));
            }
            out.push_str(" |\n");
        }
        out
    }

    /// CSV 格式
    pub fn format_csv(&self, report: &N1Report) -> String {
        let mut out = String::from("pattern,file,line,message");
        if self.include_severity {
            out.push_str(",severity");
        }
        if self.include_suggestions {
            out.push_str(",suggestion");
        }
        out.push('\n');
        for f in report.findings() {
            out.push_str(&format!(
                "{},{},{},{}",
                f.pattern.as_str(),
                f.file,
                f.line,
                f.message
            ));
            if self.include_severity {
                out.push_str(&format!(
                    ",{}",
                    N1Severity::from_pattern(f.pattern).as_str()
                ));
            }
            if self.include_suggestions {
                out.push_str(&format!(
                    ",{}",
                    crate::N1Suggestion::new(f.pattern).description()
                ));
            }
            out.push('\n');
        }
        out
    }
}

impl Default for N1ReportFormatter {
    fn default() -> Self {
        Self::new()
    }
}

/// 误报过滤器
///
/// 按文件、行号、模式过滤已知误报。
#[derive(Debug, Clone)]
pub struct FalsePositiveFilter {
    /// 忽略的文件集合
    ignored_files: HashSet<String>,
    /// 忽略的 (文件, 行号) 集合
    ignored_locations: HashSet<(String, usize)>,
    /// 忽略的模式集合
    ignored_patterns: HashSet<N1Pattern>,
    /// 忽略的消息关键词
    ignored_keywords: Vec<String>,
}

impl FalsePositiveFilter {
    /// 创建空过滤器
    pub fn new() -> Self {
        Self {
            ignored_files: HashSet::new(),
            ignored_locations: HashSet::new(),
            ignored_patterns: HashSet::new(),
            ignored_keywords: Vec::new(),
        }
    }

    /// 添加忽略文件
    pub fn ignore_file(&mut self, file: impl Into<String>) -> &mut Self {
        self.ignored_files.insert(file.into());
        self
    }

    /// 添加忽略位置
    pub fn ignore_location(&mut self, file: impl Into<String>, line: usize) -> &mut Self {
        self.ignored_locations.insert((file.into(), line));
        self
    }

    /// 添加忽略模式
    pub fn ignore_pattern(&mut self, pattern: N1Pattern) -> &mut Self {
        self.ignored_patterns.insert(pattern);
        self
    }

    /// 添加忽略消息关键词
    pub fn ignore_keyword(&mut self, keyword: impl Into<String>) -> &mut Self {
        self.ignored_keywords.push(keyword.into());
        self
    }

    /// 判断单个检测结果是否为误报
    pub fn is_false_positive(&self, finding: &N1Finding) -> bool {
        if self.ignored_files.contains(&finding.file) {
            return true;
        }
        if self
            .ignored_locations
            .contains(&(finding.file.clone(), finding.line))
        {
            return true;
        }
        if self.ignored_patterns.contains(&finding.pattern) {
            return true;
        }
        for keyword in &self.ignored_keywords {
            if finding.message.contains(keyword) {
                return true;
            }
        }
        false
    }

    /// 过滤检测结果，返回非误报列表
    pub fn filter(&self, findings: &[N1Finding]) -> Vec<N1Finding> {
        findings
            .iter()
            .filter(|f| !self.is_false_positive(f))
            .cloned()
            .collect()
    }

    /// 过滤并返回被过滤掉的误报
    pub fn filtered_out(&self, findings: &[N1Finding]) -> Vec<N1Finding> {
        findings
            .iter()
            .filter(|f| self.is_false_positive(f))
            .cloned()
            .collect()
    }

    /// 对报告应用过滤，返回新报告
    pub fn apply(&self, report: &N1Report) -> N1Report {
        let mut new_report = N1Report::new();
        for finding in report.findings() {
            if !self.is_false_positive(finding) {
                new_report.add_finding(finding.clone());
            }
        }
        new_report
    }

    /// 忽略文件数
    pub fn ignored_file_count(&self) -> usize {
        self.ignored_files.len()
    }

    /// 忽略位置数
    pub fn ignored_location_count(&self) -> usize {
        self.ignored_locations.len()
    }

    /// 忽略模式数
    pub fn ignored_pattern_count(&self) -> usize {
        self.ignored_patterns.len()
    }
}

impl Default for FalsePositiveFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// N+1 检测配置
///
/// 综合配置检测行为：忽略规则、严重度阈值、输出格式。
#[derive(Debug, Clone)]
pub struct N1DetectionConfig {
    /// 误报过滤器
    pub filter: FalsePositiveFilter,
    /// 最低报告严重度
    pub min_severity: N1Severity,
    /// 输出格式
    pub output_format: ReportFormat,
    /// 是否包含修复建议
    pub include_suggestions: bool,
}

impl Default for N1DetectionConfig {
    fn default() -> Self {
        Self {
            filter: FalsePositiveFilter::new(),
            min_severity: N1Severity::Info,
            output_format: ReportFormat::Text,
            include_suggestions: true,
        }
    }
}

impl N1DetectionConfig {
    /// 创建配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置最低严重度
    pub fn with_min_severity(mut self, severity: N1Severity) -> Self {
        self.min_severity = severity;
        self
    }

    /// 设置输出格式
    pub fn with_output_format(mut self, format: ReportFormat) -> Self {
        self.output_format = format;
        self
    }

    /// 不包含修复建议
    pub fn without_suggestions(mut self) -> Self {
        self.include_suggestions = false;
        self
    }

    /// 按严重度过滤检测结果
    pub fn filter_by_severity(&self, findings: &[N1Finding]) -> Vec<N1Finding> {
        findings
            .iter()
            .filter(|f| N1Severity::from_pattern(f.pattern) >= self.min_severity)
            .cloned()
            .collect()
    }

    /// 应用完整配置（误报过滤 + 严重度过滤）
    pub fn apply(&self, report: &N1Report) -> N1Report {
        let mut new_report = N1Report::new();
        for finding in report.findings() {
            if !self.filter.is_false_positive(finding)
                && N1Severity::from_pattern(finding.pattern) >= self.min_severity
            {
                new_report.add_finding(finding.clone());
            }
        }
        new_report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(pattern: N1Pattern, file: &str, line: usize) -> N1Finding {
        N1Finding {
            pattern,
            file: file.to_string(),
            line,
            message: "test finding".to_string(),
        }
    }

    fn make_report(findings: Vec<N1Finding>) -> N1Report {
        let mut r = N1Report::new();
        for f in findings {
            r.add_finding(f);
        }
        r
    }

    // --- ReportFormat tests ---

    #[test]
    fn format_as_str() {
        assert_eq!(ReportFormat::Text.as_str(), "text");
        assert_eq!(ReportFormat::Json.as_str(), "json");
        assert_eq!(ReportFormat::Markdown.as_str(), "markdown");
        assert_eq!(ReportFormat::Csv.as_str(), "csv");
    }

    #[test]
    fn format_file_extension() {
        assert_eq!(ReportFormat::Text.file_extension(), "txt");
        assert_eq!(ReportFormat::Json.file_extension(), "json");
        assert_eq!(ReportFormat::Markdown.file_extension(), "md");
        assert_eq!(ReportFormat::Csv.file_extension(), "csv");
    }

    #[test]
    fn format_distinct() {
        assert_ne!(ReportFormat::Text, ReportFormat::Json);
        assert_ne!(ReportFormat::Markdown, ReportFormat::Csv);
    }

    // --- N1ReportFormatter tests ---

    #[test]
    fn formatter_text_empty() {
        let f = N1ReportFormatter::new();
        let r = N1Report::new();
        let text = f.format_text(&r);
        assert!(text.contains("Total findings: 0"));
    }

    #[test]
    fn formatter_text_with_findings() {
        let f = N1ReportFormatter::new();
        let r = make_report(vec![finding(N1Pattern::QueryInLoop, "test.rs", 10)]);
        let text = f.format_text(&r);
        assert!(text.contains("query-in-loop"));
        assert!(text.contains("test.rs"));
        assert!(text.contains("10"));
    }

    #[test]
    fn formatter_text_without_severity() {
        let f = N1ReportFormatter::new().without_severity();
        let r = make_report(vec![finding(N1Pattern::QueryInLoop, "test.rs", 10)]);
        let text = f.format_text(&r);
        assert!(!text.contains("Severity"));
    }

    #[test]
    fn formatter_text_without_suggestions() {
        let f = N1ReportFormatter::new().without_suggestions();
        let r = make_report(vec![finding(N1Pattern::QueryInLoop, "test.rs", 10)]);
        let text = f.format_text(&r);
        assert!(!text.contains("Suggestion"));
    }

    #[test]
    fn formatter_json_empty() {
        let f = N1ReportFormatter::new();
        let r = N1Report::new();
        let json = f.format_json(&r);
        assert_eq!(json, "[]");
    }

    #[test]
    fn formatter_json_with_findings() {
        let f = N1ReportFormatter::new();
        let r = make_report(vec![finding(N1Pattern::QueryInLoop, "test.rs", 10)]);
        let json = f.format_json(&r);
        assert!(json.contains("query-in-loop"));
        assert!(json.contains("test.rs"));
    }

    #[test]
    fn formatter_markdown_empty() {
        let f = N1ReportFormatter::new();
        let r = N1Report::new();
        let md = f.format_markdown(&r);
        assert!(md.contains("No N+1 query issues found"));
    }

    #[test]
    fn formatter_markdown_with_findings() {
        let f = N1ReportFormatter::new();
        let r = make_report(vec![finding(N1Pattern::QueryInLoop, "test.rs", 10)]);
        let md = f.format_markdown(&r);
        assert!(md.contains("query-in-loop"));
        assert!(md.contains("| test.rs |"));
    }

    #[test]
    fn formatter_csv_with_header() {
        let f = N1ReportFormatter::new();
        let r = make_report(vec![finding(N1Pattern::QueryInLoop, "test.rs", 10)]);
        let csv = f.format_csv(&r);
        assert!(csv.starts_with("pattern,file,line,message"));
    }

    #[test]
    fn formatter_csv_with_findings() {
        let f = N1ReportFormatter::new();
        let r = make_report(vec![finding(N1Pattern::QueryInLoop, "test.rs", 10)]);
        let csv = f.format_csv(&r);
        assert!(csv.contains("query-in-loop,test.rs,10"));
    }

    #[test]
    fn formatter_format_dispatch() {
        let f = N1ReportFormatter::new();
        let r = make_report(vec![finding(N1Pattern::QueryInLoop, "test.rs", 10)]);
        let text = f.format(&r, ReportFormat::Text);
        let json = f.format(&r, ReportFormat::Json);
        let md = f.format(&r, ReportFormat::Markdown);
        let csv = f.format(&r, ReportFormat::Csv);
        assert!(text.contains("Total findings"));
        assert!(json.starts_with("["));
        assert!(md.contains("# N+1"));
        assert!(csv.starts_with("pattern"));
    }

    #[test]
    fn formatter_default() {
        let f = N1ReportFormatter::default();
        let r = make_report(vec![finding(N1Pattern::QueryInLoop, "test.rs", 10)]);
        let text = f.format_text(&r);
        assert!(!text.is_empty());
    }

    // --- FalsePositiveFilter tests ---

    #[test]
    fn filter_empty() {
        let filter = FalsePositiveFilter::new();
        let findings = vec![finding(N1Pattern::QueryInLoop, "test.rs", 10)];
        let result = filter.filter(&findings);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn filter_ignore_file() {
        let mut filter = FalsePositiveFilter::new();
        filter.ignore_file("test.rs");
        let findings = vec![finding(N1Pattern::QueryInLoop, "test.rs", 10)];
        let result = filter.filter(&findings);
        assert!(result.is_empty());
    }

    #[test]
    fn filter_ignore_location() {
        let mut filter = FalsePositiveFilter::new();
        filter.ignore_location("test.rs", 10);
        let findings = vec![finding(N1Pattern::QueryInLoop, "test.rs", 10)];
        let result = filter.filter(&findings);
        assert!(result.is_empty());
    }

    #[test]
    fn filter_ignore_pattern() {
        let mut filter = FalsePositiveFilter::new();
        filter.ignore_pattern(N1Pattern::QueryInLoop);
        let findings = vec![finding(N1Pattern::QueryInLoop, "test.rs", 10)];
        let result = filter.filter(&findings);
        assert!(result.is_empty());
    }

    #[test]
    fn filter_ignore_keyword() {
        let mut filter = FalsePositiveFilter::new();
        filter.ignore_keyword("test");
        let findings = vec![finding(N1Pattern::QueryInLoop, "test.rs", 10)];
        let result = filter.filter(&findings);
        assert!(result.is_empty());
    }

    #[test]
    fn filter_partial_match() {
        let mut filter = FalsePositiveFilter::new();
        filter.ignore_file("ignored.rs");
        let findings = vec![
            finding(N1Pattern::QueryInLoop, "ignored.rs", 10),
            finding(N1Pattern::QueryInLoop, "keep.rs", 20),
        ];
        let result = filter.filter(&findings);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].file, "keep.rs");
    }

    #[test]
    fn filter_filtered_out() {
        let mut filter = FalsePositiveFilter::new();
        filter.ignore_file("ignored.rs");
        let findings = vec![
            finding(N1Pattern::QueryInLoop, "ignored.rs", 10),
            finding(N1Pattern::QueryInLoop, "keep.rs", 20),
        ];
        let filtered = filter.filtered_out(&findings);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].file, "ignored.rs");
    }

    #[test]
    fn filter_apply_to_report() {
        let mut filter = FalsePositiveFilter::new();
        filter.ignore_file("ignored.rs");
        let report = make_report(vec![
            finding(N1Pattern::QueryInLoop, "ignored.rs", 10),
            finding(N1Pattern::QueryInLoop, "keep.rs", 20),
        ]);
        let new_report = filter.apply(&report);
        assert_eq!(new_report.count(), 1);
    }

    #[test]
    fn filter_counts() {
        let mut filter = FalsePositiveFilter::new();
        filter.ignore_file("a.rs");
        filter.ignore_file("b.rs");
        filter.ignore_location("c.rs", 5);
        filter.ignore_pattern(N1Pattern::QueryInLoop);
        assert_eq!(filter.ignored_file_count(), 2);
        assert_eq!(filter.ignored_location_count(), 1);
        assert_eq!(filter.ignored_pattern_count(), 1);
    }

    #[test]
    fn filter_default() {
        let filter = FalsePositiveFilter::default();
        let findings = vec![finding(N1Pattern::QueryInLoop, "test.rs", 10)];
        assert_eq!(filter.filter(&findings).len(), 1);
    }

    #[test]
    fn filter_is_false_positive() {
        let mut filter = FalsePositiveFilter::new();
        filter.ignore_file("test.rs");
        let f = finding(N1Pattern::QueryInLoop, "test.rs", 10);
        assert!(filter.is_false_positive(&f));
    }

    #[test]
    fn filter_not_false_positive() {
        let filter = FalsePositiveFilter::new();
        let f = finding(N1Pattern::QueryInLoop, "test.rs", 10);
        assert!(!filter.is_false_positive(&f));
    }

    // --- N1DetectionConfig tests ---

    #[test]
    fn config_default() {
        let c = N1DetectionConfig::default();
        assert_eq!(c.min_severity, N1Severity::Info);
        assert_eq!(c.output_format, ReportFormat::Text);
        assert!(c.include_suggestions);
    }

    #[test]
    fn config_with_min_severity() {
        let c = N1DetectionConfig::new().with_min_severity(N1Severity::Error);
        assert_eq!(c.min_severity, N1Severity::Error);
    }

    #[test]
    fn config_with_output_format() {
        let c = N1DetectionConfig::new().with_output_format(ReportFormat::Json);
        assert_eq!(c.output_format, ReportFormat::Json);
    }

    #[test]
    fn config_without_suggestions() {
        let c = N1DetectionConfig::new().without_suggestions();
        assert!(!c.include_suggestions);
    }

    #[test]
    fn config_filter_by_severity() {
        let c = N1DetectionConfig::new().with_min_severity(N1Severity::Warning);
        let findings = vec![
            finding(N1Pattern::QueryInLoop, "a.rs", 1),
            finding(N1Pattern::ConditionalQueryInLoop, "b.rs", 2),
            finding(N1Pattern::MissingEagerLoadHint, "c.rs", 3),
        ];
        let result = c.filter_by_severity(&findings);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn config_apply() {
        let mut c = N1DetectionConfig::new();
        c.filter.ignore_file("ignored.rs");
        let report = make_report(vec![
            finding(N1Pattern::QueryInLoop, "ignored.rs", 10),
            finding(N1Pattern::QueryInLoop, "keep.rs", 20),
        ]);
        let new_report = c.apply(&report);
        assert_eq!(new_report.count(), 1);
    }

    #[test]
    fn config_apply_with_severity() {
        let c = N1DetectionConfig::new().with_min_severity(N1Severity::Error);
        let report = make_report(vec![
            finding(N1Pattern::QueryInLoop, "a.rs", 1),
            finding(N1Pattern::MissingEagerLoadHint, "b.rs", 2),
        ]);
        let new_report = c.apply(&report);
        assert_eq!(new_report.count(), 1);
    }

    #[test]
    fn config_new_equals_default() {
        let c1 = N1DetectionConfig::new();
        let c2 = N1DetectionConfig::default();
        assert_eq!(c1.min_severity, c2.min_severity);
        assert_eq!(c1.output_format, c2.output_format);
    }

    #[test]
    fn formatter_text_includes_pattern() {
        let f = N1ReportFormatter::new();
        let r = make_report(vec![finding(N1Pattern::ConditionalQueryInLoop, "x.rs", 5)]);
        let text = f.format_text(&r);
        assert!(text.contains("conditional-query-in-loop"));
    }

    #[test]
    fn formatter_json_multiple_findings() {
        let f = N1ReportFormatter::new();
        let r = make_report(vec![
            finding(N1Pattern::QueryInLoop, "a.rs", 1),
            finding(N1Pattern::QueryInLoop, "b.rs", 2),
        ]);
        let json = f.format_json(&r);
        assert!(json.contains("a.rs"));
        assert!(json.contains("b.rs"));
    }

    #[test]
    fn formatter_csv_multiple_findings() {
        let f = N1ReportFormatter::new();
        let r = make_report(vec![
            finding(N1Pattern::QueryInLoop, "a.rs", 1),
            finding(N1Pattern::MissingEagerLoadHint, "b.rs", 2),
        ]);
        let csv = f.format_csv(&r);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn filter_multiple_ignored_patterns() {
        let mut filter = FalsePositiveFilter::new();
        filter.ignore_pattern(N1Pattern::QueryInLoop);
        filter.ignore_pattern(N1Pattern::ConditionalQueryInLoop);
        let findings = vec![
            finding(N1Pattern::QueryInLoop, "a.rs", 1),
            finding(N1Pattern::ConditionalQueryInLoop, "b.rs", 2),
            finding(N1Pattern::MissingEagerLoadHint, "c.rs", 3),
        ];
        let result = filter.filter(&findings);
        assert_eq!(result.len(), 1);
    }
}
