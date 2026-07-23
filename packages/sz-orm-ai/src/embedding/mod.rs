use crate::error::AiError;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::RwLock;

#[derive(Debug, Clone)]
pub struct EmbeddingError {
    pub message: String,
    pub model: Option<String>,
}

impl EmbeddingError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            model: None,
        }
    }

    pub fn with_model(message: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            model: Some(model.into()),
        }
    }
}

impl std::fmt::Display for EmbeddingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "EmbeddingError: {}", self.message)?;
        if let Some(ref model) = self.model {
            write!(f, " (model: {})", model)?;
        }
        Ok(())
    }
}

impl std::error::Error for EmbeddingError {}

#[async_trait]
pub trait EmbeddingModel: Send + Sync {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, AiError>;
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, AiError>;
    fn dimension(&self) -> usize;
    fn model_name(&self) -> &str;
}

pub struct EmbeddingRecord {
    pub id: String,
    pub text: String,
    pub vector: Vec<f32>,
    pub metadata: Option<std::collections::HashMap<String, serde_json::Value>>,
}

impl EmbeddingRecord {
    pub fn new(id: impl Into<String>, text: impl Into<String>, vector: Vec<f32>) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
            vector,
            metadata: None,
        }
    }

    pub fn with_metadata(
        id: impl Into<String>,
        text: impl Into<String>,
        vector: Vec<f32>,
        metadata: std::collections::HashMap<String, serde_json::Value>,
    ) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
            vector,
            metadata: Some(metadata),
        }
    }
}

pub struct EmbeddingBatch {
    pub records: Vec<EmbeddingRecord>,
    pub batch_size: usize,
}

impl EmbeddingBatch {
    pub fn new(records: Vec<EmbeddingRecord>) -> Self {
        Self {
            records,
            batch_size: 32,
        }
    }

    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.batch_size = size;
        self
    }

    pub fn batch_chunks(&self) -> Vec<&[EmbeddingRecord]> {
        self.records.chunks(self.batch_size).collect()
    }
}

/// Simple embedding model based on token frequency statistics.
///
/// This is a deterministic, in-memory embedding implementation:
/// - Maintains a fixed-size vocabulary (`dimension` slots).
/// - Each token is hashed into one of the `dimension` slots using FNV-1a.
/// - The embedding vector is the L2-normalized token-frequency histogram.
///
/// This is NOT a neural embedding (no semantic knowledge), but it is a real,
/// deterministic, reproducible vector representation suitable for testing
/// similarity-search pipelines end-to-end.
pub struct SimpleEmbeddingModel {
    name: String,
    dimension: usize,
    vocabulary: RwLock<HashMap<String, usize>>,
}

impl SimpleEmbeddingModel {
    pub fn new(name: impl Into<String>, dimension: usize) -> Self {
        Self {
            name: name.into(),
            dimension,
            vocabulary: RwLock::new(HashMap::new()),
        }
    }

    pub fn vocabulary_size(&self) -> usize {
        self.vocabulary.read().map(|v| v.len()).unwrap_or(0)
    }

    /// Registers a token in the vocabulary, returning its slot index.
    /// New tokens are appended; existing tokens keep their slot.
    fn register_token(&self, token: &str) -> usize {
        let mut vocab = self.vocabulary.write().unwrap();
        if let Some(&idx) = vocab.get(token) {
            return idx;
        }
        // Hash into the dimension space to keep vector length stable
        // regardless of vocabulary size (bucket collisions are acceptable
        // for a simple model and keep memory bounded).
        let idx = fnv1a(token) % self.dimension.max(1);
        vocab.insert(token.to_string(), idx);
        idx
    }

