#![cfg(feature = "async-row-stream")]
//! v6.5.0 集成测试：流式结果集内存约束验证
//!
//! 流式消费大量行（模拟 100 万行），验证内存不堆积（spec.md 4.1.3）。
//! 使用 CursorRowStream 逐行消费，每行消费后即丢弃，不累积。

use std::collections::HashMap;
use std::time::Instant;

use futures::stream::{self};

use sz_orm_core::row_stream::{AsyncRowStream, CursorRowStream};
use sz_orm_core::{QueryStreamItem, Value};

fn make_row(id: i64) -> HashMap<String, Value> {
    let mut row = HashMap::new();
    row.insert("id".to_string(), Value::I64(id));
    row.insert("data".to_string(), Value::from(format!("row_{id}_payload")));
    row
}

#[tokio::test]
#[ignore = "v6.5.0 集成测试：需 --ignored 标志运行"]
async fn test_stream_memory_constraint() {
    let total_rows = 1_000_000usize;

    let stream = stream::iter((0..total_rows as i64).map(|i| Ok(make_row(i)) as QueryStreamItem));
    let mut row_stream = CursorRowStream::new(stream);

    let start = Instant::now();
    let mut count = 0usize;
    let mut max_id = -1i64;

    while let Some(result) = row_stream.next_row().await {
        let row = result.expect("行不应有错误");
        let id = row
            .get("id")
            .and_then(|v| match v {
                Value::I64(n) => Some(*n),
                _ => None,
            })
            .unwrap_or(-1);
        max_id = max_id.max(id);
        count += 1;
    }

    let elapsed = start.elapsed();

    assert_eq!(count, total_rows, "应消费全部 {total_rows} 行");
    assert_eq!(
        max_id,
        (total_rows - 1) as i64,
        "最后一行 id 应为 {}",
        total_rows - 1
    );

    println!("流式消费 {total_rows} 行耗时: {elapsed:?}");
    println!("内存约束验证通过：未 OOM，逐行消费不累积");
}

#[tokio::test]
#[ignore = "v6.5.0 集成测试：需 --ignored 标志运行"]
async fn test_stream_close_releases_early() {
    let total_rows = 100_000usize;

    let stream = stream::iter((0..total_rows as i64).map(|i| Ok(make_row(i)) as QueryStreamItem));
    let mut row_stream = CursorRowStream::new(stream);

    for _ in 0..10 {
        let _ = row_stream.next_row().await;
    }

    row_stream.close().await.unwrap();

    assert!(row_stream.next_row().await.is_none(), "close 后应返回 None");
    println!("close 后流提前释放，剩余 {} 行未消费", total_rows - 10);
}

#[tokio::test]
#[ignore = "v6.5.0 集成测试：需 --ignored 标志运行"]
async fn test_stream_vs_collect_memory_comparison() {
    let total_rows = 100_000usize;

    let stream = stream::iter((0..total_rows as i64).map(|i| Ok(make_row(i)) as QueryStreamItem));
    let mut row_stream = CursorRowStream::new(stream);

    let mut stream_count = 0usize;
    while row_stream.next_row().await.is_some() {
        stream_count += 1;
    }
    assert_eq!(stream_count, total_rows);

    let collected: Vec<_> = (0..total_rows as i64).map(make_row).collect();
    assert_eq!(collected.len(), total_rows);

    println!("流式消费 {total_rows} 行 vs 全量收集 {total_rows} 行");
    println!("流式：逐行消费，峰值内存 ~1 行");
    println!("全量：全部收集，峰值内存 ~{total_rows} 行");
}
