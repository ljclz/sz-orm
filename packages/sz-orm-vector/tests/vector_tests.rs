//! 集成测试：验证 PgVectorStore 的端到端行为
//!
//! 使用 `InMemoryVectorStore` 进行测试（不需要 PG 连接）。

use std::collections::HashMap;
use std::str::FromStr;

use sz_orm_vector::{
    validate_top_k, InMemoryVectorStore, PgVectorStore, SearchResult, VectorError, VectorMetric,
    VectorRecord, MAX_TOP_K,
};

#[tokio::test]
async fn integration_create_collection_and_count() {
    let store = InMemoryVectorStore::new();
    store.create_collection("test_docs", 4, None).await.unwrap();
    assert_eq!(store.count("test_docs").await.unwrap(), 0);
}

#[tokio::test]
async fn integration_crud_workflow() {
    let store = InMemoryVectorStore::new();
    store
        .create_collection("docs", 3, Some(VectorMetric::Cosine))
        .await
        .unwrap();

    // Insert
    let records = vec![
        VectorRecord::new("a", vec![1.0, 0.0, 0.0]),
        VectorRecord::new("b", vec![0.0, 1.0, 0.0]),
        VectorRecord::new("c", vec![0.0, 0.0, 1.0]),
    ];
    store.insert("docs", records).await.unwrap();
    assert_eq!(store.count("docs").await.unwrap(), 3);

    // Search
    let results = store.search("docs", &[1.0, 0.0, 0.0], 3).await.unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].id, "a");
    assert!(results[0].score >= results[1].score);

    // Get
    let record = store.get("docs", "a").await.unwrap().unwrap();
    assert_eq!(record.id, "a");
    assert_eq!(record.vector, vec![1.0, 0.0, 0.0]);

    // Delete
    let removed = store.delete("docs", vec!["b".to_string()]).await.unwrap();
    assert_eq!(removed, 1);
    assert_eq!(store.count("docs").await.unwrap(), 2);

    // Delete collection
    store.delete_collection("docs").await.unwrap();
    assert_eq!(store.count("docs").await.unwrap(), 0);
}

#[tokio::test]
async fn integration_search_with_different_metrics() {
    let store = InMemoryVectorStore::new();

    // Euclidean
    store
        .create_collection("euclid", 2, Some(VectorMetric::Euclidean))
        .await
        .unwrap();
    store
        .insert(
            "euclid",
            vec![
                VectorRecord::new("near", vec![0.0, 0.0]),
                VectorRecord::new("far", vec![100.0, 100.0]),
            ],
        )
        .await
        .unwrap();
    let results = store.search("euclid", &[0.0, 0.0], 2).await.unwrap();
    assert_eq!(results[0].id, "near");

    // DotProduct
    store
        .create_collection("dot", 2, Some(VectorMetric::DotProduct))
        .await
        .unwrap();
    store
        .insert(
            "dot",
            vec![
                VectorRecord::new("high", vec![5.0, 5.0]),
                VectorRecord::new("low", vec![0.0, 0.0]),
            ],
        )
        .await
        .unwrap();
    let results = store.search("dot", &[1.0, 1.0], 2).await.unwrap();
    assert_eq!(results[0].id, "high");
}

#[tokio::test]
async fn integration_multiple_collections() {
    let store = InMemoryVectorStore::new();

    store.create_collection("col_a", 2, None).await.unwrap();
    store.create_collection("col_b", 3, None).await.unwrap();

    store
        .insert("col_a", vec![VectorRecord::new("a1", vec![1.0, 0.0])])
        .await
        .unwrap();
    store
        .insert("col_b", vec![VectorRecord::new("b1", vec![1.0, 0.0, 0.0])])
        .await
        .unwrap();

    assert_eq!(store.count("col_a").await.unwrap(), 1);
    assert_eq!(store.count("col_b").await.unwrap(), 1);
}

#[tokio::test]
async fn integration_delete_multiple() {
    let store = InMemoryVectorStore::new();
    store.create_collection("docs", 2, None).await.unwrap();
    store
        .insert(
            "docs",
            (0..5)
                .map(|i| VectorRecord::new(format!("r{}", i), vec![i as f32, 0.0]))
                .collect(),
        )
        .await
        .unwrap();

    let removed = store
        .delete("docs", vec!["r1".to_string(), "r3".to_string()])
        .await
        .unwrap();
    assert_eq!(removed, 2);
    assert_eq!(store.count("docs").await.unwrap(), 3);
}

