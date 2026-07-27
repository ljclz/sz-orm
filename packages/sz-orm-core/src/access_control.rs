//! 行级和字段级权限控制
//!
//! 提供基于租户/用户的行级数据隔离和字段级访问控制

use std::collections::{HashMap, HashSet};

/// 权限规则
#[derive(Debug, Clone)]
pub struct AccessRule {
    /// 表名
    pub table: String,
    /// 行级过滤条件（SQL WHERE 子句片段）
    pub row_filter: Option<String>,
    /// 允许查询的字段列表（None 表示允许所有字段）
    pub allowed_columns: Option<HashSet<String>>,
    /// 禁止查询的字段列表
    pub denied_columns: HashSet<String>,
}

/// 访问控制上下文
#[derive(Debug, Clone, Default)]
pub struct AccessContext {
    /// 当前租户 ID
    pub tenant_id: Option<String>,
    /// 当前用户 ID
    pub user_id: Option<String>,
    /// 角色列表
    pub roles: Vec<String>,
    /// 表级权限规则
    rules: HashMap<String, AccessRule>,
}

impl AccessContext {
    /// 创建新的访问控制上下文
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置租户 ID
    pub fn with_tenant(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }

    /// 设置用户 ID
    pub fn with_user(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    /// 添加访问规则
    pub fn add_rule(&mut self, rule: AccessRule) {
        self.rules.insert(rule.table.clone(), rule);
    }

    /// 获取表的行级过滤条件
    pub fn row_filter(&self, table: &str) -> Option<&str> {
        self.rules.get(table).and_then(|r| r.row_filter.as_deref())
    }

    /// 检查字段是否允许查询
    pub fn is_column_allowed(&self, table: &str, column: &str) -> bool {
        if let Some(rule) = self.rules.get(table) {
            if rule.denied_columns.contains(column) {
                return false;
            }
            if let Some(ref allowed) = rule.allowed_columns {
                return allowed.contains(column);
            }
        }
        true
    }

    /// 过滤字段列表，返回允许查询的字段
    pub fn filter_columns(&self, table: &str, columns: &[String]) -> Vec<String> {
        columns
            .iter()
            .filter(|col| self.is_column_allowed(table, col))
            .cloned()
            .collect()
    }
}

/// 行级权限构建器
pub struct RowLevelSecurity {
    context: AccessContext,
}

impl RowLevelSecurity {
    pub fn new(context: AccessContext) -> Self {
        Self { context }
    }

    /// 为表添加租户隔离规则
    pub fn tenant_isolation(mut self, table: &str, tenant_column: &str) -> Self {
        if let Some(ref tenant_id) = self.context.tenant_id {
            self.context.add_rule(AccessRule {
                table: table.to_string(),
                row_filter: Some(format!("{} = '{}'", tenant_column, tenant_id)),
                allowed_columns: None,
                denied_columns: HashSet::new(),
            });
        }
        self
    }

    /// 为表添加用户隔离规则
    pub fn user_isolation(mut self, table: &str, user_column: &str) -> Self {
        if let Some(ref user_id) = self.context.user_id {
            self.context.add_rule(AccessRule {
                table: table.to_string(),
                row_filter: Some(format!("{} = '{}'", user_column, user_id)),
                allowed_columns: None,
                denied_columns: HashSet::new(),
            });
        }
        self
    }

    /// 禁止查询敏感字段
    pub fn deny_columns(mut self, table: &str, columns: &[&str]) -> Self {
        let rule = self
            .context
            .rules
            .entry(table.to_string())
            .or_insert(AccessRule {
                table: table.to_string(),
                row_filter: None,
                allowed_columns: None,
                denied_columns: HashSet::new(),
            });
        for col in columns {
            rule.denied_columns.insert(col.to_string());
        }
        self
    }

