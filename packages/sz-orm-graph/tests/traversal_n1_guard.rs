//! 图遍历 N+1 查询防护测试
//!
//! 验证 GraphTraversalExecutor 的 N+1 检测能力。

use sz_orm_graph::{
    GraphTraversalDsl, GraphTraversalExecutor, RelationEdge, RelationGraph, TraversalPath,
};

fn build_schema() -> RelationGraph {
    let mut graph = RelationGraph::new();
    graph.add_edge(RelationEdge::new(
        "users", "id", "posts", "user_id", "posts",
    ));
    graph.add_edge(RelationEdge::new(
        "posts", "id", "comments", "post_id", "comments",
    ));
    graph
}

#[test]
fn n1_guard_detects_traversal_path() {
    let graph = build_schema();
    let executor = GraphTraversalExecutor::new(graph);
    let path = GraphTraversalDsl::parse("users.posts").unwrap();
    assert!(executor.detect_n_plus_one(&path));
}

#[test]
fn n1_guard_advice_for_deep_path() {
    let graph = build_schema();
    let executor = GraphTraversalExecutor::new(graph);
    let path = GraphTraversalDsl::parse("users.posts.comments").unwrap();
    let advice = executor.n_plus_one_advice(&path).unwrap();
    assert!(advice.contains("N+1 已防护"));
}

#[test]
fn n1_guard_no_advice_for_root_only() {
    let graph = build_schema();
    let executor = GraphTraversalExecutor::new(graph);
    let path = GraphTraversalDsl::parse("users").unwrap();
    assert!(executor.n_plus_one_advice(&path).is_none());
}

#[test]
fn n1_guard_max_depth_blocks_n1_detection() {
    let graph = build_schema();
    let executor = GraphTraversalExecutor::new(graph).with_max_depth(1);
    let path = TraversalPath::new("users").with_max_depth(1);
    let result = executor.traverse_path(&path).unwrap();
    assert_eq!(result.join_count, 0);
}

#[test]
fn n1_guard_uses_single_join_flag() {
    let graph = build_schema();
    let executor = GraphTraversalExecutor::new(graph);
    let result = executor.traverse("users.posts.comments").unwrap();
    assert!(result.uses_single_join);
    assert_eq!(result.join_count, 2);
}
