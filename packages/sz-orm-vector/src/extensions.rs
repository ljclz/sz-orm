//! pgvector 深度扩展功能
//!
//! 本模块补充 pgvector 扩展缺失的核心深度功能，包括：
//!
//! - **ANN 索引管理**：IVFFlat / HNSW 索引的创建、删除、重建、查询
//! - **相似度算法**：L2 距离、内积、余弦相似度、曼哈顿距离的批量计算
//! - **批量向量操作**：批量搜索、批量获取、批量元数据过滤
//! - **向量维度验证**：单向量/批量向量维度校验、集合兼容性校验
//! - **向量归一化工具**：L2 归一化、范数计算、归一化状态检查
//!
//! # 设计说明
//!
//! 本模块以**独立函数 + 扩展 trait** 的方式提供，不修改既有 `PgVectorStore` trait，
//! 避免破坏已有的 memory / stub / real_pg 三种实现。
//! 内存计算部分基于纯 Rust 实现，不依赖外部库。

#![allow(dead_code)]

use crate::error::VectorError;
use crate::{PgVectorStore, SearchResult, VectorMetric, VectorRecord};
use std::collections::HashMap;

// =============================================================================
// 一、ANN 索引管理
// =============================================================================

/// ANN 索引类型
///
/// pgvector 支持两种主流 ANN（近似最近邻）索引：
///
/// - **IVFFlat**：基于倒排文件 + 扁平量化，适合中小规模数据，构建快
/// - **HNSW**：基于层次化可导航小世界图，查询精度高，适合大规模数据
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnIndexType {
    /// IVFFlat 索引（倒排文件 + 扁平量化）
    Ivfflat,
    /// HNSW 索引（层次化可导航小世界图）
    Hnsw,
}

impl AnnIndexType {
    /// 转为 pgvector SQL 索引方法字符串
    pub fn as_sql_method(&self) -> &'static str {
        match self {
            AnnIndexType::Ivfflat => "ivfflat",
            AnnIndexType::Hnsw => "hnsw",
        }
    }

    /// 转为人类可读名称
    pub fn as_str(&self) -> &'static str {
        match self {
            AnnIndexType::Ivfflat => "IVFFlat",
            AnnIndexType::Hnsw => "HNSW",
        }
    }
}

/// IVFFlat 索引参数
///
/// `lists` 参数控制聚类中心数量，通常建议：
/// - 数据量 < 100 万：`lists = sqrt(rows)`
/// - 数据量 >= 100 万：`lists = rows / 1000`
#[derive(Debug, Clone)]
pub struct IvfflatParams {
    /// 聚类中心数量（lists）
    pub lists: usize,
    /// 查询时探测的聚类数量（probes）
    pub probes: usize,
}

impl IvfflatParams {
    /// 创建 IVFFlat 参数
    pub fn new(lists: usize, probes: usize) -> Self {
        Self { lists, probes }
    }

    /// 根据数据量自动推荐 lists 参数
    pub fn recommended_for_rows(rows: usize) -> Self {
        let lists = if rows < 1_000_000 {
            (rows as f64).sqrt() as usize
        } else {
            rows / 1000
        };
        let lists = lists.max(1);
        Self {
            lists,
            probes: (lists / 10).max(1),
        }
    }

    /// 生成 SQL 选项子句（`WITH (lists = N)`）
    pub fn to_sql_options(&self) -> String {
        format!("(lists = {})", self.lists)
    }
}

impl Default for IvfflatParams {
    fn default() -> Self {
        Self::new(100, 10)
    }
}

/// HNSW 索引参数
///
/// - `m`：每个节点的最大连接数，影响索引大小和精度（默认 16）
/// - `ef_construction`：构建时搜索宽度，影响构建时间和精度（默认 64）
#[derive(Debug, Clone)]
pub struct HnswParams {
    /// 每个节点的最大连接数
    pub m: usize,
    /// 构建时的搜索宽度
    pub ef_construction: usize,
}

impl HnswParams {
    /// 创建 HNSW 参数
    pub fn new(m: usize, ef_construction: usize) -> Self {
        Self { m, ef_construction }
    }

    /// 生成 SQL 选项子句（`WITH (m = N, ef_construction = N)`）
    pub fn to_sql_options(&self) -> String {
        format!("(m = {}, ef_construction = {})", self.m, self.ef_construction)
    }
}

impl Default for HnswParams {
    fn default() -> Self {
        Self::new(16, 64)
    }
}

/// ANN 索引定义
#[derive(Debug, Clone)]
pub struct AnnIndexDef {
    /// 索引名称
    pub index_name: String,
    /// 集合名称
    pub collection: String,
    /// 索引类型
    pub index_type: AnnIndexType,
    /// 距离度量
    pub metric: VectorMetric,
    /// IVFFlat 参数（仅当 index_type == Ivfflat 时有效）
    pub ivfflat_params: Option<IvfflatParams>,
    /// HNSW 参数（仅当 index_type == Hnsw 时有效）
    pub hnsw_params: Option<HnswParams>,
    /// 是否并发创建（CONCURRENTLY，不锁表）
    pub concurrently: bool,
}

impl AnnIndexDef {
    /// 创建 IVFFlat 索引定义
    pub fn new_ivfflat(collection: &str, metric: VectorMetric, params: IvfflatParams) -> Self {
        Self {
            index_name: format!("idx_{}_ivfflat", collection),
            collection: collection.to_string(),
            index_type: AnnIndexType::Ivfflat,
            metric,
            ivfflat_params: Some(params),
            hnsw_params: None,
            concurrently: false,
        }
    }

