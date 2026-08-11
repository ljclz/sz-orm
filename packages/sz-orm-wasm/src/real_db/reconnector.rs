//! WasmRealDbReconnector — 指数退避重连

use std::time::Duration;

/// WASM 真实 DB 重连器
///
/// 指数退避策略：delay = min(base * 2^attempt, max_delay)
#[derive(Debug, Clone)]
pub struct WasmRealDbReconnector {
    max_retries: u32,
    base_delay: Duration,
    max_delay: Duration,
    current_attempt: u32,
}

impl WasmRealDbReconnector {
    /// 创建重连器
    ///
    /// - `max_retries`: 最大重试次数
    /// - `base_delay`: 基础延迟（首次重试延迟）
    /// - `max_delay`: 最大延迟上限
    pub fn new(max_retries: u32, base_delay: Duration, max_delay: Duration) -> Self {
        Self {
            max_retries,
            base_delay,
            max_delay,
            current_attempt: 0,
        }
    }

    /// 创建默认重连器（5 次重试，100ms 基础，10s 上限）
    pub fn default_config() -> Self {
        Self::new(5, Duration::from_millis(100), Duration::from_secs(10))
    }

    /// 是否还有重试机会
    pub fn can_retry(&self) -> bool {
        self.current_attempt < self.max_retries
    }

    /// 计算下次重试延迟并增加计数
    ///
    /// 返回 None 表示已用完重试次数。
    pub fn next_delay(&mut self) -> Option<Duration> {
        if !self.can_retry() {
            return None;
        }

        let multiplier = 2u64.saturating_pow(self.current_attempt);
        let delay_ms = self.base_delay.as_millis() as u64 * multiplier;
        let delay = Duration::from_millis(delay_ms);
        let clamped = if delay > self.max_delay {
            self.max_delay
        } else {
            delay
        };

        self.current_attempt += 1;
        Some(clamped)
    }

    /// 当前尝试次数
    pub fn current_attempt(&self) -> u32 {
        self.current_attempt
    }

    /// 最大重试次数
    pub fn max_retries(&self) -> u32 {
        self.max_retries
    }

    /// 重置重连器
    pub fn reset(&mut self) {
        self.current_attempt = 0;
    }

    /// 基础延迟
    pub fn base_delay(&self) -> Duration {
        self.base_delay
    }

    /// 最大延迟
    pub fn max_delay(&self) -> Duration {
        self.max_delay
    }
}

impl Default for WasmRealDbReconnector {
    fn default() -> Self {
        Self::default_config()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_reconnector() {
        let r = WasmRealDbReconnector::new(3, Duration::from_millis(100), Duration::from_secs(5));
        assert_eq!(r.max_retries(), 3);
        assert_eq!(r.current_attempt(), 0);
        assert!(r.can_retry());
    }

    #[test]
    fn test_default_config() {
        let r = WasmRealDbReconnector::default_config();
        assert_eq!(r.max_retries(), 5);
        assert_eq!(r.base_delay(), Duration::from_millis(100));
        assert_eq!(r.max_delay(), Duration::from_secs(10));
    }

    #[test]
    fn test_exponential_backoff() {
        let mut r =
            WasmRealDbReconnector::new(4, Duration::from_millis(100), Duration::from_secs(10));

        let d0 = r.next_delay().unwrap();
        let d1 = r.next_delay().unwrap();
        let d2 = r.next_delay().unwrap();
        let d3 = r.next_delay().unwrap();

        assert_eq!(d0, Duration::from_millis(100));
        assert_eq!(d1, Duration::from_millis(200));
        assert_eq!(d2, Duration::from_millis(400));
        assert_eq!(d3, Duration::from_millis(800));
        assert_eq!(r.current_attempt(), 4);
    }

    #[test]
    fn test_max_delay_clamp() {
        let mut r =
            WasmRealDbReconnector::new(10, Duration::from_millis(100), Duration::from_millis(500));

        let d0 = r.next_delay().unwrap();
        let d1 = r.next_delay().unwrap();
        let d2 = r.next_delay().unwrap();
        let d3 = r.next_delay().unwrap();

        assert_eq!(d0, Duration::from_millis(100));
        assert_eq!(d1, Duration::from_millis(200));
        assert_eq!(d2, Duration::from_millis(400));
        assert_eq!(d3, Duration::from_millis(500));
    }

    #[test]
    fn test_retries_exhausted() {
        let mut r =
            WasmRealDbReconnector::new(2, Duration::from_millis(10), Duration::from_secs(1));

        assert!(r.next_delay().is_some());
        assert!(r.next_delay().is_some());
        assert!(r.next_delay().is_none());
        assert!(!r.can_retry());
    }

    #[test]
    fn test_reset() {
        let mut r =
            WasmRealDbReconnector::new(3, Duration::from_millis(10), Duration::from_secs(1));
        r.next_delay();
        r.next_delay();
        assert_eq!(r.current_attempt(), 2);
        r.reset();
        assert_eq!(r.current_attempt(), 0);
        assert!(r.can_retry());
    }

    #[test]
    fn test_zero_retries() {
        let mut r =
            WasmRealDbReconnector::new(0, Duration::from_millis(10), Duration::from_secs(1));
        assert!(!r.can_retry());
        assert!(r.next_delay().is_none());
    }

    #[test]
    fn test_default_trait() {
        let r = WasmRealDbReconnector::default();
        assert_eq!(r.max_retries(), 5);
    }
}
