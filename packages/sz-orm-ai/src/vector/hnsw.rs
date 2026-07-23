//! HNSW（Hierarchical Navigable Small World）向量索引
//!
//! 提供近似最近邻搜索（ANN）能力，相比暴力搜索在大规模数据集上有显著性能优势。
//!
//! # 算法概述
//!
//! HNSW 是一种基于多层图的近似最近邻搜索算法：
//! - 每个节点以概率 `1/ML^level` 出现在第 `level` 层（ML 为层级因子，通常为 `ln(2)`）
//! - 顶层节点稀疏，底层包含全部节点
//! - 搜索时从顶层开始贪心搜索，逐层下降到第 0 层
//! - 在第 0 层进行 `ef` 次扩展的束搜索，返回 top-k 结果
//!
//! # 特性
//!
//! - 支持 Cosine / Euclidean / DotProduct 三种距离度量
//! - 支持插入、删除（标记删除）、搜索
//! - 纯内存实现，无外部依赖
//! - 线程安全（内部使用 `RwLock`）
//!
//! # 示例
//!
//! ```ignore
//! use sz_orm_ai::vector::hnsw::{HnswIndex, HnswConfig, VectorMetric};
//!
//! let mut index = HnswIndex::new(HnswConfig::default().with_metric(VectorMetric::Cosine));
//! index.insert("v1", vec![1.0, 0.0, 0.0]).unwrap();
//! index.insert("v2", vec![0.0, 1.0, 0.0]).unwrap();
//! index.insert("v3", vec![1.0, 1.0, 0.0]).unwrap();
//!
//! let results = index.search(&[1.0, 0.0, 0.0], 2, 50).unwrap();
//! assert_eq!(results.len(), 2);
//! assert_eq!(results[0].id, "v1");
//! ```

#![allow(dead_code)]

use crate::error::AiError;
use crate::vector::VectorMetric;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::RwLock;

// ---------------------------------------------------------------------------
// 配置
// ---------------------------------------------------------------------------

/// HNSW 索引配置
#[derive(Debug, Clone)]
pub struct HnswConfig {
    /// 每个节点的最大邻居数（M 参数）
    /// 第 0 层的实际最大邻居数为 `2 * M`
    pub max_connections: usize,
    /// 搜索时的候选列表大小（efConstruction / efSearch）
    pub ef_construction: usize,
    /// 层级因子（level generation factor），通常为 `ln(2)`
    pub level_factor: f64,
    /// 距离度量
    pub metric: VectorMetric,
    /// 随机种子（用于可复现的层级生成）
    pub seed: u64,
}

impl Default for HnswConfig {
    fn default() -> Self {
        Self {
            max_connections: 16,
            ef_construction: 200,
            level_factor: std::f64::consts::LN_2,
            metric: VectorMetric::Cosine,
            seed: 42,
        }
    }
}

impl HnswConfig {
    /// 创建配置并指定度量方式
    pub fn with_metric(mut self, metric: VectorMetric) -> Self {
        self.metric = metric;
        self
    }

    /// 设置最大邻居数
    pub fn with_max_connections(mut self, m: usize) -> Self {
        self.max_connections = m;
        self
    }

    /// 设置构建时候选列表大小
    pub fn with_ef_construction(mut self, ef: usize) -> Self {
        self.ef_construction = ef;
        self
    }

    /// 设置随机种子
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }
}

// ---------------------------------------------------------------------------
// 节点与搜索结果
// ---------------------------------------------------------------------------

/// 索引中的向量节点
#[derive(Debug, Clone)]
struct Node {
    /// 节点 ID
    id: String,
    /// 向量数据
    vector: Vec<f32>,
    /// 每层的邻居列表（layer 0 是底层，包含全部节点）
    neighbors: Vec<Vec<usize>>,
    /// 是否被标记删除（软删除）
    deleted: bool,
}

/// 搜索结果
#[derive(Debug, Clone)]
pub struct HnswSearchResult {
    pub id: String,
    pub score: f32,
    pub vector: Vec<f32>,
}

impl HnswSearchResult {
    pub fn new(id: impl Into<String>, score: f32, vector: Vec<f32>) -> Self {
        Self {
            id: id.into(),
            score,
            vector,
        }
    }
}

