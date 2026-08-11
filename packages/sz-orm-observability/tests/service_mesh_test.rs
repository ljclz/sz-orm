//! M9 集成测试：服务网格集成全流程

use sz_orm_observability::service_mesh::{
    observability::{MeshMetricType, MeshObservability, MeshSpan, MeshSpanType},
    CanaryConfig, CircuitConfig, IstioAdapter, LinkerdAdapter, MeshConfig, MeshError, MeshType,
    MtlsMode, RetryConfig, ServiceMeshAdapter, SidecarConfig, TrafficGovernance,
};

#[test]
fn test_istio_full_config() {
    let adapter = IstioAdapter::new();
    let config = MeshConfig {
        mesh: MeshType::Istio,
        mtls: MtlsMode::Strict,
        traffic: TrafficGovernance {
            canary: Some(CanaryConfig {
                percentage: 10,
                version: "v2".to_string(),
            }),
            blue_green: None,
            circuit_breaker: Some(CircuitConfig {
                max_connections: 100,
                max_pending_requests: 50,
                max_retries: 3,
                outlier_consecutive_errors: 5,
            }),
            retry: Some(RetryConfig {
                attempts: 3,
                retry_on: vec!["5xx".to_string()],
                per_try_timeout_ms: 2000,
            }),
        },
        sidecar_injection: SidecarConfig {
            namespace_label: Some("istio-injection".to_string()),
            annotation: None,
        },
    };
    let output = adapter.generate_config(&config).unwrap();
    assert!(output.yaml.contains("VirtualService"));
    assert!(output.yaml.contains("DestinationRule"));
    assert!(output.yaml.contains("PeerAuthentication"));
    assert!(output.yaml.contains("STRICT"));
    assert!(output.yaml.contains("weight: 90"));
    assert!(output.yaml.contains("weight: 10"));
}

#[test]
fn test_linkerd_full_config() {
    let adapter = LinkerdAdapter::new();
    let config = MeshConfig {
        mesh: MeshType::Linkerd,
        mtls: MtlsMode::Strict,
        traffic: TrafficGovernance {
            canary: None,
            blue_green: None,
            circuit_breaker: None,
            retry: Some(RetryConfig {
                attempts: 3,
                retry_on: vec!["5xx".to_string()],
                per_try_timeout_ms: 5000,
            }),
        },
        sidecar_injection: SidecarConfig {
            namespace_label: None,
            annotation: Some("linkerd.io/inject".to_string()),
        },
    };
    let output = adapter.generate_config(&config).unwrap();
    assert!(output.yaml.contains("Server"));
    assert!(output.yaml.contains("ServiceProfile"));
    assert!(output.yaml.contains("strict"));
}

#[test]
fn test_mesh_observability_integration() {
    let obs = MeshObservability::new();
    obs.record_metric(MeshMetricType::RequestCount, 100);
    obs.record_metric(MeshMetricType::RequestDuration, 5000);
    obs.record_metric(MeshMetricType::CircuitBreakerCount, 3);
    obs.record_metric(MeshMetricType::RetryCount, 7);

    let prometheus_output = obs.render_prometheus();
    assert!(prometheus_output.contains("RequestCount"));
    assert!(prometheus_output.contains("RequestDuration"));

    let registry_result = obs.integrate_with_registry();
    assert!(registry_result.contains("MetricsRegistry"));

    let tracing_result = obs.integrate_with_tracing();
    assert!(tracing_result.contains("OTLP"));
}

#[test]
fn test_mesh_trace_spans() {
    let obs = MeshObservability::new();
    obs.record_span(MeshSpan {
        span_type: MeshSpanType::Proxy,
        name: "istio-proxy".to_string(),
        trace_id: "trace-001".to_string(),
        duration_ms: 10,
    });
    obs.record_span(MeshSpan {
        span_type: MeshSpanType::Application,
        name: "handler".to_string(),
        trace_id: "trace-001".to_string(),
        duration_ms: 50,
    });
    assert_eq!(obs.spans().len(), 2);
}

#[test]
fn test_mtls_default_strict() {
    let config = MeshConfig::default();
    assert_eq!(config.mtls, MtlsMode::Strict);
}

#[test]
fn test_istio_control_plane_unavailable() {
    let adapter = IstioAdapter::with_control_plane(false);
    let config = MeshConfig::default();
    let output = adapter.generate_config(&config).unwrap();
    assert!(output.yaml.contains("control plane unavailable"));
}

#[test]
fn test_mesh_config_invalid_for_wrong_adapter() {
    let istio = IstioAdapter::new();
    let linkerd_config = MeshConfig {
        mesh: MeshType::Linkerd,
        ..MeshConfig::default()
    };
    assert!(matches!(
        istio.generate_config(&linkerd_config),
        Err(MeshError::ConfigInvalid)
    ));
}
