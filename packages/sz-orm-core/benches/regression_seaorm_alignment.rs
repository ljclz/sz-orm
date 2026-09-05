//! v6.2 性能优化：SeaORM 对标基准
//!
//! 等价场景下 SeaORM 查询构建性能，
//! 与 sz-orm regression_query_build 对比。
//! 需要 `bench-seaorm` feature（引入 sea-orm 依赖）。

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use sea_orm::sea_query::{Expr, Order, Query};

fn seaorm_query_build(c: &mut Criterion) {
    let mut group = c.benchmark_group("v62_seaorm_alignment");
    group.throughput(Throughput::Elements(1));
    group.bench_function("seaorm_query_build", |b| {
        b.iter(|| {
            let select = Query::select()
                .from("users")
                .and_where(Expr::col("id").eq(42))
                .to_owned();
            black_box(select);
        })
    });
    group.finish();
}

fn seaorm_query_build_complex(c: &mut Criterion) {
    let mut group = c.benchmark_group("v62_seaorm_alignment");
    group.throughput(Throughput::Elements(1));
    group.bench_function("seaorm_query_build_complex", |b| {
        b.iter(|| {
            let select = Query::select()
                .from("users")
                .and_where(Expr::col("status").eq("active"))
                .and_where(Expr::col("role").eq("admin"))
                .and_where(Expr::col("dept").eq(1))
                .and_where(Expr::col("org").eq(2))
                .and_where(Expr::col("level").eq(3))
                .order_by("created_at", Order::Asc)
                .limit(100)
                .to_owned();
            black_box(select);
        })
    });
    group.finish();
}

criterion_group!(benches, seaorm_query_build, seaorm_query_build_complex);
criterion_main!(benches);