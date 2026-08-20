//! 请求验证器：字段验证规则、验证结果。
//!
//! - [`RequestValidator`] — 请求验证器（多字段、多规则）
//! - [`ValidationRule`] — 单条验证规则
//! - [`ValidationResult`] — 验证结果（错误列表）
//! - [`FieldValidator`] — 字段验证器（链式规则）

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ============================================================================
// 验证结果
// ============================================================================

/// 验证错误
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationError {
    field: String,
    message: String,
    code: String,
}

impl ValidationError {
    /// 创建验证错误
    pub fn new(field: &str, message: &str, code: &str) -> Self {
        Self {
            field: field.to_string(),
            message: message.to_string(),
            code: code.to_string(),
        }
    }

    /// 字段名
    pub fn field(&self) -> &str {
        &self.field
    }

    /// 错误消息
    pub fn message(&self) -> &str {
        &self.message
    }

    /// 错误码
    pub fn code(&self) -> &str {
        &self.code
    }
}

/// 验证结果
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ValidationResult {
    errors: Vec<ValidationError>,
}

impl ValidationResult {
    /// 创建成功结果
    pub fn success() -> Self {
        Self::default()
    }

    /// 从错误列表创建
    pub fn from_errors(errors: Vec<ValidationError>) -> Self {
        Self { errors }
    }

    /// 添加错误
    pub fn add_error(&mut self, field: &str, message: &str, code: &str) {
        self.errors.push(ValidationError::new(field, message, code));
    }

    /// 是否有效（无错误）
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    /// 错误数
    pub fn error_count(&self) -> usize {
        self.errors.len()
    }

    /// 所有错误
    pub fn errors(&self) -> &[ValidationError] {
        &self.errors
    }

    /// 按字段名过滤错误
    pub fn errors_for_field(&self, field: &str) -> Vec<&ValidationError> {
        self.errors.iter().filter(|e| e.field() == field).collect()
    }

    /// 合并另一个验证结果
    pub fn merge(&mut self, other: ValidationResult) {
        self.errors.extend(other.errors);
    }

    /// 转换为 JSON 字符串
    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.errors).unwrap_or_else(|_| "[]".to_string())
    }
}

// ============================================================================
// 验证规则类型
// ============================================================================

/// 验证规则类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuleType {
    /// 必填
    Required,
    /// 最小长度
    MinLength(usize),
    /// 最大长度
    MaxLength(usize),
    /// 最小值
    MinValue(i64),
    /// 最大值
    MaxValue(i64),
    /// 正则匹配（简化为包含检查）
    Pattern(String),
    /// 枚举值
    OneOf(Vec<String>),
    /// 邮箱格式
    Email,
    /// 数字格式
    Numeric,
    /// 不为空
    NotBlank,
}

impl RuleType {
    /// 验证值是否满足规则
    pub fn check(&self, value: &str) -> Result<(), String> {
        match self {
            RuleType::Required => {
                if value.is_empty() {
                    Err("field is required".to_string())
                } else {
                    Ok(())
                }
            }
            RuleType::MinLength(min) => {
                let len = value.chars().count();
                if len < *min {
                    Err(format!("length must be at least {}", min))
                } else {
                    Ok(())
                }
            }
            RuleType::MaxLength(max) => {
                let len = value.chars().count();
                if len > *max {
                    Err(format!("length must be at most {}", max))
                } else {
                    Ok(())
                }
            }
            RuleType::MinValue(min) => {
                let val: i64 = value
                    .parse()
                    .map_err(|_| "value must be a number".to_string())?;
                if val < *min {
                    Err(format!("value must be at least {}", min))
                } else {
                    Ok(())
                }
            }
            RuleType::MaxValue(max) => {
                let val: i64 = value
                    .parse()
                    .map_err(|_| "value must be a number".to_string())?;
                if val > *max {
                    Err(format!("value must be at most {}", max))
                } else {
                    Ok(())
                }
            }
            RuleType::Pattern(pattern) => {
                if value.contains(pattern.as_str()) {
                    Ok(())
                } else {
                    Err(format!("value must contain '{}'", pattern))
                }
            }
            RuleType::OneOf(allowed) => {
                if allowed.iter().any(|a| a == value) {
                    Ok(())
                } else {
                    Err(format!("value must be one of: {}", allowed.join(", ")))
                }
            }
            RuleType::Email => {
                if value.contains('@') && value.contains('.') && value.len() > 3 {
                    Ok(())
                } else {
                    Err("invalid email format".to_string())
                }
            }
            RuleType::Numeric => {
                if value.parse::<f64>().is_ok() {
                    Ok(())
                } else {
                    Err("value must be numeric".to_string())
                }
            }
            RuleType::NotBlank => {
                if value.trim().is_empty() {
                    Err("field must not be blank".to_string())
                } else {
                    Ok(())
                }
            }
        }
    }

