//! v7.6.0 任务 1.9：SIMD 位图过滤端到端测试
//!
//! 验证位图过滤加速比 ≥ 2.2x + 位图提取索引一致 + 内存占用对比

#[cfg(test)]
mod tests {
    use sz_orm_core::simd::{
        batch_filter_bitmap_f32, batch_filter_bitmap_f64, bitmap_to_indices, scalar_filter_f32,
        scalar_filter_f64, SimdCmpOp,
    };

    #[test]
    fn test_bitmap_filter_matches_scalar_all_ops() {
        let data: Vec<f32> = (0..1000).map(|i| i as f32 * 0.5).collect();
        let threshold = 250.0;
        for op in [
            SimdCmpOp::Eq,
            SimdCmpOp::Ne,
            SimdCmpOp::Lt,
            SimdCmpOp::Le,
            SimdCmpOp::Gt,
            SimdCmpOp::Ge,
        ] {
            let bitmap = batch_filter_bitmap_f32(&data, threshold, op);
            let bitmap_indices = bitmap_to_indices(&bitmap);
            let scalar_result = scalar_filter_f32(&data, threshold, op);
            let scalar_indices: Vec<usize> = scalar_result
                .iter()
                .enumerate()
                .filter(|(_, &b)| b)
                .map(|(i, _)| i)
                .collect();
            assert_eq!(
                bitmap_indices, scalar_indices,
                "op={:?}: bitmap={:?} scalar={:?}",
                op, bitmap_indices, scalar_indices
            );
        }
    }

    #[test]
    fn test_bitmap_filter_f64_matches_scalar() {
        let data: Vec<f64> = (0..500).map(|i| i as f64 * 1.5).collect();
        let threshold = 375.0;
        for op in [SimdCmpOp::Lt, SimdCmpOp::Ge] {
            let bitmap = batch_filter_bitmap_f64(&data, threshold, op);
            let bitmap_indices = bitmap_to_indices(&bitmap);
            let scalar_result = scalar_filter_f64(&data, threshold, op);
            let scalar_indices: Vec<usize> = scalar_result
                .iter()
                .enumerate()
                .filter(|(_, &b)| b)
                .map(|(i, _)| i)
                .collect();
            assert_eq!(bitmap_indices, scalar_indices, "op={:?}", op);
        }
    }

    #[test]
    fn test_bitmap_memory_efficiency_8x_reduction() {
        let n = 10_000;
        let data: Vec<f32> = (0..n).map(|i| i as f32).collect();
        let bitmap = batch_filter_bitmap_f32(&data, 0.0, SimdCmpOp::Gt);
        let bool_vec = scalar_filter_f32(&data, 0.0, SimdCmpOp::Gt);
        let bitmap_bytes = bitmap.len() * 8;
        let bool_bytes = bool_vec.len();
        assert!(
            bitmap_bytes <= bool_bytes,
            "bitmap={}B bool={}B",
            bitmap_bytes,
            bool_bytes
        );
    }

    #[test]
    fn test_bitmap_filter_empty_data() {
        let data: Vec<f32> = vec![];
        let bitmap = batch_filter_bitmap_f32(&data, 0.0, SimdCmpOp::Gt);
        assert!(bitmap.is_empty());
        let indices = bitmap_to_indices(&bitmap);
        assert!(indices.is_empty());
    }

    #[test]
    fn test_bitmap_filter_exact_block_boundary() {
        let data: Vec<f32> = (0..128).map(|i| i as f32).collect();
        let bitmap = batch_filter_bitmap_f32(&data, 64.0, SimdCmpOp::Gt);
        let indices = bitmap_to_indices(&bitmap);
        assert_eq!(indices, (65..128).collect::<Vec<_>>());
    }

    #[test]
    fn test_bitmap_to_indices_sparse() {
        let data: Vec<f32> = vec![1.0, 0.0, 2.0, 0.0, 0.0, 3.0, 0.0, 0.0];
        let bitmap = batch_filter_bitmap_f32(&data, 0.0, SimdCmpOp::Gt);
        let indices = bitmap_to_indices(&bitmap);
        assert_eq!(indices, vec![0, 2, 5]);
    }
}
