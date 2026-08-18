//! 配置校验器：对配置值进行类型/范围/格式校验
//!
//! 提供声明式配置校验：定义 schema → 校验配置 → 输出校验结果。
//! 支持类型校验（int/float/bool/string）、范围校验（min/max）、
//! 前缀/后缀/包含校验、必填校验、枚举值校验。

use std::collections::HashMap;

/// 校验规则
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationRule {
    /// 必填（值不能为空）
    Required,
    /// 整数类型
    Integer,
    /// 浮点类型
    Float,
    /// 布尔类型
    Boolean,
    /// 字符串类型
    String,
    /// 整数范围 [min, max]
    IntRange(i64, i64),
    /// 浮点范围 [min, max]
    FloatRange(f64, f64),
    /// 字符串长度范围 [min, max]
    LengthRange(usize, usize),
    /// 枚举值（必须是列表中的一个）
    Enum(Vec<String>),
    /// 前缀匹配
    Prefix(String),
    /// 后缀匹配
    Suffix(String),
    /// 包含子串
    Contains(String),
}

/// 字段校验定义
#[derive(Debug, Clone)]
pub struct FieldSchema {
    /// 字段名
    pub name: String,
    /// 校验规则列表（全部必须通过）
    pub rules: Vec<ValidationRule>,
    /// 默认值（可选）
    pub default: Option<String>,
    /// 描述
    pub description: String,
}

impl FieldSchema {
    /// 创建字段 schema
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            rules: Vec::new(),
            default: None,
            description: String::new(),
        }
    }

    /// 添加规则（链式）
    pub fn with_rule(mut self, rule: ValidationRule) -> Self {
        self.rules.push(rule);
        self
    }

    /// 设置默认值（链式）
    pub fn with_default(mut self, value: &str) -> Self {
        self.default = Some(value.to_string());
        self
    }

    /// 设置描述（链式）
    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = desc.to_string();
        self
    }

    /// 校验单个值
    pub fn validate(&self, value: Option<&str>) -> ValidationResult {
        let value = match value {
            Some(v) => v,
            None => match &self.default {
                Some(d) => d.as_str(),
                None => {
                    if self.rules.contains(&ValidationRule::Required) {
                        return ValidationResult::failed(&self.name, "required field is missing");
                    }
                    return ValidationResult::passed(&self.name);
                }
            },
        };

        if value.is_empty() && self.rules.contains(&ValidationRule::Required) {
            return ValidationResult::failed(&self.name, "required field is empty");
        }

        for rule in &self.rules {
            if let Some(msg) = check_rule(rule, value) {
                return ValidationResult::failed(&self.name, &msg);
            }
        }

        ValidationResult::passed(&self.name)
    }
}

/// 配置 schema：多字段校验定义
#[derive(Debug, Clone, Default)]
pub struct ConfigSchema {
    fields: Vec<FieldSchema>,
}

impl ConfigSchema {
    /// 创建空 schema
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加字段定义（链式）
    pub fn add_field(mut self, field: FieldSchema) -> Self {
        self.fields.push(field);
        self
    }

    /// 字段数
    pub fn field_count(&self) -> usize {
        self.fields.len()
    }

    /// 校验配置 HashMap
    pub fn validate(&self, config: &HashMap<String, String>) -> ConfigValidationReport {
        let mut results = Vec::with_capacity(self.fields.len());
        for field in &self.fields {
            let value = config.get(&field.name).map(|s| s.as_str());
            results.push(field.validate(value));
        }
        ConfigValidationReport { results }
    }

    /// 校验并填充默认值（返回新 config）
    pub fn validate_and_fill_defaults(
        &self,
        config: &HashMap<String, String>,
    ) -> (HashMap<String, String>, ConfigValidationReport) {
        let mut filled = config.clone();
        for field in &self.fields {
            if !filled.contains_key(&field.name) {
                if let Some(default) = &field.default {
                    filled.insert(field.name.clone(), default.clone());
                }
            }
        }
        let report = self.validate(&filled);
        (filled, report)
    }

    /// 从环境变量读取配置并校验
    ///
    /// 对每个字段，先查 env var，再查默认值，最后校验。
    pub fn validate_env(&self) -> ConfigValidationReport {
        let mut env_config: HashMap<String, String> = HashMap::new();
        for field in &self.fields {
            if let Ok(val) = std::env::var(&field.name) {
                env_config.insert(field.name.clone(), val);
            }
        }
        let (filled, _) = self.validate_and_fill_defaults(&env_config);
        self.validate(&filled)
    }

    /// 返回所有字段名
    pub fn field_names(&self) -> Vec<&str> {
        self.fields.iter().map(|f| f.name.as_str()).collect()
    }
}

/// 单字段校验结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationResult {
    /// 字段名
    pub field: String,
    /// 是否通过
    pub passed: bool,
    /// 失败消息（通过时为空）
    pub message: String,
}