    /// 规则名
    pub fn name(&self) -> String {
        match self {
            RuleType::Required => "required".to_string(),
            RuleType::MinLength(n) => format!("min_length({})", n),
            RuleType::MaxLength(n) => format!("max_length({})", n),
            RuleType::MinValue(n) => format!("min_value({})", n),
            RuleType::MaxValue(n) => format!("max_value({})", n),
            RuleType::Pattern(p) => format!("pattern({})", p),
            RuleType::OneOf(_) => "one_of".to_string(),
            RuleType::Email => "email".to_string(),
            RuleType::Numeric => "numeric".to_string(),
            RuleType::NotBlank => "not_blank".to_string(),
        }
    }
}

// ============================================================================
// 验证规则
// ============================================================================

/// 单条验证规则：字段名 + 规则类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationRule {
    field: String,
    rule: RuleType,
}

impl ValidationRule {
    /// 创建验证规则
    pub fn new(field: &str, rule: RuleType) -> Self {
        Self {
            field: field.to_string(),
            rule,
        }
    }

    /// 字段名
    pub fn field(&self) -> &str {
        &self.field
    }

    /// 规则类型
    pub fn rule(&self) -> &RuleType {
        &self.rule
    }

    /// 验证值
    pub fn validate(&self, value: &str) -> Result<(), String> {
        self.rule.check(value)
    }
}

// ============================================================================
// 字段验证器（链式）
// ============================================================================

/// 字段验证器：为单个字段链式添加多条规则
#[derive(Debug, Clone)]
pub struct FieldValidator {
    field: String,
    rules: Vec<RuleType>,
}

impl FieldValidator {
    /// 创建字段验证器
    pub fn new(field: &str) -> Self {
        Self {
            field: field.to_string(),
            rules: vec![],
        }
    }

    /// 添加必填规则（链式）
    pub fn required(mut self) -> Self {
        self.rules.push(RuleType::Required);
        self
    }

    /// 添加最小长度规则（链式）
    pub fn min_length(mut self, n: usize) -> Self {
        self.rules.push(RuleType::MinLength(n));
        self
    }

    /// 添加最大长度规则（链式）
    pub fn max_length(mut self, n: usize) -> Self {
        self.rules.push(RuleType::MaxLength(n));
        self
    }

    /// 添加邮箱规则（链式）
    pub fn email(mut self) -> Self {
        self.rules.push(RuleType::Email);
        self
    }

    /// 添加数值规则（链式）
    pub fn numeric(mut self) -> Self {
        self.rules.push(RuleType::Numeric);
        self
    }

    /// 添加枚举规则（链式）
    pub fn one_of(mut self, values: Vec<String>) -> Self {
        self.rules.push(RuleType::OneOf(values));
        self
    }

    /// 添加不为空规则（链式）
    pub fn not_blank(mut self) -> Self {
        self.rules.push(RuleType::NotBlank);
        self
    }

    /// 添加最小值规则（链式）
    pub fn min_value(mut self, n: i64) -> Self {
        self.rules.push(RuleType::MinValue(n));
        self
    }

    /// 添加最大值规则（链式）
    pub fn max_value(mut self, n: i64) -> Self {
        self.rules.push(RuleType::MaxValue(n));
        self
    }

    /// 字段名
    pub fn field(&self) -> &str {
        &self.field
    }

    /// 规则数
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// 验证值，返回所有错误
    pub fn validate(&self, value: &str) -> Vec<String> {
        self.rules
            .iter()
            .filter_map(|rule| rule.check(value).err())
            .collect()
    }

    /// 转换为验证规则列表
    pub fn to_rules(&self) -> Vec<ValidationRule> {
        self.rules
            .iter()
            .map(|r| ValidationRule::new(&self.field, r.clone()))
            .collect()
    }
}

// ============================================================================
// 请求验证器
// ============================================================================

/// 请求验证器：管理多字段验证规则，批量验证。
#[derive(Debug, Clone, Default)]
pub struct RequestValidator {
    fields: Vec<FieldValidator>,
}

