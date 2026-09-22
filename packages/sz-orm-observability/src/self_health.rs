//! 可观测性自身健康 + 统一脱敏兼容。
//!
//! 追踪/指标/日志/告警系统自身健康暴露 + 采集丢失告警。
//! 确认可观测性数据统一脱敏（ADR-008 复用 ContextAwareMasker 思路）：
//! span 不含敏感参数值、指标脱敏、日志脱敏、告警通知脱敏。
//! 兼容 v8.0.0 既有局部可观测性，未启用统一时保留局部。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// 可观测性子系统。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ObsSubsystem {
    /// 追踪。
    Tracing,
    /// 指标。
    Metrics,
    /// 日志。
    Logging,
    /// 告警。
    Alerting,
}

impl ObsSubsystem {
    /// 全部子系统。
    pub fn all() -> Vec<ObsSubsystem> {
        vec![
            ObsSubsystem::Tracing,
            ObsSubsystem::Metrics,
            ObsSubsystem::Logging,
            ObsSubsystem::Alerting,
        ]
    }

    /// 子系统名。
    pub fn as_str(&self) -> &'static str {
        match self {
            ObsSubsystem::Tracing => "tracing",
            ObsSubsystem::Metrics => "metrics",
            ObsSubsystem::Logging => "logging",
            ObsSubsystem::Alerting => "alerting",
        }
    }
}

/// 子系统健康状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    /// 健康。
    Healthy,
    /// 降级（部分功能不可用）。
    Degraded,
    /// 不健康。
    Unhealthy,
}

impl HealthStatus {
    /// 状态名。
    pub fn as_str(&self) -> &'static str {
        match self {
            HealthStatus::Healthy => "healthy",
            HealthStatus::Degraded => "degraded",
            HealthStatus::Unhealthy => "unhealthy",
        }
    }
}

/// 子系统健康报告。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubsystemHealth {
    /// 子系统。
    pub subsystem: ObsSubsystem,
    /// 健康状态。
    pub status: HealthStatus,
    /// 采集丢失计数。
    pub collection_lost_count: u64,
    /// 最后采集时间（Unix 毫秒，0 表示从未采集）。
    pub last_collect_ms: i64,
    /// 详情。
    pub detail: String,
}

/// 可观测性整体健康报告。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityHealthReport {
    /// 整体状态（取最差子系统状态）。
    pub overall: HealthStatus,
    /// 各子系统健康。
    pub subsystems: Vec<SubsystemHealth>,
    /// 采集丢失告警列表。
    pub collection_lost_alerts: Vec<String>,
    /// 脱敏校验通过。
    pub desensitization_verified: bool,
}

/// 可观测性自身健康检查器。
pub struct ObservabilitySelfHealth {
    /// 各子系统采集丢失计数。
    collection_lost: parking_lot::Mutex<HashMap<ObsSubsystem, u64>>,
    /// 各子系统最后采集时间（Unix 毫秒）。
    last_collect: parking_lot::Mutex<HashMap<ObsSubsystem, i64>>,
    /// 各子系统降级标志。
    degraded: parking_lot::Mutex<HashMap<ObsSubsystem, bool>>,
    /// 脱敏校验结果。
    desensitization_verified: parking_lot::Mutex<bool>,
}

impl ObservabilitySelfHealth {
    /// 创建健康检查器。
    pub fn new() -> Self {
        Self {
            collection_lost: parking_lot::Mutex::new(HashMap::new()),
            last_collect: parking_lot::Mutex::new(HashMap::new()),
            degraded: parking_lot::Mutex::new(HashMap::new()),
            desensitization_verified: parking_lot::Mutex::new(true),
        }
    }

    /// 记录采集丢失。
    pub fn record_collection_lost(&self, subsystem: ObsSubsystem) {
        let mut lost = self.collection_lost.lock();
        *lost.entry(subsystem).or_insert(0) += 1;
    }

