//! TimescaleDB 核心 trait 与 Builder/Wrapper/Provider

use crate::error::TimescaleError;
use crate::types::{Aggregation, DownsampleConfig, Metric, TimeBucket};
use async_trait::async_trait;
use chrono::{DateTime, Utc};

/// TimescaleDB 时序扩展 trait
#[async_trait]
pub trait TimeseriesExt: Send + Sync {
    /// 创建 hypertable（TimescaleDB 核心概念：按时间自动分区）
    ///
    /// 等价 SQL：`SELECT create_hypertable('table', 'time_column')`
    async fn create_hypertable(&self, table: &str, time_column: &str)
        -> Result<(), TimescaleError>;

    /// 插入单个指标
    async fn insert_metric(&self, metric: &Metric) -> Result<(), TimescaleError>;

    /// 批量插入指标
    async fn insert_metrics(&self, metrics: &[Metric]) -> Result<(), TimescaleError> {
        for m in metrics {
            self.insert_metric(m).await?;
        }
        Ok(())
    }

    /// 范围查询：查询指定指标在时间范围内的所有数据点
    ///
    /// 等价 SQL：`SELECT * FROM metric WHERE timestamp BETWEEN start AND end`
    async fn query_range(
        &self,
        metric: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Metric>, TimescaleError>;

    /// 时间桶聚合：按时间窗口聚合数据
    ///
    /// 等价 SQL：`SELECT time_bucket('1h', timestamp), AGG(value) FROM metric GROUP BY 1`
    async fn time_bucket_aggregate(
        &self,
        metric: &str,
        bucket: &str,
        aggregation: Aggregation,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<TimeBucket>, TimescaleError>;

    /// 创建连续聚合（物化视图，自动刷新）
    ///
    /// 等价 SQL：`CREATE MATERIALIZED VIEW ... WITH (timescaledb.continuous) AS ...`
    async fn create_continuous_aggregate(
        &self,
        view_name: &str,
        query: &str,
    ) -> Result<(), TimescaleError>;

    /// 降采样：将高频数据聚合为低频数据
    async fn downsample(&self, config: &DownsampleConfig) -> Result<(), TimescaleError>;

    /// 删除指定指标的所有数据
    async fn drop_metric(&self, metric: &str) -> Result<(), TimescaleError>;
}

/// Provider 类型
#[derive(Debug, Clone)]
pub enum TimeseriesProvider {
    /// 内存实现
    Memory,
    /// Stub 实现（生成 SQL）
    Stub,
    /// 真实 TimescaleDB（需启用 `real-timescale` feature）
    #[cfg(feature = "real-timescale")]
    RealTimescale(RealTimescaleConfig),
}

/// 真实 TimescaleDB 配置
#[cfg(feature = "real-timescale")]
#[derive(Debug, Clone, Default)]
pub struct RealTimescaleConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: String,
}

/// Wrapper enum
pub enum TimeseriesWrapper {
    Memory(crate::memory::MemoryTimeseries),
    Stub(crate::stub::StubTimeseries),
    #[cfg(feature = "real-timescale")]
    RealTimescale(crate::real_timescale::RealTimescale),
}

#[async_trait]
impl TimeseriesExt for TimeseriesWrapper {
    async fn create_hypertable(
        &self,
        table: &str,
        time_column: &str,
    ) -> Result<(), TimescaleError> {
        match self {
            TimeseriesWrapper::Memory(p) => p.create_hypertable(table, time_column).await,
            TimeseriesWrapper::Stub(p) => p.create_hypertable(table, time_column).await,
            #[cfg(feature = "real-timescale")]
            TimeseriesWrapper::RealTimescale(p) => p.create_hypertable(table, time_column).await,
        }
    }

    async fn insert_metric(&self, metric: &Metric) -> Result<(), TimescaleError> {
        match self {
            TimeseriesWrapper::Memory(p) => p.insert_metric(metric).await,
            TimeseriesWrapper::Stub(p) => p.insert_metric(metric).await,
            #[cfg(feature = "real-timescale")]
            TimeseriesWrapper::RealTimescale(p) => p.insert_metric(metric).await,
        }
    }

