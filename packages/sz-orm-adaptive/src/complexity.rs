//! 查询复杂度评估器与自适应索引选择器
//!
//! 评估查询复杂度并选择最优索引。

use std::collections::HashMap;

/// 查询复杂度等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ComplexityLevel {
    /// 简单（单表、无 JOIN、无子查询）
    Simple,
    /// 中等（单表 + WHERE + ORDER BY）
    Medium,
    /// 复杂（多表 JOIN）
    Complex,
    /// 高度复杂（子查询 + 多 JOIN + 聚合）
    HighlyComplex,
}

impl ComplexityLevel {
    /// 人类可读名称
    pub fn as_str(&self) -> &'static str {
        match self {
            ComplexityLevel::Simple => "simple",
            ComplexityLevel::Medium => "medium",
            ComplexityLevel::Complex => "complex",
            ComplexityLevel::HighlyComplex => "highly-complex",
        }
    }

    /// 复杂度权重（用于排序）
    pub fn weight(&self) -> u8 {
        match self {
            ComplexityLevel::Simple => 1,
            ComplexityLevel::Medium => 2,
            ComplexityLevel::Complex => 3,
            ComplexityLevel::HighlyComplex => 4,
        }
    }
}

/// 查询特征
#[derive(Debug, Clone, Default)]
pub struct QueryFeatures {
    /// 涉及的表数
    pub table_count: usize,
    /// WHERE 条件数
    pub where_conditions: usize,
    /// JOIN 数
    pub join_count: usize,
    /// 子查询数
    pub subquery_count: usize,
    /// 聚合函数数
    pub aggregate_count: usize,
    /// ORDER BY 列数
    pub order_by_columns: usize,
    /// GROUP BY 列数
    pub group_by_columns: usize,
    /// 是否有 DISTINCT
    pub has_distinct: bool,
    /// 是否有 LIMIT
    pub has_limit: bool,
}

impl QueryFeatures {
    /// 创建空特征
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置表数
    pub fn with_tables(mut self, count: usize) -> Self {
        self.table_count = count;
        self
    }

    /// 设置 WHERE 条件数
    pub fn with_where(mut self, count: usize) -> Self {
        self.where_conditions = count;
        self
    }

    /// 设置 JOIN 数
    pub fn with_joins(mut self, count: usize) -> Self {
        self.join_count = count;
        self
    }

    /// 设置子查询数
    pub fn with_subqueries(mut self, count: usize) -> Self {
        self.subquery_count = count;
        self
    }

    /// 设置聚合函数数
    pub fn with_aggregates(mut self, count: usize) -> Self {
        self.aggregate_count = count;
        self
    }

    /// 设置 ORDER BY 列数
    pub fn with_order_by(mut self, count: usize) -> Self {
        self.order_by_columns = count;
        self
    }

    /// 设置 GROUP BY 列数
    pub fn with_group_by(mut self, count: usize) -> Self {
        self.group_by_columns = count;
        self
    }

    /// 设置是否有 DISTINCT
    pub fn with_distinct(mut self) -> Self {
        self.has_distinct = true;
        self
    }

    /// 设置是否有 LIMIT
    pub fn with_limit(mut self) -> Self {
        self.has_limit = true;
        self
    }
}

/// 查询复杂度评估器
///
/// 根据 [`QueryFeatures`] 评估查询复杂度等级。
pub struct QueryComplexityEvaluator {
    /// 简单查询的最大 WHERE 条件数
    simple_max_where: usize,
    /// 中等查询的最大 JOIN 数
    medium_max_joins: usize,
}

impl QueryComplexityEvaluator {
    /// 创建评估器
    pub fn new() -> Self {
        Self {
            simple_max_where: 3,
            medium_max_joins: 1,
        }
    }

