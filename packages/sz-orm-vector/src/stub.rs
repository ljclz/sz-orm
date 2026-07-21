//! Stub 实现：所有方法返回 Unsupported 错误
//!
//! 适用于：
//! - 调试场景：验证调用流程
//! - 不连接数据库的代码审查

use crate::error::VectorError;
use crate::PgVectorStore;
use crate::{SearchResult, VectorMetric, VectorRecord};
use async_trait::async_trait;

/// Stub Vector Store 实现
///
/// 所有方法均返回 `Unsupported` 错误，仅在未启用 `real-pg` feature 时
/// 提供一个可编译的占位实现。
pub struct StubVectorStore;

impl StubVectorStore {
    pub fn new() -> Self {
        Self
    }
}

impl Default for StubVectorStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PgVectorStore for StubVectorStore {
    async fn create_collection(
        &self,
        _name: &str,
        _dimension: usize,
        _metric: Option<VectorMetric>,
    ) -> Result<(), VectorError> {
        Err(VectorError::Unsupported(
            "StubVectorStore does not support create_collection".to_string(),
        ))
    }

    async fn delete_collection(&self, _name: &str) -> Result<(), VectorError> {
        Err(VectorError::Unsupported(
            "StubVectorStore does not support delete_collection".to_string(),
        ))
    }

    async fn insert(
        &self,
        _collection: &str,
        _records: Vec<VectorRecord>,
    ) -> Result<(), VectorError> {
        Err(VectorError::Unsupported(
            "StubVectorStore does not support insert".to_string(),
        ))
    }

    async fn search(
        &self,
        _collection: &str,
        _query: &[f32],
        _top_k: usize,
    ) -> Result<Vec<SearchResult>, VectorError> {
        Err(VectorError::Unsupported(
            "StubVectorStore does not support search".to_string(),
        ))
    }

    async fn get(&self, _collection: &str, _id: &str) -> Result<Option<VectorRecord>, VectorError> {
        Err(VectorError::Unsupported(
            "StubVectorStore does not support get".to_string(),
        ))
    }

    async fn delete(&self, _collection: &str, _ids: Vec<String>) -> Result<u64, VectorError> {
        Err(VectorError::Unsupported(
            "StubVectorStore does not support delete".to_string(),
        ))
    }

    async fn count(&self, _collection: &str) -> Result<usize, VectorError> {
        Err(VectorError::Unsupported(
            "StubVectorStore does not support count".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_stub_create_collection_unsupported() {
        let stub = StubVectorStore::new();
        let result = stub.create_collection("docs", 3, None).await;
        assert!(matches!(result, Err(VectorError::Unsupported(_))));
    }

    #[tokio::test]
    async fn test_stub_insert_unsupported() {
        let stub = StubVectorStore::new();
        let rec = VectorRecord::new("r1", vec![1.0, 0.0, 0.0]);
        let result = stub.insert("docs", vec![rec]).await;
        assert!(matches!(result, Err(VectorError::Unsupported(_))));
    }

    #[tokio::test]
    async fn test_stub_search_unsupported() {
        let stub = StubVectorStore::new();
        let result = stub.search("docs", &[1.0, 0.0, 0.0], 5).await;
        assert!(matches!(result, Err(VectorError::Unsupported(_))));
    }

    #[tokio::test]
    async fn test_stub_get_unsupported() {
        let stub = StubVectorStore::new();
        let result = stub.get("docs", "r1").await;
        assert!(matches!(result, Err(VectorError::Unsupported(_))));
    }

    #[tokio::test]
    async fn test_stub_delete_unsupported() {
        let stub = StubVectorStore::new();
        let result = stub.delete("docs", vec!["r1".to_string()]).await;
        assert!(matches!(result, Err(VectorError::Unsupported(_))));
    }

    #[tokio::test]
    async fn test_stub_count_unsupported() {
        let stub = StubVectorStore::new();
        let result = stub.count("docs").await;
        assert!(matches!(result, Err(VectorError::Unsupported(_))));
    }

    #[tokio::test]
    async fn test_stub_delete_collection_unsupported() {
        let stub = StubVectorStore::new();
        let result = stub.delete_collection("docs").await;
        assert!(matches!(result, Err(VectorError::Unsupported(_))));
    }

    #[tokio::test]
    async fn test_stub_default() {
        let stub = StubVectorStore;
        let result = stub.count("any").await;
        assert!(matches!(result, Err(VectorError::Unsupported(_))));
    }
}
