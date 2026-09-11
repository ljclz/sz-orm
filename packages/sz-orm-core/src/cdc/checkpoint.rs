//! CDC 位点持久化（v6.8.0）
//!
//! 支持内存位点（测试用）和文件位点（生产用）。

use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::RwLock;

use super::event::ChangePosition;

/// 原子写入文件：先写入临时文件再 rename，避免进程崩溃导致文件损坏。
fn write_atomic(path: &std::path::Path, data: &[u8]) -> Result<(), CdcError> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data).map_err(|e| CdcError::IoError(e.to_string()))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        CdcError::IoError(e.to_string())
    })
}

/// CDC 位点存储
pub struct CdcCheckpointStore {
    inner: RwLock<CheckpointInner>,
}

enum CheckpointInner {
    /// 内存位点
    Memory(Option<ChangePosition>),
    /// 文件位点
    File {
        path: PathBuf,
        current: Option<ChangePosition>,
    },
}

impl CdcCheckpointStore {
    /// 创建内存位点存储
    pub fn in_memory() -> Self {
        Self {
            inner: RwLock::new(CheckpointInner::Memory(None)),
        }
    }

    /// 创建文件位点存储
    pub fn file(path: PathBuf) -> Self {
        let current = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok());
        Self {
            inner: RwLock::new(CheckpointInner::File { path, current }),
        }
    }

    /// 保存位点
    pub async fn save_checkpoint(&self, position: &ChangePosition) -> Result<(), CdcError> {
        let mut inner = self.inner.write();
        match &mut *inner {
            CheckpointInner::Memory(slot) => {
                *slot = Some(position.clone());
            }
            CheckpointInner::File { path, current } => {
                let json = serde_json::to_string(position)
                    .map_err(|e| CdcError::SerializeError(e.to_string()))?;
                write_atomic(path, json.as_bytes())?;
                *current = Some(position.clone());
            }
        }
        Ok(())
    }

    /// 加载位点
    pub async fn load_checkpoint(&self) -> Result<Option<ChangePosition>, CdcError> {
        let inner = self.inner.read();
        match &*inner {
            CheckpointInner::Memory(slot) => Ok(slot.clone()),
            CheckpointInner::File { current, .. } => Ok(current.clone()),
        }
    }

    /// 清除位点
    pub async fn clear(&self) -> Result<(), CdcError> {
        let mut inner = self.inner.write();
        match &mut *inner {
            CheckpointInner::Memory(slot) => {
                *slot = None;
            }
            CheckpointInner::File { path, current } => {
                let _ = std::fs::remove_file(path);
                *current = None;
            }
        }
        Ok(())
    }

    /// 带重试的位点保存（v6.8.0 CDC-RESUME-01）
    ///
    /// 位点写入失败时重试 `max_retries` 次，重试间隔指数退避。
    /// 全部重试失败后返回错误，**不推进位点**。
    pub async fn save_checkpoint_with_retry(
        &self,
        position: &ChangePosition,
        max_retries: u32,
    ) -> Result<(), CdcError> {
        let mut last_err = None;
        for attempt in 0..=max_retries {
            match self.save_checkpoint(position).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    last_err = Some(e);
                    if attempt < max_retries {
                        tokio::time::sleep(tokio::time::Duration::from_millis(
                            10 * (1u64 << attempt),
                        ))
                        .await;
                    }
                }
            }
        }
        Err(last_err.unwrap_or_else(|| CdcError::IoError("unknown retry failure".into())))
    }

    /// 断点续传恢复（v6.8.0 CDC-RESUME-01）
    ///
    /// 重启后调用：读取上次确认位点 → 返回恢复点。
    /// 无位点时返回 `None`，表示从头开始。
    pub async fn resume_from(&self) -> Result<Option<ChangePosition>, CdcError> {
        self.load_checkpoint().await
    }

    /// 检查位点是否已确认（v6.8.0 CDC-RESUME-SEC）
    ///
    /// 用于严格位点恢复：只有已确认的位点才会被 `load_checkpoint` 返回。
    pub async fn is_confirmed(&self, position: &ChangePosition) -> Result<bool, CdcError> {
        let saved = self.load_checkpoint().await?;
        Ok(saved.as_ref() == Some(position))
    }
}

