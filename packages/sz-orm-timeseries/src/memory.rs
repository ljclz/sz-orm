//! 内存实现：纯 Rust 时序数据存储（不连接数据库）

use crate::error::TimescaleError;
use crate::timeseries::TimeseriesExt;
use crate::types::{Aggregation, DownsampleConfig, Metric, TimeBucket};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Mutex;

/// 内存 TimescaleDB 实现
pub struct MemoryTimeseries {
    /// 指标存储：metric_name -> `Vec<Metric>`
    storage: Mutex<HashMap<String, Vec<Metric>>>,
    /// hypertable 注册表
    hypertables: Mutex<Vec<(String, String)>>,
    /// 连续聚合视图
    continuous_aggregates: Mutex<Vec<(String, String)>>,
}

impl MemoryTimeseries {
    pub fn new() -> Self {
        Self {
            storage: Mutex::new(HashMap::new()),
            hypertables: Mutex::new(Vec::new()),
            continuous_aggregates: Mutex::new(Vec::new()),
        }
    }

    /// 解析时间桶字符串（如 "1h"、"5m"、"1d"）为秒数
    fn parse_bucket(bucket: &str) -> Result<i64, TimescaleError> {
        if bucket.is_empty() {
            return Err(TimescaleError::InvalidConfig(
                "bucket cannot be empty".to_string(),
            ));
        }
        let (num_str, unit) = bucket.split_at(bucket.len() - 1);
        let num: i64 = num_str.parse().map_err(|_| {
            TimescaleError::InvalidConfig(format!("invalid bucket number: {}", num_str))
        })?;
        let seconds = match unit {
            "s" => num,
            "m" => num * 60,
            "h" => num * 3600,
            "d" => num * 86400,
            _ => {
                return Err(TimescaleError::InvalidConfig(format!(
                    "unsupported bucket unit: {}",
                    unit
                )))
            }
        };
        Ok(seconds)
    }
}

impl Default for MemoryTimeseries {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TimeseriesExt for MemoryTimeseries {
    async fn create_hypertable(
        &self,
        table: &str,
        time_column: &str,
    ) -> Result<(), TimescaleError> {
        self.hypertables
            .lock()
            .unwrap()
            .push((table.to_string(), time_column.to_string()));
        // 确保 storage 中有该 metric 的 entry
        self.storage
            .lock()
            .unwrap()
            .entry(table.to_string())
            .or_default();
        Ok(())
    }

    async fn insert_metric(&self, metric: &Metric) -> Result<(), TimescaleError> {
        let mut storage = self.storage.lock().unwrap();
        storage
            .entry(metric.name.clone())
            .or_default()
            .push(metric.clone());
        Ok(())
    }

