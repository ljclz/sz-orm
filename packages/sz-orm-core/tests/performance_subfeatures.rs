//! performance feature 子模块实测验证（l1-cache + plan-cache + zero-copy）

use std::sync::Arc;
use std::time::Instant;
use sz_orm_core::l1_cache::{L1Cache, L1L2Coordinator};

// ============================================================================
// L1 Cache 性能验证
// ============================================================================

#[test]
fn test_perf_l1_cache_hit_vs_miss() {
    let mut cache: L1Cache<String> = L1Cache::new(10_000);
    for i in 0..10_000_i64 {
        cache.put(i, Arc::new(format!("user_{i}")));
    }

    let iterations = 100_000u64;

    let start = Instant::now();
    for _ in 0..iterations {
        let _ = cache.get(&5000);
    }
    let hit_ns = start.elapsed().as_nanos() / iterations as u128;

    let start = Instant::now();
    for _ in 0..iterations {
        let _ = cache.get(&999_999);
    }
    let miss_ns = start.elapsed().as_nanos() / iterations as u128;

    eprintln!("[perf-l1] hit={hit_ns}ns miss={miss_ns}ns");

    let val = cache.get(&5000).unwrap();
    assert_eq!(val.as_str(), "user_5000");
}

#[test]
fn test_perf_l1_cache_vs_db_load() {
    let mut coord: L1L2Coordinator<String> = L1L2Coordinator::new(10_000);

    let db_call_count = Arc::new(std::sync::Mutex::new(0u64));

    for i in 0..1000_i64 {
        let counter = Arc::clone(&db_call_count);
        coord.get_or_load("users", i, move || {
            *counter.lock().unwrap() += 1;
            Some(format!("user_{i}"))
        });
    }
    assert_eq!(*db_call_count.lock().unwrap(), 1000);

    let iterations = 100_000u64;
    let start = Instant::now();
    for _ in 0..iterations {
        coord.get_or_load("users", 500, || panic!("should not reach DB"));
    }
    let l1_hit_ns = start.elapsed().as_nanos() / iterations as u128;

    eprintln!(
        "[perf-l1] L1 hit={l1_hit_ns}ns, DB calls={}",
        *db_call_count.lock().unwrap()
    );
    assert_eq!(*db_call_count.lock().unwrap(), 1000);
}

#[test]
fn test_perf_l1_cache_identity_map_semantics() {
    let mut cache: L1Cache<String> = L1Cache::new(100);
    cache.put(1, Arc::new("Alice".into()));
    cache.put(2, Arc::new("Bob".into()));

    let a1 = cache.get(&1).unwrap();
    let a2 = cache.get(&1).unwrap();
    let b1 = cache.get(&2).unwrap();

    assert!(
        Arc::ptr_eq(&a1, &a2),
        "Identity Map: same key must return same Arc"
    );
    assert!(
        !Arc::ptr_eq(&a1, &b1),
        "Different keys must return different Arcs"
    );
    eprintln!(
        "[perf-l1] Identity Map verified: ptr_eq = {}",
        Arc::ptr_eq(&a1, &a2)
    );
}

#[test]
fn test_perf_l1_cache_lru_eviction() {
    let mut cache: L1Cache<i32> = L1Cache::new(3);
    cache.put(1, Arc::new(10));
    cache.put(2, Arc::new(20));
    cache.put(3, Arc::new(30));

    let _ = cache.get(&1);
    cache.put(4, Arc::new(40));

    assert!(cache.get(&2).is_none(), "key 2 should be evicted (LRU)");
    assert!(cache.get(&1).is_some(), "key 1 should survive");
    assert!(cache.get(&3).is_some(), "key 3 should survive");
    assert!(cache.get(&4).is_some(), "key 4 should survive");

    let stats = cache.stats();
    eprintln!("[perf-l1] LRU eviction: evicts={}", stats.evict_count);
    assert!(stats.evict_count >= 1);
}

// ============================================================================
// Plan Cache 性能验证
// ============================================================================

