//! 自定义编译期诊断信息模块
//!
//! 为 typed-dsl 类型不匹配提供比 Rust 默认更清晰的编译期错误信息。
//! 诊断信息结构包含：错误位置、期望类型、实际类型、修复建议。

#![allow(dead_code)]

/// 诊断信息结构（编译期构建，运行时不会使用）
#[derive(Debug, Clone)]
pub struct TypeMismatchDiagnostic {
    /// 错误位置（列名或表达式描述）
    pub location: String,
    /// 期望的 SqlType
    pub expected: String,
    /// 实际发现的 SqlType
    pub found: String,
    /// 修复建议
    pub suggestion: String,
}

impl TypeMismatchDiagnostic {
    /// 创建新的类型不匹配诊断
    pub fn new(location: &str, expected: &str, found: &str, suggestion: &str) -> Self {
        Self {
            location: location.to_string(),
            expected: expected.to_string(),
            found: found.to_string(),
            suggestion: suggestion.to_string(),
        }
    }

    /// 格式化为编译器友好的错误信息
    pub fn format_error(&self) -> String {
        format!(
            "类型不匹配：{} 期望 `{}`，但发现 `{}`\n  help: {}",
            self.location, self.expected, self.found, self.suggestion
        )
    }
}

/// 常见诊断场景的预设建议信息
pub mod suggestions {
    /// Eq<C, T> 约束失败：列的 RustType 与比较值类型不匹配
    pub const TYPE_MISMATCH_EQ: &str = "请使用 Cast 显式转换，或检查列归属表";

    /// And<L, R> / Or<L, R> 约束失败：操作数不是 Bool 类型
    pub const NON_BOOLEAN_LOGIC: &str = "逻辑组合操作要求 Bool 类型，请检查表达式是否为比较操作";

    /// filter<E> 跨表列引用：ExprTable<Table = T> 约束失败
    pub const CROSS_TABLE_REFERENCE: &str = "列不属于当前查询的表，请检查列归属表或使用 JOIN";

    /// Cast<S, T> 约束失败：源类型不可转换为目标类型
    pub const INVALID_CAST: &str = "该类型转换不受支持，请检查 SqlType 兼容性";

    /// JSON 操作符约束失败：列不是 JSON 类型
    pub const NON_JSON_COLUMN: &str = "JSON 操作符要求列的 SqlType 为 Json，请检查列定义";
}

/// 去除字符串字面量的引号
pub fn strip_quotes(s: &str) -> &str {
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- M3-T5.1: 基础字段验证测试 ----

    #[test]
    fn test_diagnostic_location() {
        let d = TypeMismatchDiagnostic::new("users.age", "i64", "String", "使用 Cast 转换");
        assert_eq!(d.location, "users.age");
    }

    #[test]
    fn test_diagnostic_expected_type() {
        let d = TypeMismatchDiagnostic::new("col", "Bool", "i64", "使用 Cast 转换");
        assert_eq!(d.expected, "Bool");
    }

    #[test]
    fn test_diagnostic_found_type() {
        let d = TypeMismatchDiagnostic::new("col", "Bool", "i64", "使用 Cast 转换");
        assert_eq!(d.found, "i64");
    }

    #[test]
    fn test_diagnostic_suggestion() {
        let d = TypeMismatchDiagnostic::new("col", "Bool", "i64", "使用 Cast 转换");
        assert_eq!(d.suggestion, "使用 Cast 转换");
    }

    #[test]
    fn test_diagnostic_format_error() {
        let d = TypeMismatchDiagnostic::new("users.age", "i64", "String", "使用 Cast 转换");
        let msg = d.format_error();
        assert!(msg.contains("类型不匹配"));
        assert!(msg.contains("users.age"));
        assert!(msg.contains("i64"));
        assert!(msg.contains("String"));
        assert!(msg.contains("使用 Cast 转换"));
        assert!(msg.contains("help:"));
    }

    // ---- M3-T5.2: 各诊断场景测试 ----

    #[test]
    fn test_suggestion_type_mismatch_eq() {
        assert!(suggestions::TYPE_MISMATCH_EQ.contains("Cast"));
    }

    #[test]
    fn test_suggestion_non_boolean_logic() {
        assert!(suggestions::NON_BOOLEAN_LOGIC.contains("Bool"));
    }

    #[test]
    fn test_suggestion_cross_table_reference() {
        assert!(suggestions::CROSS_TABLE_REFERENCE.contains("JOIN"));
    }

    #[test]
    fn test_suggestion_invalid_cast() {
        assert!(suggestions::INVALID_CAST.contains("类型转换"));
    }

    #[test]
    fn test_suggestion_non_json_column() {
        assert!(suggestions::NON_JSON_COLUMN.contains("Json"));
    }

    // ---- strip_quotes 测试 ----

    #[test]
    fn test_strip_quotes_with_quotes() {
        assert_eq!(strip_quotes("\"hello\""), "hello");
    }

    #[test]
    fn test_strip_quotes_without_quotes() {
        assert_eq!(strip_quotes("hello"), "hello");
    }

    #[test]
    fn test_strip_quotes_empty() {
        assert_eq!(strip_quotes(""), "");
    }
}
