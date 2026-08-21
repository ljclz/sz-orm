//! sz-orm-graph 端到端测试 — 验证 InMemoryGraphEngine 真实执行 + 连接 + 生产接线

use sz_orm_graph::{
    execute_query, CypherQuery, CypherQueryBuilder, GraphConfig, GraphConnection, GraphError,
    GraphNode, GraphPool, GraphRelationship,
};

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

#[tokio::test]
async fn test_memory_connect_success() {
    let mut conn = GraphConnection::new(GraphConfig::new("memory://localhost"));
    assert!(conn.connect().is_ok());
    assert!(conn.is_connected());
    assert!(conn.engine().is_some());
}

#[tokio::test]
async fn test_neo4j_connect_rejected() {
    let mut conn = GraphConnection::new(GraphConfig::new("neo4j://neo4j:pass@127.0.0.1:7687"));
    let result = conn.connect();
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), GraphError::DriverError(_)));
    assert!(!conn.is_connected());
}

#[tokio::test]
async fn test_bolt_connect_rejected() {
    let mut conn = GraphConnection::new(GraphConfig::new("bolt://neo4j:pass@127.0.0.1:7687"));
    let result = conn.connect();
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), GraphError::DriverError(_)));
    assert!(!conn.is_connected());
}

#[tokio::test]
async fn test_invalid_scheme_rejected() {
    let mut conn = GraphConnection::new(GraphConfig::new("http://127.0.0.1:7687"));
    let result = conn.connect();
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        GraphError::ConnectionError(_)
    ));
}

#[tokio::test]
async fn test_empty_dsn_rejected() {
    let mut conn = GraphConnection::new(GraphConfig::new(""));
    let result = conn.connect();
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        GraphError::ConnectionError(_)
    ));
}

#[tokio::test]
async fn test_disconnect_releases_engine() {
    let mut conn = GraphConnection::new(GraphConfig::new("memory://localhost"));
    conn.connect().unwrap();
    assert!(conn.engine().is_some());
    conn.disconnect();
    assert!(!conn.is_connected());
    assert!(conn.engine().is_none());
    let q = CypherQuery::new("MATCH (n) RETURN n");
    let result = execute_query(&conn, &q).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_execute_query_returns_real_node() {
    let mut conn = GraphConnection::new(GraphConfig::new("memory://localhost"));
    conn.connect().unwrap();
    conn.add_node(make_node(
        "1",
        "Person",
        serde_json::json!({"name": "Alice"}),
    ))
    .unwrap();
    let q = CypherQuery::new("MATCH (n:Person) RETURN n");
    let result = execute_query(&conn, &q).await.unwrap();
    assert!(!result.is_empty());
    assert!(result[0].as_node().is_some());
    assert!(conn.engine().unwrap().query_count() >= 1);
}

#[tokio::test]
async fn test_execute_query_empty_result_real_execution() {
    let mut conn = GraphConnection::new(GraphConfig::new("memory://localhost"));
    conn.connect().unwrap();
    let q = CypherQuery::new("MATCH (n:Person) RETURN n");
    let result = execute_query(&conn, &q).await.unwrap();
    assert!(result.is_empty());
    assert!(conn.engine().unwrap().query_count() >= 1);
}

#[tokio::test]
async fn test_execute_query_parameterization_error() {
    let mut conn = GraphConnection::new(GraphConfig::new("memory://localhost"));
    conn.connect().unwrap();
    conn.add_node(make_node(
        "1",
        "Person",
        serde_json::json!({"name": "Alice"}),
    ))
    .unwrap();
    let q = CypherQuery::new("MATCH (n:Person {name: \"Alice\"}) RETURN n");
    let result = execute_query(&conn, &q).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_execute_query_sql_rejected() {
    let mut conn = GraphConnection::new(GraphConfig::new("memory://localhost"));
    conn.connect().unwrap();
    let q = CypherQuery::new("SELECT * FROM users");
    let result = execute_query(&conn, &q).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        GraphError::SqlNotSupported(_)
    ));
}

