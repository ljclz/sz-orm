//! SQL Validator - compile-time and runtime SQL validation
//!
//! Provides validation of SQL statements for syntax correctness,
//! parameter count matching, and structural integrity.

use thiserror::Error;

/// SQL validation errors
#[derive(Error, Debug, Clone, PartialEq)]
pub enum SqlValidationError {
    #[error("SQL syntax error: {0}")]
    SyntaxError(String),

    #[error("Unbalanced parentheses: {0}")]
    UnbalancedParentheses(String),

    #[error("Unclosed string literal at position {0}")]
    UnclosedString(usize),

    #[error("Missing required keyword: {0}")]
    MissingKeyword(String),

    #[error("Invalid parameter count: expected {expected}, got {got}")]
    ParameterCountMismatch { expected: usize, got: usize },

    #[error("Invalid table name: {0}")]
    InvalidTableName(String),

    #[error("Empty SELECT columns")]
    EmptySelectColumns,

    #[error("Empty INSERT data")]
    EmptyInsertData,

    #[error("Empty UPDATE data")]
    EmptyUpdateData,

    #[error("DELETE without WHERE clause")]
    DeleteWithoutWhere,

    #[error("Invalid identifier: {0}")]
    InvalidIdentifier(String),

    #[error("SQL injection detected: {0}")]
    InjectionDetected(String),
}

/// Result type for SQL validation
pub type ValidationResult = Result<(), SqlValidationError>;

/// SQL statement type detected by the parser
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SqlStatementType {
    Select,
    Insert,
    Update,
    Delete,
    Create,
    Drop,
    Alter,
    Truncate,
    Other,
}

/// Validate a SQL SELECT statement
pub fn validate_select(sql: &str) -> ValidationResult {
    let sql_upper = sql.to_uppercase();

    if !sql_upper.trim_start().starts_with("SELECT") {
        return Err(SqlValidationError::SyntaxError(
            "SELECT statement must start with SELECT".to_string(),
        ));
    }

    if !sql_upper.contains("FROM") {
        return Err(SqlValidationError::MissingKeyword("FROM".to_string()));
    }

    validate_balanced_parentheses(sql)?;
    validate_string_literals(sql)?;
    validate_no_injection_patterns(sql)?;

    Ok(())
}

/// Validate a SQL INSERT statement
pub fn validate_insert(sql: &str) -> ValidationResult {
    let sql_upper = sql.to_uppercase();

    if !sql_upper.trim_start().starts_with("INSERT") {
        return Err(SqlValidationError::SyntaxError(
            "INSERT statement must start with INSERT".to_string(),
        ));
    }

    if !sql_upper.contains("INTO") {
        return Err(SqlValidationError::MissingKeyword("INTO".to_string()));
    }

    if !sql_upper.contains("VALUES") {
        return Err(SqlValidationError::MissingKeyword("VALUES".to_string()));
    }

    validate_balanced_parentheses(sql)?;
    validate_string_literals(sql)?;
    validate_no_injection_patterns(sql)?;

    Ok(())
}

/// Validate a SQL UPDATE statement
pub fn validate_update(sql: &str) -> ValidationResult {
    let sql_upper = sql.to_uppercase();

    if !sql_upper.trim_start().starts_with("UPDATE") {
        return Err(SqlValidationError::SyntaxError(
            "UPDATE statement must start with UPDATE".to_string(),
        ));
    }

    if !sql_upper.contains("SET") {
        return Err(SqlValidationError::MissingKeyword("SET".to_string()));
    }

    validate_balanced_parentheses(sql)?;
    validate_string_literals(sql)?;
    validate_no_injection_patterns(sql)?;

    Ok(())
}

/// Validate a SQL DELETE statement
pub fn validate_delete(sql: &str) -> ValidationResult {
    let sql_upper = sql.to_uppercase();

    if !sql_upper.trim_start().starts_with("DELETE") {
        return Err(SqlValidationError::SyntaxError(
            "DELETE statement must start with DELETE".to_string(),
        ));
    }

    if !sql_upper.contains("FROM") {
        return Err(SqlValidationError::MissingKeyword("FROM".to_string()));
    }

    validate_balanced_parentheses(sql)?;
    validate_string_literals(sql)?;
    validate_no_injection_patterns(sql)?;

    Ok(())
}

