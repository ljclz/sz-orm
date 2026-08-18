//! 图属性统计（Graph Stats）
//!
//! 计算图的各种属性：度分布、密度、聚类系数等。

use std::collections::HashMap;

use crate::algorithm::{DirectedGraph, NodeId, UndirectedGraph};

/// 图统计信息
#[derive(Debug, Clone, serde::Serialize)]
pub struct GraphStats {
    pub node_count: usize,
    pub edge_count: usize,
    pub density: f64,
    pub avg_degree: f64,
    pub max_degree: usize,
    pub min_degree: usize,
    pub is_connected: bool,
    pub component_count: usize,
    pub has_cycle: bool,
    pub largest_component_size: usize,
}

/// 度分布统计
#[derive(Debug, Clone, serde::Serialize)]
pub struct DegreeDistribution {
    pub degree: usize,
    pub count: usize,
    pub fraction: f64,
}

/// 图统计计算器
pub struct GraphStatsCalculator;

impl GraphStatsCalculator {
    /// 计算有向图统计
    pub fn directed(graph: &DirectedGraph) -> GraphStats {
        let node_count = graph.node_count();
        let edge_count = graph.edge_count();
        let components = graph.connected_components();
        let component_count = components.len();
        let largest_component_size = components.iter().map(|c| c.len()).max().unwrap_or(0);
        let mut degrees: Vec<usize> = graph
            .nodes()
            .map(|n| graph.in_degree(n) + graph.out_degree(n))
            .collect();
        degrees.sort_unstable();
        let max_degree = degrees.last().copied().unwrap_or(0);
        let min_degree = degrees.first().copied().unwrap_or(0);
        let avg_degree = if node_count > 0 {
            degrees.iter().sum::<usize>() as f64 / node_count as f64
        } else {
            0.0
        };
        let density = if node_count > 1 {
            edge_count as f64 / (node_count * (node_count - 1)) as f64
        } else {
            0.0
        };
        GraphStats {
            node_count,
            edge_count,
            density,
            avg_degree,
            max_degree,
            min_degree,
            is_connected: component_count <= 1,
            component_count,
            has_cycle: graph.has_cycle(),
            largest_component_size,
        }
    }

    /// 计算无向图统计
    pub fn undirected(graph: &UndirectedGraph) -> GraphStats {
        let node_count = graph.node_count();
        let edge_count = graph.edge_count();
        let components = graph.connected_components();
        let component_count = components.len();
        let largest_component_size = components.iter().map(|c| c.len()).max().unwrap_or(0);
        let mut degrees: Vec<usize> = (0..node_count as NodeId).map(|n| graph.degree(n)).collect();
        degrees.sort_unstable();
        let max_degree = degrees.last().copied().unwrap_or(0);
        let min_degree = degrees.first().copied().unwrap_or(0);
        let avg_degree = if node_count > 0 {
            degrees.iter().sum::<usize>() as f64 / node_count as f64
        } else {
            0.0
        };
        let density = if node_count > 1 {
            2.0 * edge_count as f64 / (node_count * (node_count - 1)) as f64
        } else {
            0.0
        };
        GraphStats {
            node_count,
            edge_count,
            density,
            avg_degree,
            max_degree,
            min_degree,
            is_connected: component_count <= 1,
            component_count,
            has_cycle: false,
            largest_component_size,
        }
    }

    /// 计算度分布
    pub fn degree_distribution_directed(graph: &DirectedGraph) -> Vec<DegreeDistribution> {
        let mut degree_counts: HashMap<usize, usize> = HashMap::new();
        let node_count = graph.node_count();
        for node in graph.nodes() {
            let degree = graph.in_degree(node) + graph.out_degree(node);
            *degree_counts.entry(degree).or_insert(0) += 1;
        }
        let mut dist: Vec<DegreeDistribution> = degree_counts
            .into_iter()
            .map(|(degree, count)| DegreeDistribution {
                degree,
                count,
                fraction: if node_count > 0 {
                    count as f64 / node_count as f64
                } else {
                    0.0
                },
            })
            .collect();
        dist.sort_by_key(|d| d.degree);
        dist
    }

    /// 计算度分布
    pub fn degree_distribution_undirected(graph: &UndirectedGraph) -> Vec<DegreeDistribution> {
        let mut degree_counts: HashMap<usize, usize> = HashMap::new();
        let node_count = graph.node_count();
        for node in 0..node_count as NodeId {
            let degree = graph.degree(node);
            *degree_counts.entry(degree).or_insert(0) += 1;
        }
        let mut dist: Vec<DegreeDistribution> = degree_counts
            .into_iter()
            .map(|(degree, count)| DegreeDistribution {
                degree,
                count,
                fraction: if node_count > 0 {
                    count as f64 / node_count as f64
                } else {
                    0.0
                },
            })
            .collect();
        dist.sort_by_key(|d| d.degree);
        dist
    }
}

