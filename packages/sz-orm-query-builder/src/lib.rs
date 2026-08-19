//! # SZ-ORM QueryBuilder — Standalone SQL Builder (sea-query style)
//!
//! A pure SQL builder that does not bind to Model, can be compiled independently and published to crates.io.
//!
//! Design inspired by [sea-query](https://crates.io/crates/sea-query):
//! - Decoupled from ORM: does not depend on `Model` trait, pure SQL construction
//! - Multi-dialect support: adapts MySQL/PostgreSQL/SQLite/Oracle via [`DbType`]
//! - Chainable API: all methods return `Self`
//! - Zero runtime overhead: construction process requires zero database connections
//!
//! # Quick Start
//!
//! ```rust
//! use sz_orm_core::DbType;
//! use sz_orm_query_builder::{Query, SelectQuery};
//!
//! // SELECT
//! let sql = Query::select()
//!     .column("id")
//!     .column("name")
//!     .from("users")
//!     .where_clause("age > 18")
//!     .order_by("id", true)
//!     .limit(10)
//!     .build(DbType::MySQL);
//! assert!(sql.contains("SELECT"));
//! assert!(sql.contains("FROM `users`"));
//!
//! // INSERT
//! let sql = Query::insert()
//!     .into_table("users")
//!     .value("name", "'Alice'")
//!     .value("age", "30")
//!     .build();
//! assert!(sql.contains("INSERT INTO `users`"));
//!
//! // UPDATE
//! let sql = Query::update()
//!     .table("users")
//!     .set("name", "'Bob'")
//!     .where_clause("id = 1")
//!     .build();
//! assert!(sql.contains("UPDATE `users`"));
//!
//! // DELETE
//! let sql = Query::delete()
//!     .from_table("users")
//!     .where_clause("id = 1")
//!     .build();
//! assert!(sql.contains("DELETE FROM"));
//! ```
//!
//! # Differences from sz-orm-core::QueryBuilder
//!
//! | Feature | `sz-orm-core::QueryBuilder<M>` | sz-orm-query-builder::Query |
//! |---------|------------------------------|----------------------------|
//! | Binds Model | Yes (`<M: Model>`) | No |
//! | Type safety | Compile-time table/column validation | Runtime strings |
//! | Applicable scenarios | Full ORM workflow | Pure SQL construction, dynamic queries |
//! | Dependencies | All of sz-orm-core | Only dialect module |
//! | Independent publish | No | Yes |

use sz_orm_core::{DbType, Value};

// ============================================================================
// BuiltQuery — 参数化查询构造结果
// ============================================================================

/// Parameterized query construction result
///
/// Contains a SQL string with `?` placeholders and a list of parameters bound in order.
///
/// # Security (P0 fix: SQL injection prevention)
///
/// Through parameterized binding, user input is separated from the SQL structure,
/// eliminating SQL injection at the root. Callers should use `build_with_params()`
/// instead of string-concatenated `where_clause()`, especially when WHERE condition
/// values come from untrusted input.
#[derive(Debug, Clone, Default)]
pub struct BuiltQuery {
    /// SQL statement with `?` placeholders
    pub sql: String,
    /// List of parameters bound in order of appearance
    pub params: Vec<Value>,
}

impl BuiltQuery {
    /// Extract the SQL and parameters parts
    pub fn into_parts(self) -> (String, Vec<Value>) {
        (self.sql, self.params)
    }
}

/// Parameterized WHERE condition (internal representation)
///
/// Compared to the original string concatenation of `wheres: Vec<String>`, this structure
/// separates the SQL template (with `?`) from parameter values, fundamentally avoiding SQL injection.
///
/// # Dialect-aware quoting (v1.0.1 fix)
///
/// `column` stores the unquoted column name. At `build_with_params` time, it is quoted
/// via `dialect.quote()` for the target dialect (PostgreSQL double quotes, MySQL backticks),
/// avoiding premature hard-coding with backticks at the call site which would cause
/// PostgreSQL dialect tests to fail.
#[derive(Debug, Clone)]
enum ParamWhere {
    /// `AND <expr>` condition
    And {
        /// Unquoted column name (empty string represents a trivial expression like `1 = 0`)
        column: String,
        /// Operator + placeholder part (e.g., ` = ?`, ` IN (?, ?)`, `1 = 0`)
        op: String,
        /// Parameter values bound in order of appearance
        values: Vec<Value>,
    },
    /// `OR <expr>` condition
    Or {
        column: String,
        op: String,
        values: Vec<Value>,
    },
}

/// Wrap an identifier with backticks and escape internal backticks (MySQL standard: ` → ``)
///
/// # Security (gate 9 fix)
///
/// Unescaped backtick wrapping allows malicious identifiers to break out via ` injection.
/// This function doubles backticks within the identifier (MySQL standard escaping),
/// ensuring the concatenated SQL cannot be broken by malicious identifiers.
///
/// Supports dotted qualified identifiers: `u.id` → `u`.`id`
fn quote_ident(s: &str) -> String {
    s.split('.')
        .map(|part| format!("`{}`", part.replace('`', "``")))
        .collect::<Vec<_>>()
        .join(".")
}

/// Quote a column name for the target dialect, supporting dotted qualified identifiers
///
/// Difference from [`sz_orm_core::Dialect::quote`]: this function first splits the qualified
/// identifier by `.` (e.g., `t.id`), applies dialect quoting to each part, then joins them
/// with `.`, producing `t`.`id` (MySQL) or `"t"."id"` (PostgreSQL/SQLite). Using
/// `dialect.quote()` directly would quote the entire `t.id` as a single identifier,
/// causing test assertions of the `t`.`id` form to fail.
///
/// # Security
///
/// Each identifier segment is escaped for internal quotes by `dialect.quote()` (MySQL `` ` `` → ` `` ` ``,
/// PostgreSQL `"` → `""`), preventing identifier escape injection.
fn quote_column_dialect(dialect: &dyn sz_orm_core::Dialect, column: &str) -> String {
    column
        .split('.')
        .map(|part| dialect.quote(part))
        .collect::<Vec<_>>()
        .join(".")
}

/// Validate a WHERE condition string, rejecting obvious SQL injection patterns
///
/// v0.2.2 fix C-6: the public `where_clause(condition: &str)` accepts arbitrary strings,
/// posing a SQL injection risk. This function detects high-risk patterns (semicolon + SQL
/// keyword, line comments, block comments) and rejects obviously malicious input.
///
/// # Detection patterns
///
/// - `;` followed by a SQL keyword (DROP/DELETE/UPDATE/INSERT/ALTER/TRUNCATE/EXEC/CREATE/GRANT/REVOKE)
/// - `--` line comment sequence
/// - `/*` block comment start
/// - `*/` block comment end
///
/// # Note
///
/// This validation is a baseline defense and cannot replace parameterized queries.
/// Complex WHERE conditions should use the parameterized API.
fn check_where_injection(condition: &str) {
    let upper = condition.to_uppercase();
    const SQL_KEYWORDS: &[&str] = &[
        "DROP", "DELETE", "UPDATE", "INSERT", "ALTER", "TRUNCATE", "EXEC", "CREATE", "GRANT",
        "REVOKE",
    ];
    for kw in SQL_KEYWORDS {
        let pattern1 = format!(";{}", kw);
        let pattern2 = format!("; {}", kw);
        if upper.contains(&pattern1) || upper.contains(&pattern2) {
            panic!(
                "SQL injection detected in where_clause: semicolon followed by {} keyword: {:?}",
                kw, condition
            );
        }
    }
    if condition.contains("--") {
        panic!(
            "SQL injection detected in where_clause: line comment '--' not allowed: {:?}",
            condition
        );
    }
    if condition.contains("/*") || condition.contains("*/") {
        panic!(
            "SQL injection detected in where_clause: block comment '/*' or '*/' not allowed: {:?}",
            condition
        );
    }
}

/// Query builder entry point
///
/// # Gradual deprecation notice (v3.5.0)
///
/// Since v3.5.0, `sz-orm-query-builder` has entered gradual deprecation.
/// Please evaluate [docs/query-builder-guide.md](../../../docs/query-builder-guide.md)
/// to choose an appropriate approach:
///
/// - **Standard CRUD**: recommended to migrate to `sz_orm_core::QueryBuilder<M>` (compile-time type safety)
/// - **Complex SQL (UNION/CTE/window functions)**: may continue using this crate (not removed yet)
///
/// This crate is **not immediately removed** in v3.5.0; the API is fully compatible,
/// only a `#[deprecated]` warning is added. A decision on whether to merge into
/// `sz-orm-core` or keep it standalone will be evaluated in v4.0.0.
#[deprecated(
    since = "3.5.0",
    note = "v3.5.0: 请评估 docs/query-builder-guide.md 选择合适方案。标准 CRUD 推荐迁移到 sz_orm_core::QueryBuilder<M>"
)]
pub struct Query;

#[allow(deprecated)]
impl Query {
    /// Create a SELECT query
    #[deprecated(
        since = "3.5.0",
        note = "v3.5.0: 推荐迁移到 sz_orm_core::QueryBuilder<M>::select()。详见 docs/query-builder-guide.md"
    )]
    pub fn select() -> SelectQuery {
        SelectQuery::new()
    }

    /// Create an INSERT query
    #[deprecated(
        since = "3.5.0",
        note = "v3.5.0: 推荐迁移到 sz_orm_core::QueryBuilder<M>::insert()。详见 docs/query-builder-guide.md"
    )]
    pub fn insert() -> InsertQuery {
        InsertQuery::new()
    }

    /// Create an UPDATE query
    #[deprecated(
        since = "3.5.0",
        note = "v3.5.0: 推荐迁移到 sz_orm_core::QueryBuilder<M>::update()。详见 docs/query-builder-guide.md"
    )]
    pub fn update() -> UpdateQuery {
        UpdateQuery::new()
    }

    /// Create a DELETE query
    #[deprecated(
        since = "3.5.0",
        note = "v3.5.0: 推荐迁移到 sz_orm_core::QueryBuilder<M>::delete()。详见 docs/query-builder-guide.md"
    )]
    pub fn delete() -> DeleteQuery {
        DeleteQuery::new()
    }
}

/// Parameterized JOIN ON condition (P2 fix #68: JOIN injection risk)
///
/// Compared to the original `joins: Vec<String>` strings, this structure separates the
/// SQL template from parameter values, preventing user input from being injected into
/// the ON condition via `format!("u.id = {}", user_input)`.
#[derive(Debug, Clone)]
enum JoinOn {
    /// Raw string (backward compatible with `inner_join(table, on_str)`)
    Raw(String),
    /// Column-to-column equality join: `left_column = right_column` (no parameters, pure identifiers, already escaped)
    ColumnEq {
        left_column: String,
        right_column: String,
    },
    /// Parameterized condition: `left_column op ?` with Value
    Param {
        left_column: String,
        op: String,
        values: Vec<Value>,
    },
}

/// JOIN clause (P2 fix #68)
#[derive(Debug, Clone)]
struct JoinClause {
    /// JOIN type keyword (INNER JOIN / LEFT JOIN / RIGHT JOIN)
    join_type: &'static str,
    /// Table name part (already escaped via `quote_join_table`)
    table: String,
    /// ON condition list (supports multiple conditions, joined by AND)
    on: Vec<JoinOn>,
}

/// SELECT query builder
#[derive(Debug, Clone, Default)]
pub struct SelectQuery {
    columns: Vec<String>,
    from_table: Option<String>,
    /// FROM subquery: `(subquery SQL, alias)`. Mutually exclusive with `from_table`; later caller overrides earlier.
    from_subquery: Option<(String, String)>,
    /// Structured JOIN clauses (uniformly stores raw strings and parameterized ON conditions, P2 fix #68)
    ///
    /// The legacy `inner_join(table, on_str)` API pushes via `JoinOn::Raw(on_str)`,
    /// sharing the same render path as the new `inner_join_on` / `inner_join_param`,
    /// avoiding render order and dead code issues from dual-track data structures.
    join_clauses: Vec<JoinClause>,
    wheres: Vec<String>,
    /// Parameterized WHERE conditions (P0 fix: does not interfere with `wheres`; at render time `wheres` first, then `param_wheres`)
    param_wheres: Vec<ParamWhere>,
    order_by: Vec<String>,
    group_by: Vec<String>,
    having: Vec<String>,
    limit: Option<u64>,
    offset: Option<u64>,
    distinct: bool,
    /// CTE (Common Table Expression) clause list: (name, subquery SQL, is_recursive)
    ctes: Vec<(String, String, bool)>,
    /// Window function columns: raw expressions (e.g., `ROW_NUMBER() OVER (PARTITION BY dept ORDER BY salary DESC)`)
    window_columns: Vec<String>,
    /// FOR UPDATE lock hint
    for_update: bool,
    /// Column-qualified FOR UPDATE options (NOWAIT / SKIP LOCKED, etc.)
    for_update_options: Option<String>,
}

impl SelectQuery {
    /// Create an empty SELECT query
    pub fn new() -> Self {
        Self::default()
    }

    /// Set DISTINCT
    pub fn distinct(mut self) -> Self {
        self.distinct = true;
        self
    }

    /// Add a column
    pub fn column(mut self, name: &str) -> Self {
        self.columns.push(name.to_string());
        self
    }

    /// Add multiple columns
    pub fn columns(mut self, names: &[&str]) -> Self {
        for n in names {
            self.columns.push(n.to_string());
        }
        self
    }

    /// Add a `*` column
    pub fn all_columns(self) -> Self {
        self.column("*")
    }

    /// Set FROM table
    pub fn from(mut self, table: &str) -> Self {
        self.from_table = Some(table.to_string());
        // Mutually exclusive with from_subquery; later caller overrides earlier
        self.from_subquery = None;
        self
    }

    /// Set FROM subquery: `FROM (<subquery_sql>) AS <alias>`
    ///
    /// Mutually exclusive with [`from`](Self::from); later caller overrides earlier.
    /// The subquery SQL is constructed by the caller (may be generated by another
    /// `SelectQuery::build`), and the alias is escaped via `dialect.quote()` to
    /// prevent identifier escape.
    ///
    /// # Example
    ///
    /// ```
    /// use sz_orm_core::DbType;
    /// use sz_orm_query_builder::Query;
    ///
    /// let inner = Query::select()
    ///     .column("id")
    ///     .column("amount")
    ///     .from("orders")
    ///     .build(DbType::MySQL);
    /// let sql = Query::select()
    ///     .column("id")
    ///     .from_subquery(&inner, "t")
    ///     .build(DbType::MySQL);
    /// assert!(sql.contains("FROM (SELECT `id`, `amount` FROM `orders`) AS `t`"));
    /// ```
    pub fn from_subquery(mut self, subquery_sql: &str, alias: &str) -> Self {
        self.from_subquery = Some((subquery_sql.to_string(), alias.to_string()));
        // Mutually exclusive with from_table; later caller overrides earlier
        self.from_table = None;
        self
    }

    /// Add an INNER JOIN
    ///
    /// # Security (gate 9 fix)
    ///
    /// The table name is escaped via `quote_ident()`. The `on` condition is an
    /// expression; the caller should ensure it is not constructed with malicious input.
    pub fn inner_join(mut self, table: &str, on: &str) -> Self {
        self.join_clauses.push(JoinClause {
            join_type: "INNER JOIN",
            table: Self::quote_join_table(table),
            on: vec![JoinOn::Raw(on.to_string())],
        });
        self
    }

    /// Add a LEFT JOIN
    ///
    /// # Security (gate 9 fix)
    ///
    /// Same as `inner_join`; the table name is escaped via `quote_ident()`.
    pub fn left_join(mut self, table: &str, on: &str) -> Self {
        self.join_clauses.push(JoinClause {
            join_type: "LEFT JOIN",
            table: Self::quote_join_table(table),
            on: vec![JoinOn::Raw(on.to_string())],
        });
        self
    }

    /// Add a RIGHT JOIN
    ///
    /// # Security (gate 9 fix)
    ///
    /// Same as `inner_join`; the table name is escaped via `quote_ident()`.
    pub fn right_join(mut self, table: &str, on: &str) -> Self {
        self.join_clauses.push(JoinClause {
            join_type: "RIGHT JOIN",
            table: Self::quote_join_table(table),
            on: vec![JoinOn::Raw(on.to_string())],
        });
        self
    }

    // ========================================================================
    // 参数化 JOIN（P2 修复 #68：JOIN 注入风险）
    // ========================================================================
    //
    // 以下方法分离 JOIN ON 条件的 SQL 模板与参数值，从根源上杜绝 SQL 注入。
    // 调用方应优先使用这些方法替代 `inner_join(table, &format!(...))`，
    // 尤其是当 ON 条件值来自不可信输入时。
    //
    // 使用示例：
    // ```
    // use sz_orm_core::{DbType, Value};
    // use sz_orm_query_builder::Query;
    //
    // // 列对列等值连接（最常见场景，无参数）
    // let sql = Query::select()
    //     .column("u.id")
    //     .from("users u")
    //     .inner_join_on("orders o", "u.id", "o.user_id")
    //     .build(DbType::MySQL);
    // // INNER JOIN `orders` o ON `u`.`id` = `o`.`user_id`
    //
    // // 参数化 ON 条件（用户输入作为参数绑定）
    // let built = Query::select()
    //     .column("u.id")
    //     .from("users u")
    //     .inner_join_param("orders o", "o.status", " = ?", Value::String("paid".into()))
    //     .build_with_params(DbType::MySQL);
    // // built.sql: "INNER JOIN `orders` o ON `o`.`status` = ?"
    // // built.params: [String("paid")]
    // ```

