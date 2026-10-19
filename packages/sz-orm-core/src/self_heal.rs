//! 故障自愈配置与事件（v10.2.0）
//!
//! DB 级自愈：处理整个 DB 不可达——指数退避重连 + 熔断器自动半开探测恢复。
//!
//! 与 AutoHealer（连接级自愈）协同关系：
//! - SelfHealEngine 处理 DB 级故障（整个 DB 不可达），属故障恢复
//! - AutoHealer 处理连接级故障（单个连接 ping 超时/断开），属日常维护
//! - 两者不冲突，处理不同层级故障，可协同工作
//! - 统一配置见 `UnifiedHealConfig`（v10.3.0）

use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// 故障自愈配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfHealConfig {
    /// 是否启用自愈，默认 false
    pub self_heal_enabled: bool,
    /// 重连初始退避时间，默认 1s
    pub reconnect_initial_backoff: Duration,
    /// 退避乘数，默认 2.0
    pub reconnect_backoff_multiplier: f64,
    /// 最大退避时间，默认 30s
    pub reconnect_max_backoff: Duration,
    /// 最大重连尝试次数，默认 10
    pub reconnect_max_attempts: u32,
    /// 抖动范围 [0, 1)，默认 0.2
    pub reconnect_jitter_range: f64,
    /// 熔断器探测间隔，默认 10s
    pub circuit_breaker_probe_interval: Duration,
}

impl Default for SelfHealConfig {
    fn default() -> Self {
        Self {
            self_heal_enabled: false,
            reconnect_initial_backoff: Duration::from_secs(1),
            reconnect_backoff_multiplier: 2.0,
            reconnect_max_backoff: Duration::from_secs(30),
            reconnect_max_attempts: 10,
            reconnect_jitter_range: 0.2,
            circuit_breaker_probe_interval: Duration::from_secs(10),
        }
    }
}

/// 自愈事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SelfHealEvent {
    ReconnectAttempt {
        timestamp: u64,
        attempt: u32,
    },
    ReconnectSuccess {
        timestamp: u64,
        attempt: u32,
    },
    ReconnectFailed {
        timestamp: u64,
        attempt: u32,
        detail: String,
    },
    ReconnectMaxAttemptsReached {
        timestamp: u64,
    },
    CircuitBreakerProbe {
        timestamp: u64,
    },
    CircuitBreakerRecovered {
        timestamp: u64,
    },
}

/// 自愈错误
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SelfHealError {
    MaxAttemptsReached,
    ConfigInvalid(String),
    ReconnectFailed(String),
}

/// 自愈引擎
pub struct SelfHealEngine {
    config: SelfHealConfig,
    events: Mutex<Vec<SelfHealEvent>>,
    valid: bool,
}

impl SelfHealEngine {
    pub fn new(config: SelfHealConfig) -> Self {
        let valid = config.reconnect_max_attempts >= 1
            && config.reconnect_backoff_multiplier >= 1.0
            && config.reconnect_jitter_range >= 0.0
            && config.reconnect_jitter_range <= 0.5;
        Self {
            config,
            events: Mutex::new(Vec::new()),
            valid,
        }
    }

