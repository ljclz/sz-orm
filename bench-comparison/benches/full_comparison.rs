//! full_comparison — 全维度 × 多方言 × 竞品基准主入口（v2.3.0 任务 B）
//!
//! 聚合 5 个维度（CRUD/关联/事务/连接池/分页）× 4 档规模 × 4 竞品。
//!
//! # 运行方式
//!
//! ```bash
//! # SQLite only（默认）
//! cargo bench --bench full_comparison
//!
//! # 单独运行某维度
//! cargo bench --bench bench_crud
//! cargo bench --bench bench_relation
//! cargo bench --bench bench_transaction
//! cargo bench --bench bench_pool
//! cargo bench --bench bench_pagination
//! ```

#[path = "competitor_adapter.rs"]
mod competitor_adapter;
use competitor_adapter::*;

use criterion::{criterion_group, criterion_main, Criterion};

// ============================================================================
// CRUD 维度
// ============================================================================

fn bench_crud_single(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    for &size in DATASET_SIZES {
        for mut adapter in create_all_adapters() {
            if rt.block_on(adapter.setup(size)).is_err() { continue; }
            let name = format!("crud_single/{}/sqlite/{}", adapter.name(), size);
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async { for i in 0..iters as i64 { std::hint::black_box(adapter.insert_one(&BenchRecord::new(i + 1)).await); } });
                    start.elapsed()
                });
            });
            let _ = rt.block_on(adapter.teardown());
        }
    }
}

fn bench_crud_find(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    for &size in DATASET_SIZES {
        for mut adapter in create_all_adapters() {
            if rt.block_on(adapter.setup(size)).is_err() { continue; }
            let name = format!("crud_find/{}/sqlite/{}", adapter.name(), size);
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async { for i in 0..iters as i64 { std::hint::black_box(adapter.find_one((i % size as i64) + 1).await); } });
                    start.elapsed()
                });
            });
            let _ = rt.block_on(adapter.teardown());
        }
    }
}

fn bench_crud_batch(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    for &size in DATASET_SIZES {
        for mut adapter in create_all_adapters() {
            if rt.block_on(adapter.setup(size)).is_err() { continue; }
            let name = format!("crud_batch/{}/sqlite/{}", adapter.name(), size);
            let records: Vec<BenchRecord> = (1..=100).map(BenchRecord::new).collect();
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async { for _ in 0..iters { std::hint::black_box(adapter.insert_batch(&records).await); } });
                    start.elapsed()
                });
            });
            let _ = rt.block_on(adapter.teardown());
        }
    }
}

// ============================================================================
// 关联查询维度
// ============================================================================

fn bench_relation_has_one(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    for &size in DATASET_SIZES {
        for mut adapter in create_all_adapters() {
            if rt.block_on(adapter.setup(size)).is_err() { continue; }
            let name = format!("relation_has_one/{}/sqlite/{}", adapter.name(), size);
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async { for i in 0..iters as i64 { std::hint::black_box(adapter.find_with_has_one((i % size as i64) + 1).await); } });
                    start.elapsed()
                });
            });
            let _ = rt.block_on(adapter.teardown());
        }
    }
}

fn bench_relation_has_many(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    for &size in DATASET_SIZES {
        for mut adapter in create_all_adapters() {
            if rt.block_on(adapter.setup(size)).is_err() { continue; }
            let name = format!("relation_has_many/{}/sqlite/{}", adapter.name(), size);
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async { for i in 0..iters as i64 { std::hint::black_box(adapter.find_with_has_many((i % size as i64) + 1).await); } });
                    start.elapsed()
                });
            });
            let _ = rt.block_on(adapter.teardown());
        }
    }
}

fn bench_relation_m2m(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    for &size in DATASET_SIZES {
        for mut adapter in create_all_adapters() {
            if rt.block_on(adapter.setup(size)).is_err() { continue; }
            let name = format!("relation_m2m/{}/sqlite/{}", adapter.name(), size);
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async { for _ in 0..iters { std::hint::black_box(adapter.find_with_many_to_many(1).await); } });
                    start.elapsed()
                });
            });
            let _ = rt.block_on(adapter.teardown());
        }
    }
}

// ============================================================================
// 事务维度
// ============================================================================

fn bench_transaction(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    for &size in DATASET_SIZES {
        for mut adapter in create_all_adapters() {
            if rt.block_on(adapter.setup(size)).is_err() { continue; }
            let name = format!("transaction/{}/sqlite/{}", adapter.name(), size);
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async { for _ in 0..iters { std::hint::black_box(adapter.transaction_commit().await); } });
                    start.elapsed()
                });
            });
            let _ = rt.block_on(adapter.teardown());
        }
    }
}

// ============================================================================
// 连接池维度
// ============================================================================

fn bench_pool(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    for &size in DATASET_SIZES {
        for mut adapter in create_all_adapters() {
            if rt.block_on(adapter.setup(size)).is_err() { continue; }
            let name = format!("pool/{}/sqlite/{}", adapter.name(), size);
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async { for _ in 0..iters { std::hint::black_box(adapter.pool_acquire().await); } });
                    start.elapsed()
                });
            });
            let _ = rt.block_on(adapter.teardown());
        }
    }
}

// ============================================================================
// 分页维度
// ============================================================================

fn bench_pagination(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    for &size in DATASET_SIZES {
        for mut adapter in create_all_adapters() {
            if rt.block_on(adapter.setup(size)).is_err() { continue; }
            let name = format!("pagination/{}/sqlite/{}", adapter.name(), size);
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async { for i in 0..iters as i64 { std::hint::black_box(adapter.paginate_offset(((i as usize) % size) / 2, 20).await); } });
                    start.elapsed()
                });
            });
            let _ = rt.block_on(adapter.teardown());
        }
    }
}

// ============================================================================
// criterion 配置 + 主入口
// ============================================================================

fn configure_criterion() -> Criterion {
    Criterion::default()
        .sample_size(100)
        .warm_up_time(std::time::Duration::from_secs(3))
        .measurement_time(std::time::Duration::from_secs(10))
        .confidence_level(0.95)
        .noise_threshold(0.05)
}

criterion_group! {
    name = benches;
    config = configure_criterion();
    targets =
        bench_crud_single,
        bench_crud_find,
        bench_crud_batch,
        bench_relation_has_one,
        bench_relation_has_many,
        bench_relation_m2m,
        bench_transaction,
        bench_pool,
        bench_pagination
}

criterion_main!(benches);