    /// 记录采集成功（更新最后采集时间）。
    pub fn record_collection_success(&self, subsystem: ObsSubsystem) {
        let mut last = self.last_collect.lock();
        last.insert(subsystem, chrono::Utc::now().timestamp_millis());
    }

    /// 标记子系统降级。
    pub fn mark_degraded(&self, subsystem: ObsSubsystem) {
        let mut deg = self.degraded.lock();
        deg.insert(subsystem, true);
    }

    /// 清除降级标记。
    pub fn clear_degraded(&self, subsystem: ObsSubsystem) {
        let mut deg = self.degraded.lock();
        deg.insert(subsystem, false);
    }

    /// 设置脱敏校验结果。
    pub fn set_desensitization_verified(&self, verified: bool) {
        *self.desensitization_verified.lock() = verified;
    }

    /// 获取子系统健康状态。
    fn subsystem_status(&self, subsystem: ObsSubsystem) -> HealthStatus {
        let lost = self
            .collection_lost
            .lock()
            .get(&subsystem)
            .copied()
            .unwrap_or(0);
        let degraded = self
            .degraded
            .lock()
            .get(&subsystem)
            .copied()
            .unwrap_or(false);
        let last = self
            .last_collect
            .lock()
            .get(&subsystem)
            .copied()
            .unwrap_or(0);

        if last == 0 {
            // 从未采集 → 不健康
            HealthStatus::Unhealthy
        } else if degraded || lost > 0 {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        }
    }

    /// 健康检查，返回整体报告。
    pub fn health(&self) -> ObservabilityHealthReport {
        let subsystems: Vec<SubsystemHealth> = ObsSubsystem::all()
            .iter()
            .map(|sub| {
                let status = self.subsystem_status(*sub);
                let lost = self.collection_lost.lock().get(sub).copied().unwrap_or(0);
                let last = self.last_collect.lock().get(sub).copied().unwrap_or(0);
                let detail = format!("lost={}, last_collect_ms={}", lost, last);
                SubsystemHealth {
                    subsystem: *sub,
                    status,
                    collection_lost_count: lost,
                    last_collect_ms: last,
                    detail,
                }
            })
            .collect();

        // 整体状态取最差
        let overall = subsystems
            .iter()
            .map(|s| s.status)
            .max_by(|a, b| {
                fn rank(s: HealthStatus) -> u8 {
                    match s {
                        HealthStatus::Healthy => 0,
                        HealthStatus::Degraded => 1,
                        HealthStatus::Unhealthy => 2,
                    }
                }
                rank(*a).cmp(&rank(*b))
            })
            .unwrap_or(HealthStatus::Unhealthy);

        // 采集丢失告警
        let collection_lost_alerts: Vec<String> = subsystems
            .iter()
            .filter(|s| s.collection_lost_count > 0)
            .map(|s| {
                format!(
                    "METRICS_COLLECTION_LOST: {} lost {} samples",
                    s.subsystem.as_str(),
                    s.collection_lost_count
                )
            })
            .collect();

        let desensitization_verified = *self.desensitization_verified.lock();

        ObservabilityHealthReport {
            overall,
            subsystems,
            collection_lost_alerts,
            desensitization_verified,
        }
    }

    /// 校验脱敏：确认 span/指标/日志/告警数据不含敏感值。
    /// 检查输入数据中是否包含敏感关键词的原始值。
    pub fn verify_desensitization(data: &HashMap<String, String>) -> bool {
        let sensitive_keys = ["password", "token", "secret", "api_key", "authorization"];
        for (k, v) in data {
            // 敏感键的值应为 *** 或已脱敏
            let is_sensitive_key = sensitive_keys.iter().any(|sk| k.eq_ignore_ascii_case(sk));
            if is_sensitive_key && v != "***" && !v.is_empty() {
                return false;
            }
        }
        true
    }
}

impl Default for ObservabilitySelfHealth {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_subsystems() {
        let all = ObsSubsystem::all();
        assert_eq!(all.len(), 4);
    }

