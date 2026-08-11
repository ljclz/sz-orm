//! HybridSearcher — 三源并行查询 + 部分降级

use super::{
    DegradationStatus, FulltextQuery, HybridError, HybridQuery, HybridSearchResponse, SourceResult,
    StructuredQuery, VectorQuery,
};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Instant;

/// 向量搜索源 trait
#[async_trait]
pub trait VectorSearchSource: Send + Sync {
    async fn search(&self, query: &VectorQuery) -> Result<Vec<SourceResult>, HybridError>;
}

/// 全文搜索源 trait
#[async_trait]
pub trait FulltextSearchSource: Send + Sync {
    async fn search(&self, query: &FulltextQuery) -> Result<Vec<SourceResult>, HybridError>;
}

/// 结构化查询源 trait
#[async_trait]
pub trait StructuredSearchSource: Send + Sync {
    async fn search(&self, query: &StructuredQuery) -> Result<Vec<SourceResult>, HybridError>;
}

/// 混合搜索器（三源并行查询 + 融合排序）
pub struct HybridSearcher {
    vector_store: Option<Arc<dyn VectorSearchSource>>,
    fulltext_store: Option<Arc<dyn FulltextSearchSource>>,
    structured_conn: Option<Arc<dyn StructuredSearchSource>>,
}

impl HybridSearcher {
    /// 构造 HybridSearcher
    pub fn new(
        vector_store: Option<Arc<dyn VectorSearchSource>>,
        fulltext_store: Option<Arc<dyn FulltextSearchSource>>,
        structured_conn: Option<Arc<dyn StructuredSearchSource>>,
    ) -> Self {
        Self {
            vector_store,
            fulltext_store,
            structured_conn,
        }
    }

    /// 执行混合搜索（三源并行查询 + 融合排序）
    pub async fn search(&self, query: &HybridQuery) -> Result<HybridSearchResponse, HybridError> {
        let start = Instant::now();

        let (vector_result, fulltext_result, structured_result) = tokio::join!(
            self.search_vector(query),
            self.search_fulltext(query),
            self.search_structured(query),
        );

        let mut degradation = DegradationStatus::default();
        let mut all_failed = true;
        let mut all_skipped = true;

        let mut vector_results = Vec::new();
        match vector_result {
            Ok(results) => {
                if query.vector.is_some() {
                    all_skipped = false;
                    all_failed = false;
                }
                vector_results = results;
            }
            Err(_) if query.vector.is_some() => {
                all_skipped = false;
                degradation.vector_degraded = true;
            }
            Err(_) => {}
        }

        let mut fulltext_results = Vec::new();
        match fulltext_result {
            Ok(results) => {
                if query.fulltext.is_some() {
                    all_skipped = false;
                    all_failed = false;
                }
                fulltext_results = results;
            }
            Err(_) if query.fulltext.is_some() => {
                all_skipped = false;
                degradation.fulltext_degraded = true;
            }
            Err(_) => {}
        }

        let mut structured_results = Vec::new();
        match structured_result {
            Ok(results) => {
                if query.structured.is_some() {
                    all_skipped = false;
                    all_failed = false;
                }
                structured_results = results;
            }
            Err(_) if query.structured.is_some() => {
                all_skipped = false;
                degradation.structured_degraded = true;
            }
            Err(_) => {}
        }

        if all_failed && !all_skipped {
            return Err(HybridError::AllSourcesFailed);
        }

        let fused = super::fusion::fuse(
            &vector_results,
            &fulltext_results,
            &structured_results,
            query.strategy,
            query.top_k,
        );

        let elapsed_ms = start.elapsed().as_millis() as u64;

        Ok(HybridSearchResponse {
            results: fused,
            degradation,
            elapsed_ms,
        })
    }

    async fn search_vector(&self, query: &HybridQuery) -> Result<Vec<SourceResult>, HybridError> {
        match (&query.vector, &self.vector_store) {
            (Some(vq), Some(store)) => store.search(vq).await,
            _ => Ok(Vec::new()),
        }
    }

    async fn search_fulltext(&self, query: &HybridQuery) -> Result<Vec<SourceResult>, HybridError> {
        match (&query.fulltext, &self.fulltext_store) {
            (Some(fq), Some(store)) => store.search(fq).await,
            _ => Ok(Vec::new()),
        }
    }

