//! 存储生命周期管理模块（v4.1.0，`storage-lifecycle` feature gate）
//!
//! 提供存储生命周期管理：分层策略、过期清理、策略引擎。

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// 存储层级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StorageTier {
    /// 热存储（高频访问，SSD/内存）
    Hot,
    /// 温存储（中频访问，HDD）
    Warm,
    /// 冷存储（低频访问，对象存储）
    Cold,
    /// 归档存储（极少访问，磁带/ Glacier）
    Archive,
}

impl StorageTier {
    /// 从字符串解析
    pub fn parse_tier(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "hot" => Some(Self::Hot),
            "warm" => Some(Self::Warm),
            "cold" => Some(Self::Cold),
            "archive" => Some(Self::Archive),
            _ => None,
        }
    }

    /// 转字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Hot => "hot",
            Self::Warm => "warm",
            Self::Cold => "cold",
            Self::Archive => "archive",
        }
    }
}

/// 生命周期策略
#[derive(Debug, Clone)]
pub struct LifecyclePolicy {
    /// 对象名
    pub name: String,
    /// 当前层级
    pub tier: StorageTier,
    /// 创建时间（Unix 秒）
    pub created_at: u64,
    /// 最后访问时间（Unix 秒）
    pub last_accessed_at: u64,
    /// 访问次数
    pub access_count: u64,
    /// 过期时间（Unix 秒，None 表示永不过期）
    pub expires_at: Option<u64>,
}

impl LifecyclePolicy {
    /// 创建新策略
    pub fn new(name: String, tier: StorageTier) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            name,
            tier,
            created_at: now,
            last_accessed_at: now,
            access_count: 0,
            expires_at: None,
        }
    }

    /// 设置过期时间
    pub fn with_expiry(mut self, expires_at: u64) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    /// 记录访问
    pub fn record_access(&mut self) {
        self.last_accessed_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.access_count += 1;
    }

    /// 是否已过期
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.expires_at.is_some_and(|exp| now >= exp)
    }

    /// 距最后访问的时长（秒）
    pub fn seconds_since_access(&self) -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now.saturating_sub(self.last_accessed_at)
    }
}

/// 分层策略规则
#[derive(Debug, Clone)]
pub struct TieringRule {
    /// 源层级
    pub from_tier: StorageTier,
    /// 目标层级
    pub to_tier: StorageTier,
    /// 距最后访问超过此秒数则迁移
    pub idle_seconds: u64,
    /// 访问次数低于此值则迁移
    pub max_access_count: u64,
}

impl TieringRule {
    /// 检查对象是否满足迁移条件
    pub fn should_tier(&self, policy: &LifecyclePolicy) -> bool {
        policy.tier == self.from_tier
            && policy.seconds_since_access() >= self.idle_seconds
            && policy.access_count <= self.max_access_count
    }
}

/// 过期清理器
pub struct ExpirationCleaner {
    /// 清理间隔（秒）
    pub interval: u64,
}

impl ExpirationCleaner {
    /// 创建清理器
    pub fn new(interval: u64) -> Self {
        Self { interval }
    }

    /// 找出所有已过期的对象
    pub fn find_expired<'a>(&self, policies: &'a [LifecyclePolicy]) -> Vec<&'a LifecyclePolicy> {
        policies.iter().filter(|p| p.is_expired()).collect()
    }
}

/// 存储生命周期管理器
pub struct StorageLifecycleManager {
    /// 对象策略表
    policies: HashMap<String, LifecyclePolicy>,
    /// 分层规则
    tiering_rules: Vec<TieringRule>,
    /// 过期清理器
    cleaner: ExpirationCleaner,
}

impl StorageLifecycleManager {
    /// 创建管理器
    pub fn new(cleaner: ExpirationCleaner) -> Self {
        Self {
            policies: HashMap::new(),
            tiering_rules: Vec::new(),
            cleaner,
        }
    }

    /// 添加对象
    pub fn add(&mut self, policy: LifecyclePolicy) {
        self.policies.insert(policy.name.clone(), policy);
    }

    /// 添加分层规则
    pub fn add_tiering_rule(&mut self, rule: TieringRule) {
        self.tiering_rules.push(rule);
    }

    /// 记录访问
    pub fn access(&mut self, name: &str) -> bool {
        if let Some(policy) = self.policies.get_mut(name) {
            policy.record_access();
            true
        } else {
            false
        }
    }

    /// 执行分层迁移，返回迁移计划
    pub fn plan_tiering(&self) -> Vec<(String, StorageTier, StorageTier)> {
        let mut plan = Vec::new();
        for policy in self.policies.values() {
            for rule in &self.tiering_rules {
                if rule.should_tier(policy) {
                    plan.push((policy.name.clone(), rule.from_tier, rule.to_tier));
                    break;
                }
            }
        }
        plan
    }

