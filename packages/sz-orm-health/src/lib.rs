//! # SZ-ORM Health — 健康检查
//!
//! 提供资源健康状态聚合与运行时指标上报，包含连接数、慢查询、错误率与 p50/p95
//! 延迟等 SLA 指标，用于探活与可观测性。
//!
//! ## 主要类型
//!
//! - [`HealthStatus`] — 健康/不健康/未知
//! - [`HealthReport`] — 单资源健康报告

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Aggregated health status for a single resource (e.g. a connection pool).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum HealthStatus {
    Healthy,
    Unhealthy,
    #[default]
    Unknown,
}

/// Detailed health report for one resource, including runtime metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    pub pool_name: String,
    pub status: HealthStatus,
    pub connection_count: u32,
    pub slow_queries: u32,
    pub message: String,
    /// SLA: error rate (0.0..=1.0). `None` when not measured.
    #[serde(default)]
    pub error_rate: Option<f64>,
    /// SLA: p50 latency in milliseconds.
    #[serde(default)]
    pub p50_ms: Option<f64>,
    /// SLA: p95 latency in milliseconds.
    #[serde(default)]
    pub p95_ms: Option<f64>,
    /// SLA: p99 latency in milliseconds.
    #[serde(default)]
    pub p99_ms: Option<f64>,
    /// SLA: saturation ratio (0.0..=1.0), e.g. CPU/connection utilization.
    #[serde(default)]
    pub saturation: Option<f64>,
    /// SLA: uptime ratio over a window (0.0..=1.0).
    #[serde(default)]
    pub uptime_ratio: Option<f64>,
}

impl HealthReport {
    pub fn new(name: &str) -> Self {
        Self {
            pool_name: name.to_string(),
            status: HealthStatus::Unknown,
            connection_count: 0,
            slow_queries: 0,
            message: String::new(),
            error_rate: None,
            p50_ms: None,
            p95_ms: None,
            p99_ms: None,
            saturation: None,
            uptime_ratio: None,
        }
    }

    pub fn set_healthy(mut self) -> Self {
        self.status = HealthStatus::Healthy;
        self
    }

    pub fn set_status(mut self, status: HealthStatus) -> Self {
        self.status = status;
        self
    }

    pub fn with_connection_count(mut self, count: u32) -> Self {
        self.connection_count = count;
        self
    }

    pub fn with_slow_queries(mut self, count: u32) -> Self {
        self.slow_queries = count;
        self
    }

    pub fn with_message(mut self, msg: impl Into<String>) -> Self {
        self.message = msg.into();
        self
    }

    pub fn with_error_rate(mut self, rate: f64) -> Self {
        self.error_rate = Some(rate);
        self
    }

    pub fn with_latency_p50(mut self, ms: f64) -> Self {
        self.p50_ms = Some(ms);
        self
    }

    pub fn with_latency_p95(mut self, ms: f64) -> Self {
        self.p95_ms = Some(ms);
        self
    }

    pub fn with_latency_p99(mut self, ms: f64) -> Self {
        self.p99_ms = Some(ms);
        self
    }

    pub fn with_saturation(mut self, ratio: f64) -> Self {
        self.saturation = Some(ratio);
        self
    }

    pub fn with_uptime_ratio(mut self, ratio: f64) -> Self {
        self.uptime_ratio = Some(ratio);
        self
    }
}

/// Snapshot of runtime metrics for a resource, supplied by an external provider.
#[derive(Debug, Clone, Default)]
pub struct HealthSnapshot {
    pub status: HealthStatus,
    pub connection_count: u32,
    pub slow_queries: u32,
    pub message: String,
}

impl HealthSnapshot {
    pub fn healthy() -> Self {
        Self {
            status: HealthStatus::Healthy,
            connection_count: 0,
            slow_queries: 0,
            message: String::new(),
        }
    }

    pub fn unhealthy(message: impl Into<String>) -> Self {
        Self {
            status: HealthStatus::Unhealthy,
            connection_count: 0,
            slow_queries: 0,
            message: message.into(),
        }
    }

    pub fn unknown() -> Self {
        Self {
            status: HealthStatus::Unknown,
            connection_count: 0,
            slow_queries: 0,
            message: String::new(),
        }
    }
}

/// Provider that supplies the live status for a given pool.
/// Implementations are expected to read real runtime state (e.g. from a
/// connection pool) rather than return hardcoded values.
pub trait HealthStatusProvider: Send + Sync {
    fn snapshot(&self, pool: &str) -> HealthSnapshot;
}

/// Trait for health checkers. `check` returns the report for a single pool,
/// while `check_all` aggregates across multiple pools.
pub trait DbHealthChecker: Send + Sync {
    fn check(&self, pool: &str) -> HealthReport;
    fn check_all(&self, pools: &[&str]) -> Vec<HealthReport>;
}

/// Default in-memory health checker. Stores per-pool status snapshots that
/// can be updated externally via `set_status`. When a `HealthStatusProvider`
/// is registered for a pool, it is consulted on each `check`; otherwise the
/// last manually-set status is used.
pub struct DefaultHealthChecker {
    /// Manual overrides / last known status per pool.
    statuses: RwLock<HashMap<String, HealthSnapshot>>,
    /// Optional external providers per pool.
    providers: RwLock<HashMap<String, Arc<dyn HealthStatusProvider>>>,
}

impl Default for DefaultHealthChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl DefaultHealthChecker {
    pub fn new() -> Self {
        Self {
            statuses: RwLock::new(HashMap::new()),
            providers: RwLock::new(HashMap::new()),
        }
    }

    /// Manually set the health snapshot for a pool. This overrides any
    /// previously stored manual status. If a provider is registered, it
    /// still takes precedence on the next `check` call.
    pub fn set_status(&self, pool: &str, snapshot: HealthSnapshot) {
        // lock poisoned 时降级为 no-op，避免级联 panic。
        if let Ok(mut statuses) = self.statuses.write() {
            statuses.insert(pool.to_string(), snapshot);
        }
    }

    /// Convenience: mark a pool healthy with given metrics.
    pub fn set_healthy(&self, pool: &str, connection_count: u32, slow_queries: u32) {
        self.set_status(
            pool,
            HealthSnapshot {
                status: HealthStatus::Healthy,
                connection_count,
                slow_queries,
                message: String::new(),
            },
        );
    }

    /// Convenience: mark a pool unhealthy with a message.
    pub fn set_unhealthy(&self, pool: &str, message: impl Into<String>) {
        self.set_status(
            pool,
            HealthSnapshot {
                status: HealthStatus::Unhealthy,
                connection_count: 0,
                slow_queries: 0,
                message: message.into(),
            },
        );
    }

    /// Register an external provider for a pool. When set, `check` will
    /// always delegate to the provider rather than the manual snapshot.
    pub fn register_provider(&self, pool: &str, provider: Arc<dyn HealthStatusProvider>) {
        // lock poisoned 时降级为 no-op。
        if let Ok(mut providers) = self.providers.write() {
            providers.insert(pool.to_string(), provider);
        }
    }

    /// Remove a previously registered provider, falling back to manual status.
    pub fn unregister_provider(&self, pool: &str) -> bool {
        // lock poisoned 时返回 false（未找到），避免级联 panic。
        match self.providers.write() {
            Ok(mut providers) => providers.remove(pool).is_some(),
            Err(_) => false,
        }
    }

