//! v9.2.0 M18: query-builder 未覆盖 where_* 变体方法覆盖

#![allow(deprecated)]

use sz_orm_core::{DbType, Value};
use sz_orm_query_builder::Query;

// ---- SelectQuery where_ne/gt/ge/lt/le/like/not_in ----

#[test]
fn test_select_where_ne() {
    let b = Query::select()
        .column("id")
        .from("t")
        .where_ne("age", Value::I32(18))
        .build_with_params(DbType::MySQL);
    assert!(b.sql.contains("<> ?"));
    assert_eq!(b.params.len(), 1);
}

#[test]
fn test_select_where_gt() {
    let b = Query::select()
        .column("id")
        .from("t")
        .where_gt("age", Value::I32(18))
        .build_with_params(DbType::MySQL);
    assert!(b.sql.contains("> ?"));
    assert_eq!(b.params.len(), 1);
}

#[test]
fn test_select_where_ge() {
    let b = Query::select()
        .column("id")
        .from("t")
        .where_ge("age", Value::I32(18))
        .build_with_params(DbType::MySQL);
    assert!(b.sql.contains(">= ?"));
    assert_eq!(b.params.len(), 1);
}

#[test]
fn test_select_where_lt() {
    let b = Query::select()
        .column("id")
        .from("t")
        .where_lt("age", Value::I32(65))
        .build_with_params(DbType::MySQL);
    assert!(b.sql.contains("< ?"));
    assert_eq!(b.params.len(), 1);
}

#[test]
fn test_select_where_le() {
    let b = Query::select()
        .column("id")
        .from("t")
        .where_le("age", Value::I32(30))
        .build_with_params(DbType::MySQL);
    assert!(b.sql.contains("<= ?"));
    assert_eq!(b.params.len(), 1);
}

#[test]
fn test_select_where_like() {
    let b = Query::select()
        .column("id")
        .from("t")
        .where_like("name", Value::String("%abc%".to_string()))
        .build_with_params(DbType::MySQL);
    assert!(b.sql.contains("LIKE ?"));
    assert_eq!(b.params.len(), 1);
}

#[test]
fn test_select_where_not_in() {
    let b = Query::select()
        .column("id")
        .from("t")
        .where_not_in(
            "status",
            vec![
                Value::String("a".to_string()),
                Value::String("b".to_string()),
            ],
        )
        .build_with_params(DbType::MySQL);
    assert!(b.sql.contains("NOT IN"));
    assert_eq!(b.params.len(), 2);
}

#[test]
fn test_select_where_not_in_empty() {
    let b = Query::select()
        .column("id")
        .from("t")
        .where_not_in("status", vec![])
        .build_with_params(DbType::MySQL);
    assert!(b.sql.contains("1 = 1"));
    assert_eq!(b.params.len(), 0);
}

// ---- SelectQuery or_where_* ----

#[test]
fn test_select_or_where_ne() {
    let b = Query::select()
        .column("id")
        .from("t")
        .where_eq("a", Value::I32(1))
        .or_where_ne("b", Value::I32(2))
        .build_with_params(DbType::MySQL);
    assert!(b.sql.contains("OR"));
    assert!(b.sql.contains("<> ?"));
    assert_eq!(b.params.len(), 2);
}

#[test]
fn test_select_or_where_gt() {
    let b = Query::select()
        .column("id")
        .from("t")
        .where_eq("a", Value::I32(1))
        .or_where_gt("b", Value::I32(2))
        .build_with_params(DbType::MySQL);
    assert!(b.sql.contains("OR"));
    assert_eq!(b.params.len(), 2);
}

#[test]
fn test_select_or_where_ge() {
    let b = Query::select()
        .column("id")
        .from("t")
        .where_eq("a", Value::I32(1))
        .or_where_ge("b", Value::I32(2))
        .build_with_params(DbType::MySQL);
    assert!(b.sql.contains("OR"));
    assert_eq!(b.params.len(), 2);
}

#[test]
fn test_select_or_where_lt() {
    let b = Query::select()
        .column("id")
        .from("t")
        .where_eq("a", Value::I32(1))
        .or_where_lt("b", Value::I32(2))
        .build_with_params(DbType::MySQL);
    assert!(b.sql.contains("OR"));
    assert_eq!(b.params.len(), 2);
}

#[test]
fn test_select_or_where_le() {
    let b = Query::select()
        .column("id")
        .from("t")
        .where_eq("a", Value::I32(1))
        .or_where_le("b", Value::I32(2))
        .build_with_params(DbType::MySQL);
    assert!(b.sql.contains("OR"));
    assert_eq!(b.params.len(), 2);
}

#[test]
fn test_select_or_where_like() {
    let b = Query::select()
        .column("id")
        .from("t")
        .where_eq("a", Value::I32(1))
        .or_where_like("b", Value::String("%x%".to_string()))
        .build_with_params(DbType::MySQL);
    assert!(b.sql.contains("OR"));
    assert!(b.sql.contains("LIKE ?"));
    assert_eq!(b.params.len(), 2);
}

#[test]
fn test_select_or_where_in() {
    let b = Query::select()
        .column("id")
        .from("t")
        .where_eq("a", Value::I32(1))
        .or_where_in("b", vec![Value::I32(2), Value::I32(3)])
        .build_with_params(DbType::MySQL);
    assert!(b.sql.contains("OR"));
    assert!(b.sql.contains("IN"));
    assert_eq!(b.params.len(), 3);
}

