//! InMemoryGraphEngine — 内存图引擎
//!
//! 真实执行 Cypher 子集查询，替代 stub `Ok(vec![])` 实现。

use crate::cypher_parser::{CypherSubsetParser, ParsedQuery, ReturnItem};
use crate::error::GraphError;
use crate::query::{GraphNode, GraphRelationship, GraphResult};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug)]
pub struct InMemoryGraphEngine {
    nodes: HashMap<String, GraphNode>,
    relationships: HashMap<String, GraphRelationship>,
    query_count: AtomicU64,
}

impl InMemoryGraphEngine {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            relationships: HashMap::new(),
            query_count: AtomicU64::new(0),
        }
    }

    pub fn add_node(&mut self, node: GraphNode) -> Result<(), GraphError> {
        if self.nodes.contains_key(&node.id) {
            return Err(GraphError::QueryError(format!(
                "duplicate node id: {}",
                node.id
            )));
        }
        self.nodes.insert(node.id.clone(), node);
        Ok(())
    }

    pub fn add_relationship(&mut self, rel: GraphRelationship) -> Result<(), GraphError> {
        if !self.nodes.contains_key(&rel.start_node_id) {
            return Err(GraphError::QueryError(format!(
                "start node not found: {}",
                rel.start_node_id
            )));
        }
        if !self.nodes.contains_key(&rel.end_node_id) {
            return Err(GraphError::QueryError(format!(
                "end node not found: {}",
                rel.end_node_id
            )));
        }
        if self.relationships.contains_key(&rel.id) {
            return Err(GraphError::QueryError(format!(
                "duplicate relationship id: {}",
                rel.id
            )));
        }
        self.relationships.insert(rel.id.clone(), rel);
        Ok(())
    }

    pub fn execute(
        &self,
        query: &crate::query::CypherQuery,
    ) -> Result<Vec<GraphResult>, GraphError> {
        self.query_count.fetch_add(1, Ordering::Relaxed);
        let parsed = CypherSubsetParser::parse(&query.cypher, &query.parameters)?;

        match parsed {
            ParsedQuery::MatchNode {
                alias: _,
                label,
                where_clause,
                return_items,
            } => {
                if return_items
                    .iter()
                    .any(|ri| matches!(ri, ReturnItem::Count(_)))
                {
                    return self.match_count(label, where_clause, &query.parameters);
                }
                self.match_node(label, where_clause, &query.parameters)
            }
            ParsedQuery::MatchRelationship {
                from,
                rel,
                to,
                return_items: _,
            } => self.match_relationship(from, rel, to),
        }
    }

    fn match_node(
        &self,
        label: Option<String>,
        where_clause: Option<crate::cypher_parser::WhereClause>,
        params: &HashMap<String, serde_json::Value>,
    ) -> Result<Vec<GraphResult>, GraphError> {
        let mut results = Vec::new();
        for node in self.nodes.values() {
            if let Some(ref label) = label {
                if !node.labels.iter().any(|l| l == label) {
                    continue;
                }
            }
            if let Some(ref wc) = where_clause {
                let param_value = params.get(&wc.param_name).ok_or_else(|| {
                    GraphError::QueryError(format!("parameter not found: ${}", wc.param_name))
                })?;
                let node_prop = &node.properties;
                if let Some(prop_val) = node_prop.get(&wc.prop) {
                    if prop_val != param_value {
                        continue;
                    }
                } else {
                    continue;
                }
            }
            results.push(GraphResult::Node { node: node.clone() });
        }
        Ok(results)
    }

    fn match_count(
        &self,
        label: Option<String>,
        where_clause: Option<crate::cypher_parser::WhereClause>,
        params: &HashMap<String, serde_json::Value>,
    ) -> Result<Vec<GraphResult>, GraphError> {
        let nodes = self.match_node(label, where_clause, params)?;
        let count = nodes.len();
        Ok(vec![GraphResult::Scalar {
            value: serde_json::json!(count),
        }])
    }

    fn match_relationship(
        &self,
        from: crate::cypher_parser::NodePattern,
        rel: crate::cypher_parser::RelPattern,
        to: crate::cypher_parser::NodePattern,
    ) -> Result<Vec<GraphResult>, GraphError> {
        let mut results = Vec::new();
        for relationship in self.relationships.values() {
            if let Some(ref rel_type) = rel.rel_type {
                if &relationship.rel_type != rel_type {
                    continue;
                }
            }
            let start_node = match self.nodes.get(&relationship.start_node_id) {
                Some(n) => n,
                None => continue,
            };
            let end_node = match self.nodes.get(&relationship.end_node_id) {
                Some(n) => n,
                None => continue,
            };
            if let Some(ref label) = from.label {
                if !start_node.labels.iter().any(|l| l == label) {
                    continue;
                }
            }
            if let Some(ref label) = to.label {
                if !end_node.labels.iter().any(|l| l == label) {
                    continue;
                }
            }
            results.push(GraphResult::Node {
                node: start_node.clone(),
            });
            results.push(GraphResult::Relationship {
                relationship: relationship.clone(),
            });
            results.push(GraphResult::Node {
                node: end_node.clone(),
            });
        }
        Ok(results)
    }

    pub fn query_count(&self) -> u64 {
        self.query_count.load(Ordering::Relaxed)
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn relationship_count(&self) -> usize {
        self.relationships.len()
    }
}

