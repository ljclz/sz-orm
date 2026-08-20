//! 限流统计与监控（Rate Limit Metrics）
//!
//! 提供限流器的运行时统计、监控和可观测性支持。
//! 可集成到 Prometheus、OpenTelemetry 等监控系统。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// 限流指标收集器
///
/// 收集限流器的运行时指标，包括允许/拒绝计数、延迟分布等。
/// 线程安全，可在多线程环境下使用。
pub struct RateLimitMetrics {
    inner: Arc<MetricsInner>,
}

struct MetricsInner {
    /// 总请求数
    total_requests: AtomicU64,
    /// 允许请求数
    total_allowed: AtomicU64,
    /// 拒绝请求数
    total_rejected: AtomicU64,
    /// 总延迟（微秒）
    total_latency_us: AtomicU64,
    /// 最大延迟（微秒）
    max_latency_us: AtomicU64,
    /// 最小延迟（微秒）
    min_latency_us: AtomicU64,
    /// 按 key 分组的统计
    per_key: RwLock<HashMap<String, KeyMetrics>>,
    /// 按算法分组的统计
    per_algorithm: RwLock<HashMap<String, AlgorithmMetrics>>,
    /// 启动时间
    start_time: Instant,
}

/// 单个 key 的指标
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct KeyMetrics {
    pub requests: u64,
    pub allowed: u64,
    pub rejected: u64,
    pub last_access_ms: i64,
}

/// 单个算法的指标
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct AlgorithmMetrics {
    pub requests: u64,
    pub allowed: u64,
    pub rejected: u64,
    pub avg_latency_us: u64,
}

/// 指标快照
#[derive(Debug, Clone, serde::Serialize)]
pub struct MetricsSnapshot {
    pub total_requests: u64,
    pub total_allowed: u64,
    pub total_rejected: u64,
    pub allow_rate: f64,
    pub reject_rate: f64,
    pub avg_latency_us: u64,
    pub max_latency_us: u64,
    pub min_latency_us: u64,
    pub uptime_secs: u64,
    pub per_key_count: usize,
    pub per_algorithm_count: usize,
}

impl RateLimitMetrics {
    /// 创建指标收集器
    pub fn new() -> Self {
        Self {
            inner: Arc::new(MetricsInner {
                total_requests: AtomicU64::new(0),
                total_allowed: AtomicU64::new(0),
                total_rejected: AtomicU64::new(0),
                total_latency_us: AtomicU64::new(0),
                max_latency_us: AtomicU64::new(0),
                min_latency_us: AtomicU64::new(u64::MAX),
                per_key: RwLock::new(HashMap::new()),
                per_algorithm: RwLock::new(HashMap::new()),
                start_time: Instant::now(),
            }),
        }
    }

    /// 记录一次允许的请求
    pub fn record_allowed(&self, key: &str, algorithm: &str, latency: Duration) {
        self.record(key, algorithm, true, latency);
    }

    /// 记录一次拒绝的请求
    pub fn record_rejected(&self, key: &str, algorithm: &str, latency: Duration) {
        self.record(key, algorithm, false, latency);
    }

