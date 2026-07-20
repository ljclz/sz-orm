//! Stub 实现：生成 TimescaleDB SQL 字符串但不执行
//!
//! # 安全性（v0.2.2 修复 C-4）
//!
//! 所有 SQL 标识符（表名/列名/视图名/指标名）经 `safety::validate_identifier` 校验，
//! time_bucket 参数经 `safety::validate_time_bucket` 校验，杜绝 SQL 注入。

use crate::error::TimescaleError;
use crate::safety::{
    validate_continuous_aggregate_query, validate_identifier, validate_time_bucket,
};
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
        // v0.2.2 修复 C-4：表名/时间列名严格校验
        validate_identifier(table, "table")?;
        validate_identifier(time_column, "time column")?;
        let sql = format!("SELECT create_hypertable('{}', '{}')", table, time_column);
        self.log(sql);
        Ok(())
    }

    async fn insert_metric(&self, metric: &Metric) -> Result<(), TimescaleError> {
        // v0.2.2 修复 C-4：metric.name 严格校验
        validate_identifier(&metric.name, "metric name")?;
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
        // v0.2.2 修复 C-4：metric 严格校验
        validate_identifier(metric, "metric")?;
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
        // v0.2.2 修复 C-4：metric 与 bucket 严格校验
        validate_identifier(metric, "metric")?;
        validate_time_bucket(bucket)?;
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
        // v0.2.2 修复 C-4：视图名严格校验
        // v0.2.2 修复 P0-9（第二次审查发现）：query 严格校验（防 SQL 注入）
        validate_identifier(view_name, "view name")?;
        validate_continuous_aggregate_query(query)?;
        let sql = format!(
            "CREATE MATERIALIZED VIEW {} WITH (timescaledb.continuous) AS {} WITH NO DATA",
            view_name, query
        );
        self.log(sql);
        Ok(())
    }

    async fn downsample(&self, config: &DownsampleConfig) -> Result<(), TimescaleError> {
        // v0.2.2 修复 C-4：target_metric/source_metric/interval 严格校验
        validate_identifier(&config.target_metric, "target metric")?;
        validate_identifier(&config.source_metric, "source metric")?;
        validate_time_bucket(&config.interval)?;
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
        // v0.2.2 修复 C-4：metric 严格校验
        validate_identifier(metric, "metric")?;
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

    // v0.2.2 修复 C-4：SQL 注入测试套件

    #[tokio::test]
    async fn test_stub_rejects_sql_injection_in_table() {
        let stub = StubTimeseries::new();
        let result = stub
            .create_hypertable("metrics; DROP TABLE metrics", "ts")
            .await;
        assert!(result.is_err(), "should reject SQL injection in table name");
    }

    #[tokio::test]
    async fn test_stub_rejects_sql_injection_in_time_column() {
        let stub = StubTimeseries::new();
        let result = stub
            .create_hypertable("metrics", "ts; DROP TABLE metrics")
            .await;
        assert!(
            result.is_err(),
            "should reject SQL injection in time column"
        );
    }

    #[tokio::test]
    async fn test_stub_rejects_sql_injection_in_metric_name() {
        let stub = StubTimeseries::new();
        let m = Metric::new("cpu; DROP TABLE cpu", Utc::now(), 0.75);
        let result = stub.insert_metric(&m).await;
        assert!(
            result.is_err(),
            "should reject SQL injection in metric name"
        );
    }

    #[tokio::test]
    async fn test_stub_rejects_sql_injection_in_query_range_metric() {
        let stub = StubTimeseries::new();
        let start = Utc.with_ymd_and_hms(2026, 7, 20, 0, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 7, 20, 1, 0, 0).unwrap();
        let result = stub.query_range("cpu' OR '1'='1", start, end).await;
        assert!(
            result.is_err(),
            "should reject SQL injection in query_range metric"
        );
    }

    #[tokio::test]
    async fn test_stub_rejects_sql_injection_in_time_bucket_metric() {
        let stub = StubTimeseries::new();
        let start = Utc.with_ymd_and_hms(2026, 7, 20, 0, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 7, 20, 1, 0, 0).unwrap();
        let result = stub
            .time_bucket_aggregate("cpu; DROP TABLE", "5m", Aggregation::Avg, start, end)
            .await;
        assert!(
            result.is_err(),
            "should reject SQL injection in time_bucket metric"
        );
    }

    #[tokio::test]
    async fn test_stub_rejects_sql_injection_in_bucket() {
        let stub = StubTimeseries::new();
        let start = Utc.with_ymd_and_hms(2026, 7, 20, 0, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 7, 20, 1, 0, 0).unwrap();
        let result = stub
            .time_bucket_aggregate("cpu", "5m; DROP TABLE", Aggregation::Avg, start, end)
            .await;
        assert!(result.is_err(), "should reject SQL injection in bucket");
    }

    #[tokio::test]
    async fn test_stub_rejects_sql_injection_in_view_name() {
        let stub = StubTimeseries::new();
        let result = stub
            .create_continuous_aggregate("view; DROP TABLE view", "SELECT 1")
            .await;
        assert!(result.is_err(), "should reject SQL injection in view name");
    }

    #[tokio::test]
    async fn test_stub_rejects_sql_injection_in_continuous_aggregate_query() {
        // v0.2.2 修复 P0-9（第二次审查发现）：query 参数 SQL 注入防护
        let stub = StubTimeseries::new();
        // 分号注入
        let result = stub
            .create_continuous_aggregate("cpu_view", "SELECT 1; DROP TABLE metrics")
            .await;
        assert!(
            result.is_err(),
            "should reject semicolon injection in query"
        );
        // 非 SELECT 开头
        let result = stub
            .create_continuous_aggregate("cpu_view", "DROP TABLE metrics")
            .await;
        assert!(result.is_err(), "should reject non-SELECT query");
        // 行注释
        let result = stub
            .create_continuous_aggregate("cpu_view", "SELECT 1 -- comment")
            .await;
        assert!(result.is_err(), "should reject line comment in query");
        // 块注释
        let result = stub
            .create_continuous_aggregate("cpu_view", "SELECT /* x */ 1")
            .await;
        assert!(result.is_err(), "should reject block comment in query");
        // DDL 关键字
        let result = stub
            .create_continuous_aggregate("cpu_view", "SELECT * FROM m WHERE x = DROP")
            .await;
        assert!(result.is_err(), "should reject DDL keyword in query");
        // 合法 query 应通过
        let result = stub
            .create_continuous_aggregate(
                "cpu_view",
                "SELECT time_bucket('1h', ts), avg(v) FROM m GROUP BY 1",
            )
            .await;
        assert!(result.is_ok(), "should accept valid SELECT query");
        // 合法 CTE 应通过
        let result = stub
            .create_continuous_aggregate("cpu_view", "WITH x AS (SELECT 1) SELECT * FROM x")
            .await;
        assert!(result.is_ok(), "should accept valid WITH CTE query");
    }

    #[tokio::test]
    async fn test_stub_rejects_sql_injection_in_downsample_target() {
        let stub = StubTimeseries::new();
        let config = DownsampleConfig::new("cpu_1h; DROP TABLE", "cpu_raw", "1h", Aggregation::Avg);
        let result = stub.downsample(&config).await;
        assert!(
            result.is_err(),
            "should reject SQL injection in downsample target"
        );
    }

    #[tokio::test]
    async fn test_stub_rejects_sql_injection_in_downsample_interval() {
        let stub = StubTimeseries::new();
        let config = DownsampleConfig::new("cpu_1h", "cpu_raw", "1h; DROP TABLE", Aggregation::Avg);
        let result = stub.downsample(&config).await;
        assert!(
            result.is_err(),
            "should reject SQL injection in downsample interval"
        );
    }

    #[tokio::test]
    async fn test_stub_rejects_sql_injection_in_drop_metric() {
        let stub = StubTimeseries::new();
        let result = stub.drop_metric("cpu; DROP TABLE cpu").await;
        assert!(
            result.is_err(),
            "should reject SQL injection in drop_metric"
        );
    }

    #[tokio::test]
    async fn test_stub_rejects_invalid_bucket_unit() {
        let stub = StubTimeseries::new();
        let start = Utc.with_ymd_and_hms(2026, 7, 20, 0, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 7, 20, 1, 0, 0).unwrap();
        let result = stub
            .time_bucket_aggregate("cpu", "5x", Aggregation::Avg, start, end)
            .await;
        assert!(result.is_err(), "should reject invalid bucket unit");
    }
}