impl GraphStats {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }

    pub fn to_summary(&self) -> String {
        format!(
            "Graph: {} nodes, {} edges, density={:.4}, {} components, cycle={}",
            self.node_count, self.edge_count, self.density, self.component_count, self.has_cycle
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graph_stats_empty() {
        let g = DirectedGraph::new();
        let stats = GraphStatsCalculator::directed(&g);
        assert_eq!(stats.node_count, 0);
        assert_eq!(stats.edge_count, 0);
    }

    #[test]
    fn test_graph_stats_single_node() {
        let mut g = DirectedGraph::new();
        g.add_node(1);
        let stats = GraphStatsCalculator::directed(&g);
        assert_eq!(stats.node_count, 1);
        assert_eq!(stats.edge_count, 0);
        assert!(stats.is_connected);
    }

    #[test]
    fn test_graph_stats_simple() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(2, 3);
        let stats = GraphStatsCalculator::directed(&g);
        assert_eq!(stats.node_count, 3);
        assert_eq!(stats.edge_count, 2);
        assert!(stats.is_connected);
        assert!(!stats.has_cycle);
    }

    #[test]
    fn test_graph_stats_density() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(2, 1);
        let stats = GraphStatsCalculator::directed(&g);
        assert!((stats.density - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_graph_stats_disconnected() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(3, 4);
        let stats = GraphStatsCalculator::directed(&g);
        assert!(!stats.is_connected);
        assert_eq!(stats.component_count, 2);
    }

    #[test]
    fn test_graph_stats_with_cycle() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(2, 3);
        g.add_edge_unweighted(3, 1);
        let stats = GraphStatsCalculator::directed(&g);
        assert!(stats.has_cycle);
    }

    #[test]
    fn test_graph_stats_largest_component() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(2, 3);
        g.add_edge_unweighted(4, 5);
        let stats = GraphStatsCalculator::directed(&g);
        assert_eq!(stats.largest_component_size, 3);
    }

    #[test]
    fn test_graph_stats_avg_degree() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(1, 3);
        let stats = GraphStatsCalculator::directed(&g);
        assert!((stats.avg_degree - 4.0 / 3.0).abs() < 0.001);
    }

    #[test]
    fn test_graph_stats_max_min_degree() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(1, 3);
        g.add_edge_unweighted(1, 4);
        let stats = GraphStatsCalculator::directed(&g);
        assert_eq!(stats.max_degree, 3);
        assert_eq!(stats.min_degree, 1);
    }

    #[test]
    fn test_degree_distribution_directed() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(1, 3);
        let dist = GraphStatsCalculator::degree_distribution_directed(&g);
        assert!(!dist.is_empty());
        let total: usize = dist.iter().map(|d| d.count).sum();
        assert_eq!(total, 3);
    }

    #[test]
    fn test_degree_distribution_undirected() {
        let mut g = UndirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(1, 3);
        let dist = GraphStatsCalculator::degree_distribution_undirected(&g);
        assert!(!dist.is_empty());
    }

    #[test]
    fn test_graph_stats_to_json() {
        let g = DirectedGraph::new();
        let stats = GraphStatsCalculator::directed(&g);
        let json = stats.to_json();
        assert!(json.is_object());
    }

    #[test]
    fn test_graph_stats_to_summary() {
        let mut g = DirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        let stats = GraphStatsCalculator::directed(&g);
        let summary = stats.to_summary();
        assert!(summary.contains("2 nodes"));
    }

    #[test]
    fn test_undirected_stats() {
        let mut g = UndirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(2, 3);
        let stats = GraphStatsCalculator::undirected(&g);
        assert_eq!(stats.node_count, 3);
        assert_eq!(stats.edge_count, 2);
        assert!(stats.is_connected);
    }

    #[test]
    fn test_undirected_stats_density() {
        let mut g = UndirectedGraph::new();
        g.add_edge_unweighted(1, 2);
        g.add_edge_unweighted(1, 3);
        g.add_edge_unweighted(2, 3);
        let stats = GraphStatsCalculator::undirected(&g);
        assert!((stats.density - 1.0).abs() < 0.001);
    }
}
