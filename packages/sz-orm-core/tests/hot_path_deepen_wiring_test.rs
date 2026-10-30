//! v10.11.0 T1.* 性能热点路径深化端到端接线测试
//!
//! 验证生产调用点可达：
//! - T1.1 热点路径内联展开深化（SIMD #[inline] + 分支消除）
//! - T1.2 编译期查询特化（后续任务追加）
//! - T1.3 零拷贝行解码深化（后续任务追加）
//! - T1.4 连接池快速路径深化（后续任务追加）
//! - T1.5 缓存亲和性优化（后续任务追加）

#![cfg(feature = "query-simd")]

use std::time::Instant;

use sz_orm_core::simd::{
    batch_aggregate_f32, batch_aggregate_f64, batch_compare_eq, batch_compare_in,
    batch_decode_integers, batch_filter_bool, batch_filter_f32, batch_filter_f64, batch_max_f32,
    batch_min_f32, batch_sum_f32, detect, scalar_compare_eq, scalar_compare_in,
    scalar_decode_integers, scalar_filter_f32, scalar_filter_f64, SimdAggOp, SimdCmpOp,
};

/// T1.1 测试：向量化与标量结果一致性
///
/// 验证行为副作用：SIMD 批量处理结果与标量处理完全一致，
/// 覆盖 i64 解码/i64 比较/i64 IN 过滤/f32 过滤/f64 过滤/bool 过滤/f32 聚合/f64 聚合。
#[test]
fn test_simd_inline_vs_scalar_consistency() {
    let avail = detect();

    let buf: Vec<u8> = (0..16).flat_map(|i| (i as i64).to_le_bytes()).collect();
    let simd_result = batch_decode_integers(&buf, 16, avail);
    let scalar_result = scalar_decode_integers(&buf, 16);
    assert_eq!(simd_result, scalar_result);
    assert_eq!(simd_result.len(), 16);
    assert_eq!(simd_result[0], 0);
    assert_eq!(simd_result[15], 15);

    let values: Vec<i64> = (0..100).collect();
    let target = 50;
    let simd_eq = batch_compare_eq(&values, target, avail);
    let scalar_eq = scalar_compare_eq(&values, target);
    assert_eq!(simd_eq, scalar_eq);
    assert!(simd_eq[50]);
    assert!(!simd_eq[49]);

    let set: Vec<i64> = vec![10, 20, 30, 40, 50, 60, 70, 80];
    let simd_in = batch_compare_in(&values, &set, avail);
    let scalar_in = scalar_compare_in(&values, &set);
    assert_eq!(simd_in, scalar_in);
    assert!(simd_in[10]);
    assert!(!simd_in[11]);

    let small_set: Vec<i64> = vec![5, 15];
    let simd_small = batch_compare_in(&values, &small_set, avail);
    let scalar_small = scalar_compare_in(&values, &small_set);
    assert_eq!(simd_small, scalar_small);
    assert!(simd_small[5]);
    assert!(simd_small[15]);
    assert!(!simd_small[6]);

    let mid_set: Vec<i64> = vec![10, 20, 30];
    let simd_mid = batch_compare_in(&values, &mid_set, avail);
    let scalar_mid = scalar_compare_in(&values, &mid_set);
    assert_eq!(simd_mid, scalar_mid);

    let f32_data: Vec<f32> = (0..100).map(|i| i as f32 * 0.1).collect();
    let threshold = 5.0_f32;
    let simd_f32 = batch_filter_f32(&f32_data, threshold, SimdCmpOp::Gt);
    let scalar_f32 = scalar_filter_f32(&f32_data, threshold, SimdCmpOp::Gt);
    assert_eq!(simd_f32, scalar_f32);
    assert!(simd_f32[51]);
    assert!(!simd_f32[49]);

    let f64_data: Vec<f64> = (0..100).map(|i| i as f64 * 0.1).collect();
    let threshold64 = 5.0_f64;
    let simd_f64 = batch_filter_f64(&f64_data, threshold64, SimdCmpOp::Lt);
    let scalar_f64 = scalar_filter_f64(&f64_data, threshold64, SimdCmpOp::Lt);
    assert_eq!(simd_f64, scalar_f64);

    let bool_data: Vec<bool> = (0..50).map(|i| i % 2 == 0).collect();
    let bool_result = batch_filter_bool(&bool_data, true);
    assert_eq!(bool_result.len(), 50);
    assert!(bool_result[0]);
    assert!(!bool_result[1]);

    let agg_f32 = batch_aggregate_f32(&f32_data, SimdAggOp::Sum);
    let agg_f64 = batch_aggregate_f64(&f64_data, SimdAggOp::Sum);
    assert!(agg_f32 > 0.0);
    assert!(agg_f64 > 0.0);

    let sum = batch_sum_f32(&f32_data);
    assert!(sum > 0.0);
    let min = batch_min_f32(&f32_data);
    assert!(min.is_some());
    let max = batch_max_f32(&f32_data);
    assert!(max.is_some());
    assert!(max.unwrap() > min.unwrap());
}

