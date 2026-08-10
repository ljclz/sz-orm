//! sz-orm-config Mock 与真实 Consul/Nacos 差分测试（M3-T4.2）
//!
//! 验证 `ConsulConfigCenter`/`NacosConfigCenter`（内存 Mock）与
//! `RealConsulConfigCenter`/`RealNacosConfigCenter`（真实 Consul/Nacos）在相同输入下输出语义一致。
//!
//! - 不带 `#[ignore]` 的测试：验证 Mock 行为符合预期语义（始终运行）
//! - 带 `#[ignore]` 的测试：同时运行 Mock 和真实 Consul/Nacos，对比结果
//!
//! 运行方式：`cargo test -p sz-orm-config --features real-consul,real-nacos --test config_diff_test -- --ignored`
//!
//! 前置条件：
//! - Consul 运行于 `http://127.0.0.1:8500`
//! - Nacos 运行于 `http://127.0.0.1:8848`，用户名/密码 nacos/nacos

use std::sync::{Arc, Mutex};
use sz_orm_config::*;

// ============================================================================
// Mock 行为验证（始终运行，不依赖真实 Consul/Nacos）
// ============================================================================

#[test]
fn diff_mock_consul_get_set_consistency() {
    let mut cc = ConsulConfigCenter::new();
    cc.set("diff_key1", "value1");
    assert_eq!(cc.get("diff_key1"), Some("value1".to_string()));
    assert!(cc.exists("diff_key1"));
    assert_eq!(cc.get("diff_nonexistent"), None);
}

#[test]
fn diff_mock_consul_overwrite() {
    let mut cc = ConsulConfigCenter::new();
    cc.set("diff_key", "v1");
    cc.set("diff_key", "v2");
    assert_eq!(cc.get("diff_key"), Some("v2".to_string()));
}

#[test]
fn diff_mock_consul_delete() {
    let mut cc = ConsulConfigCenter::new();
    cc.set("diff_key", "value");
    assert!(cc.delete("diff_key"));
    assert!(!cc.exists("diff_key"));
    assert!(!cc.delete("diff_nonexistent"));
}

#[test]
fn diff_mock_consul_list_sorted() {
    let mut cc = ConsulConfigCenter::new();
    cc.set("diff_z", "1");
    cc.set("diff_a", "2");
    cc.set("diff_m", "3");
    assert_eq!(cc.list(), vec!["diff_a", "diff_m", "diff_z"]);
}

#[test]
fn diff_mock_consul_subscribe_notify() {
    let mut cc = ConsulConfigCenter::new();
    let received = Arc::new(Mutex::new(Vec::new()));
    let received_clone = received.clone();
    cc.subscribe(
        "diff_key",
        Arc::new(move |key, value| {
            received_clone
                .lock()
                .unwrap()
                .push((key.to_string(), value.to_string()));
        }),
    );
    cc.set("diff_key", "v1");
    cc.set("diff_key", "v2");
    let events = received.lock().unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0], ("diff_key".to_string(), "v1".to_string()));
    assert_eq!(events[1], ("diff_key".to_string(), "v2".to_string()));
}

#[test]
fn diff_mock_nacos_get_set_consistency() {
    let mut nc = NacosConfigCenter::new();
    nc.set("diff_nacos_key", "value");
    assert_eq!(nc.get("diff_nacos_key"), Some("value".to_string()));
    assert!(nc.exists("diff_nacos_key"));
    assert!(nc.delete("diff_nacos_key"));
    assert!(!nc.exists("diff_nacos_key"));
}

#[test]
fn diff_mock_nacos_subscribe_notify() {
    let mut nc = NacosConfigCenter::new();
    let count = Arc::new(Mutex::new(0));
    let count_clone = count.clone();
    nc.subscribe(
        "diff_nacos_key",
        Arc::new(move |_, _| {
            *count_clone.lock().unwrap() += 1;
        }),
    );
    nc.set("diff_nacos_key", "v1");
    nc.set("diff_nacos_key", "v2");
    assert_eq!(*count.lock().unwrap(), 2);
}

