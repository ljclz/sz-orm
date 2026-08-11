//! # 校验规则函数（8 种）
//!
//! 每个函数返回 `Result<(), ValidationError>`，可组合使用 `aggregate` 聚合。

use super::ValidationError;
use regex::Regex;
use std::sync::OnceLock;

/// 邮箱正则缓存（RFC 5322 简化版）
static EMAIL_REGEX: OnceLock<Regex> = OnceLock::new();

fn email_regex() -> &'static Regex {
    EMAIL_REGEX
        .get_or_init(|| Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$").unwrap())
}

/// 验证邮箱格式
pub fn validate_email(field: &str, value: &str) -> Result<(), ValidationError> {
    if email_regex().is_match(value) {
        Ok(())
    } else {
        Err(ValidationError::Email {
            field: field.to_string(),
            value: value.to_string(),
        })
    }
}

/// 验证字符串长度在 [min, max] 范围内（Unicode 安全，按字符计数）
pub fn validate_length(
    field: &str,
    value: &str,
    min: usize,
    max: usize,
) -> Result<(), ValidationError> {
    let actual = value.chars().count();
    if actual >= min && actual <= max {
        Ok(())
    } else {
        Err(ValidationError::Length {
            field: field.to_string(),
            min,
            max,
            actual,
        })
    }
}

/// 验证数值在 [min, max] 范围内
pub fn validate_range<T: PartialOrd + std::fmt::Display>(
    field: &str,
    value: T,
    min: T,
    max: T,
) -> Result<(), ValidationError> {
    if value >= min && value <= max {
        Ok(())
    } else {
        Err(ValidationError::Range {
            field: field.to_string(),
            min: min.to_string(),
            max: max.to_string(),
            actual: value.to_string(),
        })
    }
}

/// 验证正则匹配
pub fn validate_regex(field: &str, value: &str, pattern: &str) -> Result<(), ValidationError> {
    match Regex::new(pattern) {
        Ok(re) => {
            if re.is_match(value) {
                Ok(())
            } else {
                Err(ValidationError::Regex {
                    field: field.to_string(),
                    pattern: pattern.to_string(),
                    value: value.to_string(),
                })
            }
        }
        Err(_) => Err(ValidationError::Regex {
            field: field.to_string(),
            pattern: pattern.to_string(),
            value: value.to_string(),
        }),
    }
}

/// 验证非空
pub fn validate_required(field: &str, value: &str) -> Result<(), ValidationError> {
    if value.is_empty() {
        Err(ValidationError::Required {
            field: field.to_string(),
        })
    } else {
        Ok(())
    }
}

/// 验证包含子串
pub fn validate_contains(field: &str, value: &str, substring: &str) -> Result<(), ValidationError> {
    if value.contains(substring) {
        Ok(())
    } else {
        Err(ValidationError::Contains {
            field: field.to_string(),
            substring: substring.to_string(),
        })
    }
}

/// 验证不包含子串
pub fn validate_does_not_contain(
    field: &str,
    value: &str,
    substring: &str,
) -> Result<(), ValidationError> {
    if !value.contains(substring) {
        Ok(())
    } else {
        Err(ValidationError::DoesNotContain {
            field: field.to_string(),
            substring: substring.to_string(),
        })
    }
}

