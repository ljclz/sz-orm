//! # SZ-ORM AI Schema Designer
//!
//! LLM 驱动的 Schema 设计：输入业务需求描述，生成建议表结构/字段/关系/索引。
//!
//! 启用 `ai-schema-design` feature 后可用。
//!
//! ## 功能
//!
//! - [`AiSchemaDesigner`] — AI Schema 设计器
//! - [`design_schema`] — 根据业务需求生成建议 Schema
//! - [`analyze_migration_impact`] — 分析 Schema 变更影响
//! - [`denormalization_advice`] — 反范式化建议

pub mod ai_schema_designer;

pub use ai_schema_designer::{
    AiSchemaDesigner, ColumnDefinition, DenormalizationAdvice, DesignError, DesignResult,
    JoinPattern, LlmSchemaProvider, MigrationImpactReport, MigrationRisk, RedundantColumn,
    SchemaDesign, TableDefinition,
};
