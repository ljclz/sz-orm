//! 关系图定义 + 环路检测
//!
//! 定义表间关系边，检测环路防止无限=无限递归遍历。

use std::collections::{HashMap, HashSet};

/// 关系边（外键关系）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationEdge {
    /// 源表
    pub from_table: String,
    /// 源表外键列
    pub from_column: String,
    /// 目标表
    pub to_table: String,
    /// 目标表主键列
    pub to_column: String,
    /// 关系名称（如 "posts", "comments"）
    pub relation_name: String,
}

impl RelationEdge {
    /// 创建关系边
    pub fn new(
        from_table: &str,
        from_column: &str,
        to_table: &str,
        to_column: &str,
        relation_name: &str,
    ) -> Self {
        Self {
            from_table: from_table.to_string(),
            from_column: from_column.to_string(),
            to_table: to_table.to_string(),
            to_column: to_column.to_string(),
            relation_name: relation_name.to_string(),
        }
    }
}

/// 关系图
pub struct RelationGraph {
    edges: Vec<RelationEdge>,
    edges_by_table: HashMap<String, Vec<usize>>,
}

impl Default for RelationGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl RelationGraph {
    /// 创建空关系图
    pub fn new() -> Self {
        Self {
            edges: Vec::new(),
            edges_by_table: HashMap::new(),
        }
    }

    /// 添加关系边
    pub fn add_edge(&mut self, edge: RelationEdge) {
        let idx = self.edges.len();
        self.edges_by_table
            .entry(edge.from_table.clone())
            .or_default()
            .push(idx);
        self.edges.push(edge);
    }

    /// 获取表的所有出边
    pub fn edges_from(&self, table: &str) -> Vec<&RelationEdge> {
        self.edges_by_table
            .get(table)
            .map(|indices| indices.iter().map(|&i| &self.edges[i]).collect())
            .unwrap_or_default()
    }

    /// 按关系名查找边
    pub fn find_edge(&self, from_table: &str, relation_name: &str) -> Option<&RelationEdge> {
        self.edges_from(from_table)
            .into_iter()
            .find(|e| e.relation_name == relation_name)
    }

    /// 检测从 start_table 开始是否存在环路
    pub fn has_cycle(&self, start_table: &str) -> bool {
        let mut visited = HashSet::new();
        let mut stack = vec![start_table.to_string()];
        while let Some(table) = stack.pop() {
            if !visited.insert(table.clone()) {
                return true;
            }
            for edge in self.edges_from(&table) {
                stack.push(edge.to_table.clone());
            }
        }
        false
    }

    /// 检测遍历路径是否会形成环路
    pub fn path_has_cycle(&self, path: &[String]) -> bool {
        let mut visited = HashSet::new();
        for table in path {
            if !visited.insert(table.clone()) {
                return true;
            }
        }
        false
    }

    /// 边数量
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_find_edge() {
        let mut graph = RelationGraph::new();
        graph.add_edge(RelationEdge::new(
            "users", "id", "posts", "user_id", "posts",
        ));
        graph.add_edge(RelationEdge::new(
            "posts", "id", "comments", "post_id", "comments",
        ));

        let edge = graph.find_edge("users", "posts").unwrap();
        assert_eq!(edge.to_table, "posts");
        assert_eq!(edge.to_column, "user_id");
    }

    #[test]
    fn test_edges_from() {
        let mut graph = RelationGraph::new();
        graph.add_edge(RelationEdge::new(
            "users", "id", "posts", "user_id", "posts",
        ));
        graph.add_edge(RelationEdge::new(
            "users", "id", "profiles", "user_id", "profile",
        ));
        let edges = graph.edges_from("users");
        assert_eq!(edges.len(), 2);
    }

    #[test]
    fn test_no_cycle() {
        let mut graph = RelationGraph::new();
        graph.add_edge(RelationEdge::new(
            "users", "id", "posts", "user_id", "posts",
        ));
        graph.add_edge(RelationEdge::new(
            "posts", "id", "comments", "post_id", "comments",
        ));
        assert!(!graph.has_cycle("users"));
    }

    #[test]
    fn test_cycle_detected() {
        let mut graph = RelationGraph::new();
        graph.add_edge(RelationEdge::new("a", "id", "b", "a_id", "b"));
        graph.add_edge(RelationEdge::new("b", "id", "a", "b_id", "a"));
        assert!(graph.has_cycle("a"));
    }

    #[test]
    fn test_path_cycle() {
        let graph = RelationGraph::new();
        assert!(graph.path_has_cycle(&["a".into(), "b".into(), "a".into()]));
        assert!(!graph.path_has_cycle(&["a".into(), "b".into(), "c".into()]));
    }

    #[test]
    fn test_edge_count() {
        let mut graph = RelationGraph::new();
        graph.add_edge(RelationEdge::new("a", "id", "b", "a_id", "b"));
        assert_eq!(graph.edge_count(), 1);
    }
}
