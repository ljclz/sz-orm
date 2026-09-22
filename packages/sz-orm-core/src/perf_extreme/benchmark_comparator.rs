//! v8.0.0 任务 2.5：性能基准对标器
//!
//! 执行主流 ORM 基准套件 → 采集 sz-orm 指标 → 对比 → 退化告警。
//! `BenchmarkReport` 含 `framework_comparisons`/`qps_ratio`/`latency_comparison`。
//! 基准数据由外部套件产出，本组件消费对比。

use std::collections::HashMap;

/// 基准对标配置
#[derive(Debug, Clone)]
pub struct BenchmarkConfig {
    /// QPS 达标率阈值（0.0 ~ 1.0，相对 max(其他框架)）
    pub qps_threshold: f64,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            qps_threshold: 0.95,
        }
    }
}

/// 基准框架指标
#[derive(Debug, Clone)]
pub struct BenchmarkFramework {
    /// 框架名称（Diesel/SQLx/SeaORM/sz-orm 等）
    pub name: String,
    /// QPS（queries per second）
    pub qps: f64,
    /// 平均延迟（微秒）
    pub avg_latency_us: f64,
    /// P99 延迟（微秒）
    pub p99_latency_us: f64,
}

/// 单框架对比结果
#[derive(Debug, Clone)]
pub struct FrameworkComparison {
    /// 框架名称
    pub name: String,
    /// sz-orm QPS / 该框架 QPS
    pub qps_ratio: f64,
    /// sz-orm 延迟 / 该框架延迟
    pub latency_ratio: f64,
    /// 是否达标（QPS 比值 ≥ 阈值）
    pub meets_threshold: bool,
}

/// 基准对比报告
#[derive(Debug, Clone)]
pub struct BenchmarkReport {
    /// 各框架对比
    pub framework_comparisons: Vec<FrameworkComparison>,
    /// sz-orm QPS / max(其他框架) QPS
    pub qps_ratio: f64,
    /// 延迟对比（sz-orm vs 各框架平均）
    pub latency_comparison: HashMap<String, f64>,
    /// 基准套件是否可用
    pub benchmark_available: bool,
    /// sz-orm 自身指标
    pub sz_orm_metrics: Option<BenchmarkFramework>,
}

/// 性能基准对标器
pub struct PerfBenchmarkComparator {
    config: BenchmarkConfig,
}

impl PerfBenchmarkComparator {
    /// 创建基准对标器
    pub fn new(config: BenchmarkConfig) -> Self {
        Self { config }
    }

    /// 默认配置构造
    pub fn with_default() -> Self {
        Self::new(BenchmarkConfig::default())
    }

    /// 获取配置引用
    pub fn config(&self) -> &BenchmarkConfig {
        &self.config
    }

    /// 对比 sz-orm 与其他框架的基准指标
    ///
    /// `frameworks` 包含 sz-orm 与其他框架的指标。
    /// QPS ≥ `qps_threshold` × max(其他框架) 视为达标。
    /// 基准套件不可用（frameworks 为空或无 sz-orm）→ 标记 `BENCHMARK_UNAVAILABLE`。
    pub fn compare(&self, frameworks: &[BenchmarkFramework]) -> BenchmarkReport {
        if frameworks.is_empty() {
            return BenchmarkReport {
                framework_comparisons: Vec::new(),
                qps_ratio: 0.0,
                latency_comparison: HashMap::new(),
                benchmark_available: false,
                sz_orm_metrics: None,
            };
        }

        let sz_orm = frameworks.iter().find(|f| f.name == "sz-orm");
        let Some(sz) = sz_orm else {
            return BenchmarkReport {
                framework_comparisons: Vec::new(),
                qps_ratio: 0.0,
                latency_comparison: HashMap::new(),
                benchmark_available: false,
                sz_orm_metrics: None,
            };
        };

        let others: Vec<&BenchmarkFramework> =
            frameworks.iter().filter(|f| f.name != "sz-orm").collect();

        if others.is_empty() {
            return BenchmarkReport {
                framework_comparisons: Vec::new(),
                qps_ratio: 1.0,
                latency_comparison: HashMap::new(),
                benchmark_available: true,
                sz_orm_metrics: Some(sz.clone()),
            };
        }

        let max_other_qps = others.iter().map(|f| f.qps).fold(0.0f64, f64::max);
        let qps_ratio = if max_other_qps > 0.0 {
            sz.qps / max_other_qps
        } else {
            0.0
        };

        let mut comparisons = Vec::with_capacity(others.len());
        let mut latency_cmp = HashMap::new();
        for other in &others {
            let ratio = if other.qps > 0.0 {
                sz.qps / other.qps
            } else {
                0.0
            };
            let lat_ratio = if other.avg_latency_us > 0.0 {
                sz.avg_latency_us / other.avg_latency_us
            } else {
                0.0
            };
            let meets = ratio >= self.config.qps_threshold;
            comparisons.push(FrameworkComparison {
                name: other.name.clone(),
                qps_ratio: ratio,
                latency_ratio: lat_ratio,
                meets_threshold: meets,
            });
            latency_cmp.insert(other.name.clone(), lat_ratio);
        }

        BenchmarkReport {
            framework_comparisons: comparisons,
            qps_ratio,
            latency_comparison: latency_cmp,
            benchmark_available: true,
            sz_orm_metrics: Some(sz.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn framework(name: &str, qps: f64, lat: f64) -> BenchmarkFramework {
        BenchmarkFramework {
            name: name.to_string(),
            qps,
            avg_latency_us: lat,
            p99_latency_us: lat * 2.0,
        }
    }

    #[test]
    fn compare_empty_frameworks_unavailable() {
        let cmp = PerfBenchmarkComparator::with_default();
        let report = cmp.compare(&[]);
        assert!(!report.benchmark_available);
    }

    #[test]
    fn compare_no_sz_orm_unavailable() {
        let cmp = PerfBenchmarkComparator::with_default();
        let report = cmp.compare(&[framework("Diesel", 1000.0, 10.0)]);
        assert!(!report.benchmark_available);
    }

    #[test]
    fn compare_sz_orm_meets_threshold() {
        let cmp = PerfBenchmarkComparator::with_default();
        let frameworks = [
            framework("sz-orm", 980.0, 10.0),
            framework("Diesel", 1000.0, 10.0),
            framework("SQLx", 950.0, 12.0),
        ];
        let report = cmp.compare(&frameworks);
        assert!(report.benchmark_available);
        // 980 / 1000 = 0.98 ≥ 0.95
        assert!(report.qps_ratio >= 0.95);
        assert!(report
            .framework_comparisons
            .iter()
            .all(|c| c.meets_threshold));
    }

    #[test]
    fn compare_sz_orm_below_threshold() {
        let cmp = PerfBenchmarkComparator::with_default();
        let frameworks = [
            framework("sz-orm", 500.0, 20.0),
            framework("Diesel", 1000.0, 10.0),
        ];
        let report = cmp.compare(&frameworks);
        assert!(report.benchmark_available);
        assert!(report.qps_ratio < 0.95);
        assert!(!report.framework_comparisons[0].meets_threshold);
    }

    #[test]
    fn compare_only_sz_orm() {
        let cmp = PerfBenchmarkComparator::with_default();
        let report = cmp.compare(&[framework("sz-orm", 1000.0, 10.0)]);
        assert!(report.benchmark_available);
        assert_eq!(report.qps_ratio, 1.0);
    }
}