    /// 伪随机因子 ∈ [-1, 1]，基于系统时钟纳秒
    fn pseudo_rand_factor() -> f64 {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as u64)
            .unwrap_or(0);
        ((nanos % 2000) as f64 - 1000.0) / 1000.0
    }

    /// 计算退避时间（指数退避 + 抖动）
    pub fn compute_backoff(&self, attempt: u32) -> Duration {
        let base = self.config.reconnect_initial_backoff.as_millis() as f64;
        let raw = base
            * self
                .config
                .reconnect_backoff_multiplier
                .powi(attempt as i32);
        let max_ms = self.config.reconnect_max_backoff.as_millis() as f64;
        let capped = raw.min(max_ms);
        let jitter = capped * self.config.reconnect_jitter_range;
        let rand_factor = SelfHealEngine::pseudo_rand_factor();
        let with_jitter = capped + jitter * rand_factor;
        Duration::from_millis(with_jitter.max(0.0) as u64)
    }

    /// 当前时间戳（秒）
    fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// 脱敏：移除密码/凭据明文
    fn sanitize_detail(detail: &str) -> String {
        let mut s = detail.to_string();
        for pattern in &["password", "passwd", "pwd", "secret", "token"] {
            if s.to_lowercase().contains(pattern) {
                s = format!("[sanitized: contains {}]", pattern);
            }
        }
        s
    }

    fn record_event(&self, event: SelfHealEvent) {
        self.events.lock().expect("events mutex").push(event);
    }

    /// 自动重连
    pub async fn auto_reconnect<F, Fut>(&self, reconnect_fn: F) -> Result<(), SelfHealError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<(), String>>,
    {
        if !self.valid {
            return Err(SelfHealError::ConfigInvalid(
                "max_attempts >= 1, multiplier >= 1.0, jitter ∈ [0, 0.5] required".to_string(),
            ));
        }
        for attempt in 1..=self.config.reconnect_max_attempts {
            let backoff = self.compute_backoff(attempt - 1);
            tokio::time::sleep(backoff).await;
            let ts = SelfHealEngine::now_secs();
            self.record_event(SelfHealEvent::ReconnectAttempt {
                timestamp: ts,
                attempt,
            });
            match reconnect_fn().await {
                Ok(()) => {
                    self.record_event(SelfHealEvent::ReconnectSuccess {
                        timestamp: ts,
                        attempt,
                    });
                    return Ok(());
                }
                Err(detail) => {
                    let sanitized = SelfHealEngine::sanitize_detail(&detail);
                    self.record_event(SelfHealEvent::ReconnectFailed {
                        timestamp: ts,
                        attempt,
                        detail: sanitized,
                    });
                }
            }
        }
        let ts = SelfHealEngine::now_secs();
        self.record_event(SelfHealEvent::ReconnectMaxAttemptsReached { timestamp: ts });
        Err(SelfHealError::MaxAttemptsReached)
    }

    /// 熔断器自动半开探测
    pub async fn auto_circuit_breaker_probe(
        &self,
        cb: &mut dyn crate::circuit_breaker::CircuitBreaker,
    ) -> bool {
        tokio::time::sleep(self.config.circuit_breaker_probe_interval).await;
        let ts = SelfHealEngine::now_secs();
        self.record_event(SelfHealEvent::CircuitBreakerProbe { timestamp: ts });
        if cb.can_execute() {
            cb.record_success();
            let ts2 = SelfHealEngine::now_secs();
            self.record_event(SelfHealEvent::CircuitBreakerRecovered { timestamp: ts2 });
            true
        } else {
            cb.record_failure();
            false
        }
    }

    /// 事件历史查询
    pub fn event_history(&self) -> Vec<SelfHealEvent> {
        self.events.lock().expect("events mutex").clone()
    }

    /// 配置是否有效
    pub fn is_valid(&self) -> bool {
        self.valid
    }

    /// 获取配置引用
    pub fn config(&self) -> &SelfHealConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_self_heal_config_default_disabled() {
        let config = SelfHealConfig::default();
        assert!(!config.self_heal_enabled);
        assert_eq!(config.reconnect_max_attempts, 10, "default max attempts 10");
        let enabled = SelfHealConfig {
            self_heal_enabled: true,
            ..Default::default()
        };
        assert!(enabled.self_heal_enabled, "enabled config");
    }

    #[test]
    fn test_compute_backoff_exponential_growth() {
        let config = SelfHealConfig {
            self_heal_enabled: true,
            reconnect_initial_backoff: Duration::from_millis(100),
            reconnect_backoff_multiplier: 2.0,
            reconnect_max_backoff: Duration::from_secs(60),
            reconnect_jitter_range: 0.0,
            ..Default::default()
        };
        let engine = SelfHealEngine::new(config);
        let b0 = engine.compute_backoff(0);
        let b1 = engine.compute_backoff(1);
        let b2 = engine.compute_backoff(2);
        assert!(b1 > b0, "backoff must grow: b0={b0:?} b1={b1:?}");
        assert!(b2 > b1, "backoff must grow: b1={b1:?} b2={b2:?}");
    }

    #[test]
    fn test_compute_backoff_capped_at_max() {
        let config = SelfHealConfig {
            self_heal_enabled: true,
            reconnect_initial_backoff: Duration::from_millis(100),
            reconnect_backoff_multiplier: 10.0,
            reconnect_max_backoff: Duration::from_millis(500),
            reconnect_jitter_range: 0.0,
            ..Default::default()
        };
        let engine = SelfHealEngine::new(config);
        for attempt in 0..20 {
            let backoff = engine.compute_backoff(attempt);
            assert!(
                backoff <= Duration::from_millis(500),
                "backoff must be capped: attempt={attempt} backoff={backoff:?}"
            );
        }
    }

    #[test]
    fn test_compute_backoff_jitter_range() {
        let config = SelfHealConfig {
            self_heal_enabled: true,
            reconnect_initial_backoff: Duration::from_millis(1000),
            reconnect_backoff_multiplier: 1.0,
            reconnect_max_backoff: Duration::from_secs(60),
            reconnect_jitter_range: 0.2,
            ..Default::default()
        };
        let engine = SelfHealEngine::new(config);
        for _ in 0..50 {
            let backoff = engine.compute_backoff(0);
            assert!(backoff <= Duration::from_millis(1200), "jitter upper bound");
            assert!(backoff >= Duration::from_millis(800), "jitter lower bound");
        }
    }

    #[tokio::test]
    async fn test_auto_reconnect_success() {
        let config = SelfHealConfig {
            self_heal_enabled: true,
            reconnect_initial_backoff: Duration::from_millis(1),
            reconnect_max_attempts: 5,
            ..Default::default()
        };
        let engine = SelfHealEngine::new(config);
        let attempt_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let ac = attempt_count.clone();
        let result = engine
            .auto_reconnect(move || {
                let ac = ac.clone();
                async move {
                    let n = ac.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if n == 0 {
                        Err("connection refused".to_string())
                    } else {
                        Ok(())
                    }
                }
            })
            .await;
        assert!(result.is_ok(), "reconnect should succeed on second attempt");
        let history = engine.event_history();
        assert!(
            history
                .iter()
                .any(|e| matches!(e, SelfHealEvent::ReconnectAttempt { .. })),
            "must have attempt event"
        );
        assert!(
            history
                .iter()
                .any(|e| matches!(e, SelfHealEvent::ReconnectFailed { .. })),
            "must have failed event"
        );
        assert!(
            history
                .iter()
                .any(|e| matches!(e, SelfHealEvent::ReconnectSuccess { .. })),
            "must have success event"
        );
    }

    #[tokio::test]
    async fn test_auto_reconnect_max_attempts() {
        let config = SelfHealConfig {
            self_heal_enabled: true,
            reconnect_initial_backoff: Duration::from_millis(1),
            reconnect_max_attempts: 3,
            ..Default::default()
        };
        let engine = SelfHealEngine::new(config);
        let result = engine
            .auto_reconnect(|| async { Err("db unreachable".to_string()) })
            .await;
        assert!(matches!(result, Err(SelfHealError::MaxAttemptsReached)));
        let history = engine.event_history();
        assert!(history
            .iter()
            .any(|e| matches!(e, SelfHealEvent::ReconnectMaxAttemptsReached { .. })));
        let attempt_count = history
            .iter()
            .filter(|e| matches!(e, SelfHealEvent::ReconnectAttempt { .. }))
            .count();
        assert_eq!(attempt_count, 3, "must attempt exactly 3 times");
    }

    #[test]
    fn test_self_heal_event_no_credentials() {
        let detail = SelfHealEngine::sanitize_detail("password=mysecret123 token=abc");
        assert!(
            !detail.contains("mysecret123"),
            "must not contain password value"
        );
        assert!(!detail.contains("abc"), "must not contain token value");
    }

    #[test]
    fn test_self_heal_config_invalid_rejected() {
        let config = SelfHealConfig {
            self_heal_enabled: true,
            reconnect_max_attempts: 0,
            ..Default::default()
        };
        let engine = SelfHealEngine::new(config);
        assert!(!engine.is_valid(), "max_attempts=0 must be invalid");
    }
}
