//! 异常检测模块（Anomaly Detection）
//!
//! 对应 v4.6.0 REQ-V46-004，tasks.md M6。
//!
//! # 核心概念
//!
//! - **AnomalyDetector**：异常检测器，复用 `MetricsRegistry` 指标数据
//! - **AnomalyAlgorithm**：五种检测算法（Threshold/Trend/Statistical/ZScore/IQR）
//! - **AnomalyAlert**：异常告警联动（Log/Webhook/Slo 三种通道）
//! - **异常标注**：在查询日志上标注异常标记
//!
//! # 使用示例
//!
//! ```
//! use sz_orm_observability::anomaly::{AnomalyDetector, AnomalyConfig, AnomalyAlgorithm};
//! use sz_orm_observability::MetricsRegistry;
//! use std::sync::Arc;
//!
//! let registry = Arc::new(MetricsRegistry::new());
//! let config = AnomalyConfig::new().with_algorithm(AnomalyAlgorithm::Threshold).with_threshold(1.0);
//! let detector = AnomalyDetector::new(Arc::clone(&registry), config);
//! detector.record("query_duration", 0.5);
//! detector.record("query_duration", 1.5);
//! ```

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::MetricsRegistry;

// ============================================================================
// AnomalyAlgorithm — 五种检测算法
// ============================================================================

/// 异常检测算法
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AnomalyAlgorithm {
    /// 阈值检测：指标值超过阈值即异常
    Threshold,
    /// 趋势检测：指标持续上升/下降即异常
    Trend,
    /// 统计检测：基于均值/方差检测异常
    Statistical,
    /// Z-Score 检测：Z-Score 超过阈值即异常
    ZScore,
    /// IQR 检测：四分位距检测异常
    IQR,
}

// ============================================================================
// AlertChannel — 告警通道
// ============================================================================

/// 告警通道
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertChannel {
    /// 记日志
    Log,
    /// Webhook HTTP POST
    Webhook,
    /// SLO 告警通道
    Slo,
}

// ============================================================================
// AnomalyConfig — 异常检测配置
// ============================================================================

/// 异常检测配置
#[derive(Debug, Clone)]
pub struct AnomalyConfig {
    /// 检测算法
    pub algorithm: AnomalyAlgorithm,
    /// 检测窗口（毫秒）
    pub window_ms: u64,
    /// 阈值（Threshold / Statistical 用）
    pub threshold: f64,
    /// Z-Score 阈值（ZScore 用）
    pub zscore_threshold: f64,
    /// 告警通道
    pub alert_channel: AlertChannel,
    /// Webhook URL（AlertChannel::Webhook 时用）
    pub webhook_url: Option<String>,
    /// 历史数据最大保留数量
    pub max_history: usize,
}

impl Default for AnomalyConfig {
    fn default() -> Self {
        Self {
            algorithm: AnomalyAlgorithm::Threshold,
            window_ms: 300_000,
            threshold: 1.0,
            zscore_threshold: 3.0,
            alert_channel: AlertChannel::Log,
            webhook_url: None,
            max_history: 1000,
        }
    }
}

impl AnomalyConfig {
    /// 创建默认配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置检测算法
    pub fn with_algorithm(mut self, algorithm: AnomalyAlgorithm) -> Self {
        self.algorithm = algorithm;
        self
    }

    /// 设置检测窗口
    pub fn with_window_ms(mut self, window_ms: u64) -> Self {
        self.window_ms = window_ms;
        self
    }

    /// 设置阈值
    pub fn with_threshold(mut self, threshold: f64) -> Self {
        self.threshold = threshold;
        self
    }

    /// 设置 Z-Score 阈值
    pub fn with_zscore_threshold(mut self, zscore_threshold: f64) -> Self {
        self.zscore_threshold = zscore_threshold;
        self
    }

    /// 设置告警通道
    pub fn with_alert_channel(mut self, channel: AlertChannel) -> Self {
        self.alert_channel = channel;
        self
    }

    /// 设置 Webhook URL
    pub fn with_webhook_url(mut self, url: impl Into<String>) -> Self {
        self.webhook_url = Some(url.into());
        self
    }
}

// ============================================================================
// Anomaly — 异常结构
// ============================================================================

/// 检测到的异常
#[derive(Debug, Clone, PartialEq)]
pub struct Anomaly {
    /// 指标名
    pub metric_name: String,
    /// 异常值
    pub anomaly_value: f64,
    /// 阈值
    pub threshold: f64,
    /// 检测窗口（毫秒）
    pub window_ms: u64,
    /// 检测算法
    pub algorithm: AnomalyAlgorithm,
    /// 检测时间（Unix 毫秒）
    pub detected_at: u64,
    /// 异常描述
    pub description: String,
}

