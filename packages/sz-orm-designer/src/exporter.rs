use crate::code_gen::DesignerCodeGenerator;
use crate::design_ir::{select_ddl_generator, Dialect, SchemaDesign};
use crate::designer::DesignerError;
use crate::er_editor::ErDiagramEditor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    DdlSql,
    Migration,
    RustModel,
    ErPng,
    ErSvg,
    JsonDesign,
}

pub struct DesignerExporter;

impl DesignerExporter {
    pub fn export(
        design: &SchemaDesign,
        format: ExportFormat,
        dialect: Dialect,
    ) -> Result<Vec<u8>, DesignerError> {
        match format {
            ExportFormat::DdlSql => {
                let diff = design.to_schema_diff();
                let generator = select_ddl_generator(dialect);
                let ddl =
                    generator
                        .generate(&diff)
                        .map_err(|e| DesignerError::DdlGenerationPartial {
                            dialect: dialect.as_str().to_string(),
                            feature: e.to_string(),
                        })?;
                Ok(ddl.join("\n").into_bytes())
            }
            ExportFormat::Migration => {
                let gen = DesignerCodeGenerator::new(dialect);
                let migration = gen.generate_migration(design)?;
                Ok(format!(
                    "-- Migration: {}\n-- Version: {}\n\n-- UP:\n{}\n\n-- DOWN:\n{}",
                    migration.name, migration.version, migration.sql_up, migration.sql_down
                )
                .into_bytes())
            }
            ExportFormat::RustModel => {
                let gen = DesignerCodeGenerator::new(dialect);
                let code = gen.generate_model_code(design)?;
                Ok(code.into_bytes())
            }
            ExportFormat::ErSvg => {
                let editor = ErDiagramEditor::new(design.clone());
                Ok(editor.to_svg().into_bytes())
            }
            ExportFormat::ErPng => Err(DesignerError::DdlGenerationPartial {
                dialect: "png".to_string(),
                feature: "PNG export requires additional graphics dependencies".to_string(),
            }),
            ExportFormat::JsonDesign => {
                serde_json::to_vec(design).map_err(|e| DesignerError::ParseFailed {
                    line: 0,
                    reason: e.to_string(),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design_ir::*;

    fn sample_design() -> SchemaDesign {
        SchemaDesign::new(Dialect::MySql).with_table(DesignTable::new(
            "users",
            vec![
                DesignColumn::new("id", ColumnType::BigInt).primary_key(),
                DesignColumn::new("name", ColumnType::Varchar(Some(255))),
            ],
        ))
    }

    #[test]
    fn test_export_ddl() {
        let design = sample_design();
        let result = DesignerExporter::export(&design, ExportFormat::DdlSql, Dialect::MySql);
        assert!(result.is_ok());
        let bytes = result.unwrap();
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.to_uppercase().contains("CREATE TABLE"));
    }

    #[test]
    fn test_export_svg() {
        let design = sample_design();
        let result = DesignerExporter::export(&design, ExportFormat::ErSvg, Dialect::MySql);
        assert!(result.is_ok());
        let bytes = result.unwrap();
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("<svg"));
        assert!(s.contains("users"));
    }

    #[test]
    fn test_export_json() {
        let design = sample_design();
        let result = DesignerExporter::export(&design, ExportFormat::JsonDesign, Dialect::MySql);
        assert!(result.is_ok());
        let bytes = result.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["tables"].is_array());
        assert_eq!(json["tables"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_export_migration() {
        let design = sample_design();
        let result = DesignerExporter::export(&design, ExportFormat::Migration, Dialect::MySql);
        assert!(result.is_ok());
        let bytes = result.unwrap();
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("-- UP:"));
        assert!(s.contains("-- DOWN:"));
    }
}
