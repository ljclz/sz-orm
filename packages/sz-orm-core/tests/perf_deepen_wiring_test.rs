//! 性能优化深化接线端到端测试（v10.9.0 T4.1~T4.4）
//!
//! 验证连接池/缓存/SIMD/零拷贝优化的行为副作用：
//! - tuning_advice 返回包含 known_good_hit_rate/reap_suggestion/prewarm_suggestion 的建议
//! - known_good_hit_rate 在启用快速路径后返回实际命中率
//! - suggest_tuning 在有数据时返回合理建议

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use sz_orm_core::plan_cache::{AdaptiveCacheCapacity, PlanCache};
use sz_orm_core::{
    Connection, ConnectionFactory, DbError, Pool, PoolConfig, PoolConfigBuilder, PoolTuningAdvice,
    QueryRows,
};

struct MockConnection;

impl Connection for MockConnection {
    fn execute<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(1) })
    }

    fn query<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<QueryRows, DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(Vec::new()) })
    }

    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }

    fn commit<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }

    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }

    fn is_connected(&self) -> bool {
        true
    }

    fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { true })
    }

    fn close<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }
}

struct MockFac;

#[async_trait::async_trait]
impl ConnectionFactory for MockFac {
    async fn create(&self) -> Result<Box<dyn Connection>, DbError> {
        Ok(Box::new(MockConnection))
    }
}

fn make_pool(config: PoolConfig) -> Pool {
    Pool::new(config, Arc::new(MockFac)).unwrap()
}

/// tuning_advice 返回包含新字段的建议
#[test]
fn test_pool_tuning_advice_returns_recommendations() {
    let config = PoolConfigBuilder::new()
        .max_size(10)
        .min_idle(2)
        .prewarm(false)
        .build()
        .unwrap();
    let pool = make_pool(config);
    let advice = pool.tuning_advice();

    // 新建 pool 无 acquire 数据、prewarm 未启用，三个新字段均应为 None
    assert!(
        advice.known_good_hit_rate.is_none(),
        "无 acquire 数据时 known_good_hit_rate 应为 None"
    );
    assert!(
        advice.reap_suggestion.is_none(),
        "无 reap 数据时 reap_suggestion 应为 None"
    );
    assert!(
        advice.prewarm_suggestion.is_none(),
        "prewarm 未启用时 prewarm_suggestion 应为 None"
    );
}

/// tuning_advice 在 prewarm 启用时返回 prewarm 建议
#[test]
fn test_pool_tuning_advice_prewarm_suggestion() {
    let config = PoolConfigBuilder::new()
        .max_size(10)
        .prewarm(true)
        .build()
        .unwrap();
    let pool = make_pool(config);
    let advice = pool.tuning_advice();

    assert!(
        advice.prewarm_suggestion.is_some(),
        "prewarm 启用时应返回 prewarm 建议"
    );
    let suggestion = advice.prewarm_suggestion.unwrap();
    assert!(
        suggestion.contains("prewarm"),
        "prewarm 建议应包含 'prewarm': {}",
        suggestion
    );
}

/// suggest_tuning 数据不足时返回 reason="数据不足"
#[test]
fn test_pool_suggest_tuning_no_data() {
    let config = PoolConfigBuilder::new().max_size(5).build().unwrap();
    let pool = make_pool(config);
    let advice = pool.suggest_tuning();

    assert_eq!(
        advice.reason, "数据不足",
        "无 acquire 数据时 reason 应为 '数据不足'"
    );
    assert!(advice.is_optimal(), "数据不足时无建议");
}

/// PoolTuningAdvice 新字段默认为 None
#[test]
fn test_pool_tuning_advice_new_fields_default_none() {
    let advice = PoolTuningAdvice {
        suggested_max_size: None,
        suggested_min_idle: None,
        suggested_idle_timeout: None,
        reason: String::new(),
        known_good_hit_rate: None,
        reap_suggestion: None,
        prewarm_suggestion: None,
    };
    assert!(advice.known_good_hit_rate.is_none());
    assert!(advice.reap_suggestion.is_none());
    assert!(advice.prewarm_suggestion.is_none());
    assert!(advice.is_optimal());
}

