//! 防全表 UPDATE/DELETE 攻击守卫（Safe SQL Guard）
//!
//! 对应文档 6.8 节改进项 26（防全表 UPDATE/DELETE 攻击拦截）。
//!
//! # 核心概念
//!
//! - **GuardPolicy**：守卫策略，配置是否允许无 WHERE 子句的 UPDATE/DELETE
//! - **SafeSqlGuard**：守卫实例，对 SQL 进行检查并拦截危险操作
//! - **GuardError**：守卫错误类型
//!
//! # 设计灵感
//!
//! - MyBatis-Plus `block-attack-inner-interceptor`（阻止全表更新/删除）
//! - MySQL `--safe-updates` 模式
//! - Hibernate `hibernate.query.mutation_strategy`
//!
//! # 使用示例
//!
//! ```no_run
//! use sz_orm_core::guard::{SafeSqlGuard, GuardPolicy};
//!
//! let guard = SafeSqlGuard::new(GuardPolicy::Strict);
//! // 拦截无 WHERE 子句的 UPDATE
//! assert!(guard.check("UPDATE users SET name = 'a'").is_err());
//! // 拦截无 WHERE 子句的 DELETE
//! assert!(guard.check("DELETE FROM users").is_err());
//! // 允许带 WHERE 子句的 UPDATE
//! assert!(guard.check("UPDATE users SET name = 'a' WHERE id = 1").is_ok());
//! ```

// ============================================================================
// GuardError — 守卫错误类型
// ============================================================================

/// 守卫错误类型
#[derive(Debug)]
pub enum GuardError {
    /// 全表 UPDATE（无 WHERE 子句）
    FullTableUpdate {
        /// 表名
        table: String,
    },
    /// 全表 DELETE（无 WHERE 子句）
    FullTableDelete {
        /// 表名
        table: String,
    },
    /// SQL 解析失败
    ParseError(String),
}

impl std::fmt::Display for GuardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GuardError::FullTableUpdate { table } => write!(
                f,
                "Blocked full-table UPDATE on `{}` (no WHERE clause). Add WHERE clause or use GuardPolicy::Permissive.",
                table
            ),
            GuardError::FullTableDelete { table } => write!(
                f,
                "Blocked full-table DELETE on `{}` (no WHERE clause). Add WHERE clause or use GuardPolicy::Permissive.",
                table
            ),
            GuardError::ParseError(msg) => write!(f, "SQL parse error in guard: {}", msg),
        }
    }
}

impl std::error::Error for GuardError {}

/// 守卫结果
pub type GuardResult<T> = Result<T, GuardError>;

// ============================================================================
// GuardPolicy — 守卫策略
// ============================================================================

/// 守卫策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GuardPolicy {
    /// 严格模式（默认）：禁止任何无 WHERE 子句的 UPDATE/DELETE
    #[default]
    Strict,
    /// 宽松模式：允许无 WHERE 子句的 UPDATE/DELETE（仅记录日志，不拦截）
    Permissive,
    /// 完全关闭守卫
    Disabled,
}

// ============================================================================
// SafeSqlGuard — 守卫实例
// ============================================================================

/// 防全表 UPDATE/DELETE 攻击守卫
///
/// 通过 SQL 解析检测是否存在 WHERE 子句，对全表 UPDATE/DELETE 操作进行拦截。
///
/// # 示例
///
/// ```
/// use sz_orm_core::guard::{SafeSqlGuard, GuardPolicy, GuardError};
///
/// let guard = SafeSqlGuard::new(GuardPolicy::Strict);
///
/// // 拦截无 WHERE 子句的 UPDATE
/// assert!(matches!(
///     guard.check("UPDATE users SET name = 'a'"),
///     Err(GuardError::FullTableUpdate { .. })
/// ));
///
/// // 允许带 WHERE 子句的 UPDATE
/// assert!(guard.check("UPDATE users SET name = 'a' WHERE id = 1").is_ok());
/// ```
#[derive(Debug, Clone, Copy)]
pub struct SafeSqlGuard {
    /// 策略
    pub policy: GuardPolicy,
}

impl SafeSqlGuard {
    /// 创建守卫实例
    pub fn new(policy: GuardPolicy) -> Self {
        Self { policy }
    }