// ============================================================================
// Mock vs 真实 Consul 差分对比（需真实 Consul，标注 #[ignore]）
// ============================================================================

#[cfg(feature = "real-consul")]
mod real_consul_diff {
    use super::*;
    use sz_orm_config::consul_client::*;

    const CONSUL_URL: &str = "http://127.0.0.1:8500";

    async fn consul_available() -> bool {
        let client = ConsulClient::new(ConsulConfig::new(CONSUL_URL)).unwrap();
        // 尝试 list_keys，如果成功则 Consul 可用
        client.list_keys("sz_orm_diff_health_check").await.is_ok()
    }

    async fn cleanup(key: &str) {
        let client = ConsulClient::new(ConsulConfig::new(CONSUL_URL)).unwrap();
        let _ = client.delete_config(key).await;
    }

    /// 差分测试：get/set 语义一致
    #[tokio::test]
    #[ignore]
    async fn diff_real_consul_get_set() {
        if !consul_available().await {
            eprintln!("Consul not available, skipping");
            return;
        }
        let key = "sz_orm_diff_consul/get_set";
        cleanup(key).await;

        // Mock 行为
        let mut mock = ConsulConfigCenter::new();
        mock.set(key, "hello world");
        let mock_value = mock.get(key).unwrap();

        // 真实 Consul 行为
        let client = ConsulClient::new(ConsulConfig::new(CONSUL_URL)).unwrap();
        client.set_config(key, "hello world").await.unwrap();
        let real_value = client.get_config(key).await.unwrap();

        assert_eq!(mock_value, real_value);
        assert_eq!(mock_value, "hello world");

        cleanup(key).await;
    }

    /// 差分测试：delete 语义一致
    #[tokio::test]
    #[ignore]
    async fn diff_real_consul_delete() {
        if !consul_available().await {
            eprintln!("Consul not available, skipping");
            return;
        }
        let key = "sz_orm_diff_consul/delete";
        cleanup(key).await;

        // Mock 行为
        let mut mock = ConsulConfigCenter::new();
        mock.set(key, "to be deleted");
        assert!(mock.delete(key));
        assert!(!mock.exists(key));

        // 真实 Consul 行为
        let client = ConsulClient::new(ConsulConfig::new(CONSUL_URL)).unwrap();
        client.set_config(key, "to be deleted").await.unwrap();
        assert!(client.get_config(key).await.is_ok());
        client.delete_config(key).await.unwrap();
        assert!(client.get_config(key).await.is_err());

        cleanup(key).await;
    }

    /// 差分测试：overwrite 语义一致
    #[tokio::test]
    #[ignore]
    async fn diff_real_consul_overwrite() {
        if !consul_available().await {
            eprintln!("Consul not available, skipping");
            return;
        }
        let key = "sz_orm_diff_consul/overwrite";
        cleanup(key).await;

        // Mock 行为
        let mut mock = ConsulConfigCenter::new();
        mock.set(key, "v1");
        mock.set(key, "v2");
        let mock_value = mock.get(key).unwrap();

        // 真实 Consul 行为
        let client = ConsulClient::new(ConsulConfig::new(CONSUL_URL)).unwrap();
        client.set_config(key, "v1").await.unwrap();
        client.set_config(key, "v2").await.unwrap();
        let real_value = client.get_config(key).await.unwrap();

        assert_eq!(mock_value, real_value);
        assert_eq!(mock_value, "v2");

        cleanup(key).await;
    }

