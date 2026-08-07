//! bench_pool — 连接池维度基准（获取/并发竞争）（v2.3.0 T-B-005）
//!
//! 通过 CompetitorAdapter trait 调用四竞品，测量连接池操作性能。
//! Diesel 使用单连接，pool_acquire 返回 Unsupported，标注 N/A。

#[path = "competitor_adapter.rs"]
mod competitor_adapter;
use competitor_adapter::*;

use criterion::{criterion_group, criterion_main, Criterion};

fn bench_pool_acquire(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    for &size in DATASET_SIZES {
        for mut adapter in create_all_adapters() {
            if rt.block_on(adapter.setup(size)).is_err() {
                continue;
            }
            let name = format!("pool_acquire/{}/sqlite/{}", adapter.name(), size);
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async {
                        for _ in 0..iters {
                            std::hint::black_box(adapter.pool_acquire().await);
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
    targets = bench_pool_acquire
}

criterion_main!(benches);