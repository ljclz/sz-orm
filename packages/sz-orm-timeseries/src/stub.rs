//! Stub 实现：生成 TimescaleDB SQL 字符串但不执行

use crate::error::TimescaleError;
use crate::timeseries::TimeseriesExt;
use crate::types::{Aggregation, DownsampleConfig, Metric, TimeBucket};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::sync::Mutex;

/// Stub TimescaleDB 实现
pub struct StubTimeseries {
    pub sql_log: Mutex<Vec<String>>,
}

impl StubTimeseries {
    pub fn new() -> Self {
        Self {
            sql_log: Mutex::new(Vec::new()),
        }
    }

    pub fn sql_history(&self) -> Vec<String> {
        self.sql_log.lock().unwrap().clone()
    }

    pub fn clear(&self) {
        self.sql_log.lock().unwrap().clear();
    }

    fn log(&self, sql: String) {
        self.sql_log.lock().unwrap().push(sql);
    }
}

impl Default for StubTimeseries {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TimeseriesExt for StubTimeseries {
    async fn create_hypertable(
        &self,
        table: &str,
        time_column: &str,
    ) -> Result<(), TimescaleError> {
        let sql = format!("SELECT create_hypertable('{}', '{}')", table, time_column);
        self.log(sql);
        Ok(())
    }

    async fn insert_metric(&self, metric: &Metric) -> Result<(), TimescaleError> {
        let tags_json = serde_json::to_string(&metric.tags).unwrap_or_default();
        let sql = format!(
            "INSERT INTO {} (timestamp, value, tags) VALUES ('{}', {}, '{}')",
            metric.name, metric.timestamp, metric.value, tags_json
        );
        self.log(sql);
        Ok(())
    }

    async fn query_range(
        &self,
        metric: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Metric>, TimescaleError> {
        let sql = format!(
            "SELECT * FROM {} WHERE timestamp >= '{}' AND timestamp < '{}'",
            metric, start, end
        );
        self.log(sql);
        Ok(Vec::new())
    }

    async fn time_bucket_aggregate(
        &self,
        metric: &str,
        bucket: &str,
        aggregation: Aggregation,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<TimeBucket>, TimescaleError> {
        let sql = format!(
            "SELECT time_bucket('{}', timestamp) AS bucket, {}(value) AS value FROM {} WHERE timestamp >= '{}' AND timestamp < '{}' GROUP BY bucket ORDER BY bucket",
            bucket, aggregation.as_sql(), metric, start, end
        );
        self.log(sql);
        Ok(Vec::new())
    }

    async fn create_continuous_aggregate(
        &self,
        view_name: &str,
        query: &str,
    ) -> Result<(), TimescaleError> {
        let sql = format!(
            "CREATE MATERIALIZED VIEW {} WITH (timescaledb.continuous) AS {} WITH NO DATA",
            view_name, query
        );
        self.log(sql);
        Ok(())
    }

    async fn downsample(&self, config: &DownsampleConfig) -> Result<(), TimescaleError> {
        let sql = format!(
            "INSERT INTO {} SELECT time_bucket('{}', timestamp), {}(value) FROM {} GROUP BY 1",
            config.target_metric,
            config.interval,
            config.aggregation.as_sql(),
            config.source_metric
        );
        self.log(sql);
        Ok(())
    }

    async fn drop_metric(&self, metric: &str) -> Result<(), TimescaleError> {
        let sql = format!("DROP TABLE IF EXISTS {}", metric);
        self.log(sql);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[tokio::test]
    async fn test_stub_create_hypertable() {
        let stub = StubTimeseries::new();
        stub.create_hypertable("metrics", "ts").await.unwrap();
        let history = stub.sql_history();
        assert_eq!(history.len(), 1);
        assert!(history[0].contains("create_hypertable"));
        assert!(history[0].contains("metrics"));
        assert!(history[0].contains("ts"));
    }

    #[tokio::test]
    async fn test_stub_insert_metric() {
        let stub = StubTimeseries::new();
        let m = Metric::new("cpu", Utc::now(), 0.75);
        stub.insert_metric(&m).await.unwrap();
        let history = stub.sql_history();
        assert_eq!(history.len(), 1);
        assert!(history[0].contains("INSERT INTO cpu"));
    }

    #[tokio::test]
    async fn test_stub_query_range() {
        let stub = StubTimeseries::new();
        let start = Utc.with_ymd_and_hms(2026, 7, 20, 0, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 7, 20, 1, 0, 0).unwrap();
        let _ = stub.query_range("cpu", start, end).await.unwrap();
        let history = stub.sql_history();
        assert_eq!(history.len(), 1);
        assert!(history[0].contains("SELECT * FROM cpu"));
        assert!(history[0].contains("WHERE timestamp"));
    }

    #[tokio::test]
    async fn test_stub_time_bucket() {
        let stub = StubTimeseries::new();
        let start = Utc.with_ymd_and_hms(2026, 7, 20, 0, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 7, 20, 1, 0, 0).unwrap();
        let _ = stub
            .time_bucket_aggregate("cpu", "5m", Aggregation::Avg, start, end)
            .await
            .unwrap();
        let history = stub.sql_history();
        assert_eq!(history.len(), 1);
        assert!(history[0].contains("time_bucket('5m'"));
        assert!(history[0].contains("AVG(value)"));
    }

    #[tokio::test]
    async fn test_stub_create_continuous_aggregate() {
        let stub = StubTimeseries::new();
        stub.create_continuous_aggregate("cpu_1h_view", "SELECT time_bucket('1h', ts) ...")
            .await
            .unwrap();
        let history = stub.sql_history();
        assert!(history[0].contains("CREATE MATERIALIZED VIEW"));
        assert!(history[0].contains("timescaledb.continuous"));
    }

    #[tokio::test]
    async fn test_stub_downsample() {
        let stub = StubTimeseries::new();
        let config = DownsampleConfig::new("cpu_raw", "cpu_1h", "1h", Aggregation::Avg);
        stub.downsample(&config).await.unwrap();
        let history = stub.sql_history();
        assert!(history[0].contains("INSERT INTO cpu_1h"));
        assert!(history[0].contains("time_bucket('1h'"));
    }
}
