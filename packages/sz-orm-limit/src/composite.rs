//! 限流策略组合器（Composite Limiter）
//!
//! 提供更丰富的限流器组合策略，包括链式、回退、权重等。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::{RateLimitError, RateLimitResult, RateLimiter};

/// 组合策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CompositeStrategy {
    /// 全部通过才允许（最严格）
    AllMustAllow,
    /// 任一通过即允许（最宽松）
    AnyCanAllow,
    /// 按优先级，第一个拒绝即拒绝
    FirstRejectWins,
    /// 按权重投票
    WeightedVote,
}

/// 组合限流器
///
/// 将多个限流器按指定策略组合。
pub struct CompositeLimiter {
    limiters: Vec<Arc<dyn RateLimiter>>,
    strategy: CompositeStrategy,
    weights: Vec<u32>,
    total_calls: AtomicU64,
}

impl CompositeLimiter {
    /// 创建组合限流器
    pub fn new(limiters: Vec<Arc<dyn RateLimiter>>, strategy: CompositeStrategy) -> Self {
        let count = limiters.len();
        Self {
            limiters,
            strategy,
            weights: vec![1; count],
            total_calls: AtomicU64::new(0),
        }
    }

    /// 设置权重（仅 WeightedVote 策略生效）
    pub fn with_weights(mut self, weights: Vec<u32>) -> Self {
        if weights.len() == self.limiters.len() {
            self.weights = weights;
        }
        self
    }

    /// 添加限流器
    pub fn with_limiter(mut self, limiter: Arc<dyn RateLimiter>) -> Self {
        self.limiters.push(limiter);
        self.weights.push(1);
        self
    }

    /// 限流器数量
    pub fn limiter_count(&self) -> usize {
        self.limiters.len()
    }

    /// 策略
    pub fn strategy(&self) -> CompositeStrategy {
        self.strategy
    }

    /// 检查
    pub fn check(&self, key: &str) -> Result<RateLimitResult, RateLimitError> {
        self.total_calls.fetch_add(1, Ordering::Relaxed);
        if self.limiters.is_empty() {
            return Err(RateLimitError::Internal(
                "No limiters configured".to_string(),
            ));
        }
        match self.strategy {
            CompositeStrategy::AllMustAllow => self.check_all_must_allow(key),
            CompositeStrategy::AnyCanAllow => self.check_any_can_allow(key),
            CompositeStrategy::FirstRejectWins => self.check_first_reject_wins(key),
            CompositeStrategy::WeightedVote => self.check_weighted_vote(key),
        }
    }

    fn check_all_must_allow(&self, key: &str) -> Result<RateLimitResult, RateLimitError> {
        let mut min_remaining = u64::MAX;
        let mut max_reset = 0i64;
        for limiter in &self.limiters {
            let result = limiter.acquire(key)?;
            if !result.allowed {
                return Ok(result);
            }
            min_remaining = min_remaining.min(result.remaining);
            max_reset = max_reset.max(result.reset_at);
        }
        Ok(RateLimitResult::allowed(min_remaining, max_reset))
    }

    fn check_any_can_allow(&self, key: &str) -> Result<RateLimitResult, RateLimitError> {
        let mut best: Option<RateLimitResult> = None;
        for limiter in &self.limiters {
            let result = limiter.acquire(key)?;
            if result.allowed {
                return Ok(result);
            }
            match &best {
                None => best = Some(result),
                Some(current) => {
                    if result.remaining > current.remaining {
                        best = Some(result);
                    }
                }
            }
        }
        best.ok_or_else(|| RateLimitError::Internal("No results".to_string()))
    }

    fn check_first_reject_wins(&self, key: &str) -> Result<RateLimitResult, RateLimitError> {
        let mut last_allowed: Option<RateLimitResult> = None;
        for limiter in &self.limiters {
            let result = limiter.acquire(key)?;
            if !result.allowed {
                return Ok(result);
            }
            last_allowed = Some(result);
        }
        last_allowed.ok_or_else(|| RateLimitError::Internal("No results".to_string()))
    }

    fn check_weighted_vote(&self, key: &str) -> Result<RateLimitResult, RateLimitError> {
        let mut allow_weight = 0u32;
        let mut reject_weight = 0u32;
        let mut best_allowed: Option<RateLimitResult> = None;
        let mut best_rejected: Option<RateLimitResult> = None;
        for (i, limiter) in self.limiters.iter().enumerate() {
            let result = limiter.acquire(key)?;
            let weight = self.weights.get(i).copied().unwrap_or(1);
            if result.allowed {
                allow_weight += weight;
                if best_allowed.is_none() {
                    best_allowed = Some(result);
                }
            } else {
                reject_weight += weight;
                if best_rejected.is_none() {
                    best_rejected = Some(result);
                }
            }
        }
        if allow_weight >= reject_weight {
            best_allowed.ok_or_else(|| RateLimitError::Internal("No allowed".to_string()))
        } else {
            best_rejected.ok_or_else(|| RateLimitError::Internal("No rejected".to_string()))
        }
    }

