//! 社区发现（Community Detection）
//!
//! 提供图社区发现算法，将图划分为紧密连接的子图。

use std::collections::{HashMap, HashSet};

use crate::algorithm::{DirectedGraph, NodeId};

/// 社区
#[derive(Debug, Clone, serde::Serialize)]
pub struct Community {
    pub id: usize,
    pub nodes: Vec<NodeId>,
}

impl Community {
    pub fn new(id: usize, nodes: Vec<NodeId>) -> Self {
        Self { id, nodes }
    }

    pub fn size(&self) -> usize {
        self.nodes.len()
    }

    pub fn contains(&self, node: NodeId) -> bool {
        self.nodes.contains(&node)
    }
}

/// 社区发现结果
#[derive(Debug, Clone, serde::Serialize)]
pub struct CommunityDetectionResult {
    pub communities: Vec<Community>,
    pub modularity: f64,
}

impl CommunityDetectionResult {
    pub fn community_count(&self) -> usize {
        self.communities.len()
    }

    pub fn largest_community_size(&self) -> usize {
        self.communities.iter().map(|c| c.size()).max().unwrap_or(0)
    }

    pub fn node_community_map(&self) -> HashMap<NodeId, usize> {
        let mut map = HashMap::new();
        for community in &self.communities {
            for &node in &community.nodes {
                map.insert(node, community.id);
            }
        }
        map
    }
}

/// 基于标签传播的社区发现
pub struct LabelPropagation;

impl LabelPropagation {
    /// 执行标签传播算法
    ///
    /// 每个节点初始化为唯一标签，迭代传播直到收敛。
    pub fn detect(graph: &DirectedGraph, max_iterations: usize) -> CommunityDetectionResult {
        let nodes: Vec<NodeId> = graph.nodes().collect();
        if nodes.is_empty() {
            return CommunityDetectionResult {
                communities: Vec::new(),
                modularity: 0.0,
            };
        }
        let mut labels: HashMap<NodeId, usize> = HashMap::new();
        for (i, &node) in nodes.iter().enumerate() {
            labels.insert(node, i);
        }
        for _ in 0..max_iterations {
            let mut changed = false;
            for &node in &nodes {
                let neighbor_labels = Self::collect_neighbor_labels(graph, node, &labels);
                if let Some(new_label) = Self::majority_label(&neighbor_labels) {
                    if labels[&node] != new_label {
                        labels.insert(node, new_label);
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        let mut communities_map: HashMap<usize, Vec<NodeId>> = HashMap::new();
        for &node in &nodes {
            let label = labels[&node];
            communities_map.entry(label).or_default().push(node);
        }
        let communities: Vec<Community> = communities_map
            .into_iter()
            .enumerate()
            .map(|(i, (_, nodes))| Community::new(i, nodes))
            .collect();
        let modularity = Self::compute_modularity(graph, &communities);
        CommunityDetectionResult {
            communities,
            modularity,
        }
    }

    fn collect_neighbor_labels(
        graph: &DirectedGraph,
        node: NodeId,
        labels: &HashMap<NodeId, usize>,
    ) -> Vec<usize> {
        let mut result = Vec::new();
        if let Some(neighbors) = graph.neighbors(node) {
            for &(neighbor, _) in neighbors {
                if let Some(&label) = labels.get(&neighbor) {
                    result.push(label);
                }
            }
        }
        result
    }

    fn majority_label(labels: &[usize]) -> Option<usize> {
        if labels.is_empty() {
            return None;
        }
        let mut counts: HashMap<usize, usize> = HashMap::new();
        for &label in labels {
            *counts.entry(label).or_insert(0) += 1;
        }
        counts
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(label, _)| label)
    }

    fn compute_modularity(graph: &DirectedGraph, communities: &[Community]) -> f64 {
        let total_edges = graph.edge_count() as f64;
        if total_edges == 0.0 {
            return 0.0;
        }
        let node_community: HashMap<NodeId, usize> = {
            let mut map = HashMap::new();
            for community in communities {
                for &node in &community.nodes {
                    map.insert(node, community.id);
                }
            }
            map
        };
        let mut q = 0.0;
        for community in communities {
            let community_nodes: HashSet<NodeId> = community.nodes.iter().copied().collect();
            let mut internal_edges = 0;
            let mut degree_sum = 0;
            for &node in &community.nodes {
                if let Some(neighbors) = graph.neighbors(node) {
                    for &(neighbor, _) in neighbors {
                        if community_nodes.contains(&neighbor) {
                            internal_edges += 1;
                        }
                        degree_sum += 1;
                    }
                }
            }
            q += internal_edges as f64 / total_edges
                - (degree_sum as f64 / (2.0 * total_edges)).powi(2);
        }
        q
    }
}

/// 基于连通分量的社区发现（简单方法）
pub struct ConnectedComponentDetector;

impl ConnectedComponentDetector {
    /// 将每个连通分量作为一个社区
    pub fn detect(graph: &DirectedGraph) -> CommunityDetectionResult {
        let components = graph.connected_components();
        let communities: Vec<Community> = components
            .into_iter()
            .enumerate()
            .map(|(i, nodes)| Community::new(i, nodes))
            .collect();
        let modularity = LabelPropagation::compute_modularity(graph, &communities);
        CommunityDetectionResult {
            communities,
            modularity,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_community_new() {
        let c = Community::new(0, vec![1, 2, 3]);
        assert_eq!(c.size(), 3);
        assert!(c.contains(2));
        assert!(!c.contains(4));
    }

    #[test]
    fn test_label_propagation_empty() {
        let g = DirectedGraph::new();
        let result = LabelPropagation::detect(&g, 10);
        assert_eq!(result.community_count(), 0);
    }

    #[test]
    fn test_label_propagation_single_node() {
        let mut g = DirectedGraph::new();
        g.add_node(1);
        let result = LabelPropagation::detect(&g, 10);
        assert_eq!(result.community_count(), 1);
    }

    #[test]
    fn test_label_propagation_two_components() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(2, 1);
        g.add_edge_unweighted(3, 4);
        g.add_edge_unweighted(4, 3);
        let result = LabelPropagation::detect(&g, 100);
        assert!(result.community_count() >= 1);
    }

    #[test]
    fn test_label_propagation_connected() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(2, 3);
        g.add_edge_unweighted(3, 1);
        let result = LabelPropagation::detect(&g, 100);
        assert!(result.community_count() >= 1);
    }

    #[test]
    fn test_connected_component_detector() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(3, 4);
        let result = ConnectedComponentDetector::detect(&g);
        assert_eq!(result.community_count(), 2);
    }

    #[test]
    fn test_connected_component_detector_single() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(2, 3);
        let result = ConnectedComponentDetector::detect(&g);
        assert_eq!(result.community_count(), 1);
    }

    #[test]
    fn test_result_node_community_map() {
        let communities = vec![Community::new(0, vec![1, 2]), Community::new(1, vec![3, 4])];
        let result = CommunityDetectionResult {
            communities,
            modularity: 0.5,
        };
        let map = result.node_community_map();
        assert_eq!(map[&1], 0);
        assert_eq!(map[&3], 1);
    }

    #[test]
    fn test_result_largest_community() {
        let communities = vec![
            Community::new(0, vec![1, 2]),
            Community::new(1, vec![3, 4, 5]),
        ];
        let result = CommunityDetectionResult {
            communities,
            modularity: 0.0,
        };
        assert_eq!(result.largest_community_size(), 3);
    }

    #[test]
    fn test_label_propagation_modularity() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(2, 1);
        let result = LabelPropagation::detect(&g, 10);
        assert!(result.modularity >= 0.0);
    }
}