    /// 创建默认（严格）守卫
    pub fn strict() -> Self {
        Self::new(GuardPolicy::Strict)
    }

    /// 创建宽松守卫
    pub fn permissive() -> Self {
        Self::new(GuardPolicy::Permissive)
    }

    /// 创建关闭守卫
    pub fn disabled() -> Self {
        Self::new(GuardPolicy::Disabled)
    }

    /// 检查 SQL 是否安全（拦截全表 UPDATE/DELETE）
    pub fn check(&self, sql: &str) -> GuardResult<()> {
        match self.policy {
            GuardPolicy::Disabled => Ok(()),
            GuardPolicy::Permissive => {
                // 宽松模式：仅记录但不拦截，简单返回 Ok
                // 生产环境可在此处接入日志系统
                Ok(())
            }
            GuardPolicy::Strict => check_sql_strict(sql),
        }
    }

    /// 检查 SQL 是否安全（与 check 等价，但明确表示检查 UPDATE）
    pub fn check_update(&self, sql: &str) -> GuardResult<()> {
        self.check(sql)
    }

    /// 检查 SQL 是否安全（与 check 等价，但明确表示检查 DELETE）
    pub fn check_delete(&self, sql: &str) -> GuardResult<()> {
        self.check(sql)
    }
}

impl Default for SafeSqlGuard {
    fn default() -> Self {
        Self::strict()
    }
}

// ============================================================================
// 内部解析函数
// ============================================================================

/// 严格模式检查 SQL：检测无 WHERE 子句的 UPDATE/DELETE
///
/// # 已知局限
///
/// 本守卫使用简化的字符串匹配，**不能**完全防御以下绕过场景：
/// - 子查询中的 WHERE 被误认为外层 UPDATE/DELETE 的 WHERE
///   （如 `UPDATE users SET name = (SELECT name FROM other WHERE id = 1)`）
/// - 字符串字面量中的 WHERE 关键字
///
/// 完全防御需要 SQL 解析器（如 `sqlparser` crate）。
/// 本守卫的目标是拦截**明显的**全表 UPDATE/DELETE（无 WHERE 子句）。
fn check_sql_strict(sql: &str) -> GuardResult<()> {
    let normalized = normalize_sql(sql);
    let upper = normalized.to_uppercase();

    // 检测是否为 UPDATE 语句（UPDATE 关键字开头）
    if upper.starts_with("UPDATE") {
        let table = extract_update_table(&normalized, &upper);
        // WHERE 必须在 SET 之后（避免 SET 之前的 WHERE 被误判）
        if !has_where_after_keyword(&upper, "SET") {
            return Err(GuardError::FullTableUpdate {
                table: table.unwrap_or_else(|| "unknown".to_string()),
            });
        }
    }

    // 检测是否为 DELETE 语句
    if upper.starts_with("DELETE") {
        let table = extract_delete_table(&normalized, &upper);
        // WHERE 必须在 FROM 之后
        if !has_where_after_keyword(&upper, "FROM") {
            return Err(GuardError::FullTableDelete {
                table: table.unwrap_or_else(|| "unknown".to_string()),
            });
        }
    }

    Ok(())
}

/// 规范化 SQL：去除多余空白、注释、换行
fn normalize_sql(sql: &str) -> String {
    // 去除单行注释（-- ...）
    let without_line_comments: String = sql
        .lines()
        .map(|line| {
            if let Some(idx) = line.find("--") {
                &line[..idx]
            } else {
                line
            }
        })
        .collect::<Vec<&str>>()
        .join(" ");

    // 去除多行注释（/* ... */）
    let mut without_block_comments = String::with_capacity(without_line_comments.len());
    let mut in_block_comment = false;
    let mut chars = without_line_comments.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '/' && chars.peek() == Some(&'*') {
            in_block_comment = true;
            chars.next(); // 消费 '*'
            continue;
        }
        if in_block_comment && c == '*' && chars.peek() == Some(&'/') {
            in_block_comment = false;
            chars.next(); // 消费 '/'
            continue;
        }
        if !in_block_comment {
            without_block_comments.push(c);
        }
    }

    // 折叠空白
    let mut result = String::with_capacity(without_block_comments.len());
    let mut prev_whitespace = false;
    for c in without_block_comments.chars() {
        if c.is_whitespace() {
            if !prev_whitespace {
                result.push(' ');
                prev_whitespace = true;
            }
        } else {
            result.push(c);
            prev_whitespace = false;
        }
    }
    result.trim().to_string()
}