    /// 评估复杂度
    pub fn evaluate(&self, features: &QueryFeatures) -> ComplexityLevel {
        if features.subquery_count > 0 || (features.join_count > 2 && features.aggregate_count > 0)
        {
            return ComplexityLevel::HighlyComplex;
        }
        if features.join_count > self.medium_max_joins || features.group_by_columns > 0 {
            return ComplexityLevel::Complex;
        }
        if features.where_conditions > self.simple_max_where
            || features.order_by_columns > 0
            || features.has_distinct
        {
            return ComplexityLevel::Medium;
        }
        ComplexityLevel::Simple
    }

    /// 估算成本因子（1.0 = 基准）
    pub fn cost_factor(&self, features: &QueryFeatures) -> f64 {
        let level = self.evaluate(features);
        let base = match level {
            ComplexityLevel::Simple => 1.0,
            ComplexityLevel::Medium => 2.0,
            ComplexityLevel::Complex => 5.0,
            ComplexityLevel::HighlyComplex => 10.0,
        };
        let join_factor = 1.0 + features.join_count as f64 * 0.5;
        let subquery_factor = 1.0 + features.subquery_count as f64 * 2.0;
        base * join_factor * subquery_factor
    }

    /// 是否需要优化
    pub fn needs_optimization(&self, features: &QueryFeatures) -> bool {
        self.evaluate(features) >= ComplexityLevel::Complex
    }
}

impl Default for QueryComplexityEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

/// 索引信息
#[derive(Debug, Clone)]
pub struct IndexInfo {
    /// 索引名
    pub name: String,
    /// 索引列
    pub columns: Vec<String>,
    /// 是否唯一索引
    pub is_unique: bool,
    /// 是否覆盖索引
    pub is_covering: bool,
    /// 基数（不同值数量）
    pub cardinality: u64,
}

impl IndexInfo {
    /// 创建索引信息
    pub fn new(name: impl Into<String>, columns: Vec<String>) -> Self {
        Self {
            name: name.into(),
            columns,
            is_unique: false,
            is_covering: false,
            cardinality: 0,
        }
    }

    /// 标记为唯一索引
    pub fn unique(mut self) -> Self {
        self.is_unique = true;
        self
    }

    /// 标记为覆盖索引
    pub fn covering(mut self) -> Self {
        self.is_covering = true;
        self
    }

    /// 设置基数
    pub fn with_cardinality(mut self, cardinality: u64) -> Self {
        self.cardinality = cardinality;
        self
    }

    /// 选择性（0.0~1.0，越高越优）
    pub fn selectivity(&self, table_rows: u64) -> f64 {
        if table_rows == 0 {
            return 0.0;
        }
        (self.cardinality as f64 / table_rows as f64).min(1.0)
    }

    /// 是否包含指定列
    pub fn contains_column(&self, column: &str) -> bool {
        self.columns.iter().any(|c| c == column)
    }
}

/// 自适应索引选择器
///
/// 根据查询条件与可用索引选择最优索引。
pub struct AdaptiveIndexSelector {
    /// 表的可用索引：table_name -> indexes
    indexes: HashMap<String, Vec<IndexInfo>>,
}

impl AdaptiveIndexSelector {
    /// 创建空选择器
    pub fn new() -> Self {
        Self {
            indexes: HashMap::new(),
        }
    }

    /// 为表添加索引
    pub fn add_index(&mut self, table: impl Into<String>, index: IndexInfo) {
        self.indexes.entry(table.into()).or_default().push(index);
    }

    /// 获取表的索引列表
    pub fn indexes_for(&self, table: &str) -> Option<&[IndexInfo]> {
        self.indexes.get(table).map(|v| v.as_slice())
    }

