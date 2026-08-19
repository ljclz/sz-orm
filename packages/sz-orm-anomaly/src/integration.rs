//! 集成模块：Prometheus 指标导出 + 健康度集成
//!
//! Prometheus 导出 `anomaly_count` / `anomaly_last_timestamp` 指标。
//! 健康度集成：CRITICAL 异常降低健康度。

use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::alert::{Alert, AnomalyType, Severity};

/// Prometheus 指标导出器
///
/// 导出 `anomaly_count` / `anomaly_last_timestamp` 指标。
/// 若 sz-orm-observability 未接入则跳过（仅输出文本格式）。
pub struct PrometheusExporter {
    /// 各类型异常计数
    counts: RwLock<std::collections::HashMap<AnomalyType, u64>>,
    /// 各类型最后告警时间戳
    last_timestamps: RwLock<std::collections::HashMap<AnomalyType, u64>>,
    /// 累计告警数
    total_count: AtomicU64,
}

impl PrometheusExporter {
    /// 创建 Prometheus 导出器
    pub fn new() -> Self {
        Self {
            counts: RwLock::new(std::collections::HashMap::new()),
            last_timestamps: RwLock::new(std::collections::HashMap::new()),
            total_count: AtomicU64::new(0),
        }
    }

    /// 记录告警
    pub fn record_alert(&self, alert: &Alert) {
        let mut counts = self.counts.write();
        *counts.entry(alert.anomaly_type).or_insert(0) += 1;
        drop(counts);
        let mut timestamps = self.last_timestamps.write();
        timestamps.insert(alert.anomaly_type, alert.timestamp);
        drop(timestamps);
        self.total_count.fetch_add(1, Ordering::Relaxed);
    }

    /// 导出 Prometheus 格式指标
    ///
    /// 输出格式：
    /// ```text
    /// # HELP anomaly_count Total number of anomalies detected
    /// # TYPE anomaly_count counter
    /// anomaly_count{type="slow_query_spike"} 5
    /// anomaly_count{type="error_rate_spike"} 2
    /// # HELP anomaly_last_timestamp Last anomaly timestamp
    /// # TYPE anomaly_last_timestamp gauge
    /// anomaly_last_timestamp{type="slow_query_spike"} 1234567890
    /// ```
    pub fn export_metrics(&self) -> String {
        let counts = self.counts.read();
        let timestamps = self.last_timestamps.read();
        let mut output = String::new();

        output.push_str("# HELP anomaly_count Total number of anomalies detected\n");
        output.push_str("# TYPE anomaly_count counter\n");
        for (atype, count) in counts.iter() {
            output.push_str(&format!(
                "anomaly_count{{type=\"{}\"}} {}\n",
                atype.as_str(),
                count
            ));
        }
        output.push_str(&format!(
            "anomaly_count_total {}\n",
            self.total_count.load(Ordering::Relaxed)
        ));

        output.push_str("\n# HELP anomaly_last_timestamp Last anomaly timestamp\n");
        output.push_str("# TYPE anomaly_last_timestamp gauge\n");
        for (atype, ts) in timestamps.iter() {
            output.push_str(&format!(
                "anomaly_last_timestamp{{type=\"{}\"}} {}\n",
                atype.as_str(),
                ts
            ));
        }

        output
    }

    /// 获取某类型的异常计数
    pub fn count(&self, anomaly_type: AnomalyType) -> u64 {
        self.counts.read().get(&anomaly_type).copied().unwrap_or(0)
    }

    /// 获取累计告警数
    pub fn total_count(&self) -> u64 {
        self.total_count.load(Ordering::Relaxed)
    }

    /// 重置指标
    pub fn reset(&self) {
        self.counts.write().clear();
        self.last_timestamps.write().clear();
        self.total_count.store(0, Ordering::Relaxed);
    }
}

impl Default for PrometheusExporter {
    fn default() -> Self {
        Self::new()
    }
}

/// 健康度影响
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HealthImpact {
    /// 当前健康度（0.0 ~ 1.0，1.0 表示完全健康）
    pub health_score: f64,
    /// CRITICAL 异常数
    pub critical_count: u64,
    /// WARN 异常数
    pub warn_count: u64,
}

impl Default for HealthImpact {
    fn default() -> Self {
        Self {
            health_score: 1.0,
            critical_count: 0,
            warn_count: 0,
        }
    }
}

impl HealthImpact {
    /// 创建健康度影响
    pub fn new() -> Self {
        Self::default()
    }

    /// 应用告警影响
    ///
    /// - CRITICAL：健康度降低 0.1（最低 0.0）
    /// - WARN：健康度降低 0.02（最低 0.0）
    /// - INFO：不影响
    pub fn apply_alert(&mut self, alert: &Alert) {
        match alert.severity {
            Severity::Critical => {
                self.critical_count += 1;
                self.health_score = (self.health_score - 0.1).max(0.0);
            }
            Severity::Warn => {
                self.warn_count += 1;
                self.health_score = (self.health_score - 0.02).max(0.0);
            }
            Severity::Info => {}
        }
    }

    /// 恢复健康度（每次恢复 0.05，最高 1.0）
    pub fn recover(&mut self) {
        self.health_score = (self.health_score + 0.05).min(1.0);
    }

    /// 是否健康（健康度 > 0.5）
    pub fn is_healthy(&self) -> bool {
        self.health_score > 0.5
    }

    /// 健康状态字符串
    pub fn status(&self) -> &'static str {
        if self.health_score > 0.8 {
            "healthy"
        } else if self.health_score > 0.5 {
            "degraded"
        } else if self.health_score > 0.2 {
            "unhealthy"
        } else {
            "critical"
        }
    }
}

