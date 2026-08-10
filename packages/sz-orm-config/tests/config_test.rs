use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use sz_orm_config::*;

#[test]
fn test_consul_config_basic_get_set() {
    let mut cc = ConsulConfigCenter::new();
    assert!(!cc.exists("key1"));
    cc.set("key1", "value1");
    assert!(cc.exists("key1"));
    assert_eq!(cc.get("key1"), Some("value1".to_string()));
    assert_eq!(cc.get("nonexistent"), None);
}

#[test]
fn test_consul_config_delete() {
    let mut cc = ConsulConfigCenter::new();
    cc.set("key1", "value1");
    assert!(cc.delete("key1"));
    assert!(!cc.exists("key1"));
    assert!(!cc.delete("key1"));
}

#[test]
fn test_consul_config_list_sorted() {
    let mut cc = ConsulConfigCenter::new();
    cc.set("zebra", "1");
    cc.set("apple", "2");
    cc.set("mango", "3");
    let keys = cc.list();
    assert_eq!(keys, vec!["apple", "mango", "zebra"]);
}

#[test]
fn test_consul_config_empty_list() {
    let cc = ConsulConfigCenter::new();
    assert!(cc.list().is_empty());
}

#[test]
fn test_consul_config_overwrite() {
    let mut cc = ConsulConfigCenter::new();
    cc.set("key", "v1");
    cc.set("key", "v2");
    assert_eq!(cc.get("key"), Some("v2".to_string()));
    assert_eq!(cc.list().len(), 1);
}

#[test]
fn test_consul_config_subscribe_and_notify() {
    let mut cc = ConsulConfigCenter::new();
    let received = Arc::new(Mutex::new(Vec::new()));
    let received_clone = received.clone();
    cc.subscribe(
        "key1",
        Arc::new(move |key, value| {
            received_clone
                .lock()
                .unwrap()
                .push((key.to_string(), value.to_string()));
        }),
    );
    assert_eq!(cc.subscriber_count("key1"), 1);
    cc.set("key1", "value1");
    let events = cc.events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].key, "key1");
    assert_eq!(events[0].value, "value1");
    assert!(!events[0].deleted);
    let received = received.lock().unwrap();
    assert_eq!(*received, vec![("key1".to_string(), "value1".to_string())]);
}

#[test]
fn test_consul_config_delete_event() {
    let mut cc = ConsulConfigCenter::new();
    cc.set("key1", "value1");
    cc.delete("key1");
    let events = cc.events();
    assert_eq!(events.len(), 2);
    assert!(events[1].deleted);
    assert_eq!(events[1].value, "");
}

#[test]
fn test_consul_config_watch_always_true() {
    let cc = ConsulConfigCenter::new();
    assert!(cc.watch("any_key"));
    assert!(cc.watch(""));
}

#[test]
fn test_nacos_config_basic() {
    let mut nc = NacosConfigCenter::new();
    nc.set("key1", "value1");
    assert_eq!(nc.get("key1"), Some("value1".to_string()));
    assert!(nc.exists("key1"));
    assert!(nc.delete("key1"));
    assert!(!nc.exists("key1"));
}

#[test]
fn test_nacos_config_events_and_subscribers() {
    let mut nc = NacosConfigCenter::new();
    let count = Arc::new(Mutex::new(0));
    let count_clone = count.clone();
    nc.subscribe(
        "k",
        Arc::new(move |_, _| {
            *count_clone.lock().unwrap() += 1;
        }),
    );
    nc.set("k", "v1");
    nc.set("k", "v2");
    assert_eq!(*count.lock().unwrap(), 2);
    assert_eq!(nc.events().len(), 2);
}

#[test]
fn test_config_watcher_poll_detects_changes() {
    let mut cc = ConsulConfigCenter::new();
    let watcher = ConfigWatcher::new(100);
    let changes = Arc::new(Mutex::new(Vec::new()));
    let changes_clone = changes.clone();
    watcher.watch(
        "key1",
        Arc::new(move |key, value| {
            changes_clone
                .lock()
                .unwrap()
                .push((key.to_string(), value.to_string()));
        }),
    );
    assert_eq!(watcher.watcher_count(), 1);
    cc.set("key1", "value1");
    let n = watcher.poll(&cc);
    assert_eq!(n, 1);
    let c = changes.lock().unwrap();
    assert_eq!(*c, vec![("key1".to_string(), "value1".to_string())]);
}

#[test]
fn test_config_watcher_poll_no_changes() {
    let cc = ConsulConfigCenter::new();
    let watcher = ConfigWatcher::new(100);
    assert_eq!(watcher.poll(&cc), 0);
}

#[test]
fn test_config_watcher_poll_detects_deletion() {
    let mut cc = ConsulConfigCenter::new();
    let watcher = ConfigWatcher::new(100);
    cc.set("key1", "value1");
    watcher.poll(&cc);
    cc.delete("key1");
    let n = watcher.poll(&cc);
    assert_eq!(n, 1);
}

