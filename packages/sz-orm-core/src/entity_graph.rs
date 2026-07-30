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

    /// P2-7：检测实体图中的循环引用
    ///
    /// 使用 DFS 遍历图（含嵌套子图），检测是否存在循环路径。
    /// 循环引用会导致递归 eager load 时栈溢出，必须在加载前检测。
    ///
    /// # 算法
    ///
    /// 1. **展平**：递归收集主图 + 所有子图的边到统一邻接表
    /// 2. **三色标记 DFS**：
    ///    - **白色（未访问）**：节点尚未访问
    ///    - **灰色（访问中）**：节点正在当前 DFS 路径中，若再次遇到则发现回边（循环）
    ///    - **黑色（已完成）**：节点及其所有子节点已访问完毕
    ///
    /// # 返回
    ///
    /// - `Ok(())`：无循环引用
    /// - `Err(cycle_path)`：检测到循环，`cycle_path` 是循环路径上的节点列表
    ///   （如 `["user", "posts", "user"]` 表示 user → posts → user 的循环）
    ///
    /// # 示例
    ///
    /// ```
    /// use sz_orm_core::entity_graph::EntityGraph;
    ///
    /// // 无循环：user → posts → comments
    /// let mut graph = EntityGraph::new();
    /// graph.add_edge("user", "posts");
    /// graph.add_edge("posts", "comments");
    /// assert!(graph.detect_cycles().is_ok());
    ///
    /// // 有循环：user → posts → user
    /// let mut graph = EntityGraph::new();
    /// graph.add_edge_with_graph("user", "posts", {
    ///     let mut sub = EntityGraph::new();
    ///     sub.add_edge("posts", "user");
    ///     sub
    /// });
    /// assert!(graph.detect_cycles().is_err());
    /// ```
    pub fn detect_cycles(&self) -> Result<(), Vec<String>> {
        // 1. 展平：收集所有边（含子图）到邻接表
        let mut adj: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        self.collect_edges_recursive(&mut adj);

        // 1.1 对每个节点的邻接列表排序，保证 DFS 遍历顺序确定
        for neighbors in adj.values_mut() {
            neighbors.sort();
        }

        // 2. 三色标记 DFS
        let mut visited = std::collections::HashSet::new();
        let mut visiting = std::collections::HashSet::new();
        let mut path = Vec::new();

        // 2.1 按字典序排序节点，保证 DFS 起点确定
        let mut sorted_nodes: Vec<String> = adj.keys().cloned().collect();
        sorted_nodes.sort();
        for node in &sorted_nodes {
            if !visited.contains(node) {
                dfs_cycle_detect(node, &adj, &mut visited, &mut visiting, &mut path)?;
            }
        }
        Ok(())
    }

    /// P2-7：递归收集所有边到邻接表（含子图）
    ///
    /// 将主图和所有子图的边统一收集到 `adj` 中。
    /// 子图的边也会被加入，因为子图定义了从 `edge.relation` 出发的额外边。
    fn collect_edges_recursive(
        &self,
        adj: &mut std::collections::HashMap<String, Vec<String>>,
    ) {
        for edge in &self.edges {
            adj.entry(edge.parent_field.clone())
                .or_default()
                .push(edge.relation.clone());
            if let Some(sub) = &edge.sub_graph {
                sub.collect_edges_recursive(adj);
            }
        }
    }

    /// P2-7：检测并拒绝重复边（相同 parent_field + relation）
    ///
    /// 重复边不会导致栈溢出，但会产生冗余 SQL 查询，应检测并警告。
    ///
    /// 返回 `Ok(())` 表示无重复；返回 `Err(duplicates)` 表示有重复边。
    pub fn detect_duplicate_edges(&self) -> Result<(), Vec<(String, String)>> {
        let mut seen = std::collections::HashSet::new();
        let mut duplicates = Vec::new();
        for edge in &self.edges {
            let key = (edge.parent_field.clone(), edge.relation.clone());
            if !seen.insert(key.clone()) {
                duplicates.push((edge.parent_field.clone(), edge.relation.clone()));
            }
        }
        if duplicates.is_empty() {
            Ok(())
        } else {
            Err(duplicates)
        }
    }

    /// P2-7：综合校验（循环引用 + 重复边）
    ///
    /// 在 `load_eager` / `load_join` 前调用，确保图结构安全。
    pub fn validate(&self) -> Result<(), String> {
        // 1. 检测循环引用
        if let Err(cycle) = self.detect_cycles() {
            return Err(format!(
                "EntityGraph 循环引用检测失败：{}",
                cycle.join(" → ")
            ));
        }
        // 2. 检测重复边
        if let Err(duplicates) = self.detect_duplicate_edges() {
            let dup_str: Vec<String> = duplicates
                .iter()
                .map(|(p, r)| format!("({}->{})", p, r))
                .collect();
            return Err(format!(
                "EntityGraph 重复边检测失败：{}",
                dup_str.join(", ")
            ));
        }
        Ok(())
    }
}

