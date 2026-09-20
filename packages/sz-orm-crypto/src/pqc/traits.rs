//! PQC trait 抽象

use super::PqcError;

/// PQC KEM（密钥封装机制）trait
pub trait PqcKem: Send + Sync {
    fn keygen(&self) -> Result<(Vec<u8>, Vec<u8>), PqcError>;
    fn encapsulate(&self, public_key: &[u8]) -> Result<(Vec<u8>, Vec<u8>), PqcError>;
    fn decapsulate(&self, private_key: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, PqcError>;
}

/// PQC 签名 trait
pub trait PqcSign: Send + Sync {
    fn keygen(&self) -> Result<(Vec<u8>, Vec<u8>), PqcError>;
    fn sign(&self, private_key: &[u8], message: &[u8]) -> Result<Vec<u8>, PqcError>;
    fn verify(&self, public_key: &[u8], message: &[u8], signature: &[u8])
        -> Result<bool, PqcError>;
}

/// 经典密钥协商 trait
pub trait ClassicKex: Send + Sync {
    fn keygen(&self) -> Result<(Vec<u8>, Vec<u8>), PqcError>;
    fn exchange(&self, private_key: &[u8], peer_public: &[u8]) -> Result<Vec<u8>, PqcError>;
}

/// 模拟 PQC KEM 实现（用于测试和占位）
pub struct MockPqcKem;

impl PqcKem for MockPqcKem {
    fn keygen(&self) -> Result<(Vec<u8>, Vec<u8>), PqcError> {
        Ok((vec![0u8; 32], vec![1u8; 32]))
    }

    fn encapsulate(&self, _public_key: &[u8]) -> Result<(Vec<u8>, Vec<u8>), PqcError> {
        Ok((vec![2u8; 32], vec![3u8; 32]))
    }

    fn decapsulate(&self, _private_key: &[u8], _ciphertext: &[u8]) -> Result<Vec<u8>, PqcError> {
        Ok(vec![4u8; 32])
    }
}

/// 模拟 PQC 签名实现
pub struct MockPqcSign;

impl PqcSign for MockPqcSign {
    fn keygen(&self) -> Result<(Vec<u8>, Vec<u8>), PqcError> {
        Ok((vec![0u8; 32], vec![1u8; 32]))
    }

    fn sign(&self, _private_key: &[u8], message: &[u8]) -> Result<Vec<u8>, PqcError> {
        Ok(message.to_vec())
    }

    fn verify(
        &self,
        _public_key: &[u8],
        message: &[u8],
        signature: &[u8],
    ) -> Result<bool, PqcError> {
        Ok(message == signature)
    }
}

/// 模拟经典密钥协商
pub struct MockClassicKex;

impl ClassicKex for MockClassicKex {
    fn keygen(&self) -> Result<(Vec<u8>, Vec<u8>), PqcError> {
        Ok((vec![5u8; 32], vec![6u8; 32]))
    }

    fn exchange(&self, _private_key: &[u8], _peer_public: &[u8]) -> Result<Vec<u8>, PqcError> {
        Ok(vec![7u8; 32])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_pqc_kem() {
        let kem = MockPqcKem;
        let (sk, pk) = kem.keygen().unwrap();
        assert_eq!(sk.len(), 32);
        assert_eq!(pk.len(), 32);

        let (ct, ss) = kem.encapsulate(&pk).unwrap();
        assert_eq!(ct.len(), 32);
        assert_eq!(ss.len(), 32);

        let shared = kem.decapsulate(&sk, &ct).unwrap();
        assert_eq!(shared.len(), 32);
    }

    #[test]
    fn test_mock_pqc_sign() {
        let signer = MockPqcSign;
        let (sk, pk) = signer.keygen().unwrap();

        let msg = b"hello world";
        let sig = signer.sign(&sk, msg).unwrap();
        assert!(signer.verify(&pk, msg, &sig).unwrap());
        assert!(!signer.verify(&pk, b"tampered", &sig).unwrap());
    }

    #[test]
    fn test_mock_classic_kex() {
        let kex = MockClassicKex;
        let (sk, pk) = kex.keygen().unwrap();
        let shared = kex.exchange(&sk, &pk).unwrap();
        assert_eq!(shared.len(), 32);
    }
}
