//! IstioAdapter：生成 Istio CRD（VirtualService/DestinationRule/PeerAuthentication）

use super::{MeshConfig, MeshConfigOutput, MeshError, MeshType, MtlsMode, ServiceMeshAdapter};

/// Istio 适配器
pub struct IstioAdapter {
    control_plane_available: bool,
}

impl IstioAdapter {
    pub fn new() -> Self {
        Self {
            control_plane_available: true,
        }
    }

    pub fn with_control_plane(available: bool) -> Self {
        Self {
            control_plane_available: available,
        }
    }

    fn generate_virtual_service(config: &MeshConfig) -> String {
        let mut yaml = String::new();
        yaml.push_str("apiVersion: networking.istio.io/v1beta1\n");
        yaml.push_str("kind: VirtualService\n");
        yaml.push_str("metadata:\n  name: sz-orm-vs\n");
        yaml.push_str("spec:\n  hosts:\n    - \"*\"\n  http:\n");

        if let Some(canary) = &config.traffic.canary {
            let v1_pct = 100 - canary.percentage as u32;
            let v2_pct = canary.percentage as u32;
            yaml.push_str(&format!(
                "    - route:\n      - destination:\n          host: service\n          subset: v1\n        weight: {}\n      - destination:\n          host: service\n          subset: {}\n        weight: {}\n",
                v1_pct, canary.version, v2_pct
            ));
        } else if let Some(bg) = &config.traffic.blue_green {
            yaml.push_str(&format!(
                "    - route:\n      - destination:\n          host: service\n          subset: {}\n        weight: 100\n",
                bg.active_version
            ));
        } else {
            yaml.push_str("    - route:\n      - destination:\n          host: service\n");
        }

        if let Some(retry) = &config.traffic.retry {
            yaml.push_str(&format!(
                "    retries:\n      attempts: {}\n      retryOn: {}\n      perTryTimeout: {}ms\n",
                retry.attempts,
                retry.retry_on.join(","),
                retry.per_try_timeout_ms
            ));
        }

        yaml
    }

    fn generate_destination_rule(config: &MeshConfig) -> String {
        let mut yaml = String::new();
        yaml.push_str("apiVersion: networking.istio.io/v1beta1\n");
        yaml.push_str("kind: DestinationRule\n");
        yaml.push_str("metadata:\n  name: sz-orm-dr\n");
        yaml.push_str("spec:\n  host: service\n");

        if let Some(cb) = &config.traffic.circuit_breaker {
            yaml.push_str(&format!(
                "  trafficPolicy:\n    connectionPool:\n      tcp:\n        maxConnections: {}\n      http:\n        pendingRequestsSizeLimit: {}\n    outlierDetection:\n      consecutiveErrors: {}\n      interval: 30s\n",
                cb.max_connections, cb.max_pending_requests, cb.outlier_consecutive_errors
            ));
        }

        yaml
    }

    fn generate_peer_authentication(config: &MeshConfig) -> String {
        let mode = match config.mtls {
            MtlsMode::Strict => "STRICT",
            MtlsMode::Permissive => "PERMISSIVE",
        };
        format!(
            "apiVersion: security.istio.io/v1beta1\nkind: PeerAuthentication\nmetadata:\n  name: sz-orm-pa\nspec:\n  mtls:\n    mode: {}\n",
            mode
        )
    }

    fn generate_namespace_label(config: &MeshConfig) -> String {
        let label = config
            .sidecar_injection
            .namespace_label
            .as_deref()
            .unwrap_or("istio-injection");
        format!("namespace labels:\n  {label}: enabled\n")
    }
}

impl Default for IstioAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ServiceMeshAdapter for IstioAdapter {
    fn generate_config(&self, config: &MeshConfig) -> Result<MeshConfigOutput, MeshError> {
        if config.mesh != MeshType::Istio {
            return Err(MeshError::ConfigInvalid);
        }

        let vs = Self::generate_virtual_service(config);
        let dr = Self::generate_destination_rule(config);
        let pa = Self::generate_peer_authentication(config);
        let ns = Self::generate_namespace_label(config);

        let mut yaml = String::new();
        yaml.push_str("---\n");
        yaml.push_str(&vs);
        yaml.push_str("---\n");
        yaml.push_str(&dr);
        yaml.push_str("---\n");
        yaml.push_str(&pa);
        yaml.push_str("---\n");
        yaml.push_str(&ns);

        if !self.control_plane_available {
            yaml.push_str("# WARNING: mesh control plane unavailable\n");
        }

        Ok(MeshConfigOutput {
            yaml,
            resources: vec![
                "VirtualService/sz-orm-vs".to_string(),
                "DestinationRule/sz-orm-dr".to_string(),
                "PeerAuthentication/sz-orm-pa".to_string(),
            ],
        })
    }

