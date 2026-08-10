//! M2-T3.3: 字符串表达式单元测试
//! 覆盖 to_sql 输出、方言分派、ZST 断言、五方言一致性

#![cfg(feature = "typed-dsl")]

use sz_orm_core::dialect::{
    MySqlDialect, OracleDialect, PostgreSqlDialect, SqlServerDialect, SqliteDialect,
};
use sz_orm_core::typed::{TypedColumn, TypedTable};
use sz_orm_core::typed_ast::*;

struct T;
impl TypedTable for T {
    const NAME: &'static str = "t";
}

struct CName;
impl TypedColumn for CName {
    const NAME: &'static str = "name";
    type Table = T;
    type RustType = String;
    type SqlType = Text;
}

struct CId;
impl TypedColumn for CId {
    const NAME: &'static str = "id";
    type Table = T;
    type RustType = i64;
    type SqlType = BigInt;
}

#[test]
fn test_concat_pg_uses_pipe() {
    let d = PostgreSqlDialect;
    let expr = Concat::<Max<CId>, Min<CId>>::new();
    let (sql, _) = expr.to_sql(&d);
    assert_eq!(sql, "(MAX(\"id\") || MIN(\"id\"))");
}

#[test]
fn test_concat_mysql_uses_function() {
    let d = MySqlDialect;
    let expr = Concat::<Max<CId>, Min<CId>>::new();
    let (sql, _) = expr.to_sql(&d);
    assert_eq!(sql, "CONCAT(MAX(`id`), MIN(`id`))");
}

#[test]
fn test_ilike_pg_native() {
    let d = PostgreSqlDialect;
    let expr = ILike::<CName>::new();
    let (sql, _) = expr.to_sql(&d);
    assert_eq!(sql, "\"name\" ILIKE ?");
}

#[test]
fn test_ilike_mysql_lower_like() {
    let d = MySqlDialect;
    let expr = ILike::<CName>::new();
    let (sql, _) = expr.to_sql(&d);
    assert_eq!(sql, "LOWER(`name`) LIKE LOWER(?)");
}

#[test]
fn test_length_sqlserver_uses_len() {
    let d = SqlServerDialect;
    let expr = Length::<CName>::new();
    let (sql, _) = expr.to_sql(&d);
    assert_eq!(sql, "LEN([name])");
}

#[test]
fn test_length_pg_uses_length() {
    let d = PostgreSqlDialect;
    let expr = Length::<CName>::new();
    let (sql, _) = expr.to_sql(&d);
    assert_eq!(sql, "LENGTH(\"name\")");
}

#[test]
fn test_lower_to_sql() {
    let d = MySqlDialect;
    let (sql, _) = Lower::<CName>::new().to_sql(&d);
    assert_eq!(sql, "LOWER(`name`)");
}

#[test]
fn test_upper_to_sql() {
    let d = SqliteDialect;
    let (sql, _) = Upper::<CName>::new().to_sql(&d);
    assert_eq!(sql, "UPPER(\"name\")");
}

#[test]
fn test_trim_to_sql() {
    let d = OracleDialect;
    let (sql, _) = Trim::<CName>::new().to_sql(&d);
    assert_eq!(sql, "TRIM(\"name\")");
}

#[test]
fn test_substring_pg_uses_substr() {
    let d = PostgreSqlDialect;
    let (sql, _) = Substring::<CName, 1, 3>::new().to_sql(&d);
    assert_eq!(sql, "SUBSTR(\"name\", 1, 3)");
}

#[test]
fn test_substring_mysql_uses_substring() {
    let d = MySqlDialect;
    let (sql, _) = Substring::<CName, 1, 3>::new().to_sql(&d);
    assert_eq!(sql, "SUBSTRING(`name`, 1, 3)");
}

#[test]
fn test_string_zst() {
    assert_eq!(std::mem::size_of::<Concat<Max<CId>, Min<CId>>>(), 0);
    assert_eq!(std::mem::size_of::<ILike<CName>>(), 0);
    assert_eq!(std::mem::size_of::<Length<CName>>(), 0);
    assert_eq!(std::mem::size_of::<Lower<CName>>(), 0);
    assert_eq!(std::mem::size_of::<Upper<CName>>(), 0);
    assert_eq!(std::mem::size_of::<Trim<CName>>(), 0);
    assert_eq!(std::mem::size_of::<Substring<CName, 1, 3>>(), 0);
}