/// P2-7：DFS 循环检测（独立函数，避免 self 借用问题）
///
/// `node` 是当前访问节点，`adj` 是邻接表，`visiting` 是灰色标记集合，
/// `visited` 是黑色标记集合，`path` 是当前路径。
fn dfs_cycle_detect(
    node: &str,
    adj: &std::collections::HashMap<String, Vec<String>>,
    visited: &mut std::collections::HashSet<String>,
    visiting: &mut std::collections::HashSet<String>,
    path: &mut Vec<String>,
) -> Result<(), Vec<String>> {
    // 灰色节点再次被访问 → 发现回边（循环）
    if visiting.contains(node) {
        let cycle_start = path.iter().position(|n| n == node).unwrap_or(0);
        let mut cycle = path[cycle_start..].to_vec();
        cycle.push(node.to_string());
        return Err(cycle);
    }
    // 黑色节点已完全访问，跳过
    if visited.contains(node) {
        return Ok(());
    }

    // 标记为灰色（访问中）
    visiting.insert(node.to_string());
    path.push(node.to_string());

    // 遍历所有邻接节点
    if let Some(neighbors) = adj.get(node) {
        for neighbor in neighbors {
            dfs_cycle_detect(neighbor, adj, visited, visiting, path)?;
        }
    }

    // 标记为黑色（已完成）
    visiting.remove(node);
    visited.insert(node.to_string());
    path.pop();
    Ok(())
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

        // 1. 从缓存读取（锁毒化时视缓存为空，全部 key 重新加载）
        let mut to_load: Vec<K> = Vec::new();
        if let Ok(cached) = self.cache.read() {
            for k in keys {
                if let Some(v) = cached.get(k) {
                    result.insert(k.clone(), v.clone());
                } else {
                    to_load.push(k.clone());
                }
            }
        } else {
            to_load.extend(keys.iter().cloned());
        }

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

        // 3. 写入缓存（锁毒化时跳过写入，不影响本次返回结果）
        if let Ok(mut cache) = self.cache.write() {
            for (k, v) in &all_loaded {
                cache.insert(k.clone(), v.clone());
            }
        }

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
        if let Ok(mut cache) = self.cache.write() {
            cache.clear();
        }
    }

    /// 返回当前缓存大小
    pub fn cache_size(&self) -> usize {
        match self.cache.read() {
            Ok(g) => g.len(),
            Err(_) => 0,
        }
    }

    /// 返回 batch_size
    pub fn batch_size(&self) -> usize {
        self.batch_size
    }
}

// ============================================================================
// N1QueryDetector — N+1 查询检测器（S-2：SeaORM 对标短板补全）
// ============================================================================

