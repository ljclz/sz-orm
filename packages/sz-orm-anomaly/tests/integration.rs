#![cfg(feature = "anomaly-detection")]
//! 集成测试：模拟异常场景，验证检测准确性

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use sz_orm_anomaly::{
    AlertEmitter, AnomalyConfig, AnomalyDetector, AnomalyType, ErrorType, ReportExporter, Severity,
    TimeRange,
};

fn fast_config() -> AnomalyConfig {
    AnomalyConfig::default()
        .with_window_size(Duration::from_secs(60))
        .with_alert_cooldown(Duration::from_millis(0))
        .with_min_baseline_samples(5)
        .with_slow_query_spike_count(5)
}

fn now() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[test]
fn integration_slow_query_spike_scenario() {
    let detector = AnomalyDetector::new(fast_config());
    let ts = now();

    // 阶段 1：正常基线（少量慢查询）
    for i in 0..3 {
        detector.record_slow_query(120, "SELECT * FROM users WHERE id = ?", ts + i);
    }
    let alerts = detector.detect_anomalies();
    assert!(alerts.is_empty(), "基线阶段不应告警");

    // 阶段 2：突增（大量慢查询）
    for i in 0..30 {
        detector.record_slow_query(200, "SELECT * FROM orders WHERE user_id = ?", ts + 100 + i);
    }
    let alerts = detector.detect_anomalies();
    assert!(
        alerts
            .iter()
            .any(|a| a.anomaly_type == AnomalyType::SlowQuerySpike),
        "应检测到慢查询突增"
    );
}

#[test]
fn integration_error_rate_spike_scenario() {
    let detector = AnomalyDetector::new(fast_config());
    let ts = now();

    // 注入大量错误
    for i in 0..20 {
        detector.record_error(ErrorType::SqlError, ts + i);
    }
    // 少量正常查询
    for i in 0..5 {
        detector.record_slow_query(120, "SELECT 1", ts + i);
    }

    let alerts = detector.detect_anomalies();
    assert!(
        alerts
            .iter()
            .any(|a| a.anomaly_type == AnomalyType::ErrorRateSpike),
        "应检测到错误率突增"
    );
}

#[test]
fn integration_pool_exhaustion_scenario() {
    let config = fast_config()
        .with_pool_max_connections(50)
        .with_pool_wait_count_threshold(10);
    let detector = AnomalyDetector::new(config);
    let ts = now();

    // 模拟连接池耗尽：活跃=上限，等待>阈值
    for i in 0..5 {
        detector.record_pool_usage(50, 0, 15, 1500, ts + i);
    }

    let alerts = detector.detect_anomalies();
    assert!(
        alerts
            .iter()
            .any(|a| a.anomaly_type == AnomalyType::PoolExhaustion),
        "应检测到连接池耗尽"
    );
}

#[test]
fn integration_pool_exhaustion_by_time() {
    let config = fast_config()
        .with_pool_max_connections(30)
        .with_pool_wait_count_threshold(100)
        .with_pool_wait_time_threshold_ms(500);
    let detector = AnomalyDetector::new(config);
    let ts = now();

    // 活跃=上限，等待未超阈值但获取耗时超阈值
    detector.record_pool_usage(30, 0, 5, 800, ts);

    let alerts = detector.detect_anomalies();
    assert!(
        alerts
            .iter()
            .any(|a| a.anomaly_type == AnomalyType::PoolExhaustion),
        "应通过获取耗时检测到连接池耗尽"
    );
}

#[test]
fn integration_alert_subscription_callback() {
    let detector = AnomalyDetector::new(fast_config());
    let received = Arc::new(AtomicU64::new(0));
    let received_clone = Arc::clone(&received);

    detector.subscribe_alerts(Arc::new(move |_alert| {
        received_clone.fetch_add(1, Ordering::Relaxed);
    }));

    let ts = now();
    for i in 0..30 {
        detector.record_slow_query(200, "SELECT * FROM t WHERE id = ?", ts + i);
    }
    detector.detect_anomalies();

    assert!(received.load(Ordering::Relaxed) > 0, "订阅回调应被调用");
}

#[test]
fn integration_alert_dedup_cooldown() {
    let config = fast_config().with_alert_cooldown(Duration::from_millis(100));
    let detector = AnomalyDetector::new(config);
    let ts = now();

    // 第一次检测
    for i in 0..30 {
        detector.record_slow_query(200, "SELECT * FROM t WHERE id = ?", ts + i);
    }
    let first_alerts = detector.detect_anomalies();
    assert!(!first_alerts.is_empty());

    // 立即再次检测，应被去重
    let second_alerts = detector.detect_anomalies();
    assert!(second_alerts.is_empty(), "冷却期内应被去重，无新告警");
    assert!(detector.suppressed_count() > 0);
}

