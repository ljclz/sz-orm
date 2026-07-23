use crate::embedding::EmbeddingRecord;
use crate::error::AiError;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::RwLock;

// HNSW 向量索引子模块（近似最近邻搜索）
pub mod hnsw;

#[derive(Debug, Clone)]
pub struct VectorError {
    pub message: String,
    pub collection: Option<String>,
}

impl VectorError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            collection: None,
        }
    }

    pub fn with_collection(message: impl Into<String>, collection: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            collection: Some(collection.into()),
        }
    }
}

impl std::fmt::Display for VectorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "VectorError: {}", self.message)?;
        if let Some(ref coll) = self.collection {
            write!(f, " (collection: {})", coll)?;
        }
        Ok(())
    }
}

impl std::error::Error for VectorError {}

#[derive(Debug, Clone)]
pub struct VectorRecord {
    pub id: String,
    pub vector: Vec<f32>,
    pub score: Option<f32>,
    pub metadata: Option<std::collections::HashMap<String, serde_json::Value>>,
}

impl VectorRecord {
    pub fn new(id: impl Into<String>, vector: Vec<f32>) -> Self {
        Self {
            id: id.into(),
            vector,
            score: None,
            metadata: None,
        }
    }

    pub fn with_score(mut self, score: f32) -> Self {
        self.score = Some(score);
        self
    }

    pub fn with_metadata(
        mut self,
        metadata: std::collections::HashMap<String, serde_json::Value>,
    ) -> Self {
        self.metadata = Some(metadata);
        self
    }

    pub fn from_embedding(record: &EmbeddingRecord) -> Self {
        Self {
            id: record.id.clone(),
            vector: record.vector.clone(),
            score: None,
            metadata: record.metadata.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub id: String,
    pub score: f32,
    pub vector: Vec<f32>,
    pub text: Option<String>,
    pub metadata: Option<std::collections::HashMap<String, serde_json::Value>>,
}

impl SearchResult {
    pub fn new(id: impl Into<String>, score: f32, vector: Vec<f32>) -> Self {
        Self {
            id: id.into(),
            score,
            vector,
            text: None,
            metadata: None,
        }
    }

    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }
}

#[derive(Debug, Clone, Default)]
pub struct VectorFilter {
    pub field: Option<String>,
    pub operator: Option<String>,
    pub value: Option<serde_json::Value>,
}

impl VectorFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn field(mut self, field: impl Into<String>) -> Self {
        self.field = Some(field.into());
        self
    }

    pub fn eq(mut self, value: impl Into<serde_json::Value>) -> Self {
        self.operator = Some("eq".to_string());
        self.value = Some(value.into());
        self
    }

    pub fn gt(mut self, value: impl Into<serde_json::Value>) -> Self {
        self.operator = Some("gt".to_string());
        self.value = Some(value.into());
        self
    }

    pub fn lt(mut self, value: impl Into<serde_json::Value>) -> Self {
        self.operator = Some("lt".to_string());
        self.value = Some(value.into());
        self
    }

