//! v6.2 性能优化：流式性能集成测试
//!
//! 验证 StreamResultSet 真实流式拉取吞吐量、背压控制、配置校验。

use futures::StreamExt;
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use sz_orm_core::DbType;
use sz_orm_stream::{PaginationStrategy, StreamResultSet, StreamResultSetConfig};

const TOTAL_ELEMENTS: usize = 100_000;
const BATCH_SIZE: usize = 1000;
const THROUGHPUT_THRESHOLD: usize = 50_000;

fn make_memory_executor(
    total: usize,
    batch_size: usize,
) -> impl Fn(&str, &[Value]) -> futures::future::BoxFuture<'static, Result<Vec<Value>, String>>
       + Send
       + Sync
       + 'static {
    let remaining = Arc::new(AtomicUsize::new(total));
    move |_sql, _params| {
        let rem = remaining.load(Ordering::Relaxed);
        let take = batch_size.min(rem);
        remaining.fetch_sub(take, Ordering::Relaxed);
        Box::pin(async move {
            let batch: Vec<Value> = (0..take)
                .map(|i| serde_json::json!({"id": i, "name": format!("row{i}")}))
                .collect();
            Ok(batch)
        })
    }
}

/// 验证内存执行器流式拉取 100,000 元素的吞吐量 ≥ 50,000 elements/s
#[tokio::test]
async fn stream_throughput_real() {
    let config = StreamResultSetConfig::new(DbType::PostgreSQL)
        .with_batch_size(BATCH_SIZE)
        .with_backpressure_threshold(10_000)
        .with_pagination_strategy(PaginationStrategy::LimitOffset);

    let stream = StreamResultSet::new("SELECT * FROM items", config)
        .stream_query(make_memory_executor(TOTAL_ELEMENTS, BATCH_SIZE));

    let start = Instant::now();
    let mut stream = stream;
    let mut total = 0usize;
    while let Some(result) = stream.next().await {
        let batch = result.expect("batch ok");
        total += batch.len();
    }
    let elapsed = start.elapsed();

    assert_eq!(total, TOTAL_ELEMENTS, "应拉取全部元素");

    let elapsed_secs = elapsed.as_secs_f64();
    let throughput = (TOTAL_ELEMENTS as f64 / elapsed_secs) as usize;
    assert!(
        throughput >= THROUGHPUT_THRESHOLD,
        "吞吐量应 ≥ {THROUGHPUT_THRESHOLD} elements/s，实际: {throughput} elements/s（{elapsed_secs:.3}s）"
    );
}

/// 验证背压触发后流式暂停
#[tokio::test]
async fn stream_backpressure_pauses() {
    let config = StreamResultSetConfig::new(DbType::PostgreSQL)
        .with_batch_size(50)
        .with_backpressure_threshold(2)
        .with_pagination_strategy(PaginationStrategy::LimitOffset);

    let stream_rs = StreamResultSet::new("SELECT * FROM items", config);
    let bp = stream_rs.backpressure().clone();

    let mut stream = stream_rs.stream_query(make_memory_executor(10_000, 50));

    let mut batches = 0usize;
    while let Some(result) = stream.next().await {
        let batch = result.expect("batch ok");
        batches += 1;
        if batches >= 2 {
            break;
        }
        let _ = &batch;
    }

    assert!(batches >= 2, "应至少消费 2 批");
    assert!(
        bp.is_over_threshold(),
        "消费 {batches} 批后背压应触发，pending: {}",
        bp.pending()
    );
}

/// 验证 Keyset 策略未设 keyset_column 时 validate() 返回 Err
#[test]
fn stream_keyset_validation() {
    let config = StreamResultSetConfig::new(DbType::PostgreSQL)
        .with_batch_size(BATCH_SIZE)
        .with_pagination_strategy(PaginationStrategy::Keyset);

    let result = config.validate();
    assert!(
        result.is_err(),
        "Keyset 策略未设 keyset_column 时 validate() 应返回 Err"
    );
    let err_msg = result.unwrap_err();
    assert!(
        err_msg.contains("keyset_column"),
        "错误信息应包含 keyset_column: {err_msg}"
    );
}

/// 验证 batch_size=0 时被 clamp 为 1，不 panic
#[test]
fn stream_batch_size_clamp() {
    let config = StreamResultSetConfig::new(DbType::PostgreSQL)
        .with_batch_size(0)
        .with_pagination_strategy(PaginationStrategy::LimitOffset);

    assert_eq!(
        config.batch_size(),
        1,
        "batch_size=0 应被 clamp 为 1"
    );
    assert!(
        config.validate().is_ok(),
        "clamp 后 validate() 应通过"
    );
}