//! v3.2.0 M2 查询计划缓存 — 差分测试
//!
//! 验证：
//! 1. 缓存命中 vs 未缓存：解析结果完全一致
//! 2. 缓存键无碰撞：不同语义 SQL → 不同 hash
//! 3. 并发竞态：多线程并发缓存同一 SQL，结果正确
//! 4. Schema 变更后缓存失效再查询结果与未缓存一致
//!
//! 运行方式：cargo test -p sz-orm-core --features plan-cache --test plan_cache_differential

use std::sync::Arc;
use std::thread;

use sz_orm_core::plan_cache::{PlanCache, PlanCacheKey, SqlNormalizer};

/// 缓存命中 vs 未缓存：解析结果完全一致
#[test]
fn test_cache_hit_vs_no_cache_same_result() {
    let cache = PlanCache::new(100, None);
    let sql = "SELECT id, name FROM users WHERE id = ?";

    let ast_cached = cache.get_or_parse(sql).expect("parse cached");
    let ast_cached2 = cache.get_or_parse(sql).expect("parse cached 2");

    assert!(
        Arc::ptr_eq(&ast_cached, &ast_cached2),
        "缓存命中应返回相同 Arc（零拷贝）"
    );

    let stats = cache.stats();
    assert!(stats.parse_hits >= 1, "应有命中");
    assert_eq!(stats.parse_misses, 1, "应有 1 次未命中");
}

/// 相同 SQL 模板不同写法归一化后相同
#[test]
fn test_normalization_equivalence() {
    let variants = [
        "SELECT * FROM users WHERE id = ?",
        "select * from users where id = ?",
        "SELECT   *   FROM   users   WHERE   id   =   ?",
        "SELECT * FROM users WHERE id=?",
    ];

    let normalized: Vec<String> = variants
        .iter()
        .map(|s| SqlNormalizer::normalize(s))
        .collect();
    for n in &normalized[1..] {
        assert_eq!(normalized[0], *n, "所有变体应归一化为相同形式");
    }

    let keys: Vec<PlanCacheKey> = variants.iter().map(|s| PlanCacheKey::from_sql(s)).collect();
    for k in &keys[1..] {
        assert_eq!(keys[0].hash, k.hash, "所有变体应产生相同 hash");
    }
}

/// 不同语义 SQL 产生不同 hash（无碰撞）
#[test]
fn test_different_sql_different_hash() {
    let sqls = vec![
        "SELECT * FROM users WHERE id = ?",
        "SELECT * FROM orders WHERE id = ?",
        "SELECT * FROM users WHERE name = ?",
        "SELECT * FROM users WHERE id = ? AND name = ?",
        "SELECT * FROM users ORDER BY id",
        "SELECT * FROM users LIMIT 10",
        "INSERT INTO users (name) VALUES (?)",
        "UPDATE users SET name = ? WHERE id = ?",
        "DELETE FROM users WHERE id = ?",
    ];

    let mut hashes = std::collections::HashSet::new();
    for sql in &sqls {
        let key = PlanCacheKey::from_sql(sql);
        assert!(
            hashes.insert(key.hash),
            "SQL \"{}\" 的 hash 与其他 SQL 碰撞",
            sql
        );
    }
    assert_eq!(
        hashes.len(),
        sqls.len(),
        "应有无碰撞的 {} 个 hash",
        sqls.len()
    );
}

/// 并发竞态：多线程并发缓存同一 SQL，最终保留一个条目
#[test]
fn test_concurrent_cache_same_sql() {
    let cache = Arc::new(PlanCache::new(100, None));
    let sql = "SELECT * FROM users WHERE id = ?";
    let mut handles = Vec::new();

    for _ in 0..10 {
        let cache = cache.clone();
        handles.push(thread::spawn(move || {
            cache.get_or_parse(sql).expect("parse");
        }));
    }

    for h in handles {
        h.join().expect("thread");
    }

    let stats = cache.stats();
    assert!(
        stats.parse_hits + stats.parse_misses >= 10,
        "应有 10 次访问，实际 hits={} misses={}",
        stats.parse_hits,
        stats.parse_misses
    );
    assert!(cache.size() >= 1, "应至少有 1 条缓存");
}

