//! # SZ-ORM Tracing — Distributed tracing
//!
//! Provides Span/Tracer abstractions for distributed tracing, with OTLP exporter
//! reporting support, for collection and visualization of cross-service call chains.
//!
//! ## Main types
//!
//! - [`Span`] — single trace segment
//! - `Tracer` — tracer and exporter
//!
//! ## Sampling and context propagation (`sampling` module)
//!
//! - [`sampling::Sampler`] / [`sampling::TraceIdRatioSampler`] — sampling strategies
//! - [`sampling::Baggage`] / [`sampling::BaggagePropagator`] — W3C Baggage propagation
//! - [`sampling::BatchSpanExporter`] — batch Span export

pub mod sampling;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;

pub use sampling::{
    AlwaysOffSampler, AlwaysOnSampler, Baggage, BaggagePropagator, BatchConfig, BatchSpanExporter,
    ParentBasedSampler, Sampler, SamplingDecision, TraceIdRatioSampler,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    pub trace_id: String,
    pub span_id: String,
    pub parent_id: Option<String>,
    pub operation_name: String,
    pub service_name: String,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub tags: HashMap<String, String>,
    pub logs: Vec<SpanLog>,
}

impl Span {
    pub fn new(
        trace_id: impl Into<String>,
        span_id: impl Into<String>,
        operation_name: impl Into<String>,
    ) -> Self {
        Self {
            trace_id: trace_id.into(),
            span_id: span_id.into(),
            parent_id: None,
            operation_name: operation_name.into(),
            service_name: String::new(),
            start_time: current_timestamp(),
            end_time: None,
            tags: HashMap::new(),
            logs: Vec::new(),
        }
    }

    pub fn with_parent(mut self, parent_id: impl Into<String>) -> Self {
        self.parent_id = Some(parent_id.into());
        self
    }

    pub fn with_service(mut self, service_name: impl Into<String>) -> Self {
        self.service_name = service_name.into();
        self
    }

    pub fn with_tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.tags.insert(key.into(), value.into());
        self
    }

    pub fn finish(&mut self) {
        self.end_time = Some(current_timestamp());
    }

    pub fn duration(&self) -> Option<i64> {
        self.end_time.map(|end| end - self.start_time)
    }

    pub fn add_log(&mut self, message: impl Into<String>) {
        self.logs.push(SpanLog {
            timestamp: current_timestamp(),
            message: message.into(),
            fields: HashMap::new(),
        });
    }

    pub fn trace_id(&self) -> &str {
        &self.trace_id
    }

    pub fn span_id(&self) -> &str {
        &self.span_id
    }

    pub fn parent_id(&self) -> Option<&str> {
        self.parent_id.as_deref()
    }

    pub fn operation_name(&self) -> &str {
        &self.operation_name
    }

    pub fn service_name(&self) -> &str {
        &self.service_name
    }

    pub fn tags(&self) -> &HashMap<String, String> {
        &self.tags
    }

    pub fn logs(&self) -> &[SpanLog] {
        &self.logs
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanLog {
    pub timestamp: i64,
    pub message: String,
    pub fields: HashMap<String, String>,
}

pub trait Tracer: Send + Sync {
    fn start_span(&self, operation_name: &str) -> Span;
    fn end_span(&self, span: Span);
    fn inject(&self, span: &Span) -> HashMap<String, String>;
    fn extract(&self, headers: &HashMap<String, String>) -> Option<Span>;
}

pub struct SzTracer {
    spans: Arc<RwLock<Vec<Span>>>,
    service_name: String,
}

impl SzTracer {
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            spans: Arc::new(RwLock::new(Vec::new())),
            service_name: service_name.into(),
        }
    }

    pub fn generate_trace_id() -> String {
        format!("{:032x}", rand_u64())
    }

    pub fn generate_span_id() -> String {
        format!("{:016x}", rand_u64())
    }

    pub fn get_spans(&self) -> Vec<Span> {
        self.spans
            .read()
            .map_err(|e| TracingError::Internal(e.to_string()))
            .unwrap()
            .clone()
    }

    pub fn clear(&self) {
        self.spans
            .write()
            .map_err(|e| TracingError::Internal(e.to_string()))
            .unwrap()
            .clear();
    }
}

impl Default for SzTracer {
    fn default() -> Self {
        Self::new("unknown")
    }
}

impl Tracer for SzTracer {
    fn start_span(&self, operation_name: &str) -> Span {
        Span::new(
            Self::generate_trace_id(),
            Self::generate_span_id(),
            operation_name,
        )
        .with_service(&self.service_name)
    }

    fn end_span(&self, mut span: Span) {
        span.finish();

        if let Ok(mut spans) = self.spans.write() {
            spans.push(span);
        }
    }

    /// Injects the W3C TraceContext `traceparent` header
    ///
    /// v0.2.2 fixes P2-2: upgrades from custom `trace-id`/`span-id`/`parent-span-id`
    /// headers to the W3C TraceContext standard `traceparent` header.
    ///
    /// Format: `00-<trace_id>-<span_id>-<trace_flags>`
    /// - `00`: version number (fixed to `00` by the W3C specification)
    /// - `trace_id`: 32-char hex (16 bytes)
    /// - `span_id`: 16-char hex (8 bytes)
    /// - `trace_flags`: 2-char hex (`01` means sampled, `00` means not sampled)
    ///
    /// The `parent-span-id` header is retained for backward compatibility with older extract.
    fn inject(&self, span: &Span) -> HashMap<String, String> {
        let mut headers = HashMap::new();

        // W3C TraceContext: traceparent = version-trace_id-span_id-flags
        let traceparent = format!("00-{}-{}-01", span.trace_id, span.span_id);
        headers.insert("traceparent".to_string(), traceparent);

        if let Some(ref parent_id) = span.parent_id {
            headers.insert("parent-span-id".to_string(), parent_id.clone());
        }

        headers
    }

    /// Extracts a Span from headers
    ///
    /// v0.2.2 fixes P2-2: prioritizes parsing the W3C TraceContext `traceparent` header;
    /// if it does not exist, falls back to legacy custom headers (`trace-id`/`span-id`).
    ///
    /// W3C `traceparent` format: `00-<trace_id>-<span_id>-<trace_flags>`
    /// - The version number must be `00`
    /// - trace_id must be 32-char hex
    /// - span_id must be 16-char hex
    /// - trace_flags must be 2-char hex
    fn extract(&self, headers: &HashMap<String, String>) -> Option<Span> {
        // 优先尝试 W3C traceparent
        if let Some(traceparent) = headers.get("traceparent") {
            if let Some(span) = Self::parse_traceparent(traceparent) {
                let mut span = span.with_service(&self.service_name);

                // 同时检查 legacy parent-span-id header
                if let Some(parent_id) = headers.get("parent-span-id") {
                    span = span.with_parent(parent_id.clone());
                }

                return Some(span);
            }
        }

        // 回退到 legacy header（向后兼容）
        let trace_id = headers.get("trace-id")?;
        let span_id = headers.get("span-id")?;

        let mut span = Span::new(trace_id.clone(), span_id.clone(), "extracted");

        if let Some(parent_id) = headers.get("parent-span-id") {
            span = span.with_parent(parent_id.clone());
        }

        span = span.with_service(&self.service_name);

        Some(span)
    }
}

impl SzTracer {
    /// Parses the W3C TraceContext `traceparent` header
    ///
    /// Format: `00-<trace_id>-<span_id>-<trace_flags>`
    /// - The version number must be `00`
    /// - trace_id must be 32-char hex (not all zeros)
    /// - span_id must be 16-char hex (not all zeros)
    /// - trace_flags must be 2-char hex
    ///
    /// Returns `Some(Span)` on successful parsing, `None` if the format is invalid.
    fn parse_traceparent(traceparent: &str) -> Option<Span> {
        let parts: Vec<&str> = traceparent.split('-').collect();
        if parts.len() != 4 {
            return None;
        }

        let version = parts[0];
        let trace_id = parts[1];
        let span_id = parts[2];
        let trace_flags = parts[3];

        // 版本号必须是 2 字符 hex，且 W3C 规范当前固定为 "00"
        if version.len() != 2 || !version.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }

        // trace_id 必须是 32 字符 hex，且不能全为 0
        if trace_id.len() != 32
            || !trace_id.chars().all(|c| c.is_ascii_hexdigit())
            || trace_id.chars().all(|c| c == '0')
        {
            return None;
        }

        // span_id 必须是 16 字符 hex，且不能全为 0
        if span_id.len() != 16
            || !span_id.chars().all(|c| c.is_ascii_hexdigit())
            || span_id.chars().all(|c| c == '0')
        {
            return None;
        }

