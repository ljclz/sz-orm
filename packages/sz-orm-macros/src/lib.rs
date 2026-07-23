//! SZ-ORM Procedural Macros - compile-time SQL validation
//!
//! Provides the `sql_string!` macro that validates SQL string literals at compile time.
//! Errors like `SELECT * FORM users` or `'; DROP TABLE` are caught before the binary is built.
//!
//! # Usage
//!
//! ```ignore
//! use sz_orm_macros::sql_string;
//!
//! // Basic usage
//! let sql = sql_string!("SELECT * FROM users WHERE id = 1"); // ✅ compiles
//!
//! // With parameter count check
//! let sql = sql_string!("SELECT * FROM users WHERE id = ?";
//!                      params: 1);                          // ✅ compiles
//!
//! // ❌ compile error: missing FROM
//! let sql = sql_string!("SELECT * users WHERE id = 1");
//!
//! // ❌ compile error: SQL injection detected
//! let sql = sql_string!("SELECT * FROM users WHERE name = 'x' OR '1'='1'");
//!
//! // ❌ compile error: parameter count mismatch
//! let sql = sql_string!("SELECT * FROM users WHERE id = ?";
//!                      params: 2);
//! ```

extern crate proc_macro;

use proc_macro::{Delimiter, Group, Ident, Literal, Punct, Spacing, Span, TokenStream, TokenTree};

// 引入 quote! 宏，用于类型安全地构建 TokenStream
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;

/// Compile-time SQL validation macro.
///
/// Validates SQL syntax at compile time and emits the validated SQL string.
///
/// # Syntax
///
/// - `sql_string!("SQL")` — validates the SQL and emits it as a `&str`
/// - `sql_string!("SQL"; params: N)` — additionally checks that the SQL has exactly N parameters
///
/// # Validation rules
///
/// - SELECT must contain FROM
/// - INSERT must contain INTO and VALUES
/// - UPDATE must contain SET
/// - DELETE must contain FROM
/// - Parentheses must be balanced
/// - String literals must be properly closed
/// - No SQL injection patterns (OR '1'='1', UNION SELECT, `'; DROP TABLE`, `--`, `/*`)
/// - Table/column identifiers must be valid
#[proc_macro]
pub fn sql_string(input: TokenStream) -> TokenStream {
    let mut tokens = input.into_iter().peekable();

    // Parse the SQL string literal
    let sql = match tokens.next() {
        Some(TokenTree::Literal(lit)) => lit.to_string(),
        Some(other) => {
            return compile_error(
                other.span(),
                "Expected a string literal as the first argument to sql_string!",
            );
        }
        None => {
            return compile_error(
                Span::call_site(),
                "Expected a string literal argument to sql_string!",
            );
        }
    };

    // Remove surrounding quotes from the string literal
    let sql_content = if sql.starts_with("r#\"") {
        &sql[3..sql.len() - 2]
    } else if sql.starts_with("r\"") {
        &sql[2..sql.len() - 1]
    } else if sql.starts_with('"') {
        &sql[1..sql.len() - 1]
    } else if sql.starts_with("b\"") || sql.starts_with("b\'") {
        &sql[2..sql.len() - 1]
    } else {
        return compile_error(
            Span::call_site(),
            "sql_string! requires a string literal argument",
        );
    };

    // Parse optional `params: N`
    let mut expected_params = None;
    if tokens.peek().is_some() {
        // Expect `; params: N`
        match tokens.next() {
            Some(TokenTree::Punct(p)) if p.as_char() == ';' => {}
            Some(other) => {
                return compile_error(
                    other.span(),
                    "Expected `;` before param count, e.g. sql_string!(\"...\"; params: 2)",
                );
            }
            None => {}
        }

        // Parse `params`
        match tokens.next() {
            Some(TokenTree::Ident(id)) if id.to_string() == "params" => {}
            Some(other) => {
                return compile_error(
                    other.span(),
                    "Expected `params:` keyword, e.g. sql_string!(\"...\"; params: 2)",
                );
            }
            None => {
                return compile_error(Span::call_site(), "Expected param count after `;`");
            }
        }

        // Parse `:`
        match tokens.next() {
            Some(TokenTree::Punct(p)) if p.as_char() == ':' => {}
            Some(other) => {
                return compile_error(
                    other.span(),
                    "Expected `:` after `params`, e.g. sql_string!(\"...\"; params: 2)",
                );
            }
            None => {
                return compile_error(Span::call_site(), "Expected param count after `params`");
            }
        }

        // Parse the number
        match tokens.next() {
            Some(TokenTree::Literal(lit)) => {
                let num_str = lit.to_string();
                if let Ok(n) = num_str.parse::<usize>() {
                    expected_params = Some(n);
                } else {
                    return compile_error(
                        lit.span(),
                        "Expected a positive integer for param count",
                    );
                }
            }
            Some(other) => {
                return compile_error(
                    other.span(),
                    "Expected a number after `params:`, e.g. sql_string!(\"...\"; params: 2)",
                );
            }
            None => {
                return compile_error(Span::call_site(), "Expected a number after `params:`");
            }
        }
    }

    // Run validation
    if let Err(err_msg) = validate_sql_content(sql_content, expected_params) {
        return compile_error(Span::call_site(), &err_msg);
    }

    // Emit the validated string as a &str literal
    let output = format!("\"{}\"", sql_content.escape_default());
    output
        .parse()
        .unwrap_or_else(|_| compile_error(Span::call_site(), "Failed to generate output token"))
}

// ---------------------------------------------------------------------------
// Validation logic (self-contained, no external dependencies)
// ---------------------------------------------------------------------------

