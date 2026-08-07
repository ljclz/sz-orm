//! bench_transaction — 事务维度基准（提交/回滚/savepoint）（v2.3.0 T-B-004）
//!
//! 通过 CompetitorAdapter trait 调用四竞品，测量事务操作性能。
//! savepoint 不支持的竞品标注 N/A。

#[path = "competitor_adapter.rs"]
mod competitor_adapter;
use competitor_adapter::*;

use criterion::{criterion_group, criterion_main, Criterion};

fn bench_transaction_commit(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    for &size in DATASET_SIZES {
        for mut adapter in create_all_adapters() {
            if rt.block_on(adapter.setup(size)).is_err() {
                continue;
            }
            let name = format!("tx_commit/{}/sqlite/{}", adapter.name(), size);
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async {
                        for _ in 0..iters {
                            std::hint::black_box(adapter.transaction_commit().await);
                        }
                    });
                    start.elapsed()
                });
            });
            let _ = rt.block_on(adapter.teardown());
        }
    }
}

fn bench_transaction_rollback(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    for &size in DATASET_SIZES {
        for mut adapter in create_all_adapters() {
            if rt.block_on(adapter.setup(size)).is_err() {
                continue;
            }
            let name = format!("tx_rollback/{}/sqlite/{}", adapter.name(), size);
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async {
                        for _ in 0..iters {
                            std::hint::black_box(adapter.transaction_rollback().await);
                        }
                    });
                    start.elapsed()
                });
            });
            let _ = rt.block_on(adapter.teardown());
        }
    }
}

fn bench_nested_transaction(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    for &size in DATASET_SIZES {
        for mut adapter in create_all_adapters() {
            if rt.block_on(adapter.setup(size)).is_err() {
                continue;
            }
            let name = format!("tx_nested/{}/sqlite/{}", adapter.name(), size);
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async {
                        for _ in 0..iters {
                            std::hint::black_box(adapter.nested_transaction().await);
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
        bench_transaction_commit,
        bench_transaction_rollback,
        bench_nested_transaction
}

criterion_main!(benches);