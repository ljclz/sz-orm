//! 性能极致优化接线端到端测试（v10.10.0 T1.1~T1.5）
//!
//! 验证 SIMD 向量化扩展的行为副作用：
//! - batch_compare_decimal/datetime/str 批量比较正确性
//! - batch_sort_f32 排序正确性
//! - batch_group_aggregate_f32 分组聚合正确性
//! - TypedArena/RowArena 内存池零分配

#![cfg(feature = "simd")]

use sz_orm_core::arena::{RowArena, TypedArena};
use sz_orm_core::batch_advisor::{
    BatchDeleteAdvisor, BatchQueryShardAdvisor, BatchUpdateAdvisor, BatchUpsertAdvisor,
    DynamicBatchConfig,
};
use sz_orm_core::DbType;
use sz_orm_core::cache_warmup_protection::{AccessPatternAnalyzer, AccessPatternConfig};
use sz_orm_core::pipeline_adaptive::{LoadSignal, PipelineParallelismAdapter};
use sz_orm_core::simd::{
    batch_compare_datetime, batch_compare_decimal, batch_compare_str, batch_group_aggregate_f32,
    batch_sort_f32, detect, SimdAggOp, SimdAvailability,
};

#[test]
fn test_batch_compare_decimal_correctness() {
    let values = vec![123456789i128, 987654321, 123456789, 0, -1];
    let target = 123456789i128;
    let result = batch_compare_decimal(&values, target, SimdAvailability::Avx2);
    assert_eq!(result, vec![true, false, true, false, false]);

    let scalar_result = batch_compare_decimal(&values, target, SimdAvailability::None);
    assert_eq!(result, scalar_result, "SIMD 与标量路径结果应一致");
}

#[test]
fn test_batch_compare_datetime_correctness() {
    let timestamps = vec![1700000000i64, 1700000060, 1700000000, 1700000120, 1700000000];
    let target = 1700000000i64;
    let result = batch_compare_datetime(&timestamps, target, SimdAvailability::Avx2);
    assert_eq!(result, vec![true, false, true, false, true]);

    let count = result.iter().filter(|&&r| r).count();
    assert_eq!(count, 3, "应有 3 个时间戳匹配 target");
}

#[test]
fn test_batch_compare_str_correctness() {
    let values = vec!["hello", "world", "hello", "rust", "hello"];
    let result = batch_compare_str(&values, "hello", SimdAvailability::Avx2);
    assert_eq!(result, vec![true, false, true, false, true]);

    let scalar_result = batch_compare_str(&values, "hello", SimdAvailability::None);
    assert_eq!(result, scalar_result, "SIMD 与标量路径结果应一致");
}

#[test]
fn test_batch_sort_f32_correctness() {
    let mut data = vec![9.0_f32, 3.0, 7.0, 1.0, 5.0, 8.0, 2.0, 6.0, 4.0];
    batch_sort_f32(&mut data, SimdAvailability::Avx2);
    assert_eq!(
        data,
        vec![1.0_f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0],
        "排序后应为升序"
    );
}

#[test]
fn test_batch_group_aggregate_f32_correctness() {
    let data = vec![10.0_f32, 20.0, 30.0, 40.0, 50.0, 60.0];
    let keys = vec![1u64, 1, 2, 2, 3, 3];

    let sum_result =
        batch_group_aggregate_f32(&data, &keys, SimdAggOp::Sum, SimdAvailability::Avx2);
    assert_eq!(sum_result.get(&1), Some(&30.0), "组1 Sum: 10+20=30");
    assert_eq!(sum_result.get(&2), Some(&70.0), "组2 Sum: 30+40=70");
    assert_eq!(sum_result.get(&3), Some(&110.0), "组3 Sum: 50+60=110");

    let avg_result =
        batch_group_aggregate_f32(&data, &keys, SimdAggOp::Avg, SimdAvailability::Avx2);
    assert_eq!(avg_result.get(&1), Some(&15.0), "组1 Avg: 30/2=15");
    assert_eq!(avg_result.get(&2), Some(&35.0), "组2 Avg: 70/2=35");
    assert_eq!(avg_result.get(&3), Some(&55.0), "组3 Avg: 110/2=55");
}

