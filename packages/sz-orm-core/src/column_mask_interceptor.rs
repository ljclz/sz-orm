//! 列级脱敏拦截器（v7.1.0）
//!
//! 对查询结果中的敏感列进行脱敏处理。

use std::collections::{HashMap, HashSet};

use serde_json::Value;

/// 脱敏策略
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaskStrategy {
    /// 完全遮蔽
    Full,
    /// 保留前 N 位
    Prefix(usize),
    /// 保留后 N 位
    Suffix(usize),
    /// 哈希脱敏
    Hash,
    /// 替换为固定值
    Replace(String),
}

/// 列脱敏规则
#[derive(Debug, Clone)]
pub struct ColumnMaskRule {
    /// 表名
    pub table: String,
    /// 列名
    pub column: String,
    /// 脱敏策略
    pub strategy: MaskStrategy,
}

impl ColumnMaskRule {
    /// 创建规则
    pub fn new(table: &str, column: &str, strategy: MaskStrategy) -> Self {
        Self {
            table: table.to_string(),
            column: column.to_string(),
            strategy,
        }
    }

    /// 应用脱敏
    pub fn apply(&self, value: &Value) -> Value {
        match value {
            Value::String(s) => Value::String(self.mask_string(s)),
            Value::Null => Value::Null,
            other => Value::String(self.mask_string(&other.to_string())),
        }
    }

    fn mask_string(&self, s: &str) -> String {
        match &self.strategy {
            MaskStrategy::Full => "***".to_string(),
            MaskStrategy::Prefix(n) => {
                let chars: Vec<char> = s.chars().collect();
                if chars.len() <= *n {
                    "***".to_string()
                } else {
                    let prefix: String = chars.iter().take(*n).collect();
                    format!("{}{}", prefix, "*".repeat(chars.len() - *n))
                }
            }
            MaskStrategy::Suffix(n) => {
                let chars: Vec<char> = s.chars().collect();
                if chars.len() <= *n {
                    "***".to_string()
                } else {
                    let suffix: String = chars
                        .iter()
                        .rev()
                        .take(*n)
                        .collect::<String>()
                        .chars()
                        .rev()
                        .collect();
                    format!("{}{}", "*".repeat(chars.len() - *n), suffix)
                }
            }
            MaskStrategy::Hash => {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::Hasher;
                let mut hasher = DefaultHasher::new();
                std::hash::Hash::hash(&s, &mut hasher);
                format!("{:016x}", hasher.finish())
            }
            MaskStrategy::Replace(val) => val.clone(),
        }
    }
}

/// 列级脱敏拦截器
pub struct ColumnMaskInterceptor {
    rules: HashMap<String, Vec<ColumnMaskRule>>,
}

impl ColumnMaskInterceptor {
    /// 创建拦截器
    pub fn new() -> Self {
        Self {
            rules: HashMap::new(),
        }
    }

    /// 添加脱敏规则
    pub fn add_rule(&mut self, rule: ColumnMaskRule) {
        self.rules.entry(rule.table.clone()).or_default().push(rule);
    }

    /// 对查询结果行应用脱敏
    pub fn mask_row(&self, table: &str, row: &mut HashMap<String, Value>) {
        if let Some(rules) = self.rules.get(table) {
            for rule in rules {
                if let Some(value) = row.get_mut(&rule.column) {
                    *value = rule.apply(value);
                }
            }
        }
    }

    /// 获取表的脱敏列
    pub fn masked_columns(&self, table: &str) -> HashSet<String> {
        match self.rules.get(table) {
            Some(rules) => rules.iter().map(|r| r.column.clone()).collect(),
            None => HashSet::new(),
        }
    }

    /// 规则数
    pub fn rule_count(&self) -> usize {
        self.rules.values().map(|v| v.len()).sum()
    }
}

impl Default for ColumnMaskInterceptor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_full_mask() {
        let rule = ColumnMaskRule::new("users", "password", MaskStrategy::Full);
        let result = rule.apply(&Value::String("secret123".into()));
        assert_eq!(result, Value::String("***".into()));
    }

    #[test]
    fn test_prefix_mask() {
        let rule = ColumnMaskRule::new("users", "email", MaskStrategy::Prefix(3));
        let result = rule.apply(&Value::String("alice@example.com".into()));
        assert!(result.as_str().unwrap().starts_with("ali"));
        assert!(result.as_str().unwrap().contains("*"));
    }

    #[test]
    fn test_suffix_mask() {
        let rule = ColumnMaskRule::new("users", "phone", MaskStrategy::Suffix(4));
        let result = rule.apply(&Value::String("13812345678".into()));
        let s = result.as_str().unwrap();
        assert!(s.ends_with("5678"));
        assert!(s.contains("*"));
    }

    #[test]
    fn test_replace_mask() {
        let rule = ColumnMaskRule::new("users", "ssn", MaskStrategy::Replace("[REDACTED]".into()));
        let result = rule.apply(&Value::String("123-45-6789".into()));
        assert_eq!(result, Value::String("[REDACTED]".into()));
    }

    #[test]
    fn test_interceptor_mask_row() {
        let mut interceptor = ColumnMaskInterceptor::new();
        interceptor.add_rule(ColumnMaskRule::new("users", "password", MaskStrategy::Full));
        interceptor.add_rule(ColumnMaskRule::new(
            "users",
            "email",
            MaskStrategy::Prefix(2),
        ));
        let mut row = HashMap::new();
        row.insert("password".into(), Value::String("secret".into()));
        row.insert("email".into(), Value::String("alice@test.com".into()));
        row.insert("name".into(), Value::String("Alice".into()));
        interceptor.mask_row("users", &mut row);
        assert_eq!(row["password"], Value::String("***".into()));
        assert!(row["email"].as_str().unwrap().starts_with("al"));
        assert_eq!(row["name"], Value::String("Alice".into()));
    }

    #[test]
    fn test_masked_columns() {
        let mut interceptor = ColumnMaskInterceptor::new();
        interceptor.add_rule(ColumnMaskRule::new("users", "password", MaskStrategy::Full));
        interceptor.add_rule(ColumnMaskRule::new("users", "ssn", MaskStrategy::Full));
        let cols = interceptor.masked_columns("users");
        assert_eq!(cols.len(), 2);
        assert!(cols.contains("password"));
        assert!(cols.contains("ssn"));
    }
}
