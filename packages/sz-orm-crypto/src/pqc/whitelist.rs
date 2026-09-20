//! PQC 算法白名单

use std::collections::HashSet;

use super::PqcError;

/// NIST 标准化 PQC 算法
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum PqcAlgorithm {
    MlKem768,
    MlKem1024,
    MlDsa65,
    MlDsa87,
    SlhDsa128s,
    SlhDsa128f,
}

impl PqcAlgorithm {
    pub fn name(&self) -> &'static str {
        match self {
            PqcAlgorithm::MlKem768 => "ML-KEM-768",
            PqcAlgorithm::MlKem1024 => "ML-KEM-1024",
            PqcAlgorithm::MlDsa65 => "ML-DSA-65",
            PqcAlgorithm::MlDsa87 => "ML-DSA-87",
            PqcAlgorithm::SlhDsa128s => "SLH-DSA-128s",
            PqcAlgorithm::SlhDsa128f => "SLH-DSA-128f",
        }
    }

    pub fn is_kem(&self) -> bool {
        matches!(self, PqcAlgorithm::MlKem768 | PqcAlgorithm::MlKem1024)
    }

    pub fn is_sign(&self) -> bool {
        matches!(
            self,
            PqcAlgorithm::MlDsa65
                | PqcAlgorithm::MlDsa87
                | PqcAlgorithm::SlhDsa128s
                | PqcAlgorithm::SlhDsa128f
        )
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "ML-KEM-768" => Some(PqcAlgorithm::MlKem768),
            "ML-KEM-1024" => Some(PqcAlgorithm::MlKem1024),
            "ML-DSA-65" => Some(PqcAlgorithm::MlDsa65),
            "ML-DSA-87" => Some(PqcAlgorithm::MlDsa87),
            "SLH-DSA-128s" => Some(PqcAlgorithm::SlhDsa128s),
            "SLH-DSA-128f" => Some(PqcAlgorithm::SlhDsa128f),
            _ => None,
        }
    }
}

/// PQC 算法白名单
pub struct PqcAlgorithmWhitelist {
    approved: HashSet<PqcAlgorithm>,
}

impl PqcAlgorithmWhitelist {
    pub fn new() -> Self {
        Self {
            approved: Self::all_algorithms().into_iter().collect(),
        }
    }

    pub fn all_algorithms() -> Vec<PqcAlgorithm> {
        vec![
            PqcAlgorithm::MlKem768,
            PqcAlgorithm::MlKem1024,
            PqcAlgorithm::MlDsa65,
            PqcAlgorithm::MlDsa87,
            PqcAlgorithm::SlhDsa128s,
            PqcAlgorithm::SlhDsa128f,
        ]
    }

    pub fn is_approved(&self, algo: &PqcAlgorithm) -> bool {
        self.approved.contains(algo)
    }

    pub fn approved_algorithms(&self) -> Vec<PqcAlgorithm> {
        self.approved.iter().copied().collect()
    }

    pub fn validate(&self, algo: &PqcAlgorithm) -> Result<(), PqcError> {
        if self.is_approved(algo) {
            Ok(())
        } else {
            Err(PqcError::AlgorithmNotApproved(format!(
                "PQC_ALGORITHM_NOT_APPROVED: {}",
                algo.name()
            )))
        }
    }

    pub fn kem_algorithms(&self) -> Vec<PqcAlgorithm> {
        self.approved
            .iter()
            .filter(|a| a.is_kem())
            .copied()
            .collect()
    }

    pub fn sign_algorithms(&self) -> Vec<PqcAlgorithm> {
        self.approved
            .iter()
            .filter(|a| a.is_sign())
            .copied()
            .collect()
    }
}

impl Default for PqcAlgorithmWhitelist {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_algorithms_approved() {
        let whitelist = PqcAlgorithmWhitelist::new();
        for algo in PqcAlgorithmWhitelist::all_algorithms() {
            assert!(whitelist.is_approved(&algo), "{} 应在白名单中", algo.name());
        }
    }

    #[test]
    fn test_validate_approved() {
        let whitelist = PqcAlgorithmWhitelist::new();
        assert!(whitelist.validate(&PqcAlgorithm::MlKem768).is_ok());
    }

    #[test]
    fn test_kem_algorithms() {
        let whitelist = PqcAlgorithmWhitelist::new();
        let kems = whitelist.kem_algorithms();
        assert_eq!(kems.len(), 2);
        for kem in &kems {
            assert!(kem.is_kem());
        }
    }

    #[test]
    fn test_sign_algorithms() {
        let whitelist = PqcAlgorithmWhitelist::new();
        let signs = whitelist.sign_algorithms();
        assert_eq!(signs.len(), 4);
        for sign in &signs {
            assert!(sign.is_sign());
        }
    }

    #[test]
    fn test_from_name() {
        assert_eq!(
            PqcAlgorithm::from_name("ML-KEM-768"),
            Some(PqcAlgorithm::MlKem768)
        );
        assert_eq!(PqcAlgorithm::from_name("UNKNOWN"), None);
    }

    #[test]
    fn test_algorithm_names() {
        assert_eq!(PqcAlgorithm::MlKem768.name(), "ML-KEM-768");
        assert_eq!(PqcAlgorithm::MlDsa65.name(), "ML-DSA-65");
        assert_eq!(PqcAlgorithm::SlhDsa128s.name(), "SLH-DSA-128s");
    }
}
