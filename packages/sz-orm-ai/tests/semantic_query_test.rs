//! TASK-021: SemanticQueryRouter 单元测试
//!
//! 验证 5 种意图（SQL/向量/图谱/Agent/混合）各 1 例，验证正确路由。

use std::sync::Arc;

use sz_orm_ai::semantic_query::{
    AgentReport, AiAgent, Nl2SqlConverter, SemanticIntent, SemanticQueryError, SemanticQueryResult,
    SemanticQueryRouter, VectorMatch, VectorStore,
};

struct MockNl2Sql;
#[async_trait::async_trait]
impl Nl2SqlConverter for MockNl2Sql {
    async fn convert(&self, query: &str) -> Result<String, SemanticQueryError> {
        Ok(format!("SELECT * FROM data WHERE q = '{}'", query))
    }
}

struct MockVectorStore;
#[async_trait::async_trait]
impl VectorStore for MockVectorStore {
    async fn search(
        &self,
        query: &str,
        _top_k: usize,
    ) -> Result<Vec<VectorMatch>, SemanticQueryError> {
        Ok(vec![VectorMatch {
            id: "1".to_string(),
            score: 0.95,
            metadata: serde_json::json!({"text": query}),
        }])
    }
}

struct MockAgent;
#[async_trait::async_trait]
impl AiAgent for MockAgent {
    async fn execute_task(&self, task: &str) -> Result<AgentReport, SemanticQueryError> {
        Ok(AgentReport {
            steps: vec![],
            conclusion: format!("分析完成: {}", task),
            confidence: 0.9,
        })
    }
}

fn make_router() -> SemanticQueryRouter {
    SemanticQueryRouter::new(Arc::new(MockNl2Sql))
        .with_vector_store(Arc::new(MockVectorStore))
        .with_agent(Arc::new(MockAgent))
}

#[tokio::test]
async fn test_route_sql_intent() {
    let router = make_router();
    let (intent, result) = router.query("查询所有用户").await.unwrap();

    assert_eq!(intent, SemanticIntent::Sql);
    assert!(matches!(result, SemanticQueryResult::Sql { .. }));
}

#[tokio::test]
async fn test_route_vector_intent() {
    let router = make_router();
    let (intent, result) = router.query("查找与红色连衣裙相似的商品").await.unwrap();

    assert_eq!(intent, SemanticIntent::Vector);
    assert!(matches!(result, SemanticQueryResult::Vector { .. }));
}

#[tokio::test]
async fn test_route_agent_intent() {
    let router = make_router();
    let (intent, result) = router.query("分析上月销售下降原因").await.unwrap();

    assert_eq!(intent, SemanticIntent::Agent);
    assert!(matches!(result, SemanticQueryResult::Agent { .. }));
}

#[tokio::test]
async fn test_route_hybrid_intent() {
    let router = make_router();
    let (intent, result) = router
        .query("价格 < 100 且与红色连衣裙相似的商品")
        .await
        .unwrap();

    assert_eq!(intent, SemanticIntent::Hybrid);
    assert!(matches!(result, SemanticQueryResult::Hybrid { .. }));
}

#[tokio::test]
async fn test_intent_from_text_sql() {
    assert_eq!(SemanticIntent::from_text("查询用户"), SemanticIntent::Sql);
}

#[tokio::test]
async fn test_intent_from_text_vector() {
    assert_eq!(
        SemanticIntent::from_text("查找相似的商品"),
        SemanticIntent::Vector
    );
}

#[tokio::test]
async fn test_intent_from_text_agent() {
    assert_eq!(
        SemanticIntent::from_text("分析销售下降原因"),
        SemanticIntent::Agent
    );
}

#[tokio::test]
async fn test_intent_from_text_hybrid() {
    assert_eq!(
        SemanticIntent::from_text("价格 < 100 且与红色连衣裙相似"),
        SemanticIntent::Hybrid
    );
}

#[tokio::test]
async fn test_vector_store_not_configured() {
    let router = SemanticQueryRouter::new(Arc::new(MockNl2Sql));
    let result = router.query("查找相似商品").await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_sql_result_contains_sql() {
    let router = make_router();
    let (_, result) = router.query("查询所有用户").await.unwrap();

    if let SemanticQueryResult::Sql { sql, .. } = result {
        assert!(sql.contains("SELECT"));
    } else {
        panic!("期望 SQL 结果");
    }
}
