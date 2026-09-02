//! TASK-033 集成测试：A/B 测试框架端到端验证

use sz_orm_model_ops::ab_test::{AbTestConfig, AbTestFramework, AbTestSample, Variant};

fn make_config(min_samples: usize) -> AbTestConfig {
    AbTestConfig {
        test_name: "nl2sql_model_comparison".to_string(),
        variant_a_name: "qwen-7b".to_string(),
        variant_b_name: "qwen-14b".to_string(),
        sample_ratio: 0.5,
        min_samples,
    }
}

#[test]
fn test_insufficient_samples() {
    let framework = AbTestFramework::new(make_config(100));
    let result = framework.analyze().unwrap();
    assert!(result.winner.is_none());
    assert!(result.conclusion.contains("样本不足"));
}

#[test]
fn test_variant_b_wins_on_success_rate() {
    let mut framework = AbTestFramework::new(make_config(10));

    for _ in 0..10 {
        framework.record(AbTestSample {
            variant: Variant::A,
            success: true,
            latency_ms: 100.0,
        });
        framework.record(AbTestSample {
            variant: Variant::A,
            success: false,
            latency_ms: 100.0,
        });
    }
    for _ in 0..18 {
        framework.record(AbTestSample {
            variant: Variant::B,
            success: true,
            latency_ms: 80.0,
        });
    }
    for _ in 0..2 {
        framework.record(AbTestSample {
            variant: Variant::B,
            success: false,
            latency_ms: 80.0,
        });
    }

    let result = framework.analyze().unwrap();
    assert_eq!(result.winner, Some(Variant::B));
    assert!(result.conclusion.contains("B"));
}

#[test]
fn test_variant_a_wins() {
    let mut framework = AbTestFramework::new(make_config(10));

    for _ in 0..18 {
        framework.record(AbTestSample {
            variant: Variant::A,
            success: true,
            latency_ms: 80.0,
        });
    }
    for _ in 0..2 {
        framework.record(AbTestSample {
            variant: Variant::A,
            success: false,
            latency_ms: 80.0,
        });
    }
    for _ in 0..10 {
        framework.record(AbTestSample {
            variant: Variant::B,
            success: true,
            latency_ms: 100.0,
        });
        framework.record(AbTestSample {
            variant: Variant::B,
            success: false,
            latency_ms: 100.0,
        });
    }

    let result = framework.analyze().unwrap();
    assert_eq!(result.winner, Some(Variant::A));
}

#[test]
fn test_no_significant_difference() {
    let mut framework = AbTestFramework::new(make_config(10));

    for _ in 0..10 {
        framework.record(AbTestSample {
            variant: Variant::A,
            success: true,
            latency_ms: 100.0,
        });
        framework.record(AbTestSample {
            variant: Variant::B,
            success: true,
            latency_ms: 100.0,
        });
    }

    let result = framework.analyze().unwrap();
    assert!(result.winner.is_none());
    assert!(result.conclusion.contains("无显著差异"));
}

#[test]
fn test_sample_counts_by_variant() {
    let mut framework = AbTestFramework::new(make_config(10));

    for _ in 0..5 {
        framework.record(AbTestSample {
            variant: Variant::A,
            success: true,
            latency_ms: 100.0,
        });
    }
    for _ in 0..3 {
        framework.record(AbTestSample {
            variant: Variant::B,
            success: true,
            latency_ms: 100.0,
        });
    }

    assert_eq!(framework.sample_count(), 8);
    let counts = framework.sample_counts_by_variant();
    assert_eq!(counts[&Variant::A], 5);
    assert_eq!(counts[&Variant::B], 3);
}

#[test]
fn test_latency_tracking() {
    let mut framework = AbTestFramework::new(make_config(2));

    framework.record(AbTestSample {
        variant: Variant::A,
        success: true,
        latency_ms: 50.0,
    });
    framework.record(AbTestSample {
        variant: Variant::A,
        success: true,
        latency_ms: 150.0,
    });
    framework.record(AbTestSample {
        variant: Variant::B,
        success: true,
        latency_ms: 80.0,
    });
    framework.record(AbTestSample {
        variant: Variant::B,
        success: true,
        latency_ms: 120.0,
    });

    let result = framework.analyze().unwrap();
    assert!((result.stats_a.avg_latency_ms - 100.0).abs() < 1e-10);
    assert!((result.stats_b.avg_latency_ms - 100.0).abs() < 1e-10);
}
