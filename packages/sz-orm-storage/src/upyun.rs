use crate::error::StorageError;
use crate::storage::Storage;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct UpYunStorage {
    pub bucket: String,
    store: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl UpYunStorage {
    pub fn new(bucket: impl Into<String>) -> Self {
        Self {
            bucket: bucket.into(),
            store: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn url_for(&self, key: &str) -> String {
        format!("upyun://{}/{}", self.bucket, key)
    }
}

#[async_trait]
impl Storage for UpYunStorage {
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
            .ok_or_else(|| StorageError::NotFound(format!("upyun://{}/{}", self.bucket, key)))
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
    async fn test_upyun_put_and_get() {
        let storage = UpYunStorage::new("my-bucket");
        let url = storage
            .put("file.txt", b"upyun-data", "text/plain")
            .await
            .unwrap();
        assert!(url.starts_with("upyun://my-bucket/"));
        assert!(url.contains("file.txt"));

        let data = storage.get("file.txt").await.unwrap();
        assert_eq!(data, b"upyun-data");
    }

    #[tokio::test]
    async fn test_upyun_get_not_found() {
        let storage = UpYunStorage::new("bucket");
        let result = storage.get("missing").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), StorageError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_upyun_delete_and_exists() {
        let storage = UpYunStorage::new("bucket");
        storage.put("key", b"data", "text/plain").await.unwrap();
        assert!(storage.exists("key").await.unwrap());

        storage.delete("key").await.unwrap();
        assert!(!storage.exists("key").await.unwrap());
    }

    #[tokio::test]
    async fn test_upyun_overwrite() {
        let storage = UpYunStorage::new("bucket");
        storage.put("key", b"v1", "text/plain").await.unwrap();
        storage.put("key", b"v2", "text/plain").await.unwrap();
        assert_eq!(storage.get("key").await.unwrap(), b"v2");
    }

    #[tokio::test]
    async fn test_upyun_url_format() {
        let storage = UpYunStorage::new("my-bucket");
        assert_eq!(storage.url_for("file.txt"), "upyun://my-bucket/file.txt");
        assert_eq!(
            storage.url_for("dir/file.txt"),
            "upyun://my-bucket/dir/file.txt"
        );
    }
}
