//! v6.2 性能优化：SQLx 对标基准
//!
//! 等价场景下 SQLx 池 acquire 与查询构建性能，
//! 与 sz-orm regression_pool_steady / regression_query_build 对比。

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::Pool;
use sqlx::Sqlite;

async fn create_pool(max_size: u32) -> Pool<Sqlite> {
    SqlitePoolOptions::new()
        .max_connections(max_size)
        .connect("sqlite::memory:")
        .await
        .expect("sqlite pool")
}

fn sqlx_pool_acquire(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let pool = rt.block_on(async { create_pool(10).await });

    let mut group = c.benchmark_group("v62_sqlx_alignment");
    group.throughput(Throughput::Elements(1));
    group.bench_function("sqlx_pool_acquire", |b| {
        b.to_async(&rt).iter(|| async {
            let conn = pool.acquire().await.expect("acquire");
            black_box(&*conn);
            drop(conn);
        })
    });
    group.finish();

    rt.block_on(async {
        pool.close().await;
    });
}

fn sqlx_query_build(c: &mut Criterion) {
    let mut group = c.benchmark_group("v62_sqlx_alignment");
    group.throughput(Throughput::Elements(1));
    group.bench_function("sqlx_query_build", |b| {
        b.iter(|| {
            let query = sqlx::query::<Sqlite>("SELECT * FROM users WHERE id = ? AND status = ?")
                .bind(42i64)
                .bind("active");
            let _ = black_box(query);
        })
    });
    group.finish();
}

criterion_group!(benches, sqlx_pool_acquire, sqlx_query_build);
criterion_main!(benches);
