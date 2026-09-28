//! v9.2.0 M7-T38：evict_expired/update_ttl/remaining_ttl + 边界（4 tests）

use std::time::Duration;
use sz_orm_core::l2_cache::{CacheKey, L2Cache};
use sz_orm_core::Value;

#[tokio::test]
async fn test_l2_cache_evict_expired_removes_timed_out_entries() {
    let cache = L2Cache::new();
    let key = CacheKey::by_pk("users", 1);
    cache.put(&key, Value::I64(1), Some(Duration::from_millis(50)));
    assert!(cache.get(&key).is_some());
    tokio::time::sleep(Duration::from_millis(80)).await;
    let removed = cache.evict_expired();
    assert_eq!(removed, 1, "应清理 1 个过期项");
    assert!(cache.get(&key).is_none());
}

#[test]
fn test_l2_cache_update_ttl_resets_expiry() {
    let cache = L2Cache::new();
    let key = CacheKey::by_pk("users", 1);
    cache.put(&key, Value::I64(1), Some(Duration::from_secs(1)));
    let updated = cache.update_ttl(&key, Duration::from_secs(60));
    assert!(updated, "存在的 key 应更新成功");
    let remaining = cache.remaining_ttl(&key);
    assert!(remaining.is_some(), "更新后应存在");
    if let Some(Some(ttl)) = remaining {
        assert!(
            ttl > Duration::from_secs(30),
            "更新后剩余 TTL 应接近 60s，实际: {:?}",
            ttl
        );
    }
}

#[test]
fn test_l2_cache_update_ttl_returns_false_for_missing_key() {
    let cache = L2Cache::new();
    let key = CacheKey::by_pk("users", 999);
    let updated = cache.update_ttl(&key, Duration::from_secs(60));
    assert!(!updated, "不存在的 key 应返回 false");
}

#[test]
fn test_l2_cache_remaining_ttl_no_ttl_returns_some_none() {
    let cache = L2Cache::new();
    let key = CacheKey::by_pk("users", 1);
    cache.put(&key, Value::I64(1), None);
    let remaining = cache.remaining_ttl(&key);
    assert_eq!(
        remaining,
        Some(None),
        "无 TTL 的 key 应返回 Some(None) 表示永不过期"
    );
}