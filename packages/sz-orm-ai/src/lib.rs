//! # SZ-ORM AI — AI 能力包
//!
//! 提供自然语言转 SQL（NL2SQL）、检索增强生成（RAG）、文本 embedding 与向量搜索能力，
//! 内置安全防护与 OpenAI 兼容 API 客户端（启用 `real` feature 时编译）。
//!
//! ## 主要模块
//!
//! - [`embedding`] — 文本向量化接口
//! - [`nl2sql`] — 自然语言到 SQL 的转换
//! - [`rag`] — 检索增强生成
//! - [`vector`] — 向量存储与相似度检索
//! - [`safety`] — 输入安全检查
//! - [`sql_sanitizer`] — SQL 敏感字面量脱敏
//!
//! 启用 `llm-optimizer` feature 时额外提供：
//! - [`explain_parser`] — EXPLAIN 执行计划解析（5 方言）
//! - [`query_plan_optimizer`] — 统一查询计划优化器（规则 + LLM）

pub mod embedding;
pub mod error;
pub mod nl2sql;
pub mod rag;
pub mod safety;
pub mod sql_sanitizer;
pub mod vector;

pub use embedding::*;
pub use error::AiError;
pub use nl2sql::*;
pub use rag::*;
pub use safety::*;
pub use sql_sanitizer::SqlSanitizer;
pub use vector::*;

// 仅在启用 `real` feature 时编译真实 OpenAI 兼容 API 客户端
#[cfg(feature = "real")]
pub mod real_embedding;
#[cfg(feature = "real")]
pub use real_embedding::OpenAIEmbeddingClient;

// 仅在启用 `llm-optimizer` feature 时编译查询计划优化器模块
#[cfg(feature = "llm-optimizer")]
pub mod explain_parser;
#[cfg(feature = "llm-optimizer")]
pub mod query_plan_optimizer;
#[cfg(feature = "llm-optimizer")]
pub use explain_parser::{
    ExplainPlanParser, ExplainSignal, MssqlExplainParser, MySqlExplainParser, OracleExplainParser,
    PgExplainParser, SqliteExplainParser,
};
#[cfg(feature = "llm-optimizer")]
pub use query_plan_optimizer::{
    HintSource, OptimizerConfig, UnifiedOptimizationHint, UnifiedQueryAnalysis,
    UnifiedQueryOptimizer,
};

// v3.3.0 M4：AI 自然语言查询增强
// 共享类型模块（任一 AI 增强 feature 启用时编译）
#[cfg(any(
    feature = "ai-nl2sql-enhanced",
    feature = "ai-index-advisor",
    feature = "ai-rewrite-advisor"
))]
pub mod advice_common;
#[cfg(any(
    feature = "ai-nl2sql-enhanced",
    feature = "ai-index-advisor",
    feature = "ai-rewrite-advisor"
))]
pub use advice_common::{AdviceSource, AdviceType, AiAdviceAuditRecord, BenefitEstimate};

// 查询意图分析模块
#[cfg(feature = "ai-nl2sql-enhanced")]
pub mod intent_analysis;
#[cfg(feature = "ai-nl2sql-enhanced")]
pub use intent_analysis::{
    IntentAnalysisResult, IntentAnalyzer, IntentError, OrderDirection, OrderField, Pagination,
    ParameterizedCondition, QueryIntent, RiskLevel,
};

// 自动索引建议模块
#[cfg(feature = "ai-index-advisor")]
pub mod index_advisor;
#[cfg(feature = "ai-index-advisor")]
pub use index_advisor::{
    IndexAdvisor, IndexError, IndexSuggestion, IndexType, QueryPattern, SlowQueryLog,
};

// 查询重写建议模块
#[cfg(feature = "ai-rewrite-advisor")]
pub mod rewrite_advisor;
#[cfg(feature = "ai-rewrite-advisor")]
pub use rewrite_advisor::{
    EquivalenceProof, RewriteAdvisor, RewriteError, RewriteSuggestion, TransformType,
};
