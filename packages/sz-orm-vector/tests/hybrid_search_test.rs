#![cfg(feature = "hybrid-search")]

use std::sync::Arc;
use sz_orm_vector::hybrid_search::{
    fusion::fuse, FilterPushdown, FulltextQuery, FulltextSearchSource, FusionStrategy, HybridError,
    HybridQuery, HybridSearcher, SourceResult, StructuredQuery, VectorMetric, VectorQuery,
    VectorSearchSource,
};

fn make_source(
    id: &str,
    score: f32,
    source: sz_orm_vector::hybrid_search::SearchResultSource,
) -> SourceResult {
    SourceResult {
        id: id.to_string(),
        score,
        source,
    }
}

#[test]
fn test_hybrid_search_fusion_rrf_combines_vector_and_keyword() {
    use sz_orm_vector::hybrid_search::SearchResultSource;
    let vector_results = vec![
        make_source("a", 0.9, SearchResultSource::Vector),
        make_source("b", 0.8, SearchResultSource::Vector),
    ];
    let fulltext_results = vec![
        make_source("b", 0.95, SearchResultSource::Fulltext),
        make_source("c", 0.7, SearchResultSource::Fulltext),
    ];
    let structured_results: Vec<SourceResult> = vec![];

    let results = fuse(
        &vector_results,
        &fulltext_results,
        &structured_results,
        FusionStrategy::Rrf { k: 60 },
        10,
    );

    assert!(!results.is_empty());
    let ids: Vec<&str> = results.iter().map(|r| r.id.as_str()).collect();
    assert!(ids.contains(&"a"));
    assert!(ids.contains(&"b"));
    assert!(ids.contains(&"c"));
}

#[test]
fn test_hybrid_search_fusion_weighted() {
    use sz_orm_vector::hybrid_search::SearchResultSource;
    let vector_results = vec![make_source("a", 1.0, SearchResultSource::Vector)];
    let fulltext_results = vec![make_source("a", 0.5, SearchResultSource::Fulltext)];
    let structured_results: Vec<SourceResult> = vec![];

    let results = fuse(
        &vector_results,
        &fulltext_results,
        &structured_results,
        FusionStrategy::Weighted {
            vector_w: 0.6,
            fulltext_w: 0.4,
            structured_w: 0.0,
        },
        10,
    );

    assert!(!results.is_empty());
    assert_eq!(results[0].id, "a");
}

#[test]
fn test_hybrid_search_fusion_cascade() {
    use sz_orm_vector::hybrid_search::SearchResultSource;
    let vector_results = vec![
        make_source("a", 0.9, SearchResultSource::Vector),
        make_source("b", 0.8, SearchResultSource::Vector),
    ];
    let fulltext_results = vec![make_source("a", 0.95, SearchResultSource::Fulltext)];
    let structured_results: Vec<SourceResult> = vec![];

    let results = fuse(
        &vector_results,
        &fulltext_results,
        &structured_results,
        FusionStrategy::Cascade,
        10,
    );

    assert!(!results.is_empty());
}

#[test]
fn test_hybrid_search_fusion_empty_inputs() {
    let results = fuse(&[], &[], &[], FusionStrategy::Rrf { k: 60 }, 10);
    assert!(results.is_empty());
}

#[test]
fn test_hybrid_search_pushdown_to_vector() {
    let filter = StructuredQuery {
        table: "products".to_string(),
        where_clauses: vec!["price < 1000".to_string()],
        order_by: None,
    };
    let mut vector_query = VectorQuery {
        collection: "docs".to_string(),
        query_vector: vec![1.0, 0.0],
        metric: VectorMetric::Cosine,
        filter: None,
    };
    FilterPushdown::pushdown_to_vector(&filter, &mut vector_query);
    assert!(vector_query.filter.is_some());
    assert!(vector_query
        .filter
        .as_ref()
        .unwrap()
        .contains("price < 1000"));
}

#[test]
fn test_hybrid_search_pushdown_to_fulltext() {
    let filter = StructuredQuery {
        table: "products".to_string(),
        where_clauses: vec!["category = 'electronics'".to_string()],
        order_by: None,
    };
    let mut fulltext_query = FulltextQuery {
        index: "docs_idx".to_string(),
        query_text: "hello".to_string(),
        fields: vec!["title".to_string()],
    };
    FilterPushdown::pushdown_to_fulltext(&filter, &mut fulltext_query);
    assert!(fulltext_query
        .fields
        .iter()
        .any(|f| f.contains("__filter__")));
}

#[test]
fn test_hybrid_search_pushdown_empty_clauses_no_op() {
    let filter = StructuredQuery {
        table: "products".to_string(),
        where_clauses: vec![],
        order_by: None,
    };
    let mut vector_query = VectorQuery {
        collection: "docs".to_string(),
        query_vector: vec![1.0, 0.0],
        metric: VectorMetric::Cosine,
        filter: None,
    };
    FilterPushdown::pushdown_to_vector(&filter, &mut vector_query);
    assert!(vector_query.filter.is_none());
}

struct MockVectorSource;

#[async_trait::async_trait]
impl VectorSearchSource for MockVectorSource {
    async fn search(&self, _query: &VectorQuery) -> Result<Vec<SourceResult>, HybridError> {
        use sz_orm_vector::hybrid_search::SearchResultSource;
        Ok(vec![
            make_source("a", 0.9, SearchResultSource::Vector),
            make_source("b", 0.8, SearchResultSource::Vector),
        ])
    }
}

struct MockFulltextSource;

#[async_trait::async_trait]
impl FulltextSearchSource for MockFulltextSource {
    async fn search(&self, _query: &FulltextQuery) -> Result<Vec<SourceResult>, HybridError> {
        use sz_orm_vector::hybrid_search::SearchResultSource;
        Ok(vec![make_source("a", 0.95, SearchResultSource::Fulltext)])
    }
}

#[tokio::test]
async fn test_hybrid_search_searcher_score_fusion() {
    let searcher = HybridSearcher::new(
        Some(Arc::new(MockVectorSource)),
        Some(Arc::new(MockFulltextSource)),
        None,
    );

    let query = HybridQuery {
        vector: Some(VectorQuery {
            collection: "docs".to_string(),
            query_vector: vec![1.0, 0.0],
            metric: VectorMetric::Cosine,
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
    assert!(!response.degradation.vector_degraded);
    assert!(!response.degradation.fulltext_degraded);
}

#[tokio::test]
async fn test_hybrid_search_searcher_vector_only() {
    let searcher = HybridSearcher::new(Some(Arc::new(MockVectorSource)), None, None);

    let query = HybridQuery {
        vector: Some(VectorQuery {
            collection: "docs".to_string(),
            query_vector: vec![1.0, 0.0],
            metric: VectorMetric::Cosine,
            filter: None,
        }),
        fulltext: None,
        structured: None,
        strategy: FusionStrategy::Rrf { k: 60 },
        top_k: 10,
    };

    let response = searcher.search(&query).await.unwrap();
    assert!(!response.results.is_empty());
}

#[tokio::test]
async fn test_hybrid_search_searcher_all_sources_missing() {
    let searcher = HybridSearcher::new(None, None, None);

    let query = HybridQuery {
        vector: None,
        fulltext: None,
        structured: None,
        strategy: FusionStrategy::Rrf { k: 60 },
        top_k: 10,
    };

    let response = searcher.search(&query).await.unwrap();
    assert!(response.results.is_empty());
}
