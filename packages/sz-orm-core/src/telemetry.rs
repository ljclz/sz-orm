//! # 可观测性遥测（Telemetry）
//!
//! 提供结构化链路追踪 span + 指标记录，基于 `tracing` crate。
//!
//! ## OpenTelemetry 桥接
//!
//! 本模块仅创建 `tracing` span，不直接依赖 `opentelemetry` SDK。
//! 用户可通过 `tracing-opentelemetry` 桥接器将 span 导出为 OpenTelemetry 格式：
//!
//! ```ignore
//! use tracing_subscriber::layer::SubscriberExt;
//! use tracing_opentelemetry::OpenTelemetryLayer;
//!
//! let tracer = opentelemetry::global::tracer("sz-orm");
//! let telemetry_layer = OpenTelemetryLayer::new(tracer);
//! let subscriber = tracing_subscriber::fmt::layer()
//!     .with_subscriber(tracing_subscriber::registry().with(telemetry_layer));
//! tracing::subscriber::set_global_default(subscriber).unwrap();
//! ```
//!
//! ## 指标
//!
//! - `sz_orm_query_duration` — 查询耗时（毫秒）
//! - `sz_orm_query_rows` — 查询返回行数
//! - `sz_orm_pool_acquire_duration` — 连接获取耗时（毫秒）
//! - `sz_orm_pool_size` — 当前连接池大小

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::Instrument;

/// 遥测配置
#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    /// 服务名（用于 span 属性 `service.name`）
    pub service_name: String,
    /// 是否启用查询 span（默认 true）
    pub enable_query_span: bool,
    /// 是否连接池 span（默认 true）
    pub enable_pool_span: bool,
    /// 采样率（0.0 ~ 1.0，1.0 = 全采样，默认 1.0）
    pub sample_rate: f64,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            service_name: "sz-orm".to_string(),
            enable_query_span: true,
            enable_pool_span: true,
            sample_rate: 1.0,
        }
    }
}

impl TelemetryConfig {
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
            ..Default::default()
        }
    }

    pub fn with_sample_rate(mut self, rate: f64) -> Self {
        self.sample_rate = rate.clamp(0.0, 1.0);
        self
    }

    pub fn with_query_span(mut self, enabled: bool) -> Self {
        self.enable_query_span = enabled;
        self
    }

    pub fn with_pool_span(mut self, enabled: bool) -> Self {
        self.enable_pool_span = enabled;
        self
    }
}

/// 遥测指标计数器（无锁原子操作）
#[derive(Debug, Default)]
pub struct TelemetryMetrics {
    /// 累计查询次数
    query_count: AtomicU64,
    /// 累计查询耗时（纳秒）
    query_total_duration_ns: AtomicU64,
    /// 累计查询行数
    query_total_rows: AtomicU64,
    /// 累计连接获取次数
    pool_acquire_count: AtomicU64,
    /// 累计连接获取耗时（纳秒）
    pool_acquire_total_duration_ns: AtomicU64,
    /// 累计查询错误次数
    query_error_count: AtomicU64,
    /// v3.2.0：累计预热成功次数
    prewarm_count: AtomicU64,
    /// v3.2.0：累计预热失败次数
    prewarm_failed_count: AtomicU64,
    /// v3.2.0：累计预热耗时（纳秒）
    prewarm_duration_ns: AtomicU64,
}

impl TelemetryMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录一次查询
    pub fn record_query(&self, duration: Duration, rows: u64) {
        self.query_count.fetch_add(1, Ordering::Relaxed);
        self.query_total_duration_ns
            .fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);
        self.query_total_rows.fetch_add(rows, Ordering::Relaxed);
    }

    /// 记录一次查询错误
    pub fn record_query_error(&self) {
        self.query_error_count.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录一次连接获取
    pub fn record_pool_acquire(&self, duration: Duration) {
        self.pool_acquire_count.fetch_add(1, Ordering::Relaxed);
        self.pool_acquire_total_duration_ns
            .fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);
    }

    /// v3.2.0：记录预热成功
    pub fn record_prewarm_success(&self) {
        self.prewarm_count.fetch_add(1, Ordering::Relaxed);
    }

    /// v3.2.0：记录预热失败
    pub fn record_prewarm_failure(&self) {
        self.prewarm_failed_count.fetch_add(1, Ordering::Relaxed);
    }

    /// v3.2.0：记录预热耗时
    pub fn record_prewarm_duration(&self, duration: Duration) {
        self.prewarm_duration_ns
            .fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);
    }

    /// 获取指标快照
    pub fn snapshot(&self) -> TelemetryMetricsSnapshot {
        TelemetryMetricsSnapshot {
            query_count: self.query_count.load(Ordering::Relaxed),
            query_total_duration: Duration::from_nanos(
                self.query_total_duration_ns.load(Ordering::Relaxed),
            ),
            query_total_rows: self.query_total_rows.load(Ordering::Relaxed),
            pool_acquire_count: self.pool_acquire_count.load(Ordering::Relaxed),
            pool_acquire_total_duration: Duration::from_nanos(
                self.pool_acquire_total_duration_ns.load(Ordering::Relaxed),
            ),
            query_error_count: self.query_error_count.load(Ordering::Relaxed),
            prewarm_count: self.prewarm_count.load(Ordering::Relaxed),
            prewarm_failed_count: self.prewarm_failed_count.load(Ordering::Relaxed),
            prewarm_duration: Duration::from_nanos(
                self.prewarm_duration_ns.load(Ordering::Relaxed),
            ),
        }
    }
}

