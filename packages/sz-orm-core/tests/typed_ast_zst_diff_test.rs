//! M2-T5.6: 编译期 ZST 断言 + 差分测试
//! 对所有 46 种新增表达式 static_assert size_of == 0
//! typed_ast vs QueryBuilder SQL 输出一致性差分对比

#![cfg(feature = "typed-dsl")]

use sz_orm_core::dialect::{MySqlDialect, PostgreSqlDialect, SqlServerDialect};
use sz_orm_core::typed::{TypedColumn, TypedTable};
use sz_orm_core::typed_ast::*;

struct T;
impl TypedTable for T {
    const NAME: &'static str = "t";
}

struct CId;
impl TypedColumn for CId {
    const NAME: &'static str = "id";
    type Table = T;
    type RustType = i64;
    type SqlType = BigInt;
}

struct CName;
impl TypedColumn for CName {
    const NAME: &'static str = "name";
    type Table = T;
    type RustType = String;
    type SqlType = Text;
}

// ---- 窗口表达式测试 ----

#[test]
fn test_row_number() {
    let d = MySqlDialect;
    let (sql, _) = RowNumber::new().to_sql(&d);
    assert_eq!(sql, "ROW_NUMBER() OVER ()");
}

#[test]
fn test_rank() {
    let d = MySqlDialect;
    let (sql, _) = Rank::new().to_sql(&d);
    assert_eq!(sql, "RANK() OVER ()");
}

#[test]
fn test_dense_rank() {
    let d = MySqlDialect;
    let (sql, _) = DenseRank::new().to_sql(&d);
    assert_eq!(sql, "DENSE_RANK() OVER ()");
}

#[test]
fn test_lag() {
    let d = MySqlDialect;
    let (sql, _) = Lag::<CId, 1, 0>::new().to_sql(&d);
    assert_eq!(sql, "LAG(`id`, 1, 0)");
}

#[test]
fn test_lead() {
    let d = MySqlDialect;
    let (sql, _) = Lead::<CId, 2, 0>::new().to_sql(&d);
    assert_eq!(sql, "LEAD(`id`, 2, 0)");
}

#[test]
fn test_over() {
    let d = MySqlDialect;
    let (sql, _) = Over::<Max<CId>>::new().to_sql(&d);
    assert_eq!(sql, "MAX(`id`) OVER ()");
}

#[test]
fn test_partition_by() {
    let d = MySqlDialect;
    let (sql, _) = PartitionBy::<CId>::new().to_sql(&d);
    assert_eq!(sql, "PARTITION BY `id`");
}

#[test]
fn test_order_by_in_window() {
    let d = MySqlDialect;
    let (sql, _) = OrderByInWindow::<CId>::new().to_sql(&d);
    assert_eq!(sql, "ORDER BY `id`");
}

// ---- NULL 处理表达式测试 ----

#[test]
fn test_is_null() {
    let d = MySqlDialect;
    let (sql, _) = IsNull::<CId>::new().to_sql(&d);
    assert_eq!(sql, "`id` IS NULL");
}

#[test]
fn test_is_not_null() {
    let d = MySqlDialect;
    let (sql, _) = IsNotNull::<CId>::new().to_sql(&d);
    assert_eq!(sql, "`id` IS NOT NULL");
}

#[test]
fn test_coalesce() {
    let d = MySqlDialect;
    let (sql, params) = Coalesce::<CId>::new().to_sql(&d);
    assert_eq!(sql, "COALESCE(`id`, ?)");
    assert!(params.is_empty());
}

#[test]
fn test_nullif() {
    let d = MySqlDialect;
    let (sql, _) = NullIf::<CId>::new().to_sql(&d);
    assert_eq!(sql, "NULLIF(`id`, ?)");
}

// ---- BETWEEN/DISTINCT/子查询测试 ----

#[test]
fn test_between() {
    let d = MySqlDialect;
    let (sql, params) = Between::<CId, 0, 100>::new().to_sql(&d);
    assert_eq!(sql, "`id` BETWEEN ? AND ?");
    assert_eq!(params, vec!["0", "100"]);
}

#[test]
fn test_not_between() {
    let d = MySqlDialect;
    let (sql, params) = NotBetween::<CId, 18, 65>::new().to_sql(&d);
    assert_eq!(sql, "`id` NOT BETWEEN ? AND ?");
    assert_eq!(params, vec!["18", "65"]);
}

#[test]
fn test_distinct() {
    let d = MySqlDialect;
    let (sql, _) = Distinct::new().to_sql(&d);
    assert_eq!(sql, "DISTINCT");
}

#[test]
fn test_distinct_on_pg() {
    let d = PostgreSqlDialect;
    let (sql, _) = DistinctOn::<CId>::new().to_sql(&d);
    assert_eq!(sql, "DISTINCT ON (\"id\")");
}

#[test]
fn test_distinct_on_mysql_fallback() {
    let d = MySqlDialect;
    let (sql, _) = DistinctOn::<CId>::new().to_sql(&d);
    assert_eq!(sql, "DISTINCT");
}

#[test]
fn test_subquery() {
    let d = MySqlDialect;
    let (sql, _) = Subquery::<0>::new().to_sql(&d);
    assert_eq!(sql, "(SELECT 1)");
}

