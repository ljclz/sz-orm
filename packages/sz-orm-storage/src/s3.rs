use crate::error::StorageError;
use crate::storage::Storage;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct S3Storage {
    pub bucket: String,
    pub region: String,
    store: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl S3Storage {
    pub fn new(bucket: impl Into<String>, region: impl Into<String>) -> Self {
        Self {
            bucket: bucket.into(),
            region: region.into(),
            store: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn url_for(&self, key: &str) -> String {
        format!("s3://{}.{}/{}", self.bucket, self.region, key)
    }
}

#[async_trait]
impl Storage for S3Storage {
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
            .ok_or_else(|| StorageError::NotFound(format!("s3://{}/{}", self.bucket, key)))
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
    async fn test_s3_put_and_get() {
        let storage = S3Storage::new("my-bucket", "us-east-1");
        let url = storage
            .put("file.txt", b"hello s3", "text/plain")
            .await
            .unwrap();
        assert!(url.starts_with("s3://my-bucket.us-east-1/"));
        assert!(url.contains("file.txt"));

        let data = storage.get("file.txt").await.unwrap();
        assert_eq!(data, b"hello s3");
    }

    #[tokio::test]
    async fn test_s3_get_not_found() {
        let storage = S3Storage::new("bucket", "us-west-2");
        let result = storage.get("missing.txt").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), StorageError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_s3_delete() {
        let storage = S3Storage::new("bucket", "us-east-1");
        storage.put("del.txt", b"data", "text/plain").await.unwrap();
        assert!(storage.exists("del.txt").await.unwrap());

        storage.delete("del.txt").await.unwrap();
        assert!(!storage.exists("del.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_s3_exists_false_for_missing() {
        let storage = S3Storage::new("bucket", "us-east-1");
        assert!(!storage.exists("nope").await.unwrap());
    }

    #[tokio::test]
    async fn test_s3_overwrite() {
        let storage = S3Storage::new("bucket", "us-east-1");
        storage.put("key", b"v1", "text/plain").await.unwrap();
        storage.put("key", b"v2", "text/plain").await.unwrap();
        let data = storage.get("key").await.unwrap();
        assert_eq!(data, b"v2");
    }

    #[tokio::test]
    async fn test_s3_multiple_keys() {
        let storage = S3Storage::new("bucket", "us-east-1");
        storage.put("a", b"a-data", "text/plain").await.unwrap();
        storage.put("b", b"b-data", "text/plain").await.unwrap();
        storage.put("c", b"c-data", "text/plain").await.unwrap();

        assert_eq!(storage.get("a").await.unwrap(), b"a-data");
        assert_eq!(storage.get("b").await.unwrap(), b"b-data");
        assert_eq!(storage.get("c").await.unwrap(), b"c-data");

        storage.delete("b").await.unwrap();
        assert!(storage.exists("a").await.unwrap());
        assert!(!storage.exists("b").await.unwrap());
        assert!(storage.exists("c").await.unwrap());
    }
}
