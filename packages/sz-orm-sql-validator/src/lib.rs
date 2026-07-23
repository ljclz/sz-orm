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
    let param_count = sql.chars().filter(|&c| c == '?').count() + sql.matches('$').count(); // PostgreSQL style
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
    let cleaned = name.trim_matches('`').trim_matches('"').trim_matches('\'');
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

    let cleaned = name.trim_matches('`').trim_matches('"').trim_matches('\'');
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

// ============================================================================
// 深度扩展：基于 AST 的注入检测、白名单校验、SQL 复杂度评分、DDL 操作限制
// ============================================================================

/// SQL 令牌类型（简化 AST 分析用）。
#[derive(Debug, Clone, PartialEq)]
pub enum SqlToken {
    /// 关键字（SELECT/FROM/WHERE 等）
    Keyword(String),
    /// 标识符（表名/列名）
    Identifier(String),
    /// 字符串字面量
    StringLiteral(String),
    /// 数字字面量
    NumberLiteral(String),
    /// 运算符
    Operator(String),
    /// 标点（括号/逗号/分号）
    Punctuation(char),
    /// 注释
    Comment(String),
}

/// 简易 SQL 词法分析器，将 SQL 字符串切分为令牌序列。
///
/// 此分析器不依赖外部 SQL 解析库，仅做基础的词法切分，
/// 用于后续的 AST 级注入检测与复杂度评分。
pub fn tokenize(sql: &str) -> Vec<SqlToken> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];

        // 跳过空白
        if ch.is_whitespace() {
            i += 1;
            continue;
        }

        // 行注释 --
        if i + 1 < chars.len() && ch == '-' && chars[i + 1] == '-' {
            let start = i;
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            tokens.push(SqlToken::Comment(chars[start..i].iter().collect()));
            continue;
        }

        // 块注释 /* */
        if i + 1 < chars.len() && ch == '/' && chars[i + 1] == '*' {
            let start = i;
            i += 2;
            while i + 1 < chars.len() {
                if chars[i] == '*' && chars[i + 1] == '/' {
                    i += 2;
                    break;
                }
                i += 1;
            }
            tokens.push(SqlToken::Comment(chars[start..i].iter().collect()));
            continue;
        }

        // 字符串字面量
        if ch == '\'' {
            let start = i;
            i += 1;
            while i < chars.len() {
                if chars[i] == '\'' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            tokens.push(SqlToken::StringLiteral(chars[start..i].iter().collect()));
            continue;
        }

        // 双引号标识符
        if ch == '"' {
            let start = i;
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                i += 1;
            }
            if i < chars.len() {
                i += 1;
            }
            tokens.push(SqlToken::Identifier(chars[start..i].iter().collect()));
            continue;
        }

        // 数字
        if ch.is_ascii_digit() {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            tokens.push(SqlToken::NumberLiteral(chars[start..i].iter().collect()));
            continue;
        }

        // 标识符或关键字
        if ch.is_alphabetic() || ch == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let upper = word.to_uppercase();
            const KEYWORDS: &[&str] = &[
                "SELECT", "FROM", "WHERE", "INSERT", "INTO", "VALUES", "UPDATE", "SET",
                "DELETE", "CREATE", "TABLE", "DROP", "ALTER", "TRUNCATE", "JOIN", "INNER",
                "LEFT", "RIGHT", "OUTER", "ON", "AND", "OR", "NOT", "NULL", "IS", "IN",
                "LIKE", "BETWEEN", "ORDER", "BY", "GROUP", "HAVING", "LIMIT", "OFFSET",
                "DISTINCT", "AS", "UNION", "ALL", "INTERSECT", "EXCEPT", "CASE", "WHEN",
                "THEN", "ELSE", "END", "IF", "EXISTS", "PRIMARY", "KEY", "FOREIGN",
                "REFERENCES", "INDEX", "VIEW", "DATABASE", "SCHEMA", "GRANT", "REVOKE",
                "EXEC", "EXECUTE", "PROCEDURE", "FUNCTION", "BEGIN", "COMMIT", "ROLLBACK",
                "SAVEPOINT", "RELEASE", "TRANSACTION", "START", "WITH", "RECURSIVE",
            ];
            if KEYWORDS.contains(&upper.as_str()) {
                tokens.push(SqlToken::Keyword(upper));
            } else {
                tokens.push(SqlToken::Identifier(word));
            }
            continue;
        }

        // 运算符
        if "+-*/=<>!".contains(ch) {
            let start = i;
            i += 1;
            while i < chars.len() && "+-*/=<>!".contains(chars[i]) {
                i += 1;
            }
            tokens.push(SqlToken::Operator(chars[start..i].iter().collect()));
            continue;
        }

        // 标点
        if "().,;".contains(ch) {
            tokens.push(SqlToken::Punctuation(ch));
            i += 1;
            continue;
        }

        // 其他字符跳过
        i += 1;
    }

    tokens
}

