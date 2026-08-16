//! 查询统计采集与决策（原子操作，无锁）
//!
//! [`QueryStats`] 用 `AtomicU64` 维护累计执行次数/行数/耗时，
//! 提供平均值查询与决策阈值判断（`should_paginate` / `should_cache`）。
//! 统计采集开销 < 1μs/次（纯原子累加，无锁无分配）。

use std::collections::VecDeque;
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

/// 滑动窗口统计：保留最近 N 个样本，提供均值/中位数/P95 百分位
///
/// 适用于对时间序列做短窗口统计（如最近 100 次查询的延迟分布），
/// 旧样本自动淘汰，无需全量历史。
#[derive(Debug, Clone)]
pub struct SlidingWindowStats {
    window_size: usize,
    samples: VecDeque<u64>,
}

impl SlidingWindowStats {
    /// 创建指定窗口大小的统计器
    pub fn new(window_size: usize) -> Self {
        Self {
            window_size,
            samples: VecDeque::with_capacity(window_size),
        }
    }

    /// 添加一个样本（窗口满时淘汰最旧样本）
    pub fn push(&mut self, value: u64) {
        if self.samples.len() >= self.window_size {
            self.samples.pop_front();
        }
        self.samples.push_back(value);
    }

    /// 当前样本数
    pub fn count(&self) -> usize {
        self.samples.len()
    }

    /// 窗口是否已满
    pub fn is_full(&self) -> bool {
        self.samples.len() >= self.window_size
    }

    /// 均值（空窗口返回 0.0）
    pub fn mean(&self) -> f64 {
        if self.samples.is_empty() {
            0.0
        } else {
            let sum: u64 = self.samples.iter().sum();
            sum as f64 / self.samples.len() as f64
        }
    }

    /// 中位数（空窗口返回 0.0）
    pub fn median(&self) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let mut sorted: Vec<u64> = self.samples.iter().copied().collect();
        sorted.sort_unstable();
        let n = sorted.len();
        if n % 2 == 1 {
            sorted[n / 2] as f64
        } else {
            (sorted[n / 2 - 1] + sorted[n / 2]) as f64 / 2.0
        }
    }

    /// P95 百分位（空窗口返回 0.0）
    pub fn p95(&self) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let mut sorted: Vec<u64> = self.samples.iter().copied().collect();
        sorted.sort_unstable();
        let idx = ((sorted.len() as f64 - 1.0) * 0.95).ceil() as usize;
        sorted[idx.min(sorted.len() - 1)] as f64
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

    #[test]
    fn zero_row_record_handled() {
        let s = QueryStats::new();
        s.record(0, 100);
        assert_eq!(s.total_rows(), 0);
        assert_eq!(s.avg_rows(), 0.0);
    }

    #[test]
    fn cache_decision_requires_min_executions() {
        // 少量执行不应触发缓存建议（样本不足）
        let s = QueryStats::new();
        s.record(50, 100);
        assert!(!s.should_cache(50, 10), "one sample is not enough");
    }

    #[test]
    fn paginate_decision_threshold() {
        let s = QueryStats::new();
        for _ in 0..10 {
            s.record(5000, 100);
        }
        // avg_rows = 5000：超过 1000 阈值 → 建议分页；低于 10000 阈值 → 不建议
        assert!(s.should_paginate(1000), "avg 5000 > 1000 should paginate");
        assert!(!s.should_paginate(10000), "avg 5000 < 10000 should not");
    }

    #[test]
    fn empty_stats_zero_avg_time() {
        let s = QueryStats::new();
        assert_eq!(s.avg_time_ms(), 0.0);
    }

    // --- SlidingWindowStats tests ---

    #[test]
    fn sliding_window_new_empty() {
        let sw = SlidingWindowStats::new(10);
        assert_eq!(sw.count(), 0);
        assert!(!sw.is_full());
    }

    #[test]
    fn sliding_window_push_increments_count() {
        let mut sw = SlidingWindowStats::new(10);
        sw.push(1);
        sw.push(2);
        assert_eq!(sw.count(), 2);
    }

    #[test]
    fn sliding_window_mean_single() {
        let mut sw = SlidingWindowStats::new(10);
        sw.push(42);
        assert!((sw.mean() - 42.0).abs() < 1e-9);
    }

    #[test]
    fn sliding_window_mean_multiple() {
        let mut sw = SlidingWindowStats::new(10);
        for v in [10, 20, 30] {
            sw.push(v);
        }
        assert!((sw.mean() - 20.0).abs() < 1e-9);
    }

    #[test]
    fn sliding_window_mean_empty_returns_zero() {
        let sw = SlidingWindowStats::new(10);
        assert_eq!(sw.mean(), 0.0);
    }

    #[test]
    fn sliding_window_median_odd() {
        let mut sw = SlidingWindowStats::new(10);
        for v in [3, 1, 2] {
            sw.push(v);
        }
        assert!((sw.median() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn sliding_window_median_even() {
        let mut sw = SlidingWindowStats::new(10);
        for v in [1, 2, 3, 4] {
            sw.push(v);
        }
        assert!((sw.median() - 2.5).abs() < 1e-9);
    }

    #[test]
    fn sliding_window_median_empty() {
        let sw = SlidingWindowStats::new(10);
        assert_eq!(sw.median(), 0.0);
    }

    #[test]
    fn sliding_window_p95_basic() {
        let mut sw = SlidingWindowStats::new(100);
        for v in 1..=100u64 {
            sw.push(v);
        }
        let p = sw.p95();
        assert!((94.0..=96.0).contains(&p), "p95={p} should be in [94, 96]");
    }

    #[test]
    fn sliding_window_p95_empty() {
        let sw = SlidingWindowStats::new(10);
        assert_eq!(sw.p95(), 0.0);
    }

    #[test]
    fn sliding_window_is_full() {
        let mut sw = SlidingWindowStats::new(3);
        sw.push(1);
        sw.push(2);
        assert!(!sw.is_full());
        sw.push(3);
        assert!(sw.is_full());
    }

    #[test]
    fn sliding_window_evicts_oldest() {
        let mut sw = SlidingWindowStats::new(3);
        sw.push(10);
        sw.push(20);
        sw.push(30);
        sw.push(40);
        assert_eq!(sw.count(), 3);
        assert!((sw.mean() - 30.0).abs() < 1e-9, "mean of [20,30,40] = 30");
    }

    #[test]
    fn sliding_window_push_many_and_median() {
        let mut sw = SlidingWindowStats::new(5);
        for v in [5, 1, 4, 2, 3] {
            sw.push(v);
        }
        assert!((sw.median() - 3.0).abs() < 1e-9);
    }
}
