//! 统一自愈配置与事件（v10.3.0）
//!
//! 统一 AutoHealer（连接级自愈）与 SelfHealEngine（DB 级自愈）的配置和事件查询。

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::self_heal::{SelfHealConfig, SelfHealEvent};

/// 统一自愈配置
///
/// 连接级自愈（AutoHealer）处理单个连接不健康：ping 探测失败剔除 + 触底重建至 min_idle。
/// DB 级自愈（SelfHealEngine）处理整个 DB 不可达：指数退避重连 + 熔断器自动半开探测恢复。
/// 两者不冲突，处理不同层级故障，可协同工作。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UnifiedHealConfig {
    /// 是否启用统一自愈，默认 false
    pub unified_heal_enabled: bool,
    /// AutoHealer 配置（连接级自愈，需 auto-healer feature）
    #[cfg(feature = "auto-healer")]
    pub auto_healer_config: crate::pool::HealerConfig,
    /// SelfHealEngine 配置（DB 级自愈）
    pub self_heal_config: SelfHealConfig,
    /// 是否融合退避策略，默认 false
    ///
    /// 启用后 AutoHealer 探测间隔选用 SelfHealEngine 的指数退避策略，
    /// 统一退避参数避免两套退避策略不一致。
    pub backoff_fusion_enabled: bool,
}

/// 自愈事件来源
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HealEventSource {
    /// 连接级自愈（AutoHealer）
    AutoHealer,
    /// DB 级自愈（SelfHealEngine）
    SelfHealEngine,
}

/// 统一自愈事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedHealEvent {
    /// 事件来源
    pub event_source: HealEventSource,
    /// 事件类型
    pub event_type: String,
    /// 时间戳（UNIX 毫秒）
    pub timestamp: u64,
    /// 事件详情（已脱敏，不含凭据/连接字符串明文）
    pub detail: Option<String>,
}

impl UnifiedHealEvent {
    /// 从 SelfHealEvent 转换为 UnifiedHealEvent
    pub fn from_self_heal_event(event: &SelfHealEvent) -> Self {
        let (event_type, timestamp, detail) = match event {
            SelfHealEvent::ReconnectAttempt { timestamp, attempt } => (
                "ReconnectAttempt".to_string(),
                *timestamp,
                Some(format!("attempt={attempt}")),
            ),
            SelfHealEvent::ReconnectSuccess { timestamp, attempt } => (
                "ReconnectSuccess".to_string(),
                *timestamp,
                Some(format!("attempt={attempt}")),
            ),
            SelfHealEvent::ReconnectFailed {
                timestamp,
                attempt,
                detail,
            } => (
                "ReconnectFailed".to_string(),
                *timestamp,
                Some(format!(
                    "attempt={attempt}, detail={}",
                    sanitize_detail(detail)
                )),
            ),
            SelfHealEvent::ReconnectMaxAttemptsReached { timestamp } => {
                ("ReconnectMaxAttemptsReached".to_string(), *timestamp, None)
            }
            SelfHealEvent::CircuitBreakerProbe { timestamp } => {
                ("CircuitBreakerProbe".to_string(), *timestamp, None)
            }
            SelfHealEvent::CircuitBreakerRecovered { timestamp } => {
                ("CircuitBreakerRecovered".to_string(), *timestamp, None)
            }
        };
        Self {
            event_source: HealEventSource::SelfHealEngine,
            event_type,
            timestamp,
            detail,
        }
    }

    /// 当前 UNIX 时间戳（毫秒）
    pub fn now_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}

/// 脱敏：移除密码/凭据明文
fn sanitize_detail(detail: &str) -> String {
    let mut result = detail.to_string();
    for keyword in &["password=", "passwd=", "secret=", "token=", "credential="] {
        if let Some(pos) = result.find(keyword) {
            let start = pos + keyword.len();
            if let Some(end) =
                result[start..].find(|c: char| c.is_whitespace() || c == ',' || c == ';')
            {
                result.replace_range(start..start + end, "***");
            } else {
                result.replace_range(start.., "***");
            }
        }
    }
    result
}

/// 计算融合退避间隔
///
/// 委托 SelfHealEngine 的指数退避策略计算含抖动的退避间隔
pub fn compute_fused_heal_interval(self_heal_config: &SelfHealConfig, attempt: u32) -> Duration {
    let base = self_heal_config.reconnect_initial_backoff.as_millis() as f64;
    let multiplier = self_heal_config
        .reconnect_backoff_multiplier
        .powi(attempt as i32);
    let backoff_ms =
        (base * multiplier).min(self_heal_config.reconnect_max_backoff.as_millis() as f64);
    let jitter = (backoff_ms
        * self_heal_config.reconnect_jitter_range
        * (SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as f64
            % 1000.0
            / 1000.0))
        .min(backoff_ms * 0.5);
    Duration::from_millis((backoff_ms + jitter) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unified_heal_config_default_disabled() {
        let config = UnifiedHealConfig::default();
        assert!(!config.unified_heal_enabled);
        assert!(!config.backoff_fusion_enabled);
    }

    #[test]
    fn test_unified_heal_event_source_distinct() {
        let auto_event = UnifiedHealEvent {
            event_source: HealEventSource::AutoHealer,
            event_type: "ProbeFailed".to_string(),
            timestamp: 1000,
            detail: None,
        };
        let db_event = UnifiedHealEvent {
            event_source: HealEventSource::SelfHealEngine,
            event_type: "ReconnectFailed".to_string(),
            timestamp: 2000,
            detail: None,
        };
        assert_ne!(auto_event.event_source, db_event.event_source);
    }

    #[test]
    fn test_unified_heal_event_serializable() {
        let event = UnifiedHealEvent {
            event_source: HealEventSource::SelfHealEngine,
            event_type: "ReconnectSuccess".to_string(),
            timestamp: 12345,
            detail: Some("attempt=3".to_string()),
        };
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: UnifiedHealEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.event_type, event.event_type);
        assert_eq!(deserialized.timestamp, event.timestamp);
    }

    #[test]
    fn test_unified_heal_event_detail_sanitized() {
        let event = UnifiedHealEvent::from_self_heal_event(&SelfHealEvent::ReconnectFailed {
            timestamp: 1000,
            attempt: 1,
            detail: "password=secret123 connection refused".to_string(),
        });
        assert!(!event
            .detail
            .as_ref()
            .map(|d| d.contains("secret123"))
            .unwrap_or(false));
    }

    #[test]
    fn test_backoff_fusion_disabled_default() {
        let config = UnifiedHealConfig::default();
        assert!(!config.backoff_fusion_enabled);
    }

    #[test]
    fn test_fused_heal_interval_within_max() {
        let config = SelfHealConfig {
            reconnect_initial_backoff: Duration::from_millis(100),
            reconnect_backoff_multiplier: 2.0,
            reconnect_max_backoff: Duration::from_secs(5),
            reconnect_jitter_range: 0.1,
            ..Default::default()
        };
        let interval = compute_fused_heal_interval(&config, 10);
        assert!(interval <= Duration::from_secs(6));
    }
}
