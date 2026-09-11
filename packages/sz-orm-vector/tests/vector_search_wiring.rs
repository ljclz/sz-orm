//! VEC-SEARCH-01 接线验证测试（v6.8.0）
//!
//! 验证 EmbeddingStore::search → Top-K 相似度查询 端到端管线。

use sz_orm_vector::embedding_store::EmbeddingStore;
use sz_orm_vector::memory::InMemoryVectorStore;
use sz_orm_vector::{validate_top_k, VectorError, VectorMetric, VectorRecord, MAX_TOP_K};

fn make_store(metric: VectorMetric) -> EmbeddingStore {
    let backend = InMemoryVectorStore::new();
    EmbeddingStore::new(Box::new(backend), metric)
}

#[tokio::test]
async fn wiring_cosine_search_returns_top_k_sorted() {
    let store = make_store(VectorMetric::Cosine);
    store.create_collection("docs", 3).await.unwrap();

    let records: Vec<_> = (0..100)
        .map(|i| {
            VectorRecord::new(
                format!("doc_{}", i),
                vec![i as f32, (i as f32 * 2.0), (i as f32 * 3.0)],
            )
        })
        .collect();
    store.write("docs", records).await.unwrap();

    let results = store.search("docs", &[1.0, 2.0, 3.0], 10).await.unwrap();
    assert_eq!(results.len(), 10);
    for i in 0..results.len() - 1 {
        assert!(
            results[i].score >= results[i + 1].score,
            "Results should be sorted by score descending: {} >= {}",
            results[i].score,
            results[i + 1].score
        );
    }
}

#[tokio::test]
async fn wiring_euclidean_search_returns_top_k() {
    let store = make_store(VectorMetric::Euclidean);
    store.create_collection("docs", 2).await.unwrap();

    let records = vec![
        VectorRecord::new("a", vec![1.0, 0.0]),
        VectorRecord::new("b", vec![0.0, 1.0]),
        VectorRecord::new("c", vec![1.0, 1.0]),
        VectorRecord::new("d", vec![2.0, 2.0]),
    ];
    store.write("docs", records).await.unwrap();

    let results = store.search("docs", &[1.0, 1.0], 2).await.unwrap();
    assert_eq!(results.len(), 2);
}

#[tokio::test]
async fn wiring_dot_product_search_returns_top_k() {
    let store = make_store(VectorMetric::DotProduct);
    store.create_collection("docs", 2).await.unwrap();

    let records = vec![
        VectorRecord::new("a", vec![1.0, 0.0]),
        VectorRecord::new("b", vec![0.0, 1.0]),
        VectorRecord::new("c", vec![1.0, 1.0]),
    ];
    store.write("docs", records).await.unwrap();

    let results = store.search("docs", &[1.0, 1.0], 3).await.unwrap();
    assert_eq!(results.len(), 3);
}

#[tokio::test]
async fn wiring_top_k_exceeds_max_returns_error() {
    let store = make_store(VectorMetric::Cosine);
    store.create_collection("docs", 2).await.unwrap();
    store
        .write("docs", vec![VectorRecord::new("a", vec![1.0, 0.0])])
        .await
        .unwrap();

    let result = store.search("docs", &[1.0, 0.0], MAX_TOP_K + 1).await;
    assert!(result.is_err());
    match result {
        Err(VectorError::TopKExceeded { requested, max }) => {
            assert_eq!(requested, MAX_TOP_K + 1);
            assert_eq!(max, MAX_TOP_K);
        }
        _ => panic!("Expected TopKExceeded error"),
    }
}

#[tokio::test]
async fn wiring_top_k_zero_returns_error() {
    let store = make_store(VectorMetric::Cosine);
    store.create_collection("docs", 2).await.unwrap();

    let result = store.search("docs", &[1.0, 0.0], 0).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn wiring_large_dataset_search() {
    let store = make_store(VectorMetric::Cosine);
    store.create_collection("vectors", 10).await.unwrap();

    let records: Vec<_> = (0..1000)
        .map(|i| {
            let v: Vec<f32> = (0..10).map(|j| (i * 10 + j) as f32).collect();
            VectorRecord::new(format!("vec_{}", i), v)
        })
        .collect();
    store.write("vectors", records).await.unwrap();

    let query: Vec<f32> = (0..10).map(|j| j as f32).collect();
    let results = store.search("vectors", &query, 10).await.unwrap();
    assert_eq!(results.len(), 10);
    for r in &results {
        assert!(r.id.starts_with("vec_"));
    }
}

#[tokio::test]
async fn wiring_search_empty_collection_returns_empty() {
    let store = make_store(VectorMetric::Cosine);
    store.create_collection("empty", 2).await.unwrap();

    let results = store.search("empty", &[1.0, 0.0], 10).await.unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn wiring_search_preserves_metric() {
    let cosine_store = make_store(VectorMetric::Cosine);
    assert_eq!(cosine_store.metric(), &VectorMetric::Cosine);

    let euclidean_store = make_store(VectorMetric::Euclidean);
    assert_eq!(euclidean_store.metric(), &VectorMetric::Euclidean);

    let dot_store = make_store(VectorMetric::DotProduct);
    assert_eq!(dot_store.metric(), &VectorMetric::DotProduct);
}

#[tokio::test]
async fn wiring_validate_top_k_boundary() {
    assert!(validate_top_k(1).is_ok());
    assert!(validate_top_k(MAX_TOP_K).is_ok());
    assert!(validate_top_k(0).is_err());
    assert!(validate_top_k(MAX_TOP_K + 1).is_err());
}

#[tokio::test]
async fn wiring_search_results_have_scores() {
    let store = make_store(VectorMetric::Cosine);
    store.create_collection("docs", 3).await.unwrap();

    let records = vec![
        VectorRecord::new("a", vec![1.0, 0.0, 0.0]),
        VectorRecord::new("b", vec![0.0, 1.0, 0.0]),
        VectorRecord::new("c", vec![0.0, 0.0, 1.0]),
    ];
    store.write("docs", records).await.unwrap();

    let results = store.search("docs", &[1.0, 0.0, 0.0], 3).await.unwrap();
    for r in &results {
        assert!(r.score.is_finite());
    }
    assert_eq!(results[0].id, "a");
}

#[tokio::test]
async fn wiring_search_top_1_returns_best_match() {
    let store = make_store(VectorMetric::Cosine);
    store.create_collection("docs", 2).await.unwrap();

    let records = vec![
        VectorRecord::new("near", vec![1.0, 0.0]),
        VectorRecord::new("far", vec![0.0, 1.0]),
    ];
    store.write("docs", records).await.unwrap();

    let results = store.search("docs", &[1.0, 0.0], 1).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "near");
}
