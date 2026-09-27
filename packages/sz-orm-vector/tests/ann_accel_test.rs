#![cfg(feature = "ann-accel")]

use std::collections::HashMap;
use sz_orm_vector::{AnnAccelerated, InMemoryVectorStore, PgVectorStore, VectorRecord};

#[tokio::test]
async fn test_ann_search_with_tenant_filter_returns_only_tenant_vectors() {
    let store = InMemoryVectorStore::new();
    store.create_collection("docs", 2, None).await.unwrap();

    let mut meta_a = HashMap::new();
    meta_a.insert("tenant_id".to_string(), serde_json::json!("tenant_a"));
    let mut meta_b = HashMap::new();
    meta_b.insert("tenant_id".to_string(), serde_json::json!("tenant_b"));

    let rec_a1 = VectorRecord::new("a1", vec![1.0, 0.0]).with_metadata(meta_a.clone());
    let rec_a2 = VectorRecord::new("a2", vec![0.9, 0.1]).with_metadata(meta_a);
    let rec_b1 = VectorRecord::new("b1", vec![1.0, 0.0]).with_metadata(meta_b.clone());
    let rec_b2 = VectorRecord::new("b2", vec![0.9, 0.1]).with_metadata(meta_b);
    store
        .insert("docs", vec![rec_a1, rec_a2, rec_b1, rec_b2])
        .await
        .unwrap();

    let result = store
        .ann_search("docs", &[1.0, 0.0], 10, Some("tenant_a"))
        .await
        .unwrap();

    assert_eq!(result.records.len(), 2);
    for r in &result.records {
        let tid = r
            .metadata
            .as_ref()
            .and_then(|m| m.get("tenant_id"))
            .and_then(|v| v.as_str());
        assert_eq!(tid, Some("tenant_a"));
    }
}

#[tokio::test]
async fn test_ann_search_without_tenant_returns_all() {
    let store = InMemoryVectorStore::new();
    store.create_collection("docs", 2, None).await.unwrap();
    store
        .insert(
            "docs",
            vec![
                VectorRecord::new("a", vec![1.0, 0.0]),
                VectorRecord::new("b", vec![0.0, 1.0]),
            ],
        )
        .await
        .unwrap();

    let result = store
        .ann_search("docs", &[1.0, 0.0], 10, None)
        .await
        .unwrap();
    assert_eq!(result.records.len(), 2);
}

#[tokio::test]
async fn test_ann_search_recall_metric() {
    let store = InMemoryVectorStore::new();
    store.create_collection("docs", 3, None).await.unwrap();
    store
        .insert(
            "docs",
            vec![
                VectorRecord::new("a", vec![1.0, 0.0, 0.0]),
                VectorRecord::new("b", vec![0.0, 1.0, 0.0]),
                VectorRecord::new("c", vec![0.0, 0.0, 1.0]),
            ],
        )
        .await
        .unwrap();

    let result = store
        .ann_search("docs", &[1.0, 0.0, 0.0], 2, None)
        .await
        .unwrap();
    assert_eq!(result.records.len(), 2);
    assert_eq!(result.recall_rate, 1.0, "memory kNN recall must be 1.0");
}

#[tokio::test]
async fn test_ann_search_latency_recorded() {
    let store = InMemoryVectorStore::new();
    store.create_collection("docs", 2, None).await.unwrap();
    store
        .insert("docs", vec![VectorRecord::new("a", vec![1.0, 0.0])])
        .await
        .unwrap();

    let result = store
        .ann_search("docs", &[1.0, 0.0], 1, None)
        .await
        .unwrap();
    assert_eq!(result.records.len(), 1);
    assert!(
        result.latency_ms < 1000,
        "latency should be < 1s for memory"
    );
}

#[tokio::test]
async fn test_ann_search_empty_tenant_returns_empty() {
    let store = InMemoryVectorStore::new();
    store.create_collection("docs", 2, None).await.unwrap();
    store
        .insert("docs", vec![VectorRecord::new("a", vec![1.0, 0.0])])
        .await
        .unwrap();

    let result = store
        .ann_search("docs", &[1.0, 0.0], 10, Some("nonexistent_tenant"))
        .await
        .unwrap();
    assert_eq!(result.records.len(), 0);
}
