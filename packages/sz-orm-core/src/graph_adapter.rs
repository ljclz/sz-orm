//! # Graph Adapter — sz-orm-core 图数据库适配层
//!
//! v5.0.0：将 sz-orm-graph 的 InMemoryGraphEngine 接入 sz-orm-core，
//! 提供 `graph_query` / `graph_add_node` / `graph_add_relationship` 三个入口，
//! 使图数据库能力从"幻影交付"变为"生产可达"。
//!
//! ## 设计
//!
//! - 全局引擎：`OnceLock<parking_lot::RwLock<InMemoryGraphEngine>>`
//! - 首次调用时惰性初始化空引擎
//! - 读操作（graph_query）取读锁，写操作（add_node/add_relationship）取写锁
//! - Cypher 参数化校验由 sz-orm-graph::CypherValidator 负责

use std::sync::OnceLock;

use parking_lot::RwLock;
use sz_orm_graph::{CypherQuery, GraphNode, GraphRelationship, GraphResult, InMemoryGraphEngine};

static GRAPH_ENGINE: OnceLock<RwLock<InMemoryGraphEngine>> = OnceLock::new();

fn engine() -> &'static RwLock<InMemoryGraphEngine> {
    GRAPH_ENGINE.get_or_init(|| RwLock::new(InMemoryGraphEngine::new()))
}

/// 执行 Cypher 查询（只读）
///
/// 内部取读锁，调用 `InMemoryGraphEngine::execute`。
/// Cypher 参数化校验由引擎内部完成。
pub fn graph_query(query: &CypherQuery) -> Result<Vec<GraphResult>, sz_orm_graph::GraphError> {
    let engine = engine().read();
    engine.execute(query)
}

/// 添加节点（写操作）
///
/// 内部取写锁，调用 `InMemoryGraphEngine::add_node`。
pub fn graph_add_node(node: GraphNode) -> Result<(), sz_orm_graph::GraphError> {
    let mut engine = engine().write();
    engine.add_node(node)
}

/// 添加关系（写操作）
///
/// 内部取写锁，调用 `InMemoryGraphEngine::add_relationship`。
pub fn graph_add_relationship(rel: GraphRelationship) -> Result<(), sz_orm_graph::GraphError> {
    let mut engine = engine().write();
    engine.add_relationship(rel)
}

/// 获取当前引擎中的查询计数（用于测试验证真实执行）
pub fn graph_query_count() -> u64 {
    let engine = engine().read();
    engine.query_count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graph_add_and_query() {
        let node = GraphNode {
            id: "1".into(),
            labels: vec!["Person".into()],
            properties: serde_json::json!({"name": "Alice"}),
        };
        graph_add_node(node).unwrap();

        let q = CypherQuery::new("MATCH (n:Person) RETURN n");
        let result = graph_query(&q).unwrap();
        assert!(!result.is_empty());
        assert!(result[0].as_node().is_some());
    }

    #[test]
    fn test_graph_query_count_increments() {
        let before = graph_query_count();
        let q = CypherQuery::new("MATCH (n:Person) RETURN n");
        let _ = graph_query(&q).unwrap();
        let after = graph_query_count();
        assert!(after > before);
    }
}
