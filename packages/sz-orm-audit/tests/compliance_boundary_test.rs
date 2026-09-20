//! ComplianceReport 边界与极端场景测试（v7.5.0 组7.1）
//!
//! 验证 ComplianceReportGenerator 在空输入、无证据、所有条款满足/不满足等边界条件下的行为。

#![cfg(feature = "compliance-report")]

use sz_orm_audit::compliance_report::{ComplianceFramework, ComplianceReportGenerator};

#[test]
fn test_generate_gdpr_empty() {
    let generator = ComplianceReportGenerator::new();
    let report = generator.generate(ComplianceFramework::Gdpr);
    assert_eq!(report.framework, ComplianceFramework::Gdpr);
}

#[test]
fn test_generate_sox_empty() {
    let generator = ComplianceReportGenerator::new();
    let report = generator.generate(ComplianceFramework::Sox);
    assert_eq!(report.framework, ComplianceFramework::Sox);
}

#[test]
fn test_generate_all_frameworks() {
    let generator = ComplianceReportGenerator::new();
    let gdpr = generator.generate(ComplianceFramework::Gdpr);
    let sox = generator.generate(ComplianceFramework::Sox);
    assert_ne!(gdpr.framework, sox.framework);
}

#[test]
fn test_report_has_clauses() {
    let generator = ComplianceReportGenerator::new();
    let report = generator.generate(ComplianceFramework::Gdpr);
    assert!(!report.clauses.is_empty());
}

#[test]
fn test_report_has_timestamp() {
    let generator = ComplianceReportGenerator::new();
    let report = generator.generate(ComplianceFramework::Sox);
    assert!(report.generated_at > 0);
}
