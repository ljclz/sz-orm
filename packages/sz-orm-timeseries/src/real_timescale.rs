//! 真实 TimescaleDB 实现（feature = "real-timescale"）

use crate::error::TimescaleError;
use crate::timeseries::{RealTimescaleConfig, TimeseriesExt};
use crate::types::{Aggregation, DownsampleConfig, Metric, TimeBucket};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio_postgres::Client;

/// 真实 TimescaleDB 实现
pub struct RealTimescale {
    client: Client,
}

impl RealTimescale {
    pub fn new(config: RealTimescaleConfig) -> Result<Self, TimescaleError> {
        let conn_str = format!(
            "host={} port={} dbname={} user={} password={}",
            config.host, config.port, config.database, config.username, config.password
        );
        let (client, connection) = tokio_postgres::connect(&conn_str, tokio_postgres::NoTls)
            .map_err(|e| TimescaleError::Connection(e.to_string()))?;
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("[sz-orm-timeseries] connection error: {}", e);
            }
        });
        Ok(Self { client })
    }

    async fn execute_ddl(&self, sql: &str) -> Result<(), TimescaleError> {
        self.client
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
        // 先创建普通表（如不存在）
        let create_table = format!(
            "CREATE TABLE IF NOT EXISTS {} (timestamp TIMESTAMPTZ NOT NULL, value DOUBLE PRECISION, tags JSONB)",
            table
        );
        self.execute_ddl(&create_table).await?;
        // 转为 hypertable
        let sql = format!(
            "SELECT create_hypertable('{}', '{}', if_not_exists => TRUE)",
            table, time_column
        );
        self.execute_ddl(&sql).await
    }

    async fn insert_metric(&self, metric: &Metric) -> Result<(), TimescaleError> {
        let tags_json = serde_json::to_string(&metric.tags).unwrap_or_default();
        let sql = format!(
            "INSERT INTO {} (timestamp, value, tags) VALUES ($1, $2, $3)",
            metric.name
        );
        self.client
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
        let sql = format!(
            "SELECT timestamp, value, tags FROM {} WHERE timestamp >= $1 AND timestamp < $2 ORDER BY timestamp",
            metric
        );
        let rows = self
            .client
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
        aggregation: Aggregation,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<TimeBucket>, TimescaleError> {
        let sql = format!(
            "SELECT time_bucket('{}', timestamp) AS bucket, COUNT(*), SUM(value), MIN(value), MAX(value), AVG(value) FROM {} WHERE timestamp >= $1 AND timestamp < $2 GROUP BY bucket ORDER BY bucket",
            bucket, metric
        );
        let rows = self
            .client
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
        let sql = format!(
            "CREATE MATERIALIZED VIEW {} WITH (timescaledb.continuous) AS {} WITH NO DATA",
            view_name, query
        );
        self.execute_ddl(&sql).await
    }

    async fn downsample(&self, config: &DownsampleConfig) -> Result<(), TimescaleError> {
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
        let sql = format!("DROP TABLE IF EXISTS {}", metric);
        self.execute_ddl(&sql).await
    }
}
