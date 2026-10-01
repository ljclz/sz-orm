//! v9.2.0 M7-T40：L2CacheStats::hit_rate/miss_rate/merge（3 tests）

use sz_orm_core::l2_cache::L2CacheStats;

#[test]
fn test_l2_cache_stats_hit_rate_calculation() {
    let stats = L2CacheStats {
        hits: 8,
        misses: 2,
        sets: 10,
        evictions: 0,
        size: 8,
    };
    assert_eq!(stats.total_lookups(), 10);
    assert!((stats.hit_rate() - 0.8).abs() < 1e-9);

    let empty = L2CacheStats::default();
    assert_eq!(empty.hit_rate(), 0.0, "无查询时命中率应为 0");
}

#[test]
fn test_l2_cache_stats_miss_rate_is_complement_of_hit_rate() {
    let stats = L2CacheStats {
        hits: 7,
        misses: 3,
        sets: 10,
        evictions: 1,
        size: 7,
    };
    let hit = stats.hit_rate();
    let miss = stats.miss_rate();
    assert!(
        (hit + miss - 1.0).abs() < 1e-9,
        "hit_rate + miss_rate 应 = 1.0"
    );
    assert!((miss - 0.3).abs() < 1e-9);
}

#[test]
fn test_l2_cache_stats_merge_accumulates_fields() {
    let mut a = L2CacheStats {
        hits: 5,
        misses: 1,
        sets: 6,
        evictions: 0,
        size: 5,
    };
    let b = L2CacheStats {
        hits: 3,
        misses: 2,
        sets: 5,
        evictions: 1,
        size: 4,
    };
    a.merge(&b);
    assert_eq!(a.hits, 8);
    assert_eq!(a.misses, 3);
    assert_eq!(a.sets, 11);
    assert_eq!(a.evictions, 1);
    assert_eq!(a.size, 9);
}
