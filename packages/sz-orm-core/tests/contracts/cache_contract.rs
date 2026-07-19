//! Cache 模块契约测试 — 对应 `docs/api-contracts.md` §10
//!
//! 锁定 Cache trait、MemoryCache、MultiLevelCache 契约。
//!
//! 注意：实际 API 与 api-contracts.md §10 文档有差异：
//!
//! - `MemoryCache::new()` 不接 capacity 参数，返回 Self（非 Result）
//! - `Cache::get()` 返回 `Result<Option<Vec<u8>>, CacheError>`（非 `Option<Value>`）
//! - `Cache::set()` 接受 `Vec<u8>` + `Option<Duration>` TTL（非 Value）
//!
//! 这些差异已在本契约测试中锁定为实际行为；api-contracts.md 需后续同步修正。

use std::time::Duration;
use sz_orm_core::{Cache, CacheError, MemoryCache, MultiLevelCache};

// ===== §10.1 Cache trait 契约 =====

#[test]
fn test_cache_get_misses_return_ok_none_contract() {
    let cache = MemoryCache::new();
    // 未命中的键应返回 Ok(None)，不是 Err
    let result = cache.get("missing_key").unwrap();
    assert!(result.is_none());
}

#[test]
fn test_cache_set_then_get_contract() {
    let cache = MemoryCache::new();
    cache.set("k1", b"v1".to_vec(), None).unwrap();
    let v = cache.get("k1").unwrap();
    assert_eq!(v, Some(b"v1".to_vec()));
}

#[test]
fn test_cache_delete_contract() {
    let cache = MemoryCache::new();
    cache.set("k1", b"v1".to_vec(), None).unwrap();
    assert!(cache.get("k1").unwrap().is_some());

    cache.delete("k1").unwrap();
    assert!(cache.get("k1").unwrap().is_none());
}

#[test]
fn test_cache_clear_contract() {
    let cache = MemoryCache::new();
    cache.set("k1", b"v1".to_vec(), None).unwrap();
    cache.set("k2", b"v2".to_vec(), None).unwrap();

    cache.clear().unwrap();

    assert!(cache.get("k1").unwrap().is_none());
    assert!(cache.get("k2").unwrap().is_none());
}

#[test]
fn test_cache_exists_contract() {
    let cache = MemoryCache::new();
    assert!(!cache.exists("k1").unwrap());

    cache.set("k1", b"v1".to_vec(), None).unwrap();
    assert!(cache.exists("k1").unwrap());

    cache.delete("k1").unwrap();
    assert!(!cache.exists("k1").unwrap());
}

// ===== §10.1 TTL 过期契约 =====

#[test]
fn test_cache_set_with_ttl_expires_contract() {
    let cache = MemoryCache::new();
    cache
        .set("k1", b"v1".to_vec(), Some(Duration::from_millis(50)))
        .unwrap();
    assert!(cache.get("k1").unwrap().is_some());

    // 等待 TTL 过期
    std::thread::sleep(Duration::from_millis(100));
    // 过期后应返回 None
    assert!(
        cache.get("k1").unwrap().is_none(),
        "TTL 过期后 get 应返回 None"
    );
}

#[test]
fn test_cache_set_without_ttl_persists_contract() {
    let cache = MemoryCache::new();
    cache.set("k1", b"v1".to_vec(), None).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    // 无 TTL 应持久存储
    assert!(cache.get("k1").unwrap().is_some());
}

#[test]
fn test_cache_expire_sets_ttl_contract() {
    let cache = MemoryCache::new();
    cache.set("k1", b"v1".to_vec(), None).unwrap();
    // 为已有键设置 TTL
    cache.expire("k1", Duration::from_millis(50)).unwrap();

    std::thread::sleep(Duration::from_millis(100));
    assert!(cache.get("k1").unwrap().is_none());
}

#[test]
fn test_cache_expire_missing_key_returns_not_found_contract() {
    let cache = MemoryCache::new();
    let err = cache.expire("missing", Duration::from_secs(1)).unwrap_err();
    assert!(matches!(err, CacheError::NotFound(_)));
}

#[test]
fn test_cache_ttl_returns_remaining_duration_contract() {
    let cache = MemoryCache::new();
    cache
        .set("k1", b"v1".to_vec(), Some(Duration::from_secs(10)))
        .unwrap();
    let ttl = cache.ttl("k1").unwrap();
    assert!(ttl.is_some(), "有 TTL 的键应返回 Some(剩余时长)");
    // 剩余时长应 <= 10s
    assert!(ttl.unwrap() <= Duration::from_secs(10));
}

#[test]
fn test_cache_ttl_missing_key_returns_not_found_contract() {
    let cache = MemoryCache::new();
    let err = cache.ttl("missing").unwrap_err();
    assert!(matches!(err, CacheError::NotFound(_)));
}

// ===== §10.2 MemoryCache 契约 =====

#[test]
fn test_memory_cache_new_returns_self_contract() {
    // 实际 API：new() 返回 Self（不接 capacity，不返回 Result）
    let _cache: MemoryCache = MemoryCache::new();
}

#[test]
fn test_memory_cache_default_contract() {
    let _cache: MemoryCache = Default::default();
}

#[test]
fn test_memory_cache_with_ttl_contract() {
    // with_ttl 设置默认 TTL
    let cache = MemoryCache::with_ttl(Duration::from_secs(100));
    cache.set("k1", b"v1".to_vec(), None).unwrap();
    // 未显式传 TTL 时应使用默认 TTL
    let ttl = cache.ttl("k1").unwrap();
    assert!(ttl.is_some());
}

// ===== §10.3 MultiLevelCache 契约 =====

#[test]
fn test_multi_level_cache_new_contract() {
    let _cache: MultiLevelCache = MultiLevelCache::new();
}

#[test]
fn test_multi_level_cache_empty_misses_contract() {
    // 空的多级缓存，所有 get 都应返回 Ok(None)
    let cache = MultiLevelCache::new();
    assert!(cache.get("k1").unwrap().is_none());
}
