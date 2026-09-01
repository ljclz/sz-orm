//! TASK-025: sz-orm-studio E2E 测试
//!
//! 启动 WebGuiServer → HTTP 请求验证表数据浏览/编辑/关系导航。

use std::collections::HashMap;

use sz_orm_studio::{
    handlers::{RelationInfo, StudioData, TableInfo, TableRow},
    server::{ServerConfig, WebGuiServer},
};

fn make_test_data() -> StudioData {
    let mut data = StudioData::default();

    let users_table = TableInfo {
        name: "users".to_string(),
        columns: vec!["id".to_string(), "name".to_string(), "email".to_string()],
        row_count: 2,
    };
    let orders_table = TableInfo {
        name: "orders".to_string(),
        columns: vec![
            "id".to_string(),
            "user_id".to_string(),
            "amount".to_string(),
        ],
        row_count: 1,
    };

    data.tables.insert("users".to_string(), users_table);
    data.tables.insert("orders".to_string(), orders_table);

    let mut user1 = HashMap::new();
    user1.insert("id".to_string(), serde_json::json!(1));
    user1.insert("name".to_string(), serde_json::json!("Alice"));
    user1.insert("email".to_string(), serde_json::json!("alice@example.com"));

    let mut user2 = HashMap::new();
    user2.insert("id".to_string(), serde_json::json!(2));
    user2.insert("name".to_string(), serde_json::json!("Bob"));
    user2.insert("email".to_string(), serde_json::json!("bob@example.com"));

    data.rows.insert(
        "users".to_string(),
        vec![
            TableRow {
                id: "1".to_string(),
                data: user1,
            },
            TableRow {
                id: "2".to_string(),
                data: user2,
            },
        ],
    );

    let mut order1 = HashMap::new();
    order1.insert("id".to_string(), serde_json::json!(101));
    order1.insert("user_id".to_string(), serde_json::json!(1));
    order1.insert("amount".to_string(), serde_json::json!(99.99));

    data.rows.insert(
        "orders".to_string(),
        vec![TableRow {
            id: "101".to_string(),
            data: order1,
        }],
    );

    data.relations.insert(
        "users".to_string(),
        vec![RelationInfo {
            name: "user_orders".to_string(),
            from_table: "users".to_string(),
            from_column: "id".to_string(),
            to_table: "orders".to_string(),
            to_column: "user_id".to_string(),
        }],
    );

    data
}

async fn start_server(port: u16) -> WebGuiServer {
    let config = ServerConfig::new("127.0.0.1", port);
    let server = WebGuiServer::with_data(config, make_test_data());

    let server_clone =
        WebGuiServer::with_data(ServerConfig::new("127.0.0.1", port), make_test_data());
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port))
        .await
        .unwrap();
    let router = server_clone.router();
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });

    server
}

#[tokio::test]
async fn test_get_tables() {
    let _server = start_server(18001).await;
    let client = reqwest::Client::new();

    let resp = client
        .get("http://127.0.0.1:18001/tables")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let tables: Vec<TableInfo> = resp.json().await.unwrap();
    assert!(tables.len() >= 2);
    assert!(tables.iter().any(|t| t.name == "users"));
    assert!(tables.iter().any(|t| t.name == "orders"));
}

#[tokio::test]
async fn test_get_table_data() {
    let _server = start_server(18002).await;
    let client = reqwest::Client::new();

    let resp = client
        .get("http://127.0.0.1:18002/tables/users/data")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let rows: Vec<TableRow> = resp.json().await.unwrap();
    assert_eq!(rows.len(), 2);
}

#[tokio::test]
async fn test_get_table_data_with_filter() {
    let _server = start_server(18003).await;
    let client = reqwest::Client::new();

    let resp = client
        .get("http://127.0.0.1:18003/tables/users/data?column=name&value=Alice")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let rows: Vec<TableRow> = resp.json().await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "1");
}

#[tokio::test]
async fn test_get_table_data_with_limit() {
    let _server = start_server(18004).await;
    let client = reqwest::Client::new();

    let resp = client
        .get("http://127.0.0.1:18004/tables/users/data?limit=1")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let rows: Vec<TableRow> = resp.json().await.unwrap();
    assert_eq!(rows.len(), 1);
}

#[tokio::test]
async fn test_edit_record() {
    let _server = start_server(18005).await;
    let client = reqwest::Client::new();

    let mut edit_data = HashMap::new();
    edit_data.insert("name".to_string(), serde_json::json!("AliceUpdated"));

    let resp = client
        .put("http://127.0.0.1:18005/tables/users/data/1")
        .json(&serde_json::json!({ "data": edit_data }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let row: TableRow = resp.json().await.unwrap();
    assert_eq!(
        row.data.get("name").unwrap(),
        &serde_json::json!("AliceUpdated")
    );
}

#[tokio::test]
async fn test_get_table_relations() {
    let _server = start_server(18006).await;
    let client = reqwest::Client::new();

    let resp = client
        .get("http://127.0.0.1:18006/tables/users/relations")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let relations: Vec<RelationInfo> = resp.json().await.unwrap();
    assert_eq!(relations.len(), 1);
    assert_eq!(relations[0].to_table, "orders");
}

#[tokio::test]
async fn test_get_nonexistent_table() {
    let _server = start_server(18007).await;
    let client = reqwest::Client::new();

    let resp = client
        .get("http://127.0.0.1:18007/tables/nonexistent/data")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_server_config() {
    let config = ServerConfig::new("127.0.0.1", 18008);
    assert_eq!(config.bind_addr(), "127.0.0.1:18008");
}

#[tokio::test]
async fn test_server_router() {
    let config = ServerConfig::new("127.0.0.1", 18009);
    let server = WebGuiServer::with_data(config, make_test_data());
    let _router = server.router();
}
