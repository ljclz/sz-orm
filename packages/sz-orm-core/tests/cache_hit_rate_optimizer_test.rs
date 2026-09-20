use std::collections::HashSet;
use sz_orm_core::query_result_cache::*;

#[tokio::test]
async fn e2e_smart_preload_high_frequency() {
    let optimizer = CacheHitRateOptimizer::new();
    let cache = QueryResultCache::with_default();
    let patterns = vec![
        QueryPattern {
            sql_fingerprint: "SELECT * FROM users WHERE id = ?".to_string(),
            access_frequency: 100.0,
            last_access_ms: 1000,
            depends_on: HashSet::new(),
        },
        QueryPattern {
            sql_fingerprint: "SELECT * FROM orders WHERE id = ?".to_string(),
            access_frequency: 2.0,
            last_access_ms: 2000,
            depends_on: HashSet::new(),
        },
    ];
    let result = optimizer.smart_preload(&cache, &patterns).await;
    assert_eq!(result.preloaded_count, 1);
}

#[test]
fn e2e_identify_hotspots() {
    let optimizer = CacheHitRateOptimizer::new();
    let mut access_counts = std::collections::HashMap::new();
    let key1 = CacheKey::from_hashes(1, 0);
    let key2 = CacheKey::from_hashes(2, 0);
    access_counts.insert(key1.clone(), 200);
    access_counts.insert(key2.clone(), 3);
    let stats = AccessStats {
        access_counts,
        last_access: std::collections::HashMap::new(),
        total_accesses: 203,
    };
    let hotspots = optimizer.identify_hotspots(&stats);
    assert!(!hotspots.is_empty());
    assert_eq!(hotspots[0].key, key1);
}

#[test]
fn e2e_optimize_invalidation_hybrid() {
    let optimizer = CacheHitRateOptimizer::new();
    let result = optimizer.optimize_invalidation(InvalidationStrategy::Hybrid);
    assert!(result.invalid_invalidations_after < result.invalid_invalidations_before);
}

#[test]
fn e2e_compute_optimization_result_cache() {
    let optimizer = CacheHitRateOptimizer::new();
    let result = optimizer.compute_optimization_result("ResultCache", 0.70, 0.76);
    assert!(result.improvement >= 5.0);
}

#[test]
fn e2e_compute_optimization_dist_cache() {
    let optimizer = CacheHitRateOptimizer::new();
    let result = optimizer.compute_optimization_result("DistCache", 0.60, 0.64);
    assert!(result.improvement >= 3.0);
}
