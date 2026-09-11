//! 图查询联合投影（v6.8.0 GRAPH-QUERY-01）
//!
//! 多跳遍历/路径查询/子图匹配，支持超时中断和部分结果返回。

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// 图节点
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
}

/// 图边
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub relation: String,
}

/// 路径查询结果
#[derive(Debug, Clone)]
pub struct PathResult {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub truncated: bool,
    pub depth_reached: usize,
}

/// 子图匹配结果
#[derive(Debug, Clone)]
pub struct SubgraphMatchResult {
    pub matches: Vec<HashMap<String, GraphNode>>,
    pub truncated: bool,
}

/// 联合投影查询器
pub struct JointProjection {
    nodes: HashMap<String, GraphNode>,
    edges: Vec<GraphEdge>,
    adjacency: HashMap<String, Vec<(String, String)>>,
    max_depth: usize,
    timeout: Duration,
}

impl JointProjection {
    /// 创建查询器
    pub fn new(max_depth: usize, timeout: Duration) -> Self {
        Self {
            nodes: HashMap::new(),
            edges: Vec::new(),
            adjacency: HashMap::new(),
            max_depth,
            timeout,
        }
    }

    /// 添加节点
    pub fn add_node(&mut self, node: GraphNode) {
        self.nodes.insert(node.id.clone(), node);
    }

    /// 添加边
    pub fn add_edge(&mut self, edge: GraphEdge) {
        self.adjacency
            .entry(edge.from.clone())
            .or_default()
            .push((edge.to.clone(), edge.relation.clone()));
        self.edges.push(edge);
    }

    /// 多跳路径查询
    pub fn path_query(&self, start: &str, hops: usize) -> PathResult {
        let start_time = Instant::now();
        let max_hops = hops.min(self.max_depth);

        let mut visited = HashSet::new();
        visited.insert(start.to_string());

        let mut nodes = vec![];
        let mut edges = vec![];
        let mut truncated = false;
        let mut depth_reached = 0;

        if let Some(start_node) = self.nodes.get(start) {
            nodes.push(start_node.clone());
        }

        let mut current_level: Vec<String> = vec![start.to_string()];

        for hop in 0..max_hops {
            if start_time.elapsed() > self.timeout {
                truncated = true;
                break;
            }

            let mut next_level = vec![];
            for node_id in &current_level {
                if let Some(neighbors) = self.adjacency.get(node_id) {
                    for (neighbor_id, relation) in neighbors {
                        if !visited.contains(neighbor_id) {
                            visited.insert(neighbor_id.clone());
                            next_level.push(neighbor_id.clone());

                            if let Some(node) = self.nodes.get(neighbor_id) {
                                nodes.push(node.clone());
                            }
                            edges.push(GraphEdge {
                                from: node_id.clone(),
                                to: neighbor_id.clone(),
                                relation: relation.clone(),
                            });

                            if start_time.elapsed() > self.timeout {
                                truncated = true;
                                break;
                            }
                        }
                    }
                }
                if truncated {
                    break;
                }
            }

            depth_reached = hop + 1;
            if next_level.is_empty() || truncated {
                break;
            }
            current_level = next_level;
        }

        if hops > self.max_depth {
            truncated = true;
        }

        PathResult {
            nodes,
            edges,
            truncated,
            depth_reached,
        }
    }

    /// 子图匹配
    pub fn subgraph_match(
        &self,
        pattern_nodes: &[GraphNode],
        pattern_edges: &[GraphEdge],
    ) -> SubgraphMatchResult {
        let start_time = Instant::now();
        let mut matches = vec![];
        let mut truncated = false;

        if pattern_nodes.is_empty() {
            return SubgraphMatchResult { matches, truncated };
        }

        let first_pattern = &pattern_nodes[0];
        for node in self.nodes.values() {
            if node.label == first_pattern.label || first_pattern.label == "*" {
                let mut mapping = HashMap::new();
                mapping.insert(first_pattern.id.clone(), node.clone());

                if pattern_edges.is_empty() {
                    matches.push(mapping);
                } else {
                    if self.try_match_pattern(
                        &mapping,
                        pattern_nodes,
                        pattern_edges,
                        &mut matches,
                        start_time,
                    ) {
                        truncated = true;
                        break;
                    }
                }

                if start_time.elapsed() > self.timeout {
                    truncated = true;
                    break;
                }
            }
        }

        SubgraphMatchResult { matches, truncated }
    }

