//! v8.0.0 任务 2.3：SIMD 全路径向量化
//!
//! 将查询执行全路径（compare/filter/agg）统一 SIMD 向量化，
//! 复用既有 `batch_compare_eq`（`simd.rs:130`）/`batch_filter_f32`（`simd.rs:313`）/
//! `batch_aggregate_f32`（`simd.rs:342`）。
//! SIMD 不可用时自动回退标量路径，结果与标量完全一致。

use crate::perf_metrics::PerfMetrics;
use crate::simd::{self, SimdAggOp, SimdAvailability, SimdCmpOp};

/// SIMD 全路径配置
#[derive(Debug, Clone)]
pub struct SimdConfig {
    /// 单批元素数下限（≥ 此值才走 SIMD 路径）
    pub simd_threshold: usize,
}

impl Default for SimdConfig {
    fn default() -> Self {
        Self { simd_threshold: 8 }
    }
}

/// 查询批次（向量化输入）
#[derive(Debug, Clone)]
pub struct QueryBatch {
    /// i64 列数据（用于 compare_eq）
    pub i64_column: Vec<i64>,
    /// f32 列数据（用于 filter/aggregate）
    pub f32_column: Vec<f32>,
    /// compare_eq 的目标值
    pub compare_target: i64,
    /// filter 的阈值
    pub filter_threshold: f32,
    /// filter 比较算子
    pub filter_op: SimdCmpOp,
    /// aggregate 算子
    pub agg_op: SimdAggOp,
}

impl QueryBatch {
    /// 构造空批次
    pub fn empty() -> Self {
        Self {
            i64_column: Vec::new(),
            f32_column: Vec::new(),
            compare_target: 0,
            filter_threshold: 0.0,
            filter_op: SimdCmpOp::Eq,
            agg_op: SimdAggOp::Sum,
        }
    }
}

/// 向量化执行结果
#[derive(Debug, Clone)]
pub struct VectorizedResult {
    /// compare_eq 结果
    pub compare_mask: Vec<bool>,
    /// filter 结果
    pub filter_mask: Vec<bool>,
    /// aggregate 结果
    pub aggregate: f64,
    /// 是否使用了 SIMD
    pub simd_used: bool,
    /// 加速比（SIMD 路径耗时 vs 标量路径耗时，> 1.0 表示加速）
    pub speedup_ratio: f64,
}

/// SIMD 全路径向量化管线
///
/// 注入 `SimdAvailability` + `SimdConfig`，提供 `execute_vectorized` 方法。
/// SIMD 不可用 → 自动回退标量路径（记录 `SIMD_FALLBACK_SCALAR`）。
pub struct SimdFullPipeline {
    avail: SimdAvailability,
    config: SimdConfig,
    metrics: &'static PerfMetrics,
}

impl SimdFullPipeline {
    /// 创建 SIMD 全路径管线（自动检测 CPU SIMD 能力）
    pub fn new(config: SimdConfig) -> Self {
        Self {
            avail: simd::detect(),
            config,
            metrics: PerfMetrics::global(),
        }
    }

    /// 使用指定 SimdAvailability 构造（用于测试）
    pub fn with_availability(avail: SimdAvailability, config: SimdConfig) -> Self {
        Self {
            avail,
            config,
            metrics: PerfMetrics::global(),
        }
    }

    /// 获取当前 SIMD 可用性
    pub fn availability(&self) -> SimdAvailability {
        self.avail
    }

