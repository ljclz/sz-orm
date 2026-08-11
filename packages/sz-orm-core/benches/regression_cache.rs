//! 路径 3：缓存 benchmark（3 基准点）
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::collections::HashMap;

fn l1_hit(c: &mut Criterion) {
    let mut cache: HashMap<String, String> = HashMap::new();
    cache.insert("key1".to_string(), "value1".to_string());
    c.bench_function("l1_hit", |b| {
        b.iter(|| black_box(cache.get(black_box("key1"))))
    });
}

fn l1_miss(c: &mut Criterion) {
    let cache: HashMap<String, String> = HashMap::new();
    c.bench_function("l1_miss", |b| {
        b.iter(|| black_box(cache.get(black_box("nonexistent"))))
    });
}

fn l2_hit(c: &mut Criterion) {
    let mut l1: HashMap<String, String> = HashMap::new();
    let mut l2: HashMap<String, String> = HashMap::new();
    l2.insert("key2".to_string(), "value2".to_string());
    c.bench_function("l2_hit", |b| {
        b.iter(|| {
            let key = black_box("key2");
            l1.get(key).or_else(|| l2.get(key))
        })
    });
}

criterion_group!(benches, l1_hit, l1_miss, l2_hit);
criterion_main!(benches);
