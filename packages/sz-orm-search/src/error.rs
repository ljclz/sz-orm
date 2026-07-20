//! Search 错误类型

use std::fmt;

/// Search 操作错误
#[derive(Debug)]
pub enum SearchError {
    /// 索引未找到
    NotFound(String),
    /// 文档未找到
    DocNotFound { index: String, id: String },
    /// 查询语法错误
    InvalidQuery(String),
    /// 索引已存在
    IndexAlreadyExists(String),
    /// 索引配置无效
    InvalidConfig(String),
    /// 序列化/反序列化错误
    Serialization(String),
    /// 查询执行错误
    Query(String),
    /// 连接错误
    Connection(String),
}

impl fmt::Display for SearchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SearchError::NotFound(msg) => write!(f, "not found: {}", msg),
            SearchError::DocNotFound { index, id } => {
                write!(f, "document not found: {}/{}", index, id)
            }
            SearchError::InvalidQuery(msg) => write!(f, "invalid query: {}", msg),
            SearchError::IndexAlreadyExists(msg) => {
                write!(f, "index already exists: {}", msg)
            }
            SearchError::InvalidConfig(msg) => write!(f, "invalid config: {}", msg),
            SearchError::Serialization(msg) => write!(f, "serialization error: {}", msg),
            SearchError::Query(msg) => write!(f, "query error: {}", msg),
            SearchError::Connection(msg) => write!(f, "connection error: {}", msg),
        }
    }
}

impl std::error::Error for SearchError {}

impl From<serde_json::Error> for SearchError {
    fn from(err: serde_json::Error) -> Self {
        SearchError::Serialization(err.to_string())
    }
}

impl From<std::io::Error> for SearchError {
    fn from(err: std::io::Error) -> Self {
        SearchError::Query(err.to_string())
    }
}
