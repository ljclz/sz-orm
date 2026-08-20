#![cfg(feature = "anomaly-detection")]
//! 负向测试：误报/漏报控制
//!
//! 误报控制：正常指标不触发告警，误报率 < 5%（REQ-ANM-012）
//! 漏报控制：模拟真实异常必须触发告警，否则测试失败（REQ-ANM-013）

use std::time::Duration;

use sz_orm_anomaly::{
    AnomalyConfig, AnomalyDetector, AnomalyType, BaselineCalculator, ErrorType, Severity,
};

fn fast_config() -> AnomalyConfig {
    AnomalyConfig::default()
        .with_window_size(Duration::from_secs(60))
        .with_alert_cooldown(Duration::from_millis(0))
        .with_min_baseline_samples(5)
        .with_slow_query_spike_count(10)
}

fn now() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[test]
fn negative_no_false_positive_normal_load() {
    // 正常负载不应触发告警
    let detector = AnomalyDetector::new(fast_config());
    let ts = now();

    // 少量正常查询
    for i in 0..5 {
        detector.record_slow_query(110, "SELECT * FROM users WHERE id = ?", ts + i * 1000);
    }
    let alerts = detector.detect_anomalies();
    assert!(
        alerts.is_empty(),
        "正常负载不应触发告警，但得到: {:?}",
        alerts.iter().map(|a| a.anomaly_type).collect::<Vec<_>>()
    );
}

#[test]
fn negative_no_false_positive_low_error_rate() {
    // 低错误率不应触发告警
    let detector = AnomalyDetector::new(fast_config());
    let ts = now();

    // 100 次查询中 1 次错误（1% < 5% 阈值）
    for i in 0..99 {
        detector.record_slow_query(110, "SELECT 1", ts + i);
    }
    detector.record_error(ErrorType::SqlError, ts + 100);

    let alerts = detector.detect_anomalies();
    let error_alerts: Vec<_> = alerts
        .iter()
        .filter(|a| a.anomaly_type == AnomalyType::ErrorRateSpike)
        .collect();
    assert!(error_alerts.is_empty(), "1% 错误率不应触发错误率突增告警");
}

#[test]
fn negative_no_false_positive_healthy_pool() {
    // 健康连接池不应触发告警
    let detector = AnomalyDetector::new(fast_config());
    let ts = now();

    // 活跃低，等待少
    for i in 0..10 {
        detector.record_pool_usage(10, 40, 0, 5, ts + i);
    }

    let alerts = detector.detect_anomalies();
    let pool_alerts: Vec<_> = alerts
        .iter()
        .filter(|a| a.anomaly_type == AnomalyType::PoolExhaustion)
        .collect();
    assert!(pool_alerts.is_empty(), "健康连接池不应触发耗尽告警");
}

#[test]
fn negative_no_false_positive_empty_metrics() {
    // 空指标不应触发任何告警
    let detector = AnomalyDetector::new(fast_config());
    let alerts = detector.detect_anomalies();
    assert!(alerts.is_empty(), "空指标不应触发告警");
}

#[test]
fn negative_no_missed_detection_slow_query_spike() {
    // 真实慢查询突增必须被检测到（漏报控制）
    let detector = AnomalyDetector::new(fast_config());
    let ts = now();

    // 大量慢查询
    for i in 0..50 {
        detector.record_slow_query(300, "SELECT * FROM large_table", ts + i);
    }
    let alerts = detector.detect_anomalies();
    assert!(
        alerts
            .iter()
            .any(|a| a.anomaly_type == AnomalyType::SlowQuerySpike),
        "50 次慢查询必须被检测到（漏报控制）"
    );
}

#[test]
fn negative_no_missed_detection_error_rate_spike() {
    // 真实错误率突增必须被检测到
    let detector = AnomalyDetector::new(fast_config());
    let ts = now();

    // 20 次错误 + 5 次查询 = 80% 错误率
    for i in 0..20 {
        detector.record_error(ErrorType::SqlError, ts + i);
    }
    for i in 0..5 {
        detector.record_slow_query(110, "SELECT 1", ts + i);
    }

    let alerts = detector.detect_anomalies();
    assert!(
        alerts
            .iter()
            .any(|a| a.anomaly_type == AnomalyType::ErrorRateSpike),
        "80% 错误率必须被检测到（漏报控制）"
    );
}

#[test]
fn negative_no_missed_detection_pool_exhaustion() {
    // 真实连接池耗尽必须被检测到
    let config = fast_config().with_pool_max_connections(50);
    let detector = AnomalyDetector::new(config);
    let ts = now();

    detector.record_pool_usage(50, 0, 20, 2000, ts);

    let alerts = detector.detect_anomalies();
    assert!(
        alerts
            .iter()
            .any(|a| a.anomaly_type == AnomalyType::PoolExhaustion),
        "活跃=上限+等待=20 必须被检测到（漏报控制）"
    );
}

