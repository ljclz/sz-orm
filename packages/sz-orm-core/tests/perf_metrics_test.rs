//! v7.4.0 任务 3.7：性能指标暴露端到端测试

use sz_orm_core::perf_metrics::PerfMetrics;

#[test]
fn test_perf_metrics_all_counters() {
    let m = PerfMetrics::default();
    m.record_simd_hit();
    m.record_simd_hit();
    m.record_simd_miss();
    m.record_zero_copy_hit();
    m.record_zero_copy_miss();
    m.record_alloc_reduction(1024);
    m.record_pool_acquire();
    m.record_pool_acquire_failed();
    m.record_pool_throughput(5000);
    m.record_plan_cache_hit();
    m.record_plan_cache_miss();
    m.record_simd_speedup(1.5);

    let s = m.snapshot();
    assert_eq!(s.simd_hits, 2);
    assert_eq!(s.simd_misses, 1);
    assert_eq!(s.zero_copy_hits, 1);
    assert_eq!(s.zero_copy_misses, 1);
    assert_eq!(s.zero_copy_alloc_reduction, 1024);
    assert_eq!(s.pool_acquire_count, 1);
    assert_eq!(s.pool_acquire_failed_count, 1);
    assert_eq!(s.pool_throughput_ops, 5000);
    assert_eq!(s.plan_cache_hits, 1);
    assert_eq!(s.plan_cache_misses, 1);
    assert!((s.simd_speedup_avg - 1.5).abs() < 1e-6);
}

#[test]
fn test_perf_metrics_hit_rates() {
    let m = PerfMetrics::default();
    for _ in 0..8 { m.record_simd_hit(); }
    for _ in 0..2 { m.record_simd_miss(); }
    for _ in 0..9 { m.record_plan_cache_hit(); }
    for _ in 0..1 { m.record_plan_cache_miss(); }

    let s = m.snapshot();
    assert!((s.simd_hit_rate - 0.8).abs() < 1e-9);
    assert!((s.plan_cache_hit_rate - 0.9).abs() < 1e-9);
}

#[test]
fn test_perf_metrics_global_singleton() {
    let m = PerfMetrics::global();
    m.record_simd_hit();
    let s = m.snapshot();
    assert!(s.simd_hits >= 1);
}

#[test]
fn test_perf_metrics_prometheus_export() {
    let m = PerfMetrics::default();
    m.record_simd_hit();
    m.record_zero_copy_hit();
    m.record_plan_cache_hit();
    m.record_pool_acquire();
    let prom = m.to_prometheus();
    assert!(prom.contains("# HELP sz_orm_simd_hits"));
    assert!(prom.contains("# TYPE sz_orm_simd_hits counter"));
    assert!(prom.contains("sz_orm_simd_hits 1"));
    assert!(prom.contains("sz_orm_zero_copy_hits 1"));
    assert!(prom.contains("sz_orm_plan_cache_hits 1"));
    assert!(prom.contains("sz_orm_pool_acquire_count 1"));
}

#[test]
fn test_perf_metrics_empty_snapshot() {
    let m = PerfMetrics::default();
    let s = m.snapshot();
    assert_eq!(s.simd_hits, 0);
    assert_eq!(s.simd_misses, 0);
    assert_eq!(s.simd_hit_rate, 0.0);
    assert_eq!(s.plan_cache_hit_rate, 0.0);
    assert_eq!(s.zero_copy_hit_rate, 0.0);
    assert_eq!(s.pool_acquire_success_rate, 0.0);
}