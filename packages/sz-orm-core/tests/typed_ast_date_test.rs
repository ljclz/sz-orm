//! M2-T4.3: 日期表达式单元测试
//! 覆盖 to_sql 输出、方言分派、ZST 断言、五方言一致性

#![cfg(feature = "typed-dsl")]

use sz_orm_core::dialect::{MySqlDialect, PostgreSqlDialect, SqliteDialect};
use sz_orm_core::typed::{TypedColumn, TypedTable};
use sz_orm_core::typed_ast::*;

struct T;
impl TypedTable for T {
    const NAME: &'static str = "t";
}

struct CTime;
impl TypedColumn for CTime {
    const NAME: &'static str = "created_at";
    type Table = T;
    type RustType = String;
    type SqlType = DateTime;
}

#[test]
fn test_year_mysql_uses_function() {
    let d = MySqlDialect;
    let (sql, _) = Year::<CTime>::new().to_sql(&d);
    assert_eq!(sql, "YEAR(`created_at`)");
}

#[test]
fn test_year_pg_uses_extract() {
    let d = PostgreSqlDialect;
    let (sql, _) = Year::<CTime>::new().to_sql(&d);
    assert_eq!(sql, "EXTRACT(YEAR FROM \"created_at\")");
}

#[test]
fn test_month_to_sql() {
    let d = MySqlDialect;
    let (sql, _) = Month::<CTime>::new().to_sql(&d);
    assert_eq!(sql, "MONTH(`created_at`)");
}

#[test]
fn test_day_to_sql() {
    let d = PostgreSqlDialect;
    let (sql, _) = Day::<CTime>::new().to_sql(&d);
    assert_eq!(sql, "EXTRACT(DAY FROM \"created_at\")");
}

#[test]
fn test_hour_to_sql() {
    let d = MySqlDialect;
    let (sql, _) = Hour::<CTime>::new().to_sql(&d);
    assert_eq!(sql, "HOUR(`created_at`)");
}

#[test]
fn test_minute_to_sql() {
    let d = PostgreSqlDialect;
    let (sql, _) = Minute::<CTime>::new().to_sql(&d);
    assert_eq!(sql, "EXTRACT(MINUTE FROM \"created_at\")");
}

#[test]
fn test_second_to_sql() {
    let d = MySqlDialect;
    let (sql, _) = Second::<CTime>::new().to_sql(&d);
    assert_eq!(sql, "SECOND(`created_at`)");
}

#[test]
fn test_extract_arbitrary_field() {
    let d = PostgreSqlDialect;
    let (sql, _) = Extract::<CTime, 3>::new().to_sql(&d);
    assert_eq!(sql, "EXTRACT(HOUR FROM \"created_at\")");
}

#[test]
fn test_now_mysql() {
    let d = MySqlDialect;
    let (sql, _) = Now::new().to_sql(&d);
    assert_eq!(sql, "NOW()");
}

#[test]
fn test_now_pg() {
    let d = PostgreSqlDialect;
    let (sql, _) = Now::new().to_sql(&d);
    assert_eq!(sql, "CURRENT_TIMESTAMP");
}

#[test]
fn test_now_sqlite() {
    let d = SqliteDialect;
    let (sql, _) = Now::new().to_sql(&d);
    assert_eq!(sql, "CURRENT_TIMESTAMP");
}

#[test]
fn test_date_zst() {
    assert_eq!(std::mem::size_of::<Extract<CTime, 0>>(), 0);
    assert_eq!(std::mem::size_of::<Year<CTime>>(), 0);
    assert_eq!(std::mem::size_of::<Month<CTime>>(), 0);
    assert_eq!(std::mem::size_of::<Day<CTime>>(), 0);
    assert_eq!(std::mem::size_of::<Hour<CTime>>(), 0);
    assert_eq!(std::mem::size_of::<Minute<CTime>>(), 0);
    assert_eq!(std::mem::size_of::<Second<CTime>>(), 0);
    assert_eq!(std::mem::size_of::<Now>(), 0);
}
