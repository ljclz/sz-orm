//! 并行查询错误类型

use std::fmt;

/// 并行查询错误
#[derive(Debug, Clone)]
pub enum ParallelQueryError {
    /// 传入空查询列表
    NoQueries,
    /// 整体超时
    OverallTimeout,
    /// 全部查询失败
    AllQueriesFailed,
    /// 并发度无效
    InvalidConcurrency,
    /// 内部错误
    Internal { reason: String },
}

impl fmt::Display for ParallelQueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParallelQueryError::NoQueries => write!(f, "no queries provided"),
            ParallelQueryError::OverallTimeout => write!(f, "overall timeout exceeded"),
            ParallelQueryError::AllQueriesFailed => write!(f, "all queries failed"),
            ParallelQueryError::InvalidConcurrency => {
                write!(f, "invalid concurrency (must be > 0)")
            }
            ParallelQueryError::Internal { reason } => write!(f, "internal error: {reason}"),
        }
    }
}

impl std::error::Error for ParallelQueryError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display() {
        assert!(ParallelQueryError::NoQueries
            .to_string()
            .contains("no queries"));
        assert!(ParallelQueryError::OverallTimeout
            .to_string()
            .contains("timeout"));
        assert!(ParallelQueryError::AllQueriesFailed
            .to_string()
            .contains("all queries"));
        assert!(ParallelQueryError::InvalidConcurrency
            .to_string()
            .contains("concurrency"));
        assert!(ParallelQueryError::Internal {
            reason: "test".into()
        }
        .to_string()
        .contains("test"));
    }
}
