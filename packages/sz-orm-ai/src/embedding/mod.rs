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
}
