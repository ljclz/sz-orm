//! 快速入门 — QueryBuilder 基础用法
//!
//! 演示如何使用 QueryBuilder 构建 SELECT/INSERT/UPDATE/DELETE SQL。
//! 无需数据库连接，仅生成 SQL 字符串。
//!
//! 运行：`cargo run -p sz-orm-examples --bin quick_start`

use std::collections::HashMap;

use sz_orm_core::dialect::get_dialect;
use sz_orm_core::{DbType, Model, QueryBuilder, TimestampFields, Value};

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
struct User {
    id: i64,
    name: String,
    email: String,
}

impl Model for User {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "users"
    }
    fn pk(&self) -> Self::PrimaryKey {
        self.id
    }
    fn set_pk(&mut self, pk: Self::PrimaryKey) {
        self.id = pk;
    }
    fn timestamp_fields() -> Option<TimestampFields> {
        Some(TimestampFields::with_both("created_at", "updated_at"))
    }
}

fn main() {
    let make_dialect = || get_dialect(DbType::MySQL).expect("MySQL 方言可用");

    // ===== SELECT =====
    let select_sql = QueryBuilder::<User>::new(make_dialect())
        .table("users")
        .select(vec!["id", "name", "email"])
        .where_cond("status = 'active'")
        .order_by("created_at")
        .order_desc("id")
        .limit(10)
        .build_select();
    println!("SELECT:\n{}\n", select_sql);

    // ===== WHERE 复合条件 =====
    let complex_sql = QueryBuilder::<User>::new(make_dialect())
        .table("users")
        .select(vec!["id", "name"])
        .where_cond("status = 'active'")
        .or_where("role = 'admin'")
        .where_in("id", vec![Value::I64(1), Value::I64(2), Value::I64(3)])
        .where_between("age", Value::I64(18), Value::I64(65))
        .where_null("deleted_at")
        .page(3, 20)
        .build_select();
    println!("复杂 WHERE:\n{}\n", complex_sql);

    // ===== JOIN =====
    let join_sql = QueryBuilder::<User>::new(make_dialect())
        .table("users")
        .select(vec!["users.id", "posts.title"])
        .join_inner("posts", "users.id", "posts.user_id")
        .join_left("profiles", "users.id", "profiles.user_id")
        .where_cond("users.status = 'active'")
        .build_select();
    println!("JOIN:\n{}\n", join_sql);

    // ===== 聚合 =====
    let count_sql = QueryBuilder::<User>::new(make_dialect())
        .table("users")
        .where_cond("status = 'active'")
        .build_count();
    println!("COUNT:\n{}\n", count_sql);

    // ===== INSERT =====
    let mut data = HashMap::new();
    data.insert("name".to_string(), Value::String("Alice".to_string()));
    data.insert(
        "email".to_string(),
        Value::String("alice@example.com".to_string()),
    );
    data.insert("age".to_string(), Value::I64(25));

    let insert_sql = QueryBuilder::<User>::new(make_dialect())
        .table("users")
        .build_insert(&data);
    println!("INSERT:\n{}\n", insert_sql);

    // ===== UPDATE =====
    let mut update_data = HashMap::new();
    update_data.insert("name".to_string(), Value::String("Bob".to_string()));

    let update_sql = QueryBuilder::<User>::new(make_dialect())
        .table("users")
        .where_cond("id = 1")
        .build_update(&update_data);
    println!("UPDATE:\n{}\n", update_sql);

    // ===== DELETE =====
    let delete_sql = QueryBuilder::<User>::new(make_dialect())
        .table("users")
        .where_cond("id = 1")
        .build_delete();
    println!("DELETE:\n{}\n", delete_sql);

    // ===== 校验 =====
    let validate_result = QueryBuilder::<User>::new(make_dialect())
        .table("users")
        .select(vec!["id", "name"])
        .where_cond("id = 1")
        .validate();
    println!("校验结果: {:?}", validate_result);
}
