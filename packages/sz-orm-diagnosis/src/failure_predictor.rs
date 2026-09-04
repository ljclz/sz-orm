//! 故障预测器模块
//!
//! 基于性能指标时序数据（连接池使用率/慢查询比例）预测未来故障，
//! 输出预警信息。连接池使用率连续 10 分钟 > 80% 时触发预警。

use serde::{Deserialize, Serialize};

/// 性能指标采样点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSample {
    /// 时间戳（Unix 秒）
    pub timestamp: u64,
    /// 连接池使用率（0.0~1.0）
    pub pool_utilization: f64,
    /// 慢查询比例（0.0~1.0）
    pub slow_query_ratio: f64,
    /// 错误率（0.0~1.0）
    pub error_rate: f64,
}

impl MetricSample {
    /// 创建一个指标采样点
    pub fn new(
        timestamp: u64,
        pool_utilization: f64,
        slow_query_ratio: f64,
        error_rate: f64,
    ) -> Self {
        Self {
            timestamp,
            pool_utilization: pool_utilization.clamp(0.0, 1.0),
            slow_query_ratio: slow_query_ratio.clamp(0.0, 1.0),
            error_rate: error_rate.clamp(0.0, 1.0),
        }
    }
}

/// 故障预警级别
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertSeverity {
    /// 信息
    Info,
    /// 警告
    Warning,
    /// 严重
    Critical,
}

/// 故障预警
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureAlert {
    /// 预警级别
    pub severity: AlertSeverity,
    /// 预警标题
    pub title: String,
    /// 预警详情
    pub description: String,
    /// 预测故障发生时间（Unix 秒，None = 已触发）
    pub predicted_failure_time: Option<u64>,
    /// 预警指标名
    pub metric_name: String,
    /// 当前指标值
    pub current_value: f64,
    /// 阈值
    pub threshold: f64,
}

/// 故障预测结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailurePrediction {
    /// 触发的预警列表
    pub alerts: Vec<FailureAlert>,
    /// 连接池使用率趋势（每分钟变化率）
    pub pool_utilization_trend: f64,
    /// 慢查询比例趋势
    pub slow_query_trend: f64,
    /// 整体健康评分（0-100，越高越健康）
    pub health_score: f64,
    /// 预测说明
    pub summary: String,
}

/// 故障预测器配置
#[derive(Debug, Clone)]
pub struct FailurePredictorConfig {
    /// 连接池使用率预警阈值（默认 0.8）
    pub pool_utilization_threshold: f64,
    /// 慢查询比例预警阈值（默认 0.2）
    pub slow_query_threshold: f64,
    /// 错误率预警阈值（默认 0.05）
    pub error_rate_threshold: f64,
    /// 连续超阈值持续时长触发预警（秒，默认 600 = 10 分钟）
    pub sustained_duration_secs: u64,
    /// 采样间隔（秒，默认 60）
    pub sample_interval_secs: u64,
}

impl Default for FailurePredictorConfig {
    fn default() -> Self {
        Self {
            pool_utilization_threshold: 0.8,
            slow_query_threshold: 0.2,
            error_rate_threshold: 0.05,
            sustained_duration_secs: 600,
            sample_interval_secs: 60,
        }
    }
}

/// 故障预测器
///
/// 基于性能指标时序数据预测未来故障。
///
/// # 预警规则
///
/// - 连接池使用率连续 > 80% 持续 10 分钟 → Warning
/// - 连接池使用率 > 95% → Critical
/// - 慢查询比例 > 20% 且持续上升 → Warning
/// - 错误率 > 5% → Critical
pub struct FailurePredictor {
    config: FailurePredictorConfig,
}

impl Default for FailurePredictor {
    fn default() -> Self {
        Self::new(FailurePredictorConfig::default())
    }
}

impl FailurePredictor {
    /// 创建故障预测器
    pub fn new(config: FailurePredictorConfig) -> Self {
        Self { config }
    }

