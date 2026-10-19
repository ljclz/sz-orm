//! 性能联动配置（v10.1.0）
//!
//! 零拷贝/缓存/批量三者协同联动配置 + 可观测性闭环/合规自动化纯配置类型。
//! 引擎逻辑在 sz-orm-observability / sz-orm-audit 包中实现，此处仅定义配置数据结构。
//! 默认全部 disabled，向后兼容 v10.0.0 各模块独立工作。

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// 性能联动配置（零拷贝+缓存+批量三者协同）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PerformanceLinkageConfig {
    /// 是否启用联动，默认 false（向后兼容）
    pub enabled: bool,
    /// 缓存命中时是否以零拷贝路径返回
    pub cache_zero_copy_return: bool,
    /// 自适应批大小是否考虑缓存命中率
    pub batch_cache_aware: bool,
}

/// 联动指标
#[derive(Debug, Clone, Default)]
pub struct LinkageMetrics {
    pub enabled: bool,
    pub hit_count: u64,
    pub skip_count: u64,
}

impl LinkageMetrics {
    pub fn record_hit(&mut self) {
        self.hit_count += 1;
    }

    pub fn record_skip(&mut self) {
        self.skip_count += 1;
    }
}

/// 可观测性闭环配置（纯配置类型，引擎在 sz-orm-observability 中实现）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityLoopConfig {
    /// 是否启用闭环，默认 false
    pub enabled: bool,
    /// 慢查询→AI 分析→索引建议
    pub slow_query_to_ai: bool,
    /// CRITICAL 告警→诊断报告
    pub alert_to_diagnosis: bool,
    /// 事件队列容量
    pub event_queue_capacity: u32,
    /// AI 分析超时
    pub ai_analysis_timeout: Duration,
}

impl Default for ObservabilityLoopConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            slow_query_to_ai: false,
            alert_to_diagnosis: false,
            event_queue_capacity: 1000,
            ai_analysis_timeout: Duration::from_secs(30),
        }
    }
}

/// 安全配置预设
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SecurityPreset {
    #[default]
    None,
    ComplianceFirst,
    PerformanceFirst,
    Development,
}

/// 合规自动化配置（纯配置类型，检查器在 sz-orm-audit 中实现）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceAutoCheckConfig {
    /// 是否启用自动合规检查，默认 false
    pub auto_check_enabled: bool,
    /// 检查间隔
    pub check_interval: Duration,
    /// 自动生成报告
    pub auto_report: bool,
    /// 违规告警
    pub violation_alert: bool,
    /// 安全配置预设
    pub preset: SecurityPreset,
}

impl Default for ComplianceAutoCheckConfig {
    fn default() -> Self {
        Self {
            auto_check_enabled: false,
            check_interval: Duration::from_secs(86400),
            auto_report: true,
            violation_alert: true,
            preset: SecurityPreset::None,
        }
    }
}

/// 闭环健康度配置（v10.2.0，镜像定义在 core 中供 PoolConfig 使用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopHealthConfig {
    pub loop_health_enabled: bool,
    pub health_alert_threshold: u8,
    pub backlog_alert_limit: u32,
    pub degradation_rate_alert_limit: u32,
}

impl Default for LoopHealthConfig {
    fn default() -> Self {
        Self {
            loop_health_enabled: false,
            health_alert_threshold: 60,
            backlog_alert_limit: 500,
            degradation_rate_alert_limit: 10,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linkage_config_default_disabled() {
        let config = PerformanceLinkageConfig::default();
        assert!(!config.enabled);
        assert!(!config.cache_zero_copy_return);
        assert!(!config.batch_cache_aware);

        let mut metrics = LinkageMetrics {
            enabled: true,
            ..Default::default()
        };
        metrics.record_hit();
        assert_eq!(metrics.hit_count, 1, "enabled metrics should record hit");
    }

    #[test]
    fn test_linkage_metrics_record() {
        let mut metrics = LinkageMetrics::default();
        assert_eq!(metrics.hit_count, 0);
        assert_eq!(metrics.skip_count, 0);

        metrics.record_hit();
        metrics.record_hit();
        metrics.record_skip();
        assert_eq!(metrics.hit_count, 2);
        assert_eq!(metrics.skip_count, 1);
    }

    #[test]
    fn test_observability_loop_config_default_disabled() {
        let config = ObservabilityLoopConfig::default();
        assert!(!config.enabled);
        assert!(!config.slow_query_to_ai);
        assert!(!config.alert_to_diagnosis);
        assert_eq!(config.event_queue_capacity, 1000);

        let enabled = ObservabilityLoopConfig {
            enabled: true,
            ..Default::default()
        };
        assert!(enabled.enabled, "enabled config should have enabled=true");
    }

    #[test]
    fn test_compliance_auto_check_config_default_disabled() {
        let config = ComplianceAutoCheckConfig::default();
        assert!(!config.auto_check_enabled);
        assert_eq!(config.preset, SecurityPreset::None);

        let enabled = ComplianceAutoCheckConfig {
            auto_check_enabled: true,
            ..Default::default()
        };
        assert!(
            enabled.auto_check_enabled,
            "enabled config should have auto_check_enabled=true"
        );
    }

    #[test]
    fn test_security_preset_default_is_none() {
        assert_eq!(SecurityPreset::default(), SecurityPreset::None);
        assert_ne!(
            SecurityPreset::ComplianceFirst,
            SecurityPreset::None,
            "presets should differ"
        );
    }
}
