//! sz-orm-core graph_adapter 端到端测试
//! 验证 sz-orm-core 通过 graph feature 接入 sz-orm-graph InMemoryGraphEngine 的生产可达性

use sz_orm_core::graph_adapter::{
    graph_add_node, graph_add_relationship, graph_query, graph_query_count,
};
use sz_orm_graph::{CypherQuery, CypherQueryBuilder, GraphNode, GraphRelationship};

fn make_node(id: &str, label: &str, props: serde_json::Value) -> GraphNode {
    GraphNode {
        id: id.into(),
        labels: vec![label.into()],
        properties: props,
    }
}

fn make_rel(id: &str, rel_type: &str, start: &str, end: &str) -> GraphRelationship {
    GraphRelationship {
        id: id.into(),
        rel_type: rel_type.into(),
        start_node_id: start.into(),
        end_node_id: end.into(),
        properties: serde_json::json!({}),
    }
}

#[test]
fn test_graph_adapter_add_node_and_query() {
    graph_add_node(make_node(
        "e2e-1",
        "Person",
        serde_json::json!({"name": "Alice"}),
    ))
    .unwrap();
    let q = CypherQuery::new("MATCH (n:Person) RETURN n");
    let result = graph_query(&q).unwrap();
    assert!(
        result
            .iter()
            .any(|r| { r.as_node().map(|n| n.id == "e2e-1").unwrap_or(false) }),
        "should find the added node"
    );
    assert!(graph_query_count() >= 1);
}

#[test]
fn test_graph_adapter_where_param_query() {
    graph_add_node(make_node(
        "e2e-2",
        "Person",
        serde_json::json!({"name": "Bob"}),
    ))
    .unwrap();
    graph_add_node(make_node(
        "e2e-3",
        "Person",
        serde_json::json!({"name": "Carol"}),
    ))
    .unwrap();
    let q = CypherQueryBuilder::new("MATCH (n:Person) WHERE n.name = $name RETURN n")
        .param("name", "Bob")
        .build();
    let result = graph_query(&q).unwrap();
    assert_eq!(result.len(), 1);
    let node = result[0].as_node().unwrap();
    assert_eq!(node.id, "e2e-2");
}

#[test]
fn test_graph_adapter_relationship_query() {
    graph_add_node(make_node("e2e-4", "Person", serde_json::json!({}))).unwrap();
    graph_add_node(make_node("e2e-5", "Person", serde_json::json!({}))).unwrap();
    graph_add_relationship(make_rel("e2e-r1", "KNOWS", "e2e-4", "e2e-5")).unwrap();
    let q = CypherQuery::new("MATCH (a:Person)-[r:KNOWS]->(b:Person) RETURN a, r, b");
    let result = graph_query(&q).unwrap();
    assert!(
        result.iter().any(|r| r.as_relationship().is_some()),
        "should find the relationship"
    );
}

#[test]
fn test_graph_adapter_count_aggregation() {
    graph_add_node(make_node("e2e-6", "Person", serde_json::json!({}))).unwrap();
    graph_add_node(make_node("e2e-7", "Person", serde_json::json!({}))).unwrap();
    let q = CypherQuery::new("MATCH (n:Person) RETURN count(n)");
    let result = graph_query(&q).unwrap();
    assert_eq!(result.len(), 1);
    let scalar = result[0].as_scalar().unwrap();
    assert!(scalar.as_u64().unwrap() >= 2);
}
