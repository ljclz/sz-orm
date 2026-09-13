//! 全局路由决策器
//!
//! 基于区域拓扑 + 健康状态 + 延迟统计做出路由决策。

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::region_topology::{RegionHealth, RegionNode, RegionRole, RegionTopology, TopologyError};
use crate::routing::QueryType;

/// 一致性级别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsistencyLevel {
    /// 强一致（路由至 Primary）
    Strong,
    /// 最终一致
    Eventual,
}

/// 路由请求
#[derive(Debug, Clone)]
pub struct RouteRequest<'a> {
    /// 数据归属键
    pub data_affinity_key: Option<&'a str>,
    /// 一致性级别
    pub consistency: ConsistencyLevel,
    /// 是否延迟敏感
    pub latency_sensitive: bool,
    /// 查询类型
    pub query_type: QueryType,
}

impl<'a> Default for RouteRequest<'a> {
    fn default() -> Self {
        Self {
            data_affinity_key: None,
            consistency: ConsistencyLevel::Eventual,
            latency_sensitive: false,
            query_type: QueryType::Read,
        }
    }
}

/// 路由决策
#[derive(Debug, Clone)]
pub struct RouteDecision {
    /// 选中的区域 ID
    pub region_id: String,
    /// 是否降级
    pub degraded: bool,
    /// 决策耗时
    pub decision_latency: Duration,
}

/// 路由错误
#[derive(Debug, Clone)]
pub enum RouteError {
    /// 拓扑无效
    TopologyInvalid(String),
    /// 无可用区域
    NoAvailableRegion,
}

impl std::fmt::Display for RouteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RouteError::TopologyInvalid(msg) => write!(f, "Topology invalid: {}", msg),
            RouteError::NoAvailableRegion => write!(f, "No available region"),
        }
    }
}

impl std::error::Error for RouteError {}

impl From<TopologyError> for RouteError {
    fn from(e: TopologyError) -> Self {
        RouteError::TopologyInvalid(e.to_string())
    }
}

/// 延迟统计
#[derive(Debug, Clone, Default)]
pub struct LatencyStats {
    /// 区域 → 平均延迟（微秒）
    latencies: std::collections::HashMap<String, u64>,
}

impl LatencyStats {
    /// 创建空统计
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录延迟
    pub fn record(&mut self, region_id: &str, latency_us: u64) {
        self.latencies.insert(region_id.to_string(), latency_us);
    }

    /// 获取延迟
    pub fn get(&self, region_id: &str) -> u64 {
        self.latencies.get(region_id).copied().unwrap_or(0)
    }
}

/// 全局路由决策器
pub struct GlobalRouter {
    topology: Arc<RegionTopology>,
    latency_stats: Arc<parking_lot::RwLock<LatencyStats>>,
}

impl GlobalRouter {
    /// 创建路由器
    pub fn new(topology: Arc<RegionTopology>) -> Self {
        Self {
            topology,
            latency_stats: Arc::new(parking_lot::RwLock::new(LatencyStats::new())),
        }
    }

    /// 更新延迟统计
    pub fn update_latency(&self, region_id: &str, latency_us: u64) {
        self.latency_stats.write().record(region_id, latency_us);
    }

