//! 路径 1：查询构造 benchmark（3 基准点）
//!
//! 运行：cargo bench --features benchmark-suite --bench regression_query_build

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use std::collections::HashMap;
use sz_orm_core::dialect::MySqlDialect;
use sz_orm_core::QueryBuilder;
use sz_orm_core::Value;

#[derive(Clone, Debug)]
struct BenchModel {
    id: i64,
    name: String,
}

impl sz_orm_core::Model for BenchModel {
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
}

impl sz_orm_core::ModelExt for BenchModel {
    fn columns() -> Vec<&'static str> {
        vec!["id", "name"]
    }
    fn fillable() -> Vec<&'static str> {
        vec!["name"]
    }
    fn to_value(&self) -> std::collections::HashMap<String, Value> {
        let mut map = std::collections::HashMap::new();
        map.insert("id".to_string(), Value::I64(self.id));
        map.insert("name".to_string(), Value::String(self.name.clone()));
        map
    }
}

fn select_simple(c: &mut Criterion) {
    c.bench_function("select_simple", |b| {
        b.iter(|| {
            let qb = QueryBuilder::<BenchModel>::new(Box::new(MySqlDialect));
            let sql = qb.build_select();
            black_box(sql);
        })
    });
}

fn select_with_where(c: &mut Criterion) {
    c.bench_function("select_with_where", |b| {
        b.iter(|| {
            let qb = QueryBuilder::<BenchModel>::new(Box::new(MySqlDialect))
                .where_eq("id", Value::I64(42));
            let sql = qb.build_select();
            black_box(sql);
        })
    });
}

fn select_with_join(c: &mut Criterion) {
    c.bench_function("select_with_join", |b| {
        b.iter(|| {
            let qb = QueryBuilder::<BenchModel>::new(Box::new(MySqlDialect)).join_inner(
                "orders",
                "users.id",
                "orders.user_id",
            );
            let sql = qb.build_select();
            black_box(sql);
        })
    });
}

// ===================== v6.2 参数化构建基准 =====================

fn build_select_with_params_simple(c: &mut Criterion) {
    let mut group = c.benchmark_group("v62_build_select_with_params");
    group.throughput(Throughput::Elements(1));
    group.bench_function("build_select_with_params_simple", |b| {
        let qb = QueryBuilder::<BenchModel>::new(Box::new(MySqlDialect))
            .table("users")
            .where_eq("id", Value::I64(42));
        b.iter(|| {
            let (sql, params) = qb.build_select_with_params();
            black_box((sql, params));
        })
    });
    group.bench_function("build_select_with_params_simple_full", |b| {
        b.iter(|| {
            let qb = QueryBuilder::<BenchModel>::new(Box::new(MySqlDialect))
                .table("users")
                .where_eq("id", Value::I64(42));
            let (sql, params) = qb.build_select_with_params();
            black_box((sql, params));
        })
    });
    group.finish();
}

fn build_select_with_params_complex(c: &mut Criterion) {
    let mut group = c.benchmark_group("v62_build_select_with_params");
    group.throughput(Throughput::Elements(1));
    group.bench_function("build_select_with_params_complex", |b| {
        let qb = QueryBuilder::<BenchModel>::new(Box::new(MySqlDialect))
            .table("users")
            .join_inner("orders", "users.id", "orders.user_id")
            .where_eq("status", Value::String("active".to_string()))
            .where_eq("role", Value::String("admin".to_string()))
            .where_eq("dept", Value::I64(1))
            .where_eq("org", Value::I64(2))
            .where_eq("level", Value::I64(3))
            .order_by("created_at")
            .limit(100);
        b.iter(|| {
            let (sql, params) = qb.build_select_with_params();
            black_box((sql, params));
        })
    });
    group.bench_function("build_select_with_params_complex_full", |b| {
        b.iter(|| {
            let qb = QueryBuilder::<BenchModel>::new(Box::new(MySqlDialect))
                .table("users")
                .join_inner("orders", "users.id", "orders.user_id")
                .where_eq("status", Value::String("active".to_string()))
                .where_eq("role", Value::String("admin".to_string()))
                .where_eq("dept", Value::I64(1))
                .where_eq("org", Value::I64(2))
                .where_eq("level", Value::I64(3))
                .order_by("created_at")
                .limit(100);
            let (sql, params) = qb.build_select_with_params();
            black_box((sql, params));
        })
    });
    group.finish();
}

fn build_batch_insert_1000(c: &mut Criterion) {
    let rows: Vec<HashMap<String, Value>> = (0..1000)
        .map(|i| {
            let mut row = HashMap::new();
            row.insert("name".to_string(), Value::String(format!("user_{i}")));
            row.insert("age".to_string(), Value::I32(i % 100));
            row
        })
        .collect();

    c.bench_function("build_batch_insert_1000", |b| {
        b.iter(|| {
            let qb = QueryBuilder::<BenchModel>::new(Box::new(MySqlDialect)).table("users");
            let (sql, params) = qb.build_batch_insert_with_params(&rows);
            assert!(
                sql.matches("VALUES").count() == 1,
                "应生成 1 条多值 SQL，而非 1000 条"
            );
            black_box((sql, params));
        })
    });
}

fn build_batch_upsert_1000(c: &mut Criterion) {
    let rows: Vec<HashMap<String, Value>> = (0..1000)
        .map(|i| {
            let mut row = HashMap::new();
            row.insert("name".to_string(), Value::String(format!("user_{i}")));
            row.insert("age".to_string(), Value::I32(i % 100));
            row
        })
        .collect();

    c.bench_function("build_batch_upsert_1000", |b| {
        b.iter(|| {
            let qb = QueryBuilder::<BenchModel>::new(Box::new(MySqlDialect)).table("users");
            let result = qb.build_batch_upsert_with_params(&rows, &["name"], &["age"]);
            let _ = black_box(result);
        })
    });
}

criterion_group!(
    benches,
    select_simple,
    select_with_where,
    select_with_join,
    build_select_with_params_simple,
    build_select_with_params_complex,
    build_batch_insert_1000,
    build_batch_upsert_1000
);
criterion_main!(benches);