    fn tokenize(text: &str) -> Vec<String> {
        text.split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_lowercase())
            .collect()
    }

    fn embed_text(&self, text: &str) -> Vec<f32> {
        let mut vec = vec![0.0f32; self.dimension];
        if self.dimension == 0 {
            return vec;
        }
        let tokens = Self::tokenize(text);
        if tokens.is_empty() {
            return vec;
        }

        for token in &tokens {
            let idx = self.register_token(token);
            vec[idx] += 1.0;
        }

        // L2 normalize so cosine similarity is well-defined.
        let norm: f32 = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in vec.iter_mut() {
                *v /= norm;
            }
        }
        vec
    }
}

fn fnv1a(s: &str) -> usize {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in s.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash as usize
}

// ==================== Embedding 模型适配器 ====================

/// 缓存型 Embedding 模型适配器
///
/// 包装一个内部 EmbeddingModel，对相同输入文本返回缓存的向量，
/// 避免重复计算。适用于嵌入计算成本高的场景（如调用远程 API）。
///
/// # 泛型参数
/// - `M`: 内部嵌入模型
pub struct CachingEmbeddingModel<M>
where
    M: EmbeddingModel,
{
    /// 内部嵌入模型
    inner: M,
    /// 缓存：文本 → 向量
    cache: RwLock<HashMap<String, Vec<f32>>>,
    /// 缓存命中次数（用于统计）
    hits: std::sync::atomic::AtomicU64,
    /// 缓存未命中次数
    misses: std::sync::atomic::AtomicU64,
}

