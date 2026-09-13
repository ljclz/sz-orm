//! 多区域多活示例
//!
//! 演示 RegionTopology + GlobalRouter + Failover + ReplicationLag + EdgeNode + MultiVersion Conflict。

use std::sync::Arc;
use std::time::Duration;

use sz_orm_fusion::{
    Conflict, ConflictResolver, ConflictType, ConsistencyLevel, DataAffinityPolicy, DataVersion,
    EdgeNode, EdgeNodeRouter, GeoLocation, GlobalRouter, QueryType, RegionFailoverCoordinator,
    RegionNode, RegionRole, RegionTopology, ReplicationLagTracker, ReplicationMode,
    ResolutionStrategy, RouteRequest,
};

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

fn main() {
    let topo = RegionTopology::declare(vec![
        make_region("cn-east-1", RegionRole::Primary, 1),
        make_region("cn-east-2", RegionRole::Secondary, 2),
        make_region("us-west-1", RegionRole::Secondary, 3),
    ])
    .expect("拓扑声明失败");
    println!("区域数量: {}", topo.len());

    let arc_topo = Arc::new(topo);

    let router = GlobalRouter::new(arc_topo.clone());
    let req = RouteRequest {
        consistency: ConsistencyLevel::Strong,
        query_type: QueryType::Read,
        ..Default::default()
    };
    let decision = router.route(req).unwrap();
    println!(
        "路由决策: {} (降级: {})",
        decision.region_id, decision.degraded
    );

    let failover = RegionFailoverCoordinator::new(arc_topo.clone());
    let failover_decision = failover.on_region_failure("cn-east-1").unwrap();
    println!(
        "容灾切换: {} -> {} (RTO: {:?})",
        failover_decision.from_region, failover_decision.to_region, failover_decision.rto_elapsed
    );

    let lag_tracker = ReplicationLagTracker::new();
    lag_tracker.record_lag("cn-east-1", "cn-east-2", Duration::from_millis(15));
    println!(
        "P99 延迟: {:?}",
        lag_tracker.p99_lag("cn-east-1", "cn-east-2")
    );

    let edge = EdgeNodeRouter::new(vec![
        EdgeNode::new("edge-cn", "Beijing", 39.90, 116.40),
        EdgeNode::new("edge-us", "NewYork", 40.71, -74.00),
    ]);
    let client = GeoLocation::new(31.23, 121.47);
    let nearest = edge.find_nearest(&client).unwrap();
    println!("最近边缘节点: {} ({})", nearest.node_id, nearest.location);

    let resolver = ConflictResolver::new(ResolutionStrategy::MultiVersion, "cn-east-1");
    let versions = vec![
        DataVersion::new("cn-east-1", serde_json::json!({"v": 1}), 1),
        DataVersion::new("cn-east-2", serde_json::json!({"v": 2}), 2),
    ];
    let conflict = Conflict::new("user:123", ConflictType::ValueMismatch, versions);
    let resolution = resolver.resolve(&conflict);
    println!(
        "冲突解决: 策略={:?}, 胜出版本={:?}",
        resolution.strategy, resolution.resolved_value
    );

    let _ = arc_topo.health("cn-east-1");
}