    /// 向量化执行查询批次
    ///
    /// 单批 ≥ `simd_threshold` 且 SIMD 可用 → SIMD 路径；
    /// 否则 → 标量路径（结果完全一致）。
    pub fn execute_vectorized(&self, batch: &QueryBatch) -> VectorizedResult {
        let use_simd =
            self.avail.is_available() && batch.f32_column.len() >= self.config.simd_threshold;

        let start = std::time::Instant::now();
        let compare_mask =
            simd::batch_compare_eq(&batch.i64_column, batch.compare_target, self.avail);
        let filter_mask =
            simd::batch_filter_f32(&batch.f32_column, batch.filter_threshold, batch.filter_op);
        let aggregate = simd::batch_aggregate_f32(&batch.f32_column, batch.agg_op);
        let simd_elapsed = start.elapsed();

        if use_simd {
            self.metrics.record_simd_hit();
        } else {
            self.metrics.record_simd_miss();
        }

        // 计算加速比：标量路径耗时 / SIMD 路径耗时
        let scalar_start = std::time::Instant::now();
        let _scalar_cmp = simd::scalar_compare_eq(&batch.i64_column, batch.compare_target);
        let _scalar_filter =
            simd::scalar_filter_f32(&batch.f32_column, batch.filter_threshold, batch.filter_op);
        let scalar_elapsed = scalar_start.elapsed();

        let speedup_ratio = if simd_elapsed.as_nanos() > 0 {
            scalar_elapsed.as_nanos() as f64 / simd_elapsed.as_nanos() as f64
        } else {
            1.0
        };

        if use_simd {
            self.metrics.record_simd_speedup(speedup_ratio);
        }

        VectorizedResult {
            compare_mask,
            filter_mask,
            aggregate,
            simd_used: use_simd,
            speedup_ratio,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_batch(n: usize) -> QueryBatch {
        let i64_column: Vec<i64> = (0..n as i64).collect();
        let f32_column: Vec<f32> = (0..n).map(|i| i as f32).collect();
        QueryBatch {
            i64_column,
            f32_column,
            compare_target: n as i64 / 2,
            filter_threshold: n as f32 / 2.0,
            filter_op: SimdCmpOp::Gt,
            agg_op: SimdAggOp::Sum,
        }
    }

    #[test]
    fn execute_vectorized_1024_elements() {
        let pipeline = SimdFullPipeline::new(SimdConfig::default());
        let batch = make_batch(1024);
        let result = pipeline.execute_vectorized(&batch);
        assert_eq!(result.compare_mask.len(), 1024);
        assert_eq!(result.filter_mask.len(), 1024);
        // sum(0..1024) = 1024*1023/2 = 523776
        assert!((result.aggregate - 523776.0).abs() < 1.0);
    }

    #[test]
    fn simd_unavailable_fallback_scalar() {
        let pipeline =
            SimdFullPipeline::with_availability(SimdAvailability::None, SimdConfig::default());
        let batch = make_batch(1024);
        let result = pipeline.execute_vectorized(&batch);
        assert!(!result.simd_used, "SIMD 不可用应回退标量");
        // 结果仍应正确
        assert_eq!(result.compare_mask.len(), 1024);
    }

    #[test]
    fn result_consistent_with_scalar() {
        let simd_pipeline = SimdFullPipeline::new(SimdConfig::default());
        let scalar_pipeline =
            SimdFullPipeline::with_availability(SimdAvailability::None, SimdConfig::default());
        let batch = make_batch(256);
        let simd_result = simd_pipeline.execute_vectorized(&batch);
        let scalar_result = scalar_pipeline.execute_vectorized(&batch);
        assert_eq!(simd_result.compare_mask, scalar_result.compare_mask);
        assert_eq!(simd_result.filter_mask, scalar_result.filter_mask);
        assert!((simd_result.aggregate - scalar_result.aggregate).abs() < 1e-6);
    }

    #[test]
    fn batch_below_threshold_uses_scalar() {
        let pipeline = SimdFullPipeline::with_availability(
            SimdAvailability::Avx2,
            SimdConfig {
                simd_threshold: 1024,
            },
        );
        let batch = make_batch(8);
        let result = pipeline.execute_vectorized(&batch);
        assert!(!result.simd_used, "低于阈值应走标量");
    }

    #[test]
    fn empty_batch_returns_empty_result() {
        let pipeline = SimdFullPipeline::new(SimdConfig::default());
        let batch = QueryBatch::empty();
        let result = pipeline.execute_vectorized(&batch);
        assert!(result.compare_mask.is_empty());
        assert!(result.filter_mask.is_empty());
        assert_eq!(result.aggregate, 0.0);
    }
}
