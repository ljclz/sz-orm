//! 碳中和路径规划模块：碳减排目标 + 碳抵消建议 + 中和预测 + 强化学习优化

mod forecaster;
mod offset_advisor;
mod rl_optimizer;
mod target;

pub use forecaster::{CarbonNeutralityForecaster, ForecastReport, ForecastScenario};
pub use offset_advisor::{CarbonOffsetAdvisor, OffsetSuggestion};
pub use rl_optimizer::{GreenRlOptimizer, StrategySuggestion};
pub use target::{CarbonReductionTarget, TargetProgress};

/// 碳中和错误类型
#[derive(Debug, thiserror::Error)]
pub enum CarbonError {
    /// 碳抵消市场数据不可用
    #[error("碳抵消市场数据不可用")]
    OffsetDataUnavailable,
    /// 强化学习历史数据不足：需要至少30天，实际 {actual} 天
    #[error("强化学习历史数据不足：需要至少30天，实际{actual}天")]
    RlDataInsufficient { actual: usize },
    /// 预测历史数据不足
    #[error("预测历史数据不足：至少需要2条记录，实际{actual}条")]
    ForecastDataInsufficient { actual: usize },
    /// 导出失败
    #[error("导出失败: {0}")]
    ExportFailed(String),
    /// 目标设定无效
    #[error("目标设定无效: {0}")]
    InvalidTarget(String),
}