/// Validate any SQL statement by detecting its type and applying appropriate rules
pub fn validate_sql(sql: &str) -> ValidationResult {
    let trimmed = sql.trim();
    if trimmed.is_empty() {
        return Err(SqlValidationError::SyntaxError(
            "Empty SQL statement".to_string(),
        ));
    }

    let sql_type = detect_statement_type(trimmed);
    match sql_type {
        SqlStatementType::Select => validate_select(trimmed),
        SqlStatementType::Insert => validate_insert(trimmed),
        SqlStatementType::Update => validate_update(trimmed),
        SqlStatementType::Delete => validate_delete(trimmed),
        _ => {
            validate_balanced_parentheses(trimmed)?;
            validate_string_literals(trimmed)?;
            validate_no_injection_patterns(trimmed)?;
            Ok(())
        }
    }
}

/// Validate balanced parentheses (including nested levels)
fn validate_balanced_parentheses(sql: &str) -> ValidationResult {
    let mut depth: i32 = 0;
    for (i, ch) in sql.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return Err(SqlValidationError::UnbalancedParentheses(format!(
                        "Unexpected ')' at position {}",
                        i
                    )));
                }
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Err(SqlValidationError::UnbalancedParentheses(format!(
            "{} unclosed '(' parentheses",
            depth
        )));
    }
    Ok(())
}

/// Validate string literals are properly closed
fn validate_string_literals(sql: &str) -> ValidationResult {
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut prev_ch = '\0';

    for (_i, ch) in sql.char_indices() {
        if prev_ch == '\\' {
            prev_ch = ch;
            continue;
        }

        match ch {
            '\'' if !in_double_quote => {
                in_single_quote = !in_single_quote;
            }
            '"' if !in_single_quote => {
                in_double_quote = !in_double_quote;
            }
            _ => {}
        }
        prev_ch = ch;
    }

    if in_single_quote {
        return Err(SqlValidationError::UnclosedString(sql.len()));
    }
    if in_double_quote {
        return Err(SqlValidationError::UnclosedString(sql.len()));
    }

    Ok(())
}

/// Validate no obvious SQL injection patterns
fn validate_no_injection_patterns(sql: &str) -> ValidationResult {
    let sql_upper = sql.to_uppercase();

    // Check for suspicious patterns
    let suspicious_patterns = [
        ("'; DROP TABLE", "DROP TABLE injection"),
        ("' OR '1'='1", "classic OR injection"),
        ("' OR 1=1", "OR 1=1 injection"),
        (
            "UNION SELECT",
            "UNION SELECT injection (not allowed in simple queries)",
        ),
        ("--", "comment injection (not allowed)"),
        ("/*", "block comment (not allowed)"),
    ];

    for (pattern, desc) in &suspicious_patterns {
        if sql_upper.contains(pattern) {
            return Err(SqlValidationError::InjectionDetected(format!(
                "{}: {}",
                desc, pattern
            )));
        }
    }

    Ok(())
}

/// Detect the type of SQL statement
pub fn detect_statement_type(sql: &str) -> SqlStatementType {
    let trimmed = sql.trim().to_uppercase();

    if trimmed.starts_with("SELECT") {
        SqlStatementType::Select
    } else if trimmed.starts_with("INSERT") {
        SqlStatementType::Insert
    } else if trimmed.starts_with("UPDATE") {
        SqlStatementType::Update
    } else if trimmed.starts_with("DELETE") {
        SqlStatementType::Delete
    } else if trimmed.starts_with("CREATE") {
        SqlStatementType::Create
    } else if trimmed.starts_with("DROP") {
        SqlStatementType::Drop
    } else if trimmed.starts_with("ALTER") {
        SqlStatementType::Alter
    } else if trimmed.starts_with("TRUNCATE") {
        SqlStatementType::Truncate
    } else {
        SqlStatementType::Other
    }
}

/// Validate parameter count in prepared statements
pub fn validate_parameter_count(sql: &str, expected_params: usize) -> ValidationResult {
    let param_count = sql.chars().filter(|&c| c == '?').count()
        + sql.matches('$').count(); // PostgreSQL style
    if param_count != expected_params {
        return Err(SqlValidationError::ParameterCountMismatch {
            expected: expected_params,
            got: param_count,
        });
    }
    Ok(())
}

