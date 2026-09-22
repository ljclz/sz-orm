//! SLO 驱动的自治调度模块：扩缩容 + 降级 + 路由 + 多 SLO 仲裁

mod degrader;
mod router;
mod scaler;

pub use degrader::{DegradeDecision, DegraderConfig, SloDrivenDegrader};
pub use router::{RouteAdjustment, RouteWeight, RouterConfig, SloDrivenRouter};
pub use scaler::{ScaleAction, ScaleDecision, ScalerConfig, SloDrivenScaler};

#[cfg(feature = "slo-driven-arbitration")]
mod arbitrator;

#[cfg(feature = "slo-driven-arbitration")]
pub use arbitrator::{ArbitrationOutcome, SloAction, SloArbitrator};

/// SLO 驱动调度错误类型
#[derive(Debug, thiserror::Error)]
pub enum SloDrivenError {
    /// 核心功能不可降级
    #[error("核心功能不可降级：{0}")]
    DegradeCoreBlocked(String),
    /// 路由调整阈值非法
    #[error("路由调整阈值必须 ∈ (0, 1)，实际：{0}")]
    InvalidRouterThreshold(f64),
    /// 冷却期非法
    #[error("冷却期必须 >= 5 分钟，实际：{0}")]
    InvalidCooldownMinutes(u32),
    /// SLO 仲裁无法解决
    #[error("SLO 仲裁无法解决：{0}")]
    SloArbitrationUnresolved(String),
}
