//! 多区域多活集成测试
//!
//! 验证 RegionTopology / GlobalRouter / Failover / ReplicationLag / EdgeNode / Conflict 多版本 协同工作。

#![cfg(feature = "multi-region")]

use std::sync::Arc;
use std::time::Duration;

use sz_orm_fusion::*;

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

#[test]
fn test_region_topology_with_router() {
    let topo = RegionTopology::declare(vec![
        make_region("cn-east-1", RegionRole::Primary, 1),
        make_region("cn-east-2", RegionRole::Secondary, 2),
    ])
    .unwrap();
    assert_eq!(topo.len(), 2);

    let router = GlobalRouter::new(Arc::new(topo));
    let req = RouteRequest {
        consistency: ConsistencyLevel::Strong,
        query_type: QueryType::Read,
        ..Default::default()
    };
    let decision = router.route(req).unwrap();
    assert!(!decision.region_id.is_empty());
}

#[test]
fn test_failover_with_replication_lag() {
    let topo = RegionTopology::declare(vec![
        make_region("r1", RegionRole::Primary, 1),
        make_region("r2", RegionRole::Secondary, 2),
    ])
    .unwrap();
    let arc_topo = Arc::new(topo);

    let failover = RegionFailoverCoordinator::new(arc_topo.clone());
    let lag_tracker = ReplicationLagTracker::new();

    lag_tracker.record_lag("r1", "r2", Duration::from_millis(10));
    let p99 = lag_tracker.p99_lag("r1", "r2");
    assert!(p99 > Duration::ZERO);

    let decision = failover.on_region_failure("r1").unwrap();
    assert_eq!(decision.from_region, "r1");
    assert_eq!(decision.to_region, "r2");
}

#[test]
fn test_edge_node_geo_routing() {
    let edge = EdgeNodeRouter::new(vec![
        EdgeNode::new("edge-cn", "Beijing", 39.90, 116.40),
        EdgeNode::new("edge-us", "NewYork", 40.71, -74.00),
    ]);

    let client = GeoLocation::new(31.23, 121.47);
    let nearest = edge.find_nearest(&client).unwrap();
    assert_eq!(nearest.node_id, "edge-cn");
}

#[test]
fn test_multi_version_conflict_resolution() {
    let resolver = ConflictResolver::new(ResolutionStrategy::MultiVersion, "r1");
    let versions = vec![
        DataVersion::new("r1", serde_json::json!({"v": 1}), 1),
        DataVersion::new("r2", serde_json::json!({"v": 2}), 2),
    ];
    let conflict = Conflict::new("k", ConflictType::ValueMismatch, versions);
    let resolution = resolver.resolve(&conflict);
    assert_eq!(resolution.strategy, ResolutionStrategy::MultiVersion);
    assert!(resolution.resolved_value.is_array());
    assert_eq!(resolution.resolved_value.as_array().unwrap().len(), 2);
}
