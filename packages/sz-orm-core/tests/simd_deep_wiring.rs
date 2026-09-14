//! PERF-SIMD-01 接线验证测试（v6.8.0）
//!
//! 验证 SIMD 聚合函数结果与标量路径一致。

use sz_orm_core::simd::{
    batch_cosine_distance, batch_count_nonzero, batch_euclidean_distance, batch_max_f32,
    batch_min_f32, batch_sum_f32, detect,
};

#[test]
fn wiring_batch_sum_correct() {
    let data: Vec<f32> = (1..=1000).map(|i| i as f32).collect();
    let result = batch_sum_f32(&data);
    let expected: f32 = (1..=1000).map(|i| i as f32).sum();
    assert!((result - expected).abs() < 0.001);
}

#[test]
fn wiring_batch_count_nonzero_correct() {
    let mut data = vec![0.0; 1000];
    for i in 0..500 {
        data[i * 2] = 1.0;
    }
    let result = batch_count_nonzero(&data);
    assert_eq!(result, 500);
}

#[test]
fn wiring_batch_min_correct() {
    let data: Vec<f32> = vec![3.0, 1.0, 4.0, 1.0, 5.0, 9.0, 2.0, 6.0];
    assert_eq!(batch_min_f32(&data), Some(1.0));
}

#[test]
fn wiring_batch_max_correct() {
    let data: Vec<f32> = vec![3.0, 1.0, 4.0, 1.0, 5.0, 9.0, 2.0, 6.0];
    assert_eq!(batch_max_f32(&data), Some(9.0));
}

#[test]
fn wiring_cosine_distance_identical_vectors() {
    let a = vec![1.0, 2.0, 3.0];
    let b = vec![1.0, 2.0, 3.0];
    let dist = batch_cosine_distance(&a, &b);
    assert!((dist - 1.0).abs() < 0.001);
}

#[test]
fn wiring_cosine_distance_orthogonal_vectors() {
    let a = vec![1.0, 0.0];
    let b = vec![0.0, 1.0];
    let dist = batch_cosine_distance(&a, &b);
    assert!(dist.abs() < 0.001);
}

#[test]
fn wiring_euclidean_distance_correct() {
    let a = vec![0.0, 0.0];
    let b = vec![3.0, 4.0];
    let dist = batch_euclidean_distance(&a, &b);
    assert!((dist - 5.0).abs() < 0.001);
}

#[test]
fn wiring_simd_availability_detected() {
    let avail = detect();
    // 在 x86_64 上至少应该有 SSE2
    #[cfg(target_arch = "x86_64")]
    assert!(avail.is_available());
}

#[test]
fn wiring_empty_data_edge_cases() {
    assert_eq!(batch_sum_f32(&[]), 0.0);
    assert_eq!(batch_min_f32(&[]), None);
    assert_eq!(batch_max_f32(&[]), None);
    assert_eq!(batch_count_nonzero(&[]), 0);
}

#[test]
fn wiring_large_batch_sum() {
    let data: Vec<f32> = (1..=100_000).map(|i| i as f32).collect();
    let result = batch_sum_f32(&data);
    let expected: f32 = (1..=100_000).map(|i| i as f32).sum();
    assert!((result - expected).abs() / expected < 0.001);
}

#[test]
fn wiring_cosine_distance_empty_vectors() {
    assert_eq!(batch_cosine_distance(&[], &[]), 0.0);
}

#[test]
fn wiring_euclidean_distance_same_vector() {
    let a = vec![1.0, 2.0, 3.0];
    assert_eq!(batch_euclidean_distance(&a, &a), 0.0);
}
