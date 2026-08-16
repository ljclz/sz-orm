//! gRPC 调用指标采集与监控
//!
//! 跟踪每个服务方法的调用次数、延迟分布、错误率，
//! 提供运行时指标查询和健康评估。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

/// 单个方法的指标快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodMetrics {
    /// 方法全名（service/method）
    pub method: String,
    /// 总调用次数
    pub total_calls: u64,
    /// 成功调用次数
    pub success_calls: u64,
    /// 失败调用次数
    pub failure_calls: u64,
    /// 最小延迟（纳秒）
    pub min_latency_ns: u64,
    /// 最大延迟（纳秒）
    pub max_latency_ns: u64,
    /// 累计延迟（纳秒）
    pub sum_latency_ns: u128,
    /// 最后一次调用的时间戳（从监控器创建起的秒数）
    pub last_call_secs: u64,
}

impl MethodMetrics {
    /// 创建空指标
    pub fn new(method: &str) -> Self {
        Self {
            method: method.to_string(),
            total_calls: 0,
            success_calls: 0,
            failure_calls: 0,
            min_latency_ns: 0,
            max_latency_ns: 0,
            sum_latency_ns: 0,
            last_call_secs: 0,
        }
    }

    /// 记录一次调用
    pub fn record(&mut self, latency: Duration, success: bool, now_secs: u64) {
        let ns = latency.as_nanos();
        self.total_calls += 1;
        if success {
            self.success_calls += 1;
        } else {
            self.failure_calls += 1;
        }
        if self.min_latency_ns == 0 || ns < self.min_latency_ns as u128 {
            self.min_latency_ns = ns.min(u64::MAX as u128) as u64;
        }
        if ns > self.max_latency_ns as u128 {
            self.max_latency_ns = ns.min(u64::MAX as u128) as u64;
        }
        self.sum_latency_ns = self.sum_latency_ns.saturating_add(ns);
        self.last_call_secs = now_secs;
    }

    /// 平均延迟（纳秒）
    pub fn avg_latency_ns(&self) -> u64 {
        if self.total_calls == 0 {
            0
        } else {
            (self.sum_latency_ns / self.total_calls as u128) as u64
        }
    }

    /// 错误率（0.0-1.0）
    pub fn error_rate(&self) -> f64 {
        if self.total_calls == 0 {
            0.0
        } else {
            self.failure_calls as f64 / self.total_calls as f64
        }
    }

    /// 是否健康（错误率低于阈值且平均延迟低于阈值）
    pub fn is_healthy(&self, max_error_rate: f64, max_latency: Duration) -> bool {
        self.error_rate() <= max_error_rate
            && (self.avg_latency_ns() <= max_latency.as_nanos() as u64 || self.total_calls == 0)
    }
}

/// gRPC 指标监控器
pub struct GrpcMetricsMonitor {
    metrics: Mutex<HashMap<String, MethodMetrics>>,
    /// 监控器创建时间（用于计算相对时间戳）
    started: std::time::Instant,
    /// 全局总请求数
    global_total: AtomicU64,
    /// 全局失败请求数
    global_failures: AtomicU64,
}

impl Default for GrpcMetricsMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl GrpcMetricsMonitor {
    /// 创建监控器
    pub fn new() -> Self {
        Self {
            metrics: Mutex::new(HashMap::new()),
            started: std::time::Instant::now(),
            global_total: AtomicU64::new(0),
            global_failures: AtomicU64::new(0),
        }
    }

    /// 记录一次方法调用
    pub fn record_call(&self, method: &str, latency: Duration, success: bool) {
        let now = self.started.elapsed().as_secs();
        self.global_total.fetch_add(1, Ordering::Relaxed);
        if !success {
            self.global_failures.fetch_add(1, Ordering::Relaxed);
        }
        if let Ok(mut metrics) = self.metrics.lock() {
            let m = metrics
                .entry(method.to_string())
                .or_insert_with(|| MethodMetrics::new(method));
            m.record(latency, success, now);
        }
    }