    /// Add an INNER JOIN with a column-to-column equality ON condition (`left_col = right_col`)
    ///
    /// # Security
    ///
    /// Column names are escaped per dialect via `quote_column_dialect`, preventing identifier escape.
    /// No parameter values; pure identifier join, the most common and safest JOIN form.
    pub fn inner_join_on(mut self, table: &str, left_col: &str, right_col: &str) -> Self {
        self.join_clauses.push(JoinClause {
            join_type: "INNER JOIN",
            table: Self::quote_join_table(table),
            on: vec![JoinOn::ColumnEq {
                left_column: left_col.to_string(),
                right_column: right_col.to_string(),
            }],
        });
        self
    }

    /// Add a LEFT JOIN with a column-to-column equality ON condition
    pub fn left_join_on(mut self, table: &str, left_col: &str, right_col: &str) -> Self {
        self.join_clauses.push(JoinClause {
            join_type: "LEFT JOIN",
            table: Self::quote_join_table(table),
            on: vec![JoinOn::ColumnEq {
                left_column: left_col.to_string(),
                right_column: right_col.to_string(),
            }],
        });
        self
    }

    /// Add a RIGHT JOIN with a column-to-column equality ON condition
    pub fn right_join_on(mut self, table: &str, left_col: &str, right_col: &str) -> Self {
        self.join_clauses.push(JoinClause {
            join_type: "RIGHT JOIN",
            table: Self::quote_join_table(table),
            on: vec![JoinOn::ColumnEq {
                left_column: left_col.to_string(),
                right_column: right_col.to_string(),
            }],
        });
        self
    }

    /// Add an INNER JOIN with a parameterized expression ON condition (`left_col op ?`)
    ///
    /// # Parameters
    ///
    /// - `table`: JOIN table name (supports alias `orders o`)
    /// - `left_col`: left column name (already escaped)
    /// - `op_expr`: operator + placeholder part (e.g., ` = ?`, ` > ?`, ` IN (?, ?)`)
    /// - `value`: single parameter value
    pub fn inner_join_param(
        mut self,
        table: &str,
        left_col: &str,
        op_expr: &str,
        value: Value,
    ) -> Self {
        self.join_clauses.push(JoinClause {
            join_type: "INNER JOIN",
            table: Self::quote_join_table(table),
            on: vec![JoinOn::Param {
                left_column: left_col.to_string(),
                op: op_expr.to_string(),
                values: vec![value],
            }],
        });
        self
    }

    /// Add a LEFT JOIN with a parameterized expression ON condition
    pub fn left_join_param(
        mut self,
        table: &str,
        left_col: &str,
        op_expr: &str,
        value: Value,
    ) -> Self {
        self.join_clauses.push(JoinClause {
            join_type: "LEFT JOIN",
            table: Self::quote_join_table(table),
            on: vec![JoinOn::Param {
                left_column: left_col.to_string(),
                op: op_expr.to_string(),
                values: vec![value],
            }],
        });
        self
    }

    /// Add a RIGHT JOIN with a parameterized expression ON condition
    pub fn right_join_param(
        mut self,
        table: &str,
        left_col: &str,
        op_expr: &str,
        value: Value,
    ) -> Self {
        self.join_clauses.push(JoinClause {
            join_type: "RIGHT JOIN",
            table: Self::quote_join_table(table),
            on: vec![JoinOn::Param {
                left_column: left_col.to_string(),
                op: op_expr.to_string(),
                values: vec![value],
            }],
        });
        self
    }

    /// Render JOIN clauses (including parameterized ON conditions) into a SQL string
    ///
    /// Uniformly renders `join_clauses`: includes raw string ON conditions (`JoinOn::Raw`,
    /// backward compatible) and parameterized ON conditions (`ColumnEq` / `Param`).
    /// Parameters from parameterized JOINs are appended to `params`.
    fn render_joins(&self, dialect: &dyn sz_orm_core::Dialect, params: &mut Vec<Value>) -> String {
        let mut sql = String::new();
        for clause in &self.join_clauses {
            sql.push(' ');
            sql.push_str(clause.join_type);
            sql.push(' ');
            sql.push_str(&clause.table);
            sql.push_str(" ON ");
            for (i, on) in clause.on.iter().enumerate() {
                if i > 0 {
                    sql.push_str(" AND ");
                }
                match on {
                    JoinOn::Raw(raw) => sql.push_str(raw),
                    JoinOn::ColumnEq {
                        left_column,
                        right_column,
                    } => {
                        sql.push_str(&quote_column_dialect(dialect, left_column));
                        sql.push_str(" = ");
                        sql.push_str(&quote_column_dialect(dialect, right_column));
                    }
                    JoinOn::Param {
                        left_column,
                        op,
                        values,
                    } => {
                        sql.push_str(&quote_column_dialect(dialect, left_column));
                        sql.push_str(op);
                        params.extend(values.iter().cloned());
                    }
                }
            }
        }
        sql
    }

    /// Escape the JOIN table name part (supports alias: `orders o` → `` `orders` o ``)
    fn quote_join_table(table: &str) -> String {
        if let Some((tbl, alias)) = table.rsplit_once(' ') {
            if alias.to_uppercase() == "AS" {
                // `orders AS o`
                format!("{} AS {}", quote_ident(tbl), alias)
            } else {
                // `orders o`
                format!("{} {}", quote_ident(tbl), alias)
            }
        } else {
            quote_ident(table)
        }
    }

    /// Add a WHERE condition (AND joined)
    ///
    /// # Security (v0.2.2 fix C-6)
    ///
    /// Calls `check_where_injection` to detect high-risk patterns (semicolon + SQL keyword,
    /// line comments, block comments). Complex WHERE conditions should use the parameterized
    /// query API to avoid direct string concatenation.
    pub fn where_clause(mut self, condition: &str) -> Self {
        check_where_injection(condition);
        self.wheres.push(condition.to_string());
        self
    }

    // ========================================================================
    // 参数化 WHERE 条件（P0 修复：SQL 注入防护）
    // ========================================================================
    //
    // 以下方法分离 SQL 模板（含 `?` 占位符）与参数值，从根源上杜绝 SQL 注入。
    // 调用方应优先使用这些方法替代 `where_clause(&str)`，尤其是当 WHERE 条件值
    // 来自不可信输入时。
    //
    // 使用示例：
    // ```
    // use sz_orm_core::DbType;
    // use sz_orm_query_builder::Query;
    //
    // let built = Query::select()
    //     .column("id")
    //     .from("users")
    //     .where_eq("age", Value::I32(18))           // age > 18
    //     .where_eq("status", Value::String("active".into()))  // AND status = ?
    //     .or_where_eq("role", Value::String("admin".into()))  // OR role = ?
    //     .build_with_params(DbType::MySQL);
    // // built.sql: "SELECT `id` FROM `users` WHERE `age` = ? AND `status` = ? OR `role` = ?"
    // // built.params: [I32(18), String("active"), String("admin")]
    // ```

    /// Add a `column = ?` AND condition
    pub fn where_eq(mut self, column: &str, value: Value) -> Self {
        self.param_wheres.push(ParamWhere::And {
            column: column.to_string(),
            op: " = ?".to_string(),
            values: vec![value],
        });
        self
    }

    /// Add a `column <> ?` AND condition
    pub fn where_ne(mut self, column: &str, value: Value) -> Self {
        self.param_wheres.push(ParamWhere::And {
            column: column.to_string(),
            op: " <> ?".to_string(),
            values: vec![value],
        });
        self
    }

    /// Add a `column > ?` AND condition
    pub fn where_gt(mut self, column: &str, value: Value) -> Self {
        self.param_wheres.push(ParamWhere::And {
            column: column.to_string(),
            op: " > ?".to_string(),
            values: vec![value],
        });
        self
    }

    /// Add a `column >= ?` AND condition
    pub fn where_ge(mut self, column: &str, value: Value) -> Self {
        self.param_wheres.push(ParamWhere::And {
            column: column.to_string(),
            op: " >= ?".to_string(),
            values: vec![value],
        });
        self
    }

    /// Add a `column < ?` AND condition
    pub fn where_lt(mut self, column: &str, value: Value) -> Self {
        self.param_wheres.push(ParamWhere::And {
            column: column.to_string(),
            op: " < ?".to_string(),
            values: vec![value],
        });
        self
    }

    /// Add a `column <= ?` AND condition
    pub fn where_le(mut self, column: &str, value: Value) -> Self {
        self.param_wheres.push(ParamWhere::And {
            column: column.to_string(),
            op: " <= ?".to_string(),
            values: vec![value],
        });
        self
    }

    /// Add a `column LIKE ?` AND condition
    pub fn where_like(mut self, column: &str, pattern: Value) -> Self {
        self.param_wheres.push(ParamWhere::And {
            column: column.to_string(),
            op: " LIKE ?".to_string(),
            values: vec![pattern],
        });
        self
    }

    /// Add a `column IN (?, ?, ...)` AND condition
    ///
    /// An empty list produces `1 = 0` (always false), avoiding an invalid `IN ()`.
    pub fn where_in(mut self, column: &str, values: Vec<Value>) -> Self {
        let (column, op) = if values.is_empty() {
            (String::new(), "1 = 0".to_string())
        } else {
            let placeholders: Vec<&str> = std::iter::repeat_n("?", values.len()).collect();
            (
                column.to_string(),
                format!(" IN ({})", placeholders.join(", ")),
            )
        };
        self.param_wheres
            .push(ParamWhere::And { column, op, values });
        self
    }

    /// Add a `column NOT IN (?, ?, ...)` AND condition
    ///
    /// An empty list produces `1 = 1` (always true), avoiding an invalid `NOT IN ()`.
    pub fn where_not_in(mut self, column: &str, values: Vec<Value>) -> Self {
        let (column, op) = if values.is_empty() {
            (String::new(), "1 = 1".to_string())
        } else {
            let placeholders: Vec<&str> = std::iter::repeat_n("?", values.len()).collect();
            (
                column.to_string(),
                format!(" NOT IN ({})", placeholders.join(", ")),
            )
        };
        self.param_wheres
            .push(ParamWhere::And { column, op, values });
        self
    }

    /// Add a `column BETWEEN ? AND ?` AND condition
    pub fn where_between(mut self, column: &str, low: Value, high: Value) -> Self {
        self.param_wheres.push(ParamWhere::And {
            column: column.to_string(),
            op: " BETWEEN ? AND ?".to_string(),
            values: vec![low, high],
        });
        self
    }

    /// Add a `column IS NULL` AND condition
    pub fn where_null(mut self, column: &str) -> Self {
        self.param_wheres.push(ParamWhere::And {
            column: column.to_string(),
            op: " IS NULL".to_string(),
            values: vec![],
        });
        self
    }

    /// Add a `column IS NOT NULL` AND condition
    pub fn where_not_null(mut self, column: &str) -> Self {
        self.param_wheres.push(ParamWhere::And {
            column: column.to_string(),
            op: " IS NOT NULL".to_string(),
            values: vec![],
        });
        self
    }

    /// Add a `column = ?` OR condition
    pub fn or_where_eq(mut self, column: &str, value: Value) -> Self {
        self.param_wheres.push(ParamWhere::Or {
            column: column.to_string(),
            op: " = ?".to_string(),
            values: vec![value],
        });
        self
    }

    /// Add a `column <> ?` OR condition
    pub fn or_where_ne(mut self, column: &str, value: Value) -> Self {
        self.param_wheres.push(ParamWhere::Or {
            column: column.to_string(),
            op: " <> ?".to_string(),
            values: vec![value],
        });
        self
    }

    /// Add a `column > ?` OR condition
    pub fn or_where_gt(mut self, column: &str, value: Value) -> Self {
        self.param_wheres.push(ParamWhere::Or {
            column: column.to_string(),
            op: " > ?".to_string(),
            values: vec![value],
        });
        self
    }

    /// Add a `column >= ?` OR condition
    pub fn or_where_ge(mut self, column: &str, value: Value) -> Self {
        self.param_wheres.push(ParamWhere::Or {
            column: column.to_string(),
            op: " >= ?".to_string(),
            values: vec![value],
        });
        self
    }

    /// Add a `column < ?` OR condition
    pub fn or_where_lt(mut self, column: &str, value: Value) -> Self {
        self.param_wheres.push(ParamWhere::Or {
            column: column.to_string(),
            op: " < ?".to_string(),
            values: vec![value],
        });
        self
    }

    /// Add a `column <= ?` OR condition
    pub fn or_where_le(mut self, column: &str, value: Value) -> Self {
        self.param_wheres.push(ParamWhere::Or {
            column: column.to_string(),
            op: " <= ?".to_string(),
            values: vec![value],
        });
        self
    }

    /// Add a `column LIKE ?` OR condition
    pub fn or_where_like(mut self, column: &str, pattern: Value) -> Self {
        self.param_wheres.push(ParamWhere::Or {
            column: column.to_string(),
            op: " LIKE ?".to_string(),
            values: vec![pattern],
        });
        self
    }

    /// Add a `column IN (?, ?, ...)` OR condition
    pub fn or_where_in(mut self, column: &str, values: Vec<Value>) -> Self {
        let (column, op) = if values.is_empty() {
            (String::new(), "1 = 0".to_string())
        } else {
            let placeholders: Vec<&str> = std::iter::repeat_n("?", values.len()).collect();
            (
                column.to_string(),
                format!(" IN ({})", placeholders.join(", ")),
            )
        };
        self.param_wheres
            .push(ParamWhere::Or { column, op, values });
        self
    }

    /// Add a `column BETWEEN ? AND ?` OR condition
    pub fn or_where_between(mut self, column: &str, low: Value, high: Value) -> Self {
        self.param_wheres.push(ParamWhere::Or {
            column: column.to_string(),
            op: " BETWEEN ? AND ?".to_string(),
            values: vec![low, high],
        });
        self
    }

    /// Add a `column IS NULL` OR condition
    pub fn or_where_null(mut self, column: &str) -> Self {
        self.param_wheres.push(ParamWhere::Or {
            column: column.to_string(),
            op: " IS NULL".to_string(),
            values: vec![],
        });
        self
    }

    /// Add a `column IS NOT NULL` OR condition
    pub fn or_where_not_null(mut self, column: &str) -> Self {
        self.param_wheres.push(ParamWhere::Or {
            column: column.to_string(),
            op: " IS NOT NULL".to_string(),
            values: vec![],
        });
        self
    }

    /// Add GROUP BY
    pub fn group_by(mut self, column: &str) -> Self {
        self.group_by.push(column.to_string());
        self
    }

    /// Add HAVING
    pub fn having(mut self, condition: &str) -> Self {
        self.having.push(condition.to_string());
        self
    }

    /// Add ORDER BY
    ///
    /// # Parameters
    ///
    /// - `column`: column name
    /// - `asc`: true=ASC, false=DESC
    pub fn order_by(mut self, column: &str, asc: bool) -> Self {
        let dir = if asc { "ASC" } else { "DESC" };
        self.order_by.push(format!("{} {}", column, dir));
        self
    }

    /// Set LIMIT
    pub fn limit(mut self, n: u64) -> Self {
        self.limit = Some(n);
        self
    }

    /// Set OFFSET
    pub fn offset(mut self, n: u64) -> Self {
        self.offset = Some(n);
        self
    }

    /// Generate pagination (sets both LIMIT and OFFSET)
    ///
    /// # Parameters
    ///
    /// - `page`: page number (1-based)
    /// - `size`: page size
    pub fn paginate(self, page: u64, size: u64) -> Self {
        let offset = (page.saturating_sub(1)) * size;
        self.limit(size).offset(offset)
    }

    /// Add a CTE (Common Table Expression / WITH clause).
    ///
    /// Generates SQL of the form `WITH name AS (subquery) SELECT ...`.
    ///
    /// # Parameters
    ///
    /// - `name`: CTE name
    /// - `subquery`: subquery SQL (a complete SELECT statement)
    pub fn with_cte(mut self, name: &str, subquery: &str) -> Self {
        self.ctes
            .push((name.to_string(), subquery.to_string(), false));
        self
    }

    /// Add a recursive CTE (`WITH RECURSIVE name AS (...) SELECT ...`).
    ///
    /// # Parameters
    ///
    /// - `name`: CTE name
    /// - `subquery`: recursive subquery SQL
    pub fn with_recursive_cte(mut self, name: &str, subquery: &str) -> Self {
        self.ctes
            .push((name.to_string(), subquery.to_string(), true));
        self
    }

