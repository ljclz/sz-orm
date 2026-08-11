//! # 服务网格集成
//!
//! 提供 `ServiceMeshAdapter` trait、`MeshConfig`/`MtlsMode`/`TrafficGovernance` 数据结构，
//! `IstioAdapter`/`LinkerdAdapter` 实现，可观测性接入。

#![allow(missing_docs)]

pub mod istio;
pub mod linkerd;
pub mod observability;

use std::fmt;

/// 网格类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeshType {
    Istio,
    Linkerd,
}

/// mTLS 模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MtlsMode {
    #[default]
    Strict,
    Permissive,
}

/// 金丝雀配置
#[derive(Debug, Clone)]
pub struct CanaryConfig {
    pub percentage: u8,
    pub version: String,
}

/// 蓝绿配置
#[derive(Debug, Clone)]
pub struct BlueGreenConfig {
    pub active_version: String,
    pub preview_version: String,
}

/// 熔断配置
#[derive(Debug, Clone)]
pub struct CircuitConfig {
    pub max_connections: u32,
    pub max_pending_requests: u32,
    pub max_retries: u32,
    pub outlier_consecutive_errors: u32,
}

/// 重试配置
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub attempts: u32,
    pub retry_on: Vec<String>,
    pub per_try_timeout_ms: u64,
}

/// 流量治理
#[derive(Debug, Clone)]
pub struct TrafficGovernance {
    pub canary: Option<CanaryConfig>,
    pub blue_green: Option<BlueGreenConfig>,
    pub circuit_breaker: Option<CircuitConfig>,
    pub retry: Option<RetryConfig>,
}

/// Sidecar 注入配置
#[derive(Debug, Clone)]
pub struct SidecarConfig {
    pub namespace_label: Option<String>,
    pub annotation: Option<String>,
}

/// 网格配置
#[derive(Debug, Clone)]
pub struct MeshConfig {
    pub mesh: MeshType,
    pub mtls: MtlsMode,
    pub traffic: TrafficGovernance,
    pub sidecar_injection: SidecarConfig,
}

impl Default for MeshConfig {
    fn default() -> Self {
        Self {
            mesh: MeshType::Istio,
            mtls: MtlsMode::Strict,
            traffic: TrafficGovernance {
                canary: None,
                blue_green: None,
                circuit_breaker: None,
                retry: None,
            },
            sidecar_injection: SidecarConfig {
                namespace_label: None,
                annotation: None,
            },
        }
    }
}

/// 网格配置输出
#[derive(Debug, Clone)]
pub struct MeshConfigOutput {
    pub yaml: String,
    pub resources: Vec<String>,
}

/// 网格错误
#[derive(Debug, Clone)]
pub enum MeshError {
    ControlPlaneUnavailable,
    MtlsConflict { existing: MtlsMode, new: MtlsMode },
    ConfigInvalid,
}

impl fmt::Display for MeshError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MeshError::ControlPlaneUnavailable => write!(f, "mesh control plane unavailable"),
            MeshError::MtlsConflict { existing, new } => {
                write!(f, "mTLS conflict: existing={existing:?}, new={new:?}")
            }
            MeshError::ConfigInvalid => write!(f, "mesh config invalid"),
        }
    }
}

impl std::error::Error for MeshError {}

/// 服务网格适配器 trait
pub trait ServiceMeshAdapter: Send + Sync {
    fn generate_config(&self, config: &MeshConfig) -> Result<MeshConfigOutput, MeshError>;
    fn mesh_type(&self) -> &'static str;
}

pub use istio::IstioAdapter;
pub use linkerd::LinkerdAdapter;
pub use observability::MeshObservability;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mesh_config_default() {
        let config = MeshConfig::default();
        assert_eq!(config.mesh, MeshType::Istio);
        assert_eq!(config.mtls, MtlsMode::Strict);
    }

    #[test]
    fn test_mtls_mode_default() {
        assert_eq!(MtlsMode::default(), MtlsMode::Strict);
    }

    #[test]
    fn test_mesh_error_display() {
        assert!(MeshError::ControlPlaneUnavailable
            .to_string()
            .contains("unavailable"));
        assert!(MeshError::ConfigInvalid.to_string().contains("invalid"));
    }

    #[test]
    fn test_mtls_conflict_display() {
        let err = MeshError::MtlsConflict {
            existing: MtlsMode::Permissive,
            new: MtlsMode::Strict,
        };
        assert!(err.to_string().contains("conflict"));
    }

    #[test]
    fn test_traffic_governance_default() {
        let config = MeshConfig::default();
        assert!(config.traffic.canary.is_none());
        assert!(config.traffic.blue_green.is_none());
        assert!(config.traffic.circuit_breaker.is_none());
        assert!(config.traffic.retry.is_none());
    }
}
