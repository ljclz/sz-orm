//! 限流器抽象（P1-4：抽象提升到核心层，供连接池执行路径集成）
//!
//! 连接池在 `rate-limit` feature 下通过 `set_rate_limiter()` 配置限流器，
//! 获取连接前调用 [`RateLimiter::try_acquire`] 检查是否放行。
//!
//! 本模块为自包含抽象（不依赖 sz-orm-limit），消除核心层对上层 crate 的反向依赖；
//! 具体实现（滑动窗口/令牌桶等）由 sz-orm-limit 提供并实现本 trait。

use std::fmt;

/// 限流判定结果。
#[derive(Debug, Clone)]
pub struct RateLimitResult {
    /// 是否放行。
    pub allowed: bool,
    /// 放行后剩余配额。
    pub remaining: u64,
    /// 配额重置时间戳（Unix 毫秒）。
    pub reset_at: i64,
}

impl RateLimitResult {
    /// 放行结果。
    pub fn allowed(remaining: u64, reset_at: i64) -> Self {
        Self {
            allowed: true,
            remaining,
            reset_at,
        }
    }

    /// 拒绝结果。
    pub fn rejected(remaining: u64, reset_at: i64) -> Self {
        Self {
            allowed: false,
            remaining,
            reset_at,
        }
    }
}

/// 限流器内部错误（如后端不可用）。
///
/// 连接池对内部错误采取保守放行策略（避免误杀正常请求）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateLimitError(pub String);

impl fmt::Display for RateLimitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "rate limiter error: {}", self.0)
    }
}

impl std::error::Error for RateLimitError {}

/// 限流器抽象 trait。
pub trait RateLimiter: Send + Sync {
    /// 尝试获取 `key` 的配额。`Ok(result)` 中 `result.allowed` 为 `false` 时拒绝放行。
    fn try_acquire(&self, key: &str) -> Result<RateLimitResult, RateLimitError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_result_allowed() {
        let r = RateLimitResult::allowed(9, 1_700_000_000_000);
        assert!(r.allowed);
        assert_eq!(r.remaining, 9);
    }

    #[test]
    fn test_rate_limit_result_rejected() {
        let r = RateLimitResult::rejected(0, 1_700_000_000_000);
        assert!(!r.allowed);
        assert_eq!(r.remaining, 0);
    }

    #[test]
    fn test_rate_limit_error_display() {
        let e = RateLimitError("backend down".to_string());
        assert!(e.to_string().contains("backend down"));
    }

    /// 桩实现：验证 trait 可被外部实现并驱动 try_acquire。
    struct AlwaysAllow;

    impl RateLimiter for AlwaysAllow {
        fn try_acquire(&self, _key: &str) -> Result<RateLimitResult, RateLimitError> {
            Ok(RateLimitResult::allowed(1, 0))
        }
    }

    #[test]
    fn test_trait_dispatch() {
        let limiter: &dyn RateLimiter = &AlwaysAllow;
        let r = limiter.try_acquire("test-key").unwrap();
        assert!(r.allowed);
    }
}
