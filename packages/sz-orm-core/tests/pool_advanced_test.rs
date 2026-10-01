//! v9.2.0 M15: pool.rs 高级分支覆盖

use std::time::Duration;
use sz_orm_core::{PoolConfig, PoolConfigBuilder, PoolMetrics, PoolStatus, PoolTuningAdvice};

#[test]
fn test_pool_metrics_empty() {
    let metrics = PoolMetrics {
        acquire_count: 0,
        acquire_wait_time: Duration::ZERO,
        connection_created_count: 0,
        ..Default::default()
    };
    assert_eq!(metrics.average_acquire_wait_time(), Duration::ZERO);
    assert_eq!(metrics.connection_reuse_rate(), 0.0);
}

#[test]
fn test_pool_metrics_normal() {
    let metrics = PoolMetrics {
        acquire_count: 10,
        acquire_wait_time: Duration::from_millis(500),
        connection_created_count: 3,
        ..Default::default()
    };
    assert_eq!(
        metrics.average_acquire_wait_time(),
        Duration::from_millis(50)
    );
    let reuse = metrics.connection_reuse_rate();
    assert!((reuse - 0.7).abs() < 0.01);
}

#[test]
fn test_pool_metrics_created_exceeds_acquire() {
    let metrics = PoolMetrics {
        acquire_count: 5,
        connection_created_count: 10,
        ..Default::default()
    };
    assert_eq!(metrics.connection_reuse_rate(), 0.0);
}

#[test]
fn test_pool_tuning_advice_is_optimal() {
    let advice = PoolTuningAdvice {
        suggested_max_size: None,
        suggested_min_idle: None,
        suggested_idle_timeout: None,
        reason: String::new(),
    };
    assert!(advice.is_optimal());
}

#[test]
fn test_pool_tuning_advice_not_optimal() {
    let advice = PoolTuningAdvice {
        suggested_max_size: Some(20),
        suggested_min_idle: None,
        suggested_idle_timeout: None,
        reason: String::new(),
    };
    assert!(!advice.is_optimal());
}

#[test]
fn test_pool_config_builder_basic() {
    let config = PoolConfigBuilder::new()
        .max_size(20)
        .min_idle(5)
        .acquire_timeout(10)
        .idle_timeout(300)
        .max_lifetime(1800)
        .build();
    assert!(config.is_ok());
    let cfg = config.unwrap();
    assert_eq!(cfg.max_size, 20);
}

#[test]
fn test_pool_config_validate_zero_max() {
    let result = PoolConfigBuilder::new().max_size(0).build();
    assert!(result.is_err());
}

#[test]
fn test_pool_config_validate_min_idle_exceeds_max() {
    let result = PoolConfigBuilder::new().max_size(5).min_idle(10).build();
    assert!(result.is_err());
}

#[test]
fn test_pool_config_builder_with_prewarm() {
    let config = PoolConfigBuilder::new().max_size(10).prewarm(true).build();
    assert!(config.is_ok());
}

#[test]
fn test_pool_config_default() {
    let config = PoolConfig::default();
    assert!(config.max_size > 0);
}

#[test]
fn test_pool_status_debug() {
    let status = PoolStatus {
        idle: 5,
        active: 3,
        max: 8,
        min: 2,
        waiters: 0,
    };
    let debug = format!("{:?}", status);
    assert!(debug.contains("idle"));
    assert!(debug.contains("active"));
}

#[test]
fn test_pool_metrics_full_reuse() {
    let metrics = PoolMetrics {
        acquire_count: 10,
        connection_created_count: 1,
        ..Default::default()
    };
    let reuse = metrics.connection_reuse_rate();
    assert!((reuse - 0.9).abs() < 0.01);
}

#[test]
fn test_pool_metrics_no_reuse() {
    let metrics = PoolMetrics {
        acquire_count: 10,
        connection_created_count: 10,
        ..Default::default()
    };
    let reuse = metrics.connection_reuse_rate();
    assert!((reuse - 0.0).abs() < 0.01);
}

#[test]
fn test_pool_config_builder_all_methods() {
    let config = PoolConfigBuilder::new()
        .max_size(50)
        .min_idle(10)
        .acquire_timeout(30)
        .idle_timeout(600)
        .max_lifetime(3600)
        .test_before_acquire(true)
        .build();
    assert!(config.is_ok());
}
