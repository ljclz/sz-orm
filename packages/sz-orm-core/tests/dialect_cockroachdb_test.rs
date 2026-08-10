//! CockroachDB 方言单元测试（M5-T1.7）
//!
//! 验证 CockroachDbDialect 委派 PostgreSqlDialect 的行为一致性：
//! - to_sql 输出与 PG 一致
//! - quote/escape_string/build_pagination 与 PG 一致
//! - feature gate 隔离正确

#![cfg(feature = "dialect-cockroachdb")]

use sz_orm_core::dialect::{CockroachDbDialect, Dialect, PostgreSqlDialect};
use sz_orm_core::DbType;

#[test]
fn test_cockroachdb_db_type() {
    let dialect = CockroachDbDialect;
    assert_eq!(dialect.db_type(), DbType::CockroachDB);
}

#[test]
fn test_cockroachdb_is_postgres_family() {
    assert!(DbType::CockroachDB.is_postgres_family());
}

#[test]
fn test_cockroachdb_as_str() {
    assert_eq!(DbType::CockroachDB.as_str(), "cockroachdb");
}

#[test]
fn test_cockroachdb_from_str() {
    assert_eq!(DbType::from_str("cockroachdb"), Some(DbType::CockroachDB));
    assert_eq!(DbType::from_str("cockroach"), Some(DbType::CockroachDB));
    assert_eq!(DbType::from_str("COCKROACHDB"), Some(DbType::CockroachDB));
}

#[test]
fn test_cockroachdb_default_port() {
    assert_eq!(DbType::CockroachDB.default_port(), 26257);
}

#[test]
fn test_cockroachdb_quote_matches_pg() {
    let crdb = CockroachDbDialect;
    let pg = PostgreSqlDialect;
    let identifiers = [
        "users",
        "order",
        "select",
        "table_name",
        "col_with_underscore",
    ];
    for id in identifiers {
        assert_eq!(crdb.quote(id), pg.quote(id), "quote mismatch for: {}", id);
    }
}

#[test]
fn test_cockroachdb_escape_string_matches_pg() {
    let crdb = CockroachDbDialect;
    let pg = PostgreSqlDialect;
    let inputs = [
        "simple",
        "with'quote",
        "with\"double",
        "back\\slash",
        "newline\n",
    ];
    for s in inputs {
        assert_eq!(
            crdb.escape_string(s),
            pg.escape_string(s),
            "escape_string mismatch for: {:?}",
            s
        );
    }
}

#[test]
fn test_cockroachdb_pagination_matches_pg() {
    let crdb = CockroachDbDialect;
    let pg = PostgreSqlDialect;
    let sql = "SELECT * FROM users";
    for (page, limit) in [(1, 10), (2, 20), (5, 100), (0, 1)] {
        assert_eq!(
            crdb.build_pagination(sql, page, limit),
            pg.build_pagination(sql, page, limit),
            "pagination mismatch for page={}, limit={}",
            page,
            limit
        );
    }
}

#[test]
fn test_cockroachdb_supports_returning() {
    let crdb = CockroachDbDialect;
    let pg = PostgreSqlDialect;
    assert_eq!(crdb.supports_returning(), pg.supports_returning());
    assert!(
        crdb.supports_returning(),
        "CockroachDB should support RETURNING"
    );
}

#[test]
fn test_cockroachdb_json_type_matches_pg() {
    let crdb = CockroachDbDialect;
    let pg = PostgreSqlDialect;
    assert_eq!(crdb.json_type(), pg.json_type());
}

#[test]
fn test_cockroachdb_json_extract_matches_pg() {
    let crdb = CockroachDbDialect;
    let pg = PostgreSqlDialect;
    assert_eq!(
        crdb.json_extract("data", "field"),
        pg.json_extract("data", "field")
    );
}

#[test]
fn test_cockroachdb_clone_box() {
    let crdb = CockroachDbDialect;
    let cloned = crdb.clone_box();
    assert_eq!(cloned.db_type(), DbType::CockroachDB);
}

#[test]
fn test_cockroachdb_auto_increment_keyword_matches_pg() {
    let crdb = CockroachDbDialect;
    let pg = PostgreSqlDialect;
    assert_eq!(crdb.auto_increment_keyword(), pg.auto_increment_keyword());
}

#[test]
fn test_cockroachdb_concat_matches_pg() {
    let crdb = CockroachDbDialect;
    let pg = PostgreSqlDialect;
    let parts = ["a", "b", "c"];
    assert_eq!(crdb.concat(&parts), pg.concat(&parts));
}

#[test]
fn test_cockroachdb_bool_to_int_matches_pg() {
    let crdb = CockroachDbDialect;
    let pg = PostgreSqlDialect;
    assert_eq!(crdb.bool_to_int("true"), pg.bool_to_int("true"));
    assert_eq!(crdb.bool_to_int("false"), pg.bool_to_int("false"));
}
