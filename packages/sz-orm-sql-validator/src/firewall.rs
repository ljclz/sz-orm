//! SQL 防火墙：运行时拦截危险 SQL

use std::sync::RwLock;

/// SQL 防火墙规则
#[derive(Debug, Clone)]
pub struct FirewallRule {
    /// 规则名称
    pub name: String,
    /// 匹配模式（正则表达式）
    pub pattern: String,
    /// 规则动作
    pub action: FirewallAction,
    /// 例外模式：若匹配则规则不触发（替代正则 lookahead，Rust regex 不支持 lookahead）
    pub unless_pattern: Option<String>,
}

/// 防火墙动作
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FirewallAction {
    /// 阻断
    Block,
    /// 记录但放行
    Log,
    /// 需要审批
    RequireApproval,
}

/// SQL 防火墙
pub struct SqlFirewall {
    rules: RwLock<Vec<FirewallRule>>,
    blocked_count: std::sync::atomic::AtomicU64,
    logged_count: std::sync::atomic::AtomicU64,
}

impl SqlFirewall {
    pub fn new() -> Self {
        // 默认规则（使用 vec![] 宏避免 clippy::vec_init_then_push 警告）
        let rules = vec![
            FirewallRule {
                name: "block_drop_table".into(),
                pattern: r"(?i)\bDROP\s+TABLE\b".into(),
                action: FirewallAction::Block,
                unless_pattern: None,
            },
            FirewallRule {
                name: "block_truncate".into(),
                pattern: r"(?i)\bTRUNCATE\b".into(),
                action: FirewallAction::Block,
                unless_pattern: None,
            },
            FirewallRule {
                name: "block_drop_database".into(),
                pattern: r"(?i)\bDROP\s+DATABASE\b".into(),
                action: FirewallAction::Block,
                unless_pattern: None,
            },
            FirewallRule {
                name: "block_drop_schema".into(),
                pattern: r"(?i)\bDROP\s+SCHEMA\b".into(),
                action: FirewallAction::Block,
                unless_pattern: None,
            },
            FirewallRule {
                name: "block_alter_table_drop".into(),
                pattern: r"(?i)\bALTER\s+TABLE\b.*\bDROP\b".into(),
                action: FirewallAction::Block,
                unless_pattern: None,
            },
            // DELETE FROM without WHERE — Rust regex 不支持 lookahead，改用 unless_pattern 表达"含 WHERE 则放行"
            FirewallRule {
                name: "log_delete_without_where".into(),
                pattern: r"(?i)\bDELETE\s+FROM\b".into(),
                action: FirewallAction::Block,
                unless_pattern: Some(r"(?i)\bWHERE\b".into()),
            },
            FirewallRule {
                name: "block_update_without_where".into(),
                pattern: r"(?i)\bUPDATE\b.*\bSET\b".into(),
                action: FirewallAction::Block,
                unless_pattern: Some(r"(?i)\bWHERE\b".into()),
            },
            FirewallRule {
                name: "block_grant".into(),
                pattern: r"(?i)\bGRANT\b".into(),
                action: FirewallAction::Block,
                unless_pattern: None,
            },
            FirewallRule {
                name: "block_revoke".into(),
                pattern: r"(?i)\bREVOKE\b".into(),
                action: FirewallAction::Block,
                unless_pattern: None,
            },
        ];

        Self {
            rules: RwLock::new(rules),
            blocked_count: std::sync::atomic::AtomicU64::new(0),
            logged_count: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// 检查 SQL 是否允许执行
    pub fn check(&self, sql: &str) -> Result<(), FirewallViolation> {
        let rules = self.rules.read().expect("Firewall rules lock poisoned");
        for rule in rules.iter() {
            if let Ok(regex) = regex::Regex::new(&rule.pattern) {
                if regex.is_match(sql) {
                    // 检查例外条件：若 unless_pattern 匹配，则跳过此规则
                    if let Some(unless_pat) = &rule.unless_pattern {
                        if let Ok(unless_re) = regex::Regex::new(unless_pat) {
                            if unless_re.is_match(sql) {
                                continue;
                            }
                        }
                    }
                    match rule.action {
                        FirewallAction::Block => {
                            self.blocked_count
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            return Err(FirewallViolation {
                                rule_name: rule.name.clone(),
                                sql: sql.to_string(),
                                action: rule.action.clone(),
                            });
                        }
                        FirewallAction::Log => {
                            self.logged_count
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                        FirewallAction::RequireApproval => {
                            self.logged_count
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// 添加自定义规则
    pub fn add_rule(&self, rule: FirewallRule) {
        let mut rules = self.rules.write().expect("Firewall rules lock poisoned");
        rules.push(rule);
    }

    /// 获取被阻断次数
    pub fn blocked_count(&self) -> u64 {
        self.blocked_count
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// 获取被记录次数
    pub fn logged_count(&self) -> u64 {
        self.logged_count.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl Default for SqlFirewall {
    fn default() -> Self {
        Self::new()
    }
}

/// 防火墙违规
#[derive(Debug, Clone)]
pub struct FirewallViolation {
    pub rule_name: String,
    pub sql: String,
    pub action: FirewallAction,
}

impl std::fmt::Display for FirewallViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SQL blocked by rule '{}': {}", self.rule_name, self.sql)
    }
}

impl std::error::Error for FirewallViolation {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allows_simple_select() {
        let fw = SqlFirewall::new();
        assert!(fw.check("SELECT id, name FROM users WHERE id = 1").is_ok());
    }

    #[test]
    fn test_blocks_drop_table() {
        let fw = SqlFirewall::new();
        let result = fw.check("DROP TABLE users");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.rule_name, "block_drop_table");
        assert_eq!(err.action, FirewallAction::Block);
    }

    #[test]
    fn test_blocks_truncate() {
        let fw = SqlFirewall::new();
        assert!(fw.check("TRUNCATE TABLE users").is_err());
    }

    #[test]
    fn test_blocks_drop_database() {
        let fw = SqlFirewall::new();
        assert!(fw.check("DROP DATABASE prod").is_err());
    }

    #[test]
    fn test_blocks_drop_schema() {
        let fw = SqlFirewall::new();
        assert!(fw.check("DROP SCHEMA public").is_err());
    }

    #[test]
    fn test_blocks_alter_table_drop() {
        let fw = SqlFirewall::new();
        assert!(fw.check("ALTER TABLE users DROP COLUMN password").is_err());
    }

    #[test]
    fn test_blocks_delete_without_where() {
        let fw = SqlFirewall::new();
        assert!(fw.check("DELETE FROM users").is_err());
    }

    #[test]
    fn test_allows_delete_with_where() {
        let fw = SqlFirewall::new();
        assert!(fw.check("DELETE FROM users WHERE id = 1").is_ok());
    }

    #[test]
    fn test_blocks_update_without_where() {
        let fw = SqlFirewall::new();
        assert!(fw.check("UPDATE users SET name = 'a'").is_err());
    }

    #[test]
    fn test_allows_update_with_where() {
        let fw = SqlFirewall::new();
        assert!(fw.check("UPDATE users SET name = 'a' WHERE id = 1").is_ok());
    }

    #[test]
    fn test_blocks_grant() {
        let fw = SqlFirewall::new();
        assert!(fw.check("GRANT SELECT ON users TO app").is_err());
    }

    #[test]
    fn test_blocks_revoke() {
        let fw = SqlFirewall::new();
        assert!(fw.check("REVOKE SELECT ON users FROM app").is_err());
    }

    #[test]
    fn test_blocked_count_increments() {
        let fw = SqlFirewall::new();
        assert_eq!(fw.blocked_count(), 0);
        let _ = fw.check("DROP TABLE x");
        let _ = fw.check("TRUNCATE TABLE y");
        assert_eq!(fw.blocked_count(), 2);
    }

    #[test]
    fn test_add_custom_rule() {
        let fw = SqlFirewall::new();
        fw.add_rule(FirewallRule {
            name: "block_select_star".into(),
            pattern: r"(?i)SELECT\s+\*".into(),
            action: FirewallAction::Block,
            unless_pattern: None,
        });
        // 自定义规则应拦截 SELECT *
        assert!(fw.check("SELECT * FROM users").is_err());
        // 普通 SELECT 仍允许
        assert!(fw.check("SELECT id FROM users").is_ok());
    }

    #[test]
    fn test_violation_display() {
        let v = FirewallViolation {
            rule_name: "block_drop_table".to_string(),
            sql: "DROP TABLE users".to_string(),
            action: FirewallAction::Block,
        };
        let s = format!("{}", v);
        assert!(s.contains("block_drop_table"));
        assert!(s.contains("DROP TABLE users"));
    }

    #[test]
    fn test_case_insensitive_match() {
        let fw = SqlFirewall::new();
        assert!(fw.check("drop table users").is_err());
        assert!(fw.check("Drop Table users").is_err());
    }

    #[test]
    fn test_default_impl() {
        let fw = SqlFirewall::default();
        assert!(fw.check("SELECT 1").is_ok());
    }
}