/// Validate table name is a valid SQL identifier
pub fn validate_table_name(name: &str) -> ValidationResult {
    if name.is_empty() {
        return Err(SqlValidationError::InvalidTableName(
            "empty table name".to_string(),
        ));
    }

    // Table names should only contain alphanumeric, underscore
    // Allow backtick-quoted identifiers
    let cleaned = name
        .trim_matches('`')
        .trim_matches('"')
        .trim_matches('\'');
    if cleaned.is_empty() {
        return Err(SqlValidationError::InvalidTableName(name.to_string()));
    }

    for ch in cleaned.chars() {
        if !ch.is_alphanumeric() && ch != '_' {
            return Err(SqlValidationError::InvalidTableName(format!(
                "table name '{}' contains invalid character '{}'",
                name, ch
            )));
        }
    }

    Ok(())
}

/// Validate column name is a valid SQL identifier
pub fn validate_column_name(name: &str) -> ValidationResult {
    if name.is_empty() || name == "*" {
        return Ok(()); // * is valid for SELECT
    }

    let cleaned = name
        .trim_matches('`')
        .trim_matches('"')
        .trim_matches('\'');
    if cleaned.is_empty() {
        return Err(SqlValidationError::InvalidIdentifier(name.to_string()));
    }

    // Allow alphanumeric, underscore, dot (for table.column)
    for ch in cleaned.chars() {
        if !ch.is_alphanumeric() && ch != '_' && ch != '.' {
            return Err(SqlValidationError::InvalidIdentifier(format!(
                "column '{}' contains invalid character '{}'",
                name, ch
            )));
        }
    }

    Ok(())
}

