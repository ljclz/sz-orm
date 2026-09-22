//! 模型版本注册表：版本注册 + 签名验证 + 存储 + 回滚
//!
//! 禁止未签名模型，回滚 ≤ 5s，可追溯。

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

/// 模型版本 ID
pub type ModelVersionId = String;

/// 模型版本元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelVersionMeta {
    pub version_id: ModelVersionId,
    pub model_name: String,
    pub signature: String,
    pub created_at: SystemTime,
    pub is_canary: bool,
    pub checksum: String,
}

/// 版本注册错误
#[derive(Debug, thiserror::Error)]
pub enum ModelVersionError {
    #[error("模型未签名: {0}")]
    ModelNotSigned(String),
    #[error("版本不存在: {0}")]
    VersionNotFound(ModelVersionId),
    #[error("回滚失败: {0}")]
    RollbackFailed(String),
    #[error("签名无效: {0}")]
    SignatureInvalid(String),
    #[error("版本已存在: {0}")]
    VersionAlreadyExists(ModelVersionId),
    #[error("快照持久化失败: {0}")]
    SnapshotFailed(String),
}

/// 版本注册记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionRegistryEntry {
    pub meta: ModelVersionMeta,
    pub registered_at: SystemTime,
    pub is_active: bool,
}

/// 模型版本注册表
pub struct ModelVersionRegistry {
    versions: RwLock<HashMap<ModelVersionId, VersionRegistryEntry>>,
    active_version: RwLock<Option<ModelVersionId>>,
    rollback_timeout: Duration,
}

impl ModelVersionRegistry {
    pub fn new() -> Self {
        Self {
            versions: RwLock::new(HashMap::new()),
            active_version: RwLock::new(None),
            rollback_timeout: Duration::from_secs(5),
        }
    }

    pub fn with_rollback_timeout(timeout: Duration) -> Self {
        Self {
            versions: RwLock::new(HashMap::new()),
            active_version: RwLock::new(None),
            rollback_timeout: timeout,
        }
    }

    /// 注册模型版本（禁止未签名模型）
    pub async fn register(
        &self,
        meta: ModelVersionMeta,
    ) -> Result<ModelVersionId, ModelVersionError> {
        if meta.signature.is_empty() {
            return Err(ModelVersionError::ModelNotSigned(meta.version_id.clone()));
        }
        if meta.checksum.is_empty() {
            return Err(ModelVersionError::SignatureInvalid(
                "checksum 为空".to_string(),
            ));
        }
        let mut versions = self.versions.write();
        if versions.contains_key(&meta.version_id) {
            return Err(ModelVersionError::VersionAlreadyExists(
                meta.version_id.clone(),
            ));
        }
        let version_id = meta.version_id.clone();
        let is_first = versions.is_empty();
        let entry = VersionRegistryEntry {
            meta,
            registered_at: SystemTime::now(),
            is_active: is_first,
        };
        versions.insert(version_id.clone(), entry);
        if is_first {
            *self.active_version.write() = Some(version_id.clone());
        }
        Ok(version_id)
    }

    /// 回滚到指定版本（≤ 5s）
    pub async fn rollback(&self, to_version: &ModelVersionId) -> Result<(), ModelVersionError> {
        let start = SystemTime::now();
        let mut versions = self.versions.write();
        if !versions.contains_key(to_version) {
            return Err(ModelVersionError::VersionNotFound(to_version.clone()));
        }
        for v in versions.values_mut() {
            v.is_active = false;
        }
        if let Some(entry) = versions.get_mut(to_version) {
            entry.is_active = true;
        }
        drop(versions);
        *self.active_version.write() = Some(to_version.clone());
        let elapsed = start.elapsed().unwrap_or(Duration::ZERO);
        if elapsed > self.rollback_timeout {
            return Err(ModelVersionError::RollbackFailed(format!(
                "回滚耗时 {:?} 超过阈值 {:?}",
                elapsed, self.rollback_timeout
            )));
        }
        Ok(())
    }

    /// 获取当前活跃版本
    pub fn active_version(&self) -> Option<ModelVersionId> {
        self.active_version.read().clone()
    }

    /// 获取版本元数据
    pub fn get_version(&self, version_id: &str) -> Option<ModelVersionMeta> {
        self.versions.read().get(version_id).map(|e| e.meta.clone())
    }

    /// 列出所有版本
    pub fn list_versions(&self) -> Vec<VersionRegistryEntry> {
        self.versions.read().values().cloned().collect()
    }

    /// 持久化快照到 JSON
    pub fn snapshot(&self, path: &str) -> Result<usize, ModelVersionError> {
        let versions = self.versions.read();
        let entries: Vec<_> = versions.values().cloned().collect();
        let json = serde_json::to_string_pretty(&entries)
            .map_err(|e| ModelVersionError::SnapshotFailed(e.to_string()))?;
        std::fs::write(path, json).map_err(|e| ModelVersionError::SnapshotFailed(e.to_string()))?;
        Ok(entries.len())
    }

