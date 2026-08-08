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
