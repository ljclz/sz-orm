//! v6.8.0 性能差分正确性测试 — 优化路径 vs 基线路径结果一致性验证
//!
//! 验证零拷贝管线、SIMD 向量化等性能优化路径与基线（拷贝/标量）路径
//! 在各种输入下产生完全一致的结果，确保性能优化不改变语义。

#![cfg(feature = "perf-diff")]

use std::collections::HashMap;

use sz_orm_core::simd::{
    batch_compare_eq, batch_decode_integers, detect, scalar_compare_eq, scalar_decode_integers,
};
use sz_orm_core::zero_copy_pipeline::{ZeroCopyPipeline, ZeroCopyRow};
use sz_orm_core::Value;

fn make_rows(n: usize, cols: &[&str]) -> Vec<HashMap<String, Value>> {
    (0..n)
        .map(|i| {
            let mut row = HashMap::new();
            for col in cols {
                row.insert(col.to_string(), Value::String(format!("val_{}_{}", i, col)));
            }
            row
        })
        .collect()
}

fn make_i64_buf(values: &[i64]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(values.len() * 8);
    for v in values {
        buf.extend_from_slice(&v.to_le_bytes());
    }
    buf
}

#[test]
fn zero_copy_vs_copy_string_values() {
    let pipeline = ZeroCopyPipeline::new();
    let columns = vec!["id".to_string(), "name".to_string()];
    let rows = make_rows(100, &["id", "name"]);
    let mut stream = pipeline.stream_rows(rows.clone(), &columns);

    for (i, original_row) in rows.iter().enumerate() {
        let zc_row = stream.next().unwrap();
        for col in &columns {
            let original_val = original_row.get(col).unwrap();
            let expected = match original_val {
                Value::String(s) => s.as_bytes(),
                _ => panic!("expected string"),
            };
            let zc_val = zc_row.get(col).unwrap();
            assert_eq!(zc_val, expected, "mismatch at row {} col {}", i, col);
        }
    }
}

#[test]
fn zero_copy_vs_copy_bytes_values() {
    let pipeline = ZeroCopyPipeline::new();
    let columns = vec!["data".to_string()];
    let mut rows = Vec::new();
    for i in 0..50 {
        let mut row = HashMap::new();
        row.insert(
            "data".to_string(),
            Value::Bytes(format!("byte_payload_{}", i).into_bytes()),
        );
        rows.push(row);
    }

    let originals: Vec<_> = rows
        .iter()
        .map(|r| match r.get("data").unwrap() {
            Value::Bytes(b) => b.clone(),
            _ => panic!(),
        })
        .collect();

    let mut stream = pipeline.stream_rows(rows, &columns);
    for (i, expected) in originals.iter().enumerate() {
        let zc_row = stream.next().unwrap();
        let zc_val = zc_row.get("data").unwrap();
        assert_eq!(zc_val, expected.as_slice(), "bytes mismatch at row {}", i);
    }
}

#[test]
fn zero_copy_vs_copy_mixed_types() {
    let pipeline = ZeroCopyPipeline::new();
    let columns = vec!["name".to_string(), "age".to_string(), "score".to_string()];

    let mut rows = Vec::new();
    for i in 0..30 {
        let mut row = HashMap::new();
        row.insert("name".to_string(), Value::String(format!("user_{}", i)));
        row.insert("age".to_string(), Value::I64(i as i64));
        row.insert("score".to_string(), Value::F64(i as f64 * 1.5));
        rows.push(row);
    }

    let mut stream = pipeline.stream_rows(rows, &columns);
    let collected: Vec<_> = stream.by_ref().collect();
    assert_eq!(collected.len(), 30);

    for row in &collected {
        assert_eq!(row.column_count(), 3);
        assert!(row.get("name").is_some());
        assert!(row.get("age").is_some());
        assert!(row.get("score").is_some());
    }
}

#[test]
fn zero_copy_vs_copy_empty_result() {
    let pipeline = ZeroCopyPipeline::new();
    let columns = vec!["id".to_string()];
    let stream = pipeline.stream_rows(Vec::new(), &columns);
    assert_eq!(stream.total_rows(), 0);
    assert_eq!(stream.remaining(), 0);
    let collected: Vec<_> = stream.collect();
    assert!(collected.is_empty());
}

