//! 图遍历端到端接线测试
//!
//! 验证 DSL → 编译器 → 执行器 → 水化器 全链路接线。

use std::collections::HashMap;

use sz_orm_graph::{
    GraphResultHydrator, GraphTraversalCompiler, GraphTraversalDsl, GraphTraversalExecutor,
    RelationEdge, RelationGraph, TraversalPathCache,
};

fn build_schema() -> RelationGraph {
    let mut graph = RelationGraph::new();
    graph.add_edge(RelationEdge::new(
        "users", "id", "posts", "user_id", "posts",
    ));
    graph.add_edge(RelationEdge::new(
        "posts", "id", "comments", "post_id", "comments",
    ));
    graph.add_edge(RelationEdge::new(
        "users", "id", "profiles", "user_id", "profiles",
    ));
    graph
}

#[test]
fn wiring_dsl_to_compiler() {
    let graph = build_schema();
    let compiler = GraphTraversalCompiler::new(graph);
    let path = GraphTraversalDsl::parse("users.posts.comments").unwrap();
    let compiled = compiler.compile(&path).unwrap();
    assert!(compiled.sql.contains("LEFT JOIN posts"));
    assert!(compiled.sql.contains("LEFT JOIN comments"));
    assert_eq!(compiled.join_count, 2);
}

#[test]
fn wiring_compiler_to_executor() {
    let graph = build_schema();
    let executor = GraphTraversalExecutor::new(graph);
    let result = executor.traverse("users.posts").unwrap();
    assert_eq!(result.join_count, 1);
    assert!(result.uses_single_join);
}

#[test]
fn wiring_executor_to_hydrator() {
    let hydrator = GraphResultHydrator::new(vec!["users".into(), "posts".into()]);
    let mut row = HashMap::new();
    row.insert("id".into(), "1".into());
    row.insert("name".into(), "Alice".into());
    row.insert("t1_title".into(), "Hello".into());
    let nodes = hydrator.hydrate(&[row]);
    assert_eq!(nodes.len(), 1);
    assert!(nodes[0].children.contains_key("posts"));
}

#[test]
fn wiring_path_cache() {
    let mut cache = TraversalPathCache::new();
    let path = "users.posts";
    let sql = "SELECT t0.*, t1.* FROM users LEFT JOIN posts AS t1 ON users.id = t1.user_id";

    assert!(cache.get(path).is_none());
    cache.insert(path.to_string(), sql.to_string());
    assert_eq!(cache.get(path).unwrap(), sql);
}

#[test]
fn wiring_filter_in_path() {
    let graph = build_schema();
    let compiler = GraphTraversalCompiler::new(graph);
    let path = GraphTraversalDsl::parse_with_filters("users.posts[active=true]").unwrap();
    let compiled = compiler.compile(&path).unwrap();
    assert!(compiled.sql.contains("active=true"));
}

#[test]
fn wiring_count_query() {
    let graph = build_schema();
    let compiler = GraphTraversalCompiler::new(graph);
    let path = GraphTraversalDsl::parse("users.posts").unwrap();
    let compiled = compiler.compile_count(&path).unwrap();
    assert!(compiled.sql.contains("COUNT(*)"));
}

#[test]
fn wiring_cycle_rejection() {
    let mut graph = RelationGraph::new();
    graph.add_edge(RelationEdge::new("a", "id", "b", "a_id", "b"));
    graph.add_edge(RelationEdge::new("b", "id", "a", "b_id", "a"));
    assert!(graph.has_cycle("a"));
}

#[test]
fn wiring_deep_traversal_3_levels() {
    let graph = build_schema();
    let compiler = GraphTraversalCompiler::new(graph);
    let path = GraphTraversalDsl::parse("users.posts.comments").unwrap();
    let compiled = compiler.compile(&path).unwrap();
    assert_eq!(compiled.tables.len(), 3);
    assert_eq!(compiled.join_count, 2);
}
