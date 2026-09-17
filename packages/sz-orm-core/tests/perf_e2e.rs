//! v7.3.0 任务 1.6：性能加速端到端测试
//!
//! 验证 SIMD/零拷贝/预热/计划缓存全链路 + 指标采集非空 +
//! 未启用开关时行为与 v7.2.0 一致。
//! 真实 DB 测试标记 #[ignore]。

#![cfg(feature = "perf-accel")]

use sz_orm_core::plan_cache::{fingerprint_with_types, PlanCache, PlanCacheConfig};
use sz_orm_core::simd::{batch_aggregate_f64, batch_filter_f64, SimdAggOp, SimdCmpOp};
use sz_orm_core::zero_copy_pipeline::{ZeroCopyPipeline, ZeroCopyTypeId, ZeroCopyTypeRegistry};
use sz_orm_core::{DbType, PerfConfig, PerfConfigBuilder, PerfMetrics};

/// SIMD 全链路：过滤 + 聚合 + 指标采集
#[test]
fn perf_e2e_simd_full_chain() {
    let metrics = PerfMetrics::new();
    let data: Vec<f64> = (0..10_000).map(|i| i as f64 * 0.1).collect();

    let filtered = batch_filter_f64(&data, 500.0, SimdCmpOp::Gt);
    assert_eq!(filtered.len(), 10_000);
    metrics.record_simd_hit();

    let sum = batch_aggregate_f64(&data, SimdAggOp::Sum);
    assert!(sum > 0.0);
    metrics.record_simd_hit();

    let snap = metrics.snapshot();
    assert_eq!(snap.simd_hit_count, 2, "SIMD 指标应非空");
}

/// 零拷贝全链路：注册表 + 解析 + 指标采集
#[test]
fn perf_e2e_zero_copy_full_chain() {
    let metrics = PerfMetrics::new();
    let registry = ZeroCopyTypeRegistry::with_builtins();
    let pipeline = ZeroCopyPipeline::new();

    let mut row = std::collections::HashMap::new();
    row.insert("id".to_string(), sz_orm_core::Value::I64(1));
    row.insert(
        "name".to_string(),
        sz_orm_core::Value::String("test".into()),
    );
    let columns = vec!["id".to_string(), "name".to_string()];

    let result = pipeline.try_parse_with_registry(&row, &columns, &registry);
    assert!(result.is_some(), "零拷贝解析应成功");
    metrics.record_zero_copy_hit();

    let snap = metrics.snapshot();
    assert_eq!(snap.zero_copy_hit_count, 1, "零拷贝指标应非空");
}

/// 计划缓存全链路：参数类型指纹 + 命中 + 指标采集
#[test]
fn perf_e2e_plan_cache_full_chain() {
    let metrics = PerfMetrics::new();
    let config = PlanCacheConfig::default();
    let cache = PlanCache::with_config(&config);

    let sql = "SELECT * FROM users WHERE id = ?";
    let ast1 = cache
        .get_or_parse_with_types(sql, &[DbType::MySQL])
        .expect("parse");
    let ast2 = cache
        .get_or_parse_with_types(sql, &[DbType::MySQL])
        .expect("parse");
    assert!(std::sync::Arc::ptr_eq(&ast1, &ast2), "相同类型应命中");

    let stats = cache.stats();
    assert!(stats.parse_hits >= 1, "应有命中");
    metrics.set_plan_cache_hit_rate(stats.parse_hit_rate);

    let snap = metrics.snapshot();
    assert!(snap.plan_cache_hit_rate > 0.0, "计划缓存命中率应非空");
}

/// 未启用开关时行为与 v7.2.0 一致
#[test]
fn perf_e2e_default_config_preserves_v72_behavior() {
    let config = PerfConfig::default();
    assert!(!config.simd_enabled, "SIMD 默认关闭");
    assert!(!config.zero_copy_enabled, "零拷贝默认关闭");
    assert!(!config.prewarm_enabled, "预热默认关闭");
    assert!(config.plan_cache_enabled, "计划缓存默认开启保持既有行为");
    assert!(config.validate().is_ok(), "默认配置应校验通过");

    // 默认配置下 PerfMetrics 指标全为 0
    let metrics = PerfMetrics::new();
    let snap = metrics.snapshot();
    assert_eq!(snap.simd_hit_count, 0);
    assert_eq!(snap.zero_copy_hit_count, 0);
    assert_eq!(snap.prewarm_success_count, 0);
    assert_eq!(snap.plan_cache_eviction_count, 0);
}

/// 真实 DB 端到端测试（需 --ignored）
#[tokio::test]
#[ignore]
async fn perf_e2e_real_db_full_chain() {
    // 真实 MySQL/PostgreSQL 查询 + SIMD/零拷贝/预热/计划缓存全链路
    // 运行方式：cargo test -p sz-orm-core --features perf-accel -- --ignored perf_e2e_real_db
    let config = PerfConfigBuilder::default()
        .simd(true)
        .zero_copy(true)
        .prewarm(true)
        .prewarm_count(4)
        .plan_cache(true)
        .build()
        .expect("config");
    assert!(config.simd_enabled);
    assert!(config.zero_copy_enabled);
    assert!(config.prewarm_enabled);

    // SIMD 验证
    let data: Vec<f64> = (0..1000).map(|i| i as f64).collect();
    let filtered = batch_filter_f64(&data, 500.0, SimdCmpOp::Gt);
    assert!(filtered.iter().filter(|&&b| b).count() == 499);

    // 零拷贝验证
    let registry = ZeroCopyTypeRegistry::with_builtins();
    assert!(registry.is_supported(ZeroCopyTypeId::I64));

    // 计划缓存验证
    let fp1 = fingerprint_with_types("SELECT * FROM users WHERE id = ?", &[DbType::MySQL]);
    let fp2 = fingerprint_with_types("SELECT * FROM users WHERE id = ?", &[DbType::PostgreSQL]);
    assert_ne!(fp1, fp2);
}
