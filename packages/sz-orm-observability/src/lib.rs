//! SZ-ORM Observability Module
//!
//! Provides Prometheus exporter, SLO burn rate monitoring, and other capabilities.
//! OTLP export is provided by the `otlp` feature of `sz-orm-tracing` (this module does not include OTLP).
//!
//! # Core Capabilities
//!
//! ## 1. MetricsRegistry (enabled by default)
//!
//! Unified metric registry, supporting Counter / Gauge / Histogram types,
//! with built-in thread safety (`RwLock`), and can output Prometheus text format via `render()`.
//!
//! ## 2. Prometheus exporter
//!
//! Exposes the `/metrics` HTTP endpoint on the specified port via `start_metrics_server`,
//! for Prometheus to scrape. Implemented based on `tokio::net::TcpListener` (does not depend on hyper).
//!
//! ## 3. SLO burn rate
//!
//! Computes SLO burn rate based on 5m / 1h windows, supporting multi-window alerts.
//!
//! # Quick Start
//!
//! ```no_run
//! use sz_orm_observability::{MetricsRegistry, MetricKind};
//! use std::time::Duration;
//!
//! // Create metric registry
//! let registry = MetricsRegistry::new();
//!
//! // Register metrics
//! let counter = registry.register_counter("sz_orm_pool_acquires_total", "Total pool acquire calls");
//! let gauge = registry.register_gauge("sz_orm_pool_active_connections", "Current active connections");
//! let histogram = registry.register_histogram(
//!     "sz_orm_query_duration_seconds",
//!     "Query duration in seconds",
//!     vec![0.001, 0.01, 0.1, 1.0, 10.0],
//! );
//!
//! // Update metrics
//! counter.inc();
//! gauge.set(5.0);
//! histogram.observe(0.025);
//!
//! // Output Prometheus text format
//! let output = registry.render();
//! println!("{}", output);
//! ```

#![warn(missing_docs)]

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

pub mod slo;
pub mod summary;

#[cfg(feature = "service-mesh")]
pub mod service_mesh;

#[cfg(feature = "query-logging")]
pub mod query_logger;

#[cfg(feature = "anomaly-detection")]
pub mod anomaly;

#[cfg(feature = "anomaly-remediation-rca")]
#[allow(missing_docs)]
pub mod anomaly_remediation_rca;

pub use slo::{SloBurnRate, SloConfig, SloMonitor};
pub use summary::{
    LabeledHistogram, PushSnapshot, PushgatewayConfig, PushgatewayExporter, Summary,
};

#[cfg(feature = "query-logging")]
pub use query_logger::{mask_params, LogLevel, QueryLogEntry, QueryLogger};

/// Metric type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricKind {
    /// Monotonically increasing counter (e.g. total request count)
    Counter,
    /// Instantaneous value that can increase or decrease (e.g. current connection count)
    Gauge,
    /// Histogram (e.g. request latency distribution)
    Histogram,
}

/// Metric metadata
#[derive(Debug, Clone)]
pub struct MetricMeta {
    /// Metric name (e.g. `sz_orm_pool_acquires_total`)
    pub name: String,
    /// Help text
    pub help: String,
    /// Metric type
    pub kind: MetricKind,
}

/// Counter (monotonically increasing)
pub struct Counter {
    name: String,
    value: Arc<RwLock<f64>>,
    labels: HashMap<String, String>,
}

impl Counter {
    /// Increment by 1
    pub fn inc(&self) {
        self.inc_by(1.0);
    }

    /// Increment by the specified value
    pub fn inc_by(&self, delta: f64) {
        let mut v = self.value.write();
        *v += delta;
    }

    /// Current value
    pub fn value(&self) -> f64 {
        *self.value.read()
    }

    /// Metric name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Render to Prometheus text format
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

/// Gauge (can increase or decrease)
pub struct Gauge {
    name: String,
    value: Arc<RwLock<f64>>,
    labels: HashMap<String, String>,
}

impl Gauge {
    /// Set value
    pub fn set(&self, value: f64) {
        *self.value.write() = value;
    }

    /// Increment
    pub fn inc(&self) {
        self.inc_by(1.0);
    }

    /// Increment by the specified value
    pub fn inc_by(&self, delta: f64) {
        let mut v = self.value.write();
        *v += delta;
    }

    /// Decrement by the specified value
    pub fn dec_by(&self, delta: f64) {
        let mut v = self.value.write();
        *v -= delta;
    }

    /// Current value
    pub fn value(&self) -> f64 {
        *self.value.read()
    }