    /// 为查询条件选择最优索引
    ///
    /// `where_columns` 为 WHERE 条件涉及的列。
    /// 返回最优索引名，无合适索引返回 `None`。
    pub fn select(
        &self,
        table: &str,
        where_columns: &[String],
        table_rows: u64,
    ) -> Option<&IndexInfo> {
        let indexes = self.indexes.get(table)?;
        let mut best: Option<(&IndexInfo, f64)> = None;
        for index in indexes {
            let covers_query = where_columns.iter().all(|col| index.contains_column(col));
            if !covers_query && !index.is_covering {
                continue;
            }
            let score = self.score_index(index, where_columns, table_rows);
            match best {
                None => best = Some((index, score)),
                Some((_, best_score)) if score > best_score => best = Some((index, score)),
                _ => {}
            }
        }
        best.map(|(idx, _)| idx)
    }

    fn score_index(&self, index: &IndexInfo, where_columns: &[String], table_rows: u64) -> f64 {
        let mut score = index.selectivity(table_rows);
        if index.is_unique {
            score += 1.0;
        }
        if index.is_covering {
            score += 0.5;
        }
        let matched_columns = where_columns
            .iter()
            .filter(|col| index.contains_column(col))
            .count();
        score += matched_columns as f64 * 0.1;
        score
    }

    /// 已注册的表数
    pub fn table_count(&self) -> usize {
        self.indexes.len()
    }

    /// 总索引数
    pub fn total_index_count(&self) -> usize {
        self.indexes.values().map(|v| v.len()).sum()
    }
}