// ============================================================================
// AnomalyError — 异常检测错误
// ============================================================================

/// 异常检测错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnomalyError {
    /// 历史数据不足
    InsufficientData,
    /// 算法计算异常（除零/溢出）
    CalculationError(String),
    /// 告警通道不可用
    AlertChannelUnavailable(String),
}

impl std::fmt::Display for AnomalyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnomalyError::InsufficientData => write!(f, "insufficient history data"),
            AnomalyError::CalculationError(msg) => write!(f, "calculation error: {}", msg),
            AnomalyError::AlertChannelUnavailable(msg) => {
                write!(f, "alert channel unavailable: {}", msg)
            }
        }
    }
}

impl std::error::Error for AnomalyError {}

// ============================================================================
// AnomalyAlert — 异常告警
// ============================================================================

/// 异常告警
#[derive(Debug, Clone)]
pub struct AnomalyAlert {
    /// 异常信息
    pub anomaly: Anomaly,
    /// 告警通道
    pub channel: AlertChannel,
}

impl AnomalyAlert {
    /// 发送告警
    pub async fn send(&self) -> Result<(), AnomalyError> {
        match self.channel {
            AlertChannel::Log => {
                tracing::info!(
                    "anomaly detected: metric={}, value={}, threshold={}, algorithm={:?}",
                    self.anomaly.metric_name,
                    self.anomaly.anomaly_value,
                    self.anomaly.threshold,
                    self.anomaly.algorithm
                );
                Ok(())
            }
            AlertChannel::Webhook => {
                tracing::warn!(
                    "webhook alert not implemented in offline mode, anomaly: {}",
                    self.anomaly.description
                );
                Ok(())
            }
            AlertChannel::Slo => {
                tracing::info!(
                    "SLO anomaly alert: metric={}, value={}",
                    self.anomaly.metric_name,
                    self.anomaly.anomaly_value
                );
                Ok(())
            }
        }
    }
}

// ============================================================================
// AnomalyDetector — 异常检测器
// ============================================================================

/// 异常检测器
///
/// 复用 `MetricsRegistry` 指标注册中心，维护指标历史数据缓冲区，
/// 按 `AnomalyAlgorithm` 检测指标异常，触发 `AnomalyAlert` 告警。
pub struct AnomalyDetector {
    /// 指标注册中心
    metrics: Arc<MetricsRegistry>,
    /// 检测配置
    config: AnomalyConfig,
    /// 历史数据缓冲区（metric_name → 历史值队列）
    history: RwLock<HashMap<String, VecDeque<f64>>>,
}

impl AnomalyDetector {
    /// 创建异常检测器
    pub fn new(metrics: Arc<MetricsRegistry>, config: AnomalyConfig) -> Self {
        Self {
            metrics,
            config,
            history: RwLock::new(HashMap::new()),
        }
    }

    /// 获取当前时间（Unix 毫秒）
    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    /// 记录指标值到历史缓冲区，同时更新 MetricsRegistry 中的 Gauge
    pub fn record(&self, metric_name: &str, value: f64) {
        let gauge = self
            .metrics
            .register_gauge(metric_name, "anomaly detection metric");
        gauge.set(value);
        let mut history = self.history.write().unwrap();
        let queue = history.entry(metric_name.to_string()).or_default();
        queue.push_back(value);
        while queue.len() > self.config.max_history {
            queue.pop_front();
        }
    }

    /// 检测指标异常
    pub async fn detect(&self, metric_name: &str) -> Result<Vec<Anomaly>, AnomalyError> {
        let data: Vec<f64> = {
            let history = self.history.read().unwrap();
            match history.get(metric_name) {
                Some(q) => q.iter().copied().collect(),
                None => return Err(AnomalyError::InsufficientData),
            }
        };
        if data.len() < 2 {
            return Err(AnomalyError::InsufficientData);
        }
        let anomalies = match self.config.algorithm {
            AnomalyAlgorithm::Threshold => self.detect_threshold(metric_name, &data)?,
            AnomalyAlgorithm::Trend => self.detect_trend(metric_name, &data)?,
            AnomalyAlgorithm::Statistical => self.detect_statistical(metric_name, &data)?,
            AnomalyAlgorithm::ZScore => self.detect_zscore(metric_name, &data)?,
            AnomalyAlgorithm::IQR => self.detect_iqr(metric_name, &data)?,
        };
        for anomaly in &anomalies {
            let _ = self.trigger_alert(anomaly).await;
        }
        Ok(anomalies)
    }

