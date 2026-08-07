//! bench_crud — CRUD 维度基准（单条 + 批量）（v2.3.0 T-B-002）
//!
//! 通过 CompetitorAdapter trait 调用四竞品，测量 CRUD 操作性能。
//! 数据集规模：10/100/1000/10000 四档。

#[path = "competitor_adapter.rs"]
mod competitor_adapter;
use competitor_adapter::*;

use criterion::{criterion_group, criterion_main, Criterion};

fn bench_crud_single(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    for &size in DATASET_SIZES {
        for mut adapter in create_all_adapters() {
            if rt.block_on(adapter.setup(size)).is_err() {
                continue;
            }
            let name = format!("crud_single/{}/sqlite/{}", adapter.name(), size);
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async {
                        for i in 0..iters as i64 {
                            let rec = BenchRecord::new(i + 1);
                            std::hint::black_box(adapter.insert_one(&rec).await);
                        }
                    });
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
            if rt.block_on(adapter.setup(size)).is_err() {
                continue;
            }
            let name = format!("crud_find/{}/sqlite/{}", adapter.name(), size);
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async {
                        for i in 0..iters as i64 {
                            let id = (i % size as i64) + 1;
                            std::hint::black_box(adapter.find_one(id).await);
                        }
                    });
                    start.elapsed()
                });
            });
            let _ = rt.block_on(adapter.teardown());
        }
    }
}

fn bench_crud_update(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    for &size in DATASET_SIZES {
        for mut adapter in create_all_adapters() {
            if rt.block_on(adapter.setup(size)).is_err() {
                continue;
            }
            let name = format!("crud_update/{}/sqlite/{}", adapter.name(), size);
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async {
                        for i in 0..iters as i64 {
                            let id = (i % size as i64) + 1;
                            std::hint::black_box(adapter.update_one(id, "updated").await);
                        }
                    });
                    start.elapsed()
                });
            });
            let _ = rt.block_on(adapter.teardown());
        }
    }
}

fn bench_crud_delete(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    for &size in DATASET_SIZES {
        for mut adapter in create_all_adapters() {
            if rt.block_on(adapter.setup(size)).is_err() {
                continue;
            }
            let name = format!("crud_delete/{}/sqlite/{}", adapter.name(), size);
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async {
                        for i in 0..iters as i64 {
                            let id = (i % size as i64) + 1;
                            std::hint::black_box(adapter.delete_one(id).await);
                        }
                    });
                    start.elapsed()
                });
            });
            let _ = rt.block_on(adapter.teardown());
        }
    }
}

fn bench_crud_batch_insert(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    for &size in DATASET_SIZES {
        for mut adapter in create_all_adapters() {
            if rt.block_on(adapter.setup(size)).is_err() {
                continue;
            }
            let name = format!("crud_batch_insert/{}/sqlite/{}", adapter.name(), size);
            let records: Vec<BenchRecord> = (1..=100).map(BenchRecord::new).collect();
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async {
                        for _ in 0..iters {
                            std::hint::black_box(adapter.insert_batch(&records).await);
                        }
                    });
                    start.elapsed()
                });
            });
            let _ = rt.block_on(adapter.teardown());
        }
    }
}

fn bench_crud_batch_find(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    for &size in DATASET_SIZES {
        for mut adapter in create_all_adapters() {
            if rt.block_on(adapter.setup(size)).is_err() {
                continue;
            }
            let name = format!("crud_batch_find/{}/sqlite/{}", adapter.name(), size);
            let ids: Vec<i64> = (1..=100).collect();
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async {
                        for _ in 0..iters {
                            std::hint::black_box(adapter.find_batch(&ids).await);
                        }
                    });
                    start.elapsed()
                });
            });
            let _ = rt.block_on(adapter.teardown());
        }
    }
}

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
        bench_crud_update,
        bench_crud_delete,
        bench_crud_batch_insert,
        bench_crud_batch_find
}

criterion_main!(benches);