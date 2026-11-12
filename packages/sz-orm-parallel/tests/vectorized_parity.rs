//! 批① L2 路径对标：向量化执行器 vs 标量直算（对拍，olap-vectorized feature）
//!
//! 对标对象：
//! - 快路径：`VectorizedExecutor::execute`（分块聚合，batch_size 分块）
//! - 慢路径：朴素顺序标量实现（本文件内联基准实现）
//! - 分级：Sum 顺序敏感（分块改变累加顺序）→ 相对容差 1e-9；Min/Max/Count
//!   顺序无关 → 严格相等。Filter 计数 → 严格相等（整数计数）。
//!
//! 运行方式：cargo test -p sz-orm-parallel --features olap-vectorized --test vectorized_parity

#![cfg(feature = "olap-vectorized")]

use proptest::prelude::*;
use sz_orm_parallel::{ColumnarBatch, VectorizedExecutor, VectorizedOp};

/// 慢路径基准实现（朴素顺序聚合，与分块路径形成对拍）
mod scalar {
    pub fn sum(data: &[f64]) -> f64 {
        data.iter().sum()
    }
    pub fn min(data: &[f64]) -> f64 {
        data.iter().copied().fold(f64::INFINITY, f64::min)
    }
    pub fn max(data: &[f64]) -> f64 {
        data.iter().copied().fold(f64::NEG_INFINITY, f64::max)
    }
    pub fn filter_count(data: &[f64], threshold: f64) -> usize {
        data.iter().filter(|&&v| v > threshold).count()
    }
}

fn rel_close(a: f64, b: f64, tol: f64) -> bool {
    if a == b {
        return true;
    }
    let scale = a.abs().max(b.abs()).max(1.0);
    (a - b).abs() <= tol * scale
}

fn make_batch(data: &[f64]) -> ColumnarBatch {
    let mut batch = ColumnarBatch::new(vec!["v".to_string()]);
    for &v in data {
        batch.push_row(&[v]);
    }
    batch
}

/// 固定边界集：空 / 单元素 / 极值 / 跨块边界（batch_size 默认值附近）
#[test]
fn parity_boundary_sets() {
    let executor = VectorizedExecutor::new();
    let sets: Vec<Vec<f64>> = vec![
        vec![],
        vec![0.0],
        vec![1.0, -2.0, 3.5],
        vec![f64::MAX, f64::MIN, 0.0],
        vec![0.1; 1024],
        (0..1025).map(|i| i as f64 - 512.0).collect(),
    ];
    for data in &sets {
        let batch = make_batch(data);
        // Sum：分块顺序敏感 → 容差
        let fast = executor.execute(&VectorizedOp::Sum { column: "v".into() }, &batch);
        assert!(
            rel_close(fast.values[0], scalar::sum(data), 1e-9),
            "Sum 容差超限: len={} fast={} slow={}",
            data.len(),
            fast.values[0],
            scalar::sum(data)
        );
        // Min/Max：严格
        let fast_min = executor.execute(&VectorizedOp::Min { column: "v".into() }, &batch);
        assert_eq!(
            fast_min.values[0],
            scalar::min(data),
            "Min 严格相等 len={}",
            data.len()
        );
        let fast_max = executor.execute(&VectorizedOp::Max { column: "v".into() }, &batch);
        assert_eq!(
            fast_max.values[0],
            scalar::max(data),
            "Max 严格相等 len={}",
            data.len()
        );
        // Filter count：严格（整数计数）
        let fast_f = executor.execute(
            &VectorizedOp::Filter {
                column: "v".into(),
                threshold: 0.0,
            },
            &batch,
        );
        assert_eq!(
            fast_f.values[0],
            scalar::filter_count(data, 0.0) as f64,
            "Filter count 严格相等 len={}",
            data.len()
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// PBT：随机向量（有限值域，避免溢出未定义域）四算子对拍
    #[test]
    fn parity_random_vectors(data in prop::collection::vec(-1e6f64..1e6f64, 0..=2000)) {
        let executor = VectorizedExecutor::new();
        let batch = make_batch(&data);

        let fast_sum = executor.execute(&VectorizedOp::Sum { column: "v".into() }, &batch);
        prop_assert!(rel_close(fast_sum.values[0], scalar::sum(&data), 1e-9), "Sum len={}", data.len());

        let fast_min = executor.execute(&VectorizedOp::Min { column: "v".into() }, &batch);
        if data.is_empty() {
            // 空集契约：Min 返回 +Inf（执行器语义），慢路径同为 +Inf
            prop_assert_eq!(fast_min.values[0], f64::INFINITY);
        } else {
            prop_assert_eq!(fast_min.values[0], scalar::min(&data), "Min len={}", data.len());
        }

        let fast_max = executor.execute(&VectorizedOp::Max { column: "v".into() }, &batch);
        if data.is_empty() {
            prop_assert_eq!(fast_max.values[0], f64::NEG_INFINITY);
        } else {
            prop_assert_eq!(fast_max.values[0], scalar::max(&data), "Max len={}", data.len());
        }

        let fast_f = executor.execute(
            &VectorizedOp::Filter { column: "v".into(), threshold: 100.0 },
            &batch,
        );
        prop_assert_eq!(
            fast_f.values[0],
            scalar::filter_count(&data, 100.0) as f64,
            "Filter count len={}",
            data.len()
        );
    }
}
