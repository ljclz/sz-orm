//! W3-6 PERF-POOL-01：连接池 IO 复用接线测试
//!
//! 验证 `IoReuseChannel::execute_reuse` 端到端接线：
//! - 首次执行 SQL → miss → prepare → 缓存句柄 → 执行
//! - 后续执行同一 SQL → hit → 直接复用缓存句柄执行
//! - 1000 次 1 行查询 → 断言结果正确 + 高命中率
//!
//! 生产入口：`IoReuseChannel::execute_reuse`（packages/sz-orm-core/src/pool_elastic.rs）

use std::collections::HashMap;
use std::sync::Arc;
use sz_orm_core::pool_elastic::IoReuseChannel;
use sz_orm_core::prepared_cache::ExecuteFn;
use sz_orm_core::{QueryRows, Value};

/// 创建 mock execute 闭包：返回固定 1 行结果
fn make_execute_fn(row_value: i64) -> ExecuteFn {
    Arc::new(move |_params: &[Value]| {
        let mut row = HashMap::new();
        row.insert("count".to_string(), Value::I64(row_value));
        let rows: QueryRows = vec![row];
        Box::pin(async move { Ok(rows) })
    })
}

/// 创建参数化 mock execute 闭包：返回参数值作为结果
fn make_param_execute_fn() -> ExecuteFn {
    Arc::new(move |params: &[Value]| {
        let val = params.first().cloned().unwrap_or(Value::Null);
        let mut row = HashMap::new();
        row.insert("val".to_string(), val);
        let rows: QueryRows = vec![row];
        Box::pin(async move { Ok(rows) })
    })
}

#[tokio::test]
async fn test_execute_reuse_first_call_is_miss() {
    let channel = IoReuseChannel::new(256);
    let conn_id = 1;
    let sql = "SELECT COUNT(*) FROM users WHERE active = ?";
    let params = vec![Value::Bool(true)];

    let result = channel
        .execute_reuse(conn_id, sql, &params, vec!["users".to_string()], || {
            make_execute_fn(42)
        })
        .await
        .unwrap();

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].get("count"), Some(&Value::I64(42)));

    let stats = channel.stats();
    assert_eq!(stats.misses, 1);
    assert_eq!(stats.hits, 0);
}

#[tokio::test]
async fn test_execute_reuse_second_call_is_hit() {
    let channel = IoReuseChannel::new(256);
    let conn_id = 1;
    let sql = "SELECT 1";
    let params: Vec<Value> = vec![];

    // 第一次调用：miss
    channel
        .execute_reuse(conn_id, sql, &params, vec![], || make_execute_fn(1))
        .await
        .unwrap();

    // 第二次调用：hit（复用缓存句柄）
    let result = channel
        .execute_reuse(conn_id, sql, &params, vec![], || make_execute_fn(999))
        .await
        .unwrap();

    // 命中缓存后执行的是缓存的闭包（返回 1，不是 999）
    assert_eq!(result[0].get("count"), Some(&Value::I64(1)));

    let stats = channel.stats();
    assert_eq!(stats.misses, 1);
    assert_eq!(stats.hits, 1);
}

#[tokio::test]
async fn test_execute_reuse_1000_queries_high_hit_rate() {
    let channel = IoReuseChannel::new(256);
    let conn_id = 1;
    let sql = "SELECT COUNT(*) FROM orders WHERE status = ?";
    let params = vec![Value::I32(1)];

    let mut results_ok = 0;
    for _ in 0..1000 {
        let result = channel
            .execute_reuse(conn_id, sql, &params, vec!["orders".to_string()], || {
                make_execute_fn(100)
            })
            .await
            .unwrap();
        if result.len() == 1 && result[0].get("count") == Some(&Value::I64(100)) {
            results_ok += 1;
        }
    }

    assert_eq!(results_ok, 1000);

    let stats = channel.stats();
    assert_eq!(stats.misses, 1);
    assert_eq!(stats.hits, 999);
    let hit_rate = stats.hit_rate;
    assert!(hit_rate > 0.99, "hit rate should be > 0.99, got {hit_rate}");
}

#[tokio::test]
async fn test_execute_reuse_different_sql_separate_cache() {
    let channel = IoReuseChannel::new(256);
    let conn_id = 1;

    let r1 = channel
        .execute_reuse(conn_id, "SELECT 1", &[], vec![], || make_execute_fn(10))
        .await
        .unwrap();
    let r2 = channel
        .execute_reuse(conn_id, "SELECT 2", &[], vec![], || make_execute_fn(20))
        .await
        .unwrap();

    assert_eq!(r1[0].get("count"), Some(&Value::I64(10)));
    assert_eq!(r2[0].get("count"), Some(&Value::I64(20)));

    let stats = channel.stats();
    assert_eq!(stats.misses, 2);
    assert_eq!(stats.hits, 0);
}

#[tokio::test]
async fn test_execute_reuse_different_conn_separate_cache() {
    let channel = IoReuseChannel::new(256);
    let sql = "SELECT 1";

    // 同一 SQL 在不同连接上各自 miss
    channel
        .execute_reuse(1, sql, &[], vec![], || make_execute_fn(100))
        .await
        .unwrap();
    channel
        .execute_reuse(2, sql, &[], vec![], || make_execute_fn(200))
        .await
        .unwrap();

    let stats = channel.stats();
    assert_eq!(stats.misses, 2);
    assert_eq!(stats.hits, 0);
}

#[tokio::test]
async fn test_execute_reuse_param_passthrough() {
    let channel = IoReuseChannel::new(256);
    let conn_id = 1;
    let sql = "SELECT ? AS val";

    let params = vec![Value::I64(777)];
    let result = channel
        .execute_reuse(conn_id, sql, &params, vec![], || make_param_execute_fn())
        .await
        .unwrap();

    assert_eq!(result[0].get("val"), Some(&Value::I64(777)));

    // 第二次调用不同参数，应命中缓存但执行同一闭包
    let params2 = vec![Value::I64(888)];
    let result2 = channel
        .execute_reuse(conn_id, sql, &params2, vec![], || make_param_execute_fn())
        .await
        .unwrap();

    assert_eq!(result2[0].get("val"), Some(&Value::I64(888)));
}

#[tokio::test]
async fn test_execute_reuse_invalidate_conn() {
    let channel = IoReuseChannel::new(256);
    let conn_id = 1;
    let sql = "SELECT 1";

    channel
        .execute_reuse(conn_id, sql, &[], vec![], || make_execute_fn(1))
        .await
        .unwrap();

    // 第二次：hit
    channel
        .execute_reuse(conn_id, sql, &[], vec![], || make_execute_fn(1))
        .await
        .unwrap();

    assert_eq!(channel.stats().hits, 1);

    // 失效连接级缓存
    channel.invalidate_conn(conn_id);

    // 第三次：miss（缓存已失效）
    channel
        .execute_reuse(conn_id, sql, &[], vec![], || make_execute_fn(1))
        .await
        .unwrap();

    let stats = channel.stats();
    assert_eq!(stats.misses, 2);
    assert_eq!(stats.hits, 1);
}
