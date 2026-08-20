//! 图算法（Graph Algorithms）
//!
//! 提供常用图算法：BFS/DFS 遍历、Dijkstra 最短路径、拓扑排序、
//! 环检测、连通分量等。基于邻接表表示的图。

use std::collections::{HashMap, HashSet, VecDeque};

/// 图节点 ID 类型
pub type NodeId = u64;

/// 边权重类型
pub type Weight = f64;

/// 有向图
///
/// 使用邻接表表示，支持带权边。
#[derive(Debug, Clone, Default)]
pub struct DirectedGraph {
    /// 邻接表：节点 -> [(邻居, 权重)]
    adjacency: HashMap<NodeId, Vec<(NodeId, Weight)>>,
    /// 节点数（含孤立节点）
    node_count: usize,
}

impl DirectedGraph {
    /// 创建空图
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加节点
    pub fn add_node(&mut self, node: NodeId) {
        if let std::collections::hash_map::Entry::Vacant(e) = self.adjacency.entry(node) {
            e.insert(Vec::new());
            self.node_count += 1;
        }
    }

    /// 添加带权有向边
    pub fn add_edge(&mut self, from: NodeId, to: NodeId, weight: Weight) {
        self.add_node(from);
        self.add_node(to);
        self.adjacency.get_mut(&from).unwrap().push((to, weight));
    }

    /// 添加无权有向边（权重=1.0）
    pub fn add_edge_unweighted(&mut self, from: NodeId, to: NodeId) {
        self.add_edge(from, to, 1.0);
    }

    /// 获取节点的邻居
    pub fn neighbors(&self, node: NodeId) -> Option<&Vec<(NodeId, Weight)>> {
        self.adjacency.get(&node)
    }

    /// 所有节点
    pub fn nodes(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.adjacency.keys().copied()
    }

    /// 节点数
    pub fn node_count(&self) -> usize {
        self.node_count
    }

    /// 边数
    pub fn edge_count(&self) -> usize {
        self.adjacency.values().map(|v| v.len()).sum()
    }

    /// 是否包含节点
    pub fn has_node(&self, node: NodeId) -> bool {
        self.adjacency.contains_key(&node)
    }

    /// 是否包含边
    pub fn has_edge(&self, from: NodeId, to: NodeId) -> bool {
        self.adjacency
            .get(&from)
            .map(|v| v.iter().any(|(n, _)| *n == to))
            .unwrap_or(false)
    }

    /// 获取边权重
    pub fn edge_weight(&self, from: NodeId, to: NodeId) -> Option<Weight> {
        self.adjacency
            .get(&from)
            .and_then(|v| v.iter().find(|(n, _)| *n == to).map(|(_, w)| *w))
    }

    /// 广度优先搜索（BFS）
    ///
    /// 从 `start` 出发，返回按 BFS 顺序访问的节点列表。
    pub fn bfs(&self, start: NodeId) -> Vec<NodeId> {
        if !self.has_node(start) {
            return Vec::new();
        }
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut result = Vec::new();
        visited.insert(start);
        queue.push_back(start);
        while let Some(node) = queue.pop_front() {
            result.push(node);
            if let Some(neighbors) = self.neighbors(node) {
                for &(neighbor, _) in neighbors {
                    if visited.insert(neighbor) {
                        queue.push_back(neighbor);
                    }
                }
            }
        }
        result
    }

    /// 深度优先搜索（DFS）
    ///
    /// 从 `start` 出发，返回按 DFS 顺序访问的节点列表。
    pub fn dfs(&self, start: NodeId) -> Vec<NodeId> {
        if !self.has_node(start) {
            return Vec::new();
        }
        let mut visited = HashSet::new();
        let mut result = Vec::new();
        self.dfs_visit(start, &mut visited, &mut result);
        result
    }

    fn dfs_visit(&self, node: NodeId, visited: &mut HashSet<NodeId>, result: &mut Vec<NodeId>) {
        if !visited.insert(node) {
            return;
        }
        result.push(node);
        if let Some(neighbors) = self.neighbors(node) {
            for &(neighbor, _) in neighbors {
                self.dfs_visit(neighbor, visited, result);
            }
        }
    }

