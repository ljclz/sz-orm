//! W2-17 GOV-SLA-01 接线验证：SLA 违规追踪器生产入口可达性
//!
//! 验证 SlaViolationTracker 可在模拟生产场景中持续度量多接口 SLA 达成率，
//! 并正确区分达标/违规/样本不足三类窗口。

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
fn wiring_sla_tracker_tracks_multiple_interfaces_over_time() {
    let mut tracker = SlaViolationTracker::new(50, 3);
    let t = target(100.0, 0.01);

    for _ in 0..10 {
        let r = tracker.record_window("order_api", sample(80.0, 0.001, 200, t.clone()));
        assert!(r.compliant);
        assert!(!r.insufficient_samples);
    }

    for _ in 0..5 {
        let r = tracker.record_window("payment_api", sample(120.0, 0.001, 200, t.clone()));
        assert!(!r.compliant);
    }

    assert!(tracker.achievement_rate("order_api") > 0.99);
    assert_eq!(tracker.achievement_rate("payment_api"), 0.0);
    assert!(!tracker.alerts().is_empty());
    assert_eq!(tracker.reports().len(), 15);
}

#[test]
fn wiring_sla_tracker_achievement_rate_converges() {
    let mut tracker = SlaViolationTracker::new(10, 5);
    let t = target(100.0, 0.01);

    for i in 0..20 {
        let p99 = if i % 3 == 0 { 150.0 } else { 50.0 };
        tracker.record_window("api", sample(p99, 0.001, 100, t.clone()));
    }

    let rate = tracker.achievement_rate("api");
    assert!(
        rate > 0.6 && rate < 0.7,
        "expected ~13/20=0.65, got {}",
        rate
    );
}

#[test]
fn wiring_sla_tracker_insufficient_samples_not_counted_as_violation() {
    let mut tracker = SlaViolationTracker::new(100, 2);
    let t = target(100.0, 0.01);

    let r = tracker.record_window("api", sample(500.0, 0.5, 10, t.clone()));
    assert!(r.insufficient_samples);
    assert!(r.compliant);
    assert_eq!(tracker.consecutive_violations("api"), 0);
    assert!(tracker.alerts().is_empty());
}

#[test]
fn wiring_sla_tracker_compliant_resets_consecutive_violations() {
    let mut tracker = SlaViolationTracker::new(10, 3);
    let t = target(100.0, 0.01);

    tracker.record_window("api", sample(150.0, 0.001, 100, t.clone()));
    tracker.record_window("api", sample(160.0, 0.001, 100, t.clone()));
    assert_eq!(tracker.consecutive_violations("api"), 2);

    tracker.record_window("api", sample(50.0, 0.001, 100, t.clone()));
    assert_eq!(tracker.consecutive_violations("api"), 0);

    tracker.record_window("api", sample(150.0, 0.001, 100, t.clone()));
    assert_eq!(tracker.consecutive_violations("api"), 1);
    assert!(tracker.alerts().is_empty());
}

#[test]
fn wiring_sla_tracker_report_fields_populated() {
    let mut tracker = SlaViolationTracker::new(10, 3);
    let t = target(100.0, 0.01);

    let r = tracker.record_window("user_api", sample(75.0, 0.002, 500, t.clone()));
    assert_eq!(r.interface, "user_api");
    assert!((r.p99_latency_ms - 75.0).abs() < f64::EPSILON);
    assert!((r.error_rate - 0.002).abs() < f64::EPSILON);
    assert_eq!(r.call_count, 500);
    assert!(r.compliant);
    assert!(!r.insufficient_samples);
    assert!((r.achievement_rate - 1.0).abs() < f64::EPSILON);
}

#[test]
fn wiring_sla_tracker_alert_threshold_boundary() {
    let mut tracker = SlaViolationTracker::new(10, 3);
    let t = target(100.0, 0.01);

    tracker.record_window("api", sample(150.0, 0.001, 100, t.clone()));
    assert!(tracker.alerts().is_empty());

    tracker.record_window("api", sample(150.0, 0.001, 100, t.clone()));
    assert!(tracker.alerts().is_empty());

    tracker.record_window("api", sample(150.0, 0.001, 100, t.clone()));
    assert_eq!(tracker.alerts().len(), 1);
    assert_eq!(tracker.alerts()[0].consecutive_violations, 3);
}

#[test]
fn wiring_sla_tracker_error_rate_violation_metric_in_alert() {
    let mut tracker = SlaViolationTracker::new(10, 1);
    let t = target(100.0, 0.01);

    tracker.record_window("api", sample(50.0, 0.05, 100, t.clone()));
    assert_eq!(tracker.alerts().len(), 1);
    assert!(tracker.alerts()[0].violated_metric.contains("error_rate"));
    assert!(tracker.alerts()[0].violated_metric.contains("0.05"));
}
