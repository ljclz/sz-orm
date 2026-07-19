//! Entity Graph + @BatchSize 批量抓取
//!
//! 对应文档 6.8 节改进项 27（Entity Graph）+ 28（@BatchSize 批量抓取）。
//!
//! # 核心概念
//!
//! - **EntityGraph**：实体图，定义一组关联关系一起抓取，避免 N+1 查询
//! - **BatchSizeConfig**：批量抓取配置（大小 + 策略）
//! - **BatchLoader**：通用批量加载器，将 N 次单条查询合并为 ⌈N/batch_size⌉ 次批量查询
//! - **BatchStrategy**：批量策略（IN / JOIN / SUBQUERY）
//!
//! # 设计灵感
//!
//! - Hibernate `@NamedEntityGraph` / `@BatchSize`
//! - Doctrine `FetchMode::EAGER` / partial
//! - Django `select_related` / `prefetch_related`
//! - Sequelize `include` + `separate: true`
//!
//! # 使用示例
//!
//! ```
//! use sz_orm_core::entity_graph::{EntityGraph, BatchLoader, BatchSizeConfig, BatchStrategy};
//! use std::collections::HashMap;
//!
//! // 1. 定义 EntityGraph：抓取 user.posts.comments
//! let mut graph = EntityGraph::new();
//! graph.add_edge("user", "posts");
//! graph.add_edge_with_graph("posts", "comments", {
//!     let mut sub = EntityGraph::new();
//!     sub.add_edge("comments", "author");
//!     sub
//! });
//!
//! // 2. 使用 BatchLoader 批量加载用户
//! fn load_users(ids: &[i64]) -> HashMap<i64, String> {
//!     ids.iter().map(|id| (*id, format!("user_{}", id))).collect()
//! }
//!
//! let loader = BatchLoader::new(100, Box::new(load_users));
//! let users = loader.load_many(&[1, 2, 3, 4, 5]);
//! assert_eq!(users.len(), 5);
//! assert_eq!(users.get(&3), Some(&"user_3".to_string()));
//! ```

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::RwLock;

// ============================================================================
// EntityGraph — 实体图
// ============================================================================

/// 实体图边（关联关系）
#[derive(Debug, Clone)]
pub struct GraphEdge {
    /// 父字段名
    pub parent_field: String,
    /// 关联名（如 "posts"、"comments"）
    pub relation: String,
    /// 嵌套子图（用于递归抓取）
    pub sub_graph: Option<Box<EntityGraph>>,
}

/// 实体图
///
/// 描述一组关联关系的抓取计划。
///
/// # 示例
///
/// ```
/// use sz_orm_core::entity_graph::EntityGraph;
///
/// let mut graph = EntityGraph::new();
/// graph.add_edge("user", "profile");
/// graph.add_edge("user", "posts");
/// ```
#[derive(Debug, Clone, Default)]
pub struct EntityGraph {
    /// 边列表
    edges: Vec<GraphEdge>,
}

impl EntityGraph {
    /// 创建空实体图
    pub fn new() -> Self {
        Self { edges: Vec::new() }
    }

    /// 添加一条边
    pub fn add_edge(
        &mut self,
        parent_field: impl Into<String>,
        relation: impl Into<String>,
    ) -> &mut Self {
        self.edges.push(GraphEdge {
            parent_field: parent_field.into(),
            relation: relation.into(),
            sub_graph: None,
        });
        self
    }

    /// 添加一条带子图的边（嵌套抓取）
    pub fn add_edge_with_graph(
        &mut self,
        parent_field: impl Into<String>,
        relation: impl Into<String>,
        sub_graph: EntityGraph,
    ) -> &mut Self {
        self.edges.push(GraphEdge {
            parent_field: parent_field.into(),
            relation: relation.into(),
            sub_graph: Some(Box::new(sub_graph)),
        });
        self
    }

    /// 返回所有边
    pub fn edges(&self) -> &[GraphEdge] {
        &self.edges
    }

    /// 返回边的数量
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// 查询某个父字段的所有关联
    pub fn relations_of(&self, parent_field: &str) -> Vec<&GraphEdge> {
        self.edges
            .iter()
            .filter(|e| e.parent_field == parent_field)
            .collect()
    }