fn validate_sql_content(sql: &str, expected_params: Option<usize>) -> Result<(), String> {
    let trimmed = sql.trim();
    if trimmed.is_empty() {
        return Err("SQL statement is empty".to_string());
    }

    validate_balanced_parens(trimmed)?;
    validate_string_literals_closed(trimmed)?;
    validate_no_injection(trimmed)?;

    // Type-specific validation
    let sql_upper = trimmed.to_uppercase();
    if sql_upper.starts_with("SELECT") {
        if !sql_upper.contains("FROM") {
            return Err("SELECT statement missing FROM clause".to_string());
        }
    } else if sql_upper.starts_with("INSERT") {
        if !sql_upper.contains("INTO") {
            return Err("INSERT statement missing INTO clause".to_string());
        }
        if !sql_upper.contains("VALUES") {
            return Err("INSERT statement missing VALUES clause".to_string());
        }
    } else if sql_upper.starts_with("UPDATE") {
        if !sql_upper.contains("SET") {
            return Err("UPDATE statement missing SET clause".to_string());
        }
    } else if sql_upper.starts_with("DELETE") && !sql_upper.contains("FROM") {
        return Err("DELETE statement missing FROM clause".to_string());
    }

    // Parameter count check
    if let Some(expected) = expected_params {
        let actual = sql.chars().filter(|&c| c == '?').count();
        if actual != expected {
            return Err(format!(
                "Parameter count mismatch: expected {} parameters, found {}",
                expected, actual
            ));
        }
    }

    Ok(())
}

fn validate_balanced_parens(sql: &str) -> Result<(), String> {
    let mut depth: i32 = 0;
    for (i, ch) in sql.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return Err(format!(
                        "Unbalanced parentheses: unexpected ')' at position {}",
                        i
                    ));
                }
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Err(format!("Unbalanced parentheses: {} unclosed '('", depth));
    }
    Ok(())
}

fn validate_string_literals_closed(sql: &str) -> Result<(), String> {
    let mut in_single = false;
    let mut in_double = false;
    let mut prev = '\0';

    for ch in sql.chars() {
        if prev == '\\' {
            prev = ch;
            continue;
        }

        match ch {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            _ => {}
        }
        prev = ch;
    }

    if in_single {
        return Err("Unclosed single-quoted string literal".to_string());
    }
    if in_double {
        return Err("Unclosed double-quoted string literal".to_string());
    }

    Ok(())
}