    /// Metric name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Render to Prometheus text format
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

/// Histogram (latency distribution, etc.)
pub struct Histogram {
    name: String,
    buckets: Vec<f64>,
    counts: Arc<RwLock<Vec<u64>>>,
    sum: Arc<RwLock<f64>>,
    count: Arc<RwLock<u64>>,
}

impl Histogram {
    /// Observe a value
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

    /// Total observation count
    pub fn count(&self) -> u64 {
        *self.count.read()
    }

    /// Sum of all observed values
    pub fn sum(&self) -> f64 {
        *self.sum.read()
    }

    /// Metric name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Render to Prometheus text format
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

/// Metric registry
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
    /// Create an empty registry
    pub fn new() -> Self {
        Self {
            counters: RwLock::new(HashMap::new()),
            gauges: RwLock::new(HashMap::new()),
            histograms: RwLock::new(HashMap::new()),
            metas: RwLock::new(Vec::new()),
        }
    }

    /// Register a Counter
    pub fn register_counter(&self, name: &str, help: &str) -> Arc<Counter> {
        self.register_counter_with_labels(name, help, HashMap::new())
    }

    /// Register a Counter with labels
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

    /// Register a Gauge
    pub fn register_gauge(&self, name: &str, help: &str) -> Arc<Gauge> {
        self.register_gauge_with_labels(name, help, HashMap::new())
    }

    /// Register a Gauge with labels
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

    /// Register a Histogram
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

    /// Render all metrics to Prometheus text format
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

/// Start the Prometheus metrics HTTP server
///
/// Exposes the `/metrics` endpoint at the specified address, returning metric data in Prometheus text format.
/// Each connection is handled in an independent tokio task.
pub async fn start_metrics_server(
    registry: Arc<MetricsRegistry>,
    addr: std::net::SocketAddr,
) -> Result<(), std::io::Error> {
    use tokio::io::AsyncWriteExt;

    let listener = tokio::net::TcpListener::bind(addr).await?;
    loop {
        let (mut stream, _) = listener.accept().await?;
        let registry = registry.clone();
        tokio::spawn(async move {
            let metrics = registry.render();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain; version=0.0.4\r\nContent-Length: {}\r\n\r\n{}",
                metrics.len(),
                metrics
            );
            let _ = stream.write_all(response.as_bytes()).await;
        });
    }
}

/// v3.8.0: metrics endpoint access control configuration
#[cfg(feature = "prod-metrics-acl")]
#[derive(Debug, Clone)]
pub struct MetricsAccessControl {
    /// Whether access control is enabled
    pub enabled: bool,
    /// IP whitelist (CIDR format strings, e.g. "10.0.0.0/8")
    pub ip_whitelist: Vec<String>,
    /// Bearer Token authentication
    pub bearer_token: Option<String>,
    /// Basic Auth authentication (username, password)
    pub basic_auth: Option<(String, String)>,
}

#[cfg(feature = "prod-metrics-acl")]
impl MetricsAccessControl {
    /// Create a configuration with access control disabled
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ip_whitelist: Vec::new(),
            bearer_token: None,
            basic_auth: None,
        }
    }

    /// Check whether the IP is in the whitelist
    pub fn check_ip_whitelist(peer_ip: &str, whitelist: &[String]) -> bool {
        if whitelist.is_empty() {
            return true;
        }
        for cidr in whitelist {
            if ip_in_cidr(peer_ip, cidr) {
                return true;
            }
        }
        false
    }

    /// Check Bearer Token (constant-time comparison)
    pub fn check_bearer_token(auth_header: Option<&str>, expected: &str) -> bool {
        use subtle::ConstantTimeEq;
        if let Some(header) = auth_header {
            if let Some(token) = header.strip_prefix("Bearer ") {
                return token.as_bytes().ct_eq(expected.as_bytes()).into();
            }
        }
        false
    }

    /// Check Basic Auth (constant-time comparison)
    pub fn check_basic_auth(
        auth_header: Option<&str>,
        expected_user: &str,
        expected_pass: &str,
    ) -> bool {
        use subtle::ConstantTimeEq;
        if let Some(header) = auth_header {
            if let Some(encoded) = header.strip_prefix("Basic ") {
                if let Ok(decoded) = base64_decode(encoded) {
                    let parts: Vec<&str> = decoded.splitn(2, ':').collect();
                    if parts.len() == 2 {
                        return parts[0].as_bytes().ct_eq(expected_user.as_bytes()).into()
                            && parts[1].as_bytes().ct_eq(expected_pass.as_bytes()).into();
                    }
                }
            }
        }
        false
    }

