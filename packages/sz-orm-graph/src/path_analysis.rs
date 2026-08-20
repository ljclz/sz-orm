//! 图路径分析（Path Analysis）
//!
//! 提供路径查找、路径枚举、可达性分析等功能。

use std::collections::{HashMap, HashSet, VecDeque};

use crate::algorithm::{DirectedGraph, NodeId};

/// 路径分析器
pub struct PathAnalyzer;

impl PathAnalyzer {
    /// 检查从 `from` 到 `to` 是否可达
    pub fn is_reachable(graph: &DirectedGraph, from: NodeId, to: NodeId) -> bool {
        if from == to {
            return true;
        }
        if !graph.has_node(from) || !graph.has_node(to) {
            return false;
        }
        let bfs = graph.bfs(from);
        bfs.contains(&to)
    }

    /// 查找从 `from` 到 `to` 的所有简单路径（限制最大长度）
    ///
    /// 简单路径：不重复访问节点。
    /// `max_depth` 限制路径最大长度，防止指数爆炸。
    pub fn find_all_paths(
        graph: &DirectedGraph,
        from: NodeId,
        to: NodeId,
        max_depth: usize,
    ) -> Vec<Vec<NodeId>> {
        if !graph.has_node(from) || !graph.has_node(to) {
            return Vec::new();
        }
        let mut results = Vec::new();
        let mut current_path = vec![from];
        let mut visited = HashSet::new();
        visited.insert(from);
        Self::find_paths_dfs(
            graph,
            from,
            to,
            max_depth,
            &mut current_path,
            &mut visited,
            &mut results,
        );
        results
    }

    fn find_paths_dfs(
        graph: &DirectedGraph,
        current: NodeId,
        target: NodeId,
        max_depth: usize,
        current_path: &mut Vec<NodeId>,
        visited: &mut HashSet<NodeId>,
        results: &mut Vec<Vec<NodeId>>,
    ) {
        if current == target {
            results.push(current_path.clone());
            return;
        }
        if current_path.len() >= max_depth {
            return;
        }
        if let Some(neighbors) = graph.neighbors(current) {
            for &(neighbor, _) in neighbors {
                if visited.insert(neighbor) {
                    current_path.push(neighbor);
                    Self::find_paths_dfs(
                        graph,
                        neighbor,
                        target,
                        max_depth,
                        current_path,
                        visited,
                        results,
                    );
                    current_path.pop();
                    visited.remove(&neighbor);
                }
            }
        }
    }

    /// 计算从 `from` 到 `to` 的最短路径长度（BFS）
    pub fn shortest_path_length(graph: &DirectedGraph, from: NodeId, to: NodeId) -> Option<usize> {
        if from == to {
            return Some(0);
        }
        if !graph.has_node(from) || !graph.has_node(to) {
            return None;
        }
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        visited.insert(from);
        queue.push_back((from, 0usize));
        while let Some((node, dist)) = queue.pop_front() {
            if let Some(neighbors) = graph.neighbors(node) {
                for &(neighbor, _) in neighbors {
                    if neighbor == to {
                        return Some(dist + 1);
                    }
                    if visited.insert(neighbor) {
                        queue.push_back((neighbor, dist + 1));
                    }
                }
            }
        }
        None
    }

    /// 计算从 `start` 到所有可达节点的最短距离
    pub fn bfs_distances(graph: &DirectedGraph, start: NodeId) -> HashMap<NodeId, usize> {
        let mut distances = HashMap::new();
        if !graph.has_node(start) {
            return distances;
        }
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        distances.insert(start, 0);
        visited.insert(start);
        queue.push_back(start);
        while let Some(node) = queue.pop_front() {
            let dist = distances[&node];
            if let Some(neighbors) = graph.neighbors(node) {
                for &(neighbor, _) in neighbors {
                    if visited.insert(neighbor) {
                        distances.insert(neighbor, dist + 1);
                        queue.push_back(neighbor);
                    }
                }
            }
        }
        distances
    }

    /// 查找从 `start` 可达的所有节点
    pub fn reachable_nodes(graph: &DirectedGraph, start: NodeId) -> HashSet<NodeId> {
        graph.bfs(start).into_iter().collect()
    }

    /// 计算图的直径（最长最短路径）
    ///
    /// 对于无权图，直径是所有节点对之间最短路径的最大值。
    pub fn diameter(graph: &DirectedGraph) -> Option<usize> {
        let nodes: Vec<NodeId> = graph.nodes().collect();
        let mut max_dist = 0;
        let mut found = false;
        for &start in &nodes {
            let distances = Self::bfs_distances(graph, start);
            for &dist in distances.values() {
                if dist > max_dist {
                    max_dist = dist;
                    found = true;
                }
            }
        }
        if found {
            Some(max_dist)
        } else {
            None
        }
    }

    /// 计算节点偏心度（到最远可达节点的距离）
    pub fn eccentricity(graph: &DirectedGraph, node: NodeId) -> Option<usize> {
        let distances = Self::bfs_distances(graph, node);
        distances.values().copied().max()
    }

    /// 计算图的半径（最小偏心度）
    pub fn radius(graph: &DirectedGraph) -> Option<usize> {
        let nodes: Vec<NodeId> = graph.nodes().collect();
        let mut min_ecc = usize::MAX;
        for &node in &nodes {
            if let Some(ecc) = Self::eccentricity(graph, node) {
                if ecc < min_ecc {
                    min_ecc = ecc;
                }
            }
        }
        if min_ecc != usize::MAX {
            Some(min_ecc)
        } else {
            None
        }
    }
}

/// 可达性矩阵
pub struct ReachabilityMatrix {
    matrix: HashMap<(NodeId, NodeId), bool>,
    node_count: usize,
}

