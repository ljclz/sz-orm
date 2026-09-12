//! # 迁移脚手架（v6.9.0 REQ-DEV-004）
//!
//! 从 up SQL 自动推导 down SQL 逆操作。

/// 逆操作推导结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeriveDownResult {
    /// 推导出的 down SQL
    pub down_sql: String,
    /// 是否需要手动补充
    pub requires_manual: bool,
    /// 告警消息
    pub warnings: Vec<String>,
}

/// 从 up SQL 推导 down SQL
///
/// 支持的转换规则：
/// - `CREATE TABLE xxx` → `DROP TABLE xxx`
/// - `ALTER TABLE xxx ADD COLUMN col` → `ALTER TABLE xxx DROP COLUMN col`
/// - `INSERT INTO xxx` → `DELETE FROM xxx`（条件无法推导，留空）
/// - 无法推导 → down 留空 + 告警
pub fn derive_down(up_sql: &str) -> DeriveDownResult {
    let mut down_statements = Vec::new();
    let mut warnings = Vec::new();
    let mut requires_manual = false;

    for statement in split_sql_statements(up_sql) {
        let trimmed = statement.trim();
        if trimmed.is_empty() {
            continue;
        }

        let upper = trimmed.to_uppercase();

        if upper.starts_with("CREATE TABLE") {
            if let Some(table) = extract_table_name(&upper, trimmed, "CREATE TABLE") {
                down_statements.push(format!("DROP TABLE IF EXISTS {};", table));
            }
        } else if upper.starts_with("ALTER TABLE") && upper.contains("ADD COLUMN") {
            if let Some((table, column)) = extract_add_column(&upper, trimmed) {
                down_statements.push(format!("ALTER TABLE {} DROP COLUMN {};", table, column));
            }
        } else if upper.starts_with("ALTER TABLE") && upper.contains("ADD INDEX") {
            if let Some((table, index)) = extract_add_index(&upper, trimmed) {
                down_statements.push(format!("DROP INDEX {} ON {};", index, table));
            }
        } else if upper.starts_with("CREATE INDEX") {
            if let Some(index) = extract_index_name(&upper, trimmed) {
                down_statements.push(format!("DROP INDEX IF EXISTS {};", index));
            }
        } else if upper.starts_with("INSERT INTO") {
            if let Some(table) = extract_table_name(&upper, trimmed, "INSERT INTO") {
                warnings.push(format!(
                    "INSERT INTO {} → DELETE FROM {} (条件无法自动推导，需手动补充 WHERE)",
                    table, table
                ));
                requires_manual = true;
            }
        } else if upper.starts_with("UPDATE") {
            warnings.push(format!(
                "UPDATE 语句无法自动推导逆操作: {}",
                truncate(trimmed, 50)
            ));
            requires_manual = true;
        } else if upper.starts_with("DELETE") {
            warnings.push(format!(
                "DELETE 语句无法自动推导逆操作: {}",
                truncate(trimmed, 50)
            ));
            requires_manual = true;
        } else {
            warnings.push(format!("无法推导逆操作: {}", truncate(trimmed, 50)));
            requires_manual = true;
        }
    }

    let down_sql = if down_statements.is_empty() {
        String::new()
    } else {
        down_statements.join("\n")
    };

    DeriveDownResult {
        down_sql,
        requires_manual,
        warnings,
    }
}