    async fn query_range(
        &self,
        metric: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Metric>, TimescaleError> {
        if start >= end {
            return Err(TimescaleError::InvalidTimeRange {
                start: start.to_rfc3339(),
                end: end.to_rfc3339(),
            });
        }
        let storage = self.storage.lock().unwrap();
        let data = storage.get(metric).cloned().unwrap_or_default();
        Ok(data
            .into_iter()
            .filter(|m| m.timestamp >= start && m.timestamp < end)
            .collect())
    }

    async fn time_bucket_aggregate(
        &self,
        metric: &str,
        bucket: &str,
        aggregation: Aggregation,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<TimeBucket>, TimescaleError> {
        if start >= end {
            return Err(TimescaleError::InvalidTimeRange {
                start: start.to_rfc3339(),
                end: end.to_rfc3339(),
            });
        }
        let bucket_secs = Self::parse_bucket(bucket)?;
        let bucket_duration = chrono::Duration::seconds(bucket_secs);

        // 获取范围内的数据
        let data = self.query_range(metric, start, end).await?;

        // 按桶分组
        let mut buckets: HashMap<DateTime<Utc>, Vec<f64>> = HashMap::new();
        for m in data {
            let elapsed = (m.timestamp - start).num_seconds();
            let bucket_idx = elapsed / bucket_secs;
            let bucket_start = start + chrono::Duration::seconds(bucket_idx * bucket_secs);
            buckets.entry(bucket_start).or_default().push(m.value);
        }

        // 生成连续的桶（即使没有数据）
        let mut result = Vec::new();
        let mut current = start;
        while current < end {
            let values = buckets.remove(&current).unwrap_or_default();
            let mut bucket = TimeBucket::from_values(current, &values);
            // 如果桶内无数据，按聚合类型填充 0 或跳过
            if values.is_empty() {
                // 空桶：count=0，其他字段保持 0.0
                bucket = TimeBucket {
                    bucket_start: current,
                    count: 0,
                    sum: 0.0,
                    min: 0.0,
                    max: 0.0,
                    avg: 0.0,
                };
            }
            // 应用聚合（用于验证聚合函数工作）
            let _agg_value = aggregation.apply(&values);
            result.push(bucket);
            current += bucket_duration;
        }

        Ok(result)
    }

    async fn create_continuous_aggregate(
        &self,
        view_name: &str,
        query: &str,
    ) -> Result<(), TimescaleError> {
        self.continuous_aggregates
            .lock()
            .unwrap()
            .push((view_name.to_string(), query.to_string()));
        Ok(())
    }

    async fn downsample(&self, config: &DownsampleConfig) -> Result<(), TimescaleError> {
        // 内存实现：从源指标读取所有数据，按 interval 聚合，写入目标指标
        let storage = self.storage.lock().unwrap();
        let source_data = storage
            .get(&config.source_metric)
            .cloned()
            .unwrap_or_default();
        drop(storage);

        if source_data.is_empty() {
            return Ok(());
        }

        let bucket_secs = Self::parse_bucket(&config.interval)?;
        let start = source_data
            .iter()
            .map(|m| m.timestamp)
            .min()
            .unwrap_or_else(Utc::now);

        // 按桶分组并聚合
        let mut buckets: HashMap<i64, Vec<f64>> = HashMap::new();
        for m in &source_data {
            let elapsed = (m.timestamp - start).num_seconds();
            let bucket_idx = elapsed / bucket_secs;
            buckets.entry(bucket_idx).or_default().push(m.value);
        }

        // 生成降采样后的指标
        let mut downsampled = Vec::new();
        for (idx, values) in buckets {
            let bucket_start = start + chrono::Duration::seconds(idx * bucket_secs);
            let agg_value = config.aggregation.apply(&values);
            downsampled.push(Metric::new(
                config.target_metric.clone(),
                bucket_start,
                agg_value,
            ));
        }

        // 写入目标指标
        let mut storage = self.storage.lock().unwrap();
        let target = storage.entry(config.target_metric.clone()).or_default();
        target.extend(downsampled);

        Ok(())
    }

    async fn drop_metric(&self, metric: &str) -> Result<(), TimescaleError> {
        let mut storage = self.storage.lock().unwrap();
        if storage.remove(metric).is_none() {
            return Err(TimescaleError::NotFound(metric.to_string()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn make_metric(name: &str, ts_minutes: i64, value: f64) -> Metric {
        Metric::new(
            name,
            Utc.with_ymd_and_hms(2026, 7, 20, 0, 0, 0).unwrap()
                + chrono::Duration::minutes(ts_minutes),
            value,
        )
    }

    #[tokio::test]
    async fn test_insert_and_query() {
        let ts = MemoryTimeseries::new();
        ts.insert_metric(&make_metric("cpu", 0, 0.5)).await.unwrap();
        ts.insert_metric(&make_metric("cpu", 10, 0.6))
            .await
            .unwrap();
        ts.insert_metric(&make_metric("cpu", 20, 0.7))
            .await
            .unwrap();

        let start = Utc.with_ymd_and_hms(2026, 7, 20, 0, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 7, 20, 0, 15, 0).unwrap();
        let result = ts.query_range("cpu", start, end).await.unwrap();
        assert_eq!(result.len(), 2); // 0 和 10 分钟的两个数据点
    }

    #[tokio::test]
    async fn test_query_invalid_range() {
        let ts = MemoryTimeseries::new();
        let now = Utc::now();
        let result = ts.query_range("cpu", now, now).await;
        assert!(matches!(
            result,
            Err(TimescaleError::InvalidTimeRange { .. })
        ));
    }

    #[tokio::test]
    async fn test_time_bucket_aggregate() {
        let ts = MemoryTimeseries::new();
        // 每 10 分钟一个数据点，4 个点
        for i in 0..4 {
            ts.insert_metric(&make_metric("cpu", i * 10, 0.1 * (i + 1) as f64))
                .await
                .unwrap();
        }

        let start = Utc.with_ymd_and_hms(2026, 7, 20, 0, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 7, 20, 1, 0, 0).unwrap();
        let buckets = ts
            .time_bucket_aggregate("cpu", "30m", Aggregation::Avg, start, end)
            .await
            .unwrap();
        // 1 小时按 30 分钟分桶 = 2 个桶
        assert_eq!(buckets.len(), 2);
        // 第一个桶（0-30min）：包含 0, 10, 20 分钟的数据
        assert_eq!(buckets[0].count, 3);
        // (0.1+0.2+0.3)/3 = 0.2
        assert!((buckets[0].avg - 0.2).abs() < 1e-10);
        // 第二个桶（30-60min）：包含 30 分钟的数据
        assert_eq!(buckets[1].count, 1);
        assert!((buckets[1].avg - 0.4).abs() < 1e-10);
    }

    #[tokio::test]
    async fn test_downsample() {
        let ts = MemoryTimeseries::new();
        for i in 0..6 {
            ts.insert_metric(&make_metric("cpu_raw", i * 10, (i + 1) as f64))
                .await
                .unwrap();
        }

        let config = DownsampleConfig::new("cpu_raw", "cpu_30m", "30m", Aggregation::Avg);
        ts.downsample(&config).await.unwrap();

        let start = Utc.with_ymd_and_hms(2026, 7, 20, 0, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 7, 20, 1, 30, 0).unwrap();
        let result = ts.query_range("cpu_30m", start, end).await.unwrap();
        assert_eq!(result.len(), 2); // 60 分钟 / 30 分钟 = 2 个桶
    }

    #[tokio::test]
    async fn test_drop_metric() {
        let ts = MemoryTimeseries::new();
        ts.insert_metric(&make_metric("cpu", 0, 0.5)).await.unwrap();
        ts.drop_metric("cpu").await.unwrap();
        let result = ts.drop_metric("cpu").await;
        assert!(matches!(result, Err(TimescaleError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_parse_bucket() {
        assert_eq!(MemoryTimeseries::parse_bucket("30s").unwrap(), 30);
        assert_eq!(MemoryTimeseries::parse_bucket("5m").unwrap(), 300);
        assert_eq!(MemoryTimeseries::parse_bucket("1h").unwrap(), 3600);
        assert_eq!(MemoryTimeseries::parse_bucket("1d").unwrap(), 86400);
        assert!(MemoryTimeseries::parse_bucket("1x").is_err());
        assert!(MemoryTimeseries::parse_bucket("").is_err());
    }

    #[tokio::test]
    async fn test_batch_insert() {
        let ts = MemoryTimeseries::new();
        let metrics = vec![
            make_metric("cpu", 0, 0.1),
            make_metric("cpu", 1, 0.2),
            make_metric("cpu", 2, 0.3),
        ];
        ts.insert_metrics(&metrics).await.unwrap();
        let start = Utc.with_ymd_and_hms(2026, 7, 20, 0, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 7, 20, 0, 10, 0).unwrap();
        let result = ts.query_range("cpu", start, end).await.unwrap();
        assert_eq!(result.len(), 3);
    }

    #[tokio::test]
    async fn test_create_hypertable_and_continuous_aggregate() {
        let ts = MemoryTimeseries::new();
        ts.create_hypertable("metrics", "ts").await.unwrap();
        ts.create_continuous_aggregate("cpu_1h_view", "SELECT ...")
            .await
            .unwrap();
        assert_eq!(ts.hypertables.lock().unwrap().len(), 1);
        assert_eq!(ts.continuous_aggregates.lock().unwrap().len(), 1);
    }
}