/// 健康度集成器
///
/// 维护当前健康度，CRITICAL 异常降低健康度。
/// 若 sz-orm-health 未接入则仅维护本地健康度。
pub struct HealthIntegrator {
    impact: RwLock<HealthImpact>,
}

impl HealthIntegrator {
    /// 创建健康度集成器
    pub fn new() -> Self {
        Self {
            impact: RwLock::new(HealthImpact::new()),
        }
    }

    /// 应用告警影响
    pub fn impact_health(&self, alert: &Alert) {
        self.impact.write().apply_alert(alert);
    }

    /// 获取当前健康度影响快照
    pub fn health_impact(&self) -> HealthImpact {
        *self.impact.read()
    }

    /// 获取当前健康度
    pub fn health_score(&self) -> f64 {
        self.impact.read().health_score
    }

    /// 是否健康
    pub fn is_healthy(&self) -> bool {
        self.impact.read().is_healthy()
    }

    /// 健康状态字符串
    pub fn status(&self) -> &'static str {
        self.impact.read().status()
    }

    /// 恢复健康度
    pub fn recover(&self) {
        self.impact.write().recover();
    }

    /// 重置
    pub fn reset(&self) {
        *self.impact.write() = HealthImpact::new();
    }
}

impl Default for HealthIntegrator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_alert(severity: Severity, anomaly_type: AnomalyType) -> Alert {
        Alert {
            anomaly_type,
            severity,
            timestamp: 1000,
            metric_value: 100.0,
            threshold: 50.0,
            baseline: None,
            suggestion: "test".to_string(),
            sql_summary: None,
        }
    }

    #[test]
    fn test_prometheus_exporter_record() {
        let exporter = PrometheusExporter::new();
        exporter.record_alert(&sample_alert(Severity::Warn, AnomalyType::SlowQuerySpike));
        exporter.record_alert(&sample_alert(
            Severity::Critical,
            AnomalyType::ErrorRateSpike,
        ));
        assert_eq!(exporter.count(AnomalyType::SlowQuerySpike), 1);
        assert_eq!(exporter.count(AnomalyType::ErrorRateSpike), 1);
        assert_eq!(exporter.total_count(), 2);
    }

    #[test]
    fn test_prometheus_export_metrics_format() {
        let exporter = PrometheusExporter::new();
        exporter.record_alert(&sample_alert(Severity::Warn, AnomalyType::SlowQuerySpike));
        let metrics = exporter.export_metrics();
        assert!(metrics.contains("# HELP anomaly_count"));
        assert!(metrics.contains("# TYPE anomaly_count counter"));
        assert!(metrics.contains("anomaly_count{type=\"slow_query_spike\"} 1"));
        assert!(metrics.contains("# HELP anomaly_last_timestamp"));
        assert!(metrics.contains("anomaly_last_timestamp{type=\"slow_query_spike\"}"));
    }

    #[test]
    fn test_prometheus_exporter_reset() {
        let exporter = PrometheusExporter::new();
        exporter.record_alert(&sample_alert(Severity::Warn, AnomalyType::SlowQuerySpike));
        exporter.reset();
        assert_eq!(exporter.total_count(), 0);
    }

    #[test]
    fn test_health_impact_critical() {
        let mut impact = HealthImpact::new();
        impact.apply_alert(&sample_alert(
            Severity::Critical,
            AnomalyType::SlowQuerySpike,
        ));
        assert!((impact.health_score - 0.9).abs() < 1e-9);
        assert_eq!(impact.critical_count, 1);
    }

    #[test]
    fn test_health_impact_warn() {
        let mut impact = HealthImpact::new();
        impact.apply_alert(&sample_alert(Severity::Warn, AnomalyType::SlowQuerySpike));
        assert!((impact.health_score - 0.98).abs() < 1e-9);
        assert_eq!(impact.warn_count, 1);
    }

    #[test]
    fn test_health_impact_info() {
        let mut impact = HealthImpact::new();
        impact.apply_alert(&sample_alert(Severity::Info, AnomalyType::SlowQuerySpike));
        assert!((impact.health_score - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_health_impact_recover() {
        let mut impact = HealthImpact::new();
        impact.apply_alert(&sample_alert(
            Severity::Critical,
            AnomalyType::SlowQuerySpike,
        ));
        impact.recover();
        assert!((impact.health_score - 0.95).abs() < 1e-9);
    }

    #[test]
    fn test_health_impact_status() {
        let mut impact = HealthImpact::new();
        assert_eq!(impact.status(), "healthy");
        // 降低到 degraded
        for _ in 0..3 {
            impact.apply_alert(&sample_alert(
                Severity::Critical,
                AnomalyType::SlowQuerySpike,
            ));
        }
        assert_eq!(impact.status(), "degraded");
    }

    #[test]
    fn test_health_integrator() {
        let integrator = HealthIntegrator::new();
        assert!(integrator.is_healthy());
        assert!((integrator.health_score() - 1.0).abs() < 1e-9);
        integrator.impact_health(&sample_alert(
            Severity::Critical,
            AnomalyType::SlowQuerySpike,
        ));
        assert!((integrator.health_score() - 0.9).abs() < 1e-9);
        integrator.recover();
        assert!((integrator.health_score() - 0.95).abs() < 1e-9);
    }

    #[test]
    fn test_health_impact_min_zero() {
        let mut impact = HealthImpact::new();
        // 大量 CRITICAL 告警，健康度不应低于 0
        for _ in 0..100 {
            impact.apply_alert(&sample_alert(
                Severity::Critical,
                AnomalyType::SlowQuerySpike,
            ));
        }
        assert_eq!(impact.health_score, 0.0);
    }
}