#[test]
fn zero_copy_vs_copy_large_result() {
    let pipeline = ZeroCopyPipeline::new();
    let columns: Vec<String> = (0..10).map(|i| format!("col_{}", i)).collect();
    let col_refs: Vec<&str> = columns.iter().map(|s| s.as_str()).collect();
    let rows = make_rows(10000, &col_refs);

    let originals: Vec<_> = rows
        .iter()
        .map(|r| {
            columns
                .iter()
                .map(|c| match r.get(c).unwrap() {
                    Value::String(s) => s.clone(),
                    _ => panic!(),
                })
                .collect::<Vec<_>>()
        })
        .collect();

    let mut stream = pipeline.stream_rows(rows, &columns);
    for (i, expected_row) in originals.iter().enumerate() {
        let zc_row = stream.next().unwrap();
        for (j, col) in columns.iter().enumerate() {
            let zc_val = zc_row.get(col).unwrap();
            assert_eq!(
                zc_val,
                expected_row[j].as_bytes(),
                "mismatch at row {} col {}",
                i,
                col
            );
        }
    }
}

#[test]
fn zero_copy_stats_consistency() {
    let pipeline = ZeroCopyPipeline::new();
    let columns = vec!["name".to_string(), "age".to_string()];
    let mut rows = Vec::new();
    for i in 0..100 {
        let mut row = HashMap::new();
        row.insert("name".to_string(), Value::String(format!("u_{}", i)));
        row.insert("age".to_string(), Value::I64(i));
        rows.push(row);
    }

    let _stream = pipeline.stream_rows(rows, &columns);
    let stats = pipeline.stats();
    let total = stats.total();
    let hits = stats.zero_copy_hits();
    let misses = stats.fallback_copies();

    assert_eq!(hits + misses, total, "hit + miss must equal total");
    assert_eq!(total, 200, "100 rows x 2 columns = 200 total ops");
    assert_eq!(hits, 100, "string column hits");
    assert_eq!(misses, 100, "i64 column fallbacks");
}

#[test]
fn simd_vs_scalar_decode_diff() {
    let test_cases: Vec<Vec<i64>> = vec![
        vec![],
        vec![0],
        vec![1, 2, 3],
        vec![-1, -2, -3],
        vec![i64::MAX, i64::MIN, 0],
        vec![42; 1024],
        (0..5000).map(|i| i * 3 - 7).collect(),
    ];

    let avail = detect();
    for values in &test_cases {
        let buf = make_i64_buf(values);
        let scalar = scalar_decode_integers(&buf, values.len());
        let simd = batch_decode_integers(&buf, values.len(), avail);
        assert_eq!(scalar, simd, "decode mismatch for len={}", values.len());
        assert_eq!(simd, *values, "decode incorrect for len={}", values.len());
    }
}

#[test]
fn simd_vs_scalar_compare_eq_diff() {
    let values: Vec<i64> = (0..1000).collect();
    let targets: Vec<i64> = vec![0, 500, 999, -1, 1000];

    let avail = detect();
    for target in &targets {
        let scalar = scalar_compare_eq(&values, *target);
        let simd = batch_compare_eq(&values, *target, avail);
        assert_eq!(scalar, simd, "compare_eq mismatch for target={}", target);

        for i in 0..values.len() {
            let expected = values[i] == *target;
            assert_eq!(
                scalar[i], expected,
                "scalar wrong at {} target={}",
                i, target
            );
            assert_eq!(simd[i], expected, "simd wrong at {} target={}", i, target);
        }
    }
}

#[test]
fn zero_copy_row_get_column() {
    let pipeline = ZeroCopyPipeline::new();
    let columns = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    let mut row = HashMap::new();
    row.insert("a".to_string(), Value::String("alpha".into()));
    row.insert("b".to_string(), Value::String("beta".into()));
    row.insert("c".to_string(), Value::String("gamma".into()));

    let mut stream = pipeline.stream_rows(vec![row], &columns);
    let zc_row = stream.next().unwrap();

    assert_eq!(zc_row.get("a").unwrap(), b"alpha");
    assert_eq!(zc_row.get("b").unwrap(), b"beta");
    assert_eq!(zc_row.get("c").unwrap(), b"gamma");
    assert!(zc_row.get("nonexistent").is_none());
    assert_eq!(zc_row.column_count(), 3);
}

#[test]
fn zero_copy_stream_iterator_consistency() {
    let pipeline = ZeroCopyPipeline::new();
    let columns = vec!["id".to_string()];
    let rows = make_rows(500, &["id"]);
    let mut stream = pipeline.stream_rows(rows, &columns);

    let mut iterator_collected: Vec<ZeroCopyRow> = Vec::new();
    let mut next_row_collected: Vec<ZeroCopyRow> = Vec::new();

    for _ in 0..250 {
        if let Some(row) = stream.next() {
            iterator_collected.push(row);
        }
    }
    while let Some(row) = stream.next_row() {
        next_row_collected.push(row.clone());
    }

    assert_eq!(iterator_collected.len(), 250);
    assert_eq!(next_row_collected.len(), 250);
    assert_eq!(stream.remaining(), 0);
    assert_eq!(stream.total_rows(), 500);
}
