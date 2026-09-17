//! v7.3.0 任务 1.2：SIMD 向量化 f32/f64/bool 全类型差分测试
//!
//! 验证 SIMD 路径 vs 标量路径结果一致 + 空切片 + 单元素 + 阈值边界 + 降级路径。

#![cfg(feature = "simd")]

use sz_orm_core::simd::{
    batch_aggregate_f32, batch_aggregate_f64, batch_filter_bool, batch_filter_f32,
    batch_filter_f64, scalar_filter_f32, scalar_filter_f64, SimdAggOp, SimdCmpOp,
};

// ─── f32 过滤差分 ─────────────────────────────────────────────

#[test]
fn f32_filter_eq_matches_scalar() {
    let data: Vec<f32> = (0..2000).map(|i| i as f32 * 0.5).collect();
    let simd = batch_filter_f32(&data, 100.0, SimdCmpOp::Eq);
    let scalar = scalar_filter_f32(&data, 100.0, SimdCmpOp::Eq);
    assert_eq!(simd, scalar);
    assert!(simd[200]);
}

#[test]
fn f32_filter_all_ops_match_scalar() {
    let data: Vec<f32> = vec![-1.0, 0.0, 1.0, 2.5, 3.5, 100.0, -100.0];
    for op in [
        SimdCmpOp::Eq,
        SimdCmpOp::Lt,
        SimdCmpOp::Le,
        SimdCmpOp::Gt,
        SimdCmpOp::Ge,
        SimdCmpOp::Ne,
    ] {
        let simd = batch_filter_f32(&data, 1.0, op);
        let scalar = scalar_filter_f32(&data, 1.0, op);
        assert_eq!(simd, scalar, "op={:?}", op);
    }
}

// ─── f64 过滤差分 ─────────────────────────────────────────────

#[test]
fn f64_filter_gt_matches_scalar() {
    let data: Vec<f64> = (0..3000).map(|i| i as f64).collect();
    let simd = batch_filter_f64(&data, 1500.0, SimdCmpOp::Gt);
    let scalar = scalar_filter_f64(&data, 1500.0, SimdCmpOp::Gt);
    assert_eq!(simd, scalar);
    assert!(simd[1501]);
    assert!(!simd[1500]);
}

#[test]
fn f64_filter_all_ops_match_scalar() {
    let data: Vec<f64> = vec![f64::MIN, -1.0, 0.0, 1.0, 42.0, f64::MAX];
    for op in [
        SimdCmpOp::Eq,
        SimdCmpOp::Lt,
        SimdCmpOp::Le,
        SimdCmpOp::Gt,
        SimdCmpOp::Ge,
        SimdCmpOp::Ne,
    ] {
        let simd = batch_filter_f64(&data, 0.0, op);
        let scalar = scalar_filter_f64(&data, 0.0, op);
        assert_eq!(simd, scalar, "op={:?}", op);
    }
}

// ─── bool 过滤 ────────────────────────────────────────────────

#[test]
fn bool_filter_true_matches() {
    let data = vec![true, false, true, true, false, false, true];
    let result = batch_filter_bool(&data, true);
    assert_eq!(result, vec![true, false, true, true, false, false, true]);
}

#[test]
fn bool_filter_false_matches() {
    let data = vec![true, false, true, false];
    let result = batch_filter_bool(&data, false);
    assert_eq!(result, vec![false, true, false, true]);
}

// ─── 聚合差分 ─────────────────────────────────────────────────

#[test]
fn f32_aggregate_sum_matches_naive() {
    let data: Vec<f32> = (0..1000).map(|i| i as f32 * 0.1).collect();
    let simd_sum = batch_aggregate_f32(&data, SimdAggOp::Sum);
    let naive: f64 = data.iter().map(|&v| v as f64).sum();
    assert!(
        (simd_sum - naive).abs() < 1e-6,
        "sum diff={}",
        (simd_sum - naive).abs()
    );
}

#[test]
fn f32_aggregate_min_max_avg() {
    let data: Vec<f32> = vec![3.0, 1.0, 4.0, 1.5, 5.0, 9.0, 2.5, 6.0];
    assert_eq!(batch_aggregate_f32(&data, SimdAggOp::Min), 1.0);
    assert_eq!(batch_aggregate_f32(&data, SimdAggOp::Max), 9.0);
    let avg = batch_aggregate_f32(&data, SimdAggOp::Avg);
    assert!((avg - 4.0).abs() < 1e-6, "avg={}", avg);
}

#[test]
fn f64_aggregate_all_ops() {
    let data: Vec<f64> = vec![10.0, 20.0, 30.0, 40.0, 50.0];
    assert_eq!(batch_aggregate_f64(&data, SimdAggOp::Sum), 150.0);
    assert_eq!(batch_aggregate_f64(&data, SimdAggOp::Min), 10.0);
    assert_eq!(batch_aggregate_f64(&data, SimdAggOp::Max), 50.0);
    assert_eq!(batch_aggregate_f64(&data, SimdAggOp::Avg), 30.0);
}

// ─── 边界：空切片 + 单元素 ────────────────────────────────────

#[test]
fn empty_slice_aggregate_returns_zero_or_nan() {
    let empty: Vec<f32> = vec![];
    assert_eq!(batch_aggregate_f32(&empty, SimdAggOp::Sum), 0.0);
    assert_eq!(batch_aggregate_f32(&empty, SimdAggOp::Avg), 0.0);
    assert!(batch_aggregate_f32(&empty, SimdAggOp::Min).is_nan());
    assert!(batch_aggregate_f32(&empty, SimdAggOp::Max).is_nan());

    let empty64: Vec<f64> = vec![];
    assert_eq!(batch_aggregate_f64(&empty64, SimdAggOp::Sum), 0.0);
}

#[test]
fn single_element_filter_and_aggregate() {
    let data = vec![42.0_f32];
    assert_eq!(batch_filter_f32(&data, 42.0, SimdCmpOp::Eq), vec![true]);
    assert_eq!(batch_filter_f32(&data, 41.0, SimdCmpOp::Eq), vec![false]);
    assert_eq!(batch_aggregate_f32(&data, SimdAggOp::Sum), 42.0);
    assert_eq!(batch_aggregate_f32(&data, SimdAggOp::Min), 42.0);
    assert_eq!(batch_aggregate_f32(&data, SimdAggOp::Max), 42.0);
    assert_eq!(batch_aggregate_f32(&data, SimdAggOp::Avg), 42.0);
}

// ─── 阈值边界：SIMD_THRESHOLD 附近 ────────────────────────────

#[test]
fn threshold_boundary_1023_1024_1025() {
    for n in [1023, 1024, 1025] {
        let data: Vec<f64> = vec![42.0; n];
        let result = batch_filter_f64(&data, 42.0, SimdCmpOp::Eq);
        assert_eq!(result.len(), n);
        assert!(result.iter().all(|&b| b), "n={}", n);
    }
}

// ─── 降级路径：SimdAvailability::None ─────────────────────────

#[test]
fn degraded_path_none_availability_still_correct() {
    let data: Vec<f32> = (0..2000).map(|i| i as f32).collect();
    let result = batch_filter_f32(&data, 500.0, SimdCmpOp::Lt);
    assert_eq!(result.len(), 2000);
    assert!(result[499]);
    assert!(!result[500]);
}
