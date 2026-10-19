//! M16.5+M16.6：XA 事务恢复 + Saga 协调端到端测试

#[cfg(feature = "xa")]
use std::sync::Arc;

#[cfg(feature = "xa")]
use sz_orm_dtx::InMemoryTransactionLog;

#[cfg(feature = "xa")]
use sz_orm_dtx::recovery::{XaRecoveryConfig, XaRecoveryCoordinator};
#[cfg(feature = "xa")]
use sz_orm_dtx::xa::XaCoordinator;

#[cfg(feature = "dtx-saga-coordinator")]
use sz_orm_dtx::saga_coordinator::coordinator::SagaStepDef;

#[cfg(feature = "xa")]
#[tokio::test]
async fn test_xa_recovery_with_participant_check_e2e() {
    let log_store = Arc::new(InMemoryTransactionLog::new());
    let xa_coord = Arc::new(XaCoordinator::new(log_store.clone()));
    let recovery = XaRecoveryCoordinator::new(log_store, xa_coord);

    let participants = vec![];
    let config = XaRecoveryConfig::default();
    assert_eq!(config.recover_retry_limit, 3, "默认重试次数应为 3");
    assert!(!config.auto_recover, "默认不自动恢复");
    let results = recovery
        .recover_with_participant_check(&participants, &config)
        .await;
    assert!(results.is_empty(), "空日志无未决事务，应返回空结果");

    let config_auto = XaRecoveryConfig {
        auto_recover: true,
        ..Default::default()
    };
    let results2 = recovery
        .recover_with_participant_check(&participants, &config_auto)
        .await;
    assert!(
        results2.is_empty(),
        "空日志即使启用 auto_recover 也应返回空结果"
    );
}

#[cfg(feature = "dtx-saga-coordinator")]
#[tokio::test]
async fn test_saga_step_with_lock_e2e() {
    let step = SagaStepDef::new("book_inventory", "inventory_service")
        .with_lock("inventory_lock", std::time::Duration::from_secs(30));
    assert!(step.has_lock());
    let (lock_id, timeout) = step.lock_config().unwrap();
    assert_eq!(lock_id, "inventory_lock");
    assert_eq!(*timeout, std::time::Duration::from_secs(30));

    let no_lock_step = SagaStepDef::new("payment", "payment_service");
    assert!(!no_lock_step.has_lock(), "无锁步骤 has_lock 应为 false");
    assert!(
        no_lock_step.lock_config().is_none(),
        "无锁步骤 lock_config 应为 None"
    );

    let cloned = step.clone();
    assert!(cloned.has_lock(), "clone 后锁配置应保留");
    let (cloned_lock_id, _) = cloned.lock_config().unwrap();
    assert_eq!(cloned_lock_id, "inventory_lock", "clone 后 lock_id 应一致");
}

#[cfg(feature = "dtx-saga-coordinator")]
#[tokio::test]
async fn test_saga_coord_config_default_e2e() {
    use std::sync::Arc;
    use sz_orm_dtx::saga::SagaManager;
    use sz_orm_dtx::saga_coordinator::coordinator::{
        AuthContext, CrossServiceSagaDef, SagaCoordConfig, SagaCoordinator, SagaStepDef,
    };

    let config = SagaCoordConfig::default();
    assert_eq!(config.compensation_timeout_ms, 1000);

    let manager = Arc::new(SagaManager::new());
    let coordinator = SagaCoordinator::new(manager, config);

    let def = CrossServiceSagaDef::new("saga-1").with_step(SagaStepDef::new("step1", "svc_a"));
    let auth = AuthContext::new("user1", vec!["svc_a".to_string()]);

    let result = coordinator.orchestrate(&def, &auth).expect("编排应成功");
    assert!(result.success, "单步骤 Saga 应成功");
    assert!(!result.completed_steps.is_empty(), "应完成至少一个步骤");
}