impl Default for CdcCheckpointStore {
    fn default() -> Self {
        Self::in_memory()
    }
}

/// 共享位点存储
pub type SharedCheckpointStore = Arc<CdcCheckpointStore>;

/// CDC 错误
#[derive(Debug, thiserror::Error)]
pub enum CdcError {
    /// 位点已被 purge
    #[error("binlog purged: {0}")]
    BinlogPurged(String),
    /// 账号权限不足
    #[error("auth failed: {0}")]
    AuthFailed(String),
    /// 复制槽不存在
    #[error("slot missing: {0}")]
    SlotMissing(String),
    /// 背压
    #[error("backpressure: channel full")]
    Backpressure,
    /// IO 错误
    #[error("io error: {0}")]
    IoError(String),
    /// 序列化错误
    #[error("serialize error: {0}")]
    SerializeError(String),
    /// Sink 错误
    #[error("sink error: {0}")]
    SinkError(String),
    /// 连接错误
    #[error("connection error: {0}")]
    ConnectionError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_checkpoint_save_load() {
        let store = CdcCheckpointStore::in_memory();
        let pos = ChangePosition::MysqlBinlog {
            filename: "bin.001".to_string(),
            position: 100,
        };

        let store_clone = store;
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            store_clone.save_checkpoint(&pos).await.unwrap();
            let loaded = store_clone.load_checkpoint().await.unwrap();
            assert_eq!(loaded, Some(pos));
        });
    }

    #[test]
    fn in_memory_checkpoint_empty_initially() {
        let store = CdcCheckpointStore::in_memory();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let loaded = store.load_checkpoint().await.unwrap();
            assert!(loaded.is_none());
        });
    }

    #[test]
    fn in_memory_checkpoint_clear() {
        let store = CdcCheckpointStore::in_memory();
        let pos = ChangePosition::MysqlBinlog {
            filename: "bin.001".to_string(),
            position: 100,
        };

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            store.save_checkpoint(&pos).await.unwrap();
            store.clear().await.unwrap();
            let loaded = store.load_checkpoint().await.unwrap();
            assert!(loaded.is_none());
        });
    }

    #[test]
    fn file_checkpoint_save_load() {
        let path = PathBuf::from("test_cdc_checkpoint_file.json");
        let store = CdcCheckpointStore::file(path.clone());
        let pos = ChangePosition::MysqlBinlog {
            filename: "bin.002".to_string(),
            position: 200,
        };

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            store.save_checkpoint(&pos).await.unwrap();
            let loaded = store.load_checkpoint().await.unwrap();
            assert_eq!(loaded, Some(pos));
            store.clear().await.unwrap();
        });

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn file_checkpoint_atomic_write() {
        let path = PathBuf::from("test_cdc_checkpoint_atomic.json");
        let store = CdcCheckpointStore::file(path.clone());
        let pos = ChangePosition::MysqlBinlog {
            filename: "bin.003".to_string(),
            position: 300,
        };

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            store.save_checkpoint(&pos).await.unwrap();
            let loaded = store.load_checkpoint().await.unwrap();
            assert_eq!(loaded, Some(pos));
        });

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("tmp"));
    }

    #[test]
    fn checkpoint_overwrite() {
        let store = CdcCheckpointStore::in_memory();
        let pos1 = ChangePosition::MysqlBinlog {
            filename: "bin.001".to_string(),
            position: 100,
        };
        let pos2 = ChangePosition::MysqlBinlog {
            filename: "bin.001".to_string(),
            position: 200,
        };

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            store.save_checkpoint(&pos1).await.unwrap();
            store.save_checkpoint(&pos2).await.unwrap();
            let loaded = store.load_checkpoint().await.unwrap();
            assert_eq!(loaded, Some(pos2));
        });
    }
}