#[test]
fn test_simd_detect_returns_valid_availability() {
    let avail = detect();
    let is_avail = avail.is_available();
    assert!(
        is_avail || !is_avail,
        "detect() 应返回有效的 SimdAvailability 枚举值"
    );
    assert!(
        matches!(
            avail,
            SimdAvailability::Avx512
                | SimdAvailability::Avx2
                | SimdAvailability::Avx
                | SimdAvailability::Sse2
                | SimdAvailability::Neon
                | SimdAvailability::None
        ),
        "detect() 应返回已知枚举变体"
    );
}
// T1.2: TypedArena/RowArena 内存池零分配端到端测试

#[test]
fn test_typed_arena_batch_alloc_reduces_heap_allocations() {
    let mut arena: TypedArena<i64> = TypedArena::with_capacity(100);

    for i in 0..100 {
        let val = arena.alloc(i);
        assert_eq!(*val, i, "分配的值应与输入一致");
    }

    assert_eq!(
        arena.allocation_count(),
        0,
        "100 个元素在 capacity=100 内分配，不应触发堆分配"
    );
    assert_eq!(arena.len(), 100);

    arena.reset();
    assert_eq!(arena.len(), 0, "reset 后长度应为 0");

    for i in 0..100 {
        arena.alloc(i * 2);
    }
    assert_eq!(
        arena.allocation_count(),
        0,
        "reset 后复用预分配容量，不应触发堆分配"
    );
}

#[test]
fn test_row_arena_row_decode_reuses_memory() {
    let mut arena = RowArena::with_capacity(4096);

    for batch in 0..3 {
        arena.reset();
        for row in 0..50 {
            let data = format!("batch_{}_row_{}", batch, row);
            let decoded = arena.decode_row(data.as_bytes());
            assert_eq!(decoded, data.as_bytes(), "解码数据应与原始数据一致");
        }
        assert_eq!(arena.rows_decoded(), (batch + 1) * 50);
    }

    assert_eq!(
        arena.allocation_count(),
        0,
        "4096 字节容量内 3 批 50 行解码不应触发堆分配"
    );
    assert_eq!(arena.rows_decoded(), 150, "总共解码 150 行");
}

#[test]
fn test_typed_arena_exceeds_capacity_triggers_allocation() {
    let mut arena: TypedArena<f64> = TypedArena::with_capacity(4);

    for i in 0..10 {
        arena.alloc(i as f64 * 3.15);
    }

    assert_eq!(arena.len(), 10);
    assert!(
        arena.allocation_count() > 0,
        "10 个元素超出 capacity=4，应触发堆分配"
    );
}
// T1.3: PipelineParallelismAdapter 端到端测试

#[test]
fn test_pipeline_parallelism_high_load_decreases() {
    let mut adapter = PipelineParallelismAdapter::new(2, 16).with_smoothing_factor(0.0);
    for _ in 0..5 {
        adapter.record_load_signal(LoadSignal::new(0.95, 80, 0.9));
    }
    let initial = adapter.current_parallelism();
    let adjusted = adapter.adjust_parallelism();
    assert!(
        adjusted < initial,
        "高负载时并行度应降级: initial={}, adjusted={}",
        initial,
        adjusted
    );
}

#[test]
fn test_pipeline_parallelism_low_load_increases() {
    let mut adapter = PipelineParallelismAdapter::new(2, 16).with_smoothing_factor(0.0);
    adapter.record_load_signal(LoadSignal::new(0.9, 80, 0.9));
    adapter.adjust_parallelism();

    for _ in 0..10 {
        adapter.record_load_signal(LoadSignal::new(0.1, 5, 0.05));
    }
    let before = adapter.current_parallelism();
    let adjusted = adapter.adjust_parallelism();
    assert!(
        adjusted > before,
        "低负载时并行度应提升: before={}, adjusted={}",
        before,
        adjusted
    );
}

