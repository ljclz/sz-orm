//! # Query — Cypher 查询构造与执行
//!
//! CypherQuery + CypherQueryBuilder + GraphResult

use crate::error::GraphError;
use std::collections::HashMap;

/// Cypher 查询
#[derive(Debug, Clone)]
pub struct CypherQuery {
    pub cypher: String,
    pub parameters: HashMap<String, serde_json::Value>,
}

impl CypherQuery {
    pub fn new(cypher: &str) -> Self {
        Self {
            cypher: cypher.to_string(),
            parameters: HashMap::new(),
        }
    }

    pub fn with_params(cypher: &str, params: HashMap<String, serde_json::Value>) -> Self {
        Self {
            cypher: cypher.to_string(),
            parameters: params,
        }
    }

    pub fn add_param(&mut self, key: &str, value: serde_json::Value) {
        self.parameters.insert(key.to_string(), value);
    }
}

/// Cypher 查询构造器（链式）
pub struct CypherQueryBuilder {
    query: CypherQuery,
}

impl CypherQueryBuilder {
    pub fn new(cypher: &str) -> Self {
        Self {
            query: CypherQuery::new(cypher),
        }
    }

    pub fn param(mut self, key: &str, value: impl Into<serde_json::Value>) -> Self {
        self.query.add_param(key, value.into());
        self
    }

    pub fn build(self) -> CypherQuery {
        self.query
    }
}

/// 图节点
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub labels: Vec<String>,
    pub properties: serde_json::Value,
}

/// 图关系
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GraphRelationship {
    pub id: String,
    pub rel_type: String,
    pub start_node_id: String,
    pub end_node_id: String,
    pub properties: serde_json::Value,
}

/// 图路径
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GraphPath {
    pub nodes: Vec<GraphNode>,
    pub relationships: Vec<GraphRelationship>,
}

/// 图查询结果
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum GraphResult {
    Node { node: GraphNode },
    Relationship { relationship: GraphRelationship },
    Path { path: GraphPath },
    Scalar { value: serde_json::Value },
}

impl GraphResult {
    pub fn as_node(&self) -> Option<&GraphNode> {
        match self {
            GraphResult::Node { node } => Some(node),
            _ => None,
        }
    }

    pub fn as_relationship(&self) -> Option<&GraphRelationship> {
        match self {
            GraphResult::Relationship { relationship } => Some(relationship),
            _ => None,
        }
    }

    pub fn as_scalar(&self) -> Option<&serde_json::Value> {
        match self {
            GraphResult::Scalar { value } => Some(value),
            _ => None,
        }
    }
}

/// 查询执行器
pub async fn execute_query(
    conn: &crate::connection::GraphConnection,
    query: &CypherQuery,
) -> Result<Vec<GraphResult>, GraphError> {
    if !conn.is_connected() {
        return Err(GraphError::ConnectionError("not connected".into()));
    }
    if query.cypher.is_empty() {
        return Err(GraphError::QueryError("empty query".into()));
    }
    Ok(vec![])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::{GraphConfig, GraphConnection};
    use std::collections::HashMap;

    #[test]
    fn test_cypher_query_new() {
        let q = CypherQuery::new("MATCH (n:Person) RETURN n");
        assert_eq!(q.cypher, "MATCH (n:Person) RETURN n");
        assert!(q.parameters.is_empty());
    }

    #[test]
    fn test_cypher_query_with_params() {
        let mut params = HashMap::new();
        params.insert("name".into(), serde_json::json!("Alice"));
        let q = CypherQuery::with_params("MATCH (n {name: $name}) RETURN n", params);
        assert_eq!(q.parameters.len(), 1);
        assert_eq!(q.parameters["name"], serde_json::json!("Alice"));
    }

    #[test]
    fn test_cypher_query_add_param() {
        let mut q = CypherQuery::new("MATCH (n) RETURN n");
        assert!(q.parameters.is_empty());
        q.add_param("age", serde_json::json!(30));
        q.add_param("city", serde_json::json!("Beijing"));
        assert_eq!(q.parameters.len(), 2);
        assert_eq!(q.parameters["age"], serde_json::json!(30));
    }

    #[test]
    fn test_cypher_query_builder_chain() {
        let q = CypherQueryBuilder::new("MATCH (n {name: $name, age: $age}) RETURN n")
            .param("name", "Alice")
            .param("age", 30i64)
            .build();
        assert_eq!(q.parameters.len(), 2);
        assert_eq!(q.parameters["name"], serde_json::json!("Alice"));
        assert_eq!(q.parameters["age"], serde_json::json!(30));
    }

    #[test]
    fn test_graph_result_as_node() {
        let node = GraphNode {
            id: "1".into(),
            labels: vec!["Person".into()],
            properties: serde_json::json!({}),
        };
        let result = GraphResult::Node { node };
        assert!(result.as_node().is_some());
        assert!(result.as_relationship().is_none());
        assert!(result.as_scalar().is_none());
    }

    #[test]
    fn test_graph_result_as_relationship_and_scalar() {
        let rel = GraphRelationship {
            id: "r1".into(),
            rel_type: "KNOWS".into(),
            start_node_id: "1".into(),
            end_node_id: "2".into(),
            properties: serde_json::json!({}),
        };
        let rel_result = GraphResult::Relationship { relationship: rel };
        assert!(rel_result.as_relationship().is_some());
        assert!(rel_result.as_node().is_none());

        let scalar_result = GraphResult::Scalar {
            value: serde_json::json!(42),
        };
        assert_eq!(scalar_result.as_scalar(), Some(&serde_json::json!(42)));
        assert!(scalar_result.as_node().is_none());
    }

    #[tokio::test]
    async fn test_execute_query_not_connected() {
        let conn = GraphConnection::new(GraphConfig::new("bolt://localhost:7687"));
        assert!(!conn.is_connected());
        let q = CypherQuery::new("MATCH (n) RETURN n");
        let result = execute_query(&conn, &q).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_query_empty_cypher() {
        let conn = GraphConnection::new(GraphConfig::new("bolt://localhost:7687"));
        let q = CypherQuery::new("");
        let result = execute_query(&conn, &q).await;
        assert!(result.is_err());
    }
}
