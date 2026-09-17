//! v7.3.0 任务 1.1：PerfConfig/PerfMetrics 基础设施测试
//!
//! 验证默认值、校验失败、Send+Sync、指标采集。

#![cfg(feature = "perf-accel")]

use sz_orm_core::{DbError, PerfConfig, PerfConfigBuilder, PerfMetrics};


/// 验证默认值：加速全 false、plan_cache true
#[test]
fn perf_config_defaults_all_off_except_plan_cache() {
    let cfg = PerfConfig::default();
    assert!(!cfg.simd_enabled, "simd 默认关闭");
    assert!(!cfg.zero_copy_enabled, "zero_copy 默认关闭");
    assert!(!cfg.prewarm_enabled, "prewarm 默认关闭");
    assert!(cfg.plan_cache_enabled, "plan_cache 默认开启保持既有行为");
    assert_eq!(cfg.simd_row_threshold, 1024);
    assert_eq!(cfg.prewarm_count, 1);
    assert_eq!(cfg.plan_cache_capacity, 256);
    assert_eq!(cfg.plan_cache_ttl_ms, 300_000);
}

/// 验证校验失败返回 DbError::ConfigError
#[test]
fn perf_config_validate_failures_return_config_error() {
    let bad = PerfConfig {
        simd_row_threshold: 0,
        ..PerfConfig::default()
    };
    let err = bad.validate().unwrap_err();
    assert!(matches!(err, DbError::ConfigError(_)), "simd_row_threshold=0 应返回 ConfigError");

    let bad = PerfConfig {
        prewarm_count: 0,
        ..PerfConfig::default()
    };
    assert!(matches!(bad.validate().unwrap_err(), DbError::ConfigError(_)));

    let bad = PerfConfig {
        plan_cache_capacity: 0,
        ..PerfConfig::default()
    };
    assert!(matches!(bad.validate().unwrap_err(), DbError::ConfigError(_)));

    let bad = PerfConfig {
        plan_cache_ttl_ms: 0,
        ..PerfConfig::default()
    };
    assert!(matches!(bad.validate().unwrap_err(), DbError::ConfigError(_)));
}

/// 验证合法配置校验通过
#[test]
fn perf_config_validate_valid_passes() {
    let cfg = PerfConfig::default();
    assert!(cfg.validate().is_ok(), "默认配置应校验通过");

    let cfg = PerfConfigBuilder::default()
        .simd(true)
        .simd_row_threshold(512)
        .zero_copy(true)
        .prewarm(true)
        .prewarm_count(8)
        .plan_cache(true)
        .plan_cache_capacity(1024)
        .plan_cache_ttl_ms(600_000)
        .build()
        .expect("合法配置应构建成功");
    assert!(cfg.simd_enabled);
    assert!(cfg.zero_copy_enabled);
    assert!(cfg.prewarm_enabled);
    assert_eq!(cfg.prewarm_count, 8);
}

/// 验证 Send + Sync
#[test]
fn perf_config_and_metrics_are_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<PerfConfig>();
    assert_send_sync::<PerfMetrics>();
    assert_send_sync::<sz_orm_core::PerfMetricsSnapshot>();
}

/// 验证 PerfMetrics 指标采集
#[test]
fn perf_metrics_snapshot_collects_counters() {
    let metrics = PerfMetrics::new();
    metrics.record_simd_hit();
    metrics.record_simd_hit();
    metrics.record_simd_miss();
    metrics.record_zero_copy_hit();
    metrics.record_prewarm_success();
    metrics.record_prewarm_success();
    metrics.record_plan_cache_eviction();
    metrics.set_simd_latency_reduction_pct(35.5);
    metrics.set_zero_copy_rss_reduction_pct(22.0);
    metrics.set_plan_cache_hit_rate(0.85);

    let snap = metrics.snapshot();
    assert_eq!(snap.simd_hit_count, 2);
    assert_eq!(snap.simd_miss_count, 1);
    assert_eq!(snap.zero_copy_hit_count, 1);
    assert_eq!(snap.prewarm_success_count, 2);
    assert_eq!(snap.plan_cache_eviction_count, 1);
    assert!((snap.simd_latency_reduction_pct - 35.5).abs() < 0.01);
    assert!((snap.zero_copy_rss_reduction_pct - 22.0).abs() < 0.01);
    assert!((snap.plan_cache_hit_rate - 0.85).abs() < 0.001);
}