//! 反规范化建议
//!
//! 提供 [`DenormalizationAdvisor`] 基于查询模式与性能指标生成反规范化建议，
//! 包括冗余列、预计算列、汇总表等。

use std::collections::HashMap;
use std::fmt;

/// 反规范化建议类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenormalizationKind {
    /// 冗余列（复制关联表列到本表）
    RedundantColumn,
    /// 预计算列（存储计算结果）
    PrecomputedColumn,
    /// 汇总表（预聚合）
    SummaryTable,
    /// 物化视图
    MaterializedView,
    /// 嵌套存储（JSON）
    NestedStorage,
}

impl DenormalizationKind {
    /// 返回描述
    #[must_use]
    pub fn description(&self) -> &'static str {
        match self {
            DenormalizationKind::RedundantColumn => "redundant column",
            DenormalizationKind::PrecomputedColumn => "precomputed column",
            DenormalizationKind::SummaryTable => "summary table",
            DenormalizationKind::MaterializedView => "materialized view",
            DenormalizationKind::NestedStorage => "nested storage",
        }
    }
}

impl fmt::Display for DenormalizationKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.description())
    }
}

/// 反规范化建议
#[derive(Debug, Clone)]
pub struct DenormalizationSuggestion {
    /// 目标表
    pub table: String,
    /// 建议类型
    pub kind: DenormalizationKind,
    /// 新列名
    pub column_name: String,
    /// 源表达式（如 `orders.total_amount`）
    pub source_expression: String,
    /// 预期收益（0.0~1.0）
    pub benefit_score: f64,
    /// 存储开销评分（0.0~1.0）
    pub storage_cost: f64,
    /// 一致性风险评分（0.0~1.0）
    pub consistency_risk: f64,
    /// 建议原因
    pub reason: String,
}

impl DenormalizationSuggestion {
    /// 创建新建议
    #[must_use]
    pub fn new(table: &str, kind: DenormalizationKind, column_name: &str) -> Self {
        Self {
            table: table.to_string(),
            kind,
            column_name: column_name.to_string(),
            source_expression: String::new(),
            benefit_score: 0.0,
            storage_cost: 0.0,
            consistency_risk: 0.0,
            reason: String::new(),
        }
    }

    /// 设置源表达式
    #[must_use]
    pub fn with_source(mut self, expr: &str) -> Self {
        self.source_expression = expr.to_string();
        self
    }

    /// 设置收益评分
    #[must_use]
    pub fn with_benefit(mut self, score: f64) -> Self {
        self.benefit_score = score.clamp(0.0, 1.0);
        self
    }

    /// 设置存储开销
    #[must_use]
    pub fn with_storage_cost(mut self, cost: f64) -> Self {
        self.storage_cost = cost.clamp(0.0, 1.0);
        self
    }

    /// 设置一致性风险
    #[must_use]
    pub fn with_consistency_risk(mut self, risk: f64) -> Self {
        self.consistency_risk = risk.clamp(0.0, 1.0);
        self
    }

    /// 设置原因
    #[must_use]
    pub fn with_reason(mut self, reason: &str) -> Self {
        self.reason = reason.to_string();
        self
    }

    /// 综合评分（收益 - 开销 - 风险）
    #[must_use]
    pub fn composite_score(&self) -> f64 {
        self.benefit_score * 0.5 - self.storage_cost * 0.2 - self.consistency_risk * 0.3
    }

    /// 是否值得采纳
    #[must_use]
    pub fn is_worthwhile(&self) -> bool {
        self.composite_score() > 0.0
    }
}

/// JOIN 查询模式（用于检测反规范化机会）
#[derive(Debug, Clone)]
pub struct JoinPattern {
    /// 主表
    pub table: String,
    /// JOIN 表
    pub join_table: String,
    /// JOIN 列
    pub join_columns: Vec<String>,
    /// 获取的 JOIN 表列
    pub fetched_columns: Vec<String>,
    /// 执行频率
    pub frequency: u64,
    /// 平均行数
    pub avg_rows: u64,
}

impl JoinPattern {
    /// 创建新 JOIN 模式
    #[must_use]
    pub fn new(table: &str, join_table: &str) -> Self {
        Self {
            table: table.to_string(),
            join_table: join_table.to_string(),
            join_columns: Vec::new(),
            fetched_columns: Vec::new(),
            frequency: 1,
            avg_rows: 0,
        }
    }