    /// 收集图中所有关联名（去重）
    pub fn all_relations(&self) -> Vec<String> {
        let mut rels: Vec<String> = self.edges.iter().map(|e| e.relation.clone()).collect();
        rels.sort();
        rels.dedup();
        rels
    }

    /// 收集图中所有父字段（去重）
    pub fn all_parent_fields(&self) -> Vec<String> {
        let mut fields: Vec<String> = self.edges.iter().map(|e| e.parent_field.clone()).collect();
        fields.sort();
        fields.dedup();
        fields
    }

    /// 是否为空图
    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }

    /// 递归收集图中所有关联（含子图）
    pub fn all_relations_recursive(&self) -> Vec<String> {
        let mut result = Vec::new();
        for edge in &self.edges {
            result.push(edge.relation.clone());
            if let Some(sub) = &edge.sub_graph {
                result.extend(sub.all_relations_recursive());
            }
        }
        result.sort();
        result.dedup();
        result
    }
}

// ============================================================================
// BatchStrategy — 批量策略
// ============================================================================

/// 批量抓取策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BatchStrategy {
    /// 使用 `WHERE id IN (?, ?, ...)` 子句批量加载
    ///
    /// 适用场景：关联数量较少、目标表无索引时的备选方案
    #[default]
    In,
    /// 使用 `LEFT JOIN` 一次性加载所有关联
    ///
    /// 适用场景：关联数量较少、需要原子性读取
    Join,
    /// 使用 `WHERE id IN (SELECT ... FROM ...)` 子查询批量加载
    ///
    /// 适用场景：子查询可被数据库优化器优化时
    Subquery,
}

impl BatchStrategy {
    /// 策略名称
    pub fn name(&self) -> &'static str {
        match self {
            BatchStrategy::In => "in",
            BatchStrategy::Join => "join",
            BatchStrategy::Subquery => "subquery",
        }
    }

    /// 生成 IN 子句的 SQL 片段
    ///
    /// 返回形如 `"id IN (?, ?, ?)"` 的字符串（占位符数量与 values 一致）。
    pub fn render_in_clause(column: &str, placeholders: usize) -> String {
        if placeholders == 0 {
            return format!("{} IN ()", column);
        }
        let marks: Vec<&str> = vec!["?"; placeholders];
        format!("{} IN ({})", column, marks.join(", "))
    }
}

// ============================================================================
// BatchSizeConfig — 批量大小配置
// ============================================================================

/// 批量大小配置
///
/// 对应 Hibernate `@BatchSize(size = 100)` 注解。
#[derive(Debug, Clone, Copy)]
pub struct BatchSizeConfig {
    /// 每批数量
    pub size: usize,
    /// 抓取策略
    pub strategy: BatchStrategy,
}

impl Default for BatchSizeConfig {
    fn default() -> Self {
        Self {
            size: 100,
            strategy: BatchStrategy::In,
        }
    }
}

impl BatchSizeConfig {
    /// 创建配置
    pub fn new(size: usize, strategy: BatchStrategy) -> Self {
        Self { size, strategy }
    }

    /// 创建默认策略的配置（IN）
    pub fn with_size(size: usize) -> Self {
        Self {
            size,
            strategy: BatchStrategy::In,
        }
    }

    /// 计算给定总数需要分多少批
    ///
    /// # 示例
    ///
    /// ```
    /// use sz_orm_core::entity_graph::BatchSizeConfig;
    ///
    /// let config = BatchSizeConfig::with_size(100);
    /// assert_eq!(config.batch_count(0), 0);
    /// assert_eq!(config.batch_count(1), 1);
    /// assert_eq!(config.batch_count(100), 1);
    /// assert_eq!(config.batch_count(101), 2);
    /// assert_eq!(config.batch_count(250), 3);
    /// ```
    pub fn batch_count(&self, total: usize) -> usize {
        if total == 0 {
            0
        } else {
            total.div_ceil(self.size)
        }
    }

    /// 返回第 `batch_index` 批的范围（start..end，end 不超过 total）
    ///
    /// # 示例
    ///
    /// ```
    /// use sz_orm_core::entity_graph::BatchSizeConfig;
    ///
    /// let config = BatchSizeConfig::with_size(100);
    /// assert_eq!(config.batch_range(0, 250), 0..100);
    /// assert_eq!(config.batch_range(1, 250), 100..200);
    /// assert_eq!(config.batch_range(2, 250), 200..250);
    /// ```
    pub fn batch_range(&self, batch_index: usize, total: usize) -> std::ops::Range<usize> {
        let start = batch_index * self.size;
        let end = (start + self.size).min(total);
        start..end
    }
}

