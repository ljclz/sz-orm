//! M3-T3.3: enum dispatch 基准对比
//!
//! 对比 enum dispatch vs Box<dyn Dialect> 分发开销

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use sz_orm_core::{get_dialect, DbType, Dialect};

#[cfg(feature = "perf-enum-dispatch")]
use sz_orm_core::dialect::DialectKind;

fn bench_box_dyn_dispatch(c: &mut Criterion) {
    let dialects: Vec<Box<dyn Dialect>> = vec![
        get_dialect(DbType::MySQL).unwrap(),
        get_dialect(DbType::PostgreSQL).unwrap(),
        get_dialect(DbType::Sqlite).unwrap(),
        get_dialect(DbType::Oracle).unwrap(),
        get_dialect(DbType::SqlServer).unwrap(),
    ];
    c.bench_function("box_dyn_dispatch_quote", |b| {
        b.iter(|| {
            let mut result = String::new();
            for d in &dialects {
                result = black_box(d.quote("users"));
            }
            result
        })
    });
}

#[cfg(feature = "perf-enum-dispatch")]
fn bench_enum_dispatch(c: &mut Criterion) {
    let kinds = [
        DialectKind::MySQL,
        DialectKind::PostgreSQL,
        DialectKind::SQLite,
        DialectKind::Oracle,
        DialectKind::MSSQL,
    ];
    c.bench_function("enum_dispatch_quote", |b| {
        b.iter(|| {
            let mut result = String::new();
            for k in &kinds {
                result = black_box(k.quote("users"));
            }
            result
        })
    });
}

#[cfg(not(feature = "perf-enum-dispatch"))]
fn bench_enum_dispatch(c: &mut Criterion) {
    c.bench_function("enum_dispatch_quote (disabled)", |b| {
        b.iter(|| black_box(0))
    });
}

criterion_group!(benches, bench_box_dyn_dispatch, bench_enum_dispatch);
criterion_main!(benches);
