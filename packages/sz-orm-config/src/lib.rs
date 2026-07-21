//! # SZ-ORM Config — 配置中心
//!
//! 提供配置中心抽象（Consul/Nacos 等），支持 get/set/delete/list/watch，
//! 并可在配置变更时通过回调通知订阅者。
//!
//! ## 主要类型
//!
//! - [`ConfigCenter`] trait — 配置中心接口
//! - [`ConfigChangeEvent`] — 配置变更事件

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Callback invoked when a configuration value changes.
/// Arguments: `(key, new_value)`. On delete, `new_value` is empty.
pub type ConfigChangeCallback = Arc<dyn Fn(&str, &str) + Send + Sync>;

/// Configuration center abstraction (Consul/Nacos/etc).
pub trait ConfigCenter: Send + Sync {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&mut self, key: &str, value: &str);
    fn delete(&mut self, key: &str) -> bool;
    fn exists(&self, key: &str) -> bool;
    fn list(&self) -> Vec<String>;
    /// Returns true if a watch was successfully registered.
    /// In this in-memory implementation, registration always succeeds.
    fn watch(&self, key: &str) -> bool;
    /// Registers a callback for changes to `key`.
    fn subscribe(&mut self, key: &str, callback: ConfigChangeCallback);
}

/// Configuration change event record, useful for testing and auditing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigChangeEvent {
    pub key: String,
    pub value: String,
    pub deleted: bool,
}

/// Consul-style in-memory configuration center.
pub struct ConsulConfigCenter {
    data: HashMap<String, String>,
    subscribers: HashMap<String, Vec<ConfigChangeCallback>>,
    events: Mutex<Vec<ConfigChangeEvent>>,
}

impl ConsulConfigCenter {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
            subscribers: HashMap::new(),
            events: Mutex::new(Vec::new()),
        }
    }

    fn notify(&self, key: &str, value: &str, deleted: bool) {
        if let Some(callbacks) = self.subscribers.get(key) {
            for cb in callbacks {
                cb(key, value);
            }
        }
        if let Ok(mut events) = self.events.lock() {
            events.push(ConfigChangeEvent {
                key: key.to_string(),
                value: value.to_string(),
                deleted,
            });
        }
    }

    /// Returns the ordered list of all change events that have occurred.
    pub fn events(&self) -> Vec<ConfigChangeEvent> {
        self.events.lock().map(|e| e.clone()).unwrap_or_default()
    }

    pub fn subscriber_count(&self, key: &str) -> usize {
        self.subscribers.get(key).map(|cbs| cbs.len()).unwrap_or(0)
    }
}

impl Default for ConsulConfigCenter {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigCenter for ConsulConfigCenter {
    fn get(&self, key: &str) -> Option<String> {
        self.data.get(key).cloned()
    }

    fn set(&mut self, key: &str, value: &str) {
        self.data.insert(key.to_string(), value.to_string());
        self.notify(key, value, false);
    }

    fn delete(&mut self, key: &str) -> bool {
        let removed = self.data.remove(key).is_some();
        if removed {
            self.notify(key, "", true);
        }
        removed
    }

    fn exists(&self, key: &str) -> bool {
        self.data.contains_key(key)
    }

    fn list(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.data.keys().cloned().collect();
        keys.sort();
        keys
    }

    fn watch(&self, _key: &str) -> bool {
        true
    }

    fn subscribe(&mut self, key: &str, callback: ConfigChangeCallback) {
        self.subscribers
            .entry(key.to_string())
            .or_default()
            .push(callback);
    }
}

/// Nacos-style in-memory configuration center.
pub struct NacosConfigCenter {
    data: HashMap<String, String>,
    subscribers: HashMap<String, Vec<ConfigChangeCallback>>,
    events: Mutex<Vec<ConfigChangeEvent>>,
}

impl NacosConfigCenter {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
            subscribers: HashMap::new(),
            events: Mutex::new(Vec::new()),
        }
    }

    fn notify(&self, key: &str, value: &str, deleted: bool) {
        if let Some(callbacks) = self.subscribers.get(key) {
            for cb in callbacks {
                cb(key, value);
            }
        }
        if let Ok(mut events) = self.events.lock() {
            events.push(ConfigChangeEvent {
                key: key.to_string(),
                value: value.to_string(),
                deleted,
            });
        }
    }

    pub fn events(&self) -> Vec<ConfigChangeEvent> {
        self.events.lock().map(|e| e.clone()).unwrap_or_default()
    }

    pub fn subscriber_count(&self, key: &str) -> usize {
        self.subscribers.get(key).map(|cbs| cbs.len()).unwrap_or(0)
    }
}

