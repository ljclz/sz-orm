//! 真实 TimescaleDB 实现（feature = "real-timescale"）
//!
//! # v0.2.2 修复 V-4
//!
//! 原实现在 `new()` 中调用异步的 `tokio_postgres::connect()`，但 `new()` 不是 async，
//! 导致编译失败。改用 `tokio::sync::OnceCell<Client>` 延迟连接：`new()` 仅存储配置，
//! 首次调用任意方法时通过 `client()` 建立 PostgreSQL 连接。
//!
//! # v0.2.2 修复 C-4
//!
//! 所有 SQL 标识符（表名/列名/视图名/指标名）经 `safety::validate_identifier` 校验，
//! time_bucket 参数经 `safety::validate_time_bucket` 校验，杜绝 SQL 注入。

use crate::error::TimescaleError;
use crate::safety::{
    validate_continuous_aggregate_query, validate_identifier, validate_time_bucket,
};
use crate::timeseries::{RealTimescaleConfig, TimeseriesExt};
use crate::types::{Aggregation, DownsampleConfig, Metric, TimeBucket};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::sync::OnceCell;
use tokio_postgres::Client;

/// 真实 TimescaleDB 实现
pub struct RealTimescale {
    config: RealTimescaleConfig,
    client: OnceCell<Client>,
}

impl RealTimescale {
    pub fn new(config: RealTimescaleConfig) -> Result<Self, TimescaleError> {
        // v0.2.2 修复 V-4：不再在同步 new() 中调用异步 connect()
        Ok(Self {
            config,
            client: OnceCell::new(),
        })
    }

    /// 延迟建立 PostgreSQL 连接（首次调用时初始化，后续复用）
    async fn client(&self) -> Result<&Client, TimescaleError> {
        self.client
            .get_or_try_init(|| async {
                let conn_str = format!(
                    "host={} port={} dbname={} user={} password={}",
                    self.config.host,
                    self.config.port,
                    self.config.database,
                    self.config.username,
                    self.config.password
                );
                let (client, connection) =
                    tokio_postgres::connect(&conn_str, tokio_postgres::NoTls)
                        .await
                        .map_err(|e| TimescaleError::Connection(e.to_string()))?;
                tokio::spawn(async move {
                    if let Err(e) = connection.await {
                        eprintln!("[sz-orm-timeseries] connection error: {}", e);
                    }
                });
                Ok::<Client, TimescaleError>(client)
            })
            .await
    }

    async fn execute_ddl(&self, sql: &str) -> Result<(), TimescaleError> {
        let client = self.client().await?;
        client
            .execute(sql, &[])
            .await
            .map_err(|e| TimescaleError::Query(e.to_string()))?;
        Ok(())
    }
}

#[async_trait]
impl TimeseriesExt for RealTimescale {
    async fn create_hypertable(
        &self,
        table: &str,
        time_column: &str,
    ) -> Result<(), TimescaleError> {
        // v0.2.2 修复 C-4：表名/时间列名严格校验
        validate_identifier(table, "table")?;
        validate_identifier(time_column, "time column")?;
        let client = self.client().await?;
        // 先创建普通表（如不存在）
        let create_table = format!(
            "CREATE TABLE IF NOT EXISTS {} (timestamp TIMESTAMPTZ NOT NULL, value DOUBLE PRECISION, tags JSONB)",
            table
        );
        client
            .execute(&create_table, &[])
            .await
            .map_err(|e| TimescaleError::Query(e.to_string()))?;
        // 转为 hypertable
        let sql = format!(
            "SELECT create_hypertable('{}', '{}', if_not_exists => TRUE)",
            table, time_column
        );
        client
            .execute(&sql, &[])
            .await
            .map_err(|e| TimescaleError::Query(e.to_string()))?;
        Ok(())
    }