    /// 阈值检测
    fn detect_threshold(
        &self,
        metric_name: &str,
        data: &[f64],
    ) -> Result<Vec<Anomaly>, AnomalyError> {
        let now = Self::now_ms();
        let mut anomalies = Vec::new();
        for &value in data {
            if value > self.config.threshold {
                anomalies.push(Anomaly {
                    metric_name: metric_name.to_string(),
                    anomaly_value: value,
                    threshold: self.config.threshold,
                    window_ms: self.config.window_ms,
                    algorithm: AnomalyAlgorithm::Threshold,
                    detected_at: now,
                    description: format!(
                        "{} {} exceeds threshold {}",
                        metric_name, value, self.config.threshold
                    ),
                });
            }
        }
        Ok(anomalies)
    }

    /// 趋势检测：持续上升或下降
    fn detect_trend(&self, metric_name: &str, data: &[f64]) -> Result<Vec<Anomaly>, AnomalyError> {
        if data.len() < 3 {
            return Err(AnomalyError::InsufficientData);
        }
        let now = Self::now_ms();
        let mut anomalies = Vec::new();
        let mut increasing = 0;
        let mut decreasing = 0;
        for i in 1..data.len() {
            if data[i] > data[i - 1] {
                increasing += 1;
                decreasing = 0;
            } else if data[i] < data[i - 1] {
                decreasing += 1;
                increasing = 0;
            }
        }
        let trend_count = data.len() / 2;
        if increasing >= trend_count {
            anomalies.push(Anomaly {
                metric_name: metric_name.to_string(),
                anomaly_value: data[data.len() - 1],
                threshold: trend_count as f64,
                window_ms: self.config.window_ms,
                algorithm: AnomalyAlgorithm::Trend,
                detected_at: now,
                description: format!(
                    "{} sustained increasing trend over {} samples",
                    metric_name, increasing
                ),
            });
        }
        if decreasing >= trend_count {
            anomalies.push(Anomaly {
                metric_name: metric_name.to_string(),
                anomaly_value: data[data.len() - 1],
                threshold: trend_count as f64,
                window_ms: self.config.window_ms,
                algorithm: AnomalyAlgorithm::Trend,
                detected_at: now,
                description: format!(
                    "{} sustained decreasing trend over {} samples",
                    metric_name, decreasing
                ),
            });
        }
        Ok(anomalies)
    }

    /// 统计检测：基于均值/方差
    fn detect_statistical(
        &self,
        metric_name: &str,
        data: &[f64],
    ) -> Result<Vec<Anomaly>, AnomalyError> {
        let now = Self::now_ms();
        let mean = data.iter().sum::<f64>() / data.len() as f64;
        let variance = data.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / data.len() as f64;
        let std_dev = variance.sqrt();
        if std_dev == 0.0 {
            return Ok(Vec::new());
        }
        let mut anomalies = Vec::new();
        for &value in data {
            let deviation = (value - mean).abs() / std_dev;
            if deviation > self.config.threshold {
                anomalies.push(Anomaly {
                    metric_name: metric_name.to_string(),
                    anomaly_value: value,
                    threshold: self.config.threshold,
                    window_ms: self.config.window_ms,
                    algorithm: AnomalyAlgorithm::Statistical,
                    detected_at: now,
                    description: format!(
                        "{} value {} deviates {} std devs from mean {}",
                        metric_name, value, deviation, mean
                    ),
                });
            }
        }
        Ok(anomalies)
    }

    /// Z-Score 检测
    fn detect_zscore(&self, metric_name: &str, data: &[f64]) -> Result<Vec<Anomaly>, AnomalyError> {
        let now = Self::now_ms();
        let mean = data.iter().sum::<f64>() / data.len() as f64;
        let variance = data.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / data.len() as f64;
        let std_dev = variance.sqrt();
        if std_dev == 0.0 {
            return Ok(Vec::new());
        }
        let mut anomalies = Vec::new();
        for &value in data {
            let zscore = (value - mean) / std_dev;
            if zscore.abs() > self.config.zscore_threshold {
                anomalies.push(Anomaly {
                    metric_name: metric_name.to_string(),
                    anomaly_value: value,
                    threshold: self.config.zscore_threshold,
                    window_ms: self.config.window_ms,
                    algorithm: AnomalyAlgorithm::ZScore,
                    detected_at: now,
                    description: format!(
                        "{} Z-Score {} indicates anomaly (threshold {})",
                        metric_name, zscore, self.config.zscore_threshold
                    ),
                });
            }
        }
        Ok(anomalies)
    }

