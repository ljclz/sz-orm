//! LineageGraph DAG 实现：节点管理、边管理、环路检测、增量更新。

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;

/// lineage 节点标识（table.column）
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LineageNodeId {
    pub table: String,
    pub column: String,
}

impl LineageNodeId {
    pub fn new(table: impl Into<String>, column: impl Into<String>) -> Self {
        Self {
            table: table.into(),
            column: column.into(),
        }
    }
}

/// 节点类型
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NodeType {
    Table,
    Column,
    View,
    MaterializedView,
}

/// lineage 节点
#[derive(Debug, Clone)]
pub struct LineageNode {
    pub id: LineageNodeId,
    pub node_type: NodeType,
}

impl LineageNode {
    pub fn new(id: LineageNodeId, node_type: NodeType) -> Self {
        Self { id, node_type }
    }
}

/// 边类型（依赖关系种类）
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EdgeType {
    DirectDependency,
    Derived,
    Join,
    Filter,
    Projection,
}

/// lineage 边（source → target 依赖关系）
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LineageEdge {
    pub source: LineageNodeId,
    pub target: LineageNodeId,
    pub edge_type: EdgeType,
}

impl LineageEdge {
    pub fn new(source: LineageNodeId, target: LineageNodeId, edge_type: EdgeType) -> Self {
        Self {
            source,
            target,
            edge_type,
        }
    }
}

/// lineage 错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineageError {
    CycleDetected,
    NodeNotFound(String),
    ParseFailed(String),
}

impl fmt::Display for LineageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LineageError::CycleDetected => {
                write!(f, "cycle detected: adding edge would create a cycle")
            }
            LineageError::NodeNotFound(s) => write!(f, "node not found: {s}"),
            LineageError::ParseFailed(s) => write!(f, "SQL parse failed: {s}"),
        }
    }
}

impl std::error::Error for LineageError {}

/// lineage 图（DAG）
#[derive(Debug, Clone, Default)]
pub struct LineageGraph {
    pub nodes: HashMap<LineageNodeId, LineageNode>,
    pub edges: HashSet<LineageEdge>,
}