#[test]
fn test_select_or_where_between() {
    let b = Query::select()
        .column("id")
        .from("t")
        .where_eq("a", Value::I32(1))
        .or_where_between("b", Value::I32(10), Value::I32(20))
        .build_with_params(DbType::MySQL);
    assert!(b.sql.contains("OR"));
    assert!(b.sql.contains("BETWEEN"));
    assert_eq!(b.params.len(), 3);
}

#[test]
fn test_select_or_where_null() {
    let b = Query::select()
        .column("id")
        .from("t")
        .where_eq("a", Value::I32(1))
        .or_where_null("b")
        .build_with_params(DbType::MySQL);
    assert!(b.sql.contains("OR"));
    assert!(b.sql.contains("IS NULL"));
    assert_eq!(b.params.len(), 1);
}

#[test]
fn test_select_or_where_not_null() {
    let b = Query::select()
        .column("id")
        .from("t")
        .where_eq("a", Value::I32(1))
        .or_where_not_null("b")
        .build_with_params(DbType::MySQL);
    assert!(b.sql.contains("OR"));
    assert!(b.sql.contains("IS NOT NULL"));
    assert_eq!(b.params.len(), 1);
}

// ---- InsertQuery UPSERT ----

#[test]
fn test_insert_on_conflict_do_nothing() {
    let sql = Query::insert()
        .into_table("users")
        .value("id", "1")
        .value("name", "'Alice'")
        .on_conflict_do_nothing(&["id"])
        .build_with_dialect(DbType::PostgreSQL);
    assert!(sql.contains("ON CONFLICT"));
    assert!(sql.contains("DO NOTHING"));
}

#[test]
fn test_insert_on_duplicate_key_update() {
    let sql = Query::insert()
        .into_table("users")
        .value("id", "1")
        .value("name", "'Alice'")
        .value("count", "1")
        .on_duplicate_key_update(&[("count", "count + 1")])
        .build_with_dialect(DbType::MySQL);
    assert!(sql.contains("ON DUPLICATE KEY UPDATE"));
}

#[test]
fn test_insert_replace() {
    let sql = Query::insert()
        .into_table("users")
        .value("id", "1")
        .value("name", "'Alice'")
        .replace()
        .build_with_dialect(DbType::MySQL);
    assert!(sql.starts_with("REPLACE"));
}

// ---- UpdateQuery where_* ----

#[test]
fn test_update_where_ne() {
    let b = Query::update()
        .table("users")
        .set("name", "'x'")
        .where_ne("id", Value::I64(1))
        .build_with_params(DbType::MySQL);
    assert!(b.sql.contains("<> ?"));
    assert_eq!(b.params.len(), 1);
}

#[test]
fn test_update_where_gt() {
    let b = Query::update()
        .table("users")
        .set("name", "'x'")
        .where_gt("age", Value::I32(18))
        .build_with_params(DbType::MySQL);
    assert!(b.sql.contains("> ?"));
}

#[test]
fn test_update_where_ge() {
    let b = Query::update()
        .table("users")
        .set("name", "'x'")
        .where_ge("age", Value::I32(18))
        .build_with_params(DbType::MySQL);
    assert!(b.sql.contains(">= ?"));
}

#[test]
fn test_update_where_lt() {
    let b = Query::update()
        .table("users")
        .set("name", "'x'")
        .where_lt("age", Value::I32(65))
        .build_with_params(DbType::MySQL);
    assert!(b.sql.contains("< ?"));
}

#[test]
fn test_update_where_le() {
    let b = Query::update()
        .table("users")
        .set("name", "'x'")
        .where_le("age", Value::I32(30))
        .build_with_params(DbType::MySQL);
    assert!(b.sql.contains("<= ?"));
}

#[test]
fn test_update_where_like() {
    let b = Query::update()
        .table("users")
        .set("name", "'x'")
        .where_like("name", Value::String("%test%".to_string()))
        .build_with_params(DbType::MySQL);
    assert!(b.sql.contains("LIKE ?"));
}

// ---- DeleteQuery where_* ----

#[test]
fn test_delete_where_ne() {
    let b = Query::delete()
        .from_table("users")
        .where_ne("id", Value::I64(1))
        .build_with_params(DbType::MySQL);
    assert!(b.sql.contains("<> ?"));
    assert_eq!(b.params.len(), 1);
}

#[test]
fn test_delete_where_gt() {
    let b = Query::delete()
        .from_table("users")
        .where_gt("age", Value::I32(100))
        .build_with_params(DbType::MySQL);
    assert!(b.sql.contains("> ?"));
}

#[test]
fn test_delete_where_ge() {
    let b = Query::delete()
        .from_table("users")
        .where_ge("age", Value::I32(100))
        .build_with_params(DbType::MySQL);
    assert!(b.sql.contains(">= ?"));
}

#[test]
fn test_delete_where_lt() {
    let b = Query::delete()
        .from_table("users")
        .where_lt("age", Value::I32(0))
        .build_with_params(DbType::MySQL);
    assert!(b.sql.contains("< ?"));
}

#[test]
fn test_delete_where_le() {
    let b = Query::delete()
        .from_table("users")
        .where_le("age", Value::I32(0))
        .build_with_params(DbType::MySQL);
    assert!(b.sql.contains("<= ?"));
}

#[test]
fn test_delete_where_like() {
    let b = Query::delete()
        .from_table("users")
        .where_like("name", Value::String("%tmp%".to_string()))
        .build_with_params(DbType::MySQL);
    assert!(b.sql.contains("LIKE ?"));
}
