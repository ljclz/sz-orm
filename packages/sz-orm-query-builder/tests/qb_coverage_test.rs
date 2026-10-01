//! v9.2.0 M16: query-builder 未覆盖方法补齐

use sz_orm_core::{DbType, Value};
use sz_orm_query_builder::{DeleteQuery, InsertQuery, SelectQuery, UpdateQuery};

#[test]
fn test_all_columns() {
    let sql = SelectQuery::new()
        .all_columns()
        .from("users")
        .build(DbType::MySQL);
    assert!(sql.contains("SELECT *"));
}

#[test]
fn test_into_parts_select() {
    let result = SelectQuery::new()
        .column("id")
        .from("users")
        .where_eq("status", Value::String("active".into()))
        .build_with_params(DbType::MySQL);
    let (sql, params) = result.into_parts();
    assert!(sql.contains("SELECT"));
    assert!(params.contains(&Value::String("active".into())));
}

#[test]
fn test_into_parts_update() {
    let result = UpdateQuery::new()
        .table("users")
        .set("name", "'Bob'")
        .where_eq("id", Value::I64(1))
        .build_with_params(DbType::MySQL);
    let (sql, params) = result.into_parts();
    assert!(sql.to_uppercase().contains("UPDATE"));
    assert!(params.contains(&Value::I64(1)));
}

#[test]
fn test_into_parts_delete() {
    let result = DeleteQuery::new()
        .from_table("users")
        .where_eq("id", Value::I64(1))
        .build_with_params(DbType::MySQL);
    let (sql, params) = result.into_parts();
    assert!(sql.to_uppercase().contains("DELETE FROM"));
    assert!(params.contains(&Value::I64(1)));
}

#[test]
fn test_paginate_page_1() {
    let sql = SelectQuery::new()
        .column("id")
        .from("users")
        .paginate(1, 20)
        .build(DbType::MySQL);
    assert!(sql.to_uppercase().contains("LIMIT"));
}

#[test]
fn test_paginate_page_3() {
    let sql = SelectQuery::new()
        .column("id")
        .from("users")
        .paginate(3, 20)
        .build(DbType::MySQL);
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("LIMIT"));
    assert!(clean.to_uppercase().contains("OFFSET"));
}

#[test]
fn test_paginate_page_0_safe() {
    let sql = SelectQuery::new()
        .column("id")
        .from("users")
        .paginate(0, 10)
        .build(DbType::MySQL);
    assert!(sql.to_uppercase().contains("LIMIT"));
}

#[test]
fn test_paginate_pg() {
    let sql = SelectQuery::new()
        .column("id")
        .from("users")
        .paginate(2, 15)
        .build(DbType::PostgreSQL);
    assert!(sql.to_uppercase().contains("LIMIT"));
    assert!(sql.to_uppercase().contains("OFFSET"));
}

#[test]
fn test_cte_with() {
    let sql = SelectQuery::new()
        .column("id")
        .from("active_users")
        .with_cte(
            "active_users",
            "SELECT id FROM users WHERE status = 'active'",
        )
        .build(DbType::MySQL);
    assert!(sql.to_uppercase().contains("WITH"));
    assert!(sql.contains("active_users"));
}

#[test]
fn test_recursive_cte() {
    let sql = SelectQuery::new()
        .column("id")
        .from("descendants")
        .with_recursive_cte("descendants", "SELECT id FROM users WHERE parent_id = 1")
        .build(DbType::MySQL);
    assert!(sql.to_uppercase().contains("WITH RECURSIVE"));
}

#[test]
fn test_window_function() {
    let sql = SelectQuery::new()
        .column("id")
        .from("users")
        .window_function("ROW_NUMBER() OVER (PARTITION BY dept ORDER BY salary)")
        .build(DbType::MySQL);
    assert!(sql.to_uppercase().contains("ROW_NUMBER"));
}

#[test]
fn test_row_number() {
    let sql = SelectQuery::new()
        .column("id")
        .from("users")
        .row_number("dept", "salary DESC", "rn")
        .build(DbType::MySQL);
    assert!(sql.to_uppercase().contains("ROW_NUMBER"));
}

