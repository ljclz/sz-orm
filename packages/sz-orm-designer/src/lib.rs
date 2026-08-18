#[cfg(feature = "schema-designer")]
pub mod code_gen;
#[cfg(feature = "schema-designer")]
pub mod code_parse;
#[cfg(feature = "schema-designer")]
pub mod data_type_selector;
#[cfg(feature = "schema-designer")]
pub mod denormalization_advisor;
#[cfg(feature = "schema-designer")]
pub mod design_ir;
#[cfg(feature = "schema-designer")]
pub mod designer;
#[cfg(feature = "schema-designer")]
pub mod er_editor;
#[cfg(feature = "schema-designer")]
pub mod exporter;
#[cfg(feature = "schema-designer")]
pub mod index_designer;
#[cfg(feature = "schema-designer")]
pub mod masking;
#[cfg(feature = "schema-designer")]
pub mod migration_generator;
#[cfg(feature = "schema-designer")]
pub mod schema_versioning;
#[cfg(feature = "schema-designer")]
pub mod table_relationship_graph;
#[cfg(feature = "schema-designer")]
pub mod web_ui;

#[cfg(feature = "schema-designer")]
pub use data_type_selector::{
    DataCharacteristics, DataPurpose, DataTypeSelector, TypeRecommendation,
};
#[cfg(feature = "schema-designer")]
pub use denormalization_advisor::{
    DenormalizationAdvisor, DenormalizationKind, DenormalizationSuggestion, JoinPattern,
};
#[cfg(feature = "schema-designer")]
pub use design_ir::{
    Cardinality, ColumnType, DesignColumn, DesignIndex, DesignRelation, DesignTable, SchemaDesign,
};
#[cfg(feature = "schema-designer")]
pub use designer::{DesignerError, SchemaDesigner};
#[cfg(feature = "schema-designer")]
pub use er_editor::{ErDiagramEditor, LayoutAlgorithm};
#[cfg(feature = "schema-designer")]
pub use exporter::{DesignerExporter, ExportFormat};
#[cfg(feature = "schema-designer")]
pub use index_designer::{IndexDesignSuggestion, IndexDesigner, IndexSuggestionKind, QueryPattern};
#[cfg(feature = "schema-designer")]
pub use masking::DesignerMasking;
#[cfg(feature = "schema-designer")]
pub use migration_generator::{
    ColumnDef, ForeignKeyDef, IndexDef, MigrationGenerator, MigrationOp, MigrationScript,
    ReferenceAction,
};
#[cfg(feature = "schema-designer")]
pub use schema_versioning::{SchemaVersion, SchemaVersioning, VersionStatus};
#[cfg(feature = "schema-designer")]
pub use table_relationship_graph::{RelationKind, RelationshipEdge, TableRelationshipGraph};
#[cfg(feature = "schema-designer")]
pub use web_ui::SchemaDesignerWebUI;

#[cfg(feature = "schema-designer")]
pub use sz_orm_core::dialect_security::Dialect;
