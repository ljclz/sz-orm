//! 批① L2 路径对标：聚合增强路径 vs 基础聚合路径（对拍分级 B：浮点相对容差）
//!
//! 对标对象：
//! - `batch_aggregate_enhanced_f32/f64`（4 路累加器 + 缓存行分块，快路径）
//!   vs `batch_aggregate_f32/f64`（朴素顺序聚合，慢路径）。
//! - 分级：Sum/Avg 顺序敏感 → 相对容差 1e-9（f64 路径内部已有一致性守卫
//!   `verify_aggregate_consistency`，容差 1e-6——本测试用更紧的 1e-9 验证其上界）；
//!   Min/Max 顺序无关 → 严格数值相等。
//! - 已知差异探针（挂账不修）：NaN/±Inf 混合输入下 4 路累加器的 combine 顺序
//!   可能改变 NaN 传播位模式——只断言"各自稳定"，不做跨路径一致性断言。
//!
//! 运行方式：cargo test -p sz-orm-core --features simd --test simd_enhanced_parity

#![cfg(feature = "simd")]

use sz_orm_core::simd::{
    batch_aggregate_enhanced_f32, batch_aggregate_enhanced_f64, batch_aggregate_f32,
    batch_aggregate_f64, SimdAggOp,
};

/// 相对容差（f64 域）：|a-b| <= tol * max(|a|, |b|, 1.0)
fn rel_close(a: f64, b: f64, tol: f64) -> bool {
    if a == b {
        return true;
    }
    if a.is_nan() && b.is_nan() {
        return true;
    }
    let scale = a.abs().max(b.abs()).max(1.0);
    (a - b).abs() <= tol * scale
}

/// 严格相等（NaN == NaN 视为相等：双方契约都是"空集 Min/Max 返回 NaN"）
fn strict_eq(a: f64, b: f64) -> bool {
    a == b || (a.is_nan() && b.is_nan())
}

/// 固定边界集（任务模板要求：空/单元素/极值/重复值）
fn boundary_sets_f64() -> Vec<Vec<f64>> {
    vec![
        vec![],
        vec![0.0],
        vec![1.0],
        vec![-1.5, 2.5, 3.0],
        vec![f64::MAX, f64::MIN, 0.0],
        vec![1e308, 1e308, -1e308], // 溢出敏感
        vec![0.1; 1024],            // 精度敏感重复值
        vec![-0.0, 0.0],            // ±0
        (0..1025).map(|i| i as f64 * 0.5).collect(), // 跨 64 块边界
    ]
}

#[test]
fn parity_enhanced_f64_vs_baseline() {
    for data in boundary_sets_f64() {
        for op in [SimdAggOp::Sum, SimdAggOp::Min, SimdAggOp::Max, SimdAggOp::Avg] {
            let fast = batch_aggregate_enhanced_f64(&data, op);
            let slow = batch_aggregate_f64(&data, op);
            match op {
                // 顺序无关：严格相等
                SimdAggOp::Min | SimdAggOp::Max => {
                    assert!(
                        strict_eq(fast, slow),
                        "Min/Max 必须严格相等: data_len={} op={:?}",
                        data.len(),
                        op
                    );
                }
                // 顺序敏感：相对容差 1e-9
                SimdAggOp::Sum | SimdAggOp::Avg => {
                    assert!(
                        rel_close(fast, slow, 1e-9),
                        "Sum/Avg 相对容差超限: fast={} slow={} len={} op={:?}",
                        fast,
                        slow,
                        data.len(),
                        op
                    );
                }
            }
        }
    }
}

#[test]
fn parity_enhanced_f32_vs_baseline() {
    let sets: Vec<Vec<f32>> = vec![
        vec![],
        vec![0.0],
        vec![1.0, 2.0, 3.0],
        vec![f32::MAX, f32::MIN, 0.0],
        vec![0.1f32; 1024],
        (0..1025).map(|i| i as f32 * 0.5).collect(),
    ];
    for data in sets {
        for op in [SimdAggOp::Sum, SimdAggOp::Min, SimdAggOp::Max, SimdAggOp::Avg] {
            let fast = batch_aggregate_enhanced_f32(&data, op);
            let slow = batch_aggregate_f32(&data, op);
            match op {
                SimdAggOp::Min | SimdAggOp::Max => {
                    assert!(
                        strict_eq(fast, slow),
                        "Min/Max 严格相等: len={} op={:?}",
                        data.len(),
                        op
                    );
                }
                SimdAggOp::Sum | SimdAggOp::Avg => {
                    // f32 累加路径经 f64 提升后回写 f64 容差可放宽到 1e-6
                    assert!(
                        rel_close(fast, slow, 1e-6),
                        "f32 Sum/Avg 容差超限: fast={} slow={} len={} op={:?}",
                        fast,
                        slow,
                        data.len(),
                        op
                    );
                }
            }
        }
    }
}

/// 已知差异探针（挂账）：非有限值下快慢路径各自稳定即可，不互相对拍。
/// NaN 在 f64::min/max 中被忽略（Rust 语义），±Inf 溢出 Sum 顺序敏感——
/// 这些属于数学未定义域，等产品语义拍板后再决定是否统一。
#[test]
fn probe_non_finite_stability() {
    let data = vec![f64::NAN, 1.0, f64::INFINITY, -1.0];
    for op in [SimdAggOp::Sum, SimdAggOp::Min, SimdAggOp::Max, SimdAggOp::Avg] {
        let a = batch_aggregate_enhanced_f64(&data, op);
        let b = batch_aggregate_enhanced_f64(&data, op);
        assert_eq!(a.to_bits(), b.to_bits(), "快路径自身不稳定 op={:?}", op);
        let c = batch_aggregate_f64(&data, op);
        let d = batch_aggregate_f64(&data, op);
        assert_eq!(c.to_bits(), d.to_bits(), "慢路径自身不稳定 op={:?}", op);
    }
}
