//! 表关系图
//!
//! 提供 [`TableRelationshipGraph`] 用于构建与查询表之间的关系图，
//! 支持邻接表、拓扑排序、循环检测、关系路径查找等。

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;

/// 关系类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationKind {
    /// 一对一
    OneToOne,
    /// 一对多
    OneToMany,
    /// 多对一
    ManyToOne,
    /// 多对多
    ManyToMany,
}

impl RelationKind {
    /// 返回描述
    #[must_use]
    pub fn description(&self) -> &'static str {
        match self {
            RelationKind::OneToOne => "1:1",
            RelationKind::OneToMany => "1:N",
            RelationKind::ManyToOne => "N:1",
            RelationKind::ManyToMany => "N:N",
        }
    }

    /// 反转关系
    #[must_use]
    pub fn reverse(&self) -> Self {
        match self {
            RelationKind::OneToOne => RelationKind::OneToOne,
            RelationKind::OneToMany => RelationKind::ManyToOne,
            RelationKind::ManyToOne => RelationKind::OneToMany,
            RelationKind::ManyToMany => RelationKind::ManyToMany,
        }
    }
}

impl fmt::Display for RelationKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.description())
    }
}

/// 表关系边
#[derive(Debug, Clone)]
pub struct RelationshipEdge {
    /// 源表
    pub from: String,
    /// 目标表
    pub to: String,
    /// 关系类型
    pub kind: RelationKind,
    /// 外键列（from 表）
    pub from_columns: Vec<String>,
    /// 引用列（to 表）
    pub to_columns: Vec<String>,
    /// 约束名
    pub constraint_name: String,
}

impl RelationshipEdge {
    /// 创建新关系边
    #[must_use]
    pub fn new(from: &str, to: &str, kind: RelationKind) -> Self {
        Self {
            from: from.to_string(),
            to: to.to_string(),
            kind,
            from_columns: Vec::new(),
            to_columns: Vec::new(),
            constraint_name: String::new(),
        }
    }

    /// 设置列
    #[must_use]
    pub fn with_columns(mut self, from_cols: &[&str], to_cols: &[&str]) -> Self {
        self.from_columns = from_cols.iter().map(|s| s.to_string()).collect();
        self.to_columns = to_cols.iter().map(|s| s.to_string()).collect();
        self
    }

    /// 设置约束名
    #[must_use]
    pub fn with_constraint(mut self, name: &str) -> Self {
        self.constraint_name = name.to_string();
        self
    }

    /// 反转边
    #[must_use]
    pub fn reversed(&self) -> Self {
        Self {
            from: self.to.clone(),
            to: self.from.clone(),
            kind: self.kind.reverse(),
            from_columns: self.to_columns.clone(),
            to_columns: self.from_columns.clone(),
            constraint_name: self.constraint_name.clone(),
        }
    }
}

/// 表关系图
#[derive(Debug, Default)]
pub struct TableRelationshipGraph {
    /// 所有表名
    tables: HashSet<String>,
    /// 邻接表（from -> edges）
    adjacency: HashMap<String, Vec<RelationshipEdge>>,
}

impl TableRelationshipGraph {
    /// 创建新的关系图
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加表
    pub fn add_table(&mut self, table: &str) {
        self.tables.insert(table.to_string());
    }

    /// 添加关系
    pub fn add_relationship(&mut self, edge: RelationshipEdge) {
        self.tables.insert(edge.from.clone());
        self.tables.insert(edge.to.clone());
        self.adjacency
            .entry(edge.from.clone())
            .or_default()
            .push(edge);
    }

