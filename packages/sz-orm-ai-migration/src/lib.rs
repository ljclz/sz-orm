//! # SZ-ORM AI Migration Generator
//!
//! LLM 驱动的迁移脚本生成：输入 Schema 变更描述，生成 up/down 迁移脚本。
//!
//! 启用 `ai-migration-gen` feature 后可用。
//!
//! v7.3.0 任务 4.3：新增源 ORM 解析与迁移报告生成（`source_orm_parser` 模块）。

pub mod ai_migration_generator;
pub mod source_orm_parser;

pub use ai_migration_generator::{
    AiMigrationGenerator, DataImpactReport, LlmMigrationProvider, MigrationError, MigrationResult,
    MigrationScript,
};
pub use source_orm_parser::{
    ColumnDef, DieselParser, MigrationMapping, MigrationReport, OrmMigrator, RelationDef,
    SeaOrmParser, SourceOrm, SourceOrmParser, SourceSchema, SqlxParser, TableDef, UnmappableItem,
};
// v7.3.0 任务 4.3：eco-config feature gate 下复用 sz-orm-core 的 SourceOrm 枚举
// 本模块的 SourceOrm 与 sz-orm-core::SourceOrm 语义一致，此处提供转换桥梁。
#[cfg(feature = "eco-config")]
pub fn core_source_orm_to_local(orm: sz_orm_core::SourceOrm) -> SourceOrm {
    match orm {
        sz_orm_core::SourceOrm::Diesel => SourceOrm::Diesel,
        sz_orm_core::SourceOrm::SeaOrm => SourceOrm::SeaOrm,
        sz_orm_core::SourceOrm::Sqlx => SourceOrm::Sqlx,
    }
}
