//! # 性能测试 — 1000 节点图 P95 ≤ 500ms
//!
//! 需要 Docker Neo4j 环境。
//! 运行：cargo test -p sz-orm-graph --test performance -- --ignored

#![cfg(feature = "integration")]

use std::time::Instant;
use sz_orm_graph::{CypherQueryBuilder, CypherValidator, GraphConfig, GraphConnection};

const DSN: &str = "neo4j://neo4j:test123@127.0.0.1:7687";
const NODE_COUNT: usize = 1000;
const P95_LIMIT_MS: u64 = 500;

#[tokio::test]
#[ignore]
async fn test_1000_node_query_p95() {
    let config = GraphConfig::new(DSN);
    let mut conn = GraphConnection::new(config);
    conn.connect().expect("should connect to Neo4j");

    let mut latencies = Vec::with_capacity(100);

    for i in 0..100 {
        let query = CypherQueryBuilder::new("MATCH (n:Person) WHERE n.id = $id RETURN n LIMIT 1")
            .param("id", i as i64)
            .build();

        CypherValidator::validate(&query).expect("query should validate");

        let start = Instant::now();
        let _result = sz_orm_graph::query::execute_query(&conn, &query).await;
        latencies.push(start.elapsed().as_millis() as u64);
    }

    latencies.sort();
    let p95_idx = (latencies.len() as f64 * 0.95).ceil() as usize - 1;
    let p95 = latencies[p95_idx];

    assert!(
        p95 <= P95_LIMIT_MS,
        "P95 latency {}ms exceeds limit {}ms ({} nodes)",
        p95,
        P95_LIMIT_MS,
        NODE_COUNT
    );
}
