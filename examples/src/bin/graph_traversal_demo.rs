//! 图遍历 API 生产入口 demo
//!
//! 展示点分路径 → JOIN SQL 编译 → 嵌套结果水合全链路。

use std::collections::HashMap;

use sz_orm_graph::{
    GraphResultHydrator, GraphTraversalCompiler, GraphTraversalDsl, GraphTraversalExecutor,
    RelationEdge, RelationGraph, TraversalPathCache,
};

fn main() {
    println!("=== sz-orm-graph 图遍历 API demo ===\n");

    let mut graph_for_compiler = RelationGraph::new();
    graph_for_compiler.add_edge(RelationEdge::new(
        "users", "id", "posts", "user_id", "posts",
    ));
    graph_for_compiler.add_edge(RelationEdge::new(
        "posts", "id", "comments", "post_id", "comments",
    ));
    graph_for_compiler.add_edge(RelationEdge::new(
        "users", "id", "profiles", "user_id", "profiles",
    ));

    let compiler = GraphTraversalCompiler::new(graph_for_compiler);

    let path = GraphTraversalDsl::parse("users.posts.comments").unwrap();
    let compiled = compiler.compile(&path).unwrap();
    println!("路径: users.posts.comments");
    println!("编译 SQL: {}", compiled.sql);
    println!("JOIN 数: {}", compiled.join_count);
    println!();

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

    let executor = GraphTraversalExecutor::new(graph);
    let result = executor.traverse("users.posts").unwrap();
    println!(
        "执行结果: {} JOIN, uses_single_join={}",
        result.join_count, result.uses_single_join
    );

    let advice = executor.n_plus_one_advice(&GraphTraversalDsl::parse("users.posts").unwrap());
    if let Some(msg) = advice {
        println!("N+1 防护: {}", msg);
    }

    let mut cache = TraversalPathCache::new();
    cache.insert("users.posts".to_string(), compiled.sql.clone());
    println!(
        "\n缓存命中: {:?}",
        cache.get("users.posts").map(|s| s.len())
    );

    let hydrator = GraphResultHydrator::new(vec!["users".into(), "posts".into()]);
    let mut row = HashMap::new();
    row.insert("id".into(), "1".into());
    row.insert("name".into(), "Alice".into());
    row.insert("t1_title".into(), "Hello World".into());
    let nodes = hydrator.hydrate(&[row]);
    println!("\n水合结果: {} 个根节点", nodes.len());
    println!("JSON: {}", hydrator.to_json(&nodes));

    println!("\n=== demo 完成 ===");
}