#[test]
fn test_config_watcher_min_interval() {
    let w = ConfigWatcher::new(50);
    assert!(w.poll_interval_ms >= 100);
    let w2 = ConfigWatcher::new(200);
    assert_eq!(w2.poll_interval_ms, 200);
}

#[test]
fn test_multi_source_config_merge_priority() {
    let mut mc = MultiSourceConfig::new();
    let mut file_cfg = HashMap::new();
    file_cfg.insert("key1".to_string(), "file_value".to_string());
    file_cfg.insert("key2".to_string(), "file_only".to_string());
    mc.set_file_config(file_cfg);

    let mut remote_cfg = HashMap::new();
    remote_cfg.insert("key1".to_string(), "remote_value".to_string());
    mc.set_remote_config(remote_cfg);

    let mut env_cfg = HashMap::new();
    env_cfg.insert("key1".to_string(), "env_value".to_string());
    mc.set_env_config(env_cfg);

    let merged = mc.merge();
    assert_eq!(merged.get("key1"), Some(&"env_value".to_string()));
    assert_eq!(merged.get("key2"), Some(&"file_only".to_string()));
}

#[test]
fn test_multi_source_config_source_of() {
    let mut mc = MultiSourceConfig::new();
    let mut file_cfg = HashMap::new();
    file_cfg.insert("fk".to_string(), "fv".to_string());
    mc.set_file_config(file_cfg);
    let mut remote_cfg = HashMap::new();
    remote_cfg.insert("rk".to_string(), "rv".to_string());
    mc.set_remote_config(remote_cfg);
    let mut env_cfg = HashMap::new();
    env_cfg.insert("ek".to_string(), "ev".to_string());
    mc.set_env_config(env_cfg);

    assert_eq!(mc.source_of("fk"), Some(ConfigSourcePriority::File));
    assert_eq!(mc.source_of("rk"), Some(ConfigSourcePriority::Remote));
    assert_eq!(mc.source_of("ek"), Some(ConfigSourcePriority::Env));
    assert_eq!(mc.source_of("nonexistent"), None);
}

#[test]
fn test_multi_source_config_load_json() {
    let mut mc = MultiSourceConfig::new();
    let json = r#"{"key1": "value1", "key2": "value2"}"#;
    mc.load_json(json).unwrap();
    assert_eq!(mc.get("key1"), Some("value1".to_string()));
    assert_eq!(mc.get("key2"), Some("value2".to_string()));
}

#[test]
fn test_multi_source_config_load_json_invalid() {
    let mut mc = MultiSourceConfig::new();
    assert!(mc.load_json("invalid json").is_err());
}

#[test]
fn test_multi_source_config_empty_merge() {
    let mc = MultiSourceConfig::new();
    let merged = mc.merge();
    assert!(merged.is_empty());
}

#[test]
fn test_config_field_schema_builder() {
    let schema = ConfigFieldSchema::new("port", ConfigFieldType::Integer)
        .required()
        .with_range(1.0, 65535.0);
    assert!(schema.required);
    assert_eq!(schema.min, Some(1.0));
    assert_eq!(schema.max, Some(65535.0));
}

#[test]
fn test_config_field_schema_with_length() {
    let schema = ConfigFieldSchema::new("name", ConfigFieldType::String).with_length(1, 100);
    assert_eq!(schema.min_length, Some(1));
    assert_eq!(schema.max_length, Some(100));
}

#[test]
fn test_config_field_schema_with_allowed_values() {
    let schema =
        ConfigFieldSchema::new("level", ConfigFieldType::String).with_allowed_values(vec![
            "debug".to_string(),
            "info".to_string(),
            "error".to_string(),
        ]);
    assert!(schema.allowed_values.is_some());
    assert_eq!(schema.allowed_values.unwrap().len(), 3);
}

#[test]
fn test_config_change_event_serialization() {
    let event = ConfigChangeEvent {
        key: "test_key".to_string(),
        value: "test_value".to_string(),
        deleted: false,
    };
    let json = serde_json::to_string(&event).unwrap();
    let deserialized: ConfigChangeEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.key, "test_key");
    assert_eq!(deserialized.value, "test_value");
    assert!(!deserialized.deleted);
}

#[test]
fn test_config_source_priority_ordering() {
    assert!(ConfigSourcePriority::Env > ConfigSourcePriority::Remote);
    assert!(ConfigSourcePriority::Remote > ConfigSourcePriority::File);
    assert!(ConfigSourcePriority::Env > ConfigSourcePriority::File);
}

#[test]
fn test_consul_config_default() {
    let cc = ConsulConfigCenter::default();
    assert!(cc.list().is_empty());
}

#[test]
fn test_nacos_config_default() {
    let nc = NacosConfigCenter::default();
    assert!(nc.list().is_empty());
}

#[test]
fn test_multi_source_config_default() {
    let mc = MultiSourceConfig::default();
    assert!(mc.merge().is_empty());
}

#[test]
fn test_config_watcher_default() {
    let w = ConfigWatcher::default();
    assert_eq!(w.poll_interval_ms, 5000);
    assert_eq!(w.watcher_count(), 0);
}