// ---------------------------------------------------------------------------
// 内部辅助：带距离的候选（用于 BinaryHeap，最大堆）
// ---------------------------------------------------------------------------

/// 候选节点：距离越大优先级越低（用于最大堆，先出队的是距离最大的）
#[derive(Debug, Clone, Copy)]
struct Candidate {
    distance: f32,
    node_idx: usize,
}

impl PartialEq for Candidate {
    fn eq(&self, other: &Self) -> bool {
        self.distance == other.distance && self.node_idx == other.node_idx
    }
}

impl Eq for Candidate {}

impl PartialOrd for Candidate {
    /// 最大堆：距离更大者优先
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.distance.partial_cmp(&other.distance).unwrap_or(std::cmp::Ordering::Equal)
    }
}

// ---------------------------------------------------------------------------
// 简单的确定性随机数生成器（xorshift64）
// ---------------------------------------------------------------------------

struct Rng {
    state: u64,
}

impl Rng {
    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0xdeadbeef } else { seed },
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    /// 生成 [0, 1) 范围的浮点数
    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

// ---------------------------------------------------------------------------
// HNSW 索引
// ---------------------------------------------------------------------------

/// HNSW 向量索引
///
/// 线程安全的多层图向量索引，支持近似最近邻搜索。
pub struct HnswIndex {
    /// 索引配置
    config: HnswConfig,
    /// 所有节点（按插入顺序索引）
    nodes: RwLock<Vec<Node>>,
    /// ID 到节点索引的映射
    id_to_idx: RwLock<HashMap<String, usize>>,
    /// 当前最大层级
    max_level: RwLock<usize>,
    /// 入口节点索引（顶层入口）
    entry_point: RwLock<Option<usize>>,
    /// 随机数生成器（用于层级生成）
    rng: RwLock<Rng>,
    /// 向量维度
    dimension: RwLock<usize>,
}

impl HnswIndex {
    /// 创建新的 HNSW 索引
    pub fn new(config: HnswConfig) -> Self {
        Self {
            config,
            nodes: RwLock::new(Vec::new()),
            id_to_idx: RwLock::new(HashMap::new()),
            max_level: RwLock::new(0),
            entry_point: RwLock::new(None),
            rng: RwLock::new(Rng::new(42)),
            dimension: RwLock::new(0),
        }
    }

    /// 获取当前索引的节点数量（不含已删除）
    pub fn len(&self) -> usize {
        let nodes = self.nodes.read().unwrap();
        nodes.iter().filter(|n| !n.deleted).count()
    }

    /// 索引是否为空
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 获取已配置的维度
    pub fn dimension(&self) -> usize {
        *self.dimension.read().unwrap()
    }

    /// 计算两个向量之间的距离（根据配置的度量方式）
    fn distance(metric: VectorMetric, a: &[f32], b: &[f32]) -> f32 {
        match metric {
            VectorMetric::Cosine => {
                // 距离 = 1 - 相似度
                let sim = cosine_similarity(a, b);
                1.0 - sim
            }
            VectorMetric::Euclidean => {
                // 欧氏距离
                a.iter()
                    .zip(b.iter())
                    .map(|(x, y)| (x - y) * (x - y))
                    .sum::<f32>()
                    .sqrt()
            }
            VectorMetric::DotProduct => {
                // 负点积（越大越远）
                -a.iter().zip(b.iter()).map(|(x, y)| x * y).sum::<f32>()
            }
        }
    }

    /// 为新节点生成随机层级
    fn generate_level(&self) -> usize {
        let mut rng = self.rng.write().unwrap();
        let factor = self.config.level_factor;
        if factor <= 0.0 {
            return 0;
        }
        let r = rng.next_f64();
        -(r.ln() / factor) as usize
    }

