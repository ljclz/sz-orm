//! SQL 安全验证模块
//!
//! 提供 SQL 生成的安全检查功能：
//! - 只允许 SELECT 语句
//! - 检测 SQL 注入常见模式
//! - 清理危险 SQL 构造

/// 验证 SQL 是否为只读 SELECT 查询。
///
/// 仅允许以 `SELECT`（不区分大小写）开头的语句，
/// 拒绝 DROP / ALTER / TRUNCATE / INSERT / UPDATE / DELETE 等写入操作。
pub fn validate_select_only(sql: &str) -> bool {
    sql.trim().to_uppercase().starts_with("SELECT")
}

/// 检查 SQL 是否包含注入风险模式。
///
/// 检测以下危险构造：
/// - SQL 注释（`--`、`/**/`、`#`）
/// - `UNION` 语句（常用于数据泄露）
/// - 写入类 DDL/DML 关键字
/// - `OR 1=1` / `OR '1'='1'` 布尔注入
/// - 引号逃逸（`';`）
pub fn validate_no_injection(sql: &str) -> bool {
    let upper = sql.to_uppercase();

    // 禁止 SQL 注释（注入常用逃逸手段）
    if upper.contains("--") || upper.contains("/*") || upper.contains('#') {
        return false;
    }

    // 禁止 UNION（常用于数据泄露）
    if upper.contains(" UNION ") || upper.contains("\nUNION ") {
        return false;
    }

    // 禁止写入类关键字
    if upper.contains(" DROP ")
        || upper.contains(" ALTER ")
        || upper.contains(" TRUNCATE ")
        || upper.contains(" INSERT ")
        || upper.contains(" UPDATE ")
        || upper.contains(" DELETE ")
        || upper.contains(" CREATE ")
    {
        return false;
    }

    // 禁止布尔注入（OR 1=1 等恒真条件）
    if upper.contains("OR 1=1")
        || upper.contains("OR 1 = 1")
        || upper.contains("OR '1'='1'")
        || upper.contains("OR \"1\"=\"1\"")
    {
        return false;
    }

    // 禁止引号逃逸
    if upper.contains("';") || upper.contains("\";") || sql.contains('\'') && upper.contains(" OR ")
    {
        // 简单启发：单引号后跟 OR 可能是注入
        let lower = sql.to_lowercase();
        if let Some(pos) = lower.find('\'') {
            let after = &lower[pos + 1..];
            if after.trim_start().starts_with("or ") {
                return false;
            }
        }
    }

    true
}

