//! v9.2.0 M16: query.rs 未覆盖方法补齐

use std::collections::HashMap;
use sz_orm_core::{
    dialect::get_dialect, partial_model::Expr, AggExpr, DbType, HavingOp, Model, ModelExt,
    QueryBuilder, Value,
};

#[derive(Clone, Debug)]
#[allow(dead_code)] // 字段仅作为 ModelExt schema 元数据，测试中不逐一读取
struct User {
    id: i64,
    name: String,
    email: String,
    age: i64,
}

impl Model for User {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "users"
    }
    fn pk(&self) -> i64 {
        self.id
    }
    fn set_pk(&mut self, pk: i64) {
        self.id = pk;
    }
    fn tenant_field() -> Option<&'static str> {
        Some("tenant_id")
    }
}

impl ModelExt for User {
    fn columns() -> Vec<&'static str> {
        vec!["id", "name", "email", "age"]
    }
    fn fillable() -> Vec<&'static str> {
        vec!["name", "email", "age"]
    }
    fn guarded() -> Vec<&'static str> {
        vec!["id"]
    }
    fn hidden() -> Vec<&'static str> {
        vec![]
    }
    fn relations() -> std::collections::HashMap<&'static str, sz_orm_core::Relation> {
        Default::default()
    }
    fn fill(&mut self, _: HashMap<String, Value>) {}
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({})
    }
}

fn builder() -> QueryBuilder<User> {
    QueryBuilder::<User>::new(get_dialect(DbType::MySQL).unwrap())
}

fn builder_pg() -> QueryBuilder<User> {
    QueryBuilder::<User>::new(get_dialect(DbType::PostgreSQL).unwrap())
}

fn mk_data(pairs: &[(&str, Value)]) -> HashMap<String, Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

#[test]
fn test_build_max() {
    let sql = builder().build_max("age");
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("MAX("));
    assert!(clean.contains("max_val"));
}

#[test]
fn test_build_min() {
    let sql = builder().build_min("age");
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("MIN("));
    assert!(clean.contains("min_val"));
}

#[test]
fn test_build_sum() {
    let sql = builder().build_sum("age");
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("SUM("));
    assert!(clean.contains("sum_val"));
}

#[test]
fn test_build_avg() {
    let sql = builder().build_avg("age");
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("AVG("));
    assert!(clean.contains("avg_val"));
}

#[test]
fn test_build_max_with_where() {
    let sql = builder()
        .where_eq("status", Value::String("active".into()))
        .build_max("age");
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("WHERE"));
    assert!(clean.to_uppercase().contains("MAX("));
}

#[test]
fn test_build_min_pg() {
    let sql = builder_pg().build_min("age");
    assert!(sql.contains("MIN("));
}

#[test]
fn test_build_sum_pg() {
    let sql = builder_pg().build_sum("age");
    assert!(sql.contains("SUM("));
}

#[test]
fn test_build_avg_pg() {
    let sql = builder_pg().build_avg("age");
    assert!(sql.contains("AVG("));
}

#[test]
fn test_validate_select_ok() {
    let result = builder().where_eq("id", Value::I64(1)).validate();
    assert!(result.is_ok());
}

#[test]
fn test_validate_select_with_join_ok() {
    let result = builder()
        .join_inner("orders", "users.id", "orders.user_id")
        .validate();
    assert!(result.is_ok());
}

#[test]
fn test_validate_select_with_left_join_ok() {
    let result = builder()
        .join_left("orders", "users.id", "orders.user_id")
        .validate();
    assert!(result.is_ok());
}

#[test]
fn test_validate_select_with_right_join_ok() {
    let result = builder()
        .join_right("orders", "users.id", "orders.user_id")
        .validate();
    assert!(result.is_ok());
}

#[test]
fn test_validate_insert_ok() {
    let data = mk_data(&[
        ("name", Value::String("Alice".into())),
        ("age", Value::I64(30)),
    ]);
    let result = builder().validate_insert(&data);
    assert!(result.is_ok());
}

#[test]
fn test_validate_insert_empty_data() {
    let data = HashMap::new();
    let result = builder().validate_insert(&data);
    assert!(result.is_err());
}

