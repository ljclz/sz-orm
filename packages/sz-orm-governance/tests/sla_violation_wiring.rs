//! W2-18 GOV-SLA-02 接线验证：SLA 告警生成与查询
//!
//! 验证告警在连续违规窗口达到阈值时生成，告警包含违规指标详情，
//! 且多接口告警互不干扰。

use sz_orm_governance::sla_violation_tracker::{SlaSample, SlaTarget, SlaViolationTracker};

fn target(p99: f64, err: f64) -> SlaTarget {
    SlaTarget {
        max_p99_ms: p99,
        max_error_rate: err,
    }
}

fn sample(p99: f64, err: f64, count: u64, t: SlaTarget) -> SlaSample {
    SlaSample {
        p99_latency_ms: p99,
        error_rate: err,
        call_count: count,
        target: t,
    }
}

#[test]
fn wiring_sla_alert_generated_on_threshold_breach() {
    let mut tracker = SlaViolationTracker::new(10, 2);
    let t = target(100.0, 0.01);

    tracker.record_window("api", sample(200.0, 0.001, 100, t.clone()));
    assert!(tracker.alerts().is_empty());

    tracker.record_window("api", sample(200.0, 0.001, 100, t.clone()));
    assert_eq!(tracker.alerts().len(), 1);
    assert_eq!(tracker.alerts()[0].code, "GOV_SLA_VIOLATED");
    assert_eq!(tracker.alerts()[0].interface, "api");
}

#[test]
fn wiring_sla_alert_p99_metric_detail() {
    let mut tracker = SlaViolationTracker::new(10, 1);
    let t = target(100.0, 0.01);

    tracker.record_window("api", sample(250.0, 0.001, 100, t.clone()));
    let alert = &tracker.alerts()[0];
    assert!(alert.violated_metric.contains("p99_latency_ms"));
    assert!(alert.violated_metric.contains("250"));
    assert!(alert.violated_metric.contains("100"));
}

#[test]
fn wiring_sla_alert_error_rate_metric_detail() {
    let mut tracker = SlaViolationTracker::new(10, 1);
    let t = target(100.0, 0.01);

    tracker.record_window("api", sample(50.0, 0.1, 100, t.clone()));
    let alert = &tracker.alerts()[0];
    assert!(alert.violated_metric.contains("error_rate"));
    assert!(alert.violated_metric.contains("0.1"));
    assert!(alert.violated_metric.contains("0.01"));
}

#[test]
fn wiring_sla_alerts_independent_across_interfaces() {
    let mut tracker = SlaViolationTracker::new(10, 1);
    let t = target(100.0, 0.01);

    tracker.record_window("api_a", sample(200.0, 0.001, 100, t.clone()));
    tracker.record_window("api_b", sample(50.0, 0.001, 100, t.clone()));
    tracker.record_window("api_c", sample(50.0, 0.5, 100, t.clone()));

    assert_eq!(tracker.alerts().len(), 2);
    let interfaces: Vec<&str> = tracker
        .alerts()
        .iter()
        .map(|a| a.interface.as_str())
        .collect();
    assert!(interfaces.contains(&"api_a"));
    assert!(interfaces.contains(&"api_c"));
    assert!(!interfaces.contains(&"api_b"));
}

#[test]
fn wiring_sla_alert_consecutive_violations_count_accurate() {
    let mut tracker = SlaViolationTracker::new(10, 1);
    let t = target(100.0, 0.01);

    tracker.record_window("api", sample(150.0, 0.001, 100, t.clone()));
    assert_eq!(tracker.alerts()[0].consecutive_violations, 1);

    tracker.record_window("api", sample(160.0, 0.001, 100, t.clone()));
    assert_eq!(tracker.alerts()[1].consecutive_violations, 2);

    tracker.record_window("api", sample(170.0, 0.001, 100, t.clone()));
    assert_eq!(tracker.alerts()[2].consecutive_violations, 3);
}

#[test]
fn wiring_sla_alert_cleared_on_recovery() {
    let mut tracker = SlaViolationTracker::new(10, 2);
    let t = target(100.0, 0.01);

    tracker.record_window("api", sample(150.0, 0.001, 100, t.clone()));
    tracker.record_window("api", sample(150.0, 0.001, 100, t.clone()));
    let alert_count_before = tracker.alerts().len();
    assert!(alert_count_before >= 1);

    tracker.record_window("api", sample(50.0, 0.001, 100, t.clone()));
    assert_eq!(tracker.consecutive_violations("api"), 0);

    tracker.record_window("api", sample(150.0, 0.001, 100, t.clone()));
    assert_eq!(tracker.consecutive_violations("api"), 1);
    assert_eq!(tracker.alerts().len(), alert_count_before);
}

#[test]
fn wiring_sla_reports_capture_full_history() {
    let mut tracker = SlaViolationTracker::new(10, 5);
    let t = target(100.0, 0.01);

    for i in 0..10 {
        let p99 = if i % 2 == 0 { 50.0 } else { 150.0 };
        tracker.record_window("api", sample(p99, 0.001, 100, t.clone()));
    }

    assert_eq!(tracker.reports().len(), 10);
    let compliant_count = tracker.reports().iter().filter(|r| r.compliant).count();
    assert_eq!(compliant_count, 5);
    let violation_count = tracker.reports().iter().filter(|r| !r.compliant).count();
    assert_eq!(violation_count, 5);
}
