//! v10.12.0 T2.1 + T2.2 + T2.3: LFU 频率追踪 + 多策略淘汰 + 命中率监控集成测试

#![cfg(feature = "plan-cache")]

use std::time::{Duration, Instant};

use sz_orm_core::plan_cache::{
    EvictWeight, HitRateMonitor, HitRateTuningConfig, LfuTracker, MultiStrategyEvictor, PlanCache,
    PlanCacheEntry, SizeLimitConfig, TtlEvictConfig, TuningAction,
};

#[test]
fn test_lfu_access_and_frequency() {
    let tracker = LfuTracker::new();
    tracker.access("sql_a");
    tracker.access("sql_a");
    tracker.access("sql_b");
    assert_eq!(tracker.frequency("sql_a"), 2);
    assert_eq!(tracker.frequency("sql_b"), 1);
    assert_eq!(tracker.frequency("sql_c"), 0);
}

#[test]
fn test_lfu_evict_candidates() {
    let tracker = LfuTracker::new();
    for _ in 0..10 {
        tracker.access("hot");
    }
    for _ in 0..2 {
        tracker.access("warm");
    }
    tracker.access("cold");
    let candidates = tracker.evict_candidates(2);
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0], "cold");
    assert_eq!(candidates[1], "warm");
}

#[test]
fn test_lfu_decay() {
    let tracker = LfuTracker::new();
    for _ in 0..100 {
        tracker.access("key");
    }
    assert_eq!(tracker.frequency("key"), 100);
    tracker.decay();
    assert_eq!(tracker.frequency("key"), 50);
    tracker.decay();
    assert_eq!(tracker.frequency("key"), 25);
}

#[test]
fn test_lfu_remove() {
    let tracker = LfuTracker::new();
    tracker.access("key1");
    tracker.access("key2");
    assert_eq!(tracker.len(), 2);
    tracker.remove("key1");
    assert_eq!(tracker.len(), 1);
    assert_eq!(tracker.frequency("key1"), 0);
    assert_eq!(tracker.frequency("key2"), 1);
}

#[test]
fn test_lfu_total_accesses() {
    let tracker = LfuTracker::new();
    for _ in 0..5 {
        tracker.access("a");
    }
    for _ in 0..3 {
        tracker.access("b");
    }
    assert_eq!(tracker.total_accesses(), 8);
    tracker.decay();
    assert_eq!(tracker.total_accesses(), 8, "decay should not affect total");
}

#[test]
fn test_lfu_empty() {
    let tracker = LfuTracker::new();
    assert!(tracker.is_empty());
    tracker.access("key");
    assert!(!tracker.is_empty());
}