#[test]
fn test_validate_update_ok() {
    let data = mk_data(&[("name", Value::String("Bob".into()))]);
    let result = builder()
        .where_eq("id", Value::I64(1))
        .validate_update(&data);
    assert!(result.is_ok());
}

#[test]
fn test_validate_update_empty_data() {
    let data = HashMap::new();
    let result = builder()
        .where_eq("id", Value::I64(1))
        .validate_update(&data);
    assert!(result.is_err());
}

#[test]
fn test_validate_delete_ok() {
    let result = builder().where_eq("id", Value::I64(1)).validate_delete();
    assert!(result.is_ok());
}

#[test]
fn test_validate_delete_no_where() {
    let result = builder().validate_delete();
    assert!(result.is_ok());
}

#[test]
fn test_select_quoted_ok() {
    let result = builder().select_quoted(vec!["id", "name"]);
    assert!(result.is_ok());
    let (sql, _) = result.unwrap().build_select();
    let clean = sql.replace('`', "");
    assert!(clean.contains("id"));
    assert!(clean.contains("name"));
}

#[test]
fn test_select_expr() {
    let sql = builder().select_expr(vec!["COUNT(*)", "SUM(age)"]).sql();
    assert!(sql.to_uppercase().contains("COUNT(*)"));
    assert!(sql.to_uppercase().contains("SUM(AGE)"));
}

#[test]
fn test_sql_insert() {
    let data = mk_data(&[
        ("name", Value::String("Alice".into())),
        ("age", Value::I64(30)),
    ]);
    let sql = builder().sql_insert(&data);
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("INSERT INTO"));
    assert!(clean.contains("Alice"));
}

#[test]
fn test_sql_insert_empty() {
    let data = HashMap::new();
    let sql = builder().sql_insert(&data);
    assert!(sql.is_empty());
}

#[test]
fn test_sql_insert_pg() {
    let data = mk_data(&[("name", Value::String("Bob".into()))]);
    let sql = builder_pg().sql_insert(&data);
    assert!(sql.to_uppercase().contains("INSERT INTO"));
    assert!(sql.contains("Bob"));
}

#[test]
fn test_sql_update() {
    let data = mk_data(&[("name", Value::String("Charlie".into()))]);
    let sql = builder().where_eq("id", Value::I64(1)).sql_update(&data);
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("UPDATE"));
    assert!(clean.contains("Charlie"));
}

#[test]
fn test_sql_update_empty() {
    let data = HashMap::new();
    let sql = builder().sql_update(&data);
    assert!(sql.is_empty());
}

#[test]
fn test_sql_update_pg() {
    let data = mk_data(&[("name", Value::String("Dave".into()))]);
    let sql = builder_pg().where_eq("id", Value::I64(1)).sql_update(&data);
    assert!(sql.to_uppercase().contains("UPDATE"));
}

#[test]
fn test_sql_delete() {
    let sql = builder().where_eq("id", Value::I64(1)).sql_delete();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("DELETE FROM"));
}

#[test]
fn test_sql_delete_no_where() {
    let sql = builder().sql_delete();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("DELETE FROM"));
}

#[test]
fn test_sql_delete_pg() {
    let sql = builder_pg().where_eq("id", Value::I64(1)).sql_delete();
    assert!(sql.to_uppercase().contains("DELETE FROM"));
}

#[test]
fn test_select_exclude_ok() {
    let result = builder().select_exclude(&["email", "age"]);
    assert!(result.is_ok());
    let (sql, _) = result.unwrap().build_select();
    let clean = sql.replace('`', "");
    assert!(clean.contains("id"));
    assert!(clean.contains("name"));
    assert!(!clean.contains("email"));
}

#[test]
fn test_select_exclude_nonexistent_field() {
    let result = builder().select_exclude(&["nonexistent"]);
    assert!(result.is_err());
}

#[test]
fn test_select_exclude_all_fields() {
    let result = builder().select_exclude(&["id", "name", "email", "age"]);
    assert!(result.is_err());
}

#[test]
fn test_column_as_count() {
    let (sql, _) = builder()
        .select_only()
        .column_as(Expr::count("id"), "total")
        .build_select();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("COUNT("));
    assert!(clean.contains("total"));
}