    async fn insert_metric(&self, metric: &Metric) -> Result<(), TimescaleError> {
        // v0.2.2 修复 C-4：metric.name 严格校验
        validate_identifier(&metric.name, "metric name")?;
        let client = self.client().await?;
        let tags_json = serde_json::to_string(&metric.tags).unwrap_or_default();
        let sql = format!(
            "INSERT INTO {} (timestamp, value, tags) VALUES ($1, $2, $3)",
            metric.name
        );
        client
            .execute(&sql, &[&metric.timestamp, &metric.value, &tags_json])
            .await
            .map_err(|e| TimescaleError::Query(e.to_string()))?;
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
        // M-17 修复：校验时间范围（start < end 且跨度 <= MAX_QUERY_RANGE_SECS）
        crate::validate_time_range(start, end)?;
        let client = self.client().await?;
        let sql = format!(
            "SELECT timestamp, value, tags FROM {} WHERE timestamp >= $1 AND timestamp < $2 ORDER BY timestamp",
            metric
        );
        let rows = client
            .query(&sql, &[&start, &end])
            .await
            .map_err(|e| TimescaleError::Query(e.to_string()))?;

        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            let ts: chrono::DateTime<Utc> = row.get(0);
            let value: f64 = row.get(1);
            let tags_str: String = row.get(2);
            let tags: std::collections::HashMap<String, String> =
                serde_json::from_str(&tags_str).unwrap_or_default();
            result.push(Metric {
                name: metric.to_string(),
                timestamp: ts,
                value,
                tags,
            });
        }
        Ok(result)
    }

    async fn time_bucket_aggregate(
        &self,
        metric: &str,
        bucket: &str,
        _aggregation: Aggregation,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<TimeBucket>, TimescaleError> {
        // v0.2.2 修复 C-4：metric 与 bucket 严格校验
        validate_identifier(metric, "metric")?;
        validate_time_bucket(bucket)?;
        // M-17 修复：校验时间范围（start < end 且跨度 <= MAX_QUERY_RANGE_SECS）
        crate::validate_time_range(start, end)?;
        let client = self.client().await?;
        // 一次性返回所有聚合（COUNT/SUM/MIN/MAX/AVG），调用方按需选用
        let sql = format!(
            "SELECT time_bucket('{}', timestamp) AS bucket, COUNT(*), SUM(value), MIN(value), MAX(value), AVG(value) FROM {} WHERE timestamp >= $1 AND timestamp < $2 GROUP BY bucket ORDER BY bucket",
            bucket, metric
        );
        let rows = client
            .query(&sql, &[&start, &end])
            .await
            .map_err(|e| TimescaleError::Query(e.to_string()))?;

        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            let bucket_start: chrono::DateTime<Utc> = row.get(0);
            let count: i64 = row.get(1);
            let sum: f64 = row.get(2);
            let min: f64 = row.get(3);
            let max: f64 = row.get(4);
            let avg: f64 = row.get(5);
            result.push(TimeBucket {
                bucket_start,
                count: count as u64,
                sum,
                min,
                max,
                avg,
            });
        }
        Ok(result)
    }

    async fn create_continuous_aggregate(
        &self,
        view_name: &str,
        query: &str,
    ) -> Result<(), TimescaleError> {
        // v0.2.2 修复 C-4：视图名严格校验
        // v0.2.2 修复 P0-9（第二次审查发现）：query 参数严格校验（必须 SELECT/WITH 开头，
        // 禁止分号/注释/DDL/DML 关键字），杜绝 SQL 注入
        validate_identifier(view_name, "view name")?;
        validate_continuous_aggregate_query(query)?;
        let sql = format!(
            "CREATE MATERIALIZED VIEW {} WITH (timescaledb.continuous) AS {} WITH NO DATA",
            view_name, query
        );
        self.execute_ddl(&sql).await
    }

    async fn downsample(&self, config: &DownsampleConfig) -> Result<(), TimescaleError> {
        // v0.2.2 修复 C-4：target_metric/source_metric/interval 严格校验
        validate_identifier(&config.target_metric, "target metric")?;
        validate_identifier(&config.source_metric, "source metric")?;
        validate_time_bucket(&config.interval)?;
        let sql = format!(
            "INSERT INTO {} (timestamp, value, tags) SELECT time_bucket('{}', timestamp) AS bucket, {}(value), '{{}}' FROM {} GROUP BY bucket",
            config.target_metric,
            config.interval,
            config.aggregation.as_sql(),
            config.source_metric
        );
        self.execute_ddl(&sql).await
    }

    async fn drop_metric(&self, metric: &str) -> Result<(), TimescaleError> {
        // v0.2.2 修复 C-4：metric 严格校验
        validate_identifier(metric, "metric")?;
        let sql = format!("DROP TABLE IF EXISTS {}", metric);
        self.execute_ddl(&sql).await
    }
}
