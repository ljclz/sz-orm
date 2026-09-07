#![cfg(feature = "parallel-query")]
//! v6.5.0 集成测试：并行查询加速比验证
//!
//! 对 3 个独立查询（各 100ms 延迟）并行执行 vs 串行执行，
//! 断言加速比 ≥ 2.1（3 × 70%，spec.md 4.1.1）。

use std::time::Instant;

use futures::future::BoxFuture;
use sz_orm_parallel::{config::ParallelQueryConfig, outcome::QueryOutcome, parallel_queries};

async fn simulated_query(delay_ms: u64, label: &str) -> Result<QueryOutcome<Vec<String>>, String> {
    tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
    Ok(QueryOutcome::new(vec![label.to_string()], 1, delay_ms))
}

fn make_query(
    delay_ms: u64,
    label: &'static str,
) -> Box<dyn FnOnce() -> BoxFuture<'static, Result<QueryOutcome<Vec<String>>, String>> + Send> {
    Box::new(move || Box::pin(async move { simulated_query(delay_ms, label).await }))
}

#[tokio::test]
#[ignore = "v6.5.0 集成测试：需 --ignored 标志运行"]
async fn test_parallel_acceleration_ratio() {
    let delay_ms = 100u64;
    let n = 3usize;

    let serial_start = Instant::now();
    for i in 0..n {
        let _ = simulated_query(delay_ms, &format!("q{i}")).await;
    }
    let serial_elapsed = serial_start.elapsed();

    let queries: Vec<
        Box<dyn FnOnce() -> BoxFuture<'static, Result<QueryOutcome<Vec<String>>, String>> + Send>,
    > = (0..n)
        .map(|i| make_query(delay_ms, Box::leak(format!("q{i}").into_boxed_str())))
        .collect();

    let parallel_start = Instant::now();
    let results = parallel_queries(queries, ParallelQueryConfig::default())
        .await
        .expect("parallel_queries 不应失败");
    let parallel_elapsed = parallel_start.elapsed();

    for (i, result) in results.iter().enumerate() {
        assert!(result.is_ok(), "query[{i}] 应成功: {:?}", result);
    }

    let serial_ms = serial_elapsed.as_millis() as f64;
    let parallel_ms = parallel_elapsed.as_millis() as f64;
    let speedup = serial_ms / parallel_ms;

    println!("串行: {serial_ms:.1}ms, 并行: {parallel_ms:.1}ms, 加速比: {speedup:.2}x");

    assert!(
        speedup >= 2.1,
        "加速比 {speedup:.2}x < 2.1x（串行 {serial_ms:.1}ms vs 并行 {parallel_ms:.1}ms）"
    );
}

#[tokio::test]
#[ignore = "v6.5.0 集成测试：需 --ignored 标志运行"]
async fn test_parallel_10_queries_acceleration() {
    let delay_ms = 50u64;
    let n = 10usize;

    let serial_start = Instant::now();
    for i in 0..n {
        let _ = simulated_query(delay_ms, &format!("q{i}")).await;
    }
    let serial_elapsed = serial_start.elapsed();

    let queries: Vec<
        Box<dyn FnOnce() -> BoxFuture<'static, Result<QueryOutcome<Vec<String>>, String>> + Send>,
    > = (0..n)
        .map(|i| make_query(delay_ms, Box::leak(format!("q{i}").into_boxed_str())))
        .collect();

    let parallel_start = Instant::now();
    let results = parallel_queries(queries, ParallelQueryConfig::default())
        .await
        .expect("parallel_queries 不应失败");
    let parallel_elapsed = parallel_start.elapsed();

    assert_eq!(results.len(), n);
    for (i, result) in results.iter().enumerate() {
        assert!(result.is_ok(), "query[{i}] 应成功");
    }

    let serial_ms = serial_elapsed.as_millis() as f64;
    let parallel_ms = parallel_elapsed.as_millis() as f64;
    let speedup = serial_ms / parallel_ms;

    println!("10 查询 - 串行: {serial_ms:.1}ms, 并行: {parallel_ms:.1}ms, 加速比: {speedup:.2}x");

    assert!(speedup >= 3.0, "10 查询加速比 {speedup:.2}x < 3.0x");
}
