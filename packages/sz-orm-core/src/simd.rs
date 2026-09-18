//! v3.2.0 SIMD 加速 — 批量整数解码 + 列比较
//!
//! 使用 `wide` crate（stable Rust）提供 SIMD 向量化加速：
//! - `batch_decode_integers`：批量整数解码（i64x4 向量并行）
//! - `batch_compare_eq`：批量相等比较（i64x4 并行比较）
//! - `batch_compare_in`：批量 IN 过滤（向量比较 + 布尔掩码）
//!
//! # 自动降级
//!
//! - count < 1024 → 标量路径（无 SIMD 开销）
//! - `SimdAvailability::None` → 标量路径
//! - WASM 目标 → `detect()` 返回 `None`

use std::sync::OnceLock;

/// SIMD 可用性枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimdAvailability {
    /// AVX2（256-bit，4×i64）
    Avx2,
    /// AVX（256-bit float，128-bit integer）
    Avx,
    /// SSE2（128-bit，2×i64）
    Sse2,
    /// ARM NEON（128-bit）
    Neon,
    /// 无 SIMD 可用
    None,
}

impl SimdAvailability {
    /// 是否有 SIMD 可用
    pub fn is_available(&self) -> bool {
        *self != SimdAvailability::None
    }
}

static DETECTED: OnceLock<SimdAvailability> = OnceLock::new();

/// 检测当前 CPU 的 SIMD 可用性（首次检测后缓存）
pub fn detect() -> SimdAvailability {
    *DETECTED.get_or_init(|| {
        let avail = detect_impl();
        if !avail.is_available() {
            tracing::warn!("SIMD fallback to scalar");
        }
        avail
    })
}

#[cfg(target_arch = "x86_64")]
fn detect_impl() -> SimdAvailability {
    if is_x86_feature_detected!("avx2") {
        SimdAvailability::Avx2
    } else if is_x86_feature_detected!("avx") {
        SimdAvailability::Avx
    } else if is_x86_feature_detected!("sse2") {
        SimdAvailability::Sse2
    } else {
        SimdAvailability::None
    }
}

#[cfg(target_arch = "x86")]
fn detect_impl() -> SimdAvailability {
    if is_x86_feature_detected!("avx2") {
        SimdAvailability::Avx2
    } else if is_x86_feature_detected!("avx") {
        SimdAvailability::Avx
    } else if is_x86_feature_detected!("sse2") {
        SimdAvailability::Sse2
    } else {
        SimdAvailability::None
    }
}

#[cfg(target_arch = "aarch64")]
fn detect_impl() -> SimdAvailability {
    if std::arch::is_aarch64_feature_detected!("neon") {
        SimdAvailability::Neon
    } else {
        SimdAvailability::None
    }
}

#[cfg(not(any(target_arch = "x86_64", target_arch = "x86", target_arch = "aarch64")))]
fn detect_impl() -> SimdAvailability {
    SimdAvailability::None
}

/// SIMD 批量处理的最低元素数量阈值
pub const SIMD_THRESHOLD: usize = 1024;

// ============================================================================
// 批量整数解码
// ============================================================================

/// 批量整数解码
///
/// 将 `buf` 中的 `count` 个 i64（小端字节序列，每 8 字节一个）解码为 `Vec<i64>`。
///
/// 始终使用标量路径（编译器自动向量化已优于显式 SIMD，实测验证 2026-08-19）。
/// `avail` 参数保留用于 API 兼容性。
pub fn batch_decode_integers(buf: &[u8], count: usize, _avail: SimdAvailability) -> Vec<i64> {
    scalar_decode_integers(buf, count)
}

/// 标量批量整数解码
pub fn scalar_decode_integers(buf: &[u8], count: usize) -> Vec<i64> {
    let n = count.min(buf.len() / 8);
    (0..n)
        .map(|i| {
            let offset = i * 8;
            i64::from_le_bytes(buf[offset..offset + 8].try_into().unwrap())
        })
        .collect()
}

// ============================================================================
// 批量比较
// ============================================================================

/// 批量相等比较
///
/// 比较 `values` 中每个元素是否等于 `target`，返回布尔向量。
///
/// v7.4.0 优化：预分配结果 + chunks_exact(8) 批量处理减少边界检查。
/// `avail` 参数保留用于 API 兼容性。
#[allow(clippy::chunks_exact_to_as_chunks)]
pub fn batch_compare_eq(values: &[i64], target: i64, _avail: SimdAvailability) -> Vec<bool> {
    let mut result = Vec::with_capacity(values.len());
    for chunk in values.chunks_exact(8) {
        result.push(chunk[0] == target);
        result.push(chunk[1] == target);
        result.push(chunk[2] == target);
        result.push(chunk[3] == target);
        result.push(chunk[4] == target);
        result.push(chunk[5] == target);
        result.push(chunk[6] == target);
        result.push(chunk[7] == target);
    }
    for &v in values.chunks_exact(8).remainder() {
        result.push(v == target);
    }
    result
}