impl Default for NacosConfigCenter {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigCenter for NacosConfigCenter {
    fn get(&self, key: &str) -> Option<String> {
        self.data.get(key).cloned()
    }

    fn set(&mut self, key: &str, value: &str) {
        self.data.insert(key.to_string(), value.to_string());
        self.notify(key, value, false);
    }

    fn delete(&mut self, key: &str) -> bool {
        let removed = self.data.remove(key).is_some();
        if removed {
            self.notify(key, "", true);
        }
        removed
    }

    fn exists(&self, key: &str) -> bool {
        self.data.contains_key(key)
    }

    fn list(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.data.keys().cloned().collect();
        keys.sort();
        keys
    }

    fn watch(&self, _key: &str) -> bool {
        true
    }

    fn subscribe(&mut self, key: &str, callback: ConfigChangeCallback) {
        self.subscribers
            .entry(key.to_string())
            .or_default()
            .push(callback);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn test_consul_set_and_get() {
        let mut c = ConsulConfigCenter::new();
        c.set("k", "v");
        assert_eq!(c.get("k"), Some("v".to_string()));
        assert!(c.exists("k"));
        assert!(!c.exists("missing"));
    }

    #[test]
    fn test_consul_get_missing() {
        let c = ConsulConfigCenter::new();
        assert_eq!(c.get("missing"), None);
    }

    #[test]
    fn test_consul_delete() {
        let mut c = ConsulConfigCenter::new();
        c.set("k", "v");
        assert!(c.delete("k"));
        assert!(!c.exists("k"));
        assert_eq!(c.get("k"), None);
        // Deleting a missing key returns false
        assert!(!c.delete("missing"));
    }

    #[test]
    fn test_consul_list_sorted() {
        let mut c = ConsulConfigCenter::new();
        c.set("z", "1");
        c.set("a", "2");
        c.set("m", "3");
        assert_eq!(c.list(), vec!["a", "m", "z"]);
    }

    #[test]
    fn test_consul_watch_returns_true() {
        let c = ConsulConfigCenter::new();
        assert!(c.watch("any-key"));
    }

    #[test]
    fn test_nacos_set_and_get() {
        let mut c = NacosConfigCenter::new();
        c.set("k", "v");
        assert_eq!(c.get("k"), Some("v".to_string()));
    }

    #[test]
    fn test_nacos_watch_returns_true() {
        let c = NacosConfigCenter::new();
        assert!(c.watch("k"));
    }

    #[test]
    fn test_nacos_delete() {
        let mut c = NacosConfigCenter::new();
        c.set("k", "v");
        assert!(c.delete("k"));
        assert_eq!(c.get("k"), None);
    }

    #[test]
    fn test_nacos_list_sorted() {
        let mut c = NacosConfigCenter::new();
        c.set("b", "1");
        c.set("a", "2");
        assert_eq!(c.list(), vec!["a", "b"]);
    }

    // ---- Subscribe / notify tests ----

    #[test]
    fn test_consul_subscribe_receives_set_events() {
        let mut c = ConsulConfigCenter::new();
        let count = Arc::new(AtomicU32::new(0));
        let last_value = Arc::new(Mutex::new(String::new()));

        let cb_count = count.clone();
        let cb_value = last_value.clone();
        c.subscribe(
            "app.config",
            Arc::new(move |_key, value| {
                cb_count.fetch_add(1, Ordering::SeqCst);
                *cb_value.lock().unwrap() = value.to_string();
            }),
        );

        c.set("app.config", "v1");
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert_eq!(*last_value.lock().unwrap(), "v1");

        c.set("app.config", "v2");
        assert_eq!(count.load(Ordering::SeqCst), 2);
        assert_eq!(*last_value.lock().unwrap(), "v2");
    }

    #[test]
    fn test_consul_subscribe_receives_delete_events() {
        let mut c = ConsulConfigCenter::new();
        let deleted = Arc::new(AtomicU32::new(0));
        let last_value = Arc::new(Mutex::new(String::new()));

        let d_count = deleted.clone();
        let d_value = last_value.clone();
        c.subscribe(
            "k",
            Arc::new(move |_key, value| {
                d_count.fetch_add(1, Ordering::SeqCst);
                *d_value.lock().unwrap() = value.to_string();
            }),
        );

        c.set("k", "v");
        c.delete("k");
        assert_eq!(deleted.load(Ordering::SeqCst), 2); // set + delete
        assert_eq!(*last_value.lock().unwrap(), ""); // delete sends empty
    }

    #[test]
    fn test_consul_multiple_subscribers() {
        let mut c = ConsulConfigCenter::new();
        let c1 = Arc::new(AtomicU32::new(0));
        let c2 = Arc::new(AtomicU32::new(0));

        let c1_clone = c1.clone();
        c.subscribe(
            "k",
            Arc::new(move |_key, _value| {
                c1_clone.fetch_add(1, Ordering::SeqCst);
            }),
        );

        let c2_clone = c2.clone();
        c.subscribe(
            "k",
            Arc::new(move |_key, _value| {
                c2_clone.fetch_add(1, Ordering::SeqCst);
            }),
        );

        assert_eq!(c.subscriber_count("k"), 2);
        c.set("k", "v");
        assert_eq!(c1.load(Ordering::SeqCst), 1);
        assert_eq!(c2.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_consul_subscribers_are_keyed() {
        let mut c = ConsulConfigCenter::new();
        let other_count = Arc::new(AtomicU32::new(0));
        let oc = other_count.clone();
        c.subscribe(
            "other",
            Arc::new(move |_key, _value| {
                oc.fetch_add(1, Ordering::SeqCst);
            }),
        );

        c.set("this", "v");
        // Should not notify subscribers of "other"
        assert_eq!(other_count.load(Ordering::SeqCst), 0);

        c.set("other", "v");
        assert_eq!(other_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_consul_events_record() {
        let mut c = ConsulConfigCenter::new();
        c.set("a", "1");
        c.set("b", "2");
        c.delete("a");

        let events = c.events();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].key, "a");
        assert_eq!(events[0].value, "1");
        assert!(!events[0].deleted);
        assert_eq!(events[2].key, "a");
        assert!(events[2].deleted);
    }

    #[test]
    fn test_nacos_subscribe_receives_events() {
        let mut c = NacosConfigCenter::new();
        let received = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
        let r = received.clone();
        c.subscribe(
            "cfg",
            Arc::new(move |key, value| {
                r.lock().unwrap().push((key.to_string(), value.to_string()));
            }),
        );
        c.set("cfg", "v1");
        c.set("cfg", "v2");
        let received = received.lock().unwrap();
        assert_eq!(
            *received,
            vec![
                ("cfg".to_string(), "v1".to_string()),
                ("cfg".to_string(), "v2".to_string()),
            ]
        );
    }

    #[test]
    fn test_nacos_events_record() {
        let mut c = NacosConfigCenter::new();
        c.set("k", "v");
        let events = c.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].key, "k");
        assert_eq!(events[0].value, "v");
    }

    #[test]
    fn test_subscribe_via_trait_object() {
        // Verify subscribe works through a boxed trait object.
        let mut boxed: Box<dyn ConfigCenter> = Box::new(ConsulConfigCenter::new());
        let count = Arc::new(AtomicU32::new(0));
        let c = count.clone();
        boxed.subscribe(
            "k",
            Arc::new(move |_key, _value| {
                c.fetch_add(1, Ordering::SeqCst);
            }),
        );
        boxed.set("k", "v");
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_overwrite_existing_value_notifies() {
        let mut c = ConsulConfigCenter::new();
        let count = Arc::new(AtomicU32::new(0));
        let c1 = count.clone();
        c.subscribe(
            "k",
            Arc::new(move |_key, _value| {
                c1.fetch_add(1, Ordering::SeqCst);
            }),
        );
        c.set("k", "v1");
        c.set("k", "v2");
        c.set("k", "v3");
        assert_eq!(count.load(Ordering::SeqCst), 3);
    }
}