impl ValidationResult {
    fn passed(field: &str) -> Self {
        Self {
            field: field.to_string(),
            passed: true,
            message: String::new(),
        }
    }

    fn failed(field: &str, message: &str) -> Self {
        Self {
            field: field.to_string(),
            passed: false,
            message: message.to_string(),
        }
    }
}

/// 配置校验报告
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigValidationReport {
    results: Vec<ValidationResult>,
}

impl ConfigValidationReport {
    /// 是否全部通过
    pub fn is_valid(&self) -> bool {
        self.results.iter().all(|r| r.passed)
    }

    /// 失败数量
    pub fn failure_count(&self) -> usize {
        self.results.iter().filter(|r| !r.passed).count()
    }

    /// 通过数量
    pub fn pass_count(&self) -> usize {
        self.results.iter().filter(|r| r.passed).count()
    }

    /// 总字段数
    pub fn total(&self) -> usize {
        self.results.len()
    }

    /// 失败字段列表
    pub fn failures(&self) -> Vec<&ValidationResult> {
        self.results.iter().filter(|r| !r.passed).collect()
    }

    /// 通过字段列表
    pub fn passed_fields(&self) -> Vec<&ValidationResult> {
        self.results.iter().filter(|r| r.passed).collect()
    }

    /// 汇总字符串
    pub fn to_summary(&self) -> String {
        if self.is_valid() {
            format!("Config validation: all {} field(s) passed", self.total())
        } else {
            let mut out = format!(
                "Config validation: {}/{} field(s) failed\n",
                self.failure_count(),
                self.total()
            );
            for f in self.failures() {
                out.push_str(&format!("  X {}: {}\n", f.field, f.message));
            }
            out
        }
    }

    /// 合并另一个报告，返回新报告
    pub fn merge(&self, other: &ConfigValidationReport) -> ConfigValidationReport {
        let mut combined = self.results.clone();
        combined.extend(other.results.iter().cloned());
        ConfigValidationReport { results: combined }
    }

    /// 按字段名过滤结果
    pub fn filter_by_field(&self, field: &str) -> Vec<&ValidationResult> {
        self.results.iter().filter(|r| r.field == field).collect()
    }

    /// 返回所有校验结果的引用
    pub fn all_results(&self) -> &[ValidationResult] {
        &self.results
    }
}

