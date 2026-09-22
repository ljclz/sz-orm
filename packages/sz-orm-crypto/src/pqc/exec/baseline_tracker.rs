//! 性能基准追踪器
//!
//! 采集迁移后性能指标 → 与经典密码学基准对比 → 退化超2倍告警 PQC_PERFORMANCE_REGRESSION。

use std::collections::HashMap;
use std::sync::Mutex;

use super::PqcExecError;

/// 退化告警阈值（迁移后耗时 / 基准耗时 > 2.0）
const REGRESSION_THRESHOLD: f64 = 2.0;

/// 告警标签
pub const REGRESSION_ALERT_TAG: &str = "PQC_PERFORMANCE_REGRESSION";

/// 单场景性能指标
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SceneMetrics {
    pub scene_name: String,
    pub latency_micros: u64,
    pub throughput_ops: u64,
    pub memory_kib: u64,
}

/// 性能对比条目
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PerformanceComparison {
    pub scene_name: String,
    pub baseline_latency_micros: u64,
    pub observed_latency_micros: u64,
    pub ratio: f64,
    pub regressed: bool,
}

/// 性能报告
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PerformanceReport {
    pub comparisons: Vec<PerformanceComparison>,
    pub regression_count: usize,
    pub alerts: Vec<String>,
}

/// 性能基准追踪器
pub struct PerformanceBaselineTracker {
    baselines: Mutex<HashMap<String, SceneMetrics>>,
    observations: Mutex<HashMap<String, SceneMetrics>>,
}

impl PerformanceBaselineTracker {
    pub fn new() -> Self {
        Self {
            baselines: Mutex::new(HashMap::new()),
            observations: Mutex::new(HashMap::new()),
        }
    }

    /// 注册经典密码学基准指标
    pub fn set_baseline(&self, metrics: SceneMetrics) {
        let mut baselines = self.baselines.lock().unwrap();
        baselines.insert(metrics.scene_name.clone(), metrics);
    }

    /// 采集迁移后性能指标
    ///
    /// 若该场景已有基准，立即计算比值；若退化超阈值返回告警。
    pub fn track(&self, metrics: SceneMetrics) -> Result<Option<String>, PqcExecError> {
        let scene_name = metrics.scene_name.clone();
        let baselines = self.baselines.lock().unwrap();
        let baseline = baselines.get(&scene_name).ok_or_else(|| {
            PqcExecError::BaselineMissing(format!("场景 {} 缺少基准指标", scene_name))
        })?;

        if baseline.latency_micros == 0 {
            return Err(PqcExecError::BaselineMissing(format!(
                "场景 {} 基准 latency 为 0，无法计算比值",
                scene_name
            )));
        }

        let ratio = metrics.latency_micros as f64 / baseline.latency_micros as f64;
        let alert = if ratio > REGRESSION_THRESHOLD {
            Some(format!(
                "{}: scene={} baseline={}us observed={}us ratio={:.3}",
                REGRESSION_ALERT_TAG,
                scene_name,
                baseline.latency_micros,
                metrics.latency_micros,
                ratio
            ))
        } else {
            None
        };

        let mut observations = self.observations.lock().unwrap();
        observations.insert(scene_name, metrics);

        Ok(alert)
    }

    /// 生成完整性能报告
    pub fn report(&self) -> Result<PerformanceReport, PqcExecError> {
        let baselines = self.baselines.lock().unwrap();
        let observations = self.observations.lock().unwrap();

        let mut comparisons = Vec::new();
        let mut alerts = Vec::new();

        for (name, observed) in observations.iter() {
            let baseline = baselines.get(name).ok_or_else(|| {
                PqcExecError::BaselineMissing(format!("场景 {} 缺少基准指标", name))
            })?;
            if baseline.latency_micros == 0 {
                return Err(PqcExecError::BaselineMissing(format!(
                    "场景 {} 基准 latency 为 0",
                    name
                )));
            }
            let ratio = observed.latency_micros as f64 / baseline.latency_micros as f64;
            let regressed = ratio > REGRESSION_THRESHOLD;
            if regressed {
                alerts.push(format!(
                    "{}: scene={} baseline={}us observed={}us ratio={:.3}",
                    REGRESSION_ALERT_TAG,
                    name,
                    baseline.latency_micros,
                    observed.latency_micros,
                    ratio
                ));
            }
            comparisons.push(PerformanceComparison {
                scene_name: name.clone(),
                baseline_latency_micros: baseline.latency_micros,
                observed_latency_micros: observed.latency_micros,
                ratio,
                regressed,
            });
        }

        comparisons.sort_by(|a, b| a.scene_name.cmp(&b.scene_name));
        let regression_count = comparisons.iter().filter(|c| c.regressed).count();

        Ok(PerformanceReport {
            comparisons,
            regression_count,
            alerts,
        })
    }

