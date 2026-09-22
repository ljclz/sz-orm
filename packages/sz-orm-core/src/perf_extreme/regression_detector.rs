//! v8.0.0 任务 2.6：性能退化检测器
//!
//! 对比当前指标与基线，退化 ≥ 10% 触发告警。
//! `RegressionAlert` 含 `metric_name`/`degradation_pct`/`baseline_value`/`current_value`。

/// 退化检测配置
#[derive(Debug, Clone)]
pub struct RegressionConfig {
    /// 退化告警阈值（0.0 ~ 1.0，0.1 = 10%）
    pub degradation_threshold: f64,
}

impl Default for RegressionConfig {
    fn default() -> Self {
        Self {
            degradation_threshold: 0.1,
        }
    }
}

/// 性能指标快照（用于退化对比）
#[derive(Debug, Clone, PartialEq)]
pub struct PerfMetricsSnapshot {
    /// QPS
    pub qps: f64,
    /// 平均延迟（微秒）
    pub avg_latency_us: f64,
    /// P99 延迟（微秒）
    pub p99_latency_us: f64,
    /// 连接池 acquire 成功率
    pub pool_acquire_success_rate: f64,
    /// 零拷贝命中率
    pub zero_copy_hit_rate: f64,
}

/// 退化告警
#[derive(Debug, Clone, PartialEq)]
pub struct RegressionAlert {
    /// 退化指标名
    pub metric_name: String,
    /// 退化百分比（0.15 = 15%）
    pub degradation_pct: f64,
    /// 基线值
    pub baseline_value: f64,
    /// 当前值
    pub current_value: f64,
}

/// 性能退化检测器
pub struct PerfRegressionDetector {
    config: RegressionConfig,
}

impl PerfRegressionDetector {
    /// 创建退化检测器
    pub fn new(config: RegressionConfig) -> Self {
        Self { config }
    }

    /// 默认配置构造
    pub fn with_default() -> Self {
        Self::new(RegressionConfig::default())
    }

    /// 获取配置引用
    pub fn config(&self) -> &RegressionConfig {
        &self.config
    }

    /// 检测性能退化
    ///
    /// 对比当前指标与基线，退化 ≥ `degradation_threshold` 返回告警。
    /// - QPS 退化：current < baseline × (1 - threshold)
    /// - 延迟退化：current > baseline × (1 + threshold)
    /// - 成功率/命中率退化：current < baseline × (1 - threshold)
    ///
    /// 基线缺失（baseline_value == 0.0）→ 跳过该指标。
    pub fn detect(
        &self,
        current: &PerfMetricsSnapshot,
        baseline: &PerfMetricsSnapshot,
    ) -> Option<RegressionAlert> {
        // QPS 退化（越低越差）
        if let Some(alert) = self.check_degradation_lower_better("qps", baseline.qps, current.qps) {
            return Some(alert);
        }
        // 平均延迟退化（越高越差）
        if let Some(alert) = self.check_degradation_higher_worse(
            "avg_latency_us",
            baseline.avg_latency_us,
            current.avg_latency_us,
        ) {
            return Some(alert);
        }
        // P99 延迟退化
        if let Some(alert) = self.check_degradation_higher_worse(
            "p99_latency_us",
            baseline.p99_latency_us,
            current.p99_latency_us,
        ) {
            return Some(alert);
        }
        // acquire 成功率退化
        if let Some(alert) = self.check_degradation_lower_better(
            "pool_acquire_success_rate",
            baseline.pool_acquire_success_rate,
            current.pool_acquire_success_rate,
        ) {
            return Some(alert);
        }
        // 零拷贝命中率退化
        if let Some(alert) = self.check_degradation_lower_better(
            "zero_copy_hit_rate",
            baseline.zero_copy_hit_rate,
            current.zero_copy_hit_rate,
        ) {
            return Some(alert);
        }
        None
    }