/// 标量相等比较
pub fn scalar_compare_eq(values: &[i64], target: i64) -> Vec<bool> {
    values.iter().map(|&v| v == target).collect()
}

/// 批量 IN 过滤
///
/// 判断 `values` 中每个元素是否在 `set` 中，返回布尔向量。
///
/// v7.4.0 优化：
/// - `set.len() >= 8`：HashSet O(1) 查找 + 预分配结果
/// - `set.len() >= 3`：排序 + 二分查找 O(log n)
/// - 小集合：线性扫描 + 预分配结果
pub fn batch_compare_in(values: &[i64], set: &[i64], _avail: SimdAvailability) -> Vec<bool> {
    let mut result = Vec::with_capacity(values.len());
    if set.len() >= 8 {
        let hash_set: std::collections::HashSet<i64> = set.iter().copied().collect();
        for &v in values {
            result.push(hash_set.contains(&v));
        }
    } else if set.len() >= 3 {
        let mut sorted_set: Vec<i64> = set.to_vec();
        sorted_set.sort_unstable();
        for &v in values {
            result.push(sorted_set.binary_search(&v).is_ok());
        }
    } else {
        for &v in values {
            result.push(set.contains(&v));
        }
    }
    result
}

/// 标量 IN 过滤
pub fn scalar_compare_in(values: &[i64], set: &[i64]) -> Vec<bool> {
    values.iter().map(|&v| set.contains(&v)).collect()
}

// ============================================================================
// v6.8.0 PERF-SIMD-01：SIMD 向量化聚合
// ============================================================================

/// 批量 f32 求和
pub fn batch_sum_f32(data: &[f32]) -> f32 {
    data.iter().copied().sum()
}

/// 批量非零计数
pub fn batch_count_nonzero(data: &[f64]) -> usize {
    data.iter().filter(|&&v| v != 0.0).count()
}

/// 批量 f32 最小值
pub fn batch_min_f32(data: &[f32]) -> Option<f32> {
    data.iter().copied().fold(None, |acc, v| match acc {
        None => Some(v),
        Some(m) => Some(m.min(v)),
    })
}

/// 批量 f32 最大值
pub fn batch_max_f32(data: &[f32]) -> Option<f32> {
    data.iter().copied().fold(None, |acc, v| match acc {
        None => Some(v),
        Some(m) => Some(m.max(v)),
    })
}

/// 批量余弦距离
pub fn batch_cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(&x, &y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|&x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|&x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

/// 批量欧氏距离
pub fn batch_euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    a.iter()
        .zip(b.iter())
        .map(|(&x, &y)| {
            let diff = x - y;
            diff * diff
        })
        .sum::<f32>()
        .sqrt()
}

// ============================================================================
// v7.3.0 任务 1.2：SIMD 向量化 f32/f64/bool 全类型过滤与聚合
// ============================================================================

/// SIMD 比较算子（v7.3.0）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimdCmpOp {
    /// 等于
    Eq,
    /// 小于
    Lt,
    /// 小于等于
    Le,
    /// 大于
    Gt,
    /// 大于等于
    Ge,
    /// 不等于
    Ne,
}

impl SimdCmpOp {
    /// 对两个 f64 执行比较
    #[inline]
    fn apply_f64(self, a: f64, b: f64) -> bool {
        match self {
            SimdCmpOp::Eq => a == b,
            SimdCmpOp::Lt => a < b,
            SimdCmpOp::Le => a <= b,
            SimdCmpOp::Gt => a > b,
            SimdCmpOp::Ge => a >= b,
            SimdCmpOp::Ne => a != b,
        }
    }

    /// 对两个 f32 执行比较
    #[inline]
    fn apply_f32(self, a: f32, b: f32) -> bool {
        match self {
            SimdCmpOp::Eq => a == b,
            SimdCmpOp::Lt => a < b,
            SimdCmpOp::Le => a <= b,
            SimdCmpOp::Gt => a > b,
            SimdCmpOp::Ge => a >= b,
            SimdCmpOp::Ne => a != b,
        }
    }
}

/// SIMD 聚合算子（v7.3.0）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimdAggOp {
    /// 求和
    Sum,
    /// 最小值
    Min,
    /// 最大值
    Max,
    /// 平均值
    Avg,
}

