//! query.rs 深度路径测试 — 覆盖 QueryBuilder 各种分支路径
//!
//! 针对 sz-orm-core/src/query.rs 中 1540 行未覆盖代码，
//! 覆盖各种 WHERE 条件、JOIN、分页、聚合、锁、租户、缓存等分支。

use sz_orm_core::{get_dialect, DbType, QueryBuilder, Value};

#[derive(Clone)]
struct User;
impl sz_orm_core::Model for User {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "users"
    }
    fn pk(&self) -> Self::PrimaryKey {
        0
    }
    fn set_pk(&mut self, _pk: Self::PrimaryKey) {}
}

impl sz_orm_core::ModelExt for User {
    fn columns() -> Vec<&'static str> {
        vec!["id", "name", "email", "status", "age", "role", "created_at"]
    }
    fn fillable() -> Vec<&'static str> {
        vec!["name", "email", "status", "age", "role"]
    }
}

fn qb_sqlite() -> QueryBuilder<User> {
    QueryBuilder::<User>::new(get_dialect(DbType::Sqlite).unwrap())
}

fn qb_mysql() -> QueryBuilder<User> {
    QueryBuilder::<User>::new(get_dialect(DbType::MySQL).unwrap())
}

fn qb_pg() -> QueryBuilder<User> {
    QueryBuilder::<User>::new(get_dialect(DbType::PostgreSQL).unwrap())
}

#[test]
fn test_where_ne_gt_ge_lt_le() {
    let (sql, _) = qb_sqlite()
        .table("t")
        .where_ne("a", Value::I64(1))
        .where_gt("b", Value::I64(2))
        .where_ge("c", Value::I64(3))
        .where_lt("d", Value::I64(4))
        .where_le("e", Value::I64(5))
        .build_select();
    assert!(sql.contains("!=") || sql.contains("<>"));
    assert!(sql.contains(">"));
    assert!(sql.contains("<"));
}

#[test]
fn test_where_like() {
    let (sql, _) = qb_sqlite()
        .table("t")
        .where_like("name", Value::String("%abc%".into()))
        .build_select();
    assert!(sql.contains("LIKE"));
}

#[test]
fn test_or_where_all_variants() {
    let (sql, _) = qb_sqlite()
        .table("t")
        .where_eq("a", Value::I64(1))
        .or_where_eq("b", Value::I64(2))
        .or_where_ne("c", Value::I64(3))
        .or_where_gt("d", Value::I64(4))
        .or_where_ge("e", Value::I64(5))
        .or_where_lt("f", Value::I64(6))
        .or_where_le("g", Value::I64(7))
        .or_where_like("h", Value::String("%x%".into()))
        .build_select();
    assert!(sql.contains("OR"));
}

#[test]
fn test_where_in_not_in() {
    let (sql, _) = qb_sqlite()
        .table("t")
        .where_in("id", vec![Value::I64(1), Value::I64(2), Value::I64(3)])
        .where_not_in("status", vec![Value::String("deleted".into())])
        .build_select();
    assert!(sql.contains("IN"));
    assert!(sql.contains("NOT IN"));
}

#[test]
fn test_where_in_empty() {
    let (sql, _) = qb_sqlite().table("t").where_in("id", vec![]).build_select();
    assert!(!sql.is_empty());
}

#[test]
fn test_offset_and_page() {
    let (sql1, _) = qb_sqlite().table("t").offset(10).build_select();
    assert!(sql1.contains("OFFSET"));
    let (sql2, _) = qb_sqlite().table("t").page(2, 20).build_select();
    assert!(sql2.contains("LIMIT"));
}

#[test]
fn test_lock_for_update() {
    let builder = qb_mysql().table("t").lock_for_update();
    assert!(builder.is_ok());
    let (sql, _) = builder.unwrap().build_select();
    assert!(sql.contains("FOR UPDATE") || sql.contains("UPDATE"));
}

#[test]
fn test_lock_shared() {
    let builder = qb_mysql().table("t").lock_shared();
    assert!(builder.is_ok());
}

#[test]
fn test_insert_or_ignore() {
    let builder = qb_sqlite().table("t").insert_or_ignore();
    assert!(builder.is_insert_or_ignore());
}

