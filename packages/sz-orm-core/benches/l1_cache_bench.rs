//! M2-T8.3: L1 缓存性能基准
//! 对比 L1 命中 vs L2 命中 vs DB 查询延迟

#![cfg(feature = "l1-cache")]

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::sync::Arc;
use sz_orm_core::l1_cache::{L1Cache, L1L2Coordinator};

fn bench_l1_hit(c: &mut Criterion) {
    c.bench_function("l1_cache_hit", |b| {
        let mut cache: L1Cache<i32> = L1Cache::new(1000);
        for i in 0..1000 {
            cache.put(i, Arc::new(i));
        }
        b.iter(|| {
            for i in 0..1000 {
                black_box(cache.get(&i));
            }
        })
    });
}

fn bench_l1_miss(c: &mut Criterion) {
    c.bench_function("l1_cache_miss", |b| {
        let mut cache: L1Cache<i32> = L1Cache::new(1000);
        b.iter(|| {
            for i in 0..100 {
                black_box(cache.get(&(i + 10000)));
            }
        })
    });
}

fn bench_l1_put(c: &mut Criterion) {
    c.bench_function("l1_cache_put", |b| {
        b.iter(|| {
            let mut cache: L1Cache<i32> = L1Cache::new(1000);
            for i in 0..100 {
                cache.put(i, Arc::new(i));
            }
            black_box(&cache);
        })
    });
}

fn bench_l1_l2_db_db_load(c: &mut Criterion) {
    c.bench_function("l1_l2_db_db_load", |b| {
        let mut coord: L1L2Coordinator<i32> = L1L2Coordinator::new(1000);
        b.iter(|| {
            for i in 0..100 {
                black_box(coord.get_or_load("t", i, || Some(i)));
            }
        })
    });
}

fn bench_l1_l2_db_l1_hit(c: &mut Criterion) {
    c.bench_function("l1_l2_db_l1_hit", |b| {
        let mut coord: L1L2Coordinator<i32> = L1L2Coordinator::new(1000);
        for i in 0..1000 {
            coord.get_or_load("t", i, || Some(i));
        }
        b.iter(|| {
            for i in 0..100 {
                black_box(coord.get_or_load("t", i, || Some(i)));
            }
        })
    });
}

criterion_group!(
    benches,
    bench_l1_hit,
    bench_l1_miss,
    bench_l1_put,
    bench_l1_l2_db_db_load,
    bench_l1_l2_db_l1_hit
);
criterion_main!(benches);