    /// 获取方法的指标快照
    pub fn metrics(&self, method: &str) -> Option<MethodMetrics> {
        match self.metrics.lock() {
            Ok(metrics) => metrics.get(method).cloned(),
            Err(_) => None,
        }
    }

    /// 获取所有方法的指标快照
    pub fn all_metrics(&self) -> Vec<MethodMetrics> {
        match self.metrics.lock() {
            Ok(metrics) => metrics.values().cloned().collect(),
            Err(_) => Vec::new(),
        }
    }

    /// 全局总请求数
    pub fn global_total(&self) -> u64 {
        self.global_total.load(Ordering::Relaxed)
    }

    /// 全局失败请求数
    pub fn global_failures(&self) -> u64 {
        self.global_failures.load(Ordering::Relaxed)
    }

    /// 全局错误率
    pub fn global_error_rate(&self) -> f64 {
        let total = self.global_total();
        if total == 0 {
            0.0
        } else {
            self.global_failures() as f64 / total as f64
        }
    }

    /// 列出所有错误率超过阈值的方法
    pub fn unhealthy_methods(&self, max_error_rate: f64) -> Vec<String> {
        match self.metrics.lock() {
            Ok(metrics) => metrics
                .values()
                .filter(|m| m.error_rate() > max_error_rate)
                .map(|m| m.method.clone())
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// 列出所有平均延迟超过阈值的方法
    pub fn slow_methods(&self, max_latency: Duration) -> Vec<String> {
        let max_ns = max_latency.as_nanos() as u64;
        match self.metrics.lock() {
            Ok(metrics) => metrics
                .values()
                .filter(|m| m.avg_latency_ns() > max_ns)
                .map(|m| m.method.clone())
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// 重置指定方法的指标
    pub fn reset(&self, method: &str) {
        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.remove(method);
        }
    }

    /// 重置所有指标
    pub fn reset_all(&self) {
        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.clear();
        }
        self.global_total.store(0, Ordering::Relaxed);
        self.global_failures.store(0, Ordering::Relaxed);
    }

    /// 生成指标摘要字符串
    pub fn summary(&self) -> String {
        let all = self.all_metrics();
        let mut out = format!(
            "gRPC Metrics Summary: {} method(s), {} total call(s), {:.1}% error rate\n",
            all.len(),
            self.global_total(),
            self.global_error_rate() * 100.0
        );
        let mut sorted = all;
        sorted.sort_by_key(|m| std::cmp::Reverse(m.total_calls));
        for m in &sorted {
            out.push_str(&format!(
                "  {}: {} calls, {:.1}% errors, avg {}ns\n",
                m.method,
                m.total_calls,
                m.error_rate() * 100.0,
                m.avg_latency_ns()
            ));
        }
        out
    }
}

/// 延迟直方图（固定桶）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyHistogram {
    /// 桶边界（纳秒）
    pub buckets: Vec<u64>,
    /// 各桶计数
    pub counts: Vec<u64>,
    /// 未落入任何桶的计数
    pub overflow_count: u64,
}

impl LatencyHistogram {
    /// 创建默认直方图（1us, 10us, 100us, 1ms, 10ms, 100ms, 1s）
    pub fn default_buckets() -> Self {
        let buckets = vec![
            1_000,
            10_000,
            100_000,
            1_000_000,
            10_000_000,
            100_000_000,
            1_000_000_000,
        ];
        let counts = vec![0; buckets.len()];
        Self {
            buckets,
            counts,
            overflow_count: 0,
        }
    }

    /// 记录一个延迟值
    pub fn record(&mut self, latency_ns: u64) {
        for (i, &bound) in self.buckets.iter().enumerate() {
            if latency_ns <= bound {
                self.counts[i] += 1;
                return;
            }
        }
        self.overflow_count += 1;
    }

    /// 总样本数
    pub fn total(&self) -> u64 {
        self.counts.iter().sum::<u64>() + self.overflow_count
    }

