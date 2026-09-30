//! v9.2.0 M18: pool.rs 未覆盖 builder/runtime 方法覆盖

mod common;

use common::pool_mock;
use std::time::Duration;
use sz_orm_core::{PoolConfigBuilder, TlsConfig, TlsVersion};

// ---- PoolConfigBuilder builder setters ----

#[test]
fn test_builder_tls() {
    let tls = TlsConfig {
        enabled: true,
        ca_cert_path: Some("/etc/ssl/ca.pem".to_string()),
        client_cert_path: None,
        client_key_path: None,
        min_version: TlsVersion::Tls13,
    };
    let config = PoolConfigBuilder::new().tls(tls).build().unwrap();
    assert!(config.tls.is_some());
    let tls = config.tls.as_ref().unwrap();
    assert!(tls.enabled);
    assert_eq!(tls.min_version, TlsVersion::Tls13);
}

#[test]
fn test_builder_query_timeout() {
    let config = PoolConfigBuilder::new()
        .query_timeout(Duration::from_secs(15))
        .build()
        .unwrap();
    assert_eq!(config.query_timeout, Some(Duration::from_secs(15)));
}

#[test]
fn test_builder_max_rows() {
    let config = PoolConfigBuilder::new().max_rows(1000).build().unwrap();
    assert_eq!(config.max_rows, Some(1000));
}

#[test]
fn test_builder_memory_limit() {
    let config = PoolConfigBuilder::new()
        .memory_limit(512 * 1024 * 1024)
        .build()
        .unwrap();
    assert_eq!(config.memory_limit, Some(512 * 1024 * 1024));
}

#[test]
fn test_builder_with_adaptive_tuning() {
    let config = PoolConfigBuilder::new()
        .with_adaptive_tuning(30, 180, 5000)
        .build()
        .unwrap();
    assert_eq!(config.max_size, 30);
    assert_eq!(config.idle_timeout, Duration::from_secs(180));
    assert_eq!(config.acquire_timeout, Duration::from_millis(5000));
}

#[test]
fn test_builder_with_adaptive_tuning_zero_capacity() {
    let config = PoolConfigBuilder::new()
        .with_adaptive_tuning(0, 60, 3000)
        .build()
        .unwrap();
    assert_eq!(config.max_size, 100);
    assert_eq!(config.idle_timeout, Duration::from_secs(60));
    assert_eq!(config.acquire_timeout, Duration::from_millis(3000));
}

#[test]
fn test_builder_chained_setters() {
    let config = PoolConfigBuilder::new()
        .max_size(50)
        .min_idle(5)
        .query_timeout(Duration::from_secs(10))
        .max_rows(500)
        .memory_limit(256 * 1024 * 1024)
        .build()
        .unwrap();
    assert_eq!(config.max_size, 50);
    assert_eq!(config.min_idle, 5);
    assert_eq!(config.query_timeout, Some(Duration::from_secs(10)));
    assert_eq!(config.max_rows, Some(500));
    assert_eq!(config.memory_limit, Some(256 * 1024 * 1024));
}

// ---- Pool::metrics_snapshot_json ----

#[test]
fn test_pool_metrics_snapshot_json_valid() {
    let pool = pool_mock::create_pool(4);
    let json = pool.metrics_snapshot_json();
    assert!(json.starts_with('{'));
    assert!(json.ends_with('}'));
}

#[test]
fn test_pool_metrics_snapshot_json_after_activity() {
    let pool = pool_mock::create_pool(4);
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let conn = pool.acquire().await.unwrap();
        pool.release(conn).await;
    });
    let json = pool.metrics_snapshot_json();
    assert!(json.starts_with('{'));
    assert!(json.contains("acquire_count") || json.contains("acquire"));
}

// ---- PooledConnection::created_at ----

#[test]
fn test_pooled_connection_created_at() {
    let pool = pool_mock::create_pool(4);
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let conn = pool.acquire().await.unwrap();
        let _instant = conn.created_at();
    });
}

// ---- Pool::query_with_timeout ----

#[test]
fn test_pool_query_with_timeout_success() {
    let pool = pool_mock::create_pool(4);
    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(async { pool.query_with_timeout("SELECT 1").await });
    assert!(result.is_ok());
}

#[test]
fn test_pool_query_with_timeout_returns_rows() {
    let pool = pool_mock::create_pool(4);
    let rt = tokio::runtime::Runtime::new().unwrap();
    let rows = rt
        .block_on(async { pool.query_with_timeout("SELECT 1").await })
        .unwrap();
    assert!(rows.is_empty());
}

// ---- PoolConfigBuilder::tls default ----

#[test]
fn test_builder_tls_default_is_none() {
    let config = PoolConfigBuilder::new().build().unwrap();
    assert!(config.tls.is_none());
}

#[test]
fn test_builder_query_timeout_default_is_some() {
    let config = PoolConfigBuilder::new().build().unwrap();
    assert_eq!(config.query_timeout, Some(Duration::from_secs(30)));
}

#[test]
fn test_builder_max_rows_default_is_none() {
    let config = PoolConfigBuilder::new().build().unwrap();
    assert!(config.max_rows.is_none());
}

#[test]
fn test_builder_memory_limit_default_is_none() {
    let config = PoolConfigBuilder::new().build().unwrap();
    assert!(config.memory_limit.is_none());
}