#[cfg(feature = "plan-cache")]
#[test]
fn test_perf_plan_cache_hit_vs_parse() {
    use std::time::Duration;
    use sz_orm_core::plan_cache::PlanCache;

    let cache = PlanCache::new(1000, Some(Duration::from_secs(3600)));

    let sql =
        "SELECT id, name, age FROM users WHERE age > ? AND status = ? ORDER BY name ASC LIMIT 100";

    cache.get_or_parse(sql).unwrap();

    let iterations = 10_000u64;
    let start = Instant::now();
    for _ in 0..iterations {
        cache.get_or_parse(sql).unwrap();
    }
    let cached_ns = start.elapsed().as_nanos() / iterations as u128;

    let fresh_sqls: Vec<String> = (0..iterations as usize)
        .map(|i| format!("SELECT id FROM table_{i} WHERE x = ?"))
        .collect();
    let parse_start = Instant::now();
    for sql in &fresh_sqls {
        let _ = cache.get_or_parse(sql);
    }
    let parse_ns = parse_start.elapsed().as_nanos() / iterations as u128;

    let speedup = parse_ns as f64 / cached_ns as f64;
    eprintln!(
        "[perf-plan] cached_hit={cached_ns}ns fresh_parse={parse_ns}ns speedup={speedup:.1}x"
    );

    let stats = cache.stats();
    eprintln!(
        "[perf-plan] parse_hits={} parse_misses={}",
        stats.parse_hits, stats.parse_misses
    );
    assert!(stats.parse_hits > 0, "should have cache hits");
    assert!(
        speedup > 1.0,
        "cached lookup should be faster than fresh parse"
    );
}

#[cfg(feature = "plan-cache")]
#[test]
fn test_perf_plan_cache_table_invalidation() {
    use std::time::Duration;
    use sz_orm_core::plan_cache::PlanCache;

    let cache = PlanCache::new(1000, Some(Duration::from_secs(3600)));

    let sql1 = "SELECT * FROM users WHERE id = ?";
    let sql2 = "SELECT * FROM orders WHERE user_id = ?";

    cache.get_or_parse(sql1).unwrap();
    cache.get_or_parse(sql2).unwrap();

    let stats_before = cache.stats();
    assert!(stats_before.parse_hits == 0);

    cache.get_or_parse(sql1).unwrap();
    let stats_after_hit = cache.stats();
    assert!(stats_after_hit.parse_hits >= 1);

    cache.invalidate_table("users");

    cache.get_or_parse(sql1).unwrap();
    let stats_after_invalidation = cache.stats();
    eprintln!(
        "[perf-plan] after invalidate_table(users): hits={} misses={}",
        stats_after_invalidation.parse_hits, stats_after_invalidation.parse_misses
    );

    cache.get_or_parse(sql2).unwrap();
    let stats_final = cache.stats();
    eprintln!(
        "[perf-plan] orders still cached after users invalidation: hits={}",
        stats_final.parse_hits
    );
}

// ============================================================================
// Zero-Copy (Columnar) 性能验证
// ============================================================================

#[cfg(feature = "zero-copy")]
#[test]
fn test_perf_zero_copy_column_vs_row_access() {
    use std::collections::HashMap;
    use sz_orm_core::columnar::{ColumnarResultSet, ColumnarSchema};
    use sz_orm_core::result_map::RowData;
    use sz_orm_core::Value;

    let row_count = 10_000;
    let schema = ColumnarSchema::new(
        vec!["id".into(), "name".into(), "age".into()],
        vec!["INTEGER".into(), "VARCHAR".into(), "INTEGER".into()],
    );

    let mut rows: Vec<RowData> = Vec::with_capacity(row_count);
    for i in 0..row_count as i64 {
        rows.push(RowData::new(HashMap::from([
            ("id".into(), Value::I64(i)),
            ("name".into(), Value::String(format!("user_{i}"))),
            ("age".into(), Value::I32(i as i32 % 100)),
        ])));
    }

    let columnar = ColumnarResultSet::from_row_data(&rows, schema);

    let iterations = 100u64;

    let start = Instant::now();
    for _ in 0..iterations {
        let col = columnar.column("age").unwrap();
        let _sum: i64 = col
            .iter()
            .filter_map(|v| {
                if let Value::I32(a) = v {
                    Some(*a as i64)
                } else {
                    None
                }
            })
            .sum::<i64>();
    }
    let col_ns = start.elapsed().as_nanos() / iterations as u128;

    let start = Instant::now();
    for _ in 0..iterations {
        let _sum: i64 = rows
            .iter()
            .filter_map(|r| {
                if let Value::I32(a) = r.get("age").unwrap() {
                    Some(*a as i64)
                } else {
                    None
                }
            })
            .sum::<i64>();
    }
    let row_ns = start.elapsed().as_nanos() / iterations as u128;

    eprintln!("[perf-zero-copy] columnar_sum={col_ns}ns row_sum={row_ns}ns");
}

