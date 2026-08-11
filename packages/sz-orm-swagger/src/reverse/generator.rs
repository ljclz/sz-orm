//! OpenApiReverseGenerator — 反向生成器主入口

use super::config::ReverseGenConfig;
use super::injection_guard::OpenApiInjectionGuard;
use super::loop_verifier::{ApiFirstLoopVerifier, LoopReport};
use super::migration_mapper::OpenApiToMigrationMapper;
use super::model_mapper::SchemaToModelMapper;
use super::repository_mapper::OpenApiToRepositoryMapper;
use super::ReverseGenError;
use crate::OpenAPISpec;
use std::collections::HashMap;
use sz_orm_core::dialect_security::Dialect;
use sz_orm_core::migration::Migration;

/// 反向生成结果
#[derive(Debug)]
pub struct ReverseGenResult {
    /// 生成的 Model 代码（schema_name → Rust 代码）
    pub model_code: HashMap<String, String>,
    /// 生成的迁移文件（方言 → Migration）
    pub migrations: Vec<Migration>,
    /// 生成的 Repository 代码（schema_name → Rust 代码）
    pub repository_code: HashMap<String, String>,
    /// 闭环验证报告
    pub loop_report: LoopReport,
    /// 使用的方言
    pub dialect: Dialect,
}

/// OpenApiReverseGenerator — 反向生成器主入口
///
/// 编排流程：注入防护 → 解析 spec → Schema → Model/迁移/Repository → 闭环验证
pub struct OpenApiReverseGenerator {
    /// 配置
    pub config: ReverseGenConfig,
}

impl OpenApiReverseGenerator {
    /// 创建新的反向生成器
    pub fn new(config: ReverseGenConfig) -> Self {
        Self { config }
    }

    /// 从 spec 生成 ORM 代码
    pub fn generate(&self, spec: &OpenAPISpec) -> Result<ReverseGenResult, ReverseGenError> {
        let guard = if self.config.trust_unsigned {
            OpenApiInjectionGuard::with_trust_unsigned()
        } else {
            OpenApiInjectionGuard::new()
        };
        guard.check(spec)?;

        let schemas: HashMap<String, crate::Schema> = {
            if let Some(ref components) = spec.components {
                components.schemas.clone()
            } else {
                HashMap::new()
            }
        };

        if schemas.is_empty() {
            return Err(ReverseGenError::SpecParseFailed {
                path: "components.schemas".to_string(),
                reason: "no schemas found in spec".to_string(),
            });
        }

        let model_code = SchemaToModelMapper::generate_models(&schemas)?;

        let migration_mapper = OpenApiToMigrationMapper::new(self.config.target_dialect);
        let migrations = migration_mapper.generate_migrations(&schemas)?;

        let repository_code = OpenApiToRepositoryMapper::generate_repositories(&schemas)?;

        let loop_report = ApiFirstLoopVerifier::verify(spec, &model_code)?;

        Ok(ReverseGenResult {
            model_code,
            migrations,
            repository_code,
            loop_report,
            dialect: self.config.target_dialect,
        })
    }

    /// 从 spec JSON 字符串生成
    pub fn generate_from_json(&self, spec_json: &str) -> Result<ReverseGenResult, ReverseGenError> {
        let spec: OpenAPISpec =
            serde_json::from_str(spec_json).map_err(|e| ReverseGenError::SpecParseFailed {
                path: "root".to_string(),
                reason: e.to_string(),
            })?;
        self.generate(&spec)
    }

    /// 从 spec 文件生成
    pub fn generate_from_file(&self, spec_path: &str) -> Result<ReverseGenResult, ReverseGenError> {
        let content =
            std::fs::read_to_string(spec_path).map_err(|e| ReverseGenError::SpecParseFailed {
                path: spec_path.to_string(),
                reason: e.to_string(),
            })?;

        if spec_path.ends_with(".json") {
            self.generate_from_json(&content)
        } else {
            let spec: OpenAPISpec =
                serde_json::from_str(&content).map_err(|e| ReverseGenError::SpecParseFailed {
                    path: spec_path.to_string(),
                    reason: e.to_string(),
                })?;
            self.generate(&spec)
        }
    }

    /// 生成结果摘要
    pub fn summarize(result: &ReverseGenResult) -> String {
        let mut summary = String::new();
        summary.push_str("=== OpenAPI Reverse Generation Result ===\n");
        summary.push_str(&format!("Dialect: {:?}\n", result.dialect));
        summary.push_str(&format!("Models generated: {}\n", result.model_code.len()));
        summary.push_str(&format!(
            "Migrations generated: {}\n",
            result.migrations.len()
        ));
        summary.push_str(&format!(
            "Repositories generated: {}\n",
            result.repository_code.len()
        ));
        summary.push_str(&format!(
            "Loop verification consistent: {}\n",
            result.loop_report.consistent
        ));
        if !result.loop_report.diff_descriptions.is_empty() {
            summary.push_str("Loop verification diffs:\n");
            for diff in &result.loop_report.diff_descriptions {
                summary.push_str(&format!("  - {}\n", diff));
            }
        }
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Components, ObjectType, PrimitiveSchema, Schema};
    use std::collections::HashMap as StdHashMap;

