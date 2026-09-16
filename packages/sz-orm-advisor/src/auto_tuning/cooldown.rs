//! 调优冷却期（`query-auto-tuning` feature）
//!
//! 变更应用后进入冷却期，期间暂缓接受新建议；
//! 严重劣化（degradation > threshold）立即触发回滚，不等冷却结束。

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// 冷却期配置
#[derive(Debug, Clone)]
pub struct CooldownConfig {
    /// 冷却时长（默认 5 分钟）
    pub cooldown_duration: Duration,
    /// 严重劣化阈值（百分比，超过则立即回滚，默认 30.0）
    pub force_rollback_threshold_pct: f64,
}

impl Default for CooldownConfig {
    fn default() -> Self {
        Self {
            cooldown_duration: Duration::from_secs(300),
            force_rollback_threshold_pct: 30.0,
        }
    }
}

/// 调优冷却期管理器
#[derive(Debug)]
pub struct TuningCooldown {
    config: CooldownConfig,
    entries: HashMap<String, Instant>,
}

impl TuningCooldown {
    pub fn new(config: CooldownConfig) -> Self {
        Self {
            config,
            entries: HashMap::new(),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(CooldownConfig::default())
    }

    pub fn config(&self) -> &CooldownConfig {
        &self.config
    }

    /// 进入冷却期
    pub fn enter(&mut self, query_key: impl Into<String>) {
        self.entries.insert(query_key.into(), Instant::now());
    }

    /// 是否处于冷却期内
    pub fn is_in_cooldown(&self, query_key: &str) -> bool {
        if let Some(start) = self.entries.get(query_key) {
            start.elapsed() < self.config.cooldown_duration
        } else {
            false
        }
    }

    /// 手动退出冷却期
    pub fn exit(&mut self, query_key: &str) {
        self.entries.remove(query_key);
    }

    /// 清理已过期的冷却条目
    pub fn purge_expired(&mut self) -> usize {
        let before = self.entries.len();
        self.entries
            .retain(|_, start| start.elapsed() < self.config.cooldown_duration);
        before - self.entries.len()
    }

    /// 判断劣化程度是否触发强制回滚
    pub fn should_force_rollback(&self, degradation_pct: f64) -> bool {
        degradation_pct >= self.config.force_rollback_threshold_pct
    }

    /// 剩余冷却时间（不在冷却期返回 None）
    pub fn remaining(&self, query_key: &str) -> Option<Duration> {
        let start = self.entries.get(query_key)?;
        let elapsed = start.elapsed();
        if elapsed < self.config.cooldown_duration {
            Some(self.config.cooldown_duration - elapsed)
        } else {
            None
        }
    }

    /// 当前冷却条目数
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cooldown_enter_and_check() {
        let mut cd = TuningCooldown::with_defaults();
        cd.enter("q1");
        assert!(cd.is_in_cooldown("q1"));
        assert!(!cd.is_in_cooldown("q2"));
    }

    #[test]
    fn cooldown_exit() {
        let mut cd = TuningCooldown::with_defaults();
        cd.enter("q1");
        assert!(cd.is_in_cooldown("q1"));
        cd.exit("q1");
        assert!(!cd.is_in_cooldown("q1"));
    }

    #[test]
    fn force_rollback_threshold() {
        let cd = TuningCooldown::with_defaults();
        assert!(!cd.should_force_rollback(10.0));
        assert!(cd.should_force_rollback(30.0));
        assert!(cd.should_force_rollback(50.0));
    }

    #[test]
    fn remaining_time() {
        let mut cd = TuningCooldown::new(CooldownConfig {
            cooldown_duration: Duration::from_secs(10),
            force_rollback_threshold_pct: 30.0,
        });
        cd.enter("q1");
        let rem = cd.remaining("q1").expect("should have remaining");
        assert!(rem <= Duration::from_secs(10));
        assert!(rem > Duration::from_secs(8));
        assert!(cd.remaining("q2").is_none());
    }
}
