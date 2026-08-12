//! 优化建议规则引擎（`query-advisor` feature）
//!
//! [`OptimizationAdvisor::suggest`] 入口接收 `ExplainPlan` + `QueryStats`，
//! 调用六条规则生成建议列表，按置信度降序排序并处理冲突。

use crate::dialect::AdvisorDialect;
use crate::rules::{
    rule_add_index, rule_adjust_pool_size, rule_drop_index, rule_enable_cache, rule_rewrite_query,
    rule_use_pagination,
};
use crate::suggestion::OptimizationSuggestion;
use sz_orm_adaptive::stats::QueryStats;
use sz_orm_explain::ExplainPlan;

/// 规则引擎配置
#[derive(Debug, Clone)]
pub struct AdvisorConfig {
    /// 行数阈值（超过此值视为大结果集，默认 1000）
    pub row_threshold: u64,
    /// 置信度阈值（低于此值标注"需人工确认"，默认 0.5）
    pub confidence_threshold: f64,
    /// 池获取耗时占比阈值（超过此百分比建议调整池大小，默认 30.0）
    pub pool_pct_threshold: f64,
    /// 目标方言（影响 DDL 生成）
    pub dialect: AdvisorDialect,
}

impl Default for AdvisorConfig {
    fn default() -> Self {
        Self {
            row_threshold: 1000,
            confidence_threshold: 0.5,
            pool_pct_threshold: 30.0,
            dialect: AdvisorDialect::MySQL,
        }
    }
}

/// 优化建议规则引擎
pub struct OptimizationAdvisor {
    config: AdvisorConfig,
}

impl OptimizationAdvisor {
    /// 创建规则引擎实例
    pub fn new(config: AdvisorConfig) -> Self {
        Self { config }
    }

    /// 使用默认配置创建
    pub fn with_defaults() -> Self {
        Self::new(AdvisorConfig::default())
    }

    /// 获取配置引用
    pub fn config(&self) -> &AdvisorConfig {
        &self.config
    }

    /// 生成优化建议列表
    ///
    /// 输入：可选的 EXPLAIN 计划 + 可选的查询统计 + 可选的池耗时占比
    /// 输出：按置信度降序排序的建议列表，冲突建议低置信度标记 "conflict, skipped"
    pub fn suggest(
        &self,
        plan: Option<&ExplainPlan>,
        stats: Option<&QueryStats>,
        pool_pct: Option<f64>,
    ) -> Vec<OptimizationSuggestion> {
        let mut suggestions = Vec::new();

        if let Some(p) = plan {
            if let Some(s) = rule_add_index(p, &self.config, self.config.dialect) {
                suggestions.push(s);
            }
            if let Some(s) = rule_drop_index(p) {
                suggestions.push(s);
            }
            if let Some(s) = rule_rewrite_query(p) {
                suggestions.push(s);
            }
        }

        if let Some(st) = stats {
            if let Some(s) = rule_use_pagination(st, &self.config) {
                suggestions.push(s);
            }
            if let Some(s) = rule_enable_cache(st) {
                suggestions.push(s);
            }
        }

        if let Some(pct) = pool_pct {
            if let Some(s) = rule_adjust_pool_size(pct) {
                suggestions.push(s);
            }
        }

        resolve_conflicts(&mut suggestions);
        suggestions.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        suggestions
    }
}

