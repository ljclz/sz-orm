use crate::error::StorageError;
use crate::storage::Storage;
use async_trait::async_trait;
use std::path::PathBuf;

pub struct LocalStorage {
    pub base_path: String,
}

impl LocalStorage {
    pub fn new(base_path: impl Into<String>) -> Self {
        Self {
            base_path: base_path.into(),
        }
    }

    pub fn full_path(&self, key: &str) -> PathBuf {
        PathBuf::from(&self.base_path).join(key)
    }
}

#[async_trait]
impl Storage for LocalStorage {
    async fn put(
        &self,
        key: &str,
        data: &[u8],
        _content_type: &str,
    ) -> Result<String, StorageError> {
        let path = self.full_path(key);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&path, data).await?;
        Ok(format!("local://{}", key))
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>, StorageError> {
        let path = self.full_path(key);
        if !path.exists() {
            return Err(StorageError::NotFound(key.to_string()));
        }
        tokio::fs::read(&path).await.map_err(StorageError::from)
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let path = self.full_path(key);
        if path.exists() {
            tokio::fs::remove_file(&path).await?;
        }
        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        Ok(self.full_path(key).exists())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("local_storage_test_{:x}", nanos))
    }

    #[tokio::test]
    async fn test_local_put_and_get() {
        let dir = temp_dir();
        let storage = LocalStorage::new(dir.to_string_lossy());

        storage
            .put("file.txt", b"hello", "text/plain")
            .await
            .unwrap();
        let data = storage.get("file.txt").await.unwrap();
        assert_eq!(data, b"hello");

        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn test_local_get_not_found() {
        let dir = temp_dir();
        let storage = LocalStorage::new(dir.to_string_lossy());

        let result = storage.get("missing.txt").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), StorageError::NotFound(_)));

        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn test_local_delete() {
        let dir = temp_dir();
        let storage = LocalStorage::new(dir.to_string_lossy());

        storage
            .put("delete.txt", b"data", "text/plain")
            .await
            .unwrap();
        assert!(storage.exists("delete.txt").await.unwrap());

        storage.delete("delete.txt").await.unwrap();
        assert!(!storage.exists("delete.txt").await.unwrap());

        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn test_local_exists_false_for_missing() {
        let dir = temp_dir();
        let storage = LocalStorage::new(dir.to_string_lossy());
        assert!(!storage.exists("nope.txt").await.unwrap());
        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn test_local_creates_subdirectories() {
        let dir = temp_dir();
        let storage = LocalStorage::new(dir.to_string_lossy());

        storage
            .put("nested/deep/file.txt", b"nested", "text/plain")
            .await
            .unwrap();
        let data = storage.get("nested/deep/file.txt").await.unwrap();
        assert_eq!(data, b"nested");

        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn test_local_put_returns_url() {
        let dir = temp_dir();
        let storage = LocalStorage::new(dir.to_string_lossy());

        let url = storage.put("url.txt", b"data", "text/plain").await.unwrap();
        assert!(url.starts_with("local://"));
        assert!(url.contains("url.txt"));

        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn test_local_overwrite() {
        let dir = temp_dir();
        let storage = LocalStorage::new(dir.to_string_lossy());

        storage
            .put("overwrite.txt", b"v1", "text/plain")
            .await
            .unwrap();
        storage
            .put("overwrite.txt", b"v2", "text/plain")
            .await
            .unwrap();
        let data = storage.get("overwrite.txt").await.unwrap();
        assert_eq!(data, b"v2");

        tokio::fs::remove_dir_all(&dir).await.ok();
    }
}
