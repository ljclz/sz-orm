//! MCP stdio 传输层
//!
//! 通过 JSON-RPC over stdio 实现 MCP 协议传输，
//! 使 sz-orm-mcp 可作为 Claude/Cursor 等 AI 客户端的 MCP server 运行。
//!
//! 用法：
//! ```text
//! echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | sz-orm-mcp-stdio
//! ```

use crate::server::{McpRequest, McpResponse, McpServer};
use std::io::{self, BufRead, Write};
use tokio::runtime::Runtime;

/// stdio 传输层：从 stdin 读取 JSON-RPC 请求，处理后写入 stdout
pub struct StdioTransport {
    server: McpServer,
}

impl StdioTransport {
    pub fn new(server: McpServer) -> Self {
        Self { server }
    }

    /// 运行 stdio 事件循环
    ///
    /// 从 stdin 逐行读取 JSON-RPC 请求，处理后逐行写入 stdout。
    /// 遇到 EOF 或读取错误时退出。
    pub fn run(&self) -> io::Result<()> {
        let rt = Runtime::new()?;
        let stdin = io::stdin();
        let stdout = io::stdout();
        let mut stdout_lock = stdout.lock();

        for line in stdin.lock().lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }

            let response = self.process_line(&rt, &line);
            let json = serde_json::to_string(&response).map_err(io::Error::other)?;
            writeln!(stdout_lock, "{json}")?;
            stdout_lock.flush()?;
        }

        Ok(())
    }

    pub fn process_line(&self, rt: &Runtime, line: &str) -> McpResponse {
        match serde_json::from_str::<McpRequest>(line) {
            Ok(request) => rt.block_on(self.server.handle_request(request)),
            Err(e) => McpResponse {
                jsonrpc: "2.0".to_string(),
                id: serde_json::Value::Null,
                result: None,
                error: Some(crate::server::McpError {
                    code: -32700,
                    message: format!("JSON 解析失败: {e}"),
                }),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::McpServer;

    #[test]
    fn test_stdio_transport_creation() {
        let server = McpServer::new();
        let transport = StdioTransport::new(server);
        let _ = transport;
    }

    #[test]
    fn test_process_line_valid() {
        let rt = Runtime::new().unwrap();
        let server = McpServer::new();
        let transport = StdioTransport::new(server);

        let line = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;
        let response = transport.process_line(&rt, line);

        assert!(response.result.is_some());
        assert!(response.error.is_none());
    }

    #[test]
    fn test_process_line_invalid_json() {
        let rt = Runtime::new().unwrap();
        let server = McpServer::new();
        let transport = StdioTransport::new(server);

        let line = "not valid json";
        let response = transport.process_line(&rt, line);

        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32700);
    }

    #[test]
    fn test_process_line_nl_query() {
        let rt = Runtime::new().unwrap();
        let server = McpServer::new();
        let transport = StdioTransport::new(server);

        let line = r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"nl_query","arguments":{"query":"查询用户"}}}"#;
        let response = transport.process_line(&rt, line);

        assert!(response.result.is_some());
        let result = response.result.unwrap();
        assert!(result["content"][0]["text"].as_str().unwrap().contains("SELECT"));
    }
}