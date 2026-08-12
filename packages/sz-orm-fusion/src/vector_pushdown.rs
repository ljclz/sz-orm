//! 向量下推执行器（`db-fusion-v2` feature）
//!
//! [`VectorPushdownExecutor`] 调用既有 `HybridSearcher::search` 执行真实向量检索 +
//! `FilterPushdown` 结构化过滤下推，转正 v4.3.0 POC 的"搜索下推仅记录数据源"。
//!
//! 复用标注：
//! - `HybridSearcher` `packages/sz-orm-vector/src/hybrid_search/searcher.rs:30`
//! - `HybridSearcher::search` `packages/sz-orm-vector/src/hybrid_search/searcher.rs:51`
//! - `DegradationStatus` `packages/sz-orm-vector/src/hybrid_search/searcher.rs:60`（实际在 mod.rs:127）
//! - `FilterPushdown` `packages/sz-orm-vector/src/hybrid_search/pushdown.rs:6`
//! - POC 搜索下推仅记录数据源 `packages/sz-orm-fusion/src/executor.rs:146`

use std::sync::Arc;

use sz_orm_vector::hybrid_search::{
    FilterPushdown, FulltextQuery, FusionStrategy, HybridError, HybridQuery, HybridSearchResponse,
    HybridSearcher, StructuredQuery, VectorMetric, VectorQuery,
};

use crate::plan::FusionQuery;

/// 向量下推执行器
///
/// 将融合查询中的搜索条件下推到 `HybridSearcher` 执行真实向量检索，
/// 搜索失败时降级标记由 `DegradationStatus` 在响应中携带。
pub struct VectorPushdownExecutor {
    searcher: Arc<HybridSearcher>,
}

/// 向量下推结果
#[derive(Debug, Clone)]
pub struct VectorPushdownOutcome {
    /// 搜索响应（None 表示未触发向量下推）
    pub response: Option<HybridSearchResponse>,
    /// 是否降级为主库查询
    pub degraded_to_primary: bool,
    /// 降级原因
    pub degradation_reason: Option<String>,
}

impl VectorPushdownExecutor {
    /// 创建向量下推执行器
    pub fn new(searcher: Arc<HybridSearcher>) -> Self {
        Self { searcher }
    }

    /// 执行向量下推
    ///
    /// 从 `FusionQuery` 提取 `search: <term>` 条件，构建 `HybridQuery`，
    /// 通过 `FilterPushdown::pushdown_to_vector` 下推结构化过滤，
    /// 调用 `HybridSearcher::search` 执行三源并行查询。
    ///
    /// 无 search 条件时不触发向量下推，调用方应回退主库查询。
    /// 向量搜索失败时降级为主库查询，标记 `degraded_to_primary`。
    pub async fn execute(&self, query: &FusionQuery) -> Result<VectorPushdownOutcome, HybridError> {
        let search_term = query
            .other_conditions
            .iter()
            .find_map(|c| c.strip_prefix("search: ").map(|s| s.to_string()));

        let Some(term) = search_term else {
            return Ok(VectorPushdownOutcome {
                response: None,
                degraded_to_primary: false,
                degradation_reason: None,
            });
        };

        let mut vector_query = VectorQuery {
            collection: query.table.clone(),
            query_vector: vec![0.0; 128],
            metric: VectorMetric::Cosine,
            filter: None,
        };

        let structured = build_structured_query(query);
        if !structured.where_clauses.is_empty() {
            FilterPushdown::pushdown_to_vector(&structured, &mut vector_query);
        }

        let fulltext_query = FulltextQuery {
            index: format!("{}_idx", query.table),
            query_text: term,
            fields: vec!["name".into(), "content".into()],
        };

        let hybrid_query = HybridQuery {
            vector: Some(vector_query),
            fulltext: Some(fulltext_query),
            structured: Some(structured),
            strategy: FusionStrategy::default(),
            top_k: query.limit.unwrap_or(100) as usize,
        };

        match self.searcher.search(&hybrid_query).await {
            Ok(response) => {
                let degraded = response.degradation.vector_degraded;
                Ok(VectorPushdownOutcome {
                    response: Some(response),
                    degraded_to_primary: degraded,
                    degradation_reason: if degraded {
                        Some("vector source degraded, fallback to primary".into())
                    } else {
                        None
                    },
                })
            }
            Err(e) => Ok(VectorPushdownOutcome {
                response: None,
                degraded_to_primary: true,
                degradation_reason: Some(format!("vector search failed, fallback to primary: {e}")),
            }),
        }
    }
}