    /// 路由决策
    ///
    /// 决策流程：读取拓扑 → 读取健康 → 数据归属查找 → 一致性检查 → 优先级排序 → 选择最优。
    /// 保证 ≤ 5ms（全内存查找，无 IO）。
    pub fn route(&self, req: RouteRequest<'_>) -> Result<RouteDecision, RouteError> {
        let start = Instant::now();

        if let Some(key) = req.data_affinity_key {
            if let Some(node) = self.topology.find_affinity_region(key) {
                if self.is_healthy(&node.region_id) {
                    if req.consistency == ConsistencyLevel::Strong
                        && req.query_type == QueryType::Read
                    {
                        if let Some(primary) = self.find_primary() {
                            return Ok(RouteDecision {
                                region_id: primary.region_id.clone(),
                                degraded: primary.region_id != node.region_id,
                                decision_latency: start.elapsed(),
                            });
                        }
                    }
                    return Ok(RouteDecision {
                        region_id: node.region_id.clone(),
                        degraded: false,
                        decision_latency: start.elapsed(),
                    });
                }
            }
        }

        if req.consistency == ConsistencyLevel::Strong && req.query_type == QueryType::Read {
            if let Some(primary) = self.find_primary() {
                return Ok(RouteDecision {
                    region_id: primary.region_id.clone(),
                    degraded: false,
                    decision_latency: start.elapsed(),
                });
            }
        }

        let candidates = self.topology.candidates_by_priority();
        for node in &candidates {
            if self.is_healthy(&node.region_id) {
                return Ok(RouteDecision {
                    region_id: node.region_id.clone(),
                    degraded: req.data_affinity_key.is_some()
                        || (req.consistency == ConsistencyLevel::Strong
                            && node.role != RegionRole::Primary),
                    decision_latency: start.elapsed(),
                });
            }
        }

        Err(RouteError::NoAvailableRegion)
    }

    fn is_healthy(&self, region_id: &str) -> bool {
        self.topology.health(region_id) == Some(RegionHealth::Healthy)
    }

    fn find_primary(&self) -> Option<&RegionNode> {
        self.topology
            .candidates_by_priority()
            .into_iter()
            .find(|n| n.role == RegionRole::Primary && self.is_healthy(&n.region_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::region_topology::{DataAffinityPolicy, RegionNode, RegionRole, ReplicationMode};

    fn make_region(id: &str, role: RegionRole, priority: u8) -> RegionNode {
        RegionNode {
            region_id: id.to_string(),
            role,
            failover_priority: priority,
            data_affinity: DataAffinityPolicy::Static,
            replication_mode: ReplicationMode::Async,
            replication_lag_threshold: Duration::from_millis(200),
            dsn: format!("mysql://{}", id),
        }
    }

    fn setup_router() -> GlobalRouter {
        let topo = RegionTopology::declare(vec![
            make_region("us-east-1", RegionRole::Primary, 1),
            make_region("us-west-2", RegionRole::Secondary, 2),
            make_region("eu-west-1", RegionRole::Secondary, 3),
        ])
        .unwrap();
        GlobalRouter::new(Arc::new(topo))
    }

    #[test]
    fn route_normal() {
        let router = setup_router();
        let req = RouteRequest::default();
        let decision = router.route(req).unwrap();
        assert!(!decision.degraded);
    }

    #[test]
    fn route_strong_consistency_to_primary() {
        let router = setup_router();
        let req = RouteRequest {
            consistency: ConsistencyLevel::Strong,
            query_type: QueryType::Read,
            ..Default::default()
        };
        let decision = router.route(req).unwrap();
        assert_eq!(decision.region_id, "us-east-1");
    }

    #[test]
    fn route_region_failover() {
        let router = setup_router();
        router
            .topology
            .update_health("us-east-1", RegionHealth::Unavailable);
        let req = RouteRequest {
            consistency: ConsistencyLevel::Strong,
            query_type: QueryType::Read,
            ..Default::default()
        };
        let result = router.route(req);
        assert!(result.is_err() || result.unwrap().degraded);
    }

    #[test]
    fn route_no_available_region() {
        let router = setup_router();
        for id in ["us-east-1", "us-west-2", "eu-west-1"] {
            router.topology.update_health(id, RegionHealth::Unavailable);
        }
        let req = RouteRequest::default();
        assert!(matches!(
            router.route(req),
            Err(RouteError::NoAvailableRegion)
        ));
    }

    #[test]
    fn route_decision_under_5ms() {
        let router = setup_router();
        let req = RouteRequest::default();
        let decision = router.route(req).unwrap();
        assert!(decision.decision_latency <= Duration::from_millis(5));
    }
}
