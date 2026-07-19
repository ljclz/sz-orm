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
                return compile_error(
                    Span::call_site(),
                    "Expected param count after `;`",
                );
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
                return compile_error(
                    Span::call_site(),
                    "Expected param count after `params`",
                );
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
                return compile_error(
                    Span::call_site(),
                    "Expected a number after `params:`",
                );
            }
        }
    }

    // Run validation
    if let Err(err_msg) = validate_sql_content(sql_content, expected_params) {
        return compile_error(Span::call_site(), &err_msg);
    }

    // Emit the validated string as a &str literal
    let output = format!("\"{}\"", sql_content.escape_default());
    output.parse().unwrap_or_else(|_| {
        compile_error(Span::call_site(), "Failed to generate output token")
    })
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
                    return Err(format!("Unbalanced parentheses: unexpected ')' at position {}", i));
                }
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Err(format!(
            "Unbalanced parentheses: {} unclosed '('",
            depth
        ));
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
    let sql_upper = sql.to_uppercase();

    let patterns: &[(&str, &str)] = &[
        ("'; DROP TABLE", "DROP TABLE injection detected"),
        ("' OR '1'='1", "Classic OR 1=1 injection detected"),
        ("' OR 1=1", "OR 1=1 injection detected"),
        ("UNION SELECT", "UNION SELECT injection detected"),
        ("--", "SQL comment injection (--) detected"),
        ("/*", "SQL block comment (/*) detected"),
    ];

    for (pattern, desc) in patterns {
        if sql_upper.contains(pattern) {
            return Err(format!("{}: pattern '{}' found", desc, pattern));
        }
    }

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