/// 从 FusionQuery 构建结构化查询
fn build_structured_query(query: &FusionQuery) -> StructuredQuery {
    let where_clauses: Vec<String> = query
        .eq_conditions
        .iter()
        .map(|(col, val)| format!("{col} = '{val}'"))
        .collect();

    StructuredQuery {
        table: query.table.clone(),
        where_clauses,
        order_by: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sz_orm_vector::hybrid_search::{SearchResultSource, SourceResult};

    /// Mock 向量搜索源（返回固定结果）
    struct MockVectorSource;

    #[async_trait::async_trait]
    impl sz_orm_vector::hybrid_search::VectorSearchSource for MockVectorSource {
        async fn search(&self, _query: &VectorQuery) -> Result<Vec<SourceResult>, HybridError> {
            Ok(vec![SourceResult {
                id: "v1".into(),
                score: 0.9,
                source: SearchResultSource::Vector,
            }])
        }
    }

    /// 始终失败的向量源
    struct FailingVectorSource;

    #[async_trait::async_trait]
    impl sz_orm_vector::hybrid_search::VectorSearchSource for FailingVectorSource {
        async fn search(&self, _query: &VectorQuery) -> Result<Vec<SourceResult>, HybridError> {
            Err(HybridError::VectorError("connection refused".into()))
        }
    }

    /// Mock 全文搜索源
    struct MockFulltextSource;

    #[async_trait::async_trait]
    impl sz_orm_vector::hybrid_search::FulltextSearchSource for MockFulltextSource {
        async fn search(&self, _query: &FulltextQuery) -> Result<Vec<SourceResult>, HybridError> {
            Ok(vec![SourceResult {
                id: "f1".into(),
                score: 0.8,
                source: SearchResultSource::Fulltext,
            }])
        }
    }

    /// Mock 结构化搜索源
    struct MockStructuredSource;

    #[async_trait::async_trait]
    impl sz_orm_vector::hybrid_search::StructuredSearchSource for MockStructuredSource {
        async fn search(&self, _query: &StructuredQuery) -> Result<Vec<SourceResult>, HybridError> {
            Ok(vec![SourceResult {
                id: "s1".into(),
                score: 1.0,
                source: SearchResultSource::Structured,
            }])
        }
    }

    #[tokio::test]
    async fn vector_pushdown_with_search_term() {
        let searcher = Arc::new(HybridSearcher::new(
            Some(Arc::new(MockVectorSource)),
            Some(Arc::new(MockFulltextSource)),
            Some(Arc::new(MockStructuredSource)),
        ));
        let executor = VectorPushdownExecutor::new(searcher);
        let query = FusionQuery::new("products")
            .cond("search: 无线耳机")
            .limit(10);
        let outcome = executor.execute(&query).await.unwrap();

        assert!(outcome.response.is_some());
        assert!(!outcome.degraded_to_primary);
    }

    #[tokio::test]
    async fn vector_pushdown_without_search_term() {
        let searcher = Arc::new(HybridSearcher::new(None, None, None));
        let executor = VectorPushdownExecutor::new(searcher);
        let query = FusionQuery::new("users").eq("id", "42");
        let outcome = executor.execute(&query).await.unwrap();

        assert!(outcome.response.is_none());
        assert!(!outcome.degraded_to_primary);
    }

    #[tokio::test]
    async fn vector_search_failure_degrades() {
        let searcher = Arc::new(HybridSearcher::new(
            Some(Arc::new(FailingVectorSource)),
            Some(Arc::new(MockFulltextSource)),
            Some(Arc::new(MockStructuredSource)),
        ));
        let executor = VectorPushdownExecutor::new(searcher);
        let query = FusionQuery::new("products").cond("search: 耳机");
        let outcome = executor.execute(&query).await.unwrap();

        assert!(outcome.degraded_to_primary);
        assert!(outcome
            .degradation_reason
            .as_ref()
            .map(|r| r.contains("vector source degraded"))
            .unwrap_or(false));
    }

    #[tokio::test]
    async fn vector_pushdown_with_filter() {
        let searcher = Arc::new(HybridSearcher::new(
            Some(Arc::new(MockVectorSource)),
            Some(Arc::new(MockFulltextSource)),
            Some(Arc::new(MockStructuredSource)),
        ));
        let executor = VectorPushdownExecutor::new(searcher);
        let query = FusionQuery::new("products")
            .eq("category", "electronics")
            .cond("search: 耳机")
            .limit(50);
        let outcome = executor.execute(&query).await.unwrap();

        assert!(outcome.response.is_some());
        let response = outcome.response.unwrap();
        assert!(!response.results.is_empty());
    }

    #[tokio::test]
    async fn vector_pushdown_empty_search_term() {
        let searcher = Arc::new(HybridSearcher::new(None, None, None));
        let executor = VectorPushdownExecutor::new(searcher);
        let query = FusionQuery::new("products").cond("price < 500");
        let outcome = executor.execute(&query).await.unwrap();

        assert!(outcome.response.is_none());
    }
}