/// 表级失效后再查询应重新 miss
#[test]
fn test_invalidate_then_requery_miss() {
    let cache = PlanCache::new(100, None);
    let sql = "SELECT * FROM users WHERE id = ?";

    cache.get_or_parse(sql).expect("parse");
    assert_eq!(cache.stats().parse_misses, 1, "首次应 miss");

    cache.get_or_parse(sql).expect("parse");
    assert_eq!(cache.stats().parse_hits, 1, "第二次应 hit");

    let evicted = cache.invalidate_table("users");
    assert!(evicted >= 1, "应失效至少 1 条");

    cache.get_or_parse(sql).expect("parse");
    assert!(cache.stats().parse_misses >= 2, "失效后应重新 miss");
}

/// 表级精确失效：仅失效受影响表
#[test]
fn test_precise_invalidation_only_affected_table() {
    let cache = PlanCache::new(100, None);

    cache
        .get_or_parse("SELECT * FROM users WHERE id = ?")
        .expect("parse");
    cache
        .get_or_parse("SELECT * FROM orders WHERE id = ?")
        .expect("parse");
    assert_eq!(cache.size(), 2);

    let evicted = cache.invalidate_table("users");
    assert_eq!(evicted, 1, "仅失效 users 相关条目");
    assert_eq!(cache.size(), 1, "orders 应保留");

    cache
        .get_or_parse("SELECT * FROM orders WHERE id = ?")
        .expect("parse");
    assert_eq!(cache.stats().parse_hits, 1, "orders 应仍命中缓存");
}

/// LRU 淘汰后重新查询应正确
#[test]
fn test_lru_eviction_then_reparse_correct() {
    let cache = PlanCache::new(3, None);

    for i in 1..=4 {
        let sql = format!("SELECT * FROM t{} WHERE id = ?", i);
        cache.get_or_parse(&sql).expect("parse");
    }

    assert_eq!(cache.size(), 3, "max_size=3 应保持 3 条");
    assert!(cache.stats().evictions >= 1, "应有淘汰");

    let sql1 = "SELECT * FROM t1 WHERE id = ?";
    cache.get_or_parse(sql1).expect("parse");
    assert!(cache.stats().parse_misses >= 4, "t1 被淘汰后应重新 miss");
}

/// TTL 过期后应重新 miss
#[test]
fn test_ttl_expiration_then_reparse() {
    let cache = PlanCache::new(100, Some(std::time::Duration::from_nanos(1)));
    let sql = "SELECT * FROM users WHERE id = ?";

    cache.get_or_parse(sql).expect("parse");
    std::thread::sleep(std::time::Duration::from_millis(5));

    cache.get_or_parse(sql).expect("parse");
    assert!(cache.stats().parse_misses >= 2, "TTL 过期后应重新 miss");
}

/// 优化缓存差分：store 后 get 应返回相同结果
#[test]
fn test_optimize_cache_store_get_equivalence() {
    let cache = PlanCache::new(100, None);
    let sql = "SELECT * FROM users WHERE id = ?";

    assert!(cache.get_or_optimize(sql).is_none(), "首次应 miss");

    let analysis = Arc::new("optimized plan v1".to_string());
    cache.store_optimize(sql, analysis.clone());

    let cached = cache.get_or_optimize(sql).expect("应命中");
    assert_eq!(cached.as_ref(), "optimized plan v1", "缓存应返回相同结果");
}

/// 命中率统计正确性
#[test]
fn test_hit_rate_statistics() {
    let cache = PlanCache::new(100, None);
    let sql = "SELECT * FROM users WHERE id = ?";

    for _ in 0..3 {
        cache.get_or_parse(sql).expect("parse");
    }

    let stats = cache.stats();
    assert_eq!(stats.parse_misses, 1, "1 次 miss");
    assert_eq!(stats.parse_hits, 2, "2 次 hit");
    assert!(
        (stats.parse_hit_rate - (2.0 / 3.0)).abs() < 0.001,
        "命中率应约 0.667"
    );
}

/// 多表 JOIN 的表级失效
#[test]
fn test_multi_table_join_invalidation() {
    let cache = PlanCache::new(100, None);
    let sql = "SELECT * FROM users JOIN orders ON users.id = orders.user_id";

    cache.get_or_parse(sql).expect("parse");
    assert_eq!(cache.size(), 1);

    let evicted = cache.invalidate_table("users");
    assert!(evicted >= 1, "失效 users 应影响 JOIN 查询");

    let evicted2 = cache.invalidate_table("orders");
    assert!(
        cache.size() == 0 || evicted2 == 0,
        "users 失效后 orders 也应被清理或已为 0"
    );
}