        // trace_flags 必须是 2 字符 hex
        if trace_flags.len() != 2 || !trace_flags.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }

        Some(Span::new(
            trace_id.to_string(),
            span_id.to_string(),
            "extracted",
        ))
    }

    /// Injects legacy custom headers (backward compatibility)
    ///
    /// v0.2.2 fixes P2-2: retains the old header format for compatibility with old clients.
    /// New code should use [`Tracer::inject`] (W3C TraceContext).
    pub fn inject_legacy(&self, span: &Span) -> HashMap<String, String> {
        let mut headers = HashMap::new();
        headers.insert("trace-id".to_string(), span.trace_id.to_string());
        headers.insert("span-id".to_string(), span.span_id.to_string());

        if let Some(ref parent_id) = span.parent_id {
            headers.insert("parent-span-id".to_string(), parent_id.clone());
        }

        headers
    }

    /// Extracts a Span only from legacy headers (backward compatibility)
    ///
    /// v0.2.2 fixes P2-2: retains the old header parsing for compatibility with old clients.
    /// New code should use [`Tracer::extract`] (automatically prioritizes W3C, falls back to legacy).
    pub fn extract_legacy(&self, headers: &HashMap<String, String>) -> Option<Span> {
        let trace_id = headers.get("trace-id")?;
        let span_id = headers.get("span-id")?;

        let mut span = Span::new(trace_id.clone(), span_id.clone(), "extracted");

        if let Some(parent_id) = headers.get("parent-span-id") {
            span = span.with_parent(parent_id.clone());
        }

        span = span.with_service(&self.service_name);

        Some(span)
    }
}

/// A compatibility wrapper that exposes the same [`Tracer`] interface as the
/// OpenTelemetry SDK but is implemented internally by delegating every call
/// to a [`SzTracer`].
///
/// # What this is (and is not)
///
/// This type exists so that downstream code can be written against an
/// "otel-style" tracer name without pulling in the real `opentelemetry`
/// crate as a dependency. It is **not** a real OpenTelemetry implementation:
///
/// - It does **not** export spans to an OTLP / Jaeger / Zipkin collector.
/// - v0.2.2 fixes P2-2: **now implements W3C TraceContext `traceparent` header propagation**
///   (`00-<trace_id>-<span_id>-<trace_flags>`), while retaining the `parent-span-id`
///   header to convey the parent span relationship. `extract` prioritizes W3C parsing
///   and falls back to legacy.
/// - It does **not** perform sampling, baggage propagation, or context
///   extraction across `async` boundaries.
/// - Span IDs are generated from `std::collections::hash_map::RandomState`
///   rather than per the OpenTelemetry specification (16 hex chars / 8 bytes
///   random).
///
/// # When to use which
///
/// - Use [`SzTracer`] directly for in-process span collection and W3C
///   TraceContext header propagation.
/// - Use `OtelTracer` when an existing code base expects a tracer whose name
///   signals OpenTelemetry-compatibility, but you have already decided to back
///   it with SzTracer semantics.
/// - For production distributed tracing across service boundaries, depend on
///   the real `opentelemetry` SDK and use its `Tracer` implementation instead.
///
/// Both [`SzTracer`] and `OtelTracer` satisfy the [`Tracer`] trait, so they
/// can be swapped at the type level without changing call sites.
pub struct OtelTracer {
    tracer: SzTracer,
}

impl OtelTracer {
    /// Wraps a freshly-built [`SzTracer`] tagged with `service_name`.
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            tracer: SzTracer::new(service_name),
        }
    }

    /// Provides read access to the underlying [`SzTracer`] so callers can
    /// inspect accumulated spans or clear them between tests.
    pub fn inner(&self) -> &SzTracer {
        &self.tracer
    }
}

impl Tracer for OtelTracer {
    fn start_span(&self, operation_name: &str) -> Span {
        self.tracer.start_span(operation_name)
    }

    fn end_span(&self, span: Span) {
        self.tracer.end_span(span)
    }

    fn inject(&self, span: &Span) -> HashMap<String, String> {
        self.tracer.inject(span)
    }

    fn extract(&self, headers: &HashMap<String, String>) -> Option<Span> {
        self.tracer.extract(headers)
    }
}

fn current_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn rand_u64() -> u64 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    RandomState::new().build_hasher().finish()
}

#[derive(Debug)]
pub enum TracingError {
    SpanNotFound(String),
    InvalidTraceId(String),
    Internal(String),
    /// OTLP exporter initialization failed (feature = "otlp")
    #[cfg(feature = "otlp")]
    OtlpInitFailed(String),
}

impl std::fmt::Display for TracingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TracingError::SpanNotFound(id) => write!(f, "Span not found: {}", id),
            TracingError::InvalidTraceId(id) => write!(f, "Invalid trace id: {}", id),
            TracingError::Internal(msg) => write!(f, "Tracing internal error: {}", msg),
            #[cfg(feature = "otlp")]
            TracingError::OtlpInitFailed(msg) => write!(f, "OTLP init failed: {}", msg),
        }
    }
}

impl std::error::Error for TracingError {}

impl serde::Serialize for TracingError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

// ===================== L4 SLA Monitoring =====================

/// Latency percentile histogram.
///
/// Implemented with a sorted array, records all observed latency samples and supports
/// arbitrary percentile queries. Suitable for financial-grade SLA monitoring scenarios
/// where the sample size is controllable; for very large-scale samples, consider
/// approximate algorithms such as T-Digest to reduce memory footprint.
#[derive(Debug, Clone)]
pub struct LatencyHistogram {
    samples: Vec<Duration>,
    sum: Duration,
}

impl LatencyHistogram {
    pub fn new(_buckets: Vec<Duration>) -> Self {
        Self {
            samples: Vec::new(),
            sum: Duration::ZERO,
        }
    }

    pub fn record(&mut self, duration: Duration) {
        self.sum += duration;
        let pos = self.samples.partition_point(|d| *d < duration);
        self.samples.insert(pos, duration);
    }

    pub fn percentile(&self, p: f64) -> Option<Duration> {
        if !(0.0..=100.0).contains(&p) || self.samples.is_empty() {
            return None;
        }
        let n = self.samples.len();
        // 最近排名法（Nearest Rank）：rank = ceil(p/100 * n)，至少为 1。
        // 这是 SLA 监控的标准方法（Google SRE 推荐用法），保证
        // p=0 返回最小值，p=100 返回最大值，且对高分位数偏保守。
        let rank = ((p / 100.0) * n as f64).ceil() as usize;
        let rank = rank.max(1).min(n);
        Some(self.samples[rank - 1])
    }

    pub fn count(&self) -> usize {
        self.samples.len()
    }

    pub fn mean(&self) -> Option<Duration> {
        if self.samples.is_empty() {
            None
        } else {
            Some(self.sum / self.samples.len() as u32)
        }
    }
}

/// Error rate counter, based on a sliding time window for error rate statistics.
///
/// Samples outside the window are evicted on the next [`record`](Self::record) call,
/// ensuring `rate()` is always computed from the latest samples within the window.
#[derive(Debug)]
pub struct ErrorRateCounter {
    window: Duration,
    samples: Vec<(std::time::Instant, bool)>,
}

impl ErrorRateCounter {
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            samples: Vec::new(),
        }
    }

    pub fn record(&mut self, success: bool) {
        let now = std::time::Instant::now();
        let cutoff = now - self.window;
        // 驱逐窗口外的样本（保留 cutoff 之后到达的样本）。
        self.samples.retain(|(ts, _)| *ts >= cutoff);
        self.samples.push((now, success));
    }

    pub fn rate(&self) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let errors = self.samples.iter().filter(|(_, ok)| !ok).count() as f64;
        errors / self.samples.len() as f64
    }

    pub fn total(&self) -> usize {
        self.samples.len()
    }

    pub fn errors(&self) -> usize {
        self.samples.iter().filter(|(_, ok)| !ok).count()
    }
}

/// Error budget, based on the SLO target and a sliding time window.
///
/// Core model: each error consumes `(1 - slo_target)` units of budget, and the budget
/// capacity is 1.0. Therefore the number of errors required to exhaust the budget is
/// `1 / (1 - slo_target)` (e.g., 1000 errors when SLO=0.999). This model assumes the
/// baseline traffic within the window is `1/(1-SLO)` requests; it is a simplified model
/// commonly used in financial-grade SLA monitoring, suitable for error budget tracking
/// independent of traffic statistics.
///
/// `consume` calls outside the window are evicted on the next record.
#[derive(Debug)]
pub struct ErrorBudget {
    slo_target: f64,
    window: Duration,
    samples: Vec<(std::time::Instant, usize)>,
}

impl ErrorBudget {
    pub fn new(slo_target: f64, window: Duration) -> Self {
        Self {
            slo_target,
            window,
            samples: Vec::new(),
        }
    }