impl Default for InMemoryGraphEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::{CypherQuery, GraphNode, GraphRelationship};
    use std::collections::HashMap;

    fn make_node(id: &str, label: &str, props: serde_json::Value) -> GraphNode {
        GraphNode {
            id: id.into(),
            labels: vec![label.into()],
            properties: props,
        }
    }

    #[test]
    fn test_add_node_duplicate_rejected() {
        let mut engine = InMemoryGraphEngine::new();
        let node = make_node("1", "Person", serde_json::json!({}));
        engine.add_node(node).unwrap();
        let dup = make_node("1", "Person", serde_json::json!({}));
        assert!(engine.add_node(dup).is_err());
    }

    #[test]
    fn test_add_relationship_endpoint_not_found() {
        let mut engine = InMemoryGraphEngine::new();
        let rel = GraphRelationship {
            id: "r1".into(),
            rel_type: "KNOWS".into(),
            start_node_id: "1".into(),
            end_node_id: "2".into(),
            properties: serde_json::json!({}),
        };
        assert!(engine.add_relationship(rel).is_err());
    }

    #[test]
    fn test_add_relationship_success() {
        let mut engine = InMemoryGraphEngine::new();
        engine
            .add_node(make_node("1", "Person", serde_json::json!({})))
            .unwrap();
        engine
            .add_node(make_node("2", "Person", serde_json::json!({})))
            .unwrap();
        let rel = GraphRelationship {
            id: "r1".into(),
            rel_type: "KNOWS".into(),
            start_node_id: "1".into(),
            end_node_id: "2".into(),
            properties: serde_json::json!({}),
        };
        assert!(engine.add_relationship(rel).is_ok());
        assert_eq!(engine.relationship_count(), 1);
    }

    #[test]
    fn test_execute_increments_query_count() {
        let engine = InMemoryGraphEngine::new();
        let q = CypherQuery::new("MATCH (n:Person) RETURN n");
        assert_eq!(engine.query_count(), 0);
        let _ = engine.execute(&q).unwrap();
        assert!(engine.query_count() >= 1);
    }

    #[test]
    fn test_execute_empty_graph_returns_empty_but_real() {
        let engine = InMemoryGraphEngine::new();
        let q = CypherQuery::new("MATCH (n:Person) RETURN n");
        let result = engine.execute(&q).unwrap();
        assert!(result.is_empty());
        assert!(engine.query_count() >= 1);
    }

    #[test]
    fn test_execute_returns_real_node() {
        let mut engine = InMemoryGraphEngine::new();
        engine
            .add_node(make_node(
                "1",
                "Person",
                serde_json::json!({"name": "Alice"}),
            ))
            .unwrap();
        let q = CypherQuery::new("MATCH (n:Person) RETURN n");
        let result = engine.execute(&q).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].as_node().is_some());
    }

    #[test]
    fn test_execute_with_where_param() {
        let mut engine = InMemoryGraphEngine::new();
        engine
            .add_node(make_node(
                "1",
                "Person",
                serde_json::json!({"name": "Alice"}),
            ))
            .unwrap();
        engine
            .add_node(make_node("2", "Person", serde_json::json!({"name": "Bob"})))
            .unwrap();
        let mut params = HashMap::new();
        params.insert("name".into(), serde_json::json!("Alice"));
        let q = CypherQuery::with_params("MATCH (n:Person) WHERE n.name = $name RETURN n", params);
        let result = engine.execute(&q).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_execute_count_aggregation() {
        let mut engine = InMemoryGraphEngine::new();
        engine
            .add_node(make_node("1", "Person", serde_json::json!({})))
            .unwrap();
        engine
            .add_node(make_node("2", "Person", serde_json::json!({})))
            .unwrap();
        let q = CypherQuery::new("MATCH (n:Person) RETURN count(n)");
        let result = engine.execute(&q).unwrap();
        assert_eq!(result.len(), 1);
        let scalar = result[0].as_scalar().unwrap();
        assert_eq!(scalar, &serde_json::json!(2));
    }

    #[test]
    fn test_execute_relationship_query() {
        let mut engine = InMemoryGraphEngine::new();
        engine
            .add_node(make_node(
                "1",
                "Person",
                serde_json::json!({"name": "Alice"}),
            ))
            .unwrap();
        engine
            .add_node(make_node("2", "Person", serde_json::json!({"name": "Bob"})))
            .unwrap();
        let rel = GraphRelationship {
            id: "r1".into(),
            rel_type: "KNOWS".into(),
            start_node_id: "1".into(),
            end_node_id: "2".into(),
            properties: serde_json::json!({}),
        };
        engine.add_relationship(rel).unwrap();

        let q = CypherQuery::new("MATCH (a:Person)-[r:KNOWS]->(b:Person) RETURN a, r, b");
        let result = engine.execute(&q).unwrap();
        assert_eq!(result.len(), 3);
        assert!(result[0].as_node().is_some());
        assert!(result[1].as_relationship().is_some());
        assert!(result[2].as_node().is_some());
    }
}