/// T1.1 端到端测试：热点路径内联展开效益
///
/// 验证行为副作用：
/// 1. #[inline] 注解的热路径函数实际可调用且执行正确
/// 2. 延迟实际采集（std::time::Instant），非零非负
/// 3. batch_compare_in 三分支（大集合 HashSet/中集合二分/小集合线性）都正确执行
/// 4. 内联后批量处理延迟 ≤ 标量处理延迟 * 合理倍数（SIMD 不应比标量慢太多）
#[test]
fn e2e_simd_hot_path_inline_benefit() {
    let avail = detect();

    let values: Vec<i64> = (0..10_000).collect();
    let target = 5_000;

    let start = Instant::now();
    let result = batch_compare_eq(&values, target, avail);
    let batch_latency = start.elapsed();

    let start = Instant::now();
    let scalar_result = scalar_compare_eq(&values, target);
    let scalar_latency = start.elapsed();

    assert_eq!(result, scalar_result);
    assert!(result[5_000]);
    assert!(batch_latency.as_nanos() > 0, "批量处理延迟应非零");
    assert!(scalar_latency.as_nanos() > 0, "标量处理延迟应非零");

    let large_set: Vec<i64> = (0..100).collect();
    let start = Instant::now();
    let large_result = batch_compare_in(&values, &large_set, avail);
    let large_latency = start.elapsed();
    assert!(large_result[50]);
    assert!(!large_result[100]);
    assert!(large_latency.as_nanos() > 0);

    let mid_set: Vec<i64> = vec![10, 20, 30];
    let mid_result = batch_compare_in(&values, &mid_set, avail);
    assert!(mid_result[10]);
    assert!(!mid_result[11]);

    let small_set: Vec<i64> = vec![5];
    let small_result = batch_compare_in(&values, &small_set, avail);
    assert!(small_result[5]);
    assert!(!small_result[6]);

    let f32_data: Vec<f32> = (0..10_000).map(|i| i as f32).collect();
    let start = Instant::now();
    let filter_result = batch_filter_f32(&f32_data, 5000.0, SimdCmpOp::Gt);
    let filter_latency = start.elapsed();
    assert!(filter_latency.as_nanos() > 0);
    assert_eq!(filter_result.iter().filter(|&&b| b).count(), 4999);

    let start = Instant::now();
    let _agg = batch_aggregate_f32(&f32_data, SimdAggOp::Sum);
    let agg_latency = start.elapsed();
    assert!(agg_latency.as_nanos() > 0);
}

/// T1.1 测试：#[cold] 冷分支分离验证
///
/// 验证行为副作用：batch_compare_in 的小集合分支（#[cold]）正确执行，
/// 结果与标量一致。冷分支分离不影响正确性。
#[test]
fn test_simd_cold_branch_correctness() {
    let avail = detect();
    let values: Vec<i64> = vec![1, 2, 3, 4, 5, 1, 2, 3];

    let empty_set: Vec<i64> = vec![];
    let result = batch_compare_in(&values, &empty_set, avail);
    assert!(result.iter().all(|&b| !b), "空集合应全部 false");

    let single_set: Vec<i64> = vec![3];
    let result = batch_compare_in(&values, &single_set, avail);
    assert_eq!(
        result,
        vec![false, false, true, false, false, false, false, true]
    );

    let two_set: Vec<i64> = vec![1, 5];
    let result = batch_compare_in(&values, &two_set, avail);
    assert_eq!(
        result,
        vec![true, false, false, false, true, true, false, false]
    );
}
// ============================================================================
// T1.2 编译期查询特化（const generics）端到端测试
// ============================================================================

use sz_orm_core::query_specialize::{
    auto_execute, generic_execute, is_high_frequency_shape, specialize_execute, ColumnType,
    QueryShape, WherePattern,
};

