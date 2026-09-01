//! # SZ-ORM AI Migration Generator
//!
//! LLM 驱动的迁移脚本生成：输入 Schema 变更描述，生成 up/down 迁移脚本。
//!
//! 启用 `ai-migration-gen` feature 后可用。

pub mod ai_migration_generator;

pub use ai_migration_generator::{
    AiMigrationGenerator, DataImpactReport, LlmMigrationProvider, MigrationError, MigrationResult,
    MigrationScript,
};
