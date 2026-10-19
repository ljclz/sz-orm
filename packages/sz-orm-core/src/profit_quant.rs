//! 联动收益量化配置与报告（v10.2.0）
//!
//! A/B 对比基准：联动模式 vs 独立模式，自动生成收益报告。

use std::time::Instant;

use serde::{Deserialize, Serialize};

/// 联动收益量化配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfitQuantConfig {
    /// 是否启用收益量化，默认 false
    pub profit_quant_enabled: bool,
    /// 吞吐提升最低阈值百分比，默认 5.0
    pub min_throughput_uplift_pct: f64,
    /// 延迟降低最低阈值百分比，默认 10.0
    pub min_latency_reduction_pct: f64,
    /// 基准工作负载描述
    pub benchmark_workload: String,
}

impl Default for ProfitQuantConfig {
    fn default() -> Self {
        Self {
            profit_quant_enabled: false,
            min_throughput_uplift_pct: 5.0,
            min_latency_reduction_pct: 10.0,
            benchmark_workload: String::new(),
        }
    }
}

/// 工作负载采集指标（由调用方闭包返回）
#[derive(Debug, Clone)]
pub struct WorkloadMetrics {
    pub throughput: f64,
    pub latency_p99: f64,
    pub cache_hit_zero_copy_ratio: f64,
}

/// 联动收益报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfitReport {
    pub linked_throughput: f64,
    pub independent_throughput: f64,
    pub linked_latency_p99: f64,
    pub independent_latency_p99: f64,
    pub cache_hit_zero_copy_ratio: f64,
    pub throughput_uplift_pct: f64,
    pub latency_reduction_pct: f64,
    pub collection_time: u64,
    pub workload_description: String,
}

/// 收益告警
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProfitAlert {
    ThroughputBelowThreshold { actual: f64, threshold: f64 },
    LatencyBelowThreshold { actual: f64, threshold: f64 },
}

/// 收益量化错误
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProfitError {
    EmptyWorkload,
    IndependentModeFailed,
    BenchmarkFailed(String),
}

/// 收益量化引擎
pub struct ProfitQuantEngine {
    config: ProfitQuantConfig,
}

impl ProfitQuantEngine {
    pub fn new(config: ProfitQuantConfig) -> Self {
        Self { config }
    }

    /// 运行 A/B 对比基准
    ///
    /// `linked_fn` 执行联动模式工作负载，`independent_fn` 执行独立模式工作负载。
    /// 提升百分比由实际采集值计算，禁止手写。
    pub async fn run_benchmark<FL, FIL, FutL, FutI>(
        &self,
        workload: &str,
        linked_fn: FL,
        independent_fn: FIL,
    ) -> Result<ProfitReport, ProfitError>
    where
        FL: Fn(&str) -> FutL,
        FIL: Fn(&str) -> FutI,
        FutL: std::future::Future<Output = Result<WorkloadMetrics, ProfitError>>,
        FutI: std::future::Future<Output = Result<WorkloadMetrics, ProfitError>>,
    {
        if workload.is_empty() {
            return Err(ProfitError::EmptyWorkload);
        }
        let start = Instant::now();
        let linked = linked_fn(workload).await?;
        let independent = independent_fn(workload)
            .await
            .map_err(|_| ProfitError::IndependentModeFailed)?;
        if independent.throughput <= 0.0 || independent.latency_p99 <= 0.0 {
            return Err(ProfitError::IndependentModeFailed);
        }
        let throughput_uplift_pct =
            (linked.throughput - independent.throughput) / independent.throughput * 100.0;
        let latency_reduction_pct =
            (independent.latency_p99 - linked.latency_p99) / independent.latency_p99 * 100.0;
        Ok(ProfitReport {
            linked_throughput: linked.throughput,
            independent_throughput: independent.throughput,
            linked_latency_p99: linked.latency_p99,
            independent_latency_p99: independent.latency_p99,
            cache_hit_zero_copy_ratio: linked.cache_hit_zero_copy_ratio,
            throughput_uplift_pct,
            latency_reduction_pct,
            collection_time: start.elapsed().as_millis() as u64,
            workload_description: workload.to_string(),
        })
    }

