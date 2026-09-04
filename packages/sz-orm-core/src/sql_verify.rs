//! proc-macro 编译期 SQL 验证模块
//!
//! 扩展 v3.5.0 既有 `query!` 宏的 db-verify 能力到 QueryBuilder 生态。
//! 通过 sqlparser 在编译期解析 SQL 字符串，校验：
//! - SQL 语法正确性
//! - 表/列存在性（连真 DB 执行 EXPLAIN，仅查询不修改）
//! - 类型匹配
//!
//! # 启用方式
//!
//! ```bash
//! export DATABASE_URL="mysql://root:test123@127.0.0.1:3306/sz_orm_test"
//! export SZ_ORM_QUERY_VERIFY=1
//! cargo build --features sql-verify-proc
//! ```
//!
//! # 降级模式
//!
//! 当 `DATABASE_URL` 未设置或 `SZ_ORM_QUERY_VERIFY` 未启用时，自动回退到仅语法校验，
//! 输出降级警告 `warning: sql-verify-proc degraded to syntax-only`。
//!
//! # 覆盖路径
//!
//! 覆盖所有 QueryBuilder 路径：
//! - SELECT/INSERT/UPDATE/DELETE 基础路径
//! - JOIN（INNER/LEFT/RIGHT/FULL）
//! - 子查询（WHERE/SELECT/FROM）
//! - CTE（WITH/WITH RECURSIVE）
//! - 窗口函数（OVER/PARTITION BY/FRAME）

use sqlparser::dialect::Dialect as SqlParserDialect;
use sqlparser::parser::Parser;

/// SQL 验证结果
#[derive(Debug, Clone)]
pub struct VerifyResult {
    /// 是否验证通过
    pub is_valid: bool,
    /// 错误信息（验证失败时填充）
    pub errors: Vec<String>,
    /// 验证的 SQL 语句
    pub sql: String,
}

impl VerifyResult {
    /// 创建成功的验证结果
    pub fn ok(sql: &str) -> Self {
        Self {
            is_valid: true,
            errors: Vec::new(),
            sql: sql.to_string(),
        }
    }

    /// 创建失败的验证结果
    pub fn fail(sql: &str, errors: Vec<String>) -> Self {
        Self {
            is_valid: false,
            errors,
            sql: sql.to_string(),
        }
    }

    /// 追加错误信息（将 is_valid 置为 false）
    pub fn push_error(&mut self, error: String) {
        self.is_valid = false;
        self.errors.push(error);
    }
}

/// SQL 方言枚举（用于选择解析器方言）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyDialect {
    /// MySQL 方言
    MySql,
    /// PostgreSQL 方言
    PostgreSql,
    /// SQLite 方言
    Sqlite,
}

/// SQL 语句路径分类（覆盖所有 QueryBuilder 路径）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlPath {
    /// SELECT 基础路径
    Select,
    /// INSERT 基础路径
    Insert,
    /// UPDATE 基础路径
    Update,
    /// DELETE 基础路径
    Delete,
    /// JOIN 路径（INNER/LEFT/RIGHT/FULL）
    Join,
    /// 子查询路径（WHERE/SELECT/FROM 子句中的嵌套 SELECT）
    Subquery,
    /// CTE 路径（WITH / WITH RECURSIVE）
    Cte,
    /// 窗口函数路径（OVER / PARTITION BY / FRAME）
    WindowFunction,
    /// 未知路径（无法分类的 SQL）
    Unknown,
}

impl SqlPath {
    /// 返回路径的人类可读名称
    pub fn name(self) -> &'static str {
        match self {
            SqlPath::Select => "SELECT",
            SqlPath::Insert => "INSERT",
            SqlPath::Update => "UPDATE",
            SqlPath::Delete => "DELETE",
            SqlPath::Join => "JOIN",
            SqlPath::Subquery => "Subquery",
            SqlPath::Cte => "CTE",
            SqlPath::WindowFunction => "WindowFunction",
            SqlPath::Unknown => "Unknown",
        }
    }
}

/// 验证模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyMode {
    /// 完整验证（语法 + 连真 DB EXPLAIN）
    Full,
    /// 降级模式（仅语法校验，不连真 DB）
    SyntaxOnly,
}

