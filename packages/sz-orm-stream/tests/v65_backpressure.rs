#![cfg(feature = "stream-resultset")]
//! v6.5.0 集成测试：背压生效验证
//!
//! 消费者慢速处理，生产者快速生产，验证背压生效（spec.md 5.3.1 业务规则 3）。
//! 断言：当待处理行数达到阈值时，is_over_threshold 返回 true，next_row 阻塞。

use std::collections::HashMap;
use std::time::{Duration, Instant};

use futures::stream::{self};

use sz_orm_core::row_stream::{AsyncRowStream, CursorRowStream};
use sz_orm_core::{QueryStreamItem, Value};
use sz_orm_stream::backpressure_stream::BackpressureRowStream;

fn make_row(id: i64) -> HashMap<String, Value> {
    let mut row = HashMap::new();
    row.insert("id".to_string(), Value::I64(id));
    row
}

#[tokio::test]
#[ignore = "v6.5.0 集成测试：需 --ignored 标志运行"]
async fn test_backpressure_triggers_at_threshold() {
    let threshold = 10usize;
    let total_rows = threshold + 5;

    let stream = stream::iter((0..total_rows as i64).map(|i| Ok(make_row(i)) as QueryStreamItem));
    let cursor_stream = CursorRowStream::new(stream);
    let mut bp_stream = BackpressureRowStream::new(cursor_stream, threshold);

    for _ in 0..threshold {
        let result = bp_stream.next_row().await;
        assert!(result.is_some(), "应成功拉取行");
    }

    assert_eq!(bp_stream.pending(), threshold);
    assert!(
        bp_stream.is_over_threshold(),
        "拉取 {threshold} 行后应超过阈值（pending={}, threshold={}）",
        bp_stream.pending(),
        bp_stream.threshold()
    );

    for _ in 0..threshold {
        bp_stream.ack();
    }
    assert_eq!(bp_stream.pending(), 0);
    assert!(!bp_stream.is_over_threshold());

    for _ in 0..5 {
        let result = bp_stream.next_row().await;
        assert!(result.is_some(), "ack 后应能继续拉取");
        bp_stream.ack();
    }

    assert!(bp_stream.next_row().await.is_none(), "流应结束");
    println!(
        "背压触发验证通过：拉取 {threshold} 行后 pending={threshold} >= threshold={threshold}"
    );
}

#[tokio::test]
#[ignore = "v6.5.0 集成测试：需 --ignored 标志运行"]
async fn test_backpressure_next_row_blocks_when_over_threshold() {
    let threshold = 5usize;

    let stream = stream::iter((0..100i64).map(|i| Ok(make_row(i)) as QueryStreamItem));
    let cursor_stream = CursorRowStream::new(stream);
    let mut bp_stream = BackpressureRowStream::new(cursor_stream, threshold);

    for _ in 0..threshold {
        let _ = bp_stream.next_row().await;
    }
    assert!(bp_stream.is_over_threshold());

    let timeout_result =
        tokio::time::timeout(Duration::from_millis(100), bp_stream.next_row()).await;

    assert!(
        timeout_result.is_err(),
        "pending >= threshold 时 next_row 应阻塞（100ms 超时）"
    );

    bp_stream.ack();
    let result = bp_stream.next_row().await;
    assert!(result.is_some(), "ack 后 next_row 应恢复");
    println!("next_row 阻塞验证通过：pending >= threshold 时阻塞，ack 后恢复");
}

#[tokio::test]
#[ignore = "v6.5.0 集成测试：需 --ignored 标志运行"]
async fn test_backpressure_no_trigger_on_fast_consumer() {
    let total_rows = 100usize;
    let threshold = 10usize;

    let stream = stream::iter((0..total_rows as i64).map(|i| Ok(make_row(i)) as QueryStreamItem));
    let cursor_stream = CursorRowStream::new(stream);
    let mut bp_stream = BackpressureRowStream::new(cursor_stream, threshold);

    let mut count = 0usize;
    let mut backpressure_triggered_count = 0usize;

    while let Some(result) = bp_stream.next_row().await {
        let _row = result.expect("行不应有错误");
        count += 1;

        if bp_stream.is_over_threshold() {
            backpressure_triggered_count += 1;
        }

        bp_stream.ack();
    }

    assert_eq!(count, total_rows);
    assert_eq!(
        backpressure_triggered_count, 0,
        "快速消费者每行立即 ack，不应触发背压"
    );
    println!("快速消费者：背压触发 {backpressure_triggered_count} 次");
}

#[tokio::test]
#[ignore = "v6.5.0 集成测试：需 --ignored 标志运行"]
async fn test_backpressure_memory_bounded() {
    let total_rows = 1_000usize;
    let threshold = 50usize;

    let stream = stream::iter((0..total_rows as i64).map(|i| Ok(make_row(i)) as QueryStreamItem));
    let cursor_stream = CursorRowStream::new(stream);
    let mut bp_stream = BackpressureRowStream::new(cursor_stream, threshold);

    let start = Instant::now();
    let mut count = 0usize;
    let mut max_pending = 0usize;

    while let Some(result) = bp_stream.next_row().await {
        let _row = result.expect("行不应有错误");
        count += 1;

        let pending = bp_stream.pending();
        max_pending = max_pending.max(pending);

        bp_stream.ack();
    }

    let elapsed = start.elapsed();

    assert_eq!(count, total_rows);
    assert!(
        max_pending <= 1,
        "每行立即 ack，max_pending 应 <= 1（实际 {max_pending}）"
    );
    println!("消费 {count} 行，最大待处理: {max_pending}（阈值: {threshold}），耗时: {elapsed:?}");
}

#[tokio::test]
#[ignore = "v6.5.0 集成测试：需 --ignored 标志运行"]
async fn test_backpressure_threshold_zero_rejects() {
    let stream = stream::iter((0..10i64).map(|i| Ok(make_row(i)) as QueryStreamItem));
    let cursor_stream = CursorRowStream::new(stream);
    let mut bp_stream = BackpressureRowStream::new(cursor_stream, 0);

    let result = bp_stream.next_row().await;
    assert!(
        result.is_none(),
        "threshold=0 应直接返回 None（拒绝所有推送）"
    );
}
