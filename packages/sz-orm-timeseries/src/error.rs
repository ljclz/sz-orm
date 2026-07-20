//! TimescaleDB 错误类型

use std::fmt;

/// TimescaleDB 操作错误
#[derive(Debug)]
pub enum TimescaleError {
    /// 指标未找到
    NotFound(String),
    /// 时间范围无效（start >= end）
    InvalidTimeRange { start: String, end: String },
    /// 聚合类型不支持
    UnsupportedAggregation(String),
    /// SQL 执行错误
    Query(String),
    /// 连接错误
    Connection(String),
    /// 配置错误
    InvalidConfig(String),
}

impl fmt::Display for TimescaleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimescaleError::NotFound(msg) => write!(f, "not found: {}", msg),
            TimescaleError::InvalidTimeRange { start, end } => {
                write!(f, "invalid time range: start {} >= end {}", start, end)
            }
            TimescaleError::UnsupportedAggregation(msg) => {
                write!(f, "unsupported aggregation: {}", msg)
            }
            TimescaleError::Query(msg) => write!(f, "query error: {}", msg),
            TimescaleError::Connection(msg) => write!(f, "connection error: {}", msg),
            TimescaleError::InvalidConfig(msg) => write!(f, "invalid config: {}", msg),
        }
    }
}

impl std::error::Error for TimescaleError {}

impl From<std::io::Error> for TimescaleError {
    fn from(err: std::io::Error) -> Self {
        TimescaleError::Query(err.to_string())
    }
}

impl From<chrono::ParseError> for TimescaleError {
    fn from(err: chrono::ParseError) -> Self {
        TimescaleError::InvalidConfig(format!("time parse error: {}", err))
    }
}