    /// 创建 HNSW 索引定义
    pub fn new_hnsw(collection: &str, metric: VectorMetric, params: HnswParams) -> Self {
        Self {
            index_name: format!("idx_{}_hnsw", collection),
            collection: collection.to_string(),
            index_type: AnnIndexType::Hnsw,
            metric,
            ivfflat_params: None,
            hnsw_params: Some(params),
            concurrently: false,
        }
    }

    /// 设置并发创建（不锁表）
    pub fn with_concurrently(mut self) -> Self {
        self.concurrently = true;
        self
    }

    /// 生成 CREATE INDEX SQL
    pub fn to_create_sql(&self) -> String {
        let concurrently_str = if self.concurrently {
            "CONCURRENTLY "
        } else {
            ""
        };
        let op_class = match self.metric {
            VectorMetric::Cosine => "vector_cosine_ops",
            VectorMetric::Euclidean => "vector_l2_ops",
            VectorMetric::DotProduct => "vector_ip_ops",
        };
        let options = match self.index_type {
            AnnIndexType::Ivfflat => self
                .ivfflat_params
                .as_ref()
                .map(|p| p.to_sql_options())
                .unwrap_or_default(),
            AnnIndexType::Hnsw => self
                .hnsw_params
                .as_ref()
                .map(|p| p.to_sql_options())
                .unwrap_or_default(),
        };
        let options_clause = if options.is_empty() {
            String::new()
        } else {
            format!(" WITH {}", options)
        };
        format!(
            "CREATE INDEX {}{} ON vectors_{} USING {} (embedding {}){}",
            concurrently_str,
            self.index_name,
            self.collection,
            self.index_type.as_sql_method(),
            op_class,
            options_clause
        )
    }

    /// 生成 DROP INDEX SQL
    pub fn to_drop_sql(&self) -> String {
        let concurrently_str = if self.concurrently {
            "CONCURRENTLY "
        } else {
            ""
        };
        format!("DROP INDEX {}{}", concurrently_str, self.index_name)
    }

    /// 生成 REINDEX SQL
    pub fn to_reindex_sql(&self) -> String {
        format!("REINDEX INDEX {}", self.index_name)
    }
}

/// ANN 索引管理器（内存版，用于跟踪已创建的索引）
#[derive(Debug, Clone, Default)]
pub struct AnnIndexRegistry {
    /// 已注册的索引：index_name -> AnnIndexDef
    indexes: HashMap<String, AnnIndexDef>,
}

impl AnnIndexRegistry {
    /// 创建空注册表
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册一个 ANN 索引
    pub fn register(&mut self, def: AnnIndexDef) -> Result<(), VectorError> {
        if self.indexes.contains_key(&def.index_name) {
            return Err(VectorError::Query(format!(
                "ANN index already exists: {}",
                def.index_name
            )));
        }
        self.indexes.insert(def.index_name.clone(), def);
        Ok(())
    }

    /// 注销一个 ANN 索引
    pub fn unregister(&mut self, index_name: &str) -> Result<AnnIndexDef, VectorError> {
        self.indexes
            .remove(index_name)
            .ok_or_else(|| VectorError::Query(format!("ANN index not found: {}", index_name)))
    }

    /// 检查索引是否存在
    pub fn exists(&self, index_name: &str) -> bool {
        self.indexes.contains_key(index_name)
    }

    /// 列出某集合上的所有索引
    pub fn list_for_collection(&self, collection: &str) -> Vec<&AnnIndexDef> {
        self.indexes
            .values()
            .filter(|def| def.collection == collection)
            .collect()
    }

    /// 获取所有索引
    pub fn list_all(&self) -> Vec<&AnnIndexDef> {
        self.indexes.values().collect()
    }

    /// 生成重建所有索引的 SQL 列表
    pub fn reindex_all_sql(&self) -> Vec<String> {
        self.indexes
            .values()
            .map(|def| def.to_reindex_sql())
            .collect()
    }
}

// =============================================================================
// 二、相似度算法
// =============================================================================

/// 相似度算法工具集
///
/// 提供多种向量相似度/距离的纯 Rust 计算，不依赖 pgvector。
pub struct SimilarityAlgorithms;

impl SimilarityAlgorithms {
    /// L2 距离（欧氏距离）
    ///
    /// `d = sqrt(sum((a_i - b_i)^2))`
    pub fn l2_distance(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() || a.is_empty() {
            return f32::MAX;
        }
        a.iter()
            .zip(b.iter())
            .map(|(x, y)| (x - y) * (x - y))
            .sum::<f32>()
            .sqrt()
    }

    /// 内积（点积）
    ///
    /// `d = sum(a_i * b_i)`
    pub fn inner_product(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() || a.is_empty() {
            return 0.0;
        }
        a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
    }