#[test]
fn test_rank() {
    let sql = SelectQuery::new()
        .column("id")
        .from("users")
        .rank("dept", "salary DESC", "rnk")
        .build(DbType::MySQL);
    assert!(sql.to_uppercase().contains("RANK()"));
}

#[test]
fn test_dense_rank() {
    let sql = SelectQuery::new()
        .column("id")
        .from("users")
        .dense_rank("dept", "salary DESC", "drnk")
        .build(DbType::MySQL);
    assert!(sql.to_uppercase().contains("DENSE_RANK()"));
}

#[test]
fn test_inner_join_param() {
    let result = SelectQuery::new()
        .column("u.id")
        .from("users u")
        .inner_join_param("orders o", "u.id", "o.user_id", Value::I64(42))
        .build_with_params(DbType::MySQL);
    let (sql, params) = result.into_parts();
    assert!(sql.to_uppercase().contains("INNER JOIN"));
    assert!(params.contains(&Value::I64(42)));
}

#[test]
fn test_left_join_param() {
    let result = SelectQuery::new()
        .column("u.id")
        .from("users u")
        .left_join_param("profiles p", "u.id", "p.user_id", Value::I64(1))
        .build_with_params(DbType::MySQL);
    let (sql, params) = result.into_parts();
    assert!(sql.to_uppercase().contains("LEFT JOIN"));
    assert!(params.contains(&Value::I64(1)));
}

#[test]
fn test_right_join_param() {
    let result = SelectQuery::new()
        .column("u.id")
        .from("users u")
        .right_join_param("orders o", "u.id", "o.user_id", Value::I64(5))
        .build_with_params(DbType::MySQL);
    let (sql, params) = result.into_parts();
    assert!(sql.to_uppercase().contains("RIGHT JOIN"));
    assert!(params.contains(&Value::I64(5)));
}

#[test]
fn test_inner_join_on() {
    let sql = SelectQuery::new()
        .column("u.id")
        .from("users u")
        .inner_join_on("orders o", "u.id", "o.user_id")
        .build(DbType::MySQL);
    assert!(sql.to_uppercase().contains("INNER JOIN"));
}

#[test]
fn test_left_join_on() {
    let sql = SelectQuery::new()
        .column("u.id")
        .from("users u")
        .left_join_on("profiles p", "u.id", "p.user_id")
        .build(DbType::MySQL);
    assert!(sql.to_uppercase().contains("LEFT JOIN"));
}

#[test]
fn test_right_join_on() {
    let sql = SelectQuery::new()
        .column("u.id")
        .from("users u")
        .right_join_on("orders o", "u.id", "o.user_id")
        .build(DbType::MySQL);
    assert!(sql.to_uppercase().contains("RIGHT JOIN"));
}

#[test]
fn test_or_where_in() {
    let result = SelectQuery::new()
        .column("id")
        .from("users")
        .where_eq("status", Value::String("active".into()))
        .or_where_in(
            "dept",
            vec![Value::String("eng".into()), Value::String("sales".into())],
        )
        .build_with_params(DbType::MySQL);
    let (sql, _) = result.into_parts();
    assert!(sql.to_uppercase().contains("OR"));
    assert!(sql.to_uppercase().contains("IN"));
}

#[test]
fn test_or_where_between() {
    let result = SelectQuery::new()
        .column("id")
        .from("users")
        .where_eq("status", Value::String("active".into()))
        .or_where_between("age", Value::I64(18), Value::I64(65))
        .build_with_params(DbType::MySQL);
    let (sql, _) = result.into_parts();
    assert!(sql.to_uppercase().contains("OR"));
    assert!(sql.to_uppercase().contains("BETWEEN"));
}

#[test]
fn test_or_where_null() {
    let result = SelectQuery::new()
        .column("id")
        .from("users")
        .where_eq("status", Value::String("active".into()))
        .or_where_null("deleted_at")
        .build_with_params(DbType::MySQL);
    let (sql, _) = result.into_parts();
    assert!(sql.to_uppercase().contains("OR"));
    assert!(sql.to_uppercase().contains("IS NULL"));
}

