//! Query 模块契约测试 — 对应 `docs/api-contracts.md` §8
//!
//! 锁定 QueryBuilder 链式 API、SQL 生成、校验契约。

use std::collections::HashMap;
use sz_orm_core::{get_dialect, DbType, QueryBuilder, Value};

// ===== 测试用模型 =====

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

// ===== §8.1 链式 API 返回 Self 契约 =====

#[test]
fn test_query_builder_chain_returns_self_contract() {
    let d = get_dialect(DbType::MySQL).unwrap();
    // 所有 builder 方法返回 Self，支持链式调用
    let _builder = QueryBuilder::<User>::new(d)
        .table("users")
        .select(vec!["id", "name"])
        .where_cond("status = 'active'")
        .or_where("role = 'admin'")
        .order_by("created_at")
        .order_desc("id")
        .group_by("status")
        .having("COUNT(*) > 5")
        .limit(20)
        .offset(40);
    // 编译通过即契约满足
}

#[test]
fn test_query_builder_new_with_dialect_contract() {
    let d = get_dialect(DbType::MySQL).unwrap();
    let _builder = QueryBuilder::<User>::new(d);
}

// ===== §8.1 build_select 生成 SQL 契约 =====

#[test]
fn test_build_select_basic_contract() {
    let d = get_dialect(DbType::MySQL).unwrap();
    let sql = QueryBuilder::<User>::new(d)
        .table("users")
        .select(vec!["id", "name"])
        .build_select();

    assert!(sql.contains("SELECT"));
    assert!(sql.contains("FROM"));
    assert!(sql.contains("users"));
    assert!(sql.contains("id"));
    assert!(sql.contains("name"));
}

#[test]
fn test_build_select_with_where_contract() {
    let d = get_dialect(DbType::MySQL).unwrap();
    let sql = QueryBuilder::<User>::new(d)
        .table("users")
        .select(vec!["*"])
        .where_cond("age > 18")
        .build_select();

    assert!(sql.contains("WHERE"));
    assert!(sql.contains("age > 18"));
}

#[test]
fn test_build_select_with_order_by_contract() {
    let d = get_dialect(DbType::MySQL).unwrap();
    let sql = QueryBuilder::<User>::new(d)
        .table("users")
        .select(vec!["*"])
        .order_by("name")
        .order_desc("id")
        .build_select();

    assert!(sql.contains("ORDER BY"));
    assert!(sql.contains("name"));
    // order_desc 应生成 DESC
    assert!(sql.to_uppercase().contains("DESC"));
}

#[test]
fn test_build_select_with_limit_offset_contract() {
    let d = get_dialect(DbType::MySQL).unwrap();
    let sql = QueryBuilder::<User>::new(d)
        .table("users")
        .select(vec!["*"])
        .limit(10)
        .offset(20)
        .build_select();

    assert!(sql.contains("LIMIT"));
    assert!(sql.contains("10"));
    assert!(sql.contains("OFFSET"));
    assert!(sql.contains("20"));
}

#[test]
fn test_build_select_with_page_contract() {
    let d = get_dialect(DbType::MySQL).unwrap();
    let sql = QueryBuilder::<User>::new(d)
        .table("users")
        .select(vec!["*"])
        .page(3, 20) // 第 3 页，每页 20
        .build_select();

    assert!(sql.contains("LIMIT"));
    assert!(sql.contains("OFFSET"));
}

#[test]
fn test_build_select_with_where_in_contract() {
    let d = get_dialect(DbType::MySQL).unwrap();
    let sql = QueryBuilder::<User>::new(d)
        .table("users")
        .select(vec!["*"])
        .where_in("id", vec![Value::I64(1), Value::I64(2), Value::I64(3)])
        .build_select();

    assert!(sql.contains("IN"));
    assert!(sql.contains("1"));
    assert!(sql.contains("2"));
    assert!(sql.contains("3"));
}

#[test]
fn test_build_select_with_where_between_contract() {
    let d = get_dialect(DbType::MySQL).unwrap();
    let sql = QueryBuilder::<User>::new(d)
        .table("users")
        .select(vec!["*"])
        .where_between("age", Value::I64(18), Value::I64(30))
        .build_select();

    assert!(sql.contains("BETWEEN"));
    assert!(sql.contains("18"));
    assert!(sql.contains("30"));
}

