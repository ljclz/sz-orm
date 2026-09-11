//! GRAPH-QUERY-01 接线验证测试（v6.8.0）
//!
//! 验证多跳路径查询 → 路径结果 + 超时截断 端到端管线。

use std::time::Duration;
use sz_orm_graph::joint_projection::{GraphEdge, GraphNode, JointProjection};

fn make_social_graph() -> JointProjection {
    let mut graph = JointProjection::new(10, Duration::from_secs(1));
    for id in ["Alice", "Bob", "Charlie", "David", "Eve"] {
        graph.add_node(GraphNode {
            id: id.to_string(),
            label: "Person".to_string(),
        });
    }
    graph.add_node(GraphNode {
        id: "TechCorp".to_string(),
        label: "Company".to_string(),
    });
    graph.add_edge(GraphEdge {
        from: "Alice".to_string(),
        to: "Bob".to_string(),
        relation: "KNOWS".to_string(),
    });
    graph.add_edge(GraphEdge {
        from: "Bob".to_string(),
        to: "Charlie".to_string(),
        relation: "KNOWS".to_string(),
    });
    graph.add_edge(GraphEdge {
        from: "Charlie".to_string(),
        to: "David".to_string(),
        relation: "KNOWS".to_string(),
    });
    graph.add_edge(GraphEdge {
        from: "Alice".to_string(),
        to: "Eve".to_string(),
        relation: "KNOWS".to_string(),
    });
    graph.add_edge(GraphEdge {
        from: "Alice".to_string(),
        to: "TechCorp".to_string(),
        relation: "WORKS_AT".to_string(),
    });
    graph.add_edge(GraphEdge {
        from: "Bob".to_string(),
        to: "TechCorp".to_string(),
        relation: "WORKS_AT".to_string(),
    });
    graph
}

#[test]
fn wiring_two_hop_path_returns_nodes_and_edges() {
    let graph = make_social_graph();
    let result = graph.path_query("Alice", 2);
    assert!(!result.truncated);
    assert!(result.nodes.iter().any(|n| n.id == "Alice"));
    assert!(result.nodes.iter().any(|n| n.id == "Bob"));
    assert!(result.nodes.iter().any(|n| n.id == "Charlie"));
    assert!(!result.edges.is_empty());
}

#[test]
fn wiring_one_hop_returns_direct_neighbors() {
    let graph = make_social_graph();
    let result = graph.path_query("Alice", 1);
    let neighbor_ids: Vec<&str> = result.nodes.iter().map(|n| n.id.as_str()).collect();
    assert!(neighbor_ids.contains(&"Bob"));
    assert!(neighbor_ids.contains(&"Eve"));
    assert!(neighbor_ids.contains(&"TechCorp"));
}

#[test]
fn wiring_max_depth_truncates_with_flag() {
    let mut graph = JointProjection::new(2, Duration::from_secs(1));
    for id in ["A", "B", "C", "D", "E"] {
        graph.add_node(GraphNode {
            id: id.to_string(),
            label: "X".to_string(),
        });
    }
    graph.add_edge(GraphEdge {
        from: "A".to_string(),
        to: "B".to_string(),
        relation: "R".to_string(),
    });
    graph.add_edge(GraphEdge {
        from: "B".to_string(),
        to: "C".to_string(),
        relation: "R".to_string(),
    });
    graph.add_edge(GraphEdge {
        from: "C".to_string(),
        to: "D".to_string(),
        relation: "R".to_string(),
    });
    graph.add_edge(GraphEdge {
        from: "D".to_string(),
        to: "E".to_string(),
        relation: "R".to_string(),
    });
    let result = graph.path_query("A", 5);
    assert!(result.truncated);
}

#[test]
fn wiring_subgraph_match_finds_pattern() {
    let graph = make_social_graph();
    let pattern_nodes = vec![
        GraphNode {
            id: "p1".to_string(),
            label: "Person".to_string(),
        },
        GraphNode {
            id: "p2".to_string(),
            label: "Person".to_string(),
        },
    ];
    let pattern_edges = vec![GraphEdge {
        from: "p1".to_string(),
        to: "p2".to_string(),
        relation: "KNOWS".to_string(),
    }];
    let result = graph.subgraph_match(&pattern_nodes, &pattern_edges);
    assert!(!result.matches.is_empty());
}

#[test]
fn wiring_path_query_nonexistent_start() {
    let graph = make_social_graph();
    let result = graph.path_query("Nonexistent", 3);
    assert!(result.nodes.is_empty());
    assert!(!result.truncated);
}

#[test]
fn wiring_edges_have_correct_relations() {
    let graph = make_social_graph();
    let result = graph.path_query("Alice", 1);
    assert!(result.edges.iter().any(|e| e.relation == "KNOWS"));
    assert!(result.edges.iter().any(|e| e.relation == "WORKS_AT"));
}

#[test]
fn wiring_depth_reached_correct() {
    let graph = make_social_graph();
    let result = graph.path_query("Alice", 3);
    assert_eq!(result.depth_reached, 3);
}

#[test]
fn wiring_wildcard_label_matches_all() {
    let graph = make_social_graph();
    let pattern = vec![GraphNode {
        id: "n".to_string(),
        label: "*".to_string(),
    }];
    let result = graph.subgraph_match(&pattern, &[]);
    assert_eq!(result.matches.len(), 6);
}

#[test]
fn wiring_company_label_match() {
    let graph = make_social_graph();
    let pattern = vec![GraphNode {
        id: "c".to_string(),
        label: "Company".to_string(),
    }];
    let result = graph.subgraph_match(&pattern, &[]);
    assert_eq!(result.matches.len(), 1);
}

#[test]
fn wiring_graph_counts() {
    let graph = make_social_graph();
    assert_eq!(graph.node_count(), 6);
    assert_eq!(graph.edge_count(), 6);
}

#[test]
fn wiring_zero_hops_returns_start_only() {
    let graph = make_social_graph();
    let result = graph.path_query("Alice", 0);
    assert!(!result.truncated);
    assert_eq!(result.depth_reached, 0);
}
