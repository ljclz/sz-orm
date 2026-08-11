//! WasmRealDbMetrics — 查询指标采集

use std::sync::atomic::{AtomicU64, Ordering};

/// WASM 真实 DB 指标
///
/// 线程安全地记录查询次数、延迟、错误数。
#[derive(Debug)]
pub struct WasmRealDbMetrics {
    total_queries: AtomicU64,
    total_latency_ms: AtomicU64,
    total_errors: AtomicU64,
    max_latency_ms: AtomicU64,
}

impl WasmRealDbMetrics {
    /// 创建空指标
    pub fn new() -> Self {
        Self {
            total_queries: AtomicU64::new(0),
            total_latency_ms: AtomicU64::new(0),
            total_errors: AtomicU64::new(0),
            max_latency_ms: AtomicU64::new(0),
        }
    }

    /// 记录一次成功查询
    pub fn record_query(&self, latency_ms: u64) {
        self.total_queries.fetch_add(1, Ordering::Relaxed);
        self.total_latency_ms
            .fetch_add(latency_ms, Ordering::Relaxed);

        let mut current_max = self.max_latency_ms.load(Ordering::Relaxed);
        while latency_ms > current_max {
            match self.max_latency_ms.compare_exchange(
                current_max,
                latency_ms,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(new_max) => current_max = new_max,
            }
        }
    }

    /// 记录一次错误
    pub fn record_error(&self) {
        self.total_errors.fetch_add(1, Ordering::Relaxed);
    }

    /// 总查询数
    pub fn total_queries(&self) -> u64 {
        self.total_queries.load(Ordering::Relaxed)
    }

    /// 总延迟（毫秒）
    pub fn total_latency_ms(&self) -> u64 {
        self.total_latency_ms.load(Ordering::Relaxed)
    }

    /// 总错误数
    pub fn total_errors(&self) -> u64 {
        self.total_errors.load(Ordering::Relaxed)
    }

    /// 最大单次延迟（毫秒）
    pub fn max_latency_ms(&self) -> u64 {
        self.max_latency_ms.load(Ordering::Relaxed)
    }

    /// 平均延迟（毫秒）
    pub fn avg_latency_ms(&self) -> f64 {
        let count = self.total_queries();
        if count == 0 {
            0.0
        } else {
            self.total_latency_ms() as f64 / count as f64
        }
    }

    /// 错误率
    pub fn error_rate(&self) -> f64 {
        let total = self.total_queries() + self.total_errors();
        if total == 0 {
            0.0
        } else {
            self.total_errors() as f64 / total as f64
        }
    }

    /// 重置所有指标
    pub fn reset(&self) {
        self.total_queries.store(0, Ordering::Relaxed);
        self.total_latency_ms.store(0, Ordering::Relaxed);
        self.total_errors.store(0, Ordering::Relaxed);
        self.max_latency_ms.store(0, Ordering::Relaxed);
    }
}

impl Default for WasmRealDbMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for WasmRealDbMetrics {
    fn clone(&self) -> Self {
        Self {
            total_queries: AtomicU64::new(self.total_queries.load(Ordering::Relaxed)),
            total_latency_ms: AtomicU64::new(self.total_latency_ms.load(Ordering::Relaxed)),
            total_errors: AtomicU64::new(self.total_errors.load(Ordering::Relaxed)),
            max_latency_ms: AtomicU64::new(self.max_latency_ms.load(Ordering::Relaxed)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_empty() {
        let m = WasmRealDbMetrics::new();
        assert_eq!(m.total_queries(), 0);
        assert_eq!(m.total_latency_ms(), 0);
        assert_eq!(m.total_errors(), 0);
        assert_eq!(m.max_latency_ms(), 0);
        assert_eq!(m.avg_latency_ms(), 0.0);
        assert_eq!(m.error_rate(), 0.0);
    }

    #[test]
    fn test_record_query() {
        let m = WasmRealDbMetrics::new();
        m.record_query(10);
        m.record_query(20);
        m.record_query(30);
        assert_eq!(m.total_queries(), 3);
        assert_eq!(m.total_latency_ms(), 60);
        assert_eq!(m.max_latency_ms(), 30);
        assert!((m.avg_latency_ms() - 20.0).abs() < 0.001);
    }

    #[test]
    fn test_record_error() {
        let m = WasmRealDbMetrics::new();
        m.record_error();
        m.record_error();
        assert_eq!(m.total_errors(), 2);
        assert!((m.error_rate() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_error_rate_mixed() {
        let m = WasmRealDbMetrics::new();
        m.record_query(10);
        m.record_query(20);
        m.record_error();
        assert_eq!(m.total_queries(), 2);
        assert_eq!(m.total_errors(), 1);
        assert!((m.error_rate() - (1.0 / 3.0)).abs() < 0.001);
    }

    #[test]
    fn test_max_latency() {
        let m = WasmRealDbMetrics::new();
        m.record_query(5);
        m.record_query(100);
        m.record_query(3);
        assert_eq!(m.max_latency_ms(), 100);
    }

    #[test]
    fn test_reset() {
        let m = WasmRealDbMetrics::new();
        m.record_query(10);
        m.record_error();
        m.reset();
        assert_eq!(m.total_queries(), 0);
        assert_eq!(m.total_errors(), 0);
        assert_eq!(m.total_latency_ms(), 0);
        assert_eq!(m.max_latency_ms(), 0);
    }

    #[test]
    fn test_default() {
        let m = WasmRealDbMetrics::default();
        assert_eq!(m.total_queries(), 0);
    }

    #[test]
    fn test_clone() {
        let m = WasmRealDbMetrics::new();
        m.record_query(42);
        m.record_error();
        let m2 = m.clone();
        assert_eq!(m2.total_queries(), 1);
        assert_eq!(m2.total_latency_ms(), 42);
        assert_eq!(m2.total_errors(), 1);
    }

    #[test]
    fn test_avg_latency_no_queries() {
        let m = WasmRealDbMetrics::new();
        assert_eq!(m.avg_latency_ms(), 0.0);
    }
}
