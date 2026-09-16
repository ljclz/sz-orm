//! JOIN 顺序优化（`query-auto-tuning` feature）
//!
//! 基于运行时表统计（行数 + 选择性）重排 JOIN 顺序，
//! 遵循"小表驱动大表"启发式，最小化中间结果集。

use std::collections::HashMap;

use crate::suggestion::{OptimizationSuggestion, SuggestionType};

/// 表统计信息
#[derive(Debug, Clone)]
pub struct TableStats {
    /// 表名
    pub table: String,
    /// 行数
    pub row_count: u64,
    /// 选择性（0.0~1.0，值越小过滤越强）
    pub selectivity: f64,
}

impl TableStats {
    /// 有效行数 = row_count * selectivity
    pub fn effective_rows(&self) -> f64 {
        self.row_count as f64 * self.selectivity
    }
}

/// JOIN 节点（表引用）
#[derive(Debug, Clone)]
pub struct JoinTable {
    /// 表名
    pub table: String,
    /// 别名
    pub alias: Option<String>,
    /// JOIN 条件
    pub on_clause: String,
}

/// JOIN 重排建议结果
#[derive(Debug, Clone)]
pub struct JoinReorderResult {
    /// 重排后的表顺序
    pub reordered_tables: Vec<String>,
    /// 重排后 SQL
    pub reordered_sql: String,
    /// 预估代价降低百分比
    pub estimated_improvement_pct: f64,
    /// 原始顺序
    pub original_order: Vec<String>,
}

/// JOIN 重排顾问
#[derive(Debug)]
pub struct JoinReorderAdvisor {
    /// 统计仓库
    stats: HashMap<String, TableStats>,
}

impl JoinReorderAdvisor {
    pub fn new() -> Self {
        Self {
            stats: HashMap::new(),
        }
    }

    /// 注册表统计
    pub fn register_stats(&mut self, stats: TableStats) {
        self.stats.insert(stats.table.clone(), stats);
    }

    /// 批量注册
    pub fn register_all(&mut self, stats: Vec<TableStats>) {
        for s in stats {
            self.register_stats(s);
        }
    }

    /// 获取表统计
    pub fn get_stats(&self, table: &str) -> Option<&TableStats> {
        self.stats.get(table)
    }

    /// 重排 JOIN 顺序
    ///
    /// 按 effective_rows 升序排列（小表驱动大表），
    /// 无统计的表按原始顺序排在最后。
    pub fn reorder(&self, tables: &[JoinTable]) -> JoinReorderResult {
        let original_order: Vec<String> = tables
            .iter()
            .map(|t| t.alias.clone().unwrap_or_else(|| t.table.clone()))
            .collect();

        let mut indexed: Vec<(usize, f64)> = tables
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let eff = self
                    .stats
                    .get(&t.table)
                    .map(|s| s.effective_rows())
                    .unwrap_or(f64::MAX);
                (i, eff)
            })
            .collect();
        indexed.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        let reordered_tables: Vec<String> = indexed
            .iter()
            .map(|(i, _)| {
                let t = &tables[*i];
                t.alias.clone().unwrap_or_else(|| t.table.clone())
            })
            .collect();

        let reordered_sql = build_join_sql(&indexed, tables);

        let original_cost = estimate_join_cost(
            &tables.iter().map(|t| t.table.as_str()).collect::<Vec<_>>(),
            &self.stats,
        );
        let reordered_cost = estimate_join_cost(
            &indexed
                .iter()
                .map(|(i, _)| tables[*i].table.as_str())
                .collect::<Vec<_>>(),
            &self.stats,
        );
        let estimated_improvement_pct = if original_cost > 0.0 {
            ((original_cost - reordered_cost) / original_cost * 100.0).max(0.0)
        } else {
            0.0
        };

        JoinReorderResult {
            reordered_tables,
            reordered_sql,
            estimated_improvement_pct,
            original_order,
        }
    }

    /// 生成 JOIN 重排优化建议
    pub fn advise(&self, query_key: &str, tables: &[JoinTable]) -> Option<OptimizationSuggestion> {
        let result = self.reorder(tables);
        if result.estimated_improvement_pct < 5.0 {
            return None;
        }
        let original = result.original_order.join(" JOIN ");
        let reordered = result.reordered_tables.join(" JOIN ");
        Some(OptimizationSuggestion::new(
            SuggestionType::RewriteQuery,
            query_key,
            format!(
                "JOIN reorder: {} → {} (estimated {:.1}% improvement)",
                original, reordered, result.estimated_improvement_pct
            ),
            result.reordered_sql,
            (result.estimated_improvement_pct / 100.0).min(0.95),
        ))
    }
}

