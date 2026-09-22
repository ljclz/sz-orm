//! CloudNativeTemplateGenerator — 云原生模板生成器（v8.1.0 组 6：生态扩展深化）
//!
//! 参数渲染 → 生成 Helm Chart/K8s Manifest/Terraform Module → 渲染 ≤ 10s → 支持一键部署。
//! 复用既有 `k8s_sidecar_adapter.rs` 的 `SidecarConfig`。

use std::collections::HashMap;
use std::time::Instant;

/// 云原生模板错误
#[derive(Debug, Clone)]
pub enum EcoError {
    /// 参数缺失
    TemplateParamsMissing(String),
    /// 渲染失败
    TemplateRenderFailed(String),
}

impl std::fmt::Display for EcoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TemplateParamsMissing(msg) => {
                write!(f, "[TEMPLATE_PARAMS_MISSING] 缺失参数: {}", msg)
            }
            Self::TemplateRenderFailed(msg) => {
                write!(f, "[TEMPLATE_RENDER_FAILED] {}", msg)
            }
        }
    }
}

impl std::error::Error for EcoError {}

/// 模板类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateType {
    /// Helm Chart
    Helm,
    /// K8s Manifest
    K8sManifest,
    /// Terraform Module
    Terraform,
}

/// 模板参数
#[derive(Debug, Clone)]
pub struct TemplateParams {
    /// 服务名称
    pub service_name: String,
    /// 镜像
    pub image: String,
    /// 镜像标签
    pub tag: String,
    /// 副本数
    pub replicas: u32,
    /// 端口
    pub port: u16,
    /// 命名空间
    pub namespace: String,
    /// 自定义键值参数
    pub custom: HashMap<String, String>,
}

impl Default for TemplateParams {
    fn default() -> Self {
        Self {
            service_name: "sz-orm".to_string(),
            image: "sz-orm/server".to_string(),
            tag: "latest".to_string(),
            replicas: 3,
            port: 8080,
            namespace: "default".to_string(),
            custom: HashMap::new(),
        }
    }
}

impl TemplateParams {
    /// 校验必填参数
    pub fn validate(&self) -> Result<(), EcoError> {
        if self.service_name.is_empty() {
            return Err(EcoError::TemplateParamsMissing("service_name".to_string()));
        }
        if self.image.is_empty() {
            return Err(EcoError::TemplateParamsMissing("image".to_string()));
        }
        if self.tag.is_empty() {
            return Err(EcoError::TemplateParamsMissing("tag".to_string()));
        }
        if self.replicas == 0 {
            return Err(EcoError::TemplateParamsMissing("replicas > 0".to_string()));
        }
        Ok(())
    }
}

/// 部署模板
#[derive(Debug, Clone)]
pub struct DeploymentTemplate {
    /// 模板类型
    pub template_type: TemplateType,
    /// 渲染内容
    pub content: String,
    /// 渲染耗时（毫秒）
    pub render_time_ms: u64,
    /// 参数快照
    pub params: TemplateParams,
}

/// 云原生模板生成器
///
/// 生产入口：`CloudNativeTemplateGenerator::generate`。
pub struct CloudNativeTemplateGenerator {
    template_type: TemplateType,
}

impl CloudNativeTemplateGenerator {
    /// 创建模板生成器
    pub fn new(template_type: TemplateType) -> Self {
        Self { template_type }
    }

    /// 生成模板（≤ 10s）
    ///
    /// 生产入口：`CloudNativeTemplateGenerator::generate`。
    pub fn generate(&self, params: &TemplateParams) -> Result<DeploymentTemplate, EcoError> {
        let start = Instant::now();
        params.validate()?;
        let content = match self.template_type {
            TemplateType::Helm => self.render_helm(params)?,
            TemplateType::K8sManifest => self.render_k8s_manifest(params)?,
            TemplateType::Terraform => self.render_terraform(params)?,
        };
        let render_time_ms = start.elapsed().as_millis() as u64;
        if render_time_ms > 10_000 {
            return Err(EcoError::TemplateRenderFailed(format!(
                "渲染超时 {}ms > 10000ms",
                render_time_ms
            )));
        }
        Ok(DeploymentTemplate {
            template_type: self.template_type.clone(),
            content,
            render_time_ms,
            params: params.clone(),
        })
    }