#[test]
fn test_build_select_with_where_null_contract() {
    let d = get_dialect(DbType::MySQL).unwrap();
    let sql = QueryBuilder::<User>::new(d)
        .table("users")
        .select(vec!["*"])
        .where_null("deleted_at")
        .build_select();

    assert!(sql.to_uppercase().contains("IS NULL"));
}

#[test]
fn test_build_select_with_join_contract() {
    let d = get_dialect(DbType::MySQL).unwrap();
    let sql = QueryBuilder::<User>::new(d)
        .table("users")
        .select(vec!["*"])
        .join_inner("posts", "users.id", "posts.user_id")
        .build_select();

    assert!(sql.to_uppercase().contains("INNER JOIN"));
    assert!(sql.contains("posts"));
}

#[test]
fn test_build_select_with_left_join_contract() {
    let d = get_dialect(DbType::MySQL).unwrap();
    let sql = QueryBuilder::<User>::new(d)
        .table("users")
        .select(vec!["*"])
        .join_left("profiles", "users.id", "profiles.user_id")
        .build_select();

    assert!(sql.to_uppercase().contains("LEFT JOIN"));
}

// ===== §8.1 build_insert / build_update / build_delete 契约 =====

#[test]
fn test_build_insert_basic_contract() {
    let d = get_dialect(DbType::MySQL).unwrap();
    let mut data = HashMap::new();
    data.insert("name".to_string(), Value::String("Alice".to_string()));
    data.insert("age".to_string(), Value::I64(25));

    let sql = QueryBuilder::<User>::new(d)
        .table("users")
        .build_insert(&data);

    assert!(sql.to_uppercase().contains("INSERT INTO"));
    assert!(sql.contains("users"));
    assert!(sql.contains("name"));
    assert!(sql.contains("age"));
}

#[test]
fn test_build_update_basic_contract() {
    let d = get_dialect(DbType::MySQL).unwrap();
    let mut data = HashMap::new();
    data.insert("name".to_string(), Value::String("Bob".to_string()));

    let sql = QueryBuilder::<User>::new(d)
        .table("users")
        .where_cond("id = 1")
        .build_update(&data);

    assert!(sql.to_uppercase().contains("UPDATE"));
    assert!(sql.contains("users"));
    assert!(sql.to_uppercase().contains("SET"));
    assert!(sql.contains("name"));
}

#[test]
fn test_build_delete_basic_contract() {
    let d = get_dialect(DbType::MySQL).unwrap();
    let sql = QueryBuilder::<User>::new(d)
        .table("users")
        .where_cond("id = 1")
        .build_delete();

    assert!(sql.to_uppercase().contains("DELETE FROM"));
    assert!(sql.contains("users"));
    assert!(sql.contains("WHERE"));
}

// ===== §8.1 聚合函数契约 =====

#[test]
fn test_build_count_contract() {
    let d = get_dialect(DbType::MySQL).unwrap();
    let sql = QueryBuilder::<User>::new(d).table("users").build_count();

    assert!(sql.to_uppercase().contains("COUNT"));
}

#[test]
fn test_build_max_min_sum_avg_contract() {
    let builder = || QueryBuilder::<User>::new(get_dialect(DbType::MySQL).unwrap()).table("users");

    assert!(builder().build_max("score").to_uppercase().contains("MAX"));
    assert!(builder().build_min("score").to_uppercase().contains("MIN"));
    assert!(builder().build_sum("amount").to_uppercase().contains("SUM"));
    assert!(builder().build_avg("value").to_uppercase().contains("AVG"));
}

// ===== §8.1 validate 契约 =====

#[test]
fn test_validate_select_passes_for_valid_query_contract() {
    let d = get_dialect(DbType::MySQL).unwrap();
    let builder = QueryBuilder::<User>::new(d)
        .table("users")
        .select(vec!["id", "name"]);
    // 合法 SELECT 应通过校验
    assert!(builder.validate().is_ok());
}

#[test]
fn test_validate_insert_rejects_empty_data_contract() {
    let d = get_dialect(DbType::MySQL).unwrap();
    let builder = QueryBuilder::<User>::new(d).table("users");
    let empty: HashMap<String, Value> = HashMap::new();
    // 空 data 应被校验拒绝
    assert!(builder.validate_insert(&empty).is_err());
}

#[test]
fn test_validate_update_rejects_empty_data_contract() {
    let d = get_dialect(DbType::MySQL).unwrap();
    let builder = QueryBuilder::<User>::new(d)
        .table("users")
        .where_cond("id = 1");
    let empty: HashMap<String, Value> = HashMap::new();
    assert!(builder.validate_update(&empty).is_err());
}
