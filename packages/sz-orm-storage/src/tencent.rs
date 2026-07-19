use crate::error::StorageError;
use crate::storage::Storage;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct TencentCosStorage {
    pub bucket: String,
    pub region: String,
    store: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl TencentCosStorage {
    pub fn new(bucket: impl Into<String>, region: impl Into<String>) -> Self {
        Self {
            bucket: bucket.into(),
            region: region.into(),
            store: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn url_for(&self, key: &str) -> String {
        format!("cos://{}.{}/{}", self.bucket, self.region, key)
    }
}

#[async_trait]
impl Storage for TencentCosStorage {
    async fn put(
        &self,
        key: &str,
        data: &[u8],
        _content_type: &str,
    ) -> Result<String, StorageError> {
        let mut store = self.store.write().await;
        store.insert(key.to_string(), data.to_vec());
        Ok(self.url_for(key))
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>, StorageError> {
        let store = self.store.read().await;
        store
            .get(key)
            .cloned()
            .ok_or_else(|| StorageError::NotFound(format!("cos://{}/{}", self.bucket, key)))
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let mut store = self.store.write().await;
        store.remove(key);
        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        let store = self.store.read().await;
        Ok(store.contains_key(key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tencent_put_and_get() {
        let storage = TencentCosStorage::new("bucket-1250000000", "ap-guangzhou");
        let url = storage
            .put("a.txt", b"tencent-data", "text/plain")
            .await
            .unwrap();
        assert!(url.starts_with("cos://bucket-1250000000.ap-guangzhou/"));
        assert!(url.contains("a.txt"));

        let data = storage.get("a.txt").await.unwrap();
        assert_eq!(data, b"tencent-data");
    }

    #[tokio::test]
    async fn test_tencent_get_not_found() {
        let storage = TencentCosStorage::new("bucket", "ap-guangzhou");
        let result = storage.get("missing").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), StorageError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_tencent_delete_and_exists() {
        let storage = TencentCosStorage::new("bucket", "ap-guangzhou");
        storage.put("key", b"data", "text/plain").await.unwrap();
        assert!(storage.exists("key").await.unwrap());

        storage.delete("key").await.unwrap();
        assert!(!storage.exists("key").await.unwrap());
    }

    #[tokio::test]
    async fn test_tencent_overwrite() {
        let storage = TencentCosStorage::new("bucket", "ap-guangzhou");
        storage.put("key", b"v1", "text/plain").await.unwrap();
        storage.put("key", b"v2", "text/plain").await.unwrap();
        assert_eq!(storage.get("key").await.unwrap(), b"v2");
    }

    #[tokio::test]
    async fn test_tencent_multiple_keys() {
        let storage = TencentCosStorage::new("bucket", "ap-guangzhou");
        storage.put("a", b"a-data", "text/plain").await.unwrap();
        storage.put("b", b"b-data", "text/plain").await.unwrap();

        assert_eq!(storage.get("a").await.unwrap(), b"a-data");
        assert_eq!(storage.get("b").await.unwrap(), b"b-data");

        storage.delete("a").await.unwrap();
        assert!(!storage.exists("a").await.unwrap());
        assert!(storage.exists("b").await.unwrap());
    }

    #[tokio::test]
    async fn test_tencent_url_format() {
        let storage = TencentCosStorage::new("my-bucket", "ap-beijing");
        let url = storage.url_for("path/to/file.txt");
        assert_eq!(url, "cos://my-bucket.ap-beijing/path/to/file.txt");
    }
}
