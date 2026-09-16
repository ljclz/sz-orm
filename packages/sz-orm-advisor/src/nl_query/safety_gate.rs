//! NL 查询安全校验门
//!
//! 对生成的 SQL 进行注入检测、DDL 禁止、非参数化检测。

use std::collections::HashSet;

/// 安全判定结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SafetyVerdict {
    /// 通过安全校验
    Pass,
    /// 检测到 SQL 注入风险
    InjectionDetected(String),
    /// 包含 DDL 语句
    DdlDetected(String),
    /// 包含非参数化条件
    NonParameterized(String),
    /// 包含危险关键字
    DangerousKeyword(String),
}

/// NL 查询安全校验门
pub struct NlQuerySafetyGate {
    allow_ddl: bool,
    dangerous_keywords: HashSet<String>,
}

impl Default for NlQuerySafetyGate {
    fn default() -> Self {
        Self::new()
    }
}

impl NlQuerySafetyGate {
    /// 创建安全校验门（默认禁止 DDL）
    pub fn new() -> Self {
        let dangerous_keywords = [
            "DROP",
            "TRUNCATE",
            "ALTER",
            "GRANT",
            "REVOKE",
            "SHUTDOWN",
            "EXEC",
            "EXECUTE",
            "xp_cmdshell",
            "LOAD_FILE",
            "INTO OUTFILE",
            "INTO DUMPFILE",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        Self {
            allow_ddl: false,
            dangerous_keywords,
        }
    }

    /// 允许 DDL 操作（需显式开启）
    pub fn allow_ddl(mut self) -> Self {
        self.allow_ddl = true;
        self
    }

    /// 校验 SQL 安全性
    pub fn validate(&self, sql: &str) -> SafetyVerdict {
        let upper = sql.to_uppercase();

        for kw in &self.dangerous_keywords {
            if upper.contains(kw) {
                return SafetyVerdict::DangerousKeyword(kw.clone());
            }
        }

        if !self.allow_ddl {
            let ddl_prefixes = ["CREATE ", "DROP ", "ALTER ", "TRUNCATE ", "RENAME "];
            for prefix in &ddl_prefixes {
                if upper.starts_with(prefix) || upper.contains(&format!(" {} ", prefix.trim())) {
                    return SafetyVerdict::DdlDetected(prefix.trim().to_string());
                }
            }
        }

        if Self::detect_injection(sql) {
            return SafetyVerdict::InjectionDetected("潜在 SQL 注入模式".into());
        }

        if Self::detect_non_parameterized(sql) {
            return SafetyVerdict::NonParameterized("WHERE 条件可能未参数化".into());
        }

        SafetyVerdict::Pass
    }

    /// 检测 SQL 注入模式
    fn detect_injection(sql: &str) -> bool {
        let patterns = [
            "' OR '1'='1",
            "' OR 1=1",
            "'/*",
            "*/'",
            "UNION SELECT",
            "'; DROP",
            "'; DELETE",
        ];
        let upper = sql.to_uppercase();
        if patterns.iter().any(|p| upper.contains(&p.to_uppercase())) {
            return true;
        }
        if upper.contains("; --") || upper.contains(";/*") {
            return true;
        }
        if upper.contains("; DROP")
            || upper.contains("; DELETE")
            || upper.contains("; INSERT")
            || upper.contains("; UPDATE")
        {
            return true;
        }
        false
    }

    /// 检测非参数化条件（WHERE col = 'value' 而非 WHERE col = $1）
    fn detect_non_parameterized(sql: &str) -> bool {
        let upper = sql.to_uppercase();
        if !upper.contains("WHERE") {
            return false;
        }
        let has_param =
            sql.contains('?') || sql.contains('$') || sql.contains(":param") || sql.contains("@");
        !has_param && upper.contains("= '")
    }

    /// 是否允许 DDL
    pub fn is_ddl_allowed(&self) -> bool {
        self.allow_ddl
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pass_simple_select() {
        let gate = NlQuerySafetyGate::new();
        let verdict = gate.validate("SELECT * FROM users WHERE id = ?");
        assert_eq!(verdict, SafetyVerdict::Pass);
    }

    #[test]
    fn test_pass_parameterized() {
        let gate = NlQuerySafetyGate::new();
        assert_eq!(
            gate.validate("SELECT name FROM products WHERE price > $1 AND category = $2"),
            SafetyVerdict::Pass
        );
    }

    #[test]
    fn test_injection_detected() {
        let gate = NlQuerySafetyGate::new();
        let verdict = gate.validate("SELECT * FROM users WHERE name = 'admin' OR '1'='1");
        assert!(matches!(verdict, SafetyVerdict::InjectionDetected(_)));
    }

    #[test]
    fn test_injection_semicolon() {
        let gate = NlQuerySafetyGate::new();
        let verdict = gate.validate("SELECT * FROM users; --");
        assert!(matches!(verdict, SafetyVerdict::InjectionDetected(_)));
    }

    #[test]
    fn test_ddl_blocked_by_default() {
        let gate = NlQuerySafetyGate::new();
        let verdict = gate.validate("CREATE TABLE evil (id INT)");
        assert!(matches!(verdict, SafetyVerdict::DdlDetected(_)));
    }

    #[test]
    fn test_ddl_allowed_when_explicit() {
        let gate = NlQuerySafetyGate::new().allow_ddl();
        let verdict = gate.validate("CREATE TABLE ok (id INT)");
        assert_eq!(verdict, SafetyVerdict::Pass);
    }

    #[test]
    fn test_drop_blocked() {
        let gate = NlQuerySafetyGate::new();
        let verdict = gate.validate("DROP TABLE users");
        assert!(matches!(verdict, SafetyVerdict::DangerousKeyword(_)));
    }

    #[test]
    fn test_non_parameterized_detected() {
        let gate = NlQuerySafetyGate::new();
        let verdict = gate.validate("SELECT * FROM users WHERE name = 'alice'");
        assert!(matches!(verdict, SafetyVerdict::NonParameterized(_)));
    }

    #[test]
    fn test_dangerous_exec() {
        let gate = NlQuerySafetyGate::new();
        let verdict = gate.validate("EXEC xp_cmdshell('dir')");
        assert!(matches!(verdict, SafetyVerdict::DangerousKeyword(_)));
    }

    #[test]
    fn test_is_ddl_allowed() {
        assert!(!NlQuerySafetyGate::new().is_ddl_allowed());
        assert!(NlQuerySafetyGate::new().allow_ddl().is_ddl_allowed());
    }
}
