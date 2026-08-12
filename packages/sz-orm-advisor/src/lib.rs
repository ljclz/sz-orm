//! # sz-orm-advisor — 查询自动优化建议引擎 + 智能闭环联动
//!
//! 基于 `query-advisor` feature，规则引擎分析 EXPLAIN 计划 + 自适应统计，
//! 生成六种可执行优化建议（AddIndex/DropIndex/UsePagination/EnableCache/RewriteQuery/AdjustPoolSize）。
//!
//! 基于 `query-intelligence-loop` feature，串联 EXPLAIN → 自适应 → 诊断 → 建议 四步闭环。

#[cfg(feature = "query-advisor")]
pub mod advisor;
#[cfg(feature = "query-advisor")]
pub mod dialect;
#[cfg(feature = "query-advisor")]
pub mod report;
#[cfg(feature = "query-advisor")]
pub mod rules;
#[cfg(feature = "query-advisor")]
pub mod suggestion;

#[cfg(feature = "query-advisor")]
pub use advisor::{AdvisorConfig, OptimizationAdvisor};
#[cfg(feature = "query-advisor")]
pub use suggestion::{
    OptimizationSuggestion, RiskLevel, SuggestionType, TuningSuggestion, TuningSuggestionType,
};

#[cfg(feature = "query-intelligence-loop")]
pub mod intelligence_loop;
#[cfg(feature = "query-intelligence-loop")]
pub use intelligence_loop::{ExplainPlanSummary, IntelligenceLoop, LoopReport};
