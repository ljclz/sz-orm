#[cfg(feature = "schema-designer")]
pub mod code_gen;
#[cfg(feature = "schema-designer")]
pub mod code_parse;
#[cfg(feature = "schema-designer")]
pub mod design_ir;
#[cfg(feature = "schema-designer")]
pub mod designer;
#[cfg(feature = "schema-designer")]
pub mod er_editor;
#[cfg(feature = "schema-designer")]
pub mod exporter;
#[cfg(feature = "schema-designer")]
pub mod masking;
#[cfg(feature = "schema-designer")]
pub mod web_ui;

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
pub use masking::DesignerMasking;
#[cfg(feature = "schema-designer")]
pub use web_ui::SchemaDesignerWebUI;

#[cfg(feature = "schema-designer")]
pub use sz_orm_core::dialect_security::Dialect;