#[tokio::test]
async fn test_execute_query_not_connected() {
    let conn = GraphConnection::new(GraphConfig::new("memory://localhost"));
    let q = CypherQuery::new("MATCH (n) RETURN n");
    let result = execute_query(&conn, &q).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        GraphError::ConnectionError(_)
    ));
}

#[tokio::test]
async fn test_execute_query_empty_cypher() {
    let conn = GraphConnection::new(GraphConfig::new("memory://localhost"));
    let q = CypherQuery::new("");
    let result = execute_query(&conn, &q).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_execute_query_with_where_param() {
    let mut conn = GraphConnection::new(GraphConfig::new("memory://localhost"));
    conn.connect().unwrap();
    conn.add_node(make_node(
        "1",
        "Person",
        serde_json::json!({"name": "Alice"}),
    ))
    .unwrap();
    conn.add_node(make_node("2", "Person", serde_json::json!({"name": "Bob"})))
        .unwrap();
    let q = CypherQueryBuilder::new("MATCH (n:Person) WHERE n.name = $name RETURN n")
        .param("name", "Alice")
        .build();
    let result = execute_query(&conn, &q).await.unwrap();
    assert_eq!(result.len(), 1);
    let node = result[0].as_node().unwrap();
    assert_eq!(node.id, "1");
}

#[tokio::test]
async fn test_execute_query_count_aggregation() {
    let mut conn = GraphConnection::new(GraphConfig::new("memory://localhost"));
    conn.connect().unwrap();
    conn.add_node(make_node("1", "Person", serde_json::json!({})))
        .unwrap();
    conn.add_node(make_node("2", "Person", serde_json::json!({})))
        .unwrap();
    conn.add_node(make_node("3", "Person", serde_json::json!({})))
        .unwrap();
    let q = CypherQuery::new("MATCH (n:Person) RETURN count(n)");
    let result = execute_query(&conn, &q).await.unwrap();
    assert_eq!(result.len(), 1);
    let scalar = result[0].as_scalar().unwrap();
    assert_eq!(scalar, &serde_json::json!(3));
}

#[tokio::test]
async fn test_execute_query_relationship() {
    let mut conn = GraphConnection::new(GraphConfig::new("memory://localhost"));
    conn.connect().unwrap();
    conn.add_node(make_node(
        "1",
        "Person",
        serde_json::json!({"name": "Alice"}),
    ))
    .unwrap();
    conn.add_node(make_node("2", "Person", serde_json::json!({"name": "Bob"})))
        .unwrap();
    conn.add_relationship(make_rel("r1", "KNOWS", "1", "2"))
        .unwrap();
    let q = CypherQuery::new("MATCH (a:Person)-[r:KNOWS]->(b:Person) RETURN a, r, b");
    let result = execute_query(&conn, &q).await.unwrap();
    assert!(!result.is_empty());
    assert!(result.iter().any(|r| r.as_relationship().is_some()));
}

#[tokio::test]
async fn test_data_consistency() {
    let mut conn = GraphConnection::new(GraphConfig::new("memory://localhost"));
    conn.connect().unwrap();
    conn.add_node(make_node("1", "Person", serde_json::json!({})))
        .unwrap();
    conn.add_node(make_node("2", "Person", serde_json::json!({})))
        .unwrap();
    conn.add_relationship(make_rel("r1", "KNOWS", "1", "2"))
        .unwrap();

    let q_nodes = CypherQuery::new("MATCH (n:Person) RETURN n");
    let nodes = execute_query(&conn, &q_nodes).await.unwrap();
    assert_eq!(nodes.len(), 2);

    let q_rels = CypherQuery::new("MATCH (a:Person)-[r:KNOWS]->(b:Person) RETURN a, r, b");
    let rels = execute_query(&conn, &q_rels).await.unwrap();
    assert!(!rels.is_empty());
}

#[tokio::test]
async fn test_pool_with_memory_dsn() {
    let pool = GraphPool::new(GraphConfig::new("memory://localhost"));
    let conn = pool.acquire().await.unwrap();
    assert!(conn.is_connected());
    pool.release(conn).await;
    assert_eq!(pool.idle_count(), 1);
}
