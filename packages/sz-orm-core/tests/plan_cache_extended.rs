//! v7.3.0 任务 1.5：查询计划缓存参数类型指纹与 TTL 淘汰测试

#![cfg(feature = "plan-cache")]

use std::time::Duration;
use sz_orm_core::DbType;
use sz_orm_core::plan_cache::{
    fingerprint_with_types, PlanCache, PlanCacheConfig,
};

/// 相同 SQL 重复执行命中缓存
#[test]
fn same_sql_repeated_hits_cache() {
    let cache = PlanCache::new(100, None);
    let sql = "SELECT * FROM users WHERE id = ?";

    cache.get_or_parse(sql).expect("parse");
    cache.get_or_parse(sql).expect("parse");
    cache.get_or_parse(sql).expect("parse");

    let stats = cache.stats();
    assert_eq!(stats.parse_misses, 1, "首次 miss");
    assert_eq!(stats.parse_hits, 2, "后续 hit");
}

/// 不同参数类型不命中
#[test]
fn different_param_types_no_collision() {
    let fp1 = fingerprint_with_types("SELECT * FROM users WHERE id = ?", &[DbType::MySQL]);
    let fp2 = fingerprint_with_types("SELECT * FROM users WHERE id = ?", &[DbType::PostgreSQL]);
    assert_ne!(fp1, fp2, "相同 SQL 不同参数类型应生成不同指纹");
}

/// 相同 SQL 相同参数类型生成相同指纹
#[test]
fn same_sql_same_types_same_fingerprint() {
    let fp1 = fingerprint_with_types("SELECT * FROM users WHERE id = ?", &[DbType::MySQL, DbType::PostgreSQL]);
    let fp2 = fingerprint_with_types("SELECT * FROM users WHERE id = ?", &[DbType::MySQL, DbType::PostgreSQL]);
    assert_eq!(fp1, fp2);
}

/// 不同 SQL 不同指纹
#[test]
fn different_sql_different_fingerprint() {
    let fp1 = fingerprint_with_types("SELECT * FROM users", &[]);
    let fp2 = fingerprint_with_types("SELECT * FROM orders", &[]);
    assert_ne!(fp1, fp2);
}

/// TTL 过期淘汰
#[test]
fn ttl_expiration_evicts() {
    let cache = PlanCache::new(100, Some(Duration::from_nanos(1)));
    let sql = "SELECT * FROM users";

    cache.get_or_parse(sql).expect("parse");
    std::thread::sleep(Duration::from_millis(10));
    cache.get_or_parse(sql).expect("parse");

    assert!(cache.stats().parse_misses >= 2, "TTL 过期后应重新 miss");
}

/// LRU 容量满淘汰
#[test]
fn lru_capacity_eviction() {
    let cache = PlanCache::new(3, None);
    cache.get_or_parse("SELECT * FROM t1").expect("parse");
    cache.get_or_parse("SELECT * FROM t2").expect("parse");
    cache.get_or_parse("SELECT * FROM t3").expect("parse");
    assert_eq!(cache.size(), 3);

    cache.get_or_parse("SELECT * FROM t4").expect("parse");
    assert_eq!(cache.size(), 3, "max_size=3 应保持 3 条");
    assert!(cache.eviction_count() >= 1, "应有淘汰");
}

/// 淘汰数指标递增
#[test]
fn eviction_count_increments() {
    let cache = PlanCache::new(2, None);
    assert_eq!(cache.eviction_count(), 0);

    cache.get_or_parse("SELECT * FROM t1").expect("parse");
    cache.get_or_parse("SELECT * FROM t2").expect("parse");
    cache.get_or_parse("SELECT * FROM t3").expect("parse");
    cache.get_or_parse("SELECT * FROM t4").expect("parse");

    assert!(cache.eviction_count() >= 2, "应至少淘汰 2 次");
}

/// PlanCacheConfig 默认值
#[test]
fn plan_cache_config_defaults() {
    let config = PlanCacheConfig::default();
    assert_eq!(config.capacity, 256);
    assert_eq!(config.ttl, Duration::from_secs(300));
    assert!(config.include_param_types);
}

/// PlanCacheConfig builder
#[test]
fn plan_cache_config_builder() {
    let config = PlanCacheConfig::new()
        .with_capacity(1024)
        .with_ttl(Duration::from_secs(600))
        .with_param_types(false);
    assert_eq!(config.capacity, 1024);
    assert_eq!(config.ttl, Duration::from_secs(600));
    assert!(!config.include_param_types);
}

/// 带参数类型指纹的 get_or_parse_with_types
#[test]
fn get_or_parse_with_types_distinguishes() {
    let cache = PlanCache::new(100, None);
    let sql = "SELECT * FROM users WHERE id = ?";

    let ast1 = cache.get_or_parse_with_types(sql, &[DbType::MySQL]).expect("parse");
    let ast2 = cache.get_or_parse_with_types(sql, &[DbType::MySQL]).expect("parse");
    assert!(std::sync::Arc::ptr_eq(&ast1, &ast2), "相同类型应命中");

    let ast3 = cache.get_or_parse_with_types(sql, &[DbType::PostgreSQL]).expect("parse");
    assert!(!std::sync::Arc::ptr_eq(&ast1, &ast3), "不同类型不应命中相同 AST");
}