    /// 总调用次数
    pub fn total_calls(&self) -> u64 {
        self.total_calls.load(Ordering::Relaxed)
    }
}

/// 回退限流器
///
/// 主限流器失败时回退到备用限流器。
pub struct FallbackLimiter {
    primary: Arc<dyn RateLimiter>,
    fallback: Arc<dyn RateLimiter>,
    fallback_count: AtomicU64,
}

impl FallbackLimiter {
    /// 创建回退限流器
    pub fn new(primary: Arc<dyn RateLimiter>, fallback: Arc<dyn RateLimiter>) -> Self {
        Self {
            primary,
            fallback,
            fallback_count: AtomicU64::new(0),
        }
    }

    /// 检查，主限流器出错时回退
    pub fn check(&self, key: &str) -> Result<RateLimitResult, RateLimitError> {
        match self.primary.acquire(key) {
            Ok(result) => Ok(result),
            Err(_) => {
                self.fallback_count.fetch_add(1, Ordering::Relaxed);
                self.fallback.acquire(key)
            }
        }
    }

    /// 回退次数
    pub fn fallback_count(&self) -> u64 {
        self.fallback_count.load(Ordering::Relaxed)
    }
}

/// 限流键构建器
///
/// 从多个维度构建限流键，如 IP + 用户 + API。
#[derive(Debug, Clone)]
pub struct LimitKeyBuilder {
    parts: Vec<String>,
    separator: String,
}

impl Default for LimitKeyBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl LimitKeyBuilder {
    /// 创建构建器
    pub fn new() -> Self {
        Self {
            parts: Vec::new(),
            separator: ":".to_string(),
        }
    }

    /// 设置分隔符
    pub fn with_separator(mut self, sep: &str) -> Self {
        self.separator = sep.to_string();
        self
    }

    /// 添加 IP 维度
    pub fn ip(mut self, ip: &str) -> Self {
        self.parts.push(format!("ip:{}", ip));
        self
    }

    /// 添加用户维度
    pub fn user(mut self, user: &str) -> Self {
        self.parts.push(format!("user:{}", user));
        self
    }

    /// 添加 API 维度
    pub fn api(mut self, api: &str) -> Self {
        self.parts.push(format!("api:{}", api));
        self
    }

    /// 添加自定义维度
    pub fn dimension(mut self, name: &str, value: &str) -> Self {
        self.parts.push(format!("{}:{}", name, value));
        self
    }

    /// 构建限流键
    pub fn build(&self) -> String {
        self.parts.join(&self.separator)
    }

    /// 部分数量
    pub fn part_count(&self) -> usize {
        self.parts.len()
    }
}

/// 限流规则
///
/// 描述一条限流规则，包括匹配条件和限流参数。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RateLimitRule {
    /// 规则名称
    pub name: String,
    /// 匹配的 key 前缀
    pub key_prefix: String,
    /// 限流器类型
    pub limiter_type: String,
    /// 容量
    pub capacity: u64,
    /// 窗口大小（毫秒）
    pub window_ms: u64,
    /// 是否启用
    pub enabled: bool,
}

impl RateLimitRule {
    /// 创建规则
    pub fn new(name: &str, key_prefix: &str, limiter_type: &str, capacity: u64) -> Self {
        Self {
            name: name.to_string(),
            key_prefix: key_prefix.to_string(),
            limiter_type: limiter_type.to_string(),
            capacity,
            window_ms: 1000,
            enabled: true,
        }
    }

    /// 设置窗口大小
    pub fn with_window_ms(mut self, ms: u64) -> Self {
        self.window_ms = ms;
        self
    }

    /// 禁用
    pub fn disable(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// 检查 key 是否匹配
    pub fn matches(&self, key: &str) -> bool {
        self.enabled && key.starts_with(&self.key_prefix)
    }
}

/// 规则集
pub struct RuleSet {
    rules: Vec<RateLimitRule>,
}

impl RuleSet {
    /// 创建规则集
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// 添加规则
    pub fn add(&mut self, rule: RateLimitRule) -> &mut Self {
        self.rules.push(rule);
        self
    }

    /// 查找匹配的规则
    pub fn find_matches(&self, key: &str) -> Vec<&RateLimitRule> {
        self.rules.iter().filter(|r| r.matches(key)).collect()
    }

