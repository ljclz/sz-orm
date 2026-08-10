//! M4-T5.1: 零运行时开销基准对比
//!
//! 对比 typed DSL vs `&str` API 的运行时开销
//! 预期：零差异（类型检查在编译期完成，运行时仅生成相同 SQL）

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

#[cfg(feature = "type-safe-columns")]
impl sz_orm_core::column::Schema for User {
    fn schema_table_name() -> &'static str {
        "users"
    }
}

fn bench_where_eq_str(c: &mut Criterion) {
    c.bench_function("where_eq_str_api", |b| {
        b.iter(|| {
            let dialect = sz_orm_core::get_dialect(DbType::MySQL).unwrap();
            let q = QueryBuilder::<User>::new(dialect)
                .where_eq("id", Value::I64(42))
                .where_eq("name", Value::String("Alice".to_string()))
                .limit(10);
            black_box(q.build_select())
        })
    });
}

#[cfg(feature = "type-safe-columns")]
fn bench_where_eq_col(c: &mut Criterion) {
    use sz_orm_core::column::Column;
    c.bench_function("where_eq_col_typed", |b| {
        b.iter(|| {
            let dialect = sz_orm_core::get_dialect(DbType::MySQL).unwrap();
            let q = QueryBuilder::<User>::new(dialect)
                .where_eq_col(Column::<User>::new("id"), Value::I64(42))
                .where_eq_col(
                    Column::<User>::new("name"),
                    Value::String("Alice".to_string()),
                )
                .limit(10);
            black_box(q.build_select())
        })
    });
}

#[cfg(not(feature = "type-safe-columns"))]
fn bench_where_eq_col(c: &mut Criterion) {
    c.bench_function("where_eq_col_typed (disabled)", |b| b.iter(|| black_box(0)));
}

fn bench_column_name_access(c: &mut Criterion) {
    c.bench_function("column_name_str_literal", |b| b.iter(|| black_box("id")));
}

#[cfg(feature = "type-safe-columns")]
fn bench_column_name_typed(c: &mut Criterion) {
    use sz_orm_core::column::Column;
    c.bench_function("column_name_typed_access", |b| {
        b.iter(|| {
            let col = Column::<User>::new("id");
            black_box(col.name())
        })
    });
}

#[cfg(not(feature = "type-safe-columns"))]
fn bench_column_name_typed(c: &mut Criterion) {
    c.bench_function("column_name_typed_access (disabled)", |b| {
        b.iter(|| black_box(0))
    });
}

criterion_group!(
    benches,
    bench_where_eq_str,
    bench_where_eq_col,
    bench_column_name_access,
    bench_column_name_typed
);
criterion_main!(benches);