    /// 插入一个向量到索引中
    ///
    /// # 参数
    /// - `id`: 节点 ID（若已存在则更新向量）
    /// - `vector`: 向量数据
    pub fn insert(&self, id: &str, vector: Vec<f32>) -> Result<(), AiError> {
        if vector.is_empty() {
            return Err(AiError::Vector("向量不能为空".to_string()));
        }

        // 设置维度（首次插入时）
        {
            let mut dim = self.dimension.write().unwrap();
            if *dim == 0 {
                *dim = vector.len();
            } else if *dim != vector.len() {
                return Err(AiError::Vector(format!(
                    "维度不匹配: 期望 {}, 实际 {}",
                    *dim,
                    vector.len()
                )));
            }
        }

        let level = self.generate_level();
        let new_node_idx;

        // 检查是否已存在同 ID 的节点
        {
            let id_map = self.id_to_idx.read().unwrap();
            if let Some(&existing_idx) = id_map.get(id) {
                // 更新现有节点
                let mut nodes = self.nodes.write().unwrap();
                let node = &mut nodes[existing_idx];
                node.vector = vector.clone();
                node.deleted = false;
                return Ok(());
            }
        }

        // 创建新节点
        {
            let mut nodes = self.nodes.write().unwrap();
            new_node_idx = nodes.len();
            let neighbors = vec![Vec::new(); level + 1];
            nodes.push(Node {
                id: id.to_string(),
                vector: vector.clone(),
                neighbors,
                deleted: false,
            });
        }

        // 更新 ID 映射
        {
            let mut id_map = self.id_to_idx.write().unwrap();
            id_map.insert(id.to_string(), new_node_idx);
        }

        // 连接到现有图
        let entry = *self.entry_point.read().unwrap();
        if let Some(entry_idx) = entry {
            self.connect_node(new_node_idx, level, entry_idx)?;
        }

        // 更新入口点（如果新节点的层级更高）
        {
            let mut max_level = self.max_level.write().unwrap();
            if level >= *max_level {
                *max_level = level + 1;
                let mut entry = self.entry_point.write().unwrap();
                *entry = Some(new_node_idx);
            }
        }

        Ok(())
    }

    /// 将新节点连接到图中
    fn connect_node(
        &self,
        new_idx: usize,
        new_level: usize,
        entry_idx: usize,
    ) -> Result<(), AiError> {
        let max_level = *self.max_level.read().unwrap();
        let ef = self.config.ef_construction;
        let m = self.config.max_connections;
        let metric = self.config.metric;

        // 从顶层贪心下降到 new_level + 1 层
        let mut current_entry = entry_idx;
        for layer in (new_level + 1..max_level).rev() {
            let result = self.search_layer(new_idx, current_entry, 1, layer, metric)?;
            if let Some((idx, _)) = result.into_iter().next() {
                current_entry = idx;
            }
        }

        // 从 new_level 层开始连接
        for layer in (0..=new_level).rev() {
            // 搜索 ef 个最近邻居
            let candidates = self.search_layer(new_idx, current_entry, ef, layer, metric)?;

            // 选择 M 个最近的作为邻居
            let max_neighbors = if layer == 0 { 2 * m } else { m };
            let selected: Vec<usize> = candidates
                .into_iter()
                .take(max_neighbors)
                .map(|(idx, _)| idx)
                .collect();

            // 设置新节点的邻居
            {
                let mut nodes = self.nodes.write().unwrap();
                if new_idx < nodes.len() && layer < nodes[new_idx].neighbors.len() {
                    nodes[new_idx].neighbors[layer] = selected.clone();
                }
            }

            // 反向连接：将新节点添加到邻居的邻居列表中
            for &neighbor_idx in &selected {
                let mut nodes = self.nodes.write().unwrap();
                if neighbor_idx < nodes.len() && layer < nodes[neighbor_idx].neighbors.len() {
                    let neighbor_list = &mut nodes[neighbor_idx].neighbors[layer];
                    if !neighbor_list.contains(&new_idx) {
                        neighbor_list.push(new_idx);
                        // 如果邻居列表超过最大值，修剪到 max_neighbors
                        let max = if layer == 0 { 2 * m } else { m };
                        if neighbor_list.len() > max {
                            // 简单修剪：保留前 max 个（更优实现应保留距离最近的）
                            neighbor_list.truncate(max);
                        }
                    }
                }
            }

            current_entry = selected.first().copied().unwrap_or(current_entry);
        }

        Ok(())
    }

