//! # 生产就绪配置 — 敏感字段脱敏验证
//!
//! 提供 `ProdReadyConfig` 聚合配置入口与 `verify_masking()` 统一脱敏验证方法，
//! 复用 `sz_orm_masking::DataMasker` 的脱敏能力。
//!
//! ## 主要类型
//!
//! - [`ProdReadyConfig`] — 生产就绪聚合配置
//! - [`SensitiveFieldRule`] — 敏感字段规则
//! - [`MaskingReport`] — 脱敏验证报告
//! - [`MaskingViolation`] — 脱敏违规

use serde::{Deserialize, Serialize};
use sz_orm_masking::{DataMasker, MaskingRule};

/// 环境类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnvKind {
    Development,
    Staging,
    Production,
}

/// 敏感字段规则：字段路径 + 脱敏规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensitiveFieldRule {
    /// 字段路径（如 `database.password`）
    pub path: String,
    /// 脱敏规则
    pub rule: MaskingRule,
}

/// 脱敏违规
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaskingViolation {
    /// 违规字段路径
    pub field_path: String,
    /// 违规原因
    pub reason: String,
    /// 当前值的脱敏形式（不含明文）
    pub current_value_masked: String,
}

/// 脱敏验证报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaskingReport {
    /// 违规列表
    pub violations: Vec<MaskingViolation>,
    /// 已脱敏字段数
    pub masked_count: u32,
}

impl MaskingReport {
    /// 报告是否通过（无违规）
    pub fn is_pass(&self) -> bool {
        self.violations.is_empty()
    }
}

/// 生产就绪配置错误
#[derive(Debug, Clone, thiserror::Error)]
pub enum ProdReadyError {
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("field not found: {0}")]
    FieldNotFound(String),
    #[error("config load failed: {0}")]
    LoadFailed(String),
}

/// 生产就绪聚合配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProdReadyConfig {
    /// 环境类型
    pub env: EnvKind,
    /// 敏感字段规则列表
    pub sensitive_fields: Vec<SensitiveFieldRule>,
    /// 配置键值对（TOML 加载后的扁平 map）
    #[serde(default)]
    pub config_values: std::collections::HashMap<String, String>,
}

impl ProdReadyConfig {
    /// 从 TOML 文件加载配置
    pub fn load(path: &str) -> Result<Self, ProdReadyError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ProdReadyError::LoadFailed(format!("{}: {}", path, e)))?;
        let config: ProdReadyConfig = toml::from_str(&content)
            .map_err(|e| ProdReadyError::InvalidConfig(format!("TOML parse error: {}", e)))?;
        Ok(config)
    }

    /// 从字符串解析配置
    pub fn from_str(content: &str) -> Result<Self, ProdReadyError> {
        let config: ProdReadyConfig = toml::from_str(content)
            .map_err(|e| ProdReadyError::InvalidConfig(format!("TOML parse error: {}", e)))?;
        Ok(config)
    }

    /// 校验配置合理性
    pub fn validate(&self) -> Result<(), ProdReadyError> {
        if self.sensitive_fields.is_empty() && self.env == EnvKind::Production {
            return Err(ProdReadyError::InvalidConfig(
                "production environment must define at least one sensitive field rule".to_string(),
            ));
        }
        for rule in &self.sensitive_fields {
            if rule.path.is_empty() {
                return Err(ProdReadyError::InvalidConfig(format!(
                    "sensitive field path is empty for rule: {:?}",
                    rule.rule
                )));
            }
        }
        Ok(())
    }

    /// 验证所有标记敏感字段已脱敏
    pub fn verify_masking(&self) -> MaskingReport {
        let mut violations = Vec::new();
        let mut masked_count = 0u32;

        for rule in &self.sensitive_fields {
            if let Some(value) = self.config_values.get(&rule.path) {
                let masked = DataMasker::apply(&rule.rule, value);
                if value != &masked {
                    violations.push(MaskingViolation {
                        field_path: rule.path.clone(),
                        reason: "plaintext_not_masked".to_string(),
                        current_value_masked: masked,
                    });
                } else {
                    masked_count += 1;
                }
            }
        }

        MaskingReport {
            violations,
            masked_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_config(
        env: EnvKind,
        values: HashMap<String, String>,
        rules: Vec<SensitiveFieldRule>,
    ) -> ProdReadyConfig {
        ProdReadyConfig {
            env,
            sensitive_fields: rules,
            config_values: values,
        }
    }

    #[test]
    fn test_verify_masking_password_masked() {
        let mut values = HashMap::new();
        values.insert("database.password".to_string(), "***".to_string());
        let rules = vec![SensitiveFieldRule {
            path: "database.password".to_string(),
            rule: MaskingRule::Password,
        }];
        let config = make_config(EnvKind::Production, values, rules);
        let report = config.verify_masking();
        assert!(report.is_pass());
        assert_eq!(report.masked_count, 1);
    }

    #[test]
    fn test_verify_masking_violation() {
        let mut values = HashMap::new();
        values.insert("api.key".to_string(), "sk-1234567890abcdef".to_string());
        let rules = vec![SensitiveFieldRule {
            path: "api.key".to_string(),
            rule: MaskingRule::ApiKey,
        }];
        let config = make_config(EnvKind::Production, values, rules);
        let report = config.verify_masking();
        assert!(!report.is_pass());
        assert_eq!(report.violations.len(), 1);
        assert_eq!(report.violations[0].reason, "plaintext_not_masked");
    }

    #[test]
    fn test_validate_production_requires_rules() {
        let config = make_config(EnvKind::Production, HashMap::new(), vec![]);
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_development_allows_empty_rules() {
        let config = make_config(EnvKind::Development, HashMap::new(), vec![]);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_empty_path_rejected() {
        let rules = vec![SensitiveFieldRule {
            path: "".to_string(),
            rule: MaskingRule::Password,
        }];
        let config = make_config(EnvKind::Development, HashMap::new(), rules);
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_api_key_masking() {
        let masked = DataMasker::apply(&MaskingRule::ApiKey, "sk-1234567890abcdef");
        assert_eq!(masked, "sk-1***********cdef");
    }

    #[test]
    fn test_password_masking() {
        let masked = DataMasker::apply(&MaskingRule::Password, "mysecret123");
        assert_eq!(masked, "***");
    }
}
