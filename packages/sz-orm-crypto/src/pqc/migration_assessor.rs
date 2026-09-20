//! PQC 迁移评估器

use std::collections::HashMap;

use super::whitelist::{PqcAlgorithm, PqcAlgorithmWhitelist};
use super::PqcError;

/// 密码学使用场景
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CryptoScenario {
    pub name: String,
    pub current_algo: String,
    pub pqc_recommended: PqcAlgorithm,
    pub compatible: bool,
}

/// 兼容性矩阵条目
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CompatibilityEntry {
    pub scenario: String,
    pub classic_algo: String,
    pub pqc_algo: String,
    pub compatible: bool,
}

/// PQC 迁移报告
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PqcMigrationReport {
    pub scenarios: Vec<CryptoScenario>,
    pub compatibility_matrix: Vec<CompatibilityEntry>,
    pub performance_impact: HashMap<String, f64>,
    pub security_benefit: HashMap<String, String>,
    pub scan_status: String,
}

/// PQC 迁移评估器
pub struct PqcMigrationAssessor {
    whitelist: PqcAlgorithmWhitelist,
}

impl PqcMigrationAssessor {
    pub fn new() -> Self {
        Self {
            whitelist: PqcAlgorithmWhitelist::new(),
        }
    }

    pub fn assess(&self, scenarios: &[CryptoScenario]) -> Result<PqcMigrationReport, PqcError> {
        let mut compatibility_matrix = Vec::new();
        let mut performance_impact = HashMap::new();
        let mut security_benefit = HashMap::new();

        for scenario in scenarios {
            let compatible = self.whitelist.is_approved(&scenario.pqc_recommended);

            compatibility_matrix.push(CompatibilityEntry {
                scenario: scenario.name.clone(),
                classic_algo: scenario.current_algo.clone(),
                pqc_algo: scenario.pqc_recommended.name().to_string(),
                compatible,
            });

            performance_impact.insert(
                scenario.name.clone(),
                if scenario.pqc_recommended.is_kem() {
                    2.0
                } else {
                    5.0
                },
            );

            security_benefit.insert(
                scenario.name.clone(),
                if compatible {
                    "抗量子攻击".to_string()
                } else {
                    "不兼容".to_string()
                },
            );
        }

        let scan_status = if scenarios.is_empty() {
            "SCAN_INCOMPLETE".to_string()
        } else {
            "SCAN_COMPLETE".to_string()
        };

        Ok(PqcMigrationReport {
            scenarios: scenarios.to_vec(),
            compatibility_matrix,
            performance_impact,
            security_benefit,
            scan_status,
        })
    }

    pub fn assess_partial(&self, scenarios: &[CryptoScenario]) -> PqcMigrationReport {
        match self.assess(scenarios) {
            Ok(report) => report,
            Err(_) => PqcMigrationReport {
                scenarios: scenarios.to_vec(),
                compatibility_matrix: vec![],
                performance_impact: HashMap::new(),
                security_benefit: HashMap::new(),
                scan_status: "SCAN_INCOMPLETE".to_string(),
            },
        }
    }
}

impl Default for PqcMigrationAssessor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_scenario(name: &str, algo: PqcAlgorithm) -> CryptoScenario {
        CryptoScenario {
            name: name.to_string(),
            current_algo: "RSA-2048".to_string(),
            pqc_recommended: algo,
            compatible: true,
        }
    }

    #[test]
    fn test_assess_complete() {
        let assessor = PqcMigrationAssessor::new();
        let scenarios = vec![
            make_scenario("tls_handshake", PqcAlgorithm::MlKem768),
            make_scenario("signing", PqcAlgorithm::MlDsa65),
        ];

        let report = assessor.assess(&scenarios).unwrap();
        assert_eq!(report.scan_status, "SCAN_COMPLETE");
        assert_eq!(report.compatibility_matrix.len(), 2);
        assert!(report.performance_impact.contains_key("tls_handshake"));
        assert!(report.security_benefit.contains_key("signing"));
    }

    #[test]
    fn test_assess_empty_scan_incomplete() {
        let assessor = PqcMigrationAssessor::new();
        let report = assessor.assess(&[]).unwrap();
        assert_eq!(report.scan_status, "SCAN_INCOMPLETE");
    }

    #[test]
    fn test_performance_impact() {
        let assessor = PqcMigrationAssessor::new();
        let scenarios = vec![
            make_scenario("kem_scenario", PqcAlgorithm::MlKem768),
            make_scenario("sign_scenario", PqcAlgorithm::MlDsa65),
        ];

        let report = assessor.assess(&scenarios).unwrap();
        assert_eq!(report.performance_impact["kem_scenario"], 2.0);
        assert_eq!(report.performance_impact["sign_scenario"], 5.0);
    }

    #[test]
    fn test_security_benefit() {
        let assessor = PqcMigrationAssessor::new();
        let scenarios = vec![make_scenario("test", PqcAlgorithm::MlKem1024)];

        let report = assessor.assess(&scenarios).unwrap();
        assert_eq!(report.security_benefit["test"], "抗量子攻击");
    }

    #[test]
    fn test_assess_partial() {
        let assessor = PqcMigrationAssessor::new();
        let report = assessor.assess_partial(&[]);
        assert_eq!(report.scan_status, "SCAN_INCOMPLETE");
    }
}