/// N+1 查询检测器
///
/// 通过统计相同表/关系在循环加载场景下的查询次数，识别潜在的 N+1 查询问题。
///
/// # 检测策略
///
/// 1. **次数阈值**：同一 `relation` 在一次"检测窗口"内被查询次数超过
///    `threshold`（默认 5），即判定为 N+1 嫌疑；
/// 2. **检测窗口**：以 `start_window()` / `end_window()` 显式划定窗口，
///    便于在循环外包裹；
/// 3. **批量命中识别**：如果一次 `record_batch_load` 调用就加载了多个 key，
///    视为已通过批量加载规避 N+1，仅记 1 次批量查询；
/// 4. **回调告警**：触发 N+1 时调用可选的 `on_alert` 回调（用于日志/指标上报）。
///
/// # 设计动机
///
/// SeaORM 的 `find_with_related` 仅在显式调用时才会批量加载，缺乏运行时
/// 检测机制。本检测器提供运行时 introspection，可在开发/测试环境启用，
/// 在生产环境关闭（zero-cost 抽象）。
///
/// # 线程安全
///
/// 内部使用 `RwLock<HashMap>` 维护计数，可在线程间共享（`Send + Sync`）。
///
/// # 示例
///
/// ```
/// use sz_orm_core::entity_graph::{N1QueryDetector, N1DetectionConfig};
///
/// let detector = N1QueryDetector::new(N1DetectionConfig::default());
/// detector.start_window();
/// for _ in 0..10 {
///     detector.record_single_load("posts");
/// }
/// detector.end_window();
/// let alerts = detector.alerts();
/// assert_eq!(alerts.len(), 1);
/// assert_eq!(alerts[0].relation, "posts");
/// assert_eq!(alerts[0].query_count, 10);
/// ```
pub struct N1QueryDetector {
    /// 检测配置
    config: N1DetectionConfig,
    /// 当前窗口内各 relation 的单条查询计数
    counts: RwLock<HashMap<String, u64>>,
    /// 当前窗口内各 relation 的批量查询计数（每个 batch 记 1 次）
    batch_counts: RwLock<HashMap<String, u64>>,
    /// 当前窗口是否开启
    window_active: RwLock<bool>,
    /// 历史告警列表（最近一次窗口的结果）
    alerts: RwLock<Vec<N1Alert>>,
}

/// N+1 检测配置
#[derive(Debug, Clone)]
pub struct N1DetectionConfig {
    /// 触发告警的查询次数阈值（同一 relation 在一个窗口内的单条查询次数 ≥ threshold）
    pub threshold: u64,
    /// 是否启用检测（false 时所有 record_* 调用均为 no-op）
    pub enabled: bool,
}

impl Default for N1DetectionConfig {
    fn default() -> Self {
        Self {
            threshold: 5,
            enabled: true,
        }
    }
}

impl N1DetectionConfig {
    /// 创建默认配置（threshold=5, enabled=true）
    pub fn new() -> Self {
        Self::default()
    }

    /// 自定义阈值
    pub fn with_threshold(mut self, threshold: u64) -> Self {
        self.threshold = threshold.max(1);
        self
    }

    /// 启用/禁用
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// N+1 查询告警
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct N1Alert {
    /// 触发告警的 relation 名称
    pub relation: String,
    /// 单条查询次数
    pub query_count: u64,
    /// 批量查询次数（0 表示完全没有使用批量加载）
    pub batch_count: u64,
    /// 配置的阈值
    pub threshold: u64,
}

impl N1Alert {
    /// 是否完全未使用批量加载
    pub fn no_batch_used(&self) -> bool {
        self.batch_count == 0
    }

    /// 建议的批量大小（query_count 向上取整到 10 的幂级，至少 50）
    pub fn suggested_batch_size(&self) -> usize {
        let n = self.query_count as usize;
        if n <= 50 {
            50
        } else if n <= 100 {
            100
        } else if n <= 500 {
            500
        } else {
            1000
        }
    }
}

impl N1QueryDetector {
    /// 创建检测器
    pub fn new(config: N1DetectionConfig) -> Self {
        Self {
            config,
            counts: RwLock::new(HashMap::new()),
            batch_counts: RwLock::new(HashMap::new()),
            window_active: RwLock::new(false),
            alerts: RwLock::new(Vec::new()),
        }
    }

    /// 创建默认配置的检测器
    pub fn with_defaults() -> Self {
        Self::new(N1DetectionConfig::default())
    }

    /// 是否启用检测
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// 当前阈值
    pub fn threshold(&self) -> u64 {
        self.config.threshold
    }

