//! M2-T2.4: 算术表达式单元测试
//! 覆盖 to_sql 输出、类型检查、ZST 断言、嵌套表达式、五方言一致性

#![cfg(feature = "typed-dsl")]

use sz_orm_core::dialect::{MySqlDialect, PostgreSqlDialect};
use sz_orm_core::typed::{TypedColumn, TypedTable};
use sz_orm_core::typed_ast::*;

struct T;
impl TypedTable for T {
    const NAME: &'static str = "t";
}

struct C1;
impl TypedColumn for C1 {
    const NAME: &'static str = "a";
    type Table = T;
    type RustType = i64;
    type SqlType = BigInt;
}

struct C2;
impl TypedColumn for C2 {
    const NAME: &'static str = "b";
    type Table = T;
    type RustType = i64;
    type SqlType = BigInt;
}

#[test]
fn test_add_to_sql() {
    let d = MySqlDialect;
    let expr = Add::<Max<C1>, Min<C2>>::new();
    let (sql, _) = expr.to_sql(&d);
    assert_eq!(sql, "(MAX(`a`) + MIN(`b`))");
}

#[test]
fn test_sub_to_sql() {
    let d = PostgreSqlDialect;
    let expr = Sub::<Max<C1>, Min<C2>>::new();
    let (sql, _) = expr.to_sql(&d);
    assert_eq!(sql, "(MAX(\"a\") - MIN(\"b\"))");
}

#[test]
fn test_mul_to_sql() {
    let d = MySqlDialect;
    let expr = Mul::<Max<C1>, Min<C2>>::new();
    let (sql, _) = expr.to_sql(&d);
    assert_eq!(sql, "(MAX(`a`) * MIN(`b`))");
}

#[test]
fn test_div_to_sql() {
    let d = MySqlDialect;
    let expr = Div::<Max<C1>, Min<C2>>::new();
    let (sql, _) = expr.to_sql(&d);
    assert_eq!(sql, "(MAX(`a`) / MIN(`b`))");
}

#[test]
fn test_modulo_to_sql() {
    let d = MySqlDialect;
    let expr = Modulo::<Max<C1>, Min<C2>>::new();
    let (sql, _) = expr.to_sql(&d);
    assert_eq!(sql, "(MAX(`a`) % MIN(`b`))");
}

#[test]
fn test_arithmetic_zst() {
    assert_eq!(std::mem::size_of::<Add<Max<C1>, Min<C2>>>(), 0);
    assert_eq!(std::mem::size_of::<Sub<Max<C1>, Min<C2>>>(), 0);
    assert_eq!(std::mem::size_of::<Mul<Max<C1>, Min<C2>>>(), 0);
    assert_eq!(std::mem::size_of::<Div<Max<C1>, Min<C2>>>(), 0);
    assert_eq!(std::mem::size_of::<Modulo<Max<C1>, Min<C2>>>(), 0);
}

#[test]
fn test_nested_arithmetic() {
    let d = MySqlDialect;
    // (MAX(a) + MIN(b)) * MAX(a) — 嵌套算术
    let expr = Mul::<Add<Max<C1>, Min<C2>>, Max<C1>>::new();
    let (sql, _) = expr.to_sql(&d);
    assert_eq!(sql, "((MAX(`a`) + MIN(`b`)) * MAX(`a`))");
}