// ============================================================================
// BatchLoader — 通用批量加载器
// ============================================================================

/// 批量加载函数类型
pub type BatchLoaderFn<K, V> = Box<dyn Fn(&[K]) -> HashMap<K, V> + Send + Sync>;

/// 批量加载器
///
/// 将 N 个单条加载请求合并为 ⌈N/batch_size⌉ 次批量加载，避免 N+1 查询问题。
///
/// # 泛型参数
///
/// - `K`：主键类型（必须实现 `Hash + Eq + Clone`）
/// - `V`：值类型
///
/// # 示例
///
/// ```
/// use sz_orm_core::entity_graph::BatchLoader;
/// use std::collections::HashMap;
///
/// fn load_users(ids: &[i64]) -> HashMap<i64, String> {
///     ids.iter().map(|id| (*id, format!("user_{}", id))).collect()
/// }
///
/// let loader = BatchLoader::new(100, Box::new(load_users));
/// let users = loader.load_many(&[1, 2, 3]);
/// assert_eq!(users.len(), 3);
/// ```
pub struct BatchLoader<K, V>
where
    K: Hash + Eq + Clone + Send + Sync,
    V: Clone + Send + Sync,
{
    /// 每批数量
    batch_size: usize,
    /// 实际加载函数
    loader: BatchLoaderFn<K, V>,
    /// 缓存（避免重复加载相同的 key）
    cache: RwLock<HashMap<K, V>>,
}