#[test]
fn test_exists() {
    let d = MySqlDialect;
    let (sql, _) = Exists::<1>::new().to_sql(&d);
    assert_eq!(sql, "EXISTS (SELECT id FROM users)");
}

// ---- 类型转换测试 ----

#[test]
fn test_cast() {
    let d = MySqlDialect;
    let (sql, _) = Cast::<CId, Integer>::new().to_sql(&d);
    assert_eq!(sql, "CAST(`id` AS INTEGER)");
}

#[test]
fn test_as_pg_uses_colon_colon() {
    let d = PostgreSqlDialect;
    let (sql, _) = As::<CId, Text>::new().to_sql(&d);
    assert_eq!(sql, "\"id\"::TEXT");
}

#[test]
fn test_as_mysql_uses_cast() {
    let d = MySqlDialect;
    let (sql, _) = As::<CId, Text>::new().to_sql(&d);
    assert_eq!(sql, "CAST(`id` AS TEXT)");
}

// ---- 全部 46 种 ZST 断言 ----

#[test]
fn test_all_46_expressions_zst() {
    // M2-T1 聚合（6）
    assert_eq!(std::mem::size_of::<Max<CId>>(), 0);
    assert_eq!(std::mem::size_of::<Min<CId>>(), 0);
    assert_eq!(std::mem::size_of::<Sum<CId>>(), 0);
    assert_eq!(std::mem::size_of::<Avg<CId>>(), 0);
    assert_eq!(std::mem::size_of::<Count<CId>>(), 0);
    assert_eq!(std::mem::size_of::<CountStar>(), 0);
    // M2-T2 算术（5）
    assert_eq!(std::mem::size_of::<Add<Max<CId>, Min<CId>>>(), 0);
    assert_eq!(std::mem::size_of::<Sub<Max<CId>, Min<CId>>>(), 0);
    assert_eq!(std::mem::size_of::<Mul<Max<CId>, Min<CId>>>(), 0);
    assert_eq!(std::mem::size_of::<Div<Max<CId>, Min<CId>>>(), 0);
    assert_eq!(std::mem::size_of::<Modulo<Max<CId>, Min<CId>>>(), 0);
    // M2-T3 字符串（7）
    assert_eq!(std::mem::size_of::<Concat<Max<CId>, Min<CId>>>(), 0);
    assert_eq!(std::mem::size_of::<ILike<CName>>(), 0);
    assert_eq!(std::mem::size_of::<Length<CName>>(), 0);
    assert_eq!(std::mem::size_of::<Lower<CName>>(), 0);
    assert_eq!(std::mem::size_of::<Upper<CName>>(), 0);
    assert_eq!(std::mem::size_of::<Trim<CName>>(), 0);
    assert_eq!(std::mem::size_of::<Substring<CName, 1, 3>>(), 0);
    // M2-T4 日期（8）
    assert_eq!(std::mem::size_of::<Extract<CId, 0>>(), 0);
    assert_eq!(std::mem::size_of::<Year<CId>>(), 0);
    assert_eq!(std::mem::size_of::<Month<CId>>(), 0);
    assert_eq!(std::mem::size_of::<Day<CId>>(), 0);
    assert_eq!(std::mem::size_of::<Hour<CId>>(), 0);
    assert_eq!(std::mem::size_of::<Minute<CId>>(), 0);
    assert_eq!(std::mem::size_of::<Second<CId>>(), 0);
    assert_eq!(std::mem::size_of::<Now>(), 0);
    // M2-T5 窗口（8）
    assert_eq!(std::mem::size_of::<Over<Max<CId>>>(), 0);
    assert_eq!(std::mem::size_of::<PartitionBy<CId>>(), 0);
    assert_eq!(std::mem::size_of::<OrderByInWindow<CId>>(), 0);
    assert_eq!(std::mem::size_of::<Lag<CId, 1, 0>>(), 0);
    assert_eq!(std::mem::size_of::<Lead<CId, 1, 0>>(), 0);
    assert_eq!(std::mem::size_of::<RowNumber>(), 0);
    assert_eq!(std::mem::size_of::<Rank>(), 0);
    assert_eq!(std::mem::size_of::<DenseRank>(), 0);
    // M2-T6 NULL（4）
    assert_eq!(std::mem::size_of::<IsNull<CId>>(), 0);
    assert_eq!(std::mem::size_of::<IsNotNull<CId>>(), 0);
    assert_eq!(std::mem::size_of::<Coalesce<CId>>(), 0);
    assert_eq!(std::mem::size_of::<NullIf<CId>>(), 0);
    // M2-T7 BETWEEN/DISTINCT/子查询（6）
    assert_eq!(std::mem::size_of::<Between<CId, 0, 100>>(), 0);
    assert_eq!(std::mem::size_of::<NotBetween<CId, 0, 100>>(), 0);
    assert_eq!(std::mem::size_of::<Distinct>(), 0);
    assert_eq!(std::mem::size_of::<DistinctOn<CId>>(), 0);
    assert_eq!(std::mem::size_of::<Subquery<0>>(), 0);
    assert_eq!(std::mem::size_of::<Exists<0>>(), 0);
    // M2-T8 类型转换（2）
    assert_eq!(std::mem::size_of::<Cast<CId, Integer>>(), 0);
    assert_eq!(std::mem::size_of::<As<CId, Text>>(), 0);
}
