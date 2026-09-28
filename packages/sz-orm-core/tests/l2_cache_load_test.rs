//! v9.2.0 M7-T37：get_or_load_query 命中/未命中回填（2 tests）

use std::time::Duration;
use sz_orm_core::l2_cache::L2Cache;
use sz_orm_core::Value;

#[tokio::test]
async fn test_l2_cache_get_or_load_query_hit_skips_loader() {
    let cache = L2Cache::new();
    let sql = "SELECT * FROM users WHERE id = ?";
    let params = vec![Value::I64(1)];
    let rows: Vec<std::collections::HashMap<String, Value>> = vec![];
    cache
        .get_or_load_query(
            "users",
            sql,
            &params,
            Duration::from_secs(60),
            || async { Ok(rows.clone()) },
        )
        .await
        .unwrap();
    let result = cache
        .get_or_load_query(
            "users",
            sql,
            &params,
            Duration::from_secs(60),
            || async {
                Err(sz_orm_core::DbError::Internal(
                    "loader should not be called on hit".to_string(),
                ))
            },
        )
        .await;
    assert!(result.is_ok(), "缓存命中不应调用 loader");
}

#[tokio::test]
async fn test_l2_cache_get_or_load_query_miss_invokes_loader() {
    let cache = L2Cache::new();
    let sql = "SELECT * FROM users WHERE status = ?";
    let params = vec![Value::I64(1)];
    let mut row = std::collections::HashMap::new();
    row.insert("id".to_string(), Value::I64(1));
    row.insert("name".to_string(), Value::String("Alice".to_string()));
    let expected_rows = vec![row];
    let result = cache
        .get_or_load_query(
            "users",
            sql,
            &params,
            Duration::from_secs(60),
            || async { Ok(expected_rows.clone()) },
        )
        .await
        .unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].get("name"), Some(&Value::String("Alice".to_string())));
    let cached = cache
        .get_or_load_query(
            "users",
            sql,
            &params,
            Duration::from_secs(60),
            || async {
                Err(sz_orm_core::DbError::Internal(
                    "should hit cache after backfill".to_string(),
                ))
            },
        )
        .await
        .unwrap();
    assert_eq!(cached.len(), 1, "回填后应命中缓存");
}