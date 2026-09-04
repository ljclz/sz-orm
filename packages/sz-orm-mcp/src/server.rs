//! MCP Server 实现
//!
//! 通过 JSON-RPC over stdio 实现 MCP 协议，
//! 暴露 `nl_query` 和 `execute_sql` 两个工具。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sz_orm_nl_query::pipeline::{NlQueryPipeline, SqlExecutor};
use std::sync::Arc;

/// MCP 工具定义
#[derive(Debug, Clone, Serialize)]
pub struct McpTool {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

/// MCP 请求
#[derive(Debug, Deserialize)]
pub struct McpRequest {
    pub jsonrpc: String,
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

/// MCP 响应
#[derive(Debug, Serialize)]
pub struct McpResponse {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpError>,
}

#[derive(Debug, Serialize)]
pub struct McpError {
    pub code: i32,
    pub message: String,
}

/// MCP Server
pub struct McpServer {
    pipeline: NlQueryPipeline,
}

impl McpServer {
    pub fn new() -> Self {
        Self {
            pipeline: NlQueryPipeline::new(),
        }
    }

    pub fn with_executor(executor: Arc<dyn SqlExecutor>) -> Self {
        Self {
            pipeline: NlQueryPipeline::new().with_executor(executor),
        }
    }

    /// 列出可用工具
    pub fn list_tools() -> Vec<McpTool> {
        vec![
            McpTool {
                name: "nl_query",
                description: "自然语言查询数据库：NL → SQL → 执行 → 返回结果",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "自然语言查询" }
                    },
                    "required": ["query"]
                }),
            },
            McpTool {
                name: "execute_sql",
                description: "直接执行 SQL 语句（需注入执行器）",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "sql": { "type": "string", "description": "SQL 语句" }
                    },
                    "required": ["sql"]
                }),
            },
        ]
    }

    /// 处理 MCP 请求
    pub async fn handle_request(&self, request: McpRequest) -> McpResponse {
        match request.method.as_str() {
            "tools/list" => McpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: Some(serde_json::json!({
                    "tools": Self::list_tools()
                })),
                error: None,
            },
            "tools/call" => self.handle_tool_call(request).await,
            _ => McpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(McpError {
                    code: -32601,
                    message: format!("未知方法: {}", request.method),
                }),
            },
        }
    }

    async fn handle_tool_call(&self, request: McpRequest) -> McpResponse {
        let tool_name = request
            .params
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let args = request.params.get("arguments").cloned().unwrap_or(Value::Null);

        match tool_name {
            "nl_query" => {
                let query = args
                    .get("query")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                match self.pipeline.query(query).await {
                    Ok(resp) => McpResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: Some(serde_json::json!({
                            "content": [{
                                "type": "text",
                                "text": format!("SQL: {}\n行数: {}\n解释: {}",
                                    resp.sql,
                                    resp.rows.as_array().map(|a| a.len()).unwrap_or(0),
                                    resp.sql_explanation)
                            }]
                        })),
                        error: None,
                    },
                    Err(e) => McpResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: None,
                        error: Some(McpError {
                            code: -32000,
                            message: format!("查询失败: {:?}", e),
                        }),
                    },
                }
            }
            "execute_sql" => {
                let sql = args.get("sql").and_then(|v| v.as_str()).unwrap_or("");
                match self.pipeline.execute_sql(sql).await {
                    Ok(resp) => McpResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: Some(serde_json::json!({
                            "content": [{
                                "type": "text",
                                "text": format!("SQL: {}\n行数: {}",
                                    resp.sql,
                                    resp.rows.as_array().map(|a| a.len()).unwrap_or(0))
                            }]
                        })),
                        error: None,
                    },
                    Err(e) => McpResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: None,
                        error: Some(McpError {
                            code: -32000,
                            message: format!("执行失败: {:?}", e),
                        }),
                    },
                }
            }
            _ => McpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(McpError {
                    code: -32602,
                    message: format!("未知工具: {}", tool_name),
                }),
            },
        }
    }
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_list_tools() {
        let tools = McpServer::list_tools();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "nl_query");
        assert_eq!(tools[1].name, "execute_sql");
    }

    #[tokio::test]
    async fn test_handle_tools_list() {
        let server = McpServer::new();
        let request = McpRequest {
            jsonrpc: "2.0".to_string(),
            id: Value::from(1),
            method: "tools/list".to_string(),
            params: Value::Null,
        };

        let response = server.handle_request(request).await;
        assert!(response.result.is_some());
        assert!(response.error.is_none());
    }

    #[tokio::test]
    async fn test_handle_nl_query() {
        let server = McpServer::new();
        let request = McpRequest {
            jsonrpc: "2.0".to_string(),
            id: Value::from(2),
            method: "tools/call".to_string(),
            params: serde_json::json!({
                "name": "nl_query",
                "arguments": { "query": "查询所有用户" }
            }),
        };

        let response = server.handle_request(request).await;
        assert!(response.result.is_some());
        let result = response.result.unwrap();
        assert!(result["content"][0]["text"].as_str().unwrap().contains("SELECT"));
    }

    #[tokio::test]
    async fn test_unknown_method() {
        let server = McpServer::new();
        let request = McpRequest {
            jsonrpc: "2.0".to_string(),
            id: Value::from(3),
            method: "unknown".to_string(),
            params: Value::Null,
        };

        let response = server.handle_request(request).await;
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32601);
    }

    #[tokio::test]
    async fn test_unknown_tool() {
        let server = McpServer::new();
        let request = McpRequest {
            jsonrpc: "2.0".to_string(),
            id: Value::from(4),
            method: "tools/call".to_string(),
            params: serde_json::json!({
                "name": "nonexistent",
                "arguments": {}
            }),
        };

        let response = server.handle_request(request).await;
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32602);
    }
}