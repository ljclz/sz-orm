//! 回归基线持久化与对比（v7.4.0 新增）
//!
//! 提供 `RegressionBaseline` 结构，支持将基准结果保存为 JSON 文件，
//! 并与既有基线对比，P95 退化 ≥10% 时输出告警。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{BenchConfig, BenchResult, WorkloadType};

/// 回归基线
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionBaseline {
    /// 基线名称
    pub baseline_name: String,
    /// 创建时间戳（Unix 毫秒）
    pub created_at: u64,
    /// 基线配置快照
    pub config: BenchConfig,
    /// 基线结果列表
    pub results: Vec<BenchResult>,
    /// 数据库版本信息
    pub db_version: String,
}

/// 基线对比结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineComparison {
    /// 基线名称
    pub baseline_name: String,
    /// 逐工作负载对比结果
    pub workload_comparisons: Vec<WorkloadComparison>,
    /// 是否有退化告警
    pub has_regression: bool,
}

/// 单个工作负载对比
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkloadComparison {
    /// 工作负载类型
    pub workload: WorkloadType,
    /// 基线 P95（μs）
    pub baseline_p95_us: f64,
    /// 当前 P95（μs）
    pub current_p95_us: f64,
    /// 退化百分比（正值=退化，负值=改善）
    pub degradation_pct: f64,
    /// 是否退化 ≥10%
    pub is_regression: bool,
}

impl RegressionBaseline {
    /// 创建新基线
    pub fn new(name: &str, config: BenchConfig, results: Vec<BenchResult>, db_version: &str) -> Self {
        Self {
            baseline_name: name.to_string(),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            config,
            results,
            db_version: db_version.to_string(),
        }
    }

    /// 保存基线到 JSON 文件
    ///
    /// 路径格式：`bench-results/baseline_<name>.json`
    pub fn save_to_file(&self, dir: &str) -> Result<PathBuf, std::io::Error> {
        let dir_path = PathBuf::from(dir);
        std::fs::create_dir_all(&dir_path)?;
        let file_path = dir_path.join(format!("baseline_{}.json", self.baseline_name));
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(&file_path, json)?;
        Ok(file_path)
    }

    /// 从 JSON 文件加载基线
    pub fn load_from_file(path: &str) -> Result<Self, std::io::Error> {
        let content = std::fs::read_to_string(path)?;
        serde_json::from_str(&content).map_err(std::io::Error::other)
    }

    /// 对比当前结果与基线，P95 退化 ≥10% 输出告警
    pub fn compare(&self, current_results: &[BenchResult]) -> BaselineComparison {
        let mut comparisons = Vec::new();
        let mut has_regression = false;

        for baseline_result in &self.results {
            if let Some(current) = current_results
                .iter()
                .find(|r| r.workload == baseline_result.workload && r.framework == baseline_result.framework)
            {
                let baseline_p95 = baseline_result.p95_us;
                let current_p95 = current.p95_us;
                let degradation_pct = if baseline_p95 > 0.0 {
                    ((current_p95 - baseline_p95) / baseline_p95) * 100.0
                } else {
                    0.0
                };
                let is_regression = degradation_pct >= 10.0;
                if is_regression {
                    has_regression = true;
                }
                comparisons.push(WorkloadComparison {
                    workload: baseline_result.workload,
                    baseline_p95_us: baseline_p95,
                    current_p95_us: current_p95,
                    degradation_pct,
                    is_regression,
                });
            }
        }

        BaselineComparison {
            baseline_name: self.baseline_name.clone(),
            workload_comparisons: comparisons,
            has_regression,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BenchConfig, BenchResult, FrameworkType, WorkloadType};

    fn make_test_result(workload: WorkloadType, p95: f64) -> BenchResult {
        let latencies = vec![100u64, (p95 as u64), (p95 as u64 * 2)];
        BenchResult::from_latencies_ext(
            FrameworkType::SzOrm,
            workload,
            latencies,
            1024,
            10,
            1024,
            true,
            crate::DbBackend::Sqlite,
            None,
            1000,
        )
    }

    #[test]
    fn test_baseline_save_load_roundtrip() {
        let temp_dir = std::env::temp_dir().join("sz_orm_bench_test_baseline");
        let config = BenchConfig::new();
        let results = vec![make_test_result(WorkloadType::SingleRowQuery, 200.0)];
        let baseline = RegressionBaseline::new("test_roundtrip", config, results, "sqlite 3.45");

        let saved_path = baseline.save_to_file(temp_dir.to_str().unwrap()).unwrap();
        let loaded = RegressionBaseline::load_from_file(saved_path.to_str().unwrap()).unwrap();

        assert_eq!(loaded.baseline_name, "test_roundtrip");
        assert_eq!(loaded.results.len(), 1);
        assert_eq!(loaded.db_version, "sqlite 3.45");

        let _ = std::fs::remove_file(&saved_path);
        let _ = std::fs::remove_dir(&temp_dir);
    }

    #[test]
    fn test_baseline_compare_no_regression() {
        let config = BenchConfig::new();
        let baseline_results = vec![make_test_result(WorkloadType::SingleRowQuery, 200.0)];
        let baseline = RegressionBaseline::new("test_no_reg", config, baseline_results, "sqlite");

        let current_results = vec![make_test_result(WorkloadType::SingleRowQuery, 210.0)];
        let comparison = baseline.compare(&current_results);

        assert!(!comparison.has_regression);
        assert_eq!(comparison.workload_comparisons.len(), 1);
        let wc = &comparison.workload_comparisons[0];
        assert!(!wc.is_regression);
        assert!((wc.degradation_pct - 5.0).abs() < 0.01);
    }

    #[test]
    fn test_baseline_compare_with_regression() {
        let config = BenchConfig::new();
        let baseline_results = vec![make_test_result(WorkloadType::SingleRowQuery, 200.0)];
        let baseline = RegressionBaseline::new("test_reg", config, baseline_results, "sqlite");

        let current_results = vec![make_test_result(WorkloadType::SingleRowQuery, 250.0)];
        let comparison = baseline.compare(&current_results);

        assert!(comparison.has_regression);
        let wc = &comparison.workload_comparisons[0];
        assert!(wc.is_regression);
        assert!((wc.degradation_pct - 25.0).abs() < 0.01);
    }

    #[test]
    fn test_baseline_compare_missing_workload() {
        let config = BenchConfig::new();
        let baseline_results = vec![make_test_result(WorkloadType::SingleRowQuery, 200.0)];
        let baseline = RegressionBaseline::new("test_missing", config, baseline_results, "sqlite");

        let current_results = vec![make_test_result(WorkloadType::BatchQuery, 200.0)];
        let comparison = baseline.compare(&current_results);

        assert!(!comparison.has_regression);
        assert_eq!(comparison.workload_comparisons.len(), 0);
    }
}