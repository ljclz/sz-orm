//! 端到端测试：PQC 不可用降级到经典 TLS

use sz_orm_crypto::pqc::PqcDegradationManager;

#[tokio::test]
async fn e2e_pqc_degrade_logs() {
    let manager = PqcDegradationManager::new();
    manager.degrade("PQC 算法不可用", "TLS").unwrap();
    assert!(manager.is_degraded());
    let logs = manager.get_logs();
    assert_eq!(logs[0].tag, "PQC_DEGRADED");
}

#[tokio::test]
async fn e2e_pqc_degrade_recover() {
    let manager = PqcDegradationManager::new();
    manager.degrade("test", "test").unwrap();
    manager.recover();
    assert!(!manager.is_degraded());
}
