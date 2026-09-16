//! 策略热更新
//!
//! 原子交换策略集，支持版本追踪。

use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::RwLock;

use super::policy_engine::{AbacPolicy, AbacPolicyEngine};

/// 策略热更新器
pub struct PolicyHotReloader {
    engine: RwLock<AbacPolicyEngine>,
    version: AtomicU64,
    last_update_ms: AtomicU64,
}

impl PolicyHotReloader {
    /// 创建热更新器
    pub fn new(engine: AbacPolicyEngine) -> Self {
        Self {
            engine: RwLock::new(engine),
            version: AtomicU64::new(1),
            last_update_ms: AtomicU64::new(0),
        }
    }

    /// 原子替换全部策略
    pub fn swap_policies(&self, policies: Vec<AbacPolicy>) -> u64 {
        let mut engine = self.engine.write();
        *engine = AbacPolicyEngine::new();
        for policy in policies {
            engine.add_policy(policy);
        }
        let new_version = self.version.fetch_add(1, Ordering::Relaxed) + 1;
        self.last_update_ms
            .store(current_time_ms(), Ordering::Relaxed);
        new_version
    }

    /// 追加单个策略
    pub fn add_policy(&self, policy: AbacPolicy) -> u64 {
        self.engine.write().add_policy(policy);
        let new_version = self.version.fetch_add(1, Ordering::Relaxed) + 1;
        self.last_update_ms
            .store(current_time_ms(), Ordering::Relaxed);
        new_version
    }

    /// 获取当前版本
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Relaxed)
    }

    /// 最后更新时间
    pub fn last_update_ms(&self) -> u64 {
        self.last_update_ms.load(Ordering::Relaxed)
    }

    /// 获取策略数
    pub fn policy_count(&self) -> usize {
        self.engine.read().policy_count()
    }
}

fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::super::policy_engine::{AttributeScope, AttributeValue, Condition, Effect};
    use super::*;

    fn make_policy(id: &str) -> AbacPolicy {
        AbacPolicy::new(
            id,
            "read",
            Condition::Eq {
                scope: AttributeScope::Subject,
                key: "role".into(),
                value: AttributeValue::str_val("admin"),
            },
            Effect::Allow,
        )
    }

    #[test]
    fn test_initial_version() {
        let reloader = PolicyHotReloader::new(AbacPolicyEngine::new());
        assert_eq!(reloader.version(), 1);
    }

    #[test]
    fn test_swap_increments_version() {
        let reloader = PolicyHotReloader::new(AbacPolicyEngine::new());
        let v1 = reloader.swap_policies(vec![make_policy("p1")]);
        assert_eq!(v1, 2);
        let v2 = reloader.swap_policies(vec![make_policy("p1"), make_policy("p2")]);
        assert_eq!(v2, 3);
    }

    #[test]
    fn test_add_policy_increments_version() {
        let reloader = PolicyHotReloader::new(AbacPolicyEngine::new());
        let v = reloader.add_policy(make_policy("p1"));
        assert_eq!(v, 2);
        assert_eq!(reloader.policy_count(), 1);
    }

    #[test]
    fn test_swap_replaces_all() {
        let reloader = PolicyHotReloader::new(AbacPolicyEngine::new());
        reloader.add_policy(make_policy("p1"));
        reloader.add_policy(make_policy("p2"));
        assert_eq!(reloader.policy_count(), 2);
        reloader.swap_policies(vec![make_policy("p3")]);
        assert_eq!(reloader.policy_count(), 1);
    }

    #[test]
    fn test_last_update_timestamp() {
        let reloader = PolicyHotReloader::new(AbacPolicyEngine::new());
        assert_eq!(reloader.last_update_ms(), 0);
        reloader.add_policy(make_policy("p1"));
        assert!(reloader.last_update_ms() > 0);
    }
}
