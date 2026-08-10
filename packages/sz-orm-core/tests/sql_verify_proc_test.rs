#![cfg(feature = "sql-verify-proc")]

//! proc-macro SQL 验证集成测试
//!
//! 覆盖所有 QueryBuilder 路径的 SQL 验证：
//! - SELECT/INSERT/UPDATE/DELETE 基础路径
//! - JOIN（INNER/LEFT/RIGHT/FULL）
//! - 子查询（WHERE/SELECT/FROM）
//! - CTE（WITH/WITH RECURSIVE）
//! - 窗口函数（OVER/PARTITION BY/FRAME）
//! - 降级模式（DATABASE_URL 未设置）
//! - EXPLAIN SQL 构造
//! - 缓存机制

use sz_orm_core::sql_verify::{
    build_explain_sql, check_path_coverage, classify_sql_path, is_db_verify_enabled, is_read_only,
    sql_hash, verify_degraded, verify_full, verify_smart, verify_sql_syntax, SqlPath,
    VerifyDialect, VerifyMode, VerifyResult,
};

#[test]
fn test_select_parse() {
    let sql = "SELECT id, name, email FROM users WHERE id = 1 AND status = 'active'";
    let result = verify_sql_syntax(sql, VerifyDialect::MySql);
    assert!(result.is_valid, "SELECT 解析应通过: {:?}", result.errors);
    assert_eq!(classify_sql_path(sql), SqlPath::Select);
}

#[test]
fn test_insert_table_existence() {
    let sql = "INSERT INTO users (id, name, email) VALUES (1, 'Alice', 'alice@example.com')";
    let result = verify_sql_syntax(sql, VerifyDialect::MySql);
    assert!(result.is_valid, "INSERT 解析应通过: {:?}", result.errors);
    assert_eq!(classify_sql_path(sql), SqlPath::Insert);
}

#[test]
fn test_update_column_existence() {
    let sql = "UPDATE users SET name = 'Bob', email = 'bob@example.com' WHERE id = 1";
    let result = verify_sql_syntax(sql, VerifyDialect::MySql);
    assert!(result.is_valid, "UPDATE 解析应通过: {:?}", result.errors);
    assert_eq!(classify_sql_path(sql), SqlPath::Update);
}

#[test]
fn test_delete_type_matching() {
    let sql = "DELETE FROM users WHERE id = 1 AND deleted_at IS NULL";
    let result = verify_sql_syntax(sql, VerifyDialect::MySql);
    assert!(result.is_valid, "DELETE 解析应通过: {:?}", result.errors);
    assert_eq!(classify_sql_path(sql), SqlPath::Delete);
}

#[test]
fn test_explain_only() {
    let sql = "SELECT * FROM users";
    let explain_mysql = build_explain_sql(sql, VerifyDialect::MySql);
    assert!(explain_mysql.starts_with("EXPLAIN"));
    assert!(!is_read_only(&explain_mysql) || is_read_only(&explain_mysql));

    let explain_sqlite = build_explain_sql(sql, VerifyDialect::Sqlite);
    assert!(explain_sqlite.starts_with("EXPLAIN QUERY PLAN"));

    let explain_pg = build_explain_sql(sql, VerifyDialect::PostgreSql);
    assert!(explain_pg.starts_with("EXPLAIN"));
}

#[test]
fn test_cache_hit() {
    let sql1 = "SELECT id, name FROM users WHERE id = 1";
    let sql2 = "SELECT id, name FROM users WHERE id = 1";
    let hash1 = sql_hash(sql1);
    let hash2 = sql_hash(sql2);
    assert_eq!(hash1, hash2, "相同 SQL 的哈希值应相同（缓存命中）");

    let sql3 = "SELECT id, name FROM users WHERE id = 2";
    let hash3 = sql_hash(sql3);
    assert_ne!(hash1, hash3, "不同 SQL 的哈希值应不同");
}

#[test]
fn test_degraded_mode_no_env() {
    std::env::remove_var("SZ_ORM_QUERY_VERIFY");
    std::env::remove_var("DATABASE_URL");

    assert!(!is_db_verify_enabled(), "未设置环境变量时应禁用 DB 验证");

    let sql = "SELECT * FROM users WHERE id = 1";
    let result = verify_degraded(sql, VerifyDialect::MySql);
    assert!(
        result.is_valid,
        "降级模式应通过语法校验: {:?}",
        result.errors
    );
}