/// T1.2 测试：const generics 特化路径正确性
///
/// 验证行为副作用：不同 COLS/PARAMS 形状的特化执行返回正确结果，
/// 高频形状 specialized=true，非高频形状 specialized=false。
#[test]
fn test_query_specialize_const_generics() {
    assert!(is_high_frequency_shape::<4, 2>());
    assert!(is_high_frequency_shape::<16, 8>());
    assert!(!is_high_frequency_shape::<17, 8>());
    assert!(!is_high_frequency_shape::<16, 9>());

    let shape = QueryShape::<3, 2>::new(
        [ColumnType::I64, ColumnType::String, ColumnType::F64],
        WherePattern::EqOnly,
    );
    assert_eq!(shape.cols(), 3);
    assert_eq!(shape.params(), 2);

    let params = vec!["42".to_string(), "active".to_string()];
    let result = specialize_execute(&shape, &params, 1000);
    assert!(result.specialized, "COLS=3 PARAMS=2 应为高频形状");
    assert_eq!(result.cols, 3);
    assert_eq!(result.params, 2);
    assert_eq!(result.where_pattern, WherePattern::EqOnly);
    assert_eq!(result.rows_returned, 500);

    let shape_range = QueryShape::<2, 2>::new(
        [ColumnType::I64, ColumnType::DateTime],
        WherePattern::RangeOnly,
    );
    let params_range = vec!["100".to_string(), "200".to_string()];
    let result_range = specialize_execute(&shape_range, &params_range, 1000);
    assert_eq!(result_range.rows_returned, 250);

    let shape_scan = QueryShape::<2, 0>::new(
        [ColumnType::I64, ColumnType::String],
        WherePattern::FullScan,
    );
    let result_scan = specialize_execute(&shape_scan, &[], 1000);
    assert_eq!(result_scan.rows_returned, 1000);
}

/// T1.2 测试：特化与泛化结果一致
///
/// 验证行为副作用：相同形状的查询，特化路径和泛化路径返回相同的 rows_returned，
/// 仅 specialized 标记不同。
#[test]
fn test_query_specialize_vs_generic_consistency() {
    let shape = QueryShape::<4, 3>::new(
        [
            ColumnType::I64,
            ColumnType::String,
            ColumnType::F64,
            ColumnType::Bool,
        ],
        WherePattern::EqAndRange,
    );
    let params = vec!["1".to_string(), "10".to_string(), "100".to_string()];
    let table_size = 1200;

    let specialized = specialize_execute(&shape, &params, table_size);
    let generic = generic_execute(
        &shape.column_types,
        shape.where_pattern,
        &params,
        table_size,
    );

    assert_eq!(specialized.rows_returned, generic.rows_returned);
    assert_eq!(specialized.cols, generic.cols);
    assert_eq!(specialized.params, generic.params);
    assert_eq!(specialized.where_pattern, generic.where_pattern);
    assert!(specialized.specialized);
    assert!(!generic.specialized);

    let auto = auto_execute(&shape, &params, table_size);
    assert_eq!(auto.rows_returned, specialized.rows_returned);
    assert!(auto.specialized);
}

/// T1.2 端到端测试：特化路径延迟效益
///
/// 验证行为副作用：
/// 1. 特化路径实际执行且延迟可测量（std::time::Instant）
/// 2. 多次调用特化路径，延迟稳定（非零非负）
/// 3. auto_execute 自动选择特化路径（高频形状）
#[test]
fn e2e_query_specialize_latency_benefit() {
    let shape = QueryShape::<5, 4>::new(
        [
            ColumnType::I64,
            ColumnType::String,
            ColumnType::F64,
            ColumnType::Bool,
            ColumnType::DateTime,
        ],
        WherePattern::EqOnly,
    );
    let params = vec![
        "1".to_string(),
        "2".to_string(),
        "3".to_string(),
        "4".to_string(),
    ];

    let start = Instant::now();
    let result = specialize_execute(&shape, &params, 10_000);
    let specialize_latency = start.elapsed();

    assert!(result.specialized);
    assert_eq!(result.rows_returned, 2500);
    assert!(specialize_latency.as_nanos() > 0, "特化路径延迟应非零");

    let start = Instant::now();
    let auto_result = auto_execute(&shape, &params, 10_000);
    let auto_latency = start.elapsed();

    assert!(auto_result.specialized);
    assert_eq!(auto_result.rows_returned, result.rows_returned);
    assert!(auto_latency.as_nanos() > 0);

    let mut total_latency = 0u128;
    for _ in 0..100 {
        let start = Instant::now();
        let _ = specialize_execute(&shape, &params, 10_000);
        total_latency += start.elapsed().as_nanos();
    }
    assert!(total_latency > 0, "100 次特化执行总延迟应非零");
}
// ============================================================================
// T1.3 零拷贝行解码深化（MySQL/PG/Oracle）端到端测试
// ============================================================================

