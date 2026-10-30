//! v10.11.0 T1.5 缓存亲和性优化端到端接线测试
//!
//! 验证生产调用点可达：
//! - CacheLayoutOptimizer pad/align/预取
//! - AccessPatternAnalyzer 热点访问模式识别
//! - CacheLayoutStats 布局统计
//! - PadStrategy 策略选择

use sz_orm_core::cache_warmup_protection::{
    AccessPatternAnalyzer, CacheLayoutOptimizer, CacheLayoutStats, PadStrategy,
};

/// T1.5 测试：AccessPatternAnalyzer 访问模式分析
///
/// 验证行为副作用：record_access 后 hot_keys 按频率降序返回，
/// access_frequency 正确计算，unique_keys/total_accesses 正确计数。
#[test]
fn test_access_pattern_analyzer() {
    let mut analyzer = AccessPatternAnalyzer::new();

    assert_eq!(analyzer.total_accesses(), 0);
    assert_eq!(analyzer.unique_keys(), 0);

    for _ in 0..100 {
        analyzer.record_access("hot_key");
    }
    for _ in 0..10 {
        analyzer.record_access("warm_key");
    }
    analyzer.record_access("cold_key");

    assert_eq!(analyzer.total_accesses(), 111);
    assert_eq!(analyzer.unique_keys(), 3);

    let hot_freq = analyzer.access_frequency("hot_key");
    assert!((hot_freq - 100.0 / 111.0).abs() < 1e-6);

    let cold_freq = analyzer.access_frequency("cold_key");
    assert!((cold_freq - 1.0 / 111.0).abs() < 1e-6);

    let missing_freq = analyzer.access_frequency("nonexistent");
    assert_eq!(missing_freq, 0.0);

    let hot_keys = analyzer.hot_keys(2);
    assert_eq!(hot_keys.len(), 2);
    assert_eq!(hot_keys[0].0, "hot_key");
    assert_eq!(hot_keys[0].1, 100);
    assert_eq!(hot_keys[1].0, "warm_key");
    assert_eq!(hot_keys[1].1, 10);
}

/// T1.5 测试：CacheLayoutOptimizer 基本属性
#[test]
fn test_cache_layout_optimizer_basic() {
    let optimizer = CacheLayoutOptimizer::new(64);
    assert_eq!(optimizer.cache_line_size(), 64);
    assert_eq!(optimizer.pad_strategy(), &PadStrategy::None);

    let optimizer = CacheLayoutOptimizer::new(128).with_pad_strategy(PadStrategy::ToCacheLine);
    assert_eq!(optimizer.cache_line_size(), 128);
    assert_eq!(optimizer.pad_strategy(), &PadStrategy::ToCacheLine);
}

/// T1.5 测试：optimize_layout PadStrategy::None
///
/// 验证行为副作用：None 策略下 pad_bytes == 0，group_count == 1。
#[test]
fn test_optimize_layout_none_strategy() {
    let optimizer = CacheLayoutOptimizer::new(64);
    let mut entries: Vec<u64> = vec![1, 2, 3, 4, 5, 6, 7, 8];
    let stats = optimizer.optimize_layout(&mut entries);

    assert_eq!(stats.pad_bytes, 0);
    assert_eq!(stats.group_count, 1);
    assert_eq!(stats.optimized_entries, 8);
    assert_eq!(stats.cache_miss_rate, 1.0);
    assert_eq!(stats.cache_hit_rate, 0.0);
}

/// T1.5 测试：optimize_layout PadStrategy::ToCacheLine
///
/// 验证行为副作用：ToCacheLine 策略下 pad_bytes 正确计算。
/// u64 size_of = 8，cache_line = 64，remainder = 8 % 64 = 8，
/// pad_per_entry = 64 - 8 = 56，pad_bytes = 56 * entry_count。
#[test]
fn test_optimize_layout_to_cache_line() {
    let optimizer = CacheLayoutOptimizer::new(64).with_pad_strategy(PadStrategy::ToCacheLine);

    let mut entries: Vec<u64> = vec![1, 2, 3, 4];
    let stats = optimizer.optimize_layout(&mut entries);

    let entry_size = std::mem::size_of::<u64>();
    let remainder = entry_size % 64;
    let pad_per_entry = 64 - remainder;
    assert_eq!(stats.pad_bytes, pad_per_entry * 4);
    assert_eq!(stats.optimized_entries, 4);

    let mut aligned_entries: Vec<[u8; 64]> = vec![[0u8; 64], [0u8; 64]];
    let stats_aligned = optimizer.optimize_layout(&mut aligned_entries);
    assert_eq!(stats_aligned.pad_bytes, 0, "[u8; 64] 已对齐 64B，无需填充");
}

