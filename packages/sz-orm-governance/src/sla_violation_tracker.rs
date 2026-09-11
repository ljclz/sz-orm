//! SLA 违规追踪器（v6.8.0 GOV-SLA-01 + GOV-SLA-02）
//!
//! 按 SLA 目标持续度量并产出达成率时序，连续违规窗口发出告警。

use std::collections::HashMap;

/// SLA 目标
#[derive(Debug, Clone)]
pub struct SlaTarget {
    /// 最大 P99 延迟（毫秒）
    pub max_p99_ms: f64,
    /// 最大错误率（0.0 ~ D1.0）
    pub max_error_rate: f64,
}

/// SLA 样本
#[derive(Debug, Clone)]
pub struct SlaSample {
    /// P99 延迟（毫秒）
    pub p99_latency_ms: f64,
    /// 错误率
    pub error_rate: f64,
    /// 调用次数
    pub call_count: u64,
    /// SLA 目标
    pub target: SlaTarget,
}

/// SLA 窗口报告
#[derive(Debug, Clone)]
pub struct SlaWindowReport {
    /// 接口名
    pub interface: String,
    /// P99 延迟
    pub p99_latency_ms: f64,
    /// 错误率
    pub error_rate: f64,
    /// 调用次数
    pub call_count: u64,
    /// 是否达标
    pub compliant: bool,
    /// 达成率（0.0 ~ 1.0）
    pub achievement_rate: f64,
    /// 样本是否不足
    pub insufficient_samples: bool,
}

/// SLA 告警
#[derive(Debug, Clone)]
pub struct SlaAlert {
    /// 接口名
    pub interface: String,
    /// 告警码
    pub code: String,
    /// 违规指标
    pub violated_metric: String,
    /// 连续违规窗口数
    pub consecutive_violations: u32,
}

/// SLA 违规追踪器
pub struct SlaViolationTracker {
    min_samples: u64,
    alert_threshold_windows: u32,
    violation_windows: HashMap<String, u32>,
    total_windows: HashMap<String, u64>,
    compliant_windows: HashMap<String, u64>,
    alerts: Vec<SlaAlert>,
    reports: Vec<SlaWindowReport>,
}

impl SlaViolationTracker {
    /// 创建追踪器
    pub fn new(min_samples: u64, alert_threshold_windows: u32) -> Self {
        Self {
            min_samples,
            alert_threshold_windows,
            violation_windows: HashMap::new(),
            total_windows: HashMap::new(),
            compliant_windows: HashMap::new(),
            alerts: Vec::new(),
            reports: Vec::new(),
        }
    }

    /// 记录一个窗口的样本
    pub fn record_window(&mut self, interface: &str, sample: SlaSample) -> SlaWindowReport {
        let insufficient = sample.call_count < self.min_samples;
        let p99_ok = sample.p99_latency_ms <= sample.target.max_p99_ms;
        let error_ok = sample.error_rate <= sample.target.max_error_rate;
        let compliant = insufficient || (p99_ok && error_ok);

        let achievement_rate = {
            let total = self.total_windows.entry(interface.to_string()).or_insert(0);
            let compliant_count = self
                .compliant_windows
                .entry(interface.to_string())
                .or_insert(0);
            *total += 1;
            if compliant {
                *compliant_count += 1;
            }
            if *total > 0 {
                *compliant_count as f64 / *total as f64
            } else {
                0.0
            }
        };

        if !insufficient {
            let violations = self
                .violation_windows
                .entry(interface.to_string())
                .or_insert(0);
            if compliant {
                *violations = 0;
            } else {
                *violations += 1;
                if *violations >= self.alert_threshold_windows {
                    let violated_metric = if !p99_ok {
                        format!(
                            "p99_latency_ms {:.1} > target {:.1}",
                            sample.p99_latency_ms, sample.target.max_p99_ms
                        )
                    } else {
                        format!(
                            "error_rate {:.4} > target {:.4}",
                            sample.error_rate, sample.target.max_error_rate
                        )
                    };
                    self.alerts.push(SlaAlert {
                        interface: interface.to_string(),
                        code: "GOV_SLA_VIOLATED".to_string(),
                        violated_metric,
                        consecutive_violations: *violations,
                    });
                }
            }
        }

        let report = SlaWindowReport {
            interface: interface.to_string(),
            p99_latency_ms: sample.p99_latency_ms,
            error_rate: sample.error_rate,
            call_count: sample.call_count,
            compliant,
            achievement_rate,
            insufficient_samples: insufficient,
        };
        self.reports.push(report.clone());
        report
    }

    /// 返回所有告警
    pub fn alerts(&self) -> &[SlaAlert] {
        &self.alerts
    }

    /// 返回所有报告
    pub fn reports(&self) -> &[SlaWindowReport] {
        &self.reports
    }

