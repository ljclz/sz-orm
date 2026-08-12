//! # sz-orm-stream — 异步流式结果集
//!
//! 基于 `stream-resultset` feature，支持大结果集流式返回，避免一次性加载到内存。
//! v4.5.0 M3 将实现 StreamResultSet + KeysetPaginator + 背压控制。