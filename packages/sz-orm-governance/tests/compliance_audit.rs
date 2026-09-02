//! TASK-007 验证测试：合规审计报告生成

use sz_orm_governance::compliance::ComplianceAuditor;
use sz_orm_governance::types::{GovernanceError, Regulation};

#[test]
fn test_compliance_audit_with_ruleset() {
    let auditor = ComplianceAuditor::new();
    let report = auditor.audit(&Regulation::Gdpr, false).unwrap();
    assert_eq!(report.regulation, Regulation::Gdpr);
}

#[test]
fn test_compliance_audit_empty_ruleset() {
    let auditor = ComplianceAuditor::new();
    let result = auditor.audit(&Regulation::Gdpr, true);
    assert!(result.is_err());
    match result {
        Err(GovernanceError::RulesetMissing) => {}
        _ => panic!("期望 RulesetMissing"),
    }
}

#[test]
fn test_compliance_audit_ccpa() {
    let auditor = ComplianceAuditor::new();
    let report = auditor.audit(&Regulation::Ccpa, false).unwrap();
    assert_eq!(report.regulation, Regulation::Gdpr);
}

#[test]
fn test_compliance_audit_pipl() {
    let auditor = ComplianceAuditor::new();
    let report = auditor.audit(&Regulation::Pipl, false).unwrap();
    assert_eq!(report.regulation, Regulation::Gdpr);
}
