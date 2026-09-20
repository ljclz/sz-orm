#![cfg(feature = "chaos")]

//! v7.5.0 ChaosInjector 端到端测试

use sz_orm_fusion::{
    ChaosConfig, ChaosInjector, ChaosWorkloadType as WorkloadType, FaultType, RecoveryTimeMeasurer,
};

#[test]
fn test_chaos_config_validate() {
    let config = ChaosConfig::default();
    assert!(config.validate().is_ok());

    let invalid = ChaosConfig {
        fault_types: vec![],
        ..ChaosConfig::default()
    };
    assert!(invalid.validate().is_err());
}

#[test]
fn test_fault_type_can_real_trigger() {
    assert!(FaultType::ConnectionExhaust.can_real_trigger());
    assert!(FaultType::SlowQueryStorm.can_real_trigger());
    assert!(!FaultType::NetworkPartition.can_real_trigger());
    assert!(!FaultType::NodeDown.can_real_trigger());
    assert!(!FaultType::DiskFull.can_real_trigger());
}

#[test]
fn test_chaos_injector_creation() {
    let injector = ChaosInjector::new(ChaosConfig::default());
    assert!(injector.is_ok());
    let injector = injector.unwrap();
    assert_eq!(injector.active_fault(), None);
}

#[test]
fn test_inject_fault_connection_exhaust() {
    let mut injector = ChaosInjector::new(ChaosConfig::default()).unwrap();
    let is_real = injector.inject_fault(FaultType::ConnectionExhaust);
    assert!(is_real, "ConnectionExhaust should be real chaos");
    assert_eq!(injector.active_fault(), Some(FaultType::ConnectionExhaust));
}

#[test]
fn test_inject_fault_network_partition() {
    let mut injector = ChaosInjector::new(ChaosConfig::default()).unwrap();
    let is_real = injector.inject_fault(FaultType::NetworkPartition);
    assert!(!is_real, "NetworkPartition should not be real chaos");
}

#[test]
fn test_circuit_breaker_state_transitions() {
    let mut injector = ChaosInjector::new(ChaosConfig::default()).unwrap();
    assert!(injector.can_execute());
    injector.inject_fault(FaultType::ConnectionExhaust);
    assert!(!injector.can_execute());
    injector.recover_fault();
    let report = injector.collect_behavior(WorkloadType::SingleRowQuery, 0.0, true, true);
    assert!(!report.circuit_breaker_transitions.is_empty());
    assert!(report.is_real_chaos);
}

#[test]
fn test_recovery_time_measurer() {
    let mut measurer = RecoveryTimeMeasurer::new();
    measurer.record_fault_injection();
    std::thread::sleep(std::time::Duration::from_millis(100));
    measurer.record_recovery_probe();
    measurer.record_service_recovered();
    assert!(measurer.recovery_time_secs >= 0.0);
    assert!(measurer.is_within_threshold(10.0));
}

#[test]
fn test_recovery_time_bottleneck_analysis() {
    let mut measurer = RecoveryTimeMeasurer::new();
    measurer.record_fault_injection();
    measurer.recovery_time_secs = 6.0;
    let analysis = measurer.bottleneck_analysis();
    assert!(analysis.iter().any(|a| a.contains("连接池预热")));
}

#[test]
fn test_collect_behavior_report() {
    let mut injector = ChaosInjector::new(ChaosConfig::default()).unwrap();
    injector.inject_fault(FaultType::SlowQueryStorm);
    injector.recover_fault();
    let report = injector.collect_behavior(WorkloadType::BatchQuery, 50.0, true, true);
    assert_eq!(report.fault_type, FaultType::SlowQueryStorm);
    assert_eq!(report.workload_type, WorkloadType::BatchQuery);
    assert_eq!(report.availability_during_fault, 50.0);
    assert!(report.degradation_triggered);
    assert!(report.degradation_correct);
    assert!(report.is_real_chaos);
}

#[test]
fn test_record_success_resets_failures() {
    let mut injector = ChaosInjector::new(ChaosConfig::default()).unwrap();
    injector.record_failure();
    injector.record_failure();
    injector.record_success();
    injector.record_failure();
    assert!(injector.can_execute());
}

#[test]
fn test_workload_type_as_str() {
    assert_eq!(WorkloadType::SingleRowQuery.as_str(), "single_row_query");
    assert_eq!(WorkloadType::BatchQuery.as_str(), "batch_query");
    assert_eq!(WorkloadType::ComplexJoin.as_str(), "complex_join");
    assert_eq!(WorkloadType::Transaction.as_str(), "transaction");
    assert_eq!(WorkloadType::PoolConcurrency.as_str(), "pool_concurrency");
    assert_eq!(WorkloadType::SimdCompare.as_str(), "simd_compare");
}
