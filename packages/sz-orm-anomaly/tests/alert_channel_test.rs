#![cfg(feature = "anomaly-detection")]

use sz_orm_anomaly::alert::{Alert, AnomalyType, Severity};
use sz_orm_anomaly::alert::{
    AlertChannel, AlertPersistenceQueue, EmailChannel, SlackChannel, WebhookChannel,
};

fn make_alert() -> Alert {
    Alert {
        anomaly_type: AnomalyType::SlowQuerySpike,
        severity: Severity::Warn,
        timestamp: 1000,
        metric_value: 500.0,
        threshold: 200.0,
        baseline: None,
        suggestion: "check query".into(),
        sql_summary: Some("SELECT * FROM t".into()),
    }
}

#[test]
fn test_webhook_channel() {
    let ch = WebhookChannel::new("https://example.com/hook".into());
    assert_eq!(ch.channel_type(), "webhook");
    let alert = make_alert();
    assert!(ch.send(&alert).is_ok());
}

#[test]
fn test_slack_channel() {
    let ch = SlackChannel::new("https://hooks.slack.com/...".into());
    assert_eq!(ch.channel_type(), "slack");
    let alert = make_alert();
    assert!(ch.send(&alert).is_ok());
}

#[test]
fn test_email_channel() {
    let ch = EmailChannel::new("smtp.example.com".into(), 587);
    assert_eq!(ch.channel_type(), "email");
    let alert = make_alert();
    assert!(ch.send(&alert).is_ok());
}

#[test]
fn test_alert_persistence_queue_basic() {
    let mut q = AlertPersistenceQueue::new(100);
    assert!(q.is_empty());
    q.enqueue(make_alert());
    assert_eq!(q.len(), 1);
}

#[test]
fn test_alert_persistence_queue_drain() {
    let mut q = AlertPersistenceQueue::new(100);
    q.enqueue(make_alert());
    q.enqueue(make_alert());
    let drained = q.drain();
    assert_eq!(drained.len(), 2);
    assert!(q.is_empty());
}

#[test]
fn test_alert_persistence_queue_overflow() {
    let mut q = AlertPersistenceQueue::new(2);
    q.enqueue(make_alert());
    q.enqueue(make_alert());
    q.enqueue(make_alert());
    assert_eq!(q.len(), 2);
}

#[test]
fn test_alert_persistence_queue_default() {
    let q = AlertPersistenceQueue::default();
    assert_eq!(q.len(), 0);
}
