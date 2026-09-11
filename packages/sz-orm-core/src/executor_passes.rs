//! 执行器优化 Pass（v6.8.0 PERF-EXEC-01）
//!
//! 谓词下推 + 投影裁剪，复用 `sqlparser 0.47` 解析 AST → 优化 → 回写 SQL。

use sqlparser::ast::{
    BinaryOperator, Expr, Query, Select, SelectItem, SetExpr, Statement, TableFactor,
    TableWithJoins,
};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

/// 执行器优化 Pass 集合
pub struct ExecutorPasses;

/// 优化结果
#[derive(Debug, Clone)]
pub struct OptimizationResult {
    /// 优化后的 SQL
    pub optimized_sql: String,
    /// 是否执行了谓词下推
    pub predicate_pushed: bool,
    /// 是否执行了投影裁剪
    pub projection_pruned: bool,
    /// 优化描述
    pub description: String,
}

impl ExecutorPasses {
    /// 谓词下推：将外层 WHERE 条件下推到子查询内层
    ///
    /// 对于 `SELECT * FROM (SELECT * FROM t WHERE inner) sub WHERE outer`，
    /// 若 `outer` 仅引用 `sub` 的列，则合并到内层 WHERE。
    pub fn predicate_pushdown(sql: &str) -> Result<OptimizationResult, String> {
        let dialect = GenericDialect {};
        let statements =
            Parser::parse_sql(&dialect, sql).map_err(|e| format!("SQL 解析失败: {}", e))?;

        if statements.len() != 1 {
            return Ok(OptimizationResult {
                optimized_sql: sql.to_string(),
                predicate_pushed: false,
                projection_pruned: false,
                description: "多语句或空语句，跳过优化".to_string(),
            });
        }

        let mut stmt = statements.into_iter().next().unwrap();
        let mut pushed = false;

        if let Statement::Query(ref mut query) = stmt {
            if Self::try_pushdown_query(query) {
                pushed = true;
            }
        }

        let optimized_sql = stmt.to_string();

        Ok(OptimizationResult {
            optimized_sql,
            predicate_pushed: pushed,
            projection_pruned: false,
            description: if pushed {
                "谓词下推：外层 WHERE 条件已合并到子查询".to_string()
            } else {
                "无可下推谓词".to_string()
            },
        })
    }

    /// 投影裁剪：将 `SELECT *` 替换为指定列
    ///
    /// `required_fields` 为需要保留的列名列表。
    pub fn projection_pruning(
        sql: &str,
        required_fields: &[&str],
    ) -> Result<OptimizationResult, String> {
        let dialect = GenericDialect {};
        let statements =
            Parser::parse_sql(&dialect, sql).map_err(|e| format!("SQL 解析失败: {}", e))?;

        if statements.len() != 1 {
            return Ok(OptimizationResult {
                optimized_sql: sql.to_string(),
                predicate_pushed: false,
                projection_pruned: false,
                description: "多语句或空语句，跳过优化".to_string(),
            });
        }

        let mut stmt = statements.into_iter().next().unwrap();
        let mut pruned = false;

        if let Statement::Query(ref mut query) = stmt {
            if Self::try_projection_prune_query(query, required_fields) {
                pruned = true;
            }
        }

        let optimized_sql = stmt.to_string();

        Ok(OptimizationResult {
            optimized_sql,
            predicate_pushed: false,
            projection_pruned: pruned,
            description: if pruned {
                format!("投影裁剪：SELECT * → SELECT {}", required_fields.join(", "))
            } else {
                "无可裁剪投影".to_string()
            },
        })
    }

    /// 组合优化：先谓词下推，再投影裁剪
    pub fn optimize(
        sql: &str,
        required_fields: Option<&[&str]>,
    ) -> Result<OptimizationResult, String> {
        let pushdown_result = Self::predicate_pushdown(sql)?;
        let sql_after_pushdown = &pushdown_result.optimized_sql;

        if let Some(fields) = required_fields {
            let prune_result = Self::projection_pruning(sql_after_pushdown, fields)?;
            Ok(OptimizationResult {
                optimized_sql: prune_result.optimized_sql,
                predicate_pushed: pushdown_result.predicate_pushed,
                projection_pruned: prune_result.projection_pruned,
                description: format!(
                    "{}; {}",
                    pushdown_result.description, prune_result.description
                ),
            })
        } else {
            Ok(pushdown_result)
        }
    }

    fn try_pushdown_query(query: &mut Query) -> bool {
        let Query { body, .. } = query;
        if let SetExpr::Select(ref mut select) = **body {
            return Self::try_pushdown_select(select);
        }
        false
    }