    async fn query_range(
        &self,
        metric: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Metric>, TimescaleError> {
        match self {
            TimeseriesWrapper::Memory(p) => p.query_range(metric, start, end).await,
            TimeseriesWrapper::Stub(p) => p.query_range(metric, start, end).await,
            #[cfg(feature = "real-timescale")]
            TimeseriesWrapper::RealTimescale(p) => p.query_range(metric, start, end).await,
        }
    }

    async fn time_bucket_aggregate(
        &self,
        metric: &str,
        bucket: &str,
        aggregation: Aggregation,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<TimeBucket>, TimescaleError> {
        match self {
            TimeseriesWrapper::Memory(p) => {
                p.time_bucket_aggregate(metric, bucket, aggregation, start, end)
                    .await
            }
            TimeseriesWrapper::Stub(p) => {
                p.time_bucket_aggregate(metric, bucket, aggregation, start, end)
                    .await
            }
            #[cfg(feature = "real-timescale")]
            TimeseriesWrapper::RealTimescale(p) => {
                p.time_bucket_aggregate(metric, bucket, aggregation, start, end)
                    .await
            }
        }
    }

    async fn create_continuous_aggregate(
        &self,
        view_name: &str,
        query: &str,
    ) -> Result<(), TimescaleError> {
        match self {
            TimeseriesWrapper::Memory(p) => p.create_continuous_aggregate(view_name, query).await,
            TimeseriesWrapper::Stub(p) => p.create_continuous_aggregate(view_name, query).await,
            #[cfg(feature = "real-timescale")]
            TimeseriesWrapper::RealTimescale(p) => {
                p.create_continuous_aggregate(view_name, query).await
            }
        }
    }

    async fn downsample(&self, config: &DownsampleConfig) -> Result<(), TimescaleError> {
        match self {
            TimeseriesWrapper::Memory(p) => p.downsample(config).await,
            TimeseriesWrapper::Stub(p) => p.downsample(config).await,
            #[cfg(feature = "real-timescale")]
            TimeseriesWrapper::RealTimescale(p) => p.downsample(config).await,
        }
    }

    async fn drop_metric(&self, metric: &str) -> Result<(), TimescaleError> {
        match self {
            TimeseriesWrapper::Memory(p) => p.drop_metric(metric).await,
            TimeseriesWrapper::Stub(p) => p.drop_metric(metric).await,
            #[cfg(feature = "real-timescale")]
            TimeseriesWrapper::RealTimescale(p) => p.drop_metric(metric).await,
        }
    }
}

/// Builder
pub struct TimeseriesBuilder {
    provider: TimeseriesProvider,
}

impl TimeseriesBuilder {
    pub fn new(provider: TimeseriesProvider) -> Self {
        Self { provider }
    }

    pub fn build(self) -> Result<TimeseriesWrapper, TimescaleError> {
        match self.provider {
            TimeseriesProvider::Memory => Ok(TimeseriesWrapper::Memory(
                crate::memory::MemoryTimeseries::new(),
            )),
            TimeseriesProvider::Stub => {
                Ok(TimeseriesWrapper::Stub(crate::stub::StubTimeseries::new()))
            }
            #[cfg(feature = "real-timescale")]
            TimeseriesProvider::RealTimescale(config) => {
                let real = crate::real_timescale::RealTimescale::new(config)?;
                Ok(TimeseriesWrapper::RealTimescale(real))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_builder_memory() {
        let wrapper = TimeseriesBuilder::new(TimeseriesProvider::Memory)
            .build()
            .expect("build failed");
        wrapper
            .create_hypertable("metrics", "ts")
            .await
            .expect("create_hypertable failed");
    }

    #[tokio::test]
    async fn test_builder_stub() {
        let wrapper = TimeseriesBuilder::new(TimeseriesProvider::Stub)
            .build()
            .expect("build failed");
        wrapper
            .create_hypertable("metrics", "ts")
            .await
            .expect("create_hypertable failed");
    }
}