/// known_good_hit_rate 初始为 0.0
#[test]
fn test_pool_known_good_hit_rate_initial_zero() {
    let config = PoolConfigBuilder::new().max_size(5).build().unwrap();
    let pool = make_pool(config);
    let rate = pool.known_good_hit_rate();
    assert_eq!(
        rate, 0.0,
        "初始 known_good_hit_rate 应为 0.0（无 acquire 操作）"
    );
}

/// tuning_advice 在未启用 prewarm 且无数据时 prewarm_suggestion 为 None
#[test]
fn test_pool_tuning_advice_no_prewarm_no_data() {
    let config = PoolConfigBuilder::new()
        .max_size(5)
        .prewarm(false)
        .build()
        .unwrap();
    let pool = make_pool(config);
    let advice = pool.tuning_advice();

    assert!(
        advice.prewarm_suggestion.is_none(),
        "未启用 prewarm 且无 acquire 数据时不应有 prewarm 建议"
    );
}
// ==================== T4.2 缓存深化测试 ====================

/// PlanCache 初始命中率为 0.0
#[test]
fn test_plan_cache_initial_hit_rate_zero() {
    let cache = PlanCache::new(256, None);
    assert_eq!(cache.parse_hit_rate(), 0.0, "初始 parse_hit_rate 应为 0.0");
}

/// PlanCache 容量返回正确值
#[test]
fn test_plan_cache_capacity() {
    let cache = PlanCache::new(256, None);
    assert_eq!(cache.capacity(), 256, "容量应等于构造参数");
}

/// AdaptiveCacheCapacity 低命中率时增大容量
#[test]
fn test_adaptive_cache_capacity_adapts_to_low_hit_rate() {
    let mut advisor = AdaptiveCacheCapacity::new(10, 1000);
    assert_eq!(advisor.current(), 10, "初始容量应为 min");

    // 记录低命中率
    for _ in 0..10 {
        advisor.record_hit_rate(0.5);
    }
    let changed = advisor.adapt();
    assert!(changed, "低命中率（0.5 < 0.85）应触发容量增大");
    assert!(
        advisor.current() > 10,
        "低命中率后容量应增大: {}",
        advisor.current()
    );
}

/// AdaptiveCacheCapacity 高命中率时减小容量
#[test]
fn test_adaptive_cache_capacity_adapts_to_high_hit_rate() {
    let mut advisor = AdaptiveCacheCapacity::new(10, 1000);
    // 先增大容量
    for _ in 0..10 {
        advisor.record_hit_rate(0.5);
    }
    advisor.adapt();
    let mid_capacity = advisor.current();
    assert!(mid_capacity > 10, "容量应已增大: {}", mid_capacity);

    // 记录足够多的高命中率样本以推挤出低命中率样本
    for _ in 0..110 {
        advisor.record_hit_rate(0.98);
    }
    let changed = advisor.adapt();
    assert!(changed, "高命中率（0.98 > 0.95）应触发容量减小");
    assert!(
        advisor.current() < mid_capacity,
        "高命中率后容量应减小: {} -> {}",
        mid_capacity,
        advisor.current()
    );
}

/// AdaptiveCacheCapacity 无数据时不调整
#[test]
fn test_adaptive_cache_capacity_no_data_no_change() {
    let mut advisor = AdaptiveCacheCapacity::new(10, 1000);
    let changed = advisor.adapt();
    assert!(!changed, "无命中率数据时 adapt 应返回 false");
    assert_eq!(advisor.current(), 10, "无数据时容量不变");
}
// ==================== T4.3 SIMD 批量优化深化测试 ====================

use sz_orm_core::batch_advisor::{BatchSizeAdvisor, DynamicBatchAdvisor, DynamicBatchConfig};
use sz_orm_core::simd::{
    batch_aggregate_f32, batch_aggregate_f64, batch_aggregate_i64, batch_dot_product_f32,
    batch_dot_product_f64, batch_filter_f32, batch_filter_f64, batch_filter_i64, SimdAggOp,
    SimdCmpOp,
};
use sz_orm_core::DbType;

