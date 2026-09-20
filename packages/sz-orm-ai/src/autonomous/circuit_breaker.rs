//! 自治熔断器

use std::time::Instant;

use super::types::{CircuitBreakerConfig, CircuitRuntimeState, CircuitState};

/// 自治熔断器
#[derive(Debug, Clone)]
pub struct AutonomousCircuitBreaker {
    config: CircuitBreakerConfig,
    state: CircuitRuntimeState,
}

impl AutonomousCircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            state: CircuitRuntimeState::default(),
        }
    }

    pub fn state(&self) -> &CircuitState {
        &self.state.state
    }

    pub fn failure_count(&self) -> u32 {
        self.state.failure_count
    }

    /// 检查是否允许执行
    pub fn allow_request(&mut self) -> bool {
        match self.state.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                if let Some(opened_at) = self.state.opened_at {
                    if opened_at.elapsed() >= self.config.break_duration {
                        self.state.state = CircuitState::HalfOpen;
                        return true;
                    }
                }
                false
            }
            CircuitState::HalfOpen => true,
        }
    }

    /// 记录成功
    pub fn record_success(&mut self) {
        self.state.failure_count = 0;
        self.state.state = CircuitState::Closed;
        self.state.opened_at = None;
    }

    /// 记录失败
    pub fn record_failure(&mut self) {
        self.state.failure_count += 1;
        self.state.last_failure_time = Some(Instant::now());
        if self.state.failure_count >= self.config.failure_threshold {
            self.state.state = CircuitState::Open;
            self.state.opened_at = Some(Instant::now());
        }
    }

    /// 手动熔断
    pub fn manual_break(&mut self) {
        self.state.state = CircuitState::Open;
        self.state.opened_at = Some(Instant::now());
    }

    /// 手动恢复
    pub fn manual_reset(&mut self) {
        self.state.failure_count = 0;
        self.state.state = CircuitState::Closed;
        self.state.opened_at = None;
        self.state.last_failure_time = None;
    }
}

impl Default for AutonomousCircuitBreaker {
    fn default() -> Self {
        Self::new(CircuitBreakerConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_circuit_breaker_starts_closed() {
        let cb = AutonomousCircuitBreaker::default();
        assert_eq!(*cb.state(), CircuitState::Closed);
        assert_eq!(cb.failure_count(), 0);
    }

    #[test]
    fn test_circuit_breaker_opens_after_threshold() {
        let mut cb = AutonomousCircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 3,
            break_duration: Duration::from_secs(60),
        });
        cb.record_failure();
        cb.record_failure();
        assert_eq!(*cb.state(), CircuitState::Closed);
        cb.record_failure();
        assert_eq!(*cb.state(), CircuitState::Open);
    }

    #[test]
    fn test_circuit_breaker_blocks_when_open() {
        let mut cb = AutonomousCircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            break_duration: Duration::from_secs(60),
        });
        cb.record_failure();
        assert_eq!(*cb.state(), CircuitState::Open);
        assert!(!cb.allow_request());
    }

    #[test]
    fn test_circuit_breaker_half_open_after_duration() {
        let mut cb = AutonomousCircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            break_duration: Duration::from_millis(1),
        });
        cb.record_failure();
        std::thread::sleep(Duration::from_millis(10));
        assert!(cb.allow_request());
        assert_eq!(*cb.state(), CircuitState::HalfOpen);
    }

    #[test]
    fn test_circuit_breaker_success_resets() {
        let mut cb = AutonomousCircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 3,
            break_duration: Duration::from_secs(60),
        });
        cb.record_failure();
        cb.record_failure();
        cb.record_success();
        assert_eq!(cb.failure_count(), 0);
        assert_eq!(*cb.state(), CircuitState::Closed);
    }

    #[test]
    fn test_manual_break_and_reset() {
        let mut cb = AutonomousCircuitBreaker::default();
        cb.manual_break();
        assert_eq!(*cb.state(), CircuitState::Open);
        assert!(!cb.allow_request());
        cb.manual_reset();
        assert_eq!(*cb.state(), CircuitState::Closed);
        assert!(cb.allow_request());
    }
}
