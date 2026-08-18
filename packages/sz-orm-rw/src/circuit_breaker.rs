//! 断路器模式
//!
//! 当 slave 持续失败时熔断（Open），一段时间后半开（HalfOpen）尝试恢复，
//! 成功后恢复为 Closed。三种状态间转换由 [`CircuitBreaker`] 管理。
//!
//! ```rust
//! use sz_orm_rw::circuit_breaker::{CircuitBreaker, CircuitState};
//! let cb = CircuitBreaker::new(5, std::time::Duration::from_secs(30));
//! assert_eq!(cb.state("s1"), CircuitState::Closed);
//! ```

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 断路器状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// 关闭（正常放行）
    Closed,
    /// 打开（熔断中，拒绝请求）
    Open,
    /// 半开（尝试恢复中，放行少量请求）
    HalfOpen,
}

impl CircuitState {
    /// 状态名（用于日志/监控）
    pub fn as_str(&self) -> &'static str {
        match self {
            CircuitState::Closed => "closed",
            CircuitState::Open => "open",
            CircuitState::HalfOpen => "half_open",
        }
    }

    /// 是否允许请求通过
    pub fn allows_request(&self) -> bool {
        matches!(self, CircuitState::Closed | CircuitState::HalfOpen)
    }
}

/// 断路器配置
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// 连续失败次数阈值，达到后从 Closed 转为 Open
    pub failure_threshold: u32,
    /// Open 状态持续时间，过后转为 HalfOpen
    pub recovery_timeout: Duration,
    /// HalfOpen 状态下允许的试探请求数
    pub half_open_max_calls: u32,
    /// HalfOpen 状态下连续成功次数阈值，达到后转为 Closed
    pub success_threshold: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            recovery_timeout: Duration::from_secs(30),
            half_open_max_calls: 3,
            success_threshold: 2,
        }
    }
}

impl CircuitBreakerConfig {
    /// 创建配置
    pub fn new(failure_threshold: u32, recovery_timeout: Duration) -> Self {
        Self {
            failure_threshold,
            recovery_timeout,
            ..Default::default()
        }
    }

    /// 设置半开状态最大试探请求数
    pub fn with_half_open_max_calls(mut self, n: u32) -> Self {
        self.half_open_max_calls = n.max(1);
        self
    }

    /// 设置恢复成功阈值
    pub fn with_success_threshold(mut self, n: u32) -> Self {
        self.success_threshold = n.max(1);
        self
    }
}

/// 单个 slave 的断路器内部状态
struct SlaveCircuit {
    state: CircuitState,
    failure_count: u32,
    success_count: u32,
    half_open_calls: u32,
    opened_at: Option<Instant>,
}

impl SlaveCircuit {
    fn new() -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            success_count: 0,
            half_open_calls: 0,
            opened_at: None,
        }
    }
}

/// 断路器：为每个 slave 维护独立的熔断状态
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    circuits: Mutex<HashMap<String, SlaveCircuit>>,
    /// 总请求计数（监控用）
    total_requests: AtomicU64,
    /// 被熔断拒绝的请求计数
    rejected_requests: AtomicU64,
}

impl CircuitBreaker {
    /// 创建断路器
    pub fn new(failure_threshold: u32, recovery_timeout: Duration) -> Self {
        Self::with_config(CircuitBreakerConfig::new(
            failure_threshold,
            recovery_timeout,
        ))
    }

