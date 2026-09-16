//! ABAC 表达式注入防护
//!
//! 对属性键/值进行安全校验，防止表达式注入。

use super::policy_engine::AttributeValue;

/// 表达式沙箱
pub struct ExprSandbox;

impl ExprSandbox {
    /// 验证属性键名（仅允许字母/数字/下划线）
    pub fn validate_key(key: &str) -> Result<(), String> {
        if key.is_empty() {
            return Err("属性键为空".into());
        }
        if key.len() > 128 {
            return Err("属性键过长（>128）".into());
        }
        for (i, c) in key.chars().enumerate() {
            if !(c.is_ascii_alphanumeric() || c == '_') {
                return Err(format!("属性键 '{}' 第 {} 字符 '{}' 非法", key, i, c));
            }
        }
        Ok(())
    }

    /// 验证字符串属性值（拒绝危险字符）
    pub fn validate_string_value(value: &str) -> Result<(), String> {
        if value.len() > 1024 {
            return Err("属性值过长（>1024）".into());
        }
        let dangerous_patterns = ["${", "#{", "{{", "$(", "`", "\\x", "\\u"];
        for pattern in &dangerous_patterns {
            if value.contains(pattern) {
                return Err(format!("属性值包含危险模式: {}", pattern));
            }
        }
        Ok(())
    }

    /// 验证属性值
    pub fn validate_value(value: &AttributeValue) -> Result<(), String> {
        match value {
            AttributeValue::String(s) => Self::validate_string_value(s),
            AttributeValue::Int(_) => Ok(()),
            AttributeValue::Bool(_) => Ok(()),
        }
    }

    /// 清理字符串值（移除危险字符）
    pub fn sanitize(value: &str) -> String {
        value
            .chars()
            .filter(|c| !matches!(c, '$' | '`' | '\\' | '{' | '}' | '#' | '(' | ')'))
            .collect()
    }

    /// 验证操作名
    pub fn validate_action(action: &str) -> Result<(), String> {
        if action.is_empty() {
            return Err("操作名为空".into());
        }
        Self::validate_key(action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_valid_key() {
        assert!(ExprSandbox::validate_key("user_role").is_ok());
        assert!(ExprSandbox::validate_key("department123").is_ok());
    }

    #[test]
    fn test_validate_reject_special_chars() {
        assert!(ExprSandbox::validate_key("user-role").is_err());
        assert!(ExprSandbox::validate_key("user.role").is_err());
        assert!(ExprSandbox::validate_key("user role").is_err());
        assert!(ExprSandbox::validate_key("user'").is_err());
        assert!(ExprSandbox::validate_key("user;").is_err());
    }

    #[test]
    fn test_validate_reject_injection_patterns() {
        assert!(ExprSandbox::validate_string_value("${env:SECRET}").is_err());
        assert!(ExprSandbox::validate_string_value("#{1+1}").is_err());
        assert!(ExprSandbox::validate_string_value("{{template}}").is_err());
        assert!(ExprSandbox::validate_string_value("$(cmd)").is_err());
        assert!(ExprSandbox::validate_string_value("`cmd`").is_err());
    }

    #[test]
    fn test_validate_safe_string() {
        assert!(ExprSandbox::validate_string_value("admin").is_ok());
        assert!(ExprSandbox::validate_string_value("engineering dept").is_ok());
    }

    #[test]
    fn test_sanitize_removes_dangerous_chars() {
        assert_eq!(ExprSandbox::sanitize("${secret}"), "secret");
        assert_eq!(ExprSandbox::sanitize("safe_value"), "safe_value");
    }

    #[test]
    fn test_validate_action() {
        assert!(ExprSandbox::validate_action("read").is_ok());
        assert!(ExprSandbox::validate_action("write_data").is_ok());
        assert!(ExprSandbox::validate_action("").is_err());
        assert!(ExprSandbox::validate_action("read-write").is_err());
    }
}