impl<K, V> BatchLoader<K, V>
where
    K: Hash + Eq + Clone + Send + Sync,
    V: Clone + Send + Sync,
{
    /// 创建批量加载器
    ///
    /// # 参数
    /// - `batch_size`：每批数量
    /// - `loader`：实际加载函数，接收一批 key，返回 key→value 的 HashMap
    pub fn new(batch_size: usize, loader: BatchLoaderFn<K, V>) -> Self {
        Self {
            batch_size,
            loader,
            cache: RwLock::new(HashMap::new()),
        }
    }

    /// 批量加载多个 key
    ///
    /// - 自动跳过缓存中已有的 key
    /// - 按 batch_size 分批调用 loader
    /// - 返回所有 key 对应的 value（包含缓存与新加载的）
    pub fn load_many(&self, keys: &[K]) -> HashMap<K, V> {
        let mut result: HashMap<K, V> = HashMap::new();

        // 1. 从缓存读取
        let cached = self.cache.read().unwrap();
        let mut to_load: Vec<K> = Vec::new();
        for k in keys {
            if let Some(v) = cached.get(k) {
                result.insert(k.clone(), v.clone());
            } else {
                to_load.push(k.clone());
            }
        }
        drop(cached);

        if to_load.is_empty() {
            return result;
        }

        // 2. 分批加载
        let batch_size = self.batch_size.max(1);
        let mut all_loaded: HashMap<K, V> = HashMap::new();
        for chunk in to_load.chunks(batch_size) {
            let loaded = (self.loader)(chunk);
            all_loaded.extend(loaded);
        }

        // 3. 写入缓存
        let mut cache = self.cache.write().unwrap();
        for (k, v) in &all_loaded {
            cache.insert(k.clone(), v.clone());
        }
        drop(cache);

        // 4. 合并结果
        result.extend(all_loaded);
        result
    }

    /// 加载单个 key（便捷方法）
    pub fn load_one(&self, key: &K) -> Option<V> {
        let result = self.load_many(std::slice::from_ref(key));
        result.get(key).cloned()
    }

    /// 清空缓存
    pub fn clear_cache(&self) {
        self.cache.write().unwrap().clear();
    }

    /// 返回当前缓存大小
    pub fn cache_size(&self) -> usize {
        self.cache.read().unwrap().len()
    }

    /// 返回 batch_size
    pub fn batch_size(&self) -> usize {
        self.batch_size
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ===== EntityGraph 测试 =====

    #[test]
    fn test_new_graph_is_empty() {
        let g = EntityGraph::new();
        assert!(g.is_empty());
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn test_add_edge() {
        let mut g = EntityGraph::new();
        g.add_edge("user", "posts");
        assert_eq!(g.edge_count(), 1);
        assert!(!g.is_empty());
    }

    #[test]
    fn test_add_multiple_edges() {
        let mut g = EntityGraph::new();
        g.add_edge("user", "posts")
            .add_edge("user", "profile")
            .add_edge("user", "comments");
        assert_eq!(g.edge_count(), 3);
    }

    #[test]
    fn test_add_edge_with_sub_graph() {
        let mut sub = EntityGraph::new();
        sub.add_edge("comments", "author");

        let mut g = EntityGraph::new();
        g.add_edge_with_graph("user", "posts", sub);

        assert_eq!(g.edge_count(), 1);
        assert!(g.edges()[0].sub_graph.is_some());
        assert_eq!(g.edges()[0].sub_graph.as_ref().unwrap().edge_count(), 1);
    }

    #[test]
    fn test_relations_of() {
        let mut g = EntityGraph::new();
        g.add_edge("user", "posts")
            .add_edge("user", "profile")
            .add_edge("post", "comments");

        let user_relations = g.relations_of("user");
        assert_eq!(user_relations.len(), 2);
        assert_eq!(user_relations[0].relation, "posts");
        assert_eq!(user_relations[1].relation, "profile");

        let post_relations = g.relations_of("post");
        assert_eq!(post_relations.len(), 1);

        let none = g.relations_of("nonexistent");
        assert!(none.is_empty());
    }

    #[test]
    fn test_all_relations() {
        let mut g = EntityGraph::new();
        g.add_edge("user", "posts")
            .add_edge("user", "profile")
            .add_edge("post", "comments");

        let rels = g.all_relations();
        assert_eq!(rels, vec!["comments", "posts", "profile"]);
    }

    #[test]
    fn test_all_parent_fields() {
        let mut g = EntityGraph::new();
        g.add_edge("user", "posts")
            .add_edge("user", "profile")
            .add_edge("post", "comments");

        let fields = g.all_parent_fields();
        assert_eq!(fields, vec!["post", "user"]);
    }

    #[test]
    fn test_all_relations_recursive() {
        let mut sub = EntityGraph::new();
        sub.add_edge("comments", "author")
            .add_edge("comments", "likes");

        let mut g = EntityGraph::new();
        g.add_edge("user", "posts")
            .add_edge_with_graph("user", "comments", sub);

        let all = g.all_relations_recursive();
        assert!(all.contains(&"posts".to_string()));
        assert!(all.contains(&"comments".to_string()));
        assert!(all.contains(&"author".to_string()));
        assert!(all.contains(&"likes".to_string()));
        assert_eq!(all.len(), 4);
    }

    #[test]
    fn test_default_graph_is_empty() {
        let g = EntityGraph::default();
        assert!(g.is_empty());
    }

    // ===== BatchStrategy 测试 =====

    #[test]
    fn test_strategy_name() {
        assert_eq!(BatchStrategy::In.name(), "in");
        assert_eq!(BatchStrategy::Join.name(), "join");
        assert_eq!(BatchStrategy::Subquery.name(), "subquery");
    }

    #[test]
    fn test_strategy_default_is_in() {
        assert_eq!(BatchStrategy::default(), BatchStrategy::In);
    }

    #[test]
    fn test_render_in_clause_empty() {
        let sql = BatchStrategy::render_in_clause("id", 0);
        assert_eq!(sql, "id IN ()");
    }

    #[test]
    fn test_render_in_clause_single() {
        let sql = BatchStrategy::render_in_clause("id", 1);
        assert_eq!(sql, "id IN (?)");
    }

    #[test]
    fn test_render_in_clause_multiple() {
        let sql = BatchStrategy::render_in_clause("user_id", 3);
        assert_eq!(sql, "user_id IN (?, ?, ?)");
    }

    // ===== BatchSizeConfig 测试 =====

    #[test]
    fn test_default_config() {
        let config = BatchSizeConfig::default();
        assert_eq!(config.size, 100);
        assert_eq!(config.strategy, BatchStrategy::In);
    }

    #[test]
    fn test_with_size() {
        let config = BatchSizeConfig::with_size(50);
        assert_eq!(config.size, 50);
        assert_eq!(config.strategy, BatchStrategy::In);
    }

    #[test]
    fn test_new_with_strategy() {
        let config = BatchSizeConfig::new(200, BatchStrategy::Join);
        assert_eq!(config.size, 200);
        assert_eq!(config.strategy, BatchStrategy::Join);
    }

    #[test]
    fn test_batch_count_zero() {
        let config = BatchSizeConfig::with_size(100);
        assert_eq!(config.batch_count(0), 0);
    }

    #[test]
    fn test_batch_count_exact_multiple() {
        let config = BatchSizeConfig::with_size(100);
        assert_eq!(config.batch_count(100), 1);
        assert_eq!(config.batch_count(200), 2);
        assert_eq!(config.batch_count(500), 5);
    }

    #[test]
    fn test_batch_count_with_remainder() {
        let config = BatchSizeConfig::with_size(100);
        assert_eq!(config.batch_count(1), 1);
        assert_eq!(config.batch_count(99), 1);
        assert_eq!(config.batch_count(101), 2);
        assert_eq!(config.batch_count(150), 2);
        assert_eq!(config.batch_count(201), 3);
    }

    #[test]
    fn test_batch_range() {
        let config = BatchSizeConfig::with_size(100);

        assert_eq!(config.batch_range(0, 250), 0..100);
        assert_eq!(config.batch_range(1, 250), 100..200);
        assert_eq!(config.batch_range(2, 250), 200..250);
    }

    #[test]
    fn test_batch_range_exact() {
        let config = BatchSizeConfig::with_size(100);

        assert_eq!(config.batch_range(0, 100), 0..100);
        assert_eq!(config.batch_range(1, 100), 100..100); // 空范围
    }

    #[test]
    fn test_batch_range_small_batch() {
        let config = BatchSizeConfig::with_size(10);

        assert_eq!(config.batch_range(0, 25), 0..10);
        assert_eq!(config.batch_range(1, 25), 10..20);
        assert_eq!(config.batch_range(2, 25), 20..25);
    }

    // ===== BatchLoader 测试 =====

    fn make_loader() -> BatchLoader<i64, String> {
        let loader = Box::new(|ids: &[i64]| -> HashMap<i64, String> {
            ids.iter().map(|id| (*id, format!("user_{}", id))).collect()
        });
        BatchLoader::new(2, loader)
    }

    #[test]
    fn test_batch_loader_load_many_single_batch() {
        let loader = make_loader();
        let result = loader.load_many(&[1, 2]);
        assert_eq!(result.len(), 2);
        assert_eq!(result.get(&1), Some(&"user_1".to_string()));
        assert_eq!(result.get(&2), Some(&"user_2".to_string()));
    }

    #[test]
    fn test_batch_loader_load_many_multiple_batches() {
        let loader = make_loader();
        // batch_size=2, 5 keys → 3 batches
        let result = loader.load_many(&[1, 2, 3, 4, 5]);
        assert_eq!(result.len(), 5);
        for id in 1..=5 {
            assert_eq!(
                result.get(&id),
                Some(&format!("user_{}", id)),
                "missing user {}",
                id
            );
        }
    }

    #[test]
    fn test_batch_loader_load_one() {
        let loader = make_loader();
        let result = loader.load_one(&42);
        assert_eq!(result, Some("user_42".to_string()));
    }

    #[test]
    fn test_batch_loader_load_one_missing() {
        // loader 返回的 map 没有 key 100
        let loader: BatchLoader<i64, String> =
            BatchLoader::new(10, Box::new(|_ids: &[i64]| HashMap::new()));
        let result = loader.load_one(&100);
        assert_eq!(result, None);
    }

    #[test]
    fn test_batch_loader_caches_results() {
        let call_count = std::sync::Arc::new(std::sync::Mutex::new(0u32));
        let call_count_clone = call_count.clone();

        let loader = Box::new(move |ids: &[i64]| -> HashMap<i64, String> {
            *call_count_clone.lock().unwrap() += 1;
            ids.iter().map(|id| (*id, format!("user_{}", id))).collect()
        });

        let batch_loader = BatchLoader::new(100, loader);

        // 第一次加载
        batch_loader.load_many(&[1, 2, 3]);
        assert_eq!(*call_count.lock().unwrap(), 1);

        // 第二次加载相同 key，应命中缓存
        batch_loader.load_many(&[1, 2, 3]);
        assert_eq!(*call_count.lock().unwrap(), 1); // 未增加

        // 加载新 key，应触发新的 loader 调用
        batch_loader.load_many(&[4, 5]);
        assert_eq!(*call_count.lock().unwrap(), 2);
    }

    #[test]
    fn test_batch_loader_partial_cache_hit() {
        let call_count = std::sync::Arc::new(std::sync::Mutex::new(0u32));
        let call_count_clone = call_count.clone();

        let loader = Box::new(move |ids: &[i64]| -> HashMap<i64, String> {
            *call_count_clone.lock().unwrap() += 1;
            ids.iter().map(|id| (*id, format!("user_{}", id))).collect()
        });

        let batch_loader = BatchLoader::new(100, loader);

        // 加载 1, 2, 3
        batch_loader.load_many(&[1, 2, 3]);
        assert_eq!(*call_count.lock().unwrap(), 1);

        // 加载 1, 2, 3, 4, 5（前 3 个命中缓存）
        let result = batch_loader.load_many(&[1, 2, 3, 4, 5]);
        assert_eq!(result.len(), 5);
        assert_eq!(*call_count.lock().unwrap(), 2); // 只为 4, 5 调用一次

        // 缓存大小应为 5
        assert_eq!(batch_loader.cache_size(), 5);
    }

    #[test]
    fn test_batch_loader_clear_cache() {
        let loader = make_loader();
        loader.load_many(&[1, 2]);
        assert_eq!(loader.cache_size(), 2);

        loader.clear_cache();
        assert_eq!(loader.cache_size(), 0);
    }

    #[test]
    fn test_batch_loader_empty_input() {
        let loader = make_loader();
        let result = loader.load_many(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_batch_loader_batch_size_attribute() {
        let loader = make_loader();
        assert_eq!(loader.batch_size(), 2);
    }

    #[test]
    fn test_batch_loader_with_size_1() {
        let loader = BatchLoader::new(
            1,
            Box::new(|ids: &[i64]| ids.iter().map(|id| (*id, *id * 10)).collect()),
        );
        let result = loader.load_many(&[1, 2, 3]);
        assert_eq!(result.len(), 3);
        assert_eq!(result.get(&1), Some(&10));
        assert_eq!(result.get(&2), Some(&20));
        assert_eq!(result.get(&3), Some(&30));
    }

    // ===== 集成场景测试 =====

    #[test]
    fn test_workflow_graph_and_batch_loader() {
        // 模拟 User → Posts → Comments 的批量加载场景
        let mut graph = EntityGraph::new();
        graph.add_edge_with_graph("user", "posts", {
            let mut sub = EntityGraph::new();
            sub.add_edge("posts", "comments");
            sub
        });
        assert_eq!(graph.all_relations_recursive().len(), 2);

        // 模拟批量加载用户
        let user_loader = BatchLoader::new(
            50,
            Box::new(|ids: &[i64]| ids.iter().map(|id| (*id, format!("User#{}", id))).collect()),
        );

        // 加载 123 个用户（应分 3 批）
        let user_ids: Vec<i64> = (1..=123).collect();
        let users = user_loader.load_many(&user_ids);
        assert_eq!(users.len(), 123);
        assert_eq!(user_loader.cache_size(), 123);
    }

    #[test]
    fn test_n_plus_1_problem_solved() {
        // 经典 N+1 问题演示：
        // - 错误做法：N 个用户各发 1 次查询加载 posts → N+1 次查询
        // - 正确做法：用 BatchLoader 一次批量加载 → ⌈N/batch⌉+1 次查询

        let query_count = std::sync::Arc::new(std::sync::Mutex::new(0u32));
        let query_count_clone = query_count.clone();

        let post_loader = BatchLoader::new(
            100,
            Box::new(move |user_ids: &[i64]| {
                *query_count_clone.lock().unwrap() += 1;
                // 模拟为每个 user_id 返回 posts
                user_ids
                    .iter()
                    .map(|uid| (*uid, format!("posts_for_user_{}", uid)))
                    .collect()
            }),
        );

        // 250 个用户
        let user_ids: Vec<i64> = (1..=250).collect();
        let _posts = post_loader.load_many(&user_ids);

        // 应分 3 批（100+100+50），调用 loader 3 次
        assert_eq!(*query_count.lock().unwrap(), 3);
    }
}
