//! 端到端测试：混合密钥协商经典 + PQC 双通道

use sz_orm_crypto::pqc::HybridKeyExchange;

#[tokio::test]
async fn e2e_hybrid_handshake_full() {
    let kex = HybridKeyExchange::with_defaults();
    let session = kex.handshake().unwrap();
    assert!(!session.degraded);
    assert!(session.pqc_key.is_some());
}

#[tokio::test]
async fn e2e_hybrid_handshake_degraded() {
    let kex = HybridKeyExchange::classic_only(Box::new(sz_orm_crypto::pqc::traits::MockClassicKex));
    let session = kex.handshake().unwrap();
    assert!(session.degraded);
    assert!(session.pqc_key.is_none());
}

#[tokio::test]
async fn e2e_hybrid_combined_key() {
    let kex = HybridKeyExchange::with_defaults();
    let session = kex.handshake().unwrap();
    assert!(session.combined_key.len() > session.classic_key.len());
}
