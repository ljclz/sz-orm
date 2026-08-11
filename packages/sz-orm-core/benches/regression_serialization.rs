//! 路径 5：序列化 benchmark（3 基准点）
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use sz_orm_core::Value;

fn serde_serialize(c: &mut Criterion) {
    c.bench_function("serde_serialize", |b| {
        b.iter(|| {
            let v = Value::String("hello world".to_string());
            let json = serde_json::to_string(&v).unwrap();
            black_box(json);
        })
    });
}

fn serde_deserialize(c: &mut Criterion) {
    let json = serde_json::to_string(&Value::I64(42)).unwrap();
    c.bench_function("serde_deserialize", |b| {
        b.iter(|| {
            let v: Value = serde_json::from_str(black_box(&json)).unwrap();
            black_box(v);
        })
    });
}

fn value_to_param(c: &mut Criterion) {
    c.bench_function("value_to_param", |b| {
        b.iter(|| {
            let v = Value::I64(black_box(42));
            black_box(v.to_param());
        })
    });
}

criterion_group!(benches, serde_serialize, serde_deserialize, value_to_param);
criterion_main!(benches);
