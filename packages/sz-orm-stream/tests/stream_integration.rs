#![cfg(feature = "stream-resultset")]
//! 流式结果集集成测试：内存数据源 + StreamResultSet 流式消费
//!
//! 验证 KeysetPaginator 与 StreamResultSet 在全流程中的配合：
//! 数据源分页返回 → 流式逐批产出 → 全部行送达。

use futures::StreamExt;
use serde_json::{json, Value};
use sz_orm_core::DbType;
use sz_orm_stream::{KeysetPaginator, StreamResultSet, StreamResultSetConfig};

/// 从 keyset 分页 SQL 中解析游标值（"WHERE id > 9" → 9）
fn parse_keyset_cursor(sql: &str) -> i64 {
    sql.split("id > ")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(-1)
}

/// 模拟内存数据源：50 行数据
fn memory_data_source() -> Vec<Value> {
    (0..50)
        .map(|i| json!({"id": i, "name": format!("user_{i}")}))
        .collect()
}

fn make_stream(data: Vec<Value>) -> impl futures::Stream<Item = Result<Vec<Value>, String>> {
    let config = StreamResultSetConfig::new(DbType::Sqlite)
        .with_batch_size(10)
        .with_keyset_column("id")
        .with_pagination_strategy(sz_orm_stream::PaginationStrategy::Keyset);
    StreamResultSet::new("SELECT * FROM t", config).stream_query(move |sql, _params| {
        let data = data.clone();
        let last_key = parse_keyset_cursor(sql);
        Box::pin(async move {
            let page: Vec<Value> = data
                .iter()
                .filter(|row| {
                    row.get("id")
                        .and_then(|v| v.as_i64())
                        .map(|id| id > last_key)
                        .unwrap_or(false)
                })
                .take(10)
                .cloned()
                .collect();
            Ok(page)
        })
    })
}

#[tokio::test]
async fn stream_all_rows_from_memory_source() {
    let data = memory_data_source();
    let total = data.len();

    let collected: Vec<Result<Vec<Value>, String>> = make_stream(data).collect().await;
    let collected: Vec<Vec<Value>> = collected.into_iter().collect::<Result<_, _>>().unwrap();
    let rows: usize = collected.iter().map(|b| b.len()).sum();
    assert_eq!(rows, total, "stream should deliver all {total} rows");
}

#[tokio::test]
async fn stream_pages_follow_keyset_order() {
    let data = memory_data_source();
    let mut pages: Vec<i64> = Vec::new();

    let mut stream = Box::pin(make_stream(data));
    while let Some(batch) = stream.next().await {
        let batch = batch.unwrap();
        let page_ids: Vec<i64> = batch
            .iter()
            .filter_map(|row| row.get("id").and_then(|v| v.as_i64()))
            .collect();
        pages.extend(page_ids);
    }

    // 50 行按 id 升序完整送达
    assert_eq!(pages.len(), 50);
    assert!(
        pages.windows(2).all(|w| w[0] < w[1]),
        "keyset order should be ascending"
    );
}

#[tokio::test]
async fn keyset_paginator_full_cycle_with_stream_config() {
    let mut paginator = KeysetPaginator::new("id", 10);
    let config = StreamResultSetConfig::new(DbType::Sqlite)
        .with_batch_size(10)
        .with_keyset_column("id");

    // 用 config 同步 paginator 的 batch 配置
    assert_eq!(paginator.batch_size, config.batch_size);

    let mut fetched = 0usize;
    let mut pages = 0usize;
    loop {
        let sql = paginator.build_next_page_sql("SELECT * FROM t");
        assert!(sql.contains("LIMIT 10"));
        fetched += 10;
        pages += 1;
        paginator.update_last_key(&json!(fetched as i64));
        paginator.mark_batch_result(10);
        if !paginator.has_more() || fetched >= 50 {
            break;
        }
    }
    assert_eq!(fetched, 50);
    assert_eq!(pages, 5);
}
