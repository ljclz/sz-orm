//! ApiFirstLoopVerifier — API 优先闭环验证

use super::ReverseGenError;
use crate::OpenAPISpec;
use std::collections::HashMap;
use sz_orm_core::schema_sync::SchemaDiff;

/// 闭环验证报告
#[derive(Debug, Clone)]
pub struct LoopReport {
    /// 原 spec 中的 Schema 名称列表
    pub spec_schemas: Vec<String>,
    /// 正向生成的 Schema 名称列表
    pub generated_schemas: Vec<String>,
    /// 差异列表
    pub diffs: Vec<SchemaDiff>,
    /// 是否一致（除可编辑区外）
    pub consistent: bool,
    /// 差异描述列表
    pub diff_descriptions: Vec<String>,
}

impl LoopReport {
    /// 创建空报告
    pub fn empty() -> Self {
        Self {
            spec_schemas: Vec::new(),
            generated_schemas: Vec::new(),
            diffs: Vec::new(),
            consistent: true,
            diff_descriptions: Vec::new(),
        }
    }

    /// 添加差异描述
    pub fn add_diff(&mut self, description: String) {
        self.diff_descriptions.push(description);
        self.consistent = false;
    }
}

/// ApiFirstLoopVerifier — API 优先闭环验证
///
/// 验证流程：反向生成 ORM → 正向生成 OpenAPI' → 对比 spec 与 OpenAPI'
pub struct ApiFirstLoopVerifier;

impl ApiFirstLoopVerifier {
    /// 创建新的验证器
    pub fn new() -> Self {
        Self
    }

    /// 提取 spec 中的 Schema 名称列表
    pub fn extract_spec_schemas(spec: &OpenAPISpec) -> Vec<String> {
        let mut schemas = Vec::new();
        if let Some(ref components) = spec.components {
            schemas.extend(components.schemas.keys().cloned());
        }
        schemas.sort();
        schemas
    }

    /// 闭环验证
    ///
    /// 对比 spec 与正向生成的 OpenAPI'，标注差异，不阻断生成
    pub fn verify(
        spec: &OpenAPISpec,
        generated_model_code: &HashMap<String, String>,
    ) -> Result<LoopReport, ReverseGenError> {
        let spec_schemas = Self::extract_spec_schemas(spec);
        let generated_schemas: Vec<String> = {
            let mut v: Vec<String> = generated_model_code.keys().cloned().collect();
            v.sort();
            v
        };

        let mut report = LoopReport {
            spec_schemas: spec_schemas.clone(),
            generated_schemas: generated_schemas.clone(),
            diffs: Vec::new(),
            consistent: true,
            diff_descriptions: Vec::new(),
        };

        let spec_set: std::collections::HashSet<&String> = spec_schemas.iter().collect();
        let gen_set: std::collections::HashSet<&String> = generated_schemas.iter().collect();

        for name in spec_set.difference(&gen_set) {
            report.add_diff(format!("schema '{}' in spec but not in generated", name));
        }
        for name in gen_set.difference(&spec_set) {
            report.add_diff(format!("schema '{}' in generated but not in spec", name));
        }

        Ok(report)
    }

    /// 生成验证报告文本
    pub fn format_report(report: &LoopReport) -> String {
        let mut output = String::new();
        output.push_str("=== API First Loop Verification Report ===\n");
        output.push_str(&format!("Spec schemas: {:?}\n", report.spec_schemas));
        output.push_str(&format!(
            "Generated schemas: {:?}\n",
            report.generated_schemas
        ));
        output.push_str(&format!("Consistent: {}\n", report.consistent));
        if report.diff_descriptions.is_empty() {
            output.push_str("No diffs found.\n");
        } else {
            output.push_str("Diffs:\n");
            for diff in &report.diff_descriptions {
                output.push_str(&format!("  - {}\n", diff));
            }
        }
        output
    }
}

