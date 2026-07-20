//! PostGIS 错误类型

use std::fmt;

/// PostGIS 操作错误
#[derive(Debug)]
pub enum PostgisError {
    /// 几何数据无效
    InvalidGeometry(String),
    /// 坐标参考系统（SRID）不匹配
    SridMismatch { expected: i32, actual: i32 },
    /// 操作不支持
    Unsupported(String),
    /// SQL 执行错误
    Query(String),
    /// 连接错误
    Connection(String),
    /// 配置错误
    InvalidConfig(String),
    /// 几何类型不匹配
    TypeMismatch {
        expected: &'static str,
        actual: &'static str,
    },
}

impl fmt::Display for PostgisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PostgisError::InvalidGeometry(msg) => {
                write!(f, "invalid geometry: {}", msg)
            }
            PostgisError::SridMismatch { expected, actual } => {
                write!(f, "SRID mismatch: expected {}, got {}", expected, actual)
            }
            PostgisError::Unsupported(msg) => {
                write!(f, "unsupported operation: {}", msg)
            }
            PostgisError::Query(msg) => write!(f, "query error: {}", msg),
            PostgisError::Connection(msg) => write!(f, "connection error: {}", msg),
            PostgisError::InvalidConfig(msg) => write!(f, "invalid config: {}", msg),
            PostgisError::TypeMismatch { expected, actual } => {
                write!(
                    f,
                    "geometry type mismatch: expected {}, got {}",
                    expected, actual
                )
            }
        }
    }
}

impl std::error::Error for PostgisError {}

impl From<std::io::Error> for PostgisError {
    fn from(err: std::io::Error) -> Self {
        PostgisError::Query(err.to_string())
    }
}