    /// 开启检测窗口（清空旧计数与旧告警）
    ///
    /// 重复调用 `start_window` 会重置窗口。
    pub fn start_window(&self) {
        if !self.config.enabled {
            return;
        }
        if let Ok(mut counts) = self.counts.write() {
            *counts = HashMap::new();
        }
        if let Ok(mut batch_counts) = self.batch_counts.write() {
            *batch_counts = HashMap::new();
        }
        if let Ok(mut alerts) = self.alerts.write() {
            *alerts = Vec::new();
        }
        if let Ok(mut window_active) = self.window_active.write() {
            *window_active = true;
        }
    }

    /// 结束检测窗口，分析并生成告警
    ///
    /// 结束后 `record_*` 调用会被忽略，直到下一次 `start_window`。
    /// 返回本次窗口产生的告警列表。
    pub fn end_window(&self) -> Vec<N1Alert> {
        if !self.config.enabled {
            return Vec::new();
        }
        if let Ok(mut window_active) = self.window_active.write() {
            *window_active = false;
        }

        // 读锁毒化时返回空告警列表（graceful 降级）
        let new_alerts: Vec<N1Alert> =
            match (self.counts.read(), self.batch_counts.read()) {
                (Ok(counts), Ok(batch_counts)) => {
                    let mut alerts: Vec<N1Alert> = counts
                        .iter()
                        .filter_map(|(rel, &cnt)| {
                            if cnt >= self.config.threshold {
                                Some(N1Alert {
                                    relation: rel.clone(),
                                    query_count: cnt,
                                    batch_count: *batch_counts.get(rel).unwrap_or(&0),
                                    threshold: self.config.threshold,
                                })
                            } else {
                                None
                            }
                        })
                        .collect();
                    // 稳定排序便于断言
                    alerts.sort_by(|a, b| a.relation.cmp(&b.relation));
                    alerts
                }
                _ => Vec::new(),
            };

        if let Ok(mut alerts) = self.alerts.write() {
            *alerts = new_alerts.clone();
        }
        new_alerts
    }

    /// 记录一次单条加载（典型的 N+1 来源：循环内 `find_by_id`）
    pub fn record_single_load(&self, relation: &str) {
        if !self.config.enabled {
            return;
        }
        {
            let active = self
                .window_active
                .read()
                .map(|g| *g)
                .unwrap_or(false);
            if !active {
                return;
            }
        }
        if let Ok(mut counts) = self.counts.write() {
            *counts.entry(relation.to_string()).or_insert(0) += 1;
        }
    }

    /// 记录一次批量加载（已规避 N+1 的良好实践）
    ///
    /// - `keys_count`：本次批量加载的 key 数量
    /// - 一个 batch 仅记 1 次批量查询，不论 keys_count 多少
    pub fn record_batch_load(&self, relation: &str, _keys_count: usize) {
        if !self.config.enabled {
            return;
        }
        {
            let active = self
                .window_active
                .read()
                .map(|g| *g)
                .unwrap_or(false);
            if !active {
                return;
            }
        }
        if let Ok(mut batch_counts) = self.batch_counts.write() {
            *batch_counts.entry(relation.to_string()).or_insert(0) += 1;
        }
    }