/// 清理 SQL 中的危险模式，返回安全的 SQL 字符串。
///
/// 执行以下清理：
/// - 移除行注释（`--` 到行尾）
/// - 移除块注释（`/* ... */`）
/// - 保留 ASCII 可见字符和空白
pub fn sanitize_sql(sql: &str) -> String {
    let mut result = String::with_capacity(sql.len());

    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        // 跳过行注释 `--` 到行尾
        if i + 1 < chars.len() && chars[i] == '-' && chars[i + 1] == '-' {
            // 跳过到换行符
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        // 跳过块注释 `/* ... */`
        if i + 1 < chars.len() && chars[i] == '/' && chars[i + 1] == '*' {
            i += 2; // 跳过 /*
            while i + 1 < chars.len() {
                if chars[i] == '*' && chars[i + 1] == '/' {
                    i += 2; // 跳过 */
                    break;
                }
                i += 1;
            }
            continue;
        }
        // 保留 ASCII 图形字符和空白
        if chars[i].is_ascii_graphic() || chars[i].is_ascii_whitespace() {
            result.push(chars[i]);
        }
        i += 1;
    }

    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============ validate_select_only ============

    #[test]
    fn test_validate_select_only_valid() {
        assert!(validate_select_only("SELECT * FROM users"));
        assert!(validate_select_only("select id, name from users"));
        assert!(validate_select_only("  SELECT count(*) FROM orders"));
        assert!(validate_select_only("SELECT DISTINCT city FROM users"));
    }

    #[test]
    fn test_validate_select_only_rejects_write_statements() {
        assert!(!validate_select_only("DROP TABLE users"));
        assert!(!validate_select_only("ALTER TABLE users ADD col INT"));
        assert!(!validate_select_only("TRUNCATE TABLE users"));
        assert!(!validate_select_only("INSERT INTO users VALUES (1)"));
        assert!(!validate_select_only("UPDATE users SET name = 'x'"));
        assert!(!validate_select_only("DELETE FROM users"));
        assert!(!validate_select_only("CREATE TABLE t (id INT)"));
    }

    // ============ validate_no_injection ============

    #[test]
    fn test_validate_no_injection_clean() {
        assert!(validate_no_injection("SELECT * FROM users WHERE id = $1"));
        assert!(validate_no_injection(
            "SELECT name, age FROM users ORDER BY name ASC"
        ));
        assert!(validate_no_injection("SELECT COUNT(*) FROM orders"));
    }

    #[test]
    fn test_validate_no_injection_rejects_comments() {
        assert!(!validate_no_injection("SELECT * FROM users -- comment"));
        assert!(!validate_no_injection(
            "SELECT * FROM users /* block */ WHERE id = 1"
        ));
        assert!(!validate_no_injection(
            "SELECT * FROM users # inline comment"
        ));
    }

    #[test]
    fn test_validate_no_injection_rejects_union() {
        assert!(!validate_no_injection(
            "SELECT * FROM users UNION SELECT * FROM admins"
        ));
    }

    #[test]
    fn test_validate_no_injection_rejects_boolean_injection() {
        assert!(!validate_no_injection(
            "SELECT * FROM users WHERE id = 1 OR 1=1"
        ));
        assert!(!validate_no_injection(
            "SELECT * FROM users WHERE name = '' OR '1'='1'"
        ));
        assert!(!validate_no_injection(
            "SELECT * FROM users WHERE pass = '' OR \"1\"=\"1\""
        ));
    }

    #[test]
    fn test_validate_no_injection_rejects_write_keywords() {
        assert!(!validate_no_injection(
            "SELECT * FROM users; DROP TABLE users"
        ));
        assert!(!validate_no_injection(
            "SELECT * FROM users; DELETE FROM users"
        ));
        assert!(!validate_no_injection(
            "SELECT * FROM users; INSERT INTO admins VALUES (1)"
        ));
    }

    // ============ sanitize_sql ============

    #[test]
    fn test_sanitize_sql_removes_line_comment() {
        let sql = "SELECT * FROM users -- this is a comment\nWHERE id = 1";
        let cleaned = sanitize_sql(sql);
        assert!(!cleaned.contains("--"));
        assert!(cleaned.contains("WHERE"));
    }

    #[test]
    fn test_sanitize_sql_removes_block_comment() {
        let sql = "SELECT * /* nasty injection */ FROM users";
        let cleaned = sanitize_sql(sql);
        assert!(!cleaned.contains("/*"));
        assert!(cleaned.contains("SELECT *"));
        assert!(cleaned.contains("FROM users"));
    }

    #[test]
    fn test_sanitize_sql_clean_passthrough() {
        let sql = "SELECT * FROM users WHERE id = $1";
        assert_eq!(sanitize_sql(sql), sql);
    }

    #[test]
    fn test_sanitize_sql_empty() {
        assert_eq!(sanitize_sql(""), "");
        assert_eq!(sanitize_sql("   "), "");
    }

    #[test]
    fn test_sanitize_sql_removes_control_chars() {
        let sql = "SELECT\x00 * FROM\x1b users";
        let cleaned = sanitize_sql(sql);
        assert_eq!(cleaned, "SELECT * FROM users");
    }

    #[test]
    fn test_sanitize_sql_multiple_comments() {
        let sql = "SELECT a -- line1\n, b -- line2\nFROM t /* block */ WHERE x = 1";
        let cleaned = sanitize_sql(sql);
        assert!(!cleaned.contains("--"));
        assert!(!cleaned.contains("/*"));
        assert!(cleaned.contains("SELECT a"));
        assert!(cleaned.contains(", b"));
        assert!(cleaned.contains("WHERE"));
    }
}