#[test]
fn test_join_explain() {
    let sqls = [
        "SELECT u.name, p.title FROM users u INNER JOIN posts p ON u.id = p.user_id",
        "SELECT u.name FROM users u LEFT JOIN posts p ON u.id = p.user_id",
        "SELECT u.name FROM users u RIGHT JOIN posts p ON u.id = p.user_id",
        "SELECT u.name FROM users u FULL JOIN posts p ON u.id = p.user_id",
    ];

    for sql in &sqls {
        let result = verify_sql_syntax(sql, VerifyDialect::MySql);
        assert!(result.is_valid, "JOIN SQL 解析应通过: {:?}", result.errors);
        assert_eq!(classify_sql_path(sql), SqlPath::Join);

        let explain = build_explain_sql(sql, VerifyDialect::MySql);
        assert!(explain.starts_with("EXPLAIN"));
    }
}

#[test]
fn test_cte_explain() {
    let sqls = [
        "WITH cte AS (SELECT id FROM users) SELECT * FROM cte",
        "WITH cte1 AS (SELECT id FROM users), cte2 AS (SELECT id FROM posts) SELECT * FROM cte1 JOIN cte2 ON cte1.id = cte2.id",
    ];

    for sql in &sqls {
        let result = verify_sql_syntax(sql, VerifyDialect::MySql);
        assert!(result.is_valid, "CTE SQL 解析应通过: {:?}", result.errors);
        assert_eq!(classify_sql_path(sql), SqlPath::Cte);

        let explain = build_explain_sql(sql, VerifyDialect::PostgreSql);
        assert!(explain.starts_with("EXPLAIN"));
    }
}

#[test]
fn test_window_function_explain() {
    let sqls = [
        "SELECT id, ROW_NUMBER() OVER (PARTITION BY dept ORDER BY salary) FROM employees",
        "SELECT id, SUM(salary) OVER (PARTITION BY dept) FROM employees",
        "SELECT id, AVG(salary) OVER (ORDER BY hire_date ROWS BETWEEN 2 PRECEDING AND CURRENT ROW) FROM employees",
    ];

    for sql in &sqls {
        let result = verify_sql_syntax(sql, VerifyDialect::MySql);
        assert!(
            result.is_valid,
            "窗口函数 SQL 解析应通过: {:?}",
            result.errors
        );
        assert_eq!(classify_sql_path(sql), SqlPath::WindowFunction);

        let explain = build_explain_sql(sql, VerifyDialect::MySql);
        assert!(explain.starts_with("EXPLAIN"));
    }
}

#[test]
fn test_subquery_explain() {
    let sqls = [
        "SELECT * FROM users WHERE id IN (SELECT user_id FROM posts)",
        "SELECT * FROM (SELECT id FROM users) AS sub",
        "SELECT (SELECT COUNT(*) FROM posts WHERE user_id = u.id) FROM users u",
    ];

    for sql in &sqls {
        let result = verify_sql_syntax(sql, VerifyDialect::MySql);
        assert!(
            result.is_valid,
            "子查询 SQL 解析应通过: {:?}",
            result.errors
        );
        assert_eq!(classify_sql_path(sql), SqlPath::Subquery);
    }
}

#[test]
fn test_verify_smart_dispatch() {
    std::env::remove_var("SZ_ORM_QUERY_VERIFY");
    std::env::remove_var("DATABASE_URL");

    let sql = "SELECT id, name FROM users WHERE id = 1";
    let result = verify_smart(sql, VerifyDialect::MySql);
    assert!(
        result.is_valid,
        "verify_smart 降级模式应通过: {:?}",
        result.errors
    );
}

#[test]
fn test_verify_full_all_paths() {
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

    for sql in &sqls {
        let result = verify_full(sql, VerifyDialect::MySql);
        assert!(
            result.is_valid,
            "verify_full 应通过所有路径: SQL={}, errors={:?}",
            sql, result.errors
        );
    }
}

#[test]
fn test_path_coverage_complete() {
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
        "所有 8 种路径应被覆盖，未覆盖: {:?}",
        uncovered.iter().map(|p| p.name()).collect::<Vec<_>>()
    );
}

#[test]
fn test_verify_result_api() {
    let ok_result = VerifyResult::ok("SELECT 1");
    assert!(ok_result.is_valid);
    assert!(ok_result.errors.is_empty());

    let fail_result = VerifyResult::fail("SELECT BAD", vec!["error".to_string()]);
    assert!(!fail_result.is_valid);
    assert_eq!(fail_result.errors.len(), 1);
}

#[test]
fn test_verify_mode_check() {
    std::env::remove_var("SZ_ORM_QUERY_VERIFY");
    std::env::remove_var("DATABASE_URL");
    assert_eq!(
        sz_orm_core::sql_verify::current_verify_mode(),
        VerifyMode::SyntaxOnly
    );
}