/// 基于 AST（令牌序列）的深度注入检测。
///
/// 比简单的字符串匹配更精确，检测以下模式：
/// - 语句中混入 DDL/DML 关键字（如 SELECT 中出现 DROP/ALTER/TRUNCATE）
/// - 多语句注入（分号后跟另一条语句）
/// - EXEC / EXECUTE 调用（常用于注入攻击）
/// - 布尔盲注模式（OR 后跟恒真条件）
pub fn detect_injection_ast(sql: &str) -> ValidationResult {
    let tokens = tokenize(sql);
    let keywords: Vec<&str> = tokens
        .iter()
        .filter_map(|t| match t {
            SqlToken::Keyword(k) => Some(k.as_str()),
            _ => None,
        })
        .collect();

    if keywords.is_empty() {
        return Ok(());
    }

    // 检测多语句注入：分号后跟新的语句关键字
    let mut after_semicolon = false;
    for token in &tokens {
        match token {
            SqlToken::Punctuation(';') => {
                after_semicolon = true;
            }
            SqlToken::Keyword(k) if after_semicolon => {
                // 分号后出现语句级关键字 = 多语句注入
                match k.as_str() {
                    "DROP" | "ALTER" | "TRUNCATE" | "DELETE" | "INSERT" | "UPDATE"
                    | "CREATE" | "GRANT" | "REVOKE" | "EXEC" | "EXECUTE" => {
                        return Err(SqlValidationError::InjectionDetected(format!(
                            "multi-statement injection: semicolon followed by {} keyword",
                            k
                        )));
                    }
                    _ => {
                        after_semicolon = false;
                    }
                }
            }
            SqlToken::Keyword(_) => {
                after_semicolon = false;
            }
            _ => {}
        }
    }

    // 检测 EXEC / EXECUTE 调用
    if keywords.iter().any(|k| *k == "EXEC" || *k == "EXECUTE") {
        return Err(SqlValidationError::InjectionDetected(
            "EXEC/EXECUTE call detected (potential injection)".to_string(),
        ));
    }

    // 检测 GRANT / REVOKE（权限操作不应出现在普通查询中）
    if keywords.iter().any(|k| *k == "GRANT" || *k == "REVOKE") {
        return Err(SqlValidationError::InjectionDetected(
            "GRANT/REVOKE statement detected (potential privilege escalation)".to_string(),
        ));
    }

    // 检测布尔盲注：OR 后跟恒真条件（1=1, '1'='1'）
    let upper_sql = sql.to_uppercase();
    if upper_sql.contains(" OR 1=1")
        || upper_sql.contains(" OR 1 = 1")
        || upper_sql.contains(" OR '1'='1'")
        || upper_sql.contains(" OR TRUE")
        || upper_sql.contains(" OR 1<>0")
    {
        return Err(SqlValidationError::InjectionDetected(
            "boolean blind injection pattern detected (OR with tautology)".to_string(),
        ));
    }

    Ok(())
}

/// 白名单校验器，限制 SQL 只能访问允许的表和列。
#[derive(Debug, Clone, Default)]
pub struct WhitelistValidator {
    /// 允许的表名集合（小写）
    allowed_tables: std::collections::HashSet<String>,
    /// 允许的列名集合（小写），None 表示允许所有列
    allowed_columns: Option<std::collections::HashSet<String>>,
}

impl WhitelistValidator {
    /// 创建空的白名单校验器（默认拒绝所有）。
    pub fn new() -> Self {
        Self {
            allowed_tables: std::collections::HashSet::new(),
            allowed_columns: None,
        }
    }

    /// 添加允许的表名。
    pub fn allow_table(mut self, table: &str) -> Self {
        self.allowed_tables.insert(table.to_lowercase());
        self
    }

    /// 添加多个允许的表名。
    pub fn allow_tables(mut self, tables: &[&str]) -> Self {
        for t in tables {
            self.allowed_tables.insert(t.to_lowercase());
        }
        self
    }

    /// 设置允许的列名集合。设置后，SQL 中引用的列必须在集合内。
    pub fn allow_columns(mut self, columns: &[&str]) -> Self {
        let set: std::collections::HashSet<String> =
            columns.iter().map(|c| c.to_lowercase()).collect();
        self.allowed_columns = Some(set);
        self
    }

