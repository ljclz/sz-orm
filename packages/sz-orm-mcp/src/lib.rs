//! SZ-ORM MCP Server
//!
//! 实现 Model Context Protocol (MCP) 服务器，将 sz-orm 的 NL 查询和 SQL 执行
//! 暴露为 AI 工具，可被 Claude/Cursor 等 AI 客户端调用。
//!
//! 启用 `mcp` feature gate 后可用。

#[cfg(feature = "mcp")]
pub mod server;

#[cfg(feature = "mcp")]
pub use server::McpServer;