impl Default for AdaptiveIndexSelector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- ComplexityLevel tests ---

    #[test]
    fn complexity_level_as_str() {
        assert_eq!(ComplexityLevel::Simple.as_str(), "simple");
        assert_eq!(ComplexityLevel::Medium.as_str(), "medium");
        assert_eq!(ComplexityLevel::Complex.as_str(), "complex");
        assert_eq!(ComplexityLevel::HighlyComplex.as_str(), "highly-complex");
    }

    #[test]
    fn complexity_level_weight() {
        assert_eq!(ComplexityLevel::Simple.weight(), 1);
        assert_eq!(ComplexityLevel::Medium.weight(), 2);
        assert_eq!(ComplexityLevel::Complex.weight(), 3);
        assert_eq!(ComplexityLevel::HighlyComplex.weight(), 4);
    }

    #[test]
    fn complexity_level_ordering() {
        assert!(ComplexityLevel::Simple < ComplexityLevel::Medium);
        assert!(ComplexityLevel::Medium < ComplexityLevel::Complex);
        assert!(ComplexityLevel::Complex < ComplexityLevel::HighlyComplex);
    }

    // --- QueryFeatures tests ---

    #[test]
    fn features_default() {
        let f = QueryFeatures::default();
        assert_eq!(f.table_count, 0);
        assert!(!f.has_distinct);
    }

    #[test]
    fn features_builder() {
        let f = QueryFeatures::new()
            .with_tables(2)
            .with_where(3)
            .with_joins(1)
            .with_subqueries(0)
            .with_aggregates(1)
            .with_order_by(2)
            .with_group_by(1)
            .with_distinct()
            .with_limit();
        assert_eq!(f.table_count, 2);
        assert_eq!(f.where_conditions, 3);
        assert!(f.has_distinct);
        assert!(f.has_limit);
    }

    // --- QueryComplexityEvaluator tests ---

    #[test]
    fn evaluator_simple() {
        let e = QueryComplexityEvaluator::new();
        let f = QueryFeatures::new().with_tables(1).with_where(2);
        assert_eq!(e.evaluate(&f), ComplexityLevel::Simple);
    }

    #[test]
    fn evaluator_medium() {
        let e = QueryComplexityEvaluator::new();
        let f = QueryFeatures::new().with_tables(1).with_where(5);
        assert_eq!(e.evaluate(&f), ComplexityLevel::Medium);
    }

    #[test]
    fn evaluator_complex() {
        let e = QueryComplexityEvaluator::new();
        let f = QueryFeatures::new().with_tables(3).with_joins(2);
        assert_eq!(e.evaluate(&f), ComplexityLevel::Complex);
    }

    #[test]
    fn evaluator_highly_complex_subquery() {
        let e = QueryComplexityEvaluator::new();
        let f = QueryFeatures::new().with_subqueries(1);
        assert_eq!(e.evaluate(&f), ComplexityLevel::HighlyComplex);
    }

    #[test]
    fn evaluator_highly_complex_join_aggregate() {
        let e = QueryComplexityEvaluator::new();
        let f = QueryFeatures::new().with_joins(3).with_aggregates(1);
        assert_eq!(e.evaluate(&f), ComplexityLevel::HighlyComplex);
    }

    #[test]
    fn evaluator_cost_factor() {
        let e = QueryComplexityEvaluator::new();
        let f = QueryFeatures::new().with_tables(1);
        let cost = e.cost_factor(&f);
        assert!(cost >= 1.0);
    }

    #[test]
    fn evaluator_cost_factor_increases_with_complexity() {
        let e = QueryComplexityEvaluator::new();
        let simple = QueryFeatures::new().with_tables(1);
        let complex = QueryFeatures::new().with_joins(3).with_aggregates(1);
        assert!(e.cost_factor(&simple) < e.cost_factor(&complex));
    }

    #[test]
    fn evaluator_needs_optimization() {
        let e = QueryComplexityEvaluator::new();
        let simple = QueryFeatures::new().with_tables(1);
        let complex = QueryFeatures::new().with_joins(3);
        assert!(!e.needs_optimization(&simple));
        assert!(e.needs_optimization(&complex));
    }

    #[test]
    fn evaluator_default() {
        let e = QueryComplexityEvaluator::default();
        let f = QueryFeatures::new();
        assert_eq!(e.evaluate(&f), ComplexityLevel::Simple);
    }

    #[test]
    fn evaluator_order_by_medium() {
        let e = QueryComplexityEvaluator::new();
        let f = QueryFeatures::new().with_order_by(1);
        assert_eq!(e.evaluate(&f), ComplexityLevel::Medium);
    }

    #[test]
    fn evaluator_group_by_complex() {
        let e = QueryComplexityEvaluator::new();
        let f = QueryFeatures::new().with_group_by(1);
        assert_eq!(e.evaluate(&f), ComplexityLevel::Complex);
    }

    #[test]
    fn evaluator_distinct_medium() {
        let e = QueryComplexityEvaluator::new();
        let f = QueryFeatures::new().with_distinct();
        assert_eq!(e.evaluate(&f), ComplexityLevel::Medium);
    }

    // --- IndexInfo tests ---

    #[test]
    fn index_info_new() {
        let idx = IndexInfo::new("idx_email", vec!["email".to_string()]);
        assert_eq!(idx.name, "idx_email");
        assert!(!idx.is_unique);
        assert!(!idx.is_covering);
    }

    #[test]
    fn index_info_unique() {
        let idx = IndexInfo::new("idx_id", vec!["id".to_string()]).unique();
        assert!(idx.is_unique);
    }

    #[test]
    fn index_info_covering() {
        let idx = IndexInfo::new("idx_cover", vec!["a".to_string(), "b".to_string()]).covering();
        assert!(idx.is_covering);
    }

    #[test]
    fn index_info_with_cardinality() {
        let idx = IndexInfo::new("idx", vec!["a".to_string()]).with_cardinality(1000);
        assert_eq!(idx.cardinality, 1000);
    }

    #[test]
    fn index_info_selectivity() {
        let idx = IndexInfo::new("idx", vec!["a".to_string()]).with_cardinality(800);
        assert!((idx.selectivity(1000) - 0.8).abs() < 1e-9);
    }

    #[test]
    fn index_info_selectivity_zero_rows() {
        let idx = IndexInfo::new("idx", vec!["a".to_string()]);
        assert_eq!(idx.selectivity(0), 0.0);
    }

    #[test]
    fn index_info_contains_column() {
        let idx = IndexInfo::new("idx", vec!["a".to_string(), "b".to_string()]);
        assert!(idx.contains_column("a"));
        assert!(idx.contains_column("b"));
        assert!(!idx.contains_column("c"));
    }

    // --- AdaptiveIndexSelector tests ---

    #[test]
    fn selector_empty() {
        let s = AdaptiveIndexSelector::new();
        assert_eq!(s.table_count(), 0);
        assert_eq!(s.total_index_count(), 0);
    }

    #[test]
    fn selector_add_index() {
        let mut s = AdaptiveIndexSelector::new();
        s.add_index(
            "users",
            IndexInfo::new("idx_email", vec!["email".to_string()]),
        );
        assert_eq!(s.table_count(), 1);
        assert_eq!(s.total_index_count(), 1);
    }

    #[test]
    fn selector_indexes_for() {
        let mut s = AdaptiveIndexSelector::new();
        s.add_index(
            "users",
            IndexInfo::new("idx_email", vec!["email".to_string()]),
        );
        assert!(s.indexes_for("users").is_some());
        assert!(s.indexes_for("orders").is_none());
    }

    #[test]
    fn selector_select_matching() {
        let mut s = AdaptiveIndexSelector::new();
        s.add_index(
            "users",
            IndexInfo::new("idx_email", vec!["email".to_string()]).with_cardinality(900),
        );
        let selected = s.select("users", &["email".to_string()], 1000);
        assert!(selected.is_some());
        assert_eq!(selected.unwrap().name, "idx_email");
    }

    #[test]
    fn selector_select_no_match() {
        let mut s = AdaptiveIndexSelector::new();
        s.add_index(
            "users",
            IndexInfo::new("idx_email", vec!["email".to_string()]),
        );
        let selected = s.select("users", &["name".to_string()], 1000);
        assert!(selected.is_none());
    }

    #[test]
    fn selector_select_best() {
        let mut s = AdaptiveIndexSelector::new();
        s.add_index(
            "users",
            IndexInfo::new("idx_name", vec!["name".to_string()]).with_cardinality(500),
        );
        s.add_index(
            "users",
            IndexInfo::new("idx_email", vec!["email".to_string()]).with_cardinality(950),
        );
        let selected = s.select("users", &["email".to_string()], 1000);
        assert!(selected.is_some());
        assert_eq!(selected.unwrap().name, "idx_email");
    }

    #[test]
    fn selector_select_unique_preferred() {
        let mut s = AdaptiveIndexSelector::new();
        s.add_index(
            "users",
            IndexInfo::new("idx_name", vec!["name".to_string()]).with_cardinality(950),
        );
        s.add_index(
            "users",
            IndexInfo::new("idx_id", vec!["id".to_string()])
                .unique()
                .with_cardinality(950),
        );
        let selected = s.select("users", &["id".to_string()], 1000);
        assert!(selected.is_some());
    }

    #[test]
    fn selector_select_covering_preferred() {
        let mut s = AdaptiveIndexSelector::new();
        s.add_index(
            "users",
            IndexInfo::new("idx_a", vec!["a".to_string()]).with_cardinality(500),
        );
        s.add_index(
            "users",
            IndexInfo::new("idx_ab", vec!["a".to_string(), "b".to_string()])
                .covering()
                .with_cardinality(500),
        );
        let selected = s.select("users", &["a".to_string(), "b".to_string()], 1000);
        assert!(selected.is_some());
    }

    #[test]
    fn selector_default() {
        let s = AdaptiveIndexSelector::default();
        assert_eq!(s.table_count(), 0);
    }

    #[test]
    fn selector_multiple_tables() {
        let mut s = AdaptiveIndexSelector::new();
        s.add_index("users", IndexInfo::new("idx1", vec!["a".to_string()]));
        s.add_index("orders", IndexInfo::new("idx2", vec!["b".to_string()]));
        s.add_index("orders", IndexInfo::new("idx3", vec!["c".to_string()]));
        assert_eq!(s.table_count(), 2);
        assert_eq!(s.total_index_count(), 3);
    }
}
