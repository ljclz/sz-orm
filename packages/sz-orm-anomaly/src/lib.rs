//! # sz-orm-anomaly — 异常检测（Anomaly Detection）
//!
//! 基于 `anomaly-detection` feature，持续采集数据库运行指标（慢查询/错误率/连接池），
//! 通过滑动窗口 + Welford 在线基线 + 统计规则 + 阈值检测异常模式（突增/耗尽/偏离基线），
//! 输出结构化告警事件，支持告警去重（冷却期）、订阅回调、Prometheus 指标导出、健康度集成。
//!
//! ## 设计原则
//!
//! - **统计规则 + 阈值（非 ML）**：基于均值 + Nσ + 绝对阈值，简单可解释，无需训练数据
//! - **异步/旁路采集**：指标采集不阻塞查询主路径（REQ-ANM-005）
//! - **滑动窗口 + 时间淘汰**：保留最近 N 分钟数据（默认 30 分钟），内存上限 10 MB
//! - **Welford 在线算法**：数值稳定，单次更新 O(1)，适合实时基线计算
//! - **告警去重冷却期**：同类型异常 5 分钟内不重复告警，避免告警风暴
//! - **feature gate 隔离**：默认不启用，不影响既有编译
//!
//! ## 主要模块
//!
//! - [`config`] — 阈值配置 + 热更新（`AnomalyConfig` + `ConfigStore`）
//! - [`collector`] — 指标采集（`MetricCollector` + 三类指标 + SQL 摘要脱敏）
//! - [`window`] — 滑动窗口（`SlidingWindow` + 时间淘汰 + 内存上限保护）
//! - [`detector`] — 异常检测（`BaselineCalculator` Welford + `SpikeDetector` + 严重级别判定）
//! - [`alert`] — 告警输出（`Alert` + `AlertDedup` 冷却期 + 订阅 API）
//! - [`report`] — 报告导出（JSON + Markdown）
//! - [`integration`] — 集成模块（Prometheus 指标导出 + 健康度集成）
//! - [`error`] — 错误类型
//!
//! ## 复用
//!
//! - SQL 脱敏复用 `sz-orm-masking::DataMasker`（参数值 → 占位符）
//!
//! ## 快速开始
//!
//! ```no_run
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
//! ```

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
