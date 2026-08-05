//! 真实 TimescaleDB 集成测试（需启用 `real-timescale` feature + 本机 TimescaleDB 扩展）
//!
//! 运行方式（需 PostgreSQL 已安装 TimescaleDB 扩展）：
//! ```bash
//! cargo test -p sz-orm-timeseries --features real-timescale --test real_timescale_tests -- --ignored
//! ```
//!
//! 连接参数可通过环境变量覆盖（默认 127.0.0.1:5432 / sz_orm_test / postgres / test123）：
//! - `SZ_ORM_PG_HOST` / `SZ_ORM_PG_PORT` / `SZ_ORM_PG_DB` / `SZ_ORM_PG_USER` / `SZ_ORM_PG_PASSWORD`

#![cfg(feature = "real-timescale")]

use chrono::{Duration, Utc};
use sz_orm_timeseries::{
    Aggregation, Metric, RealTimescaleConfig, TimeseriesBuilder, TimeseriesExt, TimeseriesProvider,
};

fn config() -> RealTimescaleConfig {
    RealTimescaleConfig {
        host: std::env::var("SZ_ORM_PG_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
        port: std::env::var("SZ_ORM_PG_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(5432),
        database: std::env::var("SZ_ORM_PG_DB").unwrap_or_else(|_| "sz_orm_test".to_string()),
        username: std::env::var("SZ_ORM_PG_USER").unwrap_or_else(|_| "postgres".to_string()),
        password: std::env::var("SZ_ORM_PG_PASSWORD").unwrap_or_else(|_| "test123".to_string()),
    }
}

/// 使用唯一指标名，避免并行测试冲突（real_timescale 中指标名即表名）
fn unique_metric(prefix: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("{}_{}_{}", prefix, std::process::id(), nanos)
}

#[tokio::test]
#[ignore]
async fn real_ts_hypertable_insert_query() {
    let w = TimeseriesBuilder::new(TimeseriesProvider::RealTimescale(config()))
        .build()
        .expect("build real provider");
    let metric = unique_metric("cpu_usage");

    w.create_hypertable(&metric, "ts")
        .await
        .expect("create hypertable");
    let now = Utc::now();
    w.insert_metric(&Metric::new(&metric, now, 0.75))
        .await
        .unwrap();

    let rows = w
        .query_range(
            &metric,
            now - Duration::minutes(1),
            now + Duration::minutes(1),
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].value, 0.75);

    w.drop_metric(&metric).await.expect("drop metric");
}

#[tokio::test]
#[ignore]
async fn real_ts_time_bucket_aggregate() {
    let w = TimeseriesBuilder::new(TimeseriesProvider::RealTimescale(config()))
        .build()
        .expect("build real provider");
    let metric = unique_metric("temp");

    w.create_hypertable(&metric, "ts")
        .await
        .expect("create hypertable");
    let now = Utc::now();
    for (i, v) in [1.0, 2.0, 3.0].iter().enumerate() {
        w.insert_metric(&Metric::new(&metric, now + Duration::seconds(i as i64), *v))
            .await
            .unwrap();
    }

    let buckets = w
        .time_bucket_aggregate(
            &metric,
            "1m",
            Aggregation::Avg,
            now,
            now + Duration::minutes(1),
        )
        .await
        .unwrap();
    assert_eq!(buckets.len(), 1);
    assert_eq!(buckets[0].count, 3);
    assert!(
        (buckets[0].avg - 2.0).abs() < 1e-6,
        "平均值为 2.0，实际 {}",
        buckets[0].avg
    );

    w.drop_metric(&metric).await.expect("drop metric");
}
