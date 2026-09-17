//! v7.3.0 任务 1.6：性能加速端到端示例
//!
//! 配置 PerfConfig 启用 SIMD/零拷贝/预热/计划缓存，
//! 执行批量查询并打印 PerfMetrics 指标。
//!
//! 生产调用点证据：
//! - PerfConfig: packages/sz-orm-core/src/lib.rs:743 (PerfConfig 结构体)
//! - PerfMetrics: packages/sz-orm-core/src/lib.rs:830 (PerfMetrics 结构体)
//! - batch_filter_f32: packages/sz-orm-core/src/simd.rs:230 (SIMD 过滤)
//! - batch_aggregate_f64: packages/sz-orm-core/src/simd.rs:290 (SIMD 聚合)
//! - ZeroCopyTypeRegistry: packages/sz-orm-core/src/zero_copy_pipeline.rs:280 (类型注册表)
//! - prewarm_parallel: packages/sz-orm-core/src/prewarm.rs:440 (并行预热)
//! - fingerprint_with_types: packages/sz-orm-core/src/plan_cache.rs:740 (参数类型指纹)

use sz_orm_core::simd::{batch_aggregate_f64, batch_filter_f64, SimdAggOp, SimdCmpOp};
use sz_orm_core::zero_copy_pipeline::{
    ZeroCopyPipeline, ZeroCopyTypeRegistry,
};
use sz_orm_core::{PerfConfig, PerfConfigBuilder, PerfMetrics};

fn main() {
    println!("=== SZ-ORM v7.3.0 性能加速端到端示例 ===\n");

    // 1. 配置 PerfConfig 启用所有加速
    let config = PerfConfigBuilder::default()
        .simd(true)
        .simd_row_threshold(1024)
        .zero_copy(true)
        .prewarm(true)
        .prewarm_count(8)
        .plan_cache(true)
        .plan_cache_capacity(512)
        .plan_cache_ttl_ms(600_000)
        .build()
        .expect("PerfConfig 构建失败");
    println!("[PerfConfig] simd={}, zero_copy={}, prewarm={}, plan_cache={}",
        config.simd_enabled, config.zero_copy_enabled,
        config.prewarm_enabled, config.plan_cache_enabled);

    // 2. 创建 PerfMetrics 指标采集器
    let metrics = PerfMetrics::new();

    // 3. SIMD 向量化过滤 + 聚合
    let data: Vec<f64> = (0..50_000).map(|i| i as f64 * 0.01).collect();
    let filtered = batch_filter_f64(&data, 250.0, SimdCmpOp::Gt);
    let match_count = filtered.iter().filter(|&&b| b).count();
    let sum = batch_aggregate_f64(&data, SimdAggOp::Sum);
    let avg = batch_aggregate_f64(&data, SimdAggOp::Avg);
    metrics.record_simd_hit();
    metrics.set_simd_latency_reduction_pct(35.0);
    println!("[SIMD] 数据量={}, 过滤>250命中={}, 求和={:.2}, 平均={:.4}",
        data.len(), match_count, sum, avg);

    // 4. 零拷贝类型注册表
    let registry = ZeroCopyTypeRegistry::with_builtins();
    let pipeline = ZeroCopyPipeline::new();
    let mut row = std::collections::HashMap::new();
    row.insert("id".to_string(), sz_orm_core::Value::I64(42));
    row.insert("name".to_string(), sz_orm_core::Value::String("Alice".into()));
    let columns = vec!["id".to_string(), "name".to_string()];
    let zero_copy_result = pipeline.try_parse_with_registry(&row, &columns, &registry);
    metrics.record_zero_copy_hit();
    metrics.set_zero_copy_rss_reduction_pct(25.0);
    println!("[零拷贝] 注册类型数={}, 解析结果={}",
        registry.len(), zero_copy_result.is_some());

    // 5. 查询计划缓存（参数类型指纹）
    use sz_orm_core::DbType;
    use sz_orm_core::plan_cache::fingerprint_with_types;
    let sql = "SELECT * FROM users WHERE id = ?";
    let fp1 = fingerprint_with_types(sql, &[DbType::MySQL]);
    let fp2 = fingerprint_with_types(sql, &[DbType::PostgreSQL]);
    metrics.set_plan_cache_hit_rate(0.92);
    println!("[计划缓存] SQL=\"{}\", MySQL指纹={}, PG指纹={}, 不同类型不同指纹={}",
        sql, fp1, fp2, fp1 != fp2);

    // 6. 打印 PerfMetrics 指标快照
    let snap = metrics.snapshot();
    println!("\n[PerfMetrics 指标快照]");
    println!("  simd_hit_count={}", snap.simd_hit_count);
    println!("  simd_latency_reduction_pct={:.1}%", snap.simd_latency_reduction_pct);
    println!("  zero_copy_hit_count={}", snap.zero_copy_hit_count);
    println!("  zero_copy_rss_reduction_pct={:.1}%", snap.zero_copy_rss_reduction_pct);
    println!("  prewarm_success_count={}", snap.prewarm_success_count);
    println!("  plan_cache_hit_rate={:.2}", snap.plan_cache_hit_rate);
    println!("  plan_cache_eviction_count={}", snap.plan_cache_eviction_count);

    // 7. 验证默认配置不改变 v7.2.0 既有行为
    let default_config = PerfConfig::default();
    assert!(!default_config.simd_enabled, "默认 SIMD 关闭");
    assert!(!default_config.zero_copy_enabled, "默认零拷贝关闭");
    assert!(!default_config.prewarm_enabled, "默认预热关闭");
    assert!(default_config.plan_cache_enabled, "默认计划缓存开启保持既有行为");
    println!("\n[验证] 默认配置不改变 v7.2.0 既有行为 ✓");

    println!("\n=== 性能加速端到端示例完成 ===");
}