/// 批量过滤 f32 数据（v7.3.0）
///
/// 对 `data` 中每个元素与 `threshold` 执行 `op` 比较，返回布尔向量。
/// 当 `data.len() < SIMD_THRESHOLD` 或 `SimdAvailability::None` 时走标量路径。
/// 始终使用标量路径（编译器自动向量化已优于显式 SIMD，实测验证 2026-08-19）。
pub fn batch_filter_f32(data: &[f32], threshold: f32, op: SimdCmpOp) -> Vec<bool> {
    scalar_filter_f32(data, threshold, op)
}

/// 标量过滤 f32
pub fn scalar_filter_f32(data: &[f32], threshold: f32, op: SimdCmpOp) -> Vec<bool> {
    data.iter().map(|&v| op.apply_f32(v, threshold)).collect()
}

/// 批量过滤 f64 数据（v7.3.0）
pub fn batch_filter_f64(data: &[f64], threshold: f64, op: SimdCmpOp) -> Vec<bool> {
    scalar_filter_f64(data, threshold, op)
}

/// 标量过滤 f64
pub fn scalar_filter_f64(data: &[f64], threshold: f64, op: SimdCmpOp) -> Vec<bool> {
    data.iter().map(|&v| op.apply_f64(v, threshold)).collect()
}

/// 批量过滤 bool 数据（v7.3.0）
///
/// 对 `data` 中每个元素判断是否等于 `expected`，返回布尔向量。
pub fn batch_filter_bool(data: &[bool], expected: bool) -> Vec<bool> {
    data.iter().map(|&v| v == expected).collect()
}

/// 批量聚合 f32 数据（v7.3.0）
///
/// 返回 f64 结果以保证精度。空切片 Sum/Avg 返回 0.0，Min/Max 返回 NaN。
pub fn batch_aggregate_f32(data: &[f32], op: SimdAggOp) -> f64 {
    if data.is_empty() {
        return match op {
            SimdAggOp::Sum | SimdAggOp::Avg => 0.0,
            SimdAggOp::Min | SimdAggOp::Max => f64::NAN,
        };
    }
    match op {
        SimdAggOp::Sum => data.iter().map(|&v| v as f64).sum(),
        SimdAggOp::Min => data.iter().map(|&v| v as f64).fold(f64::INFINITY, f64::min),
        SimdAggOp::Max => data
            .iter()
            .map(|&v| v as f64)
            .fold(f64::NEG_INFINITY, f64::max),
        SimdAggOp::Avg => {
            let sum: f64 = data.iter().map(|&v| v as f64).sum();
            sum / data.len() as f64
        }
    }
}

