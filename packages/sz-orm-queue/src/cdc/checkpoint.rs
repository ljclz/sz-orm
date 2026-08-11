//! CheckpointManager：CDC 消费位点持久化与断点续传

use std::sync::RwLock;

use super::{CdcCheckpoint, CheckpointPosition};

/// 检查点管理器
pub struct CheckpointManager {
    checkpoint: RwLock<Option<CdcCheckpoint>>,
    store_path: Option<String>,
    failed: RwLock<bool>,
}

impl CheckpointManager {
    pub fn new() -> Self {
        Self {
            checkpoint: RwLock::new(None),
            store_path: None,
            failed: RwLock::new(false),
        }
    }

    pub fn with_file_store(path: String) -> Self {
        Self {
            checkpoint: RwLock::new(None),
            store_path: Some(path),
            failed: RwLock::new(false),
        }
    }

    /// 保存检查点
    pub fn save_checkpoint(&self, checkpoint: &CdcCheckpoint) -> Result<(), CheckpointError> {
        if let Some(path) = &self.store_path {
            let json = serde_json::to_string(checkpoint)
                .map_err(|e| CheckpointError::SerializationFailed(e.to_string()))?;
            std::fs::write(path, json).map_err(|e| CheckpointError::IoFailed(e.to_string()))?;
        }
        let mut cp = self.checkpoint.write().expect("checkpoint lock poisoned");
        *cp = Some(checkpoint.clone());
        let mut failed = self.failed.write().expect("failed lock poisoned");
        *failed = false;
        Ok(())
    }

    /// 加载检查点
    pub fn load_checkpoint(&self) -> Result<Option<CdcCheckpoint>, CheckpointError> {
        if let Some(path) = &self.store_path {
            if std::path::Path::new(path).exists() {
                let data = std::fs::read_to_string(path)
                    .map_err(|e| CheckpointError::IoFailed(e.to_string()))?;
                let checkpoint: CdcCheckpoint = serde_json::from_str(&data)
                    .map_err(|e| CheckpointError::SerializationFailed(e.to_string()))?;
                let mut cp = self.checkpoint.write().expect("checkpoint lock poisoned");
                *cp = Some(checkpoint.clone());
                return Ok(Some(checkpoint));
            }
        }
        let cp = self.checkpoint.read().expect("checkpoint lock poisoned");
        Ok(cp.clone())
    }

    /// 模拟持久化失败
    pub fn mark_failed(&self) {
        let mut failed = self.failed.write().expect("failed lock poisoned");
        *failed = true;
    }

    /// 持久化是否失败
    pub fn is_failed(&self) -> bool {
        *self.failed.read().expect("failed lock poisoned")
    }

    /// 获取当前检查点（内存）
    pub fn current(&self) -> Option<CdcCheckpoint> {
        self.checkpoint
            .read()
            .expect("checkpoint lock poisoned")
            .clone()
    }

    /// 从检查点位置获取续传位点
    pub fn resume_position(&self) -> Option<CheckpointPosition> {
        self.checkpoint
            .read()
            .expect("checkpoint lock poisoned")
            .as_ref()
            .map(|cp| cp.position.clone())
    }

    /// 清除检查点
    pub fn clear(&self) {
        let mut cp = self.checkpoint.write().expect("checkpoint lock poisoned");
        *cp = None;
    }
}

impl Default for CheckpointManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 检查点错误
#[derive(Debug, Clone)]
pub enum CheckpointError {
    IoFailed(String),
    SerializationFailed(String),
}

impl std::fmt::Display for CheckpointError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CheckpointError::IoFailed(msg) => write!(f, "checkpoint IO failed: {msg}"),
            CheckpointError::SerializationFailed(msg) => {
                write!(f, "checkpoint serialization failed: {msg}")
            }
        }
    }
}

impl std::error::Error for CheckpointError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cdc::DbType;

    fn make_checkpoint(dialect: DbType, position: CheckpointPosition) -> CdcCheckpoint {
        CdcCheckpoint {
            dialect,
            position,
            updated_at: 1234567890,
        }
    }

    #[test]
    fn test_save_and_load_checkpoint_memory() {
        let manager = CheckpointManager::new();
        let cp = make_checkpoint(DbType::Postgres, CheckpointPosition::WalLsn(100));

        manager.save_checkpoint(&cp).unwrap();
        let loaded = manager.load_checkpoint().unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().position, CheckpointPosition::WalLsn(100));
    }

    #[test]
    fn test_load_empty_checkpoint() {
        let manager = CheckpointManager::new();
        let loaded = manager.load_checkpoint().unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_resume_position() {
        let manager = CheckpointManager::new();
        let cp = make_checkpoint(
            DbType::Mysql,
            CheckpointPosition::BinlogGtid("gtid-1".to_string()),
        );
        manager.save_checkpoint(&cp).unwrap();

        let pos = manager.resume_position().unwrap();
        assert_eq!(pos, CheckpointPosition::BinlogGtid("gtid-1".to_string()));
    }

    #[test]
    fn test_clear_checkpoint() {
        let manager = CheckpointManager::new();
        let cp = make_checkpoint(DbType::Postgres, CheckpointPosition::WalLsn(100));
        manager.save_checkpoint(&cp).unwrap();
        assert!(manager.current().is_some());
        manager.clear();
        assert!(manager.current().is_none());
    }

    #[test]
    fn test_mark_failed() {
        let manager = CheckpointManager::new();
        assert!(!manager.is_failed());
        manager.mark_failed();
        assert!(manager.is_failed());
    }

    #[test]
    fn test_save_clears_failed() {
        let manager = CheckpointManager::new();
        manager.mark_failed();
        assert!(manager.is_failed());
        let cp = make_checkpoint(DbType::Postgres, CheckpointPosition::WalLsn(100));
        manager.save_checkpoint(&cp).unwrap();
        assert!(!manager.is_failed());
    }

    #[test]
    fn test_file_store_save_and_load() {
        let path = std::env::temp_dir().join("cdc_checkpoint_test.json");
        let path_str = path.to_str().unwrap().to_string();
        let manager = CheckpointManager::with_file_store(path_str.clone());
        let cp = make_checkpoint(DbType::Sqlite, CheckpointPosition::TriggerSeq(42));

        manager.save_checkpoint(&cp).unwrap();
        let loaded = manager.load_checkpoint().unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().position, CheckpointPosition::TriggerSeq(42));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_file_store_load_nonexistent() {
        let manager =
            CheckpointManager::with_file_store("/nonexistent/path/checkpoint.json".to_string());
        let loaded = manager.load_checkpoint().unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_checkpoint_error_display() {
        let err = CheckpointError::IoFailed("disk full".to_string());
        assert!(err.to_string().contains("disk full"));
    }
}