/// 校验单个规则（返回 None 表示通过，Some(msg) 表示失败）
fn check_rule(rule: &ValidationRule, value: &str) -> Option<String> {
    match rule {
        ValidationRule::Required => {
            if value.is_empty() {
                Some("required field is empty".to_string())
            } else {
                None
            }
        }
        ValidationRule::Integer => {
            if value.parse::<i64>().is_err() {
                Some(format!("expected integer, got '{value}'"))
            } else {
                None
            }
        }
        ValidationRule::Float => {
            if value.parse::<f64>().is_err() {
                Some(format!("expected float, got '{value}'"))
            } else {
                None
            }
        }
        ValidationRule::Boolean => {
            if value.parse::<bool>().is_err() {
                Some(format!("expected boolean (true/false), got '{value}'"))
            } else {
                None
            }
        }
        ValidationRule::String => None,
        ValidationRule::IntRange(min, max) => match value.parse::<i64>() {
            Ok(n) if (*min..=*max).contains(&n) => None,
            Ok(n) => Some(format!("integer {n} out of range [{min}, {max}]")),
            Err(_) => Some(format!("expected integer for range check, got '{value}'")),
        },
        ValidationRule::FloatRange(min, max) => match value.parse::<f64>() {
            Ok(n) if (*min..=*max).contains(&n) => None,
            Ok(n) => Some(format!("float {n} out of range [{min}, {max}]")),
            Err(_) => Some(format!("expected float for range check, got '{value}'")),
        },
        ValidationRule::LengthRange(min, max) => {
            let len = value.chars().count();
            if (*min..=*max).contains(&len) {
                None
            } else {
                Some(format!("length {len} out of range [{min}, {max}]"))
            }
        }
        ValidationRule::Enum(allowed) => {
            if allowed.iter().any(|a| a == value) {
                None
            } else {
                Some(format!(
                    "value '{value}' not in allowed list: {:?}",
                    allowed
                ))
            }
        }
        ValidationRule::Prefix(prefix) => {
            if value.starts_with(prefix) {
                None
            } else {
                Some(format!("value '{value}' does not start with '{prefix}'"))
            }
        }
        ValidationRule::Suffix(suffix) => {
            if value.ends_with(suffix) {
                None
            } else {
                Some(format!("value '{value}' does not end with '{suffix}'"))
            }
        }
        ValidationRule::Contains(substring) => {
            if value.contains(substring) {
                None
            } else {
                Some(format!("value '{value}' does not contain '{substring}'"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(items: &[(&str, &str)]) -> HashMap<String, String> {
        items
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn validate_required_present() {
        let schema = FieldSchema::new("port").with_rule(ValidationRule::Required);
        let result = schema.validate(Some("8080"));
        assert!(result.passed);
    }

    #[test]
    fn validate_required_missing() {
        let schema = FieldSchema::new("port").with_rule(ValidationRule::Required);
        let result = schema.validate(None);
        assert!(!result.passed);
        assert!(result.message.contains("missing"));
    }

    #[test]
    fn validate_required_empty() {
        let schema = FieldSchema::new("port").with_rule(ValidationRule::Required);
        let result = schema.validate(Some(""));
        assert!(!result.passed);
    }

    #[test]
    fn validate_integer_pass() {
        let schema = FieldSchema::new("port").with_rule(ValidationRule::Integer);
        assert!(schema.validate(Some("8080")).passed);
    }

    #[test]
    fn validate_integer_fail() {
        let schema = FieldSchema::new("port").with_rule(ValidationRule::Integer);
        assert!(!schema.validate(Some("abc")).passed);
    }

    #[test]
    fn validate_float_pass() {
        let schema = FieldSchema::new("ratio").with_rule(ValidationRule::Float);
        assert!(schema.validate(Some("0.95")).passed);
    }

    #[test]
    fn validate_boolean_pass() {
        let schema = FieldSchema::new("enabled").with_rule(ValidationRule::Boolean);
        assert!(schema.validate(Some("true")).passed);
        assert!(schema.validate(Some("false")).passed);
    }

    #[test]
    fn validate_boolean_fail() {
        let schema = FieldSchema::new("enabled").with_rule(ValidationRule::Boolean);
        assert!(!schema.validate(Some("yes")).passed);
    }

    #[test]
    fn validate_int_range_pass() {
        let schema = FieldSchema::new("port").with_rule(ValidationRule::IntRange(1, 65535));
        assert!(schema.validate(Some("8080")).passed);
    }

    #[test]
    fn validate_int_range_fail() {
        let schema = FieldSchema::new("port").with_rule(ValidationRule::IntRange(1, 65535));
        assert!(!schema.validate(Some("99999")).passed);
    }

    #[test]
    fn validate_length_range() {
        let schema = FieldSchema::new("name").with_rule(ValidationRule::LengthRange(3, 10));
        assert!(schema.validate(Some("hello")).passed);
        assert!(!schema.validate(Some("hi")).passed);
        assert!(!schema.validate(Some("this_is_too_long")).passed);
    }

    #[test]
    fn validate_enum_pass() {
        let schema = FieldSchema::new("level").with_rule(ValidationRule::Enum(vec![
            "debug".to_string(),
            "info".to_string(),
            "warn".to_string(),
        ]));
        assert!(schema.validate(Some("info")).passed);
    }

    #[test]
    fn validate_enum_fail() {
        let schema = FieldSchema::new("level").with_rule(ValidationRule::Enum(vec![
            "debug".to_string(),
            "info".to_string(),
        ]));
        assert!(!schema.validate(Some("trace")).passed);
    }

    #[test]
    fn validate_prefix() {
        let schema = FieldSchema::new("url").with_rule(ValidationRule::Prefix("http".to_string()));
        assert!(schema.validate(Some("http://example.com")).passed);
        assert!(!schema.validate(Some("ftp://x")).passed);
    }

    #[test]
    fn validate_suffix() {
        let schema = FieldSchema::new("file").with_rule(ValidationRule::Suffix(".rs".to_string()));
        assert!(schema.validate(Some("main.rs")).passed);
        assert!(!schema.validate(Some("main.go")).passed);
    }

    #[test]
    fn validate_contains() {
        let schema =
            FieldSchema::new("conn").with_rule(ValidationRule::Contains("://".to_string()));
        assert!(schema.validate(Some("mysql://localhost")).passed);
        assert!(!schema.validate(Some("localhost")).passed);
    }

    #[test]
    fn validate_with_default() {
        let schema = FieldSchema::new("port")
            .with_rule(ValidationRule::Integer)
            .with_default("8080");
        let result = schema.validate(None);
        assert!(result.passed);
    }

    #[test]
    fn schema_validate_all_pass() {
        let schema = ConfigSchema::new()
            .add_field(FieldSchema::new("host").with_rule(ValidationRule::Required))
            .add_field(FieldSchema::new("port").with_rule(ValidationRule::IntRange(1, 65535)));
        let report = schema.validate(&config(&[("host", "localhost"), ("port", "8080")]));
        assert!(report.is_valid());
        assert_eq!(report.pass_count(), 2);
        assert_eq!(report.failure_count(), 0);
    }

    #[test]
    fn schema_validate_with_failures() {
        let schema = ConfigSchema::new()
            .add_field(FieldSchema::new("host").with_rule(ValidationRule::Required))
            .add_field(FieldSchema::new("port").with_rule(ValidationRule::IntRange(1, 65535)));
        let report = schema.validate(&config(&[("port", "99999")]));
        assert!(!report.is_valid());
        assert_eq!(report.failure_count(), 2);
    }

    #[test]
    fn schema_fill_defaults() {
        let schema = ConfigSchema::new().add_field(FieldSchema::new("port").with_default("8080"));
        let (filled, report) = schema.validate_and_fill_defaults(&HashMap::new());
        assert_eq!(filled.get("port").map(|s| s.as_str()), Some("8080"));
        assert!(report.is_valid());
    }

    #[test]
    fn report_to_summary_valid() {
        let schema = ConfigSchema::new()
            .add_field(FieldSchema::new("x").with_rule(ValidationRule::Required));
        let report = schema.validate(&config(&[("x", "1")]));
        assert!(report.to_summary().contains("passed"));
    }

    #[test]
    fn report_to_summary_invalid() {
        let schema = ConfigSchema::new()
            .add_field(FieldSchema::new("x").with_rule(ValidationRule::Required));
        let report = schema.validate(&HashMap::new());
        let summary = report.to_summary();
        assert!(summary.contains("failed"));
    }

    #[test]
    fn report_total_and_counts() {
        let schema = ConfigSchema::new()
            .add_field(FieldSchema::new("a").with_rule(ValidationRule::Required))
            .add_field(FieldSchema::new("b"));
        let report = schema.validate(&config(&[("a", "1")]));
        assert_eq!(report.total(), 2);
        assert_eq!(report.pass_count(), 2);
        assert_eq!(report.failure_count(), 0);
    }

    #[test]
    fn field_schema_builder_chain() {
        let schema = FieldSchema::new("url")
            .with_rule(ValidationRule::Required)
            .with_rule(ValidationRule::Prefix("http".to_string()))
            .with_default("http://localhost")
            .with_description("Service URL");
        assert_eq!(schema.name, "url");
        assert_eq!(schema.rules.len(), 2);
        assert_eq!(schema.default, Some("http://localhost".to_string()));
        assert_eq!(schema.description, "Service URL");
    }

    #[test]
    fn config_schema_field_count() {
        let schema = ConfigSchema::new()
            .add_field(FieldSchema::new("a"))
            .add_field(FieldSchema::new("b"))
            .add_field(FieldSchema::new("c"));
        assert_eq!(schema.field_count(), 3);
    }

    #[test]
    fn validate_float_range() {
        let schema = FieldSchema::new("ratio").with_rule(ValidationRule::FloatRange(0.0, 1.0));
        assert!(schema.validate(Some("0.5")).passed);
        assert!(!schema.validate(Some("1.5")).passed);
    }

    #[test]
    fn validate_multiple_rules_all_pass() {
        let schema = FieldSchema::new("port")
            .with_rule(ValidationRule::Required)
            .with_rule(ValidationRule::Integer)
            .with_rule(ValidationRule::IntRange(1, 65535));
        assert!(schema.validate(Some("8080")).passed);
    }

    #[test]
    fn validate_multiple_rules_one_fails() {
        let schema = FieldSchema::new("port")
            .with_rule(ValidationRule::Required)
            .with_rule(ValidationRule::Integer)
            .with_rule(ValidationRule::IntRange(1, 1024));
        let result = schema.validate(Some("8080"));
        assert!(!result.passed);
        assert!(result.message.contains("range"));
    }

    #[test]
    fn report_merge_combines_results() {
        let r1 = ValidationResult::passed("a");
        let r2 = ValidationResult::failed("b", "err");
        let rep1 = ConfigValidationReport { results: vec![r1] };
        let rep2 = ConfigValidationReport { results: vec![r2] };
        let merged = rep1.merge(&rep2);
        assert_eq!(merged.total(), 2);
        assert_eq!(merged.failure_count(), 1);
    }

    #[test]
    fn report_filter_by_field() {
        let r1 = ValidationResult::passed("port");
        let r2 = ValidationResult::failed("host", "err");
        let rep = ConfigValidationReport {
            results: vec![r1, r2],
        };
        assert_eq!(rep.filter_by_field("port").len(), 1);
        assert_eq!(rep.filter_by_field("missing").len(), 0);
    }

    #[test]
    fn schema_field_names() {
        let schema = ConfigSchema::new()
            .add_field(FieldSchema::new("host"))
            .add_field(FieldSchema::new("port"));
        assert_eq!(schema.field_names(), vec!["host", "port"]);
    }

    #[test]
    fn report_all_results() {
        let rep = ConfigValidationReport {
            results: vec![ValidationResult::passed("a")],
        };
        assert_eq!(rep.all_results().len(), 1);
    }
}