    /// 差分测试：list keys 语义一致
    #[tokio::test]
    #[ignore]
    async fn diff_real_consul_list() {
        if !consul_available().await {
            eprintln!("Consul not available, skipping");
            return;
        }
        let prefix = "sz_orm_diff_consul_list/";
        let keys = vec![
            format!("{}key1", prefix),
            format!("{}key2", prefix),
            format!("{}key3", prefix),
        ];
        for k in &keys {
            cleanup(k).await;
        }

        // Mock 行为
        let mut mock = ConsulConfigCenter::new();
        for k in &keys {
            mock.set(k, "value");
        }
        let mock_list = mock.list();
        let mock_count = mock_list.iter().filter(|k| k.starts_with(prefix)).count();

        // 真实 Consul 行为
        let client = ConsulClient::new(ConsulConfig::new(CONSUL_URL)).unwrap();
        for k in &keys {
            client.set_config(k, "value").await.unwrap();
        }
        let real_list = client.list_keys(prefix).await.unwrap();

        assert_eq!(mock_count, real_list.len());
        assert_eq!(real_list.len(), 3);

        for k in &keys {
            cleanup(k).await;
        }
    }

    /// 差分测试：RealConsulConfigCenter 实现 ConfigCenter trait
    #[tokio::test]
    #[ignore]
    async fn diff_real_consul_config_center_trait() {
        if !consul_available().await {
            eprintln!("Consul not available, skipping");
            return;
        }
        let key = "sz_orm_diff_consul/trait";
        cleanup(key).await;

        // 使用 RealConsulConfigCenter（实现 ConfigCenter trait）
        let mut real = RealConsulConfigCenter::new(ConsulConfig::new(CONSUL_URL)).unwrap();
        real.set(key, "trait_value");
        assert!(real.exists(key));
        assert_eq!(real.get(key), Some("trait_value".to_string()));
        assert!(real.delete(key));
        assert!(!real.exists(key));

        cleanup(key).await;
    }

    /// 差分测试：服务注册/发现语义一致
    #[tokio::test]
    #[ignore]
    async fn diff_real_consul_service_discovery() {
        if !consul_available().await {
            eprintln!("Consul not available, skipping");
            return;
        }

        let client = ConsulClient::new(ConsulConfig::new(CONSUL_URL)).unwrap();
        let service = ConsulServiceRegistration {
            id: Some("sz-orm-diff-svc".to_string()),
            name: "sz-orm-diff-service".to_string(),
            address: Some("127.0.0.1".to_string()),
            port: Some(8080),
            tags: Some(vec!["diff-test".to_string()]),
            meta: None,
        };

        // 注册服务
        client.register_service(service).await.unwrap();

        // 发现服务
        let instances = client
            .discover_service("sz-orm-diff-service")
            .await
            .unwrap();
        assert!(!instances.is_empty());
        let found = instances
            .iter()
            .any(|i| i.service_name == "sz-orm-diff-service");
        assert!(found);

        // 注销服务
        client
            .deregister_service("sz-orm-diff-svc", "sz-orm-auto")
            .await
            .unwrap();
    }
}

// ============================================================================
// Mock vs 真实 Nacos 差分对比（需真实 Nacos，标注 #[ignore]）
// ============================================================================

#[cfg(feature = "real-nacos")]
mod real_nacos_diff {
    use super::*;
    use sz_orm_config::nacos_client::*;

    const NACOS_URL: &str = "http://127.0.0.1:8848";
    const NACOS_USER: &str = "nacos";
    const NACOS_PASS: &str = "nacos";

    fn make_config() -> NacosConfig {
        NacosConfig::new(NACOS_URL).with_auth(NACOS_USER, NACOS_PASS)
    }

    async fn nacos_available() -> bool {
        let config = make_config();
        if let Ok(mut client) = NacosClient::new(config) {
            client.login().await.is_ok()
        } else {
            false
        }
    }

    async fn cleanup(data_id: &str, group: &str) {
        let config = make_config();
        if let Ok(client) = NacosClient::new(config) {
            let _ = client.delete_config(data_id, group).await;
        }
    }

