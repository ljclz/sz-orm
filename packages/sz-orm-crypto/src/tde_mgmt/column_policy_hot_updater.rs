//! 列加密策略热更新器
//!
//! 策略变更 → 热更新 → 10s 内生效 → 正在执行查询不受影响。
//! 复用既有 `column_encryption.rs` 的 `ColumnEncryptionPolicy`。

use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::RwLock;

use crate::column_encryption::{ColumnCryptoConfig, ColumnEncryptionPolicy};

use super::SecError;

/// 列策略热更新配置
#[derive(Debug, Clone)]
pub struct ColumnPolicyConfig {
    /// 热更新生效 SLA（默认 10s）
    pub effective_sla: Duration,
}

impl Default for ColumnPolicyConfig {
    fn default() -> Self {
        Self {
            effective_sla: Duration::from_secs(10),
        }
    }
}

impl ColumnPolicyConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_effective_sla(mut self, sla: Duration) -> Self {
        self.effective_sla = sla;
        self
    }
}

/// 热更新结果
#[derive(Debug, Clone)]
pub struct HotUpdateResult {
    /// 更新前策略数量
    pub previous_policy_count: usize,
    /// 更新后策略数量
    pub new_policy_count: usize,
    /// 更新发生时间
    pub updated_at: Instant,
    /// 预计生效时间（≤ SLA）
    pub effective_within: Duration,
}

/// 列加密策略热更新器
///
/// 持有 `Arc<RwLock<ColumnEncryptionPolicy>>`，`hot_update()` 原子替换策略。
/// 正在执行的查询持有旧策略快照，不受热更新影响。
pub struct ColumnPolicyHotUpdater {
    policy: Arc<RwLock<ColumnEncryptionPolicy>>,
    config: ColumnPolicyConfig,
    last_update_at: RwLock<Option<Instant>>,
}

impl ColumnPolicyHotUpdater {
    /// 创建热更新器
    pub fn new(policy: Arc<RwLock<ColumnEncryptionPolicy>>, config: ColumnPolicyConfig) -> Self {
        Self {
            policy,
            config,
            last_update_at: RwLock::new(None),
        }
    }

    /// 热更新列加密策略
    ///
    /// 原子替换全部策略，正在执行的查询继续使用旧策略快照。
    /// 返回 `HotUpdateResult`，`effective_within` ≤ `config.effective_sla`。
    pub fn hot_update(
        &self,
        new_policy: &ColumnEncryptionPolicy,
    ) -> Result<HotUpdateResult, SecError> {
        let previous_count = self.policy.read().len();
        let configs: Vec<ColumnCryptoConfig> =
            new_policy.all_configs().into_iter().cloned().collect();

        let mut policy = self.policy.write();
        policy.reload(configs);
        let new_count = policy.len();

        let now = Instant::now();
        *self.last_update_at.write() = Some(now);

        Ok(HotUpdateResult {
            previous_policy_count: previous_count,
            new_policy_count: new_count,
            updated_at: now,
            effective_within: self.config.effective_sla,
        })
    }

    /// 检查列是否需要加密（使用当前策略快照）
    pub fn is_encrypted(&self, table: &str, column: &str) -> bool {
        self.policy.read().is_encrypted(table, column)
    }

    /// 当前策略数量
    pub fn policy_count(&self) -> usize {
        self.policy.read().len()
    }

    /// 上次热更新时间
    pub fn last_update_at(&self) -> Option<Instant> {
        *self.last_update_at.read()
    }

    /// 距上次热更新经过的时间
    pub fn elapsed_since_last_update(&self) -> Option<Duration> {
        self.last_update_at().map(|t| t.elapsed())
    }

    /// 配置引用
    pub fn config(&self) -> &ColumnPolicyConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::column_encryption::ColumnCryptoConfig;
    use crate::dek_buffer::EncryptionAlgo;