    /// Dijkstra 最短路径算法
    ///
    /// 返回从 `start` 到 `end` 的最短路径和总距离。
    /// 如果不可达返回 None。
    pub fn dijkstra(&self, start: NodeId, end: NodeId) -> Option<(Vec<NodeId>, Weight)> {
        if !self.has_node(start) || !self.has_node(end) {
            return None;
        }
        let mut dist: HashMap<NodeId, Weight> = HashMap::new();
        let mut prev: HashMap<NodeId, NodeId> = HashMap::new();
        let mut visited = HashSet::new();
        for &node in self.adjacency.keys() {
            dist.insert(node, Weight::INFINITY);
        }
        dist.insert(start, 0.0);
        while visited.len() < self.node_count {
            let current = {
                let mut best: Option<(NodeId, Weight)> = None;
                for (&node, &d) in dist.iter() {
                    if !visited.contains(&node)
                        && (best.is_none() || d < best.unwrap().1) {
                            best = Some((node, d));
                        }
                }
                best
            };
            match current {
                None => break,
                Some((node, d)) => {
                    if d == Weight::INFINITY {
                        break;
                    }
                    if node == end {
                        let mut path = vec![end];
                        let mut current = end;
                        while let Some(&p) = prev.get(&current) {
                            path.push(p);
                            current = p;
                        }
                        path.reverse();
                        return Some((path, d));
                    }
                    visited.insert(node);
                    if let Some(neighbors) = self.neighbors(node) {
                        for &(neighbor, weight) in neighbors {
                            if visited.contains(&neighbor) {
                                continue;
                            }
                            let alt = d + weight;
                            if alt < dist[&neighbor] {
                                dist.insert(neighbor, alt);
                                prev.insert(neighbor, node);
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// 拓扑排序（Kahn 算法）
    ///
    /// 返回拓扑排序结果。如果图有环返回 None。
    pub fn topological_sort(&self) -> Option<Vec<NodeId>> {
        let mut in_degree: HashMap<NodeId, usize> = HashMap::new();
        for &node in self.adjacency.keys() {
            in_degree.entry(node).or_insert(0);
        }
        for neighbors in self.adjacency.values() {
            for &(neighbor, _) in neighbors {
                *in_degree.entry(neighbor).or_insert(0) += 1;
            }
        }
        let mut queue: VecDeque<NodeId> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&n, _)| n)
            .collect();
        let mut result = Vec::new();
        while let Some(node) = queue.pop_front() {
            result.push(node);
            if let Some(neighbors) = self.neighbors(node) {
                for &(neighbor, _) in neighbors {
                    if let Some(deg) = in_degree.get_mut(&neighbor) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(neighbor);
                        }
                    }
                }
            }
        }
        if result.len() == self.node_count {
            Some(result)
        } else {
            None
        }
    }

    /// 环检测（DFS）
    ///
    /// 检测图中是否存在环。
    pub fn has_cycle(&self) -> bool {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        for &node in self.adjacency.keys() {
            if !visited.contains(&node)
                && self.has_cycle_dfs(node, &mut visited, &mut rec_stack) {
                    return true;
                }
        }
        false
    }

    fn has_cycle_dfs(
        &self,
        node: NodeId,
        visited: &mut HashSet<NodeId>,
        rec_stack: &mut HashSet<NodeId>,
    ) -> bool {
        visited.insert(node);
        rec_stack.insert(node);
        if let Some(neighbors) = self.neighbors(node) {
            for &(neighbor, _) in neighbors {
                if !visited.contains(&neighbor) {
                    if self.has_cycle_dfs(neighbor, visited, rec_stack) {
                        return true;
                    }
                } else if rec_stack.contains(&neighbor) {
                    return true;
                }
            }
        }
        rec_stack.remove(&node);
        false
    }

    /// 连通分量（弱连通）
    ///
    /// 返回每个连通分量的节点列表。
    pub fn connected_components(&self) -> Vec<Vec<NodeId>> {
        let undirected = self.to_undirected();
        let mut visited = HashSet::new();
        let mut components = Vec::new();
        for &node in undirected.adjacency.keys() {
            if !visited.contains(&node) {
                let component = undirected.bfs(node);
                visited.extend(component.iter().copied());
                components.push(component);
            }
        }
        components
    }

    /// 转为无向图（忽略方向）
    fn to_undirected(&self) -> DirectedGraph {
        let mut undirected = DirectedGraph::new();
        for (&node, neighbors) in &self.adjacency {
            undirected.add_node(node);
            for &(neighbor, weight) in neighbors {
                undirected.add_edge(node, neighbor, weight);
                undirected.add_edge(neighbor, node, weight);
            }
        }
        undirected
    }

    /// 反转图（所有边方向取反）
    pub fn reverse(&self) -> DirectedGraph {
        let mut reversed = DirectedGraph::new();
        for (&node, neighbors) in &self.adjacency {
            reversed.add_node(node);
            for &(neighbor, weight) in neighbors {
                reversed.add_edge(neighbor, node, weight);
            }
        }
        reversed
    }

    /// 节点的入度
    pub fn in_degree(&self, node: NodeId) -> usize {
        self.adjacency
            .values()
            .map(|v| v.iter().filter(|(n, _)| *n == node).count())
            .sum()
    }

    /// 节点的出度
    pub fn out_degree(&self, node: NodeId) -> usize {
        self.adjacency.get(&node).map(|v| v.len()).unwrap_or(0)
    }
}

/// 无向图
#[derive(Debug, Clone, Default)]
pub struct UndirectedGraph {
    inner: DirectedGraph,
}

impl UndirectedGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(&mut self, node: NodeId) {
        self.inner.add_node(node);
    }

    pub fn add_edge(&mut self, from: NodeId, to: NodeId, weight: Weight) {
        self.inner.add_edge(from, to, weight);
        self.inner.add_edge(to, from, weight);
    }

    pub fn add_edge_unweighted(&mut self, from: NodeId, to: NodeId) {
        self.add_edge(from, to, 1.0);
    }

    pub fn node_count(&self) -> usize {
        self.inner.node_count()
    }

    pub fn edge_count(&self) -> usize {
        self.inner.edge_count() / 2
    }

    pub fn bfs(&self, start: NodeId) -> Vec<NodeId> {
        self.inner.bfs(start)
    }

    pub fn dfs(&self, start: NodeId) -> Vec<NodeId> {
        self.inner.dfs(start)
    }

    pub fn connected_components(&self) -> Vec<Vec<NodeId>> {
        self.inner.connected_components()
    }

    pub fn has_node(&self, node: NodeId) -> bool {
        self.inner.has_node(node)
    }

    pub fn has_edge(&self, from: NodeId, to: NodeId) -> bool {
        self.inner.has_edge(from, to)
    }

    pub fn degree(&self, node: NodeId) -> usize {
        self.inner.out_degree(node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_directed_graph_new() {
        let g = DirectedGraph::new();
        assert_eq!(g.node_count(), 0);
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn test_add_node() {
        let mut g = DirectedGraph::new();
        g.add_node(1);
        assert_eq!(g.node_count(), 1);
        assert!(g.has_node(1));
    }

    #[test]
    fn test_add_edge() {
        let mut g = DirectedGraph::new();
        g.add_edge(1, 2, 3.15);
        assert_eq!(g.node_count(), 2);
        assert_eq!(g.edge_count(), 1);
        assert!(g.has_edge(1, 2));
        assert!(!g.has_edge(2, 1));
        assert_eq!(g.edge_weight(1, 2), Some(3.15));
    }

    #[test]
    fn test_add_edge_unweighted() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        assert_eq!(g.edge_weight(1, 2), Some(1.0));
    }

    #[test]
    fn test_bfs_simple() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(1, 3);
        g.add_edge_unweighted(2, 4);
        g.add_edge_unweighted(3, 4);
        let bfs = g.bfs(1);
        assert_eq!(bfs[0], 1);
        assert_eq!(bfs.len(), 4);
        assert!(bfs.contains(&4));
    }

    #[test]
    fn test_bfs_disconnected() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_node(3);
        let bfs = g.bfs(1);
        assert_eq!(bfs.len(), 2);
        assert!(!bfs.contains(&3));
    }

    #[test]
    fn test_bfs_nonexistent_start() {
        let g = DirectedGraph::new();
        assert!(g.bfs(1).is_empty());
    }

    #[test]
    fn test_dfs_simple() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(2, 3);
        g.add_edge_unweighted(3, 4);
        let dfs = g.dfs(1);
        assert_eq!(dfs.len(), 4);
        assert_eq!(dfs[0], 1);
    }

    #[test]
    fn test_dfs_nonexistent_start() {
        let g = DirectedGraph::new();
        assert!(g.dfs(1).is_empty());
    }

    #[test]
    fn test_dijkstra_shortest_path() {
        let mut g = DirectedGraph::new();
        g.add_edge(1, 2, 1.0);
        g.add_edge(2, 3, 2.0);
        g.add_edge(1, 3, 5.0);
        let (path, dist) = g.dijkstra(1, 3).unwrap();
        assert_eq!(path, vec![1, 2, 3]);
        assert!((dist - 3.0).abs() < 0.001);
    }

    #[test]
    fn test_dijkstra_direct_edge() {
        let mut g = DirectedGraph::new();
        g.add_edge(1, 2, 5.0);
        let (path, dist) = g.dijkstra(1, 2).unwrap();
        assert_eq!(path, vec![1, 2]);
        assert!((dist - 5.0).abs() < 0.001);
    }

    #[test]
    fn test_dijkstra_unreachable() {
        let mut g = DirectedGraph::new();
        g.add_edge(1, 2, 1.0);
        g.add_node(3);
        assert!(g.dijkstra(1, 3).is_none());
    }

    #[test]
    fn test_dijkstra_same_node() {
        let mut g = DirectedGraph::new();
        g.add_node(1);
        let (path, dist) = g.dijkstra(1, 1).unwrap();
        assert_eq!(path, vec![1]);
        assert!((dist - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_topological_sort_dag() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(1, 3);
        g.add_edge_unweighted(2, 4);
        g.add_edge_unweighted(3, 4);
        let topo = g.topological_sort().unwrap();
        assert_eq!(topo.len(), 4);
        let pos: HashMap<NodeId, usize> = topo.iter().enumerate().map(|(i, &n)| (n, i)).collect();
        assert!(pos[&1] < pos[&2]);
        assert!(pos[&1] < pos[&3]);
        assert!(pos[&2] < pos[&4]);
        assert!(pos[&3] < pos[&4]);
    }

    #[test]
    fn test_topological_sort_with_cycle() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(2, 3);
        g.add_edge_unweighted(3, 1);
        assert!(g.topological_sort().is_none());
    }

    #[test]
    fn test_has_cycle_no_cycle() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(2, 3);
        assert!(!g.has_cycle());
    }

    #[test]
    fn test_has_cycle_with_cycle() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(2, 3);
        g.add_edge_unweighted(3, 1);
        assert!(g.has_cycle());
    }

