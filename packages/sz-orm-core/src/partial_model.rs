//! Partial Models — 部分字段选择与聚合查询（P-F-3, v2.1.0）
//!
//! 提供 `select_only()` 进入部分选择模式，支持 `.column(C)` / `.column_as(Expr, alias)`
//! / `.group_by(C)`，追平 SeaORM `select_only()` 性能优化能力。
//!
//! # 设计
//!
//! - `SelectMode` 枚举控制 `QueryBuilder` 的 SELECT 行为（All / Partial）
//! - `ColumnTrait` trait 提供类型化列引用，编译期拒绝字符串
//! - `Expr` 结构体表达聚合函数（COUNT / SUM / AVG / MAX / MIN）
//!
//! # 用法
//!
//! ```ignore
//! use sz_orm_core::partial_model::{select_only, Expr, AggFunc};
//!
//! // SELECT id, name FROM users
//! let sql = User::find()
//!     .select_only()
//!     .column("id")
//!     .column("name")
//!     .build_select();
//!
//! // SELECT COUNT(id) AS count FROM users
//! let sql = User::find()
//!     .select_only()
//!     .column_as(Expr::count("id"), "count")
//!     .build_select();
//! ```

/// 聚合函数类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggFunc {
    /// COUNT(col)
    Count,
    /// SUM(col)
    Sum,
    /// AVG(col)
    Avg,
    /// MAX(col)
    Max,
    /// MIN(col)
    Min,
}

impl AggFunc {
    /// 转换为 SQL 函数名
    pub fn as_sql(self) -> &'static str {
        match self {
            AggFunc::Count => "COUNT",
            AggFunc::Sum => "SUM",
            AggFunc::Avg => "AVG",
            AggFunc::Max => "MAX",
            AggFunc::Min => "MIN",
        }
    }
}

/// 聚合表达式
///
/// 表达 `FUNC(column)` 形式的聚合表达式，可通过 `column_as` 添加别名。
#[derive(Debug, Clone)]
pub struct Expr {
    func: AggFunc,
    column: String,
}

impl Expr {
    /// 创建 COUNT(col) 表达式
    pub fn count(column: impl Into<String>) -> Self {
        Self {
            func: AggFunc::Count,
            column: column.into(),
        }
    }

    /// 创建 SUM(col) 表达式
    pub fn sum(column: impl Into<String>) -> Self {
        Self {
            func: AggFunc::Sum,
            column: column.into(),
        }
    }

    /// 创建 AVG(col) 表达式
    pub fn avg(column: impl Into<String>) -> Self {
        Self {
            func: AggFunc::Avg,
            column: column.into(),
        }
    }

    /// 创建 MAX(col) 表达式
    pub fn max(column: impl Into<String>) -> Self {
        Self {
            func: AggFunc::Max,
            column: column.into(),
        }
    }

    /// 创建 MIN(col) 表达式
    pub fn min(column: impl Into<String>) -> Self {
        Self {
            func: AggFunc::Min,
            column: column.into(),
        }
    }

    /// 渲染为 SQL 片段（不含别名）
    pub fn render(&self) -> String {
        format!("{}({})", self.func.as_sql(), self.column)
    }

    /// 渲染为带别名的 SQL 片段
    pub fn render_as(&self, alias: &str) -> String {
        format!("{}({}) AS {}", self.func.as_sql(), self.column, alias)
    }
}

/// SELECT 模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectMode {
    /// SELECT *（默认）
    #[default]
    All,
    /// SELECT col1, col2, ...（部分选择）
    Partial,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agg_func_as_sql() {
        assert_eq!(AggFunc::Count.as_sql(), "COUNT");
        assert_eq!(AggFunc::Sum.as_sql(), "SUM");
        assert_eq!(AggFunc::Avg.as_sql(), "AVG");
        assert_eq!(AggFunc::Max.as_sql(), "MAX");
        assert_eq!(AggFunc::Min.as_sql(), "MIN");
    }

    #[test]
    fn test_expr_count() {
        let expr = Expr::count("id");
        assert_eq!(expr.render(), "COUNT(id)");
        assert_eq!(expr.render_as("total"), "COUNT(id) AS total");
    }

    #[test]
    fn test_expr_sum() {
        let expr = Expr::sum("amount");
        assert_eq!(expr.render(), "SUM(amount)");
        assert_eq!(
            expr.render_as("total_amount"),
            "SUM(amount) AS total_amount"
        );
    }

    #[test]
    fn test_expr_avg() {
        let expr = Expr::avg("score");
        assert_eq!(expr.render(), "AVG(score)");
    }

    #[test]
    fn test_expr_max() {
        let expr = Expr::max("price");
        assert_eq!(expr.render(), "MAX(price)");
    }

    #[test]
    fn test_expr_min() {
        let expr = Expr::min("price");
        assert_eq!(expr.render(), "MIN(price)");
    }

    #[test]
    fn test_select_mode_default() {
        assert_eq!(SelectMode::default(), SelectMode::All);
    }
}
