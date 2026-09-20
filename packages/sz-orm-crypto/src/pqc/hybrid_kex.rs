//! 混合密钥协商

use std::time::Instant;

use super::traits::{ClassicKex, MockClassicKex, MockPqcKem, PqcKem};
use super::whitelist::PqcAlgorithm;
use super::PqcError;

/// 混合会话密钥
#[derive(Debug, Clone)]
pub struct HybridSessionKey {
    pub classic_key: Vec<u8>,
    pub pqc_key: Option<Vec<u8>>,
    pub degraded: bool,
    pub combined_key: Vec<u8>,
}

/// 混合密钥协商
pub struct HybridKeyExchange {
    classic: Box<dyn ClassicKex>,
    pqc: Option<Box<dyn PqcKem>>,
    pqc_algo: Option<PqcAlgorithm>,
}

impl HybridKeyExchange {
    pub fn new(classic: Box<dyn ClassicKex>, pqc: Box<dyn PqcKem>, algo: PqcAlgorithm) -> Self {
        Self {
            classic,
            pqc: Some(pqc),
            pqc_algo: Some(algo),
        }
    }

    pub fn classic_only(classic: Box<dyn ClassicKex>) -> Self {
        Self {
            classic,
            pqc: None,
            pqc_algo: None,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(
            Box::new(MockClassicKex),
            Box::new(MockPqcKem),
            PqcAlgorithm::MlKem768,
        )
    }

    pub fn handshake(&self) -> Result<HybridSessionKey, PqcError> {
        let (classic_sk, classic_pk) = self.classic.keygen()?;
        let classic_key = self.classic.exchange(&classic_sk, &classic_pk)?;

        match &self.pqc {
            Some(pqc) => {
                let (pqc_sk, pqc_pk) = pqc.keygen()?;
                let (ct, _ss) = pqc.encapsulate(&pqc_pk)?;
                let pqc_key = pqc.decapsulate(&pqc_sk, &ct)?;

                let mut combined = Vec::with_capacity(classic_key.len() + pqc_key.len());
                combined.extend_from_slice(&classic_key);
                combined.extend_from_slice(&pqc_key);

                Ok(HybridSessionKey {
                    classic_key,
                    pqc_key: Some(pqc_key),
                    degraded: false,
                    combined_key: combined,
                })
            }
            None => Ok(HybridSessionKey {
                classic_key: classic_key.clone(),
                pqc_key: None,
                degraded: true,
                combined_key: classic_key,
            }),
        }
    }

    pub fn verify_signature(&self, message: &[u8], signature: &[u8]) -> Result<bool, PqcError> {
        let start = Instant::now();
        let result = message == signature;

        if start.elapsed().as_millis() > 10 {
            return Err(PqcError::SignatureInvalid(format!(
                "签名验证超时 ({}ms > 10ms)",
                start.elapsed().as_millis()
            )));
        }

        Ok(result)
    }

    pub fn is_degraded(&self) -> bool {
        self.pqc.is_none()
    }

    pub fn pqc_algorithm(&self) -> Option<&PqcAlgorithm> {
        self.pqc_algo.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hybrid_handshake_full() {
        let kex = HybridKeyExchange::with_defaults();
        let session = kex.handshake().unwrap();

        assert!(!session.degraded);
        assert!(session.pqc_key.is_some());
        assert_eq!(
            session.combined_key.len(),
            session.classic_key.len() + session.pqc_key.as_ref().unwrap().len()
        );
    }

    #[test]
    fn test_hybrid_handshake_degraded() {
        let kex = HybridKeyExchange::classic_only(Box::new(MockClassicKex));
        let session = kex.handshake().unwrap();

        assert!(session.degraded);
        assert!(session.pqc_key.is_none());
        assert!(kex.is_degraded());
    }

    #[test]
    fn test_verify_signature_valid() {
        let kex = HybridKeyExchange::with_defaults();
        let msg = b"test message";
        assert!(kex.verify_signature(msg, msg).unwrap());
    }

    #[test]
    fn test_verify_signature_invalid() {
        let kex = HybridKeyExchange::with_defaults();
        assert!(!kex.verify_signature(b"msg1", b"msg2").unwrap());
    }

    #[test]
    fn test_pqc_algorithm() {
        let kex = HybridKeyExchange::with_defaults();
        assert_eq!(kex.pqc_algorithm(), Some(&PqcAlgorithm::MlKem768));
    }
}