    /// 差分测试：get/set 语义一致
    #[tokio::test]
    #[ignore]
    async fn diff_real_nacos_get_set() {
        if !nacos_available().await {
            eprintln!("Nacos not available, skipping");
            return;
        }
        let data_id = "sz_orm_diff_nacos_get_set";
        let group = "DEFAULT_GROUP";
        cleanup(data_id, group).await;

        // Mock 行为
        let mut mock = NacosConfigCenter::new();
        mock.set(data_id, "hello nacos");
        let mock_value = mock.get(data_id).unwrap();

        // 真实 Nacos 行为
        let client = NacosClient::new(make_config()).unwrap();
        client
            .set_config(data_id, group, "hello nacos")
            .await
            .unwrap();
        let real_value = client.get_config(data_id, group).await.unwrap();

        assert_eq!(mock_value, real_value);
        assert_eq!(mock_value, "hello nacos");

        cleanup(data_id, group).await;
    }

    /// 差分测试：delete 语义一致
    #[tokio::test]
    #[ignore]
    async fn diff_real_nacos_delete() {
        if !nacos_available().await {
            eprintln!("Nacos not available, skipping");
            return;
        }
        let data_id = "sz_orm_diff_nacos_delete";
        let group = "DEFAULT_GROUP";
        cleanup(data_id, group).await;

        // Mock 行为
        let mut mock = NacosConfigCenter::new();
        mock.set(data_id, "to be deleted");
        assert!(mock.delete(data_id));
        assert!(!mock.exists(data_id));

        // 真实 Nacos 行为
        let client = NacosClient::new(make_config()).unwrap();
        client
            .set_config(data_id, group, "to be deleted")
            .await
            .unwrap();
        assert!(client.get_config(data_id, group).await.is_ok());
        client.delete_config(data_id, group).await.unwrap();
        assert!(client.get_config(data_id, group).await.is_err());

        cleanup(data_id, group).await;
    }

    /// 差分测试：RealNacosConfigCenter 实现 ConfigCenter trait
    #[tokio::test]
    #[ignore]
    async fn diff_real_nacos_config_center_trait() {
        if !nacos_available().await {
            eprintln!("Nacos not available, skipping");
            return;
        }
        let data_id = "sz_orm_diff_nacos_trait";
        let group = "DEFAULT_GROUP";
        cleanup(data_id, group).await;

        // 使用 RealNacosConfigCenter（实现 ConfigCenter trait）
        let mut real = RealNacosConfigCenter::new(make_config(), group.to_string()).unwrap();
        real.set(data_id, "trait_value");
        assert!(real.exists(data_id));
        assert_eq!(real.get(data_id), Some("trait_value".to_string()));
        assert!(real.delete(data_id));
        assert!(!real.exists(data_id));

        cleanup(data_id, group).await;
    }

    /// 差分测试：服务注册/发现语义一致
    #[tokio::test]
    #[ignore]
    async fn diff_real_nacos_service_discovery() {
        if !nacos_available().await {
            eprintln!("Nacos not available, skipping");
            return;
        }

        let client = NacosClient::new(make_config()).unwrap();
        let service = NacosServiceRegistration {
            name: "sz-orm-diff-nacos-svc".to_string(),
            ip: "127.0.0.1".to_string(),
            port: 9090,
            weight: Some(1.0),
            metadata: None,
            cluster_name: None,
            group_name: Some("DEFAULT_GROUP".to_string()),
            enabled: Some(true),
            ephemeral: Some(true),
        };

        // 注册服务
        client.register_service(service).await.unwrap();

        // 发现服务
        let instances = client
            .discover_service("sz-orm-diff-nacos-svc", "DEFAULT_GROUP")
            .await
            .unwrap();
        assert!(!instances.is_empty());

        // 注销服务
        client
            .deregister_service("sz-orm-diff-nacos-svc", "127.0.0.1", 9090)
            .await
            .unwrap();
    }
}