    /// 获取最近一次 `end_window` 产生的告警（只读副本）
    pub fn alerts(&self) -> Vec<N1Alert> {
        self.alerts
            .read()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    /// 当前窗口内某 relation 的单条查询次数
    pub fn current_count(&self, relation: &str) -> u64 {
        self.counts
            .read()
            .map(|g| g.get(relation).copied().unwrap_or(0))
            .unwrap_or(0)
    }

    /// 当前窗口内某 relation 的批量查询次数
    pub fn current_batch_count(&self, relation: &str) -> u64 {
        self.batch_counts
            .read()
            .map(|g| g.get(relation).copied().unwrap_or(0))
            .unwrap_or(0)
    }

    /// 窗口是否处于开启状态
    pub fn is_window_active(&self) -> bool {
        self.window_active
            .read()
            .map(|g| *g)
            .unwrap_or(false)
    }

    /// 是否已检测到 N+1（基于最近一次窗口的告警）
    pub fn has_n_plus_one(&self) -> bool {
        !self.alerts().is_empty()
    }
}

impl Default for N1QueryDetector {
    fn default() -> Self {
        Self::with_defaults()
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

    // ===== N1QueryDetector 测试（S-2）=====

    #[test]
    fn test_n1_config_default() {
        let cfg = N1DetectionConfig::default();
        assert_eq!(cfg.threshold, 5);
        assert!(cfg.enabled);
    }

    #[test]
    fn test_n1_config_builder() {
        let cfg = N1DetectionConfig::new()
            .with_threshold(10)
            .with_enabled(false);
        assert_eq!(cfg.threshold, 10);
        assert!(!cfg.enabled);

        // threshold < 1 应被钳制为 1
        let cfg2 = N1DetectionConfig::new().with_threshold(0);
        assert_eq!(cfg2.threshold, 1);
    }

    #[test]
    fn test_n1_detector_default() {
        let det = N1QueryDetector::default();
        assert!(det.is_enabled());
        assert_eq!(det.threshold(), 5);
        assert!(!det.is_window_active());
        assert!(!det.has_n_plus_one());
    }

    #[test]
    fn test_n1_detector_disabled_is_noop() {
        let det = N1QueryDetector::new(N1DetectionConfig::new().with_enabled(false));
        det.start_window();
        for _ in 0..100 {
            det.record_single_load("posts");
        }
        // 禁用时计数不应增加
        assert_eq!(det.current_count("posts"), 0);
        let alerts = det.end_window();
        assert!(alerts.is_empty());
    }

    #[test]
    fn test_n1_detector_records_outside_window_ignored() {
        let det = N1QueryDetector::with_defaults();
        // 未开启窗口时记录应被忽略
        det.record_single_load("posts");
        assert_eq!(det.current_count("posts"), 0);
    }

    #[test]
    fn test_n1_detector_below_threshold_no_alert() {
        let det = N1QueryDetector::with_defaults(); // threshold=5
        det.start_window();
        for _ in 0..4 {
            det.record_single_load("posts");
        }
        assert_eq!(det.current_count("posts"), 4);
        let alerts = det.end_window();
        assert!(alerts.is_empty(), "below threshold should not alert");
        assert!(!det.has_n_plus_one());
    }

    #[test]
    fn test_n1_detector_at_threshold_triggers_alert() {
        let det = N1QueryDetector::with_defaults(); // threshold=5
        det.start_window();
        for _ in 0..5 {
            det.record_single_load("posts");
        }
        let alerts = det.end_window();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].relation, "posts");
        assert_eq!(alerts[0].query_count, 5);
        assert_eq!(alerts[0].threshold, 5);
        assert_eq!(alerts[0].batch_count, 0);
        assert!(alerts[0].no_batch_used());
        assert!(det.has_n_plus_one());
    }

