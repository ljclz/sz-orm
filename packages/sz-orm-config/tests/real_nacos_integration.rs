//! 真实 Nacos 集成测试
//!
//! 仅在 `--features real-nacos` 时编译，标注 `#[ignore]` 默认跳过。
//! 运行：`cargo test -p sz-orm-config --features real-nacos --test real_nacos_integration -- --ignored`
//!
//! 前置：Nacos 运行于 `http://127.0.0.1:8848`，用户名/密码 nacos/nacos

#![cfg(feature = "real-nacos")]

use sz_orm_config::nacos_client::*;

const NACOS_URL: &str = "http://127.0.0.1:8848";
const NACOS_USER: &str = "nacos";
const NACOS_PASS: &str = "nacos";

fn make_config() -> NacosConfig {
    NacosConfig::new(NACOS_URL).with_auth(NACOS_USER, NACOS_PASS)
}

async fn cleanup(data_id: &str, group: &str) {
    let config = make_config();
    if let Ok(client) = NacosClient::new(config) {
        let _ = client.delete_config(data_id, group).await;
    }
}

#[tokio::test]
#[ignore]
async fn test_nacos_login() {
    let config = make_config();
    let mut client = NacosClient::new(config).unwrap();
    client.login().await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_nacos_set_and_get_config() {
    let config = make_config();
    let client = NacosClient::new(config).unwrap();
    let data_id = "sz_orm_test_set_get";
    let group = "DEFAULT_GROUP";
    cleanup(data_id, group).await;

    client
        .set_config(data_id, group, "hello nacos")
        .await
        .unwrap();
    let value = client.get_config(data_id, group).await.unwrap();
    assert_eq!(value, "hello nacos");

    cleanup(data_id, group).await;
}

#[tokio::test]
#[ignore]
async fn test_nacos_delete_config() {
    let config = make_config();
    let client = NacosClient::new(config).unwrap();
    let data_id = "sz_orm_test_delete";
    let group = "DEFAULT_GROUP";
    cleanup(data_id, group).await;

    client
        .set_config(data_id, group, "to be deleted")
        .await
        .unwrap();
    assert!(client.get_config(data_id, group).await.is_ok());

    client.delete_config(data_id, group).await.unwrap();
    assert!(client.get_config(data_id, group).await.is_err());
}

#[tokio::test]
#[ignore]
async fn test_nacos_watch_config() {
    let config = make_config();
    let client = NacosClient::new(config).unwrap();
    let data_id = "sz_orm_test_watch";
    let group = "DEFAULT_GROUP";
    cleanup(data_id, group).await;

    client
        .set_config(data_id, group, "initial_value")
        .await
        .unwrap();
    let result = client.watch(data_id, group, 3000).await;
    let _ = result;

    cleanup(data_id, group).await;
}

#[tokio::test]
#[ignore]
async fn test_nacos_register_service() {
    let config = make_config();
    let client = NacosClient::new(config).unwrap();
    let service = NacosServiceRegistration {
        name: "sz-orm-test-service".to_string(),
        ip: "127.0.0.1".to_string(),
        port: 8080,
        weight: Some(1.0),
        metadata: None,
        cluster_name: None,
        group_name: Some("DEFAULT_GROUP".to_string()),
        enabled: Some(true),
        ephemeral: Some(true),
    };
    client.register_service(service).await.unwrap();
    client
        .deregister_service("sz-orm-test-service", "127.0.0.1", 8080)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore]
async fn test_nacos_not_found_error() {
    let config = make_config();
    let client = NacosClient::new(config).unwrap();
    let result = client
        .get_config("sz_orm_nonexistent_12345", "DEFAULT_GROUP")
        .await;
    assert!(result.is_err());
}

#[tokio::test]
#[ignore]
async fn test_nacos_namespace_isolation() {
    let config = make_config().with_namespace("sz_orm_test_ns");
    let client = NacosClient::new(config).unwrap();
    let data_id = "sz_orm_test_ns_key";
    let group = "DEFAULT_GROUP";

    client
        .set_config(data_id, group, "namespace_value")
        .await
        .unwrap();
    let value = client.get_config(data_id, group).await.unwrap();
    assert_eq!(value, "namespace_value");

    cleanup(data_id, group).await;
}

#[tokio::test]
#[ignore]
async fn test_nacos_large_config() {
    let config = make_config();
    let client = NacosClient::new(config).unwrap();
    let data_id = "sz_orm_test_large";
    let group = "DEFAULT_GROUP";
    cleanup(data_id, group).await;

    let large_content = "y".repeat(50000);
    client
        .set_config(data_id, group, &large_content)
        .await
        .unwrap();
    let retrieved = client.get_config(data_id, group).await.unwrap();
    assert_eq!(retrieved.len(), 50000);

    cleanup(data_id, group).await;
}
