//! WasmDbRateLimiter — QPS 限流
//!
//! 基于固定窗口计数器，默认 100 QPS。

use std::time::{Duration, Instant};

/// WASM DB 限流器
///
/// 固定窗口计数器算法，在 `window` 时间内最多允许 `max_qps` 次请求。
#[derive(Debug, Clone)]
pub struct WasmDbRateLimiter {
    max_qps: u32,
    window: Duration,
    current_count: u32,
    window_start: Instant,
}

impl WasmDbRateLimiter {
    /// 创建限流器，指定 QPS 上限（1 秒窗口）
    pub fn new(max_qps: u32) -> Self {
        Self {
            max_qps,
            window: Duration::from_secs(1),
            current_count: 0,
            window_start: Instant::now(),
        }
    }

    /// 创建限流器，指定 QPS 上限和窗口大小
    pub fn with_window(max_qps: u32, window: Duration) -> Self {
        Self {
            max_qps,
            window,
            current_count: 0,
            window_start: Instant::now(),
        }
    }

    /// 检查并增加计数
    ///
    /// 返回 true 表示允许，false 表示被限流。
    pub fn check_and_increment(&mut self) -> bool {
        let now = Instant::now();
        if now.duration_since(self.window_start) >= self.window {
            self.window_start = now;
            self.current_count = 0;
        }

        if self.current_count >= self.max_qps {
            return false;
        }

        self.current_count += 1;
        true
    }

    /// 当前窗口已用配额
    pub fn current_count(&self) -> u32 {
        self.current_count
    }

    /// QPS 上限
    pub fn max_qps(&self) -> u32 {
        self.max_qps
    }

    /// 重置计数器
    pub fn reset(&mut self) {
        self.current_count = 0;
        self.window_start = Instant::now();
    }

    /// 剩余配额
    pub fn remaining(&self) -> u32 {
        self.max_qps.saturating_sub(self.current_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_rate_limiter() {
        let rl = WasmDbRateLimiter::new(100);
        assert_eq!(rl.max_qps(), 100);
        assert_eq!(rl.current_count(), 0);
        assert_eq!(rl.remaining(), 100);
    }

    #[test]
    fn test_allow_within_limit() {
        let mut rl = WasmDbRateLimiter::new(5);
        for _ in 0..5 {
            assert!(rl.check_and_increment());
        }
        assert_eq!(rl.current_count(), 5);
        assert_eq!(rl.remaining(), 0);
    }

    #[test]
    fn test_reject_over_limit() {
        let mut rl = WasmDbRateLimiter::new(3);
        assert!(rl.check_and_increment());
        assert!(rl.check_and_increment());
        assert!(rl.check_and_increment());
        assert!(!rl.check_and_increment());
        assert!(!rl.check_and_increment());
    }

    #[test]
    fn test_reset() {
        let mut rl = WasmDbRateLimiter::new(2);
        rl.check_and_increment();
        rl.check_and_increment();
        assert!(!rl.check_and_increment());
        rl.reset();
        assert_eq!(rl.current_count(), 0);
        assert!(rl.check_and_increment());
    }

    #[test]
    fn test_remaining() {
        let mut rl = WasmDbRateLimiter::new(10);
        assert_eq!(rl.remaining(), 10);
        rl.check_and_increment();
        rl.check_and_increment();
        assert_eq!(rl.remaining(), 8);
    }

    #[test]
    fn test_zero_qps() {
        let mut rl = WasmDbRateLimiter::new(0);
        assert!(!rl.check_and_increment());
    }

    #[test]
    fn test_with_window() {
        let mut rl = WasmDbRateLimiter::with_window(2, Duration::from_millis(100));
        assert!(rl.check_and_increment());
        assert!(rl.check_and_increment());
        assert!(!rl.check_and_increment());
    }
}
