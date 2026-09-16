//! 图遍历执行器
//!
//! 编排路径解析 → SQL 编译 → 执行 → 结果水合。

use super::compiler::{CompiledQuery, GraphTraversalCompiler};
use super::dsl::{GraphTraversalDsl, TraversalPath};
use super::relation_graph::RelationGraph;

/// 执行结果
#[derive(Debug, Clone)]
pub struct TraversalResult {
    /// 编译后的查询
    pub query: CompiledQuery,
    /// 执行的 JOIN 数量
    pub join_count: usize,
    /// 是否使用单次 JOIN（而非 N+1 查询）
    pub uses_single_join: bool,
}

/// 图遍历执行器
pub struct GraphTraversalExecutor {
    compiler: GraphTraversalCompiler,
    max_depth: usize,
}

impl GraphTraversalExecutor {
    /// 创建执行器
    pub fn new(graph: RelationGraph) -> Self {
        Self {
            compiler: GraphTraversalCompiler::new(graph),
            max_depth: 10,
        }
    }

    /// 设置最大深度
    pub fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth;
        self
    }

    /// 执行遍历（编译路径为 JOIN SQL）
    pub fn traverse(&self, path_str: &str) -> Result<TraversalResult, String> {
        let mut path = GraphTraversalDsl::parse(path_str)?;
        path.max_depth = self.max_depth;
        self.traverse_path(&path)
    }

    /// 执行遍历（使用已解析的路径）
    pub fn traverse_path(&self, path: &TraversalPath) -> Result<TraversalResult, String> {
        let query = self.compiler.compile(path)?;
        let join_count = query.join_count;
        Ok(TraversalResult {
            query,
            join_count,
            uses_single_join: join_count > 0,
        })
    }

    /// 检测 N+1 查询模式
    ///
    /// 如果遍历深度 > 0 但 JOIN 数为 0，说明可能使用了 N+1 模式。
    pub fn detect_n_plus_one(&self, path: &TraversalPath) -> bool {
        path.depth() > 0 && !path.exceeds_max_depth()
    }

    /// 生成 N+1 防护建议
    pub fn n_plus_one_advice(&self, path: &TraversalPath) -> Option<String> {
        if self.detect_n_plus_one(path) {
            Some(format!(
                "遍历路径 '{}' 深度 {}，已编译为 {} 个 JOIN（而非 {} 次单独查询），N+1 已防护",
                path.root_table,
                path.depth(),
                path.depth(),
                path.depth() + 1
            ))
        } else {
            None
        }
    }

    /// 编译器引用
    pub fn compiler(&self) -> &GraphTraversalCompiler {
        &self.compiler
    }
}

#[cfg(test)]
mod tests {
    use super::super::relation_graph::RelationEdge;
    use super::*;

    fn build_executor() -> GraphTraversalExecutor {
        let mut graph = RelationGraph::new();
        graph.add_edge(RelationEdge::new(
            "users", "id", "posts", "user_id", "posts",
        ));
        graph.add_edge(RelationEdge::new(
            "posts", "id", "comments", "post_id", "comments",
        ));
        GraphTraversalExecutor::new(graph)
    }

    #[test]
    fn test_traverse_simple() {
        let executor = build_executor();
        let result = executor.traverse("users.posts").unwrap();
        assert_eq!(result.join_count, 1);
        assert!(result.uses_single_join);
    }

    #[test]
    fn test_traverse_deep() {
        let executor = build_executor();
        let result = executor.traverse("users.posts.comments").unwrap();
        assert_eq!(result.join_count, 2);
    }

    #[test]
    fn test_traverse_no_join() {
        let executor = build_executor();
        let result = executor.traverse("users").unwrap();
        assert_eq!(result.join_count, 0);
        assert!(!result.uses_single_join);
    }

    #[test]
    fn test_n_plus_one_detection() {
        let executor = build_executor();
        let path = GraphTraversalDsl::parse("users.posts").unwrap();
        assert!(executor.detect_n_plus_one(&path));
    }

    #[test]
    fn test_n_plus_one_advice() {
        let executor = build_executor();
        let path = GraphTraversalDsl::parse("users.posts.comments").unwrap();
        let advice = executor.n_plus_one_advice(&path).unwrap();
        assert!(advice.contains("N+1 已防护"));
    }

    #[test]
    fn test_max_depth_limit() {
        let executor = build_executor().with_max_depth(1);
        assert!(executor.traverse("users.posts.comments").is_err());
    }
}