    /// Comprehensive authentication check
    pub fn check_access(&self, peer_ip: &str, auth_header: Option<&str>) -> bool {
        if !self.enabled {
            return true;
        }
        if !Self::check_ip_whitelist(peer_ip, &self.ip_whitelist) {
            return false;
        }
        if let Some(ref expected_token) = self.bearer_token {
            if !Self::check_bearer_token(auth_header, expected_token) {
                return false;
            }
        }
        if let Some((ref user, ref pass)) = self.basic_auth {
            if !Self::check_basic_auth(auth_header, user, pass) {
                return false;
            }
        }
        true
    }
}

#[cfg(feature = "prod-metrics-acl")]
fn ip_in_cidr(ip: &str, cidr: &str) -> bool {
    if let Some(slash_pos) = cidr.find('/') {
        let network = &cidr[..slash_pos];
        let prefix_len: u32 = cidr[slash_pos + 1..].parse().unwrap_or(0);
        if let (Ok(ip_addr), Ok(net_addr)) = (
            ip.parse::<std::net::IpAddr>(),
            network.parse::<std::net::IpAddr>(),
        ) {
            match (ip_addr, net_addr) {
                (std::net::IpAddr::V4(ip4), std::net::IpAddr::V4(net4)) => {
                    if prefix_len > 32 {
                        return false;
                    }
                    let mask = if prefix_len == 0 {
                        0u32
                    } else {
                        (!0u32) << (32 - prefix_len)
                    };
                    let ip_int = u32::from(ip4);
                    let net_int = u32::from(net4);
                    return (ip_int & mask) == (net_int & mask);
                }
                (std::net::IpAddr::V6(_), std::net::IpAddr::V6(_)) => {
                    return ip == network;
                }
                _ => return false,
            }
        }
    }
    ip == cidr
}

#[cfg(feature = "prod-metrics-acl")]
fn base64_decode(input: &str) -> Result<String, String> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    let bytes = STANDARD.decode(input).map_err(|e| e.to_string())?;
    String::from_utf8(bytes).map_err(|e| e.to_string())
}

/// v3.8.0: metrics endpoint with access control
#[cfg(feature = "prod-metrics-acl")]
pub async fn start_metrics_server_with_acl(
    registry: Arc<MetricsRegistry>,
    addr: std::net::SocketAddr,
    acl: MetricsAccessControl,
) -> Result<(), std::io::Error> {
    use tokio::io::AsyncWriteExt;

    let listener = tokio::net::TcpListener::bind(addr).await?;
    loop {
        let (mut stream, peer) = listener.accept().await?;
        let registry = registry.clone();
        let acl = acl.clone();
        tokio::spawn(async move {
            let peer_ip = peer.ip().to_string();
            let metrics = registry.render();
            let auth_header = None;
            if !acl.check_access(&peer_ip, auth_header) {
                let response = "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n";
                let _ = stream.write_all(response.as_bytes()).await;
                return;
            }
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain; version=0.0.4\r\nContent-Length: {}\r\n\r\n{}",
                metrics.len(),
                metrics
            );
            let _ = stream.write_all(response.as_bytes()).await;
        });
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

    #[cfg(feature = "prod-metrics-acl")]
    mod prod_metrics_acl_tests {
        use super::*;

        #[test]
        fn test_ip_whitelist_match() {
            let whitelist = vec!["10.0.0.0/8".to_string()];
            assert!(MetricsAccessControl::check_ip_whitelist(
                "10.1.2.3", &whitelist
            ));
            assert!(!MetricsAccessControl::check_ip_whitelist(
                "192.168.1.1",
                &whitelist
            ));
        }

        #[test]
        fn test_ip_whitelist_empty_allows_all() {
            assert!(MetricsAccessControl::check_ip_whitelist("1.2.3.4", &[]));
        }

        #[test]
        fn test_bearer_token_valid() {
            assert!(MetricsAccessControl::check_bearer_token(
                Some("Bearer secret123"),
                "secret123"
            ));
        }

        #[test]
        fn test_bearer_token_invalid() {
            assert!(!MetricsAccessControl::check_bearer_token(
                Some("Bearer wrong"),
                "secret123"
            ));
            assert!(!MetricsAccessControl::check_bearer_token(None, "secret123"));
        }

        #[test]
        fn test_check_access_disabled_allows_all() {
            let acl = MetricsAccessControl::disabled();
            assert!(acl.check_access("1.2.3.4", None));
        }

        #[test]
        fn test_check_access_enabled_ip_rejected() {
            let acl = MetricsAccessControl {
                enabled: true,
                ip_whitelist: vec!["10.0.0.0/8".to_string()],
                bearer_token: None,
                basic_auth: None,
            };
            assert!(!acl.check_access("192.168.1.1", None));
            assert!(acl.check_access("10.1.2.3", None));
        }
    }
}
