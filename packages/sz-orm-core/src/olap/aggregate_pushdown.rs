//! OLAP 聚合下推（`olap-vectorized` feature）
//!
//! 将聚合函数（SUM/COUNT/AVG/MIN/MAX）下推到存储层执行，
//! 减少中间结果集传输量。

use std::collections::HashSet;

/// 聚合函数类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AggregateFunc {
    /// SUM
    Sum,
    /// COUNT
    Count,
    /// AVG
    Avg,
    /// MIN
    Min,
    /// MAX
    Max,
}

impl AggregateFunc {
    fn as_str(&self) -> &'static str {
        match self {
            AggregateFunc::Sum => "SUM",
            AggregateFunc::Count => "COUNT",
            AggregateFunc::Avg => "AVG",
            AggregateFunc::Min => "MIN",
            AggregateFunc::Max => "MAX",
        }
    }
}

/// 聚合列定义
#[derive(Debug, Clone)]
pub struct AggregateColumn {
    /// 聚合函数
    pub func: AggregateFunc,
    /// 目标列名
    pub column: String,
    /// 输出别名
    pub alias: String,
}

/// 下推分析结果
#[derive(Debug, Clone)]
pub struct PushdownResult {
    /// 是否可下推
    pub can_pushdown: bool,
    /// 下推的聚合列
    pub aggregates: Vec<AggregateColumn>,
    /// GROUP BY 列
    pub group_by_columns: Vec<String>,
    /// 不可下推原因
    pub reason: Option<String>,
}

/// 聚合下推分析器
#[derive(Debug)]
pub struct AggregatePushdown {
    /// 支持下推的函数集合
    supported_funcs: HashSet<AggregateFunc>,
}

impl AggregatePushdown {
    pub fn new() -> Self {
        Self {
            supported_funcs: [
                AggregateFunc::Sum,
                AggregateFunc::Count,
                AggregateFunc::Min,
                AggregateFunc::Max,
            ]
            .into_iter()
            .collect(),
        }
    }

    /// 分析聚合是否可下推
    ///
    /// 不可下推条件：
    /// - 包含 AVG（需 SUM/COUNT 两步，部分存储不支持）
    /// - 包含 DISTINCT
    /// - 包含 HAVING（需先聚合再过滤）
    /// - 包含子查询
    pub fn analyze(
        &self,
        aggregates: &[AggregateColumn],
        group_by: &[String],
        has_distinct: bool,
        has_having: bool,
        has_subquery: bool,
    ) -> PushdownResult {
        if has_subquery {
            return PushdownResult {
                can_pushdown: false,
                aggregates: aggregates.to_vec(),
                group_by_columns: group_by.to_vec(),
                reason: Some("subquery prevents pushdown".into()),
            };
        }
        if has_distinct {
            return PushdownResult {
                can_pushdown: false,
                aggregates: aggregates.to_vec(),
                group_by_columns: group_by.to_vec(),
                reason: Some("DISTINCT prevents pushdown".into()),
            };
        }
        if has_having {
            return PushdownResult {
                can_pushdown: false,
                aggregates: aggregates.to_vec(),
                group_by_columns: group_by.to_vec(),
                reason: Some("HAVING prevents pushdown".into()),
            };
        }
        for agg in aggregates {
            if !self.supported_funcs.contains(&agg.func) {
                return PushdownResult {
                    can_pushdown: false,
                    aggregates: aggregates.to_vec(),
                    group_by_columns: group_by.to_vec(),
                    reason: Some(format!("{} not supported for pushdown", agg.func.as_str())),
                };
            }
        }
        PushdownResult {
            can_pushdown: true,
            aggregates: aggregates.to_vec(),
            group_by_columns: group_by.to_vec(),
            reason: None,
        }
    }

    /// 生成下推 SQL
    pub fn generate_pushdown_sql(&self, table: &str, result: &PushdownResult) -> Option<String> {
        if !result.can_pushdown {
            return None;
        }
        let agg_parts: Vec<String> = result
            .aggregates
            .iter()
            .map(|a| format!("{}({}) AS {}", a.func.as_str(), a.column, a.alias))
            .collect();
        let mut sql = format!("SELECT {} FROM {}", agg_parts.join(", "), table);
        if !result.group_by_columns.is_empty() {
            sql.push_str(&format!(" GROUP BY {}", result.group_by_columns.join(", ")));
        }
        Some(sql)
    }
}

impl Default for AggregatePushdown {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_sum_can_pushdown() {
        let ap = AggregatePushdown::new();
        let result = ap.analyze(
            &[AggregateColumn {
                func: AggregateFunc::Sum,
                column: "amount".into(),
                alias: "total".into(),
            }],
            &["dept".into()],
            false,
            false,
            false,
        );
        assert!(result.can_pushdown);
    }

    #[test]
    fn avg_cannot_pushdown() {
        let ap = AggregatePushdown::new();
        let result = ap.analyze(
            &[AggregateColumn {
                func: AggregateFunc::Avg,
                column: "amount".into(),
                alias: "avg_amount".into(),
            }],
            &[],
            false,
            false,
            false,
        );
        assert!(!result.can_pushdown);
        assert!(result.reason.as_ref().unwrap().contains("AVG"));
    }

    #[test]
    fn distinct_prevents_pushdown() {
        let ap = AggregatePushdown::new();
        let result = ap.analyze(&[], &[], true, false, false);
        assert!(!result.can_pushdown);
        assert!(result.reason.as_ref().unwrap().contains("DISTINCT"));
    }

    #[test]
    fn having_prevents_pushdown() {
        let ap = AggregatePushdown::new();
        let result = ap.analyze(&[], &[], false, true, false);
        assert!(!result.can_pushdown);
    }

    #[test]
    fn generate_pushdown_sql() {
        let ap = AggregatePushdown::new();
        let result = ap.analyze(
            &[
                AggregateColumn {
                    func: AggregateFunc::Sum,
                    column: "amount".into(),
                    alias: "total".into(),
                },
                AggregateColumn {
                    func: AggregateFunc::Count,
                    column: "*".into(),
                    alias: "cnt".into(),
                },
            ],
            &["dept".into()],
            false,
            false,
            false,
        );
        let sql = ap.generate_pushdown_sql("sales", &result).unwrap();
        assert!(sql.contains("SUM(amount) AS total"));
        assert!(sql.contains("COUNT(*) AS cnt"));
        assert!(sql.contains("GROUP BY dept"));
    }

    #[test]
    fn no_pushdown_sql_when_not_pushable() {
        let ap = AggregatePushdown::new();
        let result = ap.analyze(&[], &[], true, false, false);
        assert!(ap.generate_pushdown_sql("sales", &result).is_none());
    }
}