    #[test]
    fn test_n1_detector_above_threshold_triggers_alert() {
        let det = N1QueryDetector::with_defaults();
        det.start_window();
        for _ in 0..10 {
            det.record_single_load("posts");
        }
        let alerts = det.end_window();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].query_count, 10);
        // 默认阈值为 5，10 次查询应建议 batch_size >= 50
        assert!(alerts[0].suggested_batch_size() >= 50);
    }

    #[test]
    fn test_n1_detector_multiple_relations() {
        let det = N1QueryDetector::with_defaults();
        det.start_window();
        for _ in 0..6 {
            det.record_single_load("posts");
        }
        for _ in 0..3 {
            det.record_single_load("comments"); // 低于阈值
        }
        for _ in 0..8 {
            det.record_single_load("tags");
        }
        let alerts = det.end_window();
        // 仅 posts 与 tags 应触发告警（comments 低于阈值）
        assert_eq!(alerts.len(), 2);
        // 排序后应为 posts, tags
        assert_eq!(alerts[0].relation, "posts");
        assert_eq!(alerts[0].query_count, 6);
        assert_eq!(alerts[1].relation, "tags");
        assert_eq!(alerts[1].query_count, 8);
    }

    #[test]
    fn test_n1_detector_batch_load_recorded_separately() {
        let det = N1QueryDetector::with_defaults();
        det.start_window();
        // 单条查询 6 次（触发 N+1）
        for _ in 0..6 {
            det.record_single_load("posts");
        }
        // 同时有 2 次批量加载（良好实践）
        det.record_batch_load("posts", 100);
        det.record_batch_load("posts", 50);
        let alerts = det.end_window();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].query_count, 6);
        assert_eq!(alerts[0].batch_count, 2);
        // batch_count != 0 表示已部分使用批量加载
        assert!(!alerts[0].no_batch_used());
    }

    #[test]
    fn test_n1_detector_batch_only_does_not_trigger() {
        // 仅使用批量加载（无单条查询）不应触发告警
        let det = N1QueryDetector::with_defaults();
        det.start_window();
        for _ in 0..100 {
            det.record_batch_load("posts", 50);
        }
        assert_eq!(det.current_batch_count("posts"), 100);
        assert_eq!(det.current_count("posts"), 0);
        let alerts = det.end_window();
        assert!(alerts.is_empty());
    }

    #[test]
    fn test_n1_detector_start_window_resets() {
        let det = N1QueryDetector::with_defaults();
        det.start_window();
        for _ in 0..10 {
            det.record_single_load("posts");
        }
        let _ = det.end_window();
        assert_eq!(det.alerts().len(), 1);

        // 再次开启窗口应清空旧告警与计数
        det.start_window();
        assert_eq!(det.alerts().len(), 0);
        assert_eq!(det.current_count("posts"), 0);
        assert!(det.is_window_active());
    }

    #[test]
    fn test_n1_detector_end_window_deactivates() {
        let det = N1QueryDetector::with_defaults();
        det.start_window();
        assert!(det.is_window_active());
        det.end_window();
        assert!(!det.is_window_active());

        // 结束后 record_* 应被忽略
        det.record_single_load("posts");
        assert_eq!(det.current_count("posts"), 0);
    }

    #[test]
    fn test_n1_detector_custom_threshold() {
        let det = N1QueryDetector::new(N1DetectionConfig::new().with_threshold(100));
        det.start_window();
        for _ in 0..50 {
            det.record_single_load("posts");
        }
        let alerts = det.end_window();
        assert!(alerts.is_empty(), "below custom threshold should not alert");

        det.start_window();
        for _ in 0..100 {
            det.record_single_load("posts");
        }
        let alerts = det.end_window();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].threshold, 100);
        assert_eq!(alerts[0].query_count, 100);
    }

    #[test]
    fn test_n1_alert_suggested_batch_size() {
        let mk = |cnt: u64| N1Alert {
            relation: "x".into(),
            query_count: cnt,
            batch_count: 0,
            threshold: 5,
        };
        assert_eq!(mk(5).suggested_batch_size(), 50);
        assert_eq!(mk(50).suggested_batch_size(), 50);
        assert_eq!(mk(51).suggested_batch_size(), 100);
        assert_eq!(mk(100).suggested_batch_size(), 100);
        assert_eq!(mk(101).suggested_batch_size(), 500);
        assert_eq!(mk(500).suggested_batch_size(), 500);
        assert_eq!(mk(501).suggested_batch_size(), 1000);
        assert_eq!(mk(10000).suggested_batch_size(), 1000);
    }

    #[test]
    fn test_n1_detector_real_n_plus_one_scenario() {
        // 模拟真实场景：循环内查询用户 posts，触发 N+1
        let det = N1QueryDetector::with_defaults();
        det.start_window();
        let user_ids: Vec<i64> = (1..=20).collect();
        for _uid in &user_ids {
            // 每个用户都单条查询 posts —— 典型 N+1
            det.record_single_load("posts");
        }
        let alerts = det.end_window();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].query_count, 20);
        assert!(alerts[0].no_batch_used());

        // 对比：使用 BatchLoader 后的批量加载场景
        det.start_window();
        det.record_batch_load("posts", 20); // 一次性批量加载 20 个用户的 posts
        let alerts2 = det.end_window();
        assert!(alerts2.is_empty(), "batch loading should not trigger N+1");
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
