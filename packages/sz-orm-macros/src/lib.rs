//! SZ-ORM Procedural Macros - compile-time SQL validation & derive macros
//!
//! Provides:
//! - `sql_string!` macro that validates SQL string literals at compile time.
//!   Errors like `SELECT * FORM users` or `'; DROP TABLE` are caught before the binary is built.
//! - `#[derive(Schema)]` — auto-generate table structure info from a struct.
//! - `#[derive(Builder)]` — auto-generate builder pattern code for a struct.
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
//! let sql = sql_string!("SELECT * FROM users WHERE id = ?");
//!                      params: 1);                          // ✅ compiles
//!
//! // ❌ compile error: missing FROM
//! let sql = sql_string!("SELECT * users WHERE id = 1");
//!
//! // ❌ compile error: SQL injection detected
//! let sql = sql_string!("SELECT * FROM users WHERE name = 'x' OR '1'='1'");
//!
//! // ❌ compile error: parameter count mismatch
//! let sql = sql_string!("SELECT * FROM users WHERE id = ?");
//!                      params: 2);
//! ```

// 抑制 Windows 链接器输出"正在创建库 ..."的诊断信息被识别为警告：
// 该输出是 link.exe 创建 DLL 导入库时的正常 stdout 提示，并非代码问题。
#![allow(linker_messages)]
//!
//! # Derive macros
//!
//! ```ignore
//! use sz_orm_macros::{Schema, Builder};
//!
//! #[derive(Schema)]
//! #[table(name = "users")]
//! struct User {
//!     #[column(primary_key)]
//!     id: i64,
//!     name: String,
//! }
//!
//! #[derive(Builder)]
//! struct Order {
//!     id: i64,
//!     total: f64,
//! }
//! ```

extern crate proc_macro;

use proc_macro::{Delimiter, Group, Ident, Literal, Punct, Spacing, Span, TokenStream, TokenTree};

#[cfg(feature = "db-verify")]
use sqlx::Row as _;

// 引入 quote! 宏，用于类型安全地构建 TokenStream
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse_macro_input;