    pub fn consume(&mut self, error_count: usize) {
        let now = std::time::Instant::now();
        let cutoff = now - self.window;
        self.samples.retain(|(ts, _)| *ts >= cutoff);
        if error_count > 0 {
            self.samples.push((now, error_count));
        }
    }

    fn total_errors_in_window(&self) -> usize {
        let now = std::time::Instant::now();
        let cutoff = now - self.window;
        self.samples
            .iter()
            .filter(|(ts, _)| *ts >= cutoff)
            .map(|(_, n)| *n)
            .sum()
    }

    pub fn remaining(&self) -> f64 {
        let total_errors = self.total_errors_in_window();
        if total_errors == 0 {
            return 1.0;
        }
        let error_budget = 1.0 - self.slo_target;
        if error_budget <= 0.0 {
            // SLO = 1.0 意味着不允许任何错误，有错误即耗尽。
            return 0.0;
        }
        let consumed = total_errors as f64 * error_budget;
        (1.0 - consumed).clamp(0.0, 1.0)
    }

    pub fn is_exhausted(&self) -> bool {
        self.remaining() == 0.0
    }
}

/// Alert level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertLevel {
    Info,
    Warning,
    Critical,
}

/// Alert event, produced by [`SaturationGauge`] / [`SlaMonitor`] and other components
/// when an anomaly is detected, and dispatched downstream (logs, webhooks, etc.) via [`AlertHook`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Alert {
    pub level: AlertLevel,
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub operation: Option<String>,
}

/// Saturation alert gauge.
///
/// Tracks a single saturation value (conventionally in the range `[0.0, 1.0]`); when the
/// value `>= threshold`, it is considered saturated and triggers an [`AlertLevel::Critical`] alert.
#[derive(Debug)]
pub struct SaturationGauge {
    threshold: f64,
    value: f64,
}

impl SaturationGauge {
    pub fn new(threshold: f64) -> Self {
        Self {
            threshold,
            value: 0.0,
        }
    }

    pub fn set(&mut self, value: f64) {
        self.value = value;
    }

    pub fn is_saturated(&self) -> bool {
        self.value >= self.threshold
    }

    pub fn check_alert(&self) -> Option<Alert> {
        if self.is_saturated() {
            Some(Alert {
                level: AlertLevel::Critical,
                message: format!(
                    "saturation {:.2} exceeded threshold {:.2}",
                    self.value, self.threshold
                ),
                timestamp: Utc::now(),
                operation: None,
            })
        } else {
            None
        }
    }
}

/// Alert dispatch hook. Implementations can write alerts to logs, send them to a webhook,
/// push them to IM, etc.
///
/// Must implement `Send + Sync` so it can be shared as `Arc<dyn AlertHook>` in multi-threaded
/// environments.
pub trait AlertHook: Send + Sync {
    /// Handles an alert event. Returns `Ok(())` on success, or an error description string on failure.
    fn notify(&self, alert: &Alert) -> Result<(), String>;
}

/// A simple implementation that writes alerts to the standard error stream.
///
/// Does not depend on the external `log` crate, avoiding extra configuration in test or
/// minimal deployment scenarios.
pub struct LogAlertHook;

impl LogAlertHook {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LogAlertHook {
    fn default() -> Self {
        Self::new()
    }
}

impl AlertHook for LogAlertHook {
    fn notify(&self, alert: &Alert) -> Result<(), String> {
        eprintln!(
            "[SLA ALERT] level={:?} op={:?} ts={} msg={}",
            alert.level, alert.operation, alert.timestamp, alert.message
        );
        Ok(())
    }
}

/// In-memory alert hook (**not a real webhook**): stores alerts in memory without any network sends.
///
/// The type name is deliberately changed to `InMemoryAlertHook` (the original `WebhookAlertHook`
/// name suggested HTTP calls, but it actually only pushes to a Vec, which would mislead users).
/// The `identifier` field is used only for test assertions and debugging, and **is not used for
/// any HTTP request**.
///
/// For real webhook push, implement a custom `AlertHook` and use `reqwest` or `hyper` to make
/// HTTP requests in `notify`.
///
/// Used for testing and local development environments; dispatched alerts can be inspected via
/// [`sent_alerts`](Self::sent_alerts).
pub struct InMemoryAlertHook {
    identifier: String,
    sent: RwLock<Vec<Alert>>,
}

impl InMemoryAlertHook {
    /// Creates an in-memory alert hook. `identifier` is only a debug label and does not participate in network calls.
    pub fn new(identifier: String) -> Self {
        Self {
            identifier,
            sent: RwLock::new(Vec::new()),
        }
    }

    /// Returns a snapshot (copy) of the alerts "sent" so far.
    pub fn sent_alerts(&self) -> Vec<Alert> {
        self.sent
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    /// Exposes the configured identifier for caller debugging or assertions.
    pub fn identifier(&self) -> &str {
        &self.identifier
    }

    /// Backward compatibility: returns the identifier (alias of the original url field).
    ///
    /// # Deprecated
    ///
    /// This method is retained only to reduce breaking changes; new code should use [`identifier`](Self::identifier).
    #[deprecated(
        since = "1.2.0",
        note = "use `identifier()` instead; this hook does not perform HTTP"
    )]
    pub fn url(&self) -> &str {
        &self.identifier
    }
}

impl AlertHook for InMemoryAlertHook {
    fn notify(&self, alert: &Alert) -> Result<(), String> {
        match self.sent.write() {
            Ok(mut guard) => {
                guard.push(alert.clone());
                Ok(())
            }
            Err(e) => Err(format!("in-memory alert hook lock poisoned: {e}")),
        }
    }
}

/// SLA statistics aggregation for a single operation (thread-safety provided by the `RwLock` of [`SlaMonitor`]).
///
/// Maintains both the latency histogram and the cumulative error count, avoiding the loss of
/// historical samples that [`ErrorRateCounter`]'s sliding window would cause in long-cycle SLA reports.
struct OperationStats {
    latency: LatencyHistogram,
    total_count: usize,
    error_count: usize,
}

impl OperationStats {
    fn new() -> Self {
        Self {
            latency: LatencyHistogram::new(Vec::new()),
            total_count: 0,
            error_count: 0,
        }
    }

    fn observe(&mut self, duration: Duration, success: bool) {
        self.latency.record(duration);
        self.total_count += 1;
        if !success {
            self.error_count += 1;
        }
    }

    fn error_rate(&self) -> f64 {
        if self.total_count == 0 {
            0.0
        } else {
            self.error_count as f64 / self.total_count as f64
        }
    }
}

/// SLA monitoring report snapshot, generated by [`SlaMonitor::report`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlaReport {
    /// P50 latency (milliseconds).
    pub p50_ms: f64,
    /// P95 latency (milliseconds).
    pub p95_ms: f64,
    /// P99 latency (milliseconds).
    pub p99_ms: f64,
    /// Error rate, in the range `[0.0, 1.0]`.
    pub error_rate: f64,
    /// Total number of observations.
    pub total_count: usize,
    /// SLO target success rate (e.g., `0.999`).
    pub slo_target: f64,
    /// Remaining error budget, in the range `[0.0, 1.0]`; `1.0` means the budget is intact.
    pub error_budget_remaining: f64,
    /// Saturation, in the range `[0.0, 1.0]`; `1.0` means the error budget is exhausted.
    pub saturation: f64,
}

/// SLA monitor, aggregates latency and error statistics by operation name, and generates [`SlaReport`].
///
/// Uses `RwLock<HashMap>` internally for thread safety; `observe` provides interior mutability via
/// `&self`, so it can be called concurrently from multiple threads via `Arc<SlaMonitor>`.
pub struct SlaMonitor {
    slo_target: f64,
    operations: RwLock<HashMap<String, OperationStats>>,
}

impl SlaMonitor {
    pub fn new(slo_target: f64) -> Self {
        Self {
            slo_target,
            operations: RwLock::new(HashMap::new()),
        }
    }

    /// 记录一次操作观测：延迟与成功/失败。
    ///
    /// 使用 `&self` 而非 `&mut self`，以便通过 `Arc<SlaMonitor>` 在多线程
    /// 中并发写入（与项目现有 `SzTracer::end_span` 模式一致）。
    pub fn observe(&self, operation: &str, duration: Duration, success: bool) {
        if let Ok(mut ops) = self.operations.write() {
            let stats = ops
                .entry(operation.to_string())
                .or_insert_with(OperationStats::new);
            stats.observe(duration, success);
        }
    }

