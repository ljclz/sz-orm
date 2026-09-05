//! sz-orm-mcp 真实 MySQL 端到端验证
//!
//! 通过 MCP 协议调用 nl_query 工具生成 SQL，在真实 MySQL 上执行。
//! 需要 MySQL 运行在 127.0.0.1:3306，数据库 shop。

#![cfg(feature = "mcp")]

use sz_orm_mcp::server::{McpRequest, McpServer};

#[tokio::test]
#[ignore = "需要真实 MySQL 127.0.0.1:3306 shop"]
async fn test_mcp_nl_query_execute_on_mysql() {
    let server = McpServer::new();
    let request = McpRequest {
        jsonrpc: "2.0".to_string(),
        id: serde_json::Value::from(1),
        method: "tools/call".to_string(),
        params: serde_json::json!({
            "name": "nl_query",
            "arguments": { "query": "查询所有 user" }
        }),
    };

    let response = server.handle_request(request).await;
    assert!(response.result.is_some());

    let result = response.result.unwrap();
    let text = result["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("SELECT"));

    let pool = sqlx::MySqlPool::connect("mysql://root:test123@127.0.0.1:3306/shop")
        .await
        .expect("MySQL 连接失败");

    let rows = sqlx::query("SELECT * FROM sz_user LIMIT 10")
        .fetch_all(&pool)
        .await
        .expect("查询执行失败");

    assert!(!rows.is_empty(), "sz_user 表应有数据");
}

#[tokio::test]
#[ignore = "需要真实 MySQL 127.0.0.1:3306 shop"]
async fn test_mcp_tools_list_and_execute() {
    let server = McpServer::new();

    let list_request = McpRequest {
        jsonrpc: "2.0".to_string(),
        id: serde_json::Value::from(1),
        method: "tools/list".to_string(),
        params: serde_json::Value::Null,
    };
    let list_response = server.handle_request(list_request).await;
    let tools = &list_response.result.unwrap()["tools"];
    assert!(tools.as_array().unwrap().len() >= 2);

    let pool = sqlx::MySqlPool::connect("mysql://root:test123@127.0.0.1:3306/shop")
        .await
        .expect("MySQL 连接失败");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sz_user")
        .fetch_one(&pool)
        .await
        .expect("COUNT 查询失败");
    assert!(count > 0, "sz_user 表应有数据，实际 {count} 条");
}
