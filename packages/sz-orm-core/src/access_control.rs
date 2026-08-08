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
    ///
    /// # 安全
    /// - `table` 会校验为合法 SQL 标识符（防注入）
    /// - `tenant_column` 会校验为合法 SQL 标识符（防注入）
    /// - `tenant_id` 会转义单引号与反斜杠（防注入）
    pub fn tenant_isolation(mut self, table: &str, tenant_column: &str) -> Self {
        if let Some(ref tenant_id) = self.context.tenant_id {
            // 校验表名为合法标识符
            if crate::sql_safety::validate_identifier(table, "table").is_err() {
                return self;
            }
            // 校验列名为合法标识符
            if crate::sql_safety::validate_identifier(tenant_column, "tenant_column").is_err() {
                return self;
            }
            let escaped_id = escape_sql_literal(tenant_id);
            self.context.add_rule(AccessRule {
                table: table.to_string(),
                row_filter: Some(format!("{} = '{}'", tenant_column, escaped_id)),
                allowed_columns: None,
                denied_columns: HashSet::new(),
            });
        }
        self
    }

    /// 为表添加用户隔离规则
    ///
    /// # 安全
    /// - `table` 会校验为合法 SQL 标识符（防注入）
    /// - `user_column` 会校验为合法 SQL 标识符（防注入）
    /// - `user_id` 会转义单引号与反斜杠（防注入）
    pub fn user_isolation(mut self, table: &str, user_column: &str) -> Self {
        if let Some(ref user_id) = self.context.user_id {
            // 校验表名为合法标识符
            if crate::sql_safety::validate_identifier(table, "table").is_err() {
                return self;
            }
            // 校验列名为合法标识符
            if crate::sql_safety::validate_identifier(user_column, "user_column").is_err() {
                return self;
            }
            let escaped_id = escape_sql_literal(user_id);
            self.context.add_rule(AccessRule {
                table: table.to_string(),
                row_filter: Some(format!("{} = '{}'", user_column, escaped_id)),
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

/// 转义 SQL 字面量字符串
///
/// # 处理规则
///
/// 1. 单引号 `'` → `''`（SQL 标准）
/// 2. 反斜杠 `\` → `\\`（MySQL 默认模式 `NO_BACKSLASH_ESCAPES` 未启用时为转义字符）
/// 3. NULL 字节 `\0` → `\0`（MySQL 会截断字符串）
/// 4. 换行 `\n` / 回车 `\r` → `\\n` / `\\r`（防止日志注入）
/// 5. Ctrl+Z `\x1a` → `\\Z`（Windows MySQL 截断字符）
///
/// # 注意
///
/// 此函数仅用于无法使用参数化查询的边角场景（如动态 WHERE 拼接）。
/// **首选方案永远是参数化查询**（`?` 占位符 + Value 绑定）。
fn escape_sql_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for ch in s.chars() {
        match ch {
            '\'' => out.push_str("''"),
            '\\' => out.push_str("\\\\"),
            '\0' => out.push_str("\\0"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\x1a' => out.push_str("\\Z"),
            other => out.push(other),
        }
    }
    out
}

// ─── v3.3.0 multi-tenant-enhanced：行级安全策略集成 ──────────────────

#[cfg(feature = "multi-tenant-enhanced")]
impl AccessContext {
    /// 应用行级安全策略，返回参数化过滤条件
    ///
    /// 从 `TenantContext` 的权限中查找匹配表名的行级安全策略，
    /// 返回其参数化过滤条件。既有 `AccessRule` 不变。
    pub fn apply_row_level_security(
        ctx: &crate::tenant_context::TenantContext,
        table: &str,
    ) -> Option<crate::tenant_security::ParameterizedCondition> {
        ctx.permissions
            .row_level_policies
            .iter()
            .find(|p| p.table == table && p.principal.tenant_id == ctx.tenant_id)
            .map(|p| p.filter_condition.clone())
    }

    /// 应用列级脱敏规则，返回匹配的脱敏规则列表
    ///
    /// 从 `TenantContext` 的权限中查找匹配表名 + 列名的脱敏规则，
    /// 且规则适用于当前角色列表。
    pub fn apply_column_masking(
        ctx: &crate::tenant_context::TenantContext,
        table: &str,
        column: &str,
    ) -> Option<crate::tenant_security::ColumnMaskingRule> {
        ctx.permissions
            .column_masking_rules
            .iter()
            .find(|r| {
                r.table == table && r.column == column && r.applies_to(&ctx.permissions.roles)
            })
            .cloned()
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

    // ===== SQL 注入防护测试 =====

    #[test]
    fn test_escape_sql_literal_single_quote() {
        // 单引号 → '' （SQL 标准）
        assert_eq!(escape_sql_literal("O'Brien"), "O''Brien");
    }

    #[test]
    fn test_escape_sql_literal_backslash() {
        // 反斜杠 → \\（MySQL 默认模式防注入）
        assert_eq!(escape_sql_literal(r"a\b"), r"a\\b");
        assert_eq!(escape_sql_literal(r"\"), r"\\");
    }

    #[test]
    fn test_escape_sql_literal_classic_injection() {
        // 经典注入 payload：' OR '1'='1
        let escaped = escape_sql_literal("' OR '1'='1");
        // 单引号成对出现，不会破坏外层字面量
        let quote_count = escaped.matches('\'').count();
        assert_eq!(quote_count % 2, 0, "escaped quotes must be paired");
        assert_eq!(escaped, "'' OR ''1''=''1");
    }

    #[test]
    fn test_escape_sql_literal_mysql_backslash_injection() {
        // MySQL 注入 payload：\'
        // 攻击者用反斜杠让单引号转义失效，escape 后应同时处理 \ 和 '
        let payload = r"\' OR 1=1--";
        let escaped = escape_sql_literal(payload);
        // \ → \\，' → ''，结果不应包含未配对单引号
        assert_eq!(escaped, r"\\'' OR 1=1--");
        let quote_count = escaped.matches('\'').count();
        assert_eq!(quote_count % 2, 0, "escaped quotes must be paired");
    }

    #[test]
    fn test_escape_sql_literal_null_byte() {
        // NULL 字节会被 MySQL 截断字符串
        assert_eq!(escape_sql_literal("a\0b"), "a\\0b");
    }

    #[test]
    fn test_escape_sql_literal_newline_carriage_return() {
        // 换行/回车防止日志注入
        assert_eq!(escape_sql_literal("a\nb\rc"), r"a\nb\rc");
    }

    #[test]
    fn test_escape_sql_literal_ctrl_z() {
        // Windows MySQL Ctrl+Z 截断：\x1a → \Z（反斜杠 + Z，共 2 字符）
        assert_eq!(escape_sql_literal("a\x1ab"), "a\\Zb");
    }

    #[test]
    fn test_tenant_isolation_rejects_invalid_table_name() {
        // 表名为非法标识符时不应添加规则
        let ctx = AccessContext::new().with_tenant("t1");
        let built = RowLevelSecurity::new(ctx)
            .tenant_isolation("orders; DROP TABLE users", "tenant_id")
            .build();
        assert_eq!(built.row_filter("orders; DROP TABLE users"), None);
    }

    #[test]
    fn test_tenant_isolation_rejects_invalid_column_name() {
        // 列名为非法标识符时不应添加规则
        let ctx = AccessContext::new().with_tenant("t1");
        let built = RowLevelSecurity::new(ctx)
            .tenant_isolation("orders", "tenant_id; DROP TABLE users")
            .build();
        assert_eq!(built.row_filter("orders"), None);
    }

    #[test]
    fn test_tenant_isolation_escapes_tenant_id_injection() {
        // tenant_id 含注入 payload，应被正确转义
        let ctx = AccessContext::new().with_tenant("' OR '1'='1");
        let built = RowLevelSecurity::new(ctx)
            .tenant_isolation("orders", "tenant_id")
            .build();
        let filter = built.row_filter("orders").unwrap();
        // 单引号应被转义为成对出现
        let quote_count = filter.matches('\'').count();
        assert_eq!(
            quote_count % 2,
            0,
            "tenant_id injection not escaped: {filter}"
        );
        assert_eq!(filter, "tenant_id = ''' OR ''1''=''1'");
    }

    #[test]
    fn test_user_isolation_escapes_user_id_backslash_injection() {
        // user_id 含反斜杠注入 payload
        let ctx = AccessContext::new().with_user(r"\' OR 1=1--");
        let built = RowLevelSecurity::new(ctx)
            .user_isolation("profiles", "user_id")
            .build();
        let filter = built.row_filter("profiles").unwrap();
        let quote_count = filter.matches('\'').count();
        assert_eq!(
            quote_count % 2,
            0,
            "user_id backslash injection not escaped: {filter}"
        );
    }
}
