//! SLI/SLO 自动化模块：SLI 采集 + SLO 达成率 + 错误预算 + 仪表盘

pub mod achievement_calculator;
pub mod budget;
pub mod collector;
pub mod dashboard;
pub mod retention_cleaner;
pub mod types;

pub use achievement_calculator::SloAchievementCalculator;
pub use budget::ErrorBudgetTracker;
pub use collector::SliCollector;
pub use dashboard::SloDashboardExporter;
pub use retention_cleaner::SliRetentionCleaner;
pub use types::*;

/// SLI/SLO 错误类型
#[derive(Debug, thiserror::Error)]
pub enum SloAutomationError {
    #[error("数据不足: {0}")]
    InsufficientData(String),
    #[error("预算耗尽: {0}")]
    BudgetExhausted(String),
    #[error("计算错误: {0}")]
    CalcError(String),
    #[error("保留期过期: {0}")]
    RetentionExpired(String),
}
