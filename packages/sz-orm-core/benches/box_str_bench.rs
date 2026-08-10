//! M3-T5.2: Box<str> vs String 内存占用基准

use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_string_size(c: &mut Criterion) {
    c.bench_function("size_of_string", |b| {
        b.iter(|| black_box(std::mem::size_of::<String>()))
    });
}

fn bench_box_str_size(c: &mut Criterion) {
    c.bench_function("size_of_box_str", |b| {
        b.iter(|| black_box(std::mem::size_of::<Box<str>>()))
    });
}

fn bench_string_alloc(c: &mut Criterion) {
    c.bench_function("string_alloc_short", |b| {
        b.iter(|| {
            let s: String = "hello".to_string();
            black_box(s)
        })
    });
}

fn bench_box_str_alloc(c: &mut Criterion) {
    c.bench_function("box_str_alloc_short", |b| {
        b.iter(|| {
            let s: Box<str> = "hello".into();
            black_box(s)
        })
    });
}

criterion_group!(
    benches,
    bench_string_size,
    bench_box_str_size,
    bench_string_alloc,
    bench_box_str_alloc
);
criterion_main!(benches);