impl RequestValidator {
    /// 创建空验证器
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加字段验证器（链式）
    pub fn add_field(mut self, validator: FieldValidator) -> Self {
        self.fields.push(validator);
        self
    }

    /// 字段数
    pub fn field_count(&self) -> usize {
        self.fields.len()
    }

    /// 总规则数
    pub fn total_rule_count(&self) -> usize {
        self.fields.iter().map(|f| f.rule_count()).sum()
    }

    /// 验证 HashMap 数据
    pub fn validate(&self, data: &HashMap<String, String>) -> ValidationResult {
        let mut result = ValidationResult::success();
        for field in &self.fields {
            let value = data.get(field.field()).map(|s| s.as_str()).unwrap_or("");
            for error in field.validate(value) {
                result.add_error(field.field(), &error, field.field());
            }
        }
        result
    }

    /// 验证单个字段
    pub fn validate_field(&self, field: &str, value: &str) -> Vec<String> {
        self.fields
            .iter()
            .find(|f| f.field() == field)
            .map(|f| f.validate(value))
            .unwrap_or_default()
    }

    /// 清空所有字段
    pub fn clear(&mut self) {
        self.fields.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ----- ValidationError -----

    #[test]
    fn validation_error_new() {
        let e = ValidationError::new("name", "is required", "REQUIRED");
        assert_eq!(e.field(), "name");
        assert_eq!(e.message(), "is required");
        assert_eq!(e.code(), "REQUIRED");
    }

    // ----- ValidationResult -----

    #[test]
    fn validation_result_success() {
        let result = ValidationResult::success();
        assert!(result.is_valid());
        assert_eq!(result.error_count(), 0);
    }

    #[test]
    fn validation_result_with_errors() {
        let result = ValidationResult::from_errors(vec![
            ValidationError::new("a", "err1", "E1"),
            ValidationError::new("b", "err2", "E2"),
        ]);
        assert!(!result.is_valid());
        assert_eq!(result.error_count(), 2);
    }

    #[test]
    fn validation_result_add_error() {
        let mut result = ValidationResult::success();
        result.add_error("name", "required", "REQ");
        assert!(!result.is_valid());
        assert_eq!(result.error_count(), 1);
    }

    #[test]
    fn validation_result_errors_for_field() {
        let mut result = ValidationResult::success();
        result.add_error("name", "too short", "MIN_LEN");
        result.add_error("email", "invalid", "EMAIL");
        result.add_error("name", "not blank", "BLANK");
        let name_errors = result.errors_for_field("name");
        assert_eq!(name_errors.len(), 2);
    }

    #[test]
    fn validation_result_merge() {
        let mut result = ValidationResult::success();
        result.add_error("a", "err", "E");
        let mut other = ValidationResult::success();
        other.add_error("b", "err", "E");
        result.merge(other);
        assert_eq!(result.error_count(), 2);
    }

    #[test]
    fn validation_result_to_json() {
        let mut result = ValidationResult::success();
        result.add_error("name", "required", "REQ");
        let json = result.to_json();
        assert!(json.contains("name"));
    }

    // ----- RuleType -----

    #[test]
    fn rule_required() {
        assert!(RuleType::Required.check("value").is_ok());
        assert!(RuleType::Required.check("").is_err());
    }

    #[test]
    fn rule_min_length() {
        assert!(RuleType::MinLength(3).check("abc").is_ok());
        assert!(RuleType::MinLength(3).check("ab").is_err());
    }

    #[test]
    fn rule_max_length() {
        assert!(RuleType::MaxLength(3).check("abc").is_ok());
        assert!(RuleType::MaxLength(3).check("abcd").is_err());
    }

    #[test]
    fn rule_min_value() {
        assert!(RuleType::MinValue(10).check("20").is_ok());
        assert!(RuleType::MinValue(10).check("5").is_err());
        assert!(RuleType::MinValue(10).check("abc").is_err());
    }

    #[test]
    fn rule_max_value() {
        assert!(RuleType::MaxValue(100).check("50").is_ok());
        assert!(RuleType::MaxValue(100).check("200").is_err());
    }

    #[test]
    fn rule_pattern() {
        assert!(RuleType::Pattern("@".to_string()).check("a@b").is_ok());
        assert!(RuleType::Pattern("@".to_string()).check("ab").is_err());
    }

    #[test]
    fn rule_one_of() {
        let rule = RuleType::OneOf(vec!["a".to_string(), "b".to_string()]);
        assert!(rule.check("a").is_ok());
        assert!(rule.check("b").is_ok());
        assert!(rule.check("c").is_err());
    }

    #[test]
    fn rule_email() {
        assert!(RuleType::Email.check("test@example.com").is_ok());
        assert!(RuleType::Email.check("invalid").is_err());
        assert!(RuleType::Email.check("a@").is_err());
    }

    #[test]
    fn rule_numeric() {
        assert!(RuleType::Numeric.check("123").is_ok());
        assert!(RuleType::Numeric.check("3.15").is_ok());
        assert!(RuleType::Numeric.check("abc").is_err());
    }

    #[test]
    fn rule_not_blank() {
        assert!(RuleType::NotBlank.check("value").is_ok());
        assert!(RuleType::NotBlank.check("  ").is_err());
        assert!(RuleType::NotBlank.check("").is_err());
    }

    #[test]
    fn rule_name() {
        assert_eq!(RuleType::Required.name(), "required");
        assert_eq!(RuleType::MinLength(3).name(), "min_length(3)");
        assert_eq!(RuleType::Email.name(), "email");
    }

    // ----- ValidationRule -----

    #[test]
    fn validation_rule_validate() {
        let rule = ValidationRule::new("name", RuleType::Required);
        assert!(rule.validate("value").is_ok());
        assert!(rule.validate("").is_err());
    }

    // ----- FieldValidator -----

    #[test]
    fn field_validator_chain() {
        let validator = FieldValidator::new("email")
            .required()
            .email()
            .max_length(100);
        assert_eq!(validator.rule_count(), 3);
    }

    #[test]
    fn field_validator_validate_pass() {
        let validator = FieldValidator::new("email").required().email();
        let errors = validator.validate("test@example.com");
        assert!(errors.is_empty());
    }

    #[test]
    fn field_validator_validate_fail() {
        let validator = FieldValidator::new("email").required().email();
        let errors = validator.validate("invalid");
        assert!(!errors.is_empty());
    }

    #[test]
    fn field_validator_to_rules() {
        let validator = FieldValidator::new("name").required().min_length(3);
        let rules = validator.to_rules();
        assert_eq!(rules.len(), 2);
    }

    // ----- RequestValidator -----

    #[test]
    fn request_validator_empty() {
        let validator = RequestValidator::new();
        assert_eq!(validator.field_count(), 0);
    }

    #[test]
    fn request_validator_add_field() {
        let validator = RequestValidator::new()
            .add_field(FieldValidator::new("name").required())
            .add_field(FieldValidator::new("email").required().email());
        assert_eq!(validator.field_count(), 2);
        assert_eq!(validator.total_rule_count(), 3);
    }

    #[test]
    fn request_validator_validate_success() {
        let validator = RequestValidator::new()
            .add_field(FieldValidator::new("name").required())
            .add_field(FieldValidator::new("email").required().email());
        let mut data = HashMap::new();
        data.insert("name".to_string(), "Alice".to_string());
        data.insert("email".to_string(), "test@example.com".to_string());
        let result = validator.validate(&data);
        assert!(result.is_valid());
    }

    #[test]
    fn request_validator_validate_failure() {
        let validator = RequestValidator::new()
            .add_field(FieldValidator::new("name").required())
            .add_field(FieldValidator::new("email").required().email());
        let mut data = HashMap::new();
        data.insert("name".to_string(), "".to_string());
        data.insert("email".to_string(), "invalid".to_string());
        let result = validator.validate(&data);
        assert!(!result.is_valid());
        assert!(result.error_count() >= 2);
    }

    #[test]
    fn request_validator_validate_missing_field() {
        let validator = RequestValidator::new().add_field(FieldValidator::new("name").required());
        let data = HashMap::new();
        let result = validator.validate(&data);
        assert!(!result.is_valid());
    }

    #[test]
    fn request_validator_validate_field() {
        let validator =
            RequestValidator::new().add_field(FieldValidator::new("email").required().email());
        let errors = validator.validate_field("email", "invalid");
        assert!(!errors.is_empty());
    }

    #[test]
    fn request_validator_validate_field_nonexistent() {
        let validator = RequestValidator::new();
        let errors = validator.validate_field("nonexistent", "value");
        assert!(errors.is_empty());
    }

    #[test]
    fn request_validator_clear() {
        let mut validator =
            RequestValidator::new().add_field(FieldValidator::new("name").required());
        validator.clear();
        assert_eq!(validator.field_count(), 0);
    }
}
