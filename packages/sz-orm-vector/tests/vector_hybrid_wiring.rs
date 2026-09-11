//! W2-13 VEC-HYBRID-01 接线验证：混合检索
//!
//! 发起向量 + 全文混合查询 → 断言结果含两路命中 + 按 RRF 融合排序；
//! 任一路失败 → 断言不阻塞整体（降级安全）。

use std::sync::Arc;

use async_trait::async_trait;
use sz_orm_vector::hybrid_search::{
    FulltextQuery, FulltextSearchSource, FusionStrategy, HybridError, HybridQuery, HybridSearcher,
    SearchResultSource, SourceResult, StructuredQuery, StructuredSearchSource, VectorMetric,
    VectorQuery, VectorSearchSource,
};

struct MockVectorSource {
    results: Vec<SourceResult>,
}

#[async_trait]
impl VectorSearchSource for MockVectorSource {
    async fn search(&self, _query: &VectorQuery) -> Result<Vec<SourceResult>, HybridError> {
        Ok(self.results.clone())
    }
}

struct MockFulltextSource {
    results: Vec<SourceResult>,
}

#[async_trait]
impl FulltextSearchSource for MockFulltextSource {
    async fn search(&self, _query: &FulltextQuery) -> Result<Vec<SourceResult>, HybridError> {
        Ok(self.results.clone())
    }
}

struct FailingVectorSource;

#[async_trait]
impl VectorSearchSource for FailingVectorSource {
    async fn search(&self, _query: &VectorQuery) -> Result<Vec<SourceResult>, HybridError> {
        Err(HybridError::VectorError("connection refused".to_string()))
    }
}

struct FailingFulltextSource;

#[async_trait]
impl FulltextSearchSource for FailingFulltextSource {
    async fn search(&self, _query: &FulltextQuery) -> Result<Vec<SourceResult>, HybridError> {
        Err(HybridError::FulltextError("timeout".to_string()))
    }
}

struct MockStructuredSource {
    #[allow(dead_code)]
    results: Vec<SourceResult>,
}

#[async_trait]
impl StructuredSearchSource for MockStructuredSource {
    async fn search(&self, _query: &StructuredQuery) -> Result<Vec<SourceResult>, HybridError> {
        Ok(self.results.clone())
    }
}

fn vector_query() -> VectorQuery {
    VectorQuery {
        collection: "docs".to_string(),
        query_vector: vec![1.0, 0.0, 0.0],
        metric: VectorMetric::Cosine,
        filter: None,
    }
}

fn fulltext_query() -> FulltextQuery {
    FulltextQuery {
        index: "docs_idx".to_string(),
        query_text: "hello".to_string(),
        fields: vec!["content".to_string()],
    }
}

fn make_hybrid_query(vector: Option<VectorQuery>, fulltext: Option<FulltextQuery>) -> HybridQuery {
    HybridQuery {
        vector,
        fulltext,
        structured: None,
        strategy: FusionStrategy::Rrf { k: 60 },
        top_k: 10,
    }
}

#[tokio::test]
async fn wiring_vector_and_fulltext_both_return_results() {
    let vector = Arc::new(MockVectorSource {
        results: vec![
            SourceResult {
                id: "doc1".to_string(),
                score: 0.9,
                source: SearchResultSource::Vector,
            },
            SourceResult {
                id: "doc2".to_string(),
                score: 0.8,
                source: SearchResultSource::Vector,
            },
        ],
    });
    let fulltext = Arc::new(MockFulltextSource {
        results: vec![
            SourceResult {
                id: "doc2".to_string(),
                score: 5.0,
                source: SearchResultSource::Fulltext,
            },
            SourceResult {
                id: "doc3".to_string(),
                score: 3.0,
                source: SearchResultSource::Fulltext,
            },
        ],
    });
    let searcher = HybridSearcher::new(Some(vector), Some(fulltext), None);
    let query = make_hybrid_query(Some(vector_query()), Some(fulltext_query()));
    let response = searcher.search(&query).await.unwrap();

    assert!(!response.results.is_empty());
    assert!(!response.degradation.any_degraded());
}

#[tokio::test]
async fn wiring_results_contain_ids_from_both_sources() {
    let vector = Arc::new(MockVectorSource {
        results: vec![SourceResult {
            id: "v1".to_string(),
            score: 0.9,
            source: SearchResultSource::Vector,
        }],
    });
    let fulltext = Arc::new(MockFulltextSource {
        results: vec![SourceResult {
            id: "f1".to_string(),
            score: 5.0,
            source: SearchResultSource::Fulltext,
        }],
    });
    let searcher = HybridSearcher::new(Some(vector), Some(fulltext), None);
    let query = make_hybrid_query(Some(vector_query()), Some(fulltext_query()));
    let response = searcher.search(&query).await.unwrap();

    let ids: Vec<&str> = response.results.iter().map(|r| r.id.as_str()).collect();
    assert!(ids.contains(&"v1"));
    assert!(ids.contains(&"f1"));
}