    /// 百分位数（p50, p90, p99 等）
    ///
    /// `percentile` 范围 0.0-1.0
    pub fn percentile(&self, percentile: f64) -> u64 {
        let total = self.total();
        if total == 0 {
            return 0;
        }
        let target = (total as f64 * percentile).ceil() as u64;
        let mut cumulative = 0u64;
        for (i, &count) in self.counts.iter().enumerate() {
            cumulative += count;
            if cumulative >= target {
                return self.buckets[i];
            }
        }
        self.buckets.last().copied().unwrap_or(0)
    }

    /// 中位数（p50）
    pub fn median(&self) -> u64 {
        self.percentile(0.5)
    }

    /// p90
    pub fn p90(&self) -> u64 {
        self.percentile(0.9)
    }

    /// p99
    pub fn p99(&self) -> u64 {
        self.percentile(0.99)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_method_metrics_new() {
        let m = MethodMetrics::new("svc/method");
        assert_eq!(m.method, "svc/method");
        assert_eq!(m.total_calls, 0);
        assert_eq!(m.error_rate(), 0.0);
    }

    #[test]
    fn test_method_metrics_record_success() {
        let mut m = MethodMetrics::new("m");
        m.record(Duration::from_millis(10), true, 0);
        assert_eq!(m.total_calls, 1);
        assert_eq!(m.success_calls, 1);
        assert_eq!(m.failure_calls, 0);
        assert_eq!(m.error_rate(), 0.0);
    }

    #[test]
    fn test_method_metrics_record_failure() {
        let mut m = MethodMetrics::new("m");
        m.record(Duration::from_millis(10), false, 0);
        assert_eq!(m.total_calls, 1);
        assert_eq!(m.failure_calls, 1);
        assert_eq!(m.error_rate(), 1.0);
    }

    #[test]
    fn test_method_metrics_avg_latency() {
        let mut m = MethodMetrics::new("m");
        m.record(Duration::from_millis(10), true, 0);
        m.record(Duration::from_millis(20), true, 0);
        m.record(Duration::from_millis(30), true, 0);
        let avg = m.avg_latency_ns();
        assert!((19_000_000..=21_000_000).contains(&avg));
    }

    #[test]
    fn test_method_metrics_error_rate_mixed() {
        let mut m = MethodMetrics::new("m");
        for _ in 0..7 {
            m.record(Duration::from_millis(1), true, 0);
        }
        for _ in 0..3 {
            m.record(Duration::from_millis(1), false, 0);
        }
        let rate = m.error_rate();
        assert!(
            (0.29..=0.31).contains(&rate),
            "error rate should be ~0.3, got {rate}"
        );
    }

    #[test]
    fn test_method_metrics_is_healthy() {
        let mut m = MethodMetrics::new("m");
        m.record(Duration::from_millis(10), true, 0);
        assert!(m.is_healthy(0.1, Duration::from_millis(100)));
        m.record(Duration::from_millis(10), false, 0);
        assert!(!m.is_healthy(0.0, Duration::from_millis(100)));
        assert!(!m.is_healthy(0.1, Duration::from_millis(5)));
    }

    #[test]
    fn test_monitor_record_and_query() {
        let mon = GrpcMetricsMonitor::new();
        mon.record_call("svc/a", Duration::from_millis(5), true);
        mon.record_call("svc/a", Duration::from_millis(15), true);
        mon.record_call("svc/a", Duration::from_millis(10), false);
        let m = mon.metrics("svc/a").unwrap();
        assert_eq!(m.total_calls, 3);
        assert_eq!(m.success_calls, 2);
        assert_eq!(m.failure_calls, 1);
    }

    #[test]
    fn test_monitor_global_counts() {
        let mon = GrpcMetricsMonitor::new();
        mon.record_call("a", Duration::from_millis(1), true);
        mon.record_call("b", Duration::from_millis(1), false);
        mon.record_call("c", Duration::from_millis(1), true);
        assert_eq!(mon.global_total(), 3);
        assert_eq!(mon.global_failures(), 1);
    }

    #[test]
    fn test_monitor_global_error_rate() {
        let mon = GrpcMetricsMonitor::new();
        for _ in 0..8 {
            mon.record_call("a", Duration::from_millis(1), true);
        }
        for _ in 0..2 {
            mon.record_call("a", Duration::from_millis(1), false);
        }
        let rate = mon.global_error_rate();
        assert!(
            (0.19..=0.21).contains(&rate),
            "global error rate should be ~0.2, got {rate}"
        );
    }

    #[test]
    fn test_monitor_unhealthy_methods() {
        let mon = GrpcMetricsMonitor::new();
        for _ in 0..5 {
            mon.record_call("good", Duration::from_millis(1), true);
        }
        for _ in 0..5 {
            mon.record_call("bad", Duration::from_millis(1), false);
        }
        let unhealthy = mon.unhealthy_methods(0.5);
        assert_eq!(unhealthy, vec!["bad".to_string()]);
    }

    #[test]
    fn test_monitor_slow_methods() {
        let mon = GrpcMetricsMonitor::new();
        mon.record_call("fast", Duration::from_millis(1), true);
        mon.record_call("slow", Duration::from_millis(100), true);
        let slow = mon.slow_methods(Duration::from_millis(50));
        assert_eq!(slow, vec!["slow".to_string()]);
    }

    #[test]
    fn test_monitor_reset() {
        let mon = GrpcMetricsMonitor::new();
        mon.record_call("a", Duration::from_millis(1), true);
        mon.reset("a");
        assert!(mon.metrics("a").is_none());
    }

    #[test]
    fn test_monitor_reset_all() {
        let mon = GrpcMetricsMonitor::new();
        mon.record_call("a", Duration::from_millis(1), true);
        mon.record_call("b", Duration::from_millis(1), false);
        mon.reset_all();
        assert_eq!(mon.global_total(), 0);
        assert_eq!(mon.all_metrics().len(), 0);
    }

    #[test]
    fn test_monitor_summary() {
        let mon = GrpcMetricsMonitor::new();
        mon.record_call("a", Duration::from_millis(1), true);
        let s = mon.summary();
        assert!(s.contains("gRPC Metrics Summary"));
        assert!(s.contains("a"));
    }

    #[test]
    fn test_histogram_default_buckets() {
        let h = LatencyHistogram::default_buckets();
        assert_eq!(h.buckets.len(), 7);
        assert_eq!(h.counts.len(), 7);
        assert_eq!(h.total(), 0);
    }

    #[test]
    fn test_histogram_record() {
        let mut h = LatencyHistogram::default_buckets();
        h.record(500);
        h.record(5_000);
        h.record(50_000);
        h.record(500_000);
        assert_eq!(h.total(), 4);
    }

    #[test]
    fn test_histogram_overflow() {
        let mut h = LatencyHistogram::default_buckets();
        h.record(2_000_000_000);
        assert_eq!(h.overflow_count, 1);
        assert_eq!(h.total(), 1);
    }

    #[test]
    fn test_histogram_percentile() {
        let mut h = LatencyHistogram::default_buckets();
        for _ in 0..50 {
            h.record(500);
        }
        for _ in 0..40 {
            h.record(5_000);
        }
        for _ in 0..10 {
            h.record(50_000);
        }
        assert_eq!(h.total(), 100);
        let p50 = h.percentile(0.5);
        assert!(p50 <= 1_000, "p50 should be in first bucket, got {p50}");
        let p90 = h.p90();
        assert!(p90 <= 10_000, "p90 should be in second bucket, got {p90}");
    }

    #[test]
    fn test_histogram_median_p90_p99() {
        let mut h = LatencyHistogram::default_buckets();
        for _ in 0..100 {
            h.record(500);
        }
        assert_eq!(h.median(), 1_000);
        assert_eq!(h.p90(), 1_000);
        assert_eq!(h.p99(), 1_000);
    }

    #[test]
    fn test_histogram_empty_percentile() {
        let h = LatencyHistogram::default_buckets();
        assert_eq!(h.median(), 0);
        assert_eq!(h.p90(), 0);
        assert_eq!(h.p99(), 0);
    }
}
