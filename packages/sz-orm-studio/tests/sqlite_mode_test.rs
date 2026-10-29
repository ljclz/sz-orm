//! 集成测试：sz-orm-studio 真实 SQLite 数据源模式（v10.13.0 增强）
//!
//! 验证 StudioData::with_sqlite 后，get_tables/get_table_data/edit_record
//! 从真实 SQLite 数据库读写，而非内存 HashMap。

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::Json;
use parking_lot::RwLock;
use sz_orm_studio::handlers::{
    edit_record, get_table_data, get_tables, EditRequest, FilterParams, StudioData,
};
use sz_orm_studio::DataStore;

fn make_sqlite_store() -> DataStore {
    let conn = rusqlite::Connection::open_in_memory().expect("内存 SQLite 打开失败");
    conn.execute(
        "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL, email TEXT)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO users (id, name, email) VALUES (1, 'Alice', 'alice@example.com'), (2, 'Bob', 'bob@example.com')",
        [],
    )
    .unwrap();
    Arc::new(RwLock::new(StudioData::with_sqlite(conn)))
}

#[tokio::test]
async fn test_sqlite_get_tables() {
    let store = make_sqlite_store();
    let Json(tables) = get_tables(State(store)).await;
    assert!(tables.iter().any(|t| t.name == "users"));
    let users = tables.iter().find(|t| t.name == "users").unwrap();
    assert!(users.columns.contains(&"id".to_string()));
    assert!(users.columns.contains(&"name".to_string()));
    assert_eq!(users.row_count, 2);
}

#[tokio::test]
async fn test_sqlite_get_table_data() {
    let store = make_sqlite_store();
    let Json(rows) = get_table_data(
        State(store),
        Path("users".to_string()),
        Query(FilterParams {
            column: None,
            value: None,
            limit: None,
        }),
    )
    .await
    .expect("读取表数据应成功");
    assert_eq!(rows.len(), 2);
    assert!(rows
        .iter()
        .any(|r| r.data.get("name").map(|v| v == "Alice").unwrap_or(false)));
    assert!(rows.iter().any(|r| r
        .data
        .get("email")
        .map(|v| v == "bob@example.com")
        .unwrap_or(false)));
}

#[tokio::test]
async fn test_sqlite_get_table_data_not_found() {
    let store = make_sqlite_store();
    let res = get_table_data(
        State(store),
        Path("missing".to_string()),
        Query(FilterParams {
            column: None,
            value: None,
            limit: None,
        }),
    )
    .await;
    assert!(res.is_err(), "不存在的表应返回错误");
}

#[tokio::test]
async fn test_sqlite_edit_record() {
    let store = make_sqlite_store();
    let mut data = std::collections::HashMap::new();
    data.insert("name".to_string(), serde_json::json!("Carol"));
    let edit = EditRequest { data };
    let Json(row) = edit_record(
        State(store.clone()),
        Path(("users".to_string(), "1".to_string())),
        Json(edit),
    )
    .await
    .expect("编辑应成功");
    assert_eq!(
        row.data.get("name").map(|v| v.as_str().unwrap_or("")),
        Some("Carol")
    );

    // 验证真实写入数据库
    let store2 = store.read();
    let conn = store2.sqlite.as_ref().unwrap().lock().unwrap();
    let name: String = conn
        .query_row("SELECT name FROM users WHERE id=1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(name, "Carol");
}
