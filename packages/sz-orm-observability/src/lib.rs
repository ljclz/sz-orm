//! SZ-ORM 可观测性模块
//!
//! 提供 Prometheus exporter、OTLP exporter、SLO 燃烧率监控等能力，
//! 与 `sz-orm-tracing` 配合形成完整的可观测性闭环。
//!
//! # 核心能力
//!
//! ## 1. MetricsRegistry（默认启用）
//!
//! 统一的指标注册中心，支持 Counter / Gauge / Histogram 三种类型，
//! 内置线程安全（`RwLock`），可通过 `render()` 输出 Prometheus 文本格式。
//!
//! ## 2. Prometheus exporter（feature = "prometheus"）
//!
//! 在指定端口暴露 `/metrics` HTTP 端点，供 Prometheus 拉取。
//!
//! ## 3. OTLP exporter（feature = "otlp"）
//!
//! 通过 OpenTelemetry OTLP 协议将 traces 导出到 Collector。
//!
//! ## 4. SLO 燃烧率
//!
//! 基于 5m / 1h 两个窗口计算 SLO 燃烧率，支持多窗口告警。
//!
//! # 快速入门
//!
//! ```no_run
//! use sz_orm_observability::{MetricsRegistry, MetricKind};
//! use std::time::Duration;
//!
//! // 创建指标注册中心
//! let registry = MetricsRegistry::new();
//!
//! // 注册指标
//! let counter = registry.register_counter("sz_orm_pool_acquires_total", "Total pool acquire calls");
//! let gauge = registry.register_gauge("sz_orm_pool_active_connections", "Current active connections");
//! let histogram = registry.register_histogram(
//!     "sz_orm_query_duration_seconds",
//!     "Query duration in seconds",
//!     vec![0.001, 0.01, 0.1, 1.0, 10.0],
//! );
//!
//! // 更新指标
//! counter.inc();
//! gauge.set(5.0);
//! histogram.observe(0.025);
//!
//! // 输出 Prometheus 文本格式
//! let output = registry.render();
//! println!("{}", output);
//! ```

#![warn(missing_docs)]

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

pub mod slo;

pub use slo::{SloBurnRate, SloConfig, SloMonitor};

/// 指标类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricKind {
    /// 单调递增计数器（如总请求数）
    Counter,
    /// 可增可减的瞬时值（如当前连接数）
    Gauge,
    /// 直方图（如请求延迟分布）
    Histogram,
}

/// 指标元数据
#[derive(Debug, Clone)]
pub struct MetricMeta {
    /// 指标名（如 `sz_orm_pool_acquires_total`）
    pub name: String,
    /// 帮助文本
    pub help: String,
    /// 指标类型
    pub kind: MetricKind,
}

/// 计数器（单调递增）
pub struct Counter {
    name: String,
    value: Arc<RwLock<f64>>,
    labels: HashMap<String, String>,
}

impl Counter {
    /// 递增 1
    pub fn inc(&self) {
        self.inc_by(1.0);
    }

    /// 递增指定值
    pub fn inc_by(&self, delta: f64) {
        let mut v = self.value.write();
        *v += delta;
    }

    /// 当前值
    pub fn value(&self) -> f64 {
        *self.value.read()
    }

    /// 指标名
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 渲染为 Prometheus 文本格式
    pub fn render(&self) -> String {
        let v = self.value.read();
        if self.labels.is_empty() {
            format!("{} {}\n", self.name, v)
        } else {
            let labels: Vec<String> = self
                .labels
                .iter()
                .map(|(k, val)| format!("{}=\"{}\"", k, val.replace('"', "\\\"")))
                .collect();
            format!("{}{{{}}} {}\n", self.name, labels.join(","), v)
        }
    }
}

/// Gauge（可增可减）
pub struct Gauge {
    name: String,
    value: Arc<RwLock<f64>>,
    labels: HashMap<String, String>,
}

impl Gauge {
    /// 设置值
    pub fn set(&self, value: f64) {
        *self.value.write() = value;
    }

    /// 递增
    pub fn inc(&self) {
        self.inc_by(1.0);
    }

    /// 递增指定值
    pub fn inc_by(&self, delta: f64) {
        let mut v = self.value.write();
        *v += delta;
    }

    /// 递减指定值
    pub fn dec_by(&self, delta: f64) {
        let mut v = self.value.write();
        *v -= delta;
    }

    /// 当前值
    pub fn value(&self) -> f64 {
        *self.value.read()
    }