    /// 规则数量
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// 启用的规则数
    pub fn enabled_count(&self) -> usize {
        self.rules.iter().filter(|r| r.enabled).count()
    }

    /// 按前缀查找
    pub fn find_by_prefix(&self, prefix: &str) -> Option<&RateLimitRule> {
        self.rules.iter().find(|r| r.key_prefix == prefix)
    }

    /// 禁用规则
    pub fn disable(&mut self, name: &str) -> bool {
        for rule in &mut self.rules {
            if rule.name == name {
                rule.enabled = false;
                return true;
            }
        }
        false
    }

    /// 启用规则
    pub fn enable(&mut self, name: &str) -> bool {
        for rule in &mut self.rules {
            if rule.name == name {
                rule.enabled = true;
                return true;
            }
        }
        false
    }
}

impl Default for RuleSet {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use crate::{SlidingWindowRateLimiter, TokenBucketRateLimiter};

    #[test]
    fn test_composite_all_must_allow() {
        let l1 = Arc::new(SlidingWindowRateLimiter::new(10, Duration::from_secs(60)));
        let l2 = Arc::new(TokenBucketRateLimiter::new(10, 1.0));
        let composite = CompositeLimiter::new(vec![l1, l2], CompositeStrategy::AllMustAllow);
        let r = composite.check("k").unwrap();
        assert!(r.allowed);
    }

    #[test]
    fn test_composite_all_must_allow_one_rejects() {
        let l1 = Arc::new(SlidingWindowRateLimiter::new(10, Duration::from_secs(60)));
        let l2 = Arc::new(TokenBucketRateLimiter::new(1, 1.0));
        let composite = CompositeLimiter::new(vec![l1, l2], CompositeStrategy::AllMustAllow);
        composite.check("k").unwrap();
        let r = composite.check("k").unwrap();
        assert!(!r.allowed);
    }

    #[test]
    fn test_composite_any_can_allow() {
        let l1 = Arc::new(SlidingWindowRateLimiter::new(1, Duration::from_secs(60)));
        let l2 = Arc::new(TokenBucketRateLimiter::new(10, 1.0));
        let composite = CompositeLimiter::new(vec![l1, l2], CompositeStrategy::AnyCanAllow);
        composite.check("k").unwrap();
        let r = composite.check("k").unwrap();
        assert!(r.allowed);
    }

    #[test]
    fn test_composite_any_can_allow_all_reject() {
        let l1 = Arc::new(SlidingWindowRateLimiter::new(1, Duration::from_secs(60)));
        let l2 = Arc::new(TokenBucketRateLimiter::new(1, 0.0));
        // 先单独消耗 l2，使 AnyCanAllow 第二次 check 时两个都满
        l2.acquire("k").unwrap();
        let composite = CompositeLimiter::new(vec![l1, l2], CompositeStrategy::AnyCanAllow);
        composite.check("k").unwrap();
        let r = composite.check("k").unwrap();
        assert!(!r.allowed);
    }

    #[test]
    fn test_composite_first_reject_wins() {
        let l1 = Arc::new(SlidingWindowRateLimiter::new(1, Duration::from_secs(60)));
        let l2 = Arc::new(TokenBucketRateLimiter::new(10, 1.0));
        let composite = CompositeLimiter::new(vec![l1, l2], CompositeStrategy::FirstRejectWins);
        composite.check("k").unwrap();
        let r = composite.check("k").unwrap();
        assert!(!r.allowed);
    }

    #[test]
    fn test_composite_weighted_vote_allow() {
        let l1 = Arc::new(SlidingWindowRateLimiter::new(10, Duration::from_secs(60)));
        let l2 = Arc::new(TokenBucketRateLimiter::new(1, 1.0));
        let composite = CompositeLimiter::new(vec![l1, l2], CompositeStrategy::WeightedVote)
            .with_weights(vec![3, 1]);
        composite.check("k").unwrap();
        let r = composite.check("k").unwrap();
        assert!(r.allowed);
    }

    #[test]
    fn test_composite_empty_errors() {
        let composite = CompositeLimiter::new(vec![], CompositeStrategy::AllMustAllow);
        assert!(composite.check("k").is_err());
    }

    #[test]
    fn test_composite_with_limiter() {
        let l1 = Arc::new(SlidingWindowRateLimiter::new(10, Duration::from_secs(60)));
        let composite =
            CompositeLimiter::new(vec![], CompositeStrategy::AllMustAllow).with_limiter(l1);
        assert_eq!(composite.limiter_count(), 1);
        assert!(composite.check("k").unwrap().allowed);
    }