impl ReachabilityMatrix {
    /// 从图构建可达性矩阵
    pub fn from_graph(graph: &DirectedGraph) -> Self {
        let nodes: Vec<NodeId> = graph.nodes().collect();
        let mut matrix = HashMap::new();
        for &start in &nodes {
            let reachable = PathAnalyzer::reachable_nodes(graph, start);
            for &end in &nodes {
                matrix.insert((start, end), reachable.contains(&end));
            }
        }
        Self {
            matrix,
            node_count: nodes.len(),
        }
    }

    /// 检查 `from` 是否可达 `to`
    pub fn is_reachable(&self, from: NodeId, to: NodeId) -> bool {
        self.matrix.get(&(from, to)).copied().unwrap_or(false)
    }

    /// 节点数
    pub fn node_count(&self) -> usize {
        self.node_count
    }

    /// 可达对数
    pub fn reachable_pairs(&self) -> usize {
        self.matrix.values().filter(|&&v| v).count()
    }

    /// 总对数
    pub fn total_pairs(&self) -> usize {
        self.matrix.len()
    }

    /// 可达率
    pub fn reachability_rate(&self) -> f64 {
        let total = self.total_pairs();
        if total > 0 {
            self.reachable_pairs() as f64 / total as f64
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_reachable_same_node() {
        let mut g = DirectedGraph::new();
        g.add_node(1);
        assert!(PathAnalyzer::is_reachable(&g, 1, 1));
    }

    #[test]
    fn test_is_reachable_direct_edge() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        assert!(PathAnalyzer::is_reachable(&g, 1, 2));
    }

    #[test]
    fn test_is_reachable_transitive() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(2, 3);
        assert!(PathAnalyzer::is_reachable(&g, 1, 3));
    }

    #[test]
    fn test_is_reachable_unreachable() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_node(3);
        assert!(!PathAnalyzer::is_reachable(&g, 1, 3));
    }

    #[test]
    fn test_find_all_paths_single() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        let paths = PathAnalyzer::find_all_paths(&g, 1, 2, 10);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0], vec![1, 2]);
    }

    #[test]
    fn test_find_all_paths_multiple() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(2, 4);
        g.add_edge_unweighted(1, 3);
        g.add_edge_unweighted(3, 4);
        let paths = PathAnalyzer::find_all_paths(&g, 1, 4, 10);
        assert_eq!(paths.len(), 2);
    }

    #[test]
    fn test_find_all_paths_none() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_node(3);
        let paths = PathAnalyzer::find_all_paths(&g, 1, 3, 10);
        assert!(paths.is_empty());
    }

    #[test]
    fn test_shortest_path_length_direct() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        assert_eq!(PathAnalyzer::shortest_path_length(&g, 1, 2), Some(1));
    }

    #[test]
    fn test_shortest_path_length_transitive() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(2, 3);
        assert_eq!(PathAnalyzer::shortest_path_length(&g, 1, 3), Some(2));
    }

    #[test]
    fn test_shortest_path_length_same() {
        let mut g = DirectedGraph::new();
        g.add_node(1);
        assert_eq!(PathAnalyzer::shortest_path_length(&g, 1, 1), Some(0));
    }

    #[test]
    fn test_shortest_path_length_unreachable() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_node(3);
        assert_eq!(PathAnalyzer::shortest_path_length(&g, 1, 3), None);
    }

    #[test]
    fn test_bfs_distances() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(1, 3);
        g.add_edge_unweighted(2, 4);
        let dist = PathAnalyzer::bfs_distances(&g, 1);
        assert_eq!(dist[&1], 0);
        assert_eq!(dist[&2], 1);
        assert_eq!(dist[&3], 1);
        assert_eq!(dist[&4], 2);
    }

    #[test]
    fn test_reachable_nodes() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(2, 3);
        g.add_node(4);
        let reachable = PathAnalyzer::reachable_nodes(&g, 1);
        assert_eq!(reachable.len(), 3);
        assert!(!reachable.contains(&4));
    }

    #[test]
    fn test_diameter() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(2, 3);
        g.add_edge_unweighted(3, 4);
        assert_eq!(PathAnalyzer::diameter(&g), Some(3));
    }

    #[test]
    fn test_diameter_disconnected() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(3, 4);
        assert_eq!(PathAnalyzer::diameter(&g), Some(1));
    }

    #[test]
    fn test_eccentricity() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(2, 3);
        assert_eq!(PathAnalyzer::eccentricity(&g, 1), Some(2));
        assert_eq!(PathAnalyzer::eccentricity(&g, 2), Some(1));
    }

    #[test]
    fn test_radius() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(2, 3);
        let radius = PathAnalyzer::radius(&g).unwrap();
        assert!((0..=2).contains(&radius));
    }

    #[test]
    fn test_reachability_matrix() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(2, 3);
        let matrix = ReachabilityMatrix::from_graph(&g);
        assert!(matrix.is_reachable(1, 3));
        assert!(!matrix.is_reachable(3, 1));
    }

    #[test]
    fn test_reachability_matrix_node_count() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        let matrix = ReachabilityMatrix::from_graph(&g);
        assert_eq!(matrix.node_count(), 2);
    }

    #[test]
    fn test_reachability_matrix_reachable_pairs() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        let matrix = ReachabilityMatrix::from_graph(&g);
        assert!(matrix.reachable_pairs() > 0);
    }

    #[test]
    fn test_reachability_matrix_rate() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(2, 1);
        let matrix = ReachabilityMatrix::from_graph(&g);
        assert!((matrix.reachability_rate() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_find_all_paths_max_depth() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(2, 3);
        g.add_edge_unweighted(3, 4);
        let paths = PathAnalyzer::find_all_paths(&g, 1, 4, 2);
        assert!(paths.is_empty());
    }
}
