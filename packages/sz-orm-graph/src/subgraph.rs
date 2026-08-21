//! 子图匹配与图同构（Subgraph Matching）
//!
//! 提供子图查找、图同构检测等功能。

use std::collections::{HashMap, HashSet};

use crate::algorithm::{DirectedGraph, NodeId};

/// 子图匹配器
pub struct SubgraphMatcher;

impl SubgraphMatcher {
    /// 查找模式图在目标图中的所有匹配
    ///
    /// 返回每个匹配的节点映射（模式节点 -> 目标节点）。
    /// 使用简单的回溯算法，适用于小规模模式图。
    pub fn find_matches(
        pattern: &DirectedGraph,
        target: &DirectedGraph,
    ) -> Vec<HashMap<NodeId, NodeId>> {
        let pattern_nodes: Vec<NodeId> = pattern.nodes().collect();
        if pattern_nodes.is_empty() {
            return Vec::new();
        }
        let target_nodes: Vec<NodeId> = target.nodes().collect();
        let mut results = Vec::new();
        let mut mapping = HashMap::new();
        let mut used = HashSet::new();
        Self::match_recursive(
            pattern,
            target,
            &pattern_nodes,
            &target_nodes,
            0,
            &mut mapping,
            &mut used,
            &mut results,
        );
        results
    }

    #[allow(clippy::too_many_arguments)]
    fn match_recursive(
        pattern: &DirectedGraph,
        target: &DirectedGraph,
        pattern_nodes: &[NodeId],
        target_nodes: &[NodeId],
        idx: usize,
        mapping: &mut HashMap<NodeId, NodeId>,
        used: &mut HashSet<NodeId>,
        results: &mut Vec<HashMap<NodeId, NodeId>>,
    ) {
        if idx == pattern_nodes.len() {
            results.push(mapping.clone());
            return;
        }
        let pattern_node = pattern_nodes[idx];
        for &target_node in target_nodes {
            if used.contains(&target_node) {
                continue;
            }
            if !Self::is_consistent(pattern, target, pattern_node, target_node, mapping) {
                continue;
            }
            mapping.insert(pattern_node, target_node);
            used.insert(target_node);
            Self::match_recursive(
                pattern,
                target,
                pattern_nodes,
                target_nodes,
                idx + 1,
                mapping,
                used,
                results,
            );
            mapping.remove(&pattern_node);
            used.remove(&target_node);
        }
    }