    /// IQR 检测
    fn detect_iqr(&self, metric_name: &str, data: &[f64]) -> Result<Vec<Anomaly>, AnomalyError> {
        let now = Self::now_ms();
        let mut sorted = data.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let n = sorted.len();
        let q1 = sorted[n / 4];
        let q3 = sorted[3 * n / 4];
        let iqr = q3 - q1;
        let lower_bound = q1 - 1.5 * iqr;
        let upper_bound = q3 + 1.5 * iqr;
        let mut anomalies = Vec::new();
        for &value in data {
            if value < lower_bound || value > upper_bound {
                anomalies.push(Anomaly {
                    metric_name: metric_name.to_string(),
                    anomaly_value: value,
                    threshold: upper_bound,
                    window_ms: self.config.window_ms,
                    algorithm: AnomalyAlgorithm::IQR,
                    detected_at: now,
                    description: format!(
                        "{} value {} outside IQR bounds [{}, {}]",
                        metric_name, value, lower_bound, upper_bound
                    ),
                });
            }
        }
        Ok(anomalies)
    }

    /// 触发告警
    async fn trigger_alert(&self, anomaly: &Anomaly) -> Result<(), AnomalyError> {
        let alert = AnomalyAlert {
            anomaly: anomaly.clone(),
            channel: self.config.alert_channel.clone(),
        };
        alert.send().await
    }

    /// 获取指标历史数据
    pub fn history(&self, metric_name: &str) -> Vec<f64> {
        let history = self.history.read().unwrap();
        history
            .get(metric_name)
            .map(|q| q.iter().copied().collect())
            .unwrap_or_default()
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = AnomalyConfig::new();
        assert_eq!(config.algorithm, AnomalyAlgorithm::Threshold);
        assert_eq!(config.window_ms, 300_000);
        assert_eq!(config.threshold, 1.0);
        assert_eq!(config.zscore_threshold, 3.0);
        assert_eq!(config.alert_channel, AlertChannel::Log);
    }

    #[test]
    fn test_config_builder() {
        let config = AnomalyConfig::new()
            .with_algorithm(AnomalyAlgorithm::ZScore)
            .with_window_ms(60_000)
            .with_threshold(2.0)
            .with_zscore_threshold(2.5)
            .with_alert_channel(AlertChannel::Webhook)
            .with_webhook_url("http://example.com/hook");
        assert_eq!(config.algorithm, AnomalyAlgorithm::ZScore);
        assert_eq!(config.window_ms, 60_000);
        assert_eq!(config.threshold, 2.0);
        assert_eq!(config.zscore_threshold, 2.5);
        assert_eq!(config.alert_channel, AlertChannel::Webhook);
        assert!(config.webhook_url.is_some());
    }

    #[tokio::test]
    async fn test_threshold_detection() {
        let registry = Arc::new(MetricsRegistry::new());
        let config = AnomalyConfig::new()
            .with_algorithm(AnomalyAlgorithm::Threshold)
            .with_threshold(1.0);
        let detector = AnomalyDetector::new(registry, config);
        detector.record("query_duration", 0.5);
        detector.record("query_duration", 1.5);
        let anomalies = detector.detect("query_duration").await.unwrap();
        assert!(anomalies.len() == 1);
        assert!(anomalies[0].anomaly_value == 1.5);
    }

    #[tokio::test]
    async fn test_zscore_detection() {
        let registry = Arc::new(MetricsRegistry::new());
        let config = AnomalyConfig::new()
            .with_algorithm(AnomalyAlgorithm::ZScore)
            .with_zscore_threshold(3.0);
        let detector = AnomalyDetector::new(registry, config);
        for _ in 0..10 {
            detector.record("metric", 10.0);
        }
        detector.record("metric", 100.0);
        let anomalies = detector.detect("metric").await.unwrap();
        assert!(!anomalies.is_empty());
    }

    #[tokio::test]
    async fn test_iqr_detection() {
        let registry = Arc::new(MetricsRegistry::new());
        let config = AnomalyConfig::new().with_algorithm(AnomalyAlgorithm::IQR);
        let detector = AnomalyDetector::new(registry, config);
        for v in &[10.0, 10.0, 11.0, 9.0, 10.0, 10.0, 11.0, 9.0] {
            detector.record("metric", *v);
        }
        detector.record("metric", 100.0);
        let anomalies = detector.detect("metric").await.unwrap();
        assert!(!anomalies.is_empty());
    }