    fn try_pushdown_select(select: &mut Select) -> bool {
        let Select {
            from, selection, ..
        } = select;

        if from.len() != 1 {
            return false;
        }

        let outer_selection = match selection {
            Some(expr) => expr.clone(),
            None => return false,
        };

        let table_with_joins = &mut from[0];
        let TableWithJoins { relation, .. } = table_with_joins;

        if let TableFactor::Derived { subquery, .. } = relation {
            if let SetExpr::Select(ref mut inner_select) = *subquery.body {
                if !Self::can_safely_pushdown(&outer_selection, inner_select) {
                    return false;
                }

                let inner_selection = &inner_select.selection;
                match inner_selection {
                    Some(inner_expr) => {
                        let combined = Expr::BinaryOp {
                            left: Box::new(inner_expr.clone()),
                            op: BinaryOperator::And,
                            right: Box::new(outer_selection.clone()),
                        };
                        inner_select.selection = Some(combined);
                    }
                    None => {
                        inner_select.selection = Some(outer_selection.clone());
                    }
                }
                *selection = None;
                return true;
            }
        }
        false
    }

    /// 检查外层条件是否可安全下推到子查询。
    ///
    /// 判定逻辑：提取外层条件中的所有列引用标识符，
    /// 若子查询投影为 `SELECT *` 则允许下推；
    /// 否则检查所有引用列是否存在于子查询投影列中。
    fn can_safely_pushdown(outer_expr: &Expr, inner_select: &Select) -> bool {
        let referenced_cols = Self::extract_column_references(outer_expr);
        if referenced_cols.is_empty() {
            return true;
        }

        let has_wildcard = inner_select
            .projection
            .iter()
            .any(|item| matches!(item, SelectItem::Wildcard(_)));
        if has_wildcard {
            return true;
        }

        let available_cols: std::collections::HashSet<String> = inner_select
            .projection
            .iter()
            .filter_map(|item| match item {
                SelectItem::UnnamedExpr(Expr::Identifier(ident)) => Some(ident.value.clone()),
                SelectItem::ExprWithAlias { expr, .. } => {
                    if let Expr::Identifier(ident) = expr {
                        Some(ident.value.clone())
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect();

        referenced_cols
            .iter()
            .all(|col| available_cols.contains(col))
    }

    /// 递归提取表达式中所有列引用标识符。
    fn extract_column_references(expr: &Expr) -> Vec<String> {
        let mut cols = Vec::new();
        Self::collect_cols(expr, &mut cols);
        cols
    }

    fn collect_cols(expr: &Expr, cols: &mut Vec<String>) {
        match expr {
            Expr::Identifier(ident) => {
                cols.push(ident.value.clone());
            }
            Expr::CompoundIdentifier(idents) => {
                if let Some(last) = idents.last() {
                    cols.push(last.value.clone());
                }
            }
            Expr::BinaryOp { left, right, .. } => {
                Self::collect_cols(left, cols);
                Self::collect_cols(right, cols);
            }
            Expr::UnaryOp { expr, .. } => Self::collect_cols(expr, cols),
            Expr::Nested(expr) => Self::collect_cols(expr, cols),
            Expr::Function(func) => {
                if let sqlparser::ast::FunctionArguments::List(list) = &func.args {
                    for arg in &list.args {
                        match arg {
                            sqlparser::ast::FunctionArg::Unnamed(
                                sqlparser::ast::FunctionArgExpr::Expr(e),
                            ) => {
                                Self::collect_cols(e, cols);
                            }
                            sqlparser::ast::FunctionArg::Named {
                                arg: sqlparser::ast::FunctionArgExpr::Expr(e),
                                ..
                            } => {
                                Self::collect_cols(e, cols);
                            }
                            _ => {}
                        }
                    }
                }
            }
            Expr::InList { expr, list, .. } => {
                Self::collect_cols(expr, cols);
                for item in list {
                    Self::collect_cols(item, cols);
                }
            }
            Expr::Between {
                expr, low, high, ..
            } => {
                Self::collect_cols(expr, cols);
                Self::collect_cols(low, cols);
                Self::collect_cols(high, cols);
            }
            _ => {}
        }
    }

    fn try_projection_prune_query(query: &mut Query, required_fields: &[&str]) -> bool {
        let Query { body, .. } = query;
        if let SetExpr::Select(ref mut select) = **body {
            return Self::try_projection_prune_select(select, required_fields);
        }
        false
    }

    fn try_projection_prune_select(select: &mut Select, required_fields: &[&str]) -> bool {
        let Select { projection, .. } = select;

        let has_wildcard = projection
            .iter()
            .any(|item| matches!(item, SelectItem::Wildcard(_)));

        if !has_wildcard || required_fields.is_empty() {
            return false;
        }

        let new_projection: Vec<SelectItem> = required_fields
            .iter()
            .map(|field| {
                SelectItem::UnnamedExpr(Expr::Identifier(sqlparser::ast::Ident::new(*field)))
            })
            .collect();

        *projection = new_projection;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predicate_pushdown_merges_outer_where_into_subquery() {
        let sql =
            "SELECT * FROM (SELECT * FROM users WHERE age > 18) AS sub WHERE sub.status = 'active'";
        let result = ExecutorPasses::predicate_pushdown(sql).unwrap();
        assert!(result.predicate_pushed);
        assert!(result.optimized_sql.contains("age > 18"));
        assert!(result.optimized_sql.contains("status = 'active'"));
    }

    #[test]
    fn predicate_pushdown_no_subquery_returns_unchanged() {
        let sql = "SELECT * FROM users WHERE age > 18";
        let result = ExecutorPasses::predicate_pushdown(sql).unwrap();
        assert!(!result.predicate_pushed);
    }

    #[test]
    fn predicate_pushdown_no_outer_where_returns_unchanged() {
        let sql = "SELECT * FROM (SELECT * FROM users WHERE age > 18) AS sub";
        let result = ExecutorPasses::predicate_pushdown(sql).unwrap();
        assert!(!result.predicate_pushed);
    }

    #[test]
    fn predicate_pushdown_subquery_without_where_adds_where() {
        let sql = "SELECT * FROM (SELECT * FROM users) AS sub WHERE sub.age > 18";
        let result = ExecutorPasses::predicate_pushdown(sql).unwrap();
        assert!(result.predicate_pushed);
        assert!(result.optimized_sql.contains("age > 18"));
    }

    #[test]
    fn projection_pruning_replaces_wildcard_with_columns() {
        let sql = "SELECT * FROM users";
        let result = ExecutorPasses::projection_pruning(sql, &["id", "name", "age"]).unwrap();
        assert!(result.projection_pruned);
        assert!(result.optimized_sql.contains("id"));
        assert!(result.optimized_sql.contains("name"));
        assert!(result.optimized_sql.contains("age"));
        assert!(!result.optimized_sql.contains("*"));
    }

    #[test]
    fn projection_pruning_no_wildcard_returns_unchanged() {
        let sql = "SELECT id, name FROM users";
        let result = ExecutorPasses::projection_pruning(sql, &["id"]).unwrap();
        assert!(!result.projection_pruned);
    }

    #[test]
    fn projection_pruning_empty_fields_returns_unchanged() {
        let sql = "SELECT * FROM users";
        let result = ExecutorPasses::projection_pruning(sql, &[]).unwrap();
        assert!(!result.projection_pruned);
    }

    #[test]
    fn optimize_combines_pushdown_and_pruning() {
        let sql =
            "SELECT * FROM (SELECT * FROM users WHERE age > 18) AS sub WHERE sub.status = 'active'";
        let result = ExecutorPasses::optimize(sql, Some(&["id", "name"])).unwrap();
        assert!(result.predicate_pushed);
        assert!(result.projection_pruned);
        assert!(result.optimized_sql.contains("id"));
        assert!(result.optimized_sql.contains("name"));
    }

    #[test]
    fn optimize_pushdown_only() {
        let sql =
            "SELECT * FROM (SELECT * FROM users WHERE age > 18) AS sub WHERE sub.status = 'active'";
        let result = ExecutorPasses::optimize(sql, None).unwrap();
        assert!(result.predicate_pushed);
        assert!(!result.projection_pruned);
    }

    #[test]
    fn invalid_sql_returns_error() {
        let sql = "SELECT FROM WHERE";
        let result = ExecutorPasses::predicate_pushdown(sql);
        assert!(result.is_err());
    }

    #[test]
    fn projection_pruning_preserves_where_clause() {
        let sql = "SELECT * FROM users WHERE age > 18 AND status = 'active'";
        let result = ExecutorPasses::projection_pruning(sql, &["id", "name"]).unwrap();
        assert!(result.projection_pruned);
        assert!(result.optimized_sql.contains("age > 18"));
        assert!(result.optimized_sql.contains("status = 'active'"));
    }
}
