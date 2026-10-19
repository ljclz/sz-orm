//! 可观测性自动接线工厂（v10.2.0）
//!
//! 实现 `AutoWireFactory` trait，为 Pool 自动注入闭环引擎事件回调。
//! v10.3.0 新增 `default_auto_wire_factory()` 便利函数，下游一行代码启用自动接线。

use std::sync::Arc;

use sz_orm_core::auto_wire::{AutoWireFactory, ComplianceHandle, DiagnosticHandle};
use sz_orm_core::diagnostic_report::DiagnosticReportConfig;
use sz_orm_core::performance_linkage::{ComplianceAutoCheckConfig, ObservabilityLoopConfig};
use sz_orm_core::pool::PoolEventCallback;

use crate::observability_loop::{create_pool_event_callback, ObservabilityLoop};

/// 零配置构造默认自动接线工厂
///
/// 下游一行代码启用自动接线：
/// ```ignore
/// use sz_orm_observability::default_auto_wire_factory;
/// let config = PoolConfigBuilder::new()
///     .auto_wire_with(default_auto_wire_factory())
///     .auto_wire(AutoWireConfig { auto_wire_enabled: true, ..Default::default() })
///     .build()?;
/// ```
pub fn default_auto_wire_factory() -> ObservabilityAutoWireFactory {
    ObservabilityAutoWireFactory::new(ObservabilityLoopConfig::default())
}

/// 可观测性自动接线工厂
pub struct ObservabilityAutoWireFactory {
    loop_engine: Arc<ObservabilityLoop>,
}

impl ObservabilityAutoWireFactory {
    pub fn new(loop_config: ObservabilityLoopConfig) -> Self {
        let loop_engine = Arc::new(ObservabilityLoop::new(loop_config));
        Self { loop_engine }
    }

    pub fn from_loop(loop_engine: Arc<ObservabilityLoop>) -> Self {
        Self { loop_engine }
    }

    pub fn loop_engine(&self) -> &Arc<ObservabilityLoop> {
        &self.loop_engine
    }
}

impl AutoWireFactory for ObservabilityAutoWireFactory {
    fn create_event_callback(
        &self,
        _loop_config: &ObservabilityLoopConfig,
    ) -> Option<PoolEventCallback> {
        Some(create_pool_event_callback(Arc::clone(&self.loop_engine)))
    }

    fn wire_compliance(
        &self,
        _compliance_config: &ComplianceAutoCheckConfig,
    ) -> Option<ComplianceHandle> {
        None
    }

    fn wire_diagnostic(&self, _diag_config: &DiagnosticReportConfig) -> Option<DiagnosticHandle> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_observability_auto_wire_factory_creates_callback() {
        let config = ObservabilityLoopConfig::default();
        let factory = ObservabilityAutoWireFactory::new(config);
        let callback = factory.create_event_callback(&ObservabilityLoopConfig::default());
        assert!(callback.is_some(), "factory must create event callback");
    }

    #[test]
    fn test_observability_auto_wire_factory_no_compliance() {
        let factory = ObservabilityAutoWireFactory::new(ObservabilityLoopConfig::default());
        let handle = factory.wire_compliance(&ComplianceAutoCheckConfig::default());
        assert!(
            handle.is_none(),
            "observability factory does not wire compliance"
        );
    }

    #[test]
    fn test_observability_auto_wire_factory_no_diagnostic() {
        let factory = ObservabilityAutoWireFactory::new(ObservabilityLoopConfig::default());
        let handle = factory.wire_diagnostic(&DiagnosticReportConfig::default());
        assert!(
            handle.is_none(),
            "observability factory does not wire diagnostic"
        );
    }
}
