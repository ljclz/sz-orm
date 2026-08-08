//! # Neo4j 集成测试
//!
//! 覆盖 Neo4j 真实连接 + 参数化 Cypher 查询 + 结果类型化映射 + 声明式建模。
//! 需要 Docker Neo4j 环境：docker-compose -f docker-compose.neo4j.yml up -d
//!
//! 运行：cargo test -p sz-orm-graph --test neo4j_integration -- --ignored

#![cfg(feature = "integration")]

use serde::Deserialize;
use sz_orm_graph::{
    CypherQuery, CypherQueryBuilder, CypherValidator, GraphConfig, GraphConnection, GraphNodeModel,
    GraphValueType, ResultMapper,
};

const DSN: &str = "neo4j://neo4j:test123@127.0.0.1:7687";

#[derive(Debug, Deserialize)]
struct Person {
    name: String,
    age: i64,
}

#[test]
#[ignore]
fn test_neo4j_connection() {
    let config = GraphConfig::new(DSN);
    let mut conn = GraphConnection::new(config);
    conn.connect().expect("should connect to Neo4j");
    assert!(conn.is_connected());
}

#[test]
#[ignore]
fn test_parameterized_cypher_query() {
    let query = CypherQueryBuilder::new("MATCH (n:Person {name: $name}) RETURN n")
        .param("name", "Alice")
        .build();

    CypherValidator::validate(&query).expect("parameterized query should pass validation");
}

#[test]
#[ignore]
fn test_sql_injection_rejected() {
    let query = CypherQuery::new("SELECT * FROM nodes");
    let result = CypherValidator::validate(&query);
    assert!(result.is_err());
}

#[test]
#[ignore]
fn test_declarative_node_model() {
    let model = GraphNodeModel::new("Person")
        .property("name", GraphValueType::String)
        .property("age", GraphValueType::Integer);

    let create_clause = model.create_clause("n");
    assert!(create_clause.contains("Person"));
    assert!(create_clause.contains("$name"));
    assert!(create_clause.contains("$age"));
}

#[test]
#[ignore]
fn test_result_type_mapping() {
    let node = sz_orm_graph::GraphNode {
        id: "1".into(),
        labels: vec!["Person".into()],
        properties: serde_json::json!({"name": "Alice", "age": 30}),
    };
    let result = sz_orm_graph::GraphResult::Node { node };
    let mapped: Vec<Person> = ResultMapper::map_to(&[result]).unwrap();
    assert_eq!(mapped[0].name, "Alice");
    assert_eq!(mapped[0].age, 30);
}

#[test]
#[ignore]
fn test_dsn_sanitization() {
    let config = GraphConfig::new(DSN);
    let sanitized = config.sanitized_dsn();
    assert!(!sanitized.contains("test123"));
    assert!(sanitized.contains("***"));
}
