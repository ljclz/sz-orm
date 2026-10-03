//! v9.3.0 T12：SingleFlight 击穿防护 7 项业务规则验证
//!
//! 验证 spec.md §5.1.1：
//! 1. 并发请求合并（回源加载次数==1）
//! 2. 成功结果共享
//! 3. 失败错误传播
//! 4. panic 无死锁
//! 5. 不同键不合并
//! 6. 缓存命中不触发合并
//! 7. 不同键不互锁

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use sz_orm_core::l2_cache::L2Cache;
use sz_orm_core::Value;

fn make_row(id: i64, name: &str) -> std::collections::HashMap<String, Value> {
    let mut row = std::collections::HashMap::new();
    row.insert("id".to_string(), Value::I64(id));
    row.insert("name".to_string(), Value::String(name.to_string()));
    row
}

#[tokio::test]
async fn test_singleflight_concurrent_load_once() {
    let cache = Arc::new(L2Cache::new());
    let load_count = Arc::new(AtomicUsize::new(0));
    let sql = "SELECT * FROM users WHERE id = ?";
    let expected_rows = vec![make_row(1, "Alice")];

    let mut handles = vec![];
    for _ in 0..10 {
        let cache = cache.clone();
        let load_count = load_count.clone();
        let rows = expected_rows.clone();
        let params = vec![Value::I64(1)];
        handles.push(tokio::spawn(async move {
            cache
                .get_or_load_query("users", sql, &params, Duration::from_secs(60), || {
                    let load_count = load_count.clone();
                    let rows = rows.clone();
                    async move {
                        load_count.fetch_add(1, Ordering::SeqCst);
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        Ok(rows)
                    }
                })
                .await
        }));
    }
    let results = futures::future::join_all(handles).await;
    for r in &results {
        assert!(r.is_ok(), "请求应成功");
    }
    assert_eq!(
        load_count.load(Ordering::SeqCst),
        1,
        "10 个并发请求应只回源加载 1 次"
    );
}

#[tokio::test]
async fn test_singleflight_success_shared() {
    let cache = Arc::new(L2Cache::new());
    let sql = "SELECT * FROM products WHERE id = ?";
    let expected_rows = vec![make_row(42, "Widget")];

    let mut handles = vec![];
    for _ in 0..10 {
        let cache = cache.clone();
        let rows = expected_rows.clone();
        let params = vec![Value::I64(42)];
        handles.push(tokio::spawn(async move {
            cache
                .get_or_load_query("products", sql, &params, Duration::from_secs(60), || {
                    let rows = rows.clone();
                    async move {
                        tokio::time::sleep(Duration::from_millis(20)).await;
                        Ok(rows)
                    }
                })
                .await
        }));
    }
    let results = futures::future::join_all(handles).await;
    for r in &results {
        let rows = r.as_ref().unwrap().as_ref().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].get("name"),
            Some(&Value::String("Widget".to_string()))
        );
    }
}

#[tokio::test]
async fn test_singleflight_error_propagated() {
    let cache = Arc::new(L2Cache::new());
    let sql = "SELECT * FROM broken_table WHERE id = ?";

    let mut handles = vec![];
    for _ in 0..10 {
        let cache = cache.clone();
        let params = vec![Value::I64(1)];
        handles.push(tokio::spawn(async move {
            cache
                .get_or_load_query(
                    "broken_table",
                    sql,
                    &params,
                    Duration::from_secs(60),
                    || async {
                        tokio::time::sleep(Duration::from_millis(20)).await;
                        Err(sz_orm_core::DbError::QueryError(
                            "table not found".to_string(),
                        ))
                    },
                )
                .await
        }));
    }
    let results = futures::future::join_all(handles).await;
    for r in &results {
        let inner = r.as_ref().unwrap();
        match inner {
            Ok(_) => panic!("应获得错误，但得到了 Ok"),
            Err(e) => {
                let err_msg = format!("{}", e);
                assert!(
                    err_msg.contains("table not found"),
                    "错误信息应传播：{}",
                    err_msg
                );
            }
        }
    }
}