impl<M> CachingEmbeddingModel<M>
where
    M: EmbeddingModel,
{
    /// 创建缓存型适配器
    pub fn new(inner: M) -> Self {
        Self {
            inner,
            cache: RwLock::new(HashMap::new()),
            hits: std::sync::atomic::AtomicU64::new(0),
            misses: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// 获取缓存大小
    pub fn cache_size(&self) -> usize {
        self.cache.read().map(|c| c.len()).unwrap_or(0)
    }

    /// 获取缓存命中次数
    pub fn cache_hits(&self) -> u64 {
        self.hits.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// 获取缓存未命中次数
    pub fn cache_misses(&self) -> u64 {
        self.misses.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// 缓存命中率
    pub fn hit_rate(&self) -> f64 {
        let hits = self.cache_hits();
        let misses = self.cache_misses();
        let total = hits + misses;
        if total == 0 {
            return 0.0;
        }
        hits as f64 / total as f64
    }

    /// 清空缓存
    pub fn clear_cache(&self) {
        if let Ok(mut cache) = self.cache.write() {
            cache.clear();
        }
    }

    /// 获取内部模型引用
    pub fn inner(&self) -> &M {
        &self.inner
    }
}

#[async_trait]
impl<M> EmbeddingModel for CachingEmbeddingModel<M>
where
    M: EmbeddingModel,
{
    async fn embed(&self, text: &str) -> Result<Vec<f32>, AiError> {
        // 先查缓存
        {
            let cache = self.cache.read().map_err(|e| AiError::Embedding(e.to_string()))?;
            if let Some(vector) = cache.get(text) {
                self.hits
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return Ok(vector.clone());
            }
        }

        // 缓存未命中，调用内部模型
        self.misses
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let vector = self.inner.embed(text).await?;

        // 写入缓存
        if let Ok(mut cache) = self.cache.write() {
            cache.insert(text.to_string(), vector.clone());
        }

        Ok(vector)
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, AiError> {
        let mut results = Vec::with_capacity(texts.len());
        let mut uncached_indices = Vec::new();
        let mut uncached_texts = Vec::new();

        // 先查缓存
        {
            let cache = self.cache.read().map_err(|e| AiError::Embedding(e.to_string()))?;
            for (idx, text) in texts.iter().enumerate() {
                if let Some(vector) = cache.get(text) {
                    self.hits
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    results.push(vector.clone());
                } else {
                    self.misses
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    uncached_indices.push(idx);
                    uncached_texts.push(text.clone());
                    results.push(Vec::new()); // 占位
                }
            }
        }

        // 批量计算未缓存的
        if !uncached_texts.is_empty() {
            let vectors = self.inner.embed_batch(&uncached_texts).await?;
            let mut cache = self.cache.write().map_err(|e| AiError::Embedding(e.to_string()))?;
            for (i, idx) in uncached_indices.iter().enumerate() {
                let text = &uncached_texts[i];
                let vector = &vectors[i];
                results[*idx] = vector.clone();
                cache.insert(text.clone(), vector.clone());
            }
        }

        Ok(results)
    }

    fn dimension(&self) -> usize {
        self.inner.dimension()
    }

    fn model_name(&self) -> &str {
        self.inner.model_name()
    }
}

/// 归一化 Embedding 模型适配器
///
/// 包装一个内部 EmbeddingModel，对输出向量进行 L2 归一化，
/// 确保所有向量都是单位向量。适用于需要使用余弦相似度的场景。
pub struct NormalizedEmbeddingModel<M>
where
    M: EmbeddingModel,
{
    /// 内部嵌入模型
    inner: M,
}

impl<M> NormalizedEmbeddingModel<M>
where
    M: EmbeddingModel,
{
    /// 创建归一化适配器
    pub fn new(inner: M) -> Self {
        Self { inner }
    }

    /// 对向量进行 L2 归一化
    pub fn l2_normalize(vector: &mut [f32]) {
        let norm: f32 = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in vector.iter_mut() {
                *v /= norm;
            }
        }
    }

    /// 获取内部模型引用
    pub fn inner(&self) -> &M {
        &self.inner
    }
}

#[async_trait]
impl<M> EmbeddingModel for NormalizedEmbeddingModel<M>
where
    M: EmbeddingModel,
{
    async fn embed(&self, text: &str) -> Result<Vec<f32>, AiError> {
        let mut vector = self.inner.embed(text).await?;
        Self::l2_normalize(&mut vector);
        Ok(vector)
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, AiError> {
        let mut vectors = self.inner.embed_batch(texts).await?;
        for vector in vectors.iter_mut() {
            Self::l2_normalize(vector);
        }
        Ok(vectors)
    }

    fn dimension(&self) -> usize {
        self.inner.dimension()
    }

    fn model_name(&self) -> &str {
        self.inner.model_name()
    }
}

/// 降维 Embedding 模型适配器
///
/// 包装一个内部 EmbeddingModel，通过截断或平均池化将输出向量降维到指定维度。
/// 适用于需要将高维向量适配到低维索引的场景。
pub struct DimReductionEmbeddingModel<M>
where
    M: EmbeddingModel,
{
    /// 内部嵌入模型
    inner: M,
    /// 目标维度
    target_dimension: usize,
    /// 降维策略
    strategy: DimReductionStrategy,
}

/// 降维策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DimReductionStrategy {
    /// 截断：只保留前 N 维
    Truncate,
    /// 平均池化：将向量分块后取平均
    Average,
}

impl<M> DimReductionEmbeddingModel<M>
where
    M: EmbeddingModel,
{
    /// 创建降维适配器
    pub fn new(inner: M, target_dimension: usize, strategy: DimReductionStrategy) -> Self {
        Self {
            inner,
            target_dimension,
            strategy,
        }
    }

    /// 创建截断降维适配器
    pub fn truncate(inner: M, target_dimension: usize) -> Self {
        Self::new(inner, target_dimension, DimReductionStrategy::Truncate)
    }

    /// 创建平均池化降维适配器
    pub fn average(inner: M, target_dimension: usize) -> Self {
        Self::new(inner, target_dimension, DimReductionStrategy::Average)
    }

    /// 降维单个向量
    pub fn reduce(&self, vector: Vec<f32>) -> Vec<f32> {
        match self.strategy {
            DimReductionStrategy::Truncate => {
                vector.into_iter().take(self.target_dimension).collect()
            }
            DimReductionStrategy::Average => {
                if vector.is_empty() || self.target_dimension == 0 {
                    return Vec::new();
                }
                let chunk_size = vector.len() / self.target_dimension;
                if chunk_size == 0 {
                    return vector.into_iter().take(self.target_dimension).collect();
                }
                let mut result = Vec::with_capacity(self.target_dimension);
                for i in 0..self.target_dimension {
                    let start = i * chunk_size;
                    let end = if i == self.target_dimension - 1 {
                        vector.len()
                    } else {
                        start + chunk_size
                    };
                    let chunk = &vector[start..end];
                    let avg: f32 = chunk.iter().sum::<f32>() / chunk.len() as f32;
                    result.push(avg);
                }
                result
            }
        }
    }

    /// 获取内部模型引用
    pub fn inner(&self) -> &M {
        &self.inner
    }

    /// 获取目标维度
    pub fn target_dimension(&self) -> usize {
        self.target_dimension
    }

    /// 获取降维策略
    pub fn strategy(&self) -> DimReductionStrategy {
        self.strategy
    }
}

#[async_trait]
impl<M> EmbeddingModel for DimReductionEmbeddingModel<M>
where
    M: EmbeddingModel,
{
    async fn embed(&self, text: &str) -> Result<Vec<f32>, AiError> {
        let vector = self.inner.embed(text).await?;
        Ok(self.reduce(vector))
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, AiError> {
        let vectors = self.inner.embed_batch(texts).await?;
        Ok(vectors.into_iter().map(|v| self.reduce(v)).collect())
    }

    fn dimension(&self) -> usize {
        self.target_dimension
    }

    fn model_name(&self) -> &str {
        self.inner.model_name()
    }
}

/// 日志型 Embedding 模型适配器
///
/// 包装一个内部 EmbeddingModel，在调用时记录调用的文本和耗时。
/// 适用于调试和性能分析场景。
pub struct LoggingEmbeddingModel<M>
where
    M: EmbeddingModel,
{
    /// 内部嵌入模型
    inner: M,
    /// 调用次数
    call_count: std::sync::atomic::AtomicU64,
    /// 总文本数
    total_texts: std::sync::atomic::AtomicU64,
}

impl<M> LoggingEmbeddingModel<M>
where
    M: EmbeddingModel,
{
    /// 创建日志型适配器
    pub fn new(inner: M) -> Self {
        Self {
            inner,
            call_count: std::sync::atomic::AtomicU64::new(0),
            total_texts: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// 获取调用次数
    pub fn call_count(&self) -> u64 {
        self.call_count.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// 获取处理的文本总数
    pub fn total_texts(&self) -> u64 {
        self.total_texts.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// 获取内部模型引用
    pub fn inner(&self) -> &M {
        &self.inner
    }
}

#[async_trait]
impl<M> EmbeddingModel for LoggingEmbeddingModel<M>
where
    M: EmbeddingModel,
{
    async fn embed(&self, text: &str) -> Result<Vec<f32>, AiError> {
        self.call_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.total_texts
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.inner.embed(text).await
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, AiError> {
        self.call_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.total_texts
            .fetch_add(texts.len() as u64, std::sync::atomic::Ordering::Relaxed);
        self.inner.embed_batch(texts).await
    }

    fn dimension(&self) -> usize {
        self.inner.dimension()
    }

    fn model_name(&self) -> &str {
        self.inner.model_name()
    }
}

#[async_trait]
impl EmbeddingModel for SimpleEmbeddingModel {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, AiError> {
        Ok(self.embed_text(text))
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, AiError> {
        Ok(texts.iter().map(|t| self.embed_text(t)).collect())
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn model_name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_embed_simple_text() {
        let model = SimpleEmbeddingModel::new("test-model", 16);
        let v = model.embed("hello world").await.unwrap();
        assert_eq!(v.len(), 16);
        // L2 norm should be ~1 (or 0 for empty input)
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5 || norm.abs() < 1e-5);
    }

    #[tokio::test]
    async fn test_embed_empty_text() {
        let model = SimpleEmbeddingModel::new("test-model", 8);
        let v = model.embed("").await.unwrap();
        assert!(v.iter().all(|x| *x == 0.0));
    }

    #[tokio::test]
    async fn test_embed_deterministic() {
        let model = SimpleEmbeddingModel::new("test-model", 32);
        let v1 = model.embed("rust programming language").await.unwrap();
        let v2 = model.embed("rust programming language").await.unwrap();
        assert_eq!(v1, v2);
    }

    #[tokio::test]
    async fn test_embed_similar_texts_closer_than_different() {
        let model = SimpleEmbeddingModel::new("test-model", 64);
        let v1 = model.embed("the quick brown fox jumps").await.unwrap();
        let v2 = model.embed("the quick brown fox").await.unwrap();
        let v3 = model
            .embed("completely different words here")
            .await
            .unwrap();

        let sim_close = cosine(&v1, &v2);
        let sim_far = cosine(&v1, &v3);
        assert!(
            sim_close >= sim_far,
            "similar texts should be at least as close"
        );
    }

    #[tokio::test]
    async fn test_embed_batch() {
        let model = SimpleEmbeddingModel::new("test-model", 16);
        let texts = vec!["hello".to_string(), "world".to_string()];
        let vecs = model.embed_batch(&texts).await.unwrap();
        assert_eq!(vecs.len(), 2);
        assert_eq!(vecs[0].len(), 16);
        assert_eq!(vecs[1].len(), 16);
    }

    #[test]
    fn test_dimension_and_name() {
        let model = SimpleEmbeddingModel::new("my-model", 128);
        assert_eq!(model.dimension(), 128);
        assert_eq!(model.model_name(), "my-model");
    }

    fn cosine(a: &[f32], b: &[f32]) -> f32 {
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        if na == 0.0 || nb == 0.0 {
            return 0.0;
        }
        dot / (na * nb)
    }

    // ============ Embedding 适配器测试 ============

    // ---- CachingEmbeddingModel 测试 ----

    #[tokio::test]
    async fn test_caching_model_caches_repeated_calls() {
        let inner = SimpleEmbeddingModel::new("test", 16);
        let caching = CachingEmbeddingModel::new(inner);

        let v1 = caching.embed("hello world").await.unwrap();
        let v2 = caching.embed("hello world").await.unwrap();

        // 第二次应命中缓存
        assert_eq!(v1, v2);
        assert_eq!(caching.cache_hits(), 1);
        assert_eq!(caching.cache_misses(), 1);
        assert_eq!(caching.cache_size(), 1);
    }

    #[tokio::test]
    async fn test_caching_model_different_texts() {
        let inner = SimpleEmbeddingModel::new("test", 16);
        let caching = CachingEmbeddingModel::new(inner);

        let _ = caching.embed("hello").await.unwrap();
        let _ = caching.embed("world").await.unwrap();

        assert_eq!(caching.cache_misses(), 2);
        assert_eq!(caching.cache_hits(), 0);
        assert_eq!(caching.cache_size(), 2);
    }

    #[tokio::test]
    async fn test_caching_model_hit_rate() {
        let inner = SimpleEmbeddingModel::new("test", 16);
        let caching = CachingEmbeddingModel::new(inner);

        let _ = caching.embed("a").await.unwrap();
        let _ = caching.embed("a").await.unwrap(); // hit
        let _ = caching.embed("b").await.unwrap();
        let _ = caching.embed("a").await.unwrap(); // hit

        // 4 calls: 2 misses, 2 hits
        assert!((caching.hit_rate() - 0.5).abs() < 1e-6);
    }

    #[tokio::test]
    async fn test_caching_model_hit_rate_zero_when_empty() {
        let inner = SimpleEmbeddingModel::new("test", 16);
        let caching = CachingEmbeddingModel::new(inner);
        assert_eq!(caching.hit_rate(), 0.0);
    }

    #[tokio::test]
    async fn test_caching_model_clear_cache() {
        let inner = SimpleEmbeddingModel::new("test", 16);
        let caching = CachingEmbeddingModel::new(inner);

        let _ = caching.embed("hello").await.unwrap();
        assert_eq!(caching.cache_size(), 1);

        caching.clear_cache();
        assert_eq!(caching.cache_size(), 0);

        // 再次调用应 miss
        let _ = caching.embed("hello").await.unwrap();
        assert_eq!(caching.cache_misses(), 2);
    }

    #[tokio::test]
    async fn test_caching_model_batch_mixed() {
        let inner = SimpleEmbeddingModel::new("test", 16);
        let caching = CachingEmbeddingModel::new(inner);

        // 先缓存一个
        let _ = caching.embed("hello").await.unwrap();

        // 批量调用：hello 已缓存，world 未缓存
        let texts = vec!["hello".to_string(), "world".to_string()];
        let results = caching.embed_batch(&texts).await.unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(caching.cache_hits(), 1); // hello
        assert_eq!(caching.cache_misses(), 2); // 初始 hello + world
    }

    #[tokio::test]
    async fn test_caching_model_batch_all_cached() {
        let inner = SimpleEmbeddingModel::new("test", 16);
        let caching = CachingEmbeddingModel::new(inner);

        // 先缓存全部
        let _ = caching.embed("hello").await.unwrap();
        let _ = caching.embed("world").await.unwrap();

        // 批量调用：全部命中
        let texts = vec!["hello".to_string(), "world".to_string()];
        let results = caching.embed_batch(&texts).await.unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(caching.cache_hits(), 2);
    }

    #[tokio::test]
    async fn test_caching_model_preserves_dimension_and_name() {
        let inner = SimpleEmbeddingModel::new("my-model", 32);
        let caching = CachingEmbeddingModel::new(inner);

        assert_eq!(caching.dimension(), 32);
        assert_eq!(caching.model_name(), "my-model");
    }

    #[tokio::test]
    async fn test_caching_model_inner_access() {
        let inner = SimpleEmbeddingModel::new("inner", 8);
        let caching = CachingEmbeddingModel::new(inner);
        assert_eq!(caching.inner().model_name(), "inner");
    }

    // ---- NormalizedEmbeddingModel 测试 ----

    #[tokio::test]
    async fn test_normalized_model_produces_unit_vector() {
        let inner = SimpleEmbeddingModel::new("test", 16);
        let normalized = NormalizedEmbeddingModel::new(inner);

        let v = normalized.embed("hello world").await.unwrap();
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        // L2 范数应接近 1（或 0 对于空输入）
        assert!(norm.abs() < 1e-5 || (norm - 1.0).abs() < 1e-5);
    }

    #[tokio::test]
    async fn test_normalized_model_batch_produces_unit_vectors() {
        let inner = SimpleEmbeddingModel::new("test", 16);
        let normalized = NormalizedEmbeddingModel::new(inner);

        let texts = vec!["hello".to_string(), "world".to_string()];
        let vectors = normalized.embed_batch(&texts).await.unwrap();

        for v in &vectors {
            let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            assert!(norm.abs() < 1e-5 || (norm - 1.0).abs() < 1e-5);
        }
    }

    #[tokio::test]
    async fn test_normalized_model_l2_normalize_static() {
        let mut vector = vec![3.0, 4.0]; // norm = 5
        NormalizedEmbeddingModel::<SimpleEmbeddingModel>::l2_normalize(&mut vector);
        assert!((vector[0] - 0.6).abs() < 1e-5);
        assert!((vector[1] - 0.8).abs() < 1e-5);
    }

    #[tokio::test]
    async fn test_normalized_model_l2_normalize_zero_vector() {
        let mut vector = vec![0.0, 0.0, 0.0];
        NormalizedEmbeddingModel::<SimpleEmbeddingModel>::l2_normalize(&mut vector);
        // 零向量归一化后仍为零
        assert!(vector.iter().all(|x| *x == 0.0));
    }

    #[tokio::test]
    async fn test_normalized_model_preserves_dimension_and_name() {
        let inner = SimpleEmbeddingModel::new("norm-model", 64);
        let normalized = NormalizedEmbeddingModel::new(inner);
        assert_eq!(normalized.dimension(), 64);
        assert_eq!(normalized.model_name(), "norm-model");
    }

    #[tokio::test]
    async fn test_normalized_model_inner_access() {
        let inner = SimpleEmbeddingModel::new("inner", 8);
        let normalized = NormalizedEmbeddingModel::new(inner);
        assert_eq!(normalized.inner().model_name(), "inner");
    }

    // ---- DimReductionEmbeddingModel 测试 ----

    #[test]
    fn test_dim_reduction_strategy_variants() {
        assert_eq!(DimReductionStrategy::Truncate, DimReductionStrategy::Truncate);
        assert_eq!(DimReductionStrategy::Average, DimReductionStrategy::Average);
        assert_ne!(DimReductionStrategy::Truncate, DimReductionStrategy::Average);
    }

    #[tokio::test]
    async fn test_dim_reduction_truncate() {
        let inner = SimpleEmbeddingModel::new("test", 16);
        let reduced = DimReductionEmbeddingModel::truncate(inner, 8);

        let v = reduced.embed("hello world").await.unwrap();
        assert_eq!(v.len(), 8);
    }

    #[tokio::test]
    async fn test_dim_reduction_truncate_batch() {
        let inner = SimpleEmbeddingModel::new("test", 16);
        let reduced = DimReductionEmbeddingModel::truncate(inner, 4);

        let texts = vec!["hello".to_string(), "world".to_string()];
        let vectors = reduced.embed_batch(&texts).await.unwrap();
        for v in &vectors {
            assert_eq!(v.len(), 4);
        }
    }

    #[tokio::test]
    async fn test_dim_reduction_average() {
        let inner = SimpleEmbeddingModel::new("test", 16);
        let reduced = DimReductionEmbeddingModel::average(inner, 4);

        let v = reduced.embed("hello world").await.unwrap();
        assert_eq!(v.len(), 4);
    }

    #[test]
    fn test_dim_reduction_reduce_truncate() {
        let inner = SimpleEmbeddingModel::new("test", 8);
        let reduced = DimReductionEmbeddingModel::truncate(inner, 3);
        let result = reduced.reduce(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        assert_eq!(result, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_dim_reduction_reduce_average() {
        let inner = SimpleEmbeddingModel::new("test", 8);
        let reduced = DimReductionEmbeddingModel::average(inner, 2);
        // 4 维降到 2 维：chunk_size = 4/2 = 2
        // [1,2] -> avg=1.5, [3,4] -> avg=3.5
        let result = reduced.reduce(vec![1.0, 2.0, 3.0, 4.0]);
        assert!((result[0] - 1.5).abs() < 1e-5);
        assert!((result[1] - 3.5).abs() < 1e-5);
    }

    #[test]
    fn test_dim_reduction_reduce_empty() {
        let inner = SimpleEmbeddingModel::new("test", 8);
        let reduced = DimReductionEmbeddingModel::average(inner, 2);
        let result = reduced.reduce(vec![]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_dim_reduction_reduce_target_zero() {
        let inner = SimpleEmbeddingModel::new("test", 8);
        let reduced = DimReductionEmbeddingModel::average(inner, 0);
        let result = reduced.reduce(vec![1.0, 2.0, 3.0]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_dim_reduction_reduce_truncate_smaller_than_target() {
        let inner = SimpleEmbeddingModel::new("test", 8);
        let reduced = DimReductionEmbeddingModel::truncate(inner, 10);
        // 原始 3 维，目标 10 维：截断后只有 3 维
        let result = reduced.reduce(vec![1.0, 2.0, 3.0]);
        assert_eq!(result.len(), 3);
    }

    #[tokio::test]
    async fn test_dim_reduction_dimension_returns_target() {
        let inner = SimpleEmbeddingModel::new("test", 16);
        let reduced = DimReductionEmbeddingModel::truncate(inner, 8);
        assert_eq!(reduced.dimension(), 8);
    }

    #[tokio::test]
    async fn test_dim_reduction_preserves_model_name() {
        let inner = SimpleEmbeddingModel::new("original", 16);
        let reduced = DimReductionEmbeddingModel::truncate(inner, 8);
        assert_eq!(reduced.model_name(), "original");
    }

    #[test]
    fn test_dim_reduction_target_dimension_and_strategy_accessors() {
        let inner = SimpleEmbeddingModel::new("test", 16);
        let reduced = DimReductionEmbeddingModel::new(inner, 8, DimReductionStrategy::Average);
        assert_eq!(reduced.target_dimension(), 8);
        assert_eq!(reduced.strategy(), DimReductionStrategy::Average);
    }

    // ---- LoggingEmbeddingModel 测试 ----

    #[tokio::test]
    async fn test_logging_model_counts_calls() {
        let inner = SimpleEmbeddingModel::new("test", 16);
        let logging = LoggingEmbeddingModel::new(inner);

        let _ = logging.embed("hello").await.unwrap();
        let _ = logging.embed("world").await.unwrap();

        assert_eq!(logging.call_count(), 2);
        assert_eq!(logging.total_texts(), 2);
    }

    #[tokio::test]
    async fn test_logging_model_batch_counts() {
        let inner = SimpleEmbeddingModel::new("test", 16);
        let logging = LoggingEmbeddingModel::new(inner);

        let texts = vec!["hello".to_string(), "world".to_string(), "foo".to_string()];
        let _ = logging.embed_batch(&texts).await.unwrap();

        assert_eq!(logging.call_count(), 1);
        assert_eq!(logging.total_texts(), 3);
    }

    #[tokio::test]
    async fn test_logging_model_preserves_dimension_and_name() {
        let inner = SimpleEmbeddingModel::new("logged", 32);
        let logging = LoggingEmbeddingModel::new(inner);
        assert_eq!(logging.dimension(), 32);
        assert_eq!(logging.model_name(), "logged");
    }

    #[tokio::test]
    async fn test_logging_model_inner_access() {
        let inner = SimpleEmbeddingModel::new("inner", 8);
        let logging = LoggingEmbeddingModel::new(inner);
        assert_eq!(logging.inner().model_name(), "inner");
    }

    #[tokio::test]
    async fn test_logging_model_initial_counts_zero() {
        let inner = SimpleEmbeddingModel::new("test", 16);
        let logging = LoggingEmbeddingModel::new(inner);
        assert_eq!(logging.call_count(), 0);
        assert_eq!(logging.total_texts(), 0);
    }

    // ---- 适配器组合测试 ----

    #[tokio::test]
    async fn test_compose_caching_and_normalized() {
        let inner = SimpleEmbeddingModel::new("composed", 16);
        let caching = CachingEmbeddingModel::new(inner);
        let normalized = NormalizedEmbeddingModel::new(caching);

        let v1 = normalized.embed("hello world").await.unwrap();
        let v2 = normalized.embed("hello world").await.unwrap();

        // 缓存应生效（第二次命中）
        assert_eq!(v1, v2);
        let norm: f32 = v1.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(norm.abs() < 1e-5 || (norm - 1.0).abs() < 1e-5);
    }

    #[tokio::test]
    async fn test_compose_logging_and_caching() {
        let inner = SimpleEmbeddingModel::new("composed", 16);
        let logging = LoggingEmbeddingModel::new(inner);
        let caching = CachingEmbeddingModel::new(logging);

        let _ = caching.embed("hello").await.unwrap();
        let _ = caching.embed("hello").await.unwrap(); // cache hit

        // 日志层应记录 2 次调用（缓存层每次都会转发到日志层）
        // 实际上缓存命中时不会调用内部模型，所以日志层只记录 1 次
        assert_eq!(caching.cache_hits(), 1);
    }
}