    pub fn build(&self) -> Option<String> {
        match (&self.field, &self.operator, &self.value) {
            (Some(field), Some(op), Some(value)) => {
                Some(format!(r#"{{"{}": {{"{}": {}}}}}"#, field, op, value))
            }
            _ => None,
        }
    }
}

#[async_trait]
pub trait VectorStore: Send + Sync {
    async fn create_collection(
        &self,
        name: &str,
        dimension: usize,
        metric: Option<VectorMetric>,
    ) -> Result<(), AiError>;

    async fn delete_collection(&self, name: &str) -> Result<(), AiError>;

    async fn insert(&self, collection: &str, records: Vec<VectorRecord>) -> Result<(), AiError>;

    async fn search(
        &self,
        collection: &str,
        query: &[f32],
        top_k: usize,
        filter: Option<&str>,
    ) -> Result<Vec<SearchResult>, AiError>;

    async fn get(&self, collection: &str, id: &str) -> Result<Option<VectorRecord>, AiError>;

    async fn delete(&self, collection: &str, ids: Vec<String>) -> Result<u64, AiError>;

    async fn count(&self, collection: &str) -> Result<usize, AiError>;
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum VectorMetric {
    #[default]
    Cosine,
    Euclidean,
    DotProduct,
}

impl VectorMetric {
    pub fn as_str(&self) -> &str {
        match self {
            VectorMetric::Cosine => "cosine",
            VectorMetric::Euclidean => "euclidean",
            VectorMetric::DotProduct => "dotproduct",
        }
    }
}

pub struct CollectionMeta {
    pub name: String,
    pub dimension: usize,
    pub metric: VectorMetric,
    pub count: usize,
}

impl CollectionMeta {
    pub fn new(name: impl Into<String>, dimension: usize) -> Self {
        Self {
            name: name.into(),
            dimension,
            metric: VectorMetric::default(),
            count: 0,
        }
    }

    pub fn with_metric(mut self, metric: VectorMetric) -> Self {
        self.metric = metric;
        self
    }
}

/// In-memory VectorStore backed by `HashMap` + `Vec`.
///
/// Stores records per collection and supports cosine/euclidean/dot-product
/// similarity search. Suitable for unit tests and small in-process workloads.
pub struct InMemoryVectorStore {
    collections: RwLock<HashMap<String, CollectionState>>,
}

#[derive(Debug, Clone)]
struct CollectionState {
    dimension: usize,
    metric: VectorMetric,
    records: Vec<StoredRecord>,
}

#[derive(Debug, Clone)]
struct StoredRecord {
    id: String,
    vector: Vec<f32>,
    metadata: Option<HashMap<String, serde_json::Value>>,
    text: Option<String>,
}

impl InMemoryVectorStore {
    pub fn new() -> Self {
        Self {
            collections: RwLock::new(HashMap::new()),
        }
    }

    fn metric_value(metric: VectorMetric, a: &[f32], b: &[f32]) -> f32 {
        match metric {
            VectorMetric::Cosine => cosine_similarity(a, b),
            VectorMetric::Euclidean => {
                // Convert distance to similarity score in [0, 1].
                let dist: f32 = a
                    .iter()
                    .zip(b.iter())
                    .map(|(x, y)| (x - y) * (x - y))
                    .sum::<f32>()
                    .sqrt();
                1.0 / (1.0 + dist)
            }
            VectorMetric::DotProduct => a.iter().zip(b.iter()).map(|(x, y)| x * y).sum(),
        }
    }
}

impl Default for InMemoryVectorStore {
    fn default() -> Self {
        Self::new()
    }
}

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

#[async_trait]
impl VectorStore for InMemoryVectorStore {
    async fn create_collection(
        &self,
        name: &str,
        dimension: usize,
        metric: Option<VectorMetric>,
    ) -> Result<(), AiError> {
        let mut collections = self
            .collections
            .write()
            .map_err(|e| AiError::Vector(format!("lock error: {}", e)))?;
        collections.insert(
            name.to_string(),
            CollectionState {
                dimension,
                metric: metric.unwrap_or_default(),
                records: Vec::new(),
            },
        );
        Ok(())
    }

    async fn delete_collection(&self, name: &str) -> Result<(), AiError> {
        let mut collections = self
            .collections
            .write()
            .map_err(|e| AiError::Vector(format!("lock error: {}", e)))?;
        collections.remove(name);
        Ok(())
    }

    async fn insert(&self, collection: &str, records: Vec<VectorRecord>) -> Result<(), AiError> {
        let mut collections = self
            .collections
            .write()
            .map_err(|e| AiError::Vector(format!("lock error: {}", e)))?;
        let state = collections
            .get_mut(collection)
            .ok_or_else(|| AiError::Vector(format!("collection not found: {}", collection)))?;

        for record in records {
            if record.vector.len() != state.dimension {
                return Err(AiError::Vector(format!(
                    "dimension mismatch: expected {}, got {}",
                    state.dimension,
                    record.vector.len()
                )));
            }
            // Replace existing record if id is the same (upsert semantics).
            if let Some(existing) = state.records.iter_mut().find(|r| r.id == record.id) {
                existing.vector = record.vector;
                existing.metadata = record.metadata;
                continue;
            }
            state.records.push(StoredRecord {
                id: record.id,
                vector: record.vector,
                metadata: record.metadata,
                text: None,
            });
        }
        Ok(())
    }

    async fn search(
        &self,
        collection: &str,
        query: &[f32],
        top_k: usize,
        filter: Option<&str>,
    ) -> Result<Vec<SearchResult>, AiError> {
        let collections = self
            .collections
            .read()
            .map_err(|e| AiError::Vector(format!("lock error: {}", e)))?;
        let state = collections
            .get(collection)
            .ok_or_else(|| AiError::Vector(format!("collection not found: {}", collection)))?;

        let mut scored: Vec<(usize, f32)> = state
            .records
            .iter()
            .enumerate()
            .filter(|(_, r)| match_filter(r.metadata.as_ref(), filter))
            .map(|(i, r)| (i, Self::metric_value(state.metric, query, &r.vector)))
            .collect();

        // Sort by score descending (stable for ties).
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let k = top_k.min(scored.len());
        let mut results = Vec::with_capacity(k);
        for (idx, score) in scored.into_iter().take(k) {
            let record = &state.records[idx];
            let mut search_result =
                SearchResult::new(record.id.clone(), score, record.vector.clone());
            if let Some(ref text) = record.text {
                search_result = search_result.with_text(text.clone());
            }
            results.push(search_result);
        }
        Ok(results)
    }

    async fn get(&self, collection: &str, id: &str) -> Result<Option<VectorRecord>, AiError> {
        let collections = self
            .collections
            .read()
            .map_err(|e| AiError::Vector(format!("lock error: {}", e)))?;
        let state = collections
            .get(collection)
            .ok_or_else(|| AiError::Vector(format!("collection not found: {}", collection)))?;
        Ok(state
            .records
            .iter()
            .find(|r| r.id == id)
            .map(|r| VectorRecord {
                id: r.id.clone(),
                vector: r.vector.clone(),
                score: None,
                metadata: r.metadata.clone(),
            }))
    }

    async fn delete(&self, collection: &str, ids: Vec<String>) -> Result<u64, AiError> {
        let mut collections = self
            .collections
            .write()
            .map_err(|e| AiError::Vector(format!("lock error: {}", e)))?;
        let state = collections
            .get_mut(collection)
            .ok_or_else(|| AiError::Vector(format!("collection not found: {}", collection)))?;
        let before = state.records.len();
        state.records.retain(|r| !ids.contains(&r.id));
        let removed = (before - state.records.len()) as u64;
        Ok(removed)
    }

    async fn count(&self, collection: &str) -> Result<usize, AiError> {
        let collections = self
            .collections
            .read()
            .map_err(|e| AiError::Vector(format!("lock error: {}", e)))?;
        Ok(collections
            .get(collection)
            .map(|s| s.records.len())
            .unwrap_or(0))
    }
}

/// Very small filter expression parser: `{"field": {"eq": value}}`.
/// Returns true if metadata matches; false (or true if no filter) otherwise.
fn match_filter(
    metadata: Option<&HashMap<String, serde_json::Value>>,
    filter: Option<&str>,
) -> bool {
    let Some(expr) = filter else { return true };
    let Some(metadata) = metadata else {
        return false;
    };
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(expr) else {
        return false;
    };
    let Some(obj) = parsed.as_object() else {
        return false;
    };
    for (field, cond) in obj {
        let Some(actual) = metadata.get(field) else {
            return false;
        };
        let Some(cond_obj) = cond.as_object() else {
            return false;
        };
        for (op, val) in cond_obj {
            match op.as_str() {
                "eq" if actual == val => continue,
                "gt" => {
                    let greater = match (actual.as_f64(), val.as_f64()) {
                        (Some(a), Some(b)) => a > b,
                        _ => false,
                    };
                    if !greater {
                        return false;
                    }
                }
                "lt" => {
                    let less = match (actual.as_f64(), val.as_f64()) {
                        (Some(a), Some(b)) => a < b,
                        _ => false,
                    };
                    if !less {
                        return false;
                    }
                }
                _ => return false,
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_and_delete_collection() {
        let store = InMemoryVectorStore::new();
        store.create_collection("docs", 4, None).await.unwrap();
        assert_eq!(store.count("docs").await.unwrap(), 0);

        store.delete_collection("docs").await.unwrap();
        // After deletion, count is 0 (collection does not exist).
        assert_eq!(store.count("docs").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_insert_and_get() {
        let store = InMemoryVectorStore::new();
        store.create_collection("docs", 3, None).await.unwrap();
        let rec = VectorRecord::new("r1", vec![1.0, 0.0, 0.0]);
        store.insert("docs", vec![rec]).await.unwrap();
        assert_eq!(store.count("docs").await.unwrap(), 1);

        let fetched = store.get("docs", "r1").await.unwrap().unwrap();
        assert_eq!(fetched.id, "r1");
        assert_eq!(fetched.vector, vec![1.0, 0.0, 0.0]);

        assert!(store.get("docs", "missing").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_insert_dimension_mismatch() {
        let store = InMemoryVectorStore::new();
        store.create_collection("docs", 3, None).await.unwrap();
        let rec = VectorRecord::new("r1", vec![1.0, 0.0]); // dim=2
        let err = store.insert("docs", vec![rec]).await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn test_insert_upsert() {
        let store = InMemoryVectorStore::new();
        store.create_collection("docs", 2, None).await.unwrap();
        store
            .insert("docs", vec![VectorRecord::new("r1", vec![1.0, 0.0])])
            .await
            .unwrap();
        store
            .insert("docs", vec![VectorRecord::new("r1", vec![0.0, 1.0])])
            .await
            .unwrap();
        // Upsert should keep count at 1.
        assert_eq!(store.count("docs").await.unwrap(), 1);
        let fetched = store.get("docs", "r1").await.unwrap().unwrap();
        assert_eq!(fetched.vector, vec![0.0, 1.0]);
    }

    #[tokio::test]
    async fn test_search_cosine_returns_closest_first() {
        let store = InMemoryVectorStore::new();
        store
            .create_collection("docs", 3, Some(VectorMetric::Cosine))
            .await
            .unwrap();
        let records = vec![
            VectorRecord::new("a", vec![1.0, 0.0, 0.0]),
            VectorRecord::new("b", vec![0.0, 1.0, 0.0]),
            VectorRecord::new("c", vec![1.0, 1.0, 0.0]),
        ];
        store.insert("docs", records).await.unwrap();

        let results = store
            .search("docs", &[1.0, 0.0, 0.0], 2, None)
            .await
            .unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, "a");
        // Cosine similarity of [1,0,0] and [1,1,0] is 1/sqrt(2) ~= 0.707
        assert!(results[0].score > results[1].score);
    }

    #[tokio::test]
    async fn test_search_top_k_limit() {
        let store = InMemoryVectorStore::new();
        store.create_collection("docs", 2, None).await.unwrap();
        for i in 0..5 {
            store
                .insert(
                    "docs",
                    vec![VectorRecord::new(format!("r{}", i), vec![i as f32, 1.0])],
                )
                .await
                .unwrap();
        }
        let results = store.search("docs", &[0.0, 1.0], 3, None).await.unwrap();
        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn test_delete_records() {
        let store = InMemoryVectorStore::new();
        store.create_collection("docs", 2, None).await.unwrap();
        store
            .insert(
                "docs",
                vec![
                    VectorRecord::new("a", vec![1.0, 0.0]),
                    VectorRecord::new("b", vec![0.0, 1.0]),
                    VectorRecord::new("c", vec![1.0, 1.0]),
                ],
            )
            .await
            .unwrap();
        let removed = store
            .delete("docs", vec!["a".to_string(), "c".to_string()])
            .await
            .unwrap();
        assert_eq!(removed, 2);
        assert_eq!(store.count("docs").await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_search_with_filter() {
        let store = InMemoryVectorStore::new();
        store.create_collection("docs", 2, None).await.unwrap();
        let mut md = HashMap::new();
        md.insert("kind".to_string(), serde_json::json!("alpha"));
        let r1 = VectorRecord::new("a", vec![1.0, 0.0]).with_metadata(md);
        let mut md2 = HashMap::new();
        md2.insert("kind".to_string(), serde_json::json!("beta"));
        let r2 = VectorRecord::new("b", vec![1.0, 0.0]).with_metadata(md2);
        store.insert("docs", vec![r1, r2]).await.unwrap();

        let results = store
            .search(
                "docs",
                &[1.0, 0.0],
                10,
                Some(r#"{"kind": {"eq": "alpha"}}"#),
            )
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "a");
    }

    #[test]
    fn test_helpers_compile() {
        // Smoke-test the helper so the binary still has coverage without async runtime.
        let _ = InMemoryVectorStore::new();
        assert_eq!(cosine_similarity(&[1.0, 0.0], &[1.0, 0.0]), 1.0);
        assert!((cosine_similarity(&[1.0, 0.0], &[0.0, 1.0])).abs() < 1e-6);
    }
}
