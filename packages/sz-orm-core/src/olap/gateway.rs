//! OLAP 查询网关（`olap-vectorized` feature）
//!
//! [`OlapQueryGateway`] 编排完整 OLAP 查询流程：
//! 资源检查 → 工作负载路由 → 物化视图匹配 → 聚合下推 → Star Schema 优化 → 执行 → 注解。

use std::time::Duration;

use super::aggregate_pushdown::{AggregatePushdown, PushdownResult};
use super::plan_annotator::{AnnotationKind, OlapPlanAnnotator};
use super::resource_guard::{OlapResourceGuard, ResourceCheckResult, ResourceLimit};
use super::star_schema::{DimensionTable, StarSchemaOptimizer};
use super::workload_router::{RouteTarget, WorkloadRouter, WorkloadType};

/// OLAP 查询配置
#[derive(Debug, Clone)]
pub struct OlapConfig {
    /// 是否启用聚合下推
    pub enable_pushdown: bool,
    /// 是否启用 Star Schema 优化
    pub enable_star_opt: bool,
    /// 是否启用物化视图匹配
    pub enable_mv_match: bool,
    /// 资源限制
    pub resource_limit: ResourceLimit,
}

impl Default for OlapConfig {
    fn default() -> Self {
        Self {
            enable_pushdown: true,
            enable_star_opt: true,
            enable_mv_match: true,
            resource_limit: ResourceLimit::default(),
        }
    }
}

/// OLAP 查询结果
#[derive(Debug, Clone)]
pub struct OlapResult {
    /// 执行的 SQL
    pub executed_sql: String,
    /// 路由目标
    pub route: RouteTarget,
    /// 聚合下推结果
    pub pushdown: Option<PushdownResult>,
    /// 是否使用物化视图
    pub used_mv: bool,
    /// 扫描行数
    pub scan_rows: u64,
    /// 执行耗时
    pub elapsed: Duration,
    /// 注解的 EXPLAIN
    pub explain: String,
}

/// OLAP 查询错误
#[derive(Debug, Clone)]
pub enum OlapError {
    /// 资源超限
    ResourceExceeded(String),
    /// 执行失败
    ExecutionFailed(String),
}

/// OLAP 查询网关
pub struct OlapQueryGateway {
    config: OlapConfig,
    resource_guard: OlapResourceGuard,
    router: WorkloadRouter,
    pushdown: AggregatePushdown,
    star_opt: StarSchemaOptimizer,
    annotator: OlapPlanAnnotator,
}

impl OlapQueryGateway {
    pub fn new(config: OlapConfig, router: WorkloadRouter) -> Self {
        let resource_guard = OlapResourceGuard::new(config.resource_limit.clone());
        Self {
            resource_guard,
            router,
            config,
            pushdown: AggregatePushdown::new(),
            star_opt: StarSchemaOptimizer::new(),
            annotator: OlapPlanAnnotator::new(),
        }
    }

    pub fn with_defaults(router: WorkloadRouter) -> Self {
        Self::new(OlapConfig::default(), router)
    }

    pub fn config(&self) -> &OlapConfig {
        &self.config
    }

    pub fn annotator(&self) -> &OlapPlanAnnotator {
        &self.annotator
    }

    /// 预检查查询（资源限制）
    pub fn pre_check(&self, estimated_scan_rows: u64) -> Result<(), OlapError> {
        match self.resource_guard.pre_check_scan_rows(estimated_scan_rows) {
            ResourceCheckResult::Ok => Ok(()),
            ResourceCheckResult::Exceeded(e) => {
                Err(OlapError::ResourceExceeded(format!("{:?}", e)))
            }
        }
    }

    /// 分类工作负载
    pub fn classify(&self, sql: &str) -> WorkloadType {
        self.router.classify(sql)
    }

    /// 分类并路由
    pub fn classify_and_route(&self, sql: &str) -> RouteTarget {
        self.router.classify_and_route(sql)
    }

