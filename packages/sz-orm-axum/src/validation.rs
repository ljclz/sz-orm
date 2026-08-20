//! 请求验证器：字段验证规则、验证结果。
//!
//! - [`RequestValidator`] — 请求验证器
//! - [`ValidationRule`] — 验证规则
//! - [`FieldValidator`] — 字段验证器（链式）
//! - [`ValidationResult`] — 验证结果

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ============================================================================
// ValidationError / ValidationResult
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

    /// 是否有效
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
// RuleType — 验证规则类型
// ============================================================================

/// 验证规则类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuleType {
    Required,
    MinLength(usize),
    MaxLength(usize),
    MinValue(i64),
    MaxValue(i64),
    Pattern(String),
    OneOf(Vec<String>),
    Email,
    Numeric,
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
                if value.chars().count() < *min {
                    Err(format!("length must be at least {}", min))
                } else {
                    Ok(())
                }
            }
            RuleType::MaxLength(max) => {
                if value.chars().count() > *max {
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
// ValidationRule — 验证规则
// ============================================================================

/// 单条验证规则
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
// FieldValidator — 字段验证器（链式）
// ============================================================================

/// 字段验证器
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
// RequestValidator — 请求验证器
// ============================================================================

/// 请求验证器
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

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_error_new() {
        let e = ValidationError::new("name", "is required", "REQUIRED");
        assert_eq!(e.field(), "name");
        assert_eq!(e.message(), "is required");
        assert_eq!(e.code(), "REQUIRED");
    }

    #[test]
    fn validation_result_success() {
        let r = ValidationResult::success();
        assert!(r.is_valid());
        assert_eq!(r.error_count(), 0);
    }

    #[test]
    fn validation_result_with_errors() {
        let r = ValidationResult::from_errors(vec![
            ValidationError::new("a", "err1", "E1"),
            ValidationError::new("b", "err2", "E2"),
        ]);
        assert!(!r.is_valid());
        assert_eq!(r.error_count(), 2);
    }

    #[test]
    fn validation_result_add_error() {
        let mut r = ValidationResult::success();
        r.add_error("name", "required", "REQ");
        assert!(!r.is_valid());
    }

    #[test]
    fn validation_result_merge() {
        let mut r = ValidationResult::success();
        r.add_error("a", "err", "E");
        let mut other = ValidationResult::success();
        other.add_error("b", "err", "E");
        r.merge(other);
        assert_eq!(r.error_count(), 2);
    }

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
    fn rule_email() {
        assert!(RuleType::Email.check("test@example.com").is_ok());
        assert!(RuleType::Email.check("invalid").is_err());
    }

    #[test]
    fn rule_numeric() {
        assert!(RuleType::Numeric.check("123").is_ok());
        assert!(RuleType::Numeric.check("3.15").is_ok());
        assert!(RuleType::Numeric.check("abc").is_err());
    }

    #[test]
    fn field_validator_chain() {
        let v = FieldValidator::new("email")
            .required()
            .email()
            .max_length(100);
        assert_eq!(v.rule_count(), 3);
    }

    #[test]
    fn request_validator_validate() {
        let v = RequestValidator::new()
            .add_field(FieldValidator::new("name").required())
            .add_field(FieldValidator::new("email").required().email());
        let mut data = HashMap::new();
        data.insert("name".to_string(), "Alice".to_string());
        data.insert("email".to_string(), "test@example.com".to_string());
        let result = v.validate(&data);
        assert!(result.is_valid());
    }

    #[test]
    fn request_validator_validate_fail() {
        let v = RequestValidator::new().add_field(FieldValidator::new("name").required());
        let data = HashMap::new();
        let result = v.validate(&data);
        assert!(!result.is_valid());
    }
}