    pub fn report(&self, operation: &str) -> Option<SlaReport> {
        let ops = self.operations.read().ok()?;
        let stats = ops.get(operation)?;
        let error_rate = stats.error_rate();
        let error_budget = 1.0 - self.slo_target;
        let (saturation, error_budget_remaining) = if error_budget <= 0.0 {
            // SLO = 1.0 意味着不允许任何错误：有错误即耗尽预算。
            if error_rate == 0.0 {
                (0.0, 1.0)
            } else {
                (1.0, 0.0)
            }
        } else {
            let sat = (error_rate / error_budget).clamp(0.0, 1.0);
            (sat, 1.0 - sat)
        };
        let ms = |p: f64| {
            stats
                .latency
                .percentile(p)
                .map(|d| d.as_nanos() as f64 / 1_000_000.0)
                .unwrap_or(0.0)
        };
        Some(SlaReport {
            p50_ms: ms(50.0),
            p95_ms: ms(95.0),
            p99_ms: ms(99.0),
            error_rate,
            total_count: stats.total_count,
            slo_target: self.slo_target,
            error_budget_remaining,
            saturation,
        })
    }

    pub fn operations(&self) -> Vec<String> {
        self.operations
            .read()
            .map(|ops| ops.keys().cloned().collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_new() {
        let span = Span::new("trace1", "span1", "operation1");
        assert_eq!(span.trace_id, "trace1");
        assert_eq!(span.span_id, "span1");
        assert_eq!(span.operation_name, "operation1");
        assert!(span.end_time.is_none());
    }

    #[test]
    fn test_span_with_parent() {
        let span = Span::new("trace1", "span1", "op").with_parent("parent1");
        assert_eq!(span.parent_id, Some("parent1".to_string()));
    }

    #[test]
    fn test_span_with_service() {
        let span = Span::new("trace1", "span1", "op").with_service("my-service");
        assert_eq!(span.service_name, "my-service");
    }

    #[test]
    fn test_span_with_tag() {
        let span = Span::new("trace1", "span1", "op").with_tag("key", "value");
        assert_eq!(span.tags.get("key"), Some(&"value".to_string()));
    }

    #[test]
    fn test_span_finish() {
        let mut span = Span::new("trace1", "span1", "op");
        span.finish();
        assert!(span.end_time.is_some());
        assert!(span.duration().is_some());
    }

    #[test]
    fn test_span_add_log() {
        let mut span = Span::new("trace1", "span1", "op");
        span.add_log("test log");
        assert_eq!(span.logs.len(), 1);
        assert_eq!(span.logs[0].message, "test log");
    }

    #[test]
    fn test_tracer_new() {
        let tracer = SzTracer::new("test-service");
        assert!(tracer.get_spans().is_empty());
    }

    #[test]
    fn test_tracer_start_span() {
        let tracer = SzTracer::new("test-service");
        let span = tracer.start_span("test-operation");
        assert_eq!(span.operation_name, "test-operation");
    }

    #[test]
    fn test_tracer_end_span() {
        let tracer = SzTracer::new("test-service");
        let span = tracer.start_span("test-operation");
        tracer.end_span(span);

        let spans = tracer.get_spans();
        assert_eq!(spans.len(), 1);
    }

    #[test]
    fn test_tracer_inject() {
        let tracer = SzTracer::new("test-service");
        let span = tracer.start_span("test");
        let headers = tracer.inject(&span);

        // v0.2.2 修复 P2-2：默认使用 W3C traceparent header
        let tp = headers
            .get("traceparent")
            .expect("traceparent header must be present");
        // 格式：00-<trace_id>-<span_id>-01
        let parts: Vec<&str> = tp.split('-').collect();
        assert_eq!(parts.len(), 4);
        assert_eq!(parts[0], "00"); // 版本号
        assert_eq!(parts[1], span.trace_id);
        assert_eq!(parts[2], span.span_id);
        assert_eq!(parts[3], "01"); // sampled
    }

    #[test]
    fn test_tracer_extract() {
        let tracer = SzTracer::new("test-service");
        let mut headers = HashMap::new();
        // W3C traceparent 格式
        let trace_id = "0af7651916cd43dd8448eb211c80319c";
        let span_id = "b7ad6b7169203331";
        headers.insert(
            "traceparent".to_string(),
            format!("00-{}-{}-01", trace_id, span_id),
        );

        let span = tracer.extract(&headers);
        assert!(span.is_some());
        let span = span.unwrap();
        assert_eq!(span.trace_id, trace_id);
        assert_eq!(span.span_id, span_id);
    }

    #[test]
    fn test_tracer_extract_legacy_headers() {
        // v0.2.2 修复 P2-2：向后兼容 legacy header
        let tracer = SzTracer::new("test-service");
        let mut headers = HashMap::new();
        headers.insert("trace-id".to_string(), "trace123".to_string());
        headers.insert("span-id".to_string(), "span456".to_string());

        let span = tracer.extract(&headers);
        assert!(span.is_some());
        let span = span.unwrap();
        assert_eq!(span.trace_id, "trace123");
        assert_eq!(span.span_id, "span456");
    }

    #[test]
    fn test_tracer_extract_missing_headers() {
        let tracer = SzTracer::new("test-service");
        let headers = HashMap::new();
        let span = tracer.extract(&headers);
        assert!(span.is_none());
    }

    // ===================== v0.2.2 修复 P2-2：W3C TraceContext 测试 =====================

    #[test]
    fn test_parse_traceparent_valid() {
        let valid = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
        let span = SzTracer::parse_traceparent(valid).expect("valid traceparent must parse");
        assert_eq!(span.trace_id, "0af7651916cd43dd8448eb211c80319c");
        assert_eq!(span.span_id, "b7ad6b7169203331");
    }

    #[test]
    fn test_parse_traceparent_invalid_version() {
        // 版本号不是 2 字符 hex
        assert!(SzTracer::parse_traceparent(
            "0-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01"
        )
        .is_none());
        // 版本号不是 hex
        assert!(SzTracer::parse_traceparent(
            "xy-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01"
        )
        .is_none());
    }

    #[test]
    fn test_parse_traceparent_invalid_trace_id_length() {
        // trace_id 不是 32 字符
        assert!(SzTracer::parse_traceparent("00-short-b7ad6b7169203331-01").is_none());
    }

    #[test]
    fn test_parse_traceparent_invalid_span_id_length() {
        // span_id 不是 16 字符
        assert!(
            SzTracer::parse_traceparent("00-0af7651916cd43dd8448eb211c80319c-short-01").is_none()
        );
    }

    #[test]
    fn test_parse_traceparent_all_zeros_trace_id_rejected() {
        // W3C 规范：trace_id 不能全为 0
        let all_zero = "00-00000000000000000000000000000000-b7ad6b7169203331-01";
        assert!(SzTracer::parse_traceparent(all_zero).is_none());
    }

    #[test]
    fn test_parse_traceparent_all_zeros_span_id_rejected() {
        // W3C 规范：span_id 不能全为 0
        let all_zero = "00-0af7651916cd43dd8448eb211c80319c-0000000000000000-01";
        assert!(SzTracer::parse_traceparent(all_zero).is_none());
    }

    #[test]
    fn test_parse_traceparent_invalid_flags() {
        // trace_flags 不是 2 字符 hex
        assert!(SzTracer::parse_traceparent(
            "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-xyz"
        )
        .is_none());
    }

    #[test]
    fn test_parse_traceparent_wrong_part_count() {
        // 不是 4 个部分
        assert!(SzTracer::parse_traceparent(
            "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331"
        )
        .is_none());
        assert!(SzTracer::parse_traceparent(
            "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01-extra"
        )
        .is_none());
    }

    #[test]
    fn test_inject_legacy_preserves_old_format() {
        let tracer = SzTracer::new("svc");
        let span = tracer.start_span("op");
        let headers = tracer.inject_legacy(&span);

        assert_eq!(headers.get("trace-id"), Some(&span.trace_id.to_string()));
        assert_eq!(headers.get("span-id"), Some(&span.span_id.to_string()));
        assert!(!headers.contains_key("parent-span-id"));
    }

    #[test]
    fn test_extract_legacy_preserves_old_format() {
        let tracer = SzTracer::new("svc");
        let mut headers = HashMap::new();
        headers.insert("trace-id".to_string(), "abc".to_string());
        headers.insert("span-id".to_string(), "def".to_string());

        let span = tracer.extract_legacy(&headers).expect("legacy extract");
        assert_eq!(span.trace_id, "abc");
        assert_eq!(span.span_id, "def");
    }

    #[test]
    fn test_w3c_traceparent_roundtrip_preserves_ids() {
        // inject → extract 必须保留 trace_id 和 span_id
        let tracer = SzTracer::new("svc");
        let original = tracer.start_span("roundtrip");
        let headers = tracer.inject(&original);

        let extracted = tracer.extract(&headers).expect("roundtrip extract");
        assert_eq!(extracted.trace_id(), original.trace_id());
        assert_eq!(extracted.span_id(), original.span_id());
        assert!(extracted.parent_id().is_none());
    }

    #[test]
    fn test_w3c_prefers_traceparent_over_legacy() {
        // 同时存在 traceparent 和 legacy header 时，优先 W3C
        let tracer = SzTracer::new("svc");
        let mut headers = HashMap::new();
        headers.insert(
            "traceparent".to_string(),
            "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01".to_string(),
        );
        headers.insert("trace-id".to_string(), "legacy-trace".to_string());
        headers.insert("span-id".to_string(), "legacy-span".to_string());

        let span = tracer.extract(&headers).expect("extract");
        assert_eq!(span.trace_id, "0af7651916cd43dd8448eb211c80319c");
        assert_eq!(span.span_id, "b7ad6b7169203331");
    }

    #[test]
    fn test_w3c_falls_back_to_legacy_on_invalid_traceparent() {
        // traceparent 格式不合法时回退到 legacy
        let tracer = SzTracer::new("svc");
        let mut headers = HashMap::new();
        headers.insert("traceparent".to_string(), "invalid-format".to_string());
        headers.insert("trace-id".to_string(), "legacy-trace".to_string());
        headers.insert("span-id".to_string(), "legacy-span".to_string());

        let span = tracer
            .extract(&headers)
            .expect("should fall back to legacy");
        assert_eq!(span.trace_id, "legacy-trace");
        assert_eq!(span.span_id, "legacy-span");
    }

    #[test]
    fn test_otel_tracer() {
        let tracer = OtelTracer::new("test-service");
        let span = tracer.start_span("test-operation");
        assert_eq!(span.operation_name, "test-operation");
    }

    #[test]
    fn test_otel_tracer_is_a_sz_tracer_wrapper_not_a_real_otel_sdk() {
        // OtelTracer delegates every Tracer method to SzTracer. Document the
        // contract: spans produced via OtelTracer must be observable through
        // the underlying SzTracer (i.e. `inner().get_spans()`).
        let tracer = OtelTracer::new("svc");
        assert!(tracer.inner().get_spans().is_empty());

        let span = tracer.start_span("op");
        assert_eq!(span.service_name(), "svc");
        tracer.end_span(span);

        let spans = tracer.inner().get_spans();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].operation_name(), "op");
        assert!(spans[0].end_time.is_some());
    }

