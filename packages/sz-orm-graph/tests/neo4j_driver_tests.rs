//! Neo4j 驱动测试（需真实 Neo4j 环境）
//!
//! 所有测试标记 `#[ignore]`，默认 `cargo test` 跳过。
//! 运行方式：`cargo test --features neo4j-driver --test neo4j_driver_tests -- --ignored`
//!
//! 需要本地 Neo4j 实例运行在 127.0.0.1:7687，用户名 neo4j，密码 neo4j。

#![cfg(feature = "neo4j-driver")]

use sz_orm_graph::{GraphConfig, GraphConnection};

#[ignore]
#[test]
fn test_neo4j_connect_success() {
    let config = GraphConfig::new("neo4j://neo4j:neo4j@127.0.0.1:7687");
    let mut conn = GraphConnection::new(config);
    let result = conn.connect();
    assert!(result.is_ok(), "connect should succeed: {:?}", result);
    assert!(conn.is_connected());
}

#[ignore]
#[test]
fn test_neo4j_auth_failure() {
    let config = GraphConfig::new("neo4j://neo4j:wrong_password@127.0.0.1:7687");
    let mut conn = GraphConnection::new(config);
    let result = conn.connect();
    assert!(result.is_err());
}

#[test]
fn test_neo4j_dsn_password_sanitized() {
    let config = GraphConfig::new("neo4j://neo4j:test123@127.0.0.1:7687");
    let sanitized = config.sanitized_dsn();
    assert!(!sanitized.contains("test123"));
    assert!(sanitized.contains("***"));
}
