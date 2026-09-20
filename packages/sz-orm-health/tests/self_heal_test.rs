#![cfg(feature = "self-heal")]

use sz_orm_health::self_heal::*;

#[tokio::test]
async fn e2e_self_heal_connection_leak() {
    let config = SelfHealConfig::default();
    let coordinator = SelfHealCoordinator::new(config);
    let fault = FaultType::ConnectionLeak;
    let result = coordinator.heal(&fault).await;
    assert!(result.is_ok());
    let r = result.unwrap();
    assert!(r.success);
    assert!(!r.idempotent);
}

#[tokio::test]
async fn e2e_self_heal_pool_exhaustion() {
    let config = SelfHealConfig::default();
    let coordinator = SelfHealCoordinator::new(config);
    let fault = FaultType::PoolExhaustion;
    let result = coordinator.heal(&fault).await;
    assert!(result.is_ok());
    let r = result.unwrap();
    assert!(r.success);
}

#[tokio::test]
async fn e2e_self_heal_slow_query_storm() {
    let config = SelfHealConfig::default();
    let coordinator = SelfHealCoordinator::new(config);
    let fault = FaultType::SlowQueryStorm;
    let result = coordinator.heal(&fault).await;
    assert!(result.is_ok());
    let r = result.unwrap();
    assert!(r.success);
}

#[tokio::test]
async fn e2e_self_heal_node_unreachable() {
    let config = SelfHealConfig::default();
    let coordinator = SelfHealCoordinator::new(config);
    let fault = FaultType::NodeUnreachable;
    let result = coordinator.heal(&fault).await;
    assert!(result.is_ok());
    let r = result.unwrap();
    assert!(r.success);
}

#[tokio::test]
async fn e2e_self_heal_idempotent_repeated() {
    let config = SelfHealConfig::default();
    let coordinator = SelfHealCoordinator::new(config);
    let fault = FaultType::ConnectionLeak;
    let r1 = coordinator.heal(&fault).await.unwrap();
    let r2 = coordinator.heal(&fault).await.unwrap();
    let r3 = coordinator.heal(&fault).await.unwrap();
    assert!(!r1.idempotent);
    assert!(r2.idempotent);
    assert!(r3.idempotent);
    assert_eq!(r1.action_taken, r2.action_taken);
}
