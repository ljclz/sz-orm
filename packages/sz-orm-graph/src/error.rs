//! # GraphError — 图数据库错误类型
//!
//! 6 变体，DSN 脱敏。

use thiserror::Error;

/// 图数据库错误
#[derive(Debug, Clone, Error)]
pub enum GraphError {
    /// 连接错误（DSN 脱敏）
    #[error("connection error: {0}")]
    ConnectionError(String),

    /// 查询错误
    #[error("query error: {0}")]
    QueryError(String),

    /// 结果映射错误
    #[error("mapping error: {0}")]
    MappingError(String),

    /// SQL 不支持（图接口拒绝 SQL 透传）
    #[error("SQL is not supported in graph interface: {0}")]
    SqlNotSupported(String),

    /// 参数化错误（检测到字面量拼接）
    #[error("parameterization error: {0}")]
    ParameterizationError(String),

    /// 驱动错误
    #[error("driver error: {0}")]
    DriverError(String),
}

/// DSN 脱敏：将密码部分替换为 ***
pub fn sanitize_dsn(dsn: &str) -> String {
    if let Some(at_pos) = dsn.find('@') {
        if let Some(scheme_end) = dsn.find("://") {
            let auth_part = &dsn[scheme_end + 3..at_pos];
            if let Some(colon_pos) = auth_part.find(':') {
                let user = &auth_part[..colon_pos];
                let rest = &dsn[at_pos..];
                let scheme = &dsn[..scheme_end + 3];
                return format!("{}{}:***{}", scheme, user, rest);
            }
        }
    }
    dsn.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_dsn_with_password() {
        let dsn = "neo4j://neo4j:test123@127.0.0.1:7687";
        let sanitized = sanitize_dsn(dsn);
        assert!(!sanitized.contains("test123"));
        assert!(sanitized.contains("***"));
        assert!(sanitized.contains("neo4j://neo4j:***@127.0.0.1:7687"));
    }

    #[test]
    fn test_sanitize_dsn_without_password() {
        let dsn = "neo4j://127.0.0.1:7687";
        let sanitized = sanitize_dsn(dsn);
        assert_eq!(sanitized, dsn);
    }

    #[test]
    fn test_graph_error_display() {
        let e = GraphError::SqlNotSupported("SELECT * FROM users".into());
        assert!(e.to_string().contains("SQL is not supported"));
    }
}
