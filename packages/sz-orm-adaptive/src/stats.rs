//! 查询统计采集与决策（原子操作，无锁）
//!
//! [`QueryStats`] 用 `AtomicU64` 维护累计执行次数/行数/耗时，
//! 提供平均值查询与决策阈值判断（`should_paginate` / `should_cache`）。
//! 统计采集开销 < 1μs/次（纯原子累加，无锁无分配）。

use std::sync::atomic::{AtomicU64, Ordering};

/// 单条查询的运行统计（按 query_key 维度）
#[derive(Debug, Default)]
pub struct QueryStats {
    total_executions: AtomicU64,
    total_rows: AtomicU64,
    total_time_us: AtomicU64,
}

impl QueryStats {
    /// 创建空统计
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录一次执行结果（行数 + 耗时微秒）
    pub fn record(&self, rows: u64, time_us: u64) {
        self.total_executions.fetch_add(1, Ordering::Relaxed);
        self.total_rows.fetch_add(rows, Ordering::Relaxed);
        self.total_time_us.fetch_add(time_us, Ordering::Relaxed);
    }

    /// 累计执行次数
    pub fn total_executions(&self) -> u64 {
        self.total_executions.load(Ordering::Relaxed)
    }

    /// 累计行数
    pub fn total_rows(&self) -> u64 {
        self.total_rows.load(Ordering::Relaxed)
    }

    /// 累计耗时（微秒）
    pub fn total_time_us(&self) -> u64 {
        self.total_time_us.load(Ordering::Relaxed)
    }

    /// 平均返回行数（未执行过返回 0）
    pub fn avg_rows(&self) -> f64 {
        let n = self.total_executions();
        if n == 0 {
            0.0
        } else {
            self.total_rows() as f64 / n as f64
        }
    }

    /// 平均耗时（毫秒）
    pub fn avg_time_ms(&self) -> f64 {
        let n = self.total_executions();
        if n == 0 {
            0.0
        } else {
            self.total_time_us() as f64 / n as f64 / 1000.0
        }
    }

    /// 大结果集判断：平均行数超过阈值 → 建议切换游标分页
    pub fn should_paginate(&self, threshold_rows: u64) -> bool {
        self.total_executions() > 0 && self.avg_rows() > threshold_rows as f64
    }

    /// 热点查询判断：平均耗时超阈值 **且** 执行次数达到采样下限 → 建议缓存
    ///
    /// 两个条件同时满足，避免对冷查询做缓存决策。
    pub fn should_cache(&self, threshold_ms: u64, min_executions: u64) -> bool {
        self.total_executions() >= min_executions && self.avg_time_ms() > threshold_ms as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_averages() {
        let s = QueryStats::new();
        s.record(10, 1_000);
        s.record(20, 3_000);
        assert_eq!(s.total_executions(), 2);
        assert_eq!(s.total_rows(), 30);
        assert!((s.avg_rows() - 15.0).abs() < 1e-9);
        assert!((s.avg_time_ms() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn fresh_stats_no_decisions() {
        let s = QueryStats::new();
        assert!(!s.should_paginate(1000));
        assert!(!s.should_cache(100, 10));
        assert_eq!(s.avg_rows(), 0.0);
    }

    #[test]
    fn paginate_flips_on_high_rows() {
        let s = QueryStats::new();
        s.record(2000, 1_000);
        assert!(s.should_paginate(1000));
        assert!(!s.should_paginate(5000));
    }

    #[test]
    fn cache_requires_both_conditions() {
        let s = QueryStats::new();
        // 只慢但执行次数不足
        s.record(1, 500_000);
        assert!(!s.should_cache(100, 10));
        // 次数达标但不够慢
        for _ in 0..10 {
            s.record(1, 1);
        }
        assert!(!s.should_cache(100, 10));
        // 又慢又高频
        for _ in 0..10 {
            s.record(1, 500_000);
        }
        assert!(s.should_cache(100, 10));
    }

    #[test]
    fn concurrent_records_are_safe() {
        let s = std::sync::Arc::new(QueryStats::new());
        let mut handles = Vec::new();
        for _ in 0..8 {
            let s = std::sync::Arc::clone(&s);
            handles.push(std::thread::spawn(move || {
                for _ in 0..100 {
                    s.record(1, 1);
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(s.total_executions(), 800);
        assert_eq!(s.total_rows(), 800);
    }
}
