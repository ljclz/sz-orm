//! # WASM 持久化错误类型
//!
//! 仅在 `persistence` feature 启用时编译。

use thiserror::Error;

/// WASM 持久化错误
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum WasmPersistenceError {
    /// IndexedDB 不可用（非浏览器环境或被禁用）
    #[error("IndexedDB is not available in this environment")]
    Unavailable,

    /// 恢复数据时版本不匹配
    #[error("storage version mismatch: expected {expected}, found {found}")]
    RestoreError { expected: u32, found: u32 },

    /// IndexedDB 操作错误
    #[error("IndexedDB error: {0}")]
    IndexedDbError(String),

    /// 序列化/反序列化错误
    #[error("serialization error: {0}")]
    SerializationError(String),
}
