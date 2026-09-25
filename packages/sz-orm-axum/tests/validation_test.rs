//! M3: validation.rs 补充测试 — 覆盖未测的 RuleType/ValidationRule/FieldValidator/RequestValidator 方法

use std::collections::HashMap;

use sz_orm_axum::{
    FieldValidator, RequestValidator, RuleType, ValidationError, ValidationResult, ValidationRule,
};

// === RuleType::check 补充 ===

#[test]
fn rule_max_length_pass() {
    assert!(RuleType::MaxLength(5).check("abc").is_ok());
}

#[test]
fn rule_max_length_fail() {
    assert!(RuleType::MaxLength(2).check("abc").is_err());
}

#[test]
fn rule_min_value_pass() {
    assert!(RuleType::MinValue(10).check("42").is_ok());
}

#[test]
fn rule_min_value_fail() {
    assert!(RuleType::MinValue(50).check("42").is_err());
}

#[test]
fn rule_min_value_non_numeric() {
    assert!(RuleType::MinValue(10).check("abc").is_err());
}

#[test]
fn rule_max_value_pass() {
    assert!(RuleType::MaxValue(100).check("42").is_ok());
}

#[test]
fn rule_max_value_fail() {
    assert!(RuleType::MaxValue(10).check("42").is_err());
}

#[test]
fn rule_max_value_non_numeric() {
    assert!(RuleType::MaxValue(10).check("xyz").is_err());
}

#[test]
fn rule_pattern_pass() {
    assert!(RuleType::Pattern("abc".to_string()).check("xabc y").is_ok());
}

#[test]
fn rule_pattern_fail() {
    assert!(RuleType::Pattern("xyz".to_string()).check("abc").is_err());
}

#[test]
fn rule_one_of_pass() {
    let allowed = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    assert!(RuleType::OneOf(allowed).check("b").is_ok());
}

#[test]
fn rule_one_of_fail() {
    let allowed = vec!["a".to_string(), "b".to_string()];
    assert!(RuleType::OneOf(allowed).check("z").is_err());
}

#[test]
fn rule_not_blank_pass() {
    assert!(RuleType::NotBlank.check("  hello  ").is_ok());
}

#[test]
fn rule_not_blank_fail_empty() {
    assert!(RuleType::NotBlank.check("").is_err());
}

#[test]
fn rule_not_blank_fail_whitespace_only() {
    assert!(RuleType::NotBlank.check("   ").is_err());
}

#[test]
fn rule_email_short_fail() {
    assert!(RuleType::Email.check("a@").is_err());
}

#[test]
fn rule_email_no_dot_fail() {
    assert!(RuleType::Email.check("test@example").is_err());
}

// === RuleType::name() ===

#[test]
fn rule_name_all_variants() {
    assert_eq!(RuleType::Required.name(), "required");
    assert_eq!(RuleType::MinLength(3).name(), "min_length(3)");
    assert_eq!(RuleType::MaxLength(5).name(), "max_length(5)");
    assert_eq!(RuleType::MinValue(10).name(), "min_value(10)");
    assert_eq!(RuleType::MaxValue(99).name(), "max_value(99)");
    assert_eq!(RuleType::Pattern("x".to_string()).name(), "pattern(x)");
    assert_eq!(RuleType::OneOf(vec![]).name(), "one_of");
    assert_eq!(RuleType::Email.name(), "email");
    assert_eq!(RuleType::Numeric.name(), "numeric");
    assert_eq!(RuleType::NotBlank.name(), "not_blank");
}

// === ValidationRule ===

#[test]
fn validation_rule_new_and_accessors() {
    let rule = ValidationRule::new("email", RuleType::Required);
    assert_eq!(rule.field(), "email");
    assert_eq!(rule.rule(), &RuleType::Required);
}

#[test]
fn validation_rule_validate_ok() {
    let rule = ValidationRule::new("name", RuleType::Required);
    assert!(rule.validate("value").is_ok());
}

#[test]
fn validation_rule_validate_err() {
    let rule = ValidationRule::new("name", RuleType::Required);
    assert!(rule.validate("").is_err());
}

// === FieldValidator 补充 ===

#[test]
fn field_validator_min_length() {
    let v = FieldValidator::new("name").min_length(3);
    assert_eq!(v.rule_count(), 1);
    let errors = v.validate("ab");
    assert_eq!(errors.len(), 1);
    let errors = v.validate("abc");
    assert_eq!(errors.len(), 0);
}

#[test]
fn field_validator_numeric() {
    let v = FieldValidator::new("age").numeric();
    assert_eq!(v.rule_count(), 1);
    assert_eq!(v.validate("abc").len(), 1);
    assert_eq!(v.validate("42").len(), 0);
}

#[test]
fn field_validator_one_of() {
    let v = FieldValidator::new("role").one_of(vec!["admin".to_string(), "user".to_string()]);
    assert_eq!(v.rule_count(), 1);
    assert_eq!(v.validate("admin").len(), 0);
    assert_eq!(v.validate("guest").len(), 1);
}

#[test]
fn field_validator_not_blank() {
    let v = FieldValidator::new("comment").not_blank();
    assert_eq!(v.rule_count(), 1);
    assert_eq!(v.validate("  ").len(), 1);
    assert_eq!(v.validate("hi").len(), 0);
}