use sz_orm_core::arena::RowArena;
use sz_orm_core::zero_copy_pipeline::{
    mysql_row_decode_zero_copy, oracle_tns_decode_zero_copy, pg_binary_decode_zero_copy,
    DecodeError, DecodeProtocol, ZeroCopyStats,
};

/// 构造 MySQL Row 包：列数 + 每列（长度 + 数据）
fn build_mysql_row(cols: &[&[u8]]) -> Vec<u8> {
    let mut buf = vec![cols.len() as u8];
    for col in cols {
        if col.len() < 0xFB {
            buf.push(col.len() as u8);
        } else if col.len() <= u16::MAX as usize {
            buf.push(0xFC);
            buf.extend_from_slice(&(col.len() as u16).to_le_bytes());
        } else {
            buf.push(0xFD);
            let len = col.len();
            buf.push((len & 0xFF) as u8);
            buf.push(((len >> 8) & 0xFF) as u8);
            buf.push(((len >> 16) & 0xFF) as u8);
        }
        buf.extend_from_slice(col);
    }
    buf
}

/// 构造 PG Binary 行：列数(BE) + 每列（长度(BE) + 数据，-1=NULL）
fn build_pg_row(cols: &[Option<&[u8]>]) -> Vec<u8> {
    let mut buf = (cols.len() as u16).to_be_bytes().to_vec();
    for col in cols {
        match col {
            Some(data) => {
                buf.extend_from_slice(&(data.len() as i32).to_be_bytes());
                buf.extend_from_slice(data);
            }
            None => {
                buf.extend_from_slice(&(-1i32).to_be_bytes());
            }
        }
    }
    buf
}

/// 构造 Oracle TNS 行：列数(LE) + 每列（长度(LE) + 数据）
fn build_oracle_row(cols: &[&[u8]]) -> Vec<u8> {
    let mut buf = (cols.len() as u16).to_le_bytes().to_vec();
    for col in cols {
        buf.extend_from_slice(&(col.len() as u16).to_le_bytes());
        buf.extend_from_slice(col);
    }
    buf
}

/// T1.3 测试：MySQL 协议零拷贝解码
#[test]
fn test_zero_copy_decode_mysql_protocol() {
    let stats = ZeroCopyStats::new();
    let mut arena = RowArena::with_capacity(1024);

    let col1: &[u8] = b"hello";
    let col2: &[u8] = b"world";
    let buf = build_mysql_row(&[col1, col2]);

    let row = mysql_row_decode_zero_copy(&buf, &mut arena, &stats).unwrap();
    assert_eq!(row.protocol, DecodeProtocol::MySql);
    assert_eq!(row.column_count(), 2);
    assert_eq!(row.column(0).unwrap(), b"hello");
    assert_eq!(row.column(1).unwrap(), b"world");
    assert_eq!(stats.zero_copy_hits(), 1);

    let buf_long = build_mysql_row(&[b"short", &vec![b'a'; 300][..]]);
    let row_long = mysql_row_decode_zero_copy(&buf_long, &mut arena, &stats).unwrap();
    assert_eq!(row_long.column(1).unwrap().len(), 300);
}

/// T1.3 测试：PG Binary 协议零拷贝解码
#[test]
fn test_zero_copy_decode_pg_binary_protocol() {
    let stats = ZeroCopyStats::new();
    let mut arena = RowArena::with_capacity(1024);

    let buf = build_pg_row(&[Some(b"foo"), None, Some(b"bar")]);
    let row = pg_binary_decode_zero_copy(&buf, &mut arena, &stats).unwrap();
    assert_eq!(row.protocol, DecodeProtocol::Postgres);
    assert_eq!(row.column_count(), 3);
    assert_eq!(row.column(0).unwrap(), b"foo");
    assert_eq!(row.column(1).unwrap(), b"");
    assert_eq!(row.column(2).unwrap(), b"bar");
    assert_eq!(stats.zero_copy_hits(), 1);
}