/// 指标快照
#[derive(Debug, Clone)]
pub struct TelemetryMetricsSnapshot {
    pub query_count: u64,
    pub query_total_duration: Duration,
    pub query_total_rows: u64,
    pub pool_acquire_count: u64,
    pub pool_acquire_total_duration: Duration,
    pub query_error_count: u64,
    /// v3.2.0：预热成功次数
    pub prewarm_count: u64,
    /// v3.2.0：预热失败次数
    pub prewarm_failed_count: u64,
    /// v3.2.0：预热总耗时
    pub prewarm_duration: Duration,
}

impl TelemetryMetricsSnapshot {
    /// 平均查询耗时
    pub fn avg_query_duration(&self) -> Duration {
        if self.query_count == 0 {
            Duration::ZERO
        } else {
            self.query_total_duration / self.query_count as u32
        }
    }

    /// 平均查询行数
    pub fn avg_query_rows(&self) -> f64 {
        if self.query_count == 0 {
            0.0
        } else {
            self.query_total_rows as f64 / self.query_count as f64
        }
    }

    /// 查询错误率
    pub fn query_error_rate(&self) -> f64 {
        if self.query_count == 0 {
            0.0
        } else {
            self.query_error_count as f64 / self.query_count as f64
        }
    }

    /// 平均连接获取耗时
    pub fn avg_pool_acquire_duration(&self) -> Duration {
        if self.pool_acquire_count == 0 {
            Duration::ZERO
        } else {
            self.pool_acquire_total_duration / self.pool_acquire_count as u32
        }
    }
}

/// 遥测上下文 — 持有配置和指标
#[derive(Clone)]
pub struct Telemetry {
    config: Arc<TelemetryConfig>,
    metrics: Arc<TelemetryMetrics>,
}

impl Telemetry {
    pub fn new(config: TelemetryConfig) -> Self {
        Self {
            config: Arc::new(config),
            metrics: Arc::new(TelemetryMetrics::new()),
        }
    }

    pub fn config(&self) -> &TelemetryConfig {
        &self.config
    }

    pub fn metrics(&self) -> TelemetryMetricsSnapshot {
        self.metrics.snapshot()
    }

    /// 创建查询 span
    ///
    /// 返回一个 `QuerySpanGuard`，drop 时自动记录耗时和行数。
    pub fn query_span(&self, sql: &str) -> QuerySpanGuard {
        if !self.config.enable_query_span {
            return QuerySpanGuard::disabled(self.metrics.clone());
        }
        let span = tracing::info_span!(
            "sz_orm_query",
            "otel.name" = "sz_orm.query",
            "otel.kind" = "client",
            "service.name" = %self.config.service_name,
            sql = %sql,
        );
        QuerySpanGuard::enabled(self.metrics.clone(), span, Instant::now())
    }

    /// 创建连接获取 span
    pub fn pool_acquire_span(&self) -> PoolAcquireSpanGuard {
        if !self.config.enable_pool_span {
            return PoolAcquireSpanGuard::disabled(self.metrics.clone());
        }
        let span = tracing::info_span!(
            "sz_orm_pool_acquire",
            "otel.name" = "sz_orm.pool.acquire",
            "otel.kind" = "internal",
            "service.name" = %self.config.service_name,
        );
        PoolAcquireSpanGuard::enabled(self.metrics.clone(), span, Instant::now())
    }

