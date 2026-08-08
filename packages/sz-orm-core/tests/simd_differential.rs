//! v3.2.0 SIMD 差分测试 — SIMD vs 标量结果一致性验证
//!
//! 验证 SIMD 路径与标量路径在所有输入下产生完全一致的结果，
//! 覆盖边界值、大数量、count 边界（1023/1024/1025）等场景。

#![cfg(feature = "simd")]

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

#[test]
fn test_differential_decode_integers_small() {
    let test_cases: Vec<Vec<i64>> = vec![
        vec![],
        vec![0],
        vec![1, 2, 3],
        vec![-1, -2, -3],
        vec![i64::MAX, i64::MIN, 0],
        vec![42; 100],
    ];

    let avail = detect();
    for values in &test_cases {
        let buf = make_i64_buf(values);
        let scalar = scalar_decode_integers(&buf, values.len());
        let simd = batch_decode_integers(&buf, values.len(), avail);
        assert_eq!(scalar, simd, "decode mismatch for len={}", values.len());
        assert_eq!(simd, *values, "decode incorrect for len={}", values.len());
    }
}

#[test]
fn test_differential_decode_integers_large() {
    let n: usize = 5000;
    let values: Vec<i64> = (0..n as i64).map(|i| i * 3 - 7).collect();
    let buf = make_i64_buf(&values);

    let scalar = scalar_decode_integers(&buf, n);
    let simd = batch_decode_integers(&buf, n, detect());

    assert_eq!(scalar, simd);
    assert_eq!(simd, values);
}

#[test]
fn test_differential_compare_eq() {
    let test_cases: Vec<(Vec<i64>, i64)> = vec![
        (vec![], 0),
        (vec![1, 2, 3], 2),
        (vec![1, 2, 3, 4, 5], 10),
        (vec![42; 2000], 42),
        (vec![42; 2000], 0),
        (vec![-1, 0, 1, i64::MAX, i64::MIN], i64::MAX),
        ((0..3000).map(|i| i - 1500).collect(), 0),
    ];

    let avail = detect();
    for (values, target) in &test_cases {
        let scalar = scalar_compare_eq(values, *target);
        let simd = batch_compare_eq(values, *target, avail);
        assert_eq!(
            scalar,
            simd,
            "compare_eq mismatch for len={}, target={}",
            values.len(),
            target
        );
    }
}

#[test]
fn test_differential_compare_in() {
    let test_cases: Vec<(Vec<i64>, Vec<i64>)> = vec![
        (vec![], vec![]),
        (vec![1, 2, 3], vec![2]),
        (vec![1, 2, 3, 4, 5], vec![2, 4, 6]),
        (vec![42; 2000], vec![42]),
        (vec![42; 2000], vec![0, 1, 2]),
        ((0..3000).collect(), vec![100, 500, 1500, 2500]),
        ((0..3000).collect(), vec![]),
    ];

    let avail = detect();
    for (values, set) in &test_cases {
        let scalar = scalar_compare_in(values, set);
        let simd = batch_compare_in(values, set, avail);
        assert_eq!(
            scalar,
            simd,
            "compare_in mismatch for values.len={}, set.len={}",
            values.len(),
            set.len()
        );
    }
}

#[test]
fn test_boundary_count_1023() {
    let n: usize = 1023;
    let values: Vec<i64> = (0..n as i64).collect();
    let buf = make_i64_buf(&values);

    let scalar = scalar_decode_integers(&buf, n);
    let simd = batch_decode_integers(&buf, n, detect());
    assert_eq!(scalar, simd);

    let target = 500_i64;
    let scalar_eq = scalar_compare_eq(&values, target);
    let simd_eq = batch_compare_eq(&values, target, detect());
    assert_eq!(scalar_eq, simd_eq);
}