    fn check_degradation_lower_better(
        &self,
        name: &str,
        baseline: f64,
        current: f64,
    ) -> Option<RegressionAlert> {
        if baseline <= 0.0 {
            return None;
        }
        let degradation = (baseline - current) / baseline;
        if degradation >= self.config.degradation_threshold {
            return Some(RegressionAlert {
                metric_name: name.to_string(),
                degradation_pct: degradation,
                baseline_value: baseline,
                current_value: current,
            });
        }
        None
    }

    fn check_degradation_higher_worse(
        &self,
        name: &str,
        baseline: f64,
        current: f64,
    ) -> Option<RegressionAlert> {
        if baseline <= 0.0 {
            return None;
        }
        let degradation = (current - baseline) / baseline;
        if degradation >= self.config.degradation_threshold {
            return Some(RegressionAlert {
                metric_name: name.to_string(),
                degradation_pct: degradation,
                baseline_value: baseline,
                current_value: current,
            });
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics(qps: f64, lat: f64, p99: f64, succ: f64, zc: f64) -> PerfMetricsSnapshot {
        PerfMetricsSnapshot {
            qps,
            avg_latency_us: lat,
            p99_latency_us: p99,
            pool_acquire_success_rate: succ,
            zero_copy_hit_rate: zc,
        }
    }

    #[test]
    fn detect_qps_degradation_15pct() {
        let detector = PerfRegressionDetector::with_default();
        let baseline = metrics(1000.0, 10.0, 20.0, 1.0, 0.9);
        let current = metrics(850.0, 10.0, 20.0, 1.0, 0.9);
        let alert = detector.detect(&current, &baseline);
        assert!(alert.is_some());
        let alert = alert.unwrap();
        assert_eq!(alert.metric_name, "qps");
        assert!((alert.degradation_pct - 0.15).abs() < 1e-6);
    }

    #[test]
    fn detect_latency_degradation_20pct() {
        let detector = PerfRegressionDetector::with_default();
        let baseline = metrics(1000.0, 10.0, 20.0, 1.0, 0.9);
        let current = metrics(1000.0, 12.0, 20.0, 1.0, 0.9);
        let alert = detector.detect(&current, &baseline);
        assert!(alert.is_some());
        assert_eq!(alert.unwrap().metric_name, "avg_latency_us");
    }

    #[test]
    fn detect_no_degradation_returns_none() {
        let detector = PerfRegressionDetector::with_default();
        let baseline = metrics(1000.0, 10.0, 20.0, 1.0, 0.9);
        let current = metrics(1050.0, 9.0, 18.0, 1.0, 0.95);
        assert!(detector.detect(&current, &baseline).is_none());
    }

    #[test]
    fn detect_baseline_zero_skipped() {
        let detector = PerfRegressionDetector::with_default();
        let baseline = metrics(0.0, 10.0, 20.0, 1.0, 0.9);
        let current = metrics(1000.0, 10.0, 20.0, 1.0, 0.9);
        // qps baseline=0 跳过，其他指标无退化
        assert!(detector.detect(&current, &baseline).is_none());
    }

    #[test]
    fn detect_zero_copy_hit_rate_degradation() {
        let detector = PerfRegressionDetector::with_default();
        let baseline = metrics(1000.0, 10.0, 20.0, 1.0, 0.9);
        let current = metrics(1000.0, 10.0, 20.0, 1.0, 0.7);
        let alert = detector.detect(&current, &baseline);
        assert!(alert.is_some());
        assert_eq!(alert.unwrap().metric_name, "zero_copy_hit_rate");
    }

    #[test]
    fn custom_threshold_5pct() {
        let detector = PerfRegressionDetector::new(RegressionConfig {
            degradation_threshold: 0.05,
        });
        let baseline = metrics(1000.0, 10.0, 20.0, 1.0, 0.9);
        let current = metrics(940.0, 10.0, 20.0, 1.0, 0.9);
        // 6% 退化 ≥ 5% 阈值
        assert!(detector.detect(&current, &baseline).is_some());
    }
}
