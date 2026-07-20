//! Vector 操作错误类型

use std::fmt;

/// Vector 操作错误
#[derive(Debug)]
pub enum VectorError {
    /// Collection 不存在
    CollectionNotFound(String),
    /// 向量维度不匹配
    DimensionMismatch { expected: usize, actual: usize },
    /// 操作不支持（stub 模式）
    Unsupported(String),
    /// SQL 执行错误
    Query(String),
    /// 连接错误
    Connection(String),
    /// 配置错误
    InvalidConfig(String),
    /// 标识符校验失败
    InvalidIdentifier(String),
}

impl fmt::Display for VectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VectorError::CollectionNotFound(name) => {
                write!(f, "collection not found: {}", name)
            }
            VectorError::DimensionMismatch { expected, actual } => {
                write!(
                    f,
                    "dimension mismatch: expected {}, got {}",
                    expected, actual
                )
            }
            VectorError::Unsupported(msg) => {
                write!(f, "unsupported operation: {}", msg)
            }
            VectorError::Query(msg) => write!(f, "query error: {}", msg),
            VectorError::Connection(msg) => write!(f, "connection error: {}", msg),
            VectorError::InvalidConfig(msg) => write!(f, "invalid config: {}", msg),
            VectorError::InvalidIdentifier(msg) => write!(f, "invalid identifier: {}", msg),
        }
    }
}

impl std::error::Error for VectorError {}