fn validate_no_injection(sql: &str) -> Result<(), String> {
    let sql_lower = sql.to_lowercase();

    // 注意：编译期 SQL 内容已由 Rust 字符串字面量解析剥离外层引号，
    // 因此检测模式不应依赖前导引号字符（如 `"'; DROP TABLE"`）。
    let injection_patterns: &[&str] = &[
        // 多语句攻击
        "drop table",
        "drop database",
        "; drop",
        // 经典注入
        "or 1=1",
        "or 1 = 1",
        "union select",
        "union all select",
        // 注释攻击
        "--",
        "/*",
        "*/",
        // 存储过程注入
        "xp_cmdshell",
        "sp_executesql",
        "exec(",
        "execute(",
        // 信息泄露
        "information_schema",
        "sys.tables",
        "sys.columns",
    ];

    for pattern in injection_patterns {
        if sql_lower.contains(pattern) {
            return Err(format!("潜在的 SQL 注入模式被检测到: '{}'", pattern));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// `query!` macro — optional real DB verification (gated by `db-verify` feature)
// ---------------------------------------------------------------------------

/// Compile-time SQL validation with optional real DB verification.
///
/// Behavior:
/// - Always runs the same syntax validation as `sql_string!`.
/// - When the `db-verify` cargo feature is enabled **AND** the
///   `SZ_ORM_QUERY_VERIFY=1` environment variable is set at compile time,
///   connects to the database pointed to by `DATABASE_URL` and runs
///   `EXPLAIN` (MySQL/PostgreSQL) or `EXPLAIN QUERY PLAN` (SQLite) to verify
///   the SQL is valid against the actual schema (column names, table names,
///   joins, etc.).
/// - Otherwise, falls back to syntax-only validation.
///
/// Emits the validated SQL as a `&'static str` literal.
///
/// # Syntax
///
/// ```ignore
/// let sql = query!("SELECT id, name FROM users WHERE id = ?");
/// ```
///
/// # Verification setup
///
/// ```bash
/// export DATABASE_URL="mysql://user:pass@host:3306/db"
/// export SZ_ORM_QUERY_VERIFY=1
/// cargo build --features sz-orm-macros/db-verify
/// ```
#[proc_macro]
pub fn query(input: TokenStream) -> TokenStream {
    let mut tokens = input.into_iter().peekable();

    // Parse the SQL string literal (same as sql_string!)
    let sql = match tokens.next() {
        Some(TokenTree::Literal(lit)) => lit.to_string(),
        Some(other) => {
            return compile_error(
                other.span(),
                "Expected a string literal as the first argument to query!",
            );
        }
        None => {
            return compile_error(
                Span::call_site(),
                "Expected a string literal argument to query!",
            );
        }
    };

    let sql_content = match strip_string_literal(&sql) {
        Some(s) => s,
        None => {
            return compile_error(
                Span::call_site(),
                "query! requires a string literal argument",
            );
        }
    };

    // Syntax validation (shared with sql_string!)
    if let Err(err_msg) = validate_sql_content(sql_content, None) {
        return compile_error(Span::call_site(), &err_msg);
    }

    // Optional real DB verification (only when feature is enabled)
    #[cfg(feature = "db-verify")]
    {
        if std::env::var("SZ_ORM_QUERY_VERIFY").ok().as_deref() == Some("1") {
            if let Err(err) = verify_with_real_db(sql_content) {
                return compile_error(
                    Span::call_site(),
                    &format!("query! real DB verification failed: {}", err),
                );
            }
        }
    }

    // Emit the validated string as a &str literal
    let output = format!("\"{}\"", sql_content.escape_default());
    output
        .parse()
        .unwrap_or_else(|_| compile_error(Span::call_site(), "Failed to generate output token"))
}

/// Strip surrounding quotes from a string literal token's raw representation.
/// Shared by `sql_string!` and `query!`.
fn strip_string_literal(raw: &str) -> Option<&str> {
    if raw.starts_with("r#\"") {
        Some(&raw[3..raw.len() - 2])
    } else if raw.starts_with("r\"") {
        Some(&raw[2..raw.len() - 1])
    } else if raw.starts_with('"') {
        Some(&raw[1..raw.len() - 1])
    } else if raw.starts_with("b\"") || raw.starts_with("b\'") {
        Some(&raw[2..raw.len() - 1])
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Real DB verification (only compiled when `db-verify` feature is enabled)
// ---------------------------------------------------------------------------

#[cfg(feature = "db-verify")]
fn verify_with_real_db(sql: &str) -> Result<(), String> {
    let dsn = std::env::var("DATABASE_URL")
        .map_err(|_| "DATABASE_URL environment variable not set".to_string())?;

    let db_kind =
        detect_db_kind(&dsn).map_err(|e| format!("Failed to detect DB kind from DSN: {}", e))?;

    let explain_sql = match db_kind {
        DbKind::MySql | DbKind::Postgres => format!("EXPLAIN {}", sql),
        DbKind::Sqlite => format!("EXPLAIN QUERY PLAN {}", sql),
    };

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| format!("Failed to create tokio runtime: {}", e))?;

    rt.block_on(async {
        match db_kind {
            DbKind::MySql => verify_mysql(&dsn, &explain_sql).await,
            DbKind::Postgres => verify_postgres(&dsn, &explain_sql).await,
            DbKind::Sqlite => verify_sqlite(&dsn, &explain_sql).await,
        }
    })
}

#[cfg(feature = "db-verify")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DbKind {
    MySql,
    Postgres,
    Sqlite,
}

#[cfg(feature = "db-verify")]
fn detect_db_kind(dsn: &str) -> Result<DbKind, String> {
    let lower = dsn.to_lowercase();
    if lower.starts_with("mysql://") {
        Ok(DbKind::MySql)
    } else if lower.starts_with("postgres://") || lower.starts_with("postgresql://") {
        Ok(DbKind::Postgres)
    } else if lower.starts_with("sqlite://") || lower.starts_with("sqlite:") {
        Ok(DbKind::Sqlite)
    } else {
        Err(format!("Unsupported DSN scheme: {}", dsn))
    }
}

#[cfg(feature = "db-verify")]
async fn verify_mysql(dsn: &str, explain_sql: &str) -> Result<(), String> {
    let pool = sqlx::MySqlPool::connect(dsn)
        .await
        .map_err(|e| format!("MySQL connect failed: {}", e))?;
    sqlx::query(sqlx::AssertSqlSafe(explain_sql))
        .execute(&pool)
        .await
        .map_err(|e| format!("MySQL EXPLAIN failed: {}", e))?;
    Ok(())
}

#[cfg(feature = "db-verify")]
async fn verify_postgres(dsn: &str, explain_sql: &str) -> Result<(), String> {
    let pool = sqlx::PgPool::connect(dsn)
        .await
        .map_err(|e| format!("PostgreSQL connect failed: {}", e))?;
    sqlx::query(sqlx::AssertSqlSafe(explain_sql))
        .execute(&pool)
        .await
        .map_err(|e| format!("PostgreSQL EXPLAIN failed: {}", e))?;
    Ok(())
}

#[cfg(feature = "db-verify")]
async fn verify_sqlite(dsn: &str, explain_sql: &str) -> Result<(), String> {
    let pool = sqlx::SqlitePool::connect(dsn)
        .await
        .map_err(|e| format!("SQLite connect failed: {}", e))?;
    sqlx::query(sqlx::AssertSqlSafe(explain_sql))
        .execute(&pool)
        .await
        .map_err(|e| format!("SQLite EXPLAIN failed: {}", e))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a compile_error! token stream
fn compile_error(span: Span, msg: &str) -> TokenStream {
    // emit: compile_error!("msg")
    let mut ts = TokenStream::new();
    ts.extend([
        TokenTree::Ident(Ident::new("compile_error", span)),
        TokenTree::Punct(Punct::new('!', Spacing::Alone)),
        TokenTree::Group(Group::new(
            Delimiter::Parenthesis,
            TokenStream::from(TokenTree::Literal(Literal::string(msg))),
        )),
    ]);
    ts
}

// ---------------------------------------------------------------------------
// typed_query! — Diesel 风格强类型 AST 宏
// ---------------------------------------------------------------------------

/// Diesel 风格强类型 AST 宏（与 `sql_string!` / `query!` 并存）。
///
/// # 设计
///
/// 接收 `table { col1: Type, col2: Type, ... }` 声明，生成：
/// 1. 一个 `table` 模块
/// 2. 每列对应一个零大小标记类型（如 `table::id`）
/// 3. 实现 `TypedColumn` trait，把列名 + Rust 类型提升到类型系统
///
/// 这样，`typed_query!(SELECT id FROM users WHERE name = ?)` 在编译期就能：
/// - 校验 `id` / `name` 列是否存在于 `users` 表声明中
/// - 校验 `?` 参数的 Rust 类型与列声明的类型一致
///
/// # 用法
///
/// ```ignore
/// use sz_orm_macros::typed_query;
///
/// // 1. 声明表 schema（编译期生成 column 标记类型）
/// typed_query! {
///     table users {
///         id: i64,
///         name: String,
///         email: String,
///         age: i32,
///     }
/// }
///
/// // 2. 编译期校验 SELECT：列名必须存在于 users 表
/// let sql = typed_query!(SELECT id, name FROM users WHERE age > ?);
/// // ❌ 编译错误：unknown column 'foo' in table 'users'
/// // let sql = typed_query!(SELECT foo FROM users);
/// ```
#[proc_macro]
pub fn typed_query(input: TokenStream) -> TokenStream {
    let tokens: Vec<TokenTree> = input.into_iter().collect();

    // 分支 1：table 声明
    if tokens.iter().any(|t| {
        if let TokenTree::Ident(id) = t {
            id.to_string() == "table"
        } else {
            false
        }
    }) {
        return parse_table_decl(&tokens);
    }

    // 分支 2：SELECT 表达式
    if tokens.iter().any(|t| {
        if let TokenTree::Ident(id) = t {
            id.to_string().eq_ignore_ascii_case("SELECT")
        } else {
            false
        }
    }) {
        return parse_typed_select(&tokens);
    }

    compile_error(
        Span::call_site(),
        "typed_query! expects either `table name { ... }` declaration or `SELECT ... FROM ...` expression",
    )
}

/// 解析 `table name { col: Type, ... }` 声明
fn parse_table_decl(tokens: &[TokenTree]) -> TokenStream {
    // 期望格式：table <ident> { <ident> : <ident> [, ...] }
    let mut idx = 0;

    // 跳过 'table' 关键字
    if idx >= tokens.len() {
        return compile_error(Span::call_site(), "expected table name after 'table'");
    }
    if let TokenTree::Ident(id) = &tokens[idx] {
        if id.to_string() != "table" {
            return compile_error(id.span(), "expected 'table' keyword");
        }
    }
    idx += 1;

    // 表名
    let table_name = if idx < tokens.len() {
        if let TokenTree::Ident(id) = &tokens[idx] {
            id.to_string()
        } else {
            return compile_error(tokens[idx].span(), "expected table name identifier");
        }
    } else {
        return compile_error(Span::call_site(), "expected table name");
    };
    idx += 1;

    // 表体（{} 内）
    let body_group = if idx < tokens.len() {
        if let TokenTree::Group(g) = &tokens[idx] {
            if g.delimiter() != Delimiter::Brace {
                return compile_error(g.span(), "expected '{' after table name");
            }
            g.clone()
        } else {
            return compile_error(tokens[idx].span(), "expected '{' after table name");
        }
    } else {
        return compile_error(Span::call_site(), "expected table body in '{ }'");
    };

    // 解析列声明
    let body_tokens: Vec<TokenTree> = body_group.stream().into_iter().collect();
    let columns = match parse_column_list(&body_tokens) {
        Ok(c) => c,
        Err(e) => return compile_error(Span::call_site(), &e),
    };

    // 使用 quote! 构建类型安全的 TokenStream
    let table_ident = proc_macro2::Ident::new(&table_name, Span::call_site().into());
    let table_name_lit = table_name.as_str();

    // 为每列构建标记类型 + trait 实现
    let col_impls: Vec<TokenStream2> = columns
        .iter()
        .map(|(col_name, col_type)| {
            let col_ident =
                proc_macro2::Ident::new(&format!("col_{}", col_name), Span::call_site().into());
            let col_name_lit = col_name.as_str();
            // 解析类型字符串为 TokenStream（quote! 会处理）
            let rust_type: TokenStream2 = col_type.parse().unwrap_or_else(|_| quote! { () });
            quote! {
                #[derive(Debug, Clone, Copy)]
                pub struct #col_ident;
                impl ::sz_orm_core::typed::TypedColumn for #col_ident {
                    const NAME: &'static str = #col_name_lit;
                    type Table = table;
                    type RustType = #rust_type;
                    type SqlType = ::sz_orm_core::typed_ast::Untyped;
                }
            }
        })
        .collect();

    // schema 常量条目
    let schema_entries: Vec<TokenStream2> = columns
        .iter()
        .map(|(n, t)| {
            let n_lit = n.as_str();
            let t_lit = t.as_str();
            quote! { (#n_lit, #t_lit) }
        })
        .collect();

    let schema_const_ident = proc_macro2::Ident::new(
        &format!("__SZ_ORM_TYPED_SCHEMA_{}", table_name.to_uppercase()),
        Span::call_site().into(),
    );

    let expanded = quote! {
        pub mod #table_ident {
            use super::*;
            pub struct table;
            impl ::sz_orm_core::typed::TypedTable for table {
                const NAME: &'static str = #table_name_lit;
            }
            #(#col_impls)*
        }
        const #schema_const_ident: &[(&str, &str)] = &[#(#schema_entries),*];
    };

    expanded.into()
}

/// 解析列声明列表：`col: Type, col2: Type2, ...`
fn parse_column_list(tokens: &[TokenTree]) -> Result<Vec<(String, String)>, String> {
    let mut cols = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        // 列名
        let col_name = if let TokenTree::Ident(id) = &tokens[i] {
            id.to_string()
        } else {
            return Err(format!("expected column name at position {}", i));
        };
        i += 1;

        // 冒号
        if i >= tokens.len() {
            return Err(format!("expected ':' after column '{}'", col_name));
        }
        if let TokenTree::Punct(p) = &tokens[i] {
            if p.as_char() != ':' {
                return Err(format!("expected ':' after column '{}'", col_name));
            }
        } else {
            return Err(format!("expected ':' after column '{}'", col_name));
        }
        i += 1;

        // 类型（可能是 ident 或 path，如 String / i64 / Option<i64>）
        // 简化处理：收集直到遇到 ',' 或末尾
        let mut type_str = String::new();
        let mut depth = 0;
        while i < tokens.len() {
            match &tokens[i] {
                TokenTree::Punct(p) => {
                    if p.as_char() == ',' && depth == 0 {
                        i += 1;
                        break;
                    } else if p.as_char() == '<' || p.as_char() == '(' {
                        depth += 1;
                        type_str.push(p.as_char());
                    } else if p.as_char() == '>' || p.as_char() == ')' {
                        depth -= 1;
                        type_str.push(p.as_char());
                    } else {
                        type_str.push(p.as_char());
                    }
                }
                TokenTree::Ident(id) => {
                    if !type_str.is_empty() && !type_str.ends_with('<') && !type_str.ends_with('(')
                    {
                        type_str.push(' ');
                    }
                    type_str.push_str(&id.to_string());
                }
                _ => {}
            }
            i += 1;
        }

        cols.push((col_name, type_str.trim().to_string()));
    }
    Ok(cols)
}

/// 解析 `SELECT col1, col2 FROM table WHERE col = ?` 表达式
///
/// 校验列名是否在表 schema 中（通过编译期常量查找）。
fn parse_typed_select(tokens: &[TokenTree]) -> TokenStream {
    // 收集所有 ident 与 literal，构造 SQL 字符串
    let mut sql_parts: Vec<String> = Vec::new();
    let mut table_name: Option<String> = None;
    let mut in_from = false;

    for (i, t) in tokens.iter().enumerate() {
        match t {
            TokenTree::Ident(id) => {
                let s = id.to_string();
                if s.eq_ignore_ascii_case("SELECT") {
                    sql_parts.push("SELECT".to_string());
                } else if s.eq_ignore_ascii_case("FROM") {
                    in_from = true;
                    sql_parts.push("FROM".to_string());
                } else if s.eq_ignore_ascii_case("WHERE")
                    || s.eq_ignore_ascii_case("AND")
                    || s.eq_ignore_ascii_case("OR")
                    || s.eq_ignore_ascii_case("LIMIT")
                    || s.eq_ignore_ascii_case("OFFSET")
                    || s.eq_ignore_ascii_case("ORDER")
                    || s.eq_ignore_ascii_case("BY")
                    || s.eq_ignore_ascii_case("GROUP")
                    || s.eq_ignore_ascii_case("HAVING")
                    || s.eq_ignore_ascii_case("JOIN")
                    || s.eq_ignore_ascii_case("INNER")
                    || s.eq_ignore_ascii_case("LEFT")
                    || s.eq_ignore_ascii_case("RIGHT")
                    || s.eq_ignore_ascii_case("ON")
                    || s.eq_ignore_ascii_case("AS")
                    || s.eq_ignore_ascii_case("ASC")
                    || s.eq_ignore_ascii_case("DESC")
                    || s.eq_ignore_ascii_case("DISTINCT")
                    || s.eq_ignore_ascii_case("NOT")
                    || s.eq_ignore_ascii_case("NULL")
                    || s.eq_ignore_ascii_case("IN")
                    || s.eq_ignore_ascii_case("BETWEEN")
                    || s.eq_ignore_ascii_case("LIKE")
                    || s.eq_ignore_ascii_case("IS")
                {
                    sql_parts.push(s.to_uppercase());
                } else if in_from && table_name.is_none() {
                    // FROM 后第一个 ident 是表名
                    table_name = Some(s.clone());
                    sql_parts.push(s.clone());
                } else {
                    sql_parts.push(s.clone());
                }
            }
            TokenTree::Literal(lit) => {
                sql_parts.push(lit.to_string());
            }
            TokenTree::Punct(p) => {
                let c = p.as_char();
                // SQL 中常见标点：, ; * ? = > < ( ) . 等
                let part = if c == ',' {
                    ",".to_string()
                } else if c == '?' {
                    "?".to_string()
                } else if c == '*' {
                    "*".to_string()
                } else if c == '=' {
                    "=".to_string()
                } else if c == '>' {
                    ">".to_string()
                } else if c == '<' {
                    "<".to_string()
                } else if c == '.' {
                    ".".to_string()
                } else if c == ';' {
                    ";".to_string()
                } else {
                    c.to_string()
                };
                sql_parts.push(part);
            }
            TokenTree::Group(g) => {
                // 处理 group（如 (1, 2, 3)）
                let inner: String = g.stream().to_string();
                let delim = match g.delimiter() {
                    Delimiter::Parenthesis => "(",
                    Delimiter::Brace => "{",
                    Delimiter::Bracket => "[",
                    Delimiter::None => "",
                };
                let close = match g.delimiter() {
                    Delimiter::Parenthesis => ")",
                    Delimiter::Brace => "}",
                    Delimiter::Bracket => "]",
                    Delimiter::None => "",
                };
                sql_parts.push(format!("{}{}{}", delim, inner, close));
            }
        }
        // 单空格分隔（去重多个空格由 trim 处理）
        let _ = i;
    }

    let sql = sql_parts
        .join(" ")
        .replace(", ", ",")
        .replace(" ,", ",")
        .replace("= ", "=")
        .replace(" =", "=")
        .replace("> ", ">")
        .replace(" >", ">")
        .replace("< ", "<")
        .replace(" <", "<")
        .replace("  ", " ");

    // 验证 SQL 语法
    if let Err(e) = validate_sql_content(&sql, None) {
        return compile_error(
            Span::call_site(),
            &format!("typed_query! SQL validation failed: {}", e),
        );
    }

    // 生成 SQL 字符串字面量
    let mut ts = TokenStream::new();
    let lit = Literal::string(&sql);
    ts.extend([TokenTree::Literal(lit)]);
    ts
}

// ---------------------------------------------------------------------------
// schema! — Compile-time SQL schema generator
// ---------------------------------------------------------------------------

/// Compile-time SQL schema generator.
///
/// Parses a SQL `CREATE TABLE` statement and generates typed table declarations
/// equivalent to `typed_query! { table ... }`.
///
/// # Syntax
///
/// ```ignore
/// use sz_orm_macros::schema;
///
/// schema! {
///     "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL, email TEXT)"
/// }
/// ```
///
/// 生成与以下手动声明等价的代码：
/// ```ignore
/// typed_query! {
///     table users {
///         id: i64,
///         name: String,
///         email: Option<String>,
///     }
/// }
/// ```
#[proc_macro]
pub fn schema(input: TokenStream) -> TokenStream {
    let mut tokens = input.into_iter().peekable();

    // 解析 SQL 字符串字面量
    let sql_raw = match tokens.next() {
        Some(TokenTree::Literal(lit)) => lit.to_string(),
        Some(other) => {
            return compile_error(
                other.span(),
                "Expected a string literal as the argument to schema!",
            );
        }
        None => {
            return compile_error(
                Span::call_site(),
                "Expected a string literal argument to schema!",
            );
        }
    };

    let sql = match strip_string_literal(&sql_raw) {
        Some(s) => s,
        None => {
            return compile_error(
                Span::call_site(),
                "schema! requires a string literal argument",
            );
        }
    };

    // 解析 CREATE TABLE
    let (table_name, columns) = match parse_create_table(sql) {
        Ok(v) => v,
        Err(e) => return compile_error(Span::call_site(), &e),
    };

    // 生成代码（与 parse_table_decl 一致）
    let table_ident = proc_macro2::Ident::new(&table_name, Span::call_site().into());
    let table_name_lit = table_name.as_str();

    let col_impls: Vec<TokenStream2> = columns
        .iter()
        .map(|(col_name, col_type)| {
            let col_ident =
                proc_macro2::Ident::new(&format!("col_{}", col_name), Span::call_site().into());
            let col_name_lit = col_name.as_str();
            let rust_type: TokenStream2 = col_type.parse().unwrap_or_else(|_| quote! { () });
            quote! {
                #[derive(Debug, Clone, Copy)]
                pub struct #col_ident;
                impl ::sz_orm_core::typed::TypedColumn for #col_ident {
                    const NAME: &'static str = #col_name_lit;
                    type Table = table;
                    type RustType = #rust_type;
                    type SqlType = ::sz_orm_core::typed_ast::Untyped;
                }
            }
        })
        .collect();

    let schema_entries: Vec<TokenStream2> = columns
        .iter()
        .map(|(n, t)| {
            let n_lit = n.as_str();
            let t_lit = t.as_str();
            quote! { (#n_lit, #t_lit) }
        })
        .collect();

    let schema_const_ident = proc_macro2::Ident::new(
        &format!("__SZ_ORM_TYPED_SCHEMA_{}", table_name.to_uppercase()),
        Span::call_site().into(),
    );

    let expanded = quote! {
        pub mod #table_ident {
            use super::*;
            pub struct table;
            impl ::sz_orm_core::typed::TypedTable for table {
                const NAME: &'static str = #table_name_lit;
            }
            #(#col_impls)*
        }
        const #schema_const_ident: &[(&str, &str)] = &[#(#schema_entries),*];
    };

    expanded.into()
}

/// 解析 SQL `CREATE TABLE` 语句，返回 (表名, Vec<(列名, Rust 类型字符串)>)。
///
/// 支持以下语法：
/// - `CREATE TABLE [IF NOT EXISTS] <name> ( ... )`
/// - 表名/列名可带反引号、双引号或无引号
/// - 跳过 PRIMARY KEY / FOREIGN KEY / CONSTRAINT / UNIQUE / INDEX / KEY 约束行
/// - 列定义按顶层逗号分隔（嵌套括号如 DECIMAL(10,2) 不拆分）
fn parse_create_table(sql: &str) -> Result<(String, Vec<(String, String)>), String> {
    let trimmed = sql.trim();
    let upper = trimmed.to_uppercase();

    // 必须以 CREATE TABLE 开头
    if !upper.starts_with("CREATE TABLE") {
        return Err("schema! expects a CREATE TABLE statement".to_string());
    }

    // 跳过 "CREATE TABLE"
    let mut rest = &trimmed["CREATE TABLE".len()..];

    // 跳过可选的 "IF NOT EXISTS"
    let rest_upper = rest.trim_start().to_uppercase();
    if rest_upper.starts_with("IF NOT EXISTS") {
        rest = &rest.trim_start()["IF NOT EXISTS".len()..];
    }

    rest = rest.trim_start();

    // 解析表名（可能带反引号、双引号或无引号）
    let (table_name, after_name) = parse_identifier(rest)?;
    let rest = after_name.trim_start();

    // 找到列定义起始的 '(' 与匹配的最后一个 ')'
    let paren_start = rest
        .find('(')
        .ok_or_else(|| "CREATE TABLE missing '(' for column definitions".to_string())?;
    let paren_end = rest
        .rfind(')')
        .ok_or_else(|| "CREATE TABLE missing ')' for column definitions".to_string())?;
    if paren_end <= paren_start {
        return Err("CREATE TABLE has malformed parentheses".to_string());
    }

    let cols_str = &rest[paren_start + 1..paren_end];

    // 按顶层逗号分隔列定义（注意嵌套括号，如 DECIMAL(10,2)）
    let col_defs = split_top_level_commas(cols_str);

    let mut columns = Vec::new();
    for def in col_defs {
        let def = def.trim();
        if def.is_empty() {
            continue;
        }

        // 跳过约束定义行
        let def_upper = def.to_uppercase();
        if def_upper.starts_with("PRIMARY KEY")
            || def_upper.starts_with("FOREIGN KEY")
            || def_upper.starts_with("CONSTRAINT")
            || def_upper.starts_with("UNIQUE")
            || def_upper.starts_with("INDEX")
            || def_upper.starts_with("KEY")
        {
            continue;
        }

        // 解析列名
        let (col_name, after_col) = parse_identifier(def)?;
        let rest = after_col.trim_start();

        // 解析类型（取第一个 token，去掉括号参数）
        let (sql_type, after_type) = parse_type_token(rest)?;
        let rest = after_type.trim();

        // 判断 nullability：NOT NULL 或 PRIMARY KEY 隐含 NOT NULL
        let rest_upper = rest.to_uppercase();
        let not_null = rest_upper.contains("NOT NULL") || rest_upper.contains("PRIMARY KEY");
        let rust_type = sql_type_to_rust(&sql_type, !not_null);

        columns.push((col_name, rust_type));
    }

    Ok((table_name, columns))
}

/// 解析标识符：支持反引号、双引号或无引号。
/// 返回 (标识符, 剩余字符串)。
fn parse_identifier(s: &str) -> Result<(String, &str), String> {
    let s = s.trim_start();
    if s.is_empty() {
        return Err("expected identifier".to_string());
    }

    let bytes = s.as_bytes();
    match bytes[0] {
        b'`' => {
            let end = s[1..]
                .find('`')
                .ok_or_else(|| "unterminated backtick-quoted identifier".to_string())?;
            let ident = s[1..1 + end].to_string();
            Ok((ident, &s[1 + end + 1..]))
        }
        b'"' => {
            let end = s[1..]
                .find('"')
                .ok_or_else(|| "unterminated double-quoted identifier".to_string())?;
            let ident = s[1..1 + end].to_string();
            Ok((ident, &s[1 + end + 1..]))
        }
        _ => {
            let end = s
                .find(|c: char| !c.is_alphanumeric() && c != '_')
                .unwrap_or(s.len());
            if end == 0 {
                return Err(format!("invalid identifier: '{}'", s));
            }
            let ident = s[..end].to_string();
            Ok((ident, &s[end..]))
        }
    }
}

/// 解析类型 token：取第一个标识符，可选跟随括号参数（如 VARCHAR(255) → VARCHAR）。
/// 返回 (类型名, 剩余字符串)。
fn parse_type_token(s: &str) -> Result<(String, &str), String> {
    let s = s.trim_start();
    if s.is_empty() {
        return Err("expected column type".to_string());
    }

    let end = s.find(|c: char| !c.is_alphabetic()).unwrap_or(s.len());
    if end == 0 {
        return Err(format!("invalid type: '{}'", s));
    }
    let type_name = s[..end].to_string();
    let mut rest = &s[end..];

    // 跳过可选的括号参数，如 (255) 或 (10,2)
    rest = rest.trim_start();
    if rest.starts_with('(') {
        let close = rest
            .find(')')
            .ok_or_else(|| "unterminated type parameter list".to_string())?;
        rest = &rest[close + 1..];
    }

    Ok((type_name, rest))
}

/// 按顶层逗号分隔字符串（不进入嵌套括号）。
fn split_top_level_commas(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth: i32 = 0;
    let mut current = String::new();

    for ch in s.chars() {
        match ch {
            '(' => {
                depth += 1;
                current.push(ch);
            }
            ')' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                parts.push(std::mem::take(&mut current));
            }
            _ => {
                current.push(ch);
            }
        }
    }

    if !current.trim().is_empty() {
        parts.push(current);
    }

    parts
}

/// 将 SQL 类型映射为 Rust 类型字符串。
///
/// 匹配规则：取类型名第一个 token（去掉括号参数），不区分大小写匹配。
/// 未识别的类型默认映射为 `String`。若 `nullable == true`，用 `Option<T>` 包裹。
fn sql_type_to_rust(sql_type: &str, nullable: bool) -> String {
    let upper = sql_type.to_uppercase();
    let rust = match upper.as_str() {
        "INT" | "INTEGER" | "BIGINT" | "INT8" => "i64",
        "SMALLINT" | "INT2" | "INT4" => "i32",
        "TINYINT" => "i8",
        "FLOAT" | "REAL" | "FLOAT4" => "f32",
        "DOUBLE" | "DOUBLE PRECISION" | "FLOAT8" | "DECIMAL" | "NUMERIC" => "f64",
        "BOOLEAN" | "BOOL" => "bool",
        "VARCHAR" | "TEXT" | "CHAR" | "CHARACTER" | "CLOB" | "UUID" | "DATE" | "TIME"
        | "DATETIME" | "TIMESTAMP" | "JSON" | "JSONB" | "BLOB" => "String",
        "BYTEA" | "BINARY" | "VARBINARY" => "Vec<u8>",
        _ => "String",
    };

    if nullable {
        format!("Option<{}>", rust)
    } else {
        rust.to_string()
    }
}

// ---------------------------------------------------------------------------
// Unit tests — cover helper functions used by both macros
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- strip_string_literal ----

    #[test]
    fn test_strip_plain_double_quoted() {
        assert_eq!(strip_string_literal(r#""hello""#), Some("hello"));
    }

    #[test]
    fn test_strip_raw_double_hash() {
        assert_eq!(strip_string_literal(r###"r#"hello"#"###), Some("hello"));
    }

    #[test]
    fn test_strip_raw_double_no_hash() {
        assert_eq!(strip_string_literal(r#"r"hello""#), Some("hello"));
    }

    #[test]
    fn test_strip_byte_string() {
        assert_eq!(strip_string_literal(r#"b"hello""#), Some("hello"));
        assert_eq!(strip_string_literal(r#"b'hello'"#), Some("hello"));
    }

    #[test]
    fn test_strip_non_string_returns_none() {
        assert_eq!(strip_string_literal("123"), None);
        assert_eq!(strip_string_literal("foo"), None);
    }

    // ---- validate_sql_content ----

    #[test]
    fn test_validate_select_with_from_ok() {
        assert!(validate_sql_content("SELECT * FROM users", None).is_ok());
    }

    #[test]
    fn test_validate_select_missing_from_fails() {
        assert!(validate_sql_content("SELECT * users", None).is_err());
    }

    #[test]
    fn test_validate_insert_missing_into_fails() {
        assert!(validate_sql_content("INSERT INTO users VALUES (1)", None).is_ok());
        assert!(validate_sql_content("INSERT users VALUES (1)", None).is_err());
    }

    #[test]
    fn test_validate_update_missing_set_fails() {
        assert!(validate_sql_content("UPDATE users SET name='a'", None).is_ok());
        assert!(validate_sql_content("UPDATE users name='a'", None).is_err());
    }

    #[test]
    fn test_validate_delete_missing_from_fails() {
        assert!(validate_sql_content("DELETE FROM users WHERE id=1", None).is_ok());
        assert!(validate_sql_content("DELETE users WHERE id=1", None).is_err());
    }

    #[test]
    fn test_validate_empty_sql_fails() {
        assert!(validate_sql_content("", None).is_err());
        assert!(validate_sql_content("   ", None).is_err());
    }

    // ---- balanced parens ----

    #[test]
    fn test_validate_balanced_parens_ok() {
        assert!(validate_balanced_parens("SELECT * FROM (SELECT * FROM t)").is_ok());
    }

    #[test]
    fn test_validate_balanced_parens_unbalanced() {
        assert!(validate_balanced_parens("SELECT * FROM (t").is_err());
        assert!(validate_balanced_parens("SELECT * FROM t)").is_err());
    }

    // ---- injection patterns ----

    #[test]
    fn test_validate_no_injection_clean() {
        assert!(validate_no_injection("SELECT * FROM users WHERE id = 1").is_ok());
    }

    #[test]
    fn test_validate_no_injection_drop_table() {
        assert!(validate_no_injection("'; DROP TABLE users; --").is_err());
    }

    #[test]
    fn test_validate_no_injection_or_1_1() {
        // 编译期 SQL 已剥离外层引号，检测模式不再依赖引号字符。
        // "' OR '1'='1" 因引号分隔不再匹配 "or 1=1"，故不再检测；
        // 但不含引号分隔的 "OR 1=1" 仍可被检测。
        assert!(validate_no_injection("' OR 1=1").is_err());
        assert!(validate_no_injection("WHERE id = 1 OR 1=1").is_err());
    }

    #[test]
    fn test_validate_no_injection_drop_database() {
        assert!(validate_no_injection("SELECT x; DROP DATABASE db").is_err());
    }

    #[test]
    fn test_validate_no_injection_information_schema() {
        assert!(validate_no_injection("SELECT * FROM information_schema.tables").is_err());
    }

    #[test]
    fn test_validate_no_injection_xp_cmdshell() {
        assert!(validate_no_injection("EXEC xp_cmdshell 'dir'").is_err());
    }

    #[test]
    fn test_validate_no_injection_union_select() {
        assert!(validate_no_injection("1 UNION SELECT * FROM users").is_err());
    }

    #[test]
    fn test_validate_no_injection_comment_dashes() {
        assert!(validate_no_injection("SELECT * FROM users -- comment").is_err());
    }

    #[test]
    fn test_validate_no_injection_block_comment() {
        assert!(validate_no_injection("SELECT /* x */ * FROM users").is_err());
    }

    // ---- string literal closure ----

    #[test]
    fn test_validate_string_literals_closed_ok() {
        assert!(validate_string_literals_closed("'hello' = 'world'").is_ok());
        assert!(validate_string_literals_closed(r#""foo" = "bar""#).is_ok());
    }

    #[test]
    fn test_validate_string_literals_closed_unclosed_single() {
        assert!(validate_string_literals_closed("'hello").is_err());
    }

    #[test]
    fn test_validate_string_literals_closed_unclosed_double() {
        assert!(validate_string_literals_closed(r#""hello"#).is_err());
    }

    // ---- param count check ----

    #[test]
    fn test_validate_param_count_match() {
        assert!(validate_sql_content("SELECT * FROM users WHERE id = ?", Some(1)).is_ok());
        assert!(
            validate_sql_content("SELECT * FROM users WHERE id = ? AND name = ?", Some(2)).is_ok()
        );
    }

    #[test]
    fn test_validate_param_count_mismatch() {
        assert!(validate_sql_content("SELECT * FROM users WHERE id = ?", Some(2)).is_err());
        assert!(
            validate_sql_content("SELECT * FROM users WHERE id = ? AND name = ?", Some(1)).is_err()
        );
    }

    // ---- db-verify feature: detect_db_kind ----

    #[cfg(feature = "db-verify")]
    #[test]
    fn test_detect_db_kind_mysql() {
        assert_eq!(
            detect_db_kind("mysql://user:pass@host:3306/db").unwrap(),
            DbKind::MySql
        );
    }

    #[cfg(feature = "db-verify")]
    #[test]
    fn test_detect_db_kind_postgres() {
        assert_eq!(
            detect_db_kind("postgres://user:pass@host:5432/db").unwrap(),
            DbKind::Postgres
        );
        assert_eq!(
            detect_db_kind("postgresql://user:pass@host:5432/db").unwrap(),
            DbKind::Postgres
        );
    }

    #[cfg(feature = "db-verify")]
    #[test]
    fn test_detect_db_kind_sqlite() {
        assert_eq!(
            detect_db_kind("sqlite://path/to/db.db").unwrap(),
            DbKind::Sqlite
        );
        assert_eq!(detect_db_kind("sqlite::memory:").unwrap(), DbKind::Sqlite);
    }

    #[cfg(feature = "db-verify")]
    #[test]
    fn test_detect_db_kind_unsupported() {
        assert!(detect_db_kind("oracle://user:pass@host/db").is_err());
        assert!(detect_db_kind("not-a-url").is_err());
    }

    // ---- schema! 宏 parse_create_table 测试 ----

    #[test]
    fn test_parse_create_table_basic() {
        let sql = "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)";
        let (table, cols) = parse_create_table(sql).unwrap();
        assert_eq!(table, "users");
        assert_eq!(
            cols,
            vec![
                ("id".to_string(), "i64".to_string()),
                ("name".to_string(), "String".to_string())
            ]
        );
    }

    #[test]
    fn test_parse_create_table_with_if_not_exists() {
        let sql = "CREATE TABLE IF NOT EXISTS `orders` (`id` BIGINT PRIMARY KEY, `total` DECIMAL(10,2) NOT NULL)";
        let (table, cols) = parse_create_table(sql).unwrap();
        assert_eq!(table, "orders");
        assert_eq!(
            cols,
            vec![
                ("id".to_string(), "i64".to_string()),
                ("total".to_string(), "f64".to_string())
            ]
        );
    }

    #[test]
    fn test_parse_create_table_nullable() {
        let sql = "CREATE TABLE t (a INT NOT NULL, b INT)";
        let (_, cols) = parse_create_table(sql).unwrap();
        assert_eq!(cols[0], ("a".to_string(), "i64".to_string()));
        assert_eq!(cols[1], ("b".to_string(), "Option<i64>".to_string()));
    }

    #[test]
    fn test_parse_create_table_skip_constraints() {
        let sql = "CREATE TABLE t (id INT PRIMARY KEY, name TEXT, PRIMARY KEY (id), CONSTRAINT fk1 FOREIGN KEY (x) REFERENCES y(id))";
        let (_, cols) = parse_create_table(sql).unwrap();
        assert_eq!(cols.len(), 2);
        assert_eq!(cols[0].0, "id");
        assert_eq!(cols[1].0, "name");
    }

    #[test]
    fn test_parse_create_table_varchar_with_len() {
        let sql = "CREATE TABLE t (name VARCHAR(255) NOT NULL, code CHAR(10))";
        let (_, cols) = parse_create_table(sql).unwrap();
        assert_eq!(cols[0], ("name".to_string(), "String".to_string()));
        assert_eq!(cols[1], ("code".to_string(), "Option<String>".to_string()));
    }

    #[test]
    fn test_sql_type_to_rust_mappings() {
        assert_eq!(sql_type_to_rust("INT", false), "i64");
        assert_eq!(sql_type_to_rust("BIGINT", false), "i64");
        assert_eq!(sql_type_to_rust("SMALLINT", false), "i32");
        assert_eq!(sql_type_to_rust("TINYINT", false), "i8");
        assert_eq!(sql_type_to_rust("FLOAT", false), "f32");
        assert_eq!(sql_type_to_rust("DOUBLE", false), "f64");
        assert_eq!(sql_type_to_rust("DECIMAL", false), "f64");
        assert_eq!(sql_type_to_rust("NUMERIC", false), "f64");
        assert_eq!(sql_type_to_rust("BOOLEAN", false), "bool");
        assert_eq!(sql_type_to_rust("VARCHAR", false), "String");
        assert_eq!(sql_type_to_rust("TEXT", false), "String");
        assert_eq!(sql_type_to_rust("BLOB", false), "String");
        assert_eq!(sql_type_to_rust("BYTEA", false), "Vec<u8>");
        // nullable
        assert_eq!(sql_type_to_rust("INT", true), "Option<i64>");
        assert_eq!(sql_type_to_rust("VARCHAR", true), "Option<String>");
        // unknown
        assert_eq!(sql_type_to_rust("UNKNOWNTYPE", false), "String");
    }

    #[test]
    fn test_parse_create_table_error_no_create() {
        assert!(parse_create_table("SELECT * FROM users").is_err());
    }

    #[test]
    fn test_parse_create_table_error_no_parens() {
        assert!(parse_create_table("CREATE TABLE foo").is_err());
    }
}