impl Default for JoinReorderAdvisor {
    fn default() -> Self {
        Self::new()
    }
}

fn build_join_sql(order: &[(usize, f64)], tables: &[JoinTable]) -> String {
    let mut sql = String::new();
    for (idx, (i, _)) in order.iter().enumerate() {
        let t = &tables[*i];
        let name = t.alias.clone().unwrap_or_else(|| t.table.clone());
        if idx == 0 {
            sql.push_str(&format!("SELECT * FROM {}", name));
        } else {
            sql.push_str(&format!(" JOIN {} ON {}", name, t.on_clause));
        }
    }
    sql
}

fn estimate_join_cost(order: &[&str], stats: &HashMap<String, TableStats>) -> f64 {
    let n = order.len() as f64;
    let mut cost = 0.0;
    for (i, table) in order.iter().enumerate() {
        let eff = stats
            .get(*table)
            .map(|s| s.effective_rows())
            .unwrap_or(1000.0);
        let weight = n - i as f64;
        cost += eff.max(1.0) * weight;
    }
    cost
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(table: &str, rows: u64, sel: f64) -> TableStats {
        TableStats {
            table: table.into(),
            row_count: rows,
            selectivity: sel,
        }
    }

    fn join_table(table: &str, on: &str) -> JoinTable {
        JoinTable {
            table: table.into(),
            alias: None,
            on_clause: on.into(),
        }
    }

    #[test]
    fn reorder_small_drives_large() {
        let mut advisor = JoinReorderAdvisor::new();
        advisor.register_all(vec![
            stats("orders", 1_000_000, 1.0),
            stats("users", 100, 1.0),
        ]);
        let tables = vec![
            join_table("orders", "orders.user_id = users.id"),
            join_table("users", "users.id = orders.user_id"),
        ];
        let result = advisor.reorder(&tables);
        assert_eq!(result.reordered_tables, vec!["users", "orders"]);
        assert!(result.estimated_improvement_pct > 0.0);
    }

    #[test]
    fn reorder_with_selectivity() {
        let mut advisor = JoinReorderAdvisor::new();
        advisor.register_all(vec![
            stats("big_table", 1_000_000, 0.001),
            stats("small_table", 1000, 1.0),
        ]);
        let tables = vec![
            join_table("big_table", "big_table.id = small_table.id"),
            join_table("small_table", "small_table.id = big_table.id"),
        ];
        let result = advisor.reorder(&tables);
        assert_eq!(result.reordered_tables[0], "big_table");
    }

    #[test]
    fn no_reorder_when_already_optimal() {
        let mut advisor = JoinReorderAdvisor::new();
        advisor.register_all(vec![stats("a", 100, 1.0), stats("b", 10000, 1.0)]);
        let tables = vec![
            join_table("a", "a.id = b.id"),
            join_table("b", "b.id = a.id"),
        ];
        let result = advisor.reorder(&tables);
        assert_eq!(result.reordered_tables, vec!["a", "b"]);
    }

    #[test]
    fn advise_returns_none_for_minimal_improvement() {
        let mut advisor = JoinReorderAdvisor::new();
        advisor.register_all(vec![stats("a", 100, 1.0), stats("b", 101, 1.0)]);
        let tables = vec![
            join_table("a", "a.id = b.id"),
            join_table("b", "b.id = a.id"),
        ];
        let result = advisor.advise("q1", &tables);
        assert!(result.is_none());
    }

    #[test]
    fn advise_returns_suggestion_for_significant_improvement() {
        let mut advisor = JoinReorderAdvisor::new();
        advisor.register_all(vec![stats("huge", 10_000_000, 1.0), stats("tiny", 10, 1.0)]);
        let tables = vec![
            join_table("huge", "huge.id = tiny.id"),
            join_table("tiny", "tiny.id = huge.id"),
        ];
        let suggestion = advisor.advise("q1", &tables).expect("should suggest");
        assert_eq!(suggestion.suggestion_type, SuggestionType::RewriteQuery);
        assert!(suggestion.confidence > 0.0);
    }

    #[test]
    fn reorder_unknown_table_goes_last() {
        let mut advisor = JoinReorderAdvisor::new();
        advisor.register_stats(stats("known", 100, 1.0));
        let tables = vec![
            join_table("unknown", "unknown.id = known.id"),
            join_table("known", "known.id = unknown.id"),
        ];
        let result = advisor.reorder(&tables);
        assert_eq!(result.reordered_tables[0], "known");
        assert_eq!(result.reordered_tables[1], "unknown");
    }
}