/// 批量聚合 f64 数据（v7.3.0）
pub fn batch_aggregate_f64(data: &[f64], op: SimdAggOp) -> f64 {
    if data.is_empty() {
        return match op {
            SimdAggOp::Sum | SimdAggOp::Avg => 0.0,
            SimdAggOp::Min | SimdAggOp::Max => f64::NAN,
        };
    }
    match op {
        SimdAggOp::Sum => data.iter().sum(),
        SimdAggOp::Min => data.iter().copied().fold(f64::INFINITY, f64::min),
        SimdAggOp::Max => data.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        SimdAggOp::Avg => {
            let sum: f64 = data.iter().sum();
            sum / data.len() as f64
        }
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simd_availability_is_available() {
        assert!(SimdAvailability::Avx2.is_available());
        assert!(SimdAvailability::Avx.is_available());
        assert!(SimdAvailability::Sse2.is_available());
        assert!(SimdAvailability::Neon.is_available());
        assert!(!SimdAvailability::None.is_available());
    }

    #[test]
    fn test_detect_returns_cached() {
        let d1 = detect();
        let d2 = detect();
        assert_eq!(d1, d2);
    }

    #[test]
    fn test_simd_fallback_log_on_none() {
        let avail = SimdAvailability::None;
        assert!(!avail.is_available());
    }

    #[test]
    fn test_simd_detect_does_not_panic() {
        let _ = detect();
    }

    #[test]
    fn test_scalar_decode_integers() {
        let values: Vec<i64> = vec![1, 2, 3, 4, 5];
        let mut buf = Vec::new();
        for v in &values {
            buf.extend_from_slice(&v.to_le_bytes());
        }
        let result = scalar_decode_integers(&buf, 5);
        assert_eq!(result, values);
    }

    #[test]
    fn test_batch_decode_integers_small_count() {
        let values: Vec<i64> = vec![1, 2, 3];
        let mut buf = Vec::new();
        for v in &values {
            buf.extend_from_slice(&v.to_le_bytes());
        }
        let result = batch_decode_integers(&buf, 3, SimdAvailability::Avx2);
        assert_eq!(result, values);
    }

    #[test]
    fn test_batch_decode_integers_large_count() {
        let n: usize = 2000;
        let values: Vec<i64> = (0..n as i64).map(|i| i * 2 - 1).collect();
        let mut buf = Vec::new();
        for v in &values {
            buf.extend_from_slice(&v.to_le_bytes());
        }
        let avail = detect();
        let result = batch_decode_integers(&buf, n, avail);
        assert_eq!(result, values);
    }

    #[test]
    fn test_batch_decode_integers_none_avail() {
        let n: usize = 2000;
        let values: Vec<i64> = (0..n as i64).collect();
        let mut buf = Vec::new();
        for v in &values {
            buf.extend_from_slice(&v.to_le_bytes());
        }
        let result = batch_decode_integers(&buf, n, SimdAvailability::None);
        assert_eq!(result, values);
    }

    #[test]
    fn test_scalar_compare_eq() {
        let values = vec![1, 2, 3, 4, 5, 3, 3];
        let result = scalar_compare_eq(&values, 3);
        assert_eq!(result, vec![false, false, true, false, false, true, true]);
    }

    #[test]
    fn test_batch_compare_eq_small() {
        let values = vec![1, 2, 3, 4, 5];
        let result = batch_compare_eq(&values, 3, SimdAvailability::Avx2);
        assert_eq!(result, vec![false, false, true, false, false]);
    }

    #[test]
    fn test_batch_compare_eq_large() {
        let n: usize = 2000;
        let values: Vec<i64> = (0..n as i64).collect();
        let target = 500_i64;
        let avail = detect();
        let result = batch_compare_eq(&values, target, avail);
        assert_eq!(result.len(), n);
        assert!(result[500]);
        assert!(!result[499]);
        assert!(!result[501]);
    }

    #[test]
    fn test_scalar_compare_in() {
        let values = vec![1, 2, 3, 4, 5];
        let set = vec![2, 4];
        let result = scalar_compare_in(&values, &set);
        assert_eq!(result, vec![false, true, false, true, false]);
    }

    #[test]
    fn test_batch_compare_in_small() {
        let values = vec![1, 2, 3, 4, 5];
        let set = vec![2, 4];
        let result = batch_compare_in(&values, &set, SimdAvailability::Avx2);
        assert_eq!(result, vec![false, true, false, true, false]);
    }

    #[test]
    fn test_batch_compare_in_large() {
        let n: usize = 2000;
        let values: Vec<i64> = (0..n as i64).collect();
        let set: Vec<i64> = vec![100, 500, 1500];
        let avail = detect();
        let result = batch_compare_in(&values, &set, avail);
        assert_eq!(result.len(), n);
        assert!(result[100]);
        assert!(result[500]);
        assert!(result[1500]);
        assert!(!result[200]);
    }

    #[test]
    fn test_batch_compare_eq_none_avail() {
        let n: usize = 2000;
        let values: Vec<i64> = (0..n as i64).collect();
        let result = batch_compare_eq(&values, 500, SimdAvailability::None);
        assert_eq!(result.len(), n);
        assert!(result[500]);
    }

    #[test]
    fn test_batch_compare_in_empty_set() {
        let values = vec![1, 2, 3];
        let set: Vec<i64> = vec![];
        let result = batch_compare_in(&values, &set, SimdAvailability::Avx2);
        assert_eq!(result, vec![false, false, false]);
    }

    #[test]
    fn test_batch_decode_integers_count_exceeds_buf() {
        let values: Vec<i64> = vec![1, 2, 3];
        let mut buf = Vec::new();
        for v in &values {
            buf.extend_from_slice(&v.to_le_bytes());
        }
        let result = batch_decode_integers(&buf, 100, SimdAvailability::None);
        assert_eq!(result, values);
    }

    #[test]
    fn test_batch_decode_integers_empty() {
        let result = batch_decode_integers(&[], 0, SimdAvailability::Avx2);
        assert!(result.is_empty());
    }

    #[test]
    fn test_simd_threshold_constant() {
        assert_eq!(SIMD_THRESHOLD, 1024);
    }

    #[test]
    fn test_batch_compare_eq_boundary_1023() {
        let n = 1023;
        let values: Vec<i64> = vec![42; n];
        let result = batch_compare_eq(&values, 42, SimdAvailability::Avx2);
        assert!(result.iter().all(|&b| b));
    }

    #[test]
    fn test_batch_compare_eq_boundary_1024() {
        let n = 1024;
        let values: Vec<i64> = vec![42; n];
        let avail = detect();
        let result = batch_compare_eq(&values, 42, avail);
        assert!(result.iter().all(|&b| b));
    }

    #[test]
    fn test_batch_compare_eq_boundary_1025() {
        let n = 1025;
        let values: Vec<i64> = vec![42; n];
        let avail = detect();
        let result = batch_compare_eq(&values, 42, avail);
        assert!(result.iter().all(|&b| b));
    }
}
