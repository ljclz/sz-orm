//! v7.7.0 任务 3.6：故障转移增强端到端测试

#![cfg(feature = "failover-enhanced")]

use sz_orm_fusion::{AutoRecoverCoordinator, FailoverEnhancer};

#[tokio::test]
async fn e2e_failover_enhance() {
    let enhancer = FailoverEnhancer::new();
    let result = enhancer.enhance_failover().await.unwrap();
    assert!(result.fault_detected);
    assert!(result.detection_dimensions.len() >= 2);
    assert!(result.double_confirmed);
    assert!(result.switch_time_ms <= 2000.0);
    assert!(result.data_intact);
    assert!(result.recovery_time_ms <= 30000.0);
    assert!(result.node_health_verified);
}

#[tokio::test]
async fn e2e_auto_recover() {
    let coordinator = AutoRecoverCoordinator::new();
    let result = coordinator.auto_recover("us-east-1").await.unwrap();
    assert!(result.node_recovered);
    assert!(result.health_verified);
    assert!(result.data_consistent);
}