    /// 用查询 span 包装异步查询闭包
    ///
    /// ```ignore
    /// let rows = telemetry.with_query_span("SELECT * FROM users", || async {
    ///     conn.query(sql).await
    /// }).await;
    /// ```
    pub async fn with_query_span<F, Fut, T>(&self, sql: &str, f: F) -> T
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        if !self.config.enable_query_span {
            let start = Instant::now();
            let result = f().await;
            self.metrics.record_query(start.elapsed(), 0);
            return result;
        }
        let span = tracing::info_span!(
            "sz_orm_query",
            "otel.name" = "sz_orm.query",
            "otel.kind" = "client",
            "service.name" = %self.config.service_name,
            sql = %sql,
        );
        let start = Instant::now();
        let result = f().instrument(span).await;
        self.metrics.record_query(start.elapsed(), 0);
        result
    }
}

impl Default for Telemetry {
    fn default() -> Self {
        Self::new(TelemetryConfig::default())
    }
}

/// 查询 span 守卫 — drop 时自动记录指标
pub struct QuerySpanGuard {
    metrics: Arc<TelemetryMetrics>,
    span: Option<tracing::span::Span>,
    start: Instant,
    rows: u64,
    error: bool,
}

impl QuerySpanGuard {
    fn enabled(metrics: Arc<TelemetryMetrics>, span: tracing::span::Span, start: Instant) -> Self {
        Self {
            metrics,
            span: Some(span),
            start,
            rows: 0,
            error: false,
        }
    }

    fn disabled(metrics: Arc<TelemetryMetrics>) -> Self {
        Self {
            metrics,
            span: None,
            start: Instant::now(),
            rows: 0,
            error: false,
        }
    }

    /// 记录返回行数
    pub fn set_rows(&mut self, rows: u64) {
        self.rows = rows;
    }

    /// 标记查询出错
    pub fn set_error(&mut self) {
        self.error = true;
    }

    /// 进入 span 上下文
    pub fn enter(&self) -> Option<tracing::span::Entered<'_>> {
        self.span.as_ref().map(|s| s.enter())
    }
}

impl Drop for QuerySpanGuard {
    fn drop(&mut self) {
        let duration = self.start.elapsed();
        self.metrics.record_query(duration, self.rows);
        if self.error {
            self.metrics.record_query_error();
        }
        if let Some(ref span) = self.span {
            span.record("query.duration_ms", duration.as_millis() as u64);
            span.record("query.rows", self.rows);
            span.record("query.error", self.error);
        }
    }
}

/// 连接获取 span 守卫
pub struct PoolAcquireSpanGuard {
    metrics: Arc<TelemetryMetrics>,
    span: Option<tracing::span::Span>,
    start: Instant,
}

impl PoolAcquireSpanGuard {
    fn enabled(metrics: Arc<TelemetryMetrics>, span: tracing::span::Span, start: Instant) -> Self {
        Self {
            metrics,
            span: Some(span),
            start,
        }
    }

    fn disabled(metrics: Arc<TelemetryMetrics>) -> Self {
        Self {
            metrics,
            span: None,
            start: Instant::now(),
        }
    }

    pub fn enter(&self) -> Option<tracing::span::Entered<'_>> {
        self.span.as_ref().map(|s| s.enter())
    }
}