    /// 在单层中搜索最近的 ef 个节点
    ///
    /// 返回值：Vec<(节点索引, 距离)>，按距离升序排列
    fn search_layer(
        &self,
        query_idx: usize,
        entry_idx: usize,
        ef: usize,
        layer: usize,
        metric: VectorMetric,
    ) -> Result<Vec<(usize, f32)>, AiError> {
        let nodes = self.nodes.read().unwrap();

        if query_idx >= nodes.len() || entry_idx >= nodes.len() {
            return Ok(Vec::new());
        }

        let query_vector = nodes[query_idx].vector.clone();
        drop(nodes);

        let mut visited = HashSet::new();
        visited.insert(entry_idx);

        // 候选堆（最小堆，距离最小者优先）
        let mut candidates: BinaryHeap<std::cmp::Reverse<Candidate>> = BinaryHeap::new();
        // 结果堆（最大堆，距离最大者优先，便于淘汰）
        let mut results: BinaryHeap<Candidate> = BinaryHeap::new();

        let entry_dist = {
            let nodes = self.nodes.read().unwrap();
            Self::distance(metric, &query_vector, &nodes[entry_idx].vector)
        };
        candidates.push(std::cmp::Reverse(Candidate {
            distance: entry_dist,
            node_idx: entry_idx,
        }));
        results.push(Candidate {
            distance: entry_dist,
            node_idx: entry_idx,
        });

        while let Some(std::cmp::Reverse(cand)) = candidates.pop() {
            // 如果候选比结果中最差的还要远，停止
            if let Some(worst) = results.peek() {
                if cand.distance > worst.distance && results.len() >= ef {
                    break;
                }
            }

            // 检查候选的邻居
            let neighbor_ids = {
                let nodes = self.nodes.read().unwrap();
                if cand.node_idx >= nodes.len() || layer >= nodes[cand.node_idx].neighbors.len() {
                    continue;
                }
                nodes[cand.node_idx].neighbors[layer].clone()
            };

            for &neighbor_idx in &neighbor_ids {
                if visited.contains(&neighbor_idx) {
                    continue;
                }
                visited.insert(neighbor_idx);

                let dist = {
                    let nodes = self.nodes.read().unwrap();
                    if neighbor_idx >= nodes.len() {
                        continue;
                    }
                    Self::distance(metric, &query_vector, &nodes[neighbor_idx].vector)
                };

                // 如果结果未满，或比最差结果好
                let should_add = results.len() < ef
                    || results
                        .peek()
                        .map(|w| dist < w.distance)
                        .unwrap_or(true);

                if should_add {
                    candidates.push(std::cmp::Reverse(Candidate {
                        distance: dist,
                        node_idx: neighbor_idx,
                    }));
                    results.push(Candidate {
                        distance: dist,
                        node_idx: neighbor_idx,
                    });

                    // 维持结果集大小
                    while results.len() > ef {
                        results.pop();
                    }
                }
            }
        }

        // 转换为升序排列的结果
        let mut result_vec: Vec<(usize, f32)> = results
            .into_iter()
            .map(|c| (c.node_idx, c.distance))
            .collect();
        result_vec.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        Ok(result_vec)
    }

    /// 搜索最近邻
    ///
    /// # 参数
    /// - `query`: 查询向量
    /// - `top_k`: 返回结果数量
    /// - `ef_search`: 搜索时候选列表大小（越大越精确但越慢）
    pub fn search(
        &self,
        query: &[f32],
        top_k: usize,
        ef_search: usize,
    ) -> Result<Vec<HnswSearchResult>, AiError> {
        if query.is_empty() {
            return Err(AiError::Vector("查询向量不能为空".to_string()));
        }

        let entry = *self.entry_point.read().unwrap();
        let entry_idx = match entry {
            Some(idx) => idx,
            None => return Ok(Vec::new()),
        };

        let dim = *self.dimension.read().unwrap();
        if dim != 0 && dim != query.len() {
            return Err(AiError::Vector(format!(
                "查询向量维度不匹配: 期望 {}, 实际 {}",
                dim,
                query.len()
            )));
        }

        let metric = self.config.metric;
        let max_level = *self.max_level.read().unwrap();
        let ef = ef_search.max(top_k);

        // 从顶层贪心下降到第 1 层
        let mut current_entry = entry_idx;
        for layer in (1..max_level).rev() {
            let result = self.search_layer_query(query, current_entry, 1, layer, metric)?;
            if let Some((idx, _)) = result.into_iter().next() {
                current_entry = idx;
            }
        }

        // 在第 0 层进行 ef 搜索
        let candidates = self.search_layer_query(query, current_entry, ef, 0, metric)?;

        // 转换为搜索结果
        let nodes = self.nodes.read().unwrap();
        let results: Vec<HnswSearchResult> = candidates
            .into_iter()
            .filter(|(idx, _)| !nodes.get(*idx).map(|n| n.deleted).unwrap_or(true))
            .take(top_k)
            .filter_map(|(idx, dist)| {
                let node = nodes.get(idx)?;
                if node.deleted {
                    return None;
                }
                // 将距离转换为分数（cosine: 1 - dist; euclidean: 1/(1+dist); dotproduct: -dist）
                let score = match metric {
                    VectorMetric::Cosine => 1.0 - dist,
                    VectorMetric::Euclidean => 1.0 / (1.0 + dist),
                    VectorMetric::DotProduct => -dist,
                };
                Some(HnswSearchResult {
                    id: node.id.clone(),
                    score,
                    vector: node.vector.clone(),
                })
            })
            .collect();

        Ok(results)
    }