    /// 已采集观测值数量
    pub fn observation_count(&self) -> usize {
        self.observations.lock().unwrap().len()
    }
}

impl Default for PerformanceBaselineTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_metrics(name: &str, latency: u64) -> SceneMetrics {
        SceneMetrics {
            scene_name: name.to_string(),
            latency_micros: latency,
            throughput_ops: 1000,
            memory_kib: 512,
        }
    }

    #[test]
    fn test_normal_comparison_no_alert() {
        let tracker = PerformanceBaselineTracker::new();
        tracker.set_baseline(make_metrics("tls", 1000));
        let alert = tracker.track(make_metrics("tls", 1500)).unwrap();
        assert!(alert.is_none());
        let report = tracker.report().unwrap();
        assert_eq!(report.regression_count, 0);
        assert!(report.alerts.is_empty());
        assert!(!report.comparisons[0].regressed);
    }

    #[test]
    fn test_regression_alert() {
        let tracker = PerformanceBaselineTracker::new();
        tracker.set_baseline(make_metrics("signing", 500));
        let alert = tracker.track(make_metrics("signing", 1500)).unwrap();
        assert!(alert.is_some());
        let alert_text = alert.unwrap();
        assert!(alert_text.starts_with(REGRESSION_ALERT_TAG));
        assert!(alert_text.contains("signing"));
        let report = tracker.report().unwrap();
        assert_eq!(report.regression_count, 1);
        assert_eq!(report.alerts.len(), 1);
        assert!(report.comparisons[0].regressed);
        assert!(report.comparisons[0].ratio > REGRESSION_THRESHOLD);
    }

    #[test]
    fn test_baseline_missing_error() {
        let tracker = PerformanceBaselineTracker::new();
        let err = tracker.track(make_metrics("unknown", 1000)).unwrap_err();
        assert!(matches!(err, PqcExecError::BaselineMissing(_)));
    }

    #[test]
    fn test_zero_baseline_error() {
        let tracker = PerformanceBaselineTracker::new();
        tracker.set_baseline(make_metrics("zero", 0));
        let err = tracker.track(make_metrics("zero", 100)).unwrap_err();
        assert!(matches!(err, PqcExecError::BaselineMissing(_)));
    }

    #[test]
    fn test_multiple_scenes_mixed() {
        let tracker = PerformanceBaselineTracker::new();
        tracker.set_baseline(make_metrics("a", 1000));
        tracker.set_baseline(make_metrics("b", 2000));
        tracker.set_baseline(make_metrics("c", 500));
        tracker.track(make_metrics("a", 1200)).unwrap();
        tracker.track(make_metrics("b", 5000)).unwrap();
        tracker.track(make_metrics("c", 600)).unwrap();
        let report = tracker.report().unwrap();
        assert_eq!(report.regression_count, 1);
        assert_eq!(report.alerts.len(), 1);
        let regressed = report.comparisons.iter().find(|c| c.regressed).unwrap();
        assert_eq!(regressed.scene_name, "b");
    }

    #[test]
    fn test_threshold_boundary() {
        let tracker = PerformanceBaselineTracker::new();
        tracker.set_baseline(make_metrics("boundary", 1000));
        let alert = tracker.track(make_metrics("boundary", 2000)).unwrap();
        assert!(alert.is_none());
        let alert2 = tracker.track(make_metrics("boundary", 2001)).unwrap();
        assert!(alert2.is_some());
    }
}
