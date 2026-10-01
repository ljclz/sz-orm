use sz_orm_core::{DbType, Value};
use sz_orm_query_builder::{DeleteQuery, UpdateQuery};

#[test]
fn test_update_where_eq_build() {
    let built = UpdateQuery::new()
        .table("users")
        .set("name", "'Bob'")
        .where_eq("id", Value::I64(1))
        .build_with_params(DbType::MySQL);
    assert!(built.sql.contains("UPDATE"));
    assert!(built.sql.contains("WHERE"));
    assert!(built.sql.contains('?'));
    assert_eq!(built.params.len(), 1);
}

#[test]
fn test_update_where_in_build() {
    let built = UpdateQuery::new()
        .table("users")
        .set("active", "0")
        .where_in("id", vec![Value::I64(1), Value::I64(2)])
        .build_with_params(DbType::MySQL);
    assert!(built.sql.contains("IN (?, ?)"));
    assert_eq!(built.params.len(), 2);
}

#[test]
fn test_delete_where_eq_build() {
    let built = DeleteQuery::new()
        .from_table("users")
        .where_eq("id", Value::I64(1))
        .build_with_params(DbType::MySQL);
    assert!(built.sql.contains("DELETE FROM"));
    assert!(built.sql.contains("WHERE"));
    assert!(built.sql.contains('?'));
    assert_eq!(built.params.len(), 1);
}

#[test]
fn test_delete_where_between_build() {
    let built = DeleteQuery::new()
        .from_table("logs")
        .where_between(
            "ts",
            Value::String("2024-01-01".into()),
            Value::String("2024-12-31".into()),
        )
        .build_with_params(DbType::MySQL);
    assert!(built.sql.contains("BETWEEN ? AND ?"));
    assert_eq!(built.params.len(), 2);
}