    fn try_match_pattern(
        &self,
        initial_mapping: &HashMap<String, GraphNode>,
        pattern_nodes: &[GraphNode],
        pattern_edges: &[GraphEdge],
        matches: &mut Vec<HashMap<String, GraphNode>>,
        start_time: Instant,
    ) -> bool {
        if pattern_nodes.len() == 1 {
            return false;
        }

        for edge in pattern_edges {
            if let (Some(from_node), Some(to_pattern)) = (
                initial_mapping.get(&edge.from),
                pattern_nodes.iter().find(|n| n.id == edge.to),
            ) {
                if let Some(neighbors) = self.adjacency.get(&from_node.id) {
                    for (neighbor_id, relation) in neighbors {
                        if relation != &edge.relation && edge.relation != "*" {
                            continue;
                        }
                        if let Some(neighbor_node) = self.nodes.get(neighbor_id) {
                            if to_pattern.label == "*" || neighbor_node.label == to_pattern.label {
                                let mut full_mapping = initial_mapping.clone();
                                full_mapping.insert(to_pattern.id.clone(), neighbor_node.clone());
                                matches.push(full_mapping);
                            }
                        }
                        if start_time.elapsed() > self.timeout {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// 返回节点数
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// 返回边数
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// 联合投影查询：图查询 → 节点主键 → 关系字段批量加载
    ///
    /// 先执行多跳路径查询得到节点主键列表，再通过 `RelationalFieldLoader`
    /// 批量加载各投影表的关系字段，返回 `JointResult`。
    pub async fn query_with_projection(
        &self,
        start: &str,
        hops: usize,
        projections: &[FieldProjection],
        loader: &(dyn RelationalFieldLoader + 'static),
    ) -> Result<JointResult, String> {
        let path_result = self.path_query(start, hops);

        let node_ids: Vec<String> = path_result.nodes.iter().map(|n| n.id.clone()).collect();

        let mut relational_fields = RelationalFields::new();
        for proj in projections {
            let loaded = loader
                .load_fields(&proj.table, &proj.fields, &node_ids)
                .await?;
            relational_fields.extend(loaded);
        }

        Ok(JointResult {
            nodes: path_result.nodes,
            edges: path_result.edges,
            relational_fields,
            truncated: path_result.truncated,
        })
    }
}

/// 字段投影描述：指定表名和字段列表
#[derive(Debug, Clone)]
pub struct FieldProjection {
    /// 表名
    pub table: String,
    /// 字段列表
    pub fields: Vec<String>,
}

impl FieldProjection {
    /// 创建字段投影
    pub fn new(table: impl Into<String>, fields: Vec<String>) -> Self {
        Self {
            table: table.into(),
            fields,
        }
    }
}

/// 关系字段值：节点 ID → (字段名 → 字段值)
pub type RelationalFields = HashMap<String, HashMap<String, String>>;

/// 联合投影查询结果
#[derive(Debug, Clone)]
pub struct JointResult {
    /// 图节点
    pub nodes: Vec<GraphNode>,
    /// 图边
    pub edges: Vec<GraphEdge>,
    /// 关系字段值
    pub relational_fields: RelationalFields,
    /// 是否被截断
    pub truncated: bool,
}

/// 关系字段加载器 trait
///
/// 实现者负责根据节点 ID 列表批量加载关系字段（对应 `sz-orm-core/query.rs` 的 `find_many`）。
#[async_trait::async_trait]
pub trait RelationalFieldLoader: Send + Sync {
    /// 批量加载字段
    ///
    /// # 参数
    /// - `table`: 表名
    /// - `fields`: 字段列表
    /// - `ids`: 节点主键列表
    ///
    /// # 返回
    /// `RelationalFields`：节点 ID → (字段名 → 字段值)
    async fn load_fields(
        &self,
        table: &str,
        fields: &[String],
        ids: &[String],
    ) -> Result<RelationalFields, String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_graph() -> JointProjection {
        let mut graph = JointProjection::new(10, Duration::from_secs(1));
        graph.add_node(GraphNode {
            id: "A".to_string(),
            label: "Person".to_string(),
        });
        graph.add_node(GraphNode {
            id: "B".to_string(),
            label: "Person".to_string(),
        });
        graph.add_node(GraphNode {
            id: "C".to_string(),
            label: "Person".to_string(),
        });
        graph.add_node(GraphNode {
            id: "D".to_string(),
            label: "Company".to_string(),
        });
        graph.add_edge(GraphEdge {
            from: "A".to_string(),
            to: "B".to_string(),
            relation: "KNOWS".to_string(),
        });
        graph.add_edge(GraphEdge {
            from: "B".to_string(),
            to: "C".to_string(),
            relation: "KNOWS".to_string(),
        });
        graph.add_edge(GraphEdge {
            from: "C".to_string(),
            to: "D".to_string(),
            relation: "WORKS_AT".to_string(),
        });
        graph.add_edge(GraphEdge {
            from: "A".to_string(),
            to: "D".to_string(),
            relation: "WORKS_AT".to_string(),
        });
        graph
    }

    #[test]
    fn one_hop_path_query() {
        let graph = make_graph();
        let result = graph.path_query("A", 1);
        assert!(!result.truncated);
        assert_eq!(result.depth_reached, 1);
        assert!(result.nodes.iter().any(|n| n.id == "B"));
        assert!(result.nodes.iter().any(|n| n.id == "D"));
    }

    #[test]
    fn two_hop_path_query() {
        let graph = make_graph();
        let result = graph.path_query("A", 2);
        assert!(!result.truncated);
        assert_eq!(result.depth_reached, 2);
        assert!(result.nodes.iter().any(|n| n.id == "C"));
    }

    #[test]
    fn three_hop_path_query() {
        let graph = make_graph();
        let result = graph.path_query("A", 3);
        assert!(!result.truncated);
        assert!(result.nodes.iter().any(|n| n.id == "D"));
    }

    #[test]
    fn max_depth_truncates() {
        let mut graph = JointProjection::new(2, Duration::from_secs(1));
        graph.add_node(GraphNode {
            id: "A".to_string(),
            label: "X".to_string(),
        });
        graph.add_node(GraphNode {
            id: "B".to_string(),
            label: "X".to_string(),
        });
        graph.add_node(GraphNode {
            id: "C".to_string(),
            label: "X".to_string(),
        });
        graph.add_node(GraphNode {
            id: "D".to_string(),
            label: "X".to_string(),
        });
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
        let result = graph.path_query("A", 5);
        assert!(result.truncated);
    }

    #[test]
    fn path_query_returns_edges() {
        let graph = make_graph();
        let result = graph.path_query("A", 1);
        assert!(!result.edges.is_empty());
        assert!(result.edges.iter().any(|e| e.relation == "KNOWS"));
    }

    #[test]
    fn subgraph_match_single_node() {
        let graph = make_graph();
        let pattern = vec![GraphNode {
            id: "p".to_string(),
            label: "Person".to_string(),
        }];
        let result = graph.subgraph_match(&pattern, &[]);
        assert!(!result.truncated);
        assert_eq!(result.matches.len(), 3);
    }

    #[test]
    fn subgraph_match_with_edge() {
        let graph = make_graph();
        let pattern_nodes = vec![
            GraphNode {
                id: "a".to_string(),
                label: "Person".to_string(),
            },
            GraphNode {
                id: "b".to_string(),
                label: "Person".to_string(),
            },
        ];
        let pattern_edges = vec![GraphEdge {
            from: "a".to_string(),
            to: "b".to_string(),
            relation: "KNOWS".to_string(),
        }];
        let result = graph.subgraph_match(&pattern_nodes, &pattern_edges);
        assert!(!result.truncated);
        assert!(!result.matches.is_empty());
    }

    #[test]
    fn empty_graph_path_query() {
        let graph = JointProjection::new(5, Duration::from_secs(1));
        let result = graph.path_query("X", 3);
        assert!(!result.truncated);
        assert!(result.nodes.is_empty());
    }

    #[test]
    fn node_and_edge_count() {
        let graph = make_graph();
        assert_eq!(graph.node_count(), 4);
        assert_eq!(graph.edge_count(), 4);
    }

    #[test]
    fn wildcard_label_match() {
        let graph = make_graph();
        let pattern = vec![GraphNode {
            id: "n".to_string(),
            label: "*".to_string(),
        }];
        let result = graph.subgraph_match(&pattern, &[]);
        assert_eq!(result.matches.len(), 4);
    }
}