    #[test]
    fn test_has_cycle_self_loop() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 1);
        assert!(g.has_cycle());
    }

    #[test]
    fn test_connected_components() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(3, 4);
        g.add_node(5);
        let components = g.connected_components();
        assert_eq!(components.len(), 3);
    }

    #[test]
    fn test_connected_components_single() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(2, 3);
        let components = g.connected_components();
        assert_eq!(components.len(), 1);
    }

    #[test]
    fn test_reverse() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(2, 3);
        let reversed = g.reverse();
        assert!(reversed.has_edge(2, 1));
        assert!(reversed.has_edge(3, 2));
        assert!(!reversed.has_edge(1, 2));
    }

    #[test]
    fn test_in_degree() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 3);
        g.add_edge_unweighted(2, 3);
        assert_eq!(g.in_degree(3), 2);
        assert_eq!(g.in_degree(1), 0);
    }

    #[test]
    fn test_out_degree() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(1, 3);
        assert_eq!(g.out_degree(1), 2);
        assert_eq!(g.out_degree(2), 0);
    }

    #[test]
    fn test_undirected_graph() {
        let mut g = UndirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(2, 3);
        assert_eq!(g.node_count(), 3);
        assert_eq!(g.edge_count(), 2);
        assert!(g.has_edge(1, 2));
        assert!(g.has_edge(2, 1));
    }

    #[test]
    fn test_undirected_connected_components() {
        let mut g = UndirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(3, 4);
        let components = g.connected_components();
        assert_eq!(components.len(), 2);
    }

    #[test]
    fn test_undirected_degree() {
        let mut g = UndirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(1, 3);
        assert_eq!(g.degree(1), 2);
    }

    #[test]
    fn test_undirected_bfs() {
        let mut g = UndirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(2, 3);
        let bfs = g.bfs(1);
        assert_eq!(bfs.len(), 3);
    }
}
