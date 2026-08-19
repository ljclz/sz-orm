//! # SZ-ORM AI — AI Capability Package
//!
//! Provides natural language to SQL (NL2SQL), retrieval-augmented generation (RAG), text embedding and vector search capabilities,
//! with built-in safety protection and OpenAI-compatible API client (compiled when `real` feature is enabled).
//!
//! ## Main Modules
//!
//! - [`embedding`] — Text vectorization interface
//! - [`nl2sql`] — Natural language to SQL conversion
//! - [`rag`] — Retrieval-augmented generation
//! - [`vector`] — Vector store and similarity search
//! - [`safety`] — Input safety check
//! - [`sql_sanitizer`] — SQL sensitive literal masking
//!
//! When `llm-optimizer` feature is enabled, additionally provides:
//! - [`explain_parser`] — EXPLAIN execution plan parser (5 dialects)
//! - [`query_plan_optimizer`] — Unified query plan optimizer (rules + LLM)

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

// v4.0.0 M1：多 LLM 模型支持（multi-llm feature gate 隔离）
#[cfg(feature = "multi-llm")]
pub mod llm_provider;
#[cfg(feature = "multi-llm")]
pub use llm_provider::{
    LlmCapability, LlmConfig, LlmError, LlmProvider, LlmProviderKind, LlmRequestConfig,
    LlmResponse, LlmUsage,
};

// v4.0.0 M2：AI 自动调优闭环（ai-auto-tuning feature gate 隔离）
#[cfg(feature = "ai-auto-tuning")]
pub mod auto_tuning;
#[cfg(feature = "ai-auto-tuning")]
pub use auto_tuning::{
    AdviseReport, AppliedSuggestion, ApplyReport, AutoTuningConfig, AutoTuningReport, DetectReport,
    RegressionRecord, SkippedSuggestion, SlowQueryInfo, SuggestionType, TuningError,
    TuningSuggestion, VerifyReport, VerifyResult,
};
