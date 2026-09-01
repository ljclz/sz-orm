//! TASK-022: 图谱集成单元测试
//!
//! 验证输入"张三的朋友的朋友" → 选择图谱查询路径 + 调用 GraphQueryExecutor。

use std::sync::Arc;

use sz_orm_ai::semantic_query::{
    GraphEdge, GraphNode, GraphQueryExecutor, Nl2SqlConverter, SemanticIntent, SemanticQueryError,
    SemanticQueryResult, SemanticQueryRouter,
};

struct MockNl2Sql;
#[async_trait::async_trait]
impl Nl2SqlConverter for MockNl2Sql {
    async fn convert(&self, query: &str) -> Result<String, SemanticQueryError> {
        Ok(format!("SELECT * FROM data WHERE q = '{}'", query))
    }
}

struct MockGraphExecutor;
#[async_trait::async_trait]
impl GraphQueryExecutor for MockGraphExecutor {
    async fn execute(
        &self,
        _cypher: &str,
    ) -> Result<(Vec<GraphNode>, Vec<GraphEdge>), SemanticQueryError> {
        Ok((
            vec![
                GraphNode {
                    id: "1".to_string(),
                    label: "Person".to_string(),
                    properties: serde_json::json!({"name": "张三"}),
                },
                GraphNode {
                    id: "2".to_string(),
                    label: "Person".to_string(),
                    properties: serde_json::json!({"name": "李四"}),
                },
            ],
            vec![GraphEdge {
                from: "1".to_string(),
                to: "2".to_string(),
                relation: "FRIEND".to_string(),
            }],
        ))
    }

    async fn nl_to_cypher(&self, query: &str) -> Result<String, SemanticQueryError> {
        Ok(format!(
            "MATCH (n)-[:FRIEND*2]->(m) WHERE n.name = '张三' RETURN m /* {} */",
            query
        ))
    }
}

#[tokio::test]
async fn test_graph_intent_routing() {
    let router = SemanticQueryRouter::new(Arc::new(MockNl2Sql))
        .with_graph_executor(Arc::new(MockGraphExecutor));

    let (intent, result) = router.query("张三的朋友的朋友").await.unwrap();

    assert_eq!(intent, SemanticIntent::Graph);
    assert!(matches!(result, SemanticQueryResult::Graph { .. }));
}

#[tokio::test]
async fn test_graph_result_contains_nodes() {
    let router = SemanticQueryRouter::new(Arc::new(MockNl2Sql))
        .with_graph_executor(Arc::new(MockGraphExecutor));

    let (_, result) = router.query("张三的朋友的朋友").await.unwrap();

    if let SemanticQueryResult::Graph { nodes, edges, .. } = result {
        assert_eq!(nodes.len(), 2);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].relation, "FRIEND");
    } else {
        panic!("期望图谱结果");
    }
}

#[tokio::test]
async fn test_graph_cypher_generation() {
    let router = SemanticQueryRouter::new(Arc::new(MockNl2Sql))
        .with_graph_executor(Arc::new(MockGraphExecutor));

    let (_, result) = router.query("张三的朋友的朋友").await.unwrap();

    if let SemanticQueryResult::Graph { cypher, .. } = result {
        assert!(cypher.contains("MATCH"));
        assert!(cypher.contains("FRIEND"));
    } else {
        panic!("期望图谱结果");
    }
}

#[tokio::test]
async fn test_graph_executor_not_configured() {
    let router = SemanticQueryRouter::new(Arc::new(MockNl2Sql));
    let result = router.query("张三的朋友的朋友").await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_graph_intent_from_text() {
    assert_eq!(
        SemanticIntent::from_text("张三的朋友的朋友"),
        SemanticIntent::Graph
    );
    assert_eq!(
        SemanticIntent::from_text("查找两个节点之间的路径"),
        SemanticIntent::Graph
    );
}
