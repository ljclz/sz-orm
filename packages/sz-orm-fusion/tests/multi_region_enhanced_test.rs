//! v7.7.0 任务 3.6：多区域多活增强端到端测试

#![cfg(feature = "multi-region-enhanced")]

use sz_orm_fusion::{AutoResolutionStrategy, ConflictAutoResolver, GeoRouter};

#[tokio::test]
async fn e2e_conflict_auto_resolve_lww() {
    let resolver = ConflictAutoResolver::new(AutoResolutionStrategy::Lww);
    let conflict = sz_orm_fusion::Conflict::new(
        "test_key",
        sz_orm_fusion::ConflictType::ValueMismatch,
        vec![
            sz_orm_fusion::DataVersion::new("region-a", serde_json::json!({"v": 1}), 1),
            sz_orm_fusion::DataVersion::new("region-b", serde_json::json!({"v": 2}), 2),
        ],
    );
    let result = resolver.auto_resolve(&conflict).await.unwrap();
    assert!(result.conflict_auto_resolved);
    assert!(result.data_intact);
    assert!(result.conflict_traceable);
}

#[test]
fn e2e_geo_router_route() {
    let router = GeoRouter::new("us-east-1");
    assert_eq!(router.current_region(), "us-east-1");
}