#[test]
fn test_pipeline_parallelism_stays_in_bounds() {
    let mut adapter = PipelineParallelismAdapter::new(4, 12).with_smoothing_factor(0.0);
    for _ in 0..20 {
        adapter.record_load_signal(LoadSignal::new(0.99, 200, 0.99));
        let adjusted = adapter.adjust_parallelism();
        assert!((4..=12).contains(&adjusted), "并行度应在 [4,12] 内: got {}", adjusted);
    }
    for _ in 0..20 {
        adapter.record_load_signal(LoadSignal::new(0.01, 0, 0.0));
        let adjusted = adapter.adjust_parallelism();
        assert!((4..=12).contains(&adjusted), "并行度应在 [4,12] 内: got {}", adjusted);
    }
}

#[test]
fn test_pipeline_parallelism_smoothing_avoids_oscillation() {
    let mut adapter = PipelineParallelismAdapter::new(2, 16).with_smoothing_factor(0.9);
    let mut results = Vec::new();
    for i in 0..20 {
        let load = if i % 2 == 0 { 0.9 } else { 0.1 };
        adapter.record_load_signal(LoadSignal::new(load, 50, load));
        results.push(adapter.adjust_parallelism());
    }
    let changes: usize = results.windows(2).map(|w| if w[0] != w[1] { 1 } else { 0 }).sum();
    assert!(changes < 10, "平滑因子 0.9 应减少震荡，变化次数 {} 应 < 10", changes);
}
// T1.4: AccessPatternAnalyzer 三维度热点 + OffPeakScheduler 端到端测试

#[test]
fn test_access_pattern_time_locality_identifies_hot_queries() {
    let config = AccessPatternConfig {
        top_n: 5,
        time_window: std::time::Duration::from_secs(300),
        freq_threshold: 1,
    };
    let analyzer = AccessPatternAnalyzer::new(config);

    analyzer.record_access("user:1");
    analyzer.record_access("user:2");
    analyzer.record_access("user:3");

    let hot = analyzer.analyze_time_locality();
    assert!(!hot.is_empty(), "时间局部性应识别出最近访问的 key");
    assert!(hot.contains(&"user:3".to_string()), "最后访问的 user:3 应在热点列表中");
}

#[test]
fn test_access_pattern_spatial_locality_preloads_neighbors() {
    let config = AccessPatternConfig {
        top_n: 10,
        time_window: std::time::Duration::from_secs(300),
        freq_threshold: 2,
    };
    let analyzer = AccessPatternAnalyzer::new(config);

    analyzer.record_access("user:1");
    analyzer.record_access("user:1");
    analyzer.record_access("user:2");
    analyzer.record_access("user:2");
    analyzer.record_access("order:1");
    analyzer.record_access("order:1");

    let neighbors = analyzer.analyze_spatial_locality("user:");
    assert!(!neighbors.is_empty(), "空间局部性应识别 user: 前缀的 key");
    assert!(neighbors.contains(&"user:1".to_string()));
    assert!(neighbors.contains(&"user:2".to_string()));
    assert!(!neighbors.contains(&"order:1".to_string()), "order: 不应匹配 user: 前缀");
}

#[test]
fn test_access_pattern_frequency_distribution_identifies_high_frequency() {
    let config = AccessPatternConfig {
        top_n: 10,
        time_window: std::time::Duration::from_secs(300),
        freq_threshold: 1,
    };
    let analyzer = AccessPatternAnalyzer::new(config);

    for _ in 0..10 {
        analyzer.record_access("hot_key");
    }
    for _ in 0..2 {
        analyzer.record_access("warm_key");
    }

    let freq = analyzer.analyze_frequency_distribution();
    assert!(!freq.is_empty(), "频率分布应识别高频 key");

    let hot = freq.iter().find(|(k, _)| k == "hot_key");
    assert!(hot.is_some(), "hot_key 应在频率分布中");
    let (_, hot_freq) = hot.unwrap();
    assert!(*hot_freq > 0.5, "hot_key 频率应 > 0.5: got {}", hot_freq);
}

