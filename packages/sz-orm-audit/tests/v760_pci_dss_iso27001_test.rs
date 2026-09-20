//! v7.6.0 方向 4 端到端测试：PCI-DSS / ISO27001 合规报告。

#![cfg(feature = "compliance-report")]

use sz_orm_audit::compliance_report::{
    ComplianceFramework, ComplianceReportGenerator, ComplianceStatus,
};

#[test]
fn v760_e2e_pci_dss_report_complete() {
    let gen = ComplianceReportGenerator::new();
    let report = gen.generate(ComplianceFramework::PciDss);

    assert_eq!(report.framework, ComplianceFramework::PciDss);
    assert!(report.clause_count() >= 6, "PCI-DSS 应至少 6 条款");
    assert_eq!(
        report.satisfied_count(),
        report.clause_count(),
        "所有条款应满足"
    );
    assert_eq!(report.overall_status, ComplianceStatus::Compliant);
}

#[test]
fn v760_e2e_iso27001_report_complete() {
    let gen = ComplianceReportGenerator::new();
    let report = gen.generate(ComplianceFramework::Iso27001);

    assert_eq!(report.framework, ComplianceFramework::Iso27001);
    assert!(report.clause_count() >= 7, "ISO27001 应至少 7 条款");
    assert_eq!(
        report.satisfied_count(),
        report.clause_count(),
        "所有条款应满足"
    );
    assert_eq!(report.overall_status, ComplianceStatus::Compliant);
}

#[test]
fn v760_e2e_all_four_frameworks_compliant() {
    let gen = ComplianceReportGenerator::new();
    let frameworks = [
        ComplianceFramework::Gdpr,
        ComplianceFramework::Sox,
        ComplianceFramework::PciDss,
        ComplianceFramework::Iso27001,
    ];

    for fw in &frameworks {
        let report = gen.generate(fw.clone());
        assert!(report.clause_count() > 0, "{} 应有条款", fw.name());
        assert_eq!(
            report.satisfied_count(),
            report.clause_count(),
            "{} 所有条款应满足",
            fw.name()
        );
        assert_eq!(report.overall_status, ComplianceStatus::Compliant);
    }
}

#[test]
fn v760_e2e_pci_dss_markdown_export() {
    let gen = ComplianceReportGenerator::new();
    let report = gen.generate(ComplianceFramework::PciDss);
    let md = report.to_markdown();

    assert!(md.contains("PCI-DSS"));
    assert!(md.contains("PCI-DSS-3.4"));
    assert!(md.contains("PCI-DSS-10.1"));
    assert!(md.contains("Evidence"));
}
