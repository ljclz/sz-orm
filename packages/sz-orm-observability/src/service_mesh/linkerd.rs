//! LinkerdAdapter：生成 Linkerd policy（Server/ServerAuthorization/ServiceProfile）

use super::{MeshConfig, MeshConfigOutput, MeshError, MeshType, MtlsMode, ServiceMeshAdapter};

/// Linkerd 适配器
pub struct LinkerdAdapter {
    control_plane_available: bool,
}

impl LinkerdAdapter {
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

    fn generate_server(config: &MeshConfig) -> String {
        let mtls_mode = match config.mtls {
            MtlsMode::Strict => "strict",
            MtlsMode::Permissive => "permissive",
        };
        format!(
            "apiVersion: policy.linkerd.io/v1beta1\nkind: Server\nmetadata:\n  name: sz-orm-server\nspec:\n  podSelector:\n    matchLabels:\n      app: sz-orm\n  port: 8080\n  proxyProtocol: {}\n",
            mtls_mode
        )
    }

    fn generate_server_authorization() -> String {
        "apiVersion: policy.linkerd.io/v1beta1\nkind: ServerAuthorization\nmetadata:\n  name: sz-orm-saz\nspec:\n  server:\n    name: sz-orm-server\n  authorization:\n    - all:\n        - principals:\n            - \"*\"\n".to_string()
    }

    fn generate_service_profile(config: &MeshConfig) -> String {
        let mut yaml = String::new();
        yaml.push_str("apiVersion: linkerd.io/v1alpha2\nkind: ServiceProfile\n");
        yaml.push_str("metadata:\n  name: sz-orm-sp\n");
        yaml.push_str(
            "spec:\n  routes:\n    - name: default\n      condition:\n        pathRegex: \"/.*\"\n",
        );

        if let Some(retry) = &config.traffic.retry {
            yaml.push_str(&format!(
                "      retryBudget:\n        retryRatio: 0.2\n        ttl: 10s\n      responseTimeout: {}ms\n",
                retry.per_try_timeout_ms
            ));
        }

        if let Some(canary) = &config.traffic.canary {
            yaml.push_str(&format!(
                "  # canary: {}% to {}\n",
                canary.percentage, canary.version
            ));
        }

        yaml
    }

    fn generate_injection_annotation(config: &MeshConfig) -> String {
        let annotation = config
            .sidecar_injection
            .annotation
            .as_deref()
            .unwrap_or("linkerd.io/inject");
        format!("annotations:\n  {annotation}: enabled\n")
    }
}

impl Default for LinkerdAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ServiceMeshAdapter for LinkerdAdapter {
    fn generate_config(&self, config: &MeshConfig) -> Result<MeshConfigOutput, MeshError> {
        if config.mesh != MeshType::Linkerd {
            return Err(MeshError::ConfigInvalid);
        }

        let server = Self::generate_server(config);
        let saz = Self::generate_server_authorization();
        let sp = Self::generate_service_profile(config);
        let annotation = Self::generate_injection_annotation(config);

        let mut yaml = String::new();
        yaml.push_str("---\n");
        yaml.push_str(&server);
        yaml.push_str("---\n");
        yaml.push_str(&saz);
        yaml.push_str("---\n");
        yaml.push_str(&sp);
        yaml.push_str("---\n");
        yaml.push_str(&annotation);

        if !self.control_plane_available {
            yaml.push_str("# WARNING: mesh control plane unavailable\n");
        }

        Ok(MeshConfigOutput {
            yaml,
            resources: vec![
                "Server/sz-orm-server".to_string(),
                "ServerAuthorization/sz-orm-saz".to_string(),
                "ServiceProfile/sz-orm-sp".to_string(),
            ],
        })
    }

    fn mesh_type(&self) -> &'static str {
        "linkerd"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service_mesh::{CanaryConfig, RetryConfig, SidecarConfig, TrafficGovernance};

    #[test]
    fn test_linkerd_adapter_mesh_type() {
        let adapter = LinkerdAdapter::new();
        assert_eq!(adapter.mesh_type(), "linkerd");
    }

    #[test]
    fn test_linkerd_generate_basic_config() {
        let adapter = LinkerdAdapter::new();
        let config = MeshConfig {
            mesh: MeshType::Linkerd,
            ..MeshConfig::default()
        };
        let output = adapter.generate_config(&config).unwrap();

        assert!(output.yaml.contains("Server"));
        assert!(output.yaml.contains("ServerAuthorization"));
        assert!(output.yaml.contains("ServiceProfile"));
        assert_eq!(output.resources.len(), 3);
    }

    #[test]
    fn test_linkerd_mtls_strict() {
        let adapter = LinkerdAdapter::new();
        let config = MeshConfig {
            mesh: MeshType::Linkerd,
            ..MeshConfig::default()
        };
        let output = adapter.generate_config(&config).unwrap();
        assert!(output.yaml.contains("strict"));
    }

    #[test]
    fn test_linkerd_mtls_permissive() {
        let adapter = LinkerdAdapter::new();
        let config = MeshConfig {
            mesh: MeshType::Linkerd,
            mtls: MtlsMode::Permissive,
            ..MeshConfig::default()
        };
        let output = adapter.generate_config(&config).unwrap();
        assert!(output.yaml.contains("permissive"));
    }

    #[test]
    fn test_linkerd_retry_config() {
        let adapter = LinkerdAdapter::new();
        let config = MeshConfig {
            mesh: MeshType::Linkerd,
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
            ..MeshConfig::default()
        };
        let output = adapter.generate_config(&config).unwrap();
        assert!(output.yaml.contains("5000ms"));
    }

    #[test]
    fn test_linkerd_sidecar_injection() {
        let adapter = LinkerdAdapter::new();
        let config = MeshConfig {
            mesh: MeshType::Linkerd,
            sidecar_injection: SidecarConfig {
                namespace_label: None,
                annotation: Some("linkerd.io/inject".to_string()),
            },
            ..MeshConfig::default()
        };
        let output = adapter.generate_config(&config).unwrap();
        assert!(output.yaml.contains("linkerd.io/inject: enabled"));
    }

    #[test]
    fn test_linkerd_control_plane_unavailable() {
        let adapter = LinkerdAdapter::with_control_plane(false);
        let config = MeshConfig {
            mesh: MeshType::Linkerd,
            ..MeshConfig::default()
        };
        let output = adapter.generate_config(&config).unwrap();
        assert!(output.yaml.contains("control plane unavailable"));
    }

    #[test]
    fn test_linkerd_wrong_mesh_type() {
        let adapter = LinkerdAdapter::new();
        let config = MeshConfig::default();
        let result = adapter.generate_config(&config);
        assert!(matches!(result, Err(MeshError::ConfigInvalid)));
    }

    #[test]
    fn test_linkerd_canary_annotation() {
        let adapter = LinkerdAdapter::new();
        let config = MeshConfig {
            mesh: MeshType::Linkerd,
            traffic: TrafficGovernance {
                canary: Some(CanaryConfig {
                    percentage: 20,
                    version: "v2".to_string(),
                }),
                blue_green: None,
                circuit_breaker: None,
                retry: None,
            },
            ..MeshConfig::default()
        };
        let output = adapter.generate_config(&config).unwrap();
        assert!(output.yaml.contains("canary: 20% to v2"));
    }
}
