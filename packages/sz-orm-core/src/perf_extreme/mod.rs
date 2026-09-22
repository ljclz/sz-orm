//! v8.0.0 组 2：性能极致优化
//!
//! 连接池零拷贝 acquire、批量 acquire 优化、SIMD 全路径向量化、
//! 零拷贝反序列化、基准对标、退化检测六大能力聚合模块。
//!
//! # Feature gate
//!
//! - `perf-extreme`：聚合门控（依赖 `perf-accel`）
//! - `pool-zero-copy`：零拷贝 acquire + 批量 acquire
//! - `query-simd`：SIMD 全路径向量化
//! - `serde-zero-copy`：零拷贝反序列化

use thiserror::Error;

/// 性能极致优化错误
#[derive(Debug, Error)]
pub enum PerfExtremeError {
    /// 连接池错误
    #[error(transparent)]
    Pool(#[from] crate::error::PoolError),
    /// 序列化错误
    #[error("serde error: {0}")]
    Serde(String),
    /// SIMD 不可用，已回退标量路径
    #[error("SIMD unavailable, fallback to scalar")]
    SimdFallbackScalar,
    /// 基准套件不可用
    #[error("benchmark suite unavailable")]
    BenchmarkUnavailable,
    /// 类型不支持零拷贝
    #[error("type not supported for zero-copy: {0}")]
    UnsupportedType(String),
}

#[cfg(feature = "pool-zero-copy")]
pub mod batch_acquire;
#[cfg(feature = "pool-zero-copy")]
pub mod zero_copy_acquire;

#[cfg(feature = "query-simd")]
pub mod simd_full_pipeline;

#[cfg(feature = "serde-zero-copy")]
pub mod zero_copy_deserializer;

pub mod benchmark_comparator;
pub mod regression_detector;

#[cfg(feature = "pool-zero-copy")]
pub use batch_acquire::{BatchAcquireOptimized, BatchConfig};
#[cfg(feature = "pool-zero-copy")]
pub use zero_copy_acquire::{ZeroCopyAcquire, ZeroCopyConfig};

#[cfg(feature = "query-simd")]
pub use simd_full_pipeline::{QueryBatch, SimdConfig, SimdFullPipeline, VectorizedResult};

#[cfg(feature = "serde-zero-copy")]
pub use zero_copy_deserializer::{ZeroCopyDeserConfig, ZeroCopyDeserializer};

pub use benchmark_comparator::{
    BenchmarkConfig, BenchmarkFramework, BenchmarkReport, FrameworkComparison,
    PerfBenchmarkComparator,
};
pub use regression_detector::{
    PerfMetricsSnapshot, PerfRegressionDetector, RegressionAlert, RegressionConfig,
};