    /// 设置 JOIN 列
    #[must_use]
    pub fn join_cols(mut self, cols: &[&str]) -> Self {
        self.join_columns = cols.iter().map(|s| s.to_string()).collect();
        self
    }

    /// 设置获取列
    #[must_use]
    pub fn fetched_cols(mut self, cols: &[&str]) -> Self {
        self.fetched_columns = cols.iter().map(|s| s.to_string()).collect();
        self
    }

    /// 设置频率
    #[must_use]
    pub fn frequency(mut self, freq: u64) -> Self {
        self.frequency = freq;
        self
    }

    /// 设置平均行数
    #[must_use]
    pub fn avg_rows(mut self, rows: u64) -> Self {
        self.avg_rows = rows;
        self
    }
}

/// 反规范化建议器
#[derive(Debug, Default)]
pub struct DenormalizationAdvisor {
    /// JOIN 模式
    join_patterns: Vec<JoinPattern>,
    /// 已生成的建议
    suggestions: Vec<DenormalizationSuggestion>,
}

impl DenormalizationAdvisor {
    /// 创建新的建议器
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加 JOIN 模式
    pub fn add_join_pattern(&mut self, pattern: JoinPattern) {
        self.join_patterns.push(pattern);
    }

    /// 分析并生成建议
    pub fn analyze(&mut self) -> &[DenormalizationSuggestion] {
        self.suggestions.clear();
        for pattern in &self.join_patterns {
            for col in &pattern.fetched_columns {
                let benefit = Self::calculate_benefit(pattern);
                let storage_cost = 0.1;
                let consistency_risk = 0.3;
                let new_col_name = format!("{}_{}", pattern.join_table, col);
                let reason = format!(
                    "frequently join {}.{} to get {} (freq={})",
                    pattern.join_table, pattern.join_table, col, pattern.frequency
                );
                self.suggestions.push(
                    DenormalizationSuggestion::new(
                        &pattern.table,
                        DenormalizationKind::RedundantColumn,
                        &new_col_name,
                    )
                    .with_source(&format!("{}.{}", pattern.join_table, col))
                    .with_benefit(benefit)
                    .with_storage_cost(storage_cost)
                    .with_consistency_risk(consistency_risk)
                    .with_reason(&reason),
                );
            }
        }
        self.suggestions.sort_by(|a, b| {
            b.composite_score()
                .partial_cmp(&a.composite_score())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        &self.suggestions
    }

    /// 计算收益评分
    fn calculate_benefit(pattern: &JoinPattern) -> f64 {
        let freq_score = (pattern.frequency as f64).ln_1p() / 10.0;
        let row_score = if pattern.avg_rows > 1000 { 0.3 } else { 0.1 };
        (freq_score + row_score).clamp(0.0, 1.0)
    }

    /// 获取所有建议
    #[must_use]
    pub fn suggestions(&self) -> &[DenormalizationSuggestion] {
        &self.suggestions
    }

    /// 获取值得采纳的建议
    #[must_use]
    pub fn worthwhile_suggestions(&self) -> Vec<&DenormalizationSuggestion> {
        self.suggestions
            .iter()
            .filter(|s| s.is_worthwhile())
            .collect()
    }

    /// 按表分组建议
    #[must_use]
    pub fn suggestions_by_table(&self) -> HashMap<String, Vec<&DenormalizationSuggestion>> {
        let mut groups: HashMap<String, Vec<&DenormalizationSuggestion>> = HashMap::new();
        for s in &self.suggestions {
            groups.entry(s.table.clone()).or_default().push(s);
        }
        groups
    }

    /// 生成建议报告
    #[must_use]
    pub fn report(&self) -> String {
        let mut report = String::from("Denormalization Suggestions:\n");
        for s in &self.suggestions {
            let mark = if s.is_worthwhile() {
                "[RECOMMEND]"
            } else {
                "[SKIP]"
            };
            report.push_str(&format!(
                "  {} {}.{} ({}) score={:.2}\n",
                mark,
                s.table,
                s.column_name,
                s.kind,
                s.composite_score()
            ));
        }
        report
    }

    /// 建议数量
    #[must_use]
    pub fn suggestion_count(&self) -> usize {
        self.suggestions.len()
    }

    /// JOIN 模式数量
    #[must_use]
    pub fn join_pattern_count(&self) -> usize {
        self.join_patterns.len()
    }
}

impl fmt::Display for DenormalizationAdvisor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "DenormalizationAdvisor(joins={}, suggestions={})",
            self.join_pattern_count(),
            self.suggestion_count()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_denormalization_kind_description() {
        assert_eq!(
            DenormalizationKind::RedundantColumn.description(),
            "redundant column"
        );
        assert_eq!(
            DenormalizationKind::SummaryTable.description(),
            "summary table"
        );
    }

    #[test]
    fn test_denormalization_suggestion_new() {
        let s = DenormalizationSuggestion::new(
            "orders",
            DenormalizationKind::RedundantColumn,
            "user_name",
        );
        assert_eq!(s.table, "orders");
        assert_eq!(s.column_name, "user_name");
    }

    #[test]
    fn test_denormalization_suggestion_composite_score() {
        let s = DenormalizationSuggestion::new("t", DenormalizationKind::RedundantColumn, "c")
            .with_benefit(0.8)
            .with_storage_cost(0.1)
            .with_consistency_risk(0.2);
        let score = s.composite_score();
        assert!(score > 0.0);
    }

    #[test]
    fn test_denormalization_suggestion_is_worthwhile() {
        let s1 = DenormalizationSuggestion::new("t", DenormalizationKind::RedundantColumn, "c")
            .with_benefit(0.9)
            .with_storage_cost(0.1)
            .with_consistency_risk(0.1);
        assert!(s1.is_worthwhile());
        let s2 = DenormalizationSuggestion::new("t", DenormalizationKind::RedundantColumn, "c")
            .with_benefit(0.1)
            .with_storage_cost(0.5)
            .with_consistency_risk(0.8);
        assert!(!s2.is_worthwhile());
    }

    #[test]
    fn test_join_pattern_new() {
        let p = JoinPattern::new("orders", "users");
        assert_eq!(p.table, "orders");
        assert_eq!(p.join_table, "users");
    }

    #[test]
    fn test_join_pattern_builder() {
        let p = JoinPattern::new("orders", "users")
            .join_cols(&["user_id"])
            .fetched_cols(&["name", "email"])
            .frequency(100)
            .avg_rows(5000);
        assert_eq!(p.fetched_columns.len(), 2);
        assert_eq!(p.frequency, 100);
    }

    #[test]
    fn test_advisor_new() {
        let advisor = DenormalizationAdvisor::new();
        assert_eq!(advisor.join_pattern_count(), 0);
    }

    #[test]
    fn test_advisor_analyze() {
        let mut advisor = DenormalizationAdvisor::new();
        advisor.add_join_pattern(
            JoinPattern::new("orders", "users")
                .fetched_cols(&["name"])
                .frequency(100),
        );
        advisor.analyze();
        assert_eq!(advisor.suggestion_count(), 1);
    }

    #[test]
    fn test_advisor_worthwhile_suggestions() {
        let mut advisor = DenormalizationAdvisor::new();
        advisor.add_join_pattern(
            JoinPattern::new("orders", "users")
                .fetched_cols(&["name"])
                .frequency(1000)
                .avg_rows(5000),
        );
        advisor.analyze();
        let worthwhile = advisor.worthwhile_suggestions();
        assert!(!worthwhile.is_empty());
    }

    #[test]
    fn test_advisor_suggestions_by_table() {
        let mut advisor = DenormalizationAdvisor::new();
        advisor.add_join_pattern(
            JoinPattern::new("orders", "users")
                .fetched_cols(&["name"])
                .frequency(10),
        );
        advisor.add_join_pattern(
            JoinPattern::new("items", "products")
                .fetched_cols(&["price"])
                .frequency(10),
        );
        advisor.analyze();
        let groups = advisor.suggestions_by_table();
        assert!(groups.contains_key("orders"));
        assert!(groups.contains_key("items"));
    }

    #[test]
    fn test_advisor_report() {
        let mut advisor = DenormalizationAdvisor::new();
        advisor.add_join_pattern(
            JoinPattern::new("orders", "users")
                .fetched_cols(&["name"])
                .frequency(10),
        );
        advisor.analyze();
        let report = advisor.report();
        assert!(report.contains("Denormalization Suggestions"));
    }

    #[test]
    fn test_advisor_display() {
        let advisor = DenormalizationAdvisor::new();
        let s = format!("{}", advisor);
        assert!(s.contains("DenormalizationAdvisor"));
    }

    #[test]
    fn test_calculate_benefit() {
        let p = JoinPattern::new("t", "u").frequency(100).avg_rows(5000);
        let score = DenormalizationAdvisor::calculate_benefit(&p);
        assert!(score > 0.0);
        assert!(score <= 1.0);
    }
}