#[test]
fn integration_report_export_json() {
    let detector = AnomalyDetector::new(fast_config());
    let ts = now();
    for i in 0..30 {
        detector.record_slow_query(200, "SELECT * FROM t WHERE id = ?", ts + i);
    }
    detector.detect_anomalies();

    let history = detector.alert_history();
    let json = ReportExporter::export_report_json(&history, TimeRange::new(ts, ts + 1000));
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("JSON 应合法");
    assert!(parsed["summary"]["total_alerts"].as_u64().unwrap_or(0) >= 1);
}

#[test]
fn integration_report_export_markdown() {
    let detector = AnomalyDetector::new(fast_config());
    let ts = now();
    for i in 0..30 {
        detector.record_slow_query(200, "SELECT * FROM t WHERE id = ?", ts + i);
    }
    detector.detect_anomalies();

    let history = detector.alert_history();
    let md = ReportExporter::export_report_markdown(&history, TimeRange::new(ts, ts + 1000));
    assert!(md.contains("# 异常检测报告"));
    assert!(md.contains("## 异常列表"));
}

#[test]
fn integration_severity_levels() {
    let detector = AnomalyDetector::new(fast_config());
    let ts = now();

    // 大量慢查询触发 CRITICAL
    for i in 0..100 {
        detector.record_slow_query(500, "SELECT * FROM huge_table", ts + i);
    }
    let alerts = detector.detect_anomalies();
    assert!(
        alerts
            .iter()
            .any(|a| a.severity == Severity::Critical || a.severity == Severity::Warn),
        "应检测到 WARN 或 CRITICAL 级别告警"
    );
}

#[test]
fn integration_multiple_anomaly_types() {
    let config = fast_config().with_pool_max_connections(50);
    let detector = AnomalyDetector::new(config);
    let ts = now();

    // 同时触发慢查询突增 + 连接池耗尽
    for i in 0..30 {
        detector.record_slow_query(200, "SELECT * FROM t WHERE id = ?", ts + i);
    }
    detector.record_pool_usage(50, 0, 15, 1500, ts);

    let alerts = detector.detect_anomalies();
    let types: std::collections::HashSet<_> = alerts.iter().map(|a| a.anomaly_type).collect();
    assert!(types.len() >= 2, "应检测到多种异常类型");
}

#[test]
fn integration_config_hot_update() {
    let detector = AnomalyDetector::new(fast_config());
    let ts = now();

    // 初始阈值低，容易触发
    for i in 0..30 {
        detector.record_slow_query(200, "SELECT * FROM t WHERE id = ?", ts + i);
    }
    let alerts_before = detector.detect_anomalies();
    assert!(!alerts_before.is_empty());

    // 热更新：提高阈值，不易触发
    detector.update_config(
        AnomalyConfig::default()
            .with_window_size(Duration::from_secs(60))
            .with_alert_cooldown(Duration::from_millis(0))
            .with_slow_query_spike_count(1000),
    );
    detector.emitter().reset_dedup();
    let alerts_after = detector.detect_anomalies();
    assert!(alerts_after.is_empty(), "提高阈值后不应触发慢查询突增告警");
}

#[test]
fn integration_sql_masking_in_alert() {
    let detector = AnomalyDetector::new(fast_config());
    let ts = now();
    // SQL 含敏感参数
    for i in 0..30 {
        detector.record_slow_query(
            200,
            "SELECT * FROM users WHERE password='secret123' AND id=42",
            ts + i,
        );
    }
    let alerts = detector.detect_anomalies();
    for alert in &alerts {
        if let Some(sql) = &alert.sql_summary {
            assert!(!sql.contains("secret123"), "告警 SQL 摘要不应含敏感参数值");
            assert!(!sql.contains("42"), "告警 SQL 摘要不应含数字参数值");
        }
    }
}

#[test]
fn integration_alert_emitter_standalone() {
    use sz_orm_anomaly::Alert;
    let emitter = AlertEmitter::new(60_000);
    let alert = Alert {
        anomaly_type: AnomalyType::SlowQuerySpike,
        severity: Severity::Warn,
        timestamp: 1000,
        metric_value: 20.0,
        threshold: 10.0,
        baseline: None,
        suggestion: "test".to_string(),
        sql_summary: None,
    };
    let emitted = emitter.emit(alert);
    assert!(emitted.is_some());
    assert_eq!(emitter.emitted_count(), 1);
}