    /// 阈值告警检查
    pub fn check_threshold(&self, report: &ProfitReport) -> Option<ProfitAlert> {
        if report.throughput_uplift_pct < self.config.min_throughput_uplift_pct {
            return Some(ProfitAlert::ThroughputBelowThreshold {
                actual: report.throughput_uplift_pct,
                threshold: self.config.min_throughput_uplift_pct,
            });
        }
        if report.latency_reduction_pct < self.config.min_latency_reduction_pct {
            return Some(ProfitAlert::LatencyBelowThreshold {
                actual: report.latency_reduction_pct,
                threshold: self.config.min_latency_reduction_pct,
            });
        }
        None
    }

    /// 获取配置引用
    pub fn config(&self) -> &ProfitQuantConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profit_quant_config_default_disabled() {
        let config = ProfitQuantConfig::default();
        assert!(!config.profit_quant_enabled);
        assert!(
            (config.min_throughput_uplift_pct - 5.0).abs() < 1e-9,
            "default threshold 5.0"
        );
        let enabled = ProfitQuantConfig {
            profit_quant_enabled: true,
            ..Default::default()
        };
        assert!(enabled.profit_quant_enabled, "enabled config");
    }

    #[tokio::test]
    async fn test_profit_report_uplift_calculation() {
        let config = ProfitQuantConfig {
            profit_quant_enabled: true,
            ..Default::default()
        };
        let engine = ProfitQuantEngine::new(config);
        let linked_fn = |_w: &str| async {
            Ok(WorkloadMetrics {
                throughput: 120.0,
                latency_p99: 80.0,
                cache_hit_zero_copy_ratio: 0.9,
            })
        };
        let independent_fn = |_w: &str| async {
            Ok(WorkloadMetrics {
                throughput: 100.0,
                latency_p99: 100.0,
                cache_hit_zero_copy_ratio: 0.5,
            })
        };
        let report = engine
            .run_benchmark("test_workload", linked_fn, independent_fn)
            .await
            .unwrap();
        assert!(
            (report.throughput_uplift_pct - 20.0).abs() < 1e-6,
            "throughput uplift 20%"
        );
        assert!(
            (report.latency_reduction_pct - 20.0).abs() < 1e-6,
            "latency reduction 20%"
        );
        assert!((report.linked_throughput - 120.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn test_profit_quant_threshold_alert() {
        let config = ProfitQuantConfig {
            profit_quant_enabled: true,
            min_throughput_uplift_pct: 30.0,
            ..Default::default()
        };
        let engine = ProfitQuantEngine::new(config);
        let report = ProfitReport {
            linked_throughput: 110.0,
            independent_throughput: 100.0,
            linked_latency_p99: 90.0,
            independent_latency_p99: 100.0,
            cache_hit_zero_copy_ratio: 0.8,
            throughput_uplift_pct: 10.0,
            latency_reduction_pct: 10.0,
            collection_time: 100,
            workload_description: "test".to_string(),
        };
        let alert = engine.check_threshold(&report);
        assert!(matches!(
            alert,
            Some(ProfitAlert::ThroughputBelowThreshold { actual: 10.0, .. })
        ));
    }

    #[tokio::test]
    async fn test_profit_quant_empty_workload_rejected() {
        let engine = ProfitQuantEngine::new(ProfitQuantConfig::default());
        let linked_fn = |_w: &str| async {
            Ok(WorkloadMetrics {
                throughput: 100.0,
                latency_p99: 100.0,
                cache_hit_zero_copy_ratio: 0.5,
            })
        };
        let result = engine.run_benchmark("", linked_fn, linked_fn).await;
        assert!(matches!(result, Err(ProfitError::EmptyWorkload)));
    }

    #[tokio::test]
    async fn test_profit_quant_independent_failure() {
        let engine = ProfitQuantEngine::new(ProfitQuantConfig::default());
        let linked_fn = |_w: &str| async {
            Ok(WorkloadMetrics {
                throughput: 120.0,
                latency_p99: 80.0,
                cache_hit_zero_copy_ratio: 0.9,
            })
        };
        let independent_fn =
            |_w: &str| async { Err(ProfitError::BenchmarkFailed("db down".to_string())) };
        let result = engine
            .run_benchmark("test", linked_fn, independent_fn)
            .await;
        assert!(matches!(result, Err(ProfitError::IndependentModeFailed)));
    }
}
