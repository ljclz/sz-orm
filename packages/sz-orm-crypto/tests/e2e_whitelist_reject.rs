//! 端到端测试：非白名单算法拒绝

use sz_orm_crypto::pqc::PqcAlgorithmWhitelist;

#[tokio::test]
async fn e2e_whitelist_all_approved() {
    let whitelist = PqcAlgorithmWhitelist::new();
    for algo in PqcAlgorithmWhitelist::all_algorithms() {
        assert!(whitelist.validate(&algo).is_ok());
    }
}

#[tokio::test]
async fn e2e_whitelist_kem_count() {
    let whitelist = PqcAlgorithmWhitelist::new();
    assert_eq!(whitelist.kem_algorithms().len(), 2);
}

#[tokio::test]
async fn e2e_whitelist_sign_count() {
    let whitelist = PqcAlgorithmWhitelist::new();
    assert_eq!(whitelist.sign_algorithms().len(), 4);
}
