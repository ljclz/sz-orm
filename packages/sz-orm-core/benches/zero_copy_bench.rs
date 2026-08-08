//! v3.2.0 零拷贝序列化性能基准测试
//!
//! 对比 owned 路径（`apply_result_map` + `Value::clone`）vs 借用路径
//! （`apply_result_map_borrowed` + `BorrowedValue` Cow 借用）在 10000 行
//! 结果集反序列化场景下的耗时与分配差异。
//!
//! 运行：cargo bench --package sz-orm-core --features zero-copy --bench zero_copy_bench

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use sz_orm_core::result_map::{
    apply_result_map, apply_result_map_borrowed, Mapping, ResultMap, ResultMapRegistry, RowData,
};
use sz_orm_core::value_borrowed::{BorrowedRowData, BorrowedValue};
use sz_orm_core::Value;

fn build_registry() -> ResultMapRegistry {
    let registry = ResultMapRegistry::new();
    let mut rm = ResultMap::new("userMap", "User");
    rm.add_id_mapping(Mapping::new("id", "user_id"))
        .add_result_mapping(Mapping::new("name", "user_name"))
        .add_result_mapping(Mapping::new("email", "user_email"))
        .add_result_mapping(Mapping::new("dept", "user_dept"));
    registry.register(rm);
    registry
}

fn generate_owned_rows(n: usize) -> Vec<RowData> {
    (0..n)
        .map(|i| {
            let mut row = RowData::empty();
            row.set("user_id", Value::I64(i as i64));
            row.set("user_name", Value::String(format!("user_{}", i)));
            row.set(
                "user_email",
                Value::String(format!("user_{}@example.com", i)),
            );
            row.set("user_dept", Value::String(format!("dept_{}", i % 10)));
            row
        })
        .collect()
}

fn generate_borrowed_rows(owned: &[RowData]) -> Vec<BorrowedRowData<'_>> {
    owned
        .iter()
        .map(|r| {
            let mut br = BorrowedRowData::new();
            for (k, v) in r.iter() {
                br.set(k.clone(), BorrowedValue::from_value(v));
            }
            br
        })
        .collect()
}

fn bench_owned_path(c: &mut Criterion) {
    let registry = build_registry();
    let rows = generate_owned_rows(10000);

    c.bench_function("owned_apply_result_map_10000", |b| {
        b.iter(|| {
            for row in black_box(&rows) {
                let _ =
                    apply_result_map(black_box(&registry), black_box("userMap"), black_box(row));
            }
        })
    });
}

fn bench_borrowed_path(c: &mut Criterion) {
    let registry = build_registry();
    let owned_rows = generate_owned_rows(10000);
    let borrowed_rows = generate_borrowed_rows(&owned_rows);

    c.bench_function("borrowed_apply_result_map_10000", |b| {
        b.iter(|| {
            for row in black_box(&borrowed_rows) {
                let _ = apply_result_map_borrowed(
                    black_box(&registry),
                    black_box("userMap"),
                    black_box(row),
                );
            }
        })
    });
}

fn bench_value_clone_vs_borrowed_clone(c: &mut Criterion) {
    let values: Vec<Value> = (0..1000)
        .map(|i| Value::String(format!("string_value_{}_with_some_length", i)))
        .collect();

    c.bench_function("value_clone_1000_strings", |b| {
        b.iter(|| {
            let cloned: Vec<Value> = black_box(&values).clone();
            black_box(cloned);
        })
    });

    let borrowed: Vec<BorrowedValue> = values.iter().map(BorrowedValue::from_value).collect();

    c.bench_function("borrowed_clone_1000_strings", |b| {
        b.iter(|| {
            let cloned: Vec<BorrowedValue> = black_box(&borrowed).clone();
            black_box(cloned);
        })
    });
}

criterion_group!(
    benches,
    bench_owned_path,
    bench_borrowed_path,
    bench_value_clone_vs_borrowed_clone,
);
criterion_main!(benches);