    /// Add a window function column (as a raw expression in the SELECT list).
    ///
    /// The caller is responsible for constructing the complete window function expression, e.g.:
    /// - `ROW_NUMBER() OVER (PARTITION BY dept ORDER BY salary DESC)`
    /// - `RANK() OVER (ORDER BY score DESC)`
    /// - `SUM(amount) OVER (PARTITION BY user_id ORDER BY created_at)`
    ///
    /// # Parameters
    ///
    /// - `expr`: complete window function expression
    pub fn window_function(mut self, expr: &str) -> Self {
        self.window_columns.push(expr.to_string());
        self
    }

    /// Add a `ROW_NUMBER()` window function column.
    ///
    /// # Parameters
    ///
    /// - `partition_by`: PARTITION BY column (may be empty)
    /// - `order_by`: ORDER BY column (e.g., `salary DESC`)
    /// - `alias`: result column alias (e.g., `row_num`)
    pub fn row_number(self, partition_by: &str, order_by: &str, alias: &str) -> Self {
        let partition_clause = if partition_by.is_empty() {
            String::new()
        } else {
            format!("PARTITION BY {} ", partition_by)
        };
        let expr = format!(
            "ROW_NUMBER() OVER ({}ORDER BY {}) AS {}",
            partition_clause, order_by, alias
        );
        self.window_function(&expr)
    }

    /// Add a `RANK()` window function column.
    ///
    /// # Parameters
    ///
    /// - `partition_by`: PARTITION BY column (may be empty)
    /// - `order_by`: ORDER BY column
    /// - `alias`: result column alias
    pub fn rank(self, partition_by: &str, order_by: &str, alias: &str) -> Self {
        let partition_clause = if partition_by.is_empty() {
            String::new()
        } else {
            format!("PARTITION BY {} ", partition_by)
        };
        let expr = format!(
            "RANK() OVER ({}ORDER BY {}) AS {}",
            partition_clause, order_by, alias
        );
        self.window_function(&expr)
    }

    /// Add a `DENSE_RANK()` window function column.
    ///
    /// # Parameters
    ///
    /// - `partition_by`: PARTITION BY column (may be empty)
    /// - `order_by`: ORDER BY column
    /// - `alias`: result column alias
    pub fn dense_rank(self, partition_by: &str, order_by: &str, alias: &str) -> Self {
        let partition_clause = if partition_by.is_empty() {
            String::new()
        } else {
            format!("PARTITION BY {} ", partition_by)
        };
        let expr = format!(
            "DENSE_RANK() OVER ({}ORDER BY {}) AS {}",
            partition_clause, order_by, alias
        );
        self.window_function(&expr)
    }

    /// Set FOR UPDATE row lock.
    ///
    /// Appends `FOR UPDATE` to the end of the generated SQL, used for pessimistic lock.
    pub fn for_update(mut self) -> Self {
        self.for_update = true;
        self.for_update_options = None;
        self
    }

    /// Set FOR UPDATE with options (e.g., `NOWAIT`, `SKIP LOCKED`).
    ///
    /// # Parameters
    ///
    /// - `options`: options string, e.g., `"NOWAIT"` or `"SKIP LOCKED"`
    pub fn for_update_with_options(mut self, options: &str) -> Self {
        self.for_update = true;
        self.for_update_options = Some(options.to_string());
        self
    }

    /// Combine the current query with another query using UNION set operation.
    ///
    /// Returns a [`SetQuery`], which can be turned into the final SQL via `build()`.
    pub fn union(self, other: SelectQuery) -> SetQuery {
        SetQuery::new(self, SetOperator::Union, other)
    }

    /// Combine the current query with another query using UNION ALL set operation.
    pub fn union_all(self, other: SelectQuery) -> SetQuery {
        SetQuery::new(self, SetOperator::UnionAll, other)
    }

    /// Combine the current query with another query using INTERSECT set operation.
    pub fn intersect(self, other: SelectQuery) -> SetQuery {
        SetQuery::new(self, SetOperator::Intersect, other)
    }

    /// Combine the current query with another query using EXCEPT set operation.
    pub fn except(self, other: SelectQuery) -> SetQuery {
        SetQuery::new(self, SetOperator::Except, other)
    }