    async fn search_structured(
        &self,
        query: &HybridQuery,
    ) -> Result<Vec<SourceResult>, HybridError> {
        match (&query.structured, &self.structured_conn) {
            (Some(sq), Some(conn)) => conn.search(sq).await,
            _ => Ok(Vec::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{FusionStrategy, SearchResultSource};
    use super::*;

    struct MockVectorSource {
        results: Vec<SourceResult>,
        fail: bool,
    }

    #[async_trait]
    impl VectorSearchSource for MockVectorSource {
        async fn search(&self, _query: &VectorQuery) -> Result<Vec<SourceResult>, HybridError> {
            if self.fail {
                Err(HybridError::VectorError("mock failure".to_string()))
            } else {
                Ok(self.results.clone())
            }
        }
    }

    struct MockFulltextSource {
        results: Vec<SourceResult>,
        fail: bool,
    }

    #[async_trait]
    impl FulltextSearchSource for MockFulltextSource {
        async fn search(&self, _query: &FulltextQuery) -> Result<Vec<SourceResult>, HybridError> {
            if self.fail {
                Err(HybridError::FulltextError("mock failure".to_string()))
            } else {
                Ok(self.results.clone())
            }
        }
    }

    fn make_result(id: &str, score: f32, source: SearchResultSource) -> SourceResult {
        SourceResult {
            id: id.to_string(),
            score,
            source,
        }
    }

    #[tokio::test]
    async fn test_parallel_search_all_sources() {
        let vector_source = Arc::new(MockVectorSource {
            results: vec![
                make_result("v1", 0.9, SearchResultSource::Vector),
                make_result("v2", 0.8, SearchResultSource::Vector),
            ],
            fail: false,
        });
        let fulltext_source = Arc::new(MockFulltextSource {
            results: vec![make_result("f1", 0.85, SearchResultSource::Fulltext)],
            fail: false,
        });

        let searcher = HybridSearcher::new(Some(vector_source), Some(fulltext_source), None);

        let query = HybridQuery {
            vector: Some(VectorQuery {
                collection: "docs".to_string(),
                query_vector: vec![1.0, 0.0],
                metric: super::super::VectorMetric::Cosine,
                filter: None,
            }),
            fulltext: Some(FulltextQuery {
                index: "docs_idx".to_string(),
                query_text: "hello".to_string(),
                fields: vec!["title".to_string()],
            }),
            structured: None,
            strategy: FusionStrategy::Rrf { k: 60 },
            top_k: 10,
        };

        let response = searcher.search(&query).await.unwrap();
        assert!(!response.results.is_empty());
        assert!(!response.degradation.any_degraded());
    }

    #[tokio::test]
    async fn test_degradation_fulltext_failed() {
        let vector_source = Arc::new(MockVectorSource {
            results: vec![make_result("v1", 0.9, SearchResultSource::Vector)],
            fail: false,
        });
        let fulltext_source = Arc::new(MockFulltextSource {
            results: vec![],
            fail: true,
        });

        let searcher = HybridSearcher::new(Some(vector_source), Some(fulltext_source), None);

        let query = HybridQuery {
            vector: Some(VectorQuery {
                collection: "docs".to_string(),
                query_vector: vec![1.0],
                metric: super::super::VectorMetric::Cosine,
                filter: None,
            }),
            fulltext: Some(FulltextQuery {
                index: "docs_idx".to_string(),
                query_text: "hello".to_string(),
                fields: vec!["title".to_string()],
            }),
            structured: None,
            strategy: FusionStrategy::Rrf { k: 60 },
            top_k: 10,
        };

        let response = searcher.search(&query).await.unwrap();
        assert!(response.degradation.fulltext_degraded);
        assert!(!response.degradation.vector_degraded);
        assert!(!response.results.is_empty());
    }

    #[tokio::test]
    async fn test_all_sources_failed() {
        let vector_source = Arc::new(MockVectorSource {
            results: vec![],
            fail: true,
        });
        let fulltext_source = Arc::new(MockFulltextSource {
            results: vec![],
            fail: true,
        });

        let searcher = HybridSearcher::new(Some(vector_source), Some(fulltext_source), None);

        let query = HybridQuery {
            vector: Some(VectorQuery {
                collection: "docs".to_string(),
                query_vector: vec![1.0],
                metric: super::super::VectorMetric::Cosine,
                filter: None,
            }),
            fulltext: Some(FulltextQuery {
                index: "docs_idx".to_string(),
                query_text: "hello".to_string(),
                fields: vec!["title".to_string()],
            }),
            structured: None,
            strategy: FusionStrategy::Rrf { k: 60 },
            top_k: 10,
        };

        let result = searcher.search(&query).await;
        assert!(matches!(result, Err(HybridError::AllSourcesFailed)));
    }

    #[tokio::test]
    async fn test_empty_results_no_error() {
        let vector_source = Arc::new(MockVectorSource {
            results: vec![],
            fail: false,
        });

        let searcher = HybridSearcher::new(Some(vector_source), None, None);

        let query = HybridQuery {
            vector: Some(VectorQuery {
                collection: "docs".to_string(),
                query_vector: vec![1.0],
                metric: super::super::VectorMetric::Cosine,
                filter: None,
            }),
            fulltext: None,
            structured: None,
            strategy: FusionStrategy::Rrf { k: 60 },
            top_k: 10,
        };

        let response = searcher.search(&query).await.unwrap();
        assert!(response.results.is_empty());
        assert!(!response.degradation.any_degraded());
    }
}
