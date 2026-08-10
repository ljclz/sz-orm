//! 真实 Consul 集成测试
//!
//! 仅在 `--features real-consul` 时编译，标注 `#[ignore]` 默认跳过。
//! 运行：`cargo test -p sz-orm-config --features real-consul --test real_consul_integration -- --ignored`
//!
//! 前置：Consul 运行于 `http://127.0.0.1:8500`

#![cfg(feature = "real-consul")]

use sz_orm_config::consul_client::*;

const CONSUL_URL: &str = "http://127.0.0.1:8500";

fn make_client() -> ConsulClient {
    let config = ConsulConfig::new(CONSUL_URL);
    ConsulClient::new(config).unwrap()
}

async fn cleanup(key: &str) {
    let client = make_client();
    let _ = client.delete_config(key).await;
}

#[tokio::test]
#[ignore]
async fn test_consul_set_and_get_config() {
    let client = make_client();
    let key = "sz_orm_test/set_get";
    cleanup(key).await;

    client.set_config(key, "hello world").await.unwrap();
    let value = client.get_config(key).await.unwrap();
    assert_eq!(value, "hello world");

    cleanup(key).await;
}

#[tokio::test]
#[ignore]
async fn test_consul_delete_config() {
    let client = make_client();
    let key = "sz_orm_test/delete";
    cleanup(key).await;

    client.set_config(key, "to be deleted").await.unwrap();
    assert!(client.get_config(key).await.is_ok());

    client.delete_config(key).await.unwrap();
    assert!(client.get_config(key).await.is_err());
}

#[tokio::test]
#[ignore]
async fn test_consul_list_keys() {
    let client = make_client();
    let prefix = "sz_orm_test_list/";
    let keys = vec![
        format!("{}key1", prefix),
        format!("{}key2", prefix),
        format!("{}key3", prefix),
    ];
    for k in &keys {
        cleanup(k).await;
    }
    for k in &keys {
        client.set_config(k, "value").await.unwrap();
    }

    let listed = client.list_keys(prefix).await.unwrap();
    for k in &keys {
        assert!(listed.contains(k), "missing key: {}", k);
    }

    for k in &keys {
        cleanup(k).await;
    }
}

#[tokio::test]
#[ignore]
async fn test_consul_watch_config_change() {
    let client = make_client();
    let key = "sz_orm_test/watch";
    cleanup(key).await;

    client.set_config(key, "initial").await.unwrap();
    let (value, _index) = client.watch(key, 0, 1).await.unwrap();
    assert_eq!(value, "initial");

    client.set_config(key, "updated").await.unwrap();
    let (value2, _) = client.watch(key, 0, 1).await.unwrap();
    assert_eq!(value2, "updated");

    cleanup(key).await;
}

#[tokio::test]
#[ignore]
async fn test_consul_register_service() {
    let client = make_client();
    let service = ConsulServiceRegistration {
        id: Some("sz-orm-test-svc".to_string()),
        name: "sz-orm-test-service".to_string(),
        address: Some("127.0.0.1".to_string()),
        port: Some(8080),
        tags: Some(vec!["test".to_string()]),
        meta: None,
    };
    client.register_service(service).await.unwrap();
    client
        .deregister_service("sz-orm-test-svc", "sz-orm-auto")
        .await
        .unwrap();
}

#[tokio::test]
#[ignore]
async fn test_consul_acl_token_auth() {
    let config = ConsulConfig::new(CONSUL_URL).with_acl_token("test-token");
    let client = ConsulClient::new(config).unwrap();
    let result = client.get_config("sz_orm_test/acl").await;
    let _ = result;
}

#[tokio::test]
#[ignore]
async fn test_consul_not_found_error() {
    let client = make_client();
    let result = client.get_config("sz_orm_test/nonexistent_key_12345").await;
    assert!(result.is_err());
    match result.unwrap_err() {
        ConsulError::NotFound(_) => {}
        other => panic!("expected NotFound, got: {:?}", other),
    }
}

#[tokio::test]
#[ignore]
async fn test_consul_large_value() {
    let client = make_client();
    let key = "sz_orm_test/large";
    cleanup(key).await;

    let large_value = "x".repeat(10000);
    client.set_config(key, &large_value).await.unwrap();
    let retrieved = client.get_config(key).await.unwrap();
    assert_eq!(retrieved.len(), 10000);

    cleanup(key).await;
}