impl LineageGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: HashSet::new(),
        }
    }

    /// 添加节点（已存在则覆盖类型）
    pub fn add_node(&mut self, node: LineageNode) {
        self.nodes.insert(node.id.clone(), node);
    }

    /// 添加边前检测环路（DFS），环路时返回 `LineageError::CycleDetected`
    pub fn add_edge(&mut self, edge: LineageEdge) -> Result<(), LineageError> {
        if edge.source == edge.target {
            return Err(LineageError::CycleDetected);
        }

        if self.would_create_cycle(&edge.source, &edge.target) {
            return Err(LineageError::CycleDetected);
        }

        self.nodes
            .entry(edge.source.clone())
            .or_insert_with(|| LineageNode::new(edge.source.clone(), NodeType::Column));
        self.nodes
            .entry(edge.target.clone())
            .or_insert_with(|| LineageNode::new(edge.target.clone(), NodeType::Column));

        self.edges.insert(edge);
        Ok(())
    }

    /// 增量更新图，新增/修改边，既有边保留
    pub fn incremental_update(&mut self, edges: Vec<LineageEdge>) {
        for edge in edges {
            let _ = self.add_edge(edge);
        }
    }

    /// 检查添加 source→target 边后是否会形成环路。
    /// 如果从 target 能到达 source，则添加 source→target 后会形成环路。
    fn would_create_cycle(&self, source: &LineageNodeId, target: &LineageNodeId) -> bool {
        if source == target {
            return true;
        }

        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(target.clone());

        while let Some(current) = queue.pop_front() {
            if &current == source {
                return true;
            }

            if !visited.insert(current.clone()) {
                continue;
            }

            for edge in &self.edges {
                if edge.source == current && !visited.contains(&edge.target) {
                    queue.push_back(edge.target.clone());
                }
            }
        }

        false
    }

    /// 获取节点
    pub fn get_node(&self, id: &LineageNodeId) -> Option<&LineageNode> {
        self.nodes.get(id)
    }

    /// 获取从某节点出发的所有边
    pub fn outgoing_edges(&self, node: &LineageNodeId) -> Vec<&LineageEdge> {
        self.edges.iter().filter(|e| &e.source == node).collect()
    }

    /// 获取指向某节点的所有边
    pub fn incoming_edges(&self, node: &LineageNodeId) -> Vec<&LineageEdge> {
        self.edges.iter().filter(|e| &e.target == node).collect()
    }

    /// 正向图遍历（BFS），输出下游受影响节点
    pub fn impact_analysis(&self, node: &LineageNodeId) -> Vec<LineageNode> {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut result = Vec::new();

        for edge in self.outgoing_edges(node) {
            queue.push_back(edge.target.clone());
        }

        while let Some(current) = queue.pop_front() {
            if !visited.insert(current.clone()) {
                continue;
            }

            if let Some(n) = self.nodes.get(&current) {
                result.push(n.clone());
            }

            for edge in self.outgoing_edges(&current) {
                if !visited.contains(&edge.target) {
                    queue.push_back(edge.target.clone());
                }
            }
        }

        result
    }

    /// 反向图遍历（BFS），输出源头节点
    pub fn origin_analysis(&self, node: &LineageNodeId) -> Vec<LineageNode> {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut result = Vec::new();

        for edge in self.incoming_edges(node) {
            queue.push_back(edge.source.clone());
        }

        while let Some(current) = queue.pop_front() {
            if !visited.insert(current.clone()) {
                continue;
            }

            if let Some(n) = self.nodes.get(&current) {
                result.push(n.clone());
            }

            for edge in self.incoming_edges(&current) {
                if !visited.contains(&edge.source) {
                    queue.push_back(edge.source.clone());
                }
            }
        }

        result
    }

    /// 节点数量
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// 边数量
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nid(table: &str, column: &str) -> LineageNodeId {
        LineageNodeId::new(table, column)
    }

    #[test]
    fn test_add_nodes_and_edges() {
        let mut graph = LineageGraph::new();

        graph.add_node(LineageNode::new(nid("users", "name"), NodeType::Column));
        graph.add_node(LineageNode::new(nid("report", "name"), NodeType::View));

        let edge = LineageEdge::new(
            nid("users", "name"),
            nid("report", "name"),
            EdgeType::Derived,
        );
        assert!(graph.add_edge(edge).is_ok());

        assert_eq!(graph.node_count(), 2);
        assert_eq!(graph.edge_count(), 1);
    }

    #[test]
    fn test_cycle_detection_a_to_b_to_a() {
        let mut graph = LineageGraph::new();

        let a = nid("table_a", "col");
        let b = nid("table_b", "col");

        assert!(graph
            .add_edge(LineageEdge::new(
                a.clone(),
                b.clone(),
                EdgeType::DirectDependency
            ))
            .is_ok());

        let result = graph.add_edge(LineageEdge::new(b, a, EdgeType::DirectDependency));
        assert_eq!(result, Err(LineageError::CycleDetected));
        assert_eq!(graph.edge_count(), 1);
    }

    #[test]
    fn test_cycle_detection_self_loop() {
        let mut graph = LineageGraph::new();
        let a = nid("table_a", "col");

        let result = graph.add_edge(LineageEdge::new(a.clone(), a, EdgeType::DirectDependency));
        assert_eq!(result, Err(LineageError::CycleDetected));
        assert_eq!(graph.edge_count(), 0);
    }

    #[test]
    fn test_cycle_detection_longer_chain() {
        let mut graph = LineageGraph::new();

        let a = nid("a", "x");
        let b = nid("b", "x");
        let c = nid("c", "x");

        assert!(graph
            .add_edge(LineageEdge::new(a.clone(), b.clone(), EdgeType::Derived))
            .is_ok());
        assert!(graph
            .add_edge(LineageEdge::new(b.clone(), c.clone(), EdgeType::Derived))
            .is_ok());

        let result = graph.add_edge(LineageEdge::new(c, a, EdgeType::Derived));
        assert_eq!(result, Err(LineageError::CycleDetected));
        assert_eq!(graph.edge_count(), 2);
    }

    #[test]
    fn test_incremental_update_preserves_existing() {
        let mut graph = LineageGraph::new();

        graph.add_node(LineageNode::new(nid("users", "name"), NodeType::Column));
        graph.add_node(LineageNode::new(nid("report", "name"), NodeType::View));

        assert!(graph
            .add_edge(LineageEdge::new(
                nid("users", "name"),
                nid("report", "name"),
                EdgeType::Derived,
            ))
            .is_ok());

        assert_eq!(graph.edge_count(), 1);

        graph.incremental_update(vec![
            LineageEdge::new(
                nid("users", "email"),
                nid("report", "email"),
                EdgeType::Derived,
            ),
            LineageEdge::new(
                nid("users", "name"),
                nid("report", "name"),
                EdgeType::Derived,
            ),
        ]);

        assert_eq!(graph.edge_count(), 2);
        assert!(graph.edges.contains(&LineageEdge::new(
            nid("users", "name"),
            nid("report", "name"),
            EdgeType::Derived,
        )));
        assert!(graph.edges.contains(&LineageEdge::new(
            nid("users", "email"),
            nid("report", "email"),
            EdgeType::Derived,
        )));
    }

    #[test]
    fn test_incremental_update_skips_cycles() {
        let mut graph = LineageGraph::new();

        let a = nid("a", "x");
        let b = nid("b", "x");

        assert!(graph
            .add_edge(LineageEdge::new(
                a.clone(),
                b.clone(),
                EdgeType::DirectDependency
            ))
            .is_ok());

        graph.incremental_update(vec![LineageEdge::new(
            b.clone(),
            a.clone(),
            EdgeType::DirectDependency,
        )]);

        assert_eq!(graph.edge_count(), 1);
    }

    #[test]
    fn test_impact_analysis() {
        let mut graph = LineageGraph::new();

        graph
            .add_edge(LineageEdge::new(
                nid("users", "name"),
                nid("report", "name"),
                EdgeType::Derived,
            ))
            .unwrap();
        graph
            .add_edge(LineageEdge::new(
                nid("report", "name"),
                nid("dashboard", "title"),
                EdgeType::Derived,
            ))
            .unwrap();

        let impacted = graph.impact_analysis(&nid("users", "name"));
        assert_eq!(impacted.len(), 2);
        assert!(impacted.iter().any(|n| n.id == nid("report", "name")));
        assert!(impacted.iter().any(|n| n.id == nid("dashboard", "title")));
    }

    #[test]
    fn test_origin_analysis() {
        let mut graph = LineageGraph::new();

        graph
            .add_edge(LineageEdge::new(
                nid("users", "name"),
                nid("report", "name"),
                EdgeType::Derived,
            ))
            .unwrap();
        graph
            .add_edge(LineageEdge::new(
                nid("users", "email"),
                nid("report", "name"),
                EdgeType::Join,
            ))
            .unwrap();

        let origins = graph.origin_analysis(&nid("report", "name"));
        assert_eq!(origins.len(), 2);
        assert!(origins.iter().any(|n| n.id == nid("users", "name")));
        assert!(origins.iter().any(|n| n.id == nid("users", "email")));
    }

    #[test]
    fn test_outgoing_and_incoming_edges() {
        let mut graph = LineageGraph::new();

        graph
            .add_edge(LineageEdge::new(
                nid("a", "x"),
                nid("b", "y"),
                EdgeType::DirectDependency,
            ))
            .unwrap();
        graph
            .add_edge(LineageEdge::new(
                nid("a", "x"),
                nid("c", "z"),
                EdgeType::Projection,
            ))
            .unwrap();

        let out = graph.outgoing_edges(&nid("a", "x"));
        assert_eq!(out.len(), 2);

        let inc = graph.incoming_edges(&nid("b", "y"));
        assert_eq!(inc.len(), 1);
    }

    #[test]
    fn test_get_node() {
        let mut graph = LineageGraph::new();
        let id = nid("users", "name");
        graph.add_node(LineageNode::new(id.clone(), NodeType::Table));

        let node = graph.get_node(&id).unwrap();
        assert_eq!(node.node_type, NodeType::Table);
    }

    #[test]
    fn test_add_edge_auto_creates_nodes() {
        let mut graph = LineageGraph::new();

        graph
            .add_edge(LineageEdge::new(
                nid("src", "col"),
                nid("dst", "col"),
                EdgeType::Derived,
            ))
            .unwrap();

        assert_eq!(graph.node_count(), 2);
        assert!(graph.get_node(&nid("src", "col")).is_some());
        assert!(graph.get_node(&nid("dst", "col")).is_some());
    }

    #[test]
    fn test_node_type_variants() {
        let mut graph = LineageGraph::new();

        graph.add_node(LineageNode::new(nid("t1", "c"), NodeType::Table));
        graph.add_node(LineageNode::new(nid("t2", "c"), NodeType::Column));
        graph.add_node(LineageNode::new(nid("t3", "c"), NodeType::View));
        graph.add_node(LineageNode::new(nid("t4", "c"), NodeType::MaterializedView));

        assert_eq!(graph.node_count(), 4);
    }

    #[test]
    fn test_edge_type_variants() {
        let mut graph = LineageGraph::new();

        let variants = vec![
            EdgeType::DirectDependency,
            EdgeType::Derived,
            EdgeType::Join,
            EdgeType::Filter,
            EdgeType::Projection,
        ];

        for (i, et) in variants.into_iter().enumerate() {
            let src = LineageNodeId::new("s", format!("col{i}"));
            let tgt = LineageNodeId::new("t", format!("col{i}"));
            assert!(graph.add_edge(LineageEdge::new(src, tgt, et)).is_ok());
        }

        assert_eq!(graph.edge_count(), 5);
    }

    #[test]
    fn test_empty_graph_default() {
        let graph = LineageGraph::default();
        assert_eq!(graph.node_count(), 0);
        assert_eq!(graph.edge_count(), 0);
    }

    #[test]
    fn test_impact_analysis_no_descendants() {
        let mut graph = LineageGraph::new();
        graph.add_node(LineageNode::new(nid("leaf", "col"), NodeType::Column));

        let impacted = graph.impact_analysis(&nid("leaf", "col"));
        assert!(impacted.is_empty());
    }

    #[test]
    fn test_origin_analysis_no_origins() {
        let mut graph = LineageGraph::new();
        graph.add_node(LineageNode::new(nid("root", "col"), NodeType::Column));

        let origins = graph.origin_analysis(&nid("root", "col"));
        assert!(origins.is_empty());
    }

    #[test]
    fn test_diamond_dependency_no_cycle() {
        let mut graph = LineageGraph::new();

        let a = nid("a", "x");
        let b = nid("b", "x");
        let c = nid("c", "x");
        let d = nid("d", "x");

        assert!(graph
            .add_edge(LineageEdge::new(a.clone(), b.clone(), EdgeType::Derived))
            .is_ok());
        assert!(graph
            .add_edge(LineageEdge::new(a.clone(), c.clone(), EdgeType::Derived))
            .is_ok());
        assert!(graph
            .add_edge(LineageEdge::new(b.clone(), d.clone(), EdgeType::Derived))
            .is_ok());
        assert!(graph
            .add_edge(LineageEdge::new(c.clone(), d.clone(), EdgeType::Derived))
            .is_ok());

        assert_eq!(graph.edge_count(), 4);

        let impacted = graph.impact_analysis(&a);
        assert_eq!(impacted.len(), 3);
    }
}
