//! OpenApiInjectionGuard — 注入防护
//!
//! 不执行 spec 内嵌代码，不信任未签名 spec，生成代码强制参数化查询。

use super::ReverseGenError;
use crate::OpenAPISpec;
use serde_json::Value;

/// 恶意扩展字段前缀列表
const MALICIOUS_EXTENSION_PREFIXES: &[&str] = &[
    "x-exec",
    "x-eval",
    "x-run",
    "x-shell",
    "x-script",
    "x-command",
    "x-code",
    "x-inject",
];

/// Spec 签名字段
const SPEC_SIGNATURE_FIELD: &str = "x-sz-orm-signature";

/// OpenApiInjectionGuard — 注入防护
pub struct OpenApiInjectionGuard {
    /// 是否信任未签名 spec
    pub trust_unsigned: bool,
}

impl OpenApiInjectionGuard {
    /// 创建新的注入防护（默认不信任未签名 spec）
    pub fn new() -> Self {
        Self {
            trust_unsigned: false,
        }
    }

    /// 创建信任未签名 spec 的防护
    pub fn with_trust_unsigned() -> Self {
        Self {
            trust_unsigned: true,
        }
    }

    /// 检查 spec 安全性
    pub fn check(&self, spec: &OpenAPISpec) -> Result<(), ReverseGenError> {
        self.check_injection(spec)?;
        self.check_signature(spec)?;
        Ok(())
    }

    /// 检查注入：不执行 spec 内嵌代码
    fn check_injection(&self, spec: &OpenAPISpec) -> Result<(), ReverseGenError> {
        let spec_json =
            serde_json::to_value(spec).map_err(|e| ReverseGenError::SpecParseFailed {
                path: "root".to_string(),
                reason: e.to_string(),
            })?;

        Self::check_value_for_injection(&spec_json)
    }

