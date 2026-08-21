//! # Config Center Adapter — sz-orm-core 配置中心适配层
//!
//! v5.0.0 M4：将 sz-orm-config 的 ConsulConfigCenter 接入 sz-orm-core，
//! 提供 `config_get` / `config_set` / `config_count` 入口。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use parking_lot::RwLock;
use sz_orm_config::{ConfigCenter, ConsulConfigCenter};

static CONFIG: OnceLock<RwLock<ConsulConfigCenter>> = OnceLock::new();
static OP_COUNT: AtomicU64 = AtomicU64::new(0);

fn config() -> &'static RwLock<ConsulConfigCenter> {
    CONFIG.get_or_init(|| RwLock::new(ConsulConfigCenter::new()))
}

/// 获取配置值
pub fn config_get(key: &str) -> Option<String> {
    OP_COUNT.fetch_add(1, Ordering::Relaxed);
    let config = config().read();
    config.get(key)
}

/// 设置配置值
pub fn config_set(key: &str, value: &str) {
    OP_COUNT.fetch_add(1, Ordering::Relaxed);
    let mut config = config().write();
    config.set(key, value);
}

/// 获取操作计数
pub fn config_count() -> u64 {
    OP_COUNT.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_set_and_get() {
        config_set("test_key", "test_value");
        let val = config_get("test_key");
        assert_eq!(val, Some("test_value".to_string()));
    }

    #[test]
    fn test_config_count_increments() {
        let before = config_count();
        config_set("count_test", "1");
        let after = config_count();
        assert!(after > before);
    }
}
