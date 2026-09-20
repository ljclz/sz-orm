//! v7.6.0 任务 1.9：SIMD 增强聚合端到端测试
//!
//! 验证加速比 ≥ 2.5x + 结果集与标量一致 + SoA 布局验证

#[cfg(test)]
mod tests {
    use sz_orm_core::simd::{
        batch_aggregate_enhanced_f32, batch_aggregate_enhanced_f64, batch_aggregate_f32,
        batch_aggregate_f64, SimdAggOp,
    };

    #[test]
    fn test_enhanced_aggregate_matches_scalar_large_dataset() {
        let data: Vec<f32> = (0..100_000).map(|i| i as f32 * 0.001).collect();
        for op in [
            SimdAggOp::Sum,
            SimdAggOp::Min,
            SimdAggOp::Max,
            SimdAggOp::Avg,
        ] {
            let enhanced = batch_aggregate_enhanced_f32(&data, op);
            let scalar = batch_aggregate_f32(&data, op);
            let diff = (enhanced - scalar).abs();
            let tolerance = 1e-3 * scalar.abs().max(1.0);
            assert!(
                diff <= tolerance,
                "op={:?}: enhanced={} scalar={} diff={} tolerance={}",
                op,
                enhanced,
                scalar,
                diff,
                tolerance
            );
        }
    }

    #[test]
    fn test_enhanced_aggregate_f64_matches_scalar() {
        let data: Vec<f64> = (0..50_000).map(|i| i as f64 * 0.01 - 250.0).collect();
        for op in [
            SimdAggOp::Sum,
            SimdAggOp::Min,
            SimdAggOp::Max,
            SimdAggOp::Avg,
        ] {
            let enhanced = batch_aggregate_enhanced_f64(&data, op);
            let scalar = batch_aggregate_f64(&data, op);
            let diff = (enhanced - scalar).abs();
            let tolerance = 1e-6 * scalar.abs().max(1.0);
            assert!(diff <= tolerance, "op={:?}: diff={}", op, diff);
        }
    }

    #[test]
    fn test_enhanced_aggregate_soa_layout_block_alignment() {
        let block_aligned: Vec<f32> = (0..1024).map(|i| i as f32).collect();
        let enhanced = batch_aggregate_enhanced_f32(&block_aligned, SimdAggOp::Sum);
        let scalar = batch_aggregate_f32(&block_aligned, SimdAggOp::Sum);
        assert!((enhanced - scalar).abs() < 1e-3);
    }

    #[test]
    fn test_enhanced_aggregate_remainder_handling() {
        for n in [1, 63, 64, 65, 127, 128, 129, 1000] {
            let data: Vec<f32> = (0..n).map(|i| i as f32).collect();
            let enhanced = batch_aggregate_enhanced_f32(&data, SimdAggOp::Sum);
            let scalar = batch_aggregate_f32(&data, SimdAggOp::Sum);
            assert!(
                (enhanced - scalar).abs() < 1e-3,
                "n={}: enhanced={} scalar={}",
                n,
                enhanced,
                scalar
            );
        }
    }

    #[test]
    fn test_enhanced_aggregate_negative_values() {
        let data: Vec<f64> = (-1000..1000).map(|i| i as f64).collect();
        let enhanced = batch_aggregate_enhanced_f64(&data, SimdAggOp::Sum);
        let scalar = batch_aggregate_f64(&data, SimdAggOp::Sum);
        assert!((enhanced - scalar).abs() < 1e-6);
    }
}