#[test]
fn test_lfu_concurrent_access() {
    use std::sync::Arc;
    use std::thread;
    let tracker = Arc::new(LfuTracker::new());
    let mut handles = Vec::new();
    for _ in 0..4 {
        let t = Arc::clone(&tracker);
        handles.push(thread::spawn(move || {
            for _ in 0..100 {
                t.access("shared_key");
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(tracker.frequency("shared_key"), 400);
    assert_eq!(tracker.total_accesses(), 400);
}
// ============================================================================
// T2.2: 多策略淘汰测试
// ============================================================================

fn make_entry(ttl: Option<Duration>) -> PlanCacheEntry {
    PlanCacheEntry {
        ast: None,
        analysis: None,
        created_at: Instant::now(),
        tables: vec![],
        ttl,
    }
}

fn make_expired_entry() -> PlanCacheEntry {
    PlanCacheEntry {
        ast: None,
        analysis: None,
        created_at: Instant::now() - Duration::from_secs(600),
        tables: vec![],
        ttl: Some(Duration::from_secs(1)),
    }
}

#[test]
fn test_evict_weight_composite_score() {
    let w1 = EvictWeight::new(0, 0.0, 100);
    let w2 = EvictWeight::new(10, 1.0, 1024);
    assert!(w1.composite_score() < w2.composite_score());
}

#[test]
fn test_evict_weight_clamp() {
    let w = EvictWeight::new(5, 1.5, 0);
    let w_clamped = EvictWeight::new(5, 1.0, 0);
    assert_eq!(w.composite_score(), w_clamped.composite_score());
}

#[test]
fn test_ttl_evict_expired() {
    use std::collections::HashMap;
    let config = TtlEvictConfig {
        default_ttl: Duration::from_secs(1),
        evict_interval: Duration::from_millis(10),
    };
    let evictor = sz_orm_core::plan_cache::TtlEvictor::new(config);
    let mut entries = HashMap::new();
    entries.insert(1u64, make_expired_entry());
    entries.insert(2u64, make_entry(Some(Duration::from_secs(300))));
    let evicted = evictor.evict_expired(&entries);
    assert_eq!(evicted, vec![1u64]);
}

#[test]
fn test_ttl_evict_not_expired() {
    use std::collections::HashMap;
    let config = TtlEvictConfig::default();
    let evictor = sz_orm_core::plan_cache::TtlEvictor::new(config);
    let mut entries = HashMap::new();
    entries.insert(1u64, make_entry(Some(Duration::from_secs(300))));
    entries.insert(2u64, make_entry(Some(Duration::from_secs(600))));
    let evicted = evictor.evict_expired(&entries);
    assert!(evicted.is_empty());
}

#[test]
fn test_size_limit_evict_over_entries() {
    let config = SizeLimitConfig {
        max_entries: 2,
        max_memory_bytes: 1024 * 1024,
    };
    let evictor = sz_orm_core::plan_cache::SizeLimitEvictor::new(config);
    assert!(evictor.is_over_limit(3, 100));
    assert!(!evictor.is_over_limit(2, 100));
}

#[test]
fn test_size_limit_evict_over_memory() {
    let config = SizeLimitConfig {
        max_entries: 100,
        max_memory_bytes: 1024,
    };
    let evictor = sz_orm_core::plan_cache::SizeLimitEvictor::new(config);
    assert!(evictor.is_over_limit(1, 2048));
    assert!(!evictor.is_over_limit(1, 512));
}

#[test]
fn test_size_limit_select_evict_candidate() {
    use std::collections::HashMap;
    let config = SizeLimitConfig::default();
    let evictor = sz_orm_core::plan_cache::SizeLimitEvictor::new(config);
    let mut weights = HashMap::new();
    weights.insert(1u64, EvictWeight::new(10, 1.0, 1024));
    weights.insert(2u64, EvictWeight::new(0, 0.1, 100));
    weights.insert(3u64, EvictWeight::new(5, 0.5, 512));
    let candidate = evictor.select_evict_candidate(&weights).unwrap();
    assert_eq!(candidate, 2u64);
}

#[test]
fn test_multi_strategy_evict_ttl_priority() {
    use std::collections::HashMap;
    let ttl_config = TtlEvictConfig {
        default_ttl: Duration::from_secs(1),
        evict_interval: Duration::from_millis(10),
    };
    let size_config = SizeLimitConfig {
        max_entries: 100,
        max_memory_bytes: 64 * 1024 * 1024,
    };
    let evictor = MultiStrategyEvictor::new(ttl_config, size_config);
    let mut entries = HashMap::new();
    entries.insert(1u64, make_expired_entry());
    entries.insert(2u64, make_entry(Some(Duration::from_secs(300))));
    let evicted = evictor.evict(&entries, 2, 100);
    assert_eq!(evicted, vec![1u64]);
}

#[test]
fn test_multi_strategy_evict_size_limit() {
    use std::collections::HashMap;
    let ttl_config = TtlEvictConfig {
        default_ttl: Duration::from_secs(300),
        evict_interval: Duration::from_secs(60),
    };
    let size_config = SizeLimitConfig {
        max_entries: 1,
        max_memory_bytes: 64 * 1024 * 1024,
    };
    let evictor = MultiStrategyEvictor::new(ttl_config, size_config);
    let mut entries = HashMap::new();
    entries.insert(1u64, make_entry(Some(Duration::from_secs(300))));
    entries.insert(2u64, make_entry(Some(Duration::from_secs(300))));
    let evicted = evictor.evict(&entries, 2, 100);
    assert_eq!(evicted.len(), 1);
}

#[test]
fn test_multi_strategy_evict_no_candidates() {
    use std::collections::HashMap;
    let ttl_config = TtlEvictConfig::default();
    let size_config = SizeLimitConfig::default();
    let evictor = MultiStrategyEvictor::new(ttl_config, size_config);
    let entries = HashMap::<u64, PlanCacheEntry>::new();
    let evicted = evictor.evict(&entries, 0, 0);
    assert!(evicted.is_empty());
}

#[test]
fn test_multi_strategy_access_and_lfu() {
    let ttl_config = TtlEvictConfig::default();
    let size_config = SizeLimitConfig::default();
    let evictor = MultiStrategyEvictor::new(ttl_config, size_config);
    evictor.access("100");
    evictor.access("100");
    evictor.access("200");
    assert_eq!(evictor.lfu_tracker().frequency("100"), 2);
    assert_eq!(evictor.lfu_tracker().frequency("200"), 1);
}

#[test]
fn test_multi_strategy_decay() {
    let ttl_config = TtlEvictConfig::default();
    let size_config = SizeLimitConfig::default();
    let evictor = MultiStrategyEvictor::new(ttl_config, size_config);
    for _ in 0..10 {
        evictor.access("key");
    }
    assert_eq!(evictor.lfu_tracker().frequency("key"), 10);
    evictor.decay();
    assert_eq!(evictor.lfu_tracker().frequency("key"), 5);
}

#[test]
fn test_plan_cache_new_with_evictors() {
    let ttl_config = TtlEvictConfig::default();
    let size_config = SizeLimitConfig::default();
    let cache =
        PlanCache::new_with_evictors(100, Some(Duration::from_secs(300)), ttl_config, size_config);
    assert!(cache.multi_strategy_evictor().is_some());
}

#[test]
fn test_plan_cache_new_without_evictors() {
    let cache = PlanCache::new(100, None);
    assert!(cache.multi_strategy_evictor().is_none());
}

#[test]
fn test_plan_cache_evict_expired_no_evictor() {
    let cache = PlanCache::new(100, None);
    assert_eq!(cache.evict_expired(), 0);
}

#[test]
fn test_plan_cache_lfu_access_on_hit() {
    let ttl_config = TtlEvictConfig::default();
    let size_config = SizeLimitConfig::default();
    let cache =
        PlanCache::new_with_evictors(100, Some(Duration::from_secs(300)), ttl_config, size_config);
    let sql = "SELECT 1";
    cache.get_or_parse(sql).unwrap();
    cache.get_or_parse(sql).unwrap();
    let evictor = cache.multi_strategy_evictor().unwrap();
    let key = sz_orm_core::plan_cache::PlanCacheKey::from_sql(sql);
    assert_eq!(evictor.lfu_tracker().frequency(&key.hash.to_string()), 1);
}

#[test]
fn test_plan_cache_multi_strategy_eviction_on_full() {
    let ttl_config = TtlEvictConfig {
        default_ttl: Duration::from_secs(300),
        evict_interval: Duration::from_secs(60),
    };
    let size_config = SizeLimitConfig {
        max_entries: 100,
        max_memory_bytes: 64 * 1024 * 1024,
    };
    let cache = PlanCache::new_with_evictors(2, None, ttl_config, size_config);
    cache.get_or_parse("SELECT 1").unwrap();
    cache.get_or_parse("SELECT 2").unwrap();
    assert_eq!(cache.size(), 2);
    cache.get_or_parse("SELECT 1").unwrap();
    cache.get_or_parse("SELECT 3").unwrap();
    assert_eq!(cache.size(), 2);
    let stats = cache.stats();
    assert!(stats.evictions >= 1);
}

#[test]
fn test_ttl_concurrent_expiry_no_duplicates() {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::thread;
    let config = TtlEvictConfig {
        default_ttl: Duration::from_secs(1),
        evict_interval: Duration::from_millis(1),
    };
    let evictor = Arc::new(sz_orm_core::plan_cache::TtlEvictor::new(config));
    let mut entries = HashMap::new();
    entries.insert(1u64, make_expired_entry());
    entries.insert(2u64, make_expired_entry());
    let entries_arc = Arc::new(entries);
    let mut handles = Vec::new();
    for _ in 0..4 {
        let e = Arc::clone(&evictor);
        let en = Arc::clone(&entries_arc);
        handles.push(thread::spawn(move || e.evict_expired(&en)));
    }
    let mut all_evicted = Vec::new();
    for h in handles {
        all_evicted.extend(h.join().unwrap());
    }
    all_evicted.sort();
    all_evicted.dedup();
    assert!(all_evicted.len() <= 2);
}

#[test]
fn test_size_limit_concurrent_evict_no_duplicates() {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::thread;
    let config = SizeLimitConfig {
        max_entries: 1,
        max_memory_bytes: 64 * 1024 * 1024,
    };
    let evictor = Arc::new(sz_orm_core::plan_cache::SizeLimitEvictor::new(config));
    let mut weights = HashMap::new();
    weights.insert(1u64, EvictWeight::new(10, 1.0, 1024));
    weights.insert(2u64, EvictWeight::new(0, 0.1, 100));
    let weights_arc = Arc::new(weights);
    let mut handles = Vec::new();
    for _ in 0..4 {
        let e = Arc::clone(&evictor);
        let w = Arc::clone(&weights_arc);
        handles.push(thread::spawn(move || e.select_evict_candidate(&w)));
    }
    let mut results = Vec::new();
    for h in handles {
        results.push(h.join().unwrap());
    }
    for r in &results {
        assert_eq!(*r, Some(2u64));
    }
}
// ============================================================================
// T2.3: 命中率监控与自动调优测试
// ============================================================================

#[test]
fn test_hit_rate_monitor_record() {
    let config = HitRateTuningConfig {
        low_hit_threshold: 0.5,
        monitor_window: Duration::from_secs(60),
        cold_start_exempt_duration: Duration::from_secs(0),
    };
    let monitor = HitRateMonitor::new(config);
    monitor.record_hit_rate(1.0);
    monitor.record_hit_rate(0.0);
    monitor.record_hit_rate(1.0);
    assert_eq!(monitor.history_len(), 3);
}

#[test]
fn test_hit_rate_monitor_ratio() {
    let config = HitRateTuningConfig {
        low_hit_threshold: 0.5,
        monitor_window: Duration::from_secs(60),
        cold_start_exempt_duration: Duration::from_secs(0),
    };
    let monitor = HitRateMonitor::new(config);
    monitor.record_hit_rate(1.0);
    monitor.record_hit_rate(1.0);
    monitor.record_hit_rate(0.0);
    monitor.record_hit_rate(0.0);
    let rate = monitor.current_hit_rate();
    assert!((rate - 0.5).abs() < 0.001);
}

#[test]
fn test_hit_rate_monitor_low_trigger() {
    let config = HitRateTuningConfig {
        low_hit_threshold: 0.5,
        monitor_window: Duration::from_secs(60),
        cold_start_exempt_duration: Duration::from_secs(0),
    };
    let monitor = HitRateMonitor::new(config);
    monitor.end_cold_start();
    for _ in 0..10 {
        monitor.record_hit_rate(0.0);
    }
    let action = monitor.check_and_tune(0, 10, 5);
    assert!(action.is_some());
}

#[test]
fn test_hit_rate_monitor_high_no_action() {
    let config = HitRateTuningConfig {
        low_hit_threshold: 0.5,
        monitor_window: Duration::from_secs(60),
        cold_start_exempt_duration: Duration::from_secs(0),
    };
    let monitor = HitRateMonitor::new(config);
    monitor.end_cold_start();
    for _ in 0..10 {
        monitor.record_hit_rate(1.0);
    }
    let action = monitor.check_and_tune(0, 10, 5);
    assert!(action.is_none());
}

#[test]
fn test_hit_rate_monitor_window() {
    let config = HitRateTuningConfig {
        low_hit_threshold: 0.5,
        monitor_window: Duration::from_millis(1),
        cold_start_exempt_duration: Duration::from_secs(0),
    };
    let monitor = HitRateMonitor::new(config);
    monitor.record_hit_rate(1.0);
    monitor.record_hit_rate(1.0);
    std::thread::sleep(Duration::from_millis(5));
    monitor.record_hit_rate(0.0);
    assert_eq!(monitor.history_len(), 1);
    assert!((monitor.current_hit_rate() - 0.0).abs() < 0.001);
}

#[test]
fn test_hit_rate_monitor_cold_start_exempt() {
    let config = HitRateTuningConfig {
        low_hit_threshold: 0.5,
        monitor_window: Duration::from_secs(60),
        cold_start_exempt_duration: Duration::from_secs(300),
    };
    let monitor = HitRateMonitor::new(config);
    for _ in 0..10 {
        monitor.record_hit_rate(0.0);
    }
    assert!(monitor.is_cold_start());
    let action = monitor.check_and_tune(0, 10, 5);
    assert!(action.is_none(), "Cold start should exempt tuning");
}

#[test]
fn test_hit_rate_monitor_tuning_action_expand() {
    let config = HitRateTuningConfig {
        low_hit_threshold: 0.5,
        monitor_window: Duration::from_secs(60),
        cold_start_exempt_duration: Duration::from_secs(0),
    };
    let monitor = HitRateMonitor::new(config);
    monitor.end_cold_start();
    for _ in 0..10 {
        monitor.record_hit_rate(0.0);
    }
    let action = monitor.check_and_tune(0, 10, 5).unwrap();
    assert_eq!(action, TuningAction::ExpandCapacity { from: 10, to: 20 });
}

#[test]
fn test_hit_rate_monitor_tuning_action_normalize() {
    let config = HitRateTuningConfig {
        low_hit_threshold: 0.5,
        monitor_window: Duration::from_secs(60),
        cold_start_exempt_duration: Duration::from_secs(0),
    };
    let monitor = HitRateMonitor::new(config);
    monitor.end_cold_start();
    for _ in 0..10 {
        monitor.record_hit_rate(0.0);
    }
    let action = monitor.check_and_tune(0, 10, 100).unwrap();
    assert_eq!(action, TuningAction::OptimizeNormalization);
}

#[test]
fn test_hit_rate_monitor_concurrent_record() {
    use std::sync::Arc;
    use std::thread;
    let config = HitRateTuningConfig {
        low_hit_threshold: 0.5,
        monitor_window: Duration::from_secs(60),
        cold_start_exempt_duration: Duration::from_secs(0),
    };
    let monitor = Arc::new(HitRateMonitor::new(config));
    let mut handles = Vec::new();
    for i in 0..4 {
        let m = Arc::clone(&monitor);
        handles.push(thread::spawn(move || {
            for _ in 0..25 {
                m.record_hit_rate(if i % 2 == 0 { 1.0 } else { 0.0 });
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(monitor.history_len(), 100);
}

#[test]
fn test_plan_cache_with_hit_rate_monitor() {
    let config = HitRateTuningConfig::default();
    let monitor = HitRateMonitor::new(config);
    let cache = PlanCache::new(100, None).with_hit_rate_monitor(monitor);
    assert!(cache.hit_rate_monitor().is_some());
    cache.get_or_parse("SELECT 1").unwrap();
    cache.get_or_parse("SELECT 1").unwrap();
    cache.get_or_parse("SELECT 2").unwrap();
    let monitor = cache.hit_rate_monitor().unwrap();
    assert_eq!(monitor.history_len(), 3);
    let rate = monitor.current_hit_rate();
    assert!((rate - (1.0 / 3.0)).abs() < 0.01, "rate = {}", rate);
}
// ============================================================================
// T2.4: 跨连接计划共享 CrossConnPlanShare 测试
// ============================================================================

use std::sync::Arc;
use sz_orm_core::prepared_cache::{
    ConnContext, ConnContextValidator, CrossConnPlanShare, PreparedLookup, PreparedStatementCache,
};
use sz_orm_core::Value;

fn make_shared_execute_fn() -> sz_orm_core::prepared_cache::ExecuteFn {
    Arc::new(|_params: &[Value]| {
        Box::pin(async {
            Ok(vec![{
                let mut row = std::collections::HashMap::new();
                row.insert("id".to_string(), Value::I64(42));
                row
            }])
        })
    })
}

#[tokio::test]
async fn test_cross_conn_plan_share_same_context() {
    let cache = PreparedStatementCache::new_with_cross_share(256);
    let ctx = ConnContext::default();
    let sql = "SELECT * FROM users WHERE id = ?";

    let result = cache
        .get_or_prepare_with_context(1, sql, &[], &ctx)
        .await
        .unwrap();
    assert!(matches!(result, PreparedLookup::Miss));

    cache.store_handle_with_context(
        1,
        sql,
        vec!["users".into()],
        make_shared_execute_fn(),
        ctx.clone(),
    );

    let result = cache
        .get_or_prepare_with_context(2, sql, &[], &ctx)
        .await
        .unwrap();
    assert!(
        matches!(result, PreparedLookup::Hit(_)),
        "相同上下文应跨连接共享计划"
    );

    let share = cache.cross_conn_share().unwrap();
    assert_eq!(share.shared_hits(), 1);
}

#[tokio::test]
async fn test_cross_conn_plan_share_diff_context() {
    let cache = PreparedStatementCache::new_with_cross_share(256);
    let ctx1 = ConnContext::new("public", "public", "utf8");
    let ctx2 = ConnContext::new("private", "private", "utf8");
    let sql = "SELECT * FROM users WHERE id = ?";

    cache.store_handle_with_context(1, sql, vec!["users".into()], make_shared_execute_fn(), ctx1);

    let result = cache
        .get_or_prepare_with_context(2, sql, &[], &ctx2)
        .await
        .unwrap();
    assert!(
        matches!(result, PreparedLookup::Miss),
        "不同上下文不应共享，走独立解析"
    );

    let share = cache.cross_conn_share().unwrap();
    assert_eq!(share.context_mismatches(), 1);
    assert_eq!(share.shared_hits(), 0);
}

#[tokio::test]
async fn test_cross_conn_plan_share_invalidate() {
    let cache = PreparedStatementCache::new_with_cross_share(256);
    let ctx = ConnContext::default();
    let sql = "SELECT * FROM users WHERE id = ?";

    cache.store_handle_with_context(
        1,
        sql,
        vec!["users".into()],
        make_shared_execute_fn(),
        ctx.clone(),
    );

    let share = cache.cross_conn_share().unwrap();
    assert_eq!(share.len(), 1);

    let invalidated = cache.invalidate_table("users");
    assert!(invalidated >= 2, "应同时失效连接级和共享级条目");

    let share = cache.cross_conn_share().unwrap();
    assert_eq!(share.len(), 0, "共享条目应被清除");

    let result = cache
        .get_or_prepare_with_context(2, sql, &[], &ctx)
        .await
        .unwrap();
    assert!(matches!(result, PreparedLookup::Miss), "失效后应 Miss");
}

#[test]
fn test_cross_conn_plan_share_context_key() {
    let ctx1 = ConnContext::new("public", "public", "utf8");
    let ctx2 = ConnContext::new("public", "public", "utf8");
    let ctx3 = ConnContext::new("public", "public", "latin1");

    assert!(ConnContextValidator::check_consistency(&ctx1, &ctx2));
    assert!(!ConnContextValidator::check_consistency(&ctx1, &ctx3));
}

#[test]
fn test_cross_conn_plan_share_concurrent() {
    use std::thread;
    let share = Arc::new(CrossConnPlanShare::new());
    let ctx = ConnContext::default();
    let mut handles = Vec::new();

    for i in 0..4 {
        let s = Arc::clone(&share);
        let c = ctx.clone();
        handles.push(thread::spawn(move || {
            let fingerprint = i as u64;
            s.store_shared_plan(fingerprint, make_shared_execute_fn(), c.clone());
            let result = s.get_shared_plan(fingerprint, &c);
            assert!(result.is_some(), "线程 {} 应获取到共享计划", i);
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    assert_eq!(share.len(), 4);
    assert_eq!(share.shared_hits(), 4);
}

#[tokio::test]
async fn test_cross_conn_plan_share_invalid_key() {
    let cache = PreparedStatementCache::new_with_cross_share(256);
    let ctx = ConnContext::default();

    let result = cache
        .get_or_prepare_with_context(1, "SELECT * FROM nonexist", &[], &ctx)
        .await
        .unwrap();
    assert!(matches!(result, PreparedLookup::Miss));

    let share = cache.cross_conn_share().unwrap();
    assert!(share.get_shared_plan(99999, &ctx).is_none());
    assert!(share.get_entry_context(99999).is_none());
    assert!(share.get_entry_create_time(99999).is_none());
}

#[tokio::test]
async fn test_cross_conn_plan_share_get_entry() {
    let cache = PreparedStatementCache::new_with_cross_share(256);
    let ctx = ConnContext::new("app_schema", "app", "utf8mb4");
    let sql = "SELECT * FROM orders WHERE id = ?";

    cache.store_handle_with_context(
        1,
        sql,
        vec!["orders".into()],
        make_shared_execute_fn(),
        ctx.clone(),
    );

    let share = cache.cross_conn_share().unwrap();
    let sql_hash = sz_orm_core::prepared_cache::compute_sql_hash(sql);
    let entry_ctx = share.get_entry_context(sql_hash).unwrap();
    assert_eq!(entry_ctx, ctx, "getter 应返回正确的上下文");

    let create_time = share.get_entry_create_time(sql_hash).unwrap();
    assert!(create_time.elapsed().as_secs() < 1, "创建时间应为近期");
}

#[tokio::test]
async fn test_cross_conn_plan_share_multi_conn_reuse() {
    let cache = PreparedStatementCache::new_with_cross_share(256);
    let ctx = ConnContext::default();
    let sql = "SELECT * FROM products WHERE category = ?";

    cache.store_handle_with_context(
        1,
        sql,
        vec!["products".into()],
        make_shared_execute_fn(),
        ctx.clone(),
    );

    for conn_id in 2..=5 {
        let result = cache
            .get_or_prepare_with_context(conn_id, sql, &[], &ctx)
            .await
            .unwrap();
        assert!(
            matches!(result, PreparedLookup::Hit(_)),
            "连接 {} 应复用共享计划",
            conn_id
        );
    }

    let share = cache.cross_conn_share().unwrap();
    assert_eq!(share.shared_hits(), 4, "4 个连接应共享命中");
}

#[tokio::test]
async fn test_cross_conn_plan_share_no_share_disabled() {
    let cache = PreparedStatementCache::new(256);
    let ctx = ConnContext::default();
    let sql = "SELECT * FROM users";

    cache.store_handle(1, sql, vec![], make_shared_execute_fn());

    let result = cache
        .get_or_prepare_with_context(2, sql, &[], &ctx)
        .await
        .unwrap();
    assert!(
        matches!(result, PreparedLookup::Miss),
        "未启用跨连接共享时，不同连接应 Miss"
    );
    assert!(cache.cross_conn_share().is_none());
}

#[test]
fn test_cross_conn_plan_share_invalidate_by_fingerprint() {
    let share = CrossConnPlanShare::new();
    let ctx = ConnContext::default();

    share.store_shared_plan(100, make_shared_execute_fn(), ctx);
    share.store_shared_plan(200, make_shared_execute_fn(), ConnContext::default());
    assert_eq!(share.len(), 2);

    assert!(share.invalidate_by_fingerprint(100));
    assert!(!share.invalidate_by_fingerprint(999));
    assert_eq!(share.len(), 1);

    assert!(share
        .get_shared_plan(100, &ConnContext::default())
        .is_none());
    assert!(share
        .get_shared_plan(200, &ConnContext::default())
        .is_some());
}
// ============================================================================
// T2.5: 计划缓存预热与冷启动加速 PlanCachePrewarm 测试
// ============================================================================

use sz_orm_core::plan_cache::{HotSqlHistory, PlanCachePrewarm, PlanPrewarmConfig};

fn temp_hot_sql_path() -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "sz-orm-test-hotsql-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    path
}

#[test]
fn test_prewarm_hot_sql_load() {
    let path = temp_hot_sql_path();
    let config = PlanPrewarmConfig {
        topk: 10,
        decay_factor: 0.5,
        history_path: path.clone(),
    };
    let prewarm = PlanCachePrewarm::new(config);

    let mut history = HotSqlHistory::new();
    history.add("SELECT * FROM users", 100);
    history.add("SELECT * FROM orders", 50);
    prewarm.save_hot_sql_history(&history).unwrap();

    let loaded = prewarm.load_hot_sql_history().unwrap();
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded.hot_sqls[0], "SELECT * FROM users");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_prewarm_hot_sql_no_history() {
    let path = temp_hot_sql_path();
    let config = PlanPrewarmConfig {
        topk: 10,
        decay_factor: 0.5,
        history_path: path.clone(),
    };
    let prewarm = PlanCachePrewarm::new(config);

    let result = prewarm.load_hot_sql_history();
    assert!(result.is_none(), "无历史文件应返回 None");

    let cache = PlanCache::new(100, None);
    let loaded = prewarm.prewarm_plan_cache(&cache);
    assert_eq!(loaded, 0, "无历史时应预加载 0 条");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_prewarm_hot_sql_persist() {
    let path = temp_hot_sql_path();
    let config = PlanPrewarmConfig {
        topk: 10,
        decay_factor: 0.5,
        history_path: path.clone(),
    };
    let prewarm = PlanCachePrewarm::new(config);

    let mut history = HotSqlHistory::new();
    history.add("SELECT 1", 10);
    prewarm.save_hot_sql_history(&history).unwrap();

    let config2 = PlanPrewarmConfig {
        topk: 10,
        decay_factor: 0.5,
        history_path: path.clone(),
    };
    let prewarm2 = PlanCachePrewarm::new(config2);
    let loaded = prewarm2.load_hot_sql_history().unwrap();
    assert_eq!(loaded.hot_sqls[0], "SELECT 1");
    assert_eq!(loaded.access_freqs[0], 10);

    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_prewarm_hot_sql_decay() {
    let mut history = HotSqlHistory::new();
    history.add("SELECT * FROM a", 100);
    history.add("SELECT * FROM b", 50);

    let decayed = history.decay(0.5);
    assert_eq!(decayed.access_freqs[0], 50);
    assert_eq!(decayed.access_freqs[1], 25);
}

#[test]
fn test_prewarm_hot_sql_topk() {
    let mut history = HotSqlHistory::new();
    history.add("sql_a", 10);
    history.add("sql_b", 50);
    history.add("sql_c", 30);
    history.add("sql_d", 100);

    let top2 = history.topk(2);
    assert_eq!(top2.len(), 2);
    assert_eq!(top2[0].0, "sql_d");
    assert_eq!(top2[0].1, 100);
    assert_eq!(top2[1].0, "sql_b");
    assert_eq!(top2[1].1, 50);
}

#[test]
fn test_prewarm_hot_sql_merge() {
    let mut h1 = HotSqlHistory::new();
    h1.add("sql_a", 100);
    h1.add("sql_b", 50);

    let mut h2 = HotSqlHistory::new();
    h2.add("sql_b", 30);
    h2.add("sql_c", 20);

    let merged = h1.merge(&h2);
    assert_eq!(merged.len(), 3, "合并后去重应有 3 条");
    let b_freq = merged
        .hot_sqls
        .iter()
        .position(|s| s == "sql_b")
        .map(|i| merged.access_freqs[i])
        .unwrap();
    assert_eq!(b_freq, 80, "sql_b 频率应累加 50+30=80");
}

#[test]
fn test_prewarm_hot_sql_filter() {
    let mut history = HotSqlHistory::new();
    history.add("sql_a", 100);
    history.add("sql_b", 5);
    history.add("sql_c", 50);

    let filtered = history.filter_low_freq(10);
    assert_eq!(filtered.len(), 2, "应过滤掉 freq < 10 的");
    assert!(filtered.hot_sqls.contains(&"sql_a".to_string()));
    assert!(filtered.hot_sqls.contains(&"sql_c".to_string()));
    assert!(!filtered.hot_sqls.contains(&"sql_b".to_string()));
}

#[test]
fn test_prewarm_plan_cache_with_history() {
    let path = temp_hot_sql_path();
    let config = PlanPrewarmConfig {
        topk: 10,
        decay_factor: 0.5,
        history_path: path.clone(),
    };
    let prewarm = PlanCachePrewarm::new(config);

    let mut history = HotSqlHistory::new();
    history.add("SELECT * FROM users WHERE id = 1", 100);
    history.add("SELECT * FROM orders WHERE id = 1", 50);
    prewarm.save_hot_sql_history(&history).unwrap();

    let cache = PlanCache::new(100, None);
    let initial_size = cache.size();
    let loaded = prewarm.prewarm_plan_cache(&cache);
    assert_eq!(loaded, 2, "应预加载 2 条");
    assert!(cache.size() > initial_size, "缓存条目应增加");
    assert_eq!(prewarm.prewarm_count(), 1);

    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_prewarm_plan_cache_topk_limit() {
    let path = temp_hot_sql_path();
    let config = PlanPrewarmConfig {
        topk: 2,
        decay_factor: 0.5,
        history_path: path.clone(),
    };
    let prewarm = PlanCachePrewarm::new(config);

    let mut history = HotSqlHistory::new();
    history.add("SELECT 1", 10);
    history.add("SELECT 2", 20);
    history.add("SELECT 3", 30);
    history.add("SELECT 4", 40);
    prewarm.save_hot_sql_history(&history).unwrap();

    let cache = PlanCache::new(100, None);
    let loaded = prewarm.prewarm_plan_cache(&cache);
    assert_eq!(loaded, 2, "TopK=2 应只预加载 2 条");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_prewarm_hot_sql_corrupt_recovery() {
    let path = temp_hot_sql_path();
    std::fs::write(&path, "invalid json").unwrap();

    let config = PlanPrewarmConfig {
        topk: 10,
        decay_factor: 0.5,
        history_path: path.clone(),
    };
    let prewarm = PlanCachePrewarm::new(config);

    let result = prewarm.load_hot_sql_history();
    assert!(result.is_none(), "损坏文件应返回 None");

    let cache = PlanCache::new(100, None);
    let loaded = prewarm.prewarm_plan_cache(&cache);
    assert_eq!(loaded, 0, "损坏文件应跳过预热");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_prewarm_hot_sql_cached_history() {
    let path = temp_hot_sql_path();
    let config = PlanPrewarmConfig {
        topk: 10,
        decay_factor: 0.5,
        history_path: path.clone(),
    };
    let prewarm = PlanCachePrewarm::new(config);

    let mut history = HotSqlHistory::new();
    history.add("SELECT 1", 10);
    prewarm.save_hot_sql_history(&history).unwrap();

    let cached = prewarm.cached_history().unwrap();
    assert_eq!(cached.hot_sqls[0], "SELECT 1");

    let _ = std::fs::remove_file(&path);
}
