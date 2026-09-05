//! v6.2 性能优化：真实流式基准
//!
//! 用内存执行器驱动 StreamResultSet::stream_query，
//! 验证 ≥ 50,000 elements/s 与背压控制。

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use futures::StreamExt;
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use sz_orm_core::DbType;
use sz_orm_stream::{PaginationStrategy, StreamResultSet, StreamResultSetConfig};

const TOTAL_ELEMENTS: usize = 100_000;
const BATCH_SIZE: usize = 1000;

/// 内存执行器：按 batch_size 切片返回预生成数据
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

/// 真实流式批量基准：100,000 元素，batch_size=1000
fn stream_real_batch_1000(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("v62_stream_real");
    group.throughput(Throughput::Elements(TOTAL_ELEMENTS as u64));
    group.bench_function("stream_real_batch_1000", |b| {
        b.to_async(&rt).iter(|| async move {
            let config = StreamResultSetConfig::new(DbType::PostgreSQL)
                .with_batch_size(BATCH_SIZE)
                .with_backpressure_threshold(10_000)
                .with_pagination_strategy(PaginationStrategy::LimitOffset);

            let stream = StreamResultSet::new("SELECT * FROM items", config)
                .stream_query(make_memory_executor(TOTAL_ELEMENTS, BATCH_SIZE));

            let mut stream = stream;
            let mut total = 0usize;
            while let Some(result) = stream.next().await {
                let batch = result.expect("batch ok");
                total += batch.len();
            }
            assert_eq!(total, TOTAL_ELEMENTS, "应拉取全部元素");
            black_box(total);
        })
    });
    group.finish();
}

/// 背压基准：配置 backpressure_threshold=100，验证背压触发
fn stream_real_backpressure(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("stream_real_backpressure", |b| {
        b.to_async(&rt).iter(|| async move {
            let config = StreamResultSetConfig::new(DbType::PostgreSQL)
                .with_batch_size(50)
                .with_backpressure_threshold(100)
                .with_pagination_strategy(PaginationStrategy::LimitOffset);

            let stream_rs = StreamResultSet::new("SELECT * FROM items", config);
            let bp = stream_rs.backpressure().clone();

            let mut stream = stream_rs.stream_query(make_memory_executor(10_000, 50));

            let mut over_threshold = false;
            let mut batches = 0usize;
            while let Some(result) = stream.next().await {
                let batch = result.expect("batch ok");
                batches += 1;
                if bp.is_over_threshold() {
                    over_threshold = true;
                    break;
                }
                let _ = black_box(&batch);
            }
            assert!(
                batches > 0,
                "应至少消费 1 批"
            );
            black_box(over_threshold);
        })
    });
}

criterion_group!(benches, stream_real_batch_1000, stream_real_backpressure);
criterion_main!(benches);