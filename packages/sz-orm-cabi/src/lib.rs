//! # SZ-ORM C ABI — 跨语言 FFI 导出层
//!
//! 为 Go/Java/C++ 提供统一的 C ABI 接口，暴露 sz-orm-core 的
//! Model/QueryBuilder/Pool/Transaction 核心 API。
//!
//! ## 安全保证
//!
//! - FFI 内存由 Rust 侧分配/释放，语言侧仅持有句柄
//! - panic 捕获转换为错误码，不跨语言边界 UB
//! - unsafe 块均有 `// SAFETY:` 注释

pub mod ffi_memory;
pub mod panic_guard;

use std::ffi::c_void;

/// 连接池句柄
pub type SzOrmPoolHandle = *mut c_void;

/// 查询构建器句柄
pub type SzOrmQueryBuilderHandle = *mut c_void;

/// 事务句柄
pub type SzOrmTransactionHandle = *mut c_void;

/// 模型句柄
pub type SzOrmModelHandle = *mut c_void;

/// 错误码
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SzOrmErrorCode {
    Ok = 0,
    NotFound = 1,
    ConnectionFailed = 2,
    QueryFailed = 3,
    PoolExhausted = 4,
    TransactionAborted = 5,
    Panic = 6,
    InvalidArgument = 7,
    RuntimeNotInitialized = 8,
    MemoryLeak = 9,
}

impl SzOrmErrorCode {
    pub fn as_i32(self) -> i32 {
        self as i32
    }

    pub fn from_i32(code: i32) -> Self {
        match code {
            0 => SzOrmErrorCode::Ok,
            1 => SzOrmErrorCode::NotFound,
            2 => SzOrmErrorCode::ConnectionFailed,
            3 => SzOrmErrorCode::QueryFailed,
            4 => SzOrmErrorCode::PoolExhausted,
            5 => SzOrmErrorCode::TransactionAborted,
            6 => SzOrmErrorCode::Panic,
            7 => SzOrmErrorCode::InvalidArgument,
            8 => SzOrmErrorCode::RuntimeNotInitialized,
            9 => SzOrmErrorCode::MemoryLeak,
            _ => SzOrmErrorCode::Panic,
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            SzOrmErrorCode::Ok => "success",
            SzOrmErrorCode::NotFound => "resource not found",
            SzOrmErrorCode::ConnectionFailed => "connection failed",
            SzOrmErrorCode::QueryFailed => "query execution failed",
            SzOrmErrorCode::PoolExhausted => "connection pool exhausted",
            SzOrmErrorCode::TransactionAborted => "transaction aborted",
            SzOrmErrorCode::Panic => "internal panic",
            SzOrmErrorCode::InvalidArgument => "invalid argument",
            SzOrmErrorCode::RuntimeNotInitialized => "tokio runtime not initialized",
            SzOrmErrorCode::MemoryLeak => "memory leak detected",
        }
    }
}

/// C ABI 兼容的连接池配置
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PoolConfigC {
    pub max_connections: u32,
    pub min_connections: u32,
    pub connect_timeout_ms: u64,
    pub idle_timeout_ms: u64,
}

impl Default for PoolConfigC {
    fn default() -> Self {
        Self {
            max_connections: 10,
            min_connections: 1,
            connect_timeout_ms: 5000,
            idle_timeout_ms: 300000,
        }
    }
}

/// C ABI 兼容的查询结果
#[repr(C)]
pub struct QueryResultC {
    pub success: i32,
    pub error_code: i32,
    pub rows_affected: u64,
    pub last_insert_id: u64,
}

impl Default for QueryResultC {
    fn default() -> Self {
        Self {
            success: 1,
            error_code: SzOrmErrorCode::Ok.as_i32(),
            rows_affected: 0,
            last_insert_id: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_code_roundtrip() {
        for code in 0..=9 {
            let ec = SzOrmErrorCode::from_i32(code);
            assert_eq!(ec.as_i32(), code);
        }
    }

    #[test]
    fn test_error_code_unknown_defaults_to_panic() {
        let ec = SzOrmErrorCode::from_i32(999);
        assert_eq!(ec, SzOrmErrorCode::Panic);
    }

    #[test]
    fn test_error_code_description() {
        assert_eq!(SzOrmErrorCode::Ok.description(), "success");
        assert_eq!(SzOrmErrorCode::Panic.description(), "internal panic");
        assert_eq!(
            SzOrmErrorCode::ConnectionFailed.description(),
            "connection failed"
        );
    }

    #[test]
    fn test_pool_config_default() {
        let config = PoolConfigC::default();
        assert_eq!(config.max_connections, 10);
        assert_eq!(config.min_connections, 1);
        assert_eq!(config.connect_timeout_ms, 5000);
        assert_eq!(config.idle_timeout_ms, 300000);
    }

    #[test]
    fn test_query_result_default() {
        let result = QueryResultC::default();
        assert_eq!(result.success, 1);
        assert_eq!(result.error_code, 0);
        assert_eq!(result.rows_affected, 0);
    }
}