#[tokio::test]
async fn integration_upsert_semantics() {
    let store = InMemoryVectorStore::new();
    store.create_collection("docs", 2, None).await.unwrap();

    store
        .insert("docs", vec![VectorRecord::new("x", vec![1.0, 0.0])])
        .await
        .unwrap();
    assert_eq!(store.count("docs").await.unwrap(), 1);

    // Upsert same id with different vector
    store
        .insert("docs", vec![VectorRecord::new("x", vec![0.0, 1.0])])
        .await
        .unwrap();
    assert_eq!(store.count("docs").await.unwrap(), 1);

    let fetched = store.get("docs", "x").await.unwrap().unwrap();
    assert_eq!(fetched.vector, vec![0.0, 1.0]);
}
#[test]
fn test_validate_top_k_zero_returns_error() {
    assert!(validate_top_k(0).is_err());
}

#[test]
fn test_validate_top_k_exceeds_max_returns_error() {
    assert!(validate_top_k(MAX_TOP_K + 1).is_err());
}

#[test]
fn test_validate_top_k_one_returns_ok() {
    assert_eq!(validate_top_k(1).unwrap(), 1);
}

#[test]
fn test_validate_top_k_at_max_returns_ok() {
    assert_eq!(validate_top_k(MAX_TOP_K).unwrap(), MAX_TOP_K);
}

#[test]
fn test_vector_metric_from_str_unknown_returns_error() {
    assert!(VectorMetric::from_str("unknown").is_err());
}

#[test]
fn test_vector_metric_from_str_all_valid() {
    let cosine = VectorMetric::from_str("cosine").unwrap();
    assert_eq!(cosine, VectorMetric::Cosine);
    assert_eq!(cosine.pg_operator(), "<=>");
    assert_eq!(cosine.as_str(), "cosine");

    let euclidean = VectorMetric::from_str("euclidean").unwrap();
    assert_eq!(euclidean, VectorMetric::Euclidean);
    assert_eq!(euclidean.pg_operator(), "<->");

    let dot = VectorMetric::from_str("dotproduct").unwrap();
    assert_eq!(dot, VectorMetric::DotProduct);
    assert_eq!(dot.pg_operator(), "<#>");
}

#[test]
fn test_vector_record_with_score() {
    let record = VectorRecord::new("r1", vec![1.0, 0.0]).with_score(0.95);
    assert_eq!(record.id, "r1");
    assert_eq!(record.score, Some(0.95));
}

#[test]
fn test_vector_record_with_metadata() {
    let mut meta = HashMap::new();
    meta.insert("tenant".to_string(), serde_json::json!("acme"));
    let record = VectorRecord::new("r1", vec![1.0]).with_metadata(meta);
    assert!(record.metadata.is_some());
}

#[test]
fn test_search_result_with_text() {
    let result = SearchResult::new("r1", 0.9, vec![1.0]).with_text("hello");
    assert_eq!(result.id, "r1");
    assert_eq!(result.text.as_deref(), Some("hello"));
}

#[test]
fn test_search_result_with_metadata() {
    let mut meta = HashMap::new();
    meta.insert("source".to_string(), serde_json::json!("doc"));
    let result = SearchResult::new("r1", 0.9, vec![1.0]).with_metadata(meta);
    assert!(result.metadata.is_some());
}
#[tokio::test]
async fn test_inmemory_store_get_nonexistent_collection_returns_error() {
    let store = InMemoryVectorStore::new();
    let err = store.get("nonexistent", "r1").await;
    assert!(matches!(err, Err(VectorError::CollectionNotFound(_))));
}

#[tokio::test]
async fn test_inmemory_store_delete_nonexistent_collection_returns_error() {
    let store = InMemoryVectorStore::new();
    let err = store.delete("nonexistent", vec!["r1".to_string()]).await;
    assert!(matches!(err, Err(VectorError::CollectionNotFound(_))));
}

#[tokio::test]
async fn test_inmemory_store_insert_nonexistent_collection_returns_error() {
    let store = InMemoryVectorStore::new();
    let err = store
        .insert("nonexistent", vec![VectorRecord::new("r1", vec![1.0])])
        .await;
    assert!(matches!(err, Err(VectorError::CollectionNotFound(_))));
}

#[tokio::test]
async fn test_inmemory_store_count_nonexistent_returns_zero() {
    let store = InMemoryVectorStore::new();
    assert_eq!(store.count("nonexistent").await.unwrap(), 0);
}