#[test]
fn test_or_where_not_null() {
    let result = SelectQuery::new()
        .column("id")
        .from("users")
        .where_eq("status", Value::String("active".into()))
        .or_where_not_null("email")
        .build_with_params(DbType::MySQL);
    let (sql, _) = result.into_parts();
    assert!(sql.to_uppercase().contains("OR"));
    assert!(sql.to_uppercase().contains("IS NOT NULL"));
}

#[test]
fn test_having_clause() {
    let sql = SelectQuery::new()
        .column("dept")
        .from("users")
        .group_by("dept")
        .having("COUNT(*) > 5")
        .build(DbType::MySQL);
    assert!(sql.to_uppercase().contains("HAVING"));
}

#[test]
fn test_complex_select_all_features() {
    let result = SelectQuery::new()
        .distinct()
        .column("u.id")
        .column("u.name")
        .from("users u")
        .inner_join("orders o", "u.id = o.user_id")
        .where_eq("u.status", Value::String("active".into()))
        .where_gt("o.amount", Value::I64(100))
        .group_by("u.id")
        .having("COUNT(o.id) > 0")
        .order_by("u.name", true)
        .limit(10)
        .offset(20)
        .build_with_params(DbType::MySQL);
    let (sql, _) = result.into_parts();
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("SELECT DISTINCT"));
    assert!(clean.to_uppercase().contains("INNER JOIN"));
    assert!(clean.to_uppercase().contains("WHERE"));
    assert!(clean.to_uppercase().contains("GROUP BY"));
    assert!(clean.to_uppercase().contains("HAVING"));
    assert!(clean.to_uppercase().contains("ORDER BY"));
    assert!(clean.to_uppercase().contains("LIMIT"));
    assert!(clean.to_uppercase().contains("OFFSET"));
}

#[test]
fn test_union_query() {
    let q1 = SelectQuery::new().column("id").from("users");
    let q2 = SelectQuery::new().column("id").from("admins");
    let sql = q1.union(q2).build(DbType::MySQL);
    assert!(sql.to_uppercase().contains("UNION"));
}

#[test]
fn test_columns_vec() {
    let sql = SelectQuery::new()
        .columns(&["id", "name", "email"])
        .from("users")
        .build(DbType::MySQL);
    let clean = sql.replace('`', "");
    assert!(clean.contains("id"));
    assert!(clean.contains("name"));
    assert!(clean.contains("email"));
}

#[test]
fn test_where_clause_raw() {
    let sql = SelectQuery::new()
        .column("id")
        .from("users")
        .where_clause("age > 18 AND status = 'active'")
        .build(DbType::MySQL);
    assert!(sql.contains("age > 18"));
}

#[test]
fn test_multiple_group_by() {
    let sql = SelectQuery::new()
        .column("dept")
        .column("role")
        .from("users")
        .group_by("dept")
        .group_by("role")
        .build(DbType::MySQL);
    let clean = sql.replace('`', "");
    assert!(clean.to_uppercase().contains("GROUP BY"));
}

#[test]
fn test_insert_build() {
    let sql = InsertQuery::new()
        .into_table("users")
        .value("name", "'Alice'")
        .value("age", "30")
        .build();
    assert!(sql.to_uppercase().contains("INSERT INTO"));
    assert!(sql.contains("Alice"));
}

#[test]
fn test_insert_values_batch() {
    let sql = InsertQuery::new()
        .into_table("users")
        .values(&[("name", "'Bob'"), ("age", "25")])
        .build();
    assert!(sql.to_uppercase().contains("INSERT INTO"));
    assert!(sql.contains("Bob"));
}

#[test]
fn test_update_build() {
    let sql = UpdateQuery::new()
        .table("users")
        .set("name", "'Charlie'")
        .where_clause("id = 1")
        .build();
    assert!(sql.to_uppercase().contains("UPDATE"));
    assert!(sql.contains("Charlie"));
}

#[test]
fn test_delete_build() {
    let sql = DeleteQuery::new()
        .from_table("users")
        .where_clause("id = 1")
        .build();
    assert!(sql.to_uppercase().contains("DELETE FROM"));
}
