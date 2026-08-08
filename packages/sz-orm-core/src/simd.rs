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
    *DETECTED.get_or_init(detect_impl)
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
/// - `count >= 1024` 且 `avail != None` → SIMD 路径（wide::i64x4 向量批量解析）
/// - `count < 1024` 或 `avail == None` → 标量降级
pub fn batch_decode_integers(buf: &[u8], count: usize, avail: SimdAvailability) -> Vec<i64> {
    if count >= SIMD_THRESHOLD && avail.is_available() {
        simd_decode_integers(buf, count)
    } else {
        scalar_decode_integers(buf, count)
    }
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

#[cfg(feature = "simd")]
fn simd_decode_integers(buf: &[u8], count: usize) -> Vec<i64> {
    use wide::i64x4;

    let n = count.min(buf.len() / 8);
    let mut result = Vec::with_capacity(n);

    let chunk_count = n / 4;
    let remainder = n % 4;

    for chunk in 0..chunk_count {
        let base = chunk * 4;
        let v0 = i64::from_le_bytes(buf[base * 8..base * 8 + 8].try_into().unwrap());
        let v1 = i64::from_le_bytes(buf[(base + 1) * 8..(base + 1) * 8 + 8].try_into().unwrap());
        let v2 = i64::from_le_bytes(buf[(base + 2) * 8..(base + 2) * 8 + 8].try_into().unwrap());
        let v3 = i64::from_le_bytes(buf[(base + 3) * 8..(base + 3) * 8 + 8].try_into().unwrap());

        let vec = i64x4::from([v0, v1, v2, v3]);
        let arr: [i64; 4] = vec.into();
        result.extend_from_slice(&arr);
    }

    for i in 0..remainder {
        let idx = chunk_count * 4 + i;
        let offset = idx * 8;
        result.push(i64::from_le_bytes(
            buf[offset..offset + 8].try_into().unwrap(),
        ));
    }

    result
}

#[cfg(not(feature = "simd"))]
fn simd_decode_integers(buf: &[u8], count: usize) -> Vec<i64> {
    scalar_decode_integers(buf, count)
}

// ============================================================================
// 批量比较
// ============================================================================

/// 批量相等比较
///
/// 比较 `values` 中每个元素是否等于 `target`，返回布尔向量。
///
/// - `values.len() >= 1024` 且 `avail != None` → SIMD 路径
/// - 否则 → 标量降级
pub fn batch_compare_eq(values: &[i64], target: i64, avail: SimdAvailability) -> Vec<bool> {
    if values.len() >= SIMD_THRESHOLD && avail.is_available() {
        simd_compare_eq(values, target)
    } else {
        scalar_compare_eq(values, target)
    }
}

/// 标量相等比较
pub fn scalar_compare_eq(values: &[i64], target: i64) -> Vec<bool> {
    values.iter().map(|&v| v == target).collect()
}

#[cfg(feature = "simd")]
fn simd_compare_eq(values: &[i64], target: i64) -> Vec<bool> {
    use wide::{i64x4, CmpEq};

    let n = values.len();
    let mut result = Vec::with_capacity(n);

    let chunk_count = n / 4;
    let remainder = n % 4;
    let target_vec = i64x4::splat(target);

    for chunk in 0..chunk_count {
        let slice = &values[chunk * 4..chunk * 4 + 4];
        let vec = i64x4::from([slice[0], slice[1], slice[2], slice[3]]);
        let cmp = vec.cmp_eq(target_vec);
        let mask: [i64; 4] = cmp.into();
        for &m in &mask {
            result.push(m != 0);
        }
    }

    for i in 0..remainder {
        let idx = chunk_count * 4 + i;
        result.push(values[idx] == target);
    }

    result
}

#[cfg(not(feature = "simd"))]
fn simd_compare_eq(values: &[i64], target: i64) -> Vec<bool> {
    scalar_compare_eq(values, target)
}

/// 批量 IN 过滤
///
/// 判断 `values` 中每个元素是否在 `set` 中，返回布尔向量。
///
/// - `values.len() >= 1024` 且 `avail != None` → SIMD 路径
/// - 否则 → 标量降级
pub fn batch_compare_in(values: &[i64], set: &[i64], avail: SimdAvailability) -> Vec<bool> {
    if values.len() >= SIMD_THRESHOLD && avail.is_available() {
        simd_compare_in(values, set)
    } else {
        scalar_compare_in(values, set)
    }
}

/// 标量 IN 过滤
pub fn scalar_compare_in(values: &[i64], set: &[i64]) -> Vec<bool> {
    values.iter().map(|&v| set.contains(&v)).collect()
}

#[cfg(feature = "simd")]
fn simd_compare_in(values: &[i64], set: &[i64]) -> Vec<bool> {
    use wide::{i64x4, CmpEq};

    let n = values.len();
    let mut result = Vec::with_capacity(n);

    let chunk_count = n / 4;
    let remainder = n % 4;

    for chunk in 0..chunk_count {
        let slice = &values[chunk * 4..chunk * 4 + 4];
        let vec = i64x4::from([slice[0], slice[1], slice[2], slice[3]]);

        let mut any_match = [false; 4];
        for &s in set {
            let target_vec = i64x4::splat(s);
            let cmp = vec.cmp_eq(target_vec);
            let mask: [i64; 4] = cmp.into();
            for j in 0..4 {
                if mask[j] != 0 {
                    any_match[j] = true;
                }
            }
        }
        result.extend_from_slice(&any_match);
    }

    for i in 0..remainder {
        let idx = chunk_count * 4 + i;
        result.push(set.contains(&values[idx]));
    }

    result
}

#[cfg(not(feature = "simd"))]
fn simd_compare_in(values: &[i64], set: &[i64]) -> Vec<bool> {
    scalar_compare_in(values, set)
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