    pub fn build(self) -> AccessContext {
        self.context
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_access_context_default() {
        let ctx = AccessContext::new();
        assert!(ctx.tenant_id.is_none());
        assert!(ctx.user_id.is_none());
        assert!(ctx.roles.is_empty());
    }

    #[test]
    fn test_with_tenant_and_user() {
        let ctx = AccessContext::new()
            .with_tenant("tenant-1")
            .with_user("user-1");
        assert_eq!(ctx.tenant_id.as_deref(), Some("tenant-1"));
        assert_eq!(ctx.user_id.as_deref(), Some("user-1"));
    }

    #[test]
    fn test_column_allowed_by_default() {
        let ctx = AccessContext::new();
        assert!(ctx.is_column_allowed("users", "id"));
        assert!(ctx.is_column_allowed("users", "password"));
    }

    #[test]
    fn test_deny_columns() {
        let mut ctx = AccessContext::new();
        ctx.add_rule(AccessRule {
            table: "users".to_string(),
            row_filter: None,
            allowed_columns: None,
            denied_columns: ["password".to_string()].into_iter().collect(),
        });
        assert!(!ctx.is_column_allowed("users", "password"));
        assert!(ctx.is_column_allowed("users", "id"));
    }

    #[test]
    fn test_allowed_columns_whitelist() {
        let mut ctx = AccessContext::new();
        let mut allowed: HashSet<String> = HashSet::new();
        allowed.insert("id".to_string());
        allowed.insert("name".to_string());
        ctx.add_rule(AccessRule {
            table: "users".to_string(),
            row_filter: None,
            allowed_columns: Some(allowed),
            denied_columns: HashSet::new(),
        });
        assert!(ctx.is_column_allowed("users", "id"));
        assert!(ctx.is_column_allowed("users", "name"));
        assert!(!ctx.is_column_allowed("users", "secret"));
    }

    #[test]
    fn test_filter_columns() {
        let mut ctx = AccessContext::new();
        ctx.add_rule(AccessRule {
            table: "users".to_string(),
            row_filter: None,
            allowed_columns: None,
            denied_columns: ["password".to_string()].into_iter().collect(),
        });
        let cols = vec!["id".to_string(), "name".to_string(), "password".to_string()];
        let filtered = ctx.filter_columns("users", &cols);
        assert_eq!(filtered, vec!["id".to_string(), "name".to_string()]);
    }

    #[test]
    fn test_row_filter() {
        let mut ctx = AccessContext::new();
        ctx.add_rule(AccessRule {
            table: "orders".to_string(),
            row_filter: Some("tenant_id = 't1'".to_string()),
            allowed_columns: None,
            denied_columns: HashSet::new(),
        });
        assert_eq!(ctx.row_filter("orders"), Some("tenant_id = 't1'"));
        assert_eq!(ctx.row_filter("users"), None);
    }

    #[test]
    fn test_row_level_security_tenant_isolation() {
        let ctx = AccessContext::new().with_tenant("tenant-42");
        let built = RowLevelSecurity::new(ctx)
            .tenant_isolation("orders", "tenant_id")
            .build();
        assert_eq!(built.row_filter("orders"), Some("tenant_id = 'tenant-42'"));
    }

    #[test]
    fn test_row_level_security_user_isolation() {
        let ctx = AccessContext::new().with_user("u-1");
        let built = RowLevelSecurity::new(ctx)
            .user_isolation("profiles", "user_id")
            .build();
        assert_eq!(built.row_filter("profiles"), Some("user_id = 'u-1'"));
    }

    #[test]
    fn test_row_level_security_deny_columns() {
        let ctx = AccessContext::new();
        let built = RowLevelSecurity::new(ctx)
            .deny_columns("users", &["password", "salt"])
            .build();
        assert!(!built.is_column_allowed("users", "password"));
        assert!(!built.is_column_allowed("users", "salt"));
        assert!(built.is_column_allowed("users", "id"));
    }

    #[test]
    fn test_tenant_isolation_skipped_without_tenant() {
        // 未设置 tenant_id 时不应添加规则
        let ctx = AccessContext::new();
        let built = RowLevelSecurity::new(ctx)
            .tenant_isolation("orders", "tenant_id")
            .build();
        assert_eq!(built.row_filter("orders"), None);
    }
}