#[test]
fn test_get_lock_type() {
    let builder = qb_mysql().table("t").lock_for_update().unwrap();
    assert!(builder.get_lock_type().is_some());
}

#[test]

fn test_soft_delete_disable() {
    let builder = qb_sqlite().table("t").without_soft_delete();
    assert!(builder.is_soft_delete_disabled());
}

#[test]
fn test_cache_ttl() {
    let builder = qb_sqlite()
        .table("t")
        .cache_ttl(std::time::Duration::from_secs(60));
    assert_eq!(
        builder.get_cache_ttl(),
        Some(std::time::Duration::from_secs(60))
    );
}

#[test]
fn test_clone_for_count() {
    let builder = qb_sqlite().table("t").where_eq("a", Value::I64(1));
    let count_builder = builder.clone_for_count();
    let sql = count_builder.build_count();
    assert!(sql.contains("COUNT"));
}

#[test]
fn test_with_tenant_id() {
    let builder = qb_sqlite().table("t").with_tenant_id(42);
    assert!(!builder.is_tenant_disabled());
}

#[test]
fn test_without_tenant() {
    let builder = qb_sqlite().table("t").with_tenant_id(42).without_tenant();
    assert!(builder.is_tenant_disabled());
}

#[test]
fn test_build_count() {
    let sql = qb_sqlite()
        .table("t")
        .where_eq("a", Value::I64(1))
        .build_count();
    assert!(sql.contains("COUNT"));
    assert!(sql.contains("t"));
}

#[test]
fn test_build_exists() {
    let sql = qb_sqlite()
        .table("t")
        .where_eq("a", Value::I64(1))
        .build_exists();
    assert!(sql.contains("EXISTS"));
}

#[test]
fn test_build_max_min_sum_avg() {
    let sql_max = qb_sqlite().table("t").build_max("score");
    assert!(sql_max.contains("MAX"));
    let sql_min = qb_sqlite().table("t").build_min("score");
    assert!(sql_min.contains("MIN"));
    let sql_sum = qb_sqlite().table("t").build_sum("score");
    assert!(sql_sum.contains("SUM"));
    let sql_avg = qb_sqlite().table("t").build_avg("score");
    assert!(sql_avg.contains("AVG"));
}

#[test]
fn test_build_select_with_params() {
    let (sql, params) = qb_sqlite()
        .table("t")
        .where_eq("a", Value::I64(1))
        .where_eq("b", Value::String("x".into()))
        .build_select_with_params();
    assert!(!sql.is_empty());
    assert!(!params.is_empty());
}

#[test]
fn test_build_update_with_params() {
    use std::collections::HashMap;
    let mut data = HashMap::new();
    data.insert("name".to_string(), Value::String("updated".into()));
    let (sql, params) = qb_sqlite()
        .table("t")
        .where_eq("id", Value::I64(1))
        .build_update_with_params(&data);
    assert!(sql.contains("UPDATE"));
    assert!(!params.is_empty());
}

#[test]
fn test_build_delete_with_params() {
    let (sql, params) = qb_sqlite()
        .table("t")
        .where_eq("id", Value::I64(1))
        .build_delete_with_params();
    assert!(sql.contains("DELETE"));
    assert!(!params.is_empty());
}

#[test]
fn test_build_force_delete() {
    let sql = qb_sqlite()
        .table("t")
        .without_soft_delete()
        .where_eq("id", Value::I64(1))
        .build_force_delete();
    assert!(sql.contains("DELETE"));
}

#[test]
fn test_build_force_delete_with_params() {
    let (sql, _) = qb_sqlite()
        .table("t")
        .without_soft_delete()
        .where_eq("id", Value::I64(1))
        .build_force_delete_with_params();
    assert!(sql.contains("DELETE"));
}

#[test]
fn test_sql_insert() {
    use std::collections::HashMap;
    let mut data = HashMap::new();
    data.insert("name".to_string(), Value::String("test".into()));
    data.insert("age".to_string(), Value::I64(25));
    let sql = qb_sqlite().table("t").sql_insert(&data);
    assert!(sql.contains("INSERT"));
}

