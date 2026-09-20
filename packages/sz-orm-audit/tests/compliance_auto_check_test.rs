//! v7.7.0 任务 4.6：ComplianceAutoChecker 端到端测试

#![cfg(feature = "compliance-auto-check")]

use sz_orm_audit::{
    ComplianceAutoChecker, CompliancePostureTracker, ComplianceViolation,
    ComplianceViolationAlerter,
};

#[tokio::test]
async fn e2e_compliance_auto_check() {
    let checker = ComplianceAutoChecker::new();
    let result = checker.auto_check().await.unwrap();
    assert!(result.evidence_verified);
    assert!(result.framework_count >= 4);
    assert!(!result.violation_alerted);
}

#[tokio::test]
async fn e2e_compliance_violation_alert() {
    let alerter = ComplianceViolationAlerter::new();
    let violation = ComplianceViolation {
        framework: "GDPR".to_string(),
        violation_type: "data_retention".to_string(),
        description: "数据保留期超限".to_string(),
        remediation: "缩短保留期".to_string(),
    };
    assert!(alerter.alert(&violation).await.is_ok());
}

#[tokio::test]
async fn e2e_compliance_posture_tracking() {
    let checker = ComplianceAutoChecker::new();
    let result = checker.auto_check().await.unwrap();
    let tracker = CompliancePostureTracker::new();
    let posture = tracker.track_posture(&[result]);
    assert!(posture.overall_compliance_rate >= 0.0);
    assert_eq!(posture.total_violations, 0);
}