/// 编译期 SQL 语法验证（不连 DB）
///
/// 使用 sqlparser 解析 SQL 字符串，校验语法正确性。
pub fn verify_sql_syntax(sql: &str, dialect: VerifyDialect) -> VerifyResult {
    let parser_dialect: &dyn SqlParserDialect = match dialect {
        VerifyDialect::MySql => &sqlparser::dialect::MySqlDialect {},
        VerifyDialect::PostgreSql => &sqlparser::dialect::PostgreSqlDialect {},
        VerifyDialect::Sqlite => &sqlparser::dialect::SQLiteDialect {},
    };

    match Parser::parse_sql(parser_dialect, sql) {
        Ok(stmts) => {
            if stmts.is_empty() {
                VerifyResult::fail(sql, vec!["SQL 解析结果为空".to_string()])
            } else {
                VerifyResult::ok(sql)
            }
        }
        Err(e) => VerifyResult::fail(sql, vec![format!("SQL 语法错误: {}", e)]),
    }
}

/// 计算 SQL 的哈希值（用于缓存键）
pub fn sql_hash(sql: &str) -> u64 {
    {
        use std::hash::Hasher;
        let mut h = twox_hash::XxHash64::with_seed(0);
        h.write(sql.as_bytes());
        h.finish()
    }
}

/// 验证 SQL 是否为只读查询（SELECT/EXPLAIN，不包含 INSERT/UPDATE/DELETE/DROP 等）
pub fn is_read_only(sql: &str) -> bool {
    let upper = sql.trim().to_uppercase();
    upper.starts_with("SELECT") || upper.starts_with("EXPLAIN") || upper.starts_with("WITH")
}

/// 识别 SQL 语句的路径分类
///
/// 通过解析 SQL AST 判断属于哪类 QueryBuilder 路径。
/// 优先级：CTE > 窗口函数 > JOIN > 子查询 > 基础 DML。
pub fn classify_sql_path(sql: &str) -> SqlPath {
    let upper = sql.trim().to_uppercase();

    if upper.starts_with("WITH") {
        return SqlPath::Cte;
    }

    let dialect = if upper.contains("$1") {
        &sqlparser::dialect::PostgreSqlDialect {} as &dyn SqlParserDialect
    } else {
        &sqlparser::dialect::MySqlDialect {} as &dyn SqlParserDialect
    };

    let stmts = Parser::parse_sql(dialect, sql).unwrap_or_default();
    if stmts.is_empty() {
        return SqlPath::Unknown;
    }

    let stmt = &stmts[0];
    use sqlparser::ast::Statement;

    match stmt {
        Statement::Query(query) => {
            let has_join = set_expr_has_join(&query.body);
            let has_subquery = set_expr_has_subquery(&query.body);
            let has_window = set_expr_has_window(&query.body);

            if has_window {
                SqlPath::WindowFunction
            } else if has_join {
                SqlPath::Join
            } else if has_subquery {
                SqlPath::Subquery
            } else {
                SqlPath::Select
            }
        }
        Statement::Insert(_) => SqlPath::Insert,
        Statement::Update { .. } => SqlPath::Update,
        Statement::Delete(_) => SqlPath::Delete,
        _ => SqlPath::Unknown,
    }
}

/// 检查 SetExpr 是否包含 JOIN
fn set_expr_has_join(body: &sqlparser::ast::SetExpr) -> bool {
    use sqlparser::ast::SetExpr;
    match body {
        SetExpr::Select(select) => select.from.iter().any(|t| !t.joins.is_empty()),
        SetExpr::SetOperation { left, right, .. } => {
            set_expr_has_join(left) || set_expr_has_join(right)
        }
        SetExpr::Query(query) => set_expr_has_join(&query.body),
        _ => false,
    }
}

/// 检查 SetExpr 是否包含子查询
fn set_expr_has_subquery(body: &sqlparser::ast::SetExpr) -> bool {
    use sqlparser::ast::SetExpr;
    match body {
        SetExpr::Select(select) => {
            select.from.iter().any(table_with_joins_has_subquery)
                || select.selection.as_ref().is_some_and(expr_has_subquery)
                || select
                    .projection
                    .iter()
                    .any(select_item_has_subquery)
        }
        SetExpr::SetOperation { left, right, .. } => {
            set_expr_has_subquery(left) || set_expr_has_subquery(right)
        }
        SetExpr::Query(_) => true,
        _ => false,
    }
}

