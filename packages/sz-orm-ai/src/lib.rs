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
    AbTestResult, AbTestSample, AbTestSummary, HintSource, OptimizerConfig, PerformancePrediction,
    PerformancePredictor, QueryABTestFramework, QueryCharacteristics, TableStatistics,
    UnifiedOptimizationHint, UnifiedQueryAnalysis, UnifiedQueryOptimizer,
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

// v5.1.0：Schema 自动提取（ai-schema-extract feature gate 隔离）
// 启用 ai-schema-extract feature 后 NL2SQL 自动提取 Schema（需传入 Connection）
#[cfg(feature = "ai-schema-extract")]
pub mod schema_extractor;
#[cfg(feature = "ai-schema-extract")]
pub use schema_extractor::{
    create_extractor, GenericSchemaExtractor, MssqlSchemaExtractor, MySqlSchemaExtractor,
    OracleSchemaExtractor, PgSchemaExtractor, SchemaExtractError, SchemaExtractor,
    SqliteSchemaExtractor,
};

// v5.1.0：NL2SQL 多轮对话（ai-nl2sql-enhanced feature gate 隔离）
// 启用 ai-nl2sql-enhanced feature 后支持多轮自然语言查询，历史上下文注入 LLM 提示词
#[cfg(feature = "ai-nl2sql-enhanced")]
pub mod multi_turn;
#[cfg(feature = "ai-nl2sql-enhanced")]
pub use multi_turn::{ConversationContext, MultiTurnNl2SqlEngine, TurnRecord};

// v5.1.0：SQL 结果验证（ai-nl2sql-enhanced feature gate 隔离）
// 启用 ai-nl2sql-enhanced feature 后提供 StaticSqlValidator（纯文本安全检查）
// 启用 ai-schema-extract feature 后额外提供 ExplainSqlValidator（连 DB 执行 EXPLAIN）
#[cfg(feature = "ai-nl2sql-enhanced")]
pub mod sql_validator;
#[cfg(all(feature = "ai-nl2sql-enhanced", feature = "ai-schema-extract"))]
pub use sql_validator::ExplainSqlValidator;
#[cfg(feature = "ai-nl2sql-enhanced")]
pub use sql_validator::{
    SqlValidator, StaticSqlValidator, ValidatedNl2SqlEngine, ValidationResult, ValidationSource,
};

// v5.1.0：索引工作负载驱动 + 组合优化（ai-index-advisor feature gate 隔离）
// 启用 ai-index-advisor feature 后提供 WorkloadDrivenIndexAdvisor + IndexCombinationOptimizer
#[cfg(feature = "ai-index-advisor")]
pub mod workload_index;
#[cfg(feature = "ai-index-advisor")]
pub use workload_index::{
    detect_low_usage_indexes, detect_unused_indexes, IndexCombinationOptimizer, IndexUsageStats,
    RedundantIndexInfo, TimeRange, WorkloadAdviceResult, WorkloadDrivenIndexAdvisor,
    WorkloadSummary,
};

// v5.1.0：LLM 安全审计（ai-security-audit feature gate 隔离）
// 启用 ai-security-audit feature 后规则引擎未识别时调用 LLM 二次判断
#[cfg(feature = "ai-security-audit")]
pub mod llm_security_audit;
#[cfg(feature = "ai-security-audit")]
pub use llm_security_audit::{
    AuditSource, InjectionPattern, InjectionPatternStore, LlmAuditProvider, LlmSecurityAuditor,
    RiskLevel as SecurityRiskLevel, SecurityAuditError, SecurityAuditResult,
};

// v5.1.0 P2：权限审计（ai-security-audit feature gate 隔离）
// 分析 SQL 查询的权限使用，识别过度授权并建议最小权限
#[cfg(feature = "ai-security-audit")]
pub mod permission_auditor;
#[cfg(feature = "ai-security-audit")]
pub use permission_auditor::{
    DbAccount, PermissionAuditResult, PermissionAuditor, PermissionFinding,
    PermissionIssueSeverity, QueryUsage,
};

// v5.1.0 P2：语义查询路由（ai-native-query feature gate 隔离）
// 启用 ai-native-query feature 后提供 SemanticQueryRouter（SQL/向量/图谱/Agent/混合）
#[cfg(feature = "ai-native-query")]
pub mod semantic_query;
#[cfg(feature = "ai-native-query")]
pub use semantic_query::{
    AgentError, AgentReport, AgentStep, AiAgent, AnalysisAgent, GraphEdge, GraphNode,
    GraphQueryExecutor, HybridMatch, HybridQueryExecutor, Nl2SqlConverter, SemanticIntent,
    SemanticQueryError, SemanticQueryResult, SemanticQueryRouter, SemanticVectorStore, VectorMatch,
};
