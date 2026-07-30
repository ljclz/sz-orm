//! # Huawei OBS Storage（**MOCK-ONLY，非生产可用**）
//!
//! ⚠️ **重要警告：本模块为内存 Mock 实现，未集成真实华为云 OBS SDK。**
//!
//! - 所有数据存储在进程内 `HashMap`，重启即丢失
//! - 不执行任何 HTTP 请求，不与真实 OBS 服务交互
//! - 不支持认证、签名、TLS、分片上传等任何 OBS 生产特性
//! - **请勿用于生产环境**——仅适用于单元测试与本地开发
//!
//! 如需真实 OBS 集成，请基于 [`crate::storage::Storage`] trait
//! 接入华为云官方 SDK 实现。

use crate::error::StorageError;
use crate::storage::Storage;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 华为云 OBS 存储后端（**Mock 实现**）
///
/// ⚠️ 仅用于测试。所有数据存储在内存 `HashMap`，不与真实 OBS 服务交互。
pub struct HuaweiObsStorage {
    pub bucket: String,
    pub endpoint: String,
    store: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl HuaweiObsStorage {
    pub fn new(bucket: impl Into<String>, endpoint: impl Into<String>) -> Self {
        Self {
            bucket: bucket.into(),
            endpoint: endpoint.into(),
            store: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn url_for(&self, key: &str) -> String {
        format!("obs://{}.{}/{}", self.bucket, self.endpoint, key)
    }
}

#[async_trait]
impl Storage for HuaweiObsStorage {
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
            .ok_or_else(|| StorageError::NotFound(format!("obs://{}/{}", self.bucket, key)))
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
    async fn test_huawei_put_and_get() {
        let storage = HuaweiObsStorage::new("my-bucket", "obs.cn-north-4.myhuaweicloud.com");
        let url = storage
            .put("a.txt", b"huawei-data", "text/plain")
            .await
            .unwrap();
        assert!(url.starts_with("obs://my-bucket.obs.cn-north-4.myhuaweicloud.com/"));

        let data = storage.get("a.txt").await.unwrap();
        assert_eq!(data, b"huawei-data");
    }

    #[tokio::test]
    async fn test_huawei_get_not_found() {
        let storage = HuaweiObsStorage::new("bucket", "obs.cn-north-4.myhuaweicloud.com");
        let result = storage.get("missing").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), StorageError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_huawei_delete_and_exists() {
        let storage = HuaweiObsStorage::new("bucket", "obs.cn-north-4.myhuaweicloud.com");
        storage.put("key", b"data", "text/plain").await.unwrap();
        assert!(storage.exists("key").await.unwrap());

        storage.delete("key").await.unwrap();
        assert!(!storage.exists("key").await.unwrap());
    }

    #[tokio::test]
    async fn test_huawei_overwrite() {
        let storage = HuaweiObsStorage::new("bucket", "obs.cn-north-4.myhuaweicloud.com");
        storage.put("key", b"v1", "text/plain").await.unwrap();
        storage.put("key", b"v2", "text/plain").await.unwrap();
        assert_eq!(storage.get("key").await.unwrap(), b"v2");
    }

    #[tokio::test]
    async fn test_huawei_multiple_keys() {
        let storage = HuaweiObsStorage::new("bucket", "obs.cn-north-4.myhuaweicloud.com");
        storage.put("a", b"a-data", "text/plain").await.unwrap();
        storage.put("b", b"b-data", "text/plain").await.unwrap();

        assert_eq!(storage.get("a").await.unwrap(), b"a-data");
        assert_eq!(storage.get("b").await.unwrap(), b"b-data");

        storage.delete("a").await.unwrap();
        assert!(!storage.exists("a").await.unwrap());
        assert!(storage.exists("b").await.unwrap());
    }
}
