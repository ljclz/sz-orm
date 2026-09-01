//! CLI 库模块：实体生成 + 文档生成

pub mod doc_generator;
pub mod entity_generator;

pub use doc_generator::{DocGenerator, DocOutput};
pub use entity_generator::{
    EntityDefinition, EntityField, EntityGenerator, EntityRelation, RelationType,
};
