//! TASK-016: QueryABTestFramework 单元测试
//!
//! 验证 A/B 测试框架：100+ 样本量，输出 P50/P95 + 显著性 p 值。

use sz_orm_ai::query_plan_optimizer::{AbTestSample, QueryABTestFramework};

fn make_samples(mean: f64, std_dev: f64, count: usize) -> Vec<AbTestSample> {
    (0..count)
        .map(|i| {
            let noise = ((i as f64 * 1.7) % 3.0 - 1.5) * std_dev;
            AbTestSample {
                elapsed_ms: (mean + noise).max(0.1),
                success: true,
            }
        })
        .collect()
}

#[test]
fn test_ab_test_basic() {
    let framework = QueryABTestFramework::new();
    let original = make_samples(50.0, 5.0, 100);
    let optimized = make_samples(30.0, 3.0, 100);

    let result = framework.run_ab_test(&original, &optimized);

    assert_eq!(result.original.sample_count, 100);
    assert_eq!(result.optimized.sample_count, 100);
    assert!(result.mean_speedup > 1.0);
    assert!(result.p50_speedup > 1.0);
}

#[test]
fn test_ab_test_p50_p95() {
    let framework = QueryABTestFramework::new();
    let original = make_samples(100.0, 20.0, 200);
    let optimized = make_samples(50.0, 10.0, 200);

    let result = framework.run_ab_test(&original, &optimized);

    assert!(result.original.p50_ms > 0.0);
    assert!(result.original.p95_ms >= result.original.p50_ms);
    assert!(result.original.p99_ms >= result.original.p95_ms);
    assert!(result.optimized.p50_ms > 0.0);
    assert!(result.optimized.p95_ms >= result.optimized.p50_ms);
}

#[test]
fn test_ab_test_significant_difference() {
    let framework = QueryABTestFramework::new();
    let original = make_samples(100.0, 5.0, 100);
    let optimized = make_samples(50.0, 5.0, 100);

    let result = framework.run_ab_test(&original, &optimized);

    assert!(result.is_significant);
    assert!(result.p_value < 0.05);
    assert!(result.mean_speedup > 1.5);
    assert!(result.conclusion.contains("显著更快"));
}

#[test]
fn test_ab_test_no_significant_difference() {
    let framework = QueryABTestFramework::new();
    let original = make_samples(50.0, 10.0, 100);
    let optimized = make_samples(50.5, 10.0, 100);

    let result = framework.run_ab_test(&original, &optimized);

    assert!(!result.is_significant || result.p_value > 0.01);
}

#[test]
fn test_ab_test_optimized_slower() {
    let framework = QueryABTestFramework::new();
    let original = make_samples(30.0, 2.0, 100);
    let optimized = make_samples(60.0, 2.0, 100);

    let result = framework.run_ab_test(&original, &optimized);

    assert!(result.mean_speedup < 1.0);
    assert!(result.is_significant);
    assert!(result.conclusion.contains("显著更慢"));
}

#[test]
fn test_ab_test_with_failures() {
    let framework = QueryABTestFramework::new();
    let mut original = make_samples(50.0, 5.0, 100);
    for i in 0..10 {
        original[i].success = false;
    }
    let optimized = make_samples(30.0, 3.0, 100);

    let result = framework.run_ab_test(&original, &optimized);

    assert_eq!(result.original.success_count, 90);
    assert_eq!(result.optimized.success_count, 100);
}

#[test]
fn test_ab_test_empty_samples() {
    let framework = QueryABTestFramework::new();
    let result = framework.run_ab_test(&[], &[]);

    assert_eq!(result.original.sample_count, 0);
    assert_eq!(result.optimized.sample_count, 0);
    assert_eq!(result.original.mean_ms, 0.0);
}

#[test]
fn test_ab_test_all_failures() {
    let framework = QueryABTestFramework::new();
    let original: Vec<AbTestSample> = (0..10)
        .map(|_| AbTestSample {
            elapsed_ms: 50.0,
            success: false,
        })
        .collect();
    let optimized = make_samples(30.0, 3.0, 10);

    let result = framework.run_ab_test(&original, &optimized);

    assert_eq!(result.original.success_count, 0);
    assert_eq!(result.original.mean_ms, 0.0);
}

#[test]
fn test_ab_test_custom_sample_count() {
    let framework = QueryABTestFramework::new().with_sample_count(50);
    assert_eq!(framework.default_sample_count(), 50);
}

#[test]
fn test_ab_test_custom_significance_threshold() {
    let framework = QueryABTestFramework::new().with_significance_threshold(0.01);

    let original = make_samples(100.0, 5.0, 100);
    let optimized = make_samples(50.0, 5.0, 100);

    let result = framework.run_ab_test(&original, &optimized);

    assert!(result.is_significant);
    assert!(result.p_value < 0.01);
}

#[test]
fn test_ab_test_large_sample() {
    let framework = QueryABTestFramework::new();
    let original = make_samples(80.0, 15.0, 500);
    let optimized = make_samples(40.0, 8.0, 500);

    let result = framework.run_ab_test(&original, &optimized);

    assert_eq!(result.original.sample_count, 500);
    assert!(result.mean_speedup > 1.5);
    assert!(result.is_significant);
    assert!(result.original.p95_ms > result.original.p50_ms);
    assert!(result.optimized.p95_ms > result.optimized.p50_ms);
}

#[test]
fn test_ab_test_speedup_ratios() {
    let framework = QueryABTestFramework::new();
    let original = make_samples(100.0, 1.0, 100);
    let optimized = make_samples(25.0, 1.0, 100);

    let result = framework.run_ab_test(&original, &optimized);

    assert!(result.mean_speedup > 3.0);
    assert!(result.p50_speedup > 3.0);
    assert!(result.p95_speedup > 2.0);
}

#[test]
fn test_ab_test_conclusion_text() {
    let framework = QueryABTestFramework::new();

    let original = make_samples(100.0, 5.0, 100);
    let optimized = make_samples(50.0, 5.0, 100);

    let result = framework.run_ab_test(&original, &optimized);

    assert!(!result.conclusion.is_empty());
    assert!(result.conclusion.contains("p="));
}

#[test]
fn test_ab_test_t_statistic_direction() {
    let framework = QueryABTestFramework::new();

    let original = make_samples(100.0, 5.0, 100);
    let optimized = make_samples(50.0, 5.0, 100);

    let result = framework.run_ab_test(&original, &optimized);

    assert!(result.t_statistic > 0.0);
    assert!(result.degrees_of_freedom > 0.0);
}