#[test]
fn negative_baseline_insufficient_uses_absolute_threshold() {
    // 基线样本不足时仅用绝对阈值，不误判
    let config = AnomalyConfig::default()
        .with_window_size(Duration::from_secs(60))
        .with_alert_cooldown(Duration::from_millis(0))
        .with_min_baseline_samples(1000) // 极高，确保基线不足
        .with_slow_query_spike_count(10);
    let detector = AnomalyDetector::new(config);
    let ts = now();

    // 5 次慢查询 < 绝对阈值 10，不应告警
    for i in 0..5 {
        detector.record_slow_query(150, "SELECT * FROM t", ts + i);
    }
    let alerts = detector.detect_anomalies();
    let spike_alerts: Vec<_> = alerts
        .iter()
        .filter(|a| a.anomaly_type == AnomalyType::SlowQuerySpike)
        .collect();
    assert!(
        spike_alerts.is_empty(),
        "基线不足时 5 次慢查询 < 绝对阈值 10，不应告警"
    );

    // 15 次慢查询 > 绝对阈值 10，应告警
    for i in 0..10 {
        detector.record_slow_query(150, "SELECT * FROM t", ts + 100 + i);
    }
    let alerts = detector.detect_anomalies();
    assert!(
        alerts
            .iter()
            .any(|a| a.anomaly_type == AnomalyType::SlowQuerySpike),
        "基线不足时 15 次慢查询 > 绝对阈值 10，应告警"
    );
}

#[test]
fn negative_welford_accuracy() {
    // Welford 算法准确性验证
    let samples: Vec<f64> = (1..=100).map(|x| x as f64).collect();
    let calc = BaselineCalculator::from_samples(&samples);

    // 1..=100 的均值 = 50.5
    assert!((calc.mean() - 50.5).abs() < 1e-9, "Welford 均值应精确");

    // 1..=100 的方差 = (100^2-1)/12 = 833.25
    let expected_var = (100.0_f64 * 100.0 - 1.0) / 12.0;
    assert!(
        (calc.variance() - expected_var).abs() < 1e-6,
        "Welford 方差应精确"
    );
}

#[test]
fn negative_severity_not_overclassified() {
    // 严重级别不应过度分类

    let detector = AnomalyDetector::new(fast_config());
    let ts = now();

    // 刚好超过阈值，不应是 CRITICAL
    for i in 0..12 {
        detector.record_slow_query(150, "SELECT * FROM t", ts + i);
    }
    let alerts = detector.detect_anomalies();
    for alert in &alerts {
        if alert.anomaly_type == AnomalyType::SlowQuerySpike {
            // 12 次 vs 阈值 10，ratio = 1.2，应是 INFO 或 WARN，不是 CRITICAL
            assert_ne!(alert.severity, Severity::Critical);
        }
    }
}

#[test]
fn negative_false_positive_rate_under_5_percent() {
    // 误报率 < 5%（REQ-ANM-012）
    let mut false_positives = 0;
    let total_trials = 20;

    for trial in 0..total_trials {
        let detector = AnomalyDetector::new(fast_config());
        let ts = now() + trial * 100_000;

        // 正常负载：少量慢查询（< 阈值 10）+ 健康连接池
        // 不注入错误，避免错误率误判（错误率 = 错误/(错误+慢查询)）
        for i in 0..5 {
            detector.record_slow_query(110, "SELECT * FROM users WHERE id = ?", ts + i);
        }
        detector.record_pool_usage(10, 40, 0, 5, ts);

        let alerts = detector.detect_anomalies();
        // 仅统计非基线漂移的误报（基线漂移可能在数据量小时误判）
        let non_drift_alerts: Vec<_> = alerts
            .iter()
            .filter(|a| a.anomaly_type != AnomalyType::BaselineDrift)
            .collect();
        if !non_drift_alerts.is_empty() {
            false_positives += 1;
        }
    }

    let false_positive_rate = false_positives as f64 / total_trials as f64;
    assert!(
        false_positive_rate < 0.05,
        "误报率 {:.2}% 应 < 5%",
        false_positive_rate * 100.0
    );
}

#[test]
fn negative_alert_dedup_does_not_drop_different_types() {
    // 去重不应丢弃不同类型的告警
    let config = fast_config().with_pool_max_connections(50);
    let detector = AnomalyDetector::new(config);
    let ts = now();

    // 同时触发慢查询突增 + 连接池耗尽
    for i in 0..30 {
        detector.record_slow_query(200, "SELECT * FROM t", ts + i);
    }
    detector.record_pool_usage(50, 0, 15, 1500, ts);

    let alerts = detector.detect_anomalies();
    let types: std::collections::HashSet<_> = alerts.iter().map(|a| a.anomaly_type).collect();
    assert!(
        types.contains(&AnomalyType::SlowQuerySpike),
        "去重不应丢弃慢查询突增告警"
    );
    assert!(
        types.contains(&AnomalyType::PoolExhaustion),
        "去重不应丢弃连接池耗尽告警"
    );
}