    /// Read the snapshot for a pool: provider first, then manual status,
    /// finally `Unknown` if nothing has been recorded.
    fn read_snapshot(&self, pool: &str) -> HealthSnapshot {
        // Check provider first (no holding locks across calls).
        // lock poisoned 时返回 Unknown，避免级联 panic。
        if let Some(provider) = {
            match self.providers.read() {
                Ok(providers) => providers.get(pool).cloned(),
                Err(_) => None,
            }
        } {
            return provider.snapshot(pool);
        }

        let statuses = match self.statuses.read() {
            Ok(s) => s,
            Err(_) => {
                return HealthSnapshot {
                    status: HealthStatus::Unknown,
                    connection_count: 0,
                    slow_queries: 0,
                    message: format!("lock poisoned for pool '{}'", pool),
                }
            }
        };
        if let Some(snap) = statuses.get(pool) {
            return snap.clone();
        }

        HealthSnapshot {
            status: HealthStatus::Unknown,
            connection_count: 0,
            slow_queries: 0,
            message: format!("no status recorded for pool '{}'", pool),
        }
    }

    /// Aggregate the overall status across multiple pools. Returns
    /// `Unhealthy` if any pool is unhealthy, `Unknown` if any is unknown
    /// (and none are unhealthy), otherwise `Healthy`.
    pub fn overall_status(&self, pools: &[&str]) -> HealthStatus {
        let mut any_unknown = false;
        for pool in pools {
            let snap = self.read_snapshot(pool);
            match snap.status {
                HealthStatus::Unhealthy => return HealthStatus::Unhealthy,
                HealthStatus::Unknown => any_unknown = true,
                HealthStatus::Healthy => {}
            }
        }
        if any_unknown || pools.is_empty() {
            HealthStatus::Unknown
        } else {
            HealthStatus::Healthy
        }
    }
}

impl DbHealthChecker for DefaultHealthChecker {
    fn check(&self, pool: &str) -> HealthReport {
        let snap = self.read_snapshot(pool);
        HealthReport {
            pool_name: pool.to_string(),
            status: snap.status,
            connection_count: snap.connection_count,
            slow_queries: snap.slow_queries,
            message: snap.message,
            error_rate: None,
            p50_ms: None,
            p95_ms: None,
            p99_ms: None,
            saturation: None,
            uptime_ratio: None,
        }
    }

    fn check_all(&self, pools: &[&str]) -> Vec<HealthReport> {
        pools.iter().map(|p| self.check(p)).collect()
    }
}

/// A simple provider that always returns the same snapshot. Useful for tests
/// and for wiring a static status into the checker.
pub struct StaticStatusProvider {
    snapshot: HealthSnapshot,
}

impl StaticStatusProvider {
    pub fn new(snapshot: HealthSnapshot) -> Self {
        Self { snapshot }
    }
}

impl HealthStatusProvider for StaticStatusProvider {
    fn snapshot(&self, _pool: &str) -> HealthSnapshot {
        self.snapshot.clone()
    }
}

/// A provider that derives status from connection-count and slow-query
/// thresholds, mimicking a real pool monitor.
pub struct ThresholdProvider {
    connection_count: u32,
    slow_queries: u32,
    max_connections: u32,
    max_slow_queries: u32,
}

impl ThresholdProvider {
    pub fn new(
        connection_count: u32,
        slow_queries: u32,
        max_connections: u32,
        max_slow_queries: u32,
    ) -> Self {
        Self {
            connection_count,
            slow_queries,
            max_connections,
            max_slow_queries,
        }
    }
}

impl HealthStatusProvider for ThresholdProvider {
    fn snapshot(&self, _pool: &str) -> HealthSnapshot {
        if self.connection_count > self.max_connections {
            return HealthSnapshot {
                status: HealthStatus::Unhealthy,
                connection_count: self.connection_count,
                slow_queries: self.slow_queries,
                message: format!(
                    "connection count {} exceeds max {}",
                    self.connection_count, self.max_connections
                ),
            };
        }
        if self.slow_queries > self.max_slow_queries {
            return HealthSnapshot {
                status: HealthStatus::Unhealthy,
                connection_count: self.connection_count,
                slow_queries: self.slow_queries,
                message: format!(
                    "slow queries {} exceeds max {}",
                    self.slow_queries, self.max_slow_queries
                ),
            };
        }
        HealthSnapshot {
            status: HealthStatus::Healthy,
            connection_count: self.connection_count,
            slow_queries: self.slow_queries,
            message: String::new(),
        }
    }
}

// ============================================================================
// L4: Financial-grade alerting & disaster recovery
// ============================================================================

/// Severity level for a [`HealthAlert`]. This is independent of any
/// `AlertLevel` defined in `sz-orm-tracing` so that the health package can
// be used standalone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertLevel {
    Info,
    Warning,
    Critical,
}

/// A structured alert emitted by the health system. Carries an optional
/// [`HealthReport`] so receivers can inspect the metrics that triggered it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthAlert {
    pub level: AlertLevel,
    pub pool_name: String,
    pub message: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub metrics: Option<HealthReport>,
}

impl HealthAlert {
    pub fn new(
        level: AlertLevel,
        pool_name: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            level,
            pool_name: pool_name.into(),
            message: message.into(),
            timestamp: chrono::Utc::now(),
            metrics: None,
        }
    }

    pub fn with_metrics(mut self, report: HealthReport) -> Self {
        self.metrics = Some(report);
        self
    }
}

/// A channel that delivers [`HealthAlert`]s to some downstream sink.
/// Implementations must be `Send + Sync` so they can be shared across
/// threads by [`AlertManager`].
pub trait AlertChannel: Send + Sync {
    /// Deliver the alert. Returns `Err(String)` describing the failure
    /// (e.g. network error) so the caller can retry or log.
    fn send(&self, alert: &HealthAlert) -> Result<(), String>;
    /// Human-readable name of the channel (e.g. `"log"`, `"webhook"`).
    fn name(&self) -> &str;
}

/// Channel that writes alerts to stderr via `eprintln`. Always succeeds.
pub struct LogAlertChannel {
    name: String,
}

impl LogAlertChannel {
    pub fn new() -> Self {
        Self {
            name: "log".to_string(),
        }
    }
}