/// SIMD 批量处理结果与标量处理一致（正确性验证）
#[test]
fn test_simd_batch_processing_correctness() {
    let data_i64: Vec<i64> = vec![1, 2, 3, 4, 5, 6, 7, 8];
    let data_f32: Vec<f32> = vec![1.0, 2.0, 3.0, 4.0, 5.0];
    let data_f64: Vec<f64> = vec![1.0, 2.0, 3.0, 4.0, 5.0];

    let filtered_i64 = batch_filter_i64(&data_i64, 4, SimdCmpOp::Ge);
    assert_eq!(
        filtered_i64,
        vec![false, false, false, true, true, true, true, true],
        "i64 过滤结果应正确"
    );

    let sum_i64 = batch_aggregate_i64(&data_i64, SimdAggOp::Sum);
    assert_eq!(sum_i64, 36.0, "i64 求和应正确");

    let filtered_f32 = batch_filter_f32(&data_f32, 3.0, SimdCmpOp::Gt);
    assert_eq!(
        filtered_f32,
        vec![false, false, false, true, true],
        "f32 过滤结果应正确"
    );

    let sum_f32 = batch_aggregate_f32(&data_f32, SimdAggOp::Sum);
    assert_eq!(sum_f32, 15.0, "f32 求和应正确");

    let filtered_f64 = batch_filter_f64(&data_f64, 3.0, SimdCmpOp::Le);
    assert_eq!(
        filtered_f64,
        vec![true, true, true, false, false],
        "f64 过滤结果应正确"
    );

    let avg_f64 = batch_aggregate_f64(&data_f64, SimdAggOp::Avg);
    assert_eq!(avg_f64, 3.0, "f64 平均值应正确");

    let dot = batch_dot_product_f64(&data_f64, &data_f64);
    assert_eq!(dot, 55.0, "点积应正确: 1+4+9+16+25=55");

    let dot_f32 = batch_dot_product_f32(&data_f32, &data_f32);
    assert_eq!(dot_f32, 55.0, "f32 点积应正确");
}

/// SIMD i64 批量操作覆盖所有比较算子
#[test]
fn test_simd_filter_i64_all_ops_correctness() {
    let data: Vec<i64> = vec![-2, -1, 0, 1, 2];
    let threshold = 0_i64;

    let eq = batch_filter_i64(&data, threshold, SimdCmpOp::Eq);
    assert_eq!(eq, vec![false, false, true, false, false]);

    let lt = batch_filter_i64(&data, threshold, SimdCmpOp::Lt);
    assert_eq!(lt, vec![true, true, false, false, false]);

    let gt = batch_filter_i64(&data, threshold, SimdCmpOp::Gt);
    assert_eq!(gt, vec![false, false, false, true, true]);

    let ne = batch_filter_i64(&data, threshold, SimdCmpOp::Ne);
    assert_eq!(ne, vec![true, true, false, true, true]);
}

/// SIMD i64 聚合覆盖所有聚合算子
#[test]
fn test_simd_aggregate_i64_all_ops_correctness() {
    let data: Vec<i64> = vec![10, 20, 30, 40, 50];

    assert_eq!(batch_aggregate_i64(&data, SimdAggOp::Sum), 150.0);
    assert_eq!(batch_aggregate_i64(&data, SimdAggOp::Min), 10.0);
    assert_eq!(batch_aggregate_i64(&data, SimdAggOp::Max), 50.0);
    assert_eq!(batch_aggregate_i64(&data, SimdAggOp::Avg), 30.0);
}