    #[test]
    fn test_otel_tracer_inject_extract_roundtrip_preserves_ids() {
        // v0.2.2 修复 P2-2：现在使用 W3C traceparent header
        let tracer = OtelTracer::new("svc");
        let original = tracer.start_span("roundtrip");
        let headers = tracer.inject(&original);

        // W3C traceparent 必须存在并包含原始 ID
        let tp = headers
            .get("traceparent")
            .expect("traceparent must be present");
        assert!(tp.contains(original.trace_id()));
        assert!(tp.contains(original.span_id()));
        assert!(!headers.contains_key("parent-span-id"));

        let extracted = tracer.extract(&headers).expect("extract should round-trip");
        assert_eq!(extracted.trace_id(), original.trace_id());
        assert_eq!(extracted.span_id(), original.span_id());
        assert!(extracted.parent_id().is_none());
    }

    #[test]
    fn test_otel_tracer_preserves_parent_id_through_roundtrip() {
        let tracer = OtelTracer::new("svc");
        let parent = tracer.start_span("parent");
        let child = tracer
            .start_span("child")
            .with_parent(parent.span_id().to_string());
        let headers = tracer.inject(&child);

        // parent-span-id 仍然通过独立 header 传递（向后兼容）
        assert_eq!(
            headers.get("parent-span-id"),
            Some(&parent.span_id().to_string())
        );

        let extracted = tracer.extract(&headers).expect("extract should round-trip");
        assert_eq!(extracted.parent_id(), Some(parent.span_id()));
    }

    #[test]
    fn test_otel_tracer_extract_returns_none_without_required_headers() {
        let tracer = OtelTracer::new("svc");
        let headers: HashMap<String, String> = HashMap::new();
        assert!(tracer.extract(&headers).is_none());

        // 仅 legacy trace-id 缺失 span-id 时也必须失败
        let mut partial = HashMap::new();
        partial.insert("trace-id".to_string(), "abc".to_string());
        // Missing span-id - extract must fail.
        assert!(tracer.extract(&partial).is_none());

        // 仅 W3C traceparent 格式错误时（且无 legacy 回退）必须失败
        let mut bad_w3c = HashMap::new();
        bad_w3c.insert("traceparent".to_string(), "garbage".to_string());
        assert!(tracer.extract(&bad_w3c).is_none());
    }