    /// 执行过期清理，返回被清理的对象名
    pub fn clean_expired(&mut self) -> Vec<String> {
        let policies: Vec<LifecyclePolicy> = self.policies.values().cloned().collect();
        let expired = self.cleaner.find_expired(&policies);
        let names: Vec<String> = expired.into_iter().map(|p| p.name.clone()).collect();
        for name in &names {
            self.policies.remove(name);
        }
        names
    }

    /// 获取对象数
    pub fn count(&self) -> usize {
        self.policies.len()
    }

    /// 获取对象策略
    pub fn get(&self, name: &str) -> Option<&LifecyclePolicy> {
        self.policies.get(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_tier_parsing() {
        assert_eq!(StorageTier::parse_tier("hot"), Some(StorageTier::Hot));
        assert_eq!(StorageTier::parse_tier("WARM"), Some(StorageTier::Warm));
        assert_eq!(StorageTier::parse_tier("invalid"), None);
    }

    #[test]
    fn test_lifecycle_policy_creation() {
        let policy = LifecyclePolicy::new("obj1".to_string(), StorageTier::Hot);
        assert_eq!(policy.tier, StorageTier::Hot);
        assert_eq!(policy.access_count, 0);
        assert!(!policy.is_expired());
    }

    #[test]
    fn test_lifecycle_policy_expiry() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let policy =
            LifecyclePolicy::new("obj1".to_string(), StorageTier::Hot).with_expiry(now - 1);
        assert!(policy.is_expired());
    }

    #[test]
    fn test_lifecycle_policy_access() {
        let mut policy = LifecyclePolicy::new("obj1".to_string(), StorageTier::Hot);
        assert_eq!(policy.access_count, 0);
        policy.record_access();
        assert_eq!(policy.access_count, 1);
        assert!(policy.last_accessed_at > 0);
    }

    #[test]
    fn test_tiering_rule() {
        let mut policy = LifecyclePolicy::new("obj1".to_string(), StorageTier::Hot);
        policy.access_count = 2;
        policy.last_accessed_at = 0;

        let rule = TieringRule {
            from_tier: StorageTier::Hot,
            to_tier: StorageTier::Warm,
            idle_seconds: 3600,
            max_access_count: 5,
        };
        assert!(rule.should_tier(&policy));
    }

    #[test]
    fn test_tiering_rule_no_match() {
        let policy = LifecyclePolicy::new("obj1".to_string(), StorageTier::Cold);
        let rule = TieringRule {
            from_tier: StorageTier::Hot,
            to_tier: StorageTier::Warm,
            idle_seconds: 3600,
            max_access_count: 5,
        };
        assert!(!rule.should_tier(&policy));
    }

    #[test]
    fn test_expiration_cleaner() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let policies = vec![
            LifecyclePolicy::new("active".to_string(), StorageTier::Hot),
            LifecyclePolicy::new("expired".to_string(), StorageTier::Hot).with_expiry(now - 1),
        ];
        let cleaner = ExpirationCleaner::new(3600);
        let expired = cleaner.find_expired(&policies);
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].name, "expired");
    }

    #[test]
    fn test_lifecycle_manager_add_and_access() {
        let mut manager = StorageLifecycleManager::new(ExpirationCleaner::new(3600));
        manager.add(LifecyclePolicy::new("obj1".to_string(), StorageTier::Hot));
        assert_eq!(manager.count(), 1);
        assert!(manager.access("obj1"));
        assert!(!manager.access("nonexistent"));
        assert_eq!(manager.get("obj1").unwrap().access_count, 1);
    }

    #[test]
    fn test_lifecycle_manager_tiering() {
        let mut manager = StorageLifecycleManager::new(ExpirationCleaner::new(3600));
        let mut policy = LifecyclePolicy::new("obj1".to_string(), StorageTier::Hot);
        policy.last_accessed_at = 0;
        policy.access_count = 1;
        manager.add(policy);

        manager.add_tiering_rule(TieringRule {
            from_tier: StorageTier::Hot,
            to_tier: StorageTier::Warm,
            idle_seconds: 3600,
            max_access_count: 5,
        });

        let plan = manager.plan_tiering();
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].0, "obj1");
        assert_eq!(plan[0].1, StorageTier::Hot);
        assert_eq!(plan[0].2, StorageTier::Warm);
    }

    #[test]
    fn test_lifecycle_manager_clean_expired() {
        let mut manager = StorageLifecycleManager::new(ExpirationCleaner::new(3600));
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        manager.add(LifecyclePolicy::new("active".to_string(), StorageTier::Hot));
        manager.add(
            LifecyclePolicy::new("expired".to_string(), StorageTier::Hot).with_expiry(now - 1),
        );

        let cleaned = manager.clean_expired();
        assert_eq!(cleaned.len(), 1);

        assert_eq!(cleaned[0], "expired");
        assert_eq!(manager.count(), 1);
    }
}