    fn make_user_spec() -> OpenAPISpec {
        let mut components = Components::default();

        let mut user_obj = ObjectType::new();
        user_obj = user_obj.with_required_property("id", Schema::integer());
        user_obj = user_obj.with_required_property(
            "name",
            Schema::Primitive(PrimitiveSchema::string().with_length_range(0, 255)),
        );
        user_obj = user_obj.with_property("email", Schema::string());
        components
            .schemas
            .insert("User".to_string(), Schema::Object(user_obj));

        let mut info = serde_json::Map::new();
        info.insert("title".to_string(), value_str("Test API"));
        info.insert("version".to_string(), value_str("1.0"));
        info.insert("x-sz-orm-signature".to_string(), value_str("sha256:abc123"));

        OpenAPISpec {
            openapi: "3.0.0".to_string(),
            info: serde_json::Value::Object(info),
            paths: StdHashMap::new(),
            components: Some(components),
            tags: vec![],
            servers: vec![],
            security: vec![],
        }
    }

    fn value_str(s: &str) -> serde_json::Value {
        serde_json::Value::String(s.to_string())
    }

    #[test]
    fn test_generate_success() {
        let config = ReverseGenConfig::new(Dialect::PostgreSql);
        let generator = OpenApiReverseGenerator::new(config);
        let spec = make_user_spec();

        let result = generator.generate(&spec).unwrap();
        assert_eq!(result.dialect, Dialect::PostgreSql);
        assert!(result.model_code.contains_key("User"));
        assert_eq!(result.migrations.len(), 1);
        assert!(result.repository_code.contains_key("User"));
        assert!(result.loop_report.consistent);
    }

    #[test]
    fn test_generate_unsigned_spec_error() {
        let config = ReverseGenConfig::new(Dialect::PostgreSql);
        let generator = OpenApiReverseGenerator::new(config);

        let mut spec = make_user_spec();
        if let serde_json::Value::Object(ref mut map) = spec.info {
            map.remove("x-sz-orm-signature");
        }

        let result = generator.generate(&spec);
        assert!(result.is_err());
        match result.unwrap_err() {
            ReverseGenError::UnsignedSpec => {}
            _ => panic!("expected UnsignedSpec"),
        }
    }

    #[test]
    fn test_generate_with_trust_unsigned() {
        let config = ReverseGenConfig::new(Dialect::MySql).with_trust_unsigned(true);
        let generator = OpenApiReverseGenerator::new(config);

        let mut spec = make_user_spec();
        if let serde_json::Value::Object(ref mut map) = spec.info {
            map.remove("x-sz-orm-signature");
        }

        let result = generator.generate(&spec).unwrap();
        assert_eq!(result.dialect, Dialect::MySql);
    }

    #[test]
    fn test_generate_no_schemas_error() {
        let config = ReverseGenConfig::new(Dialect::PostgreSql).with_trust_unsigned(true);
        let generator = OpenApiReverseGenerator::new(config);

        let spec = OpenAPISpec {
            openapi: "3.0.0".to_string(),
            info: serde_json::json!({"title": "Empty", "version": "1.0"}),
            paths: StdHashMap::new(),
            components: None,
            tags: vec![],
            servers: vec![],
            security: vec![],
        };

        let result = generator.generate(&spec);
        assert!(result.is_err());
    }

    #[test]
    fn test_generate_from_json() {
        let config = ReverseGenConfig::new(Dialect::Sqlite).with_trust_unsigned(true);
        let generator = OpenApiReverseGenerator::new(config);

        let spec = make_user_spec();
        let json = serde_json::to_string(&spec).unwrap();
        let result = generator.generate_from_json(&json).unwrap();
        assert_eq!(result.dialect, Dialect::Sqlite);
    }

    #[test]
    fn test_summarize() {
        let config = ReverseGenConfig::new(Dialect::PostgreSql).with_trust_unsigned(true);
        let generator = OpenApiReverseGenerator::new(config);
        let spec = make_user_spec();
        let result = generator.generate(&spec).unwrap();
        let summary = OpenApiReverseGenerator::summarize(&result);
        assert!(summary.contains("OpenAPI Reverse Generation Result"));
        assert!(summary.contains("PostgreSql"));
        assert!(summary.contains("Models generated: 1"));
    }

    #[test]
    fn test_generate_multiple_schemas() {
        let config = ReverseGenConfig::new(Dialect::MySql).with_trust_unsigned(true);
        let generator = OpenApiReverseGenerator::new(config);

        let mut components = Components::default();
        components
            .schemas
            .insert("User".to_string(), Schema::Object(ObjectType::new()));
        components
            .schemas
            .insert("Order".to_string(), Schema::Object(ObjectType::new()));
        components
            .schemas
            .insert("Product".to_string(), Schema::Object(ObjectType::new()));

        let spec = OpenAPISpec {
            openapi: "3.0.0".to_string(),
            info: serde_json::json!({"title": "Test", "version": "1.0"}),
            paths: StdHashMap::new(),
            components: Some(components),
            tags: vec![],
            servers: vec![],
            security: vec![],
        };

        let result = generator.generate(&spec).unwrap();
        assert_eq!(result.model_code.len(), 3);
        assert_eq!(result.migrations.len(), 3);
        assert_eq!(result.repository_code.len(), 3);
    }
}
