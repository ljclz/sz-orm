//! 路径 1：查询构造 benchmark（3 基准点）
//!
//! 运行：cargo bench --features benchmark-suite --bench regression_query_build

use criterion::{black_box, criterion_group, criterion_main, Criterion};
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
            let qb = QueryBuilder::<BenchModel>::new(Box::new(MySqlDialect))
                .join_inner("orders", "users.id", "orders.user_id");
            let sql = qb.build_select();
            black_box(sql);
        })
    });
}

criterion_group!(benches, select_simple, select_with_where, select_with_join);
criterion_main!(benches);