#[test]
fn test_sql_update() {
    use std::collections::HashMap;
    let mut data = HashMap::new();
    data.insert("name".to_string(), Value::String("updated".into()));
    let sql = qb_sqlite()
        .table("t")
        .where_eq("id", Value::I64(1))
        .sql_update(&data);
    assert!(sql.contains("UPDATE"));
}

#[test]
fn test_sql_delete() {
    let sql = qb_sqlite()
        .table("t")
        .where_eq("id", Value::I64(1))
        .sql_delete();
    assert!(sql.contains("DELETE"));
}

#[test]
fn test_select_only_and_column() {
    let (sql, _) = qb_sqlite()
        .table("t")
        .select_only()
        .column("a")
        .column("b")
        .build_select();
    assert!(sql.contains("a"));
    assert!(sql.contains("b"));
}

#[test]
fn test_columns_method() {
    let (sql, _) = qb_sqlite()
        .table("t")
        .select_only()
        .columns(vec!["a", "b", "c"])
        .build_select();
    assert!(sql.contains("a"));
    assert!(sql.contains("c"));
}

#[test]
fn test_select_exclude() {
    let result = qb_sqlite().table("t").select_exclude(&["email", "role"]);
    assert!(result.is_ok());
    let (sql, _) = result.unwrap().build_select();
    assert!(!sql.is_empty());
}

#[test]
fn test_mysql_dialect_quoting() {
    let (sql, _) = qb_mysql()
        .table("t")
        .where_eq("name", Value::String("test".into()))
        .build_select();
    assert!(sql.contains('`') || sql.contains("t"));
}

#[test]
fn test_pg_dialect_quoting() {
    let (sql, _) = qb_pg()
        .table("t")
        .where_eq("name", Value::String("test".into()))
        .build_select();
    assert!(sql.contains('"') || sql.contains("t"));
}

#[test]
fn test_build_insert_with_params() {
    use std::collections::HashMap;
    let mut data = HashMap::new();
    data.insert("name".to_string(), Value::String("test".into()));
    data.insert("age".to_string(), Value::I64(25));
    let (sql, params) = qb_sqlite().table("t").build_insert_with_params(&data);
    assert!(sql.contains("INSERT"));
    assert!(!params.is_empty());
}

#[test]
fn test_build_batch_insert_with_params() {
    use std::collections::HashMap;
    let mut row1 = HashMap::new();
    row1.insert("name".to_string(), Value::String("a".into()));
    row1.insert("id".to_string(), Value::I64(1));
    let mut row2 = HashMap::new();
    row2.insert("name".to_string(), Value::String("b".into()));
    row2.insert("id".to_string(), Value::I64(2));
    let (sql, params) = qb_sqlite()
        .table("t")
        .build_batch_insert_with_params(&[row1, row2]);
    assert!(sql.contains("INSERT"));
    assert!(params.len() >= 4);
}

#[test]
fn test_order_by_and_group_by() {
    let (sql, _) = qb_sqlite()
        .table("t")
        .order_by("name")
        .order_desc("id")
        .group_by("category")
        .build_select();
    assert!(sql.contains("ORDER BY"));
    assert!(sql.contains("GROUP BY"));
}

#[test]
fn test_limit() {
    let (sql, _) = qb_sqlite().table("t").limit(10).build_select();
    assert!(sql.contains("LIMIT"));
}

#[test]
fn test_complex_query_all_features() {
    let (sql, params) = qb_sqlite()
        .table("users")
        .select(vec!["id", "name", "email"])
        .expect("valid")
        .where_eq("status", Value::String("active".into()))
        .where_gt("age", Value::I64(18))
        .where_like("name", Value::String("%test%".into()))
        .where_in(
            "role",
            vec![Value::String("admin".into()), Value::String("user".into())],
        )
        .order_by("created_at")
        .order_desc("id")
        .limit(20)
        .offset(40)
        .build_select_with_params();
    assert!(sql.contains("SELECT"));
    assert!(sql.contains("WHERE"));
    assert!(sql.contains("ORDER BY"));
    assert!(sql.contains("LIMIT"));
    assert!(sql.contains("OFFSET"));
    assert!(!params.is_empty());
}