fn split_sql_statements(sql: &str) -> Vec<String> {
    sql.split(';')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn extract_table_name(upper: &str, original: &str, prefix: &str) -> Option<String> {
    let after_prefix = upper.strip_prefix(prefix)?;
    let table_start = after_prefix
        .char_indices()
        .find(|(_, c)| !c.is_whitespace())?
        .0;
    let after_table_start = &original[prefix.len() + table_start..];
    let table = after_table_start
        .split(|c: char| c.is_whitespace() || c == '(' || c == ';')
        .next()?;
    if table.is_empty() {
        None
    } else {
        Some(table.to_string())
    }
}

fn extract_add_column(upper: &str, original: &str) -> Option<(String, String)> {
    let after_alter = upper.strip_prefix("ALTER TABLE")?;
    let _table_end = after_alter.find("ADD COLUMN")?;
    let table = original["ALTER TABLE".len()..].trim();
    let table_name = table.split_whitespace().next()?.to_string();

    let add_col_pos = upper.find("ADD COLUMN")?;
    let after_add = &original[add_col_pos + "ADD COLUMN".len()..];
    let column = after_add.split_whitespace().next()?;
    Some((table_name, column.to_string()))
}

fn extract_add_index(upper: &str, original: &str) -> Option<(String, String)> {
    let after_alter = upper.strip_prefix("ALTER TABLE")?;
    let _table_end = after_alter.find("ADD INDEX")?;
    let table = original["ALTER TABLE".len()..].trim();
    let table_name = table.split_whitespace().next()?.to_string();

    let add_idx_pos = upper.find("ADD INDEX")?;
    let after_add = &original[add_idx_pos + "ADD INDEX".len()..];
    let index = after_add.split_whitespace().next()?;
    Some((table_name, index.to_string()))
}

fn extract_index_name(upper: &str, original: &str) -> Option<String> {
    let after_create = upper.strip_prefix("CREATE INDEX")?;
    let _ = after_create;
    let after = original["CREATE INDEX".len()..].trim();
    let index = after.split_whitespace().next()?;
    Some(index.to_string())
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max).collect();
        format!("{}...", truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_table_to_drop() {
        let result = derive_down("CREATE TABLE users (id INT, name VARCHAR(255))");
        assert_eq!(result.down_sql, "DROP TABLE IF EXISTS users;");
        assert!(!result.requires_manual);
    }

    #[test]
    fn test_add_column_to_drop_column() {
        let result = derive_down("ALTER TABLE users ADD COLUMN email VARCHAR(255)");
        assert_eq!(result.down_sql, "ALTER TABLE users DROP COLUMN email;");
        assert!(!result.requires_manual);
    }

    #[test]
    fn test_add_index_to_drop_index() {
        let result = derive_down("ALTER TABLE users ADD INDEX idx_email (email)");
        assert_eq!(result.down_sql, "DROP INDEX idx_email ON users;");
    }

    #[test]
    fn test_create_index_to_drop() {
        let result = derive_down("CREATE INDEX idx_name ON users (name)");
        assert_eq!(result.down_sql, "DROP INDEX IF EXISTS idx_name;");
    }

    #[test]
    fn test_insert_to_delete_with_warning() {
        let result = derive_down("INSERT INTO users (id, name) VALUES (1, 'Alice')");
        assert!(result.requires_manual);
        assert!(result
            .warnings
            .iter()
            .any(|w| w.contains("DELETE FROM users")));
        assert!(result.warnings.iter().any(|w| w.contains("需手动补充")));
    }

    #[test]
    fn test_update_requires_manual() {
        let result = derive_down("UPDATE users SET name = 'Bob' WHERE id = 1");
        assert!(result.requires_manual);
        assert!(result.warnings.iter().any(|w| w.contains("UPDATE")));
    }

    #[test]
    fn test_multiple_statements() {
        let up = "CREATE TABLE users (id INT); CREATE TABLE posts (id INT)";
        let result = derive_down(up);
        assert!(result.down_sql.contains("DROP TABLE IF EXISTS users;"));
        assert!(result.down_sql.contains("DROP TABLE IF EXISTS posts;"));
    }

    #[test]
    fn test_mixed_statements() {
        let up = "CREATE TABLE users (id INT); INSERT INTO users VALUES (1)";
        let result = derive_down(up);
        assert!(result.down_sql.contains("DROP TABLE IF EXISTS users;"));
        assert!(result.requires_manual);
    }

    #[test]
    fn test_empty_sql() {
        let result = derive_down("");
        assert_eq!(result.down_sql, "");
        assert!(!result.requires_manual);
    }

    #[test]
    fn test_unsupported_statement() {
        let result = derive_down("GRANT SELECT ON users TO admin");
        assert!(result.requires_manual);
        assert!(!result.warnings.is_empty());
    }
}