    /// 预测故障
    ///
    /// # 参数
    /// - `samples`: 按时间排序的性能指标时序数据
    ///
    /// # 返回
    ///
    /// [`FailurePrediction`] 包含预警列表 + 趋势 + 健康评分。
    pub fn predict(&self, samples: &[MetricSample]) -> FailurePrediction {
        let mut alerts = Vec::new();

        if samples.is_empty() {
            return FailurePrediction {
                alerts,
                pool_utilization_trend: 0.0,
                slow_query_trend: 0.0,
                health_score: 100.0,
                summary: "无指标数据".to_string(),
            };
        }

        let latest = samples.last().unwrap();
        let pool_trend = self.compute_trend(samples, |s| s.pool_utilization);
        let slow_trend = self.compute_trend(samples, |s| s.slow_query_ratio);

        self.check_pool_utilization(samples, latest, &mut alerts);
        self.check_slow_query(samples, latest, slow_trend, &mut alerts);
        self.check_error_rate(latest, &mut alerts);

        let health_score = self.compute_health_score(latest, &alerts);
        let summary = self.build_summary(latest, &alerts, pool_trend, slow_trend);

        FailurePrediction {
            alerts,
            pool_utilization_trend: pool_trend,
            slow_query_trend: slow_trend,
            health_score,
            summary,
        }
    }

    fn compute_trend<F>(&self, samples: &[MetricSample], extract: F) -> f64
    where
        F: Fn(&MetricSample) -> f64,
    {
        if samples.len() < 2 {
            return 0.0;
        }
        let n = samples.len() as f64;
        let sum_x: f64 = (0..samples.len()).map(|i| i as f64).sum();
        let sum_y: f64 = samples.iter().map(&extract).sum();
        let sum_xy: f64 = samples
            .iter()
            .enumerate()
            .map(|(i, s)| i as f64 * extract(s))
            .sum();
        let sum_x2: f64 = (0..samples.len()).map(|i| (i as f64).powi(2)).sum();

        let denominator = n * sum_x2 - sum_x * sum_x;
        if denominator.abs() < 1e-12 {
            return 0.0;
        }
        (n * sum_xy - sum_x * sum_y) / denominator
    }

    fn check_pool_utilization(
        &self,
        samples: &[MetricSample],
        latest: &MetricSample,
        alerts: &mut Vec<FailureAlert>,
    ) {
        if latest.pool_utilization > 0.95 {
            let predicted_time = self.predict_failure_time(samples, |s| s.pool_utilization, 1.0);
            alerts.push(FailureAlert {
                severity: AlertSeverity::Critical,
                title: "连接池即将耗尽".to_string(),
                description: format!(
                    "连接池使用率 {:.1}% 已超过 95% 临界值，可能立即导致连接获取失败",
                    latest.pool_utilization * 100.0
                ),
                predicted_failure_time: predicted_time,
                metric_name: "pool_utilization".to_string(),
                current_value: latest.pool_utilization,
                threshold: 0.95,
            });
            return;
        }

        let sustained_secs = self.compute_sustained_duration(samples, |s| {
            s.pool_utilization > self.config.pool_utilization_threshold
        });

        if sustained_secs >= self.config.sustained_duration_secs {
            let predicted_time = self.predict_failure_time(samples, |s| s.pool_utilization, 1.0);
            let minutes = sustained_secs / 60;
            alerts.push(FailureAlert {
                severity: AlertSeverity::Warning,
                title: "连接池使用率持续偏高".to_string(),
                description: format!(
                    "连接池使用率连续 {} 分钟超过 {:.0}% 阈值，当前 {:.1}%",
                    minutes,
                    self.config.pool_utilization_threshold * 100.0,
                    latest.pool_utilization * 100.0
                ),
                predicted_failure_time: predicted_time,
                metric_name: "pool_utilization".to_string(),
                current_value: latest.pool_utilization,
                threshold: self.config.pool_utilization_threshold,
            });
        }
    }

