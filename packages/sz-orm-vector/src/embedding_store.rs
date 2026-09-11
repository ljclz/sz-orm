//! Embedding 存储统一入口（v6.8.0 TASK W1-4 + W1-5）
//!
//! 提供 `EmbeddingStore` 统一写入/搜索/读取入口，
//! 内置维度校验 + top_k 校验 + 后端降级处理。

use crate::error::VectorError;
use crate::extensions::DimensionValidator;
use crate::{validate_top_k, PgVectorStore, SearchResult, VectorMetric, VectorRecord};

/// Embedding 存储统一入口
///
/// 包装 `PgVectorStore` 后端，提供维度校验、top_k 校验和降级处理。
pub struct EmbeddingStore {
    backend: Box<dyn PgVectorStore>,
    metric: VectorMetric,
}

impl EmbeddingStore {
    pub fn new(backend: Box<dyn PgVectorStore>, metric: VectorMetric) -> Self {
        Self { backend, metric }
    }

    /// 写入向量记录
    ///
    /// 校验所有记录维度一致后调用后端 insert。
    /// 后端不可达时返回 `BackendUnreachable`，进程不 panic。
    pub async fn write(
        &self,
        collection: &str,
        records: Vec<VectorRecord>,
    ) -> Result<(), VectorError> {
        if records.is_empty() {
            return Ok(());
        }
        let expected_dim = records[0].vector.len();
        DimensionValidator::validate_dimension(expected_dim)?;
        for r in &records {
            DimensionValidator::validate_vector(&r.vector, expected_dim)?;
        }
        self.backend
            .insert(collection, records)
            .await
            .map_err(Self::map_backend_error)
    }

    /// 相似度搜索
    ///
    /// 校验 top_k 和查询向量维度后调用后端 search。
    /// 后端不可达时返回 `BackendUnreachable`，进程不 panic。
    pub async fn search(
        &self,
        collection: &str,
        query: &[f32],
        top_k: usize,
    ) -> Result<Vec<SearchResult>, VectorError> {
        let valid_top_k = validate_top_k(top_k)?;
        DimensionValidator::validate_query(query, query.len())?;
        self.backend
            .search(collection, query, valid_top_k)
            .await
            .map_err(Self::map_backend_error)
    }

    /// 按主键读取单条记录
    ///
    /// 后端不可达时返回 `BackendUnreachable`，进程不 panic。
    pub async fn get(
        &self,
        collection: &str,
        id: &str,
    ) -> Result<Option<VectorRecord>, VectorError> {
        self.backend
            .get(collection, id)
            .await
            .map_err(Self::map_backend_error)
    }

    /// 创建集合
    pub async fn create_collection(&self, name: &str, dimension: usize) -> Result<(), VectorError> {
        DimensionValidator::validate_dimension(dimension)?;
        self.backend
            .create_collection(name, dimension, Some(self.metric))
            .await
            .map_err(Self::map_backend_error)
    }

    /// 当前度量
    pub fn metric(&self) -> &VectorMetric {
        &self.metric
    }

    fn map_backend_error(err: VectorError) -> VectorError {
        match err {
            VectorError::Connection(msg) => VectorError::BackendUnreachable { reason: msg },
            VectorError::Unsupported(ref msg) if msg.contains("connection") => {
                VectorError::BackendUnreachable {
                    reason: msg.clone(),
                }
            }
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::InMemoryVectorStore;

    fn make_store(_dim: usize) -> EmbeddingStore {
        let backend = InMemoryVectorStore::new();
        EmbeddingStore::new(Box::new(backend), VectorMetric::Cosine)
    }

    #[tokio::test]
    async fn write_and_get_roundtrip() {
        let store = make_store(3);
        store.create_collection("test", 3).await.unwrap();
        let record = VectorRecord::new("r1", vec![1.0, 2.0, 3.0]);
        store.write("test", vec![record]).await.unwrap();
        let got = store.get("test", "r1").await.unwrap();
        assert!(got.is_some());
        assert_eq!(got.unwrap().vector, vec![1.0, 2.0, 3.0]);
    }

    #[tokio::test]
    async fn write_dimension_mismatch_detected() {
        let store = make_store(3);
        store.create_collection("test", 3).await.unwrap();
        let records = vec![
            VectorRecord::new("r1", vec![1.0, 2.0, 3.0]),
            VectorRecord::new("r2", vec![1.0, 2.0]),
        ];
        let result = store.write("test", records).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn search_returns_results() {
        let store = make_store(2);
        store.create_collection("test", 2).await.unwrap();
        store
            .write(
                "test",
                vec![
                    VectorRecord::new("a", vec![1.0, 0.0]),
                    VectorRecord::new("b", vec![0.0, 1.0]),
                ],
            )
            .await
            .unwrap();
        let results = store.search("test", &[1.0, 0.0], 2).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn search_top_k_validation() {
        let store = make_store(2);
        store.create_collection("test", 2).await.unwrap();
        let result = store.search("test", &[1.0, 0.0], 0).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn write_empty_records_ok() {
        let store = make_store(2);
        store.create_collection("test", 2).await.unwrap();
        assert!(store.write("test", vec![]).await.is_ok());
    }

    #[tokio::test]
    async fn get_nonexistent_returns_none() {
        let store = make_store(2);
        store.create_collection("test", 2).await.unwrap();
        let result = store.get("test", "nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn metric_accessor() {
        let store = make_store(2);
        assert_eq!(store.metric(), &VectorMetric::Cosine);
    }

    #[tokio::test]
    async fn backend_unreachable_from_stub() {
        use crate::stub::StubVectorStore;
        let store = EmbeddingStore::new(Box::new(StubVectorStore), VectorMetric::Cosine);
        let result = store.search("any", &[1.0, 2.0, 3.0], 5).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn write_768_dim_vector() {
        let store = make_store(768);
        store.create_collection("embeddings", 768).await.unwrap();
        let vec = vec![0.1; 768];
        let record = VectorRecord::new("doc1", vec);
        store.write("embeddings", vec![record]).await.unwrap();
        let got = store.get("embeddings", "doc1").await.unwrap();
        assert!(got.is_some());
        assert_eq!(got.unwrap().vector.len(), 768);
    }

    #[tokio::test]
    async fn create_collection_invalid_dimension() {
        let store = make_store(0);
        let result = store.create_collection("bad", 0).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn search_with_different_metrics() {
        let euclidean_store = {
            let backend = InMemoryVectorStore::new();
            EmbeddingStore::new(Box::new(backend), VectorMetric::Euclidean)
        };
        euclidean_store.create_collection("test", 2).await.unwrap();
        euclidean_store
            .write("test", vec![VectorRecord::new("a", vec![1.0, 0.0])])
            .await
            .unwrap();
        let results = euclidean_store
            .search("test", &[1.0, 0.0], 1)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
    }
}