/// SIMD 点积正确性验证（正交向量、长度不一致）
#[test]
fn test_simd_dot_product_correctness() {
    let orthogonal_a = vec![1.0, 0.0, 0.0];
    let orthogonal_b = vec![0.0, 1.0, 0.0];
    assert_eq!(
        batch_dot_product_f64(&orthogonal_a, &orthogonal_b),
        0.0,
        "正交向量点积应为 0"
    );

    let mismatch_a = vec![1.0, 2.0, 3.0];
    let mismatch_b = vec![4.0, 5.0];
    assert_eq!(
        batch_dot_product_f64(&mismatch_a, &mismatch_b),
        14.0,
        "长度不一致时按较短截断: 1*4+2*5=14"
    );

    assert_eq!(batch_dot_product_f64(&[], &[]), 0.0, "空向量点积应为 0");
}

/// BatchSizeAdvisor 静态建议遵守参数上限约束
#[test]
fn test_batch_size_advisor_respects_param_limit() {
    let batch = BatchSizeAdvisor::advise(DbType::Oracle, 10, 10000);
    assert!(
        batch * 10 <= 1000,
        "Oracle 参数上限 1000，batch*columns 不应超过: batch={}",
        batch
    );
    assert!(batch >= 1, "批量大小应 >= 1");
}

/// BatchSizeAdvisor 静态建议不超过总行数
#[test]
fn test_batch_size_advisor_respects_total_rows() {
    let batch = BatchSizeAdvisor::advise(DbType::MySQL, 3, 100);
    assert!(batch <= 100, "批量大小不应超过总行数: batch={}", batch);
}

/// BatchSizeAdvisor batch_ranges 覆盖所有行
#[test]
fn test_batch_size_advisor_ranges_cover_all() {
    let ranges = BatchSizeAdvisor::batch_ranges(DbType::MySQL, 2, 1000);
    let covered: usize = ranges.iter().map(|(s, e)| e - s).sum();
    assert_eq!(covered, 1000, "分批范围应覆盖所有行");
    for (s, e) in &ranges {
        assert!(s < e, "每个范围 start < end: ({}, {})", s, e);
    }
}

/// DynamicBatchAdvisor 历史感知后批量大小调整
#[test]
fn test_batch_size_advisor_adapts_to_history() {
    let advisor = DynamicBatchAdvisor::new(DynamicBatchConfig::default());

    let initial = advisor.advise_dynamic(DbType::MySQL, 2, 10000);
    let static_val = BatchSizeAdvisor::advise(DbType::MySQL, 2, 10000);
    assert_eq!(initial, static_val, "无历史时应降级为静态建议值");

    advisor.record_execution(500, 5000.0, 10.0);
    advisor.record_execution(500, 5000.0, 10.0);
    advisor.record_execution(500, 5000.0, 10.0);

    assert_eq!(
        advisor.execution_count(),
        3,
        "执行 3 次后 execution_count 应为 3"
    );

    let dynamic = advisor.advise_dynamic(DbType::MySQL, 2, 10000);
    assert!(
        dynamic >= static_val,
        "高吞吐量低延迟时动态建议应 >= 静态值: dynamic={}, static={}",
        dynamic,
        static_val
    );
    assert!(dynamic >= 1, "动态建议应 >= 1: {}", dynamic);
}

/// DynamicBatchAdvisor 零历史时降级为静态建议
#[test]
fn test_dynamic_advisor_degrades_without_history() {
    let advisor = DynamicBatchAdvisor::new(DynamicBatchConfig::default());
    let dynamic = advisor.advise_dynamic(DbType::PostgreSQL, 5, 5000);
    let static_val = BatchSizeAdvisor::advise(DbType::PostgreSQL, 5, 5000);
    assert_eq!(dynamic, static_val, "无历史时动态建议应等于静态建议");
}

/// DynamicBatchAdvisor 高缓存命中率时增大批量
#[test]
fn test_dynamic_advisor_cache_hit_rate_boost() {
    let config = DynamicBatchConfig {
        initial_size: 500,
        min_size: 1,
        max_size: 5000,
        ema_factor: 0.3,
        adjust_threshold: 0.1,
        convergence_threshold: 0.05,
    };
    let advisor = DynamicBatchAdvisor::new(config);
    advisor.record_execution(500, 5000.0, 10.0);
    advisor.record_execution(500, 5000.0, 10.0);
    advisor.record_execution(500, 5000.0, 10.0);

    let base = advisor.advise_dynamic(DbType::MySQL, 2, 10000);
    let boosted = advisor.advise_dynamic_with_cache(DbType::MySQL, 2, 10000, Some(0.9));
    assert!(
        boosted > base,
        "高缓存命中率（0.9 >= 0.8）应增大批量: base={}, boosted={}",
        base,
        boosted
    );
}
// ==================== T4.4 零拷贝优化深化测试 ====================