    /// 返回接口的连续违规窗口数
    pub fn consecutive_violations(&self, interface: &str) -> u32 {
        self.violation_windows.get(interface).copied().unwrap_or(0)
    }

    /// 返回接口的达成率
    pub fn achievement_rate(&self, interface: &str) -> f64 {
        let total = self.total_windows.get(interface).copied().unwrap_or(0);
        let compliant = self.compliant_windows.get(interface).copied().unwrap_or(0);
        if total > 0 {
            compliant as f64 / total as f64
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_target() -> SlaTarget {
        SlaTarget {
            max_p99_ms: 100.0,
            max_error_rate: 0.01,
        }
    }

    fn make_sample(p99: f64, error: f64, count: u64) -> SlaSample {
        SlaSample {
            p99_latency_ms: p99,
            error_rate: error,
            call_count: count,
            target: make_target(),
        }
    }

    #[test]
    fn compliant_window_no_violation() {
        let mut tracker = SlaViolationTracker::new(10, 3);
        let report = tracker.record_window("api", make_sample(50.0, 0.001, 100));
        assert!(report.compliant);
        assert!(!report.insufficient_samples);
        assert_eq!(tracker.consecutive_violations("api"), 0);
    }

    #[test]
    fn p99_violation_detected() {
        let mut tracker = SlaViolationTracker::new(10, 3);
        let report = tracker.record_window("api", make_sample(150.0, 0.001, 100));
        assert!(!report.compliant);
        assert_eq!(tracker.consecutive_violations("api"), 1);
    }

    #[test]
    fn error_rate_violation_detected() {
        let mut tracker = SlaViolationTracker::new(10, 3);
        let report = tracker.record_window("api", make_sample(50.0, 0.05, 100));
        assert!(!report.compliant);
        assert_eq!(tracker.consecutive_violations("api"), 1);
    }

    #[test]
    fn insufficient_samples_flagged() {
        let mut tracker = SlaViolationTracker::new(100, 3);
        let report = tracker.record_window("api", make_sample(50.0, 0.001, 50));
        assert!(report.insufficient_samples);
        assert!(report.compliant);
    }

    #[test]
    fn consecutive_violations_alert() {
        let mut tracker = SlaViolationTracker::new(10, 3);
        tracker.record_window("api", make_sample(150.0, 0.001, 100));
        tracker.record_window("api", make_sample(160.0, 0.001, 100));
        tracker.record_window("api", make_sample(170.0, 0.001, 100));
        assert!(!tracker.alerts().is_empty());
        assert_eq!(tracker.alerts()[0].code, "GOV_SLA_VIOLATED");
        assert_eq!(tracker.alerts()[0].consecutive_violations, 3);
    }

    #[test]
    fn compliant_window_resets_violation_count() {
        let mut tracker = SlaViolationTracker::new(10, 3);
        tracker.record_window("api", make_sample(150.0, 0.001, 100));
        tracker.record_window("api", make_sample(50.0, 0.001, 100));
        assert_eq!(tracker.consecutive_violations("api"), 0);
    }

    #[test]
    fn achievement_rate_correct() {
        let mut tracker = SlaViolationTracker::new(10, 3);
        tracker.record_window("api", make_sample(50.0, 0.001, 100));
        tracker.record_window("api", make_sample(150.0, 0.001, 100));
        tracker.record_window("api", make_sample(50.0, 0.001, 100));
        let rate = tracker.achievement_rate("api");
        assert!((rate - 2.0 / 3.0).abs() < 0.001);
    }

    #[test]
    fn multiple_interfaces_tracked_independently() {
        let mut tracker = SlaViolationTracker::new(10, 2);
        tracker.record_window("api_a", make_sample(150.0, 0.001, 100));
        tracker.record_window("api_b", make_sample(50.0, 0.001, 100));
        assert_eq!(tracker.consecutive_violations("api_a"), 1);
        assert_eq!(tracker.consecutive_violations("api_b"), 0);
    }

    #[test]
    fn alert_contains_violated_metric() {
        let mut tracker = SlaViolationTracker::new(10, 1);
        tracker.record_window("api", make_sample(200.0, 0.001, 100));
        assert!(!tracker.alerts().is_empty());
        assert!(tracker.alerts()[0]
            .violated_metric
            .contains("p99_latency_ms"));
    }

    #[test]
    fn error_rate_alert_contains_metric() {
        let mut tracker = SlaViolationTracker::new(10, 1);
        tracker.record_window("api", make_sample(50.0, 0.5, 100));
        assert!(!tracker.alerts().is_empty());
        assert!(tracker.alerts()[0].violated_metric.contains("error_rate"));
    }

    #[test]
    fn reports_stored() {
        let mut tracker = SlaViolationTracker::new(10, 3);
        tracker.record_window("api", make_sample(50.0, 0.001, 100));
        tracker.record_window("api", make_sample(150.0, 0.001, 100));
        assert_eq!(tracker.reports().len(), 2);
    }
}
