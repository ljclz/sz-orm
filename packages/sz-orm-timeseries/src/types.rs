//! 时序数据类型定义

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 时序数据点
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Metric {
    /// 指标名（如 cpu_usage、memory_usage、request_latency）
    pub name: String,
    /// 时间戳
    pub timestamp: DateTime<Utc>,
    /// 数值
    pub value: f64,
    /// 标签（如 host、region、service）
    pub tags: HashMap<String, String>,
}

impl Metric {
    pub fn new(name: impl Into<String>, timestamp: DateTime<Utc>, value: f64) -> Self {
        Self {
            name: name.into(),
            timestamp,
            value,
            tags: HashMap::new(),
        }
    }

    pub fn with_tags(
        name: impl Into<String>,
        timestamp: DateTime<Utc>,
        value: f64,
        tags: HashMap<String, String>,
    ) -> Self {
        Self {
            name: name.into(),
            timestamp,
            value,
            tags,
        }
    }

    pub fn with_tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.tags.insert(key.into(), value.into());
        self
    }
}

/// 聚合类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Aggregation {
    Avg,
    Sum,
    Min,
    Max,
    Count,
}

impl Aggregation {
    pub fn as_sql(&self) -> &'static str {
        match self {
            Aggregation::Avg => "AVG",
            Aggregation::Sum => "SUM",
            Aggregation::Min => "MIN",
            Aggregation::Max => "MAX",
            Aggregation::Count => "COUNT",
        }
    }

    pub fn apply(&self, values: &[f64]) -> f64 {
        if values.is_empty() {
            return 0.0;
        }
        match self {
            Aggregation::Avg => {
                let sum: f64 = values.iter().sum();
                sum / values.len() as f64
            }
            Aggregation::Sum => values.iter().sum(),
            Aggregation::Min => values.iter().cloned().fold(f64::INFINITY, f64::min),
            Aggregation::Max => values.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
            Aggregation::Count => values.len() as f64,
        }
    }
}

/// 时间桶（聚合结果）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimeBucket {
    /// 桶起始时间
    pub bucket_start: DateTime<Utc>,
    /// 桶内数据点数
    pub count: u64,
    /// 桶内值之和
    pub sum: f64,
    /// 桶内最小值
    pub min: f64,
    /// 桶内最大值
    pub max: f64,
    /// 桶内平均值
    pub avg: f64,
}

impl TimeBucket {
    pub fn from_values(bucket_start: DateTime<Utc>, values: &[f64]) -> Self {
        if values.is_empty() {
            return Self {
                bucket_start,
                count: 0,
                sum: 0.0,
                min: 0.0,
                max: 0.0,
                avg: 0.0,
            };
        }
        let count = values.len() as u64;
        let sum: f64 = values.iter().sum();
        let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let avg = sum / count as f64;
        Self {
            bucket_start,
            count,
            sum,
            min,
            max,
            avg,
        }
    }
}

/// 降采样配置
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DownsampleConfig {
    /// 源指标名
    pub source_metric: String,
    /// 目标指标名
    pub target_metric: String,
    /// 聚合时间窗口（如 "1h"、"1d"）
    pub interval: String,
    /// 聚合方式
    pub aggregation: Aggregation,
}

impl DownsampleConfig {
    pub fn new(
        source_metric: impl Into<String>,
        target_metric: impl Into<String>,
        interval: impl Into<String>,
        aggregation: Aggregation,
    ) -> Self {
        Self {
            source_metric: source_metric.into(),
            target_metric: target_metric.into(),
            interval: interval.into(),
            aggregation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metric_with_tag() {
        let now = Utc::now();
        let m = Metric::new("cpu_usage", now, 0.75)
            .with_tag("host", "server1")
            .with_tag("region", "us-east");
        assert_eq!(m.name, "cpu_usage");
        assert_eq!(m.value, 0.75);
        assert_eq!(m.tags.get("host"), Some(&"server1".to_string()));
        assert_eq!(m.tags.get("region"), Some(&"us-east".to_string()));
    }

    #[test]
    fn test_aggregation_apply() {
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        assert!((Aggregation::Avg.apply(&values) - 3.0).abs() < 1e-10);
        assert!((Aggregation::Sum.apply(&values) - 15.0).abs() < 1e-10);
        assert!((Aggregation::Min.apply(&values) - 1.0).abs() < 1e-10);
        assert!((Aggregation::Max.apply(&values) - 5.0).abs() < 1e-10);
        assert!((Aggregation::Count.apply(&values) - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_aggregation_empty() {
        let empty: Vec<f64> = vec![];
        assert_eq!(Aggregation::Avg.apply(&empty), 0.0);
        assert_eq!(Aggregation::Count.apply(&empty), 0.0);
    }

    #[test]
    fn test_aggregation_sql() {
        assert_eq!(Aggregation::Avg.as_sql(), "AVG");
        assert_eq!(Aggregation::Sum.as_sql(), "SUM");
        assert_eq!(Aggregation::Min.as_sql(), "MIN");
        assert_eq!(Aggregation::Max.as_sql(), "MAX");
        assert_eq!(Aggregation::Count.as_sql(), "COUNT");
    }

    #[test]
    fn test_time_bucket_from_values() {
        let now = Utc::now();
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let bucket = TimeBucket::from_values(now, &values);
        assert_eq!(bucket.count, 5);
        assert!((bucket.sum - 15.0).abs() < 1e-10);
        assert!((bucket.min - 1.0).abs() < 1e-10);
        assert!((bucket.max - 5.0).abs() < 1e-10);
        assert!((bucket.avg - 3.0).abs() < 1e-10);
    }

    #[test]
    fn test_time_bucket_empty() {
        let now = Utc::now();
        let bucket = TimeBucket::from_values(now, &[]);
        assert_eq!(bucket.count, 0);
        assert_eq!(bucket.sum, 0.0);
    }

    #[test]
    fn test_downsample_config() {
        let config = DownsampleConfig::new("cpu_raw", "cpu_1h", "1h", Aggregation::Avg);
        assert_eq!(config.source_metric, "cpu_raw");
        assert_eq!(config.target_metric, "cpu_1h");
        assert_eq!(config.interval, "1h");
        assert_eq!(config.aggregation, Aggregation::Avg);
    }
}