impl Drop for PoolAcquireSpanGuard {
    fn drop(&mut self) {
        let duration = self.start.elapsed();
        self.metrics.record_pool_acquire(duration);
        if let Some(ref span) = self.span {
            span.record("pool.acquire_duration_ms", duration.as_millis() as u64);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telemetry_config_defaults() {
        let config = TelemetryConfig::default();
        assert_eq!(config.service_name, "sz-orm");
        assert!(config.enable_query_span);
        assert!(config.enable_pool_span);
        assert_eq!(config.sample_rate, 1.0);
    }

    #[test]
    fn test_telemetry_config_builders() {
        let config = TelemetryConfig::new("my-service")
            .with_sample_rate(0.5)
            .with_query_span(false)
            .with_pool_span(false);
        assert_eq!(config.service_name, "my-service");
        assert_eq!(config.sample_rate, 0.5);
        assert!(!config.enable_query_span);
        assert!(!config.enable_pool_span);
    }

    #[test]
    fn test_sample_rate_clamped() {
        let config = TelemetryConfig::default().with_sample_rate(2.0);
        assert_eq!(config.sample_rate, 1.0);

        let config = TelemetryConfig::default().with_sample_rate(-1.0);
        assert_eq!(config.sample_rate, 0.0);
    }

    #[test]
    fn test_metrics_record_query() {
        let metrics = TelemetryMetrics::new();
        metrics.record_query(Duration::from_millis(100), 10);
        metrics.record_query(Duration::from_millis(200), 20);

        let snap = metrics.snapshot();
        assert_eq!(snap.query_count, 2);
        assert_eq!(snap.query_total_duration, Duration::from_millis(300));
        assert_eq!(snap.query_total_rows, 30);
        assert_eq!(snap.avg_query_duration(), Duration::from_millis(150));
        assert_eq!(snap.avg_query_rows(), 15.0);
    }

    #[test]
    fn test_metrics_record_error() {
        let metrics = TelemetryMetrics::new();
        metrics.record_query(Duration::from_millis(50), 0);
        metrics.record_query_error();
        metrics.record_query(Duration::from_millis(100), 5);

        let snap = metrics.snapshot();
        assert_eq!(snap.query_count, 2);
        assert_eq!(snap.query_error_count, 1);
        assert_eq!(snap.query_error_rate(), 0.5);
    }

    #[test]
    fn test_metrics_record_pool_acquire() {
        let metrics = TelemetryMetrics::new();
        metrics.record_pool_acquire(Duration::from_millis(10));
        metrics.record_pool_acquire(Duration::from_millis(30));

        let snap = metrics.snapshot();
        assert_eq!(snap.pool_acquire_count, 2);
        assert_eq!(snap.pool_acquire_total_duration, Duration::from_millis(40));
        assert_eq!(snap.avg_pool_acquire_duration(), Duration::from_millis(20));
    }

    #[test]
    fn test_metrics_empty_snapshot() {
        let metrics = TelemetryMetrics::new();
        let snap = metrics.snapshot();
        assert_eq!(snap.query_count, 0);
        assert_eq!(snap.avg_query_duration(), Duration::ZERO);
        assert_eq!(snap.avg_query_rows(), 0.0);
        assert_eq!(snap.query_error_rate(), 0.0);
        assert_eq!(snap.avg_pool_acquire_duration(), Duration::ZERO);
    }

    #[test]
    fn test_query_span_guard_records_on_drop() {
        let telemetry = Telemetry::default();
        {
            let mut guard = telemetry.query_span("SELECT 1");
            guard.set_rows(42);
        }
        let snap = telemetry.metrics();
        assert_eq!(snap.query_count, 1);
        assert_eq!(snap.query_total_rows, 42);
    }

    #[test]
    fn test_query_span_guard_error() {
        let telemetry = Telemetry::default();
        {
            let mut guard = telemetry.query_span("SELECT bad");
            guard.set_error();
        }
        let snap = telemetry.metrics();
        assert_eq!(snap.query_count, 1);
        assert_eq!(snap.query_error_count, 1);
        assert_eq!(snap.query_error_rate(), 1.0);
    }

    #[test]
    fn test_pool_acquire_span_guard_records_on_drop() {
        let telemetry = Telemetry::default();
        {
            let _guard = telemetry.pool_acquire_span();
        }
        let snap = telemetry.metrics();
        assert_eq!(snap.pool_acquire_count, 1);
    }

    #[tokio::test]
    async fn test_with_query_span_async() {
        let telemetry = Telemetry::default();
        let result = telemetry.with_query_span("SELECT 1", || async { 42 }).await;
        assert_eq!(result, 42);
        let snap = telemetry.metrics();
        assert_eq!(snap.query_count, 1);
    }

    #[test]
    fn test_telemetry_disabled_spans() {
        let config = TelemetryConfig::default()
            .with_query_span(false)
            .with_pool_span(false);
        let telemetry = Telemetry::new(config);
        {
            let _guard = telemetry.query_span("SELECT 1");
        }
        {
            let _guard = telemetry.pool_acquire_span();
        }
        let snap = telemetry.metrics();
        assert_eq!(snap.query_count, 1);
        assert_eq!(snap.pool_acquire_count, 1);
    }

    #[test]
    fn test_telemetry_clone_shares_metrics() {
        let telemetry = Telemetry::default();
        let telemetry2 = telemetry.clone();
        {
            let _guard = telemetry.query_span("SELECT 1");
        }
        {
            let _guard = telemetry2.query_span("SELECT 2");
        }
        let snap = telemetry.metrics();
        assert_eq!(snap.query_count, 2);
    }
}
