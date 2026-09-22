//! v8.0.0 组 6：Python 异步流绑定（binding-async-stream feature gate）
//!
//! 提供 `AsyncStreamBinding`，流式返回查询结果，避免全量物化。
//! 复用 sz-orm-stream 的 `StreamResultSet` / `AsyncBackpressureController`。

pub mod async_stream_binding;

pub use async_stream_binding::{AsyncStreamBinding, AsyncStreamConfig, RowStream};

/// 生态扩展错误
#[derive(Debug, Clone)]
pub enum EcoError {
    /// 流中断（支持断点续流）
    StreamBroken,
    /// 流未启用（回退同步接口）
    StreamUnavailable,
    /// 无效 SQL
    InvalidSql(String),
}

impl std::fmt::Display for EcoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StreamBroken => write!(f, "[BINDING_STREAM_BROKEN] 流中断"),
            Self::StreamUnavailable => write!(f, "[BINDING_STREAM_UNAVAILABLE] 流未启用"),
            Self::InvalidSql(msg) => write!(f, "[BINDING_INVALID_SQL] 无效 SQL: {}", msg),
        }
    }
}

impl std::error::Error for EcoError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eco_error_display_stream_broken() {
        let err = EcoError::StreamBroken;
        let s = format!("{}", err);
        assert!(s.contains("BINDING_STREAM_BROKEN"));
        assert!(s.contains("流中断"));
    }

    #[test]
    fn test_eco_error_display_stream_unavailable() {
        let err = EcoError::StreamUnavailable;
        let s = format!("{}", err);
        assert!(s.contains("BINDING_STREAM_UNAVAILABLE"));
    }

    #[test]
    fn test_eco_error_display_invalid_sql() {
        let err = EcoError::InvalidSql("syntax error".to_string());
        let s = format!("{}", err);
        assert!(s.contains("syntax error"));
    }

    #[test]
    fn test_eco_error_is_error() {
        let err = EcoError::StreamBroken;
        let _: &dyn std::error::Error = &err;
    }
}
