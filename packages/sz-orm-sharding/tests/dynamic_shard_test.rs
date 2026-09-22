//! v7.6.0 组3.9：动态分片 + 分布式缓存增强端到端测试

use sz_orm_sharding::enhanced::{DynamicShardAdjuster, Hotspot, HotspotMigrator};

#[test]
fn test_e2e_dynamic_shard_scale_up() {
    let mut adjuster = DynamicShardAdjuster::new(4);
    let result = adjuster.adjust_runtime(8).unwrap();
    assert_eq!(result.shard_count_before, 4);
    assert_eq!(result.shard_count_after, 8);
    assert!(result.data_intact);
    assert!(result.query_uninterrupted);
}

#[test]
fn test_e2e_hotspot_migration() {
    let hotspot = Hotspot {
        table: "orders".to_string(),
        shard_key: serde_json::json!(42),
        access_frequency: 5000.0,
        current_shard: 0,
    };
    let result = HotspotMigrator::migrate(&hotspot, 5, 10000).unwrap();
    assert_eq!(result.migrated_rows, 10000);
    assert!(result.query_uninterrupted);
    assert!(result.data_intact);
}

#[test]
fn test_e2e_dynamic_shard_scale_down() {
    let mut adjuster = DynamicShardAdjuster::new(16);
    let result = adjuster.adjust_runtime(8).unwrap();
    assert_eq!(result.shard_count_before, 16);
    assert_eq!(result.shard_count_after, 8);
}
