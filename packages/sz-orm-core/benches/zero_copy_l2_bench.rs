//! M3-T4.3: zero-copy L2 缓存基准对比
//!
//! 对比 zero-copy vs 普通序列化分配计数

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use sz_orm_core::Value;

fn bench_normal_serialization(c: &mut Criterion) {
    let values: Vec<Value> = (0..100)
        .map(|i| Value::String(format!("value_{}", i)))
        .collect();
    c.bench_function("normal_serialize", |b| {
        b.iter(|| {
            let serialized: Vec<String> = values.iter().map(|v| format!("{:?}", v)).collect();
            black_box(serialized)
        })
    });
}

#[cfg(feature = "perf-zero-copy-l2")]
fn bench_zero_copy_serialization(c: &mut Criterion) {
    use sz_orm_core::l2_cache::zero_copy;
    let values: Vec<Value> = (0..100)
        .map(|i| Value::String(format!("value_{}", i)))
        .collect();
    c.bench_function("zero_copy_serialize", |b| {
        b.iter(|| {
            let serialized: Vec<Vec<u8>> =
                values.iter().map(zero_copy::serialize_zero_copy).collect();
            black_box(serialized)
        })
    });
}

#[cfg(not(feature = "perf-zero-copy-l2"))]
fn bench_zero_copy_serialization(c: &mut Criterion) {
    c.bench_function("zero_copy_serialize (disabled)", |b| {
        b.iter(|| black_box(0))
    });
}

criterion_group!(
    benches,
    bench_normal_serialization,
    bench_zero_copy_serialization
);
criterion_main!(benches);
