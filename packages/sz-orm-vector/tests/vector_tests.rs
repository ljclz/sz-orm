//! 集成测试：验证 PgVectorStore 的端到端行为
//!
//! 使用 `InMemoryVectorStore` 进行测试（不需要 PG 连接）。

use sz_orm_vector::{InMemoryVectorStore, PgVectorStore, VectorMetric, VectorRecord};

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
