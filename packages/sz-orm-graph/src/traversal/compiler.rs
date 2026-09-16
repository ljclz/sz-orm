//! 图遍历编译器 — 路径 → JOIN SQL
//!
//! 将点分遍历路径编译为参数化 JOIN SQL 查询。

use super::dsl::TraversalPath;
use super::relation_graph::RelationGraph;

/// 编译后的查询
#[derive(Debug, Clone)]
pub struct CompiledQuery {
    /// SQL 语句
    pub sql: String,
    /// 参数列表
    pub params: Vec<String>,
    /// 涉及的表
    pub tables: Vec<String>,
    /// JOIN 数量
    pub join_count: usize,
}

/// 图遍历编译器
pub struct GraphTraversalCompiler {
    graph: RelationGraph,
}

impl GraphTraversalCompiler {
    /// 创建编译器
    pub fn new(graph: RelationGraph) -> Self {
        Self { graph }
    }

    /// 编译遍历路径为 JOIN SQL
    pub fn compile(&self, path: &TraversalPath) -> Result<CompiledQuery, String> {
        if path.exceeds_max_depth() {
            return Err(format!(
                "遍历深度 {} 超出最大深度 {}",
                path.depth(),
                path.max_depth
            ));
        }

        let mut tables = vec![path.root_table.clone()];
        let mut joins = Vec::new();
        let mut params = Vec::new();
        let mut current_table = path.root_table.clone();

        for (i, segment) in path.segments.iter().enumerate() {
            let edge = self
                .graph
                .find_edge(&current_table, &segment.relation)
                .ok_or_else(|| format!("未找到关系: {}.{}", current_table, segment.relation))?;

            let alias = format!("t{}", i + 1);
            joins.push(format!(
                "LEFT JOIN {} AS {} ON {}.{} = {}.{}",
                edge.to_table,
                alias,
                tables.last().unwrap(),
                edge.to_column,
                alias,
                edge.from_column
            ));
            tables.push(alias.clone());

            if let Some(filter) = &segment.filter {
                joins.push(format!("WHERE {}.{}", alias, filter));
            }
            if let Some(order) = &segment.order_by {
                joins.push(format!("ORDER BY {}.{}", alias, order));
            }
            if let Some(limit) = segment.limit {
                params.push(limit.to_string());
            }

            current_table = edge.to_table.clone();
        }

        let select_columns: Vec<String> = tables.iter().map(|t| format!("{}.*", t)).collect();

        let mut sql = format!("SELECT {}", select_columns.join(", "));
        if !joins.is_empty() {
            sql.push_str(" FROM ");
            sql.push_str(&path.root_table);
            sql.push(' ');
            sql.push_str(&joins.join(" "));
        } else {
            sql.push_str(" FROM ");
            sql.push_str(&path.root_table);
        }

        let real_tables: Vec<String> = std::iter::once(path.root_table.clone())
            .chain(path.segments.iter().map(|s| {
                self.graph
                    .find_edge(&current_table, &s.relation)
                    .map(|e| e.to_table.clone())
                    .unwrap_or_default()
            }))
            .collect();

        Ok(CompiledQuery {
            sql,
            params,
            tables: real_tables,
            join_count: path.segments.len(),
        })
    }

    /// 编译为 COUNT 查询（用于 N+1 检测）
    pub fn compile_count(&self, path: &TraversalPath) -> Result<CompiledQuery, String> {
        let mut compiled = self.compile(path)?;
        compiled.sql = compiled.sql.replacen("SELECT", "SELECT COUNT(*)", 1);
        let select_pos = compiled.sql.find(" FROM").unwrap_or(compiled.sql.len());
        compiled.sql = format!("SELECT COUNT(*){}", &compiled.sql[select_pos..]);
        Ok(compiled)
    }

    /// 关系图引用
    pub fn graph(&self) -> &RelationGraph {
        &self.graph
    }
}

#[cfg(test)]
mod tests {
    use super::super::dsl::{GraphTraversalDsl, PathSegment};
    use super::super::relation_graph::RelationEdge;
    use super::*;

    fn build_test_graph() -> RelationGraph {
        let mut graph = RelationGraph::new();
        graph.add_edge(RelationEdge::new(
            "users", "id", "posts", "user_id", "posts",
        ));
        graph.add_edge(RelationEdge::new(
            "posts", "id", "comments", "post_id", "comments",
        ));
        graph
    }

    #[test]
    fn test_compile_simple_join() {
        let compiler = GraphTraversalCompiler::new(build_test_graph());
        let path = GraphTraversalDsl::parse("users.posts").unwrap();
        let compiled = compiler.compile(&path).unwrap();
        assert!(compiled.sql.contains("SELECT"));
        assert!(compiled.sql.contains("LEFT JOIN posts"));
        assert_eq!(compiled.join_count, 1);
    }

    #[test]
    fn test_compile_deep_join() {
        let compiler = GraphTraversalCompiler::new(build_test_graph());
        let path = GraphTraversalDsl::parse("users.posts.comments").unwrap();
        let compiled = compiler.compile(&path).unwrap();
        assert!(compiled.sql.contains("LEFT JOIN"));
        assert_eq!(compiled.join_count, 2);
    }

    #[test]
    fn test_compile_no_relations() {
        let compiler = GraphTraversalCompiler::new(build_test_graph());
        let path = GraphTraversalDsl::parse("users").unwrap();
        let compiled = compiler.compile(&path).unwrap();
        assert!(compiled.sql.contains("SELECT users.* FROM users"));
        assert_eq!(compiled.join_count, 0);
    }

    #[test]
    fn test_compile_missing_relation() {
        let compiler = GraphTraversalCompiler::new(build_test_graph());
        let path = GraphTraversalDsl::parse("users.nonexistent").unwrap();
        assert!(compiler.compile(&path).is_err());
    }

    #[test]
    fn test_compile_max_depth_exceeded() {
        let compiler = GraphTraversalCompiler::new(build_test_graph());
        let mut path = TraversalPath::new("users").with_max_depth(1);
        path.push(PathSegment::new("posts"));
        path.push(PathSegment::new("comments"));
        assert!(compiler.compile(&path).is_err());
    }

    #[test]
    fn test_compile_with_filter() {
        let compiler = GraphTraversalCompiler::new(build_test_graph());
        let path = GraphTraversalDsl::parse_with_filters("users.posts[active=true]").unwrap();
        let compiled = compiler.compile(&path).unwrap();
        assert!(compiled.sql.contains("active=true"));
    }

    #[test]
    fn test_compile_count() {
        let compiler = GraphTraversalCompiler::new(build_test_graph());
        let path = GraphTraversalDsl::parse("users.posts").unwrap();
        let compiled = compiler.compile_count(&path).unwrap();
        assert!(compiled.sql.contains("COUNT(*)"));
    }
}
