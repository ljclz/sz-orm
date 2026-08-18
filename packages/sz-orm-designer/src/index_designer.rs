//! 索引设计建议器
//!
//! 提供 [`IndexDesigner`] 基于查询模式、表结构、数据分布生成索引设计建议。

use std::collections::HashMap;
use std::fmt;

/// 查询模式
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryPattern {
    /// 查询涉及的表
    pub table: String,
    /// WHERE 子句等值列
    pub where_eq_columns: Vec<String>,
    /// WHERE 子句范围列
    pub where_range_columns: Vec<String>,
    /// ORDER BY 列
    pub order_by_columns: Vec<String>,
    /// GROUP BY 列
    pub group_by_columns: Vec<String>,
    /// JOIN 列
    pub join_columns: Vec<String>,
    /// SELECT 列（用于覆盖索引判断）
    pub select_columns: Vec<String>,
    /// 执行频率
    pub frequency: u64,
}

impl QueryPattern {
    /// 创建新查询模式
    #[must_use]
    pub fn new(table: &str) -> Self {
        Self {
            table: table.to_string(),
            where_eq_columns: Vec::new(),
            where_range_columns: Vec::new(),
            order_by_columns: Vec::new(),
            group_by_columns: Vec::new(),
            join_columns: Vec::new(),
            select_columns: Vec::new(),
            frequency: 1,
        }
    }

    /// 设置 WHERE 等值列
    #[must_use]
    pub fn where_eq(mut self, cols: &[&str]) -> Self {
        self.where_eq_columns = cols.iter().map(|s| s.to_string()).collect();
        self
    }

    /// 设置 WHERE 范围列
    #[must_use]
    pub fn where_range(mut self, cols: &[&str]) -> Self {
        self.where_range_columns = cols.iter().map(|s| s.to_string()).collect();
        self
    }

    /// 设置 ORDER BY 列
    #[must_use]
    pub fn order_by(mut self, cols: &[&str]) -> Self {
        self.order_by_columns = cols.iter().map(|s| s.to_string()).collect();
        self
    }

    /// 设置 SELECT 列
    #[must_use]
    pub fn select_cols(mut self, cols: &[&str]) -> Self {
        self.select_columns = cols.iter().map(|s| s.to_string()).collect();
        self
    }

    /// 设置执行频率
    #[must_use]
    pub fn frequency(mut self, freq: u64) -> Self {
        self.frequency = freq;
        self
    }

    /// 推荐索引的键列
    #[must_use]
    pub fn recommended_key_columns(&self) -> Vec<String> {
        let mut keys = Vec::new();
        keys.extend(self.where_eq_columns.iter().cloned());
        keys.extend(self.where_range_columns.iter().cloned());
        keys.extend(self.order_by_columns.iter().cloned());
        keys
    }

    /// 推荐索引的包含列（覆盖索引）
    #[must_use]
    pub fn recommended_included_columns(&self) -> Vec<String> {
        let keys = self.recommended_key_columns();
        self.select_columns
            .iter()
            .filter(|c| !keys.contains(c))
            .cloned()
            .collect()
    }

    /// 是否适合创建索引
    #[must_use]
    pub fn is_indexable(&self) -> bool {
        !self.where_eq_columns.is_empty()
            || !self.where_range_columns.is_empty()
            || !self.order_by_columns.is_empty()
    }
}

/// 索引设计建议
#[derive(Debug, Clone)]
pub struct IndexDesignSuggestion {
    /// 表名
    pub table: String,
    /// 建议名
    pub name: String,
    /// 键列
    pub key_columns: Vec<String>,
    /// 包含列
    pub included_columns: Vec<String>,
    /// 建议类型
    pub kind: IndexSuggestionKind,
    /// 预期收益评分（0.0~1.0）
    pub benefit_score: f64,
    /// 建议原因
    pub reason: String,
}

/// 索引建议类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexSuggestionKind {
    /// 单列索引
    SingleColumn,
    /// 复合索引
    Composite,
    /// 覆盖索引
    Covering,
    /// 唯一索引
    Unique,
    /// 部分索引
    Partial,
}

impl IndexSuggestionKind {
    /// 返回描述
    #[must_use]
    pub fn description(&self) -> &'static str {
        match self {
            IndexSuggestionKind::SingleColumn => "single column",
            IndexSuggestionKind::Composite => "composite",
            IndexSuggestionKind::Covering => "covering",
            IndexSuggestionKind::Unique => "unique",
            IndexSuggestionKind::Partial => "partial",
        }
    }
}

/// 索引设计器
#[derive(Debug, Default)]
pub struct IndexDesigner {
    /// 已收集的查询模式
    patterns: Vec<QueryPattern>,
    /// 已生成的建议
    suggestions: Vec<IndexDesignSuggestion>,
}

impl IndexDesigner {
    /// 创建新的索引设计器
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加查询模式
    pub fn add_pattern(&mut self, pattern: QueryPattern) {
        self.patterns.push(pattern);
    }