    /// 在单层中搜索（用于查询，query 是外部向量而非索引内节点）
    fn search_layer_query(
        &self,
        query: &[f32],
        entry_idx: usize,
        ef: usize,
        layer: usize,
        metric: VectorMetric,
    ) -> Result<Vec<(usize, f32)>, AiError> {
        let nodes = self.nodes.read().unwrap();

        if entry_idx >= nodes.len() {
            return Ok(Vec::new());
        }
        if layer >= nodes[entry_idx].neighbors.len() {
            return Ok(vec![(entry_idx, Self::distance(metric, query, &nodes[entry_idx].vector))]);
        }

        let entry_dist = Self::distance(metric, query, &nodes[entry_idx].vector);
        drop(nodes);

        let mut visited = HashSet::new();
        visited.insert(entry_idx);

        let mut candidates: BinaryHeap<std::cmp::Reverse<Candidate>> = BinaryHeap::new();
        let mut results: BinaryHeap<Candidate> = BinaryHeap::new();

        candidates.push(std::cmp::Reverse(Candidate {
            distance: entry_dist,
            node_idx: entry_idx,
        }));
        results.push(Candidate {
            distance: entry_dist,
            node_idx: entry_idx,
        });

        while let Some(std::cmp::Reverse(cand)) = candidates.pop() {
            if let Some(worst) = results.peek() {
                if cand.distance > worst.distance && results.len() >= ef {
                    break;
                }
            }

            let neighbor_ids = {
                let nodes = self.nodes.read().unwrap();
                if cand.node_idx >= nodes.len() || layer >= nodes[cand.node_idx].neighbors.len() {
                    continue;
                }
                nodes[cand.node_idx].neighbors[layer].clone()
            };

            for &neighbor_idx in &neighbor_ids {
                if visited.contains(&neighbor_idx) {
                    continue;
                }
                visited.insert(neighbor_idx);

                let dist = {
                    let nodes = self.nodes.read().unwrap();
                    if neighbor_idx >= nodes.len() || nodes[neighbor_idx].deleted {
                        continue;
                    }
                    Self::distance(metric, query, &nodes[neighbor_idx].vector)
                };

                let should_add = results.len() < ef
                    || results
                        .peek()
                        .map(|w| dist < w.distance)
                        .unwrap_or(true);

                if should_add {
                    candidates.push(std::cmp::Reverse(Candidate {
                        distance: dist,
                        node_idx: neighbor_idx,
                    }));
                    results.push(Candidate {
                        distance: dist,
                        node_idx: neighbor_idx,
                    });

                    while results.len() > ef {
                        results.pop();
                    }
                }
            }
        }

        let mut result_vec: Vec<(usize, f32)> = results
            .into_iter()
            .map(|c| (c.node_idx, c.distance))
            .collect();
        result_vec.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        Ok(result_vec)
    }

    /// 标记删除节点（软删除）
    ///
    /// 被删除的节点不会出现在搜索结果中，但仍保留在图中以维持连接性。
    pub fn delete(&self, id: &str) -> Result<bool, AiError> {
        let id_map = self.id_to_idx.read().unwrap();
        let idx = match id_map.get(id) {
            Some(&idx) => idx,
            None => return Ok(false),
        };
        drop(id_map);

        let mut nodes = self.nodes.write().unwrap();
        if idx < nodes.len() {
            let was_deleted = nodes[idx].deleted;
            nodes[idx].deleted = true;
            Ok(!was_deleted)
        } else {
            Ok(false)
        }
    }

