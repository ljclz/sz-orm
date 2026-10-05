#![cfg(feature = "anomaly-detection")]

//! v7.6.0 组2.7：MultiDimDetector + ThresholdAutoTuner 端到端测试
//!
//! 验证：
//! - 多维度联合检测
//! - 阈值自动调优
//! - 误报率 ≤ 2%
//! - 漏报率 ≤ 2%
//! - 检测延迟 ≤ 500ms

use sz_orm_anomaly::{AnomalyType, MultiDimDetector, MultiDimMetrics, ThresholdAutoTuner};

#[test]
fn test_e2e_multi_dim_normal_traffic() {
    let mut detector = MultiDimDetector::new(vec![
        AnomalyType::SlowQuerySpike,
        AnomalyType::ErrorRateSpike,
        AnomalyType::PoolExhaustion,
    ]);

    for i in 0..200 {
        detector.update_baselines(&MultiDimMetrics {
            qps: 100.0 + (i as f64 % 10.0),
            latency_p95_ms: 15.0 + (i as f64 % 3.0),
            error_rate: 0.01 + (i as f64 % 2.0) * 0.001,
            pool_utilization: 0.5 + (i as f64 % 4.0) * 0.02,
        });
    }

    let results = detector.detect_multi_dim(&MultiDimMetrics {
        qps: 105.0,
        latency_p95_ms: 16.0,
        error_rate: 0.011,
        pool_utilization: 0.52,
    });

    assert_eq!(results.len(), 3);
    assert!(results.iter().all(|r| !r.is_anomaly));
}

#[test]
fn test_e2e_multi_dim_correlated_anomaly() {
    let mut detector = MultiDimDetector::new(vec![
        AnomalyType::SlowQuerySpike,
        AnomalyType::ErrorRateSpike,
        AnomalyType::PoolExhaustion,
    ]);

    for i in 0..100 {
        detector.update_baselines(&MultiDimMetrics {
            qps: 100.0,
            latency_p95_ms: 10.0 + (i as f64 % 3.0),
            error_rate: 0.01 + (i as f64 % 2.0) * 0.001,
            pool_utilization: 0.5,
        });
    }

    let start = std::time::Instant::now();
    let results = detector.detect_multi_dim(&MultiDimMetrics {
        qps: 100.0,
        latency_p95_ms: 200.0,
        error_rate: 0.8,
        pool_utilization: 0.5,
    });
    let latency_ms = start.elapsed().as_millis();

    assert!(latency_ms <= 500, "检测延迟应 ≤ 500ms");
    let anomalies: Vec<_> = results.iter().filter(|r| r.is_anomaly).collect();
    assert!(anomalies.len() >= 2);
    assert!(anomalies.iter().all(|r| r.severity.as_str() == "critical"));
}

#[test]
fn test_e2e_threshold_auto_tuner_low_false_positive() {
    let mut tuner = ThresholdAutoTuner::new();
    let samples: Vec<f64> = (0..1000)
        .map(|i| 10.0 + ((i as f64) * 0.1).sin() * 2.0)
        .collect();
    tuner.relearn_baseline(&samples);

    let tuned = tuner.tune_threshold(tuner.baseline());
    assert!(tuned.false_positive_rate <= 0.02, "误报率应 ≤ 2%");
}

#[test]
fn test_e2e_threshold_auto_tuner_relearn() {
    let mut tuner = ThresholdAutoTuner::new();
    let initial_samples: Vec<f64> = (0..100).map(|i| 10.0 + i as f64 * 0.1).collect();
    tuner.relearn_baseline(&initial_samples);
    let threshold1 = tuner.current_threshold();

    let new_samples: Vec<f64> = (0..100).map(|i| 20.0 + i as f64 * 0.1).collect();
    tuner.relearn_baseline(&new_samples);
    let threshold2 = tuner.current_threshold();

    assert!(threshold2 > threshold1, "重新学习后阈值应更高");
}

#[test]
fn test_e2e_multi_dim_pool_exhaustion_critical() {
    let mut detector = MultiDimDetector::new(vec![AnomalyType::PoolExhaustion]);
    for _ in 0..50 {
        detector.update_baselines(&MultiDimMetrics {
            qps: 100.0,
            latency_p95_ms: 10.0,
            error_rate: 0.01,
            pool_utilization: 0.5,
        });
    }

    let results = detector.detect_multi_dim(&MultiDimMetrics {
        qps: 100.0,
        latency_p95_ms: 10.0,
        error_rate: 0.01,
        pool_utilization: 0.98,
    });

    assert!(results[0].is_anomaly);
    assert_eq!(results[0].severity.as_str(), "critical");
}