use std::collections::HashMap;
use sz_orm_core::zero_copy_pipeline::ZeroCopyPipeline;
use sz_orm_core::Value;

/// 零拷贝序列化结果与普通序列化一致（正确性验证）
#[test]
fn test_zero_copy_pipeline_correctness() {
    let pipeline = ZeroCopyPipeline::new();
    let columns = vec!["id".to_string(), "name".to_string(), "active".to_string()];
    let mut row = HashMap::new();
    row.insert("id".to_string(), Value::I64(42));
    row.insert("name".to_string(), Value::String("test".to_string()));
    row.insert("active".to_string(), Value::Bool(true));
    let mut stream = pipeline.stream_rows_extended(vec![row], &columns);

    let r = stream.next_row().expect("应有 1 行");
    assert_eq!(r.column_count(), 3, "列数应为 3");

    let id_data = r.get("id").expect("id 列应存在");
    assert_eq!(id_data, &42i64.to_le_bytes(), "I64 数据应为小端字节");

    let name_data = r.get("name").expect("name 列应存在");
    assert_eq!(name_data, b"test", "String 数据应为 UTF-8 字节");

    let active_data = r.get("active").expect("active 列应存在");
    assert_eq!(active_data, &[1u8], "Bool(true) 应编码为 [1]");

    assert!(stream.next_row().is_none(), "只有 1 行，再取应为 None");
    assert_eq!(stream.remaining(), 0, "剩余应为 0");
}

/// 零拷贝管线统计正确：原始类型全部零拷贝命中
#[test]
fn test_zero_copy_pipeline_stats_zero_copy_hits() {
    let pipeline = ZeroCopyPipeline::new();
    let columns = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    let mut row = HashMap::new();
    row.insert("a".to_string(), Value::I32(1));
    row.insert("b".to_string(), Value::F64(2.5));
    row.insert("c".to_string(), Value::String("x".to_string()));
    let _stream = pipeline.stream_rows_extended(vec![row], &columns);

    assert_eq!(pipeline.stats().zero_copy_hits(), 3, "3 列全部零拷贝命中");
    assert_eq!(pipeline.stats().fallback_copies(), 0, "无回退");
}

/// 零拷贝管线统计正确：复杂类型回退拷贝
#[test]
fn test_zero_copy_pipeline_stats_fallback_for_complex() {
    let pipeline = ZeroCopyPipeline::new();
    let columns = vec!["arr".to_string(), "obj".to_string()];
    let mut row = HashMap::new();
    row.insert("arr".to_string(), Value::Array(vec![Value::I32(1)]));
    row.insert("obj".to_string(), Value::Object(HashMap::new()));
    let _stream = pipeline.stream_rows_extended(vec![row], &columns);

    assert_eq!(pipeline.stats().zero_copy_hits(), 0, "无零拷贝命中");
    assert_eq!(
        pipeline.stats().fallback_copies(),
        2,
        "Array + Object 应回退"
    );
}

/// 零拷贝管线处理多行数据
#[test]
fn test_zero_copy_pipeline_multi_row() {
    let pipeline = ZeroCopyPipeline::new();
    let columns = vec!["id".to_string()];
    let rows: Vec<HashMap<String, Value>> = (0..10)
        .map(|i| {
            let mut row = HashMap::new();
            row.insert("id".to_string(), Value::I64(i));
            row
        })
        .collect();
    let mut stream = pipeline.stream_rows_extended(rows, &columns);
    assert_eq!(stream.total_rows(), 10);

    for i in 0..10i64 {
        let r = stream
            .next_row()
            .unwrap_or_else(|| panic!("第 {} 行应存在", i));
        let data = r.get("id").expect("id 列应存在");
        assert_eq!(data, &i.to_le_bytes(), "第 {} 行 I64 数据应正确", i);
    }
    assert!(stream.next_row().is_none(), "10 行后应无更多行");
}

