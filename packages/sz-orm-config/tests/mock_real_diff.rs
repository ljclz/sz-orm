//! 内存实现与真实实现行为差分测试
//!
//! 验证 `ConsulConfigCenter`（内存）的行为与 Consul/Nacos 真实 API 语义一致。
//! 不带 `#[ignore]` 的测试始终运行（验证内存行为符合预期语义）。

use sz_orm_config::*;

// ============================================================================
// 配置读写语义差分
// ============================================================================

#[test]
fn diff_get_set_consistency() {
    let mut cc = ConsulConfigCenter::new();
    cc.set("key1", "value1");
    assert_eq!(cc.get("key1"), Some("value1".to_string()));
    assert_eq!(cc.get("nonexistent"), None);
}

#[test]
fn diff_overwrite_on_set() {
    let mut cc = ConsulConfigCenter::new();
    cc.set("key", "v1");
    cc.set("key", "v2");
    assert_eq!(cc.get("key"), Some("v2".to_string()));
}

#[test]
fn diff_delete_behavior() {
    let mut cc = ConsulConfigCenter::new();
    cc.set("key", "value");
    assert!(cc.delete("key"));
    assert!(!cc.exists("key"));
    assert!(!cc.delete("nonexistent"));
}

#[test]
fn diff_exists_after_set() {
    let mut cc = ConsulConfigCenter::new();
    assert!(!cc.exists("key"));
    cc.set("key", "value");
    assert!(cc.exists("key"));
}

#[test]
fn diff_list_keys() {
    let mut cc = ConsulConfigCenter::new();
    cc.set("a", "1");
    cc.set("b", "2");
    cc.set("c", "3");
    let keys = cc.list();
    assert_eq!(keys.len(), 3);
    assert!(keys.contains(&"a".to_string()));
    assert!(keys.contains(&"b".to_string()));
    assert!(keys.contains(&"c".to_string()));
}

// ============================================================================
// 配置变更通知语义差分
// ============================================================================

#[test]
fn diff_watch_registration() {
    let cc = ConsulConfigCenter::new();
    assert!(cc.watch("key1"));
}

#[test]
fn diff_subscribe_callback_on_set() {
    use std::sync::{Arc, Mutex};
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
    cc.set("key1", "value1");
    cc.set("key1", "value2");
    let events = received.lock().unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0], ("key1".to_string(), "value1".to_string()));
    assert_eq!(events[1], ("key1".to_string(), "value2".to_string()));
}

#[test]
fn diff_subscribe_callback_on_delete() {
    use std::sync::{Arc, Mutex};
    let mut cc = ConsulConfigCenter::new();
    cc.set("key1", "initial");
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
    cc.delete("key1");
    let events = received.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, "key1");
    assert!(events[0].1.is_empty());
}

#[test]
fn diff_events_log() {
    let mut cc = ConsulConfigCenter::new();
    cc.set("a", "1");
    cc.set("b", "2");
    cc.delete("a");
    let events = cc.events();
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].key, "a");
    assert!(!events[0].deleted);
    assert_eq!(events[1].key, "b");
    assert!(!events[1].deleted);
    assert_eq!(events[2].key, "a");
    assert!(events[2].deleted);
}

#[test]
fn diff_subscriber_count() {
    use std::sync::Arc;
    let mut cc = ConsulConfigCenter::new();
    assert_eq!(cc.subscriber_count("key"), 0);
    cc.subscribe("key", Arc::new(|_, _| {}));
    cc.subscribe("key", Arc::new(|_, _| {}));
    assert_eq!(cc.subscriber_count("key"), 2);
    assert_eq!(cc.subscriber_count("other"), 0);
}

// ============================================================================
// Nacos 配置中心差分
// ============================================================================

#[test]
fn diff_nacos_get_set_consistency() {
    let mut cc = NacosConfigCenter::new();
    cc.set("dataId", "content");
    assert_eq!(cc.get("dataId"), Some("content".to_string()));
    assert_eq!(cc.get("nonexistent"), None);
}

#[test]
fn diff_nacos_delete_behavior() {
    let mut cc = NacosConfigCenter::new();
    cc.set("key", "value");
    assert!(cc.delete("key"));
    assert!(!cc.exists("key"));
}

#[test]
fn diff_nacos_list_keys() {
    let mut cc = NacosConfigCenter::new();
    cc.set("x", "1");
    cc.set("y", "2");
    assert_eq!(cc.list().len(), 2);
}

#[test]
fn diff_nacos_watch_and_subscribe() {
    use std::sync::{Arc, Mutex};
    let mut cc = NacosConfigCenter::new();
    assert!(cc.watch("key"));
    let received = Arc::new(Mutex::new(0));
    let received_clone = received.clone();
    cc.subscribe(
        "key",
        Arc::new(move |_, _| {
            *received_clone.lock().unwrap() += 1;
        }),
    );
    cc.set("key", "v1");
    cc.set("key", "v2");
    assert_eq!(*received.lock().unwrap(), 2);
}
