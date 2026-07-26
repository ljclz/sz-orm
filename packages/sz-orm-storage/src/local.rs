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

    /// 测试数据目录：优先 F:\test\data（用户规范），回退到环境变量或系统 temp（CI/Linux）
    ///
    /// 注意：仅检查目录存在不足以保证可用——还需验证可写性，
    /// 以避免在受限沙箱环境中因目录存在但不可写导致测试失败。
    fn test_data_base() -> std::path::PathBuf {
        let f_drive = std::path::Path::new("F:\\test\\data");
        if is_dir_writable(f_drive) {
            return f_drive.to_path_buf();
        }
        if let Ok(dir) = std::env::var("SZ_ORM_TEST_DATA_DIR") {
            let p = std::path::PathBuf::from(&dir);
            if is_dir_writable(&p) {
                return p;
            }
        }
        std::env::temp_dir()
    }

    /// 检查目录是否存在且可写：尝试在其中创建并删除一个探测文件
    fn is_dir_writable(dir: &std::path::Path) -> bool {
        if !dir.exists() {
            return false;
        }
        let probe = dir.join(format!(".probe_{}", std::process::id()));
        match std::fs::File::create(&probe) {
            Ok(_) => {
                let _ = std::fs::remove_file(&probe);
                true
            }
            Err(_) => false,
        }
    }

    fn temp_dir() -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        test_data_base().join(format!("local_storage_test_{:x}", nanos))
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