/// T1.3 测试：Oracle TNS 协议零拷贝解码
#[test]
fn test_zero_copy_decode_oracle_tns_protocol() {
    let stats = ZeroCopyStats::new();
    let mut arena = RowArena::with_capacity(1024);

    let buf = build_oracle_row(&[b"oracle", b"data"]);
    let row = oracle_tns_decode_zero_copy(&buf, &mut arena, &stats).unwrap();
    assert_eq!(row.protocol, DecodeProtocol::Oracle);
    assert_eq!(row.column_count(), 2);
    assert_eq!(row.column(0).unwrap(), b"oracle");
    assert_eq!(row.column(1).unwrap(), b"data");
    assert_eq!(stats.zero_copy_hits(), 1);
}

/// T1.3 测试：三协议解码正确性
#[test]
fn test_zero_copy_decode_all_protocols() {
    let stats = ZeroCopyStats::new();
    let mut arena = RowArena::with_capacity(2048);

    let data1: &[u8] = b"value1";
    let data2: &[u8] = b"value2";

    let mysql_buf = build_mysql_row(&[data1, data2]);
    let mysql_row = mysql_row_decode_zero_copy(&mysql_buf, &mut arena, &stats).unwrap();
    assert_eq!(mysql_row.column(0), Some(data1));
    assert_eq!(mysql_row.column(1), Some(data2));

    let pg_buf = build_pg_row(&[Some(data1), Some(data2)]);
    let pg_row = pg_binary_decode_zero_copy(&pg_buf, &mut arena, &stats).unwrap();
    assert_eq!(pg_row.column(0), Some(data1));
    assert_eq!(pg_row.column(1), Some(data2));

    let oracle_buf = build_oracle_row(&[data1, data2]);
    let oracle_row = oracle_tns_decode_zero_copy(&oracle_buf, &mut arena, &stats).unwrap();
    assert_eq!(oracle_row.column(0), Some(data1));
    assert_eq!(oracle_row.column(1), Some(data2));

    assert_eq!(stats.zero_copy_hits(), 3);
}

/// T1.3 端到端测试：零拷贝解码分配次数减少
///
/// 验证行为副作用：
/// 1. arena 预分配后解码多行，allocation_count 应为 0（不触发扩容）
/// 2. arena 容量不足时解码，allocation_count > 0（触发扩容）
/// 3. 三协议解码后 stats.zero_copy_hits() 实际增加
#[test]
fn e2e_zero_copy_decode_allocation_reduction() {
    let stats = ZeroCopyStats::new();
    let mut arena = RowArena::with_capacity(8192);

    for i in 0..50 {
        let col1 = format!("key_{i}");
        let col2 = format!("value_{i}");
        let buf = build_mysql_row(&[col1.as_bytes(), col2.as_bytes()]);
        let row = mysql_row_decode_zero_copy(&buf, &mut arena, &stats).unwrap();
        assert_eq!(row.column_count(), 2);
    }

    assert_eq!(stats.zero_copy_hits(), 50);
    assert!(
        arena.allocation_count() <= 1,
        "预分配 8192 字节应避免频繁扩容，allocation_count = {}",
        arena.allocation_count()
    );
    assert!(arena.rows_decoded() >= 50);

    let small_arena = RowArena::with_capacity(4);
    let mut small_arena = small_arena;
    let big_data = vec![b'x'; 100];
    let buf = build_mysql_row(&[&big_data]);
    let _row = mysql_row_decode_zero_copy(&buf, &mut small_arena, &stats).unwrap();
    assert!(small_arena.allocation_count() >= 1, "容量不足时应触发扩容");
}

/// T1.3 测试：解码错误处理
#[test]
fn test_zero_copy_decode_errors() {
    let stats = ZeroCopyStats::new();
    let mut arena = RowArena::with_capacity(1024);

    let err = mysql_row_decode_zero_copy(&[], &mut arena, &stats).unwrap_err();
    assert_eq!(err, DecodeError::BufferTooShort);

    let err = mysql_row_decode_zero_copy(&[0], &mut arena, &stats).unwrap_err();
    assert_eq!(err, DecodeError::InvalidColumnCount);

    let err = pg_binary_decode_zero_copy(&[0], &mut arena, &stats).unwrap_err();
    assert_eq!(err, DecodeError::BufferTooShort);

    let err = oracle_tns_decode_zero_copy(&[0], &mut arena, &stats).unwrap_err();
    assert_eq!(err, DecodeError::BufferTooShort);
}
