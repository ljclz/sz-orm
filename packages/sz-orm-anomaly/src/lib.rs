//! # sz-orm-anomaly — Anomaly Detection
//!
//! Based on `anomaly-detection` feature, continuously collects database runtime metrics (slow query/error rate/connection pool),
//! detects anomaly patterns (spike/exhaustion/baseline deviation) via sliding window + Welford online baseline + statistical rules + thresholds,
//! outputs structured alert events, supports alert deduplication (cooldown), subscription callbacks, Prometheus metric export, and health integration.
//!
//! ## Design Principles
//!
//! - **Statistical rules + thresholds (non-ML)**: Based on mean + Nσ + absolute threshold, simple and interpretable, no training data needed
//! - **Async/bypass collection**: Metric collection does not block query main path (REQ-ANM-005)
//! - **Sliding window + time eviction**: Retains last N minutes of data (default 30 minutes), memory limit 10 MB
//! - **Welford online algorithm**: Numerically stable, single update O(1), suitable for real-time baseline computation
//! - **Alert deduplication cooldown**: Same-type anomalies do not repeat alert within 5 minutes, avoiding alert storms
//! - **Feature gate isolation**: Disabled by default, does not affect existing compilation
//!
//! ## Main Modules
//!
//! - [`config`] — Threshold config + hot reload (`AnomalyConfig` + `ConfigStore`)
//! - [`collector`] — Metric collection (`MetricCollector` + 3 metric types + SQL summary masking)
//! - [`window`] — Sliding window (`SlidingWindow` + time eviction + memory limit protection)
//! - [`detector`] — Anomaly detection (`BaselineCalculator` Welford + `SpikeDetector` + severity level determination)
//! - [`alert`] — Alert output (`Alert` + `AlertDedup` cooldown + subscription API)
//! - [`report`] — Report export (JSON + Markdown)
//! - [`integration`] — Integration module (Prometheus metric export + health integration)
//! - [`error`] — Error type
//!
//! ## Reuse
//!
//! - SQL masking reuses `sz-orm-masking::DataMasker` (parameter values → placeholders)
//!
//! ## Quick Start
//!
//! ```no_run
//! # #[cfg(feature = "anomaly-detection")] {
//! use sz_orm_anomaly::{AnomalyDetector, AnomalyConfig};
//!
//! let detector = AnomalyDetector::new(AnomalyConfig::default());
//! detector.record_slow_query(150, "SELECT * FROM users WHERE id = ?", 0);
//! detector.record_error(sz_orm_anomaly::ErrorType::SqlError, 0);
//! detector.record_pool_usage(48, 2, 15, 1200, 0);
//!
//! let alerts = detector.detect_anomalies();
//! for alert in &alerts {
//!     println!("[{:?}] {:?}: {}", alert.severity, alert.anomaly_type, alert.suggestion);
//! }
//! # }

pub mod error;

#[cfg(feature = "anomaly-detection")]
pub mod alert;
#[cfg(feature = "anomaly-detection")]
pub mod collector;
#[cfg(feature = "anomaly-detection")]
pub mod config;
#[cfg(feature = "anomaly-detection")]
pub mod detector;
#[cfg(feature = "anomaly-detection")]
pub mod integration;
#[cfg(feature = "anomaly-detection")]
pub mod report;
#[cfg(feature = "anomaly-detection")]
pub mod window;

#[cfg(feature = "anomaly-detection")]
pub use alert::{Alert, AlertDedup, AlertEmitter, AnomalyType, Baseline, Severity, SubscriptionId};
#[cfg(feature = "anomaly-detection")]
pub use collector::{
    ErrorMetric, ErrorType, MetricCollector, MetricType, PoolMetric, SlowQueryMetric,
};
#[cfg(feature = "anomaly-detection")]
pub use config::{AnomalyConfig, ConfigStore};
#[cfg(feature = "anomaly-detection")]
pub use detector::{AnomalyDetector, BaselineCalculator, SpikeDetector};
#[cfg(feature = "anomaly-detection")]
pub use error::AnomalyError;
#[cfg(feature = "anomaly-detection")]
pub use integration::{HealthImpact, PrometheusExporter};
#[cfg(feature = "anomaly-detection")]
pub use report::{ReportExporter, TimeRange};

pub use error::AnomalyErrorKind;
#[cfg(feature = "anomaly-detection")]
pub use window::SlidingWindow;
