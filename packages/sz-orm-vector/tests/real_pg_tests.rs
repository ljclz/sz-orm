//! 真实 PostgreSQL + pgvector 集成测试（需启用 `real-pg` feature + 本机 pgvector 扩展）
//!
//! 运行方式（需 PostgreSQL 已安装 pgvector 扩展）：
//! ```bash
//! cargo test -p sz-orm-vector --features real-pg --test real_pg_tests -- --ignored
//! ```
//!
//! 连接参数可通过环境变量覆盖（默认 127.0.0.1:5432 / sz_orm_test / postgres / test123）：
//! - `SZ_ORM_PG_HOST` / `SZ_ORM_PG_PORT` / `SZ_ORM_PG_DB` / `SZ_ORM_PG_USER` / `SZ_ORM_PG_PASSWORD`

#![cfg(feature = "real-pg")]

use sz_orm_vector::{PgVectorStore, RealPgConfig, RealPgVectorStore, VectorMetric, VectorRecord};

fn config() -> RealPgConfig {
    RealPgConfig {
        host: std::env::var("SZ_ORM_PG_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
        port: std::env::var("SZ_ORM_PG_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(5432),
        database: std::env::var("SZ_ORM_PG_DB").unwrap_or_else(|_| "sz_orm_test".to_string()),
        username: std::env::var("SZ_ORM_PG_USER").unwrap_or_else(|_| "postgres".to_string()),
        password: std::env::var("SZ_ORM_PG_PASSWORD").unwrap_or_else(|_| "test123".to_string()),
    }
}

/// 使用唯一集合名，避免并行测试冲突
fn unique_collection(prefix: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("{}_{}_{}", prefix, std::process::id(), nanos)
}

#[tokio::test]
#[ignore]
async fn real_pg_create_insert_search_workflow() {
    let store = RealPgVectorStore::new(config()).expect("construct store");
    let collection = unique_collection("docs");

    store
        .create_collection(&collection, 3, Some(VectorMetric::Cosine))
        .await
        .expect("create collection");

    let records = vec![
        VectorRecord::new("a", vec![1.0, 0.0, 0.0]),
        VectorRecord::new("b", vec![0.0, 1.0, 0.0]),
        VectorRecord::new("c", vec![0.0, 0.0, 1.0]),
    ];
    store
        .insert(&collection, records)
        .await
        .expect("insert records");
    assert_eq!(store.count(&collection).await.unwrap(), 3);

    let results = store
        .search(&collection, &[1.0, 0.0, 0.0], 3)
        .await
        .expect("search");
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].id, "a", "最相似记录应为 a");

    let record = store
        .get(&collection, "b")
        .await
        .unwrap()
        .expect("record b exists");
    assert_eq!(record.id, "b");

    let removed = store
        .delete(&collection, vec!["c".to_string()])
        .await
        .unwrap();
    assert_eq!(removed, 1);
    assert_eq!(store.count(&collection).await.unwrap(), 2);

    store
        .delete_collection(&collection)
        .await
        .expect("delete collection");
}

#[tokio::test]
#[ignore]
async fn real_pg_upsert_same_id_overwrites() {
    let store = RealPgVectorStore::new(config()).expect("construct store");
    let collection = unique_collection("upsert");

    store
        .create_collection(&collection, 2, None)
        .await
        .expect("create collection");

    store
        .insert(&collection, vec![VectorRecord::new("k1", vec![1.0, 0.0])])
        .await
        .unwrap();
    store
        .insert(&collection, vec![VectorRecord::new("k1", vec![0.0, 1.0])])
        .await
        .unwrap();

    assert_eq!(
        store.count(&collection).await.unwrap(),
        1,
        "upsert 不应新增行"
    );
    let record = store.get(&collection, "k1").await.unwrap().unwrap();
    assert_eq!(record.vector, vec![0.0, 1.0], "向量应被覆盖");

    store.delete_collection(&collection).await.unwrap();
}

#[tokio::test]
#[ignore]
async fn real_pg_search_metric_consistency() {
    let store = RealPgVectorStore::new(config()).expect("construct store");
    let collection = unique_collection("euclid");

    store
        .create_collection(&collection, 2, Some(VectorMetric::Euclidean))
        .await
        .unwrap();
    store
        .insert(
            &collection,
            vec![
                VectorRecord::new("near", vec![0.0, 0.0]),
                VectorRecord::new("far", vec![100.0, 100.0]),
            ],
        )
        .await
        .unwrap();

    let results = store.search(&collection, &[0.0, 0.0], 2).await.unwrap();
    assert_eq!(results[0].id, "near", "欧氏距离最近应为 near");

    store.delete_collection(&collection).await.unwrap();
}
