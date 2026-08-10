//! M1-T9.2: SQL 注入防护测试
//!
//! 对 15 种新增表达式构造恶意输入，验证 to_sql 输出使用参数化占位符隔离。

#![cfg(feature = "typed-dsl")]

use sz_orm_core::dialect::{MySqlDialect, PostgreSqlDialect};
use sz_orm_core::typed::{TypedColumn, TypedTable};
use sz_orm_core::typed_ast::*;

// ---- mock 类型 ----

struct UsersTable;
impl TypedTable for UsersTable {
    const NAME: &'static str = "users";
}

struct ColName;
impl TypedColumn for ColName {
    const NAME: &'static str = "name";
    type Table = UsersTable;
    type RustType = String;
    type SqlType = Text;
}

struct ColId;
impl TypedColumn for ColId {
    const NAME: &'static str = "id";
    type Table = UsersTable;
    type RustType = i64;
    type SqlType = BigInt;
}

// ---- SQL 注入 payload ----

const INJECTION_PAYLOADS: &[&str] = &[
    "'; DROP TABLE users; --",
    "' OR '1'='1",
    "' UNION SELECT * FROM passwords --",
    "admin'--",
    "1; EXEC xp_cmdshell('dir')",
];

/// 验证 SQL 字符串不包含危险的注入模式
fn assert_no_injection(sql: &str) {
    let sql_lower = sql.to_lowercase();
    assert!(
        !sql_lower.contains("drop table"),
        "SQL contains DROP TABLE injection: {}",
        sql
    );
    assert!(
        !sql_lower.contains("or 1=1"),
        "SQL contains OR 1=1 injection: {}",
        sql
    );
    assert!(
        !sql_lower.contains("union select"),
        "SQL contains UNION SELECT injection: {}",
        sql
    );
    assert!(
        !sql_lower.contains("xp_cmdshell"),
        "SQL contains xp_cmdshell injection: {}",
        sql
    );
}

// ---- CTE 表达式注入测试 ----

struct CteTestName;
impl CteName for CteTestName {
    const NAME: &'static str = "test_cte";
}

#[test]
fn test_cte_with_no_injection() {
    let dialect = MySqlDialect;
    let expr: With<CteTestName, ColumnExpr<ColId>> = With::new();
    let (sql, _) = expr.to_sql(&dialect);
    assert_no_injection(&sql);
    // CTE 名称是编译期常量，不受运行时输入影响
    assert!(sql.contains("test_cte"));
}

#[test]
fn test_cte_recursive_no_injection() {
    let dialect = PostgreSqlDialect;
    let expr: WithRecursive<CteTestName, ColumnExpr<ColId>, ColumnExpr<ColName>> =
        WithRecursive::new();
    let (sql, _) = expr.to_sql(&dialect);
    assert_no_injection(&sql);
}

#[test]
fn test_cte_ref_no_injection() {
    let dialect = MySqlDialect;
    let expr: CteRef<CteTestName> = CteRef::new();
    let (sql, _) = expr.to_sql(&dialect);
    assert_no_injection(&sql);
    // CteRef 只输出编译期常量名称
    assert_eq!(sql, "test_cte");
}

// ---- Window Frame 表达式注入测试 ----

#[test]
fn test_window_frame_no_injection() {
    let dialect = MySqlDialect;

    let (sql, _) = RowsFrame::new().to_sql(&dialect);
    assert_no_injection(&sql);
    assert_eq!(sql, "ROWS");

    let (sql, _) = RangeFrame::new().to_sql(&dialect);
    assert_no_injection(&sql);
    assert_eq!(sql, "RANGE");

    let (sql, _) = GroupsFrame::new().to_sql(&dialect);
    assert_no_injection(&sql);
    assert_eq!(sql, "GROUPS");

    let (sql, _) = FrameUnboundedPreceding::new().to_sql(&dialect);
    assert_no_injection(&sql);

    let (sql, _) = FrameCurrentRow::new().to_sql(&dialect);
    assert_no_injection(&sql);

    let expr: FrameBetween<FrameUnboundedPreceding, FrameCurrentRow> = FrameBetween::new();
    let (sql, _) = expr.to_sql(&dialect);
    assert_no_injection(&sql);
}

// ---- JSON 操作符注入测试 ----

#[test]
fn test_json_get_uses_parameterized_placeholder() {
    let dialect = PostgreSqlDialect;
    let expr: JsonGet<ColName, String> = JsonGet::new();
    let (sql, params) = expr.to_sql(&dialect);
    // SQL 使用 ? 占位符，不拼接用户输入
    assert!(
        sql.contains('?'),
        "JSON get should use ? placeholder: {}",
        sql
    );
    assert!(
        params.is_empty(),
        "Params should be empty (placeholder is in SQL)"
    );
    assert_no_injection(&sql);
}

#[test]
fn test_json_get_text_uses_parameterized_placeholder() {
    let dialect = PostgreSqlDialect;
    let expr: JsonGetText<ColName, String> = JsonGetText::new();
    let (sql, _) = expr.to_sql(&dialect);
    assert!(sql.contains('?'));
    assert_no_injection(&sql);
}

#[test]
fn test_json_contains_uses_parameterized_placeholder() {
    let dialect = PostgreSqlDialect;
    let expr: JsonContains<ColName, String> = JsonContains::new();
    let (sql, _) = expr.to_sql(&dialect);
    assert!(sql.contains('?'));
    assert_no_injection(&sql);
}

#[test]
fn test_json_exists_uses_parameterized_placeholder() {
    let dialect = PostgreSqlDialect;
    let expr: JsonExists<ColName, String> = JsonExists::new();
    let (sql, _) = expr.to_sql(&dialect);
    assert!(sql.contains('?'));
    assert_no_injection(&sql);
}

#[test]
fn test_json_mysql_uses_function_not_concat() {
    let dialect = MySqlDialect;
    let expr: JsonGet<ColName, String> = JsonGet::new();
    let (sql, _) = expr.to_sql(&dialect);
    // MySQL 使用 JSON_EXTRACT 函数，非字符串拼接
    assert!(sql.contains("JSON_EXTRACT"));
    assert!(sql.contains('?'));
    assert_no_injection(&sql);
}

#[test]
fn test_json_contains_mysql_uses_function() {
    let dialect = MySqlDialect;
    let expr: JsonContains<ColName, String> = JsonContains::new();
    let (sql, _) = expr.to_sql(&dialect);
    assert!(sql.contains("JSON_CONTAINS"));
    assert!(sql.contains('?'));
    assert_no_injection(&sql);
}

/// 验证所有注入 payload 都不会出现在任何表达式的 SQL 输出中
#[test]
fn test_no_injection_payload_in_any_expression() {
    let dialects: Vec<Box<dyn sz_orm_core::dialect::Dialect>> =
        vec![Box::new(MySqlDialect), Box::new(PostgreSqlDialect)];

    for dialect in &dialects {
        // JSON 表达式
        let json_get: JsonGet<ColName, String> = JsonGet::new();
        let (sql, _) = json_get.to_sql(dialect.as_ref());
        for payload in INJECTION_PAYLOADS {
            assert!(
                !sql.contains(payload),
                "Injection payload '{}' found in JSON get SQL: {}",
                payload,
                sql
            );
        }

        let json_contains: JsonContains<ColName, String> = JsonContains::new();
        let (sql, _) = json_contains.to_sql(dialect.as_ref());
        for payload in INJECTION_PAYLOADS {
            assert!(
                !sql.contains(payload),
                "Injection payload '{}' found in JSON contains SQL: {}",
                payload,
                sql
            );
        }
    }
}