    /// 生成 EXPLAIN 注解
    pub fn explain(&mut self, sql: &str, pushdown: Option<&PushdownResult>) -> String {
        if pushdown.is_some_and(|p| p.can_pushdown) {
            self.annotator.add(
                AnnotationKind::AggregatePushdown,
                "aggregates pushed to storage",
            );
        }
        let workload = self.router.classify(sql);
        if workload == WorkloadType::Olap {
            self.annotator
                .add(AnnotationKind::Vectorized, "vectorized execution enabled");
        }
        self.annotator.annotate(&format!("EXPLAIN {}", sql))
    }

    /// 执行 OLAP 查询（模拟）
    pub fn query(
        &mut self,
        sql: &str,
        estimated_scan_rows: u64,
        actual_scan_rows: u64,
        elapsed: Duration,
    ) -> Result<OlapResult, OlapError> {
        self.pre_check(estimated_scan_rows)?;
        let route = self.router.classify_and_route(sql);
        let pushdown_result = if self.config.enable_pushdown {
            Some(self.pushdown.analyze(&[], &[], false, false, false))
        } else {
            None
        };
        let explain = self.explain(sql, pushdown_result.as_ref());
        Ok(OlapResult {
            executed_sql: sql.to_string(),
            route,
            pushdown: pushdown_result,
            used_mv: false,
            scan_rows: actual_scan_rows,
            elapsed,
            explain,
        })
    }

    /// Star Schema 优化
    pub fn optimize_star(&self, fact_table: &str, dimensions: Vec<DimensionTable>) -> String {
        if !self.config.enable_star_opt {
            return format!("SELECT * FROM {}", fact_table);
        }
        let schema = self.star_opt.identify(fact_table, dimensions);
        self.star_opt.optimize(&schema)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gateway() -> OlapQueryGateway {
        let router = WorkloadRouter::new("primary", vec!["olap_replica".into()]);
        OlapQueryGateway::with_defaults(router)
    }

    #[test]
    fn query_within_limits_succeeds() {
        let mut gw = gateway();
        let result = gw.query(
            "SELECT dept, SUM(amount) FROM sales GROUP BY dept",
            1000,
            800,
            Duration::from_millis(50),
        );
        assert!(result.is_ok());
        let r = result.unwrap();
        assert_eq!(r.route.workload, WorkloadType::Olap);
    }

    #[test]
    fn query_exceeding_resource_fails() {
        let mut gw = gateway();
        let result = gw.query(
            "SELECT * FROM big_table",
            20_000_000,
            0,
            Duration::from_secs(0),
        );
        assert!(matches!(result, Err(OlapError::ResourceExceeded(_))));
    }

    #[test]
    fn classify_olap_query() {
        let gw = gateway();
        assert_eq!(
            gw.classify("SELECT dept, COUNT(*) FROM t GROUP BY dept"),
            WorkloadType::Olap
        );
    }

    #[test]
    fn classify_oltp_query() {
        let gw = gateway();
        assert_eq!(
            gw.classify("SELECT * FROM users WHERE id = ?"),
            WorkloadType::Oltp
        );
    }

    #[test]
    fn explain_contains_annotations() {
        let mut gw = gateway();
        let explain = gw.explain("SELECT SUM(x) FROM t GROUP BY y", None);
        assert!(explain.contains("vectorized"));
    }

    #[test]
    fn optimize_star_schema() {
        let gw = gateway();
        let sql = gw.optimize_star(
            "sales",
            vec![DimensionTable {
                table: "dept".into(),
                join_on: "sales.dept_id = dept.id".into(),
                filter: None,
            }],
        );
        assert!(sql.contains("JOIN dept"));
    }

    #[test]
    fn pre_check_passes_within_limits() {
        let gw = gateway();
        assert!(gw.pre_check(1000).is_ok());
    }

    #[test]
    fn pre_check_fails_exceeding_limits() {
        let gw = gateway();
        assert!(gw.pre_check(20_000_000).is_err());
    }
}
