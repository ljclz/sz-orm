//! # WASM 真实数据库连接模块
//!
//! 通过 HTTP/WebSocket 代理桥接后端 DB，支持鉴权、限流、SQL 白名单。

pub mod auth;
pub mod connection;
pub mod executor;
pub mod metrics;
pub mod protocol;
pub mod proxy;
pub mod rate_limiter;
pub mod reconnector;
pub mod sql_whitelist;

#[cfg(feature = "wasm-real-db")]
pub mod orm_session;
#[cfg(feature = "wasm-real-db")]
pub mod proxy_server;

#[cfg(feature = "wasi-socket")]
pub mod wasi_socket;

pub use auth::WasmDbAuthValidator;
pub use connection::{WasmRealDbConnection, WasmTransport};
pub use executor::WasmRealDbQueryExecutor;
pub use metrics::WasmRealDbMetrics;
pub use protocol::{
    ProxyError, ProxyRequest, ProxyResponse, ProxyStatus, SerializationFormat, WasmDbProxyProtocol,
};
pub use proxy::{DbCredentials, WasmDbProxy};
pub use rate_limiter::WasmDbRateLimiter;
pub use reconnector::WasmRealDbReconnector;
pub use sql_whitelist::WasmDbSqlWhitelist;

#[cfg(feature = "wasm-real-db")]
pub use orm_session::{
    WasmLoopReport, WasmOrmLoopVerifier, WasmOrmSession, WasmQueryBuilderBridge,
};
#[cfg(feature = "wasm-real-db")]
pub use proxy_server::{
    AuthConfig, DialectProxyConfig, MultiDialectProxyBackend, ProxyServerConfig, RateLimitConfig,
    WasmProxyServer, WhitelistConfig,
};

use thiserror::Error;

/// WASM 真实 DB 错误
#[derive(Debug, Clone, Error)]
pub enum WasmRealDbError {
    /// 代理不可用
    #[error("proxy unavailable")]
    ProxyUnavailable,

    /// SQL 被白名单拒绝
    #[error("SQL rejected: {reason}")]
    SqlRejected { reason: String },

    /// 限流
    #[error("rate limited")]
    RateLimited,

    /// 鉴权失败
    #[error("authentication failed")]
    AuthFailed,

    /// 后端凭据不暴露给 WASM 端
    #[error("credentials not exposed to WASM")]
    CredentialsNotExposed,

    /// 查询失败
    #[error("query failed: {reason}")]
    QueryFailed { reason: String },

    /// 结果集过大
    #[error("result too large")]
    ResultTooLarge,

    /// 序列化错误
    #[error("serialization error: {0}")]
    SerializationError(String),
}
