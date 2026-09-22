//! 绿色计算模块：能耗采集 + 碳足迹核算 + 绿色调度 + ESG 报告

pub mod breakpoint_resumer;
pub mod calculator;
pub mod collector;
pub mod masker;
pub mod report;
pub mod scheduler;
pub mod types;

#[cfg(feature = "carbon-neutrality")]
pub mod carbon;

pub use breakpoint_resumer::EnergyBreakpointResumer;
pub use calculator::CarbonFootprintCalculator;
pub use collector::EnergyMetricsCollector;
pub use masker::EnergyDataMasker;
pub use report::EsgReportGenerator;
pub use scheduler::GreenScheduler;
pub use types::*;

/// 绿色计算错误类型
#[derive(Debug, thiserror::Error)]
pub enum GreenError {
    #[error("采集失败: {0}")]
    CollectionFailed(String),
    #[error("碳排放因子缺失: {0}")]
    CarbonFactorMissing(String),
    #[error("延迟超限: {0}")]
    LatencyExceeded(String),
    #[error("导出失败: {0}")]
    ExportFailed(String),
}