/// 自定义校验失败
pub fn validate_custom(field: &str, reason: &str) -> Result<(), ValidationError> {
    Err(ValidationError::Custom {
        field: field.to_string(),
        reason: reason.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_email_valid() {
        assert!(validate_email("email", "user@example.com").is_ok());
        assert!(validate_email("email", "a.b+c@d.co").is_ok());
    }

    #[test]
    fn test_validate_email_invalid() {
        assert!(validate_email("email", "noatsign").is_err());
        assert!(validate_email("email", "").is_err());
        assert!(validate_email("email", "a@").is_err());
        assert!(validate_email("email", "@b.com").is_err());
    }

    #[test]
    fn test_validate_length_boundary() {
        assert!(validate_length("name", "abc", 3, 5).is_ok());
        assert!(validate_length("name", "abcde", 3, 5).is_ok());
    }

    #[test]
    fn test_validate_length_too_long() {
        assert!(validate_length("name", "abcdef", 3, 5).is_err());
    }

    #[test]
    fn test_validate_length_empty() {
        assert!(validate_length("name", "", 1, 5).is_err());
    }

    #[test]
    fn test_validate_length_unicode() {
        assert!(validate_length("name", "你好", 2, 2).is_ok());
    }

    #[test]
    fn test_validate_range_boundary() {
        assert!(validate_range("age", 18i64, 18, 65).is_ok());
        assert!(validate_range("age", 65i64, 18, 65).is_ok());
    }

    #[test]
    fn test_validate_range_out_of_range() {
        assert!(validate_range("age", 17i64, 18, 65).is_err());
        assert!(validate_range("age", 66i64, 18, 65).is_err());
    }

    #[test]
    fn test_validate_range_negative() {
        assert!(validate_range("temp", -10.5f64, -20.0, 50.0).is_ok());
        assert!(validate_range("temp", -30.0f64, -20.0, 50.0).is_err());
    }

    #[test]
    fn test_validate_regex_match() {
        assert!(validate_regex("code", "ABC123", r"^[A-Z]{3}\d{3}$").is_ok());
    }

    #[test]
    fn test_validate_regex_no_match() {
        assert!(validate_regex("code", "abc123", r"^[A-Z]{3}\d{3}$").is_err());
    }

    #[test]
    fn test_validate_regex_invalid_pattern() {
        assert!(validate_regex("code", "abc", r"[invalid").is_err());
    }

    #[test]
    fn test_validate_required_non_empty() {
        assert!(validate_required("name", "value").is_ok());
    }

    #[test]
    fn test_validate_required_empty() {
        assert!(validate_required("name", "").is_err());
    }

    #[test]
    fn test_validate_contains_found() {
        assert!(validate_contains("url", "https://example.com", "https://").is_ok());
    }

    #[test]
    fn test_validate_contains_not_found() {
        assert!(validate_contains("url", "ftp://example.com", "https://").is_err());
    }

    #[test]
    fn test_validate_contains_empty_substring() {
        assert!(validate_contains("name", "anything", "").is_ok());
    }

    #[test]
    fn test_validate_does_not_contain_clean() {
        assert!(validate_does_not_contain("name", "hello", "sql").is_ok());
    }

    #[test]
    fn test_validate_does_not_contain_dirty() {
        assert!(validate_does_not_contain("name", "drop table", "drop").is_err());
    }

    #[test]
    fn test_validate_does_not_contain_empty_substring() {
        assert!(validate_does_not_contain("name", "anything", "").is_err());
    }

    #[test]
    fn test_aggregate_empty() {
        let results = vec![];
        assert!(super::super::aggregate(results).is_ok());
    }

    #[test]
    fn test_aggregate_all_ok() {
        let results = vec![Ok(()), Ok(()), Ok(())];
        assert!(super::super::aggregate(results).is_ok());
    }

    #[test]
    fn test_aggregate_single_error() {
        let results = vec![Ok(()), Err(ValidationError::Required { field: "x".into() })];
        let result = super::super::aggregate(results);
        assert!(matches!(result, Err(ValidationError::Required { .. })));
    }

    #[test]
    fn test_aggregate_multiple_errors() {
        let results = vec![
            Err(ValidationError::Required { field: "a".into() }),
            Ok(()),
            Err(ValidationError::Required { field: "b".into() }),
        ];
        let result = super::super::aggregate(results);
        match result {
            Err(ValidationError::Aggregate { errors, count }) => {
                assert_eq!(errors.len(), 2);
                assert_eq!(count, 2);
            }
            _ => panic!("expected Aggregate with 2 errors"),
        }
    }
}
