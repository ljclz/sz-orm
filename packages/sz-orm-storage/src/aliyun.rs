use crate::error::StorageError;
use crate::storage::Storage;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct AliyunOssStorage {
    pub bucket: String,
    pub endpoint: String,
    store: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl AliyunOssStorage {
    pub fn new(bucket: impl Into<String>, endpoint: impl Into<String>) -> Self {
        Self {
            bucket: bucket.into(),
            endpoint: endpoint.into(),
            store: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn url_for(&self, key: &str) -> String {
        format!("oss://{}.{}/{}", self.bucket, self.endpoint, key)
    }
}

#[async_trait]
impl Storage for AliyunOssStorage {
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
            .ok_or_else(|| StorageError::NotFound(format!("oss://{}/{}", self.bucket, key)))
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
    async fn test_aliyun_put_and_get() {
        let storage = AliyunOssStorage::new("bucket", "oss-cn-hangzhou.aliyuncs.com");
        let url = storage
            .put("a.txt", b"aliyun-data", "text/plain")
            .await
            .unwrap();
        assert!(url.starts_with("oss://bucket.oss-cn-hangzhou.aliyuncs.com/"));

        let data = storage.get("a.txt").await.unwrap();
        assert_eq!(data, b"aliyun-data");
    }

    #[tokio::test]
    async fn test_aliyun_get_not_found() {
        let storage = AliyunOssStorage::new("bucket", "oss-cn-hangzhou.aliyuncs.com");
        let result = storage.get("missing").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), StorageError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_aliyun_delete_and_exists() {
        let storage = AliyunOssStorage::new("bucket", "oss-cn-hangzhou.aliyuncs.com");
        storage.put("key", b"data", "text/plain").await.unwrap();
        assert!(storage.exists("key").await.unwrap());

        storage.delete("key").await.unwrap();
        assert!(!storage.exists("key").await.unwrap());
    }

    #[tokio::test]
    async fn test_aliyun_overwrite() {
        let storage = AliyunOssStorage::new("bucket", "oss-cn-hangzhou.aliyuncs.com");
        storage.put("key", b"v1", "text/plain").await.unwrap();
        storage.put("key", b"v2", "text/plain").await.unwrap();
        assert_eq!(storage.get("key").await.unwrap(), b"v2");
    }
}
