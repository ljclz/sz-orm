//! 六种优化建议规则（`query-advisor` feature）
//!
//! 每条规则复用既有决策谓词，输入 `ExplainPlan` + `QueryStats` → 输出 `OptimizationSuggestion`。
//! - AddIndex 复用 `ExplainPlan::missing_index` `packages/sz-orm-explain/src/lib.rs:91`
//! - UsePagination 复用 `QueryStats::should_paginate` `packages/sz-orm-adaptive/src/stats.rs:66`
//! - EnableCache 复用 `QueryStats::should_cache` `packages/sz-orm-adaptive/src/stats.rs:73`

use crate::advisor::AdvisorConfig;
use crate::dialect::{create_index_ddl, AdvisorDialect};
use crate::suggestion::{OptimizationSuggestion, SuggestionType};
use sz_orm_adaptive::stats::QueryStats;
use sz_orm_explain::ExplainPlan;

/// 规则 (a)：全表扫描 + 大结果集 → AddIndex
pub fn rule_add_index(
    plan: &ExplainPlan,
    config: &AdvisorConfig,
    dialect: AdvisorDialect,
) -> Option<OptimizationSuggestion> {
    if plan.missing_index(config.row_threshold) {
        let ddl = create_index_ddl(dialect, &plan.table, &["id"], None);
        Some(OptimizationSuggestion {
            suggestion_type: SuggestionType::AddIndex,
            target_query: format!("SELECT ... FROM {}", plan.table),
            description: format!(
                "表 '{}' 全表扫描 {} 行，超过阈值 {}，建议添加索引",
                plan.table, plan.rows, config.row_threshold
            ),
            action: ddl,
            confidence: 0.9,
            estimated_improvement: Some(format!(
                "预计将扫描行数从 {} 减少至 < {}",
                plan.rows, config.row_threshold
            )),
            conflict_note: None,
        })
    } else {
        None
    }
}

/// 规则 (b)：冗余索引检测 → DropIndex
pub fn rule_drop_index(plan: &ExplainPlan) -> Option<OptimizationSuggestion> {
    if plan.index.is_some() && plan.rows < 10 {
        let index_name = plan.index.clone().unwrap_or_default();
        Some(OptimizationSuggestion {
            suggestion_type: SuggestionType::DropIndex,
            target_query: format!("SELECT ... FROM {}", plan.table),
            description: format!(
                "表 '{}' 索引 '{}' 仅扫描 {} 行，可能冗余",
                plan.table, index_name, plan.rows
            ),
            action: format!("DROP INDEX {index_name}"),
            confidence: 0.7,
            estimated_improvement: Some("减少写入开销".into()),
            conflict_note: None,
        })
    } else {
        None
    }
}

/// 规则 (c)：大结果集 → UsePagination
pub fn rule_use_pagination(
    stats: &QueryStats,
    config: &AdvisorConfig,
) -> Option<OptimizationSuggestion> {
    if stats.should_paginate(config.row_threshold) {
        Some(OptimizationSuggestion {
            suggestion_type: SuggestionType::UsePagination,
            target_query: "query with large result set".into(),
            description: format!(
                "平均返回 {} 行，超过阈值 {}，建议改用游标分页",
                stats.avg_rows(),
                config.row_threshold
            ),
            action: "改用 cursor pagination (WHERE id > last_id LIMIT N)".into(),
            confidence: 0.8,
            estimated_improvement: Some("减少内存占用和响应时间".into()),
            conflict_note: None,
        })
    } else {
        None
    }
}

/// 规则 (d)：热点查询 → EnableCache
pub fn rule_enable_cache(stats: &QueryStats) -> Option<OptimizationSuggestion> {
    let threshold_ms = 30;
    let min_executions = 5;
    if stats.should_cache(threshold_ms, min_executions) {
        Some(OptimizationSuggestion {
            suggestion_type: SuggestionType::EnableCache,
            target_query: "frequent slow query".into(),
            description: format!(
                "平均耗时 {:.1}ms，执行 {} 次，建议启用缓存",
                stats.avg_time_ms(),
                stats.total_executions()
            ),
            action: "启用 L1/L2 缓存（TTL=300s）".into(),
            confidence: 0.8,
            estimated_improvement: Some(format!(
                "预计减少 {:.0}% 数据库负载",
                (1.0 - 1.0 / stats.total_executions().max(1) as f64) * 100.0
            )),
            conflict_note: None,
        })
    } else {
        None
    }
}