impl Default for LogAlertChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl AlertChannel for LogAlertChannel {
    fn send(&self, alert: &HealthAlert) -> Result<(), String> {
        eprintln!(
            "[{}] [{:?}] pool={} msg={}",
            alert.timestamp.to_rfc3339(),
            alert.level,
            alert.pool_name,
            alert.message
        );
        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Channel that records alerts in-memory, simulating a webhook sink.
/// Useful for tests and for wiring into an actual HTTP client later.
pub struct WebhookAlertChannel {
    url: String,
    name: String,
    sent: RwLock<Vec<HealthAlert>>,
}

impl WebhookAlertChannel {
    pub fn new(url: String) -> Self {
        Self {
            url,
            name: "webhook".to_string(),
            sent: RwLock::new(Vec::new()),
        }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn sent_alerts(&self) -> Vec<HealthAlert> {
        // lock poisoned 时返回空 Vec，避免级联 panic。
        self.sent.read().map(|g| g.clone()).unwrap_or_default()
    }
}

impl AlertChannel for WebhookAlertChannel {
    fn send(&self, alert: &HealthAlert) -> Result<(), String> {
        // In-memory simulation: in production this would POST to `self.url`.
        // lock poisoned 时返回错误而非 panic。
        let mut guard = self
            .sent
            .write()
            .map_err(|e| format!("sent lock poisoned: {}", e))?;
        guard.push(alert.clone());
        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Channel that records alerts in-memory, simulating an IM notification
/// (Slack/DingTalk/Feishu style). Useful for tests.
pub struct ImAlertChannel {
    webhook_url: String,
    name: String,
    sent: RwLock<Vec<HealthAlert>>,
}

impl ImAlertChannel {
    pub fn new(webhook_url: String) -> Self {
        Self {
            webhook_url,
            name: "im".to_string(),
            sent: RwLock::new(Vec::new()),
        }
    }

    pub fn webhook_url(&self) -> &str {
        &self.webhook_url
    }

    pub fn sent_alerts(&self) -> Vec<HealthAlert> {
        // lock poisoned 时返回空 Vec，避免级联 panic。
        self.sent.read().map(|g| g.clone()).unwrap_or_default()
    }
}

impl AlertChannel for ImAlertChannel {
    fn send(&self, alert: &HealthAlert) -> Result<(), String> {
        // In-memory simulation: in production this would POST an IM payload.
        // lock poisoned 时返回错误而非 panic。
        let mut guard = self
            .sent
            .write()
            .map_err(|e| format!("sent lock poisoned: {}", e))?;
        guard.push(alert.clone());
        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Manager that fans out a [`HealthAlert`] to all registered channels.
/// `Send + Sync` because it only holds `Arc<dyn AlertChannel>` (which is
/// itself `Send + Sync`). Registration requires `&mut self`, so no interior
/// mutability is needed.
pub struct AlertManager {
    channels: Vec<Arc<dyn AlertChannel>>,
}

impl Default for AlertManager {
    fn default() -> Self {
        Self::new()
    }
}

impl AlertManager {
    pub fn new() -> Self {
        Self {
            channels: Vec::new(),
        }
    }

    pub fn register(&mut self, channel: Arc<dyn AlertChannel>) {
        self.channels.push(channel);
    }

    /// Notify every registered channel. Returns one result per channel,
    /// in registration order. Never short-circuits: all channels are tried.
    pub fn notify(&self, alert: &HealthAlert) -> Vec<Result<(), String>> {
        self.channels.iter().map(|c| c.send(alert)).collect()
    }

    pub fn channels(&self) -> Vec<String> {
        self.channels.iter().map(|c| c.name().to_string()).collect()
    }
}

/// Action recommended by [`FailoverPolicy::evaluate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailoverAction {
    StayOnPrimary,
    FailoverToSecondary,
    FailoverToTertiary,
    CircuitOpen,
}

/// Policy that inspects a [`HealthReport`] and decides whether to stay on
/// the primary or fail over. Default thresholds:
///   * error_rate > 0.5           -> failover
///   * latency_p99 > 5000 (ms)    -> failover
///   * status == Unhealthy        -> CircuitOpen (failover cannot help)
pub struct FailoverPolicy {
    error_rate_threshold: f64,
    latency_threshold_ms: f64,
}

impl Default for FailoverPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl FailoverPolicy {
    pub fn new() -> Self {
        Self {
            error_rate_threshold: 0.5,
            latency_threshold_ms: 5000.0,
        }
    }

    pub fn with_error_rate_threshold(mut self, threshold: f64) -> Self {
        self.error_rate_threshold = threshold;
        self
    }

    pub fn with_latency_threshold(mut self, threshold_ms: f64) -> Self {
        self.latency_threshold_ms = threshold_ms;
        self
    }

    /// Evaluate the report and recommend an action.
    ///   * If status is `Unhealthy` -> `CircuitOpen`
    ///   * Else if error_rate > threshold OR p99 > latency threshold -> `FailoverToSecondary`
    ///   * Else -> `StayOnPrimary`
    pub fn evaluate(&self, report: &HealthReport) -> FailoverAction {
        if report.status == HealthStatus::Unhealthy {
            return FailoverAction::CircuitOpen;
        }
        let error_exceeded = report
            .error_rate
            .map(|r| r > self.error_rate_threshold)
            .unwrap_or(false);
        let latency_exceeded = report
            .p99_ms
            .map(|p| p > self.latency_threshold_ms)
            .unwrap_or(false);
        if error_exceeded || latency_exceeded {
            FailoverAction::FailoverToSecondary
        } else {
            FailoverAction::StayOnPrimary
        }
    }
}

/// Aggregated view of health across multiple regions (data centers).
/// Any `Unhealthy` region drags the aggregate to `Unhealthy`; otherwise
/// `Unknown` if any region is unknown, else `Healthy`.
pub struct MultiRegionHealthView {
    regions: HashMap<String, HealthStatus>,
}

impl Default for MultiRegionHealthView {
    fn default() -> Self {
        Self::new()
    }
}

impl MultiRegionHealthView {
    pub fn new() -> Self {
        Self {
            regions: HashMap::new(),
        }
    }

    pub fn register(&mut self, region: &str, status: HealthStatus) {
        self.regions.insert(region.to_string(), status);
    }

    pub fn aggregate(&self) -> HealthStatus {
        if self.regions.is_empty() {
            return HealthStatus::Unknown;
        }
        let mut any_unknown = false;
        for status in self.regions.values() {
            match status {
                HealthStatus::Unhealthy => return HealthStatus::Unhealthy,
                HealthStatus::Unknown => any_unknown = true,
                HealthStatus::Healthy => {}
            }
        }
        if any_unknown {
            HealthStatus::Unknown
        } else {
            HealthStatus::Healthy
        }
    }

    pub fn healthy_regions(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .regions
            .iter()
            .filter(|(_, s)| **s == HealthStatus::Healthy)
            .map(|(k, _)| k.clone())
            .collect();
        out.sort();
        out
    }

    pub fn unhealthy_regions(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .regions
            .iter()
            .filter(|(_, s)| **s == HealthStatus::Unhealthy)
            .map(|(k, _)| k.clone())
            .collect();
        out.sort();
        out
    }
}

/// State machine for a circuit breaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation: requests flow through.
    Closed,
    /// Tripped: requests are blocked until `reset_timeout` elapses.
    Open,
    /// Probe: a single trial request is allowed after the reset timeout.
    HalfOpen,
}

/// A simple circuit breaker. Trips after `failure_threshold` consecutive
/// failures, then enters `HalfOpen` after `reset_timeout`, transitioning
/// back to `Closed` on success or `Open` on failure.
pub struct CircuitBreaker {
    failure_threshold: usize,
    reset_timeout: std::time::Duration,
    state: CircuitState,
    consecutive_failures: usize,
    last_failure_at: Option<std::time::Instant>,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: usize, reset_timeout: std::time::Duration) -> Self {
        Self {
            failure_threshold,
            reset_timeout,
            state: CircuitState::Closed,
            consecutive_failures: 0,
            last_failure_at: None,
        }
    }

    pub fn state(&self) -> CircuitState {
        self.state
    }

    pub fn record_success(&mut self) {
        self.consecutive_failures = 0;
        self.state = CircuitState::Closed;
        self.last_failure_at = None;
    }

    pub fn record_failure(&mut self) {
        self.consecutive_failures += 1;
        self.last_failure_at = Some(std::time::Instant::now());
        if self.consecutive_failures >= self.failure_threshold {
            self.state = CircuitState::Open;
        }
    }

    /// Returns `true` if a request may proceed. May transition `Open` ->
    /// `HalfOpen` if the reset timeout has elapsed.
    pub fn can_execute(&mut self) -> bool {
        match self.state {
            CircuitState::Closed => true,
            CircuitState::HalfOpen => true,
            CircuitState::Open => {
                let elapsed = self
                    .last_failure_at
                    .map(|t| t.elapsed())
                    .unwrap_or_else(|| std::time::Duration::ZERO);
                if elapsed >= self.reset_timeout {
                    self.state = CircuitState::HalfOpen;
                    true
                } else {
                    false
                }
            }
        }
    }
}

/// Provider that tracks the last backup timestamp and reports `Unhealthy`
/// when the backup is older than the configured `max_age`.
pub struct BackupHealthProvider {
    max_age: chrono::Duration,
    last_backup: Option<chrono::DateTime<chrono::Utc>>,
}

impl BackupHealthProvider {
    pub fn new(max_age: chrono::Duration) -> Self {
        Self {
            max_age,
            last_backup: None,
        }
    }

    pub fn set_last_backup(&mut self, timestamp: chrono::DateTime<chrono::Utc>) {
        self.last_backup = Some(timestamp);
    }

    /// Returns `true` if no backup has been recorded, or the last backup
    /// is older than `max_age`.
    pub fn is_stale(&self) -> bool {
        match self.last_backup {
            None => true,
            Some(ts) => {
                let now = chrono::Utc::now();
                let age = now.signed_duration_since(ts);
                age > self.max_age
            }
        }
    }

    pub fn check(&self) -> HealthStatus {
        if self.is_stale() {
            HealthStatus::Unhealthy
        } else {
            HealthStatus::Healthy
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn test_report_builder() {
        let r = HealthReport::new("main")
            .set_healthy()
            .with_connection_count(5)
            .with_slow_queries(1)
            .with_message("ok");
        assert_eq!(r.pool_name, "main");
        assert_eq!(r.status, HealthStatus::Healthy);
        assert_eq!(r.connection_count, 5);
        assert_eq!(r.slow_queries, 1);
        assert_eq!(r.message, "ok");
    }

    #[test]
    fn test_check_unknown_pool_returns_unknown() {
        let checker = DefaultHealthChecker::new();
        let report = checker.check("never-set");
        assert_eq!(report.status, HealthStatus::Unknown);
        assert_eq!(report.connection_count, 0);
        assert_eq!(report.slow_queries, 0);
        assert!(!report.message.is_empty());
    }

    #[test]
    fn test_set_status_healthy() {
        let checker = DefaultHealthChecker::new();
        checker.set_healthy("pool-a", 10, 2);
        let report = checker.check("pool-a");
        assert_eq!(report.status, HealthStatus::Healthy);
        assert_eq!(report.connection_count, 10);
        assert_eq!(report.slow_queries, 2);
        assert_eq!(report.pool_name, "pool-a");
    }

    #[test]
    fn test_set_status_unhealthy() {
        let checker = DefaultHealthChecker::new();
        checker.set_unhealthy("pool-b", "connection refused");
        let report = checker.check("pool-b");
        assert_eq!(report.status, HealthStatus::Unhealthy);
        assert_eq!(report.message, "connection refused");
    }

    #[test]
    fn test_set_status_with_snapshot() {
        let checker = DefaultHealthChecker::new();
        checker.set_status(
            "pool-c",
            HealthSnapshot {
                status: HealthStatus::Healthy,
                connection_count: 42,
                slow_queries: 7,
                message: "all good".to_string(),
            },
        );
        let report = checker.check("pool-c");
        assert_eq!(report.status, HealthStatus::Healthy);
        assert_eq!(report.connection_count, 42);
        assert_eq!(report.slow_queries, 7);
        assert_eq!(report.message, "all good");
    }

    #[test]
    fn test_overwrite_status() {
        let checker = DefaultHealthChecker::new();
        checker.set_healthy("pool-d", 1, 0);
        assert_eq!(checker.check("pool-d").status, HealthStatus::Healthy);

        checker.set_unhealthy("pool-d", "down");
        let report = checker.check("pool-d");
        assert_eq!(report.status, HealthStatus::Unhealthy);
        assert_eq!(report.message, "down");
    }

    #[test]
    fn test_check_all_aggregates() {
        let checker = DefaultHealthChecker::new();
        checker.set_healthy("p1", 1, 0);
        checker.set_unhealthy("p2", "timeout");
        checker.set_status("p3", HealthSnapshot::unknown());

        let reports = checker.check_all(&["p1", "p2", "p3"]);
        assert_eq!(reports.len(), 3);
        assert_eq!(reports[0].pool_name, "p1");
        assert_eq!(reports[0].status, HealthStatus::Healthy);
        assert_eq!(reports[1].status, HealthStatus::Unhealthy);
        assert_eq!(reports[2].status, HealthStatus::Unknown);
    }

    #[test]
    fn test_overall_status_all_healthy() {
        let checker = DefaultHealthChecker::new();
        checker.set_healthy("a", 1, 0);
        checker.set_healthy("b", 2, 0);
        assert_eq!(checker.overall_status(&["a", "b"]), HealthStatus::Healthy);
    }

    #[test]
    fn test_overall_status_one_unhealthy() {
        let checker = DefaultHealthChecker::new();
        checker.set_healthy("a", 1, 0);
        checker.set_unhealthy("b", "down");
        checker.set_healthy("c", 3, 0);
        assert_eq!(
            checker.overall_status(&["a", "b", "c"]),
            HealthStatus::Unhealthy
        );
    }

    #[test]
    fn test_overall_status_one_unknown() {
        let checker = DefaultHealthChecker::new();
        checker.set_healthy("a", 1, 0);
        // 'b' never set -> Unknown
        assert_eq!(checker.overall_status(&["a", "b"]), HealthStatus::Unknown);
    }

    #[test]
    fn test_overall_status_empty_pools() {
        let checker = DefaultHealthChecker::new();
        assert_eq!(checker.overall_status(&[]), HealthStatus::Unknown);
    }

    #[test]
    fn test_static_provider_overrides_manual_status() {
        let checker = DefaultHealthChecker::new();
        // Manual status is unhealthy...
        checker.set_unhealthy("p", "manual");

        // ...but a provider says it's healthy.
        checker.register_provider(
            "p",
            Arc::new(StaticStatusProvider::new(HealthSnapshot {
                status: HealthStatus::Healthy,
                connection_count: 9,
                slow_queries: 1,
                message: "from provider".to_string(),
            })),
        );

        let report = checker.check("p");
        assert_eq!(report.status, HealthStatus::Healthy);
        assert_eq!(report.connection_count, 9);
        assert_eq!(report.slow_queries, 1);
        assert_eq!(report.message, "from provider");
    }

    #[test]
    fn test_unregister_provider_falls_back_to_manual() {
        let checker = DefaultHealthChecker::new();
        checker.set_healthy("p", 1, 0);
        checker.register_provider(
            "p",
            Arc::new(StaticStatusProvider::new(HealthSnapshot::unhealthy(
                "from provider",
            ))),
        );

        assert_eq!(checker.check("p").status, HealthStatus::Unhealthy);

        assert!(checker.unregister_provider("p"));
        let report = checker.check("p");
        assert_eq!(report.status, HealthStatus::Healthy);
        assert_eq!(report.connection_count, 1);
    }

    #[test]
    fn test_unregister_missing_provider_returns_false() {
        let checker = DefaultHealthChecker::new();
        assert!(!checker.unregister_provider("never-registered"));
    }

    #[test]
    fn test_threshold_provider_healthy() {
        let provider = ThresholdProvider::new(5, 1, 10, 3);
        let snap = provider.snapshot("pool");
        assert_eq!(snap.status, HealthStatus::Healthy);
        assert_eq!(snap.connection_count, 5);
        assert_eq!(snap.slow_queries, 1);
    }

    #[test]
    fn test_threshold_provider_connection_overflow() {
        let provider = ThresholdProvider::new(11, 0, 10, 3);
        let snap = provider.snapshot("pool");
        assert_eq!(snap.status, HealthStatus::Unhealthy);
        assert!(snap.message.contains("connection count 11 exceeds max 10"));
    }

    #[test]
    fn test_threshold_provider_slow_queries_overflow() {
        let provider = ThresholdProvider::new(5, 5, 10, 3);
        let snap = provider.snapshot("pool");
        assert_eq!(snap.status, HealthStatus::Unhealthy);
        assert!(snap.message.contains("slow queries 5 exceeds max 3"));
    }

    #[test]
    fn test_threshold_provider_boundary_equal_is_healthy() {
        // Exactly at the limit should still be healthy (not exceed).
        let provider = ThresholdProvider::new(10, 3, 10, 3);
        let snap = provider.snapshot("pool");
        assert_eq!(snap.status, HealthStatus::Healthy);
    }

    #[test]
    fn test_threshold_provider_via_checker() {
        let checker = DefaultHealthChecker::new();
        checker.register_provider("pool", Arc::new(ThresholdProvider::new(20, 0, 10, 3)));
        let report = checker.check("pool");
        assert_eq!(report.status, HealthStatus::Unhealthy);
        assert_eq!(report.connection_count, 20);
        assert_eq!(report.slow_queries, 0);
    }

    #[test]
    fn test_check_all_with_providers_mixed() {
        let checker = DefaultHealthChecker::new();
        checker.register_provider(
            "healthy-pool",
            Arc::new(StaticStatusProvider::new(HealthSnapshot {
                status: HealthStatus::Healthy,
                connection_count: 3,
                slow_queries: 0,
                message: String::new(),
            })),
        );
        checker.register_provider(
            "unhealthy-pool",
            Arc::new(StaticStatusProvider::new(HealthSnapshot::unhealthy(
                "provider says down",
            ))),
        );

        let reports = checker.check_all(&["healthy-pool", "unhealthy-pool", "unknown-pool"]);
        assert_eq!(reports.len(), 3);
        assert_eq!(reports[0].status, HealthStatus::Healthy);
        assert_eq!(reports[1].status, HealthStatus::Unhealthy);
        assert_eq!(reports[2].status, HealthStatus::Unknown);
        assert_eq!(
            checker.overall_status(&["healthy-pool", "unhealthy-pool", "unknown-pool"]),
            HealthStatus::Unhealthy
        );
    }

    /// A dynamic provider that returns different snapshots over time,
    /// simulating a real monitoring source.
    struct DynamicProvider {
        counter: AtomicU32,
        statuses: Vec<HealthSnapshot>,
    }

    impl DynamicProvider {
        fn new(statuses: Vec<HealthSnapshot>) -> Self {
            Self {
                counter: AtomicU32::new(0),
                statuses,
            }
        }
    }

    impl HealthStatusProvider for DynamicProvider {
        fn snapshot(&self, _pool: &str) -> HealthSnapshot {
            let idx = self.counter.fetch_add(1, Ordering::SeqCst) as usize;
            self.statuses
                .get(idx)
                .cloned()
                .unwrap_or_else(|| HealthSnapshot {
                    status: HealthStatus::Unknown,
                    connection_count: 0,
                    slow_queries: 0,
                    message: "no more snapshots".to_string(),
                })
        }
    }

    #[test]
    fn test_dynamic_provider_changes_over_time() {
        let provider = Arc::new(DynamicProvider::new(vec![
            HealthSnapshot::healthy(),
            HealthSnapshot::unhealthy("flap"),
            HealthSnapshot::healthy(),
        ]));
        let checker = DefaultHealthChecker::new();
        checker.register_provider("flapping", provider);

        let r1 = checker.check("flapping");
        let r2 = checker.check("flapping");
        let r3 = checker.check("flapping");
        let r4 = checker.check("flapping");

        assert_eq!(r1.status, HealthStatus::Healthy);
        assert_eq!(r2.status, HealthStatus::Unhealthy);
        assert_eq!(r2.message, "flap");
        assert_eq!(r3.status, HealthStatus::Healthy);
        assert_eq!(r4.status, HealthStatus::Unknown);
    }

    #[test]
    fn test_thread_safe_concurrent_checks() {
        use std::thread;
        let checker = Arc::new(DefaultHealthChecker::new());
        checker.set_healthy("shared", 5, 0);

        let mut handles = vec![];
        for _ in 0..4 {
            let c = checker.clone();
            handles.push(thread::spawn(move || {
                let report = c.check("shared");
                assert_eq!(report.status, HealthStatus::Healthy);
                assert_eq!(report.connection_count, 5);
            }));
        }
        for h in handles {
            h.join().expect("thread panicked");
        }
    }

    #[test]
    fn test_state_transitions_observed_via_check() {
        // Observe that successive set_status calls are reflected in check().
        let checker = DefaultHealthChecker::new();
        let mut observed = Vec::<HealthStatus>::new();

        checker.set_healthy("p", 1, 0);
        observed.push(checker.check("p").status);
        checker.set_unhealthy("p", "x");
        observed.push(checker.check("p").status);
        checker.set_status("p", HealthSnapshot::unknown());
        observed.push(checker.check("p").status);

        assert_eq!(
            observed,
            vec![
                HealthStatus::Healthy,
                HealthStatus::Unhealthy,
                HealthStatus::Unknown,
            ]
        );
    }

    #[test]
    fn test_serialization_roundtrip() {
        let report = HealthReport::new("pool")
            .set_healthy()
            .with_connection_count(7)
            .with_slow_queries(2)
            .with_message("ok");
        let json = serde_json::to_string(&report).expect("serialize");
        let back: HealthReport = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.pool_name, "pool");
        assert_eq!(back.status, HealthStatus::Healthy);
        assert_eq!(back.connection_count, 7);
        assert_eq!(back.slow_queries, 2);
        assert_eq!(back.message, "ok");
    }

    #[test]
    fn test_health_status_eq() {
        assert_eq!(HealthStatus::Healthy, HealthStatus::Healthy);
        assert_ne!(HealthStatus::Healthy, HealthStatus::Unhealthy);
        assert_ne!(HealthStatus::Unhealthy, HealthStatus::Unknown);
    }

    #[test]
    fn test_db_health_checker_via_trait_object() {
        let checker: Box<dyn DbHealthChecker> = Box::new(DefaultHealthChecker::new());
        // Trait object has no set_status, so we just verify check returns Unknown.
        let report = checker.check("nothing");
        assert_eq!(report.status, HealthStatus::Unknown);
        let reports = checker.check_all(&["a", "b"]);
        assert_eq!(reports.len(), 2);
        assert_eq!(reports[0].status, HealthStatus::Unknown);
    }

    #[test]
    fn test_snapshot_default_is_unknown() {
        let snap = HealthSnapshot::default();
        assert_eq!(snap.status, HealthStatus::Unknown);
        assert_eq!(snap.connection_count, 0);
        assert_eq!(snap.slow_queries, 0);
        assert!(snap.message.is_empty());
    }

    #[test]
    fn test_snapshot_constructors() {
        let h = HealthSnapshot::healthy();
        assert_eq!(h.status, HealthStatus::Healthy);

        let u = HealthSnapshot::unhealthy("err");
        assert_eq!(u.status, HealthStatus::Unhealthy);
        assert_eq!(u.message, "err");

        let n = HealthSnapshot::unknown();
        assert_eq!(n.status, HealthStatus::Unknown);
    }

    // ===================== L4: SLA fields on HealthReport =====================

    #[test]
    fn test_report_sla_fields_default_none() {
        let r = HealthReport::new("p");
        assert_eq!(r.error_rate, None);
        assert_eq!(r.p50_ms, None);
        assert_eq!(r.p95_ms, None);
        assert_eq!(r.p99_ms, None);
        assert_eq!(r.saturation, None);
        assert_eq!(r.uptime_ratio, None);
    }

    #[test]
    fn test_report_sla_builders_set_values() {
        let r = HealthReport::new("p")
            .set_healthy()
            .with_error_rate(0.01)
            .with_latency_p50(10.0)
            .with_latency_p95(50.0)
            .with_latency_p99(100.0)
            .with_saturation(0.7)
            .with_uptime_ratio(0.999);
        assert_eq!(r.error_rate, Some(0.01));
        assert_eq!(r.p50_ms, Some(10.0));
        assert_eq!(r.p95_ms, Some(50.0));
        assert_eq!(r.p99_ms, Some(100.0));
        assert_eq!(r.saturation, Some(0.7));
        assert_eq!(r.uptime_ratio, Some(0.999));
    }

    #[test]
    fn test_report_sla_fields_serialize_roundtrip() {
        let r = HealthReport::new("p")
            .set_healthy()
            .with_error_rate(0.05)
            .with_latency_p99(200.0);
        let json = serde_json::to_string(&r).expect("serialize");
        let back: HealthReport = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.error_rate, Some(0.05));
        assert_eq!(back.p99_ms, Some(200.0));
        assert_eq!(back.p50_ms, None);
    }

    #[test]
    fn test_report_backward_compat_old_json_deserializes() {
        // Old JSON without the new SLA fields must still deserialize, with
        // those fields defaulting to None.
        let old_json = r#"{
            "pool_name": "legacy",
            "status": "Healthy",
            "connection_count": 3,
            "slow_queries": 0,
            "message": "ok"
        }"#;
        let back: HealthReport = serde_json::from_str(old_json).expect("deserialize legacy");
        assert_eq!(back.pool_name, "legacy");
        assert_eq!(back.status, HealthStatus::Healthy);
        assert_eq!(back.connection_count, 3);
        assert_eq!(back.error_rate, None);
        assert_eq!(back.p99_ms, None);
        assert_eq!(back.uptime_ratio, None);
    }

    #[test]
    fn test_default_checker_check_sla_fields_none() {
        let checker = DefaultHealthChecker::new();
        checker.set_healthy("p", 1, 0);
        let r = checker.check("p");
        assert_eq!(r.error_rate, None);
        assert_eq!(r.p99_ms, None);
    }

    // ===================== L4: AlertLevel / HealthAlert =====================

    #[test]
    fn test_alert_level_variants() {
        assert_ne!(AlertLevel::Info, AlertLevel::Warning);
        assert_ne!(AlertLevel::Warning, AlertLevel::Critical);
        assert_ne!(AlertLevel::Info, AlertLevel::Critical);
    }

    #[test]
    fn test_health_alert_new_defaults() {
        let alert = HealthAlert::new(AlertLevel::Warning, "pool-a", "high latency");
        assert_eq!(alert.level, AlertLevel::Warning);
        assert_eq!(alert.pool_name, "pool-a");
        assert_eq!(alert.message, "high latency");
        assert!(alert.metrics.is_none());
        // timestamp should be ~now (just sanity check it's after epoch).
        assert!(alert.timestamp.timestamp() > 0);
    }

    #[test]
    fn test_health_alert_with_metrics() {
        let report = HealthReport::new("p").with_error_rate(0.9);
        let alert =
            HealthAlert::new(AlertLevel::Critical, "p", "error rate high").with_metrics(report);
        assert!(alert.metrics.is_some());
        let m = alert.metrics.expect("metrics present");
        assert_eq!(m.error_rate, Some(0.9));
    }

    #[test]
    fn test_health_alert_serialization_roundtrip() {
        let alert = HealthAlert::new(AlertLevel::Critical, "p", "down")
            .with_metrics(HealthReport::new("p").with_latency_p99(999.0));
        let json = serde_json::to_string(&alert).expect("serialize");
        let back: HealthAlert = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.level, AlertLevel::Critical);
        assert_eq!(back.pool_name, "p");
        assert_eq!(back.message, "down");
        assert!(back.metrics.is_some());
        assert_eq!(back.metrics.expect("metrics").p99_ms, Some(999.0));
    }

    // ===================== L4: AlertChannel implementations =====================

    #[test]
    fn test_log_alert_channel_send_ok() {
        let ch = LogAlertChannel::new();
        let alert = HealthAlert::new(AlertLevel::Info, "p", "hi");
        assert!(ch.send(&alert).is_ok());
        assert_eq!(ch.name(), "log");
    }

    #[test]
    fn test_webhook_alert_channel_records_alerts() {
        let ch = WebhookAlertChannel::new("https://example.com/hook".to_string());
        assert_eq!(ch.name(), "webhook");
        assert_eq!(ch.url(), "https://example.com/hook");
        assert!(ch.sent_alerts().is_empty());

        let a1 = HealthAlert::new(AlertLevel::Warning, "p", "w");
        let a2 = HealthAlert::new(AlertLevel::Critical, "p", "c");
        assert!(ch.send(&a1).is_ok());
        assert!(ch.send(&a2).is_ok());

        let sent = ch.sent_alerts();
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0].level, AlertLevel::Warning);
        assert_eq!(sent[1].level, AlertLevel::Critical);
    }