/// 冲突处理：AddIndex 与 DropIndex 针对同一表时，低置信度标记 "conflict, skipped"
fn resolve_conflicts(suggestions: &mut [OptimizationSuggestion]) {
    use crate::suggestion::SuggestionType;

    let mut add_indices: Vec<(usize, String, f64)> = Vec::new();
    let mut drop_indices: Vec<(usize, String, f64)> = Vec::new();

    for (i, s) in suggestions.iter().enumerate() {
        match s.suggestion_type {
            SuggestionType::AddIndex => {
                add_indices.push((i, s.target_query.clone(), s.confidence));
            }
            SuggestionType::DropIndex => {
                drop_indices.push((i, s.target_query.clone(), s.confidence));
            }
            _ => {}
        }
    }

    for (add_idx, add_query, add_conf) in &add_indices {
        for (drop_idx, drop_query, drop_conf) in &drop_indices {
            if add_query == drop_query {
                if add_conf > drop_conf {
                    suggestions[*drop_idx].conflict_note = Some("conflict, skipped".into());
                } else {
                    suggestions[*add_idx].conflict_note = Some("conflict, skipped".into());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sz_orm_explain::{ExplainPlan, ScanType};

    fn plan(scan: ScanType, index: Option<&str>, rows: u64) -> ExplainPlan {
        ExplainPlan {
            scan_type: scan,
            table: "users".into(),
            index: index.map(|s| s.to_string()),
            rows,
            extra: Vec::new(),
        }
    }

    #[test]
    fn full_table_scan_generates_add_index() {
        let advisor = OptimizationAdvisor::with_defaults();
        let p = plan(ScanType::FullTable, None, 10000);
        let suggestions = advisor.suggest(Some(&p), None, None);
        assert!(suggestions.iter().any(|s| s.suggestion_type
            == crate::suggestion::SuggestionType::AddIndex
            && (s.confidence - 0.9).abs() < 1e-9));
    }

    #[test]
    fn large_result_set_generates_pagination() {
        let advisor = OptimizationAdvisor::with_defaults();
        let stats = QueryStats::new();
        for _ in 0..10 {
            stats.record(2000, 5_000);
        }
        let suggestions = advisor.suggest(None, Some(&stats), None);
        assert!(suggestions
            .iter()
            .any(|s| s.suggestion_type == crate::suggestion::SuggestionType::UsePagination));
    }

    #[test]
    fn hot_query_generates_cache() {
        let advisor = OptimizationAdvisor::with_defaults();
        let stats = QueryStats::new();
        for _ in 0..10 {
            stats.record(10, 50_000);
        }
        let suggestions = advisor.suggest(None, Some(&stats), None);
        assert!(suggestions
            .iter()
            .any(|s| s.suggestion_type == crate::suggestion::SuggestionType::EnableCache));
    }

    #[test]
    fn conflict_add_and_drop_same_table() {
        let advisor = OptimizationAdvisor::with_defaults();
        let p1 = plan(ScanType::FullTable, None, 10000);
        let p2 = plan(ScanType::IndexLookup, Some("idx"), 5);
        let s1 = advisor.suggest(Some(&p1), None, None);
        let s2 = advisor.suggest(Some(&p2), None, None);
        assert!(s1
            .iter()
            .any(|s| s.suggestion_type == crate::suggestion::SuggestionType::AddIndex));
        assert!(s2
            .iter()
            .any(|s| s.suggestion_type == crate::suggestion::SuggestionType::DropIndex));
    }

    #[test]
    fn no_plan_no_stats_returns_empty() {
        let advisor = OptimizationAdvisor::with_defaults();
        let suggestions = advisor.suggest(None, None, None);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn suggestions_sorted_by_confidence_desc() {
        let advisor = OptimizationAdvisor::with_defaults();
        let p = plan(ScanType::FullTable, None, 10000);
        let stats = QueryStats::new();
        for _ in 0..10 {
            stats.record(2000, 5_000);
        }
        let suggestions = advisor.suggest(Some(&p), Some(&stats), Some(45.0));
        for w in suggestions.windows(2) {
            assert!(w[0].confidence >= w[1].confidence);
        }
    }

    #[test]
    fn pool_pct_triggers_adjust() {
        let advisor = OptimizationAdvisor::with_defaults();
        let suggestions = advisor.suggest(None, None, Some(45.0));
        assert!(suggestions
            .iter()
            .any(|s| s.suggestion_type == crate::suggestion::SuggestionType::AdjustPoolSize));
    }
}