    /// 指标名
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 渲染为 Prometheus 文本格式
    pub fn render(&self) -> String {
        let v = self.value.read();
        if self.labels.is_empty() {
            format!("{} {}\n", self.name, v)
        } else {
            let labels: Vec<String> = self
                .labels
                .iter()
                .map(|(k, val)| format!("{}=\"{}\"", k, val.replace('"', "\\\"")))
                .collect();
            format!("{}{{{}}} {}\n", self.name, labels.join(","), v)
        }
    }
}

/// 直方图（延迟分布等）
pub struct Histogram {
    name: String,
    buckets: Vec<f64>,
    counts: Arc<RwLock<Vec<u64>>>,
    sum: Arc<RwLock<f64>>,
    count: Arc<RwLock<u64>>,
}

impl Histogram {
    /// 观察一个值
    pub fn observe(&self, value: f64) {
        let mut counts = self.counts.write();
        for (i, bucket) in self.buckets.iter().enumerate() {
            if value <= *bucket {
                counts[i] += 1;
            }
        }
        // 最后一个 bucket 是 +Inf，必须递增
        let last = counts.len() - 1;
        counts[last] += 1;

        let mut sum = self.sum.write();
        *sum += value;
        let mut count = self.count.write();
        *count += 1;
    }

    /// 总观察次数
    pub fn count(&self) -> u64 {
        *self.count.read()
    }

    /// 所有观察值之和
    pub fn sum(&self) -> f64 {
        *self.sum.read()
    }

    /// 指标名
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 渲染为 Prometheus 文本格式
    pub fn render(&self) -> String {
        let counts = self.counts.read();
        let sum = self.sum.read();
        let count = self.count.read();

        let mut output = String::new();
        for (i, bucket) in self.buckets.iter().enumerate() {
            output.push_str(&format!(
                "{}_bucket{{le=\"{}\"}} {}\n",
                self.name, bucket, counts[i]
            ));
        }
        output.push_str(&format!("{}_sum {}\n", self.name, sum));
        output.push_str(&format!("{}_count {}\n", self.name, count));
        output
    }
}

