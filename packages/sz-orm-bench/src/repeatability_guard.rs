//! 基准可重复性守卫（v8.1.0 任务 2.2，feature gate: `bench-real-db`）
//!
//! 多次基准结果（≥ 5 次）→ 计算 QPS 偏差 → 偏差 ≤ 5% 标记稳定 → 偏差 > 5% 告警 `BENCH_UNSTABLE`。

use serde::{Deserialize, Serialize};

use crate::{BenchError, BenchResult};

/// 可重复性报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepeatabilityReport {
    /// 是否稳定（偏差 ≤ 阈值）
    pub stable: bool,
    /// QPS 偏差（最大偏差百分比，0.0 = 完全一致）
    pub deviation_pct: f64,
    /// 样本数
    pub sample_count: usize,
    /// 告警码（不稳定时为 `BENCH_UNSTABLE`，稳定时为空）
    pub alert_code: String,
    /// 排查建议（不稳定时附建议）
    pub suggestion: String,
    /// 各次基准的 QPS 值
    pub qps_samples: Vec<f64>,
}

/// 基准可重复性守卫
#[derive(Debug, Clone)]
pub struct BenchRepeatabilityGuard {
    /// 偏差阈值（默认 0.05 = 5%）
    deviation_threshold: f64,
    /// 最小样本数（默认 5）
    min_samples: usize,
}

impl Default for BenchRepeatabilityGuard {
    fn default() -> Self {
        Self::new(0.05, 5)
    }
}

impl BenchRepeatabilityGuard {
    /// 创建守卫：指定偏差阈值和最小样本数
    pub fn new(deviation_threshold: f64, min_samples: usize) -> Self {
        Self {
            deviation_threshold,
            min_samples,
        }
    }

    /// 使用默认配置（5% 偏差，5 次样本）
    pub fn with_defaults() -> Self {
        Self::default()
    }

    /// 校验多次基准结果的 QPS 偏差
    pub fn check_deviation(
        &self,
        results: &[BenchResult],
    ) -> Result<RepeatabilityReport, BenchError> {
        if results.len() < self.min_samples {
            return Err(BenchError::InsufficientSamples {
                required: self.min_samples,
                actual: results.len(),
            });
        }
        let qps_samples: Vec<f64> = results.iter().map(|r| r.throughput_ops).collect();
        let deviation_pct = compute_max_deviation_pct(&qps_samples);
        let stable = deviation_pct <= self.deviation_threshold * 100.0;
        let (alert_code, suggestion) = if stable {
            (String::new(), String::new())
        } else {
            (
                "BENCH_UNSTABLE".to_string(),
                format!(
                    "QPS 偏差 {deviation_pct:.2}% 超过阈值 {:.2}%。\
                     建议：1) 检查 DB 负载波动；2) 增加预热轮数；\
                     3) 确认连接池大小；4) 排查 GC/系统抖动",
                    self.deviation_threshold * 100.0
                ),
            )
        };
        Ok(RepeatabilityReport {
            stable,
            deviation_pct,
            sample_count: results.len(),
            alert_code,
            suggestion,
            qps_samples,
        })
    }
}

/// 计算最大偏差百分比：(max - min) / max * 100
fn compute_max_deviation_pct(samples: &[f64]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let max = samples.iter().cloned().fold(0.0f64, f64::max);
    let min = samples.iter().cloned().fold(f64::MAX, f64::min);
    if max > 0.0 {
        ((max - min) / max) * 100.0
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BenchResult, FrameworkType, WorkloadType};

    fn make_result(qps: f64) -> BenchResult {
        let latencies = vec![100u64, 200, 300];
        let mut r = BenchResult::from_latencies(
            FrameworkType::SzOrm,
            WorkloadType::SingleRowQuery,
            latencies,
            0,
            0,
            0,
            true,
        );
        r.throughput_ops = qps;
        r
    }

    fn make_results(qps_values: &[f64]) -> Vec<BenchResult> {
        qps_values.iter().map(|&q| make_result(q)).collect()
    }

    #[test]
    fn test_stable_within_threshold() {
        let guard = BenchRepeatabilityGuard::with_defaults();
        let results = make_results(&[10000.0, 10100.0, 10050.0, 9980.0, 10020.0]);
        let report = guard.check_deviation(&results).unwrap();
        assert!(report.stable);
        assert!(report.alert_code.is_empty());
        assert_eq!(report.sample_count, 5);
        assert!(report.deviation_pct <= 5.0);
    }

    #[test]
    fn test_unstable_exceeds_threshold() {
        let guard = BenchRepeatabilityGuard::with_defaults();
        let results = make_results(&[10000.0, 8000.0, 9500.0, 11000.0, 9000.0]);
        let report = guard.check_deviation(&results).unwrap();
        assert!(!report.stable);
        assert_eq!(report.alert_code, "BENCH_UNSTABLE");
        assert!(report.deviation_pct > 5.0);
        assert!(!report.suggestion.is_empty());
    }

    #[test]
    fn test_insufficient_samples() {
        let guard = BenchRepeatabilityGuard::with_defaults();
        let results = make_results(&[10000.0, 10100.0, 10050.0]);
        let err = guard.check_deviation(&results).unwrap_err();
        assert!(matches!(
            err,
            BenchError::InsufficientSamples {
                required: 5,
                actual: 3
            }
        ));
    }

    #[test]
    fn test_exact_five_samples_stable() {
        let guard = BenchRepeatabilityGuard::with_defaults();
        let results = make_results(&[10000.0, 10000.0, 10000.0, 10000.0, 10000.0]);
        let report = guard.check_deviation(&results).unwrap();
        assert!(report.stable);
        assert_eq!(report.deviation_pct, 0.0);
    }

    #[test]
    fn test_custom_threshold() {
        let guard = BenchRepeatabilityGuard::new(0.10, 5);
        // max=10200, min=9500, deviation=6.86% ≤ 10%
        let results = make_results(&[10000.0, 9500.0, 9800.0, 10200.0, 9900.0]);
        let report = guard.check_deviation(&results).unwrap();
        assert!(report.stable);
        assert!(report.deviation_pct <= 10.0);
    }

    #[test]
    fn test_custom_min_samples() {
        let guard = BenchRepeatabilityGuard::new(0.05, 3);
        let results = make_results(&[10000.0, 10100.0, 10050.0]);
        let report = guard.check_deviation(&results).unwrap();
        assert!(report.stable);
        assert_eq!(report.sample_count, 3);
    }

    #[test]
    fn test_qps_samples_recorded() {
        let guard = BenchRepeatabilityGuard::with_defaults();
        let qps = vec![10000.0, 10100.0, 10050.0, 9980.0, 10020.0];
        let results = make_results(&qps);
        let report = guard.check_deviation(&results).unwrap();
        assert_eq!(report.qps_samples, qps);
    }

    #[test]
    fn test_zero_qps_no_panic() {
        let guard = BenchRepeatabilityGuard::with_defaults();
        let results = make_results(&[0.0, 0.0, 0.0, 0.0, 0.0]);
        let report = guard.check_deviation(&results).unwrap();
        assert!(report.stable);
        assert_eq!(report.deviation_pct, 0.0);
    }
}
