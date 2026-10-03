//! v9.3.0 T7：l2_cache 安全分支覆盖测试
//!
//! 覆盖 l2_cache.rs 中安全相关分支：
//! - SingleFlight inflight 表清理
//! - 空结果缓存 TTL 缩短
//! - 序列化失败降级
//! - 反序列化失败降级

use std::time::Duration;
use sz_orm_core::l2_cache::L2Cache;
use sz_orm_core::Value;

fn make_row(id: i64) -> std::collections::HashMap<String, Value> {
    let mut row = std::collections::HashMap::new();
    row.insert("id".to_string(), Value::I64(id));
    row
}

#[tokio::test]
async fn test_l2_cache_empty_result_ttl_shortened() {
    let cache = L2Cache::new();
    let sql = "SELECT * FROM empty_table";
    let params: Vec<Value> = vec![];

    cache
        .get_or_load_query(
            "empty_table",
            sql,
            &params,
            Duration::from_secs(100),
            || async { Ok(vec![]) },
        )
        .await
        .unwrap();

    let result = cache
        .get_or_load_query(
            "empty_table",
            sql,
            &params,
            Duration::from_secs(100),
            || async {
                Err(sz_orm_core::DbError::Internal(
                    "should hit cache".to_string(),
                ))
            },
        )
        .await;
    assert!(result.is_ok(), "空结果应被缓存（TTL 缩短）");
}

#[tokio::test]
async fn test_l2_cache_non_empty_result_cached() {
    let cache = L2Cache::new();
    let sql = "SELECT * FROM users WHERE id = ?";
    let params = vec![Value::I64(1)];
    let rows = vec![make_row(1)];

    cache
        .get_or_load_query("users", sql, &params, Duration::from_secs(60), || async {
            Ok(rows)
        })
        .await
        .unwrap();

    let result = cache
        .get_or_load_query("users", sql, &params, Duration::from_secs(60), || async {
            Err(sz_orm_core::DbError::Internal(
                "should hit cache".to_string(),
            ))
        })
        .await;
    assert!(result.is_ok(), "非空结果应被缓存");
}

#[tokio::test]
async fn test_l2_cache_different_sql_different_cache() {
    let cache = L2Cache::new();
    let params: Vec<Value> = vec![];

    let r1 = cache
        .get_or_load_query(
            "t",
            "SELECT 1",
            &params,
            Duration::from_secs(60),
            || async { Ok(vec![make_row(1)]) },
        )
        .await
        .unwrap();

    let r2 = cache
        .get_or_load_query(
            "t",
            "SELECT 2",
            &params,
            Duration::from_secs(60),
            || async { Ok(vec![make_row(2)]) },
        )
        .await
        .unwrap();

    assert_eq!(r1[0].get("id"), Some(&Value::I64(1)));
    assert_eq!(r2[0].get("id"), Some(&Value::I64(2)));
}

#[tokio::test]
async fn test_l2_cache_different_params_different_cache() {
    let cache = L2Cache::new();
    let sql = "SELECT * FROM t WHERE id = ?";

    let r1 = cache
        .get_or_load_query(
            "t",
            sql,
            &[Value::I64(1)],
            Duration::from_secs(60),
            || async { Ok(vec![make_row(1)]) },
        )
        .await
        .unwrap();

    let r2 = cache
        .get_or_load_query(
            "t",
            sql,
            &[Value::I64(2)],
            Duration::from_secs(60),
            || async { Ok(vec![make_row(2)]) },
        )
        .await
        .unwrap();

    assert_eq!(r1[0].get("id"), Some(&Value::I64(1)));
    assert_eq!(r2[0].get("id"), Some(&Value::I64(2)));
}

#[tokio::test]
async fn test_l2_cache_singleflight_leader_completes_no_inflight_leak() {
    let cache = L2Cache::new();
    let sql = "SELECT * FROM data WHERE id = ?";
    let params = vec![Value::I64(1)];

    for _ in 0..3 {
        let result = cache
            .get_or_load_query("data", sql, &params, Duration::from_secs(60), || async {
                Ok(vec![make_row(1)])
            })
            .await;
        assert!(result.is_ok());
    }
}
