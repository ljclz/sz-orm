//! 5.6 端到端测试：GDPR / SOX 合规报告生成 + 证据验证。

use std::path::Path;

use sz_orm_audit::compliance_report::{
    ComplianceFramework, ComplianceReportGenerator, ComplianceStatus, Evidence,
};

#[test]
fn generate_gdpr_report() {
    let gen = ComplianceReportGenerator::new();
    let report = gen.generate(ComplianceFramework::Gdpr);
    assert_eq!(report.framework, ComplianceFramework::Gdpr);
    assert!(report.clause_count() > 0);
    assert_eq!(report.satisfied_count(), report.clause_count());
    assert_eq!(report.overall_status, ComplianceStatus::Compliant);
}

#[test]
fn generate_sox_report() {
    let gen = ComplianceReportGenerator::new();
    let report = gen.generate(ComplianceFramework::Sox);
    assert_eq!(report.framework, ComplianceFramework::Sox);
    assert!(report.clause_count() > 0);
    assert_eq!(report.satisfied_count(), report.clause_count());
    assert_eq!(report.overall_status, ComplianceStatus::Compliant);
}

#[test]
fn gdpr_report_contains_key_clauses() {
    let gen = ComplianceReportGenerator::new();
    let report = gen.generate(ComplianceFramework::Gdpr);
    let clause_ids: Vec<&str> = report
        .clauses
        .iter()
        .map(|c| c.clause_id.as_str())
        .collect();
    assert!(clause_ids.iter().any(|id| id.contains("GDPR-32")));
    assert!(clause_ids.iter().any(|id| id.contains("GDPR-15")));
    assert!(clause_ids.iter().any(|id| id.contains("GDPR-25")));
}

#[test]
fn sox_report_contains_key_clauses() {
    let gen = ComplianceReportGenerator::new();
    let report = gen.generate(ComplianceFramework::Sox);
    let clause_ids: Vec<&str> = report
        .clauses
        .iter()
        .map(|c| c.clause_id.as_str())
        .collect();
    assert!(clause_ids.iter().any(|id| id.contains("SOX-404")));
    assert!(clause_ids.iter().any(|id| id.contains("SOX-302")));
}

#[test]
fn all_clauses_have_evidence() {
    let gen = ComplianceReportGenerator::new();
    let gdpr = gen.generate(ComplianceFramework::Gdpr);
    for clause in &gdpr.clauses {
        assert!(
            !clause.evidence.is_empty(),
            "clause {} has no evidence",
            clause.clause_id
        );
        for ev in &clause.evidence {
            assert!(!ev.file_path.is_empty());
            assert!(ev.line > 0);
            assert!(!ev.description.is_empty());
        }
    }
    let sox = gen.generate(ComplianceFramework::Sox);
    for clause in &sox.clauses {
        assert!(
            !clause.evidence.is_empty(),
            "clause {} has no evidence",
            clause.clause_id
        );
    }
}

#[test]
fn verify_evidence_gdpr_real_files() {
    let gen = ComplianceReportGenerator::new();
    let mut report = gen.generate(ComplianceFramework::Gdpr);
    let base = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let verified = report.verify_all_evidence(base);
    assert!(verified);
    assert!(report.evidence_verified);
}

#[test]
fn verify_evidence_sox_real_files() {
    let gen = ComplianceReportGenerator::new();
    let mut report = gen.generate(ComplianceFramework::Sox);
    let base = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let verified = report.verify_all_evidence(base);
    assert!(verified);
    assert!(report.evidence_verified);
}

#[test]
fn evidence_nonexistent_file_fails_verification() {
    let e = Evidence::new("nonexistent/file.rs", 1, "fake");
    let base = Path::new(env!("CARGO_MANIFEST_DIR"));
    assert!(!e.verify(base));
}

#[test]
fn evidence_line_out_of_range_fails() {
    let e = Evidence::new("src/lib.rs", 999999, "out of range");
    let base = Path::new(env!("CARGO_MANIFEST_DIR"));
    assert!(!e.verify(base));
}

#[test]
fn report_to_markdown_contains_framework_and_clauses() {
    let gen = ComplianceReportGenerator::new();
    let report = gen.generate(ComplianceFramework::Gdpr);
    let md = report.to_markdown();
    assert!(md.contains("GDPR"));
    assert!(md.contains("Compliance Report"));
    assert!(md.contains("GDPR-32"));
    assert!(md.contains("Evidence"));
}

#[test]
fn report_to_markdown_sox() {
    let gen = ComplianceReportGenerator::new();
    let report = gen.generate(ComplianceFramework::Sox);
    let md = report.to_markdown();
    assert!(md.contains("SOX"));
    assert!(md.contains("SOX-404"));
}

#[test]
fn evidence_reference_format() {
    let e = Evidence::new("src/lib.rs", 42, "test");
    assert_eq!(e.reference(), "src/lib.rs:42");
}

#[test]
fn gdpr_and_sox_reports_independent() {
    let gen = ComplianceReportGenerator::new();
    let gdpr = gen.generate(ComplianceFramework::Gdpr);
    let sox = gen.generate(ComplianceFramework::Sox);
    assert_ne!(gdpr.framework, sox.framework);
    assert!(gdpr.clause_count() > 0);
    assert!(sox.clause_count() > 0);
}

#[test]
fn verify_all_evidence_updates_overall_status() {
    let gen = ComplianceReportGenerator::new();
    let mut report = gen.generate(ComplianceFramework::Gdpr);
    let base = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    report.verify_all_evidence(base);
    assert_eq!(report.overall_status, ComplianceStatus::Compliant);
}
