//! v10.5.0 性能基准测试
//!
//! 测量 T1-T6 性能优化的延迟，所有数值由代码采集（禁止手写）。
//! 运行：`cargo test -p sz-orm-bench --test v1050_perf_bench -- --nocapture`

use std::time::Instant;

// ============================================================================
// T1：批量 acquire 并行化配置基准
// ============================================================================

#[test]
fn bench_acquire_batch_parallel_config() {
    let start = Instant::now();
    for _ in 0..10000 {
        let config = sz_orm_core::AcquireBatchParallelConfig::new()
            .with_enabled(true)
            .with_threshold(8);
        let _ = config.threshold;
    }
    let elapsed = start.elapsed();
    println!(
        "T1 acquire_batch_parallel_config 10000次: {:?} (avg {:?})",
        elapsed,
        elapsed / 10000
    );
    assert!(elapsed.as_millis() < 100, "配置创建应在 100ms 内完成");
}

// ============================================================================
// T2：LRU-K evict 评分索引基准
// ============================================================================

#[test]
fn bench_lru_k_evict_with_index() {
    use sz_orm_core::plan_cache::LruKReplacer;

    let capacity = 1000;
    let mut replacer = LruKReplacer::new(2, capacity).with_evict_index(true);

    for i in 0..capacity as u64 {
        replacer.access(i);
    }

    let start = Instant::now();
    for i in 0..100 {
        let victim = replacer.evict();
        assert!(victim.is_some());
        replacer.access(capacity as u64 + i + 1);
    }
    let elapsed = start.elapsed();
    println!(
        "T2 lru_k_evict_with_index 100次 evict (capacity={}): {:?}",
        capacity, elapsed
    );
    assert!(elapsed.as_millis() < 500, "100次 evict 应在 500ms 内完成");
}

#[test]
fn bench_lru_k_evict_without_index() {
    use sz_orm_core::plan_cache::LruKReplacer;

    let capacity = 1000;
    let mut replacer = LruKReplacer::new(2, capacity).with_evict_index(false);

    for i in 0..capacity as u64 {
        replacer.access(i);
    }

    let start = Instant::now();
    for i in 0..100 {
        let victim = replacer.evict();
        assert!(victim.is_some());
        replacer.access(capacity as u64 + i + 1);
    }
    let elapsed = start.elapsed();
    println!(
        "T2 lru_k_evict_without_index 100次 evict (capacity={}): {:?}",
        capacity, elapsed
    );
    assert!(elapsed.as_millis() < 500, "100次 evict 应在 500ms 内完成");
}

// ============================================================================
// T4：零拷贝批量反序列化基准（需 serde-zero-copy feature）
// ============================================================================

#[cfg(feature = "serde-zero-copy")]
#[test]
fn bench_deserialize_batch() {
    use sz_orm_core::perf_extreme::{ZeroCopyDeserConfig, ZeroCopyDeserializer};

    let deserializer = ZeroCopyDeserializer::new(ZeroCopyDeserConfig::default());

    let rows: Vec<Vec<u8>> = (0..1000)
        .map(|i| {
            let mut row = vec![0x02];
            row.extend_from_slice(&(i as i64).to_be_bytes());
            row
        })
        .collect();
    let row_refs: Vec<&[u8]> = rows.iter().map(|r| r.as_slice()).collect();

    let start = Instant::now();
    let result = deserializer.deserialize_batch_borrowed(&row_refs);
    let elapsed = start.elapsed();

    println!(
        "T4 deserialize_batch 1000行: {:?} (avg {:?})",
        elapsed,
        elapsed / 1000
    );
    assert_eq!(result.len(), 1000);
    assert!(elapsed.as_millis() < 100, "1000行反序列化应在 100ms 内完成");
}

// ============================================================================
// T5：SIMD 阈值自适应基准（需 query-simd feature）
// ============================================================================

#[cfg(feature = "query-simd")]
#[test]
fn bench_simd_threshold_adaptive() {
    use sz_orm_core::perf_extreme::{SimdThresholdAdaptive, SimdThresholdAdaptiveConfig};

    let config = SimdThresholdAdaptiveConfig::new()
        .with_enabled(true)
        .with_history_min(1);
    let adaptive = SimdThresholdAdaptive::new(config);

    let start = Instant::now();
    for i in 0..10000 {
        adaptive.record_speedup(if i % 2 == 0 { 0.5 } else { 2.0 });
        let _ = adaptive.get_threshold();
    }
    let elapsed = start.elapsed();

    println!(
        "T5 simd_threshold_adaptive 10000次 record+get: {:?}",
        elapsed
    );
    assert!(
        elapsed.as_millis() < 100,
        "10000次自适应阈值更新应在 100ms 内完成"
    );
}

// ============================================================================
// T6：动态批量决策自适应基准基准
// ============================================================================

#[test]
fn bench_batch_advisor_adaptive_baseline() {
    use sz_orm_core::{AdaptiveBaselineConfig, DynamicBatchAdvisor, DynamicBatchConfig};

    let baseline_config = AdaptiveBaselineConfig::new()
        .with_enabled(true)
        .with_history_min(3);
    let advisor = DynamicBatchAdvisor::new(DynamicBatchConfig::default())
        .with_adaptive_baseline_config(baseline_config);

    let start = Instant::now();
    for i in 0..1000 {
        advisor.record_execution(500, 1000.0 + i as f64, 100.0);
        let result = advisor.advise_dynamic(sz_orm_core::dialect::DbType::MySQL, 2, 10000);
        assert!(result > 0, "建议值应 > 0");
    }
    let elapsed = start.elapsed();

    println!(
        "T6 batch_advisor_adaptive_baseline 1000次 record+advise: {:?}",
        elapsed
    );
    assert!(
        elapsed.as_millis() < 500,
        "1000次动态批量建议应在 500ms 内完成"
    );
}

#[test]
fn bench_adaptive_baseline_median_computation() {
    use sz_orm_core::{AdaptiveBaseline, AdaptiveBaselineConfig};

    let config = AdaptiveBaselineConfig::new().with_history_min(10);
    let baseline = AdaptiveBaseline::new(config);

    for i in 0..100 {
        baseline.record(1000.0 + i as f64, 100.0 + i as f64);
    }

    let start = Instant::now();
    for _ in 0..10000 {
        let tp = baseline.get_tp_baseline();
        let lat = baseline.get_lat_baseline();
        assert!(tp > 0.0 && lat > 0.0);
    }
    let elapsed = start.elapsed();

    println!(
        "T6 adaptive_baseline_median 10000次 get_tp+get_lat: {:?}",
        elapsed
    );
    assert!(
        elapsed.as_millis() < 100,
        "10000次中位数计算应在 100ms 内完成"
    );
}