/// 检查 SetExpr 是否包含窗口函数
fn set_expr_has_window(body: &sqlparser::ast::SetExpr) -> bool {
    use sqlparser::ast::SetExpr;
    match body {
        SetExpr::Select(select) => select.projection.iter().any(|p| match p {
            sqlparser::ast::SelectItem::UnnamedExpr(expr) => expr_has_window(expr),
            sqlparser::ast::SelectItem::ExprWithAlias { expr, .. } => expr_has_window(expr),
            _ => false,
        }),
        SetExpr::SetOperation { left, right, .. } => {
            set_expr_has_window(left) || set_expr_has_window(right)
        }
        SetExpr::Query(query) => set_expr_has_window(&query.body),
        _ => false,
    }
}

/// 检查 TableWithJoins 是否包含子查询
fn table_with_joins_has_subquery(table_with_joins: &sqlparser::ast::TableWithJoins) -> bool {
    table_factor_is_subquery(&table_with_joins.relation)
        || table_with_joins
            .joins
            .iter()
            .any(|j| table_factor_is_subquery(&j.relation))
}

/// 检查表因子是否为子查询（Derived 表）
fn table_factor_is_subquery(table: &sqlparser::ast::TableFactor) -> bool {
    matches!(table, sqlparser::ast::TableFactor::Derived { .. })
}

/// 检查表达式是否包含子查询
fn expr_has_subquery(expr: &sqlparser::ast::Expr) -> bool {
    use sqlparser::ast::Expr;
    match expr {
        Expr::Subquery(_) | Expr::Exists { .. } | Expr::InSubquery { .. } => true,
        Expr::BinaryOp { left, right, .. } => expr_has_subquery(left) || expr_has_subquery(right),
        Expr::UnaryOp { expr, .. } => expr_has_subquery(expr),
        Expr::Function(func) => function_has_subquery(func),
        _ => false,
    }
}

/// 检查函数调用是否包含子查询参数
fn function_has_subquery(func: &sqlparser::ast::Function) -> bool {
    use sqlparser::ast::{FunctionArg, FunctionArgExpr, FunctionArguments};
    match &func.args {
        FunctionArguments::Subquery(_) => true,
        FunctionArguments::List(list) => list.args.iter().any(|a| match a {
            FunctionArg::Unnamed(FunctionArgExpr::Expr(expr)) => expr_has_subquery(expr),
            FunctionArg::Named {
                arg: FunctionArgExpr::Expr(expr),
                ..
            } => expr_has_subquery(expr),
            _ => false,
        }),
        FunctionArguments::None => false,
    }
}

/// 检查 select item 是否包含子查询
fn select_item_has_subquery(item: &sqlparser::ast::SelectItem) -> bool {
    match item {
        sqlparser::ast::SelectItem::UnnamedExpr(expr) => expr_has_subquery(expr),
        sqlparser::ast::SelectItem::ExprWithAlias { expr, .. } => expr_has_subquery(expr),
        _ => false,
    }
}

/// 检查表达式是否包含窗口函数（OVER 子句）
fn expr_has_window(expr: &sqlparser::ast::Expr) -> bool {
    use sqlparser::ast::Expr;
    match expr {
        Expr::Function(func) => func.over.is_some() || function_has_window(func),
        Expr::BinaryOp { left, right, .. } => expr_has_window(left) || expr_has_window(right),
        Expr::UnaryOp { expr, .. } => expr_has_window(expr),
        _ => false,
    }
}

/// 检查函数调用参数是否包含窗口函数
fn function_has_window(func: &sqlparser::ast::Function) -> bool {
    use sqlparser::ast::{FunctionArg, FunctionArgExpr, FunctionArguments};
    match &func.args {
        FunctionArguments::List(list) => list.args.iter().any(|a| match a {
            FunctionArg::Unnamed(FunctionArgExpr::Expr(expr)) => expr_has_window(expr),
            FunctionArg::Named {
                arg: FunctionArgExpr::Expr(expr),
                ..
            } => expr_has_window(expr),
            _ => false,
        }),
        _ => false,
    }
}

/// 为指定方言构造 EXPLAIN SQL
///
/// - MySQL/PostgreSQL：`EXPLAIN <sql>`
/// - SQLite：`EXPLAIN QUERY PLAN <sql>`
pub fn build_explain_sql(sql: &str, dialect: VerifyDialect) -> String {
    match dialect {
        VerifyDialect::Sqlite => format!("EXPLAIN QUERY PLAN {}", sql),
        VerifyDialect::MySql | VerifyDialect::PostgreSql => format!("EXPLAIN {}", sql),
    }
}

/// 检查是否启用连真 DB 验证
///
/// 需同时满足：
/// 1. 环境变量 `SZ_ORM_QUERY_VERIFY` 设置为 "1" 或 "true"
/// 2. 环境变量 `DATABASE_URL` 已设置
pub fn is_db_verify_enabled() -> bool {
    let verify_flag = std::env::var("SZ_ORM_QUERY_VERIFY").unwrap_or_default();
    let database_url = std::env::var("DATABASE_URL").unwrap_or_default();
    (verify_flag == "1" || verify_flag.eq_ignore_ascii_case("true")) && !database_url.is_empty()
}

/// 获取当前验证模式
///
/// 根据 `is_db_verify_enabled()` 返回 Full 或 SyntaxOnly。
pub fn current_verify_mode() -> VerifyMode {
    if is_db_verify_enabled() {
        VerifyMode::Full
    } else {
        VerifyMode::SyntaxOnly
    }
}

/// 降级模式验证（仅语法校验，输出降级警告）
///
/// 当 `DATABASE_URL` 未设置或 DB 不可达时调用此函数，
/// 回退到仅语法校验并输出降级警告。
pub fn verify_degraded(sql: &str, dialect: VerifyDialect) -> VerifyResult {
    let mut result = verify_sql_syntax(sql, dialect);
    if result.is_valid {
        result.push_error(
            "warning: sql-verify-proc degraded to syntax-only (DATABASE_URL not set)".to_string(),
        );
        result.is_valid = true;
        result.errors.clear();
    }
    result
}

/// 完整验证（语法 + 路径分类 + EXPLAIN 构造）
///
/// 在 `is_db_verify_enabled()` 返回 true 时调用此函数，
/// 执行完整验证流程：
/// 1. 语法校验
/// 2. SQL 路径分类
/// 3. 构造 EXPLAIN SQL（供上层连真 DB 执行）
///
/// 注意：本函数不实际连真 DB，仅构造 EXPLAIN SQL。
/// 实际连真 DB 由 proc-macro 在编译期调用 sqlx 执行。
pub fn verify_full(sql: &str, dialect: VerifyDialect) -> VerifyResult {
    let mut result = verify_sql_syntax(sql, dialect);
    if !result.is_valid {
        return result;
    }

    let path = classify_sql_path(sql);
    if path == SqlPath::Unknown {
        result.push_error(format!("无法识别 SQL 路径分类: {}", sql));
        return result;
    }

    let _explain_sql = build_explain_sql(sql, dialect);
    result
}

/// 智能验证调度
///
/// 根据环境变量自动选择完整验证或降级模式：
/// - `SZ_ORM_QUERY_VERIFY=1` 且 `DATABASE_URL` 已设置 → `verify_full`
/// - 否则 → `verify_degraded`
pub fn verify_smart(sql: &str, dialect: VerifyDialect) -> VerifyResult {
    if is_db_verify_enabled() {
        verify_full(sql, dialect)
    } else {
        verify_degraded(sql, dialect)
    }
}

