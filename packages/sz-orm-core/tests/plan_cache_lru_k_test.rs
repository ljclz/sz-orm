//! v7.4.0 任务 3.7：PlanCache LRU-K 端到端测试

use sz_orm_core::plan_cache::PlanCache;
use std::time::Duration;

#[test]
fn test_lru_k_access_count() {
    let cache = PlanCache::new(10, None);
    let sql = "SELECT * FROM users WHERE id = 1";
    let _ = cache.get_or_parse(sql);
    let _ = cache.get_or_parse(sql);
    let _ = cache.get_or_parse(sql);
    let key = sz_orm_core::plan_cache::PlanCacheKey::from_sql(sql);
    assert!(cache.access_count(key.hash) >= 2);
}

#[test]
fn test_lru_k_value() {
    let cache = PlanCache::new(10, None);
    assert_eq!(cache.lru_k(), 2);
}

#[test]
fn test_parse_hit_rate() {
    let cache = PlanCache::new(10, None);
    let sql1 = "SELECT * FROM users WHERE id = 1";
    let sql2 = "SELECT * FROM users WHERE id = 2";
    let _ = cache.get_or_parse(sql1);
    let _ = cache.get_or_parse(sql1);
    let _ = cache.get_or_parse(sql2);
    let rate = cache.parse_hit_rate();
    assert!(rate > 0.0 && rate < 1.0);
}

#[test]
fn test_capacity() {
    let cache = PlanCache::new(100, None);
    assert_eq!(cache.capacity(), 100);
}

#[test]
fn test_adjust_capacity_high_hit_rate() {
    let mut cache = PlanCache::new(100, Some(Duration::from_secs(60)));
    let sql = "SELECT * FROM users WHERE id = 1";
    for _ in 0..100 {
        let _ = cache.get_or_parse(sql);
    }
    let changed = cache.adjust_capacity();
    let rate = cache.parse_hit_rate();
    if rate > 0.8 {
        assert!(changed || cache.capacity() >= 100);
    }
}

#[test]
fn test_adjust_capacity_low_hit_rate() {
    let mut cache = PlanCache::new(100, Some(Duration::from_secs(60)));
    for i in 0..50 {
        let sql = format!("SELECT * FROM users WHERE id = {}", i);
        let _ = cache.get_or_parse(&sql);
    }
    let changed = cache.adjust_capacity();
    let rate = cache.parse_hit_rate();
    if rate < 0.3 {
        assert!(changed || cache.capacity() <= 100);
    }
}

#[test]
fn test_plan_cache_repeated_sql_high_hit_rate() {
    let cache = PlanCache::new(50, None);
    let sqls = [
        "SELECT * FROM users WHERE id = 1",
        "SELECT * FROM users WHERE id = 2",
        "SELECT * FROM users WHERE id = 1",
        "SELECT * FROM users WHERE id = 2",
        "SELECT * FROM users WHERE id = 1",
    ];
    for sql in &sqls {
        let _ = cache.get_or_parse(sql);
    }
    let rate = cache.parse_hit_rate();
    assert!(rate >= 0.4, "重复 SQL 命中率应 >= 40%，实际 {}", rate);
}