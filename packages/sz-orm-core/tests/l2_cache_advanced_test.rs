//! v9.2.0 M15: l2_cache.rs 高级分支覆盖

use std::sync::Arc;
use std::time::Duration;
use sz_orm_core::l2_cache::{
    CacheKey, InvalidationBus, InvalidationMessage, L2Cache, L2CacheStats, LocalInvalidationBus,
    PerTableStats,
};
use sz_orm_core::Value;

#[test]
fn test_local_invalidation_bus_basic() {
    let bus = LocalInvalidationBus::new(10);
    bus.publish(InvalidationMessage::InvalidateKey("key1".to_string()));
    bus.publish(InvalidationMessage::InvalidateTable("table1".to_string()));
    let _msgs = bus.subscribe();
}

#[test]
fn test_cache_key_by_pk() {
    let key = CacheKey::by_pk("users", 1);
    let s = key.to_string_key();
    assert!(s.contains("users"));
}

#[test]
fn test_cache_key_by_query() {
    let key = CacheKey::by_query("users", "hash123");
    let s = key.to_string_key();
    assert!(s.contains("users"));
}

#[test]
fn test_cache_key_by_relation() {
    let key = CacheKey::by_relation("users", "posts");
    let s = key.to_string_key();
    assert!(s.contains("users"));
}

#[test]
fn test_l2_cache_with_config() {
    let _cache = L2Cache::new()
        .with_default_ttl(Duration::from_secs(60))
        .with_max_size(100)
        .with_invalidation_bus(Arc::new(LocalInvalidationBus::new(10)));
}

#[test]
fn test_l2_cache_put_get() {
    let cache = L2Cache::new().with_max_size(10);
    let key = CacheKey::by_pk("users", 1);
    cache.put(&key, Value::I64(42), Some(Duration::from_secs(60)));
    let val = cache.get(&key);
    assert!(val.is_some());
    assert_eq!(val.unwrap(), Value::I64(42));
}

#[test]
fn test_l2_cache_miss() {
    let cache = L2Cache::new().with_max_size(10);
    let key = CacheKey::by_pk("users", 999);
    let val = cache.get(&key);
    assert!(val.is_none());
}

#[test]
fn test_l2_cache_invalidate() {
    let cache = L2Cache::new().with_max_size(10);
    let key = CacheKey::by_pk("users", 1);
    cache.put(&key, Value::I64(42), Some(Duration::from_secs(60)));
    cache.invalidate(&key);
    assert!(cache.get(&key).is_none());
}

#[test]
fn test_l2_cache_invalidate_table() {
    let cache = L2Cache::new().with_max_size(10);
    let key1 = CacheKey::by_pk("users", 1);
    let key2 = CacheKey::by_pk("users", 2);
    cache.put(&key1, Value::I64(1), Some(Duration::from_secs(60)));
    cache.put(&key2, Value::I64(2), Some(Duration::from_secs(60)));
    cache.invalidate_table("users");
    assert!(cache.get(&key1).is_none());
    assert!(cache.get(&key2).is_none());
}

#[test]
fn test_l2_cache_clear() {
    let cache = L2Cache::new().with_max_size(10);
    let key = CacheKey::by_pk("users", 1);
    cache.put(&key, Value::I64(42), Some(Duration::from_secs(60)));
    cache.clear();
    assert_eq!(cache.size(), 0);
}

#[test]
fn test_l2_cache_contains() {
    let cache = L2Cache::new().with_max_size(10);
    let key = CacheKey::by_pk("users", 1);
    cache.put(&key, Value::I64(42), Some(Duration::from_secs(60)));
    assert!(cache.contains(&key));
}

#[test]
fn test_l2_cache_reset_stats() {
    let cache = L2Cache::new().with_max_size(10);
    let key = CacheKey::by_pk("users", 1);
    cache.put(&key, Value::I64(42), Some(Duration::from_secs(60)));
    let _ = cache.get(&key);
    cache.reset_stats();
    let stats = cache.stats();
    assert_eq!(stats.hits, 0);
    assert_eq!(stats.misses, 0);
}

#[test]
fn test_l2_cache_invalidate_query() {
    let cache = L2Cache::new().with_max_size(10);
    cache.invalidate_query("users", "SELECT * FROM users", &[]);
}

