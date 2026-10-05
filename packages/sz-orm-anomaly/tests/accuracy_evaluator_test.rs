#![cfg(feature = "anomaly-detection")]

use sz_orm_anomaly::detector::AccuracyEvaluator;

#[test]
fn test_accuracy_evaluator_empty() {
    let evaluator = AccuracyEvaluator::new();
    let report = evaluator.evaluate();
    assert_eq!(report.false_positive_rate, 0.0);
    assert_eq!(report.false_negative_rate, 0.0);
}

#[test]
fn test_accuracy_evaluator_all_correct() {
    let mut ev = AccuracyEvaluator::new();
    for _ in 0..95 {
        ev.record(true, true);
    }
    for _ in 0..5 {
        ev.record(false, false);
    }
    let report = ev.evaluate();
    assert_eq!(report.false_positive_rate, 0.0);
    assert_eq!(report.false_negative_rate, 0.0);
    assert!(!report.needs_recalibration);
}

#[test]
fn test_accuracy_evaluator_high_false_positive() {
    let mut ev = AccuracyEvaluator::new();
    for _ in 0..90 {
        ev.record(true, true);
    }
    for _ in 0..10 {
        ev.record(true, false);
    }
    let report = ev.evaluate();
    assert!(report.false_positive_rate > 0.05);
    assert!(report.needs_recalibration);
}

#[test]
fn test_accuracy_evaluator_high_false_negative() {
    let mut ev = AccuracyEvaluator::new();
    for _ in 0..90 {
        ev.record(true, true);
    }
    for _ in 0..10 {
        ev.record(false, true);
    }
    let report = ev.evaluate();
    assert!(report.false_negative_rate > 0.05);
    assert!(report.needs_recalibration);
}

#[test]
fn test_accuracy_evaluator_precision_recall() {
    let mut ev = AccuracyEvaluator::new();
    ev.record(true, true);
    ev.record(true, true);
    ev.record(true, false);
    ev.record(false, true);
    let report = ev.evaluate();
    assert!(report.precision > 0.0);
    assert!(report.recall > 0.0);
    assert!(report.f1_score > 0.0);
}

#[test]
fn test_accuracy_evaluator_perfect_scores() {
    let mut ev = AccuracyEvaluator::new();
    ev.record(true, true);
    ev.record(false, false);
    let report = ev.evaluate();
    assert_eq!(report.precision, 1.0);
    assert_eq!(report.recall, 1.0);
    assert_eq!(report.f1_score, 1.0);
}
