//! M3-T2.3: SmallString 基准对比
//!
//! 对比 CompactString vs String SQL 构造吞吐量（短字符串场景）

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use sz_orm_core::{DbType, Model, QueryBuilder, Value};

#[derive(Clone, Default)]
struct User {
    id: i64,
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
}

fn bench_build_select_short(c: &mut Criterion) {
    c.bench_function("build_select_short_string", |b| {
        b.iter(|| {
            let dialect = sz_orm_core::get_dialect(DbType::MySQL).unwrap();
            let q = QueryBuilder::<User>::new(dialect)
                .select(vec!["id", "name", "email"])
                .expect("valid columns")
                .where_eq("id", Value::I64(1))
                .limit(10);
            black_box(q.build_select())
        })
    });
}

fn bench_build_select_long(c: &mut Criterion) {
    let long_name = "a".repeat(100);
    c.bench_function("build_select_long_string", |b| {
        b.iter(|| {
            let dialect = sz_orm_core::get_dialect(DbType::MySQL).unwrap();
            let q = QueryBuilder::<User>::new(dialect)
                .where_eq("name", Value::String(long_name.clone()))
                .limit(10);
            black_box(q.build_select())
        })
    });
}

fn bench_build_insert(c: &mut Criterion) {
    c.bench_function("build_insert_short_string", |b| {
        b.iter(|| {
            let dialect = sz_orm_core::get_dialect(DbType::MySQL).unwrap();
            let mut data = std::collections::HashMap::new();
            data.insert("name".to_string(), Value::String("test".to_string()));
            data.insert("age".to_string(), Value::I64(30));
            let q = QueryBuilder::<User>::new(dialect);
            black_box(q.build_insert(&data))
        })
    });
}

fn bench_sql_buffer(c: &mut Criterion) {
    use sz_orm_core::sql_buffer::SqlBuffer;
    c.bench_function("sql_buffer_push_str", |b| {
        b.iter(|| {
            let mut buf = SqlBuffer::new();
            buf.push_str("SELECT id, name, email FROM users WHERE id = 1 LIMIT 10");
            black_box(buf.into_string())
        })
    });
}

criterion_group!(
    benches,
    bench_build_select_short,
    bench_build_select_long,
    bench_build_insert,
    bench_sql_buffer
);
criterion_main!(benches);
