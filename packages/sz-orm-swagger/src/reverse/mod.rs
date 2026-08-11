//! # OpenAPI → ORM 反向生成模块
//!
//! 提供 OpenAPI spec → Model + 迁移 + Repository 的反向生成能力，
//! 支持 API 优先开发闭环验证与注入防护。
//!
//! ## 主要类型
//!
//! - [`SchemaToModelMapper`] — OpenAPI Schema → Rust struct 字段映射
//! - [`OpenApiToMigrationMapper`] — OpenAPI Schema → 迁移文件（5 方言 DDL）
//! - [`OpenApiToRepositoryMapper`] — OpenAPI Schema → Repository CRUD 骨架
//! - [`ApiFirstLoopVerifier`] — API 优先闭环验证
//! - [`OpenApiInjectionGuard`] — 注入防护
//! - [`ReverseGenConfig`] — 反向生成配置
//! - [`OpenApiReverseGenerator`] — 反向生成器主入口

pub mod config;
pub mod generator;
pub mod injection_guard;
pub mod loop_verifier;
pub mod migration_mapper;
pub mod model_mapper;
pub mod repository_mapper;

pub use config::{NamingConvention, ReverseGenConfig};
pub use generator::{OpenApiReverseGenerator, ReverseGenResult};
pub use injection_guard::OpenApiInjectionGuard;
pub use loop_verifier::{ApiFirstLoopVerifier, LoopReport};
pub use migration_mapper::OpenApiToMigrationMapper;
pub use model_mapper::{Constraint, ModelField, RustType, SchemaToModelMapper};
pub use repository_mapper::OpenApiToRepositoryMapper;

use thiserror::Error;

/// 反向生成错误
#[derive(Debug, Clone, Error)]
pub enum ReverseGenError {
    /// OpenAPI spec 解析失败
    #[error("OpenAPI spec parse failed at {path}: {reason}")]
    SpecParseFailed { path: String, reason: String },

    /// 不支持的 Schema 特性（allOf/oneOf 等）
    #[error("unsupported schema construct {construct} at {schema}, skipped")]
    UnsupportedSchemaConstruct { construct: String, schema: String },

    /// 闭环验证差异
    #[error("loop verification diff: {diff}")]
    LoopVerificationDiff { diff: String },

    /// 注入防护触发
    #[error("injection detected in spec, refusing to execute embedded code")]
    InjectionDetected,

    /// 未签名 spec
    #[error("unsigned spec not trusted, provide signature or use --trust-unsigned")]
    UnsignedSpec,

    /// 覆盖用户手写逻辑
    #[error("user logic in editable region preserved, only skeleton updated")]
    UserLogicOverwrite,
}

/// 将字符串转换为 PascalCase
pub fn to_pascal_case(s: &str) -> String {
    let mut result = String::new();
    let mut next_upper = true;
    for ch in s.chars() {
        if ch == '_' || ch == '-' || ch == ' ' {
            next_upper = true;
        } else if next_upper {
            result.push(ch.to_ascii_uppercase());
            next_upper = false;
        } else {
            result.push(ch);
        }
    }
    result
}

/// 将字符串转换为 snake_case
pub fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    let mut prev_lower = false;
    for ch in s.chars() {
        if ch.is_ascii_uppercase() {
            if prev_lower {
                result.push('_');
            }
            result.push(ch.to_ascii_lowercase());
            prev_lower = false;
        } else if ch == '-' || ch == ' ' {
            result.push('_');
            prev_lower = false;
        } else {
            result.push(ch);
            prev_lower = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_pascal_case() {
        assert_eq!(to_pascal_case("user"), "User");
        assert_eq!(to_pascal_case("user_profile"), "UserProfile");
        assert_eq!(to_pascal_case("user-profile"), "UserProfile");
        assert_eq!(to_pascal_case("user profile"), "UserProfile");
    }

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("User"), "user");
        assert_eq!(to_snake_case("UserProfile"), "user_profile");
        assert_eq!(to_snake_case("user-profile"), "user_profile");
        assert_eq!(to_snake_case("userProfile"), "user_profile");
    }

    #[test]
    fn test_reverse_gen_error_display() {
        let err = ReverseGenError::SpecParseFailed {
            path: "components.schemas.User".to_string(),
            reason: "invalid type".to_string(),
        };
        assert!(err.to_string().contains("components.schemas.User"));
        assert!(err.to_string().contains("invalid type"));

        let err2 = ReverseGenError::InjectionDetected;
        assert!(err2.to_string().contains("injection detected"));
    }
}
