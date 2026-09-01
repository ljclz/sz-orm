//! Cypher 写操作测试套件
//!
//! 基于 InMemoryGraphEngine，不依赖外部 Neo4j 环境。
//! 覆盖 CREATE/MERGE/DELETE/SET 写操作 + 只读查询不退化。

use std::collections::HashMap;
use sz_orm_graph::{CypherQuery, GraphNode, GraphRelationship, InMemoryGraphEngine};

fn make_node(id: &str, label: &str, props: serde_json::Value) -> GraphNode {
    GraphNode {
        id: id.into(),
        labels: vec![label.into()],
        properties: props,
    }
}

#[test]
fn test_create_node_adds_to_graph() {
    let mut engine = InMemoryGraphEngine::new();
    let mut params = HashMap::new();
    params.insert("name".into(), serde_json::json!("Alice"));
    let q = CypherQuery::with_params("CREATE (n:Person {name: $name})", params);
    let result = engine.execute_mut(&q).unwrap();
    assert!(result.is_empty());
    assert_eq!(engine.node_count(), 1);
    assert!(engine.query_count() >= 1);
}

#[test]
fn test_create_node_parameterized() {
    let mut engine = InMemoryGraphEngine::new();
    let mut params = HashMap::new();
    params.insert("name".into(), serde_json::json!("Bob"));
    params.insert("age".into(), serde_json::json!(30));
    let q = CypherQuery::with_params("CREATE (n:Person {name: $name, age: $age})", params);
    engine.execute_mut(&q).unwrap();

    let read_q = CypherQuery::new("MATCH (n:Person) RETURN n");
    let results = engine.execute_mut(&read_q).unwrap();
    assert_eq!(results.len(), 1);
    let node = results[0].as_node().unwrap();
    assert_eq!(node.properties["name"], serde_json::json!("Bob"));
    assert_eq!(node.properties["age"], serde_json::json!(30));
}

#[test]
fn test_create_duplicate_id_rejected() {
    let mut engine = InMemoryGraphEngine::new();
    engine
        .add_node(make_node("fixed_id", "Person", serde_json::json!({})))
        .unwrap();
    let dup = make_node("fixed_id", "Person", serde_json::json!({}));
    let result = engine.add_node(dup);
    assert!(result.is_err());
    assert_eq!(engine.node_count(), 1);
}

#[test]
fn test_merge_node_idempotent() {
    let mut engine = InMemoryGraphEngine::new();
    let mut params = HashMap::new();
    params.insert("name".into(), serde_json::json!("Alice"));
    let q = CypherQuery::with_params("MERGE (n:Person {name: $name})", params.clone());
    engine.execute_mut(&q).unwrap();
    assert_eq!(engine.node_count(), 1);

    let q2 = CypherQuery::with_params("MERGE (n:Person {name: $name})", params);
    engine.execute_mut(&q2).unwrap();
    assert_eq!(engine.node_count(), 1);
}

#[test]
fn test_merge_node_creates_if_not_exists() {
    let mut engine = InMemoryGraphEngine::new();
    let mut params = HashMap::new();
    params.insert("name".into(), serde_json::json!("Charlie"));
    let q = CypherQuery::with_params("MERGE (n:Person {name: $name})", params);
    engine.execute_mut(&q).unwrap();
    assert_eq!(engine.node_count(), 1);
}

#[test]
fn test_delete_node_removes_from_graph() {
    let mut engine = InMemoryGraphEngine::new();
    engine
        .add_node(make_node("del_target", "Person", serde_json::json!({})))
        .unwrap();
    assert_eq!(engine.node_count(), 1);

    let q = CypherQuery::new("DELETE del_target");
    engine.execute_mut(&q).unwrap();
    assert_eq!(engine.node_count(), 0);
}

#[test]
fn test_delete_cascade_removes_relationships() {
    let mut engine = InMemoryGraphEngine::new();
    engine
        .add_node(make_node("n1", "Person", serde_json::json!({})))
        .unwrap();
    engine
        .add_node(make_node("n2", "Person", serde_json::json!({})))
        .unwrap();
    let rel = GraphRelationship {
        id: "r1".into(),
        rel_type: "KNOWS".into(),
        start_node_id: "n1".into(),
        end_node_id: "n2".into(),
        properties: serde_json::json!({}),
    };
    engine.add_relationship(rel).unwrap();
    assert_eq!(engine.relationship_count(), 1);

    let q = CypherQuery::new("DELETE n1");
    engine.execute_mut(&q).unwrap();
    assert_eq!(engine.node_count(), 1);
    assert_eq!(engine.relationship_count(), 0);
}

#[test]
fn test_set_updates_property() {
    let mut engine = InMemoryGraphEngine::new();
    engine
        .add_node(make_node(
            "set_target",
            "Person",
            serde_json::json!({"name": "OldName"}),
        ))
        .unwrap();

    let mut params = HashMap::new();
    params.insert("new_name".into(), serde_json::json!("NewName"));
    let q = CypherQuery::with_params("SET set_target.name = $new_name", params);
    engine.execute_mut(&q).unwrap();

    let read_q = CypherQuery::new("MATCH (n:Person) RETURN n");
    let results = engine.execute_mut(&read_q).unwrap();
    let node = results[0].as_node().unwrap();
    assert_eq!(node.properties["name"], serde_json::json!("NewName"));
}

#[test]
fn test_set_nonexistent_node_rejected() {
    let mut engine = InMemoryGraphEngine::new();
    let mut params = HashMap::new();
    params.insert("val".into(), serde_json::json!("test"));
    let q = CypherQuery::with_params("SET ghost_node.prop = $val", params);
    let result = engine.execute_mut(&q);
    assert!(result.is_err());
}

#[test]
fn test_write_op_increments_query_count() {
    let mut engine = InMemoryGraphEngine::new();
    assert_eq!(engine.query_count(), 0);
    let mut params = HashMap::new();
    params.insert("name".into(), serde_json::json!("Alice"));
    let q = CypherQuery::with_params("CREATE (n:Person {name: $name})", params);
    engine.execute_mut(&q).unwrap();
    assert!(engine.query_count() >= 1);
}

#[test]
fn test_read_query_still_works() {
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

    let q = CypherQuery::new("MATCH (n:Person) RETURN n");
    let result = engine.execute_mut(&q).unwrap();
    assert_eq!(result.len(), 2);

    let count_q = CypherQuery::new("MATCH (n:Person) RETURN count(n)");
    let count_result = engine.execute_mut(&count_q).unwrap();
    assert_eq!(count_result.len(), 1);
    let scalar = count_result[0].as_scalar().unwrap();
    assert_eq!(scalar, &serde_json::json!(2));
}
