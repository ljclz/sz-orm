//! # Mapping — 结果类型化映射
//!
//! ResultMapper + NodeMapper + RelationMapper

use crate::error::GraphError;
use crate::query::{GraphNode, GraphRelationship, GraphResult};
use serde::de::DeserializeOwned;

/// 结果映射器
pub struct ResultMapper;

impl ResultMapper {
    /// 将 GraphResult 列表反序列化到用户结构
    pub fn map_to<T: DeserializeOwned>(results: &[GraphResult]) -> Result<Vec<T>, GraphError> {
        let values: Vec<serde_json::Value> = results.iter().map(Self::result_to_json).collect();
        serde_json::from_value(serde_json::Value::Array(values))
            .map_err(|e| GraphError::MappingError(format!("deserialization failed: {}", e)))
    }

    /// 将单个 GraphResult 转为 JSON Value
    ///
    /// 对于 Node/Relationship，返回 properties 字段（用户通常需要属性而非元数据）。
    /// 对于 Scalar，直接返回值。
    fn result_to_json(result: &GraphResult) -> serde_json::Value {
        match result {
            GraphResult::Node { node } => node.properties.clone(),
            GraphResult::Relationship { relationship } => relationship.properties.clone(),
            GraphResult::Path { path } => {
                serde_json::to_value(path).unwrap_or(serde_json::Value::Null)
            }
            GraphResult::Scalar { value } => value.clone(),
        }
    }
}

/// 节点映射器
pub struct NodeMapper;

impl NodeMapper {
    /// 从结果列表提取所有节点
    pub fn extract_nodes(results: &[GraphResult]) -> Vec<&GraphNode> {
        results.iter().filter_map(|r| r.as_node()).collect()
    }

    /// 将节点属性反序列化到用户结构
    pub fn map_node<T: DeserializeOwned>(node: &GraphNode) -> Result<T, GraphError> {
        serde_json::from_value(node.properties.clone()).map_err(|e| {
            GraphError::MappingError(format!(
                "node mapping failed: {} (missing field or type mismatch)",
                e
            ))
        })
    }
}

/// 关系映射器
pub struct RelationMapper;

impl RelationMapper {
    /// 从结果列表提取所有关系
    pub fn extract_relationships(results: &[GraphResult]) -> Vec<&GraphRelationship> {
        results.iter().filter_map(|r| r.as_relationship()).collect()
    }

    /// 将关系属性反序列化到用户结构
    pub fn map_relationship<T: DeserializeOwned>(rel: &GraphRelationship) -> Result<T, GraphError> {
        serde_json::from_value(rel.properties.clone()).map_err(|e| {
            GraphError::MappingError(format!(
                "relationship mapping failed: {} (missing field or type mismatch)",
                e
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct Person {
        name: String,
        age: i64,
    }

    #[test]
    fn test_node_mapping() {
        let node = GraphNode {
            id: "1".into(),
            labels: vec!["Person".into()],
            properties: serde_json::json!({"name": "Alice", "age": 30}),
        };
        let result = GraphResult::Node { node };
        let mapped: Vec<Person> = ResultMapper::map_to(&[result]).unwrap();
        assert_eq!(mapped.len(), 1);
        assert_eq!(mapped[0].name, "Alice");
        assert_eq!(mapped[0].age, 30);
    }

    #[test]
    fn test_mapping_error_on_missing_field() {
        let node = GraphNode {
            id: "1".into(),
            labels: vec!["Person".into()],
            properties: serde_json::json!({"name": "Alice"}),
        };
        let result = GraphResult::Node { node };
        let mapped: Result<Vec<Person>, _> = ResultMapper::map_to(&[result]);
        assert!(mapped.is_err());
        let err = mapped.unwrap_err();
        assert!(err.to_string().contains("mapping"));
    }
}