#[test]
fn test_column_as_sum() {
    let (sql, _) = builder()
        .select_only()
        .column_as(Expr::sum("age"), "total_age")
        .build_select();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("SUM("));
    assert!(clean.contains("total_age"));
}

#[test]
fn test_column_as_avg() {
    let (sql, _) = builder()
        .select_only()
        .column_as(Expr::avg("age"), "avg_age")
        .build_select();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("AVG("));
}

#[test]
fn test_column_as_max() {
    let (sql, _) = builder()
        .select_only()
        .column_as(Expr::max("age"), "max_age")
        .build_select();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("MAX("));
}

#[test]
fn test_column_as_min() {
    let (sql, _) = builder()
        .select_only()
        .column_as(Expr::min("age"), "min_age")
        .build_select();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("MIN("));
}

#[test]
fn test_with_tenant_id() {
    let b = builder().with_tenant_id(42);
    let (_, params) = b.build_select();
    assert!(params.contains(&Value::I64(42)));
}

#[test]
fn test_without_tenant() {
    let b = builder().with_tenant_id(42).without_tenant();
    assert!(b.is_tenant_disabled());
}

#[test]
fn test_is_tenant_disabled_default() {
    let b = builder();
    assert!(!b.is_tenant_disabled());
}

#[test]
fn test_cache_ttl() {
    let b = builder().cache_ttl(std::time::Duration::from_secs(300));
    assert_eq!(b.get_cache_ttl(), Some(std::time::Duration::from_secs(300)));
}

#[test]
fn test_cache_ttl_default_none() {
    let b = builder();
    assert_eq!(b.get_cache_ttl(), None);
}

#[test]
fn test_without_soft_delete() {
    let b = builder().without_soft_delete();
    assert!(b.is_soft_delete_disabled());
}

#[test]
fn test_is_soft_delete_disabled_default() {
    let b = builder();
    assert!(!b.is_soft_delete_disabled());
}

#[test]
fn test_get_lock_type_default() {
    let b = builder();
    assert_eq!(b.get_lock_type(), None);
}

#[test]
fn test_lock_for_update_type() {
    let b = builder().lock_for_update().unwrap();
    assert!(b.get_lock_type().is_some());
}

#[test]
fn test_lock_shared_type() {
    let b = builder().lock_shared().unwrap();
    assert!(b.get_lock_type().is_some());
}

#[test]
fn test_is_insert_or_ignore_default() {
    let b = builder();
    assert!(!b.is_insert_or_ignore());
}

#[test]
fn test_insert_or_ignore_flag() {
    let b = builder().insert_or_ignore();
    assert!(b.is_insert_or_ignore());
}

#[test]
fn test_clone_for_count_basic() {
    let original = builder()
        .where_eq("status", Value::String("active".into()))
        .order_by("id")
        .limit(10)
        .offset(5);
    let cloned = original.clone_for_count();
    let (sql, params) = cloned.build_select();
    let clean = sql.replace('`', "");
    assert!(clean.contains("*"));
    assert!(params.contains(&Value::String("active".into())));
    assert!(!clean.to_uppercase().contains("LIMIT"));
    assert!(!clean.to_uppercase().contains("OFFSET"));
}

#[test]
fn test_clone_for_count_preserves_where() {
    let original = builder()
        .where_eq("id", Value::I64(1))
        .where_eq("status", Value::String("active".into()));
    let cloned = original.clone_for_count();
    let (_, params) = cloned.build_select();
    assert!(params.contains(&Value::I64(1)));
    assert!(params.contains(&Value::String("active".into())));
}

#[test]
fn test_clone_for_count_preserves_group_by() {
    let original = builder()
        .where_eq("status", Value::String("active".into()))
        .group_by("department");
    let cloned = original.clone_for_count();
    let (sql, _) = cloned.build_select();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("GROUP BY"));
}

#[test]
fn test_clone_for_count_preserves_joins() {
    let original = builder()
        .join_inner("orders", "users.id", "orders.user_id")
        .where_eq("status", Value::String("active".into()));
    let cloned = original.clone_for_count();
    let (sql, _) = cloned.build_select();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("INNER JOIN"));
}

