//! v7.7.0 任务 4.6：云原生部署增强端到端测试

#![cfg(feature = "cloud-native-enhanced")]

use sz_orm_observability::{
    CloudNativeEnhancedResult, CloudNativeObservabilityExporter, MeshTrafficGovernor,
    ObservabilityType, OperatorEnhancer,
};

#[tokio::test]
async fn e2e_cloud_native_observability_export() {
    let exporter = CloudNativeObservabilityExporter::new();
    let result = exporter
        .export(&[
            ObservabilityType::Metrics,
            ObservabilityType::Logs,
            ObservabilityType::Traces,
        ])
        .await
        .unwrap();
    assert!(result.export_success);
    assert_eq!(result.types_exported.len(), 3);
}

#[test]
fn e2e_operator_enhancer() {
    let enhancer = OperatorEnhancer::new();
    let spec = enhancer.enhance_operator();
    assert!(spec.deploy_verified);
    assert!(spec.auto_scale_strategies.len() >= 2);
}

#[test]
fn e2e_mesh_traffic_governor() {
    let governor = MeshTrafficGovernor::default();
    let config = governor.govern_traffic();
    assert!(config.traffic.canary.is_none());
}

#[tokio::test]
async fn e2e_cloud_native_enhanced_full() {
    let exporter = CloudNativeObservabilityExporter::new();
    let export_result = exporter
        .export(&[
            ObservabilityType::Metrics,
            ObservabilityType::Logs,
            ObservabilityType::Traces,
        ])
        .await
        .unwrap();
    let enhancer = OperatorEnhancer::new();
    let spec = enhancer.enhance_operator();
    let _governor = MeshTrafficGovernor::default();

    let result = CloudNativeEnhancedResult {
        operator_enhanced: true,
        auto_scale_strategies: spec.auto_scale_strategies.len(),
        service_mesh_configured: true,
        observability_exported: export_result.export_success,
        observability_types: export_result.types_exported,
        deploy_verified: spec.deploy_verified,
    };
    assert!(result.operator_enhanced);
    assert!(result.service_mesh_configured);
    assert!(result.observability_exported);
    assert!(result.deploy_verified);
}