    /// 余弦相似度
    ///
    /// `d = (a·b) / (|a| * |b|)`
    pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() || a.is_empty() {
            return 0.0;
        }
        let dot = Self::inner_product(a, b);
        let na = Self::l2_norm(a);
        let nb = Self::l2_norm(b);
        if na == 0.0 || nb == 0.0 {
            return 0.0;
        }
        dot / (na * nb)
    }

    /// 曼哈顿距离（L1 距离）
    ///
    /// `d = sum(|a_i - b_i|)`
    pub fn manhattan_distance(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() || a.is_empty() {
            return f32::MAX;
        }
        a.iter()
            .zip(b.iter())
            .map(|(x, y)| (x - y).abs())
            .sum()
    }

    /// L2 范数（欧氏范数）
    ///
    /// `|v| = sqrt(sum(v_i^2))`
    pub fn l2_norm(v: &[f32]) -> f32 {
        v.iter().map(|x| x * x).sum::<f32>().sqrt()
    }

    /// 将距离值转换为相似度分数（值越大越相似）
    ///
    /// - Cosine：相似度 = 1 - 距离（距离 0 → 相似度 1）
    /// - Euclidean：相似度 = 1 / (1 + 距离)（距离 0 → 相似度 1）
    /// - DotProduct：相似度 = 点积本身（无转换）
    pub fn distance_to_similarity(metric: VectorMetric, distance: f32) -> f32 {
        match metric {
            VectorMetric::Cosine => 1.0 - distance,
            VectorMetric::Euclidean => 1.0 / (1.0 + distance),
            VectorMetric::DotProduct => distance,
        }
    }

    /// 按指定度量计算相似度
    pub fn similarity(metric: VectorMetric, a: &[f32], b: &[f32]) -> f32 {
        match metric {
            VectorMetric::Cosine => Self::cosine_similarity(a, b),
            VectorMetric::Euclidean => {
                let dist = Self::l2_distance(a, b);
                Self::distance_to_similarity(metric, dist)
            }
            VectorMetric::DotProduct => Self::inner_product(a, b),
        }
    }

    /// 批量计算查询向量与多个候选向量的相似度
    ///
    /// 返回 (索引, 相似度) 对的列表（未排序）
    pub fn batch_similarity(
        metric: VectorMetric,
        query: &[f32],
        candidates: &[Vec<f32>],
    ) -> Vec<(usize, f32)> {
        candidates
            .iter()
            .enumerate()
            .map(|(i, v)| (i, Self::similarity(metric, query, v)))
            .collect()
    }

    /// 批量计算并返回 top_k 结果（降序排列）
    pub fn batch_top_k(
        metric: VectorMetric,
        query: &[f32],
        candidates: &[Vec<f32>],
        top_k: usize,
    ) -> Vec<(usize, f32)> {
        let mut scored = Self::batch_similarity(metric, query, candidates);
        // 降序排序（相似度越大越靠前）
        scored.sort_by(|a, b| {
            b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(top_k);
        scored
    }
}

// =============================================================================
// 三、向量维度验证
// =============================================================================

/// 向量维度验证工具
pub struct DimensionValidator;

/// 维度限制常量
///
/// pgvector 0.5.0+ 支持最大 16000 维
pub const MAX_VECTOR_DIMENSION: usize = 16000;
/// 最小维度（至少 1 维）
pub const MIN_VECTOR_DIMENSION: usize = 1;

impl DimensionValidator {
    /// 校验单个维度值是否合法
    pub fn validate_dimension(dim: usize) -> Result<(), VectorError> {
        if !(MIN_VECTOR_DIMENSION..=MAX_VECTOR_DIMENSION).contains(&dim) {
            return Err(VectorError::InvalidConfig(format!(
                "dimension must be between {} and {}, got {}",
                MIN_VECTOR_DIMENSION, MAX_VECTOR_DIMENSION, dim
            )));
        }
        Ok(())
    }

    /// 校验向量维度是否与期望一致
    pub fn validate_vector(vector: &[f32], expected_dim: usize) -> Result<(), VectorError> {
        Self::validate_dimension(expected_dim)?;
        if vector.len() != expected_dim {
            return Err(VectorError::DimensionMismatch {
                expected: expected_dim,
                actual: vector.len(),
            });
        }
        Ok(())
    }

    /// 批量校验向量维度是否一致
    ///
    /// 所有向量必须与 `expected_dim` 一致，且彼此之间维度也一致
    pub fn validate_batch(
        vectors: &[Vec<f32>],
        expected_dim: usize,
    ) -> Result<(), VectorError> {
        Self::validate_dimension(expected_dim)?;
        for (i, v) in vectors.iter().enumerate() {
            if v.len() != expected_dim {
                return Err(VectorError::DimensionMismatch {
                    expected: expected_dim,
                    actual: v.len(),
                });
            }
            // 检查 NaN/Inf
            if v.iter().any(|x| x.is_nan() || x.is_infinite()) {
                return Err(VectorError::InvalidConfig(format!(
                    "vector at index {} contains NaN or Inf values",
                    i
                )));
            }
        }
        Ok(())
    }

    /// 校验两个集合的维度是否兼容（用于跨集合查询）
    pub fn validate_collection_compatibility(
        dim_a: usize,
        dim_b: usize,
    ) -> Result<(), VectorError> {
        Self::validate_dimension(dim_a)?;
        Self::validate_dimension(dim_b)?;
        if dim_a != dim_b {
            return Err(VectorError::DimensionMismatch {
                expected: dim_a,
                actual: dim_b,
            });
        }
        Ok(())
    }

    /// 校验查询向量与集合维度是否匹配
    pub fn validate_query(query: &[f32], collection_dim: usize) -> Result<(), VectorError> {
        Self::validate_vector(query, collection_dim)
    }
}

// =============================================================================
// 四、向量归一化工具
// =============================================================================

/// 向量归一化工具
pub struct VectorNormalizer;

impl VectorNormalizer {
    /// L2 归一化（将向量缩放为单位长度）
    ///
    /// 零向量返回零向量（避免除零）
    pub fn l2_normalize(v: &[f32]) -> Vec<f32> {
        let norm = SimilarityAlgorithms::l2_norm(v);
        if norm == 0.0 {
            return v.to_vec();
        }
        v.iter().map(|x| x / norm).collect()
    }

    /// 原地 L2 归一化
    pub fn l2_normalize_in_place(v: &mut [f32]) {
        let norm = SimilarityAlgorithms::l2_norm(v);
        if norm != 0.0 {
            for x in v.iter_mut() {
                *x /= norm;
            }
        }
    }

    /// 检查向量是否已归一化（L2 范数接近 1）
    pub fn is_normalized(v: &[f32], tolerance: f32) -> bool {
        let norm = SimilarityAlgorithms::l2_norm(v);
        (norm - 1.0).abs() < tolerance
    }

    /// 批量 L2 归一化
    pub fn batch_l2_normalize(vectors: &[Vec<f32>]) -> Vec<Vec<f32>> {
        vectors.iter().map(|v| Self::l2_normalize(v)).collect()
    }
}

// =============================================================================
// 五、批量向量操作 trait
// =============================================================================

/// 批量向量操作扩展 trait
///
/// 提供一次调用处理多个查询/记录的能力，减少网络往返开销。
#[async_trait::async_trait]
pub trait BatchOpsExt: PgVectorStore {
    /// 批量搜索：一次提交多个查询向量
    ///
    /// 返回与查询数量相同的结果列表，每个结果为对应查询的 top_k 结果
    async fn batch_search(
        &self,
        collection: &str,
        queries: &[Vec<f32>],
        top_k: usize,
    ) -> Result<Vec<Vec<SearchResult>>, VectorError>;

    /// 批量获取：一次获取多个 id 的记录
    async fn batch_get(
        &self,
        collection: &str,
        ids: &[String],
    ) -> Result<Vec<Option<VectorRecord>>, VectorError>;

    /// 按元数据过滤搜索
    ///
    /// 仅返回 metadata 中包含指定 key=value 的记录
    async fn search_with_filter(
        &self,
        collection: &str,
        query: &[f32],
        top_k: usize,
        filter_key: &str,
        filter_value: &serde_json::Value,
    ) -> Result<Vec<SearchResult>, VectorError>;
}

// =============================================================================
// 六、内存版批量操作实现
// =============================================================================

/// 内存版批量操作实现
///
/// 包装 `InMemoryVectorStore`，提供批量操作能力。
/// 同时实现 `PgVectorStore` 以保证类型可组合使用。
pub struct MemoryBatchOps {
    store: crate::InMemoryVectorStore,
}

impl MemoryBatchOps {
    pub fn new(store: crate::InMemoryVectorStore) -> Self {
        Self { store }
    }

    pub fn from_new() -> Self {
        Self::new(crate::InMemoryVectorStore::new())
    }

    /// 获取内部 store 引用
    pub fn inner(&self) -> &crate::InMemoryVectorStore {
        &self.store
    }
}

/// 委托实现 `PgVectorStore`，将所有调用转发给内部 store
#[async_trait::async_trait]
impl PgVectorStore for MemoryBatchOps {
    async fn create_collection(
        &self,
        name: &str,
        dimension: usize,
        metric: Option<VectorMetric>,
    ) -> Result<(), VectorError> {
        self.store.create_collection(name, dimension, metric).await
    }

    async fn delete_collection(&self, name: &str) -> Result<(), VectorError> {
        self.store.delete_collection(name).await
    }

    async fn insert(
        &self,
        collection: &str,
        records: Vec<VectorRecord>,
    ) -> Result<(), VectorError> {
        self.store.insert(collection, records).await
    }

    async fn search(
        &self,
        collection: &str,
        query: &[f32],
        top_k: usize,
    ) -> Result<Vec<SearchResult>, VectorError> {
        self.store.search(collection, query, top_k).await
    }

    async fn get(&self, collection: &str, id: &str) -> Result<Option<VectorRecord>, VectorError> {
        self.store.get(collection, id).await
    }

    async fn delete(&self, collection: &str, ids: Vec<String>) -> Result<u64, VectorError> {
        self.store.delete(collection, ids).await
    }

    async fn count(&self, collection: &str) -> Result<usize, VectorError> {
        self.store.count(collection).await
    }
}

#[async_trait::async_trait]
impl BatchOpsExt for MemoryBatchOps {
    async fn batch_search(
        &self,
        collection: &str,
        queries: &[Vec<f32>],
        top_k: usize,
    ) -> Result<Vec<Vec<SearchResult>>, VectorError> {
        let mut all_results = Vec::with_capacity(queries.len());
        for query in queries {
            let results = self.store.search(collection, query, top_k).await?;
            all_results.push(results);
        }
        Ok(all_results)
    }

    async fn batch_get(
        &self,
        collection: &str,
        ids: &[String],
    ) -> Result<Vec<Option<VectorRecord>>, VectorError> {
        let mut results = Vec::with_capacity(ids.len());
        for id in ids {
            let record = self.store.get(collection, id).await?;
            results.push(record);
        }
        Ok(results)
    }

    async fn search_with_filter(
        &self,
        collection: &str,
        query: &[f32],
        top_k: usize,
        filter_key: &str,
        filter_value: &serde_json::Value,
    ) -> Result<Vec<SearchResult>, VectorError> {
        // 先执行普通搜索（取更大的 top_k 以保证过滤后仍有足够结果）
        let expanded_k = top_k.saturating_mul(4).max(top_k);
        let candidates = self.store.search(collection, query, expanded_k).await?;

        // 过滤出匹配元数据的记录
        let filtered: Vec<SearchResult> = candidates
            .into_iter()
            .filter(|r| {
                r.metadata
                    .as_ref()
                    .and_then(|m| m.get(filter_key))
                    .map(|v| v == filter_value)
                    .unwrap_or(false)
            })
            .take(top_k)
            .collect();

        Ok(filtered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // --- ANN 索引类型测试 ---

    #[test]
    fn test_ann_index_type_sql_method() {
        assert_eq!(AnnIndexType::Ivfflat.as_sql_method(), "ivfflat");
        assert_eq!(AnnIndexType::Hnsw.as_sql_method(), "hnsw");
    }

    #[test]
    fn test_ann_index_type_as_str() {
        assert_eq!(AnnIndexType::Ivfflat.as_str(), "IVFFlat");
        assert_eq!(AnnIndexType::Hnsw.as_str(), "HNSW");
    }

    // --- IVFFlat 参数测试 ---

    #[test]
    fn test_ivfflat_params_new() {
        let p = IvfflatParams::new(100, 10);
        assert_eq!(p.lists, 100);
        assert_eq!(p.probes, 10);
    }

    #[test]
    fn test_ivfflat_params_default() {
        let p = IvfflatParams::default();
        assert_eq!(p.lists, 100);
        assert_eq!(p.probes, 10);
    }

    #[test]
    fn test_ivfflat_params_recommended_for_small_dataset() {
        // 10000 行 → sqrt(10000) = 100
        let p = IvfflatParams::recommended_for_rows(10_000);
        assert_eq!(p.lists, 100);
        assert_eq!(p.probes, 10);
    }

    #[test]
    fn test_ivfflat_params_recommended_for_large_dataset() {
        // 2_000_000 行 → rows / 1000 = 2000
        let p = IvfflatParams::recommended_for_rows(2_000_000);
        assert_eq!(p.lists, 2000);
    }

    #[test]
    fn test_ivfflat_params_recommended_for_empty() {
        let p = IvfflatParams::recommended_for_rows(0);
        assert_eq!(p.lists, 1); // 至少 1
    }

    #[test]
    fn test_ivfflat_params_to_sql_options() {
        let p = IvfflatParams::new(50, 5);
        assert_eq!(p.to_sql_options(), "(lists = 50)");
    }

    // --- HNSW 参数测试 ---

    #[test]
    fn test_hnsw_params_new() {
        let p = HnswParams::new(32, 128);
        assert_eq!(p.m, 32);
        assert_eq!(p.ef_construction, 128);
    }

    #[test]
    fn test_hnsw_params_default() {
        let p = HnswParams::default();
        assert_eq!(p.m, 16);
        assert_eq!(p.ef_construction, 64);
    }

    #[test]
    fn test_hnsw_params_to_sql_options() {
        let p = HnswParams::new(32, 128);
        assert_eq!(p.to_sql_options(), "(m = 32, ef_construction = 128)");
    }

    // --- ANN 索引定义测试 ---

    #[test]
    fn test_ann_index_def_new_ivfflat() {
        let def = AnnIndexDef::new_ivfflat(
            "docs",
            VectorMetric::Cosine,
            IvfflatParams::new(100, 10),
        );
        assert_eq!(def.index_name, "idx_docs_ivfflat");
        assert_eq!(def.index_type, AnnIndexType::Ivfflat);
        assert_eq!(def.metric, VectorMetric::Cosine);
        assert!(def.ivfflat_params.is_some());
        assert!(def.hnsw_params.is_none());
        assert!(!def.concurrently);
    }

    #[test]
    fn test_ann_index_def_new_hnsw() {
        let def = AnnIndexDef::new_hnsw("docs", VectorMetric::Euclidean, HnswParams::default());
        assert_eq!(def.index_name, "idx_docs_hnsw");
        assert_eq!(def.index_type, AnnIndexType::Hnsw);
        assert_eq!(def.metric, VectorMetric::Euclidean);
        assert!(def.ivfflat_params.is_none());
        assert!(def.hnsw_params.is_some());
    }

    #[test]
    fn test_ann_index_def_with_concurrently() {
        let def = AnnIndexDef::new_hnsw("docs", VectorMetric::Cosine, HnswParams::default())
            .with_concurrently();
        assert!(def.concurrently);
    }

    #[test]
    fn test_ann_index_def_to_create_sql_ivfflat() {
        let def = AnnIndexDef::new_ivfflat(
            "docs",
            VectorMetric::Cosine,
            IvfflatParams::new(100, 10),
        );
        let sql = def.to_create_sql();
        assert!(sql.contains("CREATE INDEX"));
        assert!(sql.contains("idx_docs_ivfflat"));
        assert!(sql.contains("USING ivfflat"));
        assert!(sql.contains("vector_cosine_ops"));
        assert!(sql.contains("WITH (lists = 100)"));
    }

    #[test]
    fn test_ann_index_def_to_create_sql_hnsw() {
        let def =
            AnnIndexDef::new_hnsw("docs", VectorMetric::Euclidean, HnswParams::new(16, 64));
        let sql = def.to_create_sql();
        assert!(sql.contains("USING hnsw"));
        assert!(sql.contains("vector_l2_ops"));
        assert!(sql.contains("WITH (m = 16, ef_construction = 64)"));
    }

    #[test]
    fn test_ann_index_def_to_drop_sql() {
        let def = AnnIndexDef::new_ivfflat(
            "docs",
            VectorMetric::DotProduct,
            IvfflatParams::default(),
        );
        let sql = def.to_drop_sql();
        assert_eq!(sql, "DROP INDEX idx_docs_ivfflat");
    }

    #[test]
    fn test_ann_index_def_to_reindex_sql() {
        let def = AnnIndexDef::new_hnsw("docs", VectorMetric::Cosine, HnswParams::default());
        let sql = def.to_reindex_sql();
        assert_eq!(sql, "REINDEX INDEX idx_docs_hnsw");
    }

    // --- ANN 索引注册表测试 ---

    #[test]
    fn test_ann_index_registry_register_and_exists() {
        let mut reg = AnnIndexRegistry::new();
        let def = AnnIndexDef::new_ivfflat(
            "docs",
            VectorMetric::Cosine,
            IvfflatParams::default(),
        );
        reg.register(def).unwrap();
        assert!(reg.exists("idx_docs_ivfflat"));
        assert!(!reg.exists("nonexistent"));
    }

    #[test]
    fn test_ann_index_registry_duplicate_register_fails() {
        let mut reg = AnnIndexRegistry::new();
        let def = AnnIndexDef::new_ivfflat(
            "docs",
            VectorMetric::Cosine,
            IvfflatParams::default(),
        );
        reg.register(def).unwrap();
        // 重复注册应失败
        let def2 = AnnIndexDef::new_ivfflat(
            "docs",
            VectorMetric::Cosine,
            IvfflatParams::default(),
        );
        assert!(reg.register(def2).is_err());
    }

    #[test]
    fn test_ann_index_registry_unregister() {
        let mut reg = AnnIndexRegistry::new();
        let def = AnnIndexDef::new_ivfflat(
            "docs",
            VectorMetric::Cosine,
            IvfflatParams::default(),
        );
        reg.register(def).unwrap();
        let removed = reg.unregister("idx_docs_ivfflat").unwrap();
        assert_eq!(removed.collection, "docs");
        assert!(!reg.exists("idx_docs_ivfflat"));
    }

    #[test]
    fn test_ann_index_registry_unregister_nonexistent_fails() {
        let mut reg = AnnIndexRegistry::new();
        assert!(reg.unregister("nonexistent").is_err());
    }

    #[test]
    fn test_ann_index_registry_list_for_collection() {
        let mut reg = AnnIndexRegistry::new();
        reg.register(AnnIndexDef::new_ivfflat(
            "docs",
            VectorMetric::Cosine,
            IvfflatParams::default(),
        ))
        .unwrap();
        reg.register(AnnIndexDef::new_hnsw(
            "docs",
            VectorMetric::Euclidean,
            HnswParams::default(),
        ))
        .unwrap();
        reg.register(AnnIndexDef::new_ivfflat(
            "images",
            VectorMetric::Cosine,
            IvfflatParams::default(),
        ))
        .unwrap();

        let docs_indexes = reg.list_for_collection("docs");
        assert_eq!(docs_indexes.len(), 2);
        let images_indexes = reg.list_for_collection("images");
        assert_eq!(images_indexes.len(), 1);
    }

    #[test]
    fn test_ann_index_registry_list_all() {
        let mut reg = AnnIndexRegistry::new();
        reg.register(AnnIndexDef::new_ivfflat(
            "docs",
            VectorMetric::Cosine,
            IvfflatParams::default(),
        ))
        .unwrap();
        reg.register(AnnIndexDef::new_hnsw(
            "images",
            VectorMetric::Euclidean,
            HnswParams::default(),
        ))
        .unwrap();
        assert_eq!(reg.list_all().len(), 2);
    }

    #[test]
    fn test_ann_index_registry_reindex_all_sql() {
        let mut reg = AnnIndexRegistry::new();
        reg.register(AnnIndexDef::new_ivfflat(
            "docs",
            VectorMetric::Cosine,
            IvfflatParams::default(),
        ))
        .unwrap();
        reg.register(AnnIndexDef::new_hnsw(
            "images",
            VectorMetric::Euclidean,
            HnswParams::default(),
        ))
        .unwrap();
        let sqls = reg.reindex_all_sql();
        assert_eq!(sqls.len(), 2);
        assert!(sqls[0].contains("REINDEX INDEX"));
    }

    // --- 相似度算法测试 ---

    #[test]
    fn test_l2_distance_identical() {
        let a = vec![1.0, 2.0, 3.0];
        assert!((SimilarityAlgorithms::l2_distance(&a, &a)).abs() < 1e-10);
    }

    #[test]
    fn test_l2_distance_known() {
        let a = vec![0.0, 0.0];
        let b = vec![3.0, 4.0];
        // 3-4-5 直角三角形
        assert!((SimilarityAlgorithms::l2_distance(&a, &b) - 5.0).abs() < 1e-6);
    }

    #[test]
    fn test_l2_distance_mismatched_dims() {
        assert_eq!(SimilarityAlgorithms::l2_distance(&[1.0], &[1.0, 2.0]), f32::MAX);
    }

    #[test]
    fn test_inner_product() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![4.0, 5.0, 6.0];
        // 1*4 + 2*5 + 3*6 = 32
        assert!((SimilarityAlgorithms::inner_product(&a, &b) - 32.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 2.0, 3.0];
        assert!((SimilarityAlgorithms::cosine_similarity(&a, &a) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!(SimilarityAlgorithms::cosine_similarity(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        assert!((SimilarityAlgorithms::cosine_similarity(&a, &b) + 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let a = vec![0.0, 0.0];
        let b = vec![1.0, 0.0];
        assert_eq!(SimilarityAlgorithms::cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_manhattan_distance() {
        let a = vec![0.0, 0.0];
        let b = vec![3.0, 4.0];
        assert!((SimilarityAlgorithms::manhattan_distance(&a, &b) - 7.0).abs() < 1e-6);
    }

    #[test]
    fn test_l2_norm() {
        let v = vec![3.0, 4.0];
        assert!((SimilarityAlgorithms::l2_norm(&v) - 5.0).abs() < 1e-6);
    }

    #[test]
    fn test_l2_norm_zero() {
        assert_eq!(SimilarityAlgorithms::l2_norm(&[0.0, 0.0]), 0.0);
    }

    #[test]
    fn test_distance_to_similarity_cosine() {
        // 距离 0 → 相似度 1
        assert!((SimilarityAlgorithms::distance_to_similarity(
            VectorMetric::Cosine,
            0.0
        ) - 1.0)
            .abs()
            < 1e-6);
        // 距离 1 → 相似度 0
        assert!((SimilarityAlgorithms::distance_to_similarity(
            VectorMetric::Cosine,
            1.0
        ))
            .abs()
            < 1e-6);
    }

    #[test]
    fn test_distance_to_similarity_euclidean() {
        // 距离 0 → 相似度 1
        assert!((SimilarityAlgorithms::distance_to_similarity(
            VectorMetric::Euclidean,
            0.0
        ) - 1.0)
            .abs()
            < 1e-6);
        // 距离 1 → 相似度 0.5
        assert!((SimilarityAlgorithms::distance_to_similarity(
            VectorMetric::Euclidean,
            1.0
        ) - 0.5)
            .abs()
            < 1e-6);
    }

    #[test]
    fn test_similarity_by_metric_cosine() {
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 0.0];
        assert!((SimilarityAlgorithms::similarity(VectorMetric::Cosine, &a, &b) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_similarity_by_metric_dot_product() {
        let a = vec![2.0, 3.0];
        let b = vec![1.0, 1.0];
        // 2 + 3 = 5
        assert!((SimilarityAlgorithms::similarity(VectorMetric::DotProduct, &a, &b) - 5.0).abs() < 1e-6);
    }

    #[test]
    fn test_batch_similarity() {
        let query = vec![1.0, 0.0];
        let candidates = vec![vec![1.0, 0.0], vec![0.0, 1.0], vec![1.0, 1.0]];
        let scored = SimilarityAlgorithms::batch_similarity(
            VectorMetric::Cosine,
            &query,
            &candidates,
        );
        assert_eq!(scored.len(), 3);
        // 第一个候选应最相似
        assert_eq!(scored[0].0, 0);
        assert!((scored[0].1 - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_batch_top_k() {
        let query = vec![1.0, 0.0];
        let candidates = vec![
            vec![0.0, 1.0],
            vec![1.0, 0.0],
            vec![1.0, 1.0],
            vec![0.9, 0.1],
        ];
        let top = SimilarityAlgorithms::batch_top_k(
            VectorMetric::Cosine,
            &query,
            &candidates,
            2,
        );
        assert_eq!(top.len(), 2);
        // 最相似的应是索引 1 (1,0) 或 3 (0.9,0.1)
        assert_eq!(top[0].0, 1);
    }

    // --- 维度验证测试 ---

    #[test]
    fn test_validate_dimension_valid() {
        assert!(DimensionValidator::validate_dimension(1).is_ok());
        assert!(DimensionValidator::validate_dimension(128).is_ok());
        assert!(DimensionValidator::validate_dimension(1536).is_ok());
        assert!(DimensionValidator::validate_dimension(MAX_VECTOR_DIMENSION).is_ok());
    }

    #[test]
    fn test_validate_dimension_invalid() {
        assert!(DimensionValidator::validate_dimension(0).is_err());
        assert!(DimensionValidator::validate_dimension(MAX_VECTOR_DIMENSION + 1).is_err());
    }

    #[test]
    fn test_validate_vector_match() {
        let v = vec![1.0, 2.0, 3.0];
        assert!(DimensionValidator::validate_vector(&v, 3).is_ok());
    }

    #[test]
    fn test_validate_vector_mismatch() {
        let v = vec![1.0, 2.0];
        let result = DimensionValidator::validate_vector(&v, 3);
        assert!(matches!(result, Err(VectorError::DimensionMismatch { .. })));
    }

    #[test]
    fn test_validate_batch_all_match() {
        let vectors = vec![vec![1.0, 0.0], vec![0.0, 1.0], vec![1.0, 1.0]];
        assert!(DimensionValidator::validate_batch(&vectors, 2).is_ok());
    }

    #[test]
    fn test_validate_batch_one_mismatch() {
        let vectors = vec![vec![1.0, 0.0], vec![0.0, 1.0, 0.0]];
        let result = DimensionValidator::validate_batch(&vectors, 2);
        assert!(matches!(result, Err(VectorError::DimensionMismatch { .. })));
    }

    #[test]
    fn test_validate_batch_with_nan() {
        let vectors = vec![vec![1.0, f32::NAN]];
        let result = DimensionValidator::validate_batch(&vectors, 2);
        assert!(matches!(result, Err(VectorError::InvalidConfig(_))));
    }

    #[test]
    fn test_validate_batch_with_inf() {
        let vectors = vec![vec![1.0, f32::INFINITY]];
        let result = DimensionValidator::validate_batch(&vectors, 2);
        assert!(matches!(result, Err(VectorError::InvalidConfig(_))));
    }

    #[test]
    fn test_validate_collection_compatibility_match() {
        assert!(DimensionValidator::validate_collection_compatibility(128, 128).is_ok());
    }

    #[test]
    fn test_validate_collection_compatibility_mismatch() {
        let result = DimensionValidator::validate_collection_compatibility(128, 256);
        assert!(matches!(result, Err(VectorError::DimensionMismatch { .. })));
    }

    #[test]
    fn test_validate_query_match() {
        let query = vec![1.0, 2.0, 3.0];
        assert!(DimensionValidator::validate_query(&query, 3).is_ok());
    }

    #[test]
    fn test_validate_query_mismatch() {
        let query = vec![1.0, 2.0];
        assert!(DimensionValidator::validate_query(&query, 3).is_err());
    }

    // --- 向量归一化测试 ---

    #[test]
    fn test_l2_normalize_unit_vector() {
        let v = vec![1.0, 0.0];
        let n = VectorNormalizer::l2_normalize(&v);
        assert!((n[0] - 1.0).abs() < 1e-6);
        assert!(n[1].abs() < 1e-6);
    }

    #[test]
    fn test_l2_normalize_non_unit() {
        let v = vec![3.0, 4.0];
        let n = VectorNormalizer::l2_normalize(&v);
        // 归一化后应为 [0.6, 0.8]
        assert!((n[0] - 0.6).abs() < 1e-6);
        assert!((n[1] - 0.8).abs() < 1e-6);
    }

    #[test]
    fn test_l2_normalize_zero_vector() {
        let v = vec![0.0, 0.0];
        let n = VectorNormalizer::l2_normalize(&v);
        // 零向量保持不变
        assert_eq!(n, vec![0.0, 0.0]);
    }

    #[test]
    fn test_l2_normalize_in_place() {
        let mut v = vec![3.0, 4.0];
        VectorNormalizer::l2_normalize_in_place(&mut v);
        assert!((v[0] - 0.6).abs() < 1e-6);
        assert!((v[1] - 0.8).abs() < 1e-6);
    }

    #[test]
    fn test_is_normalized_true() {
        let v = vec![1.0, 0.0];
        assert!(VectorNormalizer::is_normalized(&v, 1e-5));
    }

    #[test]
    fn test_is_normalized_false() {
        let v = vec![3.0, 4.0];
        assert!(!VectorNormalizer::is_normalized(&v, 1e-5));
    }

    #[test]
    fn test_batch_l2_normalize() {
        let vectors = vec![vec![3.0, 4.0], vec![1.0, 0.0]];
        let normalized = VectorNormalizer::batch_l2_normalize(&vectors);
        assert!((normalized[0][0] - 0.6).abs() < 1e-6);
        assert!((normalized[0][1] - 0.8).abs() < 1e-6);
        assert!((normalized[1][0] - 1.0).abs() < 1e-6);
    }

    // --- 批量操作测试 ---

    #[tokio::test]
    async fn test_memory_batch_ops_batch_search() {
        let batch_ops = MemoryBatchOps::from_new();
        batch_ops
            .store
            .create_collection("docs", 3, Some(VectorMetric::Cosine))
            .await
            .unwrap();
        batch_ops
            .store
            .insert(
                "docs",
                vec![
                    VectorRecord::new("a", vec![1.0, 0.0, 0.0]),
                    VectorRecord::new("b", vec![0.0, 1.0, 0.0]),
                    VectorRecord::new("c", vec![0.0, 0.0, 1.0]),
                ],
            )
            .await
            .unwrap();

        let queries = vec![vec![1.0, 0.0, 0.0], vec![0.0, 0.0, 1.0]];
        let results = batch_ops.batch_search("docs", &queries, 2).await.unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0][0].id, "a");
        assert_eq!(results[1][0].id, "c");
    }

    #[tokio::test]
    async fn test_memory_batch_ops_batch_get() {
        let batch_ops = MemoryBatchOps::from_new();
        batch_ops
            .store
            .create_collection("docs", 2, None)
            .await
            .unwrap();
        batch_ops
            .store
            .insert(
                "docs",
                vec![
                    VectorRecord::new("a", vec![1.0, 0.0]),
                    VectorRecord::new("b", vec![0.0, 1.0]),
                ],
            )
            .await
            .unwrap();

        let ids = vec!["a".to_string(), "missing".to_string(), "b".to_string()];
        let results = batch_ops.batch_get("docs", &ids).await.unwrap();
        assert_eq!(results.len(), 3);
        assert!(results[0].is_some());
        assert!(results[1].is_none());
        assert!(results[2].is_some());
    }

    #[tokio::test]
    async fn test_memory_batch_ops_search_with_filter() {
        let batch_ops = MemoryBatchOps::from_new();
        batch_ops
            .store
            .create_collection("docs", 2, Some(VectorMetric::Cosine))
            .await
            .unwrap();

        let mut meta_a = HashMap::new();
        meta_a.insert("category".to_string(), json!("science"));
        let mut meta_b = HashMap::new();
        meta_b.insert("category".to_string(), json!("sports"));
        let mut meta_c = HashMap::new();
        meta_c.insert("category".to_string(), json!("science"));

        batch_ops
            .store
            .insert(
                "docs",
                vec![
                    VectorRecord::new("a", vec![1.0, 0.0]).with_metadata(meta_a),
                    VectorRecord::new("b", vec![0.0, 1.0]).with_metadata(meta_b),
                    VectorRecord::new("c", vec![0.9, 0.1]).with_metadata(meta_c),
                ],
            )
            .await
            .unwrap();

        let results = batch_ops
            .search_with_filter("docs", &[1.0, 0.0], 5, "category", &json!("science"))
            .await
            .unwrap();

        // 只应返回 a 和 c（category = science）
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.id == "a" || r.id == "c"));
    }

    #[tokio::test]
    async fn test_memory_batch_ops_filter_no_match() {
        let batch_ops = MemoryBatchOps::from_new();
        batch_ops
            .store
            .create_collection("docs", 2, None)
            .await
            .unwrap();

        let mut meta = HashMap::new();
        meta.insert("category".to_string(), json!("science"));
        batch_ops
            .store
            .insert(
                "docs",
                vec![VectorRecord::new("a", vec![1.0, 0.0]).with_metadata(meta)],
            )
            .await
            .unwrap();

        let results = batch_ops
            .search_with_filter("docs", &[1.0, 0.0], 5, "category", &json!("nonexistent"))
            .await
            .unwrap();
        assert!(results.is_empty());
    }
}