    /// Generate SQL
    ///
    /// # Parameters
    ///
    /// - `db_type`: database type, used to select the dialect
    pub fn build(self, db_type: DbType) -> String {
        let dialect = match sz_orm_core::get_dialect(db_type) {
            Ok(d) => d,
            Err(_) => return String::new(),
        };

        let mut sql = String::new();

        // CTE（WITH 子句）
        if !self.ctes.is_empty() {
            let has_recursive = self.ctes.iter().any(|(_, _, r)| *r);
            if has_recursive {
                sql.push_str("WITH RECURSIVE ");
            } else {
                sql.push_str("WITH ");
            }
            let cte_strs: Vec<String> = self
                .ctes
                .iter()
                .map(|(name, subquery, _)| format!("{} AS ({})", name, subquery))
                .collect();
            sql.push_str(&cte_strs.join(", "));
            sql.push(' ');
        }

        sql.push_str("SELECT ");

        if self.distinct {
            sql.push_str("DISTINCT ");
        }

        // 合并普通列与窗口函数列
        let mut all_columns: Vec<String> = self
            .columns
            .iter()
            .map(|c| {
                if c == "*" {
                    c.clone()
                } else {
                    dialect.quote(c)
                }
            })
            .collect();
        all_columns.extend(self.window_columns.iter().cloned());

        if all_columns.is_empty() {
            sql.push('*');
        } else {
            sql.push_str(&all_columns.join(", "));
        }

        if let Some(ref table) = self.from_table {
            sql.push_str(" FROM ");
            sql.push_str(&dialect.quote(table));
        } else if let Some((ref subquery, ref alias)) = self.from_subquery {
            // FROM 子查询：`FROM (<subquery>) AS <alias>`
            sql.push_str(" FROM (");
            sql.push_str(subquery);
            sql.push_str(") AS ");
            sql.push_str(&dialect.quote(alias));
        }

        // 渲染 JOIN（含参数化 ON 条件，P2 修复 #68）
        // build() 不收集参数（无参数化查询），参数化 JOIN 应通过 build_with_params() 调用
        let mut unused_params = Vec::new();
        sql.push_str(&self.render_joins(&*dialect, &mut unused_params));

        if !self.wheres.is_empty() {
            sql.push_str(" WHERE ");
            // 第一个条件不加 AND/OR 前缀
            sql.push_str(&self.wheres[0]);
            for w in &self.wheres[1..] {
                if w.starts_with("OR ") {
                    sql.push(' ');
                    sql.push_str(w);
                } else {
                    sql.push_str(" AND ");
                    sql.push_str(w);
                }
            }
        }

        if !self.group_by.is_empty() {
            sql.push_str(" GROUP BY ");
            sql.push_str(
                &self
                    .group_by
                    .iter()
                    .map(|c| quote_ident(c))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }

        if !self.having.is_empty() {
            sql.push_str(" HAVING ");
            sql.push_str(&self.having.join(" AND "));
        }

        if !self.order_by.is_empty() {
            sql.push_str(" ORDER BY ");
            sql.push_str(
                &self
                    .order_by
                    .iter()
                    .map(|s| {
                        // 格式为 "column ASC" 或 "column DESC"
                        if let Some((col, dir)) = s.rsplit_once(' ') {
                            format!("{} {}", quote_ident(col), dir)
                        } else {
                            quote_ident(s)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }

        if let Some(limit) = self.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }
        if let Some(offset) = self.offset {
            sql.push_str(&format!(" OFFSET {}", offset));
        }

        // FOR UPDATE 行锁
        if self.for_update {
            sql.push_str(" FOR UPDATE");
            if let Some(ref opts) = self.for_update_options {
                sql.push(' ');
                sql.push_str(opts);
            }
        }

        sql
    }

    /// Generate parameterized SQL (parameterized query, P0 fix: SQL injection prevention)
    ///
    /// Returns a [`BuiltQuery`], containing a SQL string with `?` placeholders and a
    /// list of parameters bound in order. Differences from [`build`](Self::build):
    /// - WHERE conditions can come from parameterized APIs like `where_eq`/`where_in`/`where_between`
    /// - User input is bound as parameters rather than concatenated into the SQL string
    ///
    /// # Mixed usage rules
    ///
    /// When using both raw `where_clause(&str)` and parameterized `where_eq(column, value)`:
    /// - Raw conditions render first (no parameters)
    /// - Parameterized conditions render after (parameters collected in order)
    /// - Both preserve AND/OR conjunction semantics in call order
    ///
    /// # Example
    ///
    /// ```
    /// use sz_orm_core::{DbType, Value};
    /// use sz_orm_query_builder::Query;
    ///
    /// let built = Query::select()
    ///     .column("id")
    ///     .from("users")
    ///     .where_eq("age", Value::I32(18))
    ///     .or_where_eq("role", Value::String("admin".into()))
    ///     .build_with_params(DbType::MySQL);
    /// assert!(built.sql.contains("WHERE `age` = ? OR `role` = ?"));
    /// assert_eq!(built.params.len(), 2);
    /// ```
    pub fn build_with_params(self, db_type: DbType) -> BuiltQuery {
        let dialect = match sz_orm_core::get_dialect(db_type) {
            Ok(d) => d,
            Err(_) => return BuiltQuery::default(),
        };

        let mut sql = String::new();
        let mut params: Vec<Value> = Vec::new();

        // CTE（WITH 子句）
        if !self.ctes.is_empty() {
            let has_recursive = self.ctes.iter().any(|(_, _, r)| *r);
            if has_recursive {
                sql.push_str("WITH RECURSIVE ");
            } else {
                sql.push_str("WITH ");
            }
            let cte_strs: Vec<String> = self
                .ctes
                .iter()
                .map(|(name, subquery, _)| format!("{} AS ({})", name, subquery))
                .collect();
            sql.push_str(&cte_strs.join(", "));
            sql.push(' ');
        }

        sql.push_str("SELECT ");

        if self.distinct {
            sql.push_str("DISTINCT ");
        }

        let mut all_columns: Vec<String> = self
            .columns
            .iter()
            .map(|c| {
                if c == "*" {
                    c.clone()
                } else {
                    dialect.quote(c)
                }
            })
            .collect();
        all_columns.extend(self.window_columns.iter().cloned());

        if all_columns.is_empty() {
            sql.push('*');
        } else {
            sql.push_str(&all_columns.join(", "));
        }

        if let Some(ref table) = self.from_table {
            sql.push_str(" FROM ");
            sql.push_str(&dialect.quote(table));
        } else if let Some((ref subquery, ref alias)) = self.from_subquery {
            // FROM 子查询：`FROM (<subquery>) AS <alias>`
            sql.push_str(" FROM (");
            sql.push_str(subquery);
            sql.push_str(") AS ");
            sql.push_str(&dialect.quote(alias));
        }

        // 渲染 JOIN（含参数化 ON 条件，P2 修复 #68）
        // 参数化 JOIN 的参数追加到 params 列表
        sql.push_str(&self.render_joins(&*dialect, &mut params));

        // WHERE：原始 wheres（无参数）+ 参数化 param_wheres（带 ? 占位符）
        let has_raw = !self.wheres.is_empty();
        let has_param = !self.param_wheres.is_empty();
        if has_raw || has_param {
            sql.push_str(" WHERE ");
            let mut first = true;
            // 原始条件先渲染
            for w in &self.wheres {
                if first {
                    sql.push_str(w);
                    first = false;
                } else if w.starts_with("OR ") {
                    sql.push(' ');
                    sql.push_str(w);
                } else {
                    sql.push_str(" AND ");
                    sql.push_str(w);
                }
            }
            // 参数化条件后渲染
            for pw in &self.param_wheres {
                let (conjunction, column, op, vals) = match pw {
                    ParamWhere::And { column, op, values } => ("AND", column, op, values),
                    ParamWhere::Or { column, op, values } => ("OR", column, op, values),
                };
                // 按目标方言引用列名（PostgreSQL 双引号、MySQL 反引号）
                let expr = if column.is_empty() {
                    op.clone()
                } else {
                    format!("{}{}", quote_column_dialect(&*dialect, column), op)
                };
                if first {
                    sql.push_str(&expr);
                    first = false;
                } else {
                    sql.push(' ');
                    sql.push_str(conjunction);
                    sql.push(' ');
                    sql.push_str(&expr);
                }
                params.extend(vals.iter().cloned());
            }
        }

        if !self.group_by.is_empty() {
            sql.push_str(" GROUP BY ");
            sql.push_str(
                &self
                    .group_by
                    .iter()
                    .map(|c| quote_ident(c))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }

        if !self.having.is_empty() {
            sql.push_str(" HAVING ");
            sql.push_str(&self.having.join(" AND "));
        }

        if !self.order_by.is_empty() {
            sql.push_str(" ORDER BY ");
            sql.push_str(
                &self
                    .order_by
                    .iter()
                    .map(|s| {
                        if let Some((col, dir)) = s.rsplit_once(' ') {
                            format!("{} {}", quote_ident(col), dir)
                        } else {
                            quote_ident(s)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }

        if let Some(limit) = self.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }
        if let Some(offset) = self.offset {
            sql.push_str(&format!(" OFFSET {}", offset));
        }

        if self.for_update {
            sql.push_str(" FOR UPDATE");
            if let Some(ref opts) = self.for_update_options {
                sql.push(' ');
                sql.push_str(opts);
            }
        }

        BuiltQuery { sql, params }
    }
}

// ============================================================================
// 深度扩展：集合运算（UNION / INTERSECT / EXCEPT）
// ============================================================================

/// SQL set operator type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetOperator {
    /// `UNION`: merge with deduplication
    Union,
    /// `UNION ALL`: merge without deduplication
    UnionAll,
    /// `INTERSECT`: intersection
    Intersect,
    /// `EXCEPT`: difference (MySQL 8.0+ calls it `EXCEPT`; some dialects use `MINUS`)
    Except,
}

impl SetOperator {
    /// Returns the SQL keyword corresponding to the operator.
    pub fn as_sql(&self) -> &'static str {
        match self {
            SetOperator::Union => "UNION",
            SetOperator::UnionAll => "UNION ALL",
            SetOperator::Intersect => "INTERSECT",
            SetOperator::Except => "EXCEPT",
        }
    }
}

/// Set operation query, supports chaining multiple SELECTs joined by UNION/INTERSECT/EXCEPT.
///
/// # Example
///
/// ```ignore
/// use sz_orm_core::DbType;
/// use sz_orm_query_builder::Query;
///
/// let q1 = Query::select().column("id").from("active_users");
/// let q2 = Query::select().column("id").from("pending_users");
/// let sql = q1.union(q2).build(DbType::MySQL);
/// // SELECT `id` FROM `active_users` UNION SELECT `id` FROM `pending_users`
/// ```
#[derive(Debug, Clone)]
pub struct SetQuery {
    /// First SELECT query
    first: SelectQuery,
    /// Subsequent (operator, query) pairs
    rest: Vec<(SetOperator, SelectQuery)>,
    /// Global ORDER BY (applies to the entire set operation result)
    order_by: Vec<String>,
    /// Global LIMIT
    limit: Option<u64>,
    /// Global OFFSET
    offset: Option<u64>,
}

impl SetQuery {
    /// Create a set operation query.
    pub fn new(first: SelectQuery, op: SetOperator, second: SelectQuery) -> Self {
        Self {
            first,
            rest: vec![(op, second)],
            order_by: Vec::new(),
            limit: None,
            offset: None,
        }
    }

    /// Append a UNION query.
    pub fn union(mut self, other: SelectQuery) -> Self {
        self.rest.push((SetOperator::Union, other));
        self
    }

    /// Append a UNION ALL query.
    pub fn union_all(mut self, other: SelectQuery) -> Self {
        self.rest.push((SetOperator::UnionAll, other));
        self
    }

    /// Append an INTERSECT query.
    pub fn intersect(mut self, other: SelectQuery) -> Self {
        self.rest.push((SetOperator::Intersect, other));
        self
    }

    /// Append an EXCEPT query.
    pub fn except(mut self, other: SelectQuery) -> Self {
        self.rest.push((SetOperator::Except, other));
        self
    }

    /// Set global ORDER BY (applies to the entire set operation result).
    pub fn order_by(mut self, column: &str, asc: bool) -> Self {
        let dir = if asc { "ASC" } else { "DESC" };
        self.order_by.push(format!("{} {}", column, dir));
        self
    }

    /// Set global LIMIT.
    pub fn limit(mut self, n: u64) -> Self {
        self.limit = Some(n);
        self
    }

    /// Set global OFFSET.
    pub fn offset(mut self, n: u64) -> Self {
        self.offset = Some(n);
        self
    }

    /// Generate SQL.
    ///
    /// Joins all subqueries with the corresponding set operators, and appends
    /// global ORDER BY / LIMIT / OFFSET at the end.
    pub fn build(self, db_type: DbType) -> String {
        let mut sql = self.first.build(db_type);
        for (op, query) in &self.rest {
            sql.push(' ');
            sql.push_str(op.as_sql());
            sql.push(' ');
            sql.push_str(&query.clone().build(db_type));
        }
        if !self.order_by.is_empty() {
            sql.push_str(" ORDER BY ");
            sql.push_str(
                &self
                    .order_by
                    .iter()
                    .map(|s| {
                        if let Some((col, dir)) = s.rsplit_once(' ') {
                            format!("{} {}", quote_ident(col), dir)
                        } else {
                            quote_ident(s)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
        if let Some(limit) = self.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }
        if let Some(offset) = self.offset {
            sql.push_str(&format!(" OFFSET {}", offset));
        }
        sql
    }
}

/// Upsert (insert or update) strategy
///
/// Supports conflict-handling syntax of three mainstream databases:
/// - PostgreSQL/SQLite: `ON CONFLICT ... DO NOTHING` / `DO UPDATE`
/// - MySQL/MariaDB/TiDB: `ON DUPLICATE KEY UPDATE` / `REPLACE INTO`
///
/// # Dialect compatibility
///
/// | Strategy | MySQL | PostgreSQL | SQLite |
/// |----------|-------|------------|--------|
/// | `OnConflictDoNothing` | ✗ | ✓ | ✓ |
/// | `OnConflictDoUpdate` | ✗ | ✓ | ✓ |
/// | `OnDuplicateKeyUpdate` | ✓ | ✗ | ✗ |
/// | `Replace` | ✓ | ✗ | ✓ (`INSERT OR REPLACE`)|
///
/// Incompatible combinations are skipped in `build_with_dialect` (no error; the caller
/// must ensure dialect match).
#[derive(Debug, Clone, Default)]
pub enum UpsertStrategy {
    /// No upsert (default)
    #[default]
    None,
    /// PostgreSQL/SQLite: `ON CONFLICT (cols) DO NOTHING`
    OnConflictDoNothing(Vec<String>),
    /// PostgreSQL/SQLite: `ON CONFLICT (cols) DO UPDATE SET col = expr, ...`
    /// `(conflict_cols, update_assignments)`
    OnConflictDoUpdate(Vec<String>, Vec<(String, String)>),
    /// MySQL: `ON DUPLICATE KEY UPDATE col = expr, ...`
    OnDuplicateKeyUpdate(Vec<(String, String)>),
    /// MySQL: `REPLACE INTO` (delete conflicting rows first, then insert)
    Replace,
}

/// Determine whether DbType is a MySQL-compatible dialect
fn is_mysql_family(db_type: DbType) -> bool {
    matches!(
        db_type,
        DbType::MySQL | DbType::MariaDB | DbType::TiDB | DbType::OceanBase | DbType::PolarDB
    )
}

/// Determine whether DbType is a PostgreSQL-compatible dialect
fn is_pg_family(db_type: DbType) -> bool {
    matches!(
        db_type,
        DbType::PostgreSQL | DbType::Kingbase | DbType::GaussDB | DbType::PolarDB
    ) || db_type == DbType::Sqlite
}

/// Render the upsert clause (for build_with_dialect)
///
/// Returns the corresponding upsert SQL fragment per dialect; incompatible combinations
/// return `None` (skipped).
fn render_upsert_clause(strategy: &UpsertStrategy, db_type: DbType) -> Option<String> {
    match strategy {
        UpsertStrategy::None => None,
        UpsertStrategy::OnConflictDoNothing(cols) => {
            if is_pg_family(db_type) {
                let cols_str = cols
                    .iter()
                    .map(|c| quote_ident(c))
                    .collect::<Vec<_>>()
                    .join(", ");
                Some(format!("ON CONFLICT ({}) DO NOTHING", cols_str))
            } else {
                None
            }
        }
        UpsertStrategy::OnConflictDoUpdate(cols, assignments) => {
            if is_pg_family(db_type) {
                let cols_str = cols
                    .iter()
                    .map(|c| quote_ident(c))
                    .collect::<Vec<_>>()
                    .join(", ");
                let sets = assignments
                    .iter()
                    .map(|(c, v)| format!("{} = {}", quote_ident(c), v))
                    .collect::<Vec<_>>()
                    .join(", ");
                Some(format!("ON CONFLICT ({}) DO UPDATE SET {}", cols_str, sets))
            } else {
                None
            }
        }
        UpsertStrategy::OnDuplicateKeyUpdate(assignments) => {
            if is_mysql_family(db_type) {
                let sets = assignments
                    .iter()
                    .map(|(c, v)| format!("{} = {}", quote_ident(c), v))
                    .collect::<Vec<_>>()
                    .join(", ");
                Some(format!("ON DUPLICATE KEY UPDATE {}", sets))
            } else {
                None
            }
        }
        UpsertStrategy::Replace => None, // REPLACE 在 build 阶段处理 INSERT 关键字替换
    }
}

/// Render the RETURNING clause (for build_with_dialect)
///
/// RETURNING is PostgreSQL and SQLite 3.35+ syntax; MySQL does not support it.
/// - MySQL family: returns `None` (skipped)
/// - PostgreSQL/SQLite: returns `RETURNING col1, col2, ...`
/// - A column of `*` is not quoted; other column names are escaped via `dialect.quote()`
fn render_returning_clause(columns: &Option<Vec<String>>, db_type: DbType) -> Option<String> {
    let cols = columns.as_ref()?;
    if cols.is_empty() {
        return None;
    }
    // MySQL 不支持 RETURNING
    if is_mysql_family(db_type) {
        return None;
    }
    let dialect = sz_orm_core::get_dialect(db_type).ok()?;
    let quoted: Vec<String> = cols
        .iter()
        .map(|c| {
            if c == "*" {
                c.clone()
            } else {
                dialect.quote(c)
            }
        })
        .collect();
    Some(format!("RETURNING {}", quoted.join(", ")))
}

/// INSERT query builder
#[derive(Debug, Clone, Default)]
pub struct InsertQuery {
    table: Option<String>,
    columns: Vec<String>,
    values: Vec<String>,
    /// Upsert strategy (ON CONFLICT / ON DUPLICATE KEY UPDATE / REPLACE)
    upsert: UpsertStrategy,
    /// RETURNING clause column list (supported by PostgreSQL/SQLite, not MySQL)
    returning: Option<Vec<String>>,
}

impl InsertQuery {
    /// Create an empty INSERT query
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the target table
    pub fn into_table(mut self, table: &str) -> Self {
        self.table = Some(table.to_string());
        self
    }

    /// Add a column-value pair (value should be an already-escaped SQL literal)
    pub fn value(mut self, column: &str, value: &str) -> Self {
        self.columns.push(column.to_string());
        self.values.push(value.to_string());
        self
    }

    /// Batch add column-value pairs
    pub fn values(mut self, pairs: &[(&str, &str)]) -> Self {
        for (c, v) in pairs {
            self.columns.push(c.to_string());
            self.values.push(v.to_string());
        }
        self
    }

    // ========================================================================
    // Upsert 策略（ON CONFLICT / ON DUPLICATE KEY UPDATE / REPLACE）
    // ========================================================================

    /// PostgreSQL/SQLite: `ON CONFLICT (cols) DO NOTHING`
    ///
    /// Ignore insertion on conflict. Applicable to PostgreSQL and SQLite dialects.
    /// Under MySQL dialect this setting is ignored (MySQL does not support ON CONFLICT syntax).
    ///
    /// # Example
    ///
    /// ```
    /// use sz_orm_core::DbType;
    /// use sz_orm_query_builder::Query;
    ///
    /// let sql = Query::insert()
    ///     .into_table("users")
    ///     .value("id", "1")
    ///     .value("name", "'Alice'")
    ///     .on_conflict_do_nothing(&["id"])
    ///     .build_with_dialect(DbType::PostgreSQL);
    /// // INSERT INTO "users" ("id", "name") VALUES (1, 'Alice') ON CONFLICT ("id") DO NOTHING
    /// ```
    pub fn on_conflict_do_nothing(mut self, conflict_cols: &[&str]) -> Self {
        self.upsert = UpsertStrategy::OnConflictDoNothing(
            conflict_cols.iter().map(|s| s.to_string()).collect(),
        );
        self
    }

    /// PostgreSQL/SQLite: `ON CONFLICT (cols) DO UPDATE SET col = expr, ...`
    ///
    /// Update specified columns on conflict. `assignments` is a list of `(column, expression)` pairs.
    /// Expressions may use `EXCLUDED.col` to reference the to-be-inserted value (PG/SQLite standard).
    ///
    /// # Example
    ///
    /// ```
    /// use sz_orm_core::DbType;
    /// use sz_orm_query_builder::Query;
    ///
    /// let sql = Query::insert()
    ///     .into_table("users")
    ///     .value("id", "1")
    ///     .value("name", "'Alice'")
    ///     .value("count", "1")
    ///     .on_conflict_do_update(
    ///         &["id"],
    ///         &[("name", "EXCLUDED.name"), ("count", "users.count + 1")],
    ///     )
    ///     .build_with_dialect(DbType::PostgreSQL);
    /// // INSERT INTO "users" (...) VALUES (...) ON CONFLICT ("id") DO UPDATE SET
    /// //   "name" = EXCLUDED.name, "count" = users.count + 1
    /// ```
    pub fn on_conflict_do_update(
        mut self,
        conflict_cols: &[&str],
        assignments: &[(&str, &str)],
    ) -> Self {
        self.upsert = UpsertStrategy::OnConflictDoUpdate(
            conflict_cols.iter().map(|s| s.to_string()).collect(),
            assignments
                .iter()
                .map(|(c, v)| (c.to_string(), v.to_string()))
                .collect(),
        );
        self
    }

    /// MySQL: `ON DUPLICATE KEY UPDATE col = expr, ...`
    ///
    /// Update specified columns on primary/unique key conflict. `assignments` is a list of
    /// `(column, expression)` pairs. Expressions may use `VALUES(col)` to reference the
    /// to-be-inserted value (MySQL syntax).
    ///
    /// # Example
    ///
    /// ```
    /// use sz_orm_core::DbType;
    /// use sz_orm_query_builder::Query;
    ///
    /// let sql = Query::insert()
    ///     .into_table("users")
    ///     .value("id", "1")
    ///     .value("name", "'Alice'")
    ///     .value("count", "1")
    ///     .on_duplicate_key_update(&[("name", "VALUES(name)"), ("count", "count + 1")])
    ///     .build_with_dialect(DbType::MySQL);
    /// // INSERT INTO `users` (`id`, `name`, `count`) VALUES (1, 'Alice', 1)
    /// //   ON DUPLICATE KEY UPDATE `name` = VALUES(name), `count` = count + 1
    /// ```
    pub fn on_duplicate_key_update(mut self, assignments: &[(&str, &str)]) -> Self {
        self.upsert = UpsertStrategy::OnDuplicateKeyUpdate(
            assignments
                .iter()
                .map(|(c, v)| (c.to_string(), v.to_string()))
                .collect(),
        );
        self
    }

    /// MySQL: use `REPLACE INTO` instead of `INSERT INTO`
    ///
    /// On primary/unique key conflict, delete the old row first then insert the new row.
    /// Applicable to MySQL/MariaDB/TiDB dialects.
    pub fn replace(mut self) -> Self {
        self.upsert = UpsertStrategy::Replace;
        self
    }

    /// Set the RETURNING clause (supported by PostgreSQL/SQLite 3.35+)
    ///
    /// Returns the values of specified columns after INSERT, commonly used to obtain
    /// auto-increment primary keys or default values. MySQL does not support RETURNING;
    /// it is ignored in `build()` (MySQL style) and only rendered for PostgreSQL/SQLite
    /// dialects in `build_with_dialect()`.
    ///
    /// # Example
    ///
    /// ```
    /// use sz_orm_core::DbType;
    /// use sz_orm_query_builder::Query;
    ///
    /// let sql = Query::insert()
    ///     .into_table("users")
    ///     .value("name", "'Alice'")
    ///     .returning(&["id", "created_at"])
    ///     .build_with_dialect(DbType::PostgreSQL);
    /// // INSERT INTO "users" ("name") VALUES ('Alice') RETURNING "id", "created_at"
    /// ```
    pub fn returning(mut self, columns: &[&str]) -> Self {
        self.returning = Some(columns.iter().map(|s| s.to_string()).collect());
        self
    }

    /// Set RETURNING * (return all columns)
    pub fn returning_all(mut self) -> Self {
        self.returning = Some(vec!["*".to_string()]);
        self
    }

    /// Build INSERT SQL (no dialect, hard-coded backticks, MySQL style)
    ///
    /// # Security (gate 9 fix)
    ///
    /// Identifiers are escaped via `quote_ident()` and wrapped in backticks, preventing
    /// malicious identifiers containing `` ` `` from breaking out.
    ///
    /// # Upsert behavior
    ///
    /// - `Replace`: generates `REPLACE INTO` instead of `INSERT INTO`
    /// - `OnDuplicateKeyUpdate`: appends `ON DUPLICATE KEY UPDATE` clause
    /// - `OnConflictDoNothing`/`OnConflictDoUpdate`: skipped (MySQL does not support)
    pub fn build(self) -> String {
        let table = self.table.unwrap_or_default();
        if table.is_empty() || self.columns.is_empty() {
            return String::new();
        }

        let cols: Vec<String> = self.columns.iter().map(|c| quote_ident(c)).collect();
        let vals: Vec<String> = self.values.iter().map(|v| v.to_string()).collect();

        // 根据是否 REPLACE 策略选择关键字
        let verb = match &self.upsert {
            UpsertStrategy::Replace => "REPLACE INTO",
            _ => "INSERT INTO",
        };
        let mut sql = format!(
            "{} {} ({}) VALUES ({})",
            verb,
            quote_ident(&table),
            cols.join(", "),
            vals.join(", ")
        );

        // MySQL 风格下仅支持 ON DUPLICATE KEY UPDATE
        if let UpsertStrategy::OnDuplicateKeyUpdate(assignments) = &self.upsert {
            let sets = assignments
                .iter()
                .map(|(c, v)| format!("{} = {}", quote_ident(c), v))
                .collect::<Vec<_>>()
                .join(", ");
            sql.push_str(&format!(" ON DUPLICATE KEY UPDATE {}", sets));
        }

        sql
    }

    /// Generate SQL for the specified dialect
    ///
    /// # Upsert dialect compatibility
    ///
    /// - MySQL family: supports `OnDuplicateKeyUpdate`, `Replace`
    /// - PostgreSQL/SQLite: supports `OnConflictDoNothing`, `OnConflictDoUpdate`
    /// - Incompatible combinations skip the upsert clause (no error)
    pub fn build_with_dialect(self, db_type: DbType) -> String {
        let dialect = match sz_orm_core::get_dialect(db_type) {
            Ok(d) => d,
            Err(_) => return String::new(),
        };

        let table = self.table.unwrap_or_default();
        if table.is_empty() || self.columns.is_empty() {
            return String::new();
        }

        let cols: Vec<String> = self.columns.iter().map(|c| dialect.quote(c)).collect();

        // 根据是否 REPLACE 策略和方言选择关键字
        let verb = match (&self.upsert, db_type) {
            (UpsertStrategy::Replace, dt) if is_mysql_family(dt) => "REPLACE INTO",
            (UpsertStrategy::Replace, DbType::Sqlite) => "INSERT OR REPLACE INTO",
            _ => "INSERT INTO",
        };
        let mut sql = format!(
            "{} {} ({}) VALUES ({})",
            verb,
            dialect.quote(&table),
            cols.join(", "),
            self.values.join(", ")
        );

        // 追加 upsert 子句（根据方言）
        if let Some(clause) = render_upsert_clause(&self.upsert, db_type) {
            sql.push(' ');
            sql.push_str(&clause);
        }

        // 追加 RETURNING 子句（PostgreSQL/SQLite 支持，MySQL 跳过）
        if let Some(clause) = render_returning_clause(&self.returning, db_type) {
            sql.push(' ');
            sql.push_str(&clause);
        }

        sql
    }
}

/// UPDATE query builder
#[derive(Debug, Clone, Default)]
pub struct UpdateQuery {
    table: Option<String>,
    sets: Vec<(String, String)>,
    wheres: Vec<String>,
    /// Parameterized WHERE conditions (P0 fix: SQL injection prevention)
    param_wheres: Vec<ParamWhere>,
    /// RETURNING clause column list (supported by PostgreSQL/SQLite, not MySQL)
    returning: Option<Vec<String>>,
}

impl UpdateQuery {
    /// Create an empty UPDATE query
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the target table
    pub fn table(mut self, table: &str) -> Self {
        self.table = Some(table.to_string());
        self
    }

    /// Add a SET assignment (value should be an already-escaped SQL literal)
    pub fn set(mut self, column: &str, value: &str) -> Self {
        self.sets.push((column.to_string(), value.to_string()));
        self
    }

    /// Batch add SET assignments
    pub fn sets(mut self, pairs: &[(&str, &str)]) -> Self {
        for (c, v) in pairs {
            self.sets.push((c.to_string(), v.to_string()));
        }
        self
    }

    /// Add a WHERE condition
    ///
    /// # Security (v0.2.2 fix C-6)
    ///
    /// Calls `check_where_injection` to detect high-risk patterns.
    pub fn where_clause(mut self, condition: &str) -> Self {
        check_where_injection(condition);
        self.wheres.push(condition.to_string());
        self
    }

    /// Add a `column = ?` AND condition (P0 fix: parameterized binding)
    pub fn where_eq(mut self, column: &str, value: Value) -> Self {
        self.param_wheres.push(ParamWhere::And {
            column: column.to_string(),
            op: " = ?".to_string(),
            values: vec![value],
        });
        self
    }

    /// Add a `column <> ?` AND condition
    pub fn where_ne(mut self, column: &str, value: Value) -> Self {
        self.param_wheres.push(ParamWhere::And {
            column: column.to_string(),
            op: " <> ?".to_string(),
            values: vec![value],
        });
        self
    }

    /// Add a `column > ?` AND condition
    pub fn where_gt(mut self, column: &str, value: Value) -> Self {
        self.param_wheres.push(ParamWhere::And {
            column: column.to_string(),
            op: " > ?".to_string(),
            values: vec![value],
        });
        self
    }

    /// Add a `column >= ?` AND condition
    pub fn where_ge(mut self, column: &str, value: Value) -> Self {
        self.param_wheres.push(ParamWhere::And {
            column: column.to_string(),
            op: " >= ?".to_string(),
            values: vec![value],
        });
        self
    }

    /// Add a `column < ?` AND condition
    pub fn where_lt(mut self, column: &str, value: Value) -> Self {
        self.param_wheres.push(ParamWhere::And {
            column: column.to_string(),
            op: " < ?".to_string(),
            values: vec![value],
        });
        self
    }

    /// Add a `column <= ?` AND condition
    pub fn where_le(mut self, column: &str, value: Value) -> Self {
        self.param_wheres.push(ParamWhere::And {
            column: column.to_string(),
            op: " <= ?".to_string(),
            values: vec![value],
        });
        self
    }

    /// Add a `column LIKE ?` AND condition
    pub fn where_like(mut self, column: &str, pattern: Value) -> Self {
        self.param_wheres.push(ParamWhere::And {
            column: column.to_string(),
            op: " LIKE ?".to_string(),
            values: vec![pattern],
        });
        self
    }

    /// Add a `column IN (?, ?, ...)` AND condition
    pub fn where_in(mut self, column: &str, values: Vec<Value>) -> Self {
        let (column, op) = if values.is_empty() {
            (String::new(), "1 = 0".to_string())
        } else {
            let placeholders: Vec<&str> = std::iter::repeat_n("?", values.len()).collect();
            (
                column.to_string(),
                format!(" IN ({})", placeholders.join(", ")),
            )
        };
        self.param_wheres
            .push(ParamWhere::And { column, op, values });
        self
    }

    /// Add a `column BETWEEN ? AND ?` AND condition
    pub fn where_between(mut self, column: &str, low: Value, high: Value) -> Self {
        self.param_wheres.push(ParamWhere::And {
            column: column.to_string(),
            op: " BETWEEN ? AND ?".to_string(),
            values: vec![low, high],
        });
        self
    }

    /// Add a `column IS NULL` AND condition
    pub fn where_null(mut self, column: &str) -> Self {
        self.param_wheres.push(ParamWhere::And {
            column: column.to_string(),
            op: " IS NULL".to_string(),
            values: vec![],
        });
        self
    }

    /// Add a `column IS NOT NULL` AND condition
    pub fn where_not_null(mut self, column: &str) -> Self {
        self.param_wheres.push(ParamWhere::And {
            column: column.to_string(),
            op: " IS NOT NULL".to_string(),
            values: vec![],
        });
        self
    }

    /// Set the RETURNING clause (supported by PostgreSQL/SQLite 3.35+)
    ///
    /// Returns the new values of specified columns after UPDATE. MySQL does not support
    /// RETURNING; it is ignored in `build()` (MySQL style) and under MySQL dialect in
    /// `build_with_dialect()`.
    pub fn returning(mut self, columns: &[&str]) -> Self {
        self.returning = Some(columns.iter().map(|s| s.to_string()).collect());
        self
    }

    /// Set RETURNING * (return all columns)
    pub fn returning_all(mut self) -> Self {
        self.returning = Some(vec!["*".to_string()]);
        self
    }

    /// Generate SQL
    ///
    /// # Security (gate 9 fix)
    ///
    /// Table and column names are escaped via `quote_ident()` and wrapped in backticks,
    /// preventing malicious identifiers containing `` ` `` from breaking out.
    pub fn build(self) -> String {
        let table = self.table.unwrap_or_default();
        if table.is_empty() || self.sets.is_empty() {
            return String::new();
        }

        let set_str: Vec<String> = self
            .sets
            .iter()
            .map(|(c, v)| format!("{} = {}", quote_ident(c), v))
            .collect();

        let mut sql = format!("UPDATE {} SET {}", quote_ident(&table), set_str.join(", "));

        if !self.wheres.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.wheres.join(" AND "));
        }

        sql
    }

    /// Generate SQL for the specified dialect
    pub fn build_with_dialect(self, db_type: DbType) -> String {
        let dialect = match sz_orm_core::get_dialect(db_type) {
            Ok(d) => d,
            Err(_) => return String::new(),
        };

        let table = self.table.unwrap_or_default();
        if table.is_empty() || self.sets.is_empty() {
            return String::new();
        }

        let set_str: Vec<String> = self
            .sets
            .iter()
            .map(|(c, v)| format!("{} = {}", dialect.quote(c), v))
            .collect();

        let mut sql = format!(
            "UPDATE {} SET {}",
            dialect.quote(&table),
            set_str.join(", ")
        );

        if !self.wheres.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.wheres.join(" AND "));
        }

        // 追加 RETURNING 子句（PostgreSQL/SQLite 支持，MySQL 跳过）
        if let Some(clause) = render_returning_clause(&self.returning, db_type) {
            sql.push(' ');
            sql.push_str(&clause);
        }

        sql
    }

    /// Generate parameterized SQL (parameterized query, P0 fix: SQL injection prevention)
    ///
    /// WHERE conditions can come from parameterized APIs like `where_eq`/`where_in`/`where_between`,
    /// with user input bound as parameters rather than string concatenation.
    /// Note: SET values are still raw strings; for parameterized SET, use the ORM's `save()` interface.
    pub fn build_with_params(self, db_type: DbType) -> BuiltQuery {
        let dialect = match sz_orm_core::get_dialect(db_type) {
            Ok(d) => d,
            Err(_) => return BuiltQuery::default(),
        };

        let table = self.table.unwrap_or_default();
        if table.is_empty() || self.sets.is_empty() {
            return BuiltQuery::default();
        }

        let set_str: Vec<String> = self
            .sets
            .iter()
            .map(|(c, v)| format!("{} = {}", dialect.quote(c), v))
            .collect();

        let mut sql = format!(
            "UPDATE {} SET {}",
            dialect.quote(&table),
            set_str.join(", ")
        );
        let mut params: Vec<Value> = Vec::new();

        let has_raw = !self.wheres.is_empty();
        let has_param = !self.param_wheres.is_empty();
        if has_raw || has_param {
            sql.push_str(" WHERE ");
            let mut first = true;
            for w in &self.wheres {
                if first {
                    sql.push_str(w);
                    first = false;
                } else {
                    sql.push_str(" AND ");
                    sql.push_str(w);
                }
            }
            for pw in &self.param_wheres {
                let (conjunction, column, op, vals) = match pw {
                    ParamWhere::And { column, op, values } => ("AND", column, op, values),
                    ParamWhere::Or { column, op, values } => ("OR", column, op, values),
                };
                // 按目标方言引用列名（PostgreSQL 双引号、MySQL 反引号）
                let expr = if column.is_empty() {
                    op.clone()
                } else {
                    format!("{}{}", quote_column_dialect(&*dialect, column), op)
                };
                if first {
                    sql.push_str(&expr);
                    first = false;
                } else {
                    sql.push(' ');
                    sql.push_str(conjunction);
                    sql.push(' ');
                    sql.push_str(&expr);
                }
                params.extend(vals.iter().cloned());
            }
        }

        // 追加 RETURNING 子句（PostgreSQL/SQLite 支持，MySQL 跳过）
        if let Some(clause) = render_returning_clause(&self.returning, db_type) {
            sql.push(' ');
            sql.push_str(&clause);
        }

        BuiltQuery { sql, params }
    }
}

/// DELETE query builder
#[derive(Debug, Clone, Default)]
pub struct DeleteQuery {
    table: Option<String>,
    wheres: Vec<String>,
    /// Parameterized WHERE conditions (P0 fix: SQL injection prevention)
    param_wheres: Vec<ParamWhere>,
    /// RETURNING clause column list (supported by PostgreSQL/SQLite, not MySQL)
    returning: Option<Vec<String>>,
}

impl DeleteQuery {
    /// Create an empty DELETE query
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the target table
    pub fn from_table(mut self, table: &str) -> Self {
        self.table = Some(table.to_string());
        self
    }

    /// Add a WHERE condition
    ///
    /// # Security (v0.2.2 fix C-6)
    ///
    /// Calls `check_where_injection` to detect high-risk patterns.
    pub fn where_clause(mut self, condition: &str) -> Self {
        check_where_injection(condition);
        self.wheres.push(condition.to_string());
        self
    }

    /// Add a `column = ?` AND condition (P0 fix: parameterized binding)
    pub fn where_eq(mut self, column: &str, value: Value) -> Self {
        self.param_wheres.push(ParamWhere::And {
            column: column.to_string(),
            op: " = ?".to_string(),
            values: vec![value],
        });
        self
    }

    /// Add a `column <> ?` AND condition
    pub fn where_ne(mut self, column: &str, value: Value) -> Self {
        self.param_wheres.push(ParamWhere::And {
            column: column.to_string(),
            op: " <> ?".to_string(),
            values: vec![value],
        });
        self
    }

    /// Add a `column > ?` AND condition
    pub fn where_gt(mut self, column: &str, value: Value) -> Self {
        self.param_wheres.push(ParamWhere::And {
            column: column.to_string(),
            op: " > ?".to_string(),
            values: vec![value],
        });
        self
    }

    /// Add a `column >= ?` AND condition
    pub fn where_ge(mut self, column: &str, value: Value) -> Self {
        self.param_wheres.push(ParamWhere::And {
            column: column.to_string(),
            op: " >= ?".to_string(),
            values: vec![value],
        });
        self
    }

    /// Add a `column < ?` AND condition
    pub fn where_lt(mut self, column: &str, value: Value) -> Self {
        self.param_wheres.push(ParamWhere::And {
            column: column.to_string(),
            op: " < ?".to_string(),
            values: vec![value],
        });
        self
    }

    /// Add a `column <= ?` AND condition
    pub fn where_le(mut self, column: &str, value: Value) -> Self {
        self.param_wheres.push(ParamWhere::And {
            column: column.to_string(),
            op: " <= ?".to_string(),
            values: vec![value],
        });
        self
    }

    /// Add a `column LIKE ?` AND condition
    pub fn where_like(mut self, column: &str, pattern: Value) -> Self {
        self.param_wheres.push(ParamWhere::And {
            column: column.to_string(),
            op: " LIKE ?".to_string(),
            values: vec![pattern],
        });
        self
    }

    /// Add a `column IN (?, ?, ...)` AND condition
    pub fn where_in(mut self, column: &str, values: Vec<Value>) -> Self {
        let (column, op) = if values.is_empty() {
            (String::new(), "1 = 0".to_string())
        } else {
            let placeholders: Vec<&str> = std::iter::repeat_n("?", values.len()).collect();
            (
                column.to_string(),
                format!(" IN ({})", placeholders.join(", ")),
            )
        };
        self.param_wheres
            .push(ParamWhere::And { column, op, values });
        self
    }

    /// Add a `column BETWEEN ? AND ?` AND condition
    pub fn where_between(mut self, column: &str, low: Value, high: Value) -> Self {
        self.param_wheres.push(ParamWhere::And {
            column: column.to_string(),
            op: " BETWEEN ? AND ?".to_string(),
            values: vec![low, high],
        });
        self
    }

    /// Add a `column IS NULL` AND condition
    pub fn where_null(mut self, column: &str) -> Self {
        self.param_wheres.push(ParamWhere::And {
            column: column.to_string(),
            op: " IS NULL".to_string(),
            values: vec![],
        });
        self
    }

    /// Add a `column IS NOT NULL` AND condition
    pub fn where_not_null(mut self, column: &str) -> Self {
        self.param_wheres.push(ParamWhere::And {
            column: column.to_string(),
            op: " IS NOT NULL".to_string(),
            values: vec![],
        });
        self
    }

    /// Set the RETURNING clause (supported by PostgreSQL/SQLite 3.35+)
    ///
    /// Returns the specified column values of deleted rows after DELETE. MySQL does not
    /// support RETURNING; it is ignored in `build()` (MySQL style) and under MySQL dialect
    /// in `build_with_dialect()`.
    pub fn returning(mut self, columns: &[&str]) -> Self {
        self.returning = Some(columns.iter().map(|s| s.to_string()).collect());
        self
    }

    /// Set RETURNING * (return all columns)
    pub fn returning_all(mut self) -> Self {
        self.returning = Some(vec!["*".to_string()]);
        self
    }

    /// Generate SQL
    ///
    /// # Security (gate 9 fix)
    ///
    /// The table name is escaped via `quote_ident()` and wrapped in backticks, preventing
    /// malicious table names containing `` ` `` from breaking out.
    pub fn build(self) -> String {
        let table = self.table.unwrap_or_default();
        if table.is_empty() {
            return String::new();
        }

        let mut sql = format!("DELETE FROM {}", quote_ident(&table));

        if !self.wheres.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.wheres.join(" AND "));
        }

        sql
    }

    /// Generate SQL for the specified dialect
    pub fn build_with_dialect(self, db_type: DbType) -> String {
        let dialect = match sz_orm_core::get_dialect(db_type) {
            Ok(d) => d,
            Err(_) => return String::new(),
        };

        let table = self.table.unwrap_or_default();
        if table.is_empty() {
            return String::new();
        }

        let mut sql = format!("DELETE FROM {}", dialect.quote(&table));

        if !self.wheres.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.wheres.join(" AND "));
        }

        // 追加 RETURNING 子句（PostgreSQL/SQLite 支持，MySQL 跳过）
        if let Some(clause) = render_returning_clause(&self.returning, db_type) {
            sql.push(' ');
            sql.push_str(&clause);
        }

        sql
    }

    /// Generate parameterized SQL (parameterized query, P0 fix: SQL injection prevention)
    ///
    /// WHERE conditions can come from parameterized APIs like `where_eq`/`where_in`/`where_between`,
    /// with user input bound as parameters rather than string concatenation.
    pub fn build_with_params(self, db_type: DbType) -> BuiltQuery {
        let dialect = match sz_orm_core::get_dialect(db_type) {
            Ok(d) => d,
            Err(_) => return BuiltQuery::default(),
        };

        let table = self.table.unwrap_or_default();
        if table.is_empty() {
            return BuiltQuery::default();
        }

        let mut sql = format!("DELETE FROM {}", dialect.quote(&table));
        let mut params: Vec<Value> = Vec::new();

        let has_raw = !self.wheres.is_empty();
        let has_param = !self.param_wheres.is_empty();
        if has_raw || has_param {
            sql.push_str(" WHERE ");
            let mut first = true;
            for w in &self.wheres {
                if first {
                    sql.push_str(w);
                    first = false;
                } else {
                    sql.push_str(" AND ");
                    sql.push_str(w);
                }
            }
            for pw in &self.param_wheres {
                let (conjunction, column, op, vals) = match pw {
                    ParamWhere::And { column, op, values } => ("AND", column, op, values),
                    ParamWhere::Or { column, op, values } => ("OR", column, op, values),
                };
                // 按目标方言引用列名（PostgreSQL 双引号、MySQL 反引号）
                let expr = if column.is_empty() {
                    op.clone()
                } else {
                    format!("{}{}", quote_column_dialect(&*dialect, column), op)
                };
                if first {
                    sql.push_str(&expr);
                    first = false;
                } else {
                    sql.push(' ');
                    sql.push_str(conjunction);
                    sql.push(' ');
                    sql.push_str(&expr);
                }
                params.extend(vals.iter().cloned());
            }
        }

        // 追加 RETURNING 子句（PostgreSQL/SQLite 支持，MySQL 跳过）
        if let Some(clause) = render_returning_clause(&self.returning, db_type) {
            sql.push(' ');
            sql.push_str(&clause);
        }

        BuiltQuery { sql, params }
    }
}

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::*;

    // ---- Query::select 测试 ----

    #[test]
    fn test_select_basic() {
        let sql = Query::select()
            .column("id")
            .column("name")
            .from("users")
            .build(DbType::MySQL);
        assert!(sql.starts_with("SELECT "));
        assert!(sql.contains("`id`"));
        assert!(sql.contains("`name`"));
        assert!(sql.contains("FROM `users`"));
    }

    #[test]
    fn test_select_star() {
        let sql = Query::select()
            .all_columns()
            .from("users")
            .build(DbType::MySQL);
        assert!(sql.contains("SELECT *"));
        assert!(sql.contains("FROM `users`"));
    }

    #[test]
    fn test_select_distinct() {
        let sql = Query::select()
            .distinct()
            .column("name")
            .from("users")
            .build(DbType::MySQL);
        assert!(sql.contains("SELECT DISTINCT"));
    }

    #[test]
    fn test_select_with_where() {
        let sql = Query::select()
            .column("id")
            .from("users")
            .where_clause("age > 18")
            .where_clause("status = 'active'")
            .build(DbType::MySQL);
        assert!(sql.contains("WHERE age > 18 AND status = 'active'"));
    }

    #[test]

    fn test_select_with_inner_join() {
        let sql = Query::select()
            .column("u.id")
            .from("users u")
            .inner_join("orders o", "u.id = o.user_id")
            .build(DbType::MySQL);
        assert!(sql.contains("INNER JOIN `orders` o ON u.id = o.user_id"));
    }

    #[test]
    fn test_select_with_left_join() {
        let sql = Query::select()
            .column("u.id")
            .from("users u")
            .left_join("profiles p", "u.id = p.user_id")
            .build(DbType::MySQL);
        assert!(sql.contains("LEFT JOIN `profiles` p ON u.id = p.user_id"));
    }

    #[test]
    fn test_select_with_order_by() {
        let sql = Query::select()
            .column("id")
            .from("users")
            .order_by("created_at", true)
            .order_by("id", false)
            .build(DbType::MySQL);
        assert!(sql.contains("ORDER BY `created_at` ASC, `id` DESC"));
    }

    #[test]
    fn test_select_with_limit_offset() {
        let sql = Query::select()
            .column("id")
            .from("users")
            .limit(10)
            .offset(20)
            .build(DbType::MySQL);
        assert!(sql.contains("LIMIT 10"));
        assert!(sql.contains("OFFSET 20"));
    }

    #[test]
    fn test_select_paginate() {
        let sql = Query::select()
            .column("id")
            .from("users")
            .paginate(3, 20)
            .build(DbType::MySQL);
        // page 3, size 20 -> offset = (3-1)*20 = 40
        assert!(sql.contains("LIMIT 20"));
        assert!(sql.contains("OFFSET 40"));
    }

    #[test]
    fn test_select_with_group_by_having() {
        let sql = Query::select()
            .column("status")
            .from("users")
            .group_by("status")
            .having("COUNT(*) > 5")
            .build(DbType::MySQL);
        assert!(sql.contains("GROUP BY `status`"));
        assert!(sql.contains("HAVING COUNT(*) > 5"));
    }

    #[test]
    fn test_select_postgres_dialect() {
        let sql = Query::select()
            .column("id")
            .from("users")
            .build(DbType::PostgreSQL);
        assert!(sql.contains("\"id\""));
        assert!(sql.contains("FROM \"users\""));
    }

    #[test]
    fn test_select_sqlite_dialect() {
        let sql = Query::select()
            .column("id")
            .from("users")
            .build(DbType::Sqlite);
        assert!(sql.contains("\"id\""));
    }

    #[test]
    fn test_select_multiple_joins() {
        let sql = Query::select()
            .column("u.id")
            .from("users u")
            .inner_join("orders o", "u.id = o.user_id")
            .left_join("profiles p", "u.id = p.user_id")
            .build(DbType::MySQL);
        assert!(sql.contains("INNER JOIN `orders` o"));
        assert!(sql.contains("LEFT JOIN `profiles` p"));
    }

    #[test]
    fn test_select_columns_multiple() {
        let sql = Query::select()
            .columns(&["id", "name", "email"])
            .from("users")
            .build(DbType::MySQL);
        assert!(sql.contains("`id`, `name`, `email`"));
    }

    #[test]
    fn test_select_no_columns_defaults_star() {
        let sql = Query::select().from("users").build(DbType::MySQL);
        assert!(sql.contains("SELECT *"));
    }

    // ---- Query::insert 测试 ----

    #[test]
    fn test_insert_basic() {
        let sql = Query::insert()
            .into_table("users")
            .value("name", "'Alice'")
            .value("age", "30")
            .build();
        assert!(sql.starts_with("INSERT INTO `users`"));
        assert!(sql.contains("`name`, `age`"));
        assert!(sql.contains("'Alice', 30"));
    }

    #[test]
    fn test_insert_values_batch() {
        let sql = Query::insert()
            .into_table("users")
            .values(&[("name", "'Bob'"), ("age", "25"), ("email", "'bob@x.com'")])
            .build();
        assert!(sql.contains("`name`, `age`, `email`"));
        assert!(sql.contains("'Bob', 25, 'bob@x.com'"));
    }

    #[test]
    fn test_insert_empty_returns_empty() {
        let sql = Query::insert().into_table("users").build();
        assert_eq!(sql, "");
    }

    #[test]
    fn test_insert_with_dialect() {
        let sql = Query::insert()
            .into_table("users")
            .value("name", "'Alice'")
            .build_with_dialect(DbType::PostgreSQL);
        assert!(sql.contains("\"name\""));
        assert!(sql.contains("\"users\""));
    }

    // ---- Query::update 测试 ----

    #[test]
    fn test_update_basic() {
        let sql = Query::update()
            .table("users")
            .set("name", "'Bob'")
            .where_clause("id = 1")
            .build();
        assert!(sql.starts_with("UPDATE `users` SET"));
        assert!(sql.contains("`name` = 'Bob'"));
        assert!(sql.contains("WHERE id = 1"));
    }

    #[test]
    fn test_update_multiple_sets() {
        let sql = Query::update()
            .table("users")
            .sets(&[("name", "'Bob'"), ("age", "30")])
            .where_clause("id = 1")
            .build();
        assert!(sql.contains("`name` = 'Bob', `age` = 30"));
    }

    #[test]
    fn test_update_no_where() {
        let sql = Query::update()
            .table("users")
            .set("status", "'active'")
            .build();
        assert!(sql.contains("UPDATE `users` SET `status` = 'active'"));
        assert!(!sql.contains("WHERE"));
    }

    #[test]
    fn test_update_empty_returns_empty() {
        let sql = Query::update().table("users").build();
        assert_eq!(sql, "");
    }

    #[test]
    fn test_update_with_dialect() {
        let sql = Query::update()
            .table("users")
            .set("name", "'Bob'")
            .build_with_dialect(DbType::PostgreSQL);
        assert!(sql.contains("\"users\""));
        assert!(sql.contains("\"name\""));
    }

    // ---- Query::delete 测试 ----

    #[test]
    fn test_delete_basic() {
        let sql = Query::delete()
            .from_table("users")
            .where_clause("id = 1")
            .build();
        assert!(sql.starts_with("DELETE FROM `users`"));
        assert!(sql.contains("WHERE id = 1"));
    }

    #[test]
    fn test_delete_no_where() {
        let sql = Query::delete().from_table("users").build();
        assert!(sql.contains("DELETE FROM `users`"));
        assert!(!sql.contains("WHERE"));
    }

    #[test]
    fn test_delete_multiple_wheres() {
        let sql = Query::delete()
            .from_table("users")
            .where_clause("id > 100")
            .where_clause("status = 'inactive'")
            .build();
        assert!(sql.contains("WHERE id > 100 AND status = 'inactive'"));
    }

    #[test]
    fn test_delete_empty_returns_empty() {
        let sql = Query::delete().build();
        assert_eq!(sql, "");
    }

    #[test]
    fn test_delete_with_dialect() {
        let sql = Query::delete()
            .from_table("users")
            .where_clause("id = 1")
            .build_with_dialect(DbType::PostgreSQL);
        assert!(sql.contains("\"users\""));
    }

    // ---- 完整流程测试 ----

    #[test]
    fn test_full_crud_flow() {
        // CREATE (用 INSERT 模拟)
        let insert = Query::insert()
            .into_table("users")
            .value("name", "'Alice'")
            .value("age", "30")
            .build();
        assert!(insert.contains("INSERT INTO"));

        // READ
        let select = Query::select()
            .column("id")
            .column("name")
            .from("users")
            .where_clause("age > 18")
            .order_by("id", true)
            .limit(10)
            .build(DbType::MySQL);
        assert!(select.contains("SELECT"));
        assert!(select.contains("FROM"));
        assert!(select.contains("WHERE"));
        assert!(select.contains("ORDER BY"));
        assert!(select.contains("LIMIT"));

        // UPDATE
        let update = Query::update()
            .table("users")
            .set("name", "'Bob'")
            .where_clause("id = 1")
            .build();
        assert!(update.contains("UPDATE"));
        assert!(update.contains("SET"));
        assert!(update.contains("WHERE"));

        // DELETE
        let delete = Query::delete()
            .from_table("users")
            .where_clause("id = 1")
            .build();
        assert!(delete.contains("DELETE FROM"));
    }

    #[test]
    fn test_complex_select_query() {
        let sql = Query::select()
            .distinct()
            .columns(&["u.id", "u.name", "o.total"])
            .from("users u")
            .inner_join("orders o", "u.id = o.user_id")
            .where_clause("u.status = 'active'")
            .where_clause("o.total > 100")
            .group_by("u.id")
            .having("SUM(o.total) > 1000")
            .order_by("u.id", true)
            .limit(20)
            .offset(40)
            .build(DbType::MySQL);

        assert!(sql.contains("SELECT DISTINCT"));
        assert!(sql.contains("INNER JOIN `orders` o"));
        assert!(sql.contains("WHERE u.status = 'active' AND o.total > 100"));
        assert!(sql.contains("GROUP BY"));
        assert!(sql.contains("HAVING SUM(o.total) > 1000"));
        assert!(sql.contains("ORDER BY `u`.`id` ASC"));
        assert!(sql.contains("LIMIT 20"));
        assert!(sql.contains("OFFSET 40"));
    }

    // ---- v0.2.2 修复 C-6：SQL 注入测试 ----

    #[test]
    #[should_panic(expected = "SQL injection detected")]
    fn test_select_where_rejects_semicolon_drop() {
        let _ = Query::select()
            .column("id")
            .from("users")
            .where_clause("1=1; DROP TABLE users")
            .build(DbType::MySQL);
    }

    #[test]
    #[should_panic(expected = "SQL injection detected")]
    fn test_select_where_rejects_semicolon_space_drop() {
        let _ = Query::select()
            .column("id")
            .from("users")
            .where_clause("1=1; DROP TABLE users")
            .build(DbType::MySQL);
    }

    #[test]
    #[should_panic(expected = "SQL injection detected")]
    fn test_select_where_rejects_line_comment() {
        let _ = Query::select()
            .column("id")
            .from("users")
            .where_clause("id = 1 -- DROP TABLE users")
            .build(DbType::MySQL);
    }

    #[test]
    #[should_panic(expected = "SQL injection detected")]
    fn test_select_where_rejects_block_comment() {
        let _ = Query::select()
            .column("id")
            .from("users")
            .where_clause("id = 1 /* comment */ OR 1=1")
            .build(DbType::MySQL);
    }

    #[test]
    #[should_panic(expected = "SQL injection detected")]
    fn test_update_where_rejects_delete() {
        let _ = Query::update()
            .table("users")
            .set("name", "'x'")
            .where_clause("1=1; DELETE FROM users")
            .build();
    }

    #[test]
    #[should_panic(expected = "SQL injection detected")]
    fn test_update_where_rejects_line_comment() {
        let _ = Query::update()
            .table("users")
            .set("name", "'x'")
            .where_clause("id = 1 -- bypass")
            .build();
    }

    #[test]
    #[should_panic(expected = "SQL injection detected")]
    fn test_delete_where_rejects_drop() {
        let _ = Query::delete()
            .from_table("users")
            .where_clause("1=1; DROP TABLE users")
            .build();
    }

    #[test]
    #[should_panic(expected = "SQL injection detected")]
    fn test_delete_where_rejects_block_comment() {
        let _ = Query::delete()
            .from_table("users")
            .where_clause("id = 1 /* */ OR 1=1")
            .build();
    }

    #[test]
    #[should_panic(expected = "SQL injection detected")]
    fn test_delete_where_rejects_line_comment() {
        let _ = Query::delete()
            .from_table("users")
            .where_clause("id = 1--")
            .build();
    }

    #[test]
    fn test_safe_where_clauses_pass() {
        // 这些是合法的 WHERE 条件，不应触发 panic——同时验证生成 SQL 包含预期的 WHERE 子句
        let sql_str = Query::select()
            .column("id")
            .from("users")
            .where_clause("age > 18")
            .where_clause("name = 'Alice;Bob'") // 分号在字符串字面量中
            .where_clause("id IN (1, 2, 3)")
            .where_clause("created_at > '2026-01-01'")
            .build(DbType::MySQL);
        assert!(!sql_str.is_empty(), "SELECT SQL 不应为空");
        assert!(sql_str.contains("age > 18"), "SELECT 应包含 age > 18 条件");
        assert!(
            sql_str.contains("name = 'Alice;Bob'"),
            "SELECT 应包含 name 条件（含分号字面量）"
        );
        assert!(sql_str.contains("id IN (1, 2, 3)"), "SELECT 应包含 IN 子句");
        assert!(
            sql_str.contains("created_at > '2026-01-01'"),
            "SELECT 应包含日期条件"
        );

        let sql_str = Query::update()
            .table("users")
            .set("name", "'x'")
            .where_clause("id = 1")
            .build();
        assert!(!sql_str.is_empty(), "UPDATE SQL 不应为空");
        assert!(sql_str.contains("UPDATE"), "应为 UPDATE 语句");
        assert!(sql_str.contains("WHERE"), "UPDATE 应包含 WHERE 子句");
        assert!(sql_str.contains("id = 1"), "UPDATE WHERE 应包含 id = 1");

        let sql_str = Query::delete()
            .from_table("users")
            .where_clause("id = 1")
            .build();
        assert!(!sql_str.is_empty(), "DELETE SQL 不应为空");
        assert!(sql_str.contains("DELETE"), "应为 DELETE 语句");
        assert!(sql_str.contains("WHERE"), "DELETE 应包含 WHERE 子句");
        assert!(sql_str.contains("id = 1"), "DELETE WHERE 应包含 id = 1");
    }

    // ---- 变异测试专项补防测试（杀死存活的变异体） ----
    // 目标：覆盖 `||` → `&&` 变异（`check_where_injection` 和 `build_with_dialect` 守卫条件）

    #[test]
    #[should_panic(expected = "SQL injection detected")]
    fn test_mutant_block_comment_open_only() {
        // 杀死：`contains("/*") || contains("*/")` → `&&` 变异
        // 仅含 `/*` 无 `*/`，`||` 应 panic，`&&` 不会 panic
        let _ = Query::select()
            .column("id")
            .from("users")
            .where_clause("id = 1 /* OR 1=1")
            .build(DbType::MySQL);
    }

    #[test]
    #[should_panic(expected = "SQL injection detected")]
    fn test_mutant_block_comment_close_only() {
        // 仅含 `*/` 无 `/*`
        let _ = Query::select()
            .column("id")
            .from("users")
            .where_clause("id = 1 */")
            .build(DbType::MySQL);
    }

    #[test]
    fn test_mutant_insert_dialect_table_no_columns_returns_empty() {
        // 杀死 `||` → `&&` 变异：有 table 无 columns → `||` 返回空，`&&` 不返回空
        let sql = Query::insert()
            .into_table("users")
            .build_with_dialect(DbType::MySQL);
        assert_eq!(sql, "", "有表无列时应返回空字符串");
    }

    #[test]
    fn test_mutant_update_dialect_table_no_sets_returns_empty() {
        // 杀死 `||` → `&&` 变异
        let sql = Query::update()
            .table("users")
            .build_with_dialect(DbType::MySQL);
        assert_eq!(sql, "", "有表无 SET 时应返回空字符串");
    }

    #[test]
    fn test_mutant_update_dialect_no_table_with_sets_returns_empty() {
        // 杀死 `delete !` 变异：无表有 SET → 应返回空
        let sql = Query::update()
            .set("name", "'x'")
            .build_with_dialect(DbType::MySQL);
        assert_eq!(sql, "", "无表有 SET 时应返回空字符串");
    }

    #[test]
    fn test_mutant_delete_dialect_no_table_returns_empty() {
        // 杀死 `delete !` 变异
        let sql = Query::delete()
            .where_clause("id = 1")
            .build_with_dialect(DbType::MySQL);
        assert_eq!(sql, "", "无表时应返回空字符串");
    }

    #[test]
    fn test_mutant_select_right_join() {
        // 杀死 `right_join -> Self` → `Default::default()` 变异
        let sql = Query::select()
            .column("u.id")
            .from("users u")
            .right_join("orders o", "u.id = o.user_id")
            .build(DbType::MySQL);
        assert!(sql.contains("RIGHT JOIN `orders` o ON u.id = o.user_id"));
    }

    #[test]
    fn test_mutant_all_columns_with_extra() {
        // 杀死 `all_columns -> Self` → `Default::default()` 变异
        // 原始：`SELECT *, `extra` FROM`；变异体（default）：`SELECT `extra` FROM`
        let sql = Query::select()
            .all_columns()
            .column("extra")
            .from("users")
            .build(DbType::MySQL);
        assert!(
            sql.contains("SELECT *, `extra` FROM `users`"),
            "all_columns + column 应在 SELECT 列表中同时包含 * 和 extra，实际: {sql}"
        );
    }

    #[test]
    fn test_mutant_update_dialect_no_where_no_where_clause() {
        // 杀死 `delete !` 在 build_with_dialect WHERE 守卫
        // 原始：`if !self.wheres.is_empty()`，变异体：`if self.wheres.is_empty()`
        let sql = Query::update()
            .table("users")
            .set("name", "'x'")
            .build_with_dialect(DbType::MySQL);
        assert!(
            !sql.contains("WHERE"),
            "无 WHERE 条件时不应包含 WHERE 关键字，实际: {sql}"
        );
    }

    #[test]
    fn test_mutant_delete_dialect_no_where_no_where_clause() {
        // 杀死 `delete !` 在 DeleteQuery::build_with_dialect WHERE 守卫
        let sql = Query::delete()
            .from_table("users")
            .build_with_dialect(DbType::MySQL);
        assert!(
            !sql.contains("WHERE"),
            "无 WHERE 条件时不应包含 WHERE 关键字，实际: {sql}"
        );
    }

    // ==================== 深度扩展：CTE / 窗口函数 / 集合运算 / FOR UPDATE 测试 ====================

    // ---- CTE 测试 ----

    #[test]
    fn test_cte_single_with_clause() {
        let sql = Query::select()
            .column("id")
            .column("name")
            .from("active_users")
            .with_cte(
                "active_users",
                "SELECT * FROM users WHERE status = 'active'",
            )
            .build(DbType::MySQL);
        assert!(sql.starts_with("WITH active_users AS ("));
        assert!(sql.contains("SELECT * FROM users WHERE status = 'active'"));
        assert!(sql.contains("SELECT `id`, `name` FROM `active_users`"));
    }

    #[test]
    fn test_cte_multiple_with_clauses() {
        let sql = Query::select()
            .column("id")
            .from("combined")
            .with_cte("a", "SELECT id FROM table_a")
            .with_cte("b", "SELECT id FROM table_b")
            .with_cte("combined", "SELECT id FROM a UNION SELECT id FROM b")
            .build(DbType::MySQL);
        assert!(sql.starts_with(
            "WITH a AS (SELECT id FROM table_a), b AS (SELECT id FROM table_b), combined AS ("
        ));
    }

    #[test]
    fn test_cte_recursive_with_clause() {
        let sql = Query::select()
            .column("id")
            .column("parent_id")
            .from("tree")
            .with_recursive_cte("tree", "SELECT id, parent_id FROM nodes WHERE id = 1")
            .build(DbType::MySQL);
        assert!(sql.starts_with("WITH RECURSIVE tree AS ("));
    }

    #[test]
    fn test_cte_no_cte_no_with_prefix() {
        let sql = Query::select()
            .column("id")
            .from("users")
            .build(DbType::MySQL);
        assert!(!sql.contains("WITH"));
        assert!(sql.starts_with("SELECT"));
    }

    // ---- 窗口函数测试 ----

    #[test]
    fn test_window_function_raw_expr() {
        let sql = Query::select()
            .column("id")
            .column("salary")
            .from("employees")
            .window_function("ROW_NUMBER() OVER (ORDER BY salary DESC) AS rn")
            .build(DbType::MySQL);
        assert!(sql.contains("ROW_NUMBER() OVER (ORDER BY salary DESC) AS rn"));
    }

    #[test]
    fn test_row_number_helper_with_partition() {
        let sql = Query::select()
            .column("name")
            .column("dept")
            .from("employees")
            .row_number("dept", "salary DESC", "row_num")
            .build(DbType::MySQL);
        assert!(
            sql.contains("ROW_NUMBER() OVER (PARTITION BY dept ORDER BY salary DESC) AS row_num")
        );
    }

    #[test]
    fn test_row_number_helper_without_partition() {
        let sql = Query::select()
            .column("name")
            .from("employees")
            .row_number("", "salary DESC", "rn")
            .build(DbType::MySQL);
        assert!(sql.contains("ROW_NUMBER() OVER (ORDER BY salary DESC) AS rn"));
        assert!(!sql.contains("PARTITION BY"));
    }

    #[test]
    fn test_rank_helper() {
        let sql = Query::select()
            .column("name")
            .from("scores")
            .rank("", "score DESC", "rank_num")
            .build(DbType::MySQL);
        assert!(sql.contains("RANK() OVER (ORDER BY score DESC) AS rank_num"));
    }

    #[test]
    fn test_dense_rank_helper_with_partition() {
        let sql = Query::select()
            .column("name")
            .from("scores")
            .dense_rank("class", "score DESC", "dr")
            .build(DbType::MySQL);
        assert!(sql.contains("DENSE_RANK() OVER (PARTITION BY class ORDER BY score DESC) AS dr"));
    }

    #[test]
    fn test_multiple_window_functions() {
        let sql = Query::select()
            .column("name")
            .column("salary")
            .from("employees")
            .row_number("dept", "salary DESC", "rn")
            .rank("dept", "salary DESC", "rk")
            .dense_rank("dept", "salary DESC", "dr")
            .build(DbType::MySQL);
        assert!(sql.contains("ROW_NUMBER()"));
        assert!(sql.contains("RANK()"));
        assert!(sql.contains("DENSE_RANK()"));
    }

    #[test]
    fn test_window_function_with_cte_combined() {
        let sql = Query::select()
            .column("name")
            .from("ranked")
            .with_cte(
                "ranked",
                "SELECT name, ROW_NUMBER() OVER (ORDER BY salary) AS rn FROM employees",
            )
            .where_clause("rn <= 10")
            .build(DbType::MySQL);
        assert!(sql.starts_with("WITH ranked AS ("));
        assert!(sql.contains("FROM `ranked`"));
        assert!(sql.contains("WHERE rn <= 10"));
    }

    // ---- FOR UPDATE 测试 ----

    #[test]
    fn test_for_update_basic() {
        let sql = Query::select()
            .column("id")
            .column("balance")
            .from("accounts")
            .where_clause("id = 1")
            .for_update()
            .build(DbType::MySQL);
        assert!(sql.ends_with(" FOR UPDATE"));
        assert!(sql.contains("WHERE id = 1"));
    }

    #[test]
    fn test_for_update_with_nowait() {
        let sql = Query::select()
            .column("id")
            .from("accounts")
            .where_clause("id = 1")
            .for_update_with_options("NOWAIT")
            .build(DbType::MySQL);
        assert!(sql.ends_with(" FOR UPDATE NOWAIT"));
    }

    #[test]
    fn test_for_update_with_skip_locked() {
        let sql = Query::select()
            .column("id")
            .from("accounts")
            .where_clause("id = 1")
            .for_update_with_options("SKIP LOCKED")
            .build(DbType::MySQL);
        assert!(sql.ends_with(" FOR UPDATE SKIP LOCKED"));
    }

    #[test]
    fn test_for_update_with_limit_and_order() {
        let sql = Query::select()
            .column("id")
            .from("jobs")
            .order_by("priority", false)
            .limit(1)
            .for_update_with_options("SKIP LOCKED")
            .build(DbType::MySQL);
        assert!(sql.contains("ORDER BY `priority` DESC"));
        assert!(sql.contains("LIMIT 1"));
        assert!(sql.ends_with(" FOR UPDATE SKIP LOCKED"));
    }

    #[test]
    fn test_no_for_update_by_default() {
        let sql = Query::select()
            .column("id")
            .from("users")
            .build(DbType::MySQL);
        assert!(!sql.contains("FOR UPDATE"));
    }

    // ---- 集合运算（UNION / INTERSECT / EXCEPT）测试 ----

    #[test]
    fn test_set_operator_as_sql() {
        assert_eq!(SetOperator::Union.as_sql(), "UNION");
        assert_eq!(SetOperator::UnionAll.as_sql(), "UNION ALL");
        assert_eq!(SetOperator::Intersect.as_sql(), "INTERSECT");
        assert_eq!(SetOperator::Except.as_sql(), "EXCEPT");
    }

    #[test]
    fn test_union_basic() {
        let q1 = Query::select().column("id").from("active_users");
        let q2 = Query::select().column("id").from("pending_users");
        let sql = q1.union(q2).build(DbType::MySQL);
        assert!(sql.contains("SELECT `id` FROM `active_users`"));
        assert!(sql.contains(" UNION "));
        assert!(sql.contains("SELECT `id` FROM `pending_users`"));
    }

    #[test]
    fn test_union_all_basic() {
        let q1 = Query::select().column("id").from("table_a");
        let q2 = Query::select().column("id").from("table_b");
        let sql = q1.union_all(q2).build(DbType::MySQL);
        assert!(sql.contains(" UNION ALL "));
    }

    #[test]
    fn test_intersect_basic() {
        let q1 = Query::select().column("id").from("table_a");
        let q2 = Query::select().column("id").from("table_b");
        let sql = q1.intersect(q2).build(DbType::MySQL);
        assert!(sql.contains(" INTERSECT "));
    }

    #[test]
    fn test_except_basic() {
        let q1 = Query::select().column("id").from("table_a");
        let q2 = Query::select().column("id").from("table_b");
        let sql = q1.except(q2).build(DbType::MySQL);
        assert!(sql.contains(" EXCEPT "));
    }

    #[test]
    fn test_union_chained_multiple() {
        let q1 = Query::select().column("id").from("t1");
        let q2 = Query::select().column("id").from("t2");
        let q3 = Query::select().column("id").from("t3");
        let sql = q1.union(q2).union(q3).build(DbType::MySQL);
        assert_eq!(sql.matches("UNION").count(), 2);
    }

    #[test]
    fn test_union_mixed_operators() {
        let q1 = Query::select().column("id").from("t1");
        let q2 = Query::select().column("id").from("t2");
        let q3 = Query::select().column("id").from("t3");
        let sql = q1.union(q2).intersect(q3).build(DbType::MySQL);
        assert!(sql.contains(" UNION "));
        assert!(sql.contains(" INTERSECT "));
    }

    #[test]
    fn test_union_with_order_by_limit() {
        let q1 = Query::select().column("id").from("t1");
        let q2 = Query::select().column("id").from("t2");
        let sql = q1
            .union(q2)
            .order_by("id", true)
            .limit(10)
            .offset(5)
            .build(DbType::MySQL);
        assert!(sql.contains("ORDER BY `id` ASC"));
        assert!(sql.contains("LIMIT 10"));
        assert!(sql.contains("OFFSET 5"));
    }

    #[test]
    fn test_union_postgres_dialect() {
        let q1 = Query::select().column("id").from("t1");
        let q2 = Query::select().column("id").from("t2");
        let sql = q1.union(q2).build(DbType::PostgreSQL);
        assert!(sql.contains("\"id\""));
        assert!(sql.contains(" UNION "));
    }

    #[test]
    fn test_union_with_where_clauses() {
        let q1 = Query::select()
            .column("id")
            .from("active_users")
            .where_clause("age > 18");
        let q2 = Query::select()
            .column("id")
            .from("pending_users")
            .where_clause("age > 18");
        let sql = q1.union(q2).build(DbType::MySQL);
        assert!(sql.contains("WHERE age > 18"));
        assert!(sql.contains(" UNION "));
    }

    // ---- 综合场景测试 ----

    #[test]
    fn test_cte_window_for_update_combined() {
        // 复杂查询：CTE + 窗口函数 + FOR UPDATE
        let sql = Query::select()
            .column("id")
            .column("salary")
            .from("ranked_salaries")
            .with_cte(
                "ranked_salaries",
                "SELECT id, salary, ROW_NUMBER() OVER (ORDER BY salary DESC) AS rn FROM employees",
            )
            .where_clause("rn = 1")
            .for_update()
            .build(DbType::MySQL);
        assert!(sql.starts_with("WITH ranked_salaries AS ("));
        assert!(sql.contains("FOR UPDATE"));
        assert!(sql.contains("WHERE rn = 1"));
    }

    #[test]
    fn test_complex_window_aggregation() {
        // 运行总和 + 排名
        let sql = Query::select()
            .column("user_id")
            .column("amount")
            .from("transactions")
            .window_function(
                "SUM(amount) OVER (PARTITION BY user_id ORDER BY created_at) AS running_total",
            )
            .rank("user_id", "created_at", "tx_rank")
            .build(DbType::MySQL);
        assert!(sql.contains(
            "SUM(amount) OVER (PARTITION BY user_id ORDER BY created_at) AS running_total"
        ));
        assert!(sql.contains("RANK() OVER (PARTITION BY user_id ORDER BY created_at) AS tx_rank"));
    }

    // ---- 参数化 WHERE 测试（P0 修复：SQL 注入防护） ----

    #[test]
    fn test_select_where_eq_params() {
        use sz_orm_core::Value;
        let built = Query::select()
            .column("id")
            .from("users")
            .where_eq("age", Value::I32(18))
            .build_with_params(DbType::MySQL);
        assert!(built.sql.contains("WHERE `age` = ?"));
        assert_eq!(built.params.len(), 1);
        assert_eq!(built.params[0], Value::I32(18));
    }

    #[test]
    fn test_select_multiple_where_params() {
        use sz_orm_core::Value;
        let built = Query::select()
            .column("id")
            .from("users")
            .where_eq("age", Value::I32(18))
            .where_eq("status", Value::String("active".to_string()))
            .build_with_params(DbType::MySQL);
        assert!(built.sql.contains("WHERE `age` = ? AND `status` = ?"));
        assert_eq!(built.params.len(), 2);
    }

    #[test]
    fn test_select_or_where_eq_params() {
        use sz_orm_core::Value;
        let built = Query::select()
            .column("id")
            .from("users")
            .where_eq("age", Value::I32(18))
            .or_where_eq("role", Value::String("admin".to_string()))
            .build_with_params(DbType::MySQL);
        assert!(built.sql.contains("WHERE `age` = ? OR `role` = ?"));
        assert_eq!(built.params.len(), 2);
    }

    #[test]
    fn test_select_where_in_params() {
        use sz_orm_core::Value;
        let built = Query::select()
            .column("id")
            .from("users")
            .where_in("id", vec![Value::I32(1), Value::I32(2), Value::I32(3)])
            .build_with_params(DbType::MySQL);
        assert!(built.sql.contains("WHERE `id` IN (?, ?, ?)"));
        assert_eq!(built.params.len(), 3);
    }

    #[test]
    fn test_select_where_in_empty() {
        let built = Query::select()
            .column("id")
            .from("users")
            .where_in("id", vec![])
            .build_with_params(DbType::MySQL);
        assert!(built.sql.contains("WHERE 1 = 0"));
        assert_eq!(built.params.len(), 0);
    }

    #[test]
    fn test_select_where_between_params() {
        use sz_orm_core::Value;
        let built = Query::select()
            .column("id")
            .from("users")
            .where_between("age", Value::I32(18), Value::I32(65))
            .build_with_params(DbType::MySQL);
        assert!(built.sql.contains("WHERE `age` BETWEEN ? AND ?"));
        assert_eq!(built.params.len(), 2);
    }

    #[test]
    fn test_select_where_null_params() {
        let built = Query::select()
            .column("id")
            .from("users")
            .where_null("deleted_at")
            .build_with_params(DbType::MySQL);
        assert!(built.sql.contains("WHERE `deleted_at` IS NULL"));
        assert_eq!(built.params.len(), 0);
    }

    #[test]
    fn test_select_where_not_null_params() {
        let built = Query::select()
            .column("id")
            .from("users")
            .where_not_null("email")
            .build_with_params(DbType::MySQL);
        assert!(built.sql.contains("WHERE `email` IS NOT NULL"));
    }

    #[test]
    fn test_select_mixed_raw_and_param_where() {
        use sz_orm_core::Value;
        let built = Query::select()
            .column("id")
            .from("users")
            .where_clause("age > 18")
            .where_eq("status", Value::String("active".to_string()))
            .build_with_params(DbType::MySQL);
        assert!(built.sql.contains("WHERE age > 18 AND `status` = ?"));
        assert_eq!(built.params.len(), 1);
    }

    #[test]
    fn test_select_param_where_with_order_limit() {
        use sz_orm_core::Value;
        let built = Query::select()
            .column("id")
            .from("users")
            .where_eq("age", Value::I32(18))
            .order_by("id", true)
            .limit(10)
            .build_with_params(DbType::MySQL);
        assert!(built.sql.contains("WHERE `age` = ?"));
        assert!(built.sql.contains("ORDER BY `id` ASC"));
        assert!(built.sql.contains("LIMIT 10"));
    }

    #[test]
    fn test_select_param_where_postgres_dialect() {
        use sz_orm_core::Value;
        let built = Query::select()
            .column("id")
            .from("users")
            .where_eq("age", Value::I32(18))
            .build_with_params(DbType::PostgreSQL);
        assert!(built.sql.contains("WHERE \"age\" = ?"));
    }

    #[test]
    fn test_select_param_where_injection_safe() {
        // 参数化查询下，恶意输入作为参数绑定，不会改变 SQL 结构
        use sz_orm_core::Value;
        let malicious = "'; DROP TABLE users; --".to_string();
        let built = Query::select()
            .column("id")
            .from("users")
            .where_eq("name", Value::String(malicious.clone()))
            .build_with_params(DbType::MySQL);
        // SQL 结构不含恶意输入（仅含 ? 占位符）
        assert!(!built.sql.contains("DROP TABLE"));
        assert!(!built.sql.contains(";"));
        // 恶意输入完整保留在参数中（由驱动层转义）
        assert_eq!(built.params.len(), 1);
        assert_eq!(built.params[0], Value::String(malicious));
    }

    #[test]
    fn test_update_where_eq_params() {
        use sz_orm_core::Value;
        let built = Query::update()
            .table("users")
            .set("name", "'Bob'")
            .where_eq("id", Value::I64(1))
            .build_with_params(DbType::MySQL);
        assert!(built.sql.contains("UPDATE `users` SET"));
        assert!(built.sql.contains("WHERE `id` = ?"));
        assert_eq!(built.params.len(), 1);
    }

    #[test]
    fn test_update_where_in_params() {
        use sz_orm_core::Value;
        let built = Query::update()
            .table("users")
            .set("status", "'inactive'")
            .where_in("id", vec![Value::I64(1), Value::I64(2)])
            .build_with_params(DbType::MySQL);
        assert!(built.sql.contains("WHERE `id` IN (?, ?)"));
        assert_eq!(built.params.len(), 2);
    }

    #[test]
    fn test_delete_where_eq_params() {
        use sz_orm_core::Value;
        let built = Query::delete()
            .from_table("users")
            .where_eq("id", Value::I64(1))
            .build_with_params(DbType::MySQL);
        assert!(built.sql.contains("DELETE FROM `users`"));
        assert!(built.sql.contains("WHERE `id` = ?"));
        assert_eq!(built.params.len(), 1);
    }

    #[test]
    fn test_delete_where_between_params() {
        use sz_orm_core::Value;
        let built = Query::delete()
            .from_table("logs")
            .where_between(
                "created_at",
                Value::String("2020-01-01".to_string()),
                Value::String("2020-12-31".to_string()),
            )
            .build_with_params(DbType::MySQL);
        assert!(built.sql.contains("WHERE `created_at` BETWEEN ? AND ?"));
        assert_eq!(built.params.len(), 2);
    }

    // ---- 任务4：FROM 子查询支持测试 ----

    #[test]
    fn test_from_subquery_basic() {
        let inner = Query::select()
            .column("id")
            .column("amount")
            .from("orders")
            .build(DbType::MySQL);
        let sql = Query::select()
            .column("id")
            .from_subquery(&inner, "t")
            .build(DbType::MySQL);
        assert!(
            sql.contains("FROM (SELECT `id`, `amount` FROM `orders`) AS `t`"),
            "FROM 子查询应渲染为 `FROM (subquery) AS alias`，实际: {sql}"
        );
    }

    #[test]
    fn test_from_subquery_postgres_dialect() {
        let inner = Query::select()
            .column("id")
            .from("orders")
            .build(DbType::PostgreSQL);
        let sql = Query::select()
            .column("id")
            .from_subquery(&inner, "t")
            .build(DbType::PostgreSQL);
        assert!(
            sql.contains("FROM (SELECT \"id\" FROM \"orders\") AS \"t\""),
            "PG 方言下别名应使用双引号，实际: {sql}"
        );
    }

    #[test]
    fn test_from_subquery_with_where_and_order() {
        let inner = Query::select()
            .column("id")
            .column("amount")
            .from("orders")
            .where_clause("amount > 100")
            .build(DbType::MySQL);
        let sql = Query::select()
            .column("id")
            .column("amount")
            .from_subquery(&inner, "t")
            .where_clause("t.amount > 200")
            .order_by("id", true)
            .build(DbType::MySQL);
        assert!(
            sql.contains("FROM (SELECT `id`, `amount` FROM `orders` WHERE amount > 100) AS `t`")
        );
        assert!(sql.contains("WHERE t.amount > 200"));
        assert!(sql.contains("ORDER BY `id` ASC"));
    }

    #[test]
    fn test_from_subquery_with_params() {
        use sz_orm_core::Value;
        let inner = Query::select()
            .column("id")
            .from("orders")
            .where_eq("amount", Value::I32(100))
            .build_with_params(DbType::MySQL);
        let built = Query::select()
            .column("id")
            .from_subquery(&inner.sql, "t")
            .where_eq("t.id", Value::I64(1))
            .build_with_params(DbType::MySQL);
        // 外层参数 + 内层参数（外层参数在 WHERE 之后，但内层 SQL 已固化为字符串）
        assert!(built
            .sql
            .contains("FROM (SELECT `id` FROM `orders` WHERE `amount` = ?) AS `t`"));
        assert!(built.sql.contains("WHERE `t`.`id` = ?"));
        // 外层只绑定 1 个参数（内层 SQL 已是字符串）
        assert_eq!(built.params.len(), 1);
    }

    #[test]
    fn test_from_subquery_overrides_from_table() {
        // 后调用者覆盖前者：from() 后 from_subquery()
        let sql = Query::select()
            .column("id")
            .from("users")
            .from_subquery("SELECT id FROM orders", "t")
            .build(DbType::MySQL);
        assert!(sql.contains("FROM (SELECT id FROM orders) AS `t`"));
        assert!(!sql.contains("FROM `users`"));
    }

    #[test]
    fn test_from_table_overrides_from_subquery() {
        // 后调用者覆盖前者：from_subquery() 后 from()
        let sql = Query::select()
            .column("id")
            .from_subquery("SELECT id FROM orders", "t")
            .from("users")
            .build(DbType::MySQL);
        assert!(sql.contains("FROM `users`"));
        assert!(!sql.contains("FROM ("));
    }

    #[test]
    fn test_from_subquery_no_from_when_neither_set() {
        let sql = Query::select().column("id").build(DbType::MySQL);
        assert!(!sql.contains("FROM"));
    }

    // ---- 任务5：RETURNING 子句支持测试 ----

    #[test]
    fn test_insert_returning_postgres() {
        let sql = Query::insert()
            .into_table("users")
            .value("name", "'Alice'")
            .returning(&["id", "created_at"])
            .build_with_dialect(DbType::PostgreSQL);
        assert!(
            sql.contains("RETURNING \"id\", \"created_at\""),
            "PG 方言应渲染 RETURNING，实际: {sql}"
        );
    }

    #[test]
    fn test_insert_returning_sqlite() {
        let sql = Query::insert()
            .into_table("users")
            .value("name", "'Alice'")
            .returning(&["id"])
            .build_with_dialect(DbType::Sqlite);
        assert!(
            sql.contains("RETURNING \"id\""),
            "SQLite 方言应渲染 RETURNING，实际: {sql}"
        );
    }

    #[test]
    fn test_insert_returning_all() {
        let sql = Query::insert()
            .into_table("users")
            .value("name", "'Alice'")
            .returning_all()
            .build_with_dialect(DbType::PostgreSQL);
        assert!(
            sql.contains("RETURNING *"),
            "returning_all 应渲染 `RETURNING *`，实际: {sql}"
        );
    }

    #[test]
    fn test_insert_returning_mysql_skipped() {
        // MySQL 不支持 RETURNING，应跳过
        let sql = Query::insert()
            .into_table("users")
            .value("name", "'Alice'")
            .returning(&["id"])
            .build_with_dialect(DbType::MySQL);
        assert!(
            !sql.contains("RETURNING"),
            "MySQL 方言应跳过 RETURNING，实际: {sql}"
        );
    }

    #[test]
    fn test_insert_returning_with_upsert_postgres() {
        // ON CONFLICT + RETURNING 组合（PostgreSQL 经典用法）
        let sql = Query::insert()
            .into_table("users")
            .value("id", "1")
            .value("name", "'Alice'")
            .on_conflict_do_update(&["id"], &[("name", "EXCLUDED.name")])
            .returning(&["id", "name"])
            .build_with_dialect(DbType::PostgreSQL);
        assert!(sql.contains("ON CONFLICT"));
        assert!(sql.contains("RETURNING"));
    }

    #[test]
    fn test_insert_returning_build_mysql_style_skipped() {
        // build() 是 MySQL 风格硬编码，不应渲染 RETURNING
        let sql = Query::insert()
            .into_table("users")
            .value("name", "'Alice'")
            .returning(&["id"])
            .build();
        assert!(!sql.contains("RETURNING"));
    }

    #[test]
    fn test_update_returning_postgres() {
        let sql = Query::update()
            .table("users")
            .set("status", "'active'")
            .where_clause("id = 1")
            .returning(&["id", "status"])
            .build_with_dialect(DbType::PostgreSQL);
        assert!(sql.contains("RETURNING \"id\", \"status\""));
        assert!(sql.contains("WHERE id = 1"));
    }

    #[test]
    fn test_update_returning_sqlite() {
        let sql = Query::update()
            .table("users")
            .set("status", "'active'")
            .returning(&["id"])
            .build_with_dialect(DbType::Sqlite);
        assert!(sql.contains("RETURNING \"id\""));
    }

    #[test]
    fn test_update_returning_mysql_skipped() {
        let sql = Query::update()
            .table("users")
            .set("status", "'active'")
            .returning(&["id"])
            .build_with_dialect(DbType::MySQL);
        assert!(!sql.contains("RETURNING"));
    }

    #[test]
    fn test_update_returning_with_params() {
        use sz_orm_core::Value;
        let built = Query::update()
            .table("users")
            .set("status", "'active'")
            .where_eq("id", Value::I64(1))
            .returning(&["id", "status"])
            .build_with_params(DbType::PostgreSQL);
        assert!(built.sql.contains("WHERE \"id\" = ?"));
        assert!(built.sql.contains("RETURNING \"id\", \"status\""));
        assert_eq!(built.params.len(), 1);
    }

    #[test]
    fn test_delete_returning_postgres() {
        let sql = Query::delete()
            .from_table("users")
            .where_clause("id = 1")
            .returning(&["id", "name"])
            .build_with_dialect(DbType::PostgreSQL);
        assert!(sql.contains("RETURNING \"id\", \"name\""));
        assert!(sql.contains("WHERE id = 1"));
    }

    #[test]
    fn test_delete_returning_sqlite() {
        let sql = Query::delete()
            .from_table("users")
            .where_clause("id = 1")
            .returning(&["id"])
            .build_with_dialect(DbType::Sqlite);
        assert!(sql.contains("RETURNING \"id\""));
    }

    #[test]
    fn test_delete_returning_mysql_skipped() {
        let sql = Query::delete()
            .from_table("users")
            .where_clause("id = 1")
            .returning(&["id"])
            .build_with_dialect(DbType::MySQL);
        assert!(!sql.contains("RETURNING"));
    }

    #[test]
    fn test_delete_returning_with_params() {
        use sz_orm_core::Value;
        let built = Query::delete()
            .from_table("users")
            .where_eq("id", Value::I64(1))
            .returning(&["id", "name"])
            .build_with_params(DbType::PostgreSQL);
        assert!(built.sql.contains("WHERE \"id\" = ?"));
        assert!(built.sql.contains("RETURNING \"id\", \"name\""));
        assert_eq!(built.params.len(), 1);
    }

    #[test]
    fn test_returning_star_not_quoted() {
        // `*` 在 RETURNING 中不应被引号包裹
        let sql = Query::insert()
            .into_table("users")
            .value("name", "'Alice'")
            .returning(&["*"])
            .build_with_dialect(DbType::PostgreSQL);
        assert!(sql.contains("RETURNING *"));
        assert!(!sql.contains("RETURNING \"*\""));
    }

    // ---- 参数化 JOIN 测试（P2 修复 #68：JOIN 注入风险） ----

    #[test]
    fn test_inner_join_on_column_eq() {
        let sql = Query::select()
            .column("u.id")
            .from("users u")
            .inner_join_on("orders o", "u.id", "o.user_id")
            .build(DbType::MySQL);
        assert!(
            sql.contains("INNER JOIN `orders` o ON `u`.`id` = `o`.`user_id`"),
            "列对列等值连接应渲染转义标识符，实际: {sql}"
        );
    }

    #[test]
    fn test_left_join_on_column_eq() {
        let sql = Query::select()
            .column("u.id")
            .from("users u")
            .left_join_on("profiles p", "u.id", "p.user_id")
            .build(DbType::MySQL);
        assert!(sql.contains("LEFT JOIN `profiles` p ON `u`.`id` = `p`.`user_id`"));
    }

    #[test]
    fn test_right_join_on_column_eq() {
        let sql = Query::select()
            .column("u.id")
            .from("users u")
            .right_join_on("orders o", "u.id", "o.user_id")
            .build(DbType::MySQL);
        assert!(sql.contains("RIGHT JOIN `orders` o ON `u`.`id` = `o`.`user_id`"));
    }

    #[test]
    fn test_inner_join_on_postgres_dialect() {
        let sql = Query::select()
            .column("u.id")
            .from("users u")
            .inner_join_on("orders o", "u.id", "o.user_id")
            .build(DbType::PostgreSQL);
        // 注：quote_join_table 仍使用反引号包裹表名（向后兼容），
        // 但 ON 条件的列名按 PG 方言用双引号引用
        assert!(
            sql.contains("INNER JOIN `orders` o ON \"u\".\"id\" = \"o\".\"user_id\""),
            "PG 方言下 ON 条件列名应使用双引号引用，实际: {sql}"
        );
    }

    #[test]
    fn test_inner_join_param_binds_value() {
        use sz_orm_core::Value;
        let built = Query::select()
            .column("u.id")
            .from("users u")
            .inner_join_param("orders o", "o.status", " = ?", Value::String("paid".into()))
            .build_with_params(DbType::MySQL);
        assert!(
            built
                .sql
                .contains("INNER JOIN `orders` o ON `o`.`status` = ?"),
            "参数化 JOIN 应渲染 ? 占位符，实际: {}",
            built.sql
        );
        assert_eq!(built.params.len(), 1);
        assert_eq!(built.params[0], Value::String("paid".to_string()));
    }

    #[test]
    fn test_left_join_param_binds_value() {
        use sz_orm_core::Value;
        let built = Query::select()
            .column("u.id")
            .from("users u")
            .left_join_param("orders o", "o.status", " = ?", Value::String("paid".into()))
            .build_with_params(DbType::MySQL);
        assert!(built
            .sql
            .contains("LEFT JOIN `orders` o ON `o`.`status` = ?"));
        assert_eq!(built.params.len(), 1);
    }

    #[test]
    fn test_right_join_param_binds_value() {
        use sz_orm_core::Value;
        let built = Query::select()
            .column("u.id")
            .from("users u")
            .right_join_param("orders o", "o.status", " = ?", Value::String("paid".into()))
            .build_with_params(DbType::MySQL);
        assert!(built
            .sql
            .contains("RIGHT JOIN `orders` o ON `o`.`status` = ?"));
        assert_eq!(built.params.len(), 1);
    }

    #[test]
    fn test_param_join_injection_safe() {
        // 参数化 JOIN 下，恶意输入作为参数绑定，不会改变 SQL 结构
        use sz_orm_core::Value;
        let malicious = "'; DROP TABLE orders; --".to_string();
        let built = Query::select()
            .column("u.id")
            .from("users u")
            .inner_join_param(
                "orders o",
                "o.status",
                " = ?",
                Value::String(malicious.clone()),
            )
            .build_with_params(DbType::MySQL);
        // SQL 结构不含恶意输入（仅含 ? 占位符）
        assert!(!built.sql.contains("DROP TABLE"));
        assert!(!built.sql.contains(";"));
        // 恶意输入完整保留在参数中（由驱动层转义）
        assert_eq!(built.params.len(), 1);
        assert_eq!(built.params[0], Value::String(malicious));
    }

    #[test]
    fn test_mixed_raw_and_param_join() {
        // 混合使用原始 inner_join 和参数化 inner_join_param
        use sz_orm_core::Value;
        let built = Query::select()
            .column("u.id")
            .from("users u")
            .inner_join("orders o", "u.id = o.user_id")
            .inner_join_param(
                "payments p",
                "p.status",
                " = ?",
                Value::String("paid".into()),
            )
            .build_with_params(DbType::MySQL);
        assert!(built
            .sql
            .contains("INNER JOIN `orders` o ON u.id = o.user_id"));
        assert!(built
            .sql
            .contains("INNER JOIN `payments` p ON `p`.`status` = ?"));
        assert_eq!(built.params.len(), 1);
    }

    #[test]
    fn test_param_join_with_where_params_combined() {
        // 参数化 JOIN + 参数化 WHERE 联合使用
        use sz_orm_core::Value;
        let built = Query::select()
            .column("u.id")
            .from("users u")
            .inner_join_param("orders o", "o.status", " = ?", Value::String("paid".into()))
            .where_eq("u.age", Value::I32(18))
            .build_with_params(DbType::MySQL);
        assert!(built
            .sql
            .contains("INNER JOIN `orders` o ON `o`.`status` = ?"));
        assert!(built.sql.contains("WHERE `u`.`age` = ?"));
        // JOIN 参数在前，WHERE 参数在后
        assert_eq!(built.params.len(), 2);
        assert_eq!(built.params[0], Value::String("paid".to_string()));
        assert_eq!(built.params[1], Value::I32(18));
    }
}
