//! 内存实现：纯 Rust 向量计算（不连接数据库）
//!
//! 适用于：
//! - 单元测试
//! - 不需要真实 pgvector 的场景（如原型开发）
//! - 性能基准（无 I/O 开销）

use crate::error::VectorError;
use crate::PgVectorStore;
use crate::{SearchResult, VectorMetric, VectorRecord};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::RwLock;

/// 内存 Vector Store 实现
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
                let dist: f32 = a
                    .iter()
                    .zip(b.iter())
                    .map(|(x, y)| (x - y) * (x - y))
                    .sum::<f32>()
                    .sqrt();
                // 将距离转为 [0, 1] 相似度
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

/// 余弦相似度计算
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
impl PgVectorStore for InMemoryVectorStore {
    async fn create_collection(
        &self,
        name: &str,
        dimension: usize,
        metric: Option<VectorMetric>,
    ) -> Result<(), VectorError> {
        let mut collections = self
            .collections
            .write()
            .map_err(|e| VectorError::Query(format!("lock error: {}", e)))?;
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

    async fn delete_collection(&self, name: &str) -> Result<(), VectorError> {
        let mut collections = self
            .collections
            .write()
            .map_err(|e| VectorError::Query(format!("lock error: {}", e)))?;
        collections.remove(name);
        Ok(())
    }

    async fn insert(
        &self,
        collection: &str,
        records: Vec<VectorRecord>,
    ) -> Result<(), VectorError> {
        let mut collections = self
            .collections
            .write()
            .map_err(|e| VectorError::Query(format!("lock error: {}", e)))?;
        let state = collections
            .get_mut(collection)
            .ok_or_else(|| VectorError::CollectionNotFound(collection.to_string()))?;

        for record in records {
            if record.vector.len() != state.dimension {
                return Err(VectorError::DimensionMismatch {
                    expected: state.dimension,
                    actual: record.vector.len(),
                });
            }
            // Upsert
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
    ) -> Result<Vec<SearchResult>, VectorError> {
        // M-16 修复：校验 top_k 范围
        let top_k = crate::validate_top_k(top_k)?;

        let collections = self
            .collections
            .read()
            .map_err(|e| VectorError::Query(format!("lock error: {}", e)))?;
        let state = collections
            .get(collection)
            .ok_or_else(|| VectorError::CollectionNotFound(collection.to_string()))?;

        let mut scored: Vec<(usize, f32)> = state
            .records
            .iter()
            .enumerate()
            .map(|(i, r)| (i, Self::metric_value(state.metric, query, &r.vector)))
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let k = top_k.min(scored.len());
        let mut results = Vec::with_capacity(k);
        for (idx, score) in scored.into_iter().take(k) {
            let record = &state.records[idx];
            let mut result = SearchResult::new(record.id.clone(), score, record.vector.clone());
            if let Some(ref text) = record.text {
                result = result.with_text(text.clone());
            }
            results.push(result);
        }
        Ok(results)
    }

    async fn get(&self, collection: &str, id: &str) -> Result<Option<VectorRecord>, VectorError> {
        let collections = self
            .collections
            .read()
            .map_err(|e| VectorError::Query(format!("lock error: {}", e)))?;
        let state = collections
            .get(collection)
            .ok_or_else(|| VectorError::CollectionNotFound(collection.to_string()))?;
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

    async fn delete(&self, collection: &str, ids: Vec<String>) -> Result<u64, VectorError> {
        let mut collections = self
            .collections
            .write()
            .map_err(|e| VectorError::Query(format!("lock error: {}", e)))?;
        let state = collections
            .get_mut(collection)
            .ok_or_else(|| VectorError::CollectionNotFound(collection.to_string()))?;
        let before = state.records.len();
        state.records.retain(|r| !ids.contains(&r.id));
        let removed = (before - state.records.len()) as u64;
        Ok(removed)
    }

    async fn count(&self, collection: &str) -> Result<usize, VectorError> {
        let collections = self
            .collections
            .read()
            .map_err(|e| VectorError::Query(format!("lock error: {}", e)))?;
        Ok(collections
            .get(collection)
            .map(|s| s.records.len())
            .unwrap_or(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VectorMetric;

    #[tokio::test]
    async fn test_create_and_delete_collection() {
        let store = InMemoryVectorStore::new();
        store.create_collection("docs", 4, None).await.unwrap();
        assert_eq!(store.count("docs").await.unwrap(), 0);

        store.delete_collection("docs").await.unwrap();
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
        assert!(matches!(err, Err(VectorError::DimensionMismatch { .. })));
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
        // Upsert should keep count at 1
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

        let results = store.search("docs", &[1.0, 0.0, 0.0], 2).await.unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, "a");
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
        let results = store.search("docs", &[0.0, 1.0], 3).await.unwrap();
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
    async fn test_search_euclidean() {
        let store = InMemoryVectorStore::new();
        store
            .create_collection("docs", 2, Some(VectorMetric::Euclidean))
            .await
            .unwrap();
        let records = vec![
            VectorRecord::new("near", vec![0.0, 0.0]),
            VectorRecord::new("far", vec![10.0, 10.0]),
        ];
        store.insert("docs", records).await.unwrap();

        let results = store.search("docs", &[0.0, 0.0], 2).await.unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, "near");
        assert!(results[0].score > results[1].score);
    }

    #[tokio::test]
    async fn test_search_dot_product() {
        let store = InMemoryVectorStore::new();
        store
            .create_collection("docs", 2, Some(VectorMetric::DotProduct))
            .await
            .unwrap();
        let records = vec![
            VectorRecord::new("high", vec![2.0, 3.0]),
            VectorRecord::new("low", vec![0.0, 0.0]),
        ];
        store.insert("docs", records).await.unwrap();

        let results = store.search("docs", &[1.0, 1.0], 2).await.unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, "high");
    }

    #[tokio::test]
    async fn test_collection_not_found() {
        let store = InMemoryVectorStore::new();
        let result = store.count("nonexistent").await;
        assert_eq!(result.unwrap(), 0);

        let err = store.search("nonexistent", &[1.0, 0.0], 5).await;
        assert!(matches!(err, Err(VectorError::CollectionNotFound(_))));
    }

    #[tokio::test]
    async fn test_get_nonexistent_record() {
        let store = InMemoryVectorStore::new();
        store.create_collection("docs", 2, None).await.unwrap();
        let result = store.get("docs", "nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_helpers_compile() {
        let _ = InMemoryVectorStore::new();
        assert_eq!(cosine_similarity(&[1.0, 0.0], &[1.0, 0.0]), 1.0);
        assert!((cosine_similarity(&[1.0, 0.0], &[0.0, 1.0])).abs() < 1e-6);
        assert_eq!(cosine_similarity(&[], &[]), 0.0);
    }

    /// M-16 测试：top_k = 0 应被拒绝
    #[tokio::test]
    async fn test_m16_top_k_zero_rejected() {
        let store = InMemoryVectorStore::new();
        store.create_collection("docs", 2, None).await.unwrap();
        store
            .insert("docs", vec![VectorRecord::new("a", vec![1.0, 0.0])])
            .await
            .unwrap();
        let err = store.search("docs", &[1.0, 0.0], 0).await;
        assert!(matches!(
            err,
            Err(VectorError::TopKExceeded {
                requested: 0,
                max: crate::MAX_TOP_K
            })
        ));
    }

    /// M-16 测试：top_k 超过 MAX_TOP_K 应被拒绝
    #[tokio::test]
    async fn test_m16_top_k_exceeded_rejected() {
        let store = InMemoryVectorStore::new();
        store.create_collection("docs", 2, None).await.unwrap();
        store
            .insert("docs", vec![VectorRecord::new("a", vec![1.0, 0.0])])
            .await
            .unwrap();
        let err = store
            .search("docs", &[1.0, 0.0], crate::MAX_TOP_K + 1)
            .await;
        assert!(matches!(
            err,
            Err(VectorError::TopKExceeded { requested, max }) if requested == crate::MAX_TOP_K + 1 && max == crate::MAX_TOP_K
        ));
    }

    /// M-16 测试：top_k = MAX_TOP_K 应允许
    #[tokio::test]
    async fn test_m16_top_k_max_allowed() {
        let store = InMemoryVectorStore::new();
        store.create_collection("docs", 2, None).await.unwrap();
        store
            .insert("docs", vec![VectorRecord::new("a", vec![1.0, 0.0])])
            .await
            .unwrap();
        // top_k = MAX_TOP_K 不应触发错误（即使记录数远少于 MAX_TOP_K）
        let results = store
            .search("docs", &[1.0, 0.0], crate::MAX_TOP_K)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
    }

    /// M-16 测试：validate_top_k 函数单元测试
    #[test]
    fn test_m16_validate_top_k_function() {
        use crate::validate_top_k;
        // 有效值
        assert_eq!(validate_top_k(1).unwrap(), 1);
        assert_eq!(validate_top_k(100).unwrap(), 100);
        assert_eq!(validate_top_k(crate::MAX_TOP_K).unwrap(), crate::MAX_TOP_K);
        // 无效值
        assert!(validate_top_k(0).is_err());
        assert!(validate_top_k(crate::MAX_TOP_K + 1).is_err());
        assert!(validate_top_k(usize::MAX).is_err());
    }
}