/// 检测 SQL 在指定关键字（如 "SET" 或 "FROM"）之后是否包含 WHERE 子句
///
/// 规则：
/// - 必须找到 `keyword` 关键字（作为独立词）
/// - 在 `keyword` 之后必须包含 `WHERE` 关键字（独立词，且**括号深度为 0**）
/// - WHERE 子句后必须有非空内容
///
/// 此函数防御以下绕过：
/// - "SET 之前的 WHERE 被误认为 UPDATE 的 WHERE"
/// - "子查询中的 WHERE 被误认为外层 UPDATE/DELETE 的 WHERE"
///
/// 通过括号深度跟踪识别子查询：只有 depth=0 时的 WHERE 才算外层 WHERE。
fn has_where_after_keyword(upper_sql: &str, keyword: &str) -> bool {
    // 找到 keyword 的位置（首次出现，独立词匹配）
    let kw_pos = match find_keyword_independent(upper_sql, keyword) {
        Some(idx) => idx,
        None => return false,
    };

    // 在 keyword 之后查找 WHERE 关键字（独立词，且括号深度为 0）
    let after_kw = &upper_sql[kw_pos + keyword.len()..];
    let where_idx = match find_where_at_depth_zero(after_kw) {
        Some(idx) => idx,
        None => return false,
    };

    // WHERE 之后必须有非空内容
    let after_where = after_kw[where_idx + 5..].trim();
    !after_where.is_empty()
}

/// 在 SQL 中查找 WHERE 关键字的位置（独立词，且括号深度为 0）
///
/// 扫描字符串，跟踪括号深度（`(` +1, `)` -1），仅返回深度为 0 时的 WHERE 位置。
/// 这样可以正确区分外层 WHERE 和子查询中的 WHERE。
///
/// 注意：这是简化的实现，不处理字符串字面量中的括号（如 `WHERE name = '('`）。
/// 完全处理需要 SQL 词法分析器。
fn find_where_at_depth_zero(sql: &str) -> Option<usize> {
    let bytes = sql.as_bytes();
    let mut depth: i32 = 0;
    let mut i = 0;

    while i + 5 <= bytes.len() {
        let c = bytes[i];

        // 跟踪括号深度
        if c == b'(' {
            depth += 1;
            i += 1;
            continue;
        }
        if c == b')' {
            depth -= 1;
            if depth < 0 {
                depth = 0;
            }
            i += 1;
            continue;
        }

        // 仅在深度为 0 时查找 WHERE
        if depth == 0 && &bytes[i..i + 5] == b"WHERE" {
            // 检查前一个字符是否为单词边界
            let prev_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'_';
            // 检查后一个字符是否为单词边界
            let next_idx = i + 5;
            let next_ok = next_idx >= bytes.len()
                || !bytes[next_idx].is_ascii_alphanumeric() && bytes[next_idx] != b'_';
            if prev_ok && next_ok {
                return Some(i);
            }
        }

        i += 1;
    }

    None
}