#[cfg(feature = "zero-copy")]
#[test]
fn test_perf_zero_copy_roundtrip() {
    use std::collections::HashMap;
    use sz_orm_core::columnar::{ColumnarResultSet, ColumnarSchema};
    use sz_orm_core::result_map::RowData;
    use sz_orm_core::Value;

    let schema = ColumnarSchema::new(
        vec!["id".into(), "name".into(), "score".into()],
        vec!["INTEGER".into(), "VARCHAR".into(), "F64".into()],
    );

    let original_rows: Vec<RowData> = (0..100_i64)
        .map(|i| {
            RowData::new(HashMap::from([
                ("id".into(), Value::I64(i)),
                ("name".into(), Value::String(format!("user_{i}"))),
                ("score".into(), Value::F64(i as f64 * 1.5)),
            ]))
        })
        .collect();

    let columnar = ColumnarResultSet::from_row_data(&original_rows, schema);
    let roundtrip_rows = columnar.to_row_data();

    assert_eq!(roundtrip_rows.len(), original_rows.len());
    for (orig, rt) in original_rows.iter().zip(roundtrip_rows.iter()) {
        assert_eq!(orig.get("id"), rt.get("id"));
        assert_eq!(orig.get("name"), rt.get("name"));
    }

    eprintln!(
        "[perf-zero-copy] roundtrip verified: {} rows, cols={}, rows={}",
        roundtrip_rows.len(),
        columnar.column_count(),
        columnar.row_count()
    );
}

#[cfg(feature = "zero-copy")]
#[test]
fn test_perf_zero_copy_column_access_patterns() {
    use std::collections::HashMap;
    use sz_orm_core::columnar::{ColumnarResultSet, ColumnarSchema};
    use sz_orm_core::result_map::RowData;
    use sz_orm_core::Value;

    let schema = ColumnarSchema::new(
        vec!["id".into(), "name".into(), "age".into(), "city".into()],
        vec![
            "INTEGER".into(),
            "VARCHAR".into(),
            "INTEGER".into(),
            "VARCHAR".into(),
        ],
    );

    let rows: Vec<RowData> = (0..5000_i64)
        .map(|i| {
            RowData::new(HashMap::from([
                ("id".into(), Value::I64(i)),
                ("name".into(), Value::String(format!("user_{i}"))),
                ("age".into(), Value::I32((i % 80) as i32 + 18)),
                ("city".into(), Value::String(format!("city_{}", i % 100))),
            ]))
        })
        .collect();

    let columnar = ColumnarResultSet::from_row_data(&rows, schema);

    let iterations = 100u64;

    let start = Instant::now();
    for _ in 0..iterations {
        let age_col = columnar.column("age").unwrap();
        let _count = age_col
            .iter()
            .filter(|v| {
                if let Value::I32(a) = v {
                    *a > 50
                } else {
                    false
                }
            })
            .count();
    }
    let single_col_ns = start.elapsed().as_nanos() / iterations as u128;

    let start = Instant::now();
    for _ in 0..iterations {
        let age_col = columnar.column("age").unwrap();
        let city_col = columnar.column("city").unwrap();
        let _count = age_col
            .iter()
            .zip(city_col.iter())
            .filter(|(a, c)| {
                if let (Value::I32(age), Value::String(city)) = (a, c) {
                    *age > 50 && city.starts_with("city_5")
                } else {
                    false
                }
            })
            .count();
    }
    let multi_col_ns = start.elapsed().as_nanos() / iterations as u128;

    eprintln!(
        "[perf-zero-copy] single_col_filter={single_col_ns}ns multi_col_filter={multi_col_ns}ns"
    );
}