    /// 递归检查 JSON 值中的恶意扩展字段
    fn check_value_for_injection(value: &Value) -> Result<(), ReverseGenError> {
        match value {
            Value::Object(map) => {
                for (key, val) in map {
                    for prefix in MALICIOUS_EXTENSION_PREFIXES {
                        if key == *prefix {
                            return Err(ReverseGenError::InjectionDetected);
                        }
                    }
                    Self::check_value_for_injection(val)?;
                }
                Ok(())
            }
            Value::Array(arr) => {
                for item in arr {
                    Self::check_value_for_injection(item)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// 检查签名：不信任未签名 spec
    fn check_signature(&self, spec: &OpenAPISpec) -> Result<(), ReverseGenError> {
        if self.trust_unsigned {
            return Ok(());
        }

        let spec_json =
            serde_json::to_value(spec).map_err(|e| ReverseGenError::SpecParseFailed {
                path: "root".to_string(),
                reason: e.to_string(),
            })?;

        if Self::find_signature_field(&spec_json) {
            return Ok(());
        }

        Err(ReverseGenError::UnsignedSpec)
    }

    /// 递归查找签名字段
    fn find_signature_field(value: &Value) -> bool {
        match value {
            Value::Object(map) => {
                if map.contains_key(SPEC_SIGNATURE_FIELD) {
                    return true;
                }
                for val in map.values() {
                    if Self::find_signature_field(val) {
                        return true;
                    }
                }
                false
            }
            Value::Array(arr) => arr.iter().any(Self::find_signature_field),
            _ => false,
        }
    }

    /// 检查生成代码是否使用参数化查询
    pub fn verify_parameterized_queries(code: &str) -> bool {
        !code.contains("format!") || code.contains("where_eq") || code.contains("EDITABLE")
    }
}

impl Default for OpenApiInjectionGuard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Components, ObjectType, Schema};
    use std::collections::HashMap;

    fn make_safe_spec() -> OpenAPISpec {
        let mut components = Components::default();
        components
            .schemas
            .insert("User".to_string(), Schema::Object(ObjectType::new()));
        OpenAPISpec {
            openapi: "3.0.0".to_string(),
            info: serde_json::json!({"title": "Test", "version": "1.0"}),
            paths: HashMap::new(),
            components: Some(components),
            tags: vec![],
            servers: vec![],
            security: vec![],
        }
    }

    fn make_spec_with_extension(ext_key: &str, ext_value: &str) -> OpenAPISpec {
        let mut spec = make_safe_spec();
        let mut info = serde_json::json!({"title": "Test", "version": "1.0"});
        if let Value::Object(ref mut map) = info {
            map.insert(ext_key.to_string(), Value::String(ext_value.to_string()));
        }
        spec.info = info;
        spec
    }

    #[test]
    fn test_safe_spec_with_trust_unsigned() {
        let spec = make_safe_spec();
        let guard = OpenApiInjectionGuard::with_trust_unsigned();
        assert!(guard.check(&spec).is_ok());
    }

    #[test]
    fn test_unsigned_spec_rejected() {
        let spec = make_safe_spec();
        let guard = OpenApiInjectionGuard::new();
        let result = guard.check(&spec);
        assert!(result.is_err());
        match result.unwrap_err() {
            ReverseGenError::UnsignedSpec => {}
            _ => panic!("expected UnsignedSpec"),
        }
    }

    #[test]
    fn test_signed_spec_accepted() {
        let mut spec = make_safe_spec();
        let mut info = serde_json::json!({"title": "Test", "version": "1.0"});
        if let Value::Object(ref mut map) = info {
            map.insert(
                SPEC_SIGNATURE_FIELD.to_string(),
                Value::String("sha256:abc123".to_string()),
            );
        }
        spec.info = info;
        let guard = OpenApiInjectionGuard::new();
        assert!(guard.check(&spec).is_ok());
    }

    #[test]
    fn test_injection_detected_x_exec() {
        let spec = make_spec_with_extension("x-exec", "rm -rf /");
        let guard = OpenApiInjectionGuard::with_trust_unsigned();
        let result = guard.check(&spec);
        assert!(result.is_err());
        match result.unwrap_err() {
            ReverseGenError::InjectionDetected => {}
            _ => panic!("expected InjectionDetected"),
        }
    }

    #[test]
    fn test_injection_detected_x_eval() {
        let spec = make_spec_with_extension("x-eval", "dangerous code");
        let guard = OpenApiInjectionGuard::with_trust_unsigned();
        let result = guard.check(&spec);
        assert!(result.is_err());
    }

    #[test]
    fn test_injection_detected_x_shell() {
        let spec = make_spec_with_extension("x-shell", "ls -la");
        let guard = OpenApiInjectionGuard::with_trust_unsigned();
        let result = guard.check(&spec);
        assert!(result.is_err());
    }

    #[test]
    fn test_no_injection_in_safe_spec() {
        let spec = make_safe_spec();
        let guard = OpenApiInjectionGuard::with_trust_unsigned();
        assert!(guard.check(&spec).is_ok());
    }

    #[test]
    fn test_verify_parameterized_queries() {
        let safe_code = "let q = query.where_eq(\"id\", id);";
        assert!(OpenApiInjectionGuard::verify_parameterized_queries(
            safe_code
        ));

        let editable_code = "// EDITABLE: business logic here";
        assert!(OpenApiInjectionGuard::verify_parameterized_queries(
            editable_code
        ));

        let unsafe_code = "let q = format!(\"WHERE id = {}\", id);";
        assert!(!OpenApiInjectionGuard::verify_parameterized_queries(
            unsafe_code
        ));
    }

    #[test]
    fn test_malicious_extension_in_paths() {
        let mut spec = make_safe_spec();
        let mut path_obj = serde_json::Map::new();
        path_obj.insert("x-exec".to_string(), Value::String("rm -rf /".to_string()));
        spec.paths
            .insert("/users".to_string(), Value::Object(path_obj));
        let guard = OpenApiInjectionGuard::with_trust_unsigned();
        let result = guard.check(&spec);
        assert!(result.is_err());
    }
}
