//! YugabyteDB 方言单元测试（M5-T2.4）
//!
//! 验证 YugabyteDbDialect 委派 PostgreSqlDialect 的行为一致性：
//! - to_sql 输出与 PG 一致
//! - quote/escape_string/build_pagination 与 PG 一致
//! - feature gate 隔离正确

#![cfg(feature = "dialect-yugabytedb")]

use sz_orm_core::dialect::{Dialect, PostgreSqlDialect, YugabyteDbDialect};
use sz_orm_core::DbType;

#[test]
fn test_yugabytedb_db_type() {
    let dialect = YugabyteDbDialect;
    assert_eq!(dialect.db_type(), DbType::YugabyteDB);
}

#[test]
fn test_yugabytedb_is_postgres_family() {
    assert!(DbType::YugabyteDB.is_postgres_family());
}

#[test]
fn test_yugabytedb_as_str() {
    assert_eq!(DbType::YugabyteDB.as_str(), "yugabytedb");
}

#[test]
fn test_yugabytedb_from_str() {
    assert_eq!(DbType::from_str("yugabytedb"), Some(DbType::YugabyteDB));
    assert_eq!(DbType::from_str("yugabyte"), Some(DbType::YugabyteDB));
    assert_eq!(DbType::from_str("YUGABYTE"), Some(DbType::YugabyteDB));
}

#[test]
fn test_yugabytedb_default_port() {
    assert_eq!(DbType::YugabyteDB.default_port(), 5433);
}

#[test]
fn test_yugabytedb_quote_matches_pg() {
    let yb = YugabyteDbDialect;
    let pg = PostgreSqlDialect;
    let identifiers = [
        "users",
        "order",
        "select",
        "table_name",
        "col_with_underscore",
    ];
    for id in identifiers {
        assert_eq!(yb.quote(id), pg.quote(id), "quote mismatch for: {}", id);
    }
}

#[test]
fn test_yugabytedb_escape_string_matches_pg() {
    let yb = YugabyteDbDialect;
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
            yb.escape_string(s),
            pg.escape_string(s),
            "escape_string mismatch for: {:?}",
            s
        );
    }
}

#[test]
fn test_yugabytedb_pagination_matches_pg() {
    let yb = YugabyteDbDialect;
    let pg = PostgreSqlDialect;
    let sql = "SELECT * FROM users";
    for (page, limit) in [(1, 10), (2, 20), (5, 100), (0, 1)] {
        assert_eq!(
            yb.build_pagination(sql, page, limit),
            pg.build_pagination(sql, page, limit),
            "pagination mismatch for page={}, limit={}",
            page,
            limit
        );
    }
}

#[test]
fn test_yugabytedb_supports_returning() {
    let yb = YugabyteDbDialect;
    let pg = PostgreSqlDialect;
    assert_eq!(yb.supports_returning(), pg.supports_returning());
    assert!(
        yb.supports_returning(),
        "YugabyteDB should support RETURNING"
    );
}

#[test]
fn test_yugabytedb_json_type_matches_pg() {
    let yb = YugabyteDbDialect;
    let pg = PostgreSqlDialect;
    assert_eq!(yb.json_type(), pg.json_type());
}

#[test]
fn test_yugabytedb_json_extract_matches_pg() {
    let yb = YugabyteDbDialect;
    let pg = PostgreSqlDialect;
    assert_eq!(
        yb.json_extract("data", "field"),
        pg.json_extract("data", "field")
    );
}

#[test]
fn test_yugabytedb_clone_box() {
    let yb = YugabyteDbDialect;
    let cloned = yb.clone_box();
    assert_eq!(cloned.db_type(), DbType::YugabyteDB);
}

#[test]
fn test_yugabytedb_auto_increment_keyword_matches_pg() {
    let yb = YugabyteDbDialect;
    let pg = PostgreSqlDialect;
    assert_eq!(yb.auto_increment_keyword(), pg.auto_increment_keyword());
}

#[test]
fn test_yugabytedb_concat_matches_pg() {
    let yb = YugabyteDbDialect;
    let pg = PostgreSqlDialect;
    let parts = ["a", "b", "c"];
    assert_eq!(yb.concat(&parts), pg.concat(&parts));
}

#[test]
fn test_yugabytedb_bool_to_int_matches_pg() {
    let yb = YugabyteDbDialect;
    let pg = PostgreSqlDialect;
    assert_eq!(yb.bool_to_int("true"), pg.bool_to_int("true"));
    assert_eq!(yb.bool_to_int("false"), pg.bool_to_int("false"));
}
