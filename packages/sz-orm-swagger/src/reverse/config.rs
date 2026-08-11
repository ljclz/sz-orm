//! ReverseGenConfig — 反向生成配置

use super::{to_pascal_case, to_snake_case, ReverseGenError};
use serde::{Deserialize, Serialize};
use sz_orm_core::dialect_security::Dialect;

/// 命名约定
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NamingConvention {
    /// snake_case
    #[default]
    SnakeCase,
    /// camelCase
    CamelCase,
    /// PascalCase
    PascalCase,
}

impl NamingConvention {
    /// 应用命名约定
    pub fn apply(&self, s: &str) -> String {
        match self {
            NamingConvention::SnakeCase => to_snake_case(s),
            NamingConvention::CamelCase => {
                let pascal = to_pascal_case(s);
                if let Some(first) = pascal.chars().next() {
                    first.to_ascii_lowercase().to_string() + &pascal[first.len_utf8()..]
                } else {
                    pascal
                }
            }
            NamingConvention::PascalCase => to_pascal_case(s),
        }
    }
}

/// 反向生成配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReverseGenConfig {
    /// 目标方言
    pub target_dialect: Dialect,
    /// 命名约定
    pub naming_convention: NamingConvention,
    /// 是否覆盖已有文件
    pub overwrite: bool,
    /// 可编辑区标记
    pub editable_region_marker: String,
    /// 是否信任未签名 spec
    pub trust_unsigned: bool,
    /// 配置版本号
    pub config_version: String,
}

impl Default for ReverseGenConfig {
    fn default() -> Self {
        Self {
            target_dialect: Dialect::PostgreSql,
            naming_convention: NamingConvention::SnakeCase,
            overwrite: false,
            editable_region_marker: "// EDITABLE: business logic here".to_string(),
            trust_unsigned: false,
            config_version: "1.0".to_string(),
        }
    }
}

impl ReverseGenConfig {
    /// 创建新的配置
    pub fn new(dialect: Dialect) -> Self {
        Self {
            target_dialect: dialect,
            ..Default::default()
        }
    }

    /// 设置命名约定
    pub fn with_naming_convention(mut self, convention: NamingConvention) -> Self {
        self.naming_convention = convention;
        self
    }

    /// 设置是否覆盖
    pub fn with_overwrite(mut self, overwrite: bool) -> Self {
        self.overwrite = overwrite;
        self
    }

    /// 设置是否信任未签名 spec
    pub fn with_trust_unsigned(mut self, trust: bool) -> Self {
        self.trust_unsigned = trust;
        self
    }

    /// 从 JSON 字符串解析配置
    pub fn from_json(json: &str) -> Result<Self, ReverseGenError> {
        serde_json::from_str(json).map_err(|e| ReverseGenError::SpecParseFailed {
            path: "config".to_string(),
            reason: e.to_string(),
        })
    }

    /// 从 JSON 文件读取配置
    pub fn from_file(path: &str) -> Result<Self, ReverseGenError> {
        let content =
            std::fs::read_to_string(path).map_err(|e| ReverseGenError::SpecParseFailed {
                path: path.to_string(),
                reason: e.to_string(),
            })?;

        if path.ends_with(".json") {
            Self::from_json(&content)
        } else {
            Err(ReverseGenError::SpecParseFailed {
                path: path.to_string(),
                reason: "only JSON config files are supported".to_string(),
            })
        }
    }

    /// 序列化为 JSON
    pub fn to_json(&self) -> Result<String, ReverseGenError> {
        serde_json::to_string_pretty(self).map_err(|e| ReverseGenError::SpecParseFailed {
            path: "config".to_string(),
            reason: e.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ReverseGenConfig::default();
        assert_eq!(config.target_dialect, Dialect::PostgreSql);
        assert_eq!(config.naming_convention, NamingConvention::SnakeCase);
        assert!(!config.overwrite);
        assert!(!config.trust_unsigned);
    }

    #[test]
    fn test_config_builder() {
        let config = ReverseGenConfig::new(Dialect::MySql)
            .with_naming_convention(NamingConvention::PascalCase)
            .with_overwrite(true)
            .with_trust_unsigned(true);

        assert_eq!(config.target_dialect, Dialect::MySql);
        assert_eq!(config.naming_convention, NamingConvention::PascalCase);
        assert!(config.overwrite);
        assert!(config.trust_unsigned);
    }

    #[test]
    fn test_config_from_json() {
        let json = r#"{
            "target_dialect": "MySql",
            "naming_convention": "snake_case",
            "overwrite": false,
            "editable_region_marker": "// EDITABLE",
            "trust_unsigned": true,
            "config_version": "1.0"
        }"#;
        let config = ReverseGenConfig::from_json(json).unwrap();
        assert_eq!(config.target_dialect, Dialect::MySql);
        assert!(config.trust_unsigned);
    }

    #[test]
    fn test_config_to_json() {
        let config = ReverseGenConfig::new(Dialect::MySql);
        let json = config.to_json().unwrap();
        assert!(json.contains("MySql"));
    }

    #[test]
    fn test_naming_convention_snake_case() {
        assert_eq!(NamingConvention::SnakeCase.apply("User"), "user");
        assert_eq!(
            NamingConvention::SnakeCase.apply("UserProfile"),
            "user_profile"
        );
    }

    #[test]
    fn test_naming_convention_pascal_case() {
        assert_eq!(NamingConvention::PascalCase.apply("user"), "User");
        assert_eq!(
            NamingConvention::PascalCase.apply("user_profile"),
            "UserProfile"
        );
    }

    #[test]
    fn test_naming_convention_camel_case() {
        assert_eq!(
            NamingConvention::CamelCase.apply("user_profile"),
            "userProfile"
        );
        assert_eq!(NamingConvention::CamelCase.apply("user"), "user");
    }

    #[test]
    fn test_config_roundtrip() {
        let config = ReverseGenConfig::new(Dialect::Sqlite)
            .with_naming_convention(NamingConvention::CamelCase)
            .with_overwrite(true);
        let json = config.to_json().unwrap();
        let parsed = ReverseGenConfig::from_json(&json).unwrap();
        assert_eq!(parsed.target_dialect, Dialect::Sqlite);
        assert_eq!(parsed.naming_convention, NamingConvention::CamelCase);
        assert!(parsed.overwrite);
    }
}