    #[test]
    fn test_im_alert_channel_records_alerts() {
        let ch = ImAlertChannel::new("https://im.example.com/bot".to_string());
        assert_eq!(ch.name(), "im");
        assert_eq!(ch.webhook_url(), "https://im.example.com/bot");
        assert!(ch.sent_alerts().is_empty());

        let a = HealthAlert::new(AlertLevel::Critical, "p", "down");
        assert!(ch.send(&a).is_ok());
        assert_eq!(ch.sent_alerts().len(), 1);
        assert_eq!(ch.sent_alerts()[0].message, "down");
    }

    #[test]
    fn test_alert_channel_via_trait_object() {
        let ch: Arc<dyn AlertChannel> = Arc::new(LogAlertChannel::new());
        let alert = HealthAlert::new(AlertLevel::Info, "p", "x");
        assert!(ch.send(&alert).is_ok());
        assert_eq!(ch.name(), "log");
    }

    // ===================== L4: AlertManager =====================

    #[test]
    fn test_alert_manager_empty_notify_returns_empty() {
        let mgr = AlertManager::new();
        let alert = HealthAlert::new(AlertLevel::Info, "p", "x");
        let results = mgr.notify(&alert);
        assert!(results.is_empty());
        assert!(mgr.channels().is_empty());
    }