#[test]
fn test_boundary_count_1024() {
    let n: usize = 1024;
    let values: Vec<i64> = (0..n as i64).collect();
    let buf = make_i64_buf(&values);

    let scalar = scalar_decode_integers(&buf, n);
    let simd = batch_decode_integers(&buf, n, detect());
    assert_eq!(scalar, simd);

    let target = 500_i64;
    let scalar_eq = scalar_compare_eq(&values, target);
    let simd_eq = batch_compare_eq(&values, target, detect());
    assert_eq!(scalar_eq, simd_eq);
}

#[test]
fn test_boundary_count_1025() {
    let n: usize = 1025;
    let values: Vec<i64> = (0..n as i64).collect();
    let buf = make_i64_buf(&values);

    let scalar = scalar_decode_integers(&buf, n);
    let simd = batch_decode_integers(&buf, n, detect());
    assert_eq!(scalar, simd);

    let target = 500_i64;
    let scalar_eq = scalar_compare_eq(&values, target);
    let simd_eq = batch_compare_eq(&values, target, detect());
    assert_eq!(scalar_eq, simd_eq);
}

#[test]
fn test_boundary_vector_width_multiples() {
    for &n in &[4, 8, 12, 16, 1024, 1028, 1032] {
        let values: Vec<i64> = (0..n as i64).collect();
        let buf = make_i64_buf(&values);
        let scalar = scalar_decode_integers(&buf, n);
        let simd = batch_decode_integers(&buf, n, detect());
        assert_eq!(scalar, simd, "mismatch at n={}", n);
    }
}

#[test]
fn test_boundary_extreme_values() {
    let values: Vec<i64> = vec![i64::MAX, i64::MIN, 0, -1, 1, i64::MAX - 1, i64::MIN + 1];
    let buf = make_i64_buf(&values);

    let scalar = scalar_decode_integers(&buf, values.len());
    let simd = batch_decode_integers(&buf, values.len(), detect());
    assert_eq!(scalar, simd);
    assert_eq!(simd, values);
}

#[test]
fn test_all_same_values() {
    let n: usize = 2000;
    let values: Vec<i64> = vec![42; n];
    let buf = make_i64_buf(&values);

    let scalar = scalar_decode_integers(&buf, n);
    let simd = batch_decode_integers(&buf, n, detect());
    assert_eq!(scalar, simd);

    let scalar_eq = scalar_compare_eq(&values, 42);
    let simd_eq = batch_compare_eq(&values, 42, detect());
    assert_eq!(scalar_eq, simd_eq);
    assert!(simd_eq.iter().all(|&b| b));
}

#[test]
fn test_all_different_values() {
    let n: usize = 2000;
    let values: Vec<i64> = (0..n as i64).collect();
    let buf = make_i64_buf(&values);

    let scalar = scalar_decode_integers(&buf, n);
    let simd = batch_decode_integers(&buf, n, detect());
    assert_eq!(scalar, simd);

    let target = n as i64 + 100;
    let scalar_eq = scalar_compare_eq(&values, target);
    let simd_eq = batch_compare_eq(&values, target, detect());
    assert_eq!(scalar_eq, simd_eq);
    assert!(simd_eq.iter().all(|&b| !b));
}

#[test]
fn test_none_avail_always_scalar() {
    let n: usize = 2000;
    let values: Vec<i64> = (0..n as i64).collect();
    let buf = make_i64_buf(&values);

    let scalar = scalar_decode_integers(&buf, n);
    let none = batch_decode_integers(&buf, n, SimdAvailability::None);
    assert_eq!(scalar, none);

    let scalar_eq = scalar_compare_eq(&values, 500);
    let none_eq = batch_compare_eq(&values, 500, SimdAvailability::None);
    assert_eq!(scalar_eq, none_eq);

    let set = vec![100, 200, 300];
    let scalar_in = scalar_compare_in(&values, &set);
    let none_in = batch_compare_in(&values, &set, SimdAvailability::None);
    assert_eq!(scalar_in, none_in);
}
