//! OLAP/OLTP 工作负载路由（`olap-vectorized` feature）
//!
//! 根据 SQL 特征分类工作负载类型，路由到对应副本：
//! OLAP → 读副本（列存优化），OLTP → 主库（低延迟）。

/// 工作负载类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkloadType {
    /// OLAP（分析查询）
    Olap,
    /// OLTP（事务查询）
    Oltp,
    /// 混合
    Mixed,
}

impl WorkloadType {
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkloadType::Olap => "OLAP",
            WorkloadType::Oltp => "OLTP",
            WorkloadType::Mixed => "Mixed",
        }
    }
}

/// 路由目标
#[derive(Debug, Clone)]
pub struct RouteTarget {
    /// 目标副本标识
    pub replica: String,
    /// 工作负载类型
    pub workload: WorkloadType,
    /// 是否降级（OLAP 副本不可用时降级到主库）
    pub degraded: bool,
}

/// 工作负载路由器
#[derive(Debug)]
pub struct WorkloadRouter {
    /// OLAP 副本列表
    olap_replicas: Vec<String>,
    /// OLTP 主库
    oltp_primary: String,
    /// OLAP 副本可用性
    olap_available: bool,
}

impl WorkloadRouter {
    pub fn new(oltp_primary: impl Into<String>, olap_replicas: Vec<String>) -> Self {
        Self {
            olap_replicas,
            oltp_primary: oltp_primary.into(),
            olap_available: true,
        }
    }

    pub fn set_olap_available(&mut self, available: bool) {
        self.olap_available = available;
    }

    /// 分类工作负载类型
    ///
    /// OLAP 判定条件：包含 GROUP BY / 聚合函数 / 大表扫描
    pub fn classify(&self, sql: &str) -> WorkloadType {
        let upper = sql.to_uppercase();
        let has_group_by = upper.contains("GROUP BY");
        let has_aggregate = ["SUM(", "COUNT(", "AVG(", "MIN(", "MAX("]
            .iter()
            .any(|agg| upper.contains(agg));
        let has_window = upper.contains("OVER(") || upper.contains("OVER (");
        if has_group_by || has_aggregate || has_window {
            WorkloadType::Olap
        } else if upper.starts_with("SELECT") {
            WorkloadType::Oltp
        } else {
            WorkloadType::Mixed
        }
    }

    /// 路由到目标副本
    pub fn route(&self, workload: WorkloadType) -> RouteTarget {
        match workload {
            WorkloadType::Olap => {
                if self.olap_available && !self.olap_replicas.is_empty() {
                    RouteTarget {
                        replica: self.olap_replicas[0].clone(),
                        workload: WorkloadType::Olap,
                        degraded: false,
                    }
                } else {
                    RouteTarget {
                        replica: self.oltp_primary.clone(),
                        workload: WorkloadType::Olap,
                        degraded: true,
                    }
                }
            }
            WorkloadType::Oltp => RouteTarget {
                replica: self.oltp_primary.clone(),
                workload: WorkloadType::Oltp,
                degraded: false,
            },
            WorkloadType::Mixed => RouteTarget {
                replica: self.oltp_primary.clone(),
                workload: WorkloadType::Mixed,
                degraded: false,
            },
        }
    }

    /// 分类并路由
    pub fn classify_and_route(&self, sql: &str) -> RouteTarget {
        let workload = self.classify(sql);
        self.route(workload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn router() -> WorkloadRouter {
        WorkloadRouter::new(
            "primary",
            vec!["olap_replica_1".into(), "olap_replica_2".into()],
        )
    }

    #[test]
    fn classify_olap_with_group_by() {
        let r = router();
        assert_eq!(
            r.classify("SELECT dept, SUM(amount) FROM sales GROUP BY dept"),
            WorkloadType::Olap
        );
    }

    #[test]
    fn classify_oltp_simple_select() {
        let r = router();
        assert_eq!(
            r.classify("SELECT * FROM users WHERE id = ?"),
            WorkloadType::Oltp
        );
    }

    #[test]
    fn route_olap_to_replica() {
        let r = router();
        let target = r.route(WorkloadType::Olap);
        assert_eq!(target.replica, "olap_replica_1");
        assert!(!target.degraded);
    }

    #[test]
    fn route_olap_degrades_when_unavailable() {
        let mut r = router();
        r.set_olap_available(false);
        let target = r.route(WorkloadType::Olap);
        assert_eq!(target.replica, "primary");
        assert!(target.degraded);
    }
}