/// 在 SQL 中查找指定关键字的位置（独立词匹配，大小写敏感——已 to_uppercase）
///
/// 关键字必须是纯单词（如 "SET"、"FROM"、"WHERE"），**不应包含空格**。
/// 函数会检查关键字前后是否为单词边界（非字母数字下划线）。
fn find_keyword_independent(sql: &str, keyword: &str) -> Option<usize> {
    let kw_len = keyword.len();
    if kw_len == 0 || sql.len() < kw_len {
        return None;
    }

    let bytes = sql.as_bytes();
    let kw_bytes = keyword.as_bytes();

    let mut i = 0;
    while i + kw_len <= bytes.len() {
        if &bytes[i..i + kw_len] == kw_bytes {
            // 检查前一个字符是否为单词边界
            let prev_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'_';
            // 检查后一个字符是否为单词边界
            let next_idx = i + kw_len;
            let next_ok = next_idx >= bytes.len()
                || !bytes[next_idx].is_ascii_alphanumeric() && bytes[next_idx] != b'_';
            if prev_ok && next_ok {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

/// 从 UPDATE 语句中提取表名
///
/// 支持格式：
/// - `UPDATE table SET ...`
/// - `UPDATE schema.table SET ...`
/// - `UPDATE `table` SET ...`
/// - `UPDATE "table" SET ...`
fn extract_update_table(sql: &str, upper: &str) -> Option<String> {
    // 找到 UPDATE 之后、SET 之前的内容
    let update_end = upper.find("UPDATE").map(|i| i + 6)?;
    let set_idx = upper.find(" SET ")?;

    let between = sql[update_end..set_idx].trim();

    // 处理反引号/双引号引用的表名
    let table = if (between.starts_with('`') && between.ends_with('`'))
        || (between.starts_with('"') && between.ends_with('"'))
    {
        between[1..between.len() - 1].to_string()
    } else {
        between.to_string()
    };

    // 处理 schema.table 格式，只取 table 部分
    let table = table.rsplit('.').next().unwrap_or(&table).to_string();

    if table.is_empty() {
        None
    } else {
        Some(table)
    }
}

/// 从 DELETE 语句中提取表名
///
/// 支持格式：
/// - `DELETE FROM table`
/// - `DELETE FROM schema.table`
/// - `DELETE FROM `table``
/// - `DELETE table WHERE ...`（MySQL 扩展语法）
fn extract_delete_table(sql: &str, upper: &str) -> Option<String> {
    // 优先匹配 DELETE FROM table
    if let Some(from_idx) = upper.find("DELETE FROM") {
        let after_from = sql[from_idx + 11..].trim_start();
        // 截取到下一个空格或末尾
        let end = after_from
            .find(|c: char| c.is_whitespace())
            .unwrap_or(after_from.len());
        let raw = after_from[..end].trim();

        return parse_table_name(raw);
    }

    // 处理 `DELETE table WHERE ...` 扩展语法
    if upper.starts_with("DELETE ") {
        let after_delete = sql[7..].trim_start();
        let end = after_delete
            .find(|c: char| c.is_whitespace())
            .unwrap_or(after_delete.len());
        let raw = after_delete[..end].trim();
        return parse_table_name(raw);
    }

    None
}

/// 解析表名（去除引号、schema 前缀）
fn parse_table_name(raw: &str) -> Option<String> {
    let table = if (raw.starts_with('`') && raw.ends_with('`') && raw.len() >= 2)
        || (raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2)
    {
        raw[1..raw.len() - 1].to_string()
    } else {
        raw.to_string()
    };

    let table = table.rsplit('.').next().unwrap_or(&table).to_string();

    if table.is_empty() {
        None
    } else {
        Some(table)
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ===== Strict 模式 - UPDATE 拦截 =====

    #[test]
    fn test_strict_blocks_update_without_where() {
        let guard = SafeSqlGuard::strict();
        let result = guard.check("UPDATE users SET name = 'a'");
        assert!(matches!(
            result,
            Err(GuardError::FullTableUpdate { table }) if table == "users"
        ));
    }

    #[test]
    fn test_strict_blocks_update_without_where_multiple_columns() {
        let guard = SafeSqlGuard::strict();
        let result = guard.check("UPDATE users SET name = 'a', age = 30, status = 'active'");
        assert!(matches!(result, Err(GuardError::FullTableUpdate { .. })));
    }

    #[test]
    fn test_strict_allows_update_with_where() {
        let guard = SafeSqlGuard::strict();
        let result = guard.check("UPDATE users SET name = 'a' WHERE id = 1");
        assert!(result.is_ok());
    }

    #[test]
    fn test_strict_allows_update_with_where_in() {
        let guard = SafeSqlGuard::strict();
        let result = guard.check("UPDATE users SET name = 'a' WHERE id IN (1, 2, 3)");
        assert!(result.is_ok());
    }

    #[test]
    fn test_strict_blocks_update_with_quoted_table() {
        let guard = SafeSqlGuard::strict();
        let result = guard.check("UPDATE `users` SET name = 'a'");
        assert!(matches!(
            result,
            Err(GuardError::FullTableUpdate { table }) if table == "users"
        ));
    }

    #[test]
    fn test_strict_blocks_update_with_double_quoted_table() {
        let guard = SafeSqlGuard::strict();
        let result = guard.check("UPDATE \"users\" SET name = 'a'");
        assert!(matches!(
            result,
            Err(GuardError::FullTableUpdate { table }) if table == "users"
        ));
    }

    #[test]
    fn test_strict_blocks_update_with_schema_qualified_table() {
        let guard = SafeSqlGuard::strict();
        let result = guard.check("UPDATE public.users SET name = 'a'");
        assert!(matches!(
            result,
            Err(GuardError::FullTableUpdate { table }) if table == "users"
        ));
    }

    // ===== Strict 模式 - DELETE 拦截 =====

    #[test]
    fn test_strict_blocks_delete_without_where() {
        let guard = SafeSqlGuard::strict();
        let result = guard.check("DELETE FROM users");
        assert!(matches!(
            result,
            Err(GuardError::FullTableDelete { table }) if table == "users"
        ));
    }

    #[test]
    fn test_strict_allows_delete_with_where() {
        let guard = SafeSqlGuard::strict();
        let result = guard.check("DELETE FROM users WHERE id = 1");
        assert!(result.is_ok());
    }

    #[test]
    fn test_strict_blocks_delete_with_quoted_table() {
        let guard = SafeSqlGuard::strict();
        let result = guard.check("DELETE FROM `users`");
        assert!(matches!(
            result,
            Err(GuardError::FullTableDelete { table }) if table == "users"
        ));
    }

    #[test]
    fn test_strict_blocks_delete_mysql_extension() {
        // MySQL 扩展语法：DELETE table WHERE ...
        // 这里测试无 WHERE 的情况
        let guard = SafeSqlGuard::strict();
        let result = guard.check("DELETE users");
        assert!(matches!(
            result,
            Err(GuardError::FullTableDelete { table }) if table == "users"
        ));
    }

    // ===== Permissive 模式 =====

    #[test]
    fn test_permissive_allows_update_without_where() {
        let guard = SafeSqlGuard::permissive();
        let result = guard.check("UPDATE users SET name = 'a'");
        assert!(result.is_ok());
    }

    #[test]
    fn test_permissive_allows_delete_without_where() {
        let guard = SafeSqlGuard::permissive();
        let result = guard.check("DELETE FROM users");
        assert!(result.is_ok());
    }

    // ===== Disabled 模式 =====

    #[test]
    fn test_disabled_allows_everything() {
        let guard = SafeSqlGuard::disabled();
        assert!(guard.check("UPDATE users SET name = 'a'").is_ok());
        assert!(guard.check("DELETE FROM users").is_ok());
    }

    // ===== 非 UPDATE/DELETE 语句 =====

    #[test]
    fn test_strict_allows_select_without_where() {
        let guard = SafeSqlGuard::strict();
        let result = guard.check("SELECT * FROM users");
        assert!(result.is_ok());
    }

    #[test]
    fn test_strict_allows_insert_without_where() {
        let guard = SafeSqlGuard::strict();
        let result = guard.check("INSERT INTO users (name) VALUES ('a')");
        assert!(result.is_ok());
    }

    #[test]
    fn test_strict_allows_create_table() {
        let guard = SafeSqlGuard::strict();
        let result = guard.check("CREATE TABLE users (id INT)");
        assert!(result.is_ok());
    }

    // ===== 多行/带注释 SQL =====

    #[test]
    fn test_strict_blocks_multiline_update_without_where() {
        let guard = SafeSqlGuard::strict();
        let sql = "UPDATE users\nSET name = 'a',\n    age = 30";
        let result = guard.check(sql);
        assert!(matches!(result, Err(GuardError::FullTableUpdate { .. })));
    }

    #[test]
    fn test_strict_allows_multiline_update_with_where() {
        let guard = SafeSqlGuard::strict();
        let sql = "UPDATE users\nSET name = 'a'\nWHERE id = 1";
        let result = guard.check(sql);
        assert!(result.is_ok());
    }

    #[test]
    fn test_strict_blocks_update_with_line_comment_only() {
        let guard = SafeSqlGuard::strict();
        let sql = "UPDATE users SET name = 'a' -- WHERE id = 1";
        let result = guard.check(sql);
        // 注释中的 WHERE 不应被识别为真正的 WHERE 子句
        assert!(matches!(result, Err(GuardError::FullTableUpdate { .. })));
    }

    #[test]
    fn test_strict_blocks_update_with_block_comment_only() {
        let guard = SafeSqlGuard::strict();
        let sql = "UPDATE users SET name = 'a' /* WHERE id = 1 */";
        let result = guard.check(sql);
        assert!(matches!(result, Err(GuardError::FullTableUpdate { .. })));
    }

    #[test]
    fn test_strict_allows_update_with_real_where_and_comment() {
        let guard = SafeSqlGuard::strict();
        let sql = "UPDATE users SET name = 'a' /* update name */ WHERE id = 1";
        let result = guard.check(sql);
        assert!(result.is_ok());
    }

    // ===== WHERE 关键字边界检测 =====

    #[test]
    fn test_strict_does_not_treat_nowhere_as_where() {
        // WHERE 是子串的情况：比如字段名包含 WHERE
        let guard = SafeSqlGuard::strict();
        // 表名/字段名中含 WHERE 子串，不应被误判为 WHERE 子句
        let sql = "UPDATE my_table SET somewhere = 'x'";
        let result = guard.check(sql);
        assert!(matches!(result, Err(GuardError::FullTableUpdate { .. })));
    }

    // ===== check_update / check_delete 便捷方法 =====

    #[test]
    fn test_check_update_method() {
        let guard = SafeSqlGuard::strict();
        assert!(guard.check_update("UPDATE users SET name = 'a'").is_err());
        assert!(guard
            .check_update("UPDATE users SET name = 'a' WHERE id = 1")
            .is_ok());
    }

    #[test]
    fn test_check_delete_method() {
        let guard = SafeSqlGuard::strict();
        assert!(guard.check_delete("DELETE FROM users").is_err());
        assert!(guard.check_delete("DELETE FROM users WHERE id = 1").is_ok());
    }

    // ===== Default =====

    #[test]
    fn test_default_guard_is_strict() {
        let guard = SafeSqlGuard::default();
        assert_eq!(guard.policy, GuardPolicy::Strict);
        assert!(guard.check("UPDATE users SET name = 'a'").is_err());
    }

    // ===== GuardError Display =====

    #[test]
    fn test_guard_error_display_full_table_update() {
        let e = GuardError::FullTableUpdate {
            table: "users".to_string(),
        };
        let s = format!("{}", e);
        assert!(s.contains("Blocked full-table UPDATE"));
        assert!(s.contains("users"));
    }

    #[test]
    fn test_guard_error_display_full_table_delete() {
        let e = GuardError::FullTableDelete {
            table: "orders".to_string(),
        };
        let s = format!("{}", e);
        assert!(s.contains("Blocked full-table DELETE"));
        assert!(s.contains("orders"));
    }

    // ===== GuardPolicy Default =====

    #[test]
    fn test_guard_policy_default_is_strict() {
        let p = GuardPolicy::default();
        assert_eq!(p, GuardPolicy::Strict);
    }

    // ===== 边界情况 =====

    #[test]
    fn test_strict_blocks_truncate() {
        // TRUNCATE 也是全表删除，但目前未实现 TRUNCATE 检测
        // 这里仅验证 TRUNCATE 不会被错误识别为 UPDATE/DELETE
        let guard = SafeSqlGuard::strict();
        // TRUNCATE 应该是允许的（不在守卫范围）
        let result = guard.check("TRUNCATE TABLE users");
        assert!(result.is_ok());
    }

    #[test]
    fn test_strict_blocks_update_with_lowercase_keywords() {
        let guard = SafeSqlGuard::strict();
        let result = guard.check("update users set name = 'a'");
        assert!(matches!(result, Err(GuardError::FullTableUpdate { .. })));
    }

    #[test]
    fn test_strict_blocks_delete_with_lowercase_keywords() {
        let guard = SafeSqlGuard::strict();
        let result = guard.check("delete from users");
        assert!(matches!(result, Err(GuardError::FullTableDelete { .. })));
    }

    #[test]
    fn test_strict_allows_update_with_lowercase_where() {
        let guard = SafeSqlGuard::strict();
        let result = guard.check("update users set name = 'a' where id = 1");
        assert!(result.is_ok());
    }

    #[test]
    fn test_strict_blocks_update_with_empty_where() {
        let guard = SafeSqlGuard::strict();
        // "UPDATE users SET name = 'a' WHERE" 后面没有内容
        let result = guard.check("UPDATE users SET name = 'a' WHERE ");
        assert!(matches!(result, Err(GuardError::FullTableUpdate { .. })));
    }

    #[test]
    fn test_normalize_sql_collapses_whitespace() {
        let sql = "UPDATE   users\n\nSET    name = 'a'";
        let normalized = normalize_sql(sql);
        assert_eq!(normalized, "UPDATE users SET name = 'a'");
    }

    #[test]
    fn test_normalize_sql_removes_line_comments() {
        let sql = "UPDATE users -- this is a comment\nSET name = 'a'";
        let normalized = normalize_sql(sql);
        assert!(normalized.contains("UPDATE users"));
        assert!(!normalized.contains("this is a comment"));
    }

    #[test]
    fn test_normalize_sql_removes_block_comments() {
        let sql = "UPDATE users /* block comment */ SET name = 'a'";
        let normalized = normalize_sql(sql);
        assert!(!normalized.contains("block comment"));
        assert!(normalized.contains("UPDATE users"));
        assert!(normalized.contains("SET name = 'a'"));
    }

    // ===== 子查询 WHERE 绕过防御（C2 修复回归测试） =====

    #[test]
    fn test_blocks_update_with_where_only_in_subquery() {
        // 子查询中的 WHERE 不应被误判为外层 UPDATE 的 WHERE
        // 这是 C2 修复的核心回归测试
        let guard = SafeSqlGuard::strict();
        let sql = "UPDATE users SET name = (SELECT name FROM other WHERE id = 1)";
        let result = guard.check(sql);
        assert!(
            matches!(result, Err(GuardError::FullTableUpdate { table }) if table == "users"),
            "子查询中的 WHERE 不应被误认为外层 UPDATE 的 WHERE，应拦截"
        );
    }

    #[test]
    fn test_blocks_delete_with_where_only_in_subquery() {
        // 子查询中的 WHERE 不应被误判为外层 DELETE 的 WHERE
        let guard = SafeSqlGuard::strict();
        let sql = "DELETE FROM users WHERE id IN (SELECT id FROM other)";
        // 上面的 WHERE 是真正的外层 WHERE，应该通过
        let result = guard.check(sql);
        assert!(result.is_ok(), "外层 WHERE 应被识别");

        // 但这个 SQL 外层没有 WHERE，子查询里有 WHERE，应该被拦截
        let sql2 = "DELETE FROM users RETURNING (SELECT id FROM other WHERE x = 1)";
        let result2 = guard.check(sql2);
        assert!(
            matches!(result2, Err(GuardError::FullTableDelete { table }) if table == "users"),
            "子查询中的 WHERE 不应被误认为外层 DELETE 的 WHERE，应拦截"
        );
    }

    #[test]
    fn test_allows_update_with_real_where_and_subquery_where() {
        // 外层有 WHERE + 子查询也有 WHERE，应该通过
        let guard = SafeSqlGuard::strict();
        let sql = "UPDATE users SET name = 'a' WHERE id IN (SELECT id FROM other WHERE active = 1)";
        let result = guard.check(sql);
        assert!(result.is_ok());
    }

    #[test]
    fn test_blocks_update_with_field_named_where() {
        // 字段名包含 WHERE 子串，不应被误判
        let guard = SafeSqlGuard::strict();
        let sql = "UPDATE my_table SET somewhere = 'x'";
        let result = guard.check(sql);
        assert!(matches!(result, Err(GuardError::FullTableUpdate { .. })));
    }
}
