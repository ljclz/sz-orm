//! v7.1.0 AI 自然语言查询模块
//!
//! 提供 NlQueryGateway 统一入口，编排 LLM → SQL 生成 → 安全校验 → 结果格式化。

pub mod formatter;
pub mod gateway;
pub mod hot_swap;
pub mod intent_cache;
pub mod multi_turn;
pub mod safety_gate;

pub use formatter::{FormattedResult, NlResultFormatter};
pub use gateway::{NlQueryGateway, NlQueryResult};
pub use hot_swap::{LlmAdapterSlot, LlmHotSwapper};
pub use intent_cache::IntentCache;
pub use multi_turn::{MultiTurnContext, TurnSummary};
pub use safety_gate::{NlQuerySafetyGate, SafetyVerdict};