    #[test]
    fn test_otel_tracer_generated_ids_have_correct_length() {
        // OpenTelemetry spec: trace_id is 16 bytes (32 hex chars), span_id
        // is 8 bytes (16 hex chars). OtelTracer inherits SzTracer's generator
        // and must produce the same shape so consumers parsing the IDs do not
        // trip over a different length.
        let trace_id = SzTracer::generate_trace_id();
        let span_id = SzTracer::generate_span_id();
        assert_eq!(trace_id.len(), 32, "trace_id must be 32 hex chars");
        assert_eq!(span_id.len(), 16, "span_id must be 16 hex chars");

        // Hex-only.
        assert!(trace_id.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(span_id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_span_accessors() {
        let span = Span::new("trace1", "span1", "test-op")
            .with_service("svc")
            .with_tag("k", "v");

        assert_eq!(span.trace_id(), "trace1");
        assert_eq!(span.span_id(), "span1");
        assert_eq!(span.operation_name(), "test-op");
        assert_eq!(span.service_name(), "svc");
        assert_eq!(span.tags().get("k"), Some(&"v".to_string()));
    }

    #[test]
    fn test_generate_ids() {
        let trace_id = SzTracer::generate_trace_id();
        let span_id = SzTracer::generate_span_id();

        assert_eq!(trace_id.len(), 32);
        assert_eq!(span_id.len(), 16);
    }

    #[test]
    fn test_tracer_clear() {
        let tracer = SzTracer::new("test-service");

        let span = tracer.start_span("op1");
        tracer.end_span(span);
        let span = tracer.start_span("op2");
        tracer.end_span(span);

        assert_eq!(tracer.get_spans().len(), 2);

        tracer.clear();
        assert!(tracer.get_spans().is_empty());
    }

    // ===================== LatencyHistogram tests =====================

    #[test]
    fn test_latency_histogram_new_empty() {
        let hist = LatencyHistogram::new(vec![
            Duration::from_millis(10),
            Duration::from_millis(100),
            Duration::from_millis(1000),
        ]);
        assert_eq!(hist.count(), 0);
        assert!(hist.percentile(50.0).is_none());
        assert!(hist.mean().is_none());
    }

    #[test]
    fn test_latency_histogram_record_single() {
        let mut hist = LatencyHistogram::new(vec![Duration::from_millis(100)]);
        hist.record(Duration::from_millis(50));
        assert_eq!(hist.count(), 1);
        assert_eq!(hist.percentile(50.0), Some(Duration::from_millis(50)));
        assert_eq!(hist.mean(), Some(Duration::from_millis(50)));
    }

    #[test]
    fn test_latency_histogram_percentile_p50_sorted() {
        let mut hist = LatencyHistogram::new(vec![Duration::from_millis(1000)]);
        for ms in [10, 20, 30, 40, 50] {
            hist.record(Duration::from_millis(ms));
        }
        // 5 samples sorted: [10, 20, 30, 40, 50], p50 should be 30 (median)
        assert_eq!(hist.percentile(50.0), Some(Duration::from_millis(30)));
    }

    #[test]
    fn test_latency_histogram_percentile_p95_high_value() {
        let mut hist = LatencyHistogram::new(vec![Duration::from_secs(10)]);
        for ms in [1, 2, 3, 4, 5, 6, 7, 8, 9, 100] {
            hist.record(Duration::from_millis(ms));
        }
        // p95 of 10 samples should be the 9th or 10th value (high)
        let p95 = hist.percentile(95.0).expect("p95 must exist");
        assert!(p95 >= Duration::from_millis(9));
    }

    #[test]
    fn test_latency_histogram_percentile_p99_max_value() {
        let mut hist = LatencyHistogram::new(vec![Duration::from_secs(10)]);
        for ms in [1, 2, 3, 4, 5, 6, 7, 8, 9, 100] {
            hist.record(Duration::from_millis(ms));
        }
        let p99 = hist.percentile(99.0).expect("p99 must exist");
        assert_eq!(p99, Duration::from_millis(100));
    }

    #[test]
    fn test_latency_histogram_percentile_p0_min_value() {
        let mut hist = LatencyHistogram::new(vec![Duration::from_secs(10)]);
        for ms in [10, 20, 30] {
            hist.record(Duration::from_millis(ms));
        }
        assert_eq!(hist.percentile(0.0), Some(Duration::from_millis(10)));
    }

    #[test]
    fn test_latency_histogram_percentile_p100_max_value() {
        let mut hist = LatencyHistogram::new(vec![Duration::from_secs(10)]);
        for ms in [10, 20, 30] {
            hist.record(Duration::from_millis(ms));
        }
        assert_eq!(hist.percentile(100.0), Some(Duration::from_millis(30)));
    }

    #[test]
    fn test_latency_histogram_percentile_empty_returns_none() {
        let hist = LatencyHistogram::new(vec![Duration::from_millis(100)]);
        assert!(hist.percentile(50.0).is_none());
    }

    #[test]
    fn test_latency_histogram_percentile_out_of_range_returns_none() {
        let mut hist = LatencyHistogram::new(vec![Duration::from_millis(100)]);
        hist.record(Duration::from_millis(50));
        assert!(hist.percentile(-1.0).is_none());
        assert!(hist.percentile(101.0).is_none());
    }

    #[test]
    fn test_latency_histogram_count() {
        let mut hist = LatencyHistogram::new(vec![Duration::from_millis(100)]);
        assert_eq!(hist.count(), 0);
        hist.record(Duration::from_millis(10));
        hist.record(Duration::from_millis(20));
        hist.record(Duration::from_millis(30));
        assert_eq!(hist.count(), 3);
    }

    #[test]
    fn test_latency_histogram_mean_multiple() {
        let mut hist = LatencyHistogram::new(vec![Duration::from_millis(1000)]);
        for ms in [10, 20, 30, 40, 50] {
            hist.record(Duration::from_millis(ms));
        }
        // mean = (10+20+30+40+50)/5 = 30
        assert_eq!(hist.mean(), Some(Duration::from_millis(30)));
    }

    #[test]
    fn test_latency_histogram_record_unsorted_input_stays_sorted() {
        let mut hist = LatencyHistogram::new(vec![Duration::from_millis(1000)]);
        hist.record(Duration::from_millis(50));
        hist.record(Duration::from_millis(10));
        hist.record(Duration::from_millis(30));
        // p50 should be 30 (median of sorted [10,30,50])
        assert_eq!(hist.percentile(50.0), Some(Duration::from_millis(30)));
    }

    // ===================== ErrorRateCounter tests =====================

    #[test]
    fn test_error_rate_counter_new_empty() {
        let counter = ErrorRateCounter::new(Duration::from_secs(60));
        assert_eq!(counter.total(), 0);
        assert_eq!(counter.errors(), 0);
        assert_eq!(counter.rate(), 0.0);
    }

    #[test]
    fn test_error_rate_counter_all_success_rate_zero() {
        let mut counter = ErrorRateCounter::new(Duration::from_secs(60));
        for _ in 0..10 {
            counter.record(true);
        }
        assert_eq!(counter.total(), 10);
        assert_eq!(counter.errors(), 0);
        assert_eq!(counter.rate(), 0.0);
    }

    #[test]
    fn test_error_rate_counter_all_failures_rate_one() {
        let mut counter = ErrorRateCounter::new(Duration::from_secs(60));
        for _ in 0..10 {
            counter.record(false);
        }
        assert_eq!(counter.total(), 10);
        assert_eq!(counter.errors(), 10);
        assert_eq!(counter.rate(), 1.0);
    }

    #[test]
    fn test_error_rate_counter_mixed_rate() {
        let mut counter = ErrorRateCounter::new(Duration::from_secs(60));
        // 7 success, 3 failures -> rate = 0.3
        for _ in 0..7 {
            counter.record(true);
        }
        for _ in 0..3 {
            counter.record(false);
        }
        assert_eq!(counter.total(), 10);
        assert_eq!(counter.errors(), 3);
        let rate = counter.rate();
        assert!((rate - 0.3).abs() < 1e-9, "expected 0.3, got {rate}");
    }

    #[test]
    fn test_error_rate_counter_empty_rate_is_zero() {
        let counter = ErrorRateCounter::new(Duration::from_secs(60));
        // no samples -> rate must be 0.0 (avoid div-by-zero)
        assert_eq!(counter.rate(), 0.0);
    }

    #[test]
    fn test_error_rate_counter_window_expires_old_samples() {
        // window of 100ms; old samples should be evicted on subsequent record.
        let mut counter = ErrorRateCounter::new(Duration::from_millis(100));
        counter.record(false);
        counter.record(false);
        // sleep past the window
        std::thread::sleep(Duration::from_millis(120));
        counter.record(true);
        // Only the recent success should remain -> rate = 0.0, total = 1
        assert_eq!(counter.total(), 1);
        assert_eq!(counter.errors(), 0);
        assert_eq!(counter.rate(), 0.0);
    }

    // ===================== ErrorBudget tests =====================

    #[test]
    fn test_error_budget_new_is_full() {
        let budget = ErrorBudget::new(0.999, Duration::from_secs(60));
        assert!((budget.remaining() - 1.0).abs() < 1e-9);
        assert!(!budget.is_exhausted());
    }

    #[test]
    fn test_error_budget_consume_reduces_remaining() {
        let mut budget = ErrorBudget::new(0.999, Duration::from_secs(60));
        // SLO 0.999 -> error_budget = 0.001 per error.
        budget.consume(1);
        // remaining = 1 - 1 * 0.001 = 0.999
        assert!((budget.remaining() - 0.999).abs() < 1e-9);
        assert!(!budget.is_exhausted());
    }

    #[test]
    fn test_error_budget_exhausted_at_capacity() {
        let mut budget = ErrorBudget::new(0.999, Duration::from_secs(60));
        // capacity = 1 / (1 - 0.999) = 1000 errors
        budget.consume(1000);
        assert_eq!(budget.remaining(), 0.0);
        assert!(budget.is_exhausted());
    }

    #[test]
    fn test_error_budget_over_consume_clamps_to_zero() {
        let mut budget = ErrorBudget::new(0.999, Duration::from_secs(60));
        budget.consume(2000);
        assert_eq!(budget.remaining(), 0.0);
        assert!(budget.is_exhausted());
    }

    #[test]
    fn test_error_budget_zero_errors_returns_one() {
        let budget = ErrorBudget::new(0.999, Duration::from_secs(60));
        assert_eq!(budget.remaining(), 1.0);
        assert!(!budget.is_exhausted());
    }

    #[test]
    fn test_error_budget_window_refills_after_expiry() {
        // SLO 0.5 -> error_budget = 0.5 per error -> 2 errors exhaust.
        let mut budget = ErrorBudget::new(0.5, Duration::from_millis(100));
        budget.consume(2);
        assert!(budget.is_exhausted());
        // wait for window to expire
        std::thread::sleep(Duration::from_millis(120));
        assert!((budget.remaining() - 1.0).abs() < 1e-9);
        assert!(!budget.is_exhausted());
    }

    #[test]
    fn test_error_budget_slo_one_means_no_errors_allowed() {
        // SLO = 1.0 -> error_budget = 0 -> any consume exhausts
        let mut budget = ErrorBudget::new(1.0, Duration::from_secs(60));
        assert_eq!(budget.remaining(), 1.0);
        budget.consume(1);
        assert_eq!(budget.remaining(), 0.0);
        assert!(budget.is_exhausted());
    }

    #[test]
    fn test_error_budget_multiple_consumes_accumulate() {
        let mut budget = ErrorBudget::new(0.99, Duration::from_secs(60));
        // SLO 0.99 -> error_budget = 0.01 -> capacity = 100 errors
        budget.consume(30);
        assert!((budget.remaining() - 0.7).abs() < 1e-9);
        budget.consume(30);
        assert!((budget.remaining() - 0.4).abs() < 1e-9);
        budget.consume(40);
        assert_eq!(budget.remaining(), 0.0);
        assert!(budget.is_exhausted());
    }

    // ===================== Alert / AlertLevel tests =====================

    #[test]
    fn test_alert_level_variants_exist() {
        let info = AlertLevel::Info;
        let warning = AlertLevel::Warning;
        let critical = AlertLevel::Critical;
        // Sanity: ensure variants are distinct (debug repr).
        assert_ne!(format!("{info:?}"), format!("{warning:?}"));
        assert_ne!(format!("{warning:?}"), format!("{critical:?}"));
        assert_ne!(format!("{info:?}"), format!("{critical:?}"));
    }

    #[test]
    fn test_alert_construction_with_all_fields() {
        let ts = Utc::now();
        let alert = Alert {
            level: AlertLevel::Critical,
            message: "p99 latency exceeded budget".to_string(),
            timestamp: ts,
            operation: Some("query_user".to_string()),
        };
        assert_eq!(alert.level, AlertLevel::Critical);
        assert_eq!(alert.message, "p99 latency exceeded budget");
        assert_eq!(alert.timestamp, ts);
        assert_eq!(alert.operation.as_deref(), Some("query_user"));
    }

    #[test]
    fn test_alert_construction_without_operation() {
        let alert = Alert {
            level: AlertLevel::Info,
            message: "system healthy".to_string(),
            timestamp: Utc::now(),
            operation: None,
        };
        assert!(alert.operation.is_none());
    }

    #[test]
    fn test_alert_implements_clone_debug() {
        let alert = Alert {
            level: AlertLevel::Warning,
            message: "approaching budget".to_string(),
            timestamp: Utc::now(),
            operation: Some("op".to_string()),
        };
        let cloned = alert.clone();
        assert_eq!(cloned.level, alert.level);
        assert_eq!(cloned.message, alert.message);
        // Debug formatting must contain expected fields, not just "not panic".
        let debug_str = format!("{alert:?}");
        assert!(
            debug_str.contains("approaching budget"),
            "debug output missing message: {}",
            debug_str
        );
        assert!(
            debug_str.contains("Warning"),
            "debug output missing level: {}",
            debug_str
        );
    }

    // ===================== SaturationGauge tests =====================

    #[test]
    fn test_saturation_gauge_new_starts_unsaturated() {
        let gauge = SaturationGauge::new(0.8);
        assert!(!gauge.is_saturated());
        assert!(gauge.check_alert().is_none());
    }

    #[test]
    fn test_saturation_gauge_set_below_threshold_not_saturated() {
        let mut gauge = SaturationGauge::new(0.8);
        gauge.set(0.5);
        assert!(!gauge.is_saturated());
        assert!(gauge.check_alert().is_none());
    }

    #[test]
    fn test_saturation_gauge_set_at_threshold_is_saturated() {
        let mut gauge = SaturationGauge::new(0.8);
        gauge.set(0.8);
        assert!(gauge.is_saturated());
    }

    #[test]
    fn test_saturation_gauge_set_above_threshold_is_saturated() {
        let mut gauge = SaturationGauge::new(0.8);
        gauge.set(0.95);
        assert!(gauge.is_saturated());
    }

    #[test]
    fn test_saturation_gauge_check_alert_returns_critical_when_saturated() {
        let mut gauge = SaturationGauge::new(0.8);
        gauge.set(0.95);
        let alert = gauge.check_alert().expect("alert must fire when saturated");
        assert_eq!(alert.level, AlertLevel::Critical);
        assert!(!alert.message.is_empty());
        assert!(alert.operation.is_none()); // gauge has no operation context
    }

    #[test]
    fn test_saturation_gauge_check_alert_returns_none_when_not_saturated() {
        let mut gauge = SaturationGauge::new(0.8);
        gauge.set(0.3);
        assert!(gauge.check_alert().is_none());
    }

    #[test]
    fn test_saturation_gauge_set_zero_not_saturated() {
        let mut gauge = SaturationGauge::new(0.8);
        gauge.set(0.0);
        assert!(!gauge.is_saturated());
    }

    #[test]
    fn test_saturation_gauge_set_one_saturated() {
        let mut gauge = SaturationGauge::new(0.5);
        gauge.set(1.0);
        assert!(gauge.is_saturated());
    }

    #[test]
    fn test_saturation_gauge_threshold_zero_always_saturated() {
        let mut gauge = SaturationGauge::new(0.0);
        gauge.set(0.0);
        // threshold = 0 means anything >= 0 is saturated (only negative would not be,
        // but saturation values are conventionally in [0, 1]).
        assert!(gauge.is_saturated());
    }

    // ===================== AlertHook / LogAlertHook / InMemoryAlertHook tests =====================

    fn sample_alert(level: AlertLevel, op: Option<&str>) -> Alert {
        Alert {
            level,
            message: "test alert".to_string(),
            timestamp: Utc::now(),
            operation: op.map(str::to_string),
        }
    }

    #[test]
    fn test_log_alert_hook_notify_returns_ok() {
        let hook = LogAlertHook::new();
        let alert = sample_alert(AlertLevel::Warning, Some("op"));
        let result = hook.notify(&alert);
        assert!(result.is_ok());
    }

    #[test]
    fn test_log_alert_hook_notify_critical_succeeds() {
        let hook = LogAlertHook::new();
        let alert = sample_alert(AlertLevel::Critical, None);
        let result = hook.notify(&alert);
        assert!(result.is_ok());
    }

    #[test]
    fn test_log_alert_hook_implements_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<LogAlertHook>();
    }

    #[test]
    fn test_webhook_alert_hook_new_starts_empty() {
        let hook = InMemoryAlertHook::new("https://example.com/hook".to_string());
        assert!(hook.sent_alerts().is_empty());
    }

    #[test]
    fn test_webhook_alert_hook_notify_stores_alert() {
        let hook = InMemoryAlertHook::new("https://example.com/hook".to_string());
        let alert = sample_alert(AlertLevel::Critical, Some("query_user"));
        hook.notify(&alert).expect("notify must succeed");
        let sent = hook.sent_alerts();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0], alert);
    }