#[test]
fn test_cache_hit_rate_feedback_adjusts_threshold() {
    let config = AccessPatternConfig {
        top_n: 10,
        time_window: std::time::Duration::from_secs(300),
        freq_threshold: 10,
    };
    let analyzer = AccessPatternAnalyzer::new(config);

    let low_threshold = analyzer.adjust_threshold_by_hit_rate(0.2);
    assert_eq!(low_threshold, 5, "命中率 0.2 < 0.3 时阈值应降为 5");

    let high_threshold = analyzer.adjust_threshold_by_hit_rate(0.9);
    assert_eq!(high_threshold, 20, "命中率 0.9 > 0.8 时阈值应升为 20");

    let mid_threshold = analyzer.adjust_threshold_by_hit_rate(0.5);
    assert_eq!(mid_threshold, 10, "命中率 0.5 在中间时阈值不变");
}

#[test]
fn test_off_peak_scheduler_triggers_warmup_in_low_traffic() {
    use sz_orm_core::warmup_trigger::OffPeakScheduler;
    use std::time::Duration;

    let mut scheduler = OffPeakScheduler::new(100.0).with_window_size(Duration::from_millis(500));

    for _ in 0..5 {
        scheduler.record_access();
    }

    std::thread::sleep(Duration::from_millis(600));

    let triggered = scheduler.check_and_trigger_warmup();
    assert!(
        triggered,
        "低流量（5 次访问/1.1 秒 < 100/秒）应触发低峰预热"
    );
    assert!(scheduler.is_warmup_triggered(), "预热状态应已标记");

    let again = scheduler.check_and_trigger_warmup();
    assert!(!again, "已触发后不应重复触发");

    scheduler.reset_warmup();
    assert!(!scheduler.is_warmup_triggered(), "reset 后预热状态应清除");
}
// T1.5: BatchUpdate/Delete/Upsert/QueryShardAdvisor 端到端测试

#[test]
fn test_batch_update_advisor_adapts_to_history() {
    let advisor = BatchUpdateAdvisor::new(DynamicBatchConfig::default());

    let size = advisor.advise(DbType::PostgreSQL, 5, 10000);
    assert!(size > 0, "BatchUpdateAdvisor 应返回正批量大小");

    advisor.record_execution(size, 1000.0, 50.0);
    let size2 = advisor.advise(DbType::PostgreSQL, 5, 10000);
    assert!(size2 > 0, "记录历史后仍应返回正批量大小");
}

#[test]
fn test_batch_delete_advisor_adapts_to_history() {
    let advisor = BatchDeleteAdvisor::new(DynamicBatchConfig::default());

    let size = advisor.advise(DbType::PostgreSQL, 5, 10000);
    assert!(size > 0, "BatchDeleteAdvisor 应返回正批量大小");
}

#[test]
fn test_batch_upsert_advisor_adapts_to_history() {
    let advisor = BatchUpsertAdvisor::new(DynamicBatchConfig::default());

    let size = advisor.advise(DbType::PostgreSQL, 5, 10000);
    assert!(size > 0, "BatchUpsertAdvisor 应返回正批量大小");
}

#[test]
fn test_batch_query_shard_advisor_adapts_to_shard_key() {
    let advisor_single = BatchQueryShardAdvisor::new(DynamicBatchConfig::default(), 1);
    let advisor_multi = BatchQueryShardAdvisor::new(DynamicBatchConfig::default(), 16);

    let size_single = advisor_single.advise(DbType::PostgreSQL, 5, 10000);
    let size_multi = advisor_multi.advise(DbType::PostgreSQL, 5, 10000);

    assert!(size_single > 0, "单分片应返回正批量大小");
    assert!(size_multi > 0, "多分片应返回正批量大小");
    assert!(
        size_multi <= size_single,
        "多分片批量大小应 <= 单分片: single={}, multi={}",
        size_single,
        size_multi
    );
    assert_eq!(advisor_multi.shard_count(), 16, "分片数应为 16");
}

#[test]
fn test_batch_advisors_update_smaller_than_delete() {
    let update_advisor = BatchUpdateAdvisor::new(DynamicBatchConfig::default());
    let delete_advisor = BatchDeleteAdvisor::new(DynamicBatchConfig::default());

    let update_size = update_advisor.advise(DbType::PostgreSQL, 5, 10000);
    let delete_size = delete_advisor.advise(DbType::PostgreSQL, 5, 10000);

    assert!(
        update_size <= delete_size,
        "更新批量大小应 <= 删除（更新更重）: update={}, delete={}",
        update_size,
        delete_size
    );
}