    /// 获取表的所有关系
    #[must_use]
    pub fn relationships(&self, table: &str) -> Vec<&RelationshipEdge> {
        self.adjacency
            .get(table)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    /// 获取所有表
    #[must_use]
    pub fn tables(&self) -> Vec<String> {
        let mut tables: Vec<String> = self.tables.iter().cloned().collect();
        tables.sort();
        tables
    }

    /// 表数量
    #[must_use]
    pub fn table_count(&self) -> usize {
        self.tables.len()
    }

    /// 关系数量
    #[must_use]
    pub fn relationship_count(&self) -> usize {
        self.adjacency.values().map(|v| v.len()).sum()
    }

    /// 查找两表之间的直接关系
    #[must_use]
    pub fn find_direct_relationship(&self, from: &str, to: &str) -> Option<&RelationshipEdge> {
        self.adjacency
            .get(from)
            .and_then(|edges| edges.iter().find(|e| e.to == to))
    }

    /// 查找两表之间的路径（BFS）
    #[must_use]
    pub fn find_path(&self, from: &str, to: &str) -> Option<Vec<String>> {
        if from == to {
            return Some(vec![from.to_string()]);
        }
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut parent: HashMap<String, String> = HashMap::new();
        visited.insert(from.to_string());
        queue.push_back(from.to_string());
        while let Some(current) = queue.pop_front() {
            if let Some(edges) = self.adjacency.get(&current) {
                for edge in edges {
                    if !visited.contains(&edge.to) {
                        visited.insert(edge.to.clone());
                        parent.insert(edge.to.clone(), current.clone());
                        if edge.to == to {
                            let mut path = vec![to.to_string()];
                            let mut node = to.to_string();
                            while let Some(p) = parent.get(&node) {
                                path.push(p.clone());
                                node = p.clone();
                            }
                            path.reverse();
                            return Some(path);
                        }
                        queue.push_back(edge.to.clone());
                    }
                }
            }
        }
        None
    }

    /// 检测循环依赖（DFS）
    #[must_use]
    pub fn has_cycle(&self) -> bool {
        let mut visited = HashSet::new();
        let mut recursion_stack = HashSet::new();
        for table in &self.tables {
            if !visited.contains(table) {
                if self.dfs_cycle(table, &mut visited, &mut recursion_stack) {
                    return true;
                }
            }
        }
        false
    }

    fn dfs_cycle(
        &self,
        node: &str,
        visited: &mut HashSet<String>,
        recursion_stack: &mut HashSet<String>,
    ) -> bool {
        visited.insert(node.to_string());
        recursion_stack.insert(node.to_string());
        if let Some(edges) = self.adjacency.get(node) {
            for edge in edges {
                if !visited.contains(&edge.to) {
                    if self.dfs_cycle(&edge.to, visited, recursion_stack) {
                        return true;
                    }
                } else if recursion_stack.contains(&edge.to) {
                    return true;
                }
            }
        }
        recursion_stack.remove(node);
        false
    }

    /// 拓扑排序
    #[must_use]
    pub fn topological_sort(&self) -> Option<Vec<String>> {
        if self.has_cycle() {
            return None;
        }
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        for table in &self.tables {
            in_degree.insert(table.clone(), 0);
        }
        for edges in self.adjacency.values() {
            for edge in edges {
                *in_degree.entry(edge.to.clone()).or_insert(0) += 1;
            }
        }
        let mut queue: VecDeque<String> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(k, _)| k.clone())
            .collect();
        let mut result = Vec::new();
        while let Some(node) = queue.pop_front() {
            result.push(node.clone());
            if let Some(edges) = self.adjacency.get(&node) {
                for edge in edges {
                    if let Some(deg) = in_degree.get_mut(&edge.to) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(edge.to.clone());
                        }
                    }
                }
            }
        }
        if result.len() == self.tables.len() {
            Some(result)
        } else {
            None
        }
    }

    /// 获取孤立表（无任何关系）
    #[must_use]
    pub fn isolated_tables(&self) -> Vec<String> {
        self.tables
            .iter()
            .filter(|t| !self.adjacency.contains_key(*t))
            .cloned()
            .collect()
    }

    /// 获取邻接表
    #[must_use]
    pub fn adjacency(&self) -> &HashMap<String, Vec<RelationshipEdge>> {
        &self.adjacency
    }

    /// 生成 DOT 图描述
    #[must_use]
    pub fn to_dot(&self) -> String {
        let mut dot = String::from("digraph schema {\n  rankdir=LR;\n");
        for table in &self.tables {
            dot.push_str(&format!("  \"{table}\" [shape=box];\n"));
        }
        for edges in self.adjacency.values() {
            for edge in edges {
                dot.push_str(&format!(
                    "  \"{}\" -> \"{}\" [label=\"{}\"];\n",
                    edge.from, edge.to, edge.kind
                ));
            }
        }
        dot.push_str("}\n");
        dot
    }
}

impl fmt::Display for TableRelationshipGraph {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "TableRelationshipGraph(tables={}, relationships={})",
            self.table_count(),
            self.relationship_count()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relation_kind_description() {
        assert_eq!(RelationKind::OneToOne.description(), "1:1");
        assert_eq!(RelationKind::OneToMany.description(), "1:N");
    }

    #[test]
    fn test_relation_kind_reverse() {
        assert_eq!(RelationKind::OneToMany.reverse(), RelationKind::ManyToOne);
        assert_eq!(RelationKind::OneToOne.reverse(), RelationKind::OneToOne);
    }

    #[test]
    fn test_relationship_edge_new() {
        let edge = RelationshipEdge::new("orders", "users", RelationKind::ManyToOne);
        assert_eq!(edge.from, "orders");
        assert_eq!(edge.to, "users");
    }

    #[test]
    fn test_relationship_edge_reversed() {
        let edge = RelationshipEdge::new("orders", "users", RelationKind::ManyToOne);
        let rev = edge.reversed();
        assert_eq!(rev.from, "users");
        assert_eq!(rev.to, "orders");
        assert_eq!(rev.kind, RelationKind::OneToMany);
    }

    #[test]
    fn test_graph_add_table() {
        let mut g = TableRelationshipGraph::new();
        g.add_table("users");
        assert_eq!(g.table_count(), 1);
    }