    #[test]
    fn test_initial_health_unhealthy() {
        // 初始状态：从未采集 → Unhealthy
        let health = ObservabilitySelfHealth::new();
        let report = health.health();
        assert_eq!(report.overall, HealthStatus::Unhealthy);
    }

    #[test]
    fn test_healthy_after_collection() {
        let health = ObservabilitySelfHealth::new();
        for sub in ObsSubsystem::all() {
            health.record_collection_success(sub);
        }
        let report = health.health();
        assert_eq!(report.overall, HealthStatus::Healthy);
        assert!(report.collection_lost_alerts.is_empty());
    }

    #[test]
    fn test_collection_lost_alert() {
        let health = ObservabilitySelfHealth::new();
        for sub in ObsSubsystem::all() {
            health.record_collection_success(sub);
        }
        health.record_collection_lost(ObsSubsystem::Metrics);
        let report = health.health();
        assert!(!report.collection_lost_alerts.is_empty());
        assert!(report.collection_lost_alerts[0].contains("METRICS_COLLECTION_LOST"));
        // 有丢失 → Degraded
        let metrics_health = report
            .subsystems
            .iter()
            .find(|s| s.subsystem == ObsSubsystem::Metrics)
            .unwrap();
        assert_eq!(metrics_health.status, HealthStatus::Degraded);
    }

    #[test]
    fn test_degraded_status() {
        let health = ObservabilitySelfHealth::new();
        for sub in ObsSubsystem::all() {
            health.record_collection_success(sub);
        }
        health.mark_degraded(ObsSubsystem::Tracing);
        let report = health.health();
        assert_eq!(report.overall, HealthStatus::Degraded);
    }

    #[test]
    fn test_clear_degraded() {
        let health = ObservabilitySelfHealth::new();
        for sub in ObsSubsystem::all() {
            health.record_collection_success(sub);
        }
        health.mark_degraded(ObsSubsystem::Alerting);
        assert_eq!(health.health().overall, HealthStatus::Degraded);
        health.clear_degraded(ObsSubsystem::Alerting);
        assert_eq!(health.health().overall, HealthStatus::Healthy);
    }

    #[test]
    fn test_desensitization_verified_default() {
        let health = ObservabilitySelfHealth::new();
        let report = health.health();
        assert!(report.desensitization_verified);
    }

    #[test]
    fn test_desensitization_failed() {
        let health = ObservabilitySelfHealth::new();
        health.set_desensitization_verified(false);
        let report = health.health();
        assert!(!report.desensitization_verified);
    }

    #[test]
    fn test_verify_desensitization_clean_data() {
        let mut data = HashMap::new();
        data.insert("user".to_string(), "alice".to_string());
        data.insert("query".to_string(), "SELECT * FROM t".to_string());
        assert!(ObservabilitySelfHealth::verify_desensitization(&data));
    }

    #[test]
    fn test_verify_desensitization_masked_sensitive() {
        let mut data = HashMap::new();
        data.insert("password".to_string(), "***".to_string());
        data.insert("token".to_string(), "***".to_string());
        assert!(ObservabilitySelfHealth::verify_desensitization(&data));
    }

    #[test]
    fn test_verify_desensitization_unmasked_sensitive() {
        let mut data = HashMap::new();
        data.insert("password".to_string(), "secret123".to_string());
        assert!(!ObservabilitySelfHealth::verify_desensitization(&data));
    }

    #[test]
    fn test_verify_desensitization_case_insensitive() {
        let mut data = HashMap::new();
        data.insert("Authorization".to_string(), "Bearer xxx".to_string());
        assert!(!ObservabilitySelfHealth::verify_desensitization(&data));
    }

    #[test]
    fn test_partial_subsystem_health() {
        let health = ObservabilitySelfHealth::new();
        // 只有部分子系统采集成功
        health.record_collection_success(ObsSubsystem::Tracing);
        health.record_collection_success(ObsSubsystem::Metrics);
        let report = health.health();
        // 未采集的子系统 Unhealthy → 整体 Unhealthy
        assert_eq!(report.overall, HealthStatus::Unhealthy);
    }
}
