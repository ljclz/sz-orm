//! v7.5.0 LruKReplacer 和 AdaptiveCacheCapacity 端到端测试

use sz_orm_core::plan_cache::{AdaptiveCacheCapacity, LruKReplacer};

#[test]
fn test_lru_k_basic_access() {
    let mut replacer = LruKReplacer::new(2, 3);
    replacer.access(1);
    replacer.access(2);
    replacer.access(3);
    assert_eq!(replacer.len(), 3);
    assert!(!replacer.is_empty());
}

#[test]
fn test_lru_k_evict_when_full() {
    let mut replacer = LruKReplacer::new(2, 2);
    replacer.access(1);
    replacer.access(2);
    let evicted = replacer.evict();
    assert!(evicted.is_some());
    assert_eq!(replacer.len(), 1);
}

#[test]
fn test_lru_k_no_evict_when_not_full() {
    let mut replacer = LruKReplacer::new(2, 10);
    replacer.access(1);
    let evicted = replacer.evict();
    assert!(evicted.is_none());
}

#[test]
fn test_lru_k_hit_rate() {
    let mut replacer = LruKReplacer::new(2, 10);
    replacer.access(1);
    replacer.access(1);
    replacer.access(2);
    let rate = replacer.hit_rate();
    assert!((0.0..=1.0).contains(&rate));
}

#[test]
fn test_lru_k_empty_hit_rate() {
    let replacer = LruKReplacer::new(2, 10);
    assert_eq!(replacer.hit_rate(), 0.0);
}

#[test]
fn test_adaptive_cache_capacity_new() {
    let acc = AdaptiveCacheCapacity::new(10, 100);
    assert_eq!(acc.current(), 10);
    assert_eq!(acc.min(), 10);
    assert_eq!(acc.max(), 100);
}

#[test]
fn test_adaptive_cache_capacity_expand_on_low_hit_rate() {
    let mut acc = AdaptiveCacheCapacity::new(10, 100);
    for _ in 0..10 {
        acc.record_hit_rate(0.80);
    }
    let old = acc.current();
    acc.adapt();
    assert!(acc.current() >= old);
}

#[test]
fn test_adaptive_cache_capacity_shrink_on_high_hit_rate() {
    let mut acc = AdaptiveCacheCapacity::new(10, 100);
    acc.record_hit_rate(0.96);
    acc.record_hit_rate(0.97);
    acc.adapt();
    assert_eq!(acc.current(), 10);
}

#[test]
fn test_adaptive_cache_capacity_no_adapt_on_empty() {
    let mut acc = AdaptiveCacheCapacity::new(10, 100);
    assert!(!acc.adapt());
}

#[test]
fn test_adaptive_cache_capacity_clamp_to_max() {
    let mut acc = AdaptiveCacheCapacity::new(10, 50);
    for _ in 0..20 {
        acc.record_hit_rate(0.50);
    }
    acc.adapt();
    acc.adapt();
    acc.adapt();
    assert!(acc.current() <= acc.max());
}