#[test]
fn test_build_force_delete_with_where() {
    let sql = builder().where_eq("id", Value::I64(1)).build_force_delete();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("DELETE FROM"));
    assert!(clean.to_uppercase().contains("WHERE"));
}

#[test]
fn test_build_force_delete_no_where() {
    let sql = builder().build_force_delete();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("DELETE FROM"));
}

#[test]
fn test_build_force_delete_pg() {
    let sql = builder_pg()
        .where_eq("id", Value::I64(1))
        .build_force_delete();
    assert!(sql.to_uppercase().contains("DELETE FROM"));
}

#[test]
fn test_table_override() {
    let b = builder().table("custom_table");
    let (sql, _) = b.build_select();
    let clean = sql.replace('`', "");
    assert!(clean.contains("custom_table"));
}

#[test]
fn test_page_method() {
    let (sql, _) = builder().page(3, 20).build_select();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("LIMIT"));
    assert!(clean.to_uppercase().contains("OFFSET"));
}

#[test]
fn test_page_first_page() {
    let (sql, _) = builder().page(1, 10).build_select();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("LIMIT"));
}

#[test]
fn test_multiple_order_by() {
    let (sql, _) = builder().order_by("name").order_desc("age").build_select();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("ORDER BY"));
    assert!(clean.to_uppercase().contains("DESC"));
    assert!(clean.to_uppercase().contains("ASC"));
}

#[test]
fn test_group_by_multiple() {
    let (sql, _) = builder()
        .group_by("department")
        .group_by("role")
        .build_select();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("GROUP BY"));
}

#[test]
fn test_limit_and_offset() {
    let (sql, _) = builder().limit(100).offset(50).build_select();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("LIMIT"));
    assert!(clean.to_uppercase().contains("OFFSET"));
}

#[test]
fn test_sql_with_cross_join() {
    let (sql, _) = builder()
        .join_inner("orders", "users.id", "orders.user_id")
        .join_left("profiles", "users.id", "profiles.user_id")
        .build_select();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("INNER JOIN"));
    assert!(clean.to_uppercase().contains("LEFT JOIN"));
}

#[test]
fn test_sql_with_right_join() {
    let (sql, _) = builder()
        .join_right("orders", "users.id", "orders.user_id")
        .build_select();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("RIGHT JOIN"));
}

#[test]
fn test_build_count_with_where() {
    let sql = builder()
        .where_eq("status", Value::String("active".into()))
        .build_count();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("COUNT"));
    assert!(clean.to_uppercase().contains("WHERE"));
}

#[test]
fn test_build_exists_with_where() {
    let sql = builder().where_eq("id", Value::I64(1)).build_exists();
    assert!(sql.to_uppercase().contains("EXISTS"));
}

#[test]
fn test_build_exists_no_where() {
    let sql = builder().build_exists();
    assert!(sql.to_uppercase().contains("EXISTS"));
}

#[test]
fn test_select_only_then_columns() {
    let (sql, _) = builder()
        .select_only()
        .column("id")
        .column("name")
        .build_select();
    let clean = sql.replace('`', "");
    assert!(clean.contains("id"));
    assert!(clean.contains("name"));
    assert!(!clean.contains("email"));
}

#[test]
fn test_select_only_then_columns_vec() {
    let (sql, _) = builder()
        .select_only()
        .columns(vec!["id", "name", "age"])
        .build_select();
    let clean = sql.replace('`', "");
    assert!(clean.contains("id"));
    assert!(clean.contains("name"));
    assert!(clean.contains("age"));
}

#[test]
fn test_where_in_empty() {
    let (sql, _) = builder().where_in("id", vec![]).build_select();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("WHERE"));
}

#[test]
fn test_where_not_in_empty() {
    let (sql, _) = builder().where_not_in("id", vec![]).build_select();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("WHERE"));
}

#[test]
fn test_where_null_with_other_conditions() {
    let (sql, _) = builder()
        .where_eq("status", Value::String("active".into()))
        .where_null("deleted_at")
        .build_select();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("IS NULL"));
}

#[test]
fn test_where_not_null_with_other_conditions() {
    let (sql, _) = builder()
        .where_eq("status", Value::String("active".into()))
        .where_not_null("email")
        .build_select();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("IS NOT NULL"));
}