#[test]
fn field_validator_min_value() {
    let v = FieldValidator::new("age").min_value(18);
    assert_eq!(v.rule_count(), 1);
    assert_eq!(v.validate("10").len(), 1);
    assert_eq!(v.validate("25").len(), 0);
}

#[test]
fn field_validator_max_value() {
    let v = FieldValidator::new("age").max_value(120);
    assert_eq!(v.rule_count(), 1);
    assert_eq!(v.validate("200").len(), 1);
    assert_eq!(v.validate("50").len(), 0);
}

#[test]
fn field_validator_to_rules() {
    let v = FieldValidator::new("email").required().email();
    let rules = v.to_rules();
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[0].field(), "email");
    assert_eq!(rules[1].field(), "email");
}

#[test]
fn field_validator_combined_rules() {
    let v = FieldValidator::new("password")
        .required()
        .min_length(8)
        .max_length(32)
        .not_blank();
    assert_eq!(v.rule_count(), 4);
    assert_eq!(v.validate("").len(), 3);
    assert_eq!(v.validate("short").len(), 1);
    assert_eq!(v.validate("valid_password_123").len(), 0);
}

// === ValidationResult 补充 ===

#[test]
fn validation_result_errors_for_field() {
    let mut r = ValidationResult::success();
    r.add_error("name", "required", "REQ");
    r.add_error("email", "invalid", "FMT");
    r.add_error("name", "too short", "LEN");

    let name_errors = r.errors_for_field("name");
    assert_eq!(name_errors.len(), 2);
    let email_errors = r.errors_for_field("email");
    assert_eq!(email_errors.len(), 1);
    let missing_errors = r.errors_for_field("phone");
    assert_eq!(missing_errors.len(), 0);
}

#[test]
fn validation_result_to_json_empty() {
    let r = ValidationResult::success();
    assert_eq!(r.to_json(), "[]");
}

#[test]
fn validation_result_to_json_with_errors() {
    let mut r = ValidationResult::success();
    r.add_error("name", "is required", "REQUIRED");
    let json = r.to_json();
    assert!(json.contains("name"));
    assert!(json.contains("is required"));
    assert!(json.contains("REQUIRED"));
}

#[test]
fn validation_result_errors_slice() {
    let mut r = ValidationResult::success();
    r.add_error("a", "err", "E");
    let errors = r.errors();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].field(), "a");
}

#[test]
fn validation_error_equality() {
    let e1 = ValidationError::new("name", "err", "CODE");
    let e2 = ValidationError::new("name", "err", "CODE");
    let e3 = ValidationError::new("name", "diff", "CODE");
    assert_eq!(e1, e2);
    assert_ne!(e1, e3);
}

#[test]
fn validation_result_default_is_valid() {
    let r = ValidationResult::default();
    assert!(r.is_valid());
    assert_eq!(r.error_count(), 0);
}

// === RequestValidator 补充 ===

#[test]
fn request_validator_field_count() {
    let v = RequestValidator::new()
        .add_field(FieldValidator::new("a").required())
        .add_field(FieldValidator::new("b").required())
        .add_field(FieldValidator::new("c").required());
    assert_eq!(v.field_count(), 3);
}

#[test]
fn request_validator_total_rule_count() {
    let v = RequestValidator::new()
        .add_field(FieldValidator::new("a").required().min_length(3))
        .add_field(FieldValidator::new("b").email());
    assert_eq!(v.total_rule_count(), 3);
}

#[test]
fn request_validator_validate_field() {
    let v = RequestValidator::new().add_field(FieldValidator::new("email").required().email());
    let errors = v.validate_field("email", "");
    assert!(!errors.is_empty());
    let errors = v.validate_field("email", "test@example.com");
    assert!(errors.is_empty());
}

#[test]
fn request_validator_validate_field_unknown() {
    let v = RequestValidator::new().add_field(FieldValidator::new("name").required());
    let errors = v.validate_field("unknown", "value");
    assert!(errors.is_empty());
}

#[test]
fn request_validator_clear() {
    let mut v = RequestValidator::new()
        .add_field(FieldValidator::new("a").required())
        .add_field(FieldValidator::new("b").required());
    assert_eq!(v.field_count(), 2);
    v.clear();
    assert_eq!(v.field_count(), 0);
}

#[test]
fn request_validator_default() {
    let v = RequestValidator::default();
    assert_eq!(v.field_count(), 0);
    assert_eq!(v.total_rule_count(), 0);
}

#[test]
fn request_validator_complex_validate() {
    let v = RequestValidator::new()
        .add_field(
            FieldValidator::new("username")
                .required()
                .min_length(3)
                .max_length(20),
        )
        .add_field(FieldValidator::new("email").required().email())
        .add_field(
            FieldValidator::new("age")
                .numeric()
                .min_value(0)
                .max_value(150),
        );

    let mut data = HashMap::new();
    data.insert("username".to_string(), "alice".to_string());
    data.insert("email".to_string(), "alice@example.com".to_string());
    data.insert("age".to_string(), "30".to_string());
    let result = v.validate(&data);
    assert!(result.is_valid());

    let mut bad_data = HashMap::new();
    bad_data.insert("username".to_string(), "ab".to_string());
    bad_data.insert("email".to_string(), "not-an-email".to_string());
    bad_data.insert("age".to_string(), "200".to_string());
    let result = v.validate(&bad_data);
    assert!(!result.is_valid());
    assert!(result.error_count() >= 3);
}