/// Entry-level validation that runs all checks
pub fn validate(sql: &str) -> ValidationResult {
    if sql.trim().is_empty() {
        return Err(SqlValidationError::SyntaxError(
            "Empty SQL statement".to_string(),
        ));
    }

    validate_sql(sql)?;
    validate_balanced_parentheses(sql)?;
    validate_string_literals(sql)?;
    validate_no_injection_patterns(sql)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_select_basic() {
        assert!(validate_select("SELECT * FROM users").is_ok());
        assert!(validate_select("SELECT id, name FROM users WHERE id = 1").is_ok());
        assert!(
            validate_select(
                "SELECT u.id, u.name FROM users u INNER JOIN orders o ON u.id = o.user_id"
            )
            .is_ok()
        );
    }

    #[test]
    fn test_validate_select_missing_from() {
        let result = validate_select("SELECT *");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_insert_basic() {
        assert!(validate_insert("INSERT INTO users (name) VALUES ('alice')").is_ok());
        assert!(
            validate_insert("INSERT INTO users (name, age) VALUES ('bob', 25)").is_ok()
        );
    }

    #[test]
    fn test_validate_insert_missing_values() {
        let result = validate_insert("INSERT INTO users (name)");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_update_basic() {
        assert!(validate_update("UPDATE users SET name = 'alice' WHERE id = 1").is_ok());
    }

    #[test]
    fn test_validate_update_missing_set() {
        let result = validate_update("UPDATE users WHERE id = 1");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_delete_basic() {
        assert!(validate_delete("DELETE FROM users WHERE id = 1").is_ok());
    }

    #[test]
    fn test_validate_delete_missing_from() {
        let result = validate_delete("DELETE users");
        assert!(result.is_err());
    }

    #[test]
    fn test_balanced_parentheses() {
        assert!(validate_balanced_parentheses("SELECT * FROM (SELECT * FROM users) t").is_ok());
        assert!(validate_balanced_parentheses("FUNC(a, b, c)").is_ok());
        assert!(
            validate_balanced_parentheses(
                "SELECT * FROM users WHERE (a=1 AND (b=2 OR c=3))"
            )
            .is_ok()
        );
    }

    #[test]
    fn test_unbalanced_parentheses() {
        assert!(
            validate_balanced_parentheses("SELECT * FROM (SELECT * FROM users").is_err()
        );
        assert!(validate_balanced_parentheses("SELECT * FROM users)").is_err());
    }

    #[test]
    fn test_string_literals_closed() {
        assert!(
            validate_string_literals("SELECT * FROM users WHERE name = 'alice'").is_ok()
        );
        assert!(
            validate_string_literals("INSERT INTO users (name) VALUES ('bob')").is_ok()
        );
    }

    #[test]
    fn test_unclosed_string_literal() {
        assert!(
            validate_string_literals("SELECT * FROM users WHERE name = 'alice").is_err()
        );
    }

    #[test]
    fn test_injection_detection() {
        assert!(
            validate_no_injection_patterns("SELECT * FROM users WHERE name = 'alice'").is_ok()
        );
        assert!(
            validate_no_injection_patterns(
                "SELECT * FROM users WHERE name = 'alice' OR '1'='1'"
            )
            .is_err()
        );
        assert!(
            validate_no_injection_patterns("'; DROP TABLE users; --").is_err()
        );
        assert!(
            validate_no_injection_patterns("1 UNION SELECT * FROM users").is_err()
        );
    }

    #[test]
    fn test_detect_statement_type() {
        assert_eq!(
            detect_statement_type("SELECT * FROM users"),
            SqlStatementType::Select
        );
        assert_eq!(
            detect_statement_type("INSERT INTO users VALUES (1)"),
            SqlStatementType::Insert
        );
        assert_eq!(
            detect_statement_type("UPDATE users SET a=1"),
            SqlStatementType::Update
        );
        assert_eq!(
            detect_statement_type("DELETE FROM users"),
            SqlStatementType::Delete
        );
        assert_eq!(
            detect_statement_type("CREATE TABLE users"),
            SqlStatementType::Create
        );
        assert_eq!(
            detect_statement_type("DROP TABLE users"),
            SqlStatementType::Drop
        );
        assert_eq!(
            detect_statement_type("ALTER TABLE users ADD COLUMN a"),
            SqlStatementType::Alter
        );
        assert_eq!(
            detect_statement_type("TRUNCATE TABLE users"),
            SqlStatementType::Truncate
        );
        assert_eq!(
            detect_statement_type("EXPLAIN SELECT * FROM users"),
            SqlStatementType::Other
        );
    }

    #[test]
    fn test_parameter_count() {
        assert!(
            validate_parameter_count(
                "SELECT * FROM users WHERE id = ? AND name = ?",
                2
            )
            .is_ok()
        );
        assert!(
            validate_parameter_count("SELECT * FROM users WHERE id = ?", 1).is_ok()
        );
        assert!(
            validate_parameter_count("SELECT * FROM users WHERE id = ?", 2).is_err()
        );
    }

    #[test]
    fn test_validate_table_name() {
        assert!(validate_table_name("users").is_ok());
        assert!(validate_table_name("user_orders").is_ok());
        assert!(validate_table_name("").is_err());
        assert!(validate_table_name("users; DROP TABLE").is_err());
    }

    #[test]
    fn test_validate_column_name() {
        assert!(validate_column_name("id").is_ok());
        assert!(validate_column_name("*").is_ok());
        assert!(validate_column_name("users.name").is_ok());
        assert!(validate_column_name("").is_ok()); // * replacement
    }

    #[test]
    fn test_validate_empty_sql() {
        assert!(validate("").is_err());
        assert!(validate("   ").is_err());
    }

    #[test]
    fn test_validate_complex_queries() {
        assert!(validate("SELECT u.*, o.total FROM users u LEFT JOIN orders o ON u.id = o.user_id WHERE u.status = 'active' AND u.created_at > '2024-01-01' GROUP BY u.id HAVING COUNT(o.id) > 5 ORDER BY u.name ASC LIMIT 10 OFFSET 20").is_ok());
    }

    #[test]
    fn test_empty_insert_data() {
        let sql = "INSERT INTO users () VALUES ()";
        assert!(validate_sql(sql).is_ok());
    }

    #[test]
    fn test_create_table_validation() {
        assert!(
            validate_sql("CREATE TABLE users (id INT PRIMARY KEY, name VARCHAR(100))").is_ok()
        );
    }

    #[test]
    fn test_double_quoted_identifiers() {
        assert!(
            validate_string_literals(
                "SELECT * FROM \"users\" WHERE \"name\" = 'alice'"
            )
            .is_ok()
        );
    }

    #[test]
    fn test_nested_function_calls() {
        assert!(
            validate_balanced_parentheses(
                "SELECT MAX(COUNT(*)) FROM (SELECT COUNT(*) FROM users GROUP BY status) t"
            )
            .is_ok()
        );
    }
}