#[test]
fn test_or_where_like() {
    let (sql, _) = builder()
        .where_eq("status", Value::String("active".into()))
        .or_where_like("name", Value::String("%admin%".into()))
        .build_select();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("LIKE"));
    assert!(clean.to_uppercase().contains("OR"));
}

#[test]
fn test_where_between_with_other() {
    let (sql, _) = builder()
        .where_eq("status", Value::String("active".into()))
        .where_between("age", Value::I64(18), Value::I64(65))
        .build_select();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("BETWEEN"));
}

#[test]
fn test_where_not_between() {
    let (sql, _) = builder()
        .where_not_between("age", Value::I64(0), Value::I64(17))
        .build_select();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("NOT BETWEEN"));
}

#[test]
fn test_complex_query_all_clauses() {
    let (sql, _) = builder()
        .select(vec!["id", "name", "age"])
        .unwrap()
        .where_eq("status", Value::String("active".into()))
        .where_gt("age", Value::I64(18))
        .where_like("name", Value::String("%test%".into()))
        .order_by("name")
        .order_desc("age")
        .group_by("department")
        .limit(10)
        .offset(20)
        .build_select();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("SELECT"));
    assert!(clean.to_uppercase().contains("WHERE"));
    assert!(clean.to_uppercase().contains("ORDER BY"));
    assert!(clean.to_uppercase().contains("GROUP BY"));
    assert!(clean.to_uppercase().contains("LIMIT"));
    assert!(clean.to_uppercase().contains("OFFSET"));
}

#[test]
fn test_sql_insert_with_special_chars() {
    let data = mk_data(&[("name", Value::String("O'Brien".into()))]);
    let sql = builder().sql_insert(&data);
    assert!(sql.contains("O'Brien") || sql.contains("O\\'Brien"));
}

#[test]
fn test_sql_update_multiple_fields() {
    let data = mk_data(&[
        ("name", Value::String("Updated".into())),
        ("age", Value::I64(25)),
        ("email", Value::String("new@test.com".into())),
    ]);
    let sql = builder().where_eq("id", Value::I64(1)).sql_update(&data);
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("UPDATE"));
    assert!(clean.contains("Updated"));
}

#[test]
fn test_build_select_with_params_complex() {
    let (_sql, params) = builder()
        .where_eq("status", Value::String("active".into()))
        .where_gt("age", Value::I64(18))
        .where_like("name", Value::String("%test%".into()))
        .where_in(
            "department",
            vec![Value::String("eng".into()), Value::String("sales".into())],
        )
        .build_select_with_params();
    assert!(params.len() >= 4);
}

#[test]
fn test_build_insert_with_params_multi() {
    let data = mk_data(&[
        ("name", Value::String("Alice".into())),
        ("age", Value::I64(30)),
        ("email", Value::String("alice@test.com".into())),
    ]);
    let (sql, params) = builder().build_insert_with_params(&data);
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("INSERT INTO"));
    assert!(params.len() >= 3);
}

#[test]
fn test_build_update_with_params_multi() {
    let data = mk_data(&[
        ("name", Value::String("Bob".into())),
        ("age", Value::I64(25)),
    ]);
    let (sql, params) = builder()
        .where_eq("id", Value::I64(1))
        .build_update_with_params(&data);
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("UPDATE"));
    assert!(params.len() >= 3);
}

#[test]
fn test_build_delete_with_params_complex() {
    let (sql, params) = builder()
        .where_eq("status", Value::String("inactive".into()))
        .where_lt("last_login", Value::I64(0))
        .build_delete_with_params();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("DELETE FROM"));
    assert!(params.len() >= 2);
}

#[test]
fn test_build_force_delete_with_params() {
    let (sql, params) = builder()
        .where_eq("id", Value::I64(1))
        .build_force_delete_with_params();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("DELETE FROM"));
    assert!(params.contains(&Value::I64(1)));
}

#[test]
fn test_build_batch_insert_with_params() {
    let rows = vec![
        mk_data(&[
            ("name", Value::String("Alice".into())),
            ("age", Value::I64(30)),
        ]),
        mk_data(&[
            ("name", Value::String("Bob".into())),
            ("age", Value::I64(25)),
        ]),
    ];
    let (sql, params) = builder().build_batch_insert_with_params(&rows);
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("INSERT INTO"));
    assert!(params.len() >= 4);
}