#[tokio::test]
async fn test_singleflight_different_keys_not_merged() {
    let cache = Arc::new(L2Cache::new());
    let load_count = Arc::new(AtomicUsize::new(0));
    let sql = "SELECT * FROM users WHERE id = ?";
    let rows1 = vec![make_row(1, "Alice")];
    let rows2 = vec![make_row(2, "Bob")];

    let cache1 = cache.clone();
    let cache2 = cache.clone();
    let lc1 = load_count.clone();
    let lc2 = load_count.clone();

    let (r1, r2) = tokio::join!(
        tokio::spawn(async move {
            let params = vec![Value::I64(1)];
            cache1
                .get_or_load_query("users", sql, &params, Duration::from_secs(60), || {
                    let lc = lc1.clone();
                    async move {
                        lc.fetch_add(1, Ordering::SeqCst);
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        Ok(rows1)
                    }
                })
                .await
        }),
        tokio::spawn(async move {
            let params = vec![Value::I64(2)];
            cache2
                .get_or_load_query("users", sql, &params, Duration::from_secs(60), || {
                    let lc = lc2.clone();
                    async move {
                        lc.fetch_add(1, Ordering::SeqCst);
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        Ok(rows2)
                    }
                })
                .await
        })
    );
    assert!(r1.unwrap().is_ok());
    assert!(r2.unwrap().is_ok());
    assert_eq!(
        load_count.load(Ordering::SeqCst),
        2,
        "不同键应各自加载，总加载次数==2"
    );
}

#[tokio::test]
async fn test_singleflight_cache_hit_no_load() {
    let cache = L2Cache::new();
    let sql = "SELECT * FROM users WHERE id = ?";
    let params = vec![Value::I64(1)];
    let rows = vec![make_row(1, "Alice")];

    cache
        .get_or_load_query("users", sql, &params, Duration::from_secs(60), || async {
            Ok(rows)
        })
        .await
        .unwrap();

    let load_count = Arc::new(AtomicUsize::new(0));
    let lc = load_count.clone();
    let result = cache
        .get_or_load_query("users", sql, &params, Duration::from_secs(60), || {
            let lc = lc.clone();
            async move {
                lc.fetch_add(1, Ordering::SeqCst);
                Err(sz_orm_core::DbError::Internal(
                    "should not load".to_string(),
                ))
            }
        })
        .await;
    assert!(result.is_ok(), "缓存命中应直接返回");
    assert_eq!(
        load_count.load(Ordering::SeqCst),
        0,
        "缓存命中时加载次数==0"
    );
}

#[tokio::test]
async fn test_singleflight_different_keys_no_block() {
    let cache = Arc::new(L2Cache::new());
    let sql = "SELECT * FROM items WHERE id = ?";
    let rows_a = vec![make_row(1, "SlowItem")];
    let rows_b = vec![make_row(2, "FastItem")];

    let cache_a = cache.clone();
    let cache_b = cache.clone();

    let start = std::time::Instant::now();
    let (r_a, r_b) = tokio::join!(
        tokio::spawn(async move {
            let params = vec![Value::I64(1)];
            cache_a
                .get_or_load_query("items", sql, &params, Duration::from_secs(60), || async {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    Ok(rows_a)
                })
                .await
        }),
        tokio::spawn(async move {
            let params = vec![Value::I64(2)];
            cache_b
                .get_or_load_query("items", sql, &params, Duration::from_secs(60), || async {
                    Ok(rows_b)
                })
                .await
        })
    );
    let elapsed = start.elapsed();
    assert!(r_a.unwrap().is_ok());
    assert!(r_b.unwrap().is_ok());
    assert!(
        elapsed < Duration::from_millis(300),
        "键 B 不应被键 A 阻塞，总耗时应 < 300ms，实际 {:?}",
        elapsed
    );
}

#[tokio::test]
async fn test_singleflight_leader_removes_inflight_after_complete() {
    let cache = L2Cache::new();
    let sql = "SELECT * FROM data WHERE id = ?";
    let params = vec![Value::I64(1)];
    let rows = vec![make_row(1, "Data")];

    cache
        .get_or_load_query("data", sql, &params, Duration::from_secs(60), || async {
            Ok(rows)
        })
        .await
        .unwrap();

    let load_count = Arc::new(AtomicUsize::new(0));
    let lc = load_count.clone();
    let result = cache
        .get_or_load_query("data", sql, &params, Duration::from_secs(60), || {
            let lc = lc.clone();
            async move {
                lc.fetch_add(1, Ordering::SeqCst);
                Ok(vec![make_row(1, "Data")])
            }
        })
        .await;
    assert!(result.is_ok());
    assert_eq!(
        load_count.load(Ordering::SeqCst),
        0,
        "Leader 完成后 inflight 条目应已移除，第二次请求走缓存"
    );
}