    /// 分析并生成建议
    pub fn analyze(&mut self) -> &[IndexDesignSuggestion] {
        self.suggestions.clear();
        for pattern in &self.patterns {
            if !pattern.is_indexable() {
                continue;
            }
            let key_cols = pattern.recommended_key_columns();
            let included_cols = pattern.recommended_included_columns();
            let kind = if !included_cols.is_empty() {
                IndexSuggestionKind::Covering
            } else if key_cols.len() == 1 {
                IndexSuggestionKind::SingleColumn
            } else {
                IndexSuggestionKind::Composite
            };
            let name = format!("IX_{}_{}", pattern.table, key_cols.join("_"));
            let benefit = Self::calculate_benefit(pattern);
            let reason = format!(
                "query on {} with {} eq cols, {} range cols, freq={}",
                pattern.table,
                pattern.where_eq_columns.len(),
                pattern.where_range_columns.len(),
                pattern.frequency
            );
            self.suggestions.push(IndexDesignSuggestion {
                table: pattern.table.clone(),
                name,
                key_columns: key_cols,
                included_columns: included_cols,
                kind,
                benefit_score: benefit,
                reason,
            });
        }
        self.suggestions.sort_by(|a, b| {
            b.benefit_score
                .partial_cmp(&a.benefit_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        &self.suggestions
    }

    /// 计算收益评分
    fn calculate_benefit(pattern: &QueryPattern) -> f64 {
        let mut score = 0.0;
        score += pattern.where_eq_columns.len() as f64 * 0.3;
        score += pattern.where_range_columns.len() as f64 * 0.2;
        score += pattern.order_by_columns.len() as f64 * 0.15;
        if !pattern.select_columns.is_empty() {
            score += 0.1;
        }
        let freq_factor = (pattern.frequency as f64).ln_1p() / 10.0;
        score += freq_factor;
        score.clamp(0.0, 1.0)
    }

    /// 获取所有建议
    #[must_use]
    pub fn suggestions(&self) -> &[IndexDesignSuggestion] {
        &self.suggestions
    }

    /// 获取指定表的建议
    #[must_use]
    pub fn suggestions_for_table(&self, table: &str) -> Vec<&IndexDesignSuggestion> {
        self.suggestions
            .iter()
            .filter(|s| s.table == table)
            .collect()
    }

    /// 生成 CREATE INDEX DDL
    #[must_use]
    pub fn to_create_ddls(&self) -> Vec<String> {
        self.suggestions
            .iter()
            .map(|s| {
                let mut sql = format!(
                    "CREATE INDEX {} ON {} ({})",
                    s.name,
                    s.table,
                    s.key_columns.join(", ")
                );
                if !s.included_columns.is_empty() {
                    sql.push_str(&format!(" INCLUDE ({})", s.included_columns.join(", ")));
                }
                sql.push(';');
                sql
            })
            .collect()
    }

    /// 建议数量
    #[must_use]
    pub fn suggestion_count(&self) -> usize {
        self.suggestions.len()
    }

    /// 查询模式数量
    #[must_use]
    pub fn pattern_count(&self) -> usize {
        self.patterns.len()
    }

    /// 按表分组统计建议
    #[must_use]
    pub fn suggestions_by_table(&self) -> HashMap<String, usize> {
        let mut counts = HashMap::new();
        for s in &self.suggestions {
            *counts.entry(s.table.clone()).or_insert(0) += 1;
        }
        counts
    }

    /// 平均收益评分
    #[must_use]
    pub fn avg_benefit_score(&self) -> f64 {
        if self.suggestions.is_empty() {
            0.0
        } else {
            self.suggestions
                .iter()
                .map(|s| s.benefit_score)
                .sum::<f64>()
                / self.suggestions.len() as f64
        }
    }
}

impl fmt::Display for IndexDesigner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "IndexDesigner(patterns={}, suggestions={})",
            self.pattern_count(),
            self.suggestion_count()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_pattern_new() {
        let p = QueryPattern::new("users");
        assert_eq!(p.table, "users");
        assert_eq!(p.frequency, 1);
    }

    #[test]
    fn test_query_pattern_builder() {
        let p = QueryPattern::new("users")
            .where_eq(&["email"])
            .where_range(&["created_at"])
            .order_by(&["id"])
            .select_cols(&["id", "name"])
            .frequency(100);
        assert_eq!(p.where_eq_columns.len(), 1);
        assert_eq!(p.frequency, 100);
    }

    #[test]
    fn test_query_pattern_recommended_key_columns() {
        let p = QueryPattern::new("users")
            .where_eq(&["email"])
            .where_range(&["created_at"]);
        let keys = p.recommended_key_columns();
        assert!(keys.contains(&"email".to_string()));
        assert!(keys.contains(&"created_at".to_string()));
    }

    #[test]
    fn test_query_pattern_recommended_included_columns() {
        let p = QueryPattern::new("users")
            .where_eq(&["email"])
            .select_cols(&["id", "name", "email"]);
        let included = p.recommended_included_columns();
        assert!(included.contains(&"id".to_string()));
        assert!(included.contains(&"name".to_string()));
        assert!(!included.contains(&"email".to_string()));
    }

    #[test]
    fn test_query_pattern_is_indexable() {
        let p1 = QueryPattern::new("users").where_eq(&["email"]);
        assert!(p1.is_indexable());
        let p2 = QueryPattern::new("users");
        assert!(!p2.is_indexable());
    }

    #[test]
    fn test_index_suggestion_kind_description() {
        assert_eq!(IndexSuggestionKind::Covering.description(), "covering");
        assert_eq!(IndexSuggestionKind::Composite.description(), "composite");
    }

    #[test]
    fn test_index_designer_new() {
        let designer = IndexDesigner::new();
        assert_eq!(designer.pattern_count(), 0);
    }

    #[test]
    fn test_index_designer_analyze_single() {
        let mut designer = IndexDesigner::new();
        designer.add_pattern(QueryPattern::new("users").where_eq(&["email"]));
        designer.analyze();
        assert_eq!(designer.suggestion_count(), 1);
    }

    #[test]
    fn test_index_designer_analyze_covering() {
        let mut designer = IndexDesigner::new();
        designer.add_pattern(
            QueryPattern::new("users")
                .where_eq(&["email"])
                .select_cols(&["id", "name"]),
        );
        designer.analyze();
        let s = &designer.suggestions()[0];
        assert_eq!(s.kind, IndexSuggestionKind::Covering);
        assert!(!s.included_columns.is_empty());
    }

    #[test]
    fn test_index_designer_analyze_composite() {
        let mut designer = IndexDesigner::new();
        designer.add_pattern(QueryPattern::new("users").where_eq(&["status", "type"]));
        designer.analyze();
        let s = &designer.suggestions()[0];
        assert_eq!(s.kind, IndexSuggestionKind::Composite);
    }

    #[test]
    fn test_index_designer_analyze_skip_non_indexable() {
        let mut designer = IndexDesigner::new();
        designer.add_pattern(QueryPattern::new("users"));
        designer.analyze();
        assert_eq!(designer.suggestion_count(), 0);
    }

    #[test]
    fn test_index_designer_suggestions_for_table() {
        let mut designer = IndexDesigner::new();
        designer.add_pattern(QueryPattern::new("users").where_eq(&["email"]));
        designer.add_pattern(QueryPattern::new("orders").where_eq(&["user_id"]));
        designer.analyze();
        let user_sugs = designer.suggestions_for_table("users");
        assert_eq!(user_sugs.len(), 1);
    }

    #[test]
    fn test_index_designer_to_create_ddls() {
        let mut designer = IndexDesigner::new();
        designer.add_pattern(QueryPattern::new("users").where_eq(&["email"]));
        designer.analyze();
        let ddls = designer.to_create_ddls();
        assert!(ddls[0].contains("CREATE INDEX"));
        assert!(ddls[0].contains("ON users"));
    }

    #[test]
    fn test_index_designer_to_create_ddls_with_include() {
        let mut designer = IndexDesigner::new();
        designer.add_pattern(
            QueryPattern::new("users")
                .where_eq(&["email"])
                .select_cols(&["name"]),
        );
        designer.analyze();
        let ddls = designer.to_create_ddls();
        assert!(ddls[0].contains("INCLUDE"));
    }

    #[test]
    fn test_index_designer_suggestions_by_table() {
        let mut designer = IndexDesigner::new();
        designer.add_pattern(QueryPattern::new("users").where_eq(&["email"]));
        designer.add_pattern(QueryPattern::new("users").where_eq(&["name"]));
        designer.add_pattern(QueryPattern::new("orders").where_eq(&["user_id"]));
        designer.analyze();
        let counts = designer.suggestions_by_table();
        assert_eq!(counts.get("users"), Some(&2));
        assert_eq!(counts.get("orders"), Some(&1));
    }

    #[test]
    fn test_index_designer_avg_benefit_score() {
        let mut designer = IndexDesigner::new();
        designer.add_pattern(
            QueryPattern::new("users")
                .where_eq(&["email"])
                .frequency(10),
        );
        designer.analyze();
        let avg = designer.avg_benefit_score();
        assert!(avg > 0.0);
    }

    #[test]
    fn test_index_designer_avg_benefit_empty() {
        let designer = IndexDesigner::new();
        assert_eq!(designer.avg_benefit_score(), 0.0);
    }

    #[test]
    fn test_index_designer_display() {
        let designer = IndexDesigner::new();
        let s = format!("{}", designer);
        assert!(s.contains("IndexDesigner"));
    }

    #[test]
    fn test_calculate_benefit() {
        let p = QueryPattern::new("t")
            .where_eq(&["a", "b"])
            .where_range(&["c"])
            .order_by(&["d"])
            .frequency(100);
        let score = IndexDesigner::calculate_benefit(&p);
        assert!(score > 0.0);
        assert!(score <= 1.0);
    }
}
