//! TASK-017: FailurePredictor 单元测试
//!
//! 验证基于性能指标时序数据的故障预测 + 预警触发。

use sz_orm_diagnosis::{AlertSeverity, FailurePredictor, FailurePredictorConfig, MetricSample};

fn make_samples(
    utilization: f64,
    slow_ratio: f64,
    error_rate: f64,
    count: usize,
) -> Vec<MetricSample> {
    (0..count)
        .map(|i| MetricSample::new(i as u64 * 60, utilization, slow_ratio, error_rate))
        .collect()
}

#[test]
fn test_predict_no_alerts_healthy() {
    let predictor = FailurePredictor::default();
    let samples = make_samples(0.3, 0.01, 0.001, 15);

    let result = predictor.predict(&samples);

    assert!(result.alerts.is_empty());
    assert!(result.health_score > 80.0);
    assert!(result.summary.contains("系统健康"));
}

#[test]
fn test_predict_pool_utilization_sustained_warning() {
    let predictor = FailurePredictor::default();
    let samples = make_samples(0.85, 0.01, 0.001, 15);

    let result = predictor.predict(&samples);

    let pool_alert = result
        .alerts
        .iter()
        .find(|a| a.metric_name == "pool_utilization")
        .expect("应有连接池预警");
    assert_eq!(pool_alert.severity, AlertSeverity::Warning);
    assert!(pool_alert.description.contains("连续"));
}

#[test]
fn test_predict_pool_utilization_critical() {
    let predictor = FailurePredictor::default();
    let samples = make_samples(0.97, 0.01, 0.001, 15);

    let result = predictor.predict(&samples);

    let pool_alert = result
        .alerts
        .iter()
        .find(|a| a.metric_name == "pool_utilization")
        .expect("应有连接池预警");
    assert_eq!(pool_alert.severity, AlertSeverity::Critical);
    assert!(pool_alert.description.contains("95%"));
}

#[test]
fn test_predict_slow_query_rising_warning() {
    let predictor = FailurePredictor::default();
    let samples: Vec<MetricSample> = (0..15)
        .map(|i| {
            let slow_ratio = 0.05 + i as f64 * 0.02;
            MetricSample::new(i as u64 * 60, 0.3, slow_ratio, 0.001)
        })
        .collect();

    let result = predictor.predict(&samples);

    let slow_alert = result
        .alerts
        .iter()
        .find(|a| a.metric_name == "slow_query_ratio");
    if let Some(alert) = slow_alert {
        assert_eq!(alert.severity, AlertSeverity::Warning);
        assert!(alert.description.contains("上升"));
    }
}

#[test]
fn test_predict_error_rate_critical() {
    let predictor = FailurePredictor::default();
    let samples = make_samples(0.3, 0.01, 0.08, 15);

    let result = predictor.predict(&samples);

    let error_alert = result
        .alerts
        .iter()
        .find(|a| a.metric_name == "error_rate")
        .expect("应有错误率预警");
    assert_eq!(error_alert.severity, AlertSeverity::Critical);
}

#[test]
fn test_predict_empty_samples() {
    let predictor = FailurePredictor::default();
    let result = predictor.predict(&[]);

    assert!(result.alerts.is_empty());
    assert_eq!(result.health_score, 100.0);
    assert!(result.summary.contains("无指标数据"));
}

#[test]
fn test_predict_trend_computation() {
    let predictor = FailurePredictor::default();
    let samples: Vec<MetricSample> = (0..10)
        .map(|i| MetricSample::new(i as u64 * 60, 0.5 + i as f64 * 0.02, 0.01, 0.001))
        .collect();

    let result = predictor.predict(&samples);

    assert!(result.pool_utilization_trend > 0.0);
}

#[test]
fn test_predict_health_score_decreases_with_alerts() {
    let predictor = FailurePredictor::default();
    let healthy = make_samples(0.3, 0.01, 0.001, 15);
    let critical = make_samples(0.97, 0.01, 0.08, 15);

    let healthy_result = predictor.predict(&healthy);
    let critical_result = predictor.predict(&critical);

    assert!(healthy_result.health_score > critical_result.health_score);
    assert!(critical_result.health_score < 50.0);
}

#[test]
fn test_predict_predicted_failure_time() {
    let predictor = FailurePredictor::default();
    let samples: Vec<MetricSample> = (0..15)
        .map(|i| MetricSample::new(i as u64 * 60, 0.7 + i as f64 * 0.01, 0.01, 0.001))
        .collect();

    let result = predictor.predict(&samples);

    let pool_alert = result
        .alerts
        .iter()
        .find(|a| a.metric_name == "pool_utilization");
    if let Some(alert) = pool_alert {
        if let Some(predicted) = alert.predicted_failure_time {
            assert!(predicted > samples.last().unwrap().timestamp);
        }
    }
}

#[test]
fn test_custom_config_thresholds() {
    let config = FailurePredictorConfig {
        pool_utilization_threshold: 0.6,
        slow_query_threshold: 0.1,
        error_rate_threshold: 0.01,
        sustained_duration_secs: 300,
        sample_interval_secs: 60,
    };
    let predictor = FailurePredictor::new(config);
    let samples = make_samples(0.65, 0.01, 0.001, 10);

    let result = predictor.predict(&samples);

    let pool_alert = result
        .alerts
        .iter()
        .find(|a| a.metric_name == "pool_utilization");
    assert!(pool_alert.is_some());
}

#[test]
fn test_predict_multiple_alerts() {
    let predictor = FailurePredictor::default();
    let samples = make_samples(0.97, 0.25, 0.08, 15);

    let result = predictor.predict(&samples);

    assert!(result.alerts.len() >= 2);
    let has_critical = result
        .alerts
        .iter()
        .any(|a| a.severity == AlertSeverity::Critical);
    assert!(has_critical);
}

#[test]
fn test_metric_sample_clamping() {
    let sample = MetricSample::new(0, 1.5, -0.5, 2.0);
    assert_eq!(sample.pool_utilization, 1.0);
    assert_eq!(sample.slow_query_ratio, 0.0);
    assert_eq!(sample.error_rate, 1.0);
}

#[test]
fn test_predict_sustained_duration_not_enough() {
    let config = FailurePredictorConfig {
        pool_utilization_threshold: 0.8,
        slow_query_threshold: 0.2,
        error_rate_threshold: 0.05,
        sustained_duration_secs: 600,
        sample_interval_secs: 60,
    };
    let predictor = FailurePredictor::new(config);
    let samples = make_samples(0.85, 0.01, 0.001, 5);

    let result = predictor.predict(&samples);

    let pool_warning = result
        .alerts
        .iter()
        .find(|a| a.metric_name == "pool_utilization" && a.severity == AlertSeverity::Warning);
    assert!(
        pool_warning.is_none(),
        "持续 5 分钟不应触发 10 分钟阈值预警"
    );
}
