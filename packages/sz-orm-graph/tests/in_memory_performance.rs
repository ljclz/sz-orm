//! 性能测试 — 基于 InMemoryGraphEngine（不依赖外部 Neo4j Docker）。
//!
//! 验证 1000 节点图查询 P95 ≤ 500ms。
//! InMemoryGraphEngine 为栈分配，测试结束自动释放，无临时文件、无进程残留。
//!
//! 运行：`cargo test -p sz-orm-graph --test in_memory_performance -- --nocapture`

use std::time::Instant;
use sz_orm_graph::{CypherQuery, CypherQueryBuilder, GraphNode, InMemoryGraphEngine};

const NODE_COUNT: usize = 1000;
const QUERY_COUNT: usize = 100;
const P95_LIMIT_MS: u64 = 500;

fn make_node(id: usize) -> GraphNode {
    GraphNode {
        id: id.to_string(),
        labels: vec!["Person".to_string()],
        properties: serde_json::json!({"id": id, "name": format!("person_{}", id)}),
    }
}

#[test]
fn test_in_memory_1000_node_query_p95() {
    let mut engine = InMemoryGraphEngine::new();

    for i in 0..NODE_COUNT {
        engine.add_node(make_node(i)).unwrap();
    }

    let mut latencies = Vec::with_capacity(QUERY_COUNT);

    for i in 0..QUERY_COUNT {
        let query = CypherQueryBuilder::new("MATCH (n:Person) WHERE n.id = $id RETURN n")
            .param("id", i as i64)
            .build();

        let start = Instant::now();
        let result = engine.execute(&query).unwrap();
        latencies.push(start.elapsed().as_millis() as u64);

        assert!(!result.is_empty(), "查询 {} 应返回结果", i);
    }

    latencies.sort();
    let p95_idx = (latencies.len() as f64 * 0.95).ceil() as usize - 1;
    let p95 = latencies[p95_idx];

    println!("P95 延迟: {}ms (限制: {}ms)", p95, P95_LIMIT_MS);
    assert!(
        p95 <= P95_LIMIT_MS,
        "P95 延迟 {}ms 超过限制 {}ms",
        p95,
        P95_LIMIT_MS
    );
    assert!(
        engine.query_count() >= QUERY_COUNT as u64,
        "query_count 应 >= {}, 实际: {}",
        QUERY_COUNT,
        engine.query_count()
    );
}

#[test]
fn test_in_memory_query_count_increments() {
    let mut engine = InMemoryGraphEngine::new();
    engine
        .add_node(GraphNode {
            id: "1".into(),
            labels: vec!["Person".into()],
            properties: serde_json::json!({"name": "Alice"}),
        })
        .unwrap();

    let before = engine.query_count();
    let q = CypherQuery::new("MATCH (n:Person) RETURN n");
    let _ = engine.execute(&q).unwrap();
    let after = engine.query_count();

    assert!(
        after > before,
        "query_count 必须递增: before={}, after={}",
        before,
        after
    );
}

#[test]
fn test_in_memory_real_node_returned() {
    let mut engine = InMemoryGraphEngine::new();
    engine
        .add_node(GraphNode {
            id: "1".into(),
            labels: vec!["Person".into()],
            properties: serde_json::json!({"name": "Alice"}),
        })
        .unwrap();

    let q = CypherQuery::new("MATCH (n:Person) RETURN n");
    let result = engine.execute(&q).unwrap();

    assert!(!result.is_empty(), "必须返回真实节点数据，非空");
    assert!(result[0].as_node().is_some(), "结果必须是节点类型");

    let node = result[0].as_node().unwrap();
    assert_eq!(
        node.properties.get("name").unwrap(),
        &serde_json::json!("Alice"),
        "节点属性必须匹配"
    );
}