    fn mesh_type(&self) -> &'static str {
        "istio"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service_mesh::{
        CanaryConfig, CircuitConfig, RetryConfig, SidecarConfig, TrafficGovernance,
    };

    #[test]
    fn test_istio_adapter_mesh_type() {
        let adapter = IstioAdapter::new();
        assert_eq!(adapter.mesh_type(), "istio");
    }

    #[test]
    fn test_istio_generate_basic_config() {
        let adapter = IstioAdapter::new();
        let config = MeshConfig::default();
        let output = adapter.generate_config(&config).unwrap();

        assert!(output.yaml.contains("VirtualService"));
        assert!(output.yaml.contains("DestinationRule"));
        assert!(output.yaml.contains("PeerAuthentication"));
        assert_eq!(output.resources.len(), 3);
    }

    #[test]
    fn test_istio_mtls_strict() {
        let adapter = IstioAdapter::new();
        let config = MeshConfig::default();
        let output = adapter.generate_config(&config).unwrap();
        assert!(output.yaml.contains("mode: STRICT"));
    }

    #[test]
    fn test_istio_mtls_permissive() {
        let adapter = IstioAdapter::new();
        let config = MeshConfig {
            mtls: MtlsMode::Permissive,
            ..MeshConfig::default()
        };
        let output = adapter.generate_config(&config).unwrap();
        assert!(output.yaml.contains("mode: PERMISSIVE"));
    }

    #[test]
    fn test_istio_canary_routing() {
        let adapter = IstioAdapter::new();
        let config = MeshConfig {
            traffic: TrafficGovernance {
                canary: Some(CanaryConfig {
                    percentage: 10,
                    version: "v2".to_string(),
                }),
                blue_green: None,
                circuit_breaker: None,
                retry: None,
            },
            ..MeshConfig::default()
        };
        let output = adapter.generate_config(&config).unwrap();
        assert!(output.yaml.contains("weight: 90"));
        assert!(output.yaml.contains("weight: 10"));
        assert!(output.yaml.contains("v2"));
    }

    #[test]
    fn test_istio_circuit_breaker() {
        let adapter = IstioAdapter::new();
        let config = MeshConfig {
            traffic: TrafficGovernance {
                canary: None,
                blue_green: None,
                circuit_breaker: Some(CircuitConfig {
                    max_connections: 100,
                    max_pending_requests: 50,
                    max_retries: 3,
                    outlier_consecutive_errors: 5,
                }),
                retry: None,
            },
            ..MeshConfig::default()
        };
        let output = adapter.generate_config(&config).unwrap();
        assert!(output.yaml.contains("maxConnections: 100"));
        assert!(output.yaml.contains("consecutiveErrors: 5"));
    }

    #[test]
    fn test_istio_retry_config() {
        let adapter = IstioAdapter::new();
        let config = MeshConfig {
            traffic: TrafficGovernance {
                canary: None,
                blue_green: None,
                circuit_breaker: None,
                retry: Some(RetryConfig {
                    attempts: 3,
                    retry_on: vec!["5xx".to_string(), "reset".to_string()],
                    per_try_timeout_ms: 2000,
                }),
            },
            ..MeshConfig::default()
        };
        let output = adapter.generate_config(&config).unwrap();
        assert!(output.yaml.contains("attempts: 3"));
        assert!(output.yaml.contains("5xx,reset"));
    }

    #[test]
    fn test_istio_sidecar_injection() {
        let adapter = IstioAdapter::new();
        let config = MeshConfig {
            sidecar_injection: SidecarConfig {
                namespace_label: Some("istio-injection".to_string()),
                annotation: None,
            },
            ..MeshConfig::default()
        };
        let output = adapter.generate_config(&config).unwrap();
        assert!(output.yaml.contains("istio-injection: enabled"));
    }

    #[test]
    fn test_istio_control_plane_unavailable() {
        let adapter = IstioAdapter::with_control_plane(false);
        let config = MeshConfig::default();
        let output = adapter.generate_config(&config).unwrap();
        assert!(output.yaml.contains("control plane unavailable"));
    }

    #[test]
    fn test_istio_wrong_mesh_type() {
        let adapter = IstioAdapter::new();
        let config = MeshConfig {
            mesh: MeshType::Linkerd,
            ..MeshConfig::default()
        };
        let result = adapter.generate_config(&config);
        assert!(matches!(result, Err(MeshError::ConfigInvalid)));
    }
}