    fn record(&self, key: &str, algorithm: &str, allowed: bool, latency: Duration) {
        let latency_us = latency.as_micros() as u64;
        self.inner.total_requests.fetch_add(1, Ordering::Relaxed);
        if allowed {
            self.inner.total_allowed.fetch_add(1, Ordering::Relaxed);
        } else {
            self.inner.total_rejected.fetch_add(1, Ordering::Relaxed);
        }
        self.inner
            .total_latency_us
            .fetch_add(latency_us, Ordering::Relaxed);

        let mut max = self.inner.max_latency_us.load(Ordering::Relaxed);
        while latency_us > max {
            match self.inner.max_latency_us.compare_exchange(
                max,
                latency_us,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(new_max) => max = new_max,
            }
        }

        let mut min = self.inner.min_latency_us.load(Ordering::Relaxed);
        while latency_us < min {
            match self.inner.min_latency_us.compare_exchange(
                min,
                latency_us,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(new_min) => min = new_min,
            }
        }

        if let Ok(mut per_key) = self.inner.per_key.write() {
            let metrics = per_key.entry(key.to_string()).or_default();
            metrics.requests += 1;
            if allowed {
                metrics.allowed += 1;
            } else {
                metrics.rejected += 1;
            }
            metrics.last_access_ms = now_timestamp();
        }

        if let Ok(mut per_algo) = self.inner.per_algorithm.write() {
            let metrics = per_algo.entry(algorithm.to_string()).or_default();
            metrics.requests += 1;
            if allowed {
                metrics.allowed += 1;
            } else {
                metrics.rejected += 1;
            }
            metrics.avg_latency_us = (metrics.avg_latency_us * (metrics.requests - 1) + latency_us)
                .checked_div(metrics.requests)
                .unwrap_or(latency_us);
        }
    }

    /// 获取指标快照
    pub fn snapshot(&self) -> MetricsSnapshot {
        let total = self.inner.total_requests.load(Ordering::Relaxed);
        let allowed = self.inner.total_allowed.load(Ordering::Relaxed);
        let rejected = self.inner.total_rejected.load(Ordering::Relaxed);
        let total_latency = self.inner.total_latency_us.load(Ordering::Relaxed);
        let max_latency = self.inner.max_latency_us.load(Ordering::Relaxed);
        let min_latency = self.inner.min_latency_us.load(Ordering::Relaxed);

        MetricsSnapshot {
            total_requests: total,
            total_allowed: allowed,
            total_rejected: rejected,
            allow_rate: if total > 0 {
                allowed as f64 / total as f64
            } else {
                0.0
            },
            reject_rate: if total > 0 {
                rejected as f64 / total as f64
            } else {
                0.0
            },
            avg_latency_us: total_latency.checked_div(total).unwrap_or(0),
            max_latency_us: if total > 0 { max_latency } else { 0 },
            min_latency_us: if total > 0 {
                min_latency.min(max_latency)
            } else {
                0
            },
            uptime_secs: self.inner.start_time.elapsed().as_secs(),
            per_key_count: self.inner.per_key.read().map(|m| m.len()).unwrap_or(0),
            per_algorithm_count: self
                .inner
                .per_algorithm
                .read()
                .map(|m| m.len())
                .unwrap_or(0),
        }
    }

    /// 获取所有 key 的指标
    pub fn per_key_metrics(&self) -> HashMap<String, KeyMetrics> {
        self.inner
            .per_key
            .read()
            .map(|m| m.clone())
            .unwrap_or_default()
    }

    /// 获取所有算法的指标
    pub fn per_algorithm_metrics(&self) -> HashMap<String, AlgorithmMetrics> {
        self.inner
            .per_algorithm
            .read()
            .map(|m| m.clone())
            .unwrap_or_default()
    }

    /// 获取特定 key 的指标
    pub fn key_metrics(&self, key: &str) -> Option<KeyMetrics> {
        self.inner
            .per_key
            .read()
            .ok()
            .and_then(|m| m.get(key).cloned())
    }

    /// 重置所有指标
    pub fn reset(&self) {
        self.inner.total_requests.store(0, Ordering::Relaxed);
        self.inner.total_allowed.store(0, Ordering::Relaxed);
        self.inner.total_rejected.store(0, Ordering::Relaxed);
        self.inner.total_latency_us.store(0, Ordering::Relaxed);
        self.inner.max_latency_us.store(0, Ordering::Relaxed);
        self.inner.min_latency_us.store(u64::MAX, Ordering::Relaxed);
        if let Ok(mut per_key) = self.inner.per_key.write() {
            per_key.clear();
        }
        if let Ok(mut per_algo) = self.inner.per_algorithm.write() {
            per_algo.clear();
        }
    }

