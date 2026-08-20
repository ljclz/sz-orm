#![cfg(feature = "simd")]
//! performance feature 实测验证
//!
//! 验证 `performance` feature gate（simd + l1-cache + plan-cache + zero-copy）的实际效果。
//! 直接对比标量路径 vs SIMD 路径的耗时，输出实测加速比。

use std::time::Instant;
use sz_orm_core::simd::{
    batch_compare_eq, batch_compare_in, batch_decode_integers, detect, scalar_compare_eq,
    scalar_compare_in, scalar_decode_integers, SimdAvailability,
};

fn make_i64_buf(values: &[i64]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(values.len() * 8);
    for v in values {
        buf.extend_from_slice(&v.to_le_bytes());
    }
    buf
}

fn bench_fn<F: Fn() -> T, T>(f: F, iterations: usize) -> u128 {
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = f();
    }
    start.elapsed().as_nanos() / iterations as u128
}

#[test]
fn test_performance_simd_vs_scalar_decode() {
    let n: usize = 50_000;
    let values: Vec<i64> = (0..n as i64).collect();
    let buf = make_i64_buf(&values);
    let avail = detect();

    let iterations = 200;
    let scalar_ns = bench_fn(|| scalar_decode_integers(&buf, n), iterations);
    let simd_ns = bench_fn(|| batch_decode_integers(&buf, n, avail), iterations);

    let speedup = scalar_ns as f64 / simd_ns as f64;
    eprintln!("[perf] decode_integers n={n}: scalar={scalar_ns}ns simd={simd_ns}ns speedup={speedup:.2}x avail={avail:?}");

    assert_eq!(
        scalar_decode_integers(&buf, n),
        batch_decode_integers(&buf, n, avail)
    );
}

#[test]
fn test_performance_simd_vs_scalar_compare_eq() {
    let n: usize = 50_000;
    let values: Vec<i64> = (0..n as i64).collect();
    let target = 25_000_i64;
    let avail = detect();

    let iterations = 200;
    let scalar_ns = bench_fn(|| scalar_compare_eq(&values, target), iterations);
    let simd_ns = bench_fn(|| batch_compare_eq(&values, target, avail), iterations);

    let speedup = scalar_ns as f64 / simd_ns as f64;
    eprintln!("[perf] compare_eq n={n}: scalar={scalar_ns}ns simd={simd_ns}ns speedup={speedup:.2}x avail={avail:?}");

    assert_eq!(
        scalar_compare_eq(&values, target),
        batch_compare_eq(&values, target, avail)
    );
}

#[test]
fn test_performance_simd_vs_scalar_compare_in() {
    let n: usize = 50_000;
    let values: Vec<i64> = (0..n as i64).collect();
    let set: Vec<i64> = (0..500).map(|i| i * 100).collect();
    let avail = detect();

    let iterations = 100;
    let scalar_ns = bench_fn(|| scalar_compare_in(&values, &set), iterations);
    let simd_ns = bench_fn(|| batch_compare_in(&values, &set, avail), iterations);

    let speedup = scalar_ns as f64 / simd_ns as f64;
    eprintln!("[perf] compare_in n={n} set={}(): scalar={scalar_ns}ns simd={simd_ns}ns speedup={speedup:.2}x avail={avail:?}", set.len());

    assert_eq!(
        scalar_compare_in(&values, &set),
        batch_compare_in(&values, &set, avail)
    );
}

#[test]
fn test_performance_simd_correctness_edge_cases() {
    let avail = detect();

    let empty: Vec<i64> = vec![];
    assert_eq!(
        batch_compare_eq(&empty, 42, avail),
        scalar_compare_eq(&empty, 42)
    );

    let single = vec![42_i64];
    assert_eq!(
        batch_compare_eq(&single, 42, avail),
        scalar_compare_eq(&single, 42)
    );

    let threshold: Vec<i64> = (0..1024).collect();
    assert_eq!(
        batch_compare_eq(&threshold, 500, avail),
        scalar_compare_eq(&threshold, 500)
    );

    let above_threshold: Vec<i64> = (0..1025).collect();
    assert_eq!(
        batch_compare_eq(&above_threshold, 500, avail),
        scalar_compare_eq(&above_threshold, 500)
    );

    let all_same = vec![7_i64; 10_000];
    assert_eq!(
        batch_compare_eq(&all_same, 7, avail),
        scalar_compare_eq(&all_same, 7)
    );
    assert_eq!(
        batch_compare_eq(&all_same, 99, avail),
        scalar_compare_eq(&all_same, 99)
    );
}

#[test]
fn test_performance_simd_availability_detection() {
    let avail = detect();
    eprintln!("[perf] SIMD availability on this machine: {avail:?}");
    assert!(matches!(
        avail,
        SimdAvailability::Avx2
            | SimdAvailability::Avx
            | SimdAvailability::Sse2
            | SimdAvailability::Neon
            | SimdAvailability::None
    ));
}

#[test]
fn test_performance_feature_gate_compilation() {
    let avail = detect();
    let values: Vec<i64> = (0..2000).collect();

    let result = batch_compare_eq(&values, 1000, avail);
    assert_eq!(result.len(), 2000);
    assert!(result[1000]);
    assert!(!result[0]);

    eprintln!(
        "[perf] performance feature gate: simd path active = {}",
        avail.is_available() && values.len() >= 1024
    );
}
