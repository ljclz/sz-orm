//! 血缘影响分析（v4.3.0 M3-T2，`lineage-viz` feature）
//!
//! 在既有 [`LineageGraph::impact_analysis`]（无深度限制 BFS）基础上，
//! 提供**深度受限**的下游影响分析与上游溯源，用于迁移变更前评估影响范围：
//!
//! - [`downstream_impact`]：删除/修改某节点后受影响的下游链路（BFS，depth 限制）
//! - [`upstream_trace`]：某节点数据的来源链路（反向 BFS，depth 限制）
//!
//! 与迁移 dry-run（`sz-orm-core` 的 `migration_dry_run.rs`）配合：
//! 执行 DROP/ALTER 前先计算下游影响，输出受影响表/字段清单。

use crate::lineage::graph::{LineageEdge, LineageGraph, LineageNodeId};
use std::collections::{HashSet, VecDeque};

/// 影响链路的一条边（含经过的依赖类型描述）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImpactEdge {
    /// 来源节点（`table.column`）
    pub from: String,
    /// 目标节点（`table.column`）
    pub to: String,
    /// 依赖类型（DirectDependency/Derived/Join/Filter/Projection）
    pub via: String,
}

impl ImpactEdge {
    fn new(from: &LineageNodeId, to: &LineageNodeId, via: &str) -> Self {
        Self {
            from: format!("{}.{}", from.table, from.column),
            to: format!("{}.{}", to.table, to.column),
            via: via.to_string(),
        }
    }
}

/// 下游影响分析：从 `node` 出发沿出边 BFS，深度不超过 `max_depth`（0 = 不限）。
///
/// 返回从起点出发的每条影响边（含起点直连边），可用于"删除此字段会影响谁"。
pub fn downstream_impact(
    graph: &LineageGraph,
    node: &LineageNodeId,
    max_depth: usize,
) -> Vec<ImpactEdge> {
    bfs_edges(graph, node, max_depth, true)
}

/// 上游溯源分析：从 `node` 出发沿入边反向 BFS，深度不超过 `max_depth`（0 = 不限）。
///
/// 返回指向起点的每条来源边（含直连边），可用于"此字段的数据从哪来"。
pub fn upstream_trace(
    graph: &LineageGraph,
    node: &LineageNodeId,
    max_depth: usize,
) -> Vec<ImpactEdge> {
    bfs_edges(graph, node, max_depth, false)
}

/// 通用 BFS 边遍历（downstream = 沿出边；否则沿入边），带深度限制
fn bfs_edges(
    graph: &LineageGraph,
    node: &LineageNodeId,
    max_depth: usize,
    downstream: bool,
) -> Vec<ImpactEdge> {
    let mut result = Vec::new();
    let mut visited: HashSet<LineageNodeId> = HashSet::new();
    let mut queue: VecDeque<(LineageNodeId, usize)> = VecDeque::new();
    queue.push_back((node.clone(), 0));
    visited.insert(node.clone());

    while let Some((current, depth)) = queue.pop_front() {
        // max_depth == 0 表示不限深度
        if max_depth > 0 && depth >= max_depth {
            continue;
        }
        let edges: Vec<&LineageEdge> = if downstream {
            graph.outgoing_edges(&current)
        } else {
            graph.incoming_edges(&current)
        };
        for edge in edges {
            let next = if downstream {
                edge.target.clone()
            } else {
                edge.source.clone()
            };
            result.push(ImpactEdge::new(
                &edge.source,
                &edge.target,
                edge_type_str(&edge.edge_type),
            ));
            if visited.insert(next.clone()) {
                queue.push_back((next, depth + 1));
            }
        }
    }
    result
}

fn edge_type_str(edge_type: &crate::lineage::graph::EdgeType) -> &'static str {
    match edge_type {
        crate::lineage::graph::EdgeType::DirectDependency => "DirectDependency",
        crate::lineage::graph::EdgeType::Derived => "Derived",
        crate::lineage::graph::EdgeType::Join => "Join",
        crate::lineage::graph::EdgeType::Filter => "Filter",
        crate::lineage::graph::EdgeType::Projection => "Projection",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lineage::graph::{EdgeType, LineageNode, NodeType};
    use crate::lineage::LineageGraph;

    fn node(table: &str, column: &str) -> LineageNodeId {
        LineageNodeId::new(table, column)
    }

    /// 构造链路：users.id → orders.user_id → orders.total → report.amount
    fn build_chain() -> LineageGraph {
        let mut g = LineageGraph::new();
        for (t, c) in [
            ("users", "id"),
            ("orders", "user_id"),
            ("orders", "total"),
            ("report", "amount"),
        ] {
            g.add_node(LineageNode::new(node(t, c), NodeType::Column));
        }
        g.add_edge(LineageEdge::new(
            node("users", "id"),
            node("orders", "user_id"),
            EdgeType::Join,
        ))
        .unwrap();
        g.add_edge(LineageEdge::new(
            node("orders", "user_id"),
            node("orders", "total"),
            EdgeType::Derived,
        ))
        .unwrap();
        g.add_edge(LineageEdge::new(
            node("orders", "total"),
            node("report", "amount"),
            EdgeType::Derived,
        ))
        .unwrap();
        g
    }

    #[test]
    fn downstream_full_depth() {
        let g = build_chain();
        let edges = downstream_impact(&g, &node("users", "id"), 0);
        assert_eq!(edges.len(), 3);
        assert!(edges.iter().any(|e| e.to == "orders.user_id"));
        assert!(edges.iter().any(|e| e.to == "orders.total"));
        assert!(edges.iter().any(|e| e.to == "report.amount"));
    }

    #[test]
    fn downstream_depth_limited() {
        let g = build_chain();
        // depth=1 只返回直连下游
        let edges = downstream_impact(&g, &node("users", "id"), 1);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].to, "orders.user_id");
    }

    #[test]
    fn upstream_trace_full() {
        let g = build_chain();
        let edges = upstream_trace(&g, &node("report", "amount"), 0);
        assert_eq!(edges.len(), 3);
        assert!(edges.iter().any(|e| e.from == "users.id"));
        assert!(edges.iter().any(|e| e.from == "orders.user_id"));
        assert!(edges.iter().any(|e| e.from == "orders.total"));
    }

    #[test]
    fn upstream_depth_limited() {
        let g = build_chain();
        let edges = upstream_trace(&g, &node("report", "amount"), 2);
        assert_eq!(edges.len(), 2); // orders.total + orders.user_id（depth 2 内）
        assert!(edges.iter().all(|e| e.from != "users.id"));
    }

    #[test]
    fn impact_edge_contains_dependency_type() {
        let g = build_chain();
        let edges = downstream_impact(&g, &node("users", "id"), 1);
        assert_eq!(edges[0].via, "Join");
    }

    #[test]
    fn unknown_node_returns_empty() {
        let g = build_chain();
        let edges = downstream_impact(&g, &node("nonexistent", "x"), 0);
        assert!(edges.is_empty());
    }
}