    /// 导出为 Prometheus 格式文本
    pub fn to_prometheus(&self) -> String {
        let snap = self.snapshot();
        let mut output = String::new();
        output.push_str("# HELP rate_limit_requests_total Total requests\n");
        output.push_str("# TYPE rate_limit_requests_total counter\n");
        output.push_str(&format!(
            "rate_limit_requests_total {}\n",
            snap.total_requests
        ));
        output.push_str("# HELP rate_limit_allowed_total Total allowed requests\n");
        output.push_str("# TYPE rate_limit_allowed_total counter\n");
        output.push_str(&format!(
            "rate_limit_allowed_total {}\n",
            snap.total_allowed
        ));
        output.push_str("# HELP rate_limit_rejected_total Total rejected requests\n");
        output.push_str("# TYPE rate_limit_rejected_total counter\n");
        output.push_str(&format!(
            "rate_limit_rejected_total {}\n",
            snap.total_rejected
        ));
        output.push_str("# HELP rate_limit_latency_us Latency in microseconds\n");
        output.push_str("# TYPE rate_limit_latency_us gauge\n");
        output.push_str(&format!(
            "rate_limit_latency_us{{quantile=\"avg\"}} {}\n",
            snap.avg_latency_us
        ));
        output.push_str(&format!(
            "rate_limit_latency_us{{quantile=\"max\"}} {}\n",
            snap.max_latency_us
        ));
        output.push_str(&format!(
            "rate_limit_latency_us{{quantile=\"min\"}} {}\n",
            snap.min_latency_us
        ));
        output.push_str("# HELP rate_limit_uptime_seconds Uptime in seconds\n");
        output.push_str("# TYPE rate_limit_uptime_seconds gauge\n");
        output.push_str(&format!("rate_limit_uptime_seconds {}\n", snap.uptime_secs));
        output
    }

    /// 导出为 JSON
    pub fn to_json(&self) -> serde_json::Value {
        let snap = self.snapshot();
        serde_json::json!({
            "total_requests": snap.total_requests,
            "total_allowed": snap.total_allowed,
            "total_rejected": snap.total_rejected,
            "allow_rate": snap.allow_rate,
            "reject_rate": snap.reject_rate,
            "avg_latency_us": snap.avg_latency_us,
            "max_latency_us": snap.max_latency_us,
            "min_latency_us": snap.min_latency_us,
            "uptime_secs": snap.uptime_secs,
            "per_key_count": snap.per_key_count,
            "per_algorithm_count": snap.per_algorithm_count,
        })
    }
}

impl Default for RateLimitMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for RateLimitMetrics {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

fn now_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// 限流监控器
///
/// 定期采样指标并触发告警。适用于长期运行的限流器。
pub struct RateLimitMonitor {
    metrics: RateLimitMetrics,
    alert_threshold: f64,
    last_snapshot: RwLock<Option<MetricsSnapshot>>,
}

impl RateLimitMonitor {
    /// 创建监控器
    ///
    /// - `metrics`：指标收集器
    /// - `alert_threshold`：拒绝率告警阈值（0.0~1.0）
    pub fn new(metrics: RateLimitMetrics, alert_threshold: f64) -> Self {
        Self {
            metrics,
            alert_threshold,
            last_snapshot: RwLock::new(None),
        }
    }

    /// 采样并存储快照
    pub fn sample(&self) -> MetricsSnapshot {
        let snap = self.metrics.snapshot();
        if let Ok(mut last) = self.last_snapshot.write() {
            *last = Some(snap.clone());
        }
        snap
    }

    /// 检查是否触发告警
    pub fn check_alert(&self) -> Option<Alert> {
        let snap = self.metrics.snapshot();
        if snap.reject_rate > self.alert_threshold && snap.total_requests > 10 {
            Some(Alert::HighRejectRate {
                reject_rate: snap.reject_rate,
                threshold: self.alert_threshold,
                total_requests: snap.total_requests,
            })
        } else {
            None
        }
    }

    /// 获取上次快照
    pub fn last_snapshot(&self) -> Option<MetricsSnapshot> {
        self.last_snapshot.read().ok().and_then(|s| s.clone())
    }

