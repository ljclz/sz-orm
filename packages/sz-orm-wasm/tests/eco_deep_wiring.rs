//! 生态扩展深化端到端接线测试（v8.1.0 组 6）
//!
//! ③ CloudNativeTemplateGenerator Helm Chart 生成 ≤ 10s 一键部署

#![cfg(feature = "cloudnative-template")]

use std::time::Instant;

use sz_orm_wasm::sidecar::{
    CloudNativeTemplateGenerator, TemplateEcoError, TemplateParams, TemplateType,
};

/// 端到端测试 ③：CloudNativeTemplateGenerator Helm Chart 生成 ≤ 10s 一键部署
#[test]
fn test_eco_deep_cloudnative_helm_template() {
    let start = Instant::now();
    let gen = CloudNativeTemplateGenerator::new(TemplateType::Helm);
    let params = TemplateParams {
        service_name: "sz-orm-e2e".to_string(),
        image: "sz-orm/server".to_string(),
        tag: "v8.1.0".to_string(),
        replicas: 3,
        port: 8080,
        namespace: "production".to_string(),
        custom: std::collections::HashMap::new(),
    };

    let template = gen.generate(&params).unwrap();
    assert_eq!(template.template_type, TemplateType::Helm);
    assert!(template.content.contains("apiVersion: v2"));
    assert!(template.content.contains("kind: Deployment"));
    assert!(template.content.contains("sz-orm-e2e"));
    assert!(template.content.contains("v8.1.0"));

    let elapsed = start.elapsed();
    assert!(
        elapsed <= std::time::Duration::from_secs(10),
        "Helm 模板渲染 {:?} > 10s",
        elapsed
    );
}

/// 端到端测试 ③b：K8s Manifest 生成
#[test]
fn test_eco_deep_cloudnative_k8s_manifest() {
    let gen = CloudNativeTemplateGenerator::new(TemplateType::K8sManifest);
    let params = TemplateParams::default();
    let template = gen.generate(&params).unwrap();
    assert!(template.content.contains("kind: Deployment"));
    assert!(template.content.contains("kind: Service"));
}

/// 端到端测试 ③c：Terraform Module 生成
#[test]
fn test_eco_deep_cloudnative_terraform() {
    let gen = CloudNativeTemplateGenerator::new(TemplateType::Terraform);
    let params = TemplateParams::default();
    let template = gen.generate(&params).unwrap();
    assert!(template
        .content
        .contains("resource \"kubernetes_deployment\""));
}

/// 端到端测试 ③d：参数缺失拒绝
#[test]
fn test_eco_deep_cloudnative_missing_params() {
    let gen = CloudNativeTemplateGenerator::new(TemplateType::Helm);
    let params = TemplateParams {
        service_name: String::new(),
        ..Default::default()
    };
    let err = gen.generate(&params).unwrap_err();
    assert!(matches!(err, TemplateEcoError::TemplateParamsMissing(_)));
}
