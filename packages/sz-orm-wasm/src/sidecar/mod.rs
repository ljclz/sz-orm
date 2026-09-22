//! v8.0.0 组 6：K8s sidecar 适配器（cloudnative-sidecar feature gate）
//! v8.1.0 组 6：云原生模板生成器（cloudnative-template feature gate）
//!
//! 提供 `K8sSidecarAdapter`，复用既有 `WasmDatabase` 和 `serverless-adapt`。
//! v8.1.0 新增 `CloudNativeTemplateGenerator`。

pub mod k8s_sidecar_adapter;

pub use k8s_sidecar_adapter::{HealthStatus, K8sSidecarAdapter, SidecarConfig, SidecarStatus};

// v8.1.0 组 6：云原生模板生成器（cloudnative-template feature gate）
#[cfg(feature = "cloudnative-template")]
pub mod template_generator;
#[cfg(feature = "cloudnative-template")]
pub use template_generator::{
    CloudNativeTemplateGenerator, DeploymentTemplate, EcoError as TemplateEcoError, TemplateParams,
    TemplateType,
};
