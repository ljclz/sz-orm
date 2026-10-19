//! 自动接线配置与工厂抽象（v10.2.0）
//!
//! 当 `auto_wire_enabled = true` 且 `auto_wire_factory` 已注入时，
//! `Pool::new()` 自动构造闭环引擎/合规检查器/诊断生成器并注册回调。

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::diagnostic_report::DiagnosticReportConfig;
use crate::performance_linkage::{ComplianceAutoCheckConfig, ObservabilityLoopConfig};
use crate::pool::PoolEventCallback;

/// 自动接线配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoWireConfig {
    /// 是否启用自动接线，默认 false（向后兼容）
    pub auto_wire_enabled: bool,
    /// 是否自动接入可观测性闭环引擎，默认 true（auto_wire_enabled=false 时不生效）
    pub wire_observability_loop: bool,
    /// 是否自动接入合规检查器，默认 true
    pub wire_compliance_checker: bool,
    /// 是否自动接入诊断报告生成器，默认 true
    pub wire_diagnostic_generator: bool,
}

impl Default for AutoWireConfig {
    fn default() -> Self {
        Self {
            auto_wire_enabled: false,
            wire_observability_loop: true,
            wire_compliance_checker: true,
            wire_diagnostic_generator: true,
        }
    }
}

/// 合规查询 trait（由 sz-orm-audit 的 ComplianceAutoChecker 实现）
pub trait ComplianceQuery: Send + Sync {
    fn last_result(&self) -> Option<String>;
    fn compliance_score(&self) -> f64;
}

/// 诊断查询 trait（由 DiagnosticReportGenerator 实现）
pub trait DiagnosticQuery: Send + Sync {
    fn is_ready(&self) -> bool;
}

/// 合规句柄（自动接线后由 Pool 持有）
#[derive(Clone)]
pub struct ComplianceHandle {
    pub checker: Arc<dyn ComplianceQuery>,
}

/// 诊断句柄
#[derive(Clone)]
pub struct DiagnosticHandle {
    pub generator: Arc<dyn DiagnosticQuery>,
}

/// 自动接线工厂 trait（由 sz-orm-observability / sz-orm-audit 包实现）
pub trait AutoWireFactory: Send + Sync {
    fn create_event_callback(
        &self,
        loop_config: &ObservabilityLoopConfig,
    ) -> Option<PoolEventCallback>;
    fn wire_compliance(
        &self,
        compliance_config: &ComplianceAutoCheckConfig,
    ) -> Option<ComplianceHandle>;
    fn wire_diagnostic(&self, diag_config: &DiagnosticReportConfig) -> Option<DiagnosticHandle>;
}

/// 自动接线状态
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AutoWireStatus {
    pub wired_components: Vec<String>,
    pub wired_callbacks: Vec<String>,
    pub wire_time_ms: u64,
    pub failed_components: Vec<String>,
}

impl AutoWireStatus {
    pub fn is_fully_wired(&self) -> bool {
        self.failed_components.is_empty() && !self.wired_components.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auto_wire_config_default_disabled() {
        let config = AutoWireConfig::default();
        assert!(!config.auto_wire_enabled);
        assert!(config.wire_observability_loop, "sub-toggles default true");
        let enabled = AutoWireConfig {
            auto_wire_enabled: true,
            ..Default::default()
        };
        assert!(
            enabled.auto_wire_enabled,
            "enabled config has auto_wire_enabled=true"
        );
    }

    #[test]
    fn test_auto_wire_status_is_fully_wired() {
        let ok = AutoWireStatus {
            wired_components: vec!["loop".to_string()],
            ..Default::default()
        };
        assert!(
            ok.is_fully_wired(),
            "no failures + has wired -> fully wired"
        );
        let fail = AutoWireStatus {
            wired_components: vec!["loop".to_string()],
            failed_components: vec!["compliance".to_string()],
            ..Default::default()
        };
        assert!(!fail.is_fully_wired(), "has failures -> not fully wired");
        let empty = AutoWireStatus::default();
        assert!(!empty.is_fully_wired(), "nothing wired -> not fully wired");
    }
}