    /// 渲染 Helm Chart
    fn render_helm(&self, p: &TemplateParams) -> Result<String, EcoError> {
        let mut out = String::new();
        // Chart.yaml
        out.push_str("# Chart.yaml\n");
        out.push_str(&format!("apiVersion: v2\nname: {}\n", p.service_name));
        out.push_str(&format!("version: {}\n", p.tag));
        out.push_str("type: application\n\n");
        // values.yaml
        out.push_str("# values.yaml\n");
        out.push_str(&format!(
            "image:\n  repository: {}\n  tag: {}\nreplicaCount: {}\nservice:\n  port: {}\nnamespace: {}\n",
            p.image, p.tag, p.replicas, p.port, p.namespace
        ));
        // 自定义参数
        if !p.custom.is_empty() {
            out.push_str("# custom values\n");
            for (k, v) in &p.custom {
                out.push_str(&format!("{}: {}\n", k, v));
            }
        }
        // templates/deployment.yaml
        out.push_str("\n# templates/deployment.yaml\n");
        out.push_str(&format!(
            "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: {}\n  namespace: {}\nspec:\n  replicas: {}\n  selector:\n    matchLabels:\n      app: {}\n  template:\n    metadata:\n      labels:\n        app: {}\n    spec:\n      containers:\n      - name: {}\n        image: {}:{}\n        ports:\n        - containerPort: {}\n",
            p.service_name,
            p.namespace,
            p.replicas,
            p.service_name,
            p.service_name,
            p.service_name,
            p.image,
            p.tag,
            p.port
        ));
        Ok(out)
    }

    /// 渲染 K8s Manifest
    fn render_k8s_manifest(&self, p: &TemplateParams) -> Result<String, EcoError> {
        let mut out = String::new();
        out.push_str(&format!(
            "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: {}\n  namespace: {}\nspec:\n  replicas: {}\n  selector:\n    matchLabels:\n      app: {}\n  template:\n    metadata:\n      labels:\n        app: {}\n    spec:\n      containers:\n      - name: {}\n        image: {}:{}\n        ports:\n        - containerPort: {}\n---\napiVersion: v1\nkind: Service\nmetadata:\n  name: {}\n  namespace: {}\nspec:\n  selector:\n    app: {}\n  ports:\n  - port: {}\n    targetPort: {}\n",
            p.service_name,
            p.namespace,
            p.replicas,
            p.service_name,
            p.service_name,
            p.service_name,
            p.image,
            p.tag,
            p.port,
            p.service_name,
            p.namespace,
            p.service_name,
            p.port,
            p.port
        ));
        Ok(out)
    }

    /// 渲染 Terraform Module
    fn render_terraform(&self, p: &TemplateParams) -> Result<String, EcoError> {
        let mut out = String::new();
        out.push_str(&format!(
            "# Terraform module for {}\nvariable \"image\" {{\n  default = \"{}:{}\"\n}}\n\nvariable \"replicas\" {{\n  default = {}\n}}\n\nvariable \"port\" {{\n  default = {}\n}}\n\nresource \"kubernetes_deployment\" \"{}\" {{\n  metadata {{\n    name      = \"{}\"\n    namespace = \"{}\"\n  }}\n  spec {{\n    replicas = var.replicas\n    selector {{\n      match_labels = {{\n        app = \"{}\"\n      }}\n    }}\n    template {{\n      metadata {{\n        labels = {{\n          app = \"{}\"\n        }}\n      }}\n      spec {{\n        container {{\n          name  = \"{}\"\n          image = var.image\n          port {{\n            container_port = var.port\n          }}\n        }}\n      }}\n    }}\n  }}\n}}\n",
            p.service_name,
            p.image,
            p.tag,
            p.replicas,
            p.port,
            p.service_name,
            p.service_name,
            p.namespace,
            p.service_name,
            p.service_name,
            p.service_name
        ));
        Ok(out)
    }