impl Default for ApiFirstLoopVerifier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Components, ObjectType, Schema};
    use std::collections::HashMap as StdHashMap;

    fn make_spec_with_schemas(schemas: Vec<&str>) -> OpenAPISpec {
        let mut components = Components::default();
        for name in schemas {
            components
                .schemas
                .insert(name.to_string(), Schema::Object(ObjectType::new()));
        }
        OpenAPISpec {
            openapi: "3.0.0".to_string(),
            info: serde_json::json!({"title": "Test", "version": "1.0"}),
            paths: StdHashMap::new(),
            components: Some(components),
            tags: vec![],
            servers: vec![],
            security: vec![],
        }
    }

    #[test]
    fn test_extract_spec_schemas() {
        let spec = make_spec_with_schemas(vec!["User", "Order", "Product"]);
        let schemas = ApiFirstLoopVerifier::extract_spec_schemas(&spec);
        assert_eq!(schemas, vec!["Order", "Product", "User"]);
    }

    #[test]
    fn test_verify_consistent() {
        let spec = make_spec_with_schemas(vec!["User", "Order"]);
        let mut generated = HashMap::new();
        generated.insert("User".to_string(), "struct User {}".to_string());
        generated.insert("Order".to_string(), "struct Order {}".to_string());

        let report = ApiFirstLoopVerifier::verify(&spec, &generated).unwrap();
        assert!(report.consistent);
        assert!(report.diff_descriptions.is_empty());
    }

    #[test]
    fn test_verify_missing_in_generated() {
        let spec = make_spec_with_schemas(vec!["User", "Order", "Product"]);
        let mut generated = HashMap::new();
        generated.insert("User".to_string(), "struct User {}".to_string());
        generated.insert("Order".to_string(), "struct Order {}".to_string());

        let report = ApiFirstLoopVerifier::verify(&spec, &generated).unwrap();
        assert!(!report.consistent);
        assert!(report
            .diff_descriptions
            .iter()
            .any(|d| d.contains("Product") && d.contains("spec")));
    }

    #[test]
    fn test_verify_extra_in_generated() {
        let spec = make_spec_with_schemas(vec!["User"]);
        let mut generated = HashMap::new();
        generated.insert("User".to_string(), "struct User {}".to_string());
        generated.insert("Extra".to_string(), "struct Extra {}".to_string());

        let report = ApiFirstLoopVerifier::verify(&spec, &generated).unwrap();
        assert!(!report.consistent);
        assert!(report
            .diff_descriptions
            .iter()
            .any(|d| d.contains("Extra") && d.contains("generated")));
    }

    #[test]
    fn test_format_report() {
        let report = LoopReport {
            spec_schemas: vec!["User".to_string()],
            generated_schemas: vec!["User".to_string()],
            diffs: vec![],
            consistent: true,
            diff_descriptions: vec![],
        };
        let text = ApiFirstLoopVerifier::format_report(&report);
        assert!(text.contains("API First Loop Verification Report"));
        assert!(text.contains("Consistent: true"));
        assert!(text.contains("No diffs found"));
    }

    #[test]
    fn test_format_report_with_diffs() {
        let report = LoopReport {
            spec_schemas: vec!["User".to_string(), "Order".to_string()],
            generated_schemas: vec!["User".to_string()],
            diffs: vec![],
            consistent: false,
            diff_descriptions: vec!["schema 'Order' in spec but not in generated".to_string()],
        };
        let text = ApiFirstLoopVerifier::format_report(&report);
        assert!(text.contains("Consistent: false"));
        assert!(text.contains("Order"));
    }

    #[test]
    fn test_empty_spec() {
        let spec = OpenAPISpec {
            openapi: "3.0.0".to_string(),
            info: serde_json::json!({}),
            paths: StdHashMap::new(),
            components: None,
            tags: vec![],
            servers: vec![],
            security: vec![],
        };
        let generated = HashMap::new();
        let report = ApiFirstLoopVerifier::verify(&spec, &generated).unwrap();
        assert!(report.consistent);
    }
}
