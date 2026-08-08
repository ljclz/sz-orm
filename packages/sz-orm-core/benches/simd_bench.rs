//! v3.2.0 SIMD 性能基准测试
//!
//! 对比 SIMD 路径 vs 标量路径：
//! - 1024+ 行整数列解码吞吐量
//! - 1024+ 元素 IN/批量过滤耗时
//!
//! 运行：cargo bench --package sz-orm-core --features simd --bench simd_bench

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use sz_orm_core::simd::{
    batch_compare_eq, batch_compare_in, batch_decode_integers, detect, scalar_compare_eq,
    scalar_compare_in, scalar_decode_integers,
};

fn make_i64_buf(values: &[i64]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(values.len() * 8);
    for v in values {
        buf.extend_from_slice(&v.to_le_bytes());
    }
    buf
}

fn bench_decode_integers(c: &mut Criterion) {
    let n: usize = 10000;
    let values: Vec<i64> = (0..n as i64).collect();
    let buf = make_i64_buf(&values);
    let avail = detect();

    c.bench_function("scalar_decode_integers_10000", |b| {
        b.iter(|| {
            let result = scalar_decode_integers(black_box(&buf), black_box(n));
            black_box(result);
        })
    });

    c.bench_function("simd_decode_integers_10000", |b| {
        b.iter(|| {
            let result = batch_decode_integers(black_box(&buf), black_box(n), black_box(avail));
            black_box(result);
        })
    });
}

fn bench_compare_eq(c: &mut Criterion) {
    let n: usize = 10000;
    let values: Vec<i64> = (0..n as i64).collect();
    let target = 5000_i64;
    let avail = detect();

    c.bench_function("scalar_compare_eq_10000", |b| {
        b.iter(|| {
            let result = scalar_compare_eq(black_box(&values), black_box(target));
            black_box(result);
        })
    });

    c.bench_function("simd_compare_eq_10000", |b| {
        b.iter(|| {
            let result = batch_compare_eq(black_box(&values), black_box(target), black_box(avail));
            black_box(result);
        })
    });
}

fn bench_compare_in(c: &mut Criterion) {
    let n: usize = 10000;
    let values: Vec<i64> = (0..n as i64).collect();
    let set: Vec<i64> = (0..100).map(|i| i * 100).collect();
    let avail = detect();

    c.bench_function("scalar_compare_in_10000", |b| {
        b.iter(|| {
            let result = scalar_compare_in(black_box(&values), black_box(&set));
            black_box(result);
        })
    });

    c.bench_function("simd_compare_in_10000", |b| {
        b.iter(|| {
            let result = batch_compare_in(black_box(&values), black_box(&set), black_box(avail));
            black_box(result);
        })
    });
}

criterion_group!(
    benches,
    bench_decode_integers,
    bench_compare_eq,
    bench_compare_in
);
criterion_main!(benches);