    /// 从 JSON 快照恢复
    pub fn restore(&self, path: &str) -> Result<usize, ModelVersionError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ModelVersionError::SnapshotFailed(e.to_string()))?;
        let entries: Vec<VersionRegistryEntry> = serde_json::from_str(&content)
            .map_err(|e| ModelVersionError::SnapshotFailed(e.to_string()))?;
        let mut versions = self.versions.write();
        versions.clear();
        let mut active = None;
        for entry in entries {
            if entry.is_active {
                active = Some(entry.meta.version_id.clone());
            }
            versions.insert(entry.meta.version_id.clone(), entry);
        }
        *self.active_version.write() = active;
        Ok(versions.len())
    }
}

impl Default for ModelVersionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_meta(id: &str, signed: bool) -> ModelVersionMeta {
        ModelVersionMeta {
            version_id: id.to_string(),
            model_name: "gpt-4o".to_string(),
            signature: if signed {
                "sig-abc123".to_string()
            } else {
                String::new()
            },
            created_at: SystemTime::now(),
            is_canary: false,
            checksum: "sha256:abc".to_string(),
        }
    }

    #[tokio::test]
    async fn test_register_success() {
        let registry = ModelVersionRegistry::new();
        let meta = make_meta("v1", true);
        let id = registry.register(meta).await.unwrap();
        assert_eq!(id, "v1");
        assert_eq!(registry.active_version(), Some("v1".to_string()));
    }

    #[tokio::test]
    async fn test_register_unsigned_rejected() {
        let registry = ModelVersionRegistry::new();
        let meta = make_meta("v1", false);
        let result = registry.register(meta).await;
        assert!(matches!(result, Err(ModelVersionError::ModelNotSigned(_))));
    }

    #[tokio::test]
    async fn test_register_duplicate_rejected() {
        let registry = ModelVersionRegistry::new();
        registry.register(make_meta("v1", true)).await.unwrap();
        let result = registry.register(make_meta("v1", true)).await;
        assert!(matches!(
            result,
            Err(ModelVersionError::VersionAlreadyExists(_))
        ));
    }

    #[tokio::test]
    async fn test_rollback_success() {
        let registry = ModelVersionRegistry::new();
        registry.register(make_meta("v1", true)).await.unwrap();
        registry.register(make_meta("v2", true)).await.unwrap();
        assert_eq!(registry.active_version(), Some("v1".to_string()));
        registry.rollback(&"v2".to_string()).await.unwrap();
        assert_eq!(registry.active_version(), Some("v2".to_string()));
        let v1 = registry.get_version("v1").unwrap();
        let v2 = registry.get_version("v2").unwrap();
        let entries = registry.list_versions();
        let v1_entry = entries.iter().find(|e| e.meta.version_id == "v1").unwrap();
        let v2_entry = entries.iter().find(|e| e.meta.version_id == "v2").unwrap();
        assert!(!v1_entry.is_active);
        assert!(v2_entry.is_active);
        assert_eq!(v1.version_id, "v1");
        assert_eq!(v2.version_id, "v2");
    }

    #[tokio::test]
    async fn test_rollback_nonexistent() {
        let registry = ModelVersionRegistry::new();
        let result = registry.rollback(&"nonexistent".to_string()).await;
        assert!(matches!(result, Err(ModelVersionError::VersionNotFound(_))));
    }

    #[tokio::test]
    async fn test_version_traceability() {
        let registry = ModelVersionRegistry::new();
        for i in 1..=5 {
            registry
                .register(make_meta(&format!("v{}", i), true))
                .await
                .unwrap();
        }
        let versions = registry.list_versions();
        assert_eq!(versions.len(), 5);
        registry.rollback(&"v3".to_string()).await.unwrap();
        assert_eq!(registry.active_version(), Some("v3".to_string()));
        registry.rollback(&"v5".to_string()).await.unwrap();
        assert_eq!(registry.active_version(), Some("v5".to_string()));
    }

    #[tokio::test]
    async fn test_snapshot_and_restore() {
        let registry = ModelVersionRegistry::new();
        registry.register(make_meta("v1", true)).await.unwrap();
        registry.register(make_meta("v2", true)).await.unwrap();
        let path = std::env::temp_dir().join("test_model_version_snapshot.json");
        let path_str = path.to_str().unwrap();
        registry.snapshot(path_str).unwrap();
        let registry2 = ModelVersionRegistry::new();
        let count = registry2.restore(path_str).unwrap();
        assert_eq!(count, 2);
        assert!(registry2.get_version("v1").is_some());
        assert!(registry2.get_version("v2").is_some());
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn test_empty_checksum_rejected() {
        let mut meta = make_meta("v1", true);
        meta.checksum = String::new();
        let registry = ModelVersionRegistry::new();
        let result = registry.register(meta).await;
        assert!(matches!(
            result,
            Err(ModelVersionError::SignatureInvalid(_))
        ));
    }
}
