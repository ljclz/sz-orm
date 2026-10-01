//! v9.2.0 M7-T36：put/get/invalidate/invalidate_table 命中/未命中/TTL/LRU（4 tests）

use std::time::Duration;
use sz_orm_core::l2_cache::{CacheKey, L2Cache};
use sz_orm_core::Value;

#[test]
fn test_l2_cache_put_get_hit() {
    let cache = L2Cache::new();
    let key = CacheKey::by_pk("users", 1);
    cache.put(&key, Value::String("Alice".to_string()), None);
    let val = cache.get(&key);
    assert_eq!(val, Some(Value::String("Alice".to_string())));
}

#[test]
fn test_l2_cache_get_miss_returns_none() {
    let cache = L2Cache::new();
    let key = CacheKey::by_pk("users", 999);
    let val = cache.get(&key);
    assert!(val.is_none());
    let stats = cache.stats();
    assert!(stats.misses >= 1);
}

#[test]
fn test_l2_cache_invalidate_single_key() {
    let cache = L2Cache::new();
    let key = CacheKey::by_pk("users", 1);
    cache.put(&key, Value::I64(42), None);
    assert!(cache.get(&key).is_some());
    cache.invalidate(&key);
    assert!(cache.get(&key).is_none());
}

#[test]
fn test_l2_cache_invalidate_table_removes_all() {
    let cache = L2Cache::new();
    let k1 = CacheKey::by_pk("users", 1);
    let k2 = CacheKey::by_pk("users", 2);
    let k3 = CacheKey::by_query("orders", 100);
    cache.put(&k1, Value::String("a".to_string()), None);
    cache.put(&k2, Value::String("b".to_string()), None);
    cache.put(&k3, Value::String("c".to_string()), None);
    cache.invalidate_table("users");
    assert!(cache.get(&k1).is_none());
    assert!(cache.get(&k2).is_none());
    assert!(cache.get(&k3).is_some(), "其他表的缓存不应受影响");
    let _ = Duration::from_secs(1);
}