    /// 获取指定 ID 的向量
    pub fn get(&self, id: &str) -> Option<Vec<f32>> {
        let id_map = self.id_to_idx.read().ok()?;
        let idx = id_map.get(id)?;
        let nodes = self.nodes.read().ok()?;
        nodes.get(*idx).filter(|n| !n.deleted).map(|n| n.vector.clone())
    }

    /// 获取索引中所有有效 ID
    pub fn ids(&self) -> Vec<String> {
        let nodes = self.nodes.read().unwrap();
        nodes
            .iter()
            .filter(|n| !n.deleted)
            .map(|n| n.id.clone())
            .collect()
    }
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

/// 计算余弦相似度
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na * nb)
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hnsw_config_default() {
        let config = HnswConfig::default();
        assert_eq!(config.max_connections, 16);
        assert_eq!(config.ef_construction, 200);
        assert_eq!(config.metric, VectorMetric::Cosine);
    }

    #[test]
    fn test_hnsw_config_builder() {
        let config = HnswConfig::default()
            .with_metric(VectorMetric::Euclidean)
            .with_max_connections(8)
            .with_ef_construction(100)
            .with_seed(123);
        assert_eq!(config.max_connections, 8);
        assert_eq!(config.ef_construction, 100);
        assert_eq!(config.metric, VectorMetric::Euclidean);
        assert_eq!(config.seed, 123);
    }

    #[test]
    fn test_hnsw_empty_index() {
        let index = HnswIndex::new(HnswConfig::default());
        assert!(index.is_empty());
        assert_eq!(index.len(), 0);
        assert_eq!(index.dimension(), 0);
    }

    #[test]
    fn test_hnsw_insert_single() {
        let index = HnswIndex::new(HnswConfig::default());
        index.insert("v1", vec![1.0, 0.0, 0.0]).unwrap();
        assert_eq!(index.len(), 1);
        assert_eq!(index.dimension(), 3);
        assert!(!index.is_empty());
    }

    #[test]
    fn test_hnsw_insert_multiple() {
        let index = HnswIndex::new(HnswConfig::default());
        for i in 0..10 {
            index
                .insert(&format!("v{}", i), vec![i as f32, 1.0, 0.0])
                .unwrap();
        }
        assert_eq!(index.len(), 10);
        assert_eq!(index.dimension(), 3);
    }

    #[test]
    fn test_hnsw_insert_empty_vector_fails() {
        let index = HnswIndex::new(HnswConfig::default());
        let result = index.insert("v1", vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn test_hnsw_insert_dimension_mismatch_fails() {
        let index = HnswIndex::new(HnswConfig::default());
        index.insert("v1", vec![1.0, 0.0, 0.0]).unwrap();
        let result = index.insert("v2", vec![1.0, 0.0]);
        assert!(result.is_err());
    }

    #[test]
    fn test_hnsw_insert_duplicate_id_updates() {
        let index = HnswIndex::new(HnswConfig::default());
        index.insert("v1", vec![1.0, 0.0, 0.0]).unwrap();
        index.insert("v1", vec![0.0, 1.0, 0.0]).unwrap();
        // 应该只保留一个节点
        assert_eq!(index.len(), 1);
        let vector = index.get("v1").unwrap();
        assert_eq!(vector, vec![0.0, 1.0, 0.0]);
    }

    #[test]
    fn test_hnsw_search_empty_index() {
        let index = HnswIndex::new(HnswConfig::default());
        let results = index.search(&[1.0, 0.0], 5, 50).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_hnsw_search_single_node() {
        let index = HnswIndex::new(HnswConfig::default());
        index.insert("v1", vec![1.0, 0.0, 0.0]).unwrap();
        let results = index.search(&[1.0, 0.0, 0.0], 1, 50).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "v1");
    }

    #[test]
    fn test_hnsw_search_cosine_metric() {
        let mut config = HnswConfig::default();
        config = config.with_metric(VectorMetric::Cosine);
        let index = HnswIndex::new(config);

        // 插入 3 个向量
        index.insert("a", vec![1.0, 0.0, 0.0]).unwrap();
        index.insert("b", vec![0.0, 1.0, 0.0]).unwrap();
        index.insert("c", vec![1.0, 1.0, 0.0]).unwrap();

        // 搜索与 [1, 0, 0] 最相似的 2 个
        let results = index.search(&[1.0, 0.0, 0.0], 2, 50).unwrap();
        assert_eq!(results.len(), 2);
        // 最相似的应该是 "a"
        assert_eq!(results[0].id, "a");
        // cosine similarity 应接近 1.0
        assert!((results[0].score - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_hnsw_search_euclidean_metric() {
        let config = HnswConfig::default().with_metric(VectorMetric::Euclidean);
        let index = HnswIndex::new(config);

        index.insert("a", vec![1.0, 0.0]).unwrap();
        index.insert("b", vec![5.0, 0.0]).unwrap();
        index.insert("c", vec![10.0, 0.0]).unwrap();

        let results = index.search(&[1.0, 0.0], 2, 50).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, "a");
    }

    #[test]
    fn test_hnsw_search_dotproduct_metric() {
        let config = HnswConfig::default().with_metric(VectorMetric::DotProduct);
        let index = HnswIndex::new(config);

        index.insert("a", vec![1.0, 1.0]).unwrap();
        index.insert("b", vec![2.0, 2.0]).unwrap();
        index.insert("c", vec![0.1, 0.1]).unwrap();

        let results = index.search(&[1.0, 1.0], 3, 50).unwrap();
        // 点积最大的应该是 b (dot=4)，其次是 a (dot=2)，最后是 c (dot=0.2)
        assert_eq!(results[0].id, "b");
    }

    #[test]
    fn test_hnsw_search_top_k_limit() {
        let index = HnswIndex::new(HnswConfig::default());
        for i in 0..20 {
            index
                .insert(&format!("v{}", i), vec![i as f32, 0.0, 0.0])
                .unwrap();
        }

        let results = index.search(&[5.0, 0.0, 0.0], 5, 50).unwrap();
        assert_eq!(results.len(), 5);
    }

    #[test]
    fn test_hnsw_search_larger_dataset() {
        let index = HnswIndex::new(HnswConfig::default().with_max_connections(8));

        // 插入 100 个随机向量
        for i in 0..100 {
            let x = (i as f32).sin();
            let y = (i as f32).cos();
            index
                .insert(&format!("v{}", i), vec![x, y, 0.0])
                .unwrap();
        }

        assert_eq!(index.len(), 100);

        // 搜索
        let results = index.search(&[0.0, 1.0, 0.0], 5, 100).unwrap();
        assert_eq!(results.len(), 5);
        // 第一个结果应该是与 [0, 1, 0] 最接近的（i=0 时 cos(0)=1）
        assert_eq!(results[0].id, "v0");
    }

    #[test]
    fn test_hnsw_delete_node() {
        let index = HnswIndex::new(HnswConfig::default());
        index.insert("a", vec![1.0, 0.0]).unwrap();
        index.insert("b", vec![0.0, 1.0]).unwrap();
        index.insert("c", vec![1.0, 1.0]).unwrap();

        // 删除 b
        let deleted = index.delete("b").unwrap();
        assert!(deleted);

        // 长度应减少
        assert_eq!(index.len(), 2);

        // 搜索不应返回 b
        let results = index.search(&[0.0, 1.0], 3, 50).unwrap();
        for r in &results {
            assert_ne!(r.id, "b");
        }
    }

    #[test]
    fn test_hnsw_delete_nonexistent() {
        let index = HnswIndex::new(HnswConfig::default());
        let result = index.delete("nonexistent").unwrap();
        assert!(!result);
    }

    #[test]
    fn test_hnsw_get_vector() {
        let index = HnswIndex::new(HnswConfig::default());
        index.insert("v1", vec![1.0, 2.0, 3.0]).unwrap();

        let vector = index.get("v1").unwrap();
        assert_eq!(vector, vec![1.0, 2.0, 3.0]);

        // 不存在的 ID
        assert!(index.get("nonexistent").is_none());
    }

    #[test]
    fn test_hnsw_get_deleted_returns_none() {
        let index = HnswIndex::new(HnswConfig::default());
        index.insert("v1", vec![1.0, 0.0]).unwrap();
        index.delete("v1").unwrap();

        assert!(index.get("v1").is_none());
    }

    #[test]
    fn test_hnsw_ids() {
        let index = HnswIndex::new(HnswConfig::default());
        index.insert("a", vec![1.0, 0.0]).unwrap();
        index.insert("b", vec![0.0, 1.0]).unwrap();
        index.insert("c", vec![1.0, 1.0]).unwrap();

        let mut ids = index.ids();
        ids.sort();
        assert_eq!(ids, vec!["a".to_string(), "b".to_string(), "c".to_string()]);

        // 删除后
        index.delete("b").unwrap();
        let mut ids = index.ids();
        ids.sort();
        assert_eq!(ids, vec!["a".to_string(), "c".to_string()]);
    }

    #[test]
    fn test_hnsw_search_empty_query_fails() {
        let index = HnswIndex::new(HnswConfig::default());
        index.insert("v1", vec![1.0, 0.0]).unwrap();
        let result = index.search(&[], 1, 50);
        assert!(result.is_err());
    }

    #[test]
    fn test_hnsw_search_dimension_mismatch_fails() {
        let index = HnswIndex::new(HnswConfig::default());
        index.insert("v1", vec![1.0, 0.0, 0.0]).unwrap();
        let result = index.search(&[1.0, 0.0], 1, 50);
        assert!(result.is_err());
    }

    #[test]
    fn test_hnsw_rng_deterministic() {
        let mut rng1 = Rng::new(42);
        let mut rng2 = Rng::new(42);
        assert_eq!(rng1.next_u64(), rng2.next_u64());
        assert_eq!(rng1.next_u64(), rng2.next_u64());
    }

    #[test]
    fn test_hnsw_rng_zero_seed() {
        let mut rng = Rng::new(0);
        // 不应 panic，且应产生不同的值
        let v1 = rng.next_u64();
        let v2 = rng.next_u64();
        assert_ne!(v1, v2);
    }

    #[test]
    fn test_hnsw_candidate_ord() {
        let c1 = Candidate { distance: 0.5, node_idx: 0 };
        let c2 = Candidate { distance: 1.0, node_idx: 1 };
        // 距离更大的应排在前面（最大堆）
        assert!(c2 > c1);
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let sim = cosine_similarity(&[1.0, 0.0, 0.0], &[1.0, 0.0, 0.0]);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let sim = cosine_similarity(&[1.0, 0.0], &[0.0, 1.0]);
        assert!(sim.abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let sim = cosine_similarity(&[1.0, 0.0], &[-1.0, 0.0]);
        assert!((sim + 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_empty() {
        let sim = cosine_similarity(&[], &[]);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_cosine_similarity_different_lengths() {
        let sim = cosine_similarity(&[1.0, 0.0], &[1.0]);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let sim = cosine_similarity(&[0.0, 0.0], &[1.0, 0.0]);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_hnsw_search_after_reinsert() {
        let index = HnswIndex::new(HnswConfig::default());
        index.insert("a", vec![1.0, 0.0]).unwrap();
        index.insert("b", vec![0.0, 1.0]).unwrap();

        // 删除 a
        index.delete("a").unwrap();
        assert_eq!(index.len(), 1);

        // 重新插入 a
        index.insert("a", vec![1.0, 0.0]).unwrap();
        assert_eq!(index.len(), 2);

        // 搜索应能找到 a
        let results = index.search(&[1.0, 0.0], 2, 50).unwrap();
        assert!(results.iter().any(|r| r.id == "a"));
    }

    #[test]
    fn test_hnsw_insert_high_dimensional() {
        let config = HnswConfig::default().with_max_connections(4);
        let index = HnswIndex::new(config);

        // 插入 10 维向量
        for i in 0..20 {
            let mut vector = vec![0.0f32; 10];
            vector[i % 10] = 1.0;
            index.insert(&format!("v{}", i), vector).unwrap();
        }

        assert_eq!(index.len(), 20);
        assert_eq!(index.dimension(), 10);

        let query = {
            let mut v = vec![0.0f32; 10];
            v[0] = 1.0;
            v
        };
        let results = index.search(&query, 3, 50).unwrap();
        assert_eq!(results.len(), 3);
    }
}