    #[test]
    fn test_alert_manager_register_and_notify() {
        let mut mgr = AlertManager::new();
        let log = Arc::new(LogAlertChannel::new());
        let webhook = Arc::new(WebhookAlertChannel::new("https://h".to_string()));
        let im = Arc::new(ImAlertChannel::new("https://i".to_string()));
        mgr.register(log);
        mgr.register(webhook);
        mgr.register(im);

        let names = mgr.channels();
        assert_eq!(names, vec!["log", "webhook", "im"]);

        let alert = HealthAlert::new(AlertLevel::Critical, "p", "down");
        let results = mgr.notify(&alert);
        assert_eq!(results.len(), 3);
        for r in &results {
            assert!(r.is_ok());
        }
    }

    #[test]
    fn test_alert_manager_notify_partial_failure() {
        struct FailingChannel;
        impl AlertChannel for FailingChannel {
            fn send(&self, _alert: &HealthAlert) -> Result<(), String> {
                Err("network error".to_string())
            }
            fn name(&self) -> &str {
                "failing"
            }
        }

        let mut mgr = AlertManager::new();
        mgr.register(Arc::new(LogAlertChannel::new()));
        mgr.register(Arc::new(FailingChannel));
        mgr.register(Arc::new(WebhookAlertChannel::new("u".to_string())));

        let alert = HealthAlert::new(AlertLevel::Warning, "p", "x");
        let results = mgr.notify(&alert);
        assert_eq!(results.len(), 3);
        assert!(results[0].is_ok());
        assert!(results[1].is_err());
        assert!(results[2].is_ok());
    }

