//! sqlx 错误到 sz-orm-core DbError 的映射

use sz_orm_core::{DbError, PoolError};

/// 将 sqlx::Error 转换为 DbError
pub fn map_sqlx_error(e: sqlx::Error) -> DbError {
    match e {
        sqlx::Error::Database(db_err) => {
            // 用 code() 和 message() 判断错误类型
            let msg = db_err.message().to_string();
            let code = db_err.code().map(|c| c.into_owned()).unwrap_or_default();
            // PostgreSQL SQLSTATE codes
            if code == "23505"
                || msg.contains("Duplicate entry")
                || msg.contains("unique constraint")
            {
                DbError::AlreadyExists(msg)
            } else if code == "23503"
                || code.starts_with("23")
                || msg.contains("foreign key constraint")
                || msg.contains("constraint")
            {
                DbError::ConstraintViolation(msg)
            } else if code.starts_with("42") || msg.contains("syntax") {
                DbError::InvalidInput(msg)
            } else {
                DbError::QueryError(msg)
            }
        }
        sqlx::Error::PoolClosed => {
            DbError::PoolError(PoolError::Internal("sqlx pool closed".to_string()))
        }
        sqlx::Error::PoolTimedOut => DbError::PoolError(PoolError::Timeout),
        sqlx::Error::Io(io) => DbError::IoError(io.to_string()),
        sqlx::Error::Tls(tls) => DbError::ConnectionError(tls.to_string()),
        sqlx::Error::Protocol(p) => DbError::ConnectionError(p.to_string()),
        sqlx::Error::RowNotFound => DbError::NotFound("row not found".to_string()),
        other => DbError::Internal(other.to_string()),
    }
}