#[test]
fn test_l2_cache_table_stats() {
    let cache = L2Cache::new().with_max_size(10);
    let key = CacheKey::by_pk("users", 1);
    cache.put(&key, Value::I64(42), Some(Duration::from_secs(60)));
    let _ = cache.get(&key);
    let stats = cache.table_stats("users");
    assert!(stats.is_some());
}

#[test]
fn test_l2_cache_all_table_stats() {
    let cache = L2Cache::new().with_max_size(10);
    let key = CacheKey::by_pk("users", 1);
    cache.put(&key, Value::I64(42), Some(Duration::from_secs(60)));
    let all = cache.all_table_stats();
    assert!(!all.is_empty());
}

#[test]
fn test_l2_cache_evict_expired() {
    let cache = L2Cache::new().with_max_size(10);
    let key = CacheKey::by_pk("users", 1);
    cache.put(&key, Value::I64(42), Some(Duration::from_millis(1)));
    std::thread::sleep(Duration::from_millis(10));
    let evicted = cache.evict_expired();
    assert!(evicted >= 1);
}

#[test]
fn test_l2_cache_update_ttl() {
    let cache = L2Cache::new().with_max_size(10);
    let key = CacheKey::by_pk("users", 1);
    cache.put(&key, Value::I64(42), Some(Duration::from_secs(60)));
    let ok = cache.update_ttl(&key, Duration::from_secs(120));
    assert!(ok);
}

#[test]
fn test_l2_cache_remaining_ttl() {
    let cache = L2Cache::new().with_max_size(10);
    let key = CacheKey::by_pk("users", 1);
    cache.put(&key, Value::I64(42), Some(Duration::from_secs(60)));
    let ttl = cache.remaining_ttl(&key);
    assert!(ttl.is_some());
}

#[test]
fn test_l2_cache_stats_hit_rate() {
    let stats = L2CacheStats {
        hits: 8,
        misses: 2,
        ..Default::default()
    };
    assert!((stats.hit_rate() - 0.8).abs() < 0.01);
}

#[test]
fn test_l2_cache_stats_miss_rate() {
    let stats = L2CacheStats {
        hits: 8,
        misses: 2,
        ..Default::default()
    };
    assert!((stats.miss_rate() - 0.2).abs() < 0.01);
}

#[test]
fn test_l2_cache_stats_merge() {
    let mut s1 = L2CacheStats {
        hits: 5,
        misses: 3,
        ..Default::default()
    };
    let s2 = L2CacheStats {
        hits: 2,
        misses: 1,
        ..Default::default()
    };
    s1.merge(&s2);
    assert_eq!(s1.hits, 7);
    assert_eq!(s1.misses, 4);
}

#[test]
fn test_per_table_stats() {
    let stats = PerTableStats {
        hits: 5,
        misses: 5,
        ..Default::default()
    };
    assert_eq!(stats.total_lookups(), 10);
    assert!((stats.hit_rate() - 0.5).abs() < 0.01);
}

#[test]
fn test_per_table_stats_empty() {
    let stats = PerTableStats::default();
    assert_eq!(stats.total_lookups(), 0);
    assert_eq!(stats.hit_rate(), 0.0);
}

#[test]
fn test_l2_cache_size() {
    let cache = L2Cache::new().with_max_size(10);
    assert_eq!(cache.size(), 0);
    let key = CacheKey::by_pk("users", 1);
    cache.put(&key, Value::I64(42), Some(Duration::from_secs(60)));
    assert_eq!(cache.size(), 1);
}

#[test]
fn test_l2_cache_eviction_on_full() {
    let cache = L2Cache::new().with_max_size(2);
    let k1 = CacheKey::by_pk("users", 1);
    let k2 = CacheKey::by_pk("users", 2);
    let k3 = CacheKey::by_pk("users", 3);
    cache.put(&k1, Value::I64(1), Some(Duration::from_secs(60)));
    cache.put(&k2, Value::I64(2), Some(Duration::from_secs(60)));
    cache.put(&k3, Value::I64(3), Some(Duration::from_secs(60)));
    assert!(cache.size() <= 2);
}