// 派生宏模块
mod derive;

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
/// Emits a [`sz_orm_core::queryable::Query`] object wrapping the validated SQL.
///
/// # Syntax
///
/// ```ignore
/// use sz_orm_core::queryable::Query;
/// let q = query!("SELECT id, name FROM users WHERE id = ?");
/// let rows = q.fetch_all(&mut conn).await?;
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

    // P0-1：支持可选类型参数 `query!(T, "SQL")` → `QueryAs::<T>::new(sql)`
    // 若无类型参数则保持 `query!("SQL")` → `Query::new(sql)`
    let type_param: Option<TokenStream2> = match tokens.peek() {
        Some(TokenTree::Ident(_)) | Some(TokenTree::Punct(_)) => {
            // 收集类型路径（如 `User` 或 `crate::User`）
            let mut ty_tokens = Vec::new();
            while let Some(tok) = tokens.peek() {
                match tok {
                    TokenTree::Punct(p) if p.as_char() == ',' => break,
                    TokenTree::Punct(p) if p.as_char() == ':' => {
                        ty_tokens.push(tokens.next().unwrap());
                        // 消耗 `:`
                        if let Some(TokenTree::Punct(p2)) = tokens.peek() {
                            if p2.as_char() == ':' {
                                ty_tokens.push(tokens.next().unwrap());
                            }
                        }
                    }
                    _ => ty_tokens.push(tokens.next().unwrap()),
                }
            }
            // 确认下一个 token 是逗号（类型参数分隔符）
            match tokens.peek() {
                Some(TokenTree::Punct(p)) if p.as_char() == ',' => {
                    tokens.next(); // 消耗逗号
                    let ts: proc_macro::TokenStream = ty_tokens.into_iter().collect();
                    Some(TokenStream2::from(ts))
                }
                _ => None, // 不是类型参数，回退
            }
        }
        _ => None,
    };

    // Parse the SQL string literal
    let sql = match tokens.next() {
        Some(TokenTree::Literal(lit)) => lit.to_string(),
        Some(other) => {
            return compile_error(
                other.span(),
                if type_param.is_some() {
                    "query!(T, \"SQL\"): expected a string literal as the second argument"
                } else {
                    "Expected a string literal as the first argument to query!"
                },
            );
        }
        None => {
            return compile_error(
                Span::call_site(),
                if type_param.is_some() {
                    "query!(T, \"SQL\"): missing SQL string argument"
                } else {
                    "Expected a string literal argument to query!"
                },
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
    let verify_cols: Option<Vec<(String, String)>> = {
        match std::env::var("SZ_ORM_QUERY_VERIFY").ok().as_deref() {
            // 模式 1：连真 DB 执行 EXPLAIN 验证（需 DATABASE_URL），并获取 SELECT 列的实际类型
            Some("1") => match verify_with_real_db(sql_content) {
                Ok(cols) => Some(cols),
                Err(err) => {
                    return compile_error(
                        Span::call_site(),
                        &format!("query! real DB verification failed: {}", err),
                    )
                }
            },
            // 模式 cache：从离线缓存文件查找（无需 DB，适合 CI）
            Some("cache") => {
                if let Err(err) = verify_with_cache(sql_content) {
                    return compile_error(
                        Span::call_site(),
                        &format!("query! offline cache verification failed: {}", err),
                    );
                }
                None
            }
            _ => None,
        }
    };
    #[cfg(not(feature = "db-verify"))]
    let _verify_cols: Option<Vec<(String, String)>> = None;

    // Emit the appropriate query object
    let escaped = sql_content.escape_default().to_string();
    let base = if let Some(ref ty) = type_param {
        // query!(T, "SQL") → QueryAs::<T>::new("SQL")
        format!(
            "::sz_orm_core::queryable::QueryAs::<{}>::new(\"{}\")",
            ty, escaped
        )
    } else {
        // query!("SQL") → Query::new("SQL")
        format!("::sz_orm_core::queryable::Query::new(\"{}\")", escaped)
    };
    // db-verify 通过且有类型参数时，附加编译期类型验证块（P0-2）
    #[cfg(feature = "db-verify")]
    let output = match (&verify_cols, &type_param) {
        (Some(cols), Some(ty)) if !cols.is_empty() => {
            gen_compile_time_type_check(&ty.to_string(), sql_content, cols, &base)
        }
        _ => base,
    };
    #[cfg(not(feature = "db-verify"))]
    let output = base;
    output
        .parse()
        .unwrap_or_else(|_| compile_error(Span::call_site(), "Failed to generate query! output"))
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
fn verify_with_real_db(sql: &str) -> Result<Vec<(String, String)>, String> {
    let dsn = std::env::var("DATABASE_URL")
        .map_err(|_| "DATABASE_URL environment variable not set".to_string())?;

    let db_kind =
        detect_db_kind(&dsn).map_err(|e| format!("Failed to detect DB kind from DSN: {}", e))?;

    // 将 ? 占位符替换为 NULL，使 EXPLAIN 无需绑定参数即可执行。
    // EXPLAIN 不实际执行查询，NULL 对所有列类型都合法。
    let sql_no_placeholders = replace_placeholders_with_null(sql);

    // Oracle/SQL Server 使用 EXPLAIN PLAN FOR（不同语法），其余用 EXPLAIN
    let explain_sql = match db_kind {
        DbKind::MySql | DbKind::Postgres => format!("EXPLAIN {}", sql_no_placeholders),
        DbKind::Sqlite => format!("EXPLAIN QUERY PLAN {}", sql_no_placeholders),
        // Oracle: EXPLAIN PLAN FOR 放入 PLAN_TABLE，再查询结果验证语法
        DbKind::Oracle => format!("EXPLAIN PLAN FOR {}", sql_no_placeholders),
        // SQL Server: SET SHOWPLAN_TEXT ON 后执行（不实际运行）
        DbKind::SqlServer => sql_no_placeholders,
    };

    // MySQL/PG/SQLite 走 sqlx 异步路径
    if matches!(db_kind, DbKind::MySql | DbKind::Postgres | DbKind::Sqlite) {
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| format!("Failed to create tokio runtime: {}", e))?;
        return rt.block_on(async {
            // 1. EXPLAIN 语法验证
            if let DbKind::MySql = db_kind {
                verify_mysql(&dsn, &explain_sql).await?;
            } else if let DbKind::Postgres = db_kind {
                verify_postgres(&dsn, &explain_sql).await?;
            } else {
                // 由外层 if matches! 保证此处必为 Sqlite
                verify_sqlite(&dsn, &explain_sql).await?;
            }
            // 2. 列名/类型验证（Gap 1 修复）
            verify_columns(&dsn, db_kind, sql).await?;
            // 3. 获取 SELECT 列的实际 DB 类型（供编译期类型验证使用）
            //    SQLite/Oracle/SQL Server 返回空列表（跳过类型级验证）
            fetch_column_types(&dsn, db_kind, sql).await
        });
    }

    // Oracle/SQL Server 走命令行工具验证（避免引入重依赖）
    if let DbKind::Oracle = db_kind {
        verify_oracle(&dsn, &explain_sql).map(|_| Vec::new())
    } else {
        // 由外层 if matches! 保证此处必为 SqlServer
        verify_sqlserver(&dsn, &explain_sql).map(|_| Vec::new())
    }
}

/// 离线缓存验证：从 `SZ_ORM_SQLX_CACHE` 指定的 JSON 文件中查找已验证的 SQL。
///
/// 缓存文件格式为 JSON 字符串数组，每行一条已验证 SQL：
/// ```json
/// ["SELECT `id`, `name` FROM `users` WHERE `id` = ?", ...]
/// ```
///
/// 生成方式：在有 DB 的环境中运行 `cargo build --features db-verify`（`SZ_ORM_QUERY_VERIFY=1`），
/// 或使用 `cargo sz-orm prepare` 工具扫描项目中的 `query!` 宏并生成缓存。
///
/// CI 中只需设置 `SZ_ORM_QUERY_VERIFY=cache` + `SZ_ORM_SQLX_CACHE=.sz-orm/query-cache.json`
/// 即可在不连接 DB 的情况下完成编译期 SQL 验证。
#[cfg(feature = "db-verify")]
fn verify_with_cache(sql: &str) -> Result<(), String> {
    let cache_path = std::env::var("SZ_ORM_SQLX_CACHE").map_err(|_| {
        "SZ_ORM_SQLX_CACHE not set. \
             Set it to the path of a JSON file containing verified SQL statements, \
             e.g. SZ_ORM_SQLX_CACHE=.sz-orm/query-cache.json"
            .to_string()
    })?;

    let cache_content = std::fs::read_to_string(&cache_path).map_err(|e| {
        format!(
            "Failed to read cache file '{}': {}. \
             Run `cargo sz-orm prepare` or build with SZ_ORM_QUERY_VERIFY=1 to generate it.",
            cache_path, e
        )
    })?;

    // 支持两种格式：JSON 数组 或 每行一条 SQL 的文本文件
    let verified: Vec<String> = serde_json::from_str(&cache_content).unwrap_or_else(|_| {
        cache_content
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .collect()
    });

    if verified.iter().any(|v| v.trim() == sql.trim()) {
        Ok(())
    } else {
        Err(format!(
            "SQL not found in offline cache ({} entries): \"{}\". \
             Add it to the cache by running with SZ_ORM_QUERY_VERIFY=1 first.",
            verified.len(),
            truncate_sql(sql, 80)
        ))
    }
}

/// 截断 SQL 用于错误消息显示
#[cfg(feature = "db-verify")]
fn truncate_sql(sql: &str, max: usize) -> String {
    if sql.len() <= max {
        sql.to_string()
    } else {
        format!("{}...", &sql[..max])
    }
}

#[cfg(feature = "db-verify")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DbKind {
    MySql,
    Postgres,
    Sqlite,
    Oracle,
    SqlServer,
}

/// 将 SQL 中的 `?` 占位符替换为 `NULL`，跳过字符串字面量内的 `?`。
///
/// EXPLAIN 不实际执行查询，用 NULL 代替参数可验证语法和表/列存在性，
/// 同时避免 sqlx 预处理语句要求绑定参数的问题。
#[cfg(feature = "db-verify")]
fn replace_placeholders_with_null(sql: &str) -> String {
    let mut result = String::with_capacity(sql.len() + 16);
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut prev = '\0';

    for ch in sql.chars() {
        if prev == '\\' {
            // 转义字符：直接追加
            result.push(ch);
            prev = ch;
            continue;
        }
        match ch {
            '\'' if !in_double_quote => in_single_quote = !in_single_quote,
            '"' if !in_single_quote => in_double_quote = !in_double_quote,
            '?' if !in_single_quote && !in_double_quote => {
                result.push_str("NULL");
                prev = ch;
                continue;
            }
            _ => {}
        }
        result.push(ch);
        prev = ch;
    }
    result
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
    } else if lower.starts_with("oracle://") || lower.starts_with("oracle:") {
        Ok(DbKind::Oracle)
    } else if lower.starts_with("sqlserver://")
        || lower.starts_with("mssql://")
        || lower.starts_with("tds://")
    {
        Ok(DbKind::SqlServer)
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

// ========================================================================
// Gap 1 修复：列名/类型验证（在 EXPLAIN 语法验证通过后执行）
// ========================================================================

/// 从 SQL 中提取表名和列引用，并查询 information_schema 验证列存在性。
///
/// EXPLAIN 已验证语法和表存在性；此函数进一步验证：
/// - SELECT/WHERE/ORDER BY/GROUP BY 中引用的列是否存在于对应表中
/// - 不验证表别名限定的列（由 EXPLAIN 负责）
/// - 仅对 `*` 以外的显式列名做验证
///
/// # 支持的 DB
///
/// MySQL / PostgreSQL / SQLite（Oracle/SQL Server 跳过此步骤）
#[cfg(feature = "db-verify")]
async fn verify_columns(dsn: &str, db_kind: DbKind, sql: &str) -> Result<(), String> {
    // SQLite 的 information_schema 支持有限，跳过
    if matches!(db_kind, DbKind::Sqlite | DbKind::Oracle | DbKind::SqlServer) {
        return Ok(());
    }

    let tables = extract_tables(sql);
    let columns = extract_columns(sql);

    if tables.is_empty() || columns.is_empty() {
        return Ok(());
    }

    match db_kind {
        DbKind::MySql => verify_columns_mysql(dsn, &tables, &columns, sql).await,
        DbKind::Postgres => verify_columns_postgres(dsn, &tables, &columns, sql).await,
        _ => Ok(()),
    }
}

/// 从 SQL 的 FROM 子句中提取表名（支持别名和 JOIN）
#[cfg(feature = "db-verify")]
fn extract_tables(sql: &str) -> Vec<String> {
    let mut tables = Vec::new();
    let upper = sql.to_uppercase();

    // 查找 FROM ... WHERE/ORDER/GROUP/LIMIT/HAVING/JOIN 之间的内容
    let from_idx = match upper.find("FROM") {
        Some(i) => i,
        None => return tables,
    };

    let end_patterns = ["WHERE", "ORDER", "GROUP", "LIMIT", "HAVING", "UNION"];
    let end_idx = end_patterns
        .iter()
        .filter_map(|p| {
            // 按单词边界查找，避免 "ORDER" 误匹配 "user_id" 等标识符中的子串
            let mut search_start = 0;
            while let Some(i) = upper[search_start..].find(*p) {
                let abs_i = search_start + i;
                let before = upper[..abs_i].chars().last().unwrap_or(' ');
                let after = upper[abs_i + p.len()..].chars().next().unwrap_or(' ');
                if !before.is_alphanumeric()
                    && !after.is_alphanumeric()
                    && before != '_'
                    && after != '_'
                {
                    return Some(abs_i);
                }
                search_start = abs_i + p.len();
            }
            None
        })
        .filter(|&i| i > from_idx)
        .min()
        .unwrap_or(sql.len());

    let from_clause = &sql[from_idx + 4..end_idx];

    // 按逗号、换行、JOIN 关键字分割（忽略大小写）
    let join_split = {
        let lower = from_clause.to_lowercase();
        let mut result = String::with_capacity(from_clause.len());
        let mut i = 0;
        let bytes = from_clause.as_bytes();
        let lower_bytes = lower.as_bytes();
        while i < bytes.len() {
            let mut matched = false;
            for join_kw in &[
                " join ",
                " inner join ",
                " left join ",
                " right join ",
                " left outer join ",
                " right outer join ",
                " cross join ",
                " full join ",
                " full outer join ",
            ] {
                let kw = join_kw.as_bytes();
                if i + kw.len() <= bytes.len() && &lower_bytes[i..i + kw.len()] == kw {
                    result.push(',');
                    i += kw.len();
                    matched = true;
                    break;
                }
            }
            if !matched {
                result.push(bytes[i] as char);
                i += 1;
            }
        }
        result
    };
    let parts: Vec<&str> = join_split.split([',', '\n']).collect();

    for part in parts {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        // 取第一个词作为表名（忽略别名）
        let table_word = part
            .split_whitespace()
            .next()
            .unwrap_or(part)
            .trim_end_matches([',', ';']);
        // 去除反引号/双引号
        let clean = table_word.trim_matches(|c| c == '`' || c == '"');
        if !clean.is_empty()
            && !matches!(
                clean.to_uppercase().as_str(),
                "INNER"
                    | "LEFT"
                    | "RIGHT"
                    | "OUTER"
                    | "CROSS"
                    | "FULL"
                    | "NATURAL"
                    | "ON"
                    | "USING"
                    | "AS"
            )
        {
            tables.push(clean.to_lowercase());
        }
    }

    tables
}

/// 从 SQL 中提取未限定的列名引用（SELECT/WHERE/ORDER BY/GROUP BY 中）
#[cfg(feature = "db-verify")]
fn extract_columns(sql: &str) -> Vec<String> {
    let mut columns = Vec::new();
    let upper = sql.to_uppercase();

    // 收集各子句中的标识符
    let mut collect_from_segment = |segment: &str| {
        // 简单的标识符提取：匹配 `\w+` 模式的词
        // 排除 SQL 关键字和已限定的列（table.col）
        let keywords = [
            "SELECT",
            "FROM",
            "WHERE",
            "AND",
            "OR",
            "NOT",
            "IN",
            "IS",
            "NULL",
            "LIKE",
            "BETWEEN",
            "AS",
            "ON",
            "JOIN",
            "INNER",
            "LEFT",
            "RIGHT",
            "OUTER",
            "CROSS",
            "FULL",
            "NATURAL",
            "ORDER",
            "BY",
            "GROUP",
            "HAVING",
            "LIMIT",
            "OFFSET",
            "ASC",
            "DESC",
            "DISTINCT",
            "COUNT",
            "SUM",
            "AVG",
            "MIN",
            "MAX",
            "CASE",
            "WHEN",
            "THEN",
            "ELSE",
            "END",
            "COALESCE",
            "NULLIF",
            "CAST",
            "TRUE",
            "FALSE",
            "INSERT",
            "INTO",
            "VALUES",
            "UPDATE",
            "SET",
            "DELETE",
            "CREATE",
            "TABLE",
            "INDEX",
            "IF",
            "EXISTS",
            "PRIMARY",
            "KEY",
            "REFERENCES",
            "FOREIGN",
        ];

        for word in segment.split(|c: char| !c.is_alphanumeric() && c != '_') {
            if word.is_empty() || word.len() < 2 {
                continue;
            }
            let w = word.to_uppercase();
            if keywords.contains(&w.as_str()) {
                continue;
            }
            // 跳过纯数字
            if word.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            // 跳过已限定的列（table.col）— 这些由 EXPLAIN 验证
            // 检查前面是否有 `.`
            let pos = segment.find(word).unwrap_or(0);
            if pos > 0 && segment.chars().nth(pos - 1) == Some('.') {
                continue;
            }
            // 跳过 *
            if word == "*" {
                continue;
            }
            let lower = word.to_lowercase();
            if !columns.contains(&lower) {
                columns.push(lower);
            }
        }
    };

    // 收集 SELECT 列（FROM 之前）
    if let Some(from_idx) = upper.find("FROM") {
        if let Some(sel_idx) = upper.find("SELECT") {
            let sel_segment = &sql[sel_idx + 6..from_idx];
            collect_from_segment(sel_segment);
        }
    }

    // 收集 WHERE 列
    if let Some(where_idx) = upper.find("WHERE") {
        let end_idx = ["ORDER", "GROUP", "LIMIT", "HAVING", "UNION"]
            .iter()
            .filter_map(|p| upper.find(p))
            .filter(|&i| i > where_idx)
            .min()
            .unwrap_or(sql.len());
        collect_from_segment(&sql[where_idx + 5..end_idx]);
    }

    // 收集 ORDER BY 列
    if let Some(order_idx) = upper.find("ORDER BY") {
        let end_idx = ["GROUP", "LIMIT", "HAVING", "UNION"]
            .iter()
            .filter_map(|p| upper.find(p))
            .filter(|&i| i > order_idx)
            .min()
            .unwrap_or(sql.len());
        collect_from_segment(&sql[order_idx + 8..end_idx]);
    }

    columns
}

#[cfg(feature = "db-verify")]
async fn verify_columns_mysql(
    dsn: &str,
    tables: &[String],
    columns: &[String],
    sql: &str,
) -> Result<(), String> {
    let pool = sqlx::MySqlPool::connect(dsn)
        .await
        .map_err(|e| format!("MySQL connect failed: {}", e))?;

    for col in columns {
        // 查询 information_schema.COLUMNS
        let rows = sqlx::query(
            "SELECT TABLE_NAME, COLUMN_NAME FROM INFORMATION_SCHEMA.COLUMNS \
             WHERE TABLE_SCHEMA = DATABASE() AND COLUMN_NAME = ?",
        )
        .bind(col)
        .fetch_all(&pool)
        .await
        .map_err(|e| format!("MySQL column lookup failed for '{}': {}", col, e))?;

        if rows.is_empty() {
            // 尝试检查是否是数据库函数（如 NOW, COUNT 等）
            if is_sql_function(col) {
                continue;
            }
            return Err(format!(
                "query! column verification failed: column '{}' not found in any table of the current database. \
                 SQL: {}",
                col,
                truncate_sql(sql, 120)
            ));
        }

        // 验证列至少存在于一个 FROM 表中（如果有表信息）
        if !tables.is_empty() {
            let found_in_table = rows.iter().any(|row| {
                let table_name: String = row.get("TABLE_NAME");
                tables.iter().any(|t| t == &table_name.to_lowercase())
            });
            if !found_in_table {
                let available: Vec<String> = rows.iter().map(|r| r.get("TABLE_NAME")).collect();
                return Err(format!(
                    "query! column verification failed: column '{}' exists but not in FROM table(s) {:?}. \
                     Found in: {:?}. SQL: {}",
                    col,
                    tables,
                    available,
                    truncate_sql(sql, 120)
                ));
            }
        }
    }

    Ok(())
}

#[cfg(feature = "db-verify")]
async fn verify_columns_postgres(
    dsn: &str,
    tables: &[String],
    columns: &[String],
    sql: &str,
) -> Result<(), String> {
    let pool = sqlx::PgPool::connect(dsn)
        .await
        .map_err(|e| format!("PostgreSQL connect failed: {}", e))?;

    for col in columns {
        let rows = sqlx::query(
            "SELECT TABLE_NAME, COLUMN_NAME FROM INFORMATION_SCHEMA.COLUMNS \
             WHERE TABLE_CATALOG = CURRENT_CATALOG AND COLUMN_NAME = $1",
        )
        .bind(col)
        .fetch_all(&pool)
        .await
        .map_err(|e| format!("PostgreSQL column lookup failed for '{}': {}", col, e))?;

        if rows.is_empty() && !is_sql_function(col) {
            return Err(format!(
                "query! column verification failed: column '{}' not found in any table of the current database. \
                 SQL: {}",
                col,
                truncate_sql(sql, 120)
            ));
        }

        if !tables.is_empty() && !rows.is_empty() {
            let found_in_table = rows.iter().any(|row| {
                let table_name: String = row.get("TABLE_NAME");
                tables.iter().any(|t| t == &table_name.to_lowercase())
            });
            if !found_in_table {
                let available: Vec<String> = rows.iter().map(|r| r.get("TABLE_NAME")).collect();
                return Err(format!(
                    "query! column verification failed: column '{}' exists but not in FROM table(s) {:?}. \
                     Found in: {:?}. SQL: {}",
                    col, tables, available,
                    truncate_sql(sql, 120)
                ));
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// 列类型获取（P0-2）
//
// 获取 SELECT 列的实际 DB 类型（列名 → 类型名），由 `query_as!`/`query!(T, ...)`
// 宏嵌入到生成的编译期验证代码中：用户代码在 const 上下文中将实际类型与
// 结构体 `__sz_orm_column_types()` 期望值对比，不匹配即编译失败。
// ---------------------------------------------------------------------------

/// 获取 SELECT 列的实际 DB 类型列表 `(列名, DATA_TYPE/udt_name)`。
///
/// 仅 MySQL/PostgreSQL 支持（SQLite/Oracle/SQL Server 返回空列表，跳过类型级验证）。
#[cfg(feature = "db-verify")]
async fn fetch_column_types(
    dsn: &str,
    db_kind: DbKind,
    sql: &str,
) -> Result<Vec<(String, String)>, String> {
    if !matches!(db_kind, DbKind::MySql | DbKind::Postgres) {
        return Ok(Vec::new());
    }

    let tables = extract_tables(sql);
    let columns = extract_columns(sql);
    if tables.is_empty() || columns.is_empty() {
        return Ok(Vec::new());
    }

    match db_kind {
        DbKind::MySql => fetch_column_types_mysql(dsn, &tables, &columns).await,
        DbKind::Postgres => fetch_column_types_postgres(dsn, &tables, &columns).await,
        _ => Ok(Vec::new()),
    }
}

#[cfg(feature = "db-verify")]
async fn fetch_column_types_mysql(
    dsn: &str,
    tables: &[String],
    columns: &[String],
) -> Result<Vec<(String, String)>, String> {
    let pool = sqlx::MySqlPool::connect(dsn)
        .await
        .map_err(|e| format!("MySQL connect failed for type fetch: {}", e))?;

    let mut result = Vec::new();
    for col in columns {
        let rows = sqlx::query(
            "SELECT TABLE_NAME, COLUMN_NAME, DATA_TYPE \
             FROM INFORMATION_SCHEMA.COLUMNS \
             WHERE TABLE_SCHEMA = DATABASE() AND COLUMN_NAME = ?",
        )
        .bind(col)
        .fetch_all(&pool)
        .await
        .map_err(|e| format!("MySQL type lookup failed for '{}': {}", col, e))?;

        // 取 FROM 表中的类型（多表同名列时取第一个匹配）
        let ty = rows
            .iter()
            .find(|row| {
                let tn: String = row.get("TABLE_NAME");
                tables.iter().any(|t| t == &tn.to_lowercase())
            })
            .and_then(|r| r.try_get::<String, _>("DATA_TYPE").ok());
        if let Some(ty) = ty {
            result.push((col.to_lowercase(), ty));
        }
    }
    Ok(result)
}

#[cfg(feature = "db-verify")]
async fn fetch_column_types_postgres(
    dsn: &str,
    tables: &[String],
    columns: &[String],
) -> Result<Vec<(String, String)>, String> {
    let pool = sqlx::PgPool::connect(dsn)
        .await
        .map_err(|e| format!("PostgreSQL connect failed for type fetch: {}", e))?;

    let mut result = Vec::new();
    for col in columns {
        let rows = sqlx::query(
            "SELECT TABLE_NAME, COLUMN_NAME, udt_name \
             FROM INFORMATION_SCHEMA.COLUMNS \
             WHERE TABLE_CATALOG = CURRENT_CATALOG AND COLUMN_NAME = $1",
        )
        .bind(col)
        .fetch_all(&pool)
        .await
        .map_err(|e| format!("PostgreSQL type lookup failed for '{}': {}", col, e))?;

        let ty = rows
            .iter()
            .find(|row| {
                let tn: String = row.get("TABLE_NAME");
                tables.iter().any(|t| t == &tn.to_lowercase())
            })
            .and_then(|r| r.try_get::<String, _>("udt_name").ok());
        if let Some(ty) = ty {
            result.push((col.to_lowercase(), ty));
        }
    }
    Ok(result)
}

/// 生成编译期类型验证代码块：`{ const _: () = { ...检查... }; <查询表达式> }`。
///
/// 验证逻辑在 const 上下文中执行（`panic!` 触发即编译失败，实现真正的编译期拦截）：
/// 1. 列数必须与结构体字段数一致；
/// 2. 每个 SELECT 列名必须存在于结构体字段中（与 `__sz_orm_column_types()` 对比）；
/// 3. 每个列的实际 DB 类型必须与结构体字段类型兼容（`__sz_orm_const_types_compatible`）。
///
/// `record_type` 为 `query_as!` 第一个参数（如 `User` / `crate::User`），
/// 生成的代码通过 `<记录类型>::__sz_orm_column_types()` 引用 derive 宏生成的
/// const fn（因此记录类型必须 `#[derive(FromQueryResult)]`）。
#[cfg(feature = "db-verify")]
fn gen_compile_time_type_check(
    record_type: &str,
    sql: &str,
    cols: &[(String, String)],
    query_expr: &str,
) -> String {
    let n = cols.len();
    let sql_esc = sql.escape_default().to_string();
    let mut checks = String::new();
    checks.push_str(&format!(
        "if exp.len() != {} {{ panic!(\"sz-orm compile-time type check failed for `{}`: SELECT returns {} columns but struct field count differs\"); }}",
        n, sql_esc, n
    ));
    for (i, (name, ty)) in cols.iter().enumerate() {
        let name_esc = name.escape_default().to_string();
        let ty_esc = ty.escape_default().to_string();
        checks.push_str(&format!(
            "if !::sz_orm_core::__sz_orm_const_str_eq(exp[{}].0, \"{}\") {{ panic!(\"sz-orm compile-time type check failed for `{}`: SELECT column #{} `{}` not found in struct fields\"); }}",
            i, name_esc, sql_esc, i, name_esc
        ));
        checks.push_str(&format!(
            "if !::sz_orm_core::__sz_orm_const_types_compatible(\"{}\", exp[{}].1) {{ panic!(\"sz-orm compile-time type check failed for `{}`: column `{}` type mismatch (db type `{}` not compatible with struct field type)\"); }}",
            ty_esc, i, sql_esc, name_esc, ty_esc
        ));
    }
    format!(
        "{{ const _: () = {{ let exp = <{}>::__sz_orm_column_types(); {} }}; {} }}",
        record_type, checks, query_expr
    )
}

/// 常见 SQL 函数名列表（不应作为列名验证）
#[cfg(feature = "db-verify")]
fn is_sql_function(name: &str) -> bool {
    matches!(
        name.to_uppercase().as_str(),
        "NOW"
            | "CURRENT_TIMESTAMP"
            | "CURRENT_DATE"
            | "CURRENT_TIME"
            | "COUNT"
            | "SUM"
            | "AVG"
            | "MIN"
            | "MAX"
            | "COALESCE"
            | "NULLIF"
            | "CAST"
            | "CONVERT"
            | "IFNULL"
            | "NVL"
            | "UPPER"
            | "LOWER"
            | "LENGTH"
            | "TRIM"
            | "SUBSTRING"
            | "CONCAT"
            | "REPLACE"
            | "ROUND"
            | "CEIL"
            | "FLOOR"
            | "ABS"
            | "MOD"
            | "POWER"
            | "SQRT"
            | "LOG"
            | "EXP"
            | "DATE"
            | "YEAR"
            | "MONTH"
            | "DAY"
            | "HOUR"
            | "MINUTE"
            | "SECOND"
            | "NOW()"
            | "UUID"
            | "RANDOM"
            | "MD5"
            | "TRUE"
            | "FALSE"
            | "NULL"
    )
}

/// Oracle 编译期验证：通过 sqlplus 命令行工具执行 EXPLAIN PLAN FOR
///
/// DSN 格式：`oracle://user:pass@host:port/service`（可选 `?sysdba=1`）
/// 例如：`oracle://sys:test123@127.0.0.1:1521/freepdb1.FALSE?sysdba=1`
#[cfg(feature = "db-verify")]
fn verify_oracle(dsn: &str, explain_sql: &str) -> Result<(), String> {
    let parsed = parse_oracle_dsn(dsn)?;
    // 构造 sqlplus 连接串：user/pass@host:port/service [AS SYSDBA]
    let mut conn_str = format!(
        "{}/{}@{}:{}/{}",
        parsed.user, parsed.password, parsed.host, parsed.port, parsed.service
    );
    if parsed.sysdba {
        conn_str.push_str(" AS SYSDBA");
    }
    // 用 SET SHOWPLAN 不适用于 Oracle，用 EXPLAIN PLAN FOR 并立即查询 PLAN_TABLE
    let full_script = format!(
        "SET HEADING OFF FEEDBACK OFF ECHO OFF;\n\
         EXPLAIN PLAN FOR {};\n\
         SELECT COUNT(*) FROM plan_table WHERE statement_id = (SELECT MAX(statement_id) FROM plan_table);\n\
         EXIT;\n",
        explain_sql
    );
    let output = std::process::Command::new("sqlplus")
        .args(["-S", "-L", &conn_str])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("sqlplus not found (Oracle client required): {}", e))?;
    use std::io::Write;
    let mut child = output;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(full_script.as_bytes())
            .map_err(|e| format!("sqlplus stdin write failed: {}", e))?;
    }
    let out = child
        .wait_with_output()
        .map_err(|e| format!("sqlplus wait failed: {}", e))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    if !out.status.success() || stdout.contains("ORA-") || stdout.contains("SP2-") {
        return Err(format!(
            "Oracle EXPLAIN failed: stdout={} stderr={}",
            stdout.trim(),
            stderr.trim()
        ));
    }
    Ok(())
}

/// SQL Server 编译期验证：通过 sqlcmd 命令行工具执行 SET SHOWPLAN_TEXT ON
///
/// DSN 格式：`sqlserver://user:pass@host:port/db`
/// 例如：`sqlserver://test:JkbC2jsaWAYDe2Gz@sh-mssql-adrul9nm.sql.tencentcdb.com:22527/test`
#[cfg(feature = "db-verify")]
fn verify_sqlserver(dsn: &str, explain_sql: &str) -> Result<(), String> {
    let parsed = parse_sqlserver_dsn(dsn)?;
    // sqlcmd -S host,port -U user -P pass -d db -Q "SET SHOWPLAN_TEXT ON; <sql>"
    let query = format!("SET SHOWPLAN_TEXT ON;\n{}", explain_sql);
    let out = std::process::Command::new("sqlcmd")
        .args([
            "-S",
            &format!("{},{}", parsed.host, parsed.port),
            "-U",
            &parsed.user,
            "-P",
            &parsed.password,
            "-d",
            &parsed.database,
            "-Q",
            &query,
            "-h",
            "-1",
            "-W",
        ])
        .output()
        .map_err(|e| format!("sqlcmd not found (SQL Server client required): {}", e))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    if !out.status.success() || stdout.contains("Msg ") || stdout.contains("Level ") {
        return Err(format!(
            "SQL Server SHOWPLAN failed: stdout={} stderr={}",
            stdout.trim(),
            stderr.trim()
        ));
    }
    Ok(())
}

/// Oracle DSN 解析结果
#[cfg(feature = "db-verify")]
struct OracleDsn {
    user: String,
    password: String,
    host: String,
    port: u16,
    service: String,
    sysdba: bool,
}

/// 解析 oracle://user:pass@host:port/service?sysdba=1
#[cfg(feature = "db-verify")]
fn parse_oracle_dsn(dsn: &str) -> Result<OracleDsn, String> {
    let raw = dsn
        .strip_prefix("oracle://")
        .or_else(|| dsn.strip_prefix("oracle:"))
        .ok_or_else(|| format!("Invalid Oracle DSN: {}", dsn))?;
    // 分离 query
    let (auth_host_service, query) = match raw.find('?') {
        Some(idx) => (&raw[..idx], &raw[idx + 1..]),
        None => (raw, ""),
    };
    let sysdba = query
        .split('&')
        .any(|p| p == "sysdba=1" || p == "sysdba=true");
    // user:pass@host:port/service
    let at = auth_host_service
        .find('@')
        .ok_or_else(|| format!("Oracle DSN missing '@': {}", dsn))?;
    let (user_pass, host_port_service) = (&auth_host_service[..at], &auth_host_service[at + 1..]);
    let colon = user_pass
        .find(':')
        .ok_or_else(|| format!("Oracle DSN missing password separator: {}", dsn))?;
    let (user, password) = (&user_pass[..colon], &user_pass[colon + 1..]);
    let (host_port, service) = match host_port_service.rfind('/') {
        Some(idx) => (&host_port_service[..idx], &host_port_service[idx + 1..]),
        None => return Err(format!("Oracle DSN missing service name: {}", dsn)),
    };
    let (host, port) = match host_port.find(':') {
        Some(idx) => (
            &host_port[..idx],
            host_port[idx + 1..]
                .parse::<u16>()
                .map_err(|_| format!("Oracle DSN invalid port: {}", dsn))?,
        ),
        None => (host_port, 1521u16),
    };
    Ok(OracleDsn {
        user: user.to_string(),
        password: password.to_string(),
        host: host.to_string(),
        port,
        service: service.to_string(),
        sysdba,
    })
}

/// SQL Server DSN 解析结果
#[cfg(feature = "db-verify")]
struct SqlServerDsn {
    user: String,
    password: String,
    host: String,
    port: u16,
    database: String,
}

/// 解析 sqlserver://user:pass@host:port/db
#[cfg(feature = "db-verify")]
fn parse_sqlserver_dsn(dsn: &str) -> Result<SqlServerDsn, String> {
    let raw = dsn
        .strip_prefix("sqlserver://")
        .or_else(|| dsn.strip_prefix("mssql://"))
        .or_else(|| dsn.strip_prefix("tds://"))
        .ok_or_else(|| format!("Invalid SQL Server DSN: {}", dsn))?;
    let at = raw
        .find('@')
        .ok_or_else(|| format!("SQL Server DSN missing '@': {}", dsn))?;
    let (user_pass, host_port_db) = (&raw[..at], &raw[at + 1..]);
    let colon = user_pass
        .find(':')
        .ok_or_else(|| format!("SQL Server DSN missing password separator: {}", dsn))?;
    let (user, password) = (&user_pass[..colon], &user_pass[colon + 1..]);
    let (host_port, database) = match host_port_db.rfind('/') {
        Some(idx) => (&host_port_db[..idx], &host_port_db[idx + 1..]),
        None => return Err(format!("SQL Server DSN missing database: {}", dsn)),
    };
    let (host, port) = match host_port.find(':') {
        Some(idx) => (
            &host_port[..idx],
            host_port[idx + 1..]
                .parse::<u16>()
                .map_err(|_| format!("SQL Server DSN invalid port: {}", dsn))?,
        ),
        None => (host_port, 1433u16),
    };
    Ok(SqlServerDsn {
        user: user.to_string(),
        password: password.to_string(),
        host: host.to_string(),
        port,
        database: database.to_string(),
    })
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
                    type SqlType = <#rust_type as ::sz_orm_core::typed_ast::InferSqlType>::SqlType;
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
/// 类型化裸 SQL 查询宏（SQLx `query_as!` 风格）。
///
/// 用法：`query_as!(RecordType, "SELECT col1, col2 FROM table WHERE id = ?")`
///
/// 生成 `sz_orm_core::queryable::QueryAs::<RecordType>::new("SELECT ...")`。
/// 在 `db-verify` feature + `SZ_ORM_QUERY_VERIFY=1` 环境下会连真 DB
/// 执行 EXPLAIN 验证 SQL 合法性。
///
/// **运行时列名验证**（P0-2）：`QueryAs::fetch_all` 会比对 DB 返回的列名
/// 与 `RecordType::row_desc()`（由 `#[derive(FromQueryResult)]` 自动生成）。
/// 若 SQL SELECT 列不在 struct 字段中，返回 `DbError::QueryError`。
///
/// # 示例
///
/// ```ignore
/// #[derive(FromQueryResult)]
/// struct User { id: i64, name: String }
///
/// let q = query_as!(User, "SELECT id, name FROM users WHERE id = 1");
/// let users: Vec<User> = q.fetch_all(&mut conn).await?;
/// ```
pub fn query_as(input: TokenStream) -> TokenStream {
    let mut tokens = input.into_iter().peekable();

    // 解析记录类型（第一个标识符/路径，如 User 或 crate::User）
    let mut record_type = String::new();
    loop {
        match tokens.next() {
            Some(TokenTree::Ident(ident)) => {
                record_type.push_str(&ident.to_string());
            }
            Some(TokenTree::Punct(p)) if p.as_char() == ':' => {
                // 处理 :: 路径分隔符
                record_type.push_str("::");
                // 跳过第二个 :
                if let Some(TokenTree::Punct(p2)) = tokens.peek() {
                    if p2.as_char() == ':' {
                        let _ = tokens.next();
                    }
                }
            }
            Some(TokenTree::Punct(p)) if p.as_char() == ',' => break,
            Some(TokenTree::Punct(p)) if p.as_char() == ',' => break,
            Some(other) => {
                return compile_error(
                    other.span(),
                    "query_as! 第一个参数必须是记录类型，如 query_as!(User, \"SELECT ...\")",
                );
            }
            None => {
                return compile_error(
                    Span::call_site(),
                    "query_as! 需要两个参数：query_as!(RecordType, \"SELECT ...\")",
                );
            }
        }
    }

    // 解析 SQL 字符串字面量
    let sql_raw = match tokens.next() {
        Some(TokenTree::Literal(lit)) => lit.to_string(),
        Some(other) => {
            return compile_error(other.span(), "query_as! 第二个参数必须是 SQL 字符串字面量");
        }
        None => {
            return compile_error(
                Span::call_site(),
                "query_as! 需要两个参数：query_as!(RecordType, \"SELECT ...\")",
            );
        }
    };

    let sql_content = match strip_string_literal(&sql_raw) {
        Some(s) => s,
        None => {
            return compile_error(Span::call_site(), "query_as! 的 SQL 参数必须是字符串字面量");
        }
    };

    // 语法验证
    if let Err(err_msg) = validate_sql_content(sql_content, None) {
        return compile_error(Span::call_site(), &err_msg);
    }

    // db-verify 验证
    #[cfg(feature = "db-verify")]
    let verify_cols: Option<Vec<(String, String)>> = {
        match std::env::var("SZ_ORM_QUERY_VERIFY").ok().as_deref() {
            // 模式 1：连真 DB 执行 EXPLAIN 验证，并获取 SELECT 列的实际类型
            Some("1") => match verify_with_real_db(sql_content) {
                Ok(cols) => Some(cols),
                Err(err) => {
                    return compile_error(
                        Span::call_site(),
                        &format!("query_as! real DB verification failed: {}", err),
                    )
                }
            },
            // 模式 cache：从离线缓存文件查找（无需 DB，适合 CI）
            Some("cache") => {
                if let Err(err) = verify_with_cache(sql_content) {
                    return compile_error(
                        Span::call_site(),
                        &format!("query_as! offline cache verification failed: {}", err),
                    );
                }
                None
            }
            _ => None,
        }
    };
    #[cfg(not(feature = "db-verify"))]
    let _verify_cols: Option<Vec<(String, String)>> = None;

    // 生成 QueryAs::<T>::new("...")
    // db-verify 通过时，附加编译期类型验证块（P0-2）：
    // const 上下文中将 DB 实际列类型与结构体 __sz_orm_column_types() 对比，
    // 不匹配则 const panic → 编译失败。
    let escaped = sql_content.escape_default();
    let base = format!(
        "::sz_orm_core::queryable::QueryAs::<{}>::new(\"{}\")",
        record_type, escaped
    );
    #[cfg(feature = "db-verify")]
    let output = match &verify_cols {
        Some(cols) if !cols.is_empty() => {
            gen_compile_time_type_check(&record_type, sql_content, cols, &base)
        }
        _ => base,
    };
    #[cfg(not(feature = "db-verify"))]
    let output = base;
    output
        .parse()
        .unwrap_or_else(|_| compile_error(Span::call_site(), "Failed to generate query_as output"))
}

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
                    type SqlType = <#rust_type as ::sz_orm_core::typed_ast::InferSqlType>::SqlType;
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
        // 8 字节整数
        "BIGINT" | "INT8" => "i64",
        // 4 字节整数（INT/INTEGER/INT4/SERIAL）
        "INT" | "INTEGER" | "INT4" | "SERIAL" => "i32",
        // 2 字节整数
        "SMALLINT" | "INT2" | "SMALLSERIAL" => "i16",
        // 1 字节整数
        "TINYINT" => "i8",
        // 浮点（4 字节）
        "FLOAT" | "REAL" | "FLOAT4" => "f32",
        // 浮点（8 字节）/ 定点数
        "DOUBLE" | "DOUBLE PRECISION" | "FLOAT8" | "DECIMAL" | "NUMERIC" => "f64",
        // 布尔
        "BOOLEAN" | "BOOL" => "bool",
        // 二进制（与 schema_gen::sql_type_to_rust 保持一致）
        "BLOB" | "BYTEA" | "BINARY" | "VARBINARY" => "Vec<u8>",
        // 字符串/日期/JSON/UUID（统一映射到 String，运行时再解析）
        "VARCHAR" | "TEXT" | "CHAR" | "CHARACTER" | "CLOB" | "UUID" | "DATE" | "TIME"
        | "DATETIME" | "TIMESTAMP" | "JSON" | "JSONB" => "String",
        _ => "String",
    };

    if nullable {
        format!("Option<{}>", rust)
    } else {
        rust.to_string()
    }
}

// ---------------------------------------------------------------------------
// `#[derive(Schema)]` — auto-generate table structure from a struct
// ---------------------------------------------------------------------------

/// 派生宏：自动从 Rust 结构体生成表结构信息。
///
/// 解析 `#[table(name = "...")]` 和 `#[column(...)]` 属性，
/// 生成 `Schema` trait 实现，便于在运行时反射表名与列信息。
///
/// # 支持的属性
///
/// - `#[table(name = "users")]` — 指定表名（默认使用结构体名的蛇形形式）
/// - `#[column(name = "user_id")]` — 指定列名（默认使用字段名）
/// - `#[column(type = "VARCHAR(255)")]` — 指定 SQL 类型
/// - `#[column(primary_key)]` — 标记主键
/// - `#[column(nullable)]` — 显式标记允许 NULL
/// - `#[column(skip)]` — 跳过此字段，不生成 schema 条目
/// - `#[column(default = "0")]` — 标记字段有默认值
///
/// # 类型推断
///
/// 字段的 Rust 类型会自动映射为 SQL 类型：
/// - `i64`/`u64` → `BIGINT`
/// - `i32`/`u32` → `INTEGER`
/// - `String` → `TEXT`
/// - `f64` → `DOUBLE`
/// - `bool` → `BOOLEAN`
/// - `Vec<u8>` → `BLOB`
/// - `Option<T>` → 与 `T` 相同，但标记为 nullable
#[proc_macro_derive(Schema, attributes(table, column))]
pub fn derive_schema(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    derive::derive_schema_impl(input).into()
}

// ---------------------------------------------------------------------------
// `#[derive(GraphQLModel)]` — auto-generate `impl GraphQLModelInfo`
// ---------------------------------------------------------------------------

/// 派生宏：自动生成 `sz_orm_graphql::schema_gen::GraphQLModelInfo` 实现。
///
/// 从 `#[derive(GraphQLModel)]` 结构体提取字段元数据（字段名 + Rust 类型 + 可空性），
/// 供 `SchemaGenerator::from_model` 使用。零运行时开销。
///
/// # 支持的属性
///
/// - `#[table(name = "users")]` — 指定表名（默认使用结构体名的 snake_case）
/// - `#[column(skip)]` — 跳过此字段
/// - `#[column(name = "custom_name")]` — 指定列名
///
/// # 示例
///
/// ```ignore
/// use sz_orm_macros::GraphQLModel;
///
/// #[derive(GraphQLModel)]
/// #[table(name = "users")]
/// struct User {
///     id: i64,
///     name: String,
///     email: Option<String>,
/// }
/// ```
#[proc_macro_derive(GraphQLModel, attributes(table, column))]
pub fn derive_graphql_model(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    derive::derive_graphql_model_impl(input).into()
}

// ---------------------------------------------------------------------------
// `#[derive(Builder)]` — auto-generate builder pattern code
// ---------------------------------------------------------------------------

/// 派生宏：自动生成构造器模式代码。
///
/// 为目标结构体生成一个 `XxxBuilder` 类型，包含：
/// - `new()` 构造空 builder
/// - 每个字段的 setter 方法
/// - `build()` 方法返回 `Result<T, String>`
///
/// # 支持的属性
///
/// - `#[builder(skip)]` — 跳过此字段（不生成 setter，使用 Default）
/// - `#[builder(default = expr)]` — 指定默认值表达式
///
/// # 示例
///
/// ```ignore
/// use sz_orm_macros::Builder;
///
/// #[derive(Builder)]
/// struct User {
///     id: i64,
///     name: String,
/// }
///
/// let user = User::builder()
///     .id(1)
///     .name("Alice".to_string())
///     .build()
///     .unwrap();
/// ```
#[proc_macro_derive(Builder, attributes(builder))]
pub fn derive_builder(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    derive::derive_builder_impl(input).into()
}

// ---------------------------------------------------------------------------
// `#[derive(Entity)]` — auto-generate `impl Model for Struct`
// ---------------------------------------------------------------------------

/// 派生宏：自动生成 `sz_orm_core::Model` trait 实现。
///
/// 要求结构体恰好有一个 `#[column(primary_key)]` 字段，
/// 该字段的类型即为 `Model::PrimaryKey`。
///
/// # 支持的属性
///
/// - `#[table(name = "...")]` — 指定表名，默认蛇形结构体名
/// - `#[column(primary_key)]` — 标记主键字段（必需，恰好一个）
/// - `#[column(name = "...")]` — 覆盖主键列名（默认与字段名相同）
///
/// # 示例
///
/// ```ignore
/// use sz_orm_macros::Entity;
///
/// #[derive(Entity)]
/// #[table(name = "users")]
/// struct User {
///     #[column(primary_key)]
///     id: i64,
///     name: String,
/// }
///
/// assert_eq!(User::table_name(), "users");
/// assert_eq!(User::pk_name(), "id");
/// ```
#[proc_macro_derive(Entity, attributes(table, column))]
pub fn derive_entity(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    derive::derive_entity_impl(input).into()
}

// ---------------------------------------------------------------------------
// `#[derive(FromQueryResult)]` — auto-generate `impl FromQueryResult for Struct`
// ---------------------------------------------------------------------------

/// 派生宏：自动生成 `sz_orm_core::FromQueryResult` trait 实现。
///
/// 从查询结果行（`HashMap<String, Value>`）反序列化为结构体实例。
/// `Option<T>` 字段在列缺失或值为 NULL 时自动返回 `None`。
///
/// # 支持的属性
///
/// - `#[column(name = "...")]` — 覆盖列名映射（默认使用字段名）
///
/// # 示例
///
/// ```ignore
/// use sz_orm_macros::FromQueryResult;
///
/// #[derive(FromQueryResult)]
/// struct UserRow {
///     id: i64,
///     name: String,
///     #[column(name = "user_email")]
///     email: Option<String>,
/// }
/// ```
#[proc_macro_derive(FromQueryResult, attributes(column))]
pub fn derive_from_query_result(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    derive::derive_from_query_result_impl(input).into()
}

// ---------------------------------------------------------------------------
// `#[derive(ColumnEnum)]` — auto-generate column name enum (P2-2)
// ---------------------------------------------------------------------------

/// 派生宏：从结构体字段自动生成 `<StructName>Column` 列名枚举（P2-2）。
///
/// 每个字段生成一个变体（snake_case → CamelCase），通过 `ColumnTrait::as_str()`
/// 返回数据库列名；`#[column(name = "...")]` 可覆盖列名（与 FromQueryResult 一致）。
/// 同时实现 `std::fmt::Display`。
///
/// # 示例
///
/// ```rust,ignore
/// use sz_orm_macros::ColumnEnum;
/// use sz_orm_core::ColumnTrait;
///
/// #[derive(ColumnEnum)]
/// struct User {
///     id: i64,
///     #[column(name = "user_name")]
///     name: String,
/// }
///
/// assert_eq!(UserColumn::Id.as_str(), "id");
/// assert_eq!(UserColumn::Name.as_str(), "user_name");
/// assert_eq!(UserColumn::Id.to_string(), "id");
/// ```
#[proc_macro_derive(ColumnEnum, attributes(column))]
pub fn derive_column_enum(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    derive::derive_column_enum_impl(input).into()
}

// ---------------------------------------------------------------------------
// `#[derive(FromRow)]` — auto-generate `impl FromRow for Struct`
// ---------------------------------------------------------------------------

/// 派生宏：自动生成 `sz_orm_core::queryable::FromRow` trait 实现。
///
/// 从 `HashMap<String, Value>` 按列名反序列化为结构体实例。
/// 与 `FromQueryResult` 的区别在于错误类型为 `QueryError`（含列信息），
/// 适合需要精确错误定位的底层场景。
///
/// # 支持的属性
///
/// - `#[column(name = "...")]` — 覆盖列名映射（默认使用字段名）
///
/// # 示例
///
/// ```ignore
/// use sz_orm_macros::FromRow;
///
/// #[derive(FromRow)]
/// struct User {
///     id: i64,
///     name: String,
///     #[column(name = "user_email")]
///     email: Option<String>,
/// }
/// ```
#[proc_macro_derive(FromRow, attributes(column))]
pub fn derive_from_row(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    derive::derive_from_row_impl(input).into()
}

// ---------------------------------------------------------------------------
// `#[derive(SqlType)]` — auto-generate `impl FromQueryResult + to_value()` for enums
// ---------------------------------------------------------------------------

/// 派生宏：为 Rust 枚举自动生成 `sz_orm_core::FromQueryResult` trait 实现
/// 和 `to_value()` 方法。
///
/// 这是 sz-orm 对 SQLx `#[derive(Type)]` 的等效实现：
/// 让自定义枚举可以直接用于查询结果的字段映射和查询参数的绑定。
///
/// # 支持的属性
///
/// - `#[sql_type(rename_all = "snake_case")]` — 控制变体名的序列化格式
///   （snake_case / SCREAMING_SNAKE_CASE / camelCase / PascalCase / lowercase / UPPERCASE）
/// - `#[sql_type(rename = "...")]`（变体级）— 覆盖单个变体的序列化名
///
/// # 示例
///
/// ```ignore
/// use sz_orm_macros::SqlType;
///
/// #[derive(SqlType)]
/// enum Status {
///     Active,    // → "active"
///     Inactive,  // → "inactive"
/// }
///
/// let v = Status::Active.to_value();  // Value::String("active")
/// ```
#[proc_macro_derive(SqlType, attributes(sql_type))]
pub fn derive_sql_type(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    derive::derive_sql_type_impl(input).into()
}

// ---------------------------------------------------------------------------
// `#[derive(Relation)]` — auto-generate `impl ModelExt` with relations()
// ---------------------------------------------------------------------------

/// 派生宏：自动生成 `sz_orm_core::model::ModelExt` trait 实现，
/// 填充 `relations()` 映射，消除手写关系样板代码。
///
/// # 支持的属性
///
/// - `#[relation(has_many = "orders", fk = "user_id", pk = "id")]`
/// - `#[relation(belongs_to = "users", fk = "user_id", pk = "id")]`
/// - `#[relation(has_one = "profile", fk = "user_id", pk = "id")]`
/// - `#[relation(belongs_to_many = "roles", junction = "user_roles", fk = "user_id", other_key = "role_id", target = "roles", target_pk = "id")]`
/// - `#[relation(morph_many = "comments", morph_type = "commentable_type", morph_id = "commentable_id", morph_type_value = "Post")]`
/// - `#[relation(morph_to, morph_type = "commentable_type", morph_id = "commentable_id")]`
///
/// # 示例
///
/// ```ignore
/// use sz_orm_macros::{Entity, Relation};
///
/// #[derive(Entity, Relation)]
/// #[table(name = "users")]
/// struct User {
///     #[column(primary_key)]
///     id: i64,
/// }
///
/// // 自动生成：
/// // impl ModelExt for User {
/// //     fn relations() -> HashMap<&str, Relation> {
/// //         // 包含 #[relation] 定义的关系
/// //     }
/// // }
/// ```
#[proc_macro_derive(Relation, attributes(relation, table, column))]
pub fn derive_relation(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    derive::derive_relation_impl(input).into()
}

/// `#[derive(RelationTrait)]` — 自动生成 `RelationTrait` 实现（P-F-2, v2.1.0）
///
/// 从 `#[relation(...)]` 属性生成 `RelationDef` 静量表 + `impl RelationTrait`。
/// 与 `#[derive(Relation)]` 共享属性解析，但生成零分配的静态切片而非 `HashMap`。
///
/// # 示例
///
/// ```ignore
/// #[derive(RelationTrait)]
/// #[relation(has_many = "Order", fk = "user_id", pk = "id")]
/// struct User { id: i64, name: String }
///
/// // 自动生成：
/// // static RELATIONS: &[RelationDef] = &[RelationDef::new("Order", "users", "orders", "id", "user_id", HasMany)];
/// // impl RelationTrait for User { ... }
/// ```
#[proc_macro_derive(RelationTrait, attributes(relation, table, column))]
pub fn derive_relation_trait(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    derive::derive_relation_trait_impl(input).into()
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
    fn test_detect_db_kind_oracle() {
        assert_eq!(
            detect_db_kind("oracle://sys:test123@127.0.0.1:1521/freepdb1.FALSE?sysdba=1").unwrap(),
            DbKind::Oracle
        );
        assert_eq!(
            detect_db_kind("oracle:sys:test123@127.0.0.1:1521/FREE").unwrap(),
            DbKind::Oracle
        );
    }

    #[cfg(feature = "db-verify")]
    #[test]
    fn test_detect_db_kind_sqlserver() {
        assert_eq!(
            detect_db_kind("sqlserver://test:pass@host:1433/db").unwrap(),
            DbKind::SqlServer
        );
        assert_eq!(
            detect_db_kind("mssql://test:pass@host:1433/db").unwrap(),
            DbKind::SqlServer
        );
        assert_eq!(
            detect_db_kind("tds://test:pass@host:1433/db").unwrap(),
            DbKind::SqlServer
        );
    }

    #[cfg(feature = "db-verify")]
    #[test]
    fn test_detect_db_kind_unsupported() {
        assert!(detect_db_kind("redis://user:pass@host/db").is_err());
        assert!(detect_db_kind("not-a-url").is_err());
    }

    #[cfg(feature = "db-verify")]
    #[test]
    fn test_parse_oracle_dsn_basic() {
        let dsn = "oracle://sys:test123@127.0.0.1:1521/freepdb1.FALSE?sysdba=1";
        let p = parse_oracle_dsn(dsn).unwrap();
        assert_eq!(p.user, "sys");
        assert_eq!(p.password, "test123");
        assert_eq!(p.host, "127.0.0.1");
        assert_eq!(p.port, 1521);
        assert_eq!(p.service, "freepdb1.FALSE");
        assert!(p.sysdba);
    }

    #[cfg(feature = "db-verify")]
    #[test]
    fn test_parse_oracle_dsn_default_port() {
        // 无端口号时默认 1521
        let dsn = "oracle://sys:test123@127.0.0.1/FREE";
        let p = parse_oracle_dsn(dsn).unwrap();
        assert_eq!(p.port, 1521);
        assert_eq!(p.service, "FREE");
        assert!(!p.sysdba);
    }

    #[cfg(feature = "db-verify")]
    #[test]
    fn test_parse_sqlserver_dsn_basic() {
        let dsn =
            "sqlserver://test:JkbC2jsaWAYDe2Gz@sh-mssql-adrul9nm.sql.tencentcdb.com:22527/test";
        let p = parse_sqlserver_dsn(dsn).unwrap();
        assert_eq!(p.user, "test");
        assert_eq!(p.password, "JkbC2jsaWAYDe2Gz");
        assert_eq!(p.host, "sh-mssql-adrul9nm.sql.tencentcdb.com");
        assert_eq!(p.port, 22527);
        assert_eq!(p.database, "test");
    }

    #[cfg(feature = "db-verify")]
    #[test]
    fn test_parse_sqlserver_dsn_default_port() {
        let dsn = "mssql://user:pass@host/db";
        let p = parse_sqlserver_dsn(dsn).unwrap();
        assert_eq!(p.port, 1433);
        assert_eq!(p.database, "db");
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
                ("id".to_string(), "i32".to_string()),
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
        assert_eq!(cols[0], ("a".to_string(), "i32".to_string()));
        assert_eq!(cols[1], ("b".to_string(), "Option<i32>".to_string()));
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
        // 整数（按字节宽度严格映射，与 SQL 标准一致）
        assert_eq!(sql_type_to_rust("BIGINT", false), "i64");
        assert_eq!(sql_type_to_rust("INT8", false), "i64");
        assert_eq!(sql_type_to_rust("INT", false), "i32");
        assert_eq!(sql_type_to_rust("INTEGER", false), "i32");
        assert_eq!(sql_type_to_rust("INT4", false), "i32");
        assert_eq!(sql_type_to_rust("SERIAL", false), "i32");
        assert_eq!(sql_type_to_rust("SMALLINT", false), "i16");
        assert_eq!(sql_type_to_rust("INT2", false), "i16");
        assert_eq!(sql_type_to_rust("SMALLSERIAL", false), "i16");
        assert_eq!(sql_type_to_rust("TINYINT", false), "i8");
        // 浮点
        assert_eq!(sql_type_to_rust("FLOAT", false), "f32");
        assert_eq!(sql_type_to_rust("REAL", false), "f32");
        assert_eq!(sql_type_to_rust("FLOAT4", false), "f32");
        assert_eq!(sql_type_to_rust("DOUBLE", false), "f64");
        assert_eq!(sql_type_to_rust("DOUBLE PRECISION", false), "f64");
        assert_eq!(sql_type_to_rust("FLOAT8", false), "f64");
        assert_eq!(sql_type_to_rust("DECIMAL", false), "f64");
        assert_eq!(sql_type_to_rust("NUMERIC", false), "f64");
        // 布尔
        assert_eq!(sql_type_to_rust("BOOLEAN", false), "bool");
        assert_eq!(sql_type_to_rust("BOOL", false), "bool");
        // 字符串
        assert_eq!(sql_type_to_rust("VARCHAR", false), "String");
        assert_eq!(sql_type_to_rust("TEXT", false), "String");
        assert_eq!(sql_type_to_rust("CHAR", false), "String");
        assert_eq!(sql_type_to_rust("UUID", false), "String");
        assert_eq!(sql_type_to_rust("DATE", false), "String");
        assert_eq!(sql_type_to_rust("DATETIME", false), "String");
        assert_eq!(sql_type_to_rust("TIMESTAMP", false), "String");
        assert_eq!(sql_type_to_rust("JSON", false), "String");
        assert_eq!(sql_type_to_rust("JSONB", false), "String");
        // 二进制
        assert_eq!(sql_type_to_rust("BLOB", false), "Vec<u8>");
        assert_eq!(sql_type_to_rust("BYTEA", false), "Vec<u8>");
        assert_eq!(sql_type_to_rust("BINARY", false), "Vec<u8>");
        assert_eq!(sql_type_to_rust("VARBINARY", false), "Vec<u8>");
        // nullable
        assert_eq!(sql_type_to_rust("INT", true), "Option<i32>");
        assert_eq!(sql_type_to_rust("BIGINT", true), "Option<i64>");
        assert_eq!(sql_type_to_rust("VARCHAR", true), "Option<String>");
        assert_eq!(sql_type_to_rust("BLOB", true), "Option<Vec<u8>>");
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

    // -----------------------------------------------------------------------
    // Gap 1 测试：列名/表名提取
    // -----------------------------------------------------------------------

    #[cfg(feature = "db-verify")]
    #[test]
    fn test_extract_tables_simple() {
        let tables = extract_tables("SELECT id, name FROM users WHERE id = ?");
        assert!(tables.contains(&"users".to_string()));
    }

    #[cfg(feature = "db-verify")]
    #[test]
    fn test_extract_tables_multiple() {
        let tables = extract_tables(
            "SELECT u.id, o.total FROM users u JOIN orders o ON u.id = o.user_id WHERE u.id = ?",
        );
        assert!(tables.contains(&"users".to_string()));
        assert!(tables.contains(&"orders".to_string()));
    }

    #[cfg(feature = "db-verify")]
    #[test]
    fn test_extract_columns_select_and_where() {
        let cols =
            extract_columns("SELECT id, name FROM users WHERE email = ? ORDER BY created_at");
        // id, name from SELECT; email from WHERE; created_at from ORDER BY
        assert!(cols.contains(&"id".to_string()));
        assert!(cols.contains(&"name".to_string()));
        assert!(cols.contains(&"email".to_string()));
        assert!(cols.contains(&"created_at".to_string()));
    }

    #[cfg(feature = "db-verify")]
    #[test]
    fn test_extract_columns_skips_keywords() {
        let cols = extract_columns("SELECT COUNT(id), name FROM users WHERE status = ?");
        // COUNT is a function, should be skipped
        assert!(!cols.contains(&"count".to_string()));
        assert!(cols.contains(&"id".to_string()));
        assert!(cols.contains(&"name".to_string()));
        assert!(cols.contains(&"status".to_string()));
    }

    #[cfg(feature = "db-verify")]
    #[test]
    fn test_is_sql_function() {
        assert!(is_sql_function("COUNT"));
        assert!(is_sql_function("now"));
        assert!(is_sql_function("COALESCE"));
        assert!(!is_sql_function("name"));
        assert!(!is_sql_function("user_id"));
    }
}
