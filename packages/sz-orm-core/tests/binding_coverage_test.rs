//! 绑定层 API 覆盖率报告测试（v7.5.0 组6.4）

use sz_orm_core::binding_coverage::{BindingCoverageReport, BindingLanguage};

#[test]
fn test_binding_language_variants() {
    let langs = [
        BindingLanguage::Cabi,
        BindingLanguage::Python,
        BindingLanguage::Java,
        BindingLanguage::Go,
        BindingLanguage::Cpp,
    ];
    assert_eq!(langs.len(), 5);
}

#[test]
fn test_coverage_report_creation() {
    let report = BindingCoverageReport::new(
        BindingLanguage::Cabi,
        100,
        80,
        vec!["api_a".to_string(), "api_b".to_string()],
        80,
    );
    assert_eq!(report.language, BindingLanguage::Cabi);
    assert_eq!(report.core_api_count, 100);
    assert_eq!(report.bound_api_count, 80);
    assert!((report.coverage_rate - 0.8).abs() < 1e-6);
    assert_eq!(report.missing_apis.len(), 2);
    assert!(report.is_fully_tested());
}

#[test]
fn test_coverage_report_zero_core() {
    let report = BindingCoverageReport::new(BindingLanguage::Python, 0, 0, vec![], 0);
    assert_eq!(report.coverage_rate, 0.0);
    assert!(report.is_fully_tested());
}

#[test]
fn test_coverage_report_not_fully_tested() {
    let report =
        BindingCoverageReport::new(BindingLanguage::Java, 50, 30, vec!["api_x".to_string()], 20);
    assert!(!report.is_fully_tested());
}

#[test]
fn test_coverage_report_to_json() {
    let report = BindingCoverageReport::new(
        BindingLanguage::Go,
        10,
        8,
        vec!["missing1".to_string(), "missing2".to_string()],
        8,
    );
    let json = report.to_json().unwrap();
    assert!(json.contains("Go"));
    assert!(json.contains("missing1"));
    assert!(json.contains("coverage_rate"));
}

#[test]
fn test_coverage_report_full_coverage() {
    let report = BindingCoverageReport::new(BindingLanguage::Cpp, 10, 10, vec![], 10);
    assert!((report.coverage_rate - 1.0).abs() < 1e-6);
    assert!(report.is_fully_tested());
    assert!(report.missing_apis.is_empty());
}