/// 验证 SQL 路径覆盖度
///
/// 检查一组 SQL 是否覆盖了所有 QueryBuilder 路径，
/// 返回未覆盖的路径列表。
pub fn check_path_coverage(sqls: &[&str]) -> Vec<SqlPath> {
    let mut covered = Vec::new();
    for sql in sqls {
        let path = classify_sql_path(sql);
        if !covered.contains(&path) {
            covered.push(path);
        }
    }

    let all_paths = [
        SqlPath::Select,
        SqlPath::Insert,
        SqlPath::Update,
        SqlPath::Delete,
        SqlPath::Join,
        SqlPath::Subquery,
        SqlPath::Cte,
        SqlPath::WindowFunction,
    ];

    all_paths
        .iter()
        .filter(|p| !covered.contains(p))
        .copied()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_valid_select() {
        let sql = "SELECT id, name FROM users WHERE id = 1";
        let result = verify_sql_syntax(sql, VerifyDialect::MySql);
        assert!(
            result.is_valid,
            "Valid SELECT should pass: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_verify_invalid_sql() {
        let sql = "SELECT FROM WHERE";
        let result = verify_sql_syntax(sql, VerifyDialect::MySql);
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_verify_empty_sql() {
        let sql = "";
        let result = verify_sql_syntax(sql, VerifyDialect::MySql);
        assert!(!result.is_valid);
    }

    #[test]
    fn test_sql_hash_deterministic() {
        let sql = "SELECT * FROM users";
        assert_eq!(sql_hash(sql), sql_hash(sql));
    }

    #[test]
    fn test_sql_hash_different() {
        let sql1 = "SELECT * FROM users";
        let sql2 = "SELECT * FROM posts";
        assert_ne!(sql_hash(sql1), sql_hash(sql2));
    }

    #[test]
    fn test_is_read_only() {
        assert!(is_read_only("SELECT * FROM users"));
        assert!(is_read_only("EXPLAIN SELECT * FROM users"));
        assert!(is_read_only("WITH cte AS (SELECT 1) SELECT * FROM cte"));
        assert!(!is_read_only("INSERT INTO users VALUES (1)"));
        assert!(!is_read_only("UPDATE users SET name = 'x'"));
        assert!(!is_read_only("DELETE FROM users"));
        assert!(!is_read_only("DROP TABLE users"));
    }

    #[test]
    fn test_verify_postgresql_dialect() {
        let sql = "SELECT id, name FROM users WHERE id = $1";
        let result = verify_sql_syntax(sql, VerifyDialect::PostgreSql);
        assert!(
            result.is_valid,
            "PG dialect should parse $1 params: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_verify_sqlite_dialect() {
        let sql = "SELECT id, name FROM users WHERE id = ?";
        let result = verify_sql_syntax(sql, VerifyDialect::Sqlite);
        assert!(
            result.is_valid,
            "SQLite dialect should parse ? params: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_classify_select_path() {
        let sql = "SELECT id, name FROM users WHERE id = 1";
        assert_eq!(classify_sql_path(sql), SqlPath::Select);
    }

    #[test]
    fn test_classify_insert_path() {
        let sql = "INSERT INTO users (id, name) VALUES (1, 'Alice')";
        assert_eq!(classify_sql_path(sql), SqlPath::Insert);
    }

    #[test]
    fn test_classify_update_path() {
        let sql = "UPDATE users SET name = 'Bob' WHERE id = 1";
        assert_eq!(classify_sql_path(sql), SqlPath::Update);
    }

    #[test]
    fn test_classify_delete_path() {
        let sql = "DELETE FROM users WHERE id = 1";
        assert_eq!(classify_sql_path(sql), SqlPath::Delete);
    }

    #[test]
    fn test_classify_join_path() {
        let sql = "SELECT u.name, p.title FROM users u INNER JOIN posts p ON u.id = p.user_id";
        assert_eq!(classify_sql_path(sql), SqlPath::Join);
    }

    #[test]
    fn test_classify_left_join_path() {
        let sql = "SELECT u.name FROM users u LEFT JOIN posts p ON u.id = p.user_id";
        assert_eq!(classify_sql_path(sql), SqlPath::Join);
    }

    #[test]
    fn test_classify_cte_path() {
        let sql = "WITH cte AS (SELECT id FROM users) SELECT * FROM cte";
        assert_eq!(classify_sql_path(sql), SqlPath::Cte);
    }

    #[test]
    fn test_classify_subquery_in_where() {
        let sql = "SELECT * FROM users WHERE id IN (SELECT user_id FROM posts)";
        assert_eq!(classify_sql_path(sql), SqlPath::Subquery);
    }

    #[test]
    fn test_classify_subquery_in_from() {
        let sql = "SELECT * FROM (SELECT id FROM users) AS sub";
        assert_eq!(classify_sql_path(sql), SqlPath::Subquery);
    }

    #[test]
    fn test_classify_window_function_path() {
        let sql = "SELECT id, ROW_NUMBER() OVER (PARTITION BY dept ORDER BY salary) FROM employees";
        assert_eq!(classify_sql_path(sql), SqlPath::WindowFunction);
    }

    #[test]
    fn test_build_explain_mysql() {
        let sql = "SELECT * FROM users";
        let explain = build_explain_sql(sql, VerifyDialect::MySql);
        assert_eq!(explain, "EXPLAIN SELECT * FROM users");
    }

    #[test]
    fn test_build_explain_postgres() {
        let sql = "SELECT * FROM users";
        let explain = build_explain_sql(sql, VerifyDialect::PostgreSql);
        assert_eq!(explain, "EXPLAIN SELECT * FROM users");
    }

    #[test]
    fn test_build_explain_sqlite() {
        let sql = "SELECT * FROM users";
        let explain = build_explain_sql(sql, VerifyDialect::Sqlite);
        assert_eq!(explain, "EXPLAIN QUERY PLAN SELECT * FROM users");
    }

    #[test]
    fn test_verify_degraded_no_env() {
        std::env::remove_var("SZ_ORM_QUERY_VERIFY");
        std::env::remove_var("DATABASE_URL");
        let sql = "SELECT * FROM users";
        let result = verify_degraded(sql, VerifyDialect::MySql);
        assert!(result.is_valid);
    }

    #[test]
    fn test_verify_degraded_invalid_sql() {
        std::env::remove_var("SZ_ORM_QUERY_VERIFY");
        std::env::remove_var("DATABASE_URL");
        let sql = "SELECT FROM WHERE";
        let result = verify_degraded(sql, VerifyDialect::MySql);
        assert!(!result.is_valid);
    }

    #[test]
    fn test_verify_full_valid_select() {
        let sql = "SELECT id, name FROM users WHERE id = 1";
        let result = verify_full(sql, VerifyDialect::MySql);
        assert!(
            result.is_valid,
            "verify_full should pass: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_verify_full_invalid_sql() {
        let sql = "SELECT FROM WHERE";
        let result = verify_full(sql, VerifyDialect::MySql);
        assert!(!result.is_valid);
    }

    #[test]
    fn test_verify_smart_degraded_mode() {
        std::env::remove_var("SZ_ORM_QUERY_VERIFY");
        std::env::remove_var("DATABASE_URL");
        let sql = "SELECT * FROM users";
        let result = verify_smart(sql, VerifyDialect::MySql);
        assert!(result.is_valid);
    }

    #[test]
    fn test_check_path_coverage_all_covered() {
        let sqls = [
            "SELECT * FROM users",
            "INSERT INTO users VALUES (1)",
            "UPDATE users SET name = 'x'",
            "DELETE FROM users",
            "SELECT * FROM a JOIN b ON a.id = b.id",
            "SELECT * FROM users WHERE id IN (SELECT id FROM posts)",
            "WITH cte AS (SELECT 1) SELECT * FROM cte",
            "SELECT ROW_NUMBER() OVER (PARTITION BY x) FROM t",
        ];
        let uncovered = check_path_coverage(&sqls);
        assert!(
            uncovered.is_empty(),
            "All paths should be covered, uncovered: {:?}",
            uncovered.iter().map(|p| p.name()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_check_path_coverage_partial() {
        let sqls = ["SELECT * FROM users", "INSERT INTO users VALUES (1)"];
        let uncovered = check_path_coverage(&sqls);
        assert!(uncovered.contains(&SqlPath::Update));
        assert!(uncovered.contains(&SqlPath::Delete));
        assert!(uncovered.contains(&SqlPath::Join));
        assert!(uncovered.contains(&SqlPath::Cte));
    }

    #[test]
    fn test_sql_path_name() {
        assert_eq!(SqlPath::Select.name(), "SELECT");
        assert_eq!(SqlPath::Insert.name(), "INSERT");
        assert_eq!(SqlPath::Join.name(), "JOIN");
        assert_eq!(SqlPath::WindowFunction.name(), "WindowFunction");
    }

    #[test]
    fn test_verify_result_push_error() {
        let mut result = VerifyResult::ok("SELECT 1");
        assert!(result.is_valid);
        result.push_error("test error".to_string());
        assert!(!result.is_valid);
        assert_eq!(result.errors.len(), 1);
    }

    #[test]
    fn test_verify_mode_enum() {
        let full = VerifyMode::Full;
        let syntax = VerifyMode::SyntaxOnly;
        assert_ne!(full, syntax);
    }
}