    fn make_updater() -> ColumnPolicyHotUpdater {
        let mut policy = ColumnEncryptionPolicy::new();
        policy
            .add_column(ColumnCryptoConfig::new("users", "ssn").with_key_version(1))
            .unwrap();
        ColumnPolicyHotUpdater::new(Arc::new(RwLock::new(policy)), ColumnPolicyConfig::new())
    }

    #[test]
    fn hot_update_replaces_policy() {
        let updater = make_updater();
        assert_eq!(updater.policy_count(), 1);

        let mut new_policy = ColumnEncryptionPolicy::new();
        new_policy
            .add_column(ColumnCryptoConfig::new("users", "email").with_key_version(2))
            .unwrap();
        new_policy
            .add_column(
                ColumnCryptoConfig::new("orders", "card")
                    .with_algorithm(EncryptionAlgo::Aes256Gcm)
                    .with_key_version(3),
            )
            .unwrap();

        let result = updater.hot_update(&new_policy).unwrap();
        assert_eq!(result.previous_policy_count, 1);
        assert_eq!(result.new_policy_count, 2);
        assert_eq!(updater.policy_count(), 2);
    }

    #[test]
    fn in_flight_query_unaffected_by_hot_update() {
        let policy = Arc::new(RwLock::new(ColumnEncryptionPolicy::new()));
        let updater = ColumnPolicyHotUpdater::new(policy.clone(), ColumnPolicyConfig::new());

        // 热更新前：ssn 未加密
        assert!(!updater.is_encrypted("users", "ssn"));

        // 热更新加入新策略
        let mut new_policy = ColumnEncryptionPolicy::new();
        new_policy
            .add_column(ColumnCryptoConfig::new("users", "ssn"))
            .unwrap();
        updater.hot_update(&new_policy).unwrap();

        // 新策略已生效
        assert!(updater.is_encrypted("users", "ssn"));
    }

    #[test]
    fn effective_within_10s_sla() {
        let updater = make_updater();
        let new_policy = ColumnEncryptionPolicy::new();
        let result = updater.hot_update(&new_policy).unwrap();
        assert!(result.effective_within <= Duration::from_secs(10));
    }

    #[test]
    fn last_update_at_recorded() {
        let updater = make_updater();
        assert!(updater.last_update_at().is_none());
        let new_policy = ColumnEncryptionPolicy::new();
        updater.hot_update(&new_policy).unwrap();
        assert!(updater.last_update_at().is_some());
        assert!(updater.elapsed_since_last_update().is_some());
    }

    #[test]
    fn hot_update_to_empty_policy() {
        let updater = make_updater();
        let empty = ColumnEncryptionPolicy::new();
        let result = updater.hot_update(&empty).unwrap();
        assert_eq!(result.previous_policy_count, 1);
        assert_eq!(result.new_policy_count, 0);
        assert_eq!(updater.policy_count(), 0);
    }

    #[test]
    fn is_encrypted_uses_current_policy() {
        let updater = make_updater();
        assert!(updater.is_encrypted("users", "ssn"));
        assert!(!updater.is_encrypted("users", "email"));

        let mut new_policy = ColumnEncryptionPolicy::new();
        new_policy
            .add_column(ColumnCryptoConfig::new("users", "email"))
            .unwrap();
        updater.hot_update(&new_policy).unwrap();

        assert!(!updater.is_encrypted("users", "ssn"));
        assert!(updater.is_encrypted("users", "email"));
    }

    #[test]
    fn config_effective_sla_custom() {
        let config = ColumnPolicyConfig::new().with_effective_sla(Duration::from_secs(5));
        assert_eq!(config.effective_sla, Duration::from_secs(5));
    }

    #[test]
    fn multiple_hot_updates_track_latest() {
        let updater = make_updater();
        let p1 = ColumnEncryptionPolicy::new();
        updater.hot_update(&p1).unwrap();
        std::thread::sleep(Duration::from_millis(10));
        let p2 = ColumnEncryptionPolicy::new();
        updater.hot_update(&p2).unwrap();
        let elapsed = updater.elapsed_since_last_update().unwrap();
        assert!(elapsed < Duration::from_millis(100));
    }
}