    #[test]
    fn test_composite_total_calls() {
        let l1 = Arc::new(SlidingWindowRateLimiter::new(10, Duration::from_secs(60)));
        let composite = CompositeLimiter::new(vec![l1], CompositeStrategy::AllMustAllow);
        composite.check("k").unwrap();
        composite.check("k").unwrap();
        assert_eq!(composite.total_calls(), 2);
    }

    #[test]
    fn test_fallback_primary_ok() {
        let primary = Arc::new(SlidingWindowRateLimiter::new(10, Duration::from_secs(60)));
        let fallback = Arc::new(TokenBucketRateLimiter::new(10, 1.0));
        let limiter = FallbackLimiter::new(primary, fallback);
        let r = limiter.check("k").unwrap();
        assert!(r.allowed);
        assert_eq!(limiter.fallback_count(), 0);
    }

    #[test]
    fn test_limit_key_builder_basic() {
        let key = LimitKeyBuilder::new()
            .ip("127.0.0.1")
            .user("user-1")
            .api("/query")
            .build();
        assert!(key.contains("ip:127.0.0.1"));
        assert!(key.contains("user:user-1"));
        assert!(key.contains("api:/query"));
    }

    #[test]
    fn test_limit_key_builder_separator() {
        let key = LimitKeyBuilder::new()
            .with_separator("|")
            .ip("127.0.0.1")
            .user("user-1")
            .build();
        assert!(key.contains("|"));
    }

    #[test]
    fn test_limit_key_builder_dimension() {
        let key = LimitKeyBuilder::new().dimension("tenant", "acme").build();
        assert!(key.contains("tenant:acme"));
    }

    #[test]
    fn test_limit_key_builder_part_count() {
        let builder = LimitKeyBuilder::new().ip("127.0.0.1").user("user-1");
        assert_eq!(builder.part_count(), 2);
    }

    #[test]
    fn test_rate_limit_rule_new() {
        let rule = RateLimitRule::new("ip-limit", "ip:", "sliding_window", 100);
        assert_eq!(rule.name, "ip-limit");
        assert_eq!(rule.capacity, 100);
        assert!(rule.enabled);
    }

    #[test]
    fn test_rate_limit_rule_matches() {
        let rule = RateLimitRule::new("ip-limit", "ip:", "sliding_window", 100);
        assert!(rule.matches("ip:127.0.0.1"));
        assert!(!rule.matches("user:1"));
    }

    #[test]
    fn test_rate_limit_rule_disabled() {
        let rule = RateLimitRule::new("ip-limit", "ip:", "sliding_window", 100).disable();
        assert!(!rule.matches("ip:127.0.0.1"));
    }

    #[test]
    fn test_rate_limit_rule_with_window() {
        let rule =
            RateLimitRule::new("ip-limit", "ip:", "sliding_window", 100).with_window_ms(5000);
        assert_eq!(rule.window_ms, 5000);
    }

    #[test]
    fn test_rule_set_add() {
        let mut set = RuleSet::new();
        set.add(RateLimitRule::new("r1", "ip:", "sw", 100));
        assert_eq!(set.rule_count(), 1);
    }

    #[test]
    fn test_rule_set_find_matches() {
        let mut set = RuleSet::new();
        set.add(RateLimitRule::new("r1", "ip:", "sw", 100));
        set.add(RateLimitRule::new("r2", "user:", "sw", 200));
        let matches = set.find_matches("ip:127.0.0.1");
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_rule_set_enabled_count() {
        let mut set = RuleSet::new();
        set.add(RateLimitRule::new("r1", "ip:", "sw", 100));
        set.add(RateLimitRule::new("r2", "user:", "sw", 200).disable());
        assert_eq!(set.enabled_count(), 1);
    }

    #[test]
    fn test_rule_set_find_by_prefix() {
        let mut set = RuleSet::new();
        set.add(RateLimitRule::new("r1", "ip:", "sw", 100));
        assert!(set.find_by_prefix("ip:").is_some());
        assert!(set.find_by_prefix("user:").is_none());
    }

    #[test]
    fn test_rule_set_disable_enable() {
        let mut set = RuleSet::new();
        set.add(RateLimitRule::new("r1", "ip:", "sw", 100));
        assert!(set.disable("r1"));
        assert!(!set.disable("nonexistent"));
        assert_eq!(set.enabled_count(), 0);
        assert!(set.enable("r1"));
        assert_eq!(set.enabled_count(), 1);
    }

    #[test]
    fn test_composite_strategy_serde() {
        let s = CompositeStrategy::AllMustAllow;
        let json = serde_json::to_string(&s).unwrap();
        let back: CompositeStrategy = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }
}
