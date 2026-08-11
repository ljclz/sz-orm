//! # Benchmark 回归对比结构（`benchmark-suite` feature）
//!
//! 定义 `BenchPath` enum、`BaselinePoint`/`RegressionPoint`/`RegressionReport` 结构，
//! 用于 M2-T2 回归基准线对比。

use serde::{Deserialize, Serialize};

/// 六大核心基准路径
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BenchPath {
    /// 路径 1：查询构造
    QueryBuild,
    /// 路径 2：连接池
    Pool,
    /// 路径 3：缓存
    Cache,
    /// 路径 4：事务
    Transaction,
    /// 路径 5：序列化
    Serialization,
    /// 路径 6：流式查询
    Stream,
}

/// 基准线数据点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselinePoint {
    /// 基准路径
    pub path: BenchPath,
    /// 基准点名称
    pub name: String,
    /// 均值（纳秒）
    pub mean_ns: f64,
    /// 标准差（纳秒）
    pub stddev_ns: f64,
    /// P99 延迟（纳秒）
    pub p99_ns: f64,
    /// 采集时间戳（Unix epoch 秒）
    pub timestamp: u64,
}

/// 回归对比数据点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionPoint {
    /// 基准路径
    pub path: BenchPath,
    /// 基准点名称
    pub name: String,
    /// 当前均值（纳秒）
    pub current_mean_ns: f64,
    /// 基线均值（纳秒）
    pub baseline_mean_ns: f64,
    /// 变化百分比（正数=回退，负数=改善）
    pub change_percent: f64,
    /// 是否标记为回退（≥10% 变化）
    pub is_regression: bool,
}

/// 回归报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionReport {
    /// 报告生成时间戳
    pub timestamp: u64,
    /// 所有对比数据点
    pub points: Vec<RegressionPoint>,
    /// 回退数据点数量
    pub regression_count: usize,
    /// 改善数据点数量
    pub improvement_count: usize,
}

impl RegressionReport {
    /// 从对比点列表生成报告
    pub fn from_points(points: Vec<RegressionPoint>) -> Self {
        let regression_count = points.iter().filter(|p| p.is_regression && p.change_percent > 0.0).count();
        let improvement_count = points.iter().filter(|p| p.change_percent < 0.0).count();
        Self {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            points,
            regression_count,
            improvement_count,
        }
    }
}

/// 计算变化百分比并判断是否为回退（≥10%）
pub fn compute_change(current: f64, baseline: f64) -> (f64, bool) {
    if baseline == 0.0 {
        return (0.0, false);
    }
    let change_percent = ((current - baseline) / baseline) * 100.0;
    let is_regression = change_percent.abs() >= 10.0;
    (change_percent, is_regression)
}