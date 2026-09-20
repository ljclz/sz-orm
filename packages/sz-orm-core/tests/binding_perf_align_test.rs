//! v7.7.0 任务 4.6：BindingPerfAligner 端到端测试

#![cfg(feature = "binding-perf-align")]

use sz_orm_core::{BindingLanguage, BindingPerfAligner};

#[tokio::test]
async fn e2e_binding_perf_align_cabi() {
    let aligner = BindingPerfAligner::new();
    let result = aligner.align_perf(BindingLanguage::Cabi).await.unwrap();
    assert!(result.api_coverage_rate >= 100.0);
    assert!(result.missing_apis.is_empty());
    assert!(result.perf_ratio <= 2.0);
}

#[tokio::test]
async fn e2e_binding_perf_align_all() {
    let aligner = BindingPerfAligner::new();
    for lang in [
        BindingLanguage::Cabi,
        BindingLanguage::Python,
        BindingLanguage::Java,
        BindingLanguage::Go,
        BindingLanguage::Cpp,
    ] {
        let result = aligner.align_perf(lang).await.unwrap();
        assert!(result.api_coverage_rate >= 100.0);
        assert!(result.missing_apis.is_empty());
        assert!(result.e2e_test_count > 0);
    }
}