    fn check_slow_query(
        &self,
        samples: &[MetricSample],
        latest: &MetricSample,
        trend: f64,
        alerts: &mut Vec<FailureAlert>,
    ) {
        if latest.slow_query_ratio > self.config.slow_query_threshold && trend > 0.0 {
            let predicted_time = self.predict_failure_time(samples, |s| s.slow_query_ratio, 0.5);
            alerts.push(FailureAlert {
                severity: AlertSeverity::Warning,
                title: "慢查询比例上升".to_string(),
                description: format!(
                    "慢查询比例 {:.1}% 超过阈值 {:.0}% 且呈上升趋势（每分钟 +{:.4}）",
                    latest.slow_query_ratio * 100.0,
                    self.config.slow_query_threshold * 100.0,
                    trend
                ),
                predicted_failure_time: predicted_time,
                metric_name: "slow_query_ratio".to_string(),
                current_value: latest.slow_query_ratio,
                threshold: self.config.slow_query_threshold,
            });
        }
    }

    fn check_error_rate(&self, latest: &MetricSample, alerts: &mut Vec<FailureAlert>) {
        if latest.error_rate > self.config.error_rate_threshold {
            alerts.push(FailureAlert {
                severity: AlertSeverity::Critical,
                title: "错误率过高".to_string(),
                description: format!(
                    "错误率 {:.2}% 超过阈值 {:.2}%，需立即排查",
                    latest.error_rate * 100.0,
                    self.config.error_rate_threshold * 100.0
                ),
                predicted_failure_time: None,
                metric_name: "error_rate".to_string(),
                current_value: latest.error_rate,
                threshold: self.config.error_rate_threshold,
            });
        }
    }

    fn compute_sustained_duration<F>(&self, samples: &[MetricSample], predicate: F) -> u64
    where
        F: Fn(&MetricSample) -> bool,
    {
        if samples.is_empty() {
            return 0;
        }
        let mut sustained_count: u64 = 0;
        for sample in samples.iter().rev() {
            if predicate(sample) {
                sustained_count += 1;
            } else {
                break;
            }
        }
        sustained_count * self.config.sample_interval_secs
    }

    fn predict_failure_time<F>(
        &self,
        samples: &[MetricSample],
        extract: F,
        threshold: f64,
    ) -> Option<u64>
    where
        F: Fn(&MetricSample) -> f64,
    {
        if samples.len() < 2 {
            return None;
        }
        let trend = self.compute_trend(samples, &extract);
        if trend <= 1e-9 {
            return None;
        }
        let latest = samples.last().unwrap();
        let current = extract(latest);
        if current >= threshold {
            return None;
        }
        let steps_to_threshold = (threshold - current) / trend;
        if steps_to_threshold < 0.0 || !steps_to_threshold.is_finite() {
            return None;
        }
        let predicted_secs = latest.timestamp
            + (steps_to_threshold * self.config.sample_interval_secs as f64) as u64;
        Some(predicted_secs)
    }

    fn compute_health_score(&self, latest: &MetricSample, alerts: &[FailureAlert]) -> f64 {
        let mut score = 100.0;
        score -= latest.pool_utilization * 30.0;
        score -= latest.slow_query_ratio * 30.0;
        score -= latest.error_rate * 40.0;
        for alert in alerts {
            match alert.severity {
                AlertSeverity::Critical => score -= 20.0,
                AlertSeverity::Warning => score -= 10.0,
                AlertSeverity::Info => score -= 5.0,
            }
        }
        score.clamp(0.0, 100.0)
    }

    fn build_summary(
        &self,
        latest: &MetricSample,
        alerts: &[FailureAlert],
        pool_trend: f64,
        slow_trend: f64,
    ) -> String {
        if alerts.is_empty() {
            return format!(
                "系统健康（池使用率 {:.1}%，慢查询 {:.1}%，错误率 {:.2}%）",
                latest.pool_utilization * 100.0,
                latest.slow_query_ratio * 100.0,
                latest.error_rate * 100.0
            );
        }
        let critical_count = alerts
            .iter()
            .filter(|a| a.severity == AlertSeverity::Critical)
            .count();
        let warning_count = alerts
            .iter()
            .filter(|a| a.severity == AlertSeverity::Warning)
            .count();
        format!(
            "发现 {} 条预警（{} 严重 + {} 警告），池趋势 {:+.4}/min，慢查询趋势 {:+.4}/min",
            alerts.len(),
            critical_count,
            warning_count,
            pool_trend,
            slow_trend
        )
    }
}
