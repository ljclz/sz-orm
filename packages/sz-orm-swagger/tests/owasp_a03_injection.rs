#![cfg(all(feature = "owasp-pentest-suite", feature = "openapi-reverse"))]

//! OWASP A03: 注入深化渗透测试（swagger 包）
//!
//! 对应 REQ-V49-003（OWASP A03 深化）
//!
//! 渗透测试向量：
//! - 表达式注入：OpenAPI spec 含恶意扩展字段（x-exec/x-eval）被拒绝

use std::collections::HashMap;

use sz_orm_swagger::reverse::{OpenApiInjectionGuard, ReverseGenError};
use sz_orm_swagger::OpenAPISpec;

/// A03-1：表达式注入被拒绝
///
/// 构造 OpenAPI spec 含恶意扩展字段 `x-exec`，
/// 断言 `OpenApiInjectionGuard::check` 返回 `InjectionDetected`。
#[test]
fn a03_expression_injection_rejected() {
    let mut paths = HashMap::new();
    paths.insert(
        "/users".to_string(),
        serde_json::json!({
            "x-exec": "rm -rf /",
            "get": {
                "summary": "Get users"
            }
        }),
    );

    let spec = OpenAPISpec {
        openapi: "3.0.0".to_string(),
        info: serde_json::json!({"title": "Test", "version": "1.0"}),
        paths,
        components: None,
        tags: vec![],
        servers: vec![],
        security: vec![],
    };

    let guard = OpenApiInjectionGuard::new();
    let result = guard.check(&spec);
    assert!(
        matches!(result, Err(ReverseGenError::InjectionDetected)),
        "恶意扩展字段 x-exec 必须被检测为注入"
    );

    let mut paths2 = HashMap::new();
    paths2.insert(
        "/orders".to_string(),
        serde_json::json!({
            "x-eval": "${7*7}",
            "post": {
                "summary": "Create order"
            }
        }),
    );

    let spec2 = OpenAPISpec {
        openapi: "3.0.0".to_string(),
        info: serde_json::json!({"title": "Test", "version": "1.0"}),
        paths: paths2,
        components: None,
        tags: vec![],
        servers: vec![],
        security: vec![],
    };

    let result2 = guard.check(&spec2);
    assert!(
        matches!(result2, Err(ReverseGenError::InjectionDetected)),
        "恶意扩展字段 x-eval 必须被检测为注入"
    );

    let mut paths3 = HashMap::new();
    paths3.insert(
        "/safe".to_string(),
        serde_json::json!({
            "get": {
                "summary": "Safe endpoint"
            }
        }),
    );

    let spec3 = OpenAPISpec {
        openapi: "3.0.0".to_string(),
        info: serde_json::json!({"title": "Test", "version": "1.0"}),
        paths: paths3,
        components: None,
        tags: vec![],
        servers: vec![],
        security: vec![],
    };

    let guard_trusting = OpenApiInjectionGuard::with_trust_unsigned();
    let result3 = guard_trusting.check(&spec3);
    assert!(result3.is_ok(), "无恶意字段的 spec 必须通过检查");
}