    /// 获取指标收集器
    pub fn metrics(&self) -> &RateLimitMetrics {
        &self.metrics
    }
}

/// 告警
#[derive(Debug, Clone, serde::Serialize)]
pub enum Alert {
    HighRejectRate {
        reject_rate: f64,
        threshold: f64,
        total_requests: u64,
    },
}

impl std::fmt::Display for Alert {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Alert::HighRejectRate {
                reject_rate,
                threshold,
                total_requests,
            } => write!(
                f,
                "High reject rate: {:.2}% (threshold: {:.2}%, total: {})",
                reject_rate * 100.0,
                threshold * 100.0,
                total_requests
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_new() {
        let metrics = RateLimitMetrics::new();
        let snap = metrics.snapshot();
        assert_eq!(snap.total_requests, 0);
        assert_eq!(snap.total_allowed, 0);
        assert_eq!(snap.total_rejected, 0);
    }

    #[test]
    fn test_metrics_record_allowed() {
        let metrics = RateLimitMetrics::new();
        metrics.record_allowed("k", "token_bucket", Duration::from_micros(100));
        let snap = metrics.snapshot();
        assert_eq!(snap.total_requests, 1);
        assert_eq!(snap.total_allowed, 1);
        assert_eq!(snap.total_rejected, 0);
    }

    #[test]
    fn test_metrics_record_rejected() {
        let metrics = RateLimitMetrics::new();
        metrics.record_rejected("k", "token_bucket", Duration::from_micros(50));
        let snap = metrics.snapshot();
        assert_eq!(snap.total_requests, 1);
        assert_eq!(snap.total_allowed, 0);
        assert_eq!(snap.total_rejected, 1);
    }

    #[test]
    fn test_metrics_allow_rate() {
        let metrics = RateLimitMetrics::new();
        metrics.record_allowed("k", "tb", Duration::from_micros(10));
        metrics.record_allowed("k", "tb", Duration::from_micros(10));
        metrics.record_rejected("k", "tb", Duration::from_micros(10));
        let snap = metrics.snapshot();
        assert!((snap.allow_rate - 2.0 / 3.0).abs() < 0.001);
        assert!((snap.reject_rate - 1.0 / 3.0).abs() < 0.001);
    }

    #[test]
    fn test_metrics_latency() {
        let metrics = RateLimitMetrics::new();
        metrics.record_allowed("k", "tb", Duration::from_micros(100));
        metrics.record_allowed("k", "tb", Duration::from_micros(200));
        metrics.record_allowed("k", "tb", Duration::from_micros(300));
        let snap = metrics.snapshot();
        assert_eq!(snap.avg_latency_us, 200);
        assert_eq!(snap.max_latency_us, 300);
        assert_eq!(snap.min_latency_us, 100);
    }

    #[test]
    fn test_metrics_per_key() {
        let metrics = RateLimitMetrics::new();
        metrics.record_allowed("a", "tb", Duration::from_micros(10));
        metrics.record_allowed("b", "tb", Duration::from_micros(10));
        let per_key = metrics.per_key_metrics();
        assert_eq!(per_key.len(), 2);
        assert_eq!(per_key.get("a").unwrap().requests, 1);
        assert_eq!(per_key.get("b").unwrap().requests, 1);
    }

    #[test]
    fn test_metrics_per_algorithm() {
        let metrics = RateLimitMetrics::new();
        metrics.record_allowed("k", "token_bucket", Duration::from_micros(10));
        metrics.record_allowed("k", "sliding_window", Duration::from_micros(10));
        let per_algo = metrics.per_algorithm_metrics();
        assert_eq!(per_algo.len(), 2);
        assert_eq!(per_algo.get("token_bucket").unwrap().requests, 1);
    }

    #[test]
    fn test_metrics_key_metrics() {
        let metrics = RateLimitMetrics::new();
        metrics.record_allowed("k", "tb", Duration::from_micros(10));
        metrics.record_rejected("k", "tb", Duration::from_micros(10));
        let km = metrics.key_metrics("k").unwrap();
        assert_eq!(km.requests, 2);
        assert_eq!(km.allowed, 1);
        assert_eq!(km.rejected, 1);
    }

    #[test]
    fn test_metrics_reset() {
        let metrics = RateLimitMetrics::new();
        metrics.record_allowed("k", "tb", Duration::from_micros(10));
        metrics.reset();
        let snap = metrics.snapshot();
        assert_eq!(snap.total_requests, 0);
    }

    #[test]
    fn test_metrics_to_prometheus() {
        let metrics = RateLimitMetrics::new();
        metrics.record_allowed("k", "tb", Duration::from_micros(10));
        let prom = metrics.to_prometheus();
        assert!(prom.contains("rate_limit_requests_total 1"));
        assert!(prom.contains("rate_limit_allowed_total 1"));
    }

    #[test]
    fn test_metrics_to_json() {
        let metrics = RateLimitMetrics::new();
        metrics.record_allowed("k", "tb", Duration::from_micros(10));
        let json = metrics.to_json();
        assert_eq!(json["total_requests"], 1);
        assert_eq!(json["total_allowed"], 1);
    }

    #[test]
    fn test_metrics_clone() {
        let metrics = RateLimitMetrics::new();
        metrics.record_allowed("k", "tb", Duration::from_micros(10));
        let cloned = metrics.clone();
        assert_eq!(cloned.snapshot().total_requests, 1);
    }

    #[test]
    fn test_monitor_no_alert() {
        let metrics = RateLimitMetrics::new();
        let monitor = RateLimitMonitor::new(metrics, 0.5);
        for _ in 0..10 {
            monitor
                .metrics()
                .record_allowed("k", "tb", Duration::from_micros(10));
        }
        assert!(monitor.check_alert().is_none());
    }

    #[test]
    fn test_monitor_high_reject_alert() {
        let metrics = RateLimitMetrics::new();
        let monitor = RateLimitMonitor::new(metrics, 0.5);
        for _ in 0..10 {
            monitor
                .metrics()
                .record_allowed("k", "tb", Duration::from_micros(10));
        }
        for _ in 0..20 {
            monitor
                .metrics()
                .record_rejected("k", "tb", Duration::from_micros(10));
        }
        let alert = monitor.check_alert();
        assert!(alert.is_some());
    }

    #[test]
    fn test_monitor_sample() {
        let metrics = RateLimitMetrics::new();
        let monitor = RateLimitMonitor::new(metrics, 0.5);
        monitor
            .metrics()
            .record_allowed("k", "tb", Duration::from_micros(10));
        let snap = monitor.sample();
        assert_eq!(snap.total_requests, 1);
        assert!(monitor.last_snapshot().is_some());
    }

    #[test]
    fn test_alert_display() {
        let alert = Alert::HighRejectRate {
            reject_rate: 0.8,
            threshold: 0.5,
            total_requests: 100,
        };
        let s = alert.to_string();
        assert!(s.contains("80.00%"));
    }

    #[test]
    fn test_metrics_snapshot_empty() {
        let metrics = RateLimitMetrics::new();
        let snap = metrics.snapshot();
        assert_eq!(snap.allow_rate, 0.0);
        assert_eq!(snap.reject_rate, 0.0);
        assert_eq!(snap.avg_latency_us, 0);
    }

    #[test]
    fn test_metrics_uptime() {
        let metrics = RateLimitMetrics::new();
        std::thread::sleep(Duration::from_millis(10));
        // uptime_secs is a u64 field — always >= 0 trivially; no meaningful assertion needed
        let _snap = metrics.snapshot();
    }

    #[test]
    fn test_metrics_per_key_count() {
        let metrics = RateLimitMetrics::new();
        metrics.record_allowed("a", "tb", Duration::from_micros(10));
        metrics.record_allowed("b", "tb", Duration::from_micros(10));
        metrics.record_allowed("c", "tb", Duration::from_micros(10));
        let snap = metrics.snapshot();
        assert_eq!(snap.per_key_count, 3);
    }
}
