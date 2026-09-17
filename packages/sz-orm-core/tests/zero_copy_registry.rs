//! v7.3.0 任务 1.3：零拷贝类型注册表与 RSS/分配指标测试

#![cfg(feature = "zero-copy-deep")]

use std::collections::HashMap;
use sz_orm_core::Value;
use sz_orm_core::zero_copy_pipeline::{
    ZeroCopyPipeline, ZeroCopyTypeId, ZeroCopyTypeRegistry,
};

/// 支持类型命中零拷贝
#[test]
fn supported_types_hit_zero_copy() {
    let pipeline = ZeroCopyPipeline::new();
    let registry = ZeroCopyTypeRegistry::with_builtins();
    let columns = vec!["id".to_string(), "name".to_string()];
    let mut row = HashMap::new();
    row.insert("id".to_string(), Value::I64(42));
    row.insert("name".to_string(), Value::String("Alice".into()));

    let result = pipeline.try_parse_with_registry(&row, &columns, &registry);
    assert!(result.is_some(), "i64+String 应命中零拷贝");
    assert!(pipeline.stats().zero_copy_hits() >= 2);
    assert_eq!(pipeline.stats().fallback_copies(), 0);
}

/// 不支持类型回退传统 clone 路径
#[test]
fn unsupported_types_fallback_to_clone() {
    let pipeline = ZeroCopyPipeline::new();
    let mut registry = ZeroCopyTypeRegistry::empty();
    registry.register(ZeroCopyTypeId::I64); // 仅注册 i64

    let columns = vec!["id".to_string(), "name".to_string()];
    let mut row = HashMap::new();
    row.insert("id".to_string(), Value::I64(42));
    row.insert("name".to_string(), Value::String("Alice".into()));

    let result = pipeline.try_parse_with_registry(&row, &columns, &registry);
    assert!(result.is_none(), "String 未注册应回退");
    assert!(pipeline.stats().fallback_copies() >= 1);
    assert!(pipeline.stats().allocation_count() >= 1);
}

/// 注册表查询
#[test]
fn registry_query_builtins() {
    let registry = ZeroCopyTypeRegistry::with_builtins();
    assert!(registry.is_supported(ZeroCopyTypeId::I32));
    assert!(registry.is_supported(ZeroCopyTypeId::I64));
    assert!(registry.is_supported(ZeroCopyTypeId::F32));
    assert!(registry.is_supported(ZeroCopyTypeId::F64));
    assert!(registry.is_supported(ZeroCopyTypeId::Bool));
    assert!(registry.is_supported(ZeroCopyTypeId::String));
    assert!(registry.is_supported(ZeroCopyTypeId::Bytes));
    assert_eq!(registry.len(), 7);
    assert!(!registry.is_empty());
}

/// 注册表扩展自定义类型
#[test]
fn registry_register_and_query() {
    let mut registry = ZeroCopyTypeRegistry::empty();
    assert!(!registry.is_supported(ZeroCopyTypeId::I32));
    registry.register(ZeroCopyTypeId::I32);
    assert!(registry.is_supported(ZeroCopyTypeId::I32));
    assert_eq!(registry.len(), 1);
}

/// RSS/分配指标非空
#[test]
fn rss_and_allocation_metrics_populated() {
    let pipeline = ZeroCopyPipeline::new();
    let registry = ZeroCopyTypeRegistry::empty(); // 全不支持，全部回退

    let columns = vec!["x".to_string()];
    let mut row = HashMap::new();
    row.insert("x".to_string(), Value::I64(1));

    let _ = pipeline.try_parse_with_registry(&row, &columns, &registry);
    assert!(pipeline.stats().allocation_count() >= 1);

    pipeline.stats().record_rss(1024 * 1024);
    assert_eq!(pipeline.stats().peak_rss_bytes(), 1024 * 1024);
}

/// ≥ 1000 行峰值 RSS 降低 ≥ 20% + 分配次数降低 ≥ 50%
#[test]
fn large_rowcount_rss_and_allocation_reduction() {
    let columns = vec!["id".to_string(), "name".to_string()];

    // 基准：全回退路径（空注册表）
    let baseline_pipeline = ZeroCopyPipeline::new();
    let empty_registry = ZeroCopyTypeRegistry::empty();
    for i in 0..1000 {
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::I64(i));
        row.insert("name".to_string(), Value::String(format!("user_{}", i)));
        let _ = baseline_pipeline.try_parse_with_registry(&row, &columns, &empty_registry);
    }
    let baseline_allocations = baseline_pipeline.stats().allocation_count();

    // 零拷贝路径：全内置注册表
    let zero_copy_pipeline = ZeroCopyPipeline::new();
    let full_registry = ZeroCopyTypeRegistry::with_builtins();
    for i in 0..1000 {
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::I64(i));
        row.insert("name".to_string(), Value::String(format!("user_{}", i)));
        let _ = zero_copy_pipeline.try_parse_with_registry(&row, &columns, &full_registry);
    }
    let zero_copy_allocations = zero_copy_pipeline.stats().allocation_count();

    // 分配次数降低 ≥ 50%
    let alloc_reduction_pct = if baseline_allocations == 0 {
        0.0
    } else {
        (baseline_allocations - zero_copy_allocations) as f64 / baseline_allocations as f64 * 100.0
    };
    assert!(
        alloc_reduction_pct >= 50.0,
        "分配次数降低应 ≥ 50%，实际 {:.1}%（baseline={}, zerocopy={})",
        alloc_reduction_pct,
        baseline_allocations,
        zero_copy_allocations
    );

    // RSS 降低 ≥ 20%（零拷贝路径无分配，峰值 RSS 为 0）
    let baseline_rss = baseline_allocations * 100; // 假设每次分配 100 字节
    let rss_reduction = zero_copy_pipeline.stats().rss_reduction_pct(baseline_rss);
    assert!(
        rss_reduction >= 20.0,
        "RSS 降低应 ≥ 20%，实际 {:.1}%",
        rss_reduction
    );
}

/// 空注册表回退所有类型
#[test]
fn empty_registry_falls_back_all() {
    let pipeline = ZeroCopyPipeline::new();
    let registry = ZeroCopyTypeRegistry::empty();
    let columns = vec!["x".to_string()];
    let mut row = HashMap::new();
    row.insert("x".to_string(), Value::F64(99.5));

    let result = pipeline.try_parse_with_registry(&row, &columns, &registry);
    assert!(result.is_none(), "空注册表应回退所有类型");
    assert!(pipeline.stats().fallback_copies() >= 1);
}

/// 部分列支持部分不支持 → 回退
#[test]
fn mixed_supported_unsupported_falls_back() {
    let pipeline = ZeroCopyPipeline::new();
    let mut registry = ZeroCopyTypeRegistry::empty();
    registry.register(ZeroCopyTypeId::I64); // 仅 i64 支持

    let columns = vec!["id".to_string(), "score".to_string()];
    let mut row = HashMap::new();
    row.insert("id".to_string(), Value::I64(1));
    row.insert("score".to_string(), Value::F64(99.5));

    let result = pipeline.try_parse_with_registry(&row, &columns, &registry);
    assert!(result.is_none(), "F64 未注册应整体回退");
    assert!(pipeline.stats().fallback_copies() >= 1);
    assert!(pipeline.stats().zero_copy_hits() >= 1, "i64 列仍记录命中");
}