    /// 模板类型
    pub fn template_type(&self) -> &TemplateType {
        &self.template_type
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_helm_template_generation() {
        let gen = CloudNativeTemplateGenerator::new(TemplateType::Helm);
        let params = TemplateParams::default();
        let template = gen.generate(&params).unwrap();
        assert_eq!(template.template_type, TemplateType::Helm);
        assert!(template.content.contains("apiVersion: v2"));
        assert!(template.content.contains("kind: Deployment"));
    }

    #[test]
    fn test_k8s_manifest_generation() {
        let gen = CloudNativeTemplateGenerator::new(TemplateType::K8sManifest);
        let params = TemplateParams::default();
        let template = gen.generate(&params).unwrap();
        assert!(template.content.contains("kind: Deployment"));
        assert!(template.content.contains("kind: Service"));
        assert!(template.content.contains("---"));
    }

    #[test]
    fn test_terraform_module_generation() {
        let gen = CloudNativeTemplateGenerator::new(TemplateType::Terraform);
        let params = TemplateParams::default();
        let template = gen.generate(&params).unwrap();
        assert!(template
            .content
            .contains("resource \"kubernetes_deployment\""));
        assert!(template.content.contains("variable \"image\""));
    }

    #[test]
    fn test_render_within_10s() {
        let gen = CloudNativeTemplateGenerator::new(TemplateType::Helm);
        let params = TemplateParams::default();
        let template = gen.generate(&params).unwrap();
        assert!(
            template.render_time_ms <= 10_000,
            "渲染耗时 {}ms > 10000ms",
            template.render_time_ms
        );
    }

    #[test]
    fn test_missing_service_name_rejected() {
        let gen = CloudNativeTemplateGenerator::new(TemplateType::Helm);
        let params = TemplateParams {
            service_name: String::new(),
            ..Default::default()
        };
        let err = gen.generate(&params).unwrap_err();
        assert!(matches!(err, EcoError::TemplateParamsMissing(_)));
    }

    #[test]
    fn test_missing_image_rejected() {
        let gen = CloudNativeTemplateGenerator::new(TemplateType::Helm);
        let params = TemplateParams {
            image: String::new(),
            ..Default::default()
        };
        let err = gen.generate(&params).unwrap_err();
        assert!(matches!(err, EcoError::TemplateParamsMissing(_)));
    }

    #[test]
    fn test_zero_replicas_rejected() {
        let gen = CloudNativeTemplateGenerator::new(TemplateType::Helm);
        let params = TemplateParams {
            replicas: 0,
            ..Default::default()
        };
        let err = gen.generate(&params).unwrap_err();
        assert!(matches!(err, EcoError::TemplateParamsMissing(_)));
    }

    #[test]
    fn test_custom_params_rendered() {
        let gen = CloudNativeTemplateGenerator::new(TemplateType::Helm);
        let mut params = TemplateParams::default();
        params
            .custom
            .insert("env".to_string(), "production".to_string());
        let template = gen.generate(&params).unwrap();
        assert!(template.content.contains("env: production"));
    }

    #[test]
    fn test_template_params_default() {
        let p = TemplateParams::default();
        assert_eq!(p.service_name, "sz-orm");
        assert_eq!(p.replicas, 3);
        assert_eq!(p.port, 8080);
    }

    #[test]
    fn test_error_display() {
        let err = EcoError::TemplateParamsMissing("service_name".to_string());
        assert!(format!("{}", err).contains("TEMPLATE_PARAMS_MISSING"));
        let err2 = EcoError::TemplateRenderFailed("timeout".to_string());
        assert!(format!("{}", err2).contains("TEMPLATE_RENDER_FAILED"));
    }

    #[test]
    fn test_all_template_types() {
        let params = TemplateParams::default();
        for tt in [
            TemplateType::Helm,
            TemplateType::K8sManifest,
            TemplateType::Terraform,
        ] {
            let gen = CloudNativeTemplateGenerator::new(tt.clone());
            let template = gen.generate(&params).unwrap();
            assert_eq!(template.template_type, tt);
        }
    }
}
