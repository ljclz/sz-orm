#![cfg(feature = "db-fusion")]

//! v7.6.0 组3.9：多区域多活端到端测试
//!
//! 验证：多区域复制 + 冲突解决（LWW/CRDT/BusinessMerge）+ 最终一致性/强一致性

use sz_orm_fusion::conflict::{
    Conflict, ConflictResolverEnhanced, ConflictType, DataVersion, EnhancedResolutionStrategy,
};
use sz_orm_fusion::sync::{DataChange, MultiRegionReplicator, SyncMode};

#[test]
fn test_e2e_multi_region_async_replication() {
    let replicator = MultiRegionReplicator::new(
        "us-east",
        vec!["eu-west".to_string(), "ap-south".to_string()],
        SyncMode::Async,
    );
    let change = DataChange {
        table: "users".to_string(),
        key: serde_json::json!(1),
        old_value: None,
        new_value: Some(serde_json::json!({"name": "Alice"})),
        source_region: "us-east".to_string(),
        timestamp_ms: 1700000000,
    };
    let results = replicator.replicate_async(&change);
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|r| r.success));
}

#[test]
fn test_e2e_multi_region_sync_replication() {
    let replicator =
        MultiRegionReplicator::new("us-east", vec!["eu-west".to_string()], SyncMode::Sync);
    let change = DataChange {
        table: "users".to_string(),
        key: serde_json::json!(1),
        old_value: None,
        new_value: Some(serde_json::json!({"name": "Alice"})),
        source_region: "us-east".to_string(),
        timestamp_ms: 1700000000,
    };
    let results = replicator.replicate_sync(&change);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].mode, SyncMode::Sync);
}

#[test]
fn test_e2e_conflict_lww() {
    let conflict = Conflict::new(
        "user:1",
        ConflictType::ValueMismatch,
        vec![
            DataVersion::new("region-a", serde_json::json!({"name": "Alice"}), 100),
            DataVersion::new("region-b", serde_json::json!({"name": "Bob"}), 200),
        ],
    );
    let resolver = ConflictResolverEnhanced::new(EnhancedResolutionStrategy::Lww);
    let result = resolver.resolve(&conflict).unwrap();
    assert_eq!(result.resolved_value, serde_json::json!({"name": "Bob"}));
}

#[test]
fn test_e2e_conflict_crdt() {
    let conflict = Conflict::new(
        "user:1",
        ConflictType::ValueMismatch,
        vec![
            DataVersion::new("a", serde_json::json!(1), 100),
            DataVersion::new("b", serde_json::json!(2), 200),
        ],
    );
    let resolver = ConflictResolverEnhanced::new(EnhancedResolutionStrategy::Crdt);
    let result = resolver.resolve(&conflict).unwrap();
    assert!(result.resolved_value.is_array());
}

#[test]
fn test_e2e_conflict_business_merge() {
    let conflict = Conflict::new(
        "user:1",
        ConflictType::ValueMismatch,
        vec![
            DataVersion::new("a", serde_json::json!({"name": "Alice"}), 100),
            DataVersion::new("b", serde_json::json!({"age": 30}), 200),
        ],
    );
    let resolver = ConflictResolverEnhanced::new(EnhancedResolutionStrategy::BusinessMerge);
    let result = resolver.resolve(&conflict).unwrap();
    assert!(result.resolved_value.is_object());
}