/// T1.5 端到端测试：缓存亲和性命中率提升
///
/// 验证行为副作用：
/// 1. 无访问数据时 cache_hit_rate == 0.0
/// 2. 记录热点访问后 cache_hit_rate > 0.0
/// 3. 热点集中度越高，cache_hit_rate 越高
#[test]
fn e2e_cache_affinity_hit_rate_improvement() {
    let mut optimizer = CacheLayoutOptimizer::new(64).with_pad_strategy(PadStrategy::ToCacheLine);

    let mut entries: Vec<u32> = vec![0; 100];
    let stats_before = optimizer.optimize_layout(&mut entries);
    assert_eq!(stats_before.cache_hit_rate, 0.0);
    assert_eq!(stats_before.cache_miss_rate, 1.0);

    for _ in 0..1000 {
        optimizer.analyzer_mut().record_access("hot_key_1");
    }
    for _ in 0..500 {
        optimizer.analyzer_mut().record_access("hot_key_2");
    }
    for _ in 0..50 {
        optimizer.analyzer_mut().record_access("warm_key");
    }
    for _ in 0..5 {
        optimizer.analyzer_mut().record_access("cold_key_1");
        optimizer.analyzer_mut().record_access("cold_key_2");
    }

    let stats_after = optimizer.optimize_layout(&mut entries);
    assert!(
        stats_after.cache_hit_rate > 0.9,
        "热点键占访问 1550/1560 = 99.4%，cache_hit_rate 应 > 0.9，实际 = {}",
        stats_after.cache_hit_rate
    );
    assert!(
        stats_after.cache_miss_rate < 0.1,
        "cache_miss_rate 应 < 0.1，实际 = {}",
        stats_after.cache_miss_rate
    );
    assert!(stats_after.pad_bytes > 0, "u32 size=4, 64B 对齐需要填充");
}

/// T1.5 端到端测试：GroupByAccessPattern 策略分组
///
/// 验证行为副作用：GroupByAccessPattern 策略下 group_count 等于热点键数量。
#[test]
fn e2e_cache_layout_group_by_access_pattern() {
    let mut optimizer =
        CacheLayoutOptimizer::new(64).with_pad_strategy(PadStrategy::GroupByAccessPattern);

    for _ in 0..100 {
        optimizer.analyzer_mut().record_access("key_a");
    }
    for _ in 0..80 {
        optimizer.analyzer_mut().record_access("key_b");
    }
    for _ in 0..60 {
        optimizer.analyzer_mut().record_access("key_c");
    }

    let mut entries: Vec<u64> = vec![0; 50];
    let stats = optimizer.optimize_layout(&mut entries);

    assert_eq!(stats.group_count, 3, "3 个热点键 → 3 个分组");
    assert_eq!(stats.optimized_entries, 50);
    assert_eq!(stats.pad_bytes, 0, "GroupByAccessPattern 不填充");
}

/// T1.5 测试：prefetch_hint 高置信度预取
///
/// 验证行为副作用：
/// - confidence >= 0.7 时返回 true（执行预取）
/// - confidence < 0.7 时返回 false（不预取）
#[test]
fn test_prefetch_hint() {
    let optimizer = CacheLayoutOptimizer::new(64);
    let entry = 42u64;

    assert!(optimizer.prefetch_hint(&entry, 0.9));
    assert!(optimizer.prefetch_hint(&entry, 0.7));
    assert!(!optimizer.prefetch_hint(&entry, 0.69));
    assert!(!optimizer.prefetch_hint(&entry, 0.0));
}

/// T1.5 测试：CacheLayoutStats 默认值 + 行为副作用
///
/// 验证行为副作用：
/// - Default 初始值为全零（未执行任何优化）
/// - 经过 optimize_layout 操作后 stats 字段发生变化（优化已执行）
#[test]
fn test_cache_layout_stats_default() {
    let stats = CacheLayoutStats::default();
    assert_eq!(stats.cache_miss_rate, 0.0);
    assert_eq!(stats.cache_hit_rate, 0.0);
    assert_eq!(stats.pad_bytes, 0);
    assert_eq!(stats.optimized_entries, 0);
    assert_eq!(stats.group_count, 0);

    let optimizer = CacheLayoutOptimizer::new(64).with_pad_strategy(PadStrategy::ToCacheLine);
    let mut entries: Vec<u64> = vec![0; 10];
    let stats_after = optimizer.optimize_layout(&mut entries);
    assert_eq!(
        stats_after.optimized_entries, 10,
        "优化后 entries 计数应更新"
    );
    assert!(stats_after.pad_bytes > 0, "ToCacheLine 策略应产生填充字节");
}

/// T1.5 测试：空 entries 优化
#[test]
fn test_optimize_layout_empty_entries() {
    let optimizer = CacheLayoutOptimizer::new(64).with_pad_strategy(PadStrategy::ToCacheLine);
    let mut entries: Vec<u64> = vec![];
    let stats = optimizer.optimize_layout(&mut entries);
    assert_eq!(stats.optimized_entries, 0);
    assert_eq!(stats.pad_bytes, 0);
}
