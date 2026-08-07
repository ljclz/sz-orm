//! bench_pagination — 分页维度基准（OFFSET/游标）（v2.3.0 T-B-006）
//!
//! 通过 CompetitorAdapter trait 调用四竞品，测量分页查询性能。
//! 游标分页不支持的竞品标注 N/A。

#[path = "competitor_adapter.rs"]
mod competitor_adapter;
use competitor_adapter::*;

use criterion::{criterion_group, criterion_main, Criterion};

fn bench_pagination_offset(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    for &size in DATASET_SIZES {
        for mut adapter in create_all_adapters() {
            if rt.block_on(adapter.setup(size)).is_err() {
                continue;
            }
            let name = format!("pagination_offset/{}/sqlite/{}", adapter.name(), size);
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async {
                        for i in 0..iters as i64 {
                            let offset = ((i as usize) % size) / 2;
                            std::hint::black_box(adapter.paginate_offset(offset, 20).await);
                        }
                    });
                    start.elapsed()
                });
            });
            let _ = rt.block_on(adapter.teardown());
        }
    }
}

fn bench_pagination_cursor(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    for &size in DATASET_SIZES {
        for mut adapter in create_all_adapters() {
            if rt.block_on(adapter.setup(size)).is_err() {
                continue;
            }
            let name = format!("pagination_cursor/{}/sqlite/{}", adapter.name(), size);
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async {
                        for i in 0..iters as i64 {
                            let last_id = i % size as i64;
                            std::hint::black_box(adapter.paginate_cursor(last_id, 20).await);
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
        bench_pagination_offset,
        bench_pagination_cursor
}

criterion_main!(benches);