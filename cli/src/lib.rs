//! CLI 库模块：实体生成 + 文档生成 + 迁移脚手架

pub mod doc_generator;
pub mod entity_generator;
pub mod migration_scaffold;

pub use doc_generator::{DocGenerator, DocOutput};
pub use entity_generator::{
    DbColumnSchema, DbForeignKey, DbTableSchema, EntityDefinition, EntityField, EntityGenerator,
    EntityRelation, RelationType,
};
pub use migration_scaffold::{derive_down, DeriveDownResult};