#[tokio::test]
async fn wiring_vector_failure_does_not_block_fulltext() {
    let vector = Arc::new(FailingVectorSource);
    let fulltext = Arc::new(MockFulltextSource {
        results: vec![SourceResult {
            id: "f1".to_string(),
            score: 5.0,
            source: SearchResultSource::Fulltext,
        }],
    });
    let searcher = HybridSearcher::new(Some(vector), Some(fulltext), None);
    let query = make_hybrid_query(Some(vector_query()), Some(fulltext_query()));
    let response = searcher.search(&query).await.unwrap();

    assert!(response.degradation.vector_degraded);
    assert!(!response.degradation.fulltext_degraded);
    assert!(!response.results.is_empty());
}

#[tokio::test]
async fn wiring_fulltext_failure_does_not_block_vector() {
    let vector = Arc::new(MockVectorSource {
        results: vec![SourceResult {
            id: "v1".to_string(),
            score: 0.9,
            source: SearchResultSource::Vector,
        }],
    });
    let fulltext = Arc::new(FailingFulltextSource);
    let searcher = HybridSearcher::new(Some(vector), Some(fulltext), None);
    let query = make_hybrid_query(Some(vector_query()), Some(fulltext_query()));
    let response = searcher.search(&query).await.unwrap();

    assert!(!response.degradation.vector_degraded);
    assert!(response.degradation.fulltext_degraded);
    assert!(!response.results.is_empty());
}

#[tokio::test]
async fn wiring_both_sources_failure_returns_error() {
    let vector = Arc::new(FailingVectorSource);
    let fulltext = Arc::new(FailingFulltextSource);
    let searcher = HybridSearcher::new(Some(vector), Some(fulltext), None);
    let query = make_hybrid_query(Some(vector_query()), Some(fulltext_query()));
    let result = searcher.search(&query).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn wiring_rrf_fusion_ranks_common_doc_higher() {
    let vector = Arc::new(MockVectorSource {
        results: vec![
            SourceResult {
                id: "shared".to_string(),
                score: 0.9,
                source: SearchResultSource::Vector,
            },
            SourceResult {
                id: "v_only".to_string(),
                score: 0.8,
                source: SearchResultSource::Vector,
            },
        ],
    });
    let fulltext = Arc::new(MockFulltextSource {
        results: vec![
            SourceResult {
                id: "shared".to_string(),
                score: 5.0,
                source: SearchResultSource::Fulltext,
            },
            SourceResult {
                id: "f_only".to_string(),
                score: 3.0,
                source: SearchResultSource::Fulltext,
            },
        ],
    });
    let searcher = HybridSearcher::new(Some(vector), Some(fulltext), None);
    let query = make_hybrid_query(Some(vector_query()), Some(fulltext_query()));
    let response = searcher.search(&query).await.unwrap();

    let shared_score = response
        .results
        .iter()
        .find(|r| r.id == "shared")
        .map(|r| r.score);
    let v_only_score = response
        .results
        .iter()
        .find(|r| r.id == "v_only")
        .map(|r| r.score);

    if let (Some(shared), Some(v_only)) = (shared_score, v_only_score) {
        assert!(
            shared > v_only,
            "shared doc should rank higher than vector-only doc"
        );
    }
}

#[tokio::test]
async fn wiring_vector_only_query_works() {
    let vector = Arc::new(MockVectorSource {
        results: vec![SourceResult {
            id: "v1".to_string(),
            score: 0.9,
            source: SearchResultSource::Vector,
        }],
    });
    let searcher = HybridSearcher::new(Some(vector), None, None);
    let query = make_hybrid_query(Some(vector_query()), None);
    let response = searcher.search(&query).await.unwrap();

    assert!(!response.results.is_empty());
    assert!(!response.degradation.any_degraded());
}

#[tokio::test]
async fn wiring_fulltext_only_query_works() {
    let fulltext = Arc::new(MockFulltextSource {
        results: vec![SourceResult {
            id: "f1".to_string(),
            score: 5.0,
            source: SearchResultSource::Fulltext,
        }],
    });
    let searcher = HybridSearcher::new(None, Some(fulltext), None);
    let query = make_hybrid_query(None, Some(fulltext_query()));
    let response = searcher.search(&query).await.unwrap();

    assert!(!response.results.is_empty());
    assert!(!response.degradation.any_degraded());
}