    /// 用配置创建断路器
    pub fn with_config(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            circuits: Mutex::new(HashMap::new()),
            total_requests: AtomicU64::new(0),
            rejected_requests: AtomicU64::new(0),
        }
    }

    /// 使用默认配置创建
    pub fn with_defaults() -> Self {
        Self::with_config(CircuitBreakerConfig::default())
    }

    /// 注册一个 slave
    pub fn register(&self, slave: &str) {
        if let Ok(mut circuits) = self.circuits.lock() {
            circuits
                .entry(slave.to_string())
                .or_insert_with(SlaveCircuit::new);
        }
    }

    /// 查询 slave 的当前断路器状态
    pub fn state(&self, slave: &str) -> CircuitState {
        match self.circuits.lock() {
            Ok(circuits) => {
                let c = circuits.get(slave);
                match c {
                    Some(sc) => {
                        if sc.state == CircuitState::Open {
                            if let Some(opened_at) = sc.opened_at {
                                if opened_at.elapsed() >= self.config.recovery_timeout {
                                    return CircuitState::HalfOpen;
                                }
                            }
                        }
                        sc.state
                    }
                    None => CircuitState::Closed,
                }
            }
            Err(_) => CircuitState::Closed,
        }
    }

    /// 判断是否允许请求通过
    ///
    /// 返回 true 表示放行，false 表示熔断中应拒绝。
    /// 在 HalfOpen 状态下限制试探请求数。
    pub fn allow_request(&self, slave: &str) -> bool {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        let state = self.state(slave);
        if !state.allows_request() {
            self.rejected_requests.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        if state == CircuitState::HalfOpen {
            if let Ok(mut circuits) = self.circuits.lock() {
                if let Some(sc) = circuits.get_mut(slave) {
                    if sc.half_open_calls >= self.config.half_open_max_calls {
                        self.rejected_requests.fetch_add(1, Ordering::Relaxed);
                        return false;
                    }
                    sc.half_open_calls += 1;
                }
            }
        }
        true
    }

    /// 记录一次成功
    ///
    /// 在 HalfOpen 状态下累计成功，达到阈值后转为 Closed。
    pub fn record_success(&self, slave: &str) {
        if let Ok(mut circuits) = self.circuits.lock() {
            let sc = circuits
                .entry(slave.to_string())
                .or_insert_with(SlaveCircuit::new);
            // 检查 Open 超时，转为 HalfOpen
            if sc.state == CircuitState::Open {
                if let Some(opened_at) = sc.opened_at {
                    if opened_at.elapsed() >= self.config.recovery_timeout {
                        sc.state = CircuitState::HalfOpen;
                        sc.success_count = 0;
                        sc.half_open_calls = 0;
                    }
                }
            }
            match sc.state {
                CircuitState::Closed => {
                    sc.failure_count = 0;
                }
                CircuitState::HalfOpen => {
                    sc.success_count += 1;
                    if sc.success_count >= self.config.success_threshold {
                        sc.state = CircuitState::Closed;
                        sc.failure_count = 0;
                        sc.success_count = 0;
                        sc.half_open_calls = 0;
                        sc.opened_at = None;
                    }
                }
                CircuitState::Open => {}
            }
        }
    }

    /// 记录一次失败
    ///
    /// 在 Closed 状态下累计失败，达到阈值后转为 Open。
    /// 在 HalfOpen 状态下任何失败都立即转为 Open。
    pub fn record_failure(&self, slave: &str) {
        if let Ok(mut circuits) = self.circuits.lock() {
            let sc = circuits
                .entry(slave.to_string())
                .or_insert_with(SlaveCircuit::new);
            // 检查 Open 超时，转为 HalfOpen
            if sc.state == CircuitState::Open {
                if let Some(opened_at) = sc.opened_at {
                    if opened_at.elapsed() >= self.config.recovery_timeout {
                        sc.state = CircuitState::HalfOpen;
                        sc.success_count = 0;
                        sc.half_open_calls = 0;
                    }
                }
            }
            match sc.state {
                CircuitState::Closed => {
                    sc.failure_count += 1;
                    if sc.failure_count >= self.config.failure_threshold {
                        sc.state = CircuitState::Open;
                        sc.opened_at = Some(Instant::now());
                    }
                }
                CircuitState::HalfOpen => {
                    sc.state = CircuitState::Open;
                    sc.opened_at = Some(Instant::now());
                    sc.success_count = 0;
                    sc.half_open_calls = 0;
                }
                CircuitState::Open => {}
            }
        }
    }

    /// 手动重置 slave 的断路器为 Closed
    pub fn reset(&self, slave: &str) {
        if let Ok(mut circuits) = self.circuits.lock() {
            if let Some(sc) = circuits.get_mut(slave) {
                sc.state = CircuitState::Closed;
                sc.failure_count = 0;
                sc.success_count = 0;
                sc.half_open_calls = 0;
                sc.opened_at = None;
            }
        }
    }

    /// 强制打开断路器（手动熔断）
    pub fn force_open(&self, slave: &str) {
        if let Ok(mut circuits) = self.circuits.lock() {
            let sc = circuits
                .entry(slave.to_string())
                .or_insert_with(SlaveCircuit::new);
            sc.state = CircuitState::Open;
            sc.opened_at = Some(Instant::now());
        }
    }

    /// 获取配置引用
    pub fn config(&self) -> &CircuitBreakerConfig {
        &self.config
    }

    /// 总请求数
    pub fn total_requests(&self) -> u64 {
        self.total_requests.load(Ordering::Relaxed)
    }

    /// 被拒绝的请求数
    pub fn rejected_requests(&self) -> u64 {
        self.rejected_requests.load(Ordering::Relaxed)
    }

    /// 拒绝率（0.0-1.0）
    pub fn rejection_rate(&self) -> f64 {
        let total = self.total_requests();
        if total == 0 {
            0.0
        } else {
            self.rejected_requests() as f64 / total as f64
        }
    }

    /// 列出所有处于指定状态的 slave
    pub fn list_by_state(&self, state: CircuitState) -> Vec<String> {
        match self.circuits.lock() {
            Ok(circuits) => circuits
                .iter()
                .filter(|(_, sc)| {
                    let effective = if sc.state == CircuitState::Open {
                        if let Some(opened_at) = sc.opened_at {
                            if opened_at.elapsed() >= self.config.recovery_timeout {
                                CircuitState::HalfOpen
                            } else {
                                sc.state
                            }
                        } else {
                            sc.state
                        }
                    } else {
                        sc.state
                    };
                    effective == state
                })
                .map(|(k, _)| k.clone())
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// 所有熔断中的 slave
    pub fn open_slaves(&self) -> Vec<String> {
        self.list_by_state(CircuitState::Open)
    }

    /// 所有正常的 slave
    pub fn closed_slaves(&self) -> Vec<String> {
        self.list_by_state(CircuitState::Closed)
    }

    /// 获取 slave 的失败计数
    pub fn failure_count(&self, slave: &str) -> u32 {
        match self.circuits.lock() {
            Ok(circuits) => circuits.get(slave).map(|sc| sc.failure_count).unwrap_or(0),
            Err(_) => 0,
        }
    }

    /// 获取 slave 的成功计数（HalfOpen 状态下）
    pub fn success_count(&self, slave: &str) -> u32 {
        match self.circuits.lock() {
            Ok(circuits) => circuits.get(slave).map(|sc| sc.success_count).unwrap_or(0),
            Err(_) => 0,
        }
    }

    /// 所有半开 slave
    pub fn half_open_slaves(&self) -> Vec<String> {
        self.list_by_state(CircuitState::HalfOpen)
    }

    /// 所有已注册 slave
    pub fn registered_slaves(&self) -> Vec<String> {
        match self.circuits.lock() {
            Ok(circuits) => circuits.keys().cloned().collect(),
            Err(_) => Vec::new(),
        }
    }

    /// 已注册 slave 数量
    pub fn slave_count(&self) -> usize {
        match self.circuits.lock() {
            Ok(circuits) => circuits.len(),
            Err(_) => 0,
        }
    }

    /// 健康分数（0.0-1.0）：正常 slave 占比
    pub fn health_score(&self) -> f64 {
        match self.circuits.lock() {
            Ok(circuits) => {
                if circuits.is_empty() {
                    return 1.0;
                }
                let closed = circuits
                    .values()
                    .filter(|sc| sc.state == CircuitState::Closed)
                    .count();
                closed as f64 / circuits.len() as f64
            }
            Err(_) => 0.0,
        }
    }

    /// 生成汇总报告字符串
    pub fn summary(&self) -> String {
        let circuits = match self.circuits.lock() {
            Ok(c) => c,
            Err(_) => return "CircuitBreaker: lock poisoned".to_string(),
        };
        let mut out = format!(
            "CircuitBreaker: {} slave(s), total_req={}, rejected={}, rejection_rate={:.4}\n",
            circuits.len(),
            self.total_requests(),
            self.rejected_requests(),
            self.rejection_rate()
        );
        for (slave, sc) in circuits.iter() {
            out.push_str(&format!(
                "  {} : state={}, failures={}, successes={}\n",
                slave,
                sc.state.as_str(),
                sc.failure_count,
                sc.success_count
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_circuit_state_allows_request() {
        assert!(CircuitState::Closed.allows_request());
        assert!(!CircuitState::Open.allows_request());
        assert!(CircuitState::HalfOpen.allows_request());
    }

    #[test]
    fn test_circuit_state_as_str() {
        assert_eq!(CircuitState::Closed.as_str(), "closed");
        assert_eq!(CircuitState::Open.as_str(), "open");
        assert_eq!(CircuitState::HalfOpen.as_str(), "half_open");
    }

    #[test]
    fn test_new_circuit_breaker_defaults() {
        let cb = CircuitBreaker::with_defaults();
        assert_eq!(cb.config().failure_threshold, 5);
        assert_eq!(cb.config().success_threshold, 2);
        assert_eq!(cb.config().half_open_max_calls, 3);
    }

    #[test]
    fn test_unregistered_slave_state_is_closed() {
        let cb = CircuitBreaker::with_defaults();
        assert_eq!(cb.state("unknown"), CircuitState::Closed);
        assert!(cb.allow_request("unknown"));
    }

    #[test]
    fn test_closed_state_allows_requests() {
        let cb = CircuitBreaker::new(3, Duration::from_secs(30));
        cb.register("s1");
        assert_eq!(cb.state("s1"), CircuitState::Closed);
        for _ in 0..10 {
            assert!(cb.allow_request("s1"));
        }
    }

    #[test]
    fn test_failures_below_threshold_stay_closed() {
        let cb = CircuitBreaker::new(5, Duration::from_secs(30));
        cb.register("s1");
        for _ in 0..4 {
            cb.record_failure("s1");
        }
        assert_eq!(cb.state("s1"), CircuitState::Closed);
        assert!(cb.allow_request("s1"));
    }

    #[test]
    fn test_failures_at_threshold_opens_circuit() {
        let cb = CircuitBreaker::new(3, Duration::from_secs(30));
        cb.register("s1");
        cb.record_failure("s1");
        cb.record_failure("s1");
        assert_eq!(cb.state("s1"), CircuitState::Closed);
        cb.record_failure("s1");
        assert_eq!(cb.state("s1"), CircuitState::Open);
        assert!(!cb.allow_request("s1"));
    }

    #[test]
    fn test_open_circuit_rejects_requests() {
        let cb = CircuitBreaker::new(1, Duration::from_secs(60));
        cb.register("s1");
        cb.record_failure("s1");
        assert_eq!(cb.state("s1"), CircuitState::Open);
        assert!(!cb.allow_request("s1"));
    }

    #[test]
    fn test_open_transitions_to_half_open_after_timeout() {
        let cb = CircuitBreaker::new(1, Duration::from_millis(10));
        cb.register("s1");
        cb.record_failure("s1");
        assert_eq!(cb.state("s1"), CircuitState::Open);
        thread::sleep(Duration::from_millis(15));
        assert_eq!(cb.state("s1"), CircuitState::HalfOpen);
    }

    #[test]
    fn test_half_open_allows_limited_requests() {
        let cb = CircuitBreaker::new(1, Duration::from_millis(10))
            .with_config_override(|c| c.half_open_max_calls = 2);
        cb.register("s1");
        cb.record_failure("s1");
        thread::sleep(Duration::from_millis(15));
        assert_eq!(cb.state("s1"), CircuitState::HalfOpen);
        assert!(cb.allow_request("s1"));
        assert!(cb.allow_request("s1"));
        assert!(!cb.allow_request("s1"), "third request should be rejected");
    }

    #[test]
    fn test_half_open_success_closes_circuit() {
        let cb = CircuitBreaker::new(1, Duration::from_millis(10))
            .with_config_override(|c| c.success_threshold = 2);
        cb.register("s1");
        cb.record_failure("s1");
        thread::sleep(Duration::from_millis(15));
        assert_eq!(cb.state("s1"), CircuitState::HalfOpen);
        cb.record_success("s1");
        assert_eq!(cb.state("s1"), CircuitState::HalfOpen);
        cb.record_success("s1");
        assert_eq!(cb.state("s1"), CircuitState::Closed);
    }

    #[test]
    fn test_half_open_failure_reopens_circuit() {
        let cb = CircuitBreaker::new(1, Duration::from_millis(10));
        cb.register("s1");
        cb.record_failure("s1");
        thread::sleep(Duration::from_millis(15));
        assert_eq!(cb.state("s1"), CircuitState::HalfOpen);
        cb.record_failure("s1");
        assert_eq!(cb.state("s1"), CircuitState::Open);
    }

    #[test]
    fn test_closed_success_resets_failure_count() {
        let cb = CircuitBreaker::new(3, Duration::from_secs(30));
        cb.register("s1");
        cb.record_failure("s1");
        cb.record_failure("s1");
        assert_eq!(cb.failure_count("s1"), 2);
        cb.record_success("s1");
        assert_eq!(cb.failure_count("s1"), 0);
    }

    #[test]
    fn test_manual_reset() {
        let cb = CircuitBreaker::new(1, Duration::from_secs(60));
        cb.register("s1");
        cb.record_failure("s1");
        assert_eq!(cb.state("s1"), CircuitState::Open);
        cb.reset("s1");
        assert_eq!(cb.state("s1"), CircuitState::Closed);
        assert!(cb.allow_request("s1"));
    }

    #[test]
    fn test_force_open() {
        let cb = CircuitBreaker::new(10, Duration::from_secs(60));
        cb.register("s1");
        assert_eq!(cb.state("s1"), CircuitState::Closed);
        cb.force_open("s1");
        assert_eq!(cb.state("s1"), CircuitState::Open);
        assert!(!cb.allow_request("s1"));
    }

    #[test]
    fn test_total_and_rejected_counts() {
        let cb = CircuitBreaker::new(1, Duration::from_secs(60));
        cb.register("s1");
        cb.record_failure("s1");
        assert!(!cb.allow_request("s1"));
        assert!(!cb.allow_request("s1"));
        assert!(!cb.allow_request("s1"));
        assert_eq!(cb.total_requests(), 3);
        assert_eq!(cb.rejected_requests(), 3);
    }

    #[test]
    fn test_rejection_rate() {
        let cb = CircuitBreaker::new(1, Duration::from_secs(60));
        cb.register("s1");
        assert_eq!(cb.rejection_rate(), 0.0);
        cb.allow_request("s1");
        cb.record_failure("s1");
        cb.allow_request("s1");
        let rate = cb.rejection_rate();
        assert!(
            (0.4..=0.6).contains(&rate),
            "rejection rate should be ~0.5, got {rate}"
        );
    }

    #[test]
    fn test_list_by_state() {
        let cb = CircuitBreaker::new(1, Duration::from_secs(60));
        cb.register("s1");
        cb.register("s2");
        cb.register("s3");
        cb.record_failure("s1");
        cb.force_open("s3");
        let mut closed = cb.closed_slaves();
        closed.sort();
        assert_eq!(closed, vec!["s2".to_string()]);
        let mut open = cb.open_slaves();
        open.sort();
        assert_eq!(open, vec!["s1".to_string(), "s3".to_string()]);
    }

    #[test]
    fn test_config_builder() {
        let config = CircuitBreakerConfig::new(10, Duration::from_secs(60))
            .with_half_open_max_calls(5)
            .with_success_threshold(3);
        assert_eq!(config.failure_threshold, 10);
        assert_eq!(config.half_open_max_calls, 5);
        assert_eq!(config.success_threshold, 3);
    }

    #[test]
    fn test_success_count_in_half_open() {
        let cb = CircuitBreaker::new(1, Duration::from_millis(10))
            .with_config_override(|c| c.success_threshold = 3);
        cb.register("s1");
        cb.record_failure("s1");
        thread::sleep(Duration::from_millis(15));
        cb.record_success("s1");
        assert_eq!(cb.success_count("s1"), 1);
        cb.record_success("s1");
        assert_eq!(cb.success_count("s1"), 2);
    }

    #[test]
    fn test_half_open_slaves() {
        let cb = CircuitBreaker::new(1, Duration::from_millis(10));
        cb.register("s1");
        cb.register("s2");
        cb.record_failure("s1");
        thread::sleep(Duration::from_millis(15));
        assert_eq!(cb.half_open_slaves(), vec!["s1".to_string()]);
    }

    #[test]
    fn test_registered_slaves() {
        let cb = CircuitBreaker::new(1, Duration::from_secs(60));
        cb.register("s1");
        cb.register("s2");
        let mut slaves = cb.registered_slaves();
        slaves.sort();
        assert_eq!(slaves, vec!["s1".to_string(), "s2".to_string()]);
    }

    #[test]
    fn test_slave_count() {
        let cb = CircuitBreaker::new(1, Duration::from_secs(60));
        cb.register("s1");
        cb.register("s2");
        assert_eq!(cb.slave_count(), 2);
    }

    #[test]
    fn test_health_score_all_closed() {
        let cb = CircuitBreaker::new(1, Duration::from_secs(60));
        cb.register("s1");
        cb.register("s2");
        assert_eq!(cb.health_score(), 1.0);
    }

    #[test]
    fn test_health_score_half_open() {
        let cb = CircuitBreaker::new(1, Duration::from_secs(60));
        cb.register("s1");
        cb.register("s2");
        cb.record_failure("s1");
        assert!((0.4..=0.6).contains(&cb.health_score()));
    }

    #[test]
    fn test_health_score_empty() {
        let cb = CircuitBreaker::new(1, Duration::from_secs(60));
        assert_eq!(cb.health_score(), 1.0);
    }

    #[test]
    fn test_summary_contains_info() {
        let cb = CircuitBreaker::new(1, Duration::from_secs(60));
        cb.register("s1");
        cb.record_failure("s1");
        let s = cb.summary();
        assert!(s.contains("s1"));
        assert!(s.contains("open"));
    }

    #[test]
    fn test_rejection_rate_no_requests() {
        let cb = CircuitBreaker::new(1, Duration::from_secs(60));
        assert_eq!(cb.rejection_rate(), 0.0);
    }

    #[test]
    fn test_force_open_then_reset() {
        let cb = CircuitBreaker::new(10, Duration::from_secs(60));
        cb.register("s1");
        cb.force_open("s1");
        assert!(!cb.allow_request("s1"));
        cb.reset("s1");
        assert!(cb.allow_request("s1"));
    }

    /// 配置覆盖的便捷 trait（仅测试用）
    trait CircuitBreakerConfigOverride: Sized {
        fn with_config_override<F>(self, f: F) -> Self
        where
            F: FnOnce(&mut CircuitBreakerConfig);
    }

    impl CircuitBreakerConfigOverride for CircuitBreaker {
        fn with_config_override<F>(self, f: F) -> Self
        where
            F: FnOnce(&mut CircuitBreakerConfig),
        {
            let mut config = self.config.clone();
            f(&mut config);
            Self::with_config(config)
        }
    }
}