/// 规则 (e)：可改写查询检测 → RewriteQuery
pub fn rule_rewrite_query(plan: &ExplainPlan) -> Option<OptimizationSuggestion> {
    let has_filesort = plan
        .extra
        .iter()
        .any(|e| e.contains("filesort") || e.contains("sort"));
    let has_temporary = plan
        .extra
        .iter()
        .any(|e| e.contains("temporary") || e.contains("temp"));
    if has_filesort || has_temporary {
        let issues: Vec<&str> = plan
            .extra
            .iter()
            .filter(|e| {
                e.contains("filesort")
                    || e.contains("sort")
                    || e.contains("temporary")
                    || e.contains("temp")
            })
            .map(|s| s.as_str())
            .collect();
        Some(OptimizationSuggestion {
            suggestion_type: SuggestionType::RewriteQuery,
            target_query: format!("SELECT ... FROM {}", plan.table),
            description: format!("查询存在 {:?}，建议改写", issues),
            action: "改写查询消除 filesort/temporary table".into(),
            confidence: 0.6,
            estimated_improvement: Some("减少排序/临时表开销".into()),
            conflict_note: None,
        })
    } else {
        None
    }
}

/// 规则 (f)：池获取耗时高 → AdjustPoolSize
pub fn rule_adjust_pool_size(pool_pct: f64) -> Option<OptimizationSuggestion> {
    if pool_pct > 30.0 {
        Some(OptimizationSuggestion {
            suggestion_type: SuggestionType::AdjustPoolSize,
            target_query: "pool acquisition bottleneck".into(),
            description: format!("连接池获取耗时占总查询 {:.1}%，超过 30% 阈值", pool_pct),
            action: "增大连接池 max_connections 或启用预热".into(),
            confidence: 0.7,
            estimated_improvement: Some("减少池等待时间".into()),
            conflict_note: None,
        })
    } else {
        None
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

    fn config() -> AdvisorConfig {
        AdvisorConfig::default()
    }

    #[test]
    fn full_table_scan_triggers_add_index() {
        let p = plan(ScanType::FullTable, None, 10000);
        let s = rule_add_index(&p, &config(), AdvisorDialect::MySQL).unwrap();
        assert_eq!(s.suggestion_type, SuggestionType::AddIndex);
        assert!((s.confidence - 0.9).abs() < 1e-9);
        assert!(s.action.contains("CREATE INDEX"));
    }

    #[test]
    fn indexed_small_rows_triggers_drop_index() {
        let p = plan(ScanType::IndexLookup, Some("idx"), 5);
        let s = rule_drop_index(&p).unwrap();
        assert_eq!(s.suggestion_type, SuggestionType::DropIndex);
    }

    #[test]
    fn large_result_set_triggers_pagination() {
        let stats = QueryStats::new();
        for _ in 0..10 {
            stats.record(2000, 5_000);
        }
        let s = rule_use_pagination(&stats, &config()).unwrap();
        assert_eq!(s.suggestion_type, SuggestionType::UsePagination);
    }

    #[test]
    fn hot_query_triggers_cache() {
        let stats = QueryStats::new();
        for _ in 0..10 {
            stats.record(10, 50_000);
        }
        let s = rule_enable_cache(&stats).unwrap();
        assert_eq!(s.suggestion_type, SuggestionType::EnableCache);
    }

    #[test]
    fn filesort_triggers_rewrite() {
        let mut p = plan(ScanType::IndexRange, Some("idx"), 100);
        p.extra.push("Using filesort".into());
        let s = rule_rewrite_query(&p).unwrap();
        assert_eq!(s.suggestion_type, SuggestionType::RewriteQuery);
    }

    #[test]
    fn high_pool_pct_triggers_adjust() {
        let s = rule_adjust_pool_size(45.0).unwrap();
        assert_eq!(s.suggestion_type, SuggestionType::AdjustPoolSize);
    }

    #[test]
    fn low_pool_pct_no_suggestion() {
        assert!(rule_adjust_pool_size(10.0).is_none());
    }
}