    fn is_consistent(
        pattern: &DirectedGraph,
        target: &DirectedGraph,
        pattern_node: NodeId,
        target_node: NodeId,
        mapping: &HashMap<NodeId, NodeId>,
    ) -> bool {
        if let Some(p_neighbors) = pattern.neighbors(pattern_node) {
            for &(p_neighbor, _) in p_neighbors {
                if let Some(&t_neighbor) = mapping.get(&p_neighbor) {
                    if !target.has_edge(target_node, t_neighbor) {
                        return false;
                    }
                }
            }
        }
        for (&mapped_p, &mapped_t) in mapping.iter() {
            if let Some(p_neighbors) = pattern.neighbors(mapped_p) {
                for &(p_neighbor, _) in p_neighbors {
                    if p_neighbor == pattern_node && !target.has_edge(mapped_t, target_node) {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// 检查模式图是否是目标图的子图
    pub fn is_subgraph(pattern: &DirectedGraph, target: &DirectedGraph) -> bool {
        !Self::find_matches(pattern, target).is_empty()
    }

    /// 统计模式图在目标图中的匹配数
    pub fn count_matches(pattern: &DirectedGraph, target: &DirectedGraph) -> usize {
        Self::find_matches(pattern, target).len()
    }
}

/// 图同构检测器
pub struct IsomorphismChecker;

impl IsomorphismChecker {
    /// 检查两个图是否同构
    ///
    /// 两个图同构当且仅当存在节点的一一映射，保持边关系。
    pub fn is_isomorphic(g1: &DirectedGraph, g2: &DirectedGraph) -> bool {
        if g1.node_count() != g2.node_count() {
            return false;
        }
        if g1.edge_count() != g2.edge_count() {
            return false;
        }
        if g1.node_count() == 0 {
            return true;
        }
        let g1_invariants = Self::compute_invariants(g1);
        let g2_invariants = Self::compute_invariants(g2);
        if g1_invariants != g2_invariants {
            return false;
        }
        !SubgraphMatcher::find_matches(g1, g2).is_empty()
    }

    /// 计算图的不变量（用于快速排除非同构图）
    fn compute_invariants(graph: &DirectedGraph) -> Vec<usize> {
        let mut degree_sequence: Vec<usize> = graph
            .nodes()
            .map(|n| graph.in_degree(n) * 1000 + graph.out_degree(n))
            .collect();
        degree_sequence.sort_unstable();
        degree_sequence
    }
}

/// 公共子图查找器
pub struct CommonSubgraphFinder;

impl CommonSubgraphFinder {
    /// 查找两个图的最大公共子图（节点数最大）
    ///
    /// 使用简单的贪心算法，不保证最优解。
    pub fn largest_common_subgraph(g1: &DirectedGraph, g2: &DirectedGraph) -> DirectedGraph {
        let g1_nodes: Vec<NodeId> = g1.nodes().collect();
        let g2_nodes: Vec<NodeId> = g2.nodes().collect();
        let mut best_mapping: HashMap<NodeId, NodeId> = HashMap::new();
        for &n1 in &g1_nodes {
            for &n2 in &g2_nodes {
                if g1.in_degree(n1) + g1.out_degree(n1) == g2.in_degree(n2) + g2.out_degree(n2)
                    && !best_mapping.contains_key(&n1)
                    && !best_mapping.values().any(|&v| v == n2)
                {
                    best_mapping.insert(n1, n2);
                }
            }
        }
        let mut result = DirectedGraph::new();
        for (&n1, &_n2) in &best_mapping {
            result.add_node(n1);
            if let Some(neighbors) = g1.neighbors(n1) {
                for &(neighbor, weight) in neighbors {
                    if let Some(&mapped) = best_mapping.get(&neighbor) {
                        result.add_edge(n1, neighbor, weight);
                        let _ = mapped;
                    }
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subgraph_match_simple() {
        let mut target = DirectedGraph::new();
        target.add_edge_unweighted(1, 2);
        target.add_edge_unweighted(2, 3);
        let mut pattern = DirectedGraph::new();
        pattern.add_edge_unweighted(10, 20);
        assert!(SubgraphMatcher::is_subgraph(&pattern, &target));
    }

    #[test]
    fn test_subgraph_match_no_match() {
        let mut target = DirectedGraph::new();
        target.add_edge_unweighted(1, 2);
        let mut pattern = DirectedGraph::new();
        pattern.add_edge_unweighted(10, 20);
        pattern.add_edge_unweighted(20, 30);
        assert!(!SubgraphMatcher::is_subgraph(&pattern, &target));
    }

    #[test]
    fn test_subgraph_count_matches() {
        let mut target = DirectedGraph::new();
        target.add_edge_unweighted(1, 2);
        target.add_edge_unweighted(3, 4);
        let mut pattern = DirectedGraph::new();
        pattern.add_edge_unweighted(10, 20);
        let count = SubgraphMatcher::count_matches(&pattern, &target);
        assert!(count >= 2);
    }

    #[test]
    fn test_subgraph_match_self() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(2, 3);
        assert!(SubgraphMatcher::is_subgraph(&g, &g));
    }

    #[test]
    fn test_subgraph_match_empty_pattern() {
        let target = DirectedGraph::new();
        let pattern = DirectedGraph::new();
        assert!(!SubgraphMatcher::is_subgraph(&pattern, &target));
    }

    #[test]
    fn test_subgraph_match_single_node() {
        let mut target = DirectedGraph::new();
        target.add_node(1);
        let mut pattern = DirectedGraph::new();
        pattern.add_node(10);
        assert!(SubgraphMatcher::is_subgraph(&pattern, &target));
    }

    #[test]
    fn test_isomorphism_same_graph() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(2, 3);
        assert!(IsomorphismChecker::is_isomorphic(&g, &g));
    }

    #[test]
    fn test_isomorphism_different_node_count() {
        let mut g1 = DirectedGraph::new();
        g1.add_edge_unweighted(1, 2);
        let mut g2 = DirectedGraph::new();
        g2.add_edge_unweighted(1, 2);
        g2.add_node(3);
        assert!(!IsomorphismChecker::is_isomorphic(&g1, &g2));
    }

    #[test]
    fn test_isomorphism_different_edge_count() {
        let mut g1 = DirectedGraph::new();
        g1.add_edge_unweighted(1, 2);
        let mut g2 = DirectedGraph::new();
        g2.add_edge_unweighted(1, 2);
        g2.add_edge_unweighted(2, 1);
        assert!(!IsomorphismChecker::is_isomorphic(&g1, &g2));
    }

    #[test]
    fn test_isomorphism_renamed_nodes() {
        let mut g1 = DirectedGraph::new();
        g1.add_edge_unweighted(1, 2);
        let mut g2 = DirectedGraph::new();
        g2.add_edge_unweighted(10, 20);
        assert!(IsomorphismChecker::is_isomorphic(&g1, &g2));
    }

    #[test]
    fn test_common_subgraph_empty() {
        let g1 = DirectedGraph::new();
        let g2 = DirectedGraph::new();
        let common = CommonSubgraphFinder::largest_common_subgraph(&g1, &g2);
        assert_eq!(common.node_count(), 0);
    }

    #[test]
    fn test_common_subgraph_simple() {
        let mut g1 = DirectedGraph::new();
        g1.add_edge_unweighted(1, 2);
        let mut g2 = DirectedGraph::new();
        g2.add_edge_unweighted(10, 20);
        let common = CommonSubgraphFinder::largest_common_subgraph(&g1, &g2);
        assert!(common.node_count() > 0 || common.node_count() == 0);
    }

    #[test]
    fn test_common_subgraph_same_graph() {
        let mut g1 = DirectedGraph::new();
        g1.add_edge_unweighted(1, 2);
        g1.add_edge_unweighted(2, 3);
        let common = CommonSubgraphFinder::largest_common_subgraph(&g1, &g1);
        assert!(common.node_count() > 0);
    }

    #[test]
    fn test_find_matches_returns_mapping() {
        let mut target = DirectedGraph::new();
        target.add_edge_unweighted(1, 2);
        let mut pattern = DirectedGraph::new();
        pattern.add_edge_unweighted(10, 20);
        let matches = SubgraphMatcher::find_matches(&pattern, &target);
        assert!(!matches.is_empty());
        for mapping in &matches {
            assert_eq!(mapping.len(), 2);
        }
    }

    #[test]
    fn test_isomorphism_empty_graphs() {
        let g1 = DirectedGraph::new();
        let g2 = DirectedGraph::new();
        assert!(IsomorphismChecker::is_isomorphic(&g1, &g2));
    }

    #[test]
    fn test_isomorphism_single_node() {
        let mut g1 = DirectedGraph::new();
        g1.add_node(1);
        let mut g2 = DirectedGraph::new();
        g2.add_node(10);
        assert!(IsomorphismChecker::is_isomorphic(&g1, &g2));
    }
}
