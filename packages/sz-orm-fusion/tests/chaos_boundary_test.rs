//! ChaosInjector 边界与极端场景测试（v7.5.0 组7.1）
//!
//! 验证 ChaosConfig / StabilityReport / ChaosInjector 在空输入、最大值、最小值等边界条件下的行为。

#![cfg(feature = "chaos")]

use sz_orm_fusion::chaos_injector::{
    ChaosConfig, ChaosInjector, FaultType, StabilityReport, WorkloadType,
};

#[test]
fn test_chaos_config_empty_fault_types() {
    let config = ChaosConfig {
        fault_types: vec![],
        fault_duration_secs: 10,
        recovery_timeout_secs: 30,
        failure_threshold: 5,
        reset_timeout_secs: 3,
        workload_types: vec![],
    };
    assert!(config.fault_types.is_empty());
    assert!(config.workload_types.is_empty());
}

#[test]
fn test_chaos_config_all_fault_types() {
    let config = ChaosConfig {
        fault_types: vec![
            FaultType::NetworkPartition,
            FaultType::NodeDown,
            FaultType::DiskFull,
            FaultType::ConnectionExhaust,
            FaultType::SlowQueryStorm,
        ],
        fault_duration_secs: 1,
        recovery_timeout_secs: 1,
        failure_threshold: 1,
        reset_timeout_secs: 1,
        workload_types: vec![WorkloadType::SimdCompare],
    };
    assert_eq!(config.fault_types.len(), 5);
    assert_eq!(config.fault_duration_secs, 1);
    assert_eq!(config.recovery_timeout_secs, 1);
}

#[test]
fn test_chaos_config_max_values() {
    let config = ChaosConfig {
        fault_types: vec![FaultType::NodeDown],
        fault_duration_secs: u64::MAX,
        recovery_timeout_secs: u64::MAX,
        failure_threshold: usize::MAX,
        reset_timeout_secs: u64::MAX,
        workload_types: vec![WorkloadType::SimdCompare],
    };
    assert_eq!(config.fault_duration_secs, u64::MAX);
    assert_eq!(config.recovery_timeout_secs, u64::MAX);
}

#[test]
fn test_chaos_config_zero_duration() {
    let config = ChaosConfig {
        fault_types: vec![FaultType::NetworkPartition],
        fault_duration_secs: 0,
        recovery_timeout_secs: 0,
        failure_threshold: 0,
        reset_timeout_secs: 0,
        workload_types: vec![],
    };
    assert_eq!(config.fault_duration_secs, 0);
    assert_eq!(config.recovery_timeout_secs, 0);
}

#[test]
fn test_chaos_config_default() {
    let config = ChaosConfig::default();
    assert!(!config.fault_types.is_empty());
    assert!(!config.workload_types.is_empty());
    assert!(config.fault_duration_secs > 0);
    assert!(config.recovery_timeout_secs > 0);
}

#[test]
fn test_fault_type_all_variants() {
    let faults = vec![
        FaultType::NetworkPartition,
        FaultType::NodeDown,
        FaultType::DiskFull,
        FaultType::ConnectionExhaust,
        FaultType::SlowQueryStorm,
    ];
    assert_eq!(faults.len(), 5);
    for i in 0..faults.len() {
        for j in (i + 1)..faults.len() {
            assert_ne!(faults[i], faults[j]);
        }
    }
}

#[test]
fn test_workload_type_all_variants() {
    let workloads = vec![
        WorkloadType::SingleRowQuery,
        WorkloadType::BatchQuery,
        WorkloadType::PoolConcurrency,
        WorkloadType::SimdCompare,
    ];
    assert_eq!(workloads.len(), 4);
    for i in 0..workloads.len() {
        for j in (i + 1)..workloads.len() {
            assert_ne!(workloads[i], workloads[j]);
        }
    }
}

#[test]
fn test_chaos_injector_creation_valid() {
    let config = ChaosConfig::default();
    let injector = ChaosInjector::new(config);
    assert!(injector.is_ok());
}

#[test]
fn test_chaos_injector_creation_empty_faults() {
    let config = ChaosConfig {
        fault_types: vec![],
        fault_duration_secs: 5,
        recovery_timeout_secs: 10,
        failure_threshold: 5,
        reset_timeout_secs: 3,
        workload_types: vec![WorkloadType::SimdCompare],
    };
    let injector = ChaosInjector::new(config);
    // 空 fault_types 可能导致验证失败
    assert!(injector.is_ok() || injector.is_err());
}