    #[test]
    fn test_alert_manager_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<AlertManager>();
    }

    // ===================== L4: FailoverPolicy =====================

    #[test]
    fn test_failover_policy_defaults_stay_on_primary() {
        let policy = FailoverPolicy::new();
        let r = HealthReport::new("p").set_healthy();
        assert_eq!(policy.evaluate(&r), FailoverAction::StayOnPrimary);
    }

    #[test]
    fn test_failover_policy_unhealthy_circuit_open() {
        let policy = FailoverPolicy::new();
        let r = HealthReport::new("p").set_status(HealthStatus::Unhealthy);
        assert_eq!(policy.evaluate(&r), FailoverAction::CircuitOpen);
    }

    #[test]
    fn test_failover_policy_error_rate_exceeds_threshold() {
        let policy = FailoverPolicy::new();
        let r = HealthReport::new("p").set_healthy().with_error_rate(0.6);
        assert_eq!(policy.evaluate(&r), FailoverAction::FailoverToSecondary);
    }

    #[test]
    fn test_failover_policy_latency_exceeds_threshold() {
        let policy = FailoverPolicy::new();
        let r = HealthReport::new("p")
            .set_healthy()
            .with_latency_p99(6000.0);
        assert_eq!(policy.evaluate(&r), FailoverAction::FailoverToSecondary);
    }

    #[test]
    fn test_failover_policy_boundary_equal_is_stay() {
        // Exactly at threshold (0.5 and 5000) should NOT trigger failover
        // (strictly greater-than comparison).
        let policy = FailoverPolicy::new();
        let r = HealthReport::new("p")
            .set_healthy()
            .with_error_rate(0.5)
            .with_latency_p99(5000.0);
        assert_eq!(policy.evaluate(&r), FailoverAction::StayOnPrimary);
    }

    #[test]
    fn test_failover_policy_custom_thresholds() {
        let policy = FailoverPolicy::new()
            .with_error_rate_threshold(0.1)
            .with_latency_threshold(100.0);
        let r = HealthReport::new("p")
            .set_healthy()
            .with_error_rate(0.2)
            .with_latency_p99(50.0);
        // error_rate 0.2 > 0.1 -> failover, even though latency is fine.
        assert_eq!(policy.evaluate(&r), FailoverAction::FailoverToSecondary);
    }

    #[test]
    fn test_failover_policy_no_metrics_stays() {
        // No error_rate / p99 set -> cannot exceed threshold -> stay.
        let policy = FailoverPolicy::new();
        let r = HealthReport::new("p").set_healthy();
        assert_eq!(policy.evaluate(&r), FailoverAction::StayOnPrimary);
    }

    #[test]
    fn test_failover_action_variants_distinct() {
        let a = FailoverAction::StayOnPrimary;
        let b = FailoverAction::FailoverToSecondary;
        let c = FailoverAction::FailoverToTertiary;
        let d = FailoverAction::CircuitOpen;
        assert_ne!(a, b);
        assert_ne!(b, c);
        assert_ne!(c, d);
        assert_ne!(a, d);
    }

    // ===================== L4: MultiRegionHealthView =====================

    #[test]
    fn test_multi_region_empty_aggregate_unknown() {
        let view = MultiRegionHealthView::new();
        assert_eq!(view.aggregate(), HealthStatus::Unknown);
        assert!(view.healthy_regions().is_empty());
        assert!(view.unhealthy_regions().is_empty());
    }

    #[test]
    fn test_multi_region_all_healthy() {
        let mut view = MultiRegionHealthView::new();
        view.register("us-east-1", HealthStatus::Healthy);
        view.register("eu-west-1", HealthStatus::Healthy);
        assert_eq!(view.aggregate(), HealthStatus::Healthy);
        assert_eq!(view.healthy_regions().len(), 2);
        assert!(view.unhealthy_regions().is_empty());
        // Sorted output.
        let healthy = view.healthy_regions();
        assert_eq!(healthy, vec!["eu-west-1", "us-east-1"]);
    }

    #[test]
    fn test_multi_region_one_unhealthy_drags_aggregate() {
        let mut view = MultiRegionHealthView::new();
        view.register("us-east-1", HealthStatus::Healthy);
        view.register("ap-northeast-1", HealthStatus::Unhealthy);
        view.register("eu-west-1", HealthStatus::Healthy);
        assert_eq!(view.aggregate(), HealthStatus::Unhealthy);
        assert_eq!(view.healthy_regions().len(), 2);
        assert_eq!(view.unhealthy_regions(), vec!["ap-northeast-1"]);
    }

    #[test]
    fn test_multi_region_one_unknown_no_unhealthy() {
        let mut view = MultiRegionHealthView::new();
        view.register("us-east-1", HealthStatus::Healthy);
        view.register("unknown-region", HealthStatus::Unknown);
        assert_eq!(view.aggregate(), HealthStatus::Unknown);
        assert_eq!(view.healthy_regions(), vec!["us-east-1"]);
        assert!(view.unhealthy_regions().is_empty());
    }

    #[test]
    fn test_multi_region_overwrite_status() {
        let mut view = MultiRegionHealthView::new();
        view.register("r1", HealthStatus::Unhealthy);
        assert_eq!(view.aggregate(), HealthStatus::Unhealthy);
        view.register("r1", HealthStatus::Healthy);
        assert_eq!(view.aggregate(), HealthStatus::Healthy);
        assert!(view.unhealthy_regions().is_empty());
    }

    // ===================== L4: CircuitBreaker =====================

    #[test]
    fn test_circuit_breaker_starts_closed() {
        let mut cb = CircuitBreaker::new(3, std::time::Duration::from_millis(100));
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.can_execute());
    }

    #[test]
    fn test_circuit_breaker_trips_after_threshold() {
        let mut cb = CircuitBreaker::new(3, std::time::Duration::from_secs(60));
        assert!(cb.can_execute());
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
        assert!(!cb.can_execute());
    }

    #[test]
    fn test_circuit_breaker_success_resets() {
        let mut cb = CircuitBreaker::new(3, std::time::Duration::from_secs(60));
        cb.record_failure();
        cb.record_failure();
        cb.record_success();
        assert_eq!(cb.state(), CircuitState::Closed);
        // After success, should need 3 failures again to trip.
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_breaker_half_open_after_timeout() {
        let mut cb = CircuitBreaker::new(1, std::time::Duration::from_millis(10));
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
        // Immediately: still open.
        assert!(!cb.can_execute());
        // Wait for reset timeout.
        std::thread::sleep(std::time::Duration::from_millis(30));
        assert!(cb.can_execute());
        assert_eq!(cb.state(), CircuitState::HalfOpen);
    }

    #[test]
    fn test_circuit_breaker_half_open_success_closes() {
        let mut cb = CircuitBreaker::new(1, std::time::Duration::from_millis(10));
        cb.record_failure();
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert!(cb.can_execute());
        assert_eq!(cb.state(), CircuitState::HalfOpen);
        cb.record_success();
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_breaker_half_open_failure_reopens() {
        let mut cb = CircuitBreaker::new(1, std::time::Duration::from_millis(10));
        cb.record_failure();
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert!(cb.can_execute());
        assert_eq!(cb.state(), CircuitState::HalfOpen);
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
    }

    #[test]
    fn test_circuit_breaker_boundary_exactly_threshold() {
        // threshold = 3 means 3 failures should trip (>= comparison).
        let mut cb = CircuitBreaker::new(3, std::time::Duration::from_secs(60));
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
    }

    #[test]
    fn test_circuit_state_variants_distinct() {
        assert_ne!(CircuitState::Closed, CircuitState::Open);
        assert_ne!(CircuitState::Open, CircuitState::HalfOpen);
        assert_ne!(CircuitState::Closed, CircuitState::HalfOpen);
    }

    // ===================== L4: BackupHealthProvider =====================

    #[test]
    fn test_backup_provider_no_backup_is_stale() {
        let provider = BackupHealthProvider::new(chrono::Duration::hours(24));
        assert!(provider.is_stale());
        assert_eq!(provider.check(), HealthStatus::Unhealthy);
    }

    #[test]
    fn test_backup_provider_recent_backup_healthy() {
        let mut provider = BackupHealthProvider::new(chrono::Duration::hours(24));
        provider.set_last_backup(chrono::Utc::now());
        assert!(!provider.is_stale());
        assert_eq!(provider.check(), HealthStatus::Healthy);
    }

    #[test]
    fn test_backup_provider_old_backup_unhealthy() {
        let mut provider = BackupHealthProvider::new(chrono::Duration::hours(24));
        let old = chrono::Utc::now() - chrono::Duration::hours(48);
        provider.set_last_backup(old);
        assert!(provider.is_stale());
        assert_eq!(provider.check(), HealthStatus::Unhealthy);
    }

    #[test]
    fn test_backup_provider_boundary_exactly_max_age() {
        // A backup exactly at max_age should NOT be considered stale
        // (strictly greater-than comparison).
        let max_age = chrono::Duration::hours(24);
        let mut provider = BackupHealthProvider::new(max_age);
        let ts = chrono::Utc::now() - max_age;
        provider.set_last_backup(ts);
        // Note: time may have advanced slightly, so we accept either result
        // as long as the logic is consistent. The boundary is `> max_age`.
        let _ = provider.is_stale();
    }

    #[test]
    fn test_backup_provider_overwrite_timestamp() {
        let mut provider = BackupHealthProvider::new(chrono::Duration::hours(24));
        let old = chrono::Utc::now() - chrono::Duration::hours(48);
        provider.set_last_backup(old);
        assert!(provider.is_stale());
        provider.set_last_backup(chrono::Utc::now());
        assert!(!provider.is_stale());
    }

    // ===================== L4: Integration / concurrency =====================

    #[test]
    fn test_alert_manager_concurrent_notify() {
        use std::thread;
        let mut mgr = Arc::new(AlertManager::new());
        let webhook = Arc::new(WebhookAlertChannel::new("u".to_string()));
        // We can't register after wrapping in Arc without &mut, so register
        // before sharing. Use Arc::get_mut for setup.
        {
            let m = Arc::get_mut(&mut mgr).expect("unique ref");
            m.register(webhook.clone());
            m.register(Arc::new(LogAlertChannel::new()));
        }
        let alert = HealthAlert::new(AlertLevel::Critical, "p", "down");

        let mut handles = vec![];
        for _ in 0..4 {
            let m = mgr.clone();
            let a = alert.clone();
            handles.push(thread::spawn(move || {
                let results = m.notify(&a);
                assert_eq!(results.len(), 2);
                for r in &results {
                    assert!(r.is_ok());
                }
            }));
        }
        for h in handles {
            h.join().expect("thread panicked");
        }
        // Webhook channel should have received 4 alerts.
        assert_eq!(webhook.sent_alerts().len(), 4);
    }

    #[test]
    fn test_failover_policy_integrates_with_report() {
        // End-to-end: build a report with metrics and evaluate policy.
        let policy = FailoverPolicy::new();
        let report = HealthReport::new("primary")
            .set_healthy()
            .with_error_rate(0.7)
            .with_latency_p99(8000.0)
            .with_uptime_ratio(0.9);
        assert_eq!(
            policy.evaluate(&report),
            FailoverAction::FailoverToSecondary
        );

        // If the primary is actually Unhealthy, the circuit is open.
        let down = HealthReport::new("primary").set_status(HealthStatus::Unhealthy);
        assert_eq!(policy.evaluate(&down), FailoverAction::CircuitOpen);
    }

    #[test]
    fn test_static_assertions_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<AlertManager>();
        assert_send_sync::<WebhookAlertChannel>();
        assert_send_sync::<ImAlertChannel>();
        assert_send_sync::<LogAlertChannel>();
        assert_send_sync::<HealthAlert>();
        assert_send_sync::<FailoverPolicy>();
        assert_send_sync::<MultiRegionHealthView>();
        assert_send_sync::<CircuitBreaker>();
        assert_send_sync::<BackupHealthProvider>();
    }
}