/// 指标注册中心
pub struct MetricsRegistry {
    counters: RwLock<HashMap<String, Arc<Counter>>>,
    gauges: RwLock<HashMap<String, Arc<Gauge>>>,
    histograms: RwLock<HashMap<String, Arc<Histogram>>>,
    metas: RwLock<Vec<MetricMeta>>,
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsRegistry {
    /// 创建空注册中心
    pub fn new() -> Self {
        Self {
            counters: RwLock::new(HashMap::new()),
            gauges: RwLock::new(HashMap::new()),
            histograms: RwLock::new(HashMap::new()),
            metas: RwLock::new(Vec::new()),
        }
    }

    /// 注册 Counter
    pub fn register_counter(&self, name: &str, help: &str) -> Arc<Counter> {
        self.register_counter_with_labels(name, help, HashMap::new())
    }

    /// 注册带标签的 Counter
    pub fn register_counter_with_labels(
        &self,
        name: &str,
        help: &str,
        labels: HashMap<String, String>,
    ) -> Arc<Counter> {
        let mut counters = self.counters.write();
        let key = format!("{}_{:?}", name, labels);
        if let Some(c) = counters.get(&key) {
            return c.clone();
        }
        let counter = Arc::new(Counter {
            name: name.to_string(),
            value: Arc::new(RwLock::new(0.0)),
            labels,
        });
        counters.insert(key, counter.clone());

        let mut metas = self.metas.write();
        metas.push(MetricMeta {
            name: name.to_string(),
            help: help.to_string(),
            kind: MetricKind::Counter,
        });
        counter
    }

    /// 注册 Gauge
    pub fn register_gauge(&self, name: &str, help: &str) -> Arc<Gauge> {
        self.register_gauge_with_labels(name, help, HashMap::new())
    }

    /// 注册带标签的 Gauge
    pub fn register_gauge_with_labels(
        &self,
        name: &str,
        help: &str,
        labels: HashMap<String, String>,
    ) -> Arc<Gauge> {
        let mut gauges = self.gauges.write();
        let key = format!("{}_{:?}", name, labels);
        if let Some(g) = gauges.get(&key) {
            return g.clone();
        }
        let gauge = Arc::new(Gauge {
            name: name.to_string(),
            value: Arc::new(RwLock::new(0.0)),
            labels,
        });
        gauges.insert(key, gauge.clone());

        let mut metas = self.metas.write();
        metas.push(MetricMeta {
            name: name.to_string(),
            help: help.to_string(),
            kind: MetricKind::Gauge,
        });
        gauge
    }

    /// 注册 Histogram
    pub fn register_histogram(&self, name: &str, help: &str, buckets: Vec<f64>) -> Arc<Histogram> {
        let mut histograms = self.histograms.write();
        if let Some(h) = histograms.get(name) {
            return h.clone();
        }
        // 最后一个 bucket 必须是 +Inf
        let mut all_buckets = buckets;
        all_buckets.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        if !all_buckets.contains(&f64::INFINITY) {
            all_buckets.push(f64::INFINITY);
        }
        let count = all_buckets.len();
        let histogram = Arc::new(Histogram {
            name: name.to_string(),
            buckets: all_buckets,
            counts: Arc::new(RwLock::new(vec![0; count])),
            sum: Arc::new(RwLock::new(0.0)),
            count: Arc::new(RwLock::new(0)),
        });
        histograms.insert(name.to_string(), histogram.clone());

        let mut metas = self.metas.write();
        metas.push(MetricMeta {
            name: name.to_string(),
            help: help.to_string(),
            kind: MetricKind::Histogram,
        });
        histogram
    }

    /// 渲染所有指标为 Prometheus 文本格式
    pub fn render(&self) -> String {
        let mut output = String::new();

        // 输出 HELP/TYPE 头
        let metas = self.metas.read();
        let mut seen = std::collections::HashSet::new();
        for meta in metas.iter() {
            if seen.contains(&meta.name) {
                continue;
            }
            seen.insert(meta.name.clone());
            output.push_str(&format!("# HELP {} {}\n", meta.name, meta.help));
            let type_str = match meta.kind {
                MetricKind::Counter => "counter",
                MetricKind::Gauge => "gauge",
                MetricKind::Histogram => "histogram",
            };
            output.push_str(&format!("# TYPE {} {}\n", meta.name, type_str));
        }

        // 输出 Counter 值
        let counters = self.counters.read();
        for c in counters.values() {
            output.push_str(&c.render());
        }

        // 输出 Gauge 值
        let gauges = self.gauges.read();
        for g in gauges.values() {
            output.push_str(&g.render());
        }

        // 输出 Histogram 值
        let histograms = self.histograms.read();
        for h in histograms.values() {
            output.push_str(&h.render());
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter_basic() {
        let registry = MetricsRegistry::new();
        let counter = registry.register_counter("test_counter", "Test counter");
        counter.inc();
        counter.inc_by(2.5);
        assert_eq!(counter.value(), 3.5);
    }

    #[test]
    fn test_gauge_basic() {
        let registry = MetricsRegistry::new();
        let gauge = registry.register_gauge("test_gauge", "Test gauge");
        gauge.set(10.0);
        gauge.inc();
        gauge.dec_by(3.0);
        assert_eq!(gauge.value(), 8.0);
    }

    #[test]
    fn test_histogram_basic() {
        let registry = MetricsRegistry::new();
        let histogram =
            registry.register_histogram("test_histogram", "Test histogram", vec![0.1, 0.5, 1.0]);
        histogram.observe(0.05);
        histogram.observe(0.2);
        histogram.observe(0.6);
        histogram.observe(1.5);

        assert_eq!(histogram.count(), 4);
        assert!((histogram.sum() - 2.35).abs() < 1e-9);
    }

    #[test]
    fn test_render_prometheus_format() {
        let registry = MetricsRegistry::new();
        let counter = registry.register_counter("ops_total", "Total operations");
        let gauge = registry.register_gauge("conn_active", "Active connections");
        let histogram =
            registry.register_histogram("latency_seconds", "Latency in seconds", vec![0.01, 0.1]);

        counter.inc_by(10.0);
        gauge.set(5.0);
        histogram.observe(0.005);
        histogram.observe(0.05);
        histogram.observe(0.5);

        let output = registry.render();
        assert!(output.contains("# HELP ops_total Total operations"));
        assert!(output.contains("# TYPE ops_total counter"));
        assert!(output.contains("ops_total 10"));
        assert!(output.contains("conn_active 5"));
        assert!(output.contains("latency_seconds_bucket{le=\"0.01\"} 1"));
        assert!(output.contains("latency_seconds_bucket{le=\"0.1\"} 2"));
        assert!(output.contains("latency_seconds_sum 0.555"));
        assert!(output.contains("latency_seconds_count 3"));
    }

    #[test]
    fn test_counter_with_labels() {
        let registry = MetricsRegistry::new();
        let mut labels = HashMap::new();
        labels.insert("method".to_string(), "GET".to_string());
        labels.insert("status".to_string(), "200".to_string());

        let counter =
            registry.register_counter_with_labels("http_requests_total", "HTTP requests", labels);
        counter.inc();
        let output = registry.render();
        // HashMap 顺序未定义，分别验证各标签
        assert!(output.contains("http_requests_total{"));
        assert!(output.contains("method=\"GET\""));
        assert!(output.contains("status=\"200\""));
        assert!(output.contains("} 1"));
    }
}