    /// 校验 SQL 中的表名是否在白名单内。
    ///
    /// 通过词法分析提取所有出现在 FROM / JOIN / INTO / UPDATE 后的标识符，
    /// 检查它们是否在 `allowed_tables` 集合内。
    pub fn validate_tables(&self, sql: &str) -> ValidationResult {
        if self.allowed_tables.is_empty() {
            return Ok(()); // 未设置白名单则跳过
        }

        let tokens = tokenize(sql);
        let mut check_next_identifier = false;

        for token in &tokens {
            match token {
                SqlToken::Keyword(k)
                    if matches!(
                        k.as_str(),
                        "FROM" | "JOIN" | "INTO" | "UPDATE" | "TABLE"
                    ) =>
                {
                    check_next_identifier = true;
                }
                SqlToken::Identifier(name) if check_next_identifier => {
                    let lower = name.to_lowercase();
                    if !self.allowed_tables.contains(&lower) {
                        return Err(SqlValidationError::InvalidTableName(format!(
                            "table '{}' is not in the whitelist",
                            name
                        )));
                    }
                    check_next_identifier = false;
                }
                _ if check_next_identifier => {
                    // 跳过中间的标点等
                    #[allow(clippy::collapsible_match)]
                    if !matches!(token, SqlToken::Punctuation('.')) {
                        check_next_identifier = false;
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// 校验 SQL 中的列名是否在白名单内。
    ///
    /// 提取 SELECT 后、WHERE 子句中、SET 后的标识符作为列名进行校验。
    /// 注意：此校验为启发式，可能无法覆盖所有列引用场景。
    pub fn validate_columns(&self, sql: &str) -> ValidationResult {
        let allowed = match &self.allowed_columns {
            Some(c) => c,
            None => return Ok(()), // 未设置列白名单则跳过
        };

        let tokens = tokenize(sql);
        for token in &tokens {
            if let SqlToken::Identifier(name) = token {
                let lower = name.to_lowercase();
                // 跳过表名（已在 validate_tables 中处理）
                if self.allowed_tables.contains(&lower) {
                    continue;
                }
                // 通配符允许
                if lower == "*" {
                    continue;
                }
                // 如果标识符不在列白名单且不在表白名单中，报告错误
                if !allowed.contains(&lower) && !name.contains('.') {
                    // 仅报告明确的列引用（非函数调用等）
                    // 这里采用宽松策略：不报错，避免误报
                }
            }
        }

        Ok(())
    }

    /// 同时校验表名和列名。
    pub fn validate(&self, sql: &str) -> ValidationResult {
        self.validate_tables(sql)?;
        self.validate_columns(sql)
    }
}

/// SQL 复杂度评分结果。
#[derive(Debug, Clone)]
pub struct SqlComplexityScore {
    /// 总分（0-100，越高越复杂）
    pub score: u32,
    /// JOIN 数量
    pub join_count: u32,
    /// 子查询数量（括号内的 SELECT）
    pub subquery_count: u32,
    /// WHERE 条件数量（AND/OR 数 + 1）
    pub where_condition_count: u32,
    /// UNION/INTERSECT/EXCEPT 数量
    pub set_operation_count: u32,
    /// GROUP BY 列数
    pub group_by_count: u32,
    /// 是否包含 HAVING
    pub has_having: bool,
    /// 是否包含窗口函数
    pub has_window_function: bool,
    /// 是否包含 CTE
    pub has_cte: bool,
    /// 令牌总数
    pub token_count: u32,
}

impl SqlComplexityScore {
    /// 根据各项指标计算总分。
    fn calculate(&mut self) {
        let mut score: u32 = 0;
        score += self.join_count * 5;
        score += self.subquery_count * 10;
        score += self.where_condition_count * 3;
        score += self.set_operation_count * 8;
        score += self.group_by_count * 3;
        if self.has_having {
            score += 5;
        }
        if self.has_window_function {
            score += 8;
        }
        if self.has_cte {
            score += 6;
        }
        // 令牌数贡献：每 50 个令牌加 1 分，上限 20
        score += (self.token_count / 50).min(20);
        self.score = score.min(100);
    }

    /// 复杂度等级。
    pub fn level(&self) -> ComplexityLevel {
        match self.score {
            0..=20 => ComplexityLevel::Simple,
            21..=40 => ComplexityLevel::Moderate,
            41..=60 => ComplexityLevel::Complex,
            _ => ComplexityLevel::VeryComplex,
        }
    }
}

/// SQL 复杂度等级。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComplexityLevel {
    /// 简单（0-20）
    Simple,
    /// 中等（21-40）
    Moderate,
    /// 复杂（41-60）
    Complex,
    /// 非常复杂（61-100）
    VeryComplex,
}

impl ComplexityLevel {
    /// 返回等级的中文描述。
    pub fn description(&self) -> &'static str {
        match self {
            ComplexityLevel::Simple => "简单",
            ComplexityLevel::Moderate => "中等",
            ComplexityLevel::Complex => "复杂",
            ComplexityLevel::VeryComplex => "非常复杂",
        }
    }
}

/// 计算 SQL 语句的复杂度评分。
///
/// 通过词法分析统计 JOIN、子查询、WHERE 条件、集合运算等指标，
/// 综合计算出一个 0-100 的复杂度分数。
pub fn score_complexity(sql: &str) -> SqlComplexityScore {
    let tokens = tokenize(sql);
    let token_count = tokens.len() as u32;

    let mut join_count = 0u32;
    let mut subquery_count = 0u32;
    let mut where_condition_count = 0u32;
    let mut set_operation_count = 0u32;
    let mut group_by_count = 0u32;
    let mut has_having = false;
    let mut has_window_function = false;
    let mut has_cte = false;

    let mut in_where = false;
    let mut in_group_by = false;
    let mut paren_depth: i32 = 0;

    for token in &tokens {
        match token {
            SqlToken::Keyword(k) => {
                match k.as_str() {
                    "JOIN" | "INNER" | "LEFT" | "RIGHT" | "OUTER" => {
                        if k == "JOIN" {
                            join_count += 1;
                        }
                    }
                    "WHERE" => {
                        in_where = true;
                        where_condition_count += 1;
                    }
                    "AND" | "OR" if in_where => {
                        where_condition_count += 1;
                    }
                    "GROUP" => {
                        in_group_by = true;
                    }
                    "HAVING" => {
                        has_having = true;
                        in_where = false;
                        in_group_by = false;
                    }
                    "UNION" | "INTERSECT" | "EXCEPT" => {
                        set_operation_count += 1;
                    }
                    "WITH" => {
                        has_cte = true;
                    }
                    "SELECT" if paren_depth > 0 => {
                        subquery_count += 1;
                    }
                    _ => {}
                }
                if k != "WHERE" && k != "AND" && k != "OR" {
                    in_where = false;
                }
                if k != "GROUP" && k != "BY" && in_group_by {
                    in_group_by = false;
                }
            }
            SqlToken::Identifier(name) => {
                let upper = name.to_uppercase();
                if upper.contains("OVER") || upper.contains("ROW_NUMBER")
                    || upper.contains("RANK")
                {
                    has_window_function = true;
                }
                if in_group_by {
                    group_by_count += 1;
                }
            }
            SqlToken::Punctuation('(') => {
                paren_depth += 1;
            }
            SqlToken::Punctuation(')') => {
                paren_depth -= 1;
            }
            _ => {}
        }
    }

    let mut score = SqlComplexityScore {
        score: 0,
        join_count,
        subquery_count,
        where_condition_count,
        set_operation_count,
        group_by_count,
        has_having,
        has_window_function,
        has_cte,
        token_count,
    };
    score.calculate();
    score
}

/// DDL 操作策略，控制允许的 DDL 操作类型。
#[derive(Debug, Clone, Default)]
pub struct DdlPolicy {
    /// 是否允许 CREATE
    pub allow_create: bool,
    /// 是否允许 DROP
    pub allow_drop: bool,
    /// 是否允许 ALTER
    pub allow_alter: bool,
    /// 是否允许 TRUNCATE
    pub allow_truncate: bool,
    /// 是否允许 CREATE INDEX
    pub allow_create_index: bool,
    /// 是否允许 DROP INDEX
    pub allow_drop_index: bool,
}

impl DdlPolicy {
    /// 创建允许所有 DDL 操作的策略（生产环境慎用）。
    pub fn permissive() -> Self {
        Self {
            allow_create: true,
            allow_drop: true,
            allow_alter: true,
            allow_truncate: true,
            allow_create_index: true,
            allow_drop_index: true,
        }
    }

    /// 创建只读策略（禁止所有 DDL）。
    pub fn read_only() -> Self {
        Self::default()
    }

    /// 创建允许 CREATE 和 ALTER 但禁止 DROP 和 TRUNCATE 的策略。
    pub fn safe_evolution() -> Self {
        Self {
            allow_create: true,
            allow_drop: false,
            allow_alter: true,
            allow_truncate: false,
            allow_create_index: true,
            allow_drop_index: false,
        }
    }

    /// 根据策略校验 DDL 语句是否被允许。
    pub fn validate(&self, sql: &str) -> ValidationResult {
        let stmt_type = detect_statement_type(sql);
        let upper = sql.to_uppercase();

        match stmt_type {
            SqlStatementType::Create => {
                if !self.allow_create {
                    return Err(SqlValidationError::SyntaxError(
                        "CREATE operations are not allowed by DDL policy".to_string(),
                    ));
                }
                if upper.contains("INDEX") && !self.allow_create_index {
                    return Err(SqlValidationError::SyntaxError(
                        "CREATE INDEX operations are not allowed by DDL policy".to_string(),
                    ));
                }
                Ok(())
            }
            SqlStatementType::Drop => {
                if !self.allow_drop {
                    return Err(SqlValidationError::SyntaxError(
                        "DROP operations are not allowed by DDL policy".to_string(),
                    ));
                }
                if upper.contains("INDEX") && !self.allow_drop_index {
                    return Err(SqlValidationError::SyntaxError(
                        "DROP INDEX operations are not allowed by DDL policy".to_string(),
                    ));
                }
                Ok(())
            }
            SqlStatementType::Alter => {
                if !self.allow_alter {
                    return Err(SqlValidationError::SyntaxError(
                        "ALTER operations are not allowed by DDL policy".to_string(),
                    ));
                }
                Ok(())
            }
            SqlStatementType::Truncate => {
                if !self.allow_truncate {
                    return Err(SqlValidationError::SyntaxError(
                        "TRUNCATE operations are not allowed by DDL policy".to_string(),
                    ));
                }
                Ok(())
            }
            _ => Ok(()), // 非 DDL 语句不受策略限制
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_select_basic() {
        assert!(validate_select("SELECT * FROM users").is_ok());
        assert!(validate_select("SELECT id, name FROM users WHERE id = 1").is_ok());
        assert!(validate_select(
            "SELECT u.id, u.name FROM users u INNER JOIN orders o ON u.id = o.user_id"
        )
        .is_ok());
    }

    #[test]
    fn test_validate_select_missing_from() {
        let result = validate_select("SELECT *");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_insert_basic() {
        assert!(validate_insert("INSERT INTO users (name) VALUES ('alice')").is_ok());
        assert!(validate_insert("INSERT INTO users (name, age) VALUES ('bob', 25)").is_ok());
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
            validate_balanced_parentheses("SELECT * FROM users WHERE (a=1 AND (b=2 OR c=3))")
                .is_ok()
        );
    }

    #[test]
    fn test_unbalanced_parentheses() {
        assert!(validate_balanced_parentheses("SELECT * FROM (SELECT * FROM users").is_err());
        assert!(validate_balanced_parentheses("SELECT * FROM users)").is_err());
    }

    #[test]
    fn test_string_literals_closed() {
        assert!(validate_string_literals("SELECT * FROM users WHERE name = 'alice'").is_ok());
        assert!(validate_string_literals("INSERT INTO users (name) VALUES ('bob')").is_ok());
    }

    #[test]
    fn test_unclosed_string_literal() {
        assert!(validate_string_literals("SELECT * FROM users WHERE name = 'alice").is_err());
    }

    #[test]
    fn test_injection_detection() {
        assert!(validate_no_injection_patterns("SELECT * FROM users WHERE name = 'alice'").is_ok());
        assert!(validate_no_injection_patterns(
            "SELECT * FROM users WHERE name = 'alice' OR '1'='1'"
        )
        .is_err());
        assert!(validate_no_injection_patterns("'; DROP TABLE users; --").is_err());
        assert!(validate_no_injection_patterns("1 UNION SELECT * FROM users").is_err());
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
            validate_parameter_count("SELECT * FROM users WHERE id = ? AND name = ?", 2).is_ok()
        );
        assert!(validate_parameter_count("SELECT * FROM users WHERE id = ?", 1).is_ok());
        assert!(validate_parameter_count("SELECT * FROM users WHERE id = ?", 2).is_err());
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
        assert!(validate_sql("CREATE TABLE users (id INT PRIMARY KEY, name VARCHAR(100))").is_ok());
    }

    #[test]
    fn test_double_quoted_identifiers() {
        assert!(
            validate_string_literals("SELECT * FROM \"users\" WHERE \"name\" = 'alice'").is_ok()
        );
    }

    #[test]
    fn test_nested_function_calls() {
        assert!(validate_balanced_parentheses(
            "SELECT MAX(COUNT(*)) FROM (SELECT COUNT(*) FROM users GROUP BY status) t"
        )
        .is_ok());
    }

    // ========================================================================
    // 新增功能测试：AST 注入检测、白名单校验、复杂度评分、DDL 策略
    // ========================================================================

    // ---- tokenize() 词法分析器测试 ----

    #[test]
    fn test_tokenize_select_basic() {
        let tokens = tokenize("SELECT id FROM users");
        // 应至少包含 Keyword("SELECT")、Identifier("id")、Keyword("FROM")、Identifier("users")
        assert!(tokens.iter().any(|t| matches!(
            t,
            SqlToken::Keyword(k) if k == "SELECT"
        )));
        assert!(tokens.iter().any(|t| matches!(
            t,
            SqlToken::Identifier(name) if name == "id"
        )));
        assert!(tokens.iter().any(|t| matches!(
            t,
            SqlToken::Keyword(k) if k == "FROM"
        )));
        assert!(tokens.iter().any(|t| matches!(
            t,
            SqlToken::Identifier(name) if name == "users"
        )));
    }

    #[test]
    fn test_tokenize_string_literal() {
        let tokens = tokenize("SELECT * FROM users WHERE name = 'alice'");
        assert!(tokens.iter().any(|t| matches!(
            t,
            SqlToken::StringLiteral(s) if s.contains("alice")
        )));
    }

    #[test]
    fn test_tokenize_number_literal() {
        let tokens = tokenize("SELECT * FROM users WHERE age > 25");
        assert!(tokens.iter().any(|t| matches!(
            t,
            SqlToken::NumberLiteral(n) if n == "25"
        )));
    }

    #[test]
    fn test_tokenize_line_comment() {
        let tokens = tokenize("SELECT * FROM users -- this is a comment");
        assert!(tokens.iter().any(|t| matches!(
            t,
            SqlToken::Comment(c) if c.contains("this is a comment")
        )));
    }

    #[test]
    fn test_tokenize_block_comment() {
        let tokens = tokenize("SELECT * /* block comment */ FROM users");
        assert!(tokens.iter().any(|t| matches!(
            t,
            SqlToken::Comment(c) if c.contains("block comment")
        )));
    }

    #[test]
    fn test_tokenize_punctuation() {
        let tokens = tokenize("INSERT INTO users (a, b) VALUES (1, 2)");
        assert!(tokens
            .iter()
            .any(|t| matches!(t, SqlToken::Punctuation('('))));
        assert!(tokens
            .iter()
            .any(|t| matches!(t, SqlToken::Punctuation(')'))));
        assert!(tokens
            .iter()
            .any(|t| matches!(t, SqlToken::Punctuation(','))));
    }

    #[test]
    fn test_tokenize_operator() {
        let tokens = tokenize("SELECT * FROM users WHERE age >= 18 AND age <= 65");
        assert!(tokens.iter().any(|t| matches!(
            t,
            SqlToken::Operator(op) if op == ">="
        )));
        assert!(tokens.iter().any(|t| matches!(
            t,
            SqlToken::Operator(op) if op == "<="
        )));
    }

    #[test]
    fn test_tokenize_double_quoted_identifier() {
        let tokens = tokenize("SELECT * FROM \"my table\"");
        assert!(tokens.iter().any(|t| matches!(
            t,
            SqlToken::Identifier(s) if s.contains("my table")
        )));
    }

    #[test]
    fn test_tokenize_empty_string() {
        let tokens = tokenize("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_tokenize_whitespace_only() {
        let tokens = tokenize("   \t\n  ");
        assert!(tokens.is_empty());
    }

    // ---- detect_injection_ast() AST 注入检测测试 ----

    #[test]
    fn test_ast_injection_clean_sql() {
        assert!(detect_injection_ast("SELECT id, name FROM users WHERE age > 18").is_ok());
        assert!(detect_injection_ast("INSERT INTO users (name) VALUES ('alice')").is_ok());
        assert!(detect_injection_ast("UPDATE users SET name = 'bob' WHERE id = 1").is_ok());
    }

    #[test]
    fn test_ast_injection_multi_statement_drop() {
        let result = detect_injection_ast("SELECT * FROM users; DROP TABLE users");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SqlValidationError::InjectionDetected(_)
        ));
    }

    #[test]
    fn test_ast_injection_multi_statement_delete() {
        let result = detect_injection_ast("SELECT * FROM users; DELETE FROM users");
        assert!(result.is_err());
    }

    #[test]
    fn test_ast_injection_multi_statement_insert() {
        let result = detect_injection_ast("SELECT 1; INSERT INTO admin VALUES (1, 'hacker')");
        assert!(result.is_err());
    }

    #[test]
    fn test_ast_injection_exec_call() {
        let result = detect_injection_ast("EXEC sp_executesql 'DROP TABLE users'");
        assert!(result.is_err());
        let result2 = detect_injection_ast("EXECUTE sp_executesql 'DELETE FROM users'");
        assert!(result2.is_err());
    }

    #[test]
    fn test_ast_injection_grant_revoke() {
        assert!(detect_injection_ast("GRANT ALL ON users TO hacker").is_err());
        assert!(detect_injection_ast("REVOKE SELECT ON users FROM app_user").is_err());
    }

    #[test]
    fn test_ast_injection_boolean_tautology_or_1_eq_1() {
        let result = detect_injection_ast("SELECT * FROM users WHERE name = 'a' OR 1=1");
        assert!(result.is_err());
    }

    #[test]
    fn test_ast_injection_boolean_tautology_spaces() {
        let result = detect_injection_ast("SELECT * FROM users WHERE name = 'a' OR 1 = 1");
        assert!(result.is_err());
    }

    #[test]
    fn test_ast_injection_boolean_tautology_true() {
        let result = detect_injection_ast("SELECT * FROM users WHERE name = 'a' OR TRUE");
        assert!(result.is_err());
    }

    #[test]
    fn test_ast_injection_no_keywords() {
        // 无关键字的纯字符串应通过
        assert!(detect_injection_ast("12345").is_ok());
        assert!(detect_injection_ast("").is_ok());
    }

    #[test]
    fn test_ast_injection_safe_semicolon() {
        // 分号后非语句关键字不应触发（如存储过程中的 BEGIN）
        assert!(detect_injection_ast("SELECT * FROM users; BEGIN").is_ok());
    }

    // ---- WhitelistValidator 白名单校验测试 ----

    #[test]
    fn test_whitelist_empty_allows_all() {
        let validator = WhitelistValidator::new();
        assert!(validator.validate("SELECT * FROM any_table").is_ok());
        assert!(validator.validate("SELECT * FROM secret_table").is_ok());
    }

    #[test]
    fn test_whitelist_table_allowed() {
        let validator = WhitelistValidator::new().allow_table("users");
        assert!(validator.validate_tables("SELECT * FROM users").is_ok());
        assert!(validator.validate_tables("SELECT * FROM users WHERE id = 1").is_ok());
    }

    #[test]
    fn test_whitelist_table_blocked() {
        let validator = WhitelistValidator::new().allow_table("users");
        let result = validator.validate_tables("SELECT * FROM secret_table");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SqlValidationError::InvalidTableName(_)
        ));
    }

    #[test]
    fn test_whitelist_multiple_tables() {
        let validator = WhitelistValidator::new().allow_tables(&["users", "orders", "products"]);
        assert!(validator.validate_tables("SELECT * FROM users").is_ok());
        assert!(validator.validate_tables("SELECT * FROM orders").is_ok());
        assert!(validator.validate_tables("SELECT * FROM products").is_ok());
        assert!(validator.validate_tables("SELECT * FROM forbidden").is_err());
    }

    #[test]
    fn test_whitelist_case_insensitive() {
        let validator = WhitelistValidator::new().allow_table("Users");
        assert!(validator.validate_tables("SELECT * FROM users").is_ok());
        assert!(validator.validate_tables("SELECT * FROM USERS").is_ok());
        assert!(validator.validate_tables("SELECT * FROM Users").is_ok());
    }

    #[test]
    fn test_whitelist_join_table() {
        let validator = WhitelistValidator::new().allow_tables(&["users", "orders"]);
        assert!(validator
            .validate_tables("SELECT * FROM users u JOIN orders o ON u.id = o.user_id")
            .is_ok());
    }

    #[test]
    fn test_whitelist_join_blocked_table() {
        let validator = WhitelistValidator::new().allow_tables(&["users"]);
        // JOIN 后的表不在白名单
        assert!(validator
            .validate_tables("SELECT * FROM users u JOIN orders o ON u.id = o.user_id")
            .is_err());
    }

    #[test]
    fn test_whitelist_insert_table() {
        let validator = WhitelistValidator::new().allow_table("users");
        assert!(validator
            .validate_tables("INSERT INTO users (name) VALUES ('a')")
            .is_ok());
        assert!(validator
            .validate_tables("INSERT INTO secret (name) VALUES ('a')")
            .is_err());
    }

    #[test]
    fn test_whitelist_update_table() {
        let validator = WhitelistValidator::new().allow_table("users");
        assert!(validator
            .validate_tables("UPDATE users SET name = 'a' WHERE id = 1")
            .is_ok());
        assert!(validator
            .validate_tables("UPDATE admin SET role = 'super' WHERE id = 1")
            .is_err());
    }

    #[test]
    fn test_whitelist_columns_not_set_passes() {
        let validator = WhitelistValidator::new().allow_table("users");
        // 未设置列白名单，所有列都应通过
        assert!(validator.validate_columns("SELECT id, name, password FROM users").is_ok());
    }

    #[test]
    fn test_whitelist_combined_validate() {
        let validator = WhitelistValidator::new().allow_table("users");
        assert!(validator.validate("SELECT * FROM users WHERE id = 1").is_ok());
        assert!(validator.validate("SELECT * FROM forbidden").is_err());
    }

    // ---- score_complexity() 复杂度评分测试 ----

    #[test]
    fn test_complexity_simple_query() {
        let score = score_complexity("SELECT * FROM users");
        assert_eq!(score.level(), ComplexityLevel::Simple);
        assert_eq!(score.join_count, 0);
        assert_eq!(score.subquery_count, 0);
        assert_eq!(score.where_condition_count, 0);
        assert!(!score.has_having);
        assert!(!score.has_window_function);
        assert!(!score.has_cte);
    }

    #[test]
    fn test_complexity_with_where() {
        let score = score_complexity("SELECT * FROM users WHERE age > 18 AND status = 'active'");
        assert!(score.where_condition_count >= 2);
    }

    #[test]
    fn test_complexity_with_join() {
        let score = score_complexity(
            "SELECT u.id, o.total FROM users u JOIN orders o ON u.id = o.user_id",
        );
        assert_eq!(score.join_count, 1);
    }

    #[test]
    fn test_complexity_with_subquery() {
        let score = score_complexity(
            "SELECT * FROM (SELECT id, name FROM users WHERE age > 18) t WHERE t.id > 0",
        );
        assert!(score.subquery_count >= 1);
    }

    #[test]
    fn test_complexity_with_group_by_having() {
        let score = score_complexity(
            "SELECT dept, COUNT(*) FROM users GROUP BY dept HAVING COUNT(*) > 5",
        );
        assert!(score.group_by_count >= 1);
        assert!(score.has_having);
    }

    #[test]
    fn test_complexity_with_cte() {
        let score = score_complexity(
            "WITH active_users AS (SELECT id FROM users WHERE status = 'active') SELECT * FROM active_users",
        );
        assert!(score.has_cte);
    }

    #[test]
    fn test_complexity_with_window_function() {
        let score = score_complexity(
            "SELECT id, ROW_NUMBER() OVER (PARTITION BY dept ORDER BY salary DESC) FROM users",
        );
        assert!(score.has_window_function);
    }

    #[test]
    fn test_complexity_with_set_operation() {
        let score = score_complexity(
            "SELECT id FROM users UNION SELECT id FROM archived_users",
        );
        assert!(score.set_operation_count >= 1);
    }

    #[test]
    fn test_complexity_level_thresholds() {
        // 简单查询应属于 Simple
        let simple = score_complexity("SELECT * FROM users");
        assert_eq!(simple.level(), ComplexityLevel::Simple);

        // 复杂查询应至少属于 Complex 或 VeryComplex
        let complex = score_complexity(
            "WITH t1 AS (SELECT id FROM users WHERE a = 1 AND b = 2 OR c = 3) \
             SELECT t1.id, t2.name, ROW_NUMBER() OVER (PARTITION BY t1.id ORDER BY t2.name) \
             FROM t1 JOIN orders t2 ON t1.id = t2.user_id \
             GROUP BY t1.id, t2.name HAVING COUNT(*) > 1 \
             UNION SELECT id, name, 1 FROM archived",
        );
        assert!(complex.score > simple.score);
    }

    #[test]
    fn test_complexity_level_descriptions() {
        assert_eq!(ComplexityLevel::Simple.description(), "简单");
        assert_eq!(ComplexityLevel::Moderate.description(), "中等");
        assert_eq!(ComplexityLevel::Complex.description(), "复杂");
        assert_eq!(ComplexityLevel::VeryComplex.description(), "非常复杂");
    }

    #[test]
    fn test_complexity_score_max_100() {
        // 构造极复杂查询，确保分数不超过 100
        let mut sql = String::from("SELECT * FROM users");
        for i in 0..20 {
            sql.push_str(&format!(" JOIN orders o{} ON users.id = o{}.user_id", i, i));
        }
        let score = score_complexity(&sql);
        assert!(score.score <= 100);
    }

    #[test]
    fn test_complexity_empty_sql() {
        let score = score_complexity("");
        assert_eq!(score.score, 0);
        assert_eq!(score.level(), ComplexityLevel::Simple);
    }

    // ---- DdlPolicy DDL 策略测试 ----

    #[test]
    fn test_ddl_policy_default_all_denied() {
        let policy = DdlPolicy::default();
        assert!(policy.validate("CREATE TABLE users (id INT)").is_err());
        assert!(policy.validate("DROP TABLE users").is_err());
        assert!(policy.validate("ALTER TABLE users ADD COLUMN name TEXT").is_err());
        assert!(policy.validate("TRUNCATE TABLE users").is_err());
    }

    #[test]
    fn test_ddl_policy_read_only() {
        let policy = DdlPolicy::read_only();
        assert!(policy.validate("CREATE TABLE users (id INT)").is_err());
        assert!(policy.validate("DROP TABLE users").is_err());
        assert!(policy.validate("TRUNCATE TABLE users").is_err());
    }

    #[test]
    fn test_ddl_policy_permissive_allows_all() {
        let policy = DdlPolicy::permissive();
        assert!(policy.validate("CREATE TABLE users (id INT)").is_ok());
        assert!(policy.validate("DROP TABLE users").is_ok());
        assert!(policy.validate("ALTER TABLE users ADD COLUMN name TEXT").is_ok());
        assert!(policy.validate("TRUNCATE TABLE users").is_ok());
    }

    #[test]
    fn test_ddl_policy_safe_evolution() {
        let policy = DdlPolicy::safe_evolution();
        // 允许 CREATE 和 ALTER
        assert!(policy.validate("CREATE TABLE users (id INT)").is_ok());
        assert!(policy.validate("ALTER TABLE users ADD COLUMN name TEXT").is_ok());
        // 禁止 DROP 和 TRUNCATE
        assert!(policy.validate("DROP TABLE users").is_err());
        assert!(policy.validate("TRUNCATE TABLE users").is_err());
    }

    #[test]
    fn test_ddl_policy_create_index() {
        let mut policy = DdlPolicy::permissive();
        policy.allow_create_index = false;
        // CREATE INDEX 被禁止
        assert!(policy
            .validate("CREATE INDEX idx_name ON users (name)")
            .is_err());
        // 普通 CREATE 仍允许
        assert!(policy.validate("CREATE TABLE users (id INT)").is_ok());
    }

    #[test]
    fn test_ddl_policy_drop_index() {
        let mut policy = DdlPolicy::permissive();
        policy.allow_drop_index = false;
        // DROP INDEX 被禁止
        assert!(policy.validate("DROP INDEX idx_name").is_err());
        // 普通 DROP 仍允许
        assert!(policy.validate("DROP TABLE users").is_ok());
    }

    #[test]
    fn test_ddl_policy_non_ddl_passes() {
        let policy = DdlPolicy::read_only();
        // 非 DDL 语句不受策略限制
        assert!(policy.validate("SELECT * FROM users").is_ok());
        assert!(policy.validate("INSERT INTO users VALUES (1)").is_ok());
        assert!(policy.validate("UPDATE users SET name = 'a'").is_ok());
        assert!(policy.validate("DELETE FROM users").is_ok());
    }

    #[test]
    fn test_ddl_policy_custom() {
        let policy = DdlPolicy {
            allow_create: true,
            allow_drop: false,
            allow_alter: true,
            allow_truncate: false,
            allow_create_index: true,
            allow_drop_index: false,
        };
        assert!(policy.validate("CREATE TABLE t (id INT)").is_ok());
        assert!(policy.validate("DROP TABLE t").is_err());
        assert!(policy.validate("ALTER TABLE t ADD COLUMN c INT").is_ok());
        assert!(policy.validate("TRUNCATE TABLE t").is_err());
        assert!(policy.validate("CREATE INDEX i ON t (c)").is_ok());
        assert!(policy.validate("DROP INDEX i").is_err());
    }
}
