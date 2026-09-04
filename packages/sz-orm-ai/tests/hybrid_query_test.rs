//! TASK-024: HybridQueryExecutor 单元测试
//!
//! 验证输入"价格 < 100 且与'红色连衣裙'相似的商品"，
//! 生成 SQL 过滤 + 向量排序的混合查询。

use std::sync::Arc;

use sz_orm_ai::semantic_query::{
    HybridQueryExecutor, Nl2SqlConverter, SemanticQueryError, SemanticQueryResult,
    SemanticVectorStore, VectorMatch,
};

struct MockNl2Sql;
#[async_trait::async_trait]
impl Nl2SqlConverter for MockNl2Sql {
    async fn convert(&self, query: &str) -> Result<String, SemanticQueryError> {
        Ok(format!("SELECT * FROM products WHERE {}", query))
    }
}

struct MockVectorStore;
#[async_trait::async_trait]
impl SemanticVectorStore for MockVectorStore {
    async fn search(
        &self,
        query: &str,
        top_k: usize,
    ) -> Result<Vec<VectorMatch>, SemanticQueryError> {
        Ok((0..top_k.min(3))
            .map(|i| VectorMatch {
                id: format!("p{}", i + 1),
                score: 0.95 - i as f64 * 0.1,
                metadata: serde_json::json!({"name": format!("商品{}", i + 1), "query": query}),
            })
            .collect())
    }
}

fn make_executor() -> HybridQueryExecutor {
    HybridQueryExecutor::new(Arc::new(MockNl2Sql), Arc::new(MockVectorStore))
}

#[tokio::test]
async fn test_hybrid_execute_basic() {
    let executor = make_executor();
    let result = executor
        .execute_hybrid("价格 < 100 且与红色连衣裙相似的商品", 10)
        .await
        .unwrap();

    assert!(matches!(result, SemanticQueryResult::Hybrid { .. }));
}

#[tokio::test]
async fn test_hybrid_sql_filter_generated() {
    let executor = make_executor();
    let result = executor
        .execute_hybrid("价格 < 100 且与红色连衣裙相似的商品", 10)
        .await
        .unwrap();

    if let SemanticQueryResult::Hybrid { sql_filter, .. } = result {
        assert!(sql_filter.contains("SELECT"));
        assert!(sql_filter.contains("products"));
    } else {
        panic!("期望混合结果");
    }
}

#[tokio::test]
async fn test_hybrid_vector_query_preserved() {
    let executor = make_executor();
    let result = executor
        .execute_hybrid("价格 < 100 且与红色连衣裙相似的商品", 10)
        .await
        .unwrap();

    if let SemanticQueryResult::Hybrid { vector_query, .. } = result {
        assert!(vector_query.contains("红色连衣裙") || vector_query.contains("相似"));
    } else {
        panic!("期望混合结果");
    }
}

#[tokio::test]
async fn test_hybrid_results_have_scores() {
    let executor = make_executor();
    let result = executor
        .execute_hybrid("价格 < 100 且与红色连衣裙相似的商品", 10)
        .await
        .unwrap();

    if let SemanticQueryResult::Hybrid { results, .. } = result {
        assert!(!results.is_empty());
        for r in &results {
            assert!(r.vector_score > 0.0);
            assert!(!r.id.is_empty());
        }
    } else {
        panic!("期望混合结果");
    }
}

#[tokio::test]
async fn test_hybrid_top_k_limit() {
    let executor = make_executor();
    let result = executor
        .execute_hybrid("价格 < 100 且与红色连衣裙相似的商品", 2)
        .await
        .unwrap();

    if let SemanticQueryResult::Hybrid { results, .. } = result {
        assert!(results.len() <= 2);
    } else {
        panic!("期望混合结果");
    }
}

#[tokio::test]
async fn test_hybrid_results_sorted_by_score() {
    let executor = make_executor();
    let result = executor
        .execute_hybrid("价格 < 100 且与红色连衣裙相似的商品", 3)
        .await
        .unwrap();

    if let SemanticQueryResult::Hybrid { results, .. } = result {
        for i in 1..results.len() {
            assert!(results[i - 1].vector_score >= results[i].vector_score);
        }
    } else {
        panic!("期望混合结果");
    }
}

#[tokio::test]
async fn test_hybrid_query_split() {
    let executor = make_executor();
    let result = executor
        .execute_hybrid("价格 < 100 且与红色连衣裙相似的商品", 10)
        .await
        .unwrap();

    if let SemanticQueryResult::Hybrid {
        sql_filter,
        vector_query,
        ..
    } = result
    {
        assert!(!sql_filter.is_empty());
        assert!(!vector_query.is_empty());
    } else {
        panic!("期望混合结果");
    }
}
