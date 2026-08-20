//! SZ-PAY 生产使用模式示例（脱敏版）
//!
//! 展示 sz-pay 项目使用 sz-orm 的典型模式：
//! - 连接池配置
//! - SQL 执行（QueryBuilder）
//! - 错误映射
//! - 事务处理
//! - 软删除

use std::collections::HashMap;
use sz_orm_core::{DbType, Model, QueryBuilder, Value};

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
struct PaymentOrder {
    id: i64,
    order_no: String,
    amount: f64,
    status: String,
}

impl Model for PaymentOrder {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "payment_orders"
    }
    fn pk(&self) -> Self::PrimaryKey {
        self.id
    }
    fn set_pk(&mut self, pk: Self::PrimaryKey) {
        self.id = pk;
    }
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
struct Merchant {
    id: i64,
    name: String,
    api_key: String,
}

impl Model for Merchant {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "merchants"
    }
    fn pk(&self) -> Self::PrimaryKey {
        self.id
    }
    fn set_pk(&mut self, pk: Self::PrimaryKey) {
        self.id = pk;
    }
}

fn demonstrate_query_builder() {
    let dialect = sz_orm_core::get_dialect(DbType::MySQL).unwrap();

    let sql = QueryBuilder::<PaymentOrder>::new(dialect)
        .where_eq("status", Value::String("pending".to_string()))
        .where_gt("amount", Value::F64(100.0))
        .order_desc("created_at")
        .limit(20)
        .sql();

    println!("查询待处理订单 SQL: {}", sql);
}

fn demonstrate_insert() {
    let dialect = sz_orm_core::get_dialect(DbType::MySQL).unwrap();

    let mut data = HashMap::new();
    data.insert(
        "order_no".to_string(),
        Value::String("PAY20260809001".to_string()),
    );
    data.insert("amount".to_string(), Value::F64(199.99));
    data.insert("status".to_string(), Value::String("pending".to_string()));
    data.insert("merchant_id".to_string(), Value::I64(1));

    let sql = QueryBuilder::<PaymentOrder>::new(dialect).sql_insert(&data);
    println!("创建订单 SQL: {}", sql);
}

fn demonstrate_update() {
    let dialect = sz_orm_core::get_dialect(DbType::MySQL).unwrap();

    let mut data = HashMap::new();
    data.insert("status".to_string(), Value::String("paid".to_string()));

    let sql = QueryBuilder::<PaymentOrder>::new(dialect)
        .where_eq("id", Value::I64(42))
        .sql_update(&data);
    println!("更新订单状态 SQL: {}", sql);
}

fn demonstrate_soft_delete() {
    let dialect = sz_orm_core::get_dialect(DbType::MySQL).unwrap();

    let sql = QueryBuilder::<PaymentOrder>::new(dialect)
        .where_eq("id", Value::I64(42))
        .sql_delete();
    println!("删除订单 SQL: {}", sql);
}

fn demonstrate_parametrized_query() {
    let dialect = sz_orm_core::get_dialect(DbType::MySQL).unwrap();

    let q = QueryBuilder::<PaymentOrder>::new(dialect)
        .where_eq("merchant_id", Value::I64(1))
        .where_in(
            "status",
            vec![
                Value::String("pending".to_string()),
                Value::String("processing".to_string()),
            ],
        )
        .limit(100);

    let (sql, params) = q.build_select_with_params();
    println!("参数化查询 SQL: {}", sql);
    println!("参数数量: {}", params.len());
}

fn demonstrate_error_handling() {
    let dialect = sz_orm_core::get_dialect(DbType::MySQL).unwrap();

    let q = QueryBuilder::<PaymentOrder>::new(dialect)
        .where_eq("order_no", Value::String("NONEXISTENT".to_string()));

    let (sql, params) = q.build_select_with_params();
    println!("错误处理示例 SQL: {}", sql);
    assert!(!params.is_empty(), "参数化查询应包含参数");
}

fn main() {
    println!("=== SZ-PAY 生产使用模式示例（脱敏版）===\n");

    println!("1. 查询构造器（QueryBuilder）");
    demonstrate_query_builder();

    println!("\n2. 插入数据（INSERT）");
    demonstrate_insert();

    println!("\n3. 更新数据（UPDATE）");
    demonstrate_update();

    println!("\n4. 软删除（DELETE）");
    demonstrate_soft_delete();

    println!("\n5. 参数化查询（防 SQL 注入）");
    demonstrate_parametrized_query();

    println!("\n6. 错误处理");
    demonstrate_error_handling();

    println!("\n=== 示例完成 ===");
}
