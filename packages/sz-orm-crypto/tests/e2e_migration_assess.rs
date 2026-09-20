//! 端到端测试：PQC 迁移评估产出结构化报告

use sz_orm_crypto::pqc::{CryptoScenario, PqcAlgorithm, PqcMigrationAssessor};

#[tokio::test]
async fn e2e_migration_assess_complete() {
    let assessor = PqcMigrationAssessor::new();
    let scenarios = vec![CryptoScenario {
        name: "tls".to_string(),
        current_algo: "RSA-2048".to_string(),
        pqc_recommended: PqcAlgorithm::MlKem768,
        compatible: true,
    }];
    let report = assessor.assess(&scenarios).unwrap();
    assert_eq!(report.scan_status, "SCAN_COMPLETE");
}

#[tokio::test]
async fn e2e_migration_assess_empty() {
    let assessor = PqcMigrationAssessor::new();
    let report = assessor.assess(&[]).unwrap();
    assert_eq!(report.scan_status, "SCAN_INCOMPLETE");
}