    #[test]
    fn test_webhook_alert_hook_multiple_notifications_accumulate() {
        let hook = InMemoryAlertHook::new("https://example.com/hook".to_string());
        let a1 = sample_alert(AlertLevel::Info, None);
        let a2 = sample_alert(AlertLevel::Warning, Some("op1"));
        let a3 = sample_alert(AlertLevel::Critical, Some("op2"));
        hook.notify(&a1).unwrap();
        hook.notify(&a2).unwrap();
        hook.notify(&a3).unwrap();
        let sent = hook.sent_alerts();
        assert_eq!(sent.len(), 3);
        assert_eq!(sent[0], a1);
        assert_eq!(sent[1], a2);
        assert_eq!(sent[2], a3);
    }

    #[test]
    fn test_webhook_alert_hook_sent_alerts_returns_clone() {
        // The returned Vec should be a snapshot; mutating it must not affect the hook.
        let hook = InMemoryAlertHook::new("https://example.com/hook".to_string());
        let alert = sample_alert(AlertLevel::Info, None);
        hook.notify(&alert).unwrap();
        let mut sent = hook.sent_alerts();
        sent.clear();
        // hook itself must remain unaffected.
        assert_eq!(hook.sent_alerts().len(), 1);
    }

    #[test]
    fn test_webhook_alert_hook_implements_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<InMemoryAlertHook>();
    }

    #[test]
    fn test_alert_hook_trait_object_dispatch() {
        // Confirm trait-object dispatch works for heterogeneous hooks.
        let hooks: Vec<Box<dyn AlertHook>> = vec![
            Box::new(LogAlertHook::new()),
            Box::new(InMemoryAlertHook::new(
                "https://example.com/hook".to_string(),
            )),
        ];
        let alert = sample_alert(AlertLevel::Critical, Some("op"));
        for hook in &hooks {
            assert!(hook.notify(&alert).is_ok());
        }
        // The webhook hook stored one alert; verify via downcast-less approach by
        // constructing separately.
        let webhook = InMemoryAlertHook::new("https://example.com/hook".to_string());
        webhook.notify(&alert).unwrap();
        assert_eq!(webhook.sent_alerts().len(), 1);
    }

    // ===================== SlaMonitor / SlaReport tests =====================

    #[test]
    fn test_sla_monitor_new_empty() {
        let monitor = SlaMonitor::new(0.999);
        assert!(monitor.operations().is_empty());
    }

    #[test]
    fn test_sla_monitor_observe_creates_operation() {
        let monitor = SlaMonitor::new(0.999);
        monitor.observe("query", Duration::from_millis(50), true);
        let ops = monitor.operations();
        assert_eq!(ops, vec!["query".to_string()]);
    }

    #[test]
    fn test_sla_monitor_report_unknown_returns_none() {
        let monitor = SlaMonitor::new(0.999);
        assert!(monitor.report("unknown").is_none());
    }

    #[test]
    fn test_sla_monitor_report_basic_stats_all_success() {
        let monitor = SlaMonitor::new(0.999);
        for ms in [10, 20, 30, 40, 50] {
            monitor.observe("op", Duration::from_millis(ms), true);
        }
        let report = monitor.report("op").expect("report must exist");
        assert_eq!(report.total_count, 5);
        assert_eq!(report.error_rate, 0.0);
        assert!((report.slo_target - 0.999).abs() < 1e-9);
        // p50 of [10,20,30,40,50] with nearest rank: rank=ceil(0.5*5)=3, samples[2]=30
        assert!((report.p50_ms - 30.0).abs() < 1e-9);
        // No errors -> full budget remaining, zero saturation.
        assert!((report.error_budget_remaining - 1.0).abs() < 1e-9);
        assert!((report.saturation - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_sla_monitor_report_with_errors_over_budget() {
        let monitor = SlaMonitor::new(0.999);
        // 8 success, 2 failures -> error_rate = 0.2
        for _ in 0..8 {
            monitor.observe("op", Duration::from_millis(10), true);
        }
        for _ in 0..2 {
            monitor.observe("op", Duration::from_millis(10), false);
        }
        let report = monitor.report("op").expect("report must exist");
        assert_eq!(report.total_count, 10);
        assert!((report.error_rate - 0.2).abs() < 1e-9);
        // error_budget = 1 - 0.999 = 0.001
        // 0.2 / 0.001 = 200x over budget -> clamped to saturation = 1.0
        assert!((report.saturation - 1.0).abs() < 1e-9);
        assert!((report.error_budget_remaining - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_sla_monitor_report_partial_budget() {
        // SLO 0.9 -> error_budget = 0.1
        // 1 error out of 20 -> error_rate = 0.05
        // saturation = 0.05 / 0.1 = 0.5
        // remaining = 1 - 0.5 = 0.5
        let monitor = SlaMonitor::new(0.9);
        for i in 0..20 {
            monitor.observe("op", Duration::from_millis(i), i != 5);
        }
        let report = monitor.report("op").expect("report");
        assert!((report.error_rate - 0.05).abs() < 1e-9);
        assert!((report.saturation - 0.5).abs() < 1e-9);
        assert!((report.error_budget_remaining - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_sla_monitor_operations_isolated() {
        let monitor = SlaMonitor::new(0.999);
        monitor.observe("op1", Duration::from_millis(10), true);
        monitor.observe("op2", Duration::from_millis(20), false);
        let mut ops = monitor.operations();
        ops.sort();
        assert_eq!(ops, vec!["op1".to_string(), "op2".to_string()]);

        let r1 = monitor.report("op1").expect("op1 report");
        let r2 = monitor.report("op2").expect("op2 report");
        assert_eq!(r1.total_count, 1);
        assert_eq!(r2.total_count, 1);
        assert!((r1.error_rate - 0.0).abs() < 1e-9);
        assert!((r2.error_rate - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_sla_monitor_p95_p99_high_percentile() {
        let monitor = SlaMonitor::new(0.999);
        for ms in [1, 2, 3, 4, 5, 6, 7, 8, 9, 100] {
            monitor.observe("op", Duration::from_millis(ms), true);
        }
        let report = monitor.report("op").expect("report");
        // p95/p99 with nearest rank n=10: ceil(0.95*10)=10, ceil(0.99*10)=10 -> samples[9]=100
        assert!((report.p95_ms - 100.0).abs() < 1e-9);
        assert!((report.p99_ms - 100.0).abs() < 1e-9);
    }

    #[test]
    fn test_sla_monitor_observe_aggregates_multiple_calls() {
        let monitor = SlaMonitor::new(0.999);
        for _ in 0..100 {
            monitor.observe("op", Duration::from_millis(5), true);
        }
        let report = monitor.report("op").expect("report");
        assert_eq!(report.total_count, 100);
        assert!((report.p50_ms - 5.0).abs() < 1e-9);
    }

    #[test]
    fn test_sla_monitor_implements_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SlaMonitor>();
    }

    #[test]
    fn test_sla_monitor_concurrent_observe_thread_safe() {
        use std::sync::Arc;
        use std::thread;

        let monitor = Arc::new(SlaMonitor::new(0.999));
        let mut handles = vec![];

        for t in 0..4 {
            let m = Arc::clone(&monitor);
            handles.push(thread::spawn(move || {
                for i in 0..100 {
                    // 10% failure rate (i % 10 == 0)
                    m.observe("op", Duration::from_millis(i as u64), i % 10 != 0);
                }
                // Touch another operation per thread to verify isolation.
                let op_name = format!("thread-{t}");
                m.observe(&op_name, Duration::from_millis(1), true);
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        let report = monitor.report("op").expect("op report");
        assert_eq!(report.total_count, 400);
        // 10% error rate
        assert!((report.error_rate - 0.1).abs() < 1e-9);

        // Each thread registered its own operation.
        let mut ops = monitor.operations();
        ops.sort();
        // "op" + 4 thread-specific operations
        assert_eq!(ops.len(), 5);
        assert!(ops.contains(&"op".to_string()));
    }

    #[test]
    fn test_sla_monitor_report_slo_one_with_no_errors() {
        // SLO = 1.0 with no errors -> full budget, zero saturation.
        let monitor = SlaMonitor::new(1.0);
        monitor.observe("op", Duration::from_millis(10), true);
        let report = monitor.report("op").expect("report");
        assert!((report.error_budget_remaining - 1.0).abs() < 1e-9);
        assert!((report.saturation - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_sla_monitor_report_slo_one_with_errors() {
        // SLO = 1.0 with any error -> budget fully exhausted.
        let monitor = SlaMonitor::new(1.0);
        monitor.observe("op", Duration::from_millis(10), false);
        let report = monitor.report("op").expect("report");
        assert!((report.error_budget_remaining - 0.0).abs() < 1e-9);
        assert!((report.saturation - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_sla_report_fields_are_public() {
        // Verifies all SlaReport fields are accessible per the spec.
        let monitor = SlaMonitor::new(0.999);
        monitor.observe("op", Duration::from_millis(10), true);
        let r = monitor.report("op").unwrap();
        let _p50: f64 = r.p50_ms;
        let _p95: f64 = r.p95_ms;
        let _p99: f64 = r.p99_ms;
        let _erate: f64 = r.error_rate;
        let _total: usize = r.total_count;
        let _slo: f64 = r.slo_target;
        let _ebr: f64 = r.error_budget_remaining;
        let _sat: f64 = r.saturation;
    }
}

// ============================================================================
// OTLP Exporter（feature = "otlp"）
// ============================================================================

/// OTLP exporter configuration
///
/// Exports SZ-ORM tracing Spans to a Collector via the OpenTelemetry OTLP protocol.
///
/// # Examples
///
/// ```no_run
/// # #[cfg(feature = "otlp")] {
/// use sz_orm_tracing::OtlpConfig;
///
/// let config = OtlpConfig {
///     endpoint: "http://localhost:4317".to_string(),
///     service_name: "sz-orm-app".to_string(),
///     timeout_ms: 5000,
/// };
/// # }
/// ```
#[cfg(feature = "otlp")]
#[derive(Debug, Clone)]
pub struct OtlpConfig {
    /// OTLP gRPC endpoint (e.g., `http://localhost:4317`)
    pub endpoint: String,
    /// Service name (appears as the service.name tag in traces)
    pub service_name: String,
    /// Export timeout (milliseconds)
    pub timeout_ms: u64,
}

#[cfg(feature = "otlp")]
impl Default for OtlpConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:4317".to_string(),
            service_name: "sz-orm".to_string(),
            timeout_ms: 5000,
        }
    }
}

/// Initialize the OTLP exporter
///
/// Integrates SZ-ORM tracing with OpenTelemetry so that Spans are automatically exported to the Collector.
///
/// # Errors
///
/// - `TracingError::OtlpInitFailed`: initialization failed
///
/// # Examples
///
/// ```no_run
/// # #[cfg(feature = "otlp")] {
/// # let rt = tokio::runtime::Runtime::new().unwrap();
/// # rt.block_on(async {
/// use sz_orm_tracing::{init_otlp_exporter, OtlpConfig};
///
/// let _guard = init_otlp_exporter(OtlpConfig::default()).await.unwrap();
/// // 此后通过 `Tracer` 上报的 Span 将自动导出到 OTLP Collector
/// # });
/// # }
/// ```
#[cfg(feature = "otlp")]
pub async fn init_otlp_exporter(config: OtlpConfig) -> Result<OtlpGuard, TracingError> {
    use opentelemetry_otlp::{SpanExporter, WithExportConfig};
    use opentelemetry_sdk::resource::Resource;
    use opentelemetry_sdk::runtime::Tokio;
    use opentelemetry_sdk::trace::TracerProvider;
    use std::time::Duration;

    let exporter = SpanExporter::builder()
        .with_tonic()
        .with_endpoint(config.endpoint.clone())
        .with_timeout(Duration::from_millis(config.timeout_ms))
        .build()
        .map_err(|e| TracingError::OtlpInitFailed(format!("exporter build: {e}")))?;

    let provider = TracerProvider::builder()
        .with_batch_exporter(exporter, Tokio)
        .with_resource(Resource::new_with_defaults([opentelemetry::KeyValue::new(
            "service.name",
            config.service_name.clone(),
        )]))
        .build();

    // 设置为全局 provider
    opentelemetry::global::set_tracer_provider(provider.clone());

    Ok(OtlpGuard { provider })
}

/// OTLP exporter guard
///
/// Gracefully shuts down the exporter on drop, ensuring all Spans have been exported.
#[cfg(feature = "otlp")]
pub struct OtlpGuard {
    provider: opentelemetry_sdk::trace::TracerProvider,
}

#[cfg(feature = "otlp")]
impl Drop for OtlpGuard {
    fn drop(&mut self) {
        // 优雅关闭：未发送的 span 会被丢弃（不阻塞）
        let _ = self.provider.shutdown();
    }
}
