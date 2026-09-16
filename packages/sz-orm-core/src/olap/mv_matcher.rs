//! 物化视图匹配（`olap-vectorized` feature）
//!
//! 自动匹配查询到物化视图，加速聚合查询。
//! MV 过期时回退到原始表扫描。

use std::time::{Duration, Instant};

/// 物化视图定义
#[derive(Debug, Clone)]
pub struct MaterializedView {
    /// 视图名称
    pub name: String,
    /// 视图 SQL（定义查询）
    pub sql: String,
    /// 基表名
    pub base_table: String,
    /// 聚合列
    pub aggregate_columns: Vec<String>,
    /// GROUP BY 列
    pub group_by_columns: Vec<String>,
    /// 创建时间
    pub created_at: Instant,
}

/// 匹配结果
#[derive(Debug, Clone)]
pub struct MatchResult {
    /// 匹配的物化视图
    pub view: Option<MaterializedView>,
    /// 是否因过期回退
    pub fallback_reason: Option<String>,
}

/// 物化视图匹配器
#[derive(Debug)]
pub struct MaterializedViewMatcher {
    /// 已注册的物化视图
    views: Vec<MaterializedView>,
    /// MV 有效期（超过则视为过期）
    ttl: Duration,
}

impl MaterializedViewMatcher {
    pub fn new(ttl: Duration) -> Self {
        Self {
            views: Vec::new(),
            ttl,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(Duration::from_secs(3600))
    }

    /// 注册物化视图
    pub fn register(&mut self, view: MaterializedView) {
        self.views.push(view);
    }

    /// 匹配查询到物化视图
    ///
    /// 匹配条件：基表相同 + GROUP BY 列相同 + 聚合列是 MV 聚合列的子集
    pub fn match_view(
        &self,
        base_table: &str,
        group_by: &[String],
        aggregates: &[String],
    ) -> MatchResult {
        for view in &self.views {
            if view.base_table != base_table {
                continue;
            }
            if view.group_by_columns != group_by {
                continue;
            }
            let all_covered = aggregates
                .iter()
                .all(|a| view.aggregate_columns.contains(a));
            if !all_covered {
                continue;
            }
            if self.is_expired(view) {
                return MatchResult {
                    view: None,
                    fallback_reason: Some(format!(
                        "MV {} expired, fallback to base table",
                        view.name
                    )),
                };
            }
            return MatchResult {
                view: Some(view.clone()),
                fallback_reason: None,
            };
        }
        MatchResult {
            view: None,
            fallback_reason: Some("no matching materialized view".into()),
        }
    }

    /// 检查 MV 是否过期
    pub fn is_expired(&self, view: &MaterializedView) -> bool {
        view.created_at.elapsed() > self.ttl
    }

    /// 已注册 MV 数量
    pub fn len(&self) -> usize {
        self.views.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.views.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mv(name: &str, base: &str, agg: Vec<&str>, group: Vec<&str>) -> MaterializedView {
        MaterializedView {
            name: name.into(),
            sql: format!(
                "SELECT {} FROM {} GROUP BY {}",
                agg.join(","),
                base,
                group.join(",")
            ),
            base_table: base.into(),
            aggregate_columns: agg.into_iter().map(String::from).collect(),
            group_by_columns: group.into_iter().map(String::from).collect(),
            created_at: Instant::now(),
        }
    }

    #[test]
    fn match_exact_view() {
        let mut m = MaterializedViewMatcher::with_defaults();
        m.register(mv(
            "mv_sales_dept",
            "sales",
            vec!["SUM(amount)"],
            vec!["dept"],
        ));
        let result = m.match_view("sales", &["dept".into()], &["SUM(amount)".into()]);
        assert!(result.view.is_some());
        assert_eq!(result.view.unwrap().name, "mv_sales_dept");
    }

    #[test]
    fn no_match_different_group_by() {
        let mut m = MaterializedViewMatcher::with_defaults();
        m.register(mv("mv1", "sales", vec!["SUM(amount)"], vec!["dept"]));
        let result = m.match_view("sales", &["region".into()], &["SUM(amount)".into()]);
        assert!(result.view.is_none());
    }

    #[test]
    fn no_match_different_base_table() {
        let mut m = MaterializedViewMatcher::with_defaults();
        m.register(mv("mv1", "orders", vec!["SUM(qty)"], vec!["dept"]));
        let result = m.match_view("sales", &["dept".into()], &["SUM(qty)".into()]);
        assert!(result.view.is_none());
    }

    #[test]
    fn expired_mv_fallback() {
        let mut m = MaterializedViewMatcher::new(Duration::from_millis(0));
        m.register(mv("mv1", "sales", vec!["SUM(amount)"], vec!["dept"]));
        std::thread::sleep(Duration::from_millis(1));
        let result = m.match_view("sales", &["dept".into()], &["SUM(amount)".into()]);
        assert!(result.view.is_none());
        assert!(result.fallback_reason.as_ref().unwrap().contains("expired"));
    }

    #[test]
    fn aggregate_subset_matches() {
        let mut m = MaterializedViewMatcher::with_defaults();
        m.register(mv(
            "mv1",
            "sales",
            vec!["SUM(amount)", "COUNT(*)"],
            vec!["dept"],
        ));
        let result = m.match_view("sales", &["dept".into()], &["SUM(amount)".into()]);
        assert!(result.view.is_some());
    }
}
