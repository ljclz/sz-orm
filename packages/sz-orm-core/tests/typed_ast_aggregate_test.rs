//! M2-T1.4: 聚合表达式单元测试
//! 覆盖 to_sql 输出、ZST 断言、五方言一致性

#![cfg(feature = "typed-dsl")]

use sz_orm_core::dialect::{
    MySqlDialect, OracleDialect, PostgreSqlDialect, SqlServerDialect, SqliteDialect,
};
use sz_orm_core::typed::{TypedColumn, TypedTable};
use sz_orm_core::typed_ast::*;

struct UsersTable;
impl TypedTable for UsersTable {
    const NAME: &'static str = "users";
}

struct ColId;
impl TypedColumn for ColId {
    const NAME: &'static str = "id";
    type Table = UsersTable;
    type RustType = i64;
    type SqlType = BigInt;
}

struct ColScore;
impl TypedColumn for ColScore {
    const NAME: &'static str = "score";
    type Table = UsersTable;
    type RustType = i32;
    type SqlType = Integer;
}

#[test]
fn test_max_to_sql() {
    let dialect = MySqlDialect;
    let expr = Max::<ColId>::new();
    let (sql, params) = expr.to_sql(&dialect);
    assert_eq!(sql, "MAX(`id`)");
    assert!(params.is_empty());
}

#[test]
fn test_min_to_sql() {
    let dialect = PostgreSqlDialect;
    let expr = Min::<ColId>::new();
    let (sql, _) = expr.to_sql(&dialect);
    assert_eq!(sql, "MIN(\"id\")");
}

#[test]
fn test_sum_to_sql() {
    let dialect = MySqlDialect;
    let expr = Sum::<ColScore>::new();
    let (sql, _) = expr.to_sql(&dialect);
    assert_eq!(sql, "SUM(`score`)");
}

#[test]
fn test_avg_to_sql() {
    let dialect = SqliteDialect;
    let expr = Avg::<ColScore>::new();
    let (sql, _) = expr.to_sql(&dialect);
    assert_eq!(sql, "AVG(\"score\")");
}

#[test]
fn test_count_to_sql() {
    let dialect = OracleDialect;
    let expr = Count::<ColId>::new();
    let (sql, _) = expr.to_sql(&dialect);
    assert_eq!(sql, "COUNT(\"id\")");
}

#[test]
fn test_count_star_to_sql() {
    let dialect = SqlServerDialect;
    let expr = CountStar::new();
    let (sql, _) = expr.to_sql(&dialect);
    assert_eq!(sql, "COUNT(*)");
}

#[test]
fn test_aggregate_zst() {
    assert_eq!(std::mem::size_of::<Max<ColId>>(), 0);
    assert_eq!(std::mem::size_of::<Min<ColId>>(), 0);
    assert_eq!(std::mem::size_of::<Sum<ColScore>>(), 0);
    assert_eq!(std::mem::size_of::<Avg<ColScore>>(), 0);
    assert_eq!(std::mem::size_of::<Count<ColId>>(), 0);
    assert_eq!(std::mem::size_of::<CountStar>(), 0);
}

#[test]
fn test_aggregate_five_dialect_consistency() {
    let dialects: Vec<Box<dyn sz_orm_core::dialect::Dialect>> = vec![
        Box::new(MySqlDialect),
        Box::new(PostgreSqlDialect),
        Box::new(SqliteDialect),
        Box::new(OracleDialect),
        Box::new(SqlServerDialect),
    ];
    let expr = Max::<ColId>::new();
    for d in &dialects {
        let (sql, params) = expr.to_sql(d.as_ref());
        assert!(
            sql.starts_with("MAX("),
            "MAX should work in all dialects: {}",
            sql
        );
        assert!(params.is_empty());
    }
}
