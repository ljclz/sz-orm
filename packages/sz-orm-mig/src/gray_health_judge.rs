//! 灰度健康判定器

use super::gray_release::HealthJudgeCondition;

/// 健康指标快照
#[derive(Debug, Clone)]
pub struct HealthMetrics {
    pub error_rate: f64,
    pub latency_ms: f64,
    pub custom: std::collections::HashMap<String, f64>,
}

impl Default for HealthMetrics {
    fn default() -> Self {
        Self {
            error_rate: 0.0,
            latency_ms: 0.0,
            custom: std::collections::HashMap::new(),
        }
    }
}

/// 灰度健康判定器
pub struct GrayHealthJudge {
    conditions: Vec<HealthJudgeCondition>,
    rollback_threshold: f64,
}

impl GrayHealthJudge {
    pub fn new(conditions: Vec<HealthJudgeCondition>, rollback_threshold: f64) -> Self {
        Self {
            conditions,
            rollback_threshold,
        }
    }

    pub fn judge(&self, metrics: &HealthMetrics) -> JudgeResult {
        for condition in &self.conditions {
            let value = self.get_metric_value(metrics, &condition.metric);
            if value > condition.threshold {
                return JudgeResult {
                    healthy: false,
                    should_rollback: value > self.rollback_threshold,
                    failed_metric: Some(condition.metric.clone()),
                    actual_value: value,
                    threshold: condition.threshold,
                };
            }
        }
        JudgeResult {
            healthy: true,
            should_rollback: false,
            failed_metric: None,
            actual_value: 0.0,
            threshold: 0.0,
        }
    }

    fn get_metric_value(&self, metrics: &HealthMetrics, metric: &str) -> f64 {
        match metric {
            "error_rate" => metrics.error_rate,
            "latency_ms" => metrics.latency_ms,
            _ => *metrics.custom.get(metric).unwrap_or(&0.0),
        }
    }

    pub fn should_rollback(&self, metrics: &HealthMetrics) -> bool {
        metrics.error_rate > self.rollback_threshold
    }
}

/// 判定结果
#[derive(Debug, Clone)]
pub struct JudgeResult {
    pub healthy: bool,
    pub should_rollback: bool,
    pub failed_metric: Option<String>,
    pub actual_value: f64,
    pub threshold: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_judge() -> GrayHealthJudge {
        GrayHealthJudge::new(
            vec![
                HealthJudgeCondition {
                    metric: "error_rate".to_string(),
                    threshold: 0.01,
                },
                HealthJudgeCondition {
                    metric: "latency_ms".to_string(),
                    threshold: 500.0,
                },
            ],
            0.05,
        )
    }

    #[test]
    fn test_judge_healthy() {
        let judge = make_judge();
        let metrics = HealthMetrics {
            error_rate: 0.005,
            latency_ms: 100.0,
            custom: std::collections::HashMap::new(),
        };
        let result = judge.judge(&metrics);
        assert!(result.healthy);
        assert!(!result.should_rollback);
    }

    #[test]
    fn test_judge_error_rate_exceeded() {
        let judge = make_judge();
        let metrics = HealthMetrics {
            error_rate: 0.02,
            latency_ms: 100.0,
            custom: std::collections::HashMap::new(),
        };
        let result = judge.judge(&metrics);
        assert!(!result.healthy);
        assert!(!result.should_rollback);
    }

    #[test]
    fn test_judge_should_rollback() {
        let judge = make_judge();
        let metrics = HealthMetrics {
            error_rate: 0.06,
            latency_ms: 100.0,
            custom: std::collections::HashMap::new(),
        };
        let result = judge.judge(&metrics);
        assert!(!result.healthy);
        assert!(result.should_rollback);
    }

    #[test]
    fn test_judge_latency_exceeded() {
        let judge = make_judge();
        let metrics = HealthMetrics {
            error_rate: 0.001,
            latency_ms: 600.0,
            custom: std::collections::HashMap::new(),
        };
        let result = judge.judge(&metrics);
        assert!(!result.healthy);
        assert_eq!(result.failed_metric, Some("latency_ms".to_string()));
    }

    #[test]
    fn test_custom_metric() {
        let judge = GrayHealthJudge::new(
            vec![HealthJudgeCondition {
                metric: "custom_metric".to_string(),
                threshold: 10.0,
            }],
            100.0,
        );
        let mut metrics = HealthMetrics::default();
        metrics.custom.insert("custom_metric".to_string(), 15.0);
        let result = judge.judge(&metrics);
        assert!(!result.healthy);
    }
}