#[test]
fn test_build_batch_upsert_with_params_mysql() {
    let rows = vec![
        mk_data(&[
            ("id", Value::I64(1)),
            ("name", Value::String("Alice".into())),
        ]),
        mk_data(&[("id", Value::I64(2)), ("name", Value::String("Bob".into()))]),
    ];
    let result = builder().build_batch_upsert_with_params(&rows, &["id"], &["name"]);
    assert!(result.is_ok());
    let (sql, params) = result.unwrap();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("INSERT INTO"));
    assert!(params.len() >= 4);
}

#[test]
fn test_build_batch_upsert_with_params_pg() {
    let rows = vec![
        mk_data(&[
            ("id", Value::I64(1)),
            ("name", Value::String("Alice".into())),
        ]),
        mk_data(&[("id", Value::I64(2)), ("name", Value::String("Bob".into()))]),
    ];
    let result = builder_pg().build_batch_upsert_with_params(&rows, &["id"], &["name"]);
    assert!(result.is_ok());
    let (sql, _) = result.unwrap();
    assert!(sql.to_uppercase().contains("INSERT INTO"));
}

#[test]
fn test_validate_select_complex() {
    let result = builder()
        .select(vec!["id", "name"])
        .unwrap()
        .where_eq("status", Value::String("active".into()))
        .where_gt("age", Value::I64(18))
        .order_by("name")
        .limit(10)
        .validate();
    assert!(result.is_ok());
}

#[test]
fn test_validate_select_pg() {
    let result = builder_pg().where_eq("id", Value::I64(1)).validate();
    assert!(result.is_ok());
}

#[test]
fn test_clone_for_count_with_having() {
    let original = builder()
        .where_eq("status", Value::String("active".into()))
        .group_by("department")
        .having(AggExpr::CountStar, HavingOp::Gt, Value::I64(5))
        .unwrap();
    let cloned = original.clone_for_count();
    let (sql, _) = cloned.build_select();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("GROUP BY"));
}

#[test]
fn test_having_count_star() {
    let (sql, _) = builder()
        .group_by("department")
        .having(AggExpr::CountStar, HavingOp::Gt, Value::I64(5))
        .unwrap()
        .build_select();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("HAVING"));
    assert!(clean.to_uppercase().contains("COUNT(*)"));
}

#[test]
fn test_having_sum() {
    let (sql, _) = builder()
        .group_by("department")
        .having(
            AggExpr::Sum("age".to_string()),
            HavingOp::Ge,
            Value::I64(100),
        )
        .unwrap()
        .build_select();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("HAVING"));
    assert!(clean.to_uppercase().contains("SUM("));
}

#[test]
fn test_having_avg() {
    let (sql, _) = builder()
        .group_by("department")
        .having(
            AggExpr::Avg("age".to_string()),
            HavingOp::Lt,
            Value::I64(50),
        )
        .unwrap()
        .build_select();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("HAVING"));
    assert!(clean.to_uppercase().contains("AVG("));
}

#[test]
fn test_having_max() {
    let (sql, _) = builder()
        .group_by("department")
        .having(
            AggExpr::Max("age".to_string()),
            HavingOp::Eq,
            Value::I64(65),
        )
        .unwrap()
        .build_select();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("HAVING"));
    assert!(clean.to_uppercase().contains("MAX("));
}

#[test]
fn test_having_min() {
    let (sql, _) = builder()
        .group_by("department")
        .having(AggExpr::Min("age".to_string()), HavingOp::Ne, Value::I64(0))
        .unwrap()
        .build_select();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("HAVING"));
    assert!(clean.to_uppercase().contains("MIN("));
}

#[test]
fn test_having_multiple() {
    let (sql, _) = builder()
        .group_by("department")
        .having(AggExpr::CountStar, HavingOp::Gt, Value::I64(5))
        .unwrap()
        .having(
            AggExpr::Sum("age".to_string()),
            HavingOp::Le,
            Value::I64(1000),
        )
        .unwrap()
        .build_select();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("HAVING"));
    assert!(clean.to_uppercase().contains("AND"));
}