    #[test]
    fn test_graph_add_relationship() {
        let mut g = TableRelationshipGraph::new();
        g.add_relationship(RelationshipEdge::new(
            "orders",
            "users",
            RelationKind::ManyToOne,
        ));
        assert_eq!(g.table_count(), 2);
        assert_eq!(g.relationship_count(), 1);
    }

    #[test]
    fn test_graph_relationships() {
        let mut g = TableRelationshipGraph::new();
        g.add_relationship(RelationshipEdge::new(
            "orders",
            "users",
            RelationKind::ManyToOne,
        ));
        let rels = g.relationships("orders");
        assert_eq!(rels.len(), 1);
    }

    #[test]
    fn test_graph_find_direct_relationship() {
        let mut g = TableRelationshipGraph::new();
        g.add_relationship(RelationshipEdge::new(
            "orders",
            "users",
            RelationKind::ManyToOne,
        ));
        assert!(g.find_direct_relationship("orders", "users").is_some());
        assert!(g.find_direct_relationship("users", "orders").is_none());
    }

    #[test]
    fn test_graph_find_path_direct() {
        let mut g = TableRelationshipGraph::new();
        g.add_relationship(RelationshipEdge::new(
            "orders",
            "users",
            RelationKind::ManyToOne,
        ));
        let path = g.find_path("orders", "users").unwrap();
        assert_eq!(path, vec!["orders".to_string(), "users".to_string()]);
    }

    #[test]
    fn test_graph_find_path_transitive() {
        let mut g = TableRelationshipGraph::new();
        g.add_relationship(RelationshipEdge::new(
            "items",
            "orders",
            RelationKind::ManyToOne,
        ));
        g.add_relationship(RelationshipEdge::new(
            "orders",
            "users",
            RelationKind::ManyToOne,
        ));
        let path = g.find_path("items", "users").unwrap();
        assert_eq!(path.len(), 3);
    }

    #[test]
    fn test_graph_find_path_none() {
        let mut g = TableRelationshipGraph::new();
        g.add_table("a");
        g.add_table("b");
        assert!(g.find_path("a", "b").is_none());
    }

    #[test]
    fn test_graph_find_path_self() {
        let mut g = TableRelationshipGraph::new();
        g.add_table("users");
        let path = g.find_path("users", "users").unwrap();
        assert_eq!(path, vec!["users".to_string()]);
    }

    #[test]
    fn test_graph_has_cycle_no() {
        let mut g = TableRelationshipGraph::new();
        g.add_relationship(RelationshipEdge::new(
            "orders",
            "users",
            RelationKind::ManyToOne,
        ));
        assert!(!g.has_cycle());
    }

    #[test]
    fn test_graph_has_cycle_yes() {
        let mut g = TableRelationshipGraph::new();
        g.add_relationship(RelationshipEdge::new("a", "b", RelationKind::OneToOne));
        g.add_relationship(RelationshipEdge::new("b", "a", RelationKind::OneToOne));
        assert!(g.has_cycle());
    }

    #[test]
    fn test_graph_topological_sort_no_cycle() {
        let mut g = TableRelationshipGraph::new();
        g.add_relationship(RelationshipEdge::new(
            "orders",
            "users",
            RelationKind::ManyToOne,
        ));
        let sorted = g.topological_sort().unwrap();
        let user_idx = sorted.iter().position(|t| t == "users").unwrap();
        let order_idx = sorted.iter().position(|t| t == "orders").unwrap();
        assert!(order_idx < user_idx);
    }

    #[test]
    fn test_graph_topological_sort_with_cycle() {
        let mut g = TableRelationshipGraph::new();
        g.add_relationship(RelationshipEdge::new("a", "b", RelationKind::OneToOne));
        g.add_relationship(RelationshipEdge::new("b", "a", RelationKind::OneToOne));
        assert!(g.topological_sort().is_none());
    }

    #[test]
    fn test_graph_isolated_tables() {
        let mut g = TableRelationshipGraph::new();
        g.add_table("isolated");
        g.add_relationship(RelationshipEdge::new(
            "orders",
            "users",
            RelationKind::ManyToOne,
        ));
        let isolated = g.isolated_tables();
        assert!(isolated.contains(&"isolated".to_string()));
    }

    #[test]
    fn test_graph_to_dot() {
        let mut g = TableRelationshipGraph::new();
        g.add_relationship(RelationshipEdge::new(
            "orders",
            "users",
            RelationKind::ManyToOne,
        ));
        let dot = g.to_dot();
        assert!(dot.contains("digraph"));
        assert!(dot.contains("\"orders\" -> \"users\""));
    }

    #[test]
    fn test_graph_tables() {
        let mut g = TableRelationshipGraph::new();
        g.add_table("b");
        g.add_table("a");
        let tables = g.tables();
        assert_eq!(tables, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn test_graph_display() {
        let g = TableRelationshipGraph::new();
        let s = format!("{}", g);
        assert!(s.contains("TableRelationshipGraph"));
    }
}
