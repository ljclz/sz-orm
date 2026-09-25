//! M3: server.rs 单元测试 — 覆盖 ServerConfig + WebGuiServer 未覆盖方法

use std::collections::HashMap;

use sz_orm_studio::{
    handlers::{StudioData, TableInfo},
    server::{ServerConfig, WebGuiServer},
};

#[test]
fn test_server_config_default() {
    let config = ServerConfig::default();
    assert_eq!(config.addr, "127.0.0.1");
    assert_eq!(config.port, 8080);
    assert_eq!(config.bind_addr(), "127.0.0.1:8080");
}

#[test]
fn test_server_config_new_with_str() {
    let config = ServerConfig::new("0.0.0.0", 3000);
    assert_eq!(config.addr, "0.0.0.0");
    assert_eq!(config.port, 3000);
    assert_eq!(config.bind_addr(), "0.0.0.0:3000");
}

#[test]
fn test_server_config_new_with_string() {
    let addr = String::from("192.168.1.1");
    let config = ServerConfig::new(addr, 65535);
    assert_eq!(config.bind_addr(), "192.168.1.1:65535");
}

#[test]
fn test_server_config_clone() {
    let config = ServerConfig::new("127.0.0.1", 9000);
    let cloned = config.clone();
    assert_eq!(config.bind_addr(), cloned.bind_addr());
}

#[test]
fn test_server_config_debug() {
    let config = ServerConfig::new("localhost", 80);
    let debug_str = format!("{:?}", config);
    assert!(debug_str.contains("localhost"));
    assert!(debug_str.contains("80"));
}

#[test]
fn test_webgui_server_new_empty() {
    let config = ServerConfig::new("127.0.0.1", 18100);
    let server = WebGuiServer::new(config);
    assert_eq!(server.config().bind_addr(), "127.0.0.1:18100");
}

#[test]
fn test_webgui_server_with_data() {
    let mut data = StudioData::default();
    data.tables.insert(
        "test_table".to_string(),
        TableInfo {
            name: "test_table".to_string(),
            columns: vec!["id".to_string()],
            row_count: 0,
        },
    );

    let config = ServerConfig::new("127.0.0.1", 18101);
    let server = WebGuiServer::with_data(config, data);
    assert_eq!(server.config().port, 18101);

    let store = server.data();
    let guard = store.read();
    assert_eq!(guard.tables.len(), 1);
    assert!(guard.tables.contains_key("test_table"));
}

#[test]
fn test_webgui_server_config_getter() {
    let config = ServerConfig::new("10.0.0.1", 18102);
    let server = WebGuiServer::new(config);
    assert_eq!(server.config().addr, "10.0.0.1");
    assert_eq!(server.config().port, 18102);
}

#[test]
fn test_webgui_server_data_getter_empty() {
    let config = ServerConfig::default();
    let server = WebGuiServer::new(config);
    let store = server.data();
    let guard = store.read();
    assert!(guard.tables.is_empty());
    assert!(guard.rows.is_empty());
    assert!(guard.relations.is_empty());
}

#[test]
fn test_webgui_server_data_getter_with_relations() {
    use sz_orm_studio::handlers::RelationInfo;

    let mut data = StudioData::default();
    data.relations.insert(
        "t1".to_string(),
        vec![RelationInfo {
            name: "r1".to_string(),
            from_table: "t1".to_string(),
            from_column: "id".to_string(),
            to_table: "t2".to_string(),
            to_column: "t1_id".to_string(),
        }],
    );

    let server = WebGuiServer::with_data(ServerConfig::default(), data);
    let store = server.data();
    let guard = store.read();
    assert_eq!(guard.relations.len(), 1);
    assert_eq!(guard.relations.get("t1").unwrap().len(), 1);
}

#[test]
fn test_webgui_server_router_from_empty() {
    let server = WebGuiServer::new(ServerConfig::default());
    let _router = server.router();
}

#[test]
fn test_webgui_server_router_from_populated() {
    let mut data = StudioData::default();
    data.tables.insert(
        "a".to_string(),
        TableInfo {
            name: "a".to_string(),
            columns: vec!["id".to_string()],
            row_count: 1,
        },
    );
    data.rows.insert(
        "a".to_string(),
        vec![sz_orm_studio::handlers::TableRow {
            id: "1".to_string(),
            data: HashMap::from([("id".to_string(), serde_json::json!(1))]),
        }],
    );

    let server = WebGuiServer::with_data(ServerConfig::default(), data);
    let _router = server.router();
}

#[test]
fn test_server_config_zero_port() {
    let config = ServerConfig::new("127.0.0.1", 0);
    assert_eq!(config.bind_addr(), "127.0.0.1:0");
}

#[test]
fn test_server_config_ipv6_addr() {
    let config = ServerConfig::new("::1", 8080);
    assert_eq!(config.bind_addr(), "::1:8080");
}