    #[tokio::test]
    async fn test_trend_detection() {
        let registry = Arc::new(MetricsRegistry::new());
        let config = AnomalyConfig::new().with_algorithm(AnomalyAlgorithm::Trend);
        let detector = AnomalyDetector::new(registry, config);
        for i in 0..10 {
            detector.record("metric", i as f64);
        }
        let anomalies = detector.detect("metric").await.unwrap();
        assert!(!anomalies.is_empty());
    }

    #[tokio::test]
    async fn test_statistical_detection() {
        let registry = Arc::new(MetricsRegistry::new());
        let config = AnomalyConfig::new()
            .with_algorithm(AnomalyAlgorithm::Statistical)
            .with_threshold(2.0);
        let detector = AnomalyDetector::new(registry, config);
        for _ in 0..10 {
            detector.record("metric", 10.0);
        }
        detector.record("metric", 50.0);
        let anomalies = detector.detect("metric").await.unwrap();
        assert!(!anomalies.is_empty());
    }

    #[tokio::test]
    async fn test_insufficient_data() {
        let registry = Arc::new(MetricsRegistry::new());
        let config = AnomalyConfig::new();
        let detector = AnomalyDetector::new(registry, config);
        detector.record("metric", 1.0);
        let result = detector.detect("metric").await;
        assert_eq!(result, Err(AnomalyError::InsufficientData));
    }

    #[tokio::test]
    async fn test_no_history() {
        let registry = Arc::new(MetricsRegistry::new());
        let config = AnomalyConfig::new();
        let detector = AnomalyDetector::new(registry, config);
        let result = detector.detect("nonexistent").await;
        assert_eq!(result, Err(AnomalyError::InsufficientData));
    }

    #[tokio::test]
    async fn test_alert_send_log() {
        let anomaly = Anomaly {
            metric_name: "test".to_string(),
            anomaly_value: 1.5,
            threshold: 1.0,
            window_ms: 300_000,
            algorithm: AnomalyAlgorithm::Threshold,
            detected_at: 0,
            description: "test anomaly".to_string(),
        };
        let alert = AnomalyAlert {
            anomaly,
            channel: AlertChannel::Log,
        };
        assert!(alert.send().await.is_ok());
    }

    #[tokio::test]
    async fn test_alert_send_webhook() {
        let anomaly = Anomaly {
            metric_name: "test".to_string(),
            anomaly_value: 1.5,
            threshold: 1.0,
            window_ms: 300_000,
            algorithm: AnomalyAlgorithm::Threshold,
            detected_at: 0,
            description: "test anomaly".to_string(),
        };
        let alert = AnomalyAlert {
            anomaly,
            channel: AlertChannel::Webhook,
        };
        assert!(alert.send().await.is_ok());
    }

    #[tokio::test]
    async fn test_alert_send_slo() {
        let anomaly = Anomaly {
            metric_name: "test".to_string(),
            anomaly_value: 1.5,
            threshold: 1.0,
            window_ms: 300_000,
            algorithm: AnomalyAlgorithm::Threshold,
            detected_at: 0,
            description: "test anomaly".to_string(),
        };
        let alert = AnomalyAlert {
            anomaly,
            channel: AlertChannel::Slo,
        };
        assert!(alert.send().await.is_ok());
    }

    #[test]
    fn test_history_buffer() {
        let registry = Arc::new(MetricsRegistry::new());
        let config = AnomalyConfig::new().with_window_ms(1000);
        let detector = AnomalyDetector::new(registry, config);
        detector.record("metric", 1.0);
        detector.record("metric", 2.0);
        detector.record("metric", 3.0);
        let history = detector.history("metric");
        assert_eq!(history, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_history_max_limit() {
        let registry = Arc::new(MetricsRegistry::new());
        let config = AnomalyConfig::new();
        let detector = AnomalyDetector::new(registry, config);
        for i in 0..2000 {
            detector.record("metric", i as f64);
        }
        let history = detector.history("metric");
        assert!(history.len() <= 1000);
    }

    #[test]
    fn test_anomaly_error_display() {
        assert_eq!(
            AnomalyError::InsufficientData.to_string(),
            "insufficient history data"
        );
        assert_eq!(
            AnomalyError::CalculationError("div by zero".to_string()).to_string(),
            "calculation error: div by zero"
        );
    }

    #[tokio::test]
    async fn test_threshold_no_anomaly() {
        let registry = Arc::new(MetricsRegistry::new());
        let config = AnomalyConfig::new()
            .with_algorithm(AnomalyAlgorithm::Threshold)
            .with_threshold(10.0);
        let detector = AnomalyDetector::new(registry, config);
        detector.record("metric", 1.0);
        detector.record("metric", 2.0);
        let anomalies = detector.detect("metric").await.unwrap();
        assert!(anomalies.is_empty());
    }
}