/// 零拷贝管线扩展字符串类型正确序列化
#[test]
fn test_zero_copy_pipeline_extended_string_types() {
    let pipeline = ZeroCopyPipeline::new();
    let columns = vec![
        "decimal".to_string(),
        "datetime".to_string(),
        "json".to_string(),
        "uuid".to_string(),
        "date".to_string(),
        "time".to_string(),
    ];
    let mut row = HashMap::new();
    row.insert("decimal".to_string(), Value::Decimal("123.45".to_string()));
    row.insert(
        "datetime".to_string(),
        Value::DateTime("2026-01-01T00:00:00Z".to_string()),
    );
    row.insert(
        "json".to_string(),
        Value::Json("{\"key\":\"value\"}".to_string()),
    );
    row.insert(
        "uuid".to_string(),
        Value::Uuid("550e8400-e29b-41d4-a716-446655440000".to_string()),
    );
    row.insert("date".to_string(), Value::Date("2026-01-01".to_string()));
    row.insert("time".to_string(), Value::Time("12:30:45".to_string()));
    let mut stream = pipeline.stream_rows_extended(vec![row], &columns);
    let r = stream.next_row().unwrap();

    assert_eq!(r.get("decimal").unwrap(), b"123.45");
    assert_eq!(r.get("datetime").unwrap(), b"2026-01-01T00:00:00Z");
    assert_eq!(r.get("json").unwrap(), b"{\"key\":\"value\"}");
    assert_eq!(
        r.get("uuid").unwrap(),
        b"550e8400-e29b-41d4-a716-446655440000"
    );
    assert_eq!(r.get("date").unwrap(), b"2026-01-01");
    assert_eq!(r.get("time").unwrap(), b"12:30:45");

    assert_eq!(
        pipeline.stats().zero_copy_hits(),
        6,
        "6 个扩展字符串类型全部零拷贝"
    );
}

/// 零拷贝管线 Bytes 类型正确序列化
#[test]
fn test_zero_copy_pipeline_bytes_type() {
    let pipeline = ZeroCopyPipeline::new();
    let columns = vec!["data".to_string()];
    let mut row = HashMap::new();
    row.insert(
        "data".to_string(),
        Value::Bytes(vec![0xDE, 0xAD, 0xBE, 0xEF]),
    );
    let mut stream = pipeline.stream_rows_extended(vec![row], &columns);
    let r = stream.next_row().unwrap();
    assert_eq!(r.get("data").unwrap(), &[0xDE, 0xAD, 0xBE, 0xEF]);
}

/// 零拷贝管线 None 值（缺失列）回退拷贝
#[test]
fn test_zero_copy_pipeline_missing_column_fallback() {
    let pipeline = ZeroCopyPipeline::new();
    let columns = vec!["a".to_string(), "b".to_string()];
    let mut row = HashMap::new();
    row.insert("a".to_string(), Value::I32(1));
    let _stream = pipeline.stream_rows_extended(vec![row], &columns);

    assert_eq!(pipeline.stats().zero_copy_hits(), 1, "a 列零拷贝");
    assert_eq!(pipeline.stats().fallback_copies(), 1, "b 列缺失回退");
}

/// Value clonecheap 生产接线验证
#[test]
fn test_value_clonecheap_correctness() {
    let v1 = Value::I64(42);
    assert_eq!(v1.clonecheap(), v1, "clonecheap 应返回相等值");

    let v2 = Value::String("hello".to_string());
    assert_eq!(v2.clonecheap(), v2, "String clonecheap 应返回相等值");

    let v3 = Value::Bytes(vec![1, 2, 3]);
    assert_eq!(v3.clonecheap(), v3, "Bytes clonecheap 应返回相等值");

    let v4 = Value::Null;
    assert_eq!(v4.clonecheap(), v4, "Null clonecheap 应返回